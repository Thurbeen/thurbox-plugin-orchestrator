//! Line-delimited JSON-RPC framing shared by the thurbox plugin protocol
//! and the gastown exec session provider protocol.
//!
//! Envelope:
//!
//! - Request: `{"id": N, "op": "<name>", "params": {...}}`
//! - Response: `{"id": N, "ok": true, "result": {...}}`
//!   or `{"id": N, "ok": false, "error": "..."}`
//! - Notification: `{"op": "<name>", "params": {...}}` (no `id`)
//!
//! Each message is a single line of JSON terminated by `\n`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub id: u64,
    pub op: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Notification {
    pub op: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Incoming {
    Response(Response),
    Request(Request),
    Notification(Notification),
}

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, error: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            result: None,
            error: Some(error.into()),
        }
    }
}

pub async fn write_frame<W, T>(writer: &mut W, frame: &T) -> std::result::Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let mut bytes = serde_json::to_vec(frame)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_frame<R, T>(
    reader: &mut BufReader<R>,
) -> std::result::Result<Option<T>, FrameError>
where
    R: tokio::io::AsyncRead + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Ok(None);
    }
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        return Ok(None);
    }
    let frame = serde_json::from_str(trimmed)?;
    Ok(Some(frame))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn round_trips_request() {
        let req = Request {
            id: 1,
            op: "ping".into(),
            params: json!({"x": 1}),
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &req).await.unwrap();
        assert_eq!(buf.last().copied(), Some(b'\n'));

        let mut reader = BufReader::new(buf.as_slice());
        let got: Request = read_frame(&mut reader).await.unwrap().unwrap();
        assert_eq!(got, req);
    }

    #[tokio::test]
    async fn round_trips_response_ok_and_err() {
        for resp in [Response::ok(7, json!("hi")), Response::err(8, "boom")] {
            let mut buf = Vec::new();
            write_frame(&mut buf, &resp).await.unwrap();
            let mut reader = BufReader::new(buf.as_slice());
            let got: Response = read_frame(&mut reader).await.unwrap().unwrap();
            assert_eq!(got, resp);
        }
    }

    #[tokio::test]
    async fn read_frame_returns_none_on_eof() {
        let empty: &[u8] = b"";
        let mut reader = BufReader::new(empty);
        let got: Option<Request> = read_frame(&mut reader).await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn handles_missing_trailing_newline() {
        let line = br#"{"id":1,"op":"x","params":null}"#;
        let mut reader = BufReader::new(&line[..]);
        let got: Request = read_frame(&mut reader).await.unwrap().unwrap();
        assert_eq!(got.op, "x");
    }
}
