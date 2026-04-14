//! Length-prefixed JSON wire format.
//!
//! Frame layout: `[4-byte BE length][UTF-8 JSON payload]`

use serde::{de::DeserializeOwned, Serialize};
use std::io::{self, Read, Write};

/// Maximum message size (16 MB) — sanity check against corrupt streams.
const MAX_MESSAGE_SIZE: u32 = 16 * 1024 * 1024;

/// Encode a message as length-prefixed JSON bytes.
pub fn encode<T: Serialize>(msg: &T) -> io::Result<Vec<u8>> {
    let json =
        serde_json::to_vec(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = json.len() as u32;
    let mut buf = Vec::with_capacity(4 + json.len());
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(&json);
    Ok(buf)
}

/// Read one length-prefixed JSON message from a stream.
pub fn decode<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf);

    if len > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "message too large: {} bytes (max {})",
                len, MAX_MESSAGE_SIZE
            ),
        ));
    }

    let mut payload = vec![0u8; len as usize];
    reader.read_exact(&mut payload)?;

    serde_json::from_slice(&payload).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Write one length-prefixed JSON message to a stream and flush.
pub fn send<T: Serialize>(writer: &mut impl Write, msg: &T) -> io::Result<()> {
    let buf = encode(msg)?;
    writer.write_all(&buf)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DebugRequest, DebugResponse};
    use std::io::Cursor;

    #[test]
    fn round_trip_request() {
        let req = DebugRequest::Eval {
            expr: "42.Transform".to_string(),
        };
        let encoded = encode(&req).unwrap();
        let decoded: DebugRequest = decode(&mut Cursor::new(&encoded)).unwrap();
        match decoded {
            DebugRequest::Eval { expr } => assert_eq!(expr, "42.Transform"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_response() {
        let resp = DebugResponse::Stats {
            fps: 60.0,
            avg_fps: 59.5,
            frame_time_ms: 16.6,
            entity_count: 100,
            mesh_count: 50,
            texture_count: 30,
            draw_call_count: 200,
        };
        let encoded = encode(&resp).unwrap();
        let decoded: DebugResponse = decode(&mut Cursor::new(&encoded)).unwrap();
        match decoded {
            DebugResponse::Stats { fps, .. } => assert!((fps - 60.0).abs() < 0.01),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_ping_pong() {
        let req = DebugRequest::Ping;
        let buf = encode(&req).unwrap();
        let decoded: DebugRequest = decode(&mut Cursor::new(&buf)).unwrap();
        assert!(matches!(decoded, DebugRequest::Ping));
    }

    #[test]
    fn rejects_oversized_message() {
        let fake_len = (MAX_MESSAGE_SIZE + 1).to_be_bytes();
        let mut cursor = Cursor::new(fake_len.to_vec());
        let result: io::Result<DebugRequest> = decode(&mut cursor);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("message too large"));
    }
}
