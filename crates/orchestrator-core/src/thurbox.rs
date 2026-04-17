//! Thin client around a long-lived `thurbox-mcp` subprocess.
//!
//! MVP scope: spawn `thurbox-mcp` with stdio transport, exchange
//! line-delimited JSON-RPC frames, expose typed wrappers for the
//! orchestration-relevant tools. Keeping the subprocess long-lived
//! amortises startup cost across many calls in a single rig op.

use crate::jsonrpc::{self, Request, Response};
use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cwd: String,
}

pub struct Client {
    next_id: AtomicU64,
    inner: Mutex<Inner>,
}

struct Inner {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Client {
    /// Spawn `thurbox-mcp` over stdio. The binary must be on `$PATH`.
    pub async fn spawn(binary: &str) -> anyhow::Result<Self> {
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning {binary}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("missing stdin handle for {binary}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("missing stdout handle for {binary}"))?;

        Ok(Self {
            next_id: AtomicU64::new(1),
            inner: Mutex::new(Inner {
                child,
                stdin: BufWriter::new(stdin),
                stdout: BufReader::new(stdout),
            }),
        })
    }

    pub async fn call(&self, op: &str, params: Value) -> anyhow::Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let req = Request {
            id,
            op: op.to_owned(),
            params,
        };
        let mut inner = self.inner.lock().await;
        jsonrpc::write_frame(&mut inner.stdin, &req).await?;
        let resp: Response = jsonrpc::read_frame(&mut inner.stdout)
            .await?
            .ok_or_else(|| anyhow!("thurbox-mcp closed stream while awaiting reply for {op}"))?;
        if resp.id != id {
            return Err(anyhow!("id mismatch: sent {id} got {} (op={op})", resp.id));
        }
        if !resp.ok {
            return Err(anyhow!(
                "thurbox-mcp returned error for {op}: {}",
                resp.error.unwrap_or_default()
            ));
        }
        Ok(resp.result.unwrap_or(Value::Null))
    }

    pub async fn create_session(&self, name: &str, repo_path: &str) -> anyhow::Result<SessionInfo> {
        let v = self
            .call(
                "create_session",
                json!({ "name": name, "repo_path": repo_path }),
            )
            .await?;
        Ok(serde_json::from_value(v)?)
    }

    pub async fn send_prompt(&self, session: &str, text: &str) -> anyhow::Result<()> {
        self.call("send_prompt", json!({ "session": session, "text": text }))
            .await?;
        Ok(())
    }

    pub async fn capture_session_output(
        &self,
        session: &str,
        lines: u32,
    ) -> anyhow::Result<String> {
        let v = self
            .call(
                "capture_session_output",
                json!({ "session": session, "lines": lines }),
            )
            .await?;
        Ok(v.get("output")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned())
    }

    pub async fn get_session(&self, session: &str) -> anyhow::Result<Option<SessionInfo>> {
        match self
            .call("get_session", json!({ "session": session }))
            .await
        {
            Ok(v) if v.is_null() => Ok(None),
            Ok(v) => Ok(Some(serde_json::from_value(v)?)),
            Err(err) => Err(err),
        }
    }

    pub async fn delete_session(&self, session: &str) -> anyhow::Result<()> {
        self.call("delete_session", json!({ "session": session }))
            .await?;
        Ok(())
    }

    pub async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>> {
        let v = self.call("list_sessions", Value::Null).await?;
        Ok(serde_json::from_value(v)?)
    }

    /// Best-effort orderly shutdown.
    pub async fn shutdown(self) {
        let mut inner = self.inner.into_inner();
        let _ = inner.child.start_kill();
        let _ = inner.child.wait().await;
    }
}
