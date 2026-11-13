//! Forward native-messaging requests to the desktop app named pipe.

use crate::handoff::{PipeEnvelope, PipeMessage};
use crate::HANDOFF_PIPE_NAME;
use serde_json::Value;
use std::io::{Read, Write};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipeError {
    #[error("desktop pipe not available: {0}")]
    Unavailable(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub fn forward_to_desktop(token: &str, message: PipeMessage) -> Result<Value, PipeError> {
    let envelope = PipeEnvelope {
        token: token.to_string(),
        message,
    };
    let payload = serde_json::to_string(&envelope)?;

    #[cfg(windows)]
    {
        use std::fs::OpenOptions;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(HANDOFF_PIPE_NAME)
            .map_err(|e| PipeError::Unavailable(e.to_string()))?;
        file.write_all(payload.as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()?;

        let mut resp = String::new();
        file.read_to_string(&mut resp)?;
        let line = resp.lines().next().unwrap_or("").trim();
        if line.is_empty() {
            return Ok(serde_json::json!({ "ok": true }));
        }
        Ok(serde_json::from_str(line)?)
    }

    #[cfg(not(windows))]
    {
        let _ = (token, payload);
        Err(PipeError::Unavailable("named pipe only on Windows".into()))
    }
}
