//! Thurbox plugin protocol surface (host-facing).
//!
//! The plugin daemon (binary `thurbox-plugin-orchestrator`) speaks
//! line-delimited JSON-RPC to thurbox over stdio. This module owns:
//!
//! - the handshake envelope (api_version match against
//!   `PLUGIN_API_VERSION`)
//! - the `mcp.list_tools` / `mcp.call` dispatch table that exposes
//!   the `orch.*` tools
//! - the `stop` op for orderly shutdown

use crate::bd::Bd;
use crate::orch::{self, CloseOpts, DispatchOpts};
use crate::thurbox::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const PLUGIN_API_VERSION: u32 = 1;

/// Static plugin manifest as it sits in `thurbox-plugin.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub thurbox_plugin_api: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub process: ManifestProcess,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestProcess {
    pub exec: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub activation_events: Vec<String>,
}

impl Manifest {
    pub fn from_toml(src: &str) -> Result<Self, String> {
        let m: Manifest = toml::from_str(src).map_err(|e| e.to_string())?;
        if m.thurbox_plugin_api != PLUGIN_API_VERSION {
            return Err(format!(
                "manifest thurbox_plugin_api={} but this plugin targets {}",
                m.thurbox_plugin_api, PLUGIN_API_VERSION
            ));
        }
        Ok(m)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HandshakeRequest {
    pub api_version: u32,
    #[serde(default)]
    pub plugin_name: String,
    #[serde(default)]
    pub effective_configuration: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct HandshakeResponse {
    pub api_version: u32,
    pub capabilities: Vec<String>,
}

pub fn handshake(req: HandshakeRequest) -> Result<HandshakeResponse, String> {
    if req.api_version != PLUGIN_API_VERSION {
        return Err(format!(
            "unsupported api_version {}: this plugin requires {}",
            req.api_version, PLUGIN_API_VERSION
        ));
    }
    Ok(HandshakeResponse {
        api_version: PLUGIN_API_VERSION,
        capabilities: vec!["mcp-tools".to_owned()],
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDescriptor {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

pub fn tool_catalog() -> Vec<ToolDescriptor> {
    vec![
        ToolDescriptor {
            name: "orch.ready",
            description: "List ready bd items in priority order.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDescriptor {
            name: "orch.dispatch",
            description: "Dispatch a ready bd item to a fresh thurbox session. \
                If `bd_id` is omitted, the highest-priority ready item is used. \
                repo_path resolves in order: `repo_path_override` > bead `metadata.repo_path` > \
                `THURBOX_ORCH_DEFAULT_REPO` env var. Errors if none are set.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "bd_id": { "type": "string" },
                    "repo_path_override": { "type": "string" },
                    "role_override": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "orch.poll",
            description: "Capture a session's recent output and report whether the worker has \
                emitted the `===RESULT===` sentinel. Returns status one of running|ok|error|malformed.",
            input_schema: json!({
                "type": "object",
                "required": ["session_id"],
                "properties": {
                    "session_id": { "type": "string" },
                    "lines": { "type": "integer", "default": 200 }
                }
            }),
        },
        ToolDescriptor {
            name: "orch.close",
            description: "Close a bd item and (by default) delete the thurbox session that was working on it.",
            input_schema: json!({
                "type": "object",
                "required": ["bd_id"],
                "properties": {
                    "bd_id": { "type": "string" },
                    "reason": { "type": "string" },
                    "delete_session": { "type": "boolean", "default": true }
                }
            }),
        },
        ToolDescriptor {
            name: "orch.list_active",
            description: "List currently dispatched bd↔session pairs (intersection of \
                `tx.list_sessions` and `orch:session:*` kv mappings).",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
    ]
}

pub async fn dispatch_tool(
    bd: &Bd,
    tx: &Client,
    name: &str,
    params: &Value,
) -> Result<Value, String> {
    let to_err = |e: anyhow::Error| e.to_string();
    match name {
        "orch.ready" => orch::ready(bd).await.map_err(to_err),
        "orch.dispatch" => {
            let opts: DispatchOpts =
                serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
            orch::dispatch(bd, tx, opts)
                .await
                .map(|o| serde_json::to_value(o).unwrap_or(Value::Null))
                .map_err(to_err)
        }
        "orch.poll" => {
            let session_id = required_str(params, "session_id")?;
            let lines = params.get("lines").and_then(Value::as_u64).unwrap_or(200) as u32;
            orch::poll(bd, tx, session_id, lines)
                .await
                .map(|o| serde_json::to_value(o).unwrap_or(Value::Null))
                .map_err(to_err)
        }
        "orch.close" => {
            let bd_id = required_str(params, "bd_id")?;
            let opts = CloseOpts {
                reason: optional_str(params, "reason").map(str::to_owned),
                delete_session: params
                    .get("delete_session")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            };
            orch::close(bd, tx, bd_id, opts)
                .await
                .map(|o| serde_json::to_value(o).unwrap_or(Value::Null))
                .map_err(to_err)
        }
        "orch.list_active" => orch::list_active(bd, tx).await.map_err(to_err),
        other => Err(format!("unknown tool {other}")),
    }
}

fn required_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing required param `{key}`"))
}

fn optional_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_accepts_matching_api_version() {
        let req = HandshakeRequest {
            api_version: PLUGIN_API_VERSION,
            plugin_name: "orchestrator".into(),
            effective_configuration: Value::Null,
        };
        let resp = handshake(req).unwrap();
        assert_eq!(resp.api_version, PLUGIN_API_VERSION);
        assert_eq!(resp.capabilities, vec!["mcp-tools".to_owned()]);
    }

    #[test]
    fn handshake_rejects_mismatched_api_version() {
        let req = HandshakeRequest {
            api_version: 99,
            plugin_name: "orchestrator".into(),
            effective_configuration: Value::Null,
        };
        assert!(handshake(req).is_err());
    }

    #[test]
    fn tool_catalog_has_expected_entries() {
        let names: Vec<&str> = tool_catalog().iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            vec![
                "orch.ready",
                "orch.dispatch",
                "orch.poll",
                "orch.close",
                "orch.list_active"
            ]
        );
    }
}
