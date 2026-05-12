//! Regression tests for [`cloud_tile_scale_for_dds`] — issue #529.
//!
//! WTHR records ship cloud TEXTURE paths but no authored
//! `cloud_scale` field (DATA bytes 1-2 are cloud_speed_lower /
//! cloud_speed_upper, NOT scales — see `weather.rs` DATA arm).
//! The audit (FNV-CELL-5) hedged on whether the format carried
//! scale; verifying against `weather.rs` confirmed it does not.
//!
//! Per-WTHR authority over cloud density therefore comes from the
//! authored DDS *width* — a 1024² sprite tiles half as often as a
//! 512², a 256² sprite tiles twice as often. The pure helper
//! `cloud_tile_scale_for_dds` does that math; these tests pin it.
//!
//! Pre-#529 the per-layer baseline was inlined as `0.15` / `0.20`
//! / `0.25` / `0.30` and identical for every WTHR regardless of
//! the sprite the artist authored.
use super::{cloud_tile_scale_for_dds, CLOUD_TILE_SCALE_LAYER_0, CLOUD_TILE_SCALE_LAYER_1};

/// Build a minimal DDS file with just enough of a header for
/// `parse_dds` to read width / height / a recognised pixel format.
/// Uses the BC1/DXT1 fast-path so we don't need the DX10 extended
/// header (which would add 20 B and a DXGI format code).
fn make_dds_header(width: u32, height: u32) -> Vec<u8> {
    let mut buf = vec![0u8; 128];
    // Magic 'DDS '
    buf[0..4].copy_from_slice(b"DDS ");
    // DDS_HEADER size (124) at offset 4
    buf[4..8].copy_from_slice(&124u32.to_le_bytes());
    // Height @ 12, width @ 16
    buf[12..16].copy_from_slice(&height.to_le_bytes());
    buf[16..20].copy_from_slice(&width.to_le_bytes());
    // mip_count @ 28 = 1
    buf[28..32].copy_from_slice(&1u32.to_le_bytes());
    // pf_flags @ 80 = DDPF_FOURCC (0x4)
    buf[80..84].copy_from_slice(&0x4u32.to_le_bytes());
    // pf_fourcc @ 84 = 'DXT1'
    buf[84..88].copy_from_slice(b"DXT1");
    buf
}

#[test]
fn reference_512_returns_baseline_unchanged() {
    let dds = make_dds_header(512, 512);
    // 512² is the reference resolution → scale must equal baseline
    // exactly so existing fixtures and live cloud rendering at the
    // canonical width are bit-identical to pre-#529 behaviour.
    let s = cloud_tile_scale_for_dds(&dds, CLOUD_TILE_SCALE_LAYER_0);
    assert!((s - CLOUD_TILE_SCALE_LAYER_0).abs() < 1e-6, "got {}", s);
}

#[test]
fn higher_resolution_lowers_tile_scale() {
    // 1024² → half the tile scale → twice the on-screen blob size,
    // preserving the artist's authored detail. Without this fix a
    // sharp 1024 cloud would be tiled as densely as a 512² sprite,
    // squashing every blob to 256 px on screen.
    let dds = make_dds_header(1024, 1024);
    let s = cloud_tile_scale_for_dds(&dds, CLOUD_TILE_SCALE_LAYER_0);
    assert!(
        (s - CLOUD_TILE_SCALE_LAYER_0 * 0.5).abs() < 1e-6,
        "got {}",
        s
    );
}

#[test]
fn lower_resolution_raises_tile_scale() {
    // 256² → twice the tile scale → twice as many tiled instances,
    // preserving on-screen blob density when the artist authored
    // a coarser sprite (some Oblivion DLC clouds ship at 256²).
    let dds = make_dds_header(256, 256);
    let s = cloud_tile_scale_for_dds(&dds, CLOUD_TILE_SCALE_LAYER_1);
    assert!(
        (s - CLOUD_TILE_SCALE_LAYER_1 * 2.0).abs() < 1e-6,
        "got {}",
        s
    );
}

#[test]
fn malformed_dds_falls_back_to_baseline() {
    // Garbage bytes → parse_dds errors → fall back to baseline so
    // the cloud still renders at the per-layer reference density
    // rather than disappearing or rendering at scale 0.
    let bogus = vec![0xFFu8; 128];
    let s = cloud_tile_scale_for_dds(&bogus, CLOUD_TILE_SCALE_LAYER_0);
    assert!((s - CLOUD_TILE_SCALE_LAYER_0).abs() < 1e-6, "got {}", s);
}

#[test]
fn truncated_dds_falls_back_to_baseline() {
    // Header shorter than 128 B → parse_dds errors → baseline.
    let truncated = vec![0u8; 32];
    let s = cloud_tile_scale_for_dds(&truncated, CLOUD_TILE_SCALE_LAYER_0);
    assert!((s - CLOUD_TILE_SCALE_LAYER_0).abs() < 1e-6, "got {}", s);
}
