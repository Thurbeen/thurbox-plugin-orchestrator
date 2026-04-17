//! Thurbox plugin protocol surface (host-facing).
//!
//! The plugin daemon (binary `thurbox-plugin-gastown`) speaks
//! line-delimited JSON-RPC to thurbox over stdio. This module owns:
//!
//! - the handshake envelope (api_version match against
//!   `PLUGIN_API_VERSION`)
//! - the `mcp.list_tools` / `mcp.call` dispatch table that exposes
//!   the `gastown.*` tools
//! - the `stop` op for orderly shutdown

use crate::gastown::Cli as GastownCli;
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
            name: "gastown.status",
            description: "Show gastown city status (`gt status`).",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDescriptor {
            name: "gastown.list_agents",
            description: "List configured gastown agents as JSON.",
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDescriptor {
            name: "gastown.sling",
            description: "Dispatch a bd item to an agent (`gt sling <agent> <bd-id>`).",
            input_schema: json!({
                "type": "object",
                "required": ["agent", "bd_id"],
                "properties": {
                    "agent": { "type": "string" },
                    "bd_id": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "gastown.show_bead",
            description: "Show a bd item by id (`gt bead show <bd-id>`).",
            input_schema: json!({
                "type": "object",
                "required": ["bd_id"],
                "properties": { "bd_id": { "type": "string" } }
            }),
        },
        ToolDescriptor {
            name: "gastown.tail_events",
            description: "Tail recent gastown events (`gt events tail --since=<dur>`).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "since": { "type": "string", "default": "5m" }
                }
            }),
        },
    ]
}

pub async fn dispatch_tool(cli: &GastownCli, name: &str, params: &Value) -> Result<Value, String> {
    let to_err = |e: anyhow::Error| e.to_string();
    let stdout = match name {
        "gastown.status" => cli.status().await.map_err(to_err)?,
        "gastown.list_agents" => cli.list_agents().await.map_err(to_err)?,
        "gastown.sling" => {
            let agent = required_str(params, "agent")?;
            let bd_id = required_str(params, "bd_id")?;
            cli.sling(agent, bd_id).await.map_err(to_err)?
        }
        "gastown.show_bead" => {
            let bd_id = required_str(params, "bd_id")?;
            cli.show_bead(bd_id).await.map_err(to_err)?
        }
        "gastown.tail_events" => {
            let since = optional_str(params, "since").unwrap_or("5m");
            cli.tail_events(since).await.map_err(to_err)?
        }
        other => return Err(format!("unknown tool {other}")),
    };
    Ok(json!({ "stdout": stdout }))
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
            plugin_name: "gastown".into(),
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
            plugin_name: "gastown".into(),
            effective_configuration: Value::Null,
        };
        assert!(handshake(req).is_err());
    }

    #[test]
    fn tool_catalog_has_expected_entries() {
        let names: Vec<&str> = tool_catalog().iter().map(|t| t.name).collect();
        assert!(names.contains(&"gastown.status"));
        assert!(names.contains(&"gastown.sling"));
        assert!(names.contains(&"gastown.tail_events"));
    }
}
