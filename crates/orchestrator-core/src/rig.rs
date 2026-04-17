//! Gastown exec session provider — op router.
//!
//! See <https://github.com/gastownhall/gascity/blob/HEAD/docs/reference/exec-session-provider.md>.
//!
//! The rig binary (`gc-session-thurbox`) is fork/exec'd once per op.
//! Argument shape: `<op> <session-name> [extra args...]`. Some ops
//! consume stdin (`start`, `nudge`); some emit stdout (`peek`,
//! `is-running`). Exit codes: `0` ok, `1` error, `2` unsupported op.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Ok = 0,
    Err = 1,
    Unsupported = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Start,
    Stop,
    IsRunning,
    Nudge,
    Peek,
    SetMeta,
    GetMeta,
    RemoveMeta,
    ListRunning,
    Unsupported,
}

impl Op {
    pub fn from_argv(s: &str) -> Self {
        match s {
            "start" => Op::Start,
            "stop" => Op::Stop,
            "is-running" => Op::IsRunning,
            "nudge" => Op::Nudge,
            "peek" => Op::Peek,
            "set-meta" => Op::SetMeta,
            "get-meta" => Op::GetMeta,
            "remove-meta" => Op::RemoveMeta,
            "list-running" => Op::ListRunning,
            _ => Op::Unsupported,
        }
    }
}

/// Subset of the gascity `startConfig` JSON we actually consume.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartConfig {
    #[serde(default)]
    pub work_dir: String,
    #[serde(default)]
    pub nudge: String,
    #[serde(default)]
    pub env: Value,
}

/// Resolve the per-session state directory. Honours `$GC_EXEC_STATE_DIR`
/// when set, else falls back to `/tmp/gc-thurbox`.
pub fn state_dir() -> PathBuf {
    std::env::var_os("GC_EXEC_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/gc-thurbox"))
}

pub fn session_state_dir(name: &str) -> PathBuf {
    state_dir().join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_parsing_matches_protocol() {
        assert_eq!(Op::from_argv("start"), Op::Start);
        assert_eq!(Op::from_argv("is-running"), Op::IsRunning);
        assert_eq!(Op::from_argv("set-meta"), Op::SetMeta);
        assert_eq!(Op::from_argv("nope"), Op::Unsupported);
    }

    #[test]
    fn state_dir_defaults_when_env_unset() {
        // SAFETY: tests run single-threaded by default for env mutation.
        // We only assert when var is unset.
        if std::env::var_os("GC_EXEC_STATE_DIR").is_none() {
            assert_eq!(state_dir(), PathBuf::from("/tmp/gc-thurbox"));
        }
    }

    #[test]
    fn session_state_dir_namespaces_under_root() {
        let p = session_state_dir("worker-a");
        assert!(p.ends_with("worker-a"));
    }
}
