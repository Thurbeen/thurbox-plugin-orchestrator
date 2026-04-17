//! Long-lived `thurbox-mcp` subprocess client.
//!
//! Speaks standard MCP over stdio: JSON-RPC 2.0 with the `initialize` /
//! `notifications/initialized` handshake followed by `tools/call`
//! requests. Tool results are wrapped in `{content:[{type:"text",text:…}],isError}`;
//! `call_text` returns the inner text and `call_json` deserializes it.
//!
//! The subprocess is kept alive across many calls to amortise startup
//! cost. All calls are serialized through a single `Mutex<Inner>`.

use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

const PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cwd: String,
    /// Set by `create_session`; the underlying claude session id.
    #[serde(default)]
    pub agent_session_id: Option<String>,
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
    /// Spawn `binary` over stdio and complete the MCP initialize handshake.
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

        let client = Self {
            next_id: AtomicU64::new(1),
            inner: Mutex::new(Inner {
                child,
                stdin: BufWriter::new(stdin),
                stdout: BufReader::new(stdout),
            }),
        };
        client.handshake().await?;
        Ok(client)
    }

    async fn handshake(&self) -> anyhow::Result<()> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let init = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "thurbox-orchestrator", "version": env!("CARGO_PKG_VERSION")}
            }
        });
        let initialized = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });
        let mut inner = self.inner.lock().await;
        write_line(&mut inner.stdin, &init).await?;
        let _: Value = read_response(&mut inner.stdout, id).await?;
        write_line(&mut inner.stdin, &initialized).await?;
        Ok(())
    }

    /// Call an MCP tool and return the inner text payload.
    pub async fn call_text(&self, tool: &str, arguments: Value) -> anyhow::Result<String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": tool, "arguments": arguments}
        });
        let mut inner = self.inner.lock().await;
        write_line(&mut inner.stdin, &req).await?;
        let result: Value = read_response(&mut inner.stdout, id).await?;
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(anyhow!("tool {tool} returned isError=true: {result}"));
        }
        let text = result
            .get("content")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
            .and_then(|c| c.get("text"))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("tool {tool} response missing content[0].text: {result}"))?;
        Ok(text.to_owned())
    }

    /// Call an MCP tool and parse the inner text as JSON.
    pub async fn call_json<T: serde::de::DeserializeOwned>(
        &self,
        tool: &str,
        arguments: Value,
    ) -> anyhow::Result<T> {
        let text = self.call_text(tool, arguments).await?;
        serde_json::from_str(&text)
            .with_context(|| format!("parsing JSON from tool {tool} response: {text}"))
    }

    pub async fn create_session(&self, name: &str, repo_path: &str) -> anyhow::Result<SessionInfo> {
        self.call_json(
            "create_session",
            json!({"name": name, "repo_path": repo_path}),
        )
        .await
    }

    pub async fn create_session_with(
        &self,
        name: &str,
        repo_path: &str,
        role: Option<&str>,
        skills: &[String],
    ) -> anyhow::Result<SessionInfo> {
        let mut args = json!({"name": name, "repo_path": repo_path});
        if let Some(r) = role {
            args["role"] = Value::String(r.to_owned());
        }
        if !skills.is_empty() {
            args["skills"] = json!(skills);
        }
        self.call_json("create_session", args).await
    }

    pub async fn send_prompt(&self, session: &str, text: &str) -> anyhow::Result<()> {
        self.call_text("send_prompt", json!({"session": session, "text": text}))
            .await?;
        Ok(())
    }

    pub async fn capture_session_output(
        &self,
        session: &str,
        lines: u32,
    ) -> anyhow::Result<String> {
        self.call_text(
            "capture_session_output",
            json!({"session": session, "lines": lines}),
        )
        .await
    }

    pub async fn get_session(&self, session: &str) -> anyhow::Result<Option<SessionInfo>> {
        // Errors from get_session on an unknown id come back as tool errors;
        // treat any error here as "no such session" rather than propagating.
        match self
            .call_json::<SessionInfo>("get_session", json!({"session": session}))
            .await
        {
            Ok(info) => Ok(Some(info)),
            Err(_) => Ok(None),
        }
    }

    pub async fn delete_session(&self, session: &str) -> anyhow::Result<()> {
        self.call_text("delete_session", json!({"session": session, "force": true}))
            .await?;
        Ok(())
    }

    pub async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>> {
        self.call_json("list_sessions", json!({})).await
    }

    /// Best-effort orderly shutdown.
    pub async fn shutdown(self) {
        let mut inner = self.inner.into_inner();
        let _ = inner.child.start_kill();
        let _ = inner.child.wait().await;
    }
}

async fn write_line(w: &mut BufWriter<ChildStdin>, value: &Value) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    w.write_all(&bytes).await?;
    w.flush().await?;
    Ok(())
}

async fn read_response(r: &mut BufReader<ChildStdout>, expected_id: u64) -> anyhow::Result<Value> {
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line).await?;
        if n == 0 {
            return Err(anyhow!("thurbox-mcp closed stream while awaiting response"));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(trimmed)
            .with_context(|| format!("parsing thurbox-mcp frame: {trimmed}"))?;
        // Skip notifications and unrelated frames.
        if v.get("id").and_then(Value::as_u64) != Some(expected_id) {
            continue;
        }
        if let Some(err) = v.get("error") {
            return Err(anyhow!("thurbox-mcp error response: {err}"));
        }
        return Ok(v.get("result").cloned().unwrap_or(Value::Null));
    }
}
