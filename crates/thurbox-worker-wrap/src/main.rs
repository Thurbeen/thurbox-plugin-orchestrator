//! `thurbox-worker-wrap` — gastown `session_setup_script` helper.
//!
//! Polls a thurbox session for the worker sentinel
//! (`===RESULT===` + JSON line) and, on `status:"ok"`, closes the
//! associated bd item.

use anyhow::{Context, Result};
use clap::Parser;
use orchestrator_core::sentinel::{self, Outcome};
use orchestrator_core::thurbox::Client as ThurboxClient;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::sleep;

const THURBOX_MCP: &str = "thurbox-mcp";

#[derive(Parser, Debug)]
#[command(
    name = "thurbox-worker-wrap",
    version,
    about = "Poll a thurbox session for ===RESULT=== and close its bd item"
)]
struct Cli {
    /// Thurbox session UUID to poll.
    #[arg(long)]
    session: String,

    /// Bead id to close on success (passed to `bd close`).
    #[arg(long)]
    bd_id: String,

    /// Path to the bd database.
    #[arg(
        long,
        default_value = "/home/magicletur/.local/share/thurbox/admin/.beads/"
    )]
    bd_db: String,

    /// Maximum seconds to poll before giving up.
    #[arg(long, default_value_t = 1800)]
    timeout_secs: u64,

    /// Seconds between polls.
    #[arg(long, default_value_t = 5)]
    interval_secs: u64,

    /// Lines to capture per poll.
    #[arg(long, default_value_t = 400)]
    lines: u32,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = Cli::parse();
    let client = ThurboxClient::spawn(THURBOX_MCP).await?;

    let deadline = std::time::Instant::now() + Duration::from_secs(args.timeout_secs);
    let interval = Duration::from_secs(args.interval_secs);

    loop {
        let output = client
            .capture_session_output(&args.session, args.lines)
            .await?;
        match sentinel::parse(&output) {
            Outcome::Found(result) if result.status == "ok" => {
                close_bd(&args.bd_db, &args.bd_id, &result.notes).await?;
                client.shutdown().await;
                return Ok(());
            }
            Outcome::Found(result) => {
                eprintln!(
                    "thurbox-worker-wrap: sentinel reported status={}, leaving bd open",
                    result.status
                );
                client.shutdown().await;
                std::process::exit(1);
            }
            Outcome::Malformed(err) => {
                eprintln!("thurbox-worker-wrap: malformed sentinel: {err}");
                client.shutdown().await;
                std::process::exit(1);
            }
            Outcome::NotFound => {
                if std::time::Instant::now() >= deadline {
                    eprintln!(
                        "thurbox-worker-wrap: timed out after {}s with no sentinel",
                        args.timeout_secs
                    );
                    client.shutdown().await;
                    std::process::exit(1);
                }
                sleep(interval).await;
            }
        }
    }
}

async fn close_bd(db: &str, bd_id: &str, reason: &str) -> Result<()> {
    let status = Command::new("bd")
        .arg("--db")
        .arg(db)
        .arg("close")
        .arg(bd_id)
        .arg("--reason")
        .arg(reason)
        .status()
        .await
        .context("invoking bd close")?;
    if !status.success() {
        anyhow::bail!("bd close exited with {status}");
    }
    Ok(())
}
