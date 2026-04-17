//! `gc-session-thurbox` — gastown exec session provider.
//!
//! Fork/exec'd once per op by gastown. Argv shape:
//! `<op> <session-name> [extra args]`. See
//! [exec-session-provider](https://github.com/gastownhall/gascity/blob/HEAD/docs/reference/exec-session-provider.md).
//!
//! Exit codes: 0 ok, 1 error, 2 unsupported op.

use anyhow::Result;
use orchestrator_core::rig::{session_state_dir, Op, StartConfig};
use orchestrator_core::thurbox::Client as ThurboxClient;
use std::process::ExitCode;
use tokio::io::{stdin, AsyncReadExt};

const THURBOX_MCP: &str = "thurbox-mcp";
const PEEK_DEFAULT_LINES: u32 = 200;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(op) = args.first() else {
        eprintln!("gc-session-thurbox: missing op argument");
        return ExitCode::from(1);
    };
    let op = Op::from_argv(op);
    let name = args.get(1).cloned().unwrap_or_default();
    let extra: Vec<String> = args.iter().skip(2).cloned().collect();

    match dispatch(op, &name, &extra).await {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("gc-session-thurbox: {err:#}");
            ExitCode::from(1)
        }
    }
}

async fn dispatch(op: Op, name: &str, extra: &[String]) -> Result<u8> {
    match op {
        Op::Start => op_start(name).await,
        Op::Stop => op_stop(name).await,
        Op::IsRunning => op_is_running(name).await,
        Op::Nudge => op_nudge(name).await,
        Op::Peek => op_peek(name, extra).await,
        Op::SetMeta | Op::GetMeta | Op::RemoveMeta => op_meta(op, name, extra).await,
        Op::ListRunning => op_list_running().await,
        Op::Unsupported => Ok(2),
    }
}

async fn op_start(name: &str) -> Result<u8> {
    let mut buf = Vec::new();
    stdin().read_to_end(&mut buf).await?;
    let cfg: StartConfig = if buf.is_empty() {
        StartConfig::default()
    } else {
        serde_json::from_slice(&buf)?
    };
    let client = ThurboxClient::spawn(THURBOX_MCP).await?;
    let info = client.create_session(name, cfg.work_dir.as_str()).await?;
    let dir = session_state_dir(name);
    tokio::fs::create_dir_all(&dir).await?;
    tokio::fs::write(dir.join("session_id"), &info.id).await?;
    if !cfg.nudge.is_empty() {
        client.send_prompt(&info.id, &cfg.nudge).await?;
    }
    client.shutdown().await;
    Ok(0)
}

async fn op_stop(name: &str) -> Result<u8> {
    let id_path = session_state_dir(name).join("session_id");
    if !id_path.exists() {
        return Ok(0);
    }
    let id = tokio::fs::read_to_string(&id_path).await?;
    let id = id.trim();
    let client = ThurboxClient::spawn(THURBOX_MCP).await?;
    let _ = client.delete_session(id).await;
    client.shutdown().await;
    Ok(0)
}

async fn op_is_running(name: &str) -> Result<u8> {
    let id_path = session_state_dir(name).join("session_id");
    if !id_path.exists() {
        println!("false");
        return Ok(0);
    }
    let id = tokio::fs::read_to_string(&id_path).await?;
    let id = id.trim();
    let client = ThurboxClient::spawn(THURBOX_MCP).await?;
    let info = client.get_session(id).await?;
    client.shutdown().await;
    println!("{}", info.is_some());
    Ok(0)
}

async fn op_nudge(name: &str) -> Result<u8> {
    let id_path = session_state_dir(name).join("session_id");
    let id = tokio::fs::read_to_string(&id_path).await?;
    let id = id.trim();
    let mut buf = Vec::new();
    stdin().read_to_end(&mut buf).await?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    let client = ThurboxClient::spawn(THURBOX_MCP).await?;
    client.send_prompt(id, &text).await?;
    client.shutdown().await;
    Ok(0)
}

async fn op_peek(name: &str, extra: &[String]) -> Result<u8> {
    let id_path = session_state_dir(name).join("session_id");
    let id = tokio::fs::read_to_string(&id_path).await?;
    let id = id.trim();
    let lines: u32 = extra
        .first()
        .and_then(|s| s.parse().ok())
        .unwrap_or(PEEK_DEFAULT_LINES);
    let client = ThurboxClient::spawn(THURBOX_MCP).await?;
    let output = client.capture_session_output(id, lines).await?;
    client.shutdown().await;
    print!("{output}");
    Ok(0)
}

async fn op_meta(op: Op, name: &str, extra: &[String]) -> Result<u8> {
    let dir = session_state_dir(name).join("meta");
    tokio::fs::create_dir_all(&dir).await?;
    let key = extra
        .first()
        .ok_or_else(|| anyhow::anyhow!("meta op requires <key> argument"))?;
    let path = dir.join(key);
    match op {
        Op::SetMeta => {
            let mut buf = Vec::new();
            stdin().read_to_end(&mut buf).await?;
            tokio::fs::write(&path, &buf).await?;
        }
        Op::GetMeta => {
            if !path.exists() {
                return Ok(0);
            }
            let v = tokio::fs::read(&path).await?;
            tokio::io::AsyncWriteExt::write_all(&mut tokio::io::stdout(), &v).await?;
        }
        Op::RemoveMeta => {
            let _ = tokio::fs::remove_file(&path).await;
        }
        _ => unreachable!("op_meta called with non-meta op"),
    }
    Ok(0)
}

async fn op_list_running() -> Result<u8> {
    let root = orchestrator_core::rig::state_dir();
    if !root.exists() {
        return Ok(0);
    }
    let mut entries = tokio::fs::read_dir(&root).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                println!("{name}");
            }
        }
    }
    Ok(0)
}
