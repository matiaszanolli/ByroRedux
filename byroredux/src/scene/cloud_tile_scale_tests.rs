//! Regression tests for [`cloud_tile_scale_for_dds`] — issue #529.
//!
//! WTHR records ship cloud TEXTURE paths but no authored
//! `cloud_scale` field (`weather.rs`'s `WeatherRecord` has no such field,
//! and `parse_weather_data`'s named-offset ladder accounts for every
//! `DATA` byte it reads with none left over for one). The audit
//! (FNV-CELL-5) hedged on whether the format carried scale; verifying
//! against `weather.rs` confirmed it does not. **Corrected (#4015)**:
//! this doc previously named `DATA` bytes 1-2 as `cloud_speed_lower`/
//! `cloud_speed_upper` and cited the `weather.rs` DATA arm as the
//! decoder — that arm has never decoded those bytes (they are
//! deliberately unread, see `WTHR_TRANSITION_DELTA_OFFSET`'s doc), so
//! the citation resolved to nothing. Per-layer cloud motion for FO3/FNV
//! comes from `ONAM`, not `DATA`.
//!
//! Per-WTHR authority over cloud density therefore comes from the
//! authored DDS *width* — a 1024² sprite tiles half as often as a
//! 512², a 256² sprite tiles twice as often. The pure helper
//! `cloud_tile_scale_for_dds` does that math; these tests pin it.
//!
//! Pre-#529 the per-layer baseline was inlined as `0.15` / `0.20`
//! / `0.25` / `0.30` and identical for every WTHR regardless of
//! the sprite the artist authored.
use super::{
    cloud_tile_scale_for_dds, CLOUD_TILE_SCALE_LAYER_0, CLOUD_TILE_SCALE_LAYER_1,
    CLOUD_TILE_SCALE_LAYER_2, CLOUD_TILE_SCALE_LAYER_3,
};

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

/// Rust mirror of `sky.glsl`'s `cloud_layer_lod` offset (#4230): the mip
/// offset a layer gets relative to the shared elevation term, for a sprite of
/// `width` texels with the tile scale the host would derive for it.
fn cloud_lod_offset(baseline: f32, width: u32) -> f32 {
    let tile_scale = cloud_tile_scale_for_dds(&make_dds_header(width, width), baseline);
    (tile_scale * width as f32
        / (byroredux_renderer::shader_constants::CLOUD_TILE_SCALE_LAYER_0
            * byroredux_renderer::shader_constants::CLOUD_REF_WIDTH))
        .log2()
}

/// #4230 — layer 0 keeps the look the elevation term was tuned on, at any
/// sprite resolution. That is the constraint the offset has to meet before it
/// is allowed to change anything else.
#[test]
fn layer_zero_cloud_lod_is_unchanged_at_every_sprite_size() {
    for width in [256, 512, 1024, 2048] {
        let offset = cloud_lod_offset(CLOUD_TILE_SCALE_LAYER_0, width);
        assert!(
            offset.abs() < 1e-5,
            "{width}^2 layer 0 moved by {offset} mips"
        );
    }
}

/// #4230 — the finer decks get the offset their frequency calls for, and it
/// depends only on the deck, not on the sprite the artist authored.
///
/// The second half is why the offset keys off `tile_scale * width` rather than
/// `tile_scale` alone, as the issue's suggested formula did: the host already
/// divides `tile_scale` by the sprite width, so a raw-`tile_scale` offset
/// would read a 1024^2 sprite as a coarser deck and sample it a mip *sharper*
/// than an equally dense 512^2 one.
#[test]
fn finer_cloud_decks_get_a_resolution_independent_mip_offset() {
    let expected = [
        (CLOUD_TILE_SCALE_LAYER_1, (0.20f32 / 0.15).log2()),
        (CLOUD_TILE_SCALE_LAYER_2, (0.25f32 / 0.15).log2()),
        (CLOUD_TILE_SCALE_LAYER_3, 1.0),
    ];
    for (baseline, want) in expected {
        for width in [256, 512, 1024] {
            let got = cloud_lod_offset(baseline, width);
            assert!(
                (got - want).abs() < 1e-4,
                "baseline {baseline} at {width}^2: offset {got}, expected {want}"
            );
        }
    }
    // The deepest deck is the "up to ~1 mip sharper" the issue measured.
    assert!((cloud_lod_offset(CLOUD_TILE_SCALE_LAYER_3, 512) - 1.0).abs() < 1e-5);
}

/// #4230 — every cloud layer's sample goes through the per-layer LOD, and the
/// shader's formula is the one the tests above mirror.
#[test]
fn every_cloud_layer_samples_with_its_own_lod() {
    let sky = include_str!("../../../crates/renderer/shaders/include/sky.glsl");
    for (idx, scale) in [
        ("cloud_idx", "tile_scale"),
        ("cloud_idx_1", "tile_scale_1"),
        ("cloud_idx_2", "tile_scale_2"),
        ("cloud_idx_3", "tile_scale_3"),
    ] {
        let call = format!("cloud_layer_lod(cloud_lod, {scale}, {idx})");
        assert!(
            sky.contains(&call),
            "sky.glsl must sample layer `{idx}` via `{call}`"
        );
    }
    assert!(
        !sky.contains(", cloud_lod);"),
        "a cloud layer still samples the shared layer-0 LOD directly"
    );
    assert!(
        sky.contains("log2(tile_scale * width / (CLOUD_TILE_SCALE_LAYER_0 * CLOUD_REF_WIDTH))"),
        "sky.glsl's per-layer offset no longer matches the formula these tests mirror"
    );
}
