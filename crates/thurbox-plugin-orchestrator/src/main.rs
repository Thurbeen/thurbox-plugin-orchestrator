//! `thurbox-plugin-orchestrator` — long-running thurbox plugin daemon.
//!
//! Speaks the thurbox plugin protocol (line-delimited JSON-RPC over
//! stdio). Capabilities: `mcp-tools`. Exposes `orch.*` tools that drive
//! a beads-backed multi-agent orchestrator.
//!
//! Two backends are stitched together at startup:
//!
//! - [`Bd`] — async wrapper around the `bd` (beads) CLI. The database
//!   path defaults to `~/.local/share/thurbox/admin/.beads/`; override
//!   with `THURBOX_ORCH_BD_DB`.
//! - [`thurbox::Client`] — long-lived `thurbox-mcp` subprocess speaking
//!   standard MCP. Binary path defaults to `thurbox-mcp` on `$PATH`;
//!   override with `THURBOX_ORCH_MCP_BIN`.

use anyhow::{Context, Result};
use orchestrator_core::bd::Bd;
use orchestrator_core::jsonrpc::{self, Incoming, Notification, Request, Response};
use orchestrator_core::plugin::{self, HandshakeRequest};
use orchestrator_core::thurbox::Client;
use serde_json::{json, Value};
use std::env;
use tokio::io::{stdin, stdout, BufReader, BufWriter};
use tokio::sync::Mutex;

const DEFAULT_BD_DB_REL: &str = ".local/share/thurbox/admin/.beads/";
const DEFAULT_MCP_BIN: &str = "thurbox-mcp";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let stdin = stdin();
    let stdout = stdout();
    let mut reader = BufReader::new(stdin);
    let writer = Mutex::new(BufWriter::new(stdout));

    let bd = Bd::new(resolve_bd_db()?);
    let mcp_bin = env::var("THURBOX_ORCH_MCP_BIN").unwrap_or_else(|_| DEFAULT_MCP_BIN.to_owned());
    let tx = Client::spawn(&mcp_bin)
        .await
        .with_context(|| format!("spawning thurbox-mcp at `{mcp_bin}`"))?;

    loop {
        let frame: Option<Incoming> = jsonrpc::read_frame(&mut reader).await?;
        let Some(frame) = frame else { break };
        match frame {
            Incoming::Request(req) => {
                let resp = handle_request(&bd, &tx, req).await;
                let mut w = writer.lock().await;
                jsonrpc::write_frame(&mut *w, &resp).await?;
            }
            Incoming::Notification(note) => handle_notification(note),
            // Plugins don't expect to receive responses; ignore.
            Incoming::Response(_) => {}
        }
    }
    Ok(())
}

fn resolve_bd_db() -> Result<String> {
    if let Ok(path) = env::var("THURBOX_ORCH_BD_DB") {
        return Ok(path);
    }
    let home = env::var("HOME").context("$HOME is unset; cannot locate default bd db")?;
    Ok(format!("{home}/{DEFAULT_BD_DB_REL}"))
}

async fn handle_request(bd: &Bd, tx: &Client, req: Request) -> Response {
    match req.op.as_str() {
        "handshake" => match serde_json::from_value::<HandshakeRequest>(req.params) {
            Ok(hs) => match plugin::handshake(hs) {
                Ok(resp) => Response::ok(req.id, serde_json::to_value(resp).unwrap_or(Value::Null)),
                Err(e) => Response::err(req.id, e),
            },
            Err(e) => Response::err(req.id, format!("invalid handshake params: {e}")),
        },
        "mcp.list_tools" => {
            let tools = plugin::tool_catalog();
            Response::ok(req.id, json!({ "tools": tools }))
        }
        "mcp.call" => {
            let name = req.params.get("name").and_then(Value::as_str);
            let params = req.params.get("params").cloned().unwrap_or(Value::Null);
            let Some(name) = name else {
                return Response::err(req.id, "mcp.call requires `name`");
            };
            match plugin::dispatch_tool(bd, tx, name, &params).await {
                Ok(v) => Response::ok(req.id, v),
                Err(e) => Response::err(req.id, e),
            }
        }
        "stop" => Response::ok(req.id, Value::Null),
        other => Response::err(req.id, format!("unknown op {other}")),
    }
}

fn handle_notification(note: Notification) {
    if note.op == "config.updated" {
        return;
    }
    eprintln!(
        "thurbox-plugin-orchestrator: ignoring notification {}",
        note.op
    );
}
