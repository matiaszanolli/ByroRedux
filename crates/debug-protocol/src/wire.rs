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
            meshes_in_use: 42,
            textures_in_use: 27,
            draw_command_count: 200,
            batch_count: 80,
            indirect_call_count: 20,
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

    /// Phase 1 — pin every new request variant's JSON shape so an
    /// accidental serde rename (or a field-set drift between the wire
    /// type and the in-engine source-of-truth `MetricsSnapshot`)
    /// breaks the build instead of silently corrupting the wire. The
    /// new variants are exercised together because they share the
    /// same `#[serde(tag = "cmd")]` discriminator and a clash would
    /// only surface across the full set.
    #[test]
    fn round_trip_phase1_request_variants() {
        use crate::AssetKind;

        let cases = vec![
            DebugRequest::Metrics,
            DebugRequest::LoadNif {
                path: "meshes\\architecture\\foo.nif".to_string(),
                label: Some("foo".to_string()),
            },
            DebugRequest::LoadInteriorCell {
                esm: "FalloutNV.esm".to_string(),
                cell: "GSDocMitchellHouse".to_string(),
                masters: vec![],
                bsas: vec!["Fallout - Meshes.bsa".to_string()],
                textures_bsas: vec!["Fallout - Textures.bsa".to_string()],
            },
            DebugRequest::LoadExteriorCell {
                esm: "FalloutNV.esm".to_string(),
                grid_x: -3,
                grid_y: 7,
                radius: 3,
                worldspace: Some("Wasteland".to_string()),
                masters: vec![],
                bsas: vec![],
                textures_bsas: vec![],
            },
            DebugRequest::ListGameProfiles,
            DebugRequest::ListLoadedAssets {
                kind: AssetKind::Textures,
            },
        ];
        for req in cases {
            let encoded = encode(&req).unwrap();
            let decoded: DebugRequest = decode(&mut Cursor::new(&encoded)).unwrap();
            // Compare via debug-formatted shape — covers field names
            // and values without per-variant pattern matches.
            assert_eq!(format!("{:?}", req), format!("{:?}", decoded));
        }
    }

    /// Phase 1 — same lockstep pin for the new response variants.
    /// Adding a field to `DebugResponse::Metrics` without updating
    /// this test (and the matching engine-side `MetricsSnapshot`)
    /// breaks the build at the missing assertion, not at runtime
    /// against a quiet null field.
    #[test]
    fn round_trip_phase1_response_variants() {
        use crate::{AssetItem, AssetKind, GameProfile};

        let cases = vec![
            DebugResponse::Metrics {
                sampled_at_secs: 1_700_000_000,
                cpu_pct: 42.5,
                ram_used_mb: 8192,
                ram_total_mb: 32_768,
                process_ram_mb: 512,
                vram_used_mb: 1024,
                vram_reserved_mb: 1536,
                vram_budget_mb: 12_288,
                gpu_pass_ms: vec![
                    ("skin".to_string(), 0.42),
                    ("skin_blas_refit".to_string(), 1.18),
                    ("taa".to_string(), 0.31),
                ],
            },
            DebugResponse::GameProfiles {
                profiles: vec![GameProfile {
                    key: "fnv".to_string(),
                    name: "Fallout New Vegas".to_string(),
                    root: "/games/fnv".to_string(),
                    esm: "FalloutNV.esm".to_string(),
                    default_bsas: vec!["Fallout - Meshes.bsa".to_string()],
                    default_textures_bsas: vec!["Fallout - Textures.bsa".to_string()],
                    sample_cells: vec!["GSDocMitchellHouse".to_string()],
                }],
            },
            DebugResponse::AssetList {
                asset_kind: AssetKind::Meshes,
                items: vec![AssetItem {
                    handle: 7,
                    path: Some("meshes\\foo.nif".to_string()),
                    bytes: None,
                    summary: Some("1024 verts / 3072 idx".to_string()),
                }],
            },
        ];
        for resp in cases {
            let encoded = encode(&resp).unwrap();
            let decoded: DebugResponse = decode(&mut Cursor::new(&encoded)).unwrap();
            assert_eq!(format!("{:?}", resp), format!("{:?}", decoded));
        }
    }
}
