//! Chrome native messaging length-prefixed JSON frames (little-endian u32).

use std::io::{self, Read, Write};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FramingError {
    #[error("message length {0} exceeds limit {1}")]
    TooLarge(u32, u32),
    #[error("empty message")]
    Empty,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("invalid UTF-8 in message body")]
    Utf8,
}

pub const MAX_NATIVE_MESSAGE_BYTES: u32 = 1024 * 1024;

pub fn read_frame<R: Read>(mut r: R) -> Result<Vec<u8>, FramingError> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf);
    if len == 0 {
        return Err(FramingError::Empty);
    }
    if len > MAX_NATIVE_MESSAGE_BYTES {
        return Err(FramingError::TooLarge(len, MAX_NATIVE_MESSAGE_BYTES));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)?;
    Ok(body)
}

pub fn write_frame<W: Write>(mut w: W, body: &[u8]) -> Result<(), FramingError> {
    if body.is_empty() {
        return Err(FramingError::Empty);
    }
    let len = body.len();
    if len as u32 > MAX_NATIVE_MESSAGE_BYTES {
        return Err(FramingError::TooLarge(len as u32, MAX_NATIVE_MESSAGE_BYTES));
    }
    w.write_all(&(len as u32).to_le_bytes())?;
    w.write_all(body)?;
    w.flush()?;
    Ok(())
}

pub fn read_json_frame<R: Read>(r: R) -> Result<serde_json::Value, FramingError> {
    let body = read_frame(r)?;
    let text = std::str::from_utf8(&body).map_err(|_| FramingError::Utf8)?;
    serde_json::from_str(text)
        .map_err(|e| FramingError::Io(io::Error::new(io::ErrorKind::InvalidData, e)))
}

pub fn write_json_frame<W: Write>(w: W, value: &serde_json::Value) -> Result<(), FramingError> {
    let body = serde_json::to_vec(value)
        .map_err(|e| FramingError::Io(io::Error::new(io::ErrorKind::InvalidData, e)))?;
    write_frame(w, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_frame() {
        let payload = br#"{"type":"ping"}"#;
        let mut buf = Vec::new();
        write_frame(&mut buf, payload).unwrap();
        let out = read_frame(buf.as_slice()).unwrap();
        assert_eq!(out, payload);
    }

    #[test]
    fn rejects_oversized() {
        let huge = vec![0u8; MAX_NATIVE_MESSAGE_BYTES as usize + 1];
        let mut buf = Vec::new();
        assert!(matches!(
            write_frame(&mut buf, &huge),
            Err(FramingError::TooLarge(_, _))
        ));
    }

    #[test]
    fn json_round_trip() {
        let v = serde_json::json!({ "ok": true, "n": 1 });
        let mut buf = Vec::new();
        write_json_frame(&mut buf, &v).unwrap();
        let back = read_json_frame(buf.as_slice()).unwrap();
        assert_eq!(back, v);
    }
}
