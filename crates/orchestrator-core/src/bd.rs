//! Async wrapper around the `bd` (beads) CLI.
//!
//! Every method shells out to the `bd` binary on `$PATH` with `--db <path>`
//! pinned at construction time. JSON-mode flags are used wherever
//! available so we never have to scrape free text. The `kv_*` helpers
//! treat exit code 1 with `found: false` as `Ok(None)`; any other
//! non-zero exit is an error.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::process::Stdio;
use tokio::process::Command;

/// Summary fields returned by `bd ready --json` and `bd list --json`.
/// `metadata` is only populated by `Bd::show()` (full record).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bead {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub issue_type: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct KvGetEnvelope {
    found: bool,
    #[serde(default)]
    value: String,
}

/// Async client wrapping a single `bd` database.
#[derive(Debug, Clone)]
pub struct Bd {
    binary: String,
    db: String,
}

impl Bd {
    /// Build a client pinned to a specific bd database.
    pub fn new(db: impl Into<String>) -> Self {
        Self {
            binary: "bd".to_owned(),
            db: db.into(),
        }
    }

    /// Override the `bd` binary (used by tests to inject a fake).
    pub fn with_binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }

    async fn run<I, S>(&self, args: I) -> Result<(std::process::ExitStatus, String, String)>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = Command::new(&self.binary)
            .arg("--db")
            .arg(&self.db)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .with_context(|| format!("spawning `{}`", self.binary))?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        Ok((output.status, stdout, stderr))
    }

    async fn run_ok<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let (status, stdout, stderr) = self.run(args).await?;
        if !status.success() {
            return Err(anyhow!("bd exited with {status}: {stderr}"));
        }
        Ok(stdout)
    }

    /// `bd ready --json --limit 0 --sort priority` — beads with no open
    /// blockers, ordered by priority.
    pub async fn ready(&self) -> Result<Vec<Bead>> {
        let stdout = self
            .run_ok(["ready", "--json", "--limit", "0", "--sort", "priority"])
            .await?;
        if stdout.trim().is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&stdout).context("parsing `bd ready --json`")
    }

    /// `bd show <id> --json` — full record including metadata.
    pub async fn show(&self, id: &str) -> Result<Bead> {
        let stdout = self.run_ok(["show", id, "--json"]).await?;
        let mut beads: Vec<Bead> =
            serde_json::from_str(&stdout).context("parsing `bd show --json`")?;
        beads
            .pop()
            .ok_or_else(|| anyhow!("bd show {id}: empty result"))
    }

    /// `bd close <id> --reason <reason>`.
    pub async fn close(&self, id: &str, reason: &str) -> Result<()> {
        self.run_ok(["close", id, "--reason", reason]).await?;
        Ok(())
    }

    /// `bd note <id> <text>` — append an audit note.
    pub async fn note(&self, id: &str, text: &str) -> Result<()> {
        self.run_ok(["note", id, text]).await?;
        Ok(())
    }

    /// `bd kv set <key> <value>`.
    pub async fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.run_ok(["kv", "set", key, value]).await?;
        Ok(())
    }

    /// `bd kv get <key> --json`. Returns `Ok(None)` when the key is unset.
    pub async fn kv_get(&self, key: &str) -> Result<Option<String>> {
        let (status, stdout, stderr) = self.run(["kv", "get", key, "--json"]).await?;
        let env: KvGetEnvelope = serde_json::from_str(stdout.trim()).with_context(|| {
            format!("parsing `bd kv get {key} --json` (stderr: {stderr}, status: {status})")
        })?;
        if env.found {
            Ok(Some(env.value))
        } else {
            Ok(None)
        }
    }

    /// `bd kv clear <key>`. Idempotent.
    pub async fn kv_clear(&self, key: &str) -> Result<()> {
        let (status, _stdout, stderr) = self.run(["kv", "clear", key]).await?;
        if !status.success() {
            return Err(anyhow!("bd kv clear {key} failed ({status}): {stderr}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;
    use tokio::fs;

    /// Drops a fake `bd` script into `dir` that responds to a small set
    /// of subcommands by emitting fixture stdout / exit codes.
    async fn write_fake_bd(dir: &std::path::Path, script: &str) {
        let path = dir.join("bd");
        fs::write(&path, script).await.unwrap();
        let mut perm = fs::metadata(&path).await.unwrap().permissions();
        perm.set_mode(0o755);
        fs::set_permissions(&path, perm).await.unwrap();
    }

    fn bd_at(dir: &std::path::Path) -> Bd {
        Bd::new("/dev/null").with_binary(dir.join("bd").to_string_lossy().to_string())
    }

    #[tokio::test]
    async fn ready_parses_empty_array() {
        let tmp = TempDir::new().unwrap();
        write_fake_bd(tmp.path(), "#!/bin/sh\necho '[]'\n").await;
        let bd = bd_at(tmp.path());
        let beads = bd.ready().await.unwrap();
        assert!(beads.is_empty());
    }

    #[tokio::test]
    async fn ready_parses_summary_records() {
        let tmp = TempDir::new().unwrap();
        let script = r#"#!/bin/sh
cat <<'JSON'
[
  {"id":"x-1","title":"first","status":"open","priority":0,"labels":["a"]},
  {"id":"x-2","title":"second","status":"open","priority":2}
]
JSON
"#;
        write_fake_bd(tmp.path(), script).await;
        let bd = bd_at(tmp.path());
        let beads = bd.ready().await.unwrap();
        assert_eq!(beads.len(), 2);
        assert_eq!(beads[0].id, "x-1");
        assert_eq!(beads[0].labels, vec!["a"]);
        assert!(beads[1].labels.is_empty());
    }

    #[tokio::test]
    async fn show_pulls_metadata() {
        let tmp = TempDir::new().unwrap();
        let script = r#"#!/bin/sh
cat <<'JSON'
[{"id":"x-9","title":"t","status":"open","priority":1,"metadata":{"repo_path":"/tmp/r","role":"worker"}}]
JSON
"#;
        write_fake_bd(tmp.path(), script).await;
        let bd = bd_at(tmp.path());
        let bead = bd.show("x-9").await.unwrap();
        assert_eq!(bead.metadata.get("repo_path").unwrap(), "/tmp/r");
        assert_eq!(bead.metadata.get("role").unwrap(), "worker");
    }

    #[tokio::test]
    async fn kv_get_returns_some_when_found() {
        let tmp = TempDir::new().unwrap();
        write_fake_bd(
            tmp.path(),
            "#!/bin/sh\necho '{\"found\":true,\"key\":\"k\",\"value\":\"v\"}'\n",
        )
        .await;
        let bd = bd_at(tmp.path());
        assert_eq!(bd.kv_get("k").await.unwrap(), Some("v".to_owned()));
    }

    #[tokio::test]
    async fn kv_get_returns_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        write_fake_bd(
            tmp.path(),
            "#!/bin/sh\necho '{\"found\":false,\"key\":\"k\",\"value\":\"\"}'\nexit 1\n",
        )
        .await;
        let bd = bd_at(tmp.path());
        assert_eq!(bd.kv_get("k").await.unwrap(), None);
    }

    #[tokio::test]
    async fn close_propagates_failure() {
        let tmp = TempDir::new().unwrap();
        write_fake_bd(tmp.path(), "#!/bin/sh\necho 'boom' >&2\nexit 7\n").await;
        let bd = bd_at(tmp.path());
        let err = bd.close("x-1", "done").await.unwrap_err();
        assert!(err.to_string().contains("boom"));
    }
}
