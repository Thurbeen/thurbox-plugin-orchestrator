//! Orchestration logic — composes [`Bd`], [`thurbox::Client`], and
//! [`sentinel::parse`] into the five operations the plugin exposes as
//! `orch.*` MCP tools.
//!
//! State model: bd kv stores two complementary mappings, both written
//! at dispatch time and cleared at close time:
//!
//! - `orch:bead:<bd-id>`     → thurbox session id
//! - `orch:session:<uuid>`   → bd id

use crate::bd::{Bd, Bead};
use crate::sentinel::{self, Outcome};
use crate::thurbox::Client;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

const KV_BEAD_PREFIX: &str = "orch:bead:";
const KV_SESSION_PREFIX: &str = "orch:session:";
const ENV_DEFAULT_REPO: &str = "THURBOX_ORCH_DEFAULT_REPO";

#[derive(Debug, Default, Clone, Deserialize)]
pub struct DispatchOpts {
    /// Specific bd id to dispatch. If `None`, the first ready bead is used.
    #[serde(default)]
    pub bd_id: Option<String>,
    /// Override `metadata.repo_path` from the bead.
    #[serde(default)]
    pub repo_path_override: Option<String>,
    /// Override `metadata.role`.
    #[serde(default)]
    pub role_override: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DispatchOutcome {
    pub bd_id: String,
    pub session_id: String,
    pub started_at: u64,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct CloseOpts {
    #[serde(default)]
    pub reason: Option<String>,
    /// If true (default), delete the thurbox session as part of close.
    #[serde(default = "default_true")]
    pub delete_session: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct CloseOutcome {
    pub bd_id: String,
    pub session_deleted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PollOutcome {
    /// One of `running`, `ok`, `error`, `malformed`.
    pub status: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub artifact: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    pub output_tail: String,
}

/// Return the priority-ordered list of ready beads as JSON.
pub async fn ready(bd: &Bd) -> Result<Value> {
    let beads = bd.ready().await?;
    Ok(serde_json::to_value(beads)?)
}

/// Dispatch a ready bead to a fresh thurbox session.
///
/// Reads `metadata.repo_path` (required), `metadata.role`, and
/// `metadata.skills` (comma-separated) off the bead. Records the
/// bead↔session mapping in bd kv. Rolls the session back if any of the
/// kv writes fail.
pub async fn dispatch(bd: &Bd, tx: &Client, opts: DispatchOpts) -> Result<DispatchOutcome> {
    let bead = resolve_target(bd, opts.bd_id.as_deref()).await?;
    let bd_id = bead.id.clone();

    let env_default = env::var(ENV_DEFAULT_REPO).ok();
    let repo_path = resolve_repo_path(
        opts.repo_path_override.as_deref(),
        &bead.metadata,
        env_default.as_deref(),
    )
    .ok_or_else(|| {
        anyhow!(
            "bd item {bd_id} has no `repo_path`; \
             set it with `bd update {bd_id} --set-metadata repo_path=<dir>`, \
             pass repo_path_override, or set {ENV_DEFAULT_REPO}"
        )
    })?;

    let role = opts
        .role_override
        .clone()
        .or_else(|| bead.metadata.get("role").cloned());
    let skills = parse_skills(bead.metadata.get("skills"));

    let session = tx
        .create_session_with(&bd_id, &repo_path, role.as_deref(), &skills)
        .await
        .with_context(|| format!("creating thurbox session for {bd_id}"))?;
    let session_id = session.id.clone();

    let prompt = render_worker_prompt(&bead);
    if let Err(e) = tx.send_prompt(&session_id, &prompt).await {
        let _ = tx.delete_session(&session_id).await;
        return Err(e.context(format!("sending initial prompt to session {session_id}")));
    }

    let bead_key = format!("{KV_BEAD_PREFIX}{bd_id}");
    let session_key = format!("{KV_SESSION_PREFIX}{session_id}");
    if let Err(e) = bd.kv_set(&bead_key, &session_id).await {
        let _ = tx.delete_session(&session_id).await;
        return Err(e.context(format!("recording {bead_key} → {session_id}")));
    }
    if let Err(e) = bd.kv_set(&session_key, &bd_id).await {
        let _ = bd.kv_clear(&bead_key).await;
        let _ = tx.delete_session(&session_id).await;
        return Err(e.context(format!("recording {session_key} → {bd_id}")));
    }

    Ok(DispatchOutcome {
        bd_id,
        session_id,
        started_at: now_secs(),
    })
}

/// Capture the tail of a session's output and report whether the
/// worker has emitted the result sentinel.
pub async fn poll(bd: &Bd, tx: &Client, session_id: &str, lines: u32) -> Result<PollOutcome> {
    let _ = bd; // reserved for future bd-side observation
    let lines = if lines == 0 { 200 } else { lines };
    let output = tx.capture_session_output(session_id, lines).await?;
    Ok(match sentinel::parse(&output) {
        Outcome::NotFound => PollOutcome {
            status: "running".to_owned(),
            artifact: String::new(),
            notes: String::new(),
            output_tail: output,
        },
        Outcome::Found(r) => PollOutcome {
            status: r.status,
            artifact: r.artifact,
            notes: r.notes,
            output_tail: output,
        },
        Outcome::Malformed(reason) => PollOutcome {
            status: "malformed".to_owned(),
            artifact: String::new(),
            notes: reason,
            output_tail: output,
        },
    })
}

/// Close a bead and (by default) the thurbox session that was working
/// on it. Idempotent against a missing session mapping.
pub async fn close(bd: &Bd, tx: &Client, bd_id: &str, opts: CloseOpts) -> Result<CloseOutcome> {
    let bead_key = format!("{KV_BEAD_PREFIX}{bd_id}");
    let session_id = bd.kv_get(&bead_key).await?;

    let reason = opts
        .reason
        .as_deref()
        .unwrap_or("completed via orchestrator");
    bd.close(bd_id, reason).await?;

    let mut session_deleted = false;
    if let Some(sid) = session_id.as_deref() {
        let session_key = format!("{KV_SESSION_PREFIX}{sid}");
        let _ = bd.kv_clear(&session_key).await;
        if opts.delete_session {
            match tx.delete_session(sid).await {
                Ok(()) => session_deleted = true,
                Err(e) => {
                    let _ = bd
                        .note(bd_id, &format!("orch.close: delete_session failed: {e}"))
                        .await;
                }
            }
        }
    }
    let _ = bd.kv_clear(&bead_key).await;

    Ok(CloseOutcome {
        bd_id: bd_id.to_owned(),
        session_deleted,
    })
}

/// List currently dispatched (bd_id, session_id) pairs.
///
/// Walks `tx.list_sessions()` and intersects with the
/// `orch:session:<uuid>` kv mapping so that sessions started outside
/// the orchestrator are excluded.
pub async fn list_active(bd: &Bd, tx: &Client) -> Result<Value> {
    let sessions = tx.list_sessions().await?;
    let mut active = Vec::new();
    for s in sessions {
        let key = format!("{KV_SESSION_PREFIX}{}", s.id);
        if let Some(bd_id) = bd.kv_get(&key).await? {
            active.push(json!({
                "bd_id": bd_id,
                "session_id": s.id,
                "name": s.name,
                "cwd": s.cwd,
            }));
        }
    }
    Ok(Value::Array(active))
}

async fn resolve_target(bd: &Bd, requested: Option<&str>) -> Result<Bead> {
    if let Some(id) = requested {
        return bd.show(id).await;
    }
    let mut beads = bd.ready().await?;
    if beads.is_empty() {
        return Err(anyhow!("no ready bd items"));
    }
    Ok(beads.remove(0))
}

/// Resolution order for the worker's working directory:
/// 1. explicit `repo_path_override` on the dispatch call
/// 2. bead `metadata.repo_path`
/// 3. `THURBOX_ORCH_DEFAULT_REPO` env fallback (deployment default)
fn resolve_repo_path(
    override_: Option<&str>,
    bead_metadata: &HashMap<String, String>,
    env_default: Option<&str>,
) -> Option<String> {
    override_
        .map(str::to_owned)
        .or_else(|| bead_metadata.get("repo_path").cloned())
        .or_else(|| env_default.map(str::to_owned))
        .filter(|s| !s.is_empty())
}

fn parse_skills(raw: Option<&String>) -> Vec<String> {
    raw.map(|s| {
        s.split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

fn render_worker_prompt(bead: &Bead) -> String {
    format!(
        "You are working bd item {id}: {title}.\n\
         \n\
         When you finish, emit on the very last lines of your output:\n\
         \n\
         ===RESULT===\n\
         {{\"status\":\"ok\",\"artifact\":\"<short summary>\",\"notes\":\"<details>\"}}\n\
         \n\
         If the work cannot be completed, emit:\n\
         \n\
         ===RESULT===\n\
         {{\"status\":\"error\",\"notes\":\"<what went wrong>\"}}\n\
         \n\
         The orchestrator polls your output for that sentinel and will close\n\
         bd item {id} on `status:\"ok\"`.",
        id = bead.id,
        title = bead.title,
    )
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_repo_path_prefers_override_then_metadata_then_env() {
        let mut md = HashMap::new();
        md.insert("repo_path".to_owned(), "/from/bead".to_owned());

        assert_eq!(
            resolve_repo_path(Some("/from/override"), &md, Some("/from/env")).as_deref(),
            Some("/from/override"),
        );
        assert_eq!(
            resolve_repo_path(None, &md, Some("/from/env")).as_deref(),
            Some("/from/bead"),
        );
        assert_eq!(
            resolve_repo_path(None, &HashMap::new(), Some("/from/env")).as_deref(),
            Some("/from/env"),
        );
        assert_eq!(resolve_repo_path(None, &HashMap::new(), None), None);
        // Empty strings don't count as set.
        assert_eq!(resolve_repo_path(Some(""), &HashMap::new(), None), None);
    }

    #[test]
    fn parse_skills_handles_empty_and_csv() {
        assert!(parse_skills(None).is_empty());
        assert!(parse_skills(Some(&String::new())).is_empty());
        assert_eq!(
            parse_skills(Some(&"a, b ,, c".to_owned())),
            vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]
        );
    }

    #[test]
    fn worker_prompt_mentions_bd_id_and_sentinel() {
        let bead = Bead {
            id: "x-1".to_owned(),
            title: "Do the thing".to_owned(),
            status: "ready".to_owned(),
            priority: 0,
            issue_type: String::new(),
            labels: vec![],
            metadata: Default::default(),
        };
        let prompt = render_worker_prompt(&bead);
        assert!(prompt.contains("x-1"));
        assert!(prompt.contains("Do the thing"));
        assert!(prompt.contains("===RESULT==="));
    }
}
