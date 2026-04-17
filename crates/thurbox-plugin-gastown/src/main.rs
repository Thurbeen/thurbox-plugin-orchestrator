//! `thurbox-plugin-gastown` — long-running thurbox plugin daemon.
//!
//! Speaks the thurbox plugin protocol (line-delimited JSON-RPC over
//! stdio). Capabilities: `mcp-tools`. Exposes `gastown.*` tools by
//! shelling out to the `gt` CLI.

use anyhow::Result;
use orchestrator_core::gastown::Cli as GastownCli;
use orchestrator_core::jsonrpc::{self, Incoming, Notification, Request, Response};
use orchestrator_core::plugin::{self, HandshakeRequest};
use serde_json::{json, Value};
use tokio::io::{stdin, stdout, BufReader, BufWriter};
use tokio::sync::Mutex;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let stdin = stdin();
    let stdout = stdout();
    let mut reader = BufReader::new(stdin);
    let writer = Mutex::new(BufWriter::new(stdout));
    let cli = GastownCli::new();

    loop {
        let frame: Option<Incoming> = jsonrpc::read_frame(&mut reader).await?;
        let Some(frame) = frame else { break };
        match frame {
            Incoming::Request(req) => {
                let resp = handle_request(&cli, req).await;
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

async fn handle_request(cli: &GastownCli, req: Request) -> Response {
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
            match plugin::dispatch_tool(cli, name, &params).await {
                Ok(v) => Response::ok(req.id, v),
                Err(e) => Response::err(req.id, e),
            }
        }
        "stop" => Response::ok(req.id, Value::Null),
        other => Response::err(req.id, format!("unknown op {other}")),
    }
}

fn handle_notification(note: Notification) {
    // `config.updated` is the only notification we need to honour in
    // MVP and we have no settings yet, so swallow.
    if note.op == "config.updated" {
        return;
    }
    eprintln!("thurbox-plugin-gastown: ignoring notification {}", note.op);
}
