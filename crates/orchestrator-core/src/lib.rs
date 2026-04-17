//! Shared library for the thurbox-plugin-orchestrator workspace.
//!
//! - `bd` — async wrapper around the `bd` (beads) CLI.
//! - `jsonrpc` — line-delimited JSON-RPC framing over stdio.
//! - `orch` — orchestration logic: dispatch ready beads to thurbox sessions.
//! - `plugin` — thurbox plugin protocol surface (handshake, tool catalog).
//! - `sentinel` — `===RESULT===` parser used by workers to signal completion.
//! - `thurbox` — long-lived `thurbox-mcp` subprocess client.

pub mod bd;
pub mod jsonrpc;
pub mod orch;
pub mod plugin;
pub mod sentinel;
pub mod thurbox;
