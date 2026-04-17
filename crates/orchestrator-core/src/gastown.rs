//! Thin shellout wrapper around the gastown CLI (`gt`).
//!
//! MVP set: status, list_agents, sling, show_bead, tail_events.
//! Output is returned as raw stdout; structured parsing is the caller's
//! responsibility. We only fail if the binary is missing or exits
//! non-zero (then we surface stderr).

use anyhow::{anyhow, Context};
use std::process::Stdio;
use tokio::process::Command;

pub const GT_BINARY: &str = "gt";

pub struct Cli {
    binary: String,
    cwd: Option<std::path::PathBuf>,
}

impl Cli {
    pub fn new() -> Self {
        Self {
            binary: GT_BINARY.to_owned(),
            cwd: None,
        }
    }

    pub fn with_binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }

    pub fn with_cwd(mut self, cwd: impl Into<std::path::PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    async fn run(&self, args: &[&str]) -> anyhow::Result<String> {
        let mut cmd = Command::new(&self.binary);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        let output = cmd
            .output()
            .await
            .with_context(|| format!("invoking {} {}", self.binary, args.join(" ")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "{} {} failed: {}",
                self.binary,
                args.join(" "),
                stderr.trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub async fn status(&self) -> anyhow::Result<String> {
        self.run(&["status"]).await
    }

    pub async fn list_agents(&self) -> anyhow::Result<String> {
        self.run(&["list", "agents", "--json"]).await
    }

    pub async fn sling(&self, agent: &str, bd_id: &str) -> anyhow::Result<String> {
        self.run(&["sling", agent, bd_id]).await
    }

    pub async fn show_bead(&self, bd_id: &str) -> anyhow::Result<String> {
        self.run(&["bead", "show", bd_id]).await
    }

    pub async fn tail_events(&self, since: &str) -> anyhow::Result<String> {
        let arg = format!("--since={since}");
        self.run(&["events", "tail", &arg]).await
    }
}

impl Default for Cli {
    fn default() -> Self {
        Self::new()
    }
}
