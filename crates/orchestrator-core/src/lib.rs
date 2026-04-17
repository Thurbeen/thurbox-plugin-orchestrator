//! gastown-bridge core library.
//!
//! Shared modules used by the three binaries in this workspace:
//!
//! - `thurbox-plugin-gastown` — long-running thurbox plugin daemon that
//!   exposes `gastown.*` MCP tools over the plugin JSON-RPC protocol.
//! - `gc-session-thurbox` — gastown exec session provider that spawns
//!   workers inside thurbox sessions.
//! - `thurbox-worker-wrap` — `session_setup_script` that polls a
//!   thurbox session for a sentinel and closes the bd item.

pub mod gastown;
pub mod jsonrpc;
pub mod plugin;
pub mod rig;
pub mod sentinel;
pub mod thurbox;
