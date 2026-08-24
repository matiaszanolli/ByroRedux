//! Worldspace RT-precision predicate + SpeedTree import tests.
//!
//! Extracted from `references/mod.rs`'s inline `mod tests`
//! (#2409 / TD1-006). Contents unchanged.

use super::*;
// Test-only symbols not referenced by production code in this module
// (they'd warn as unused at file scope). #1877 split.
use byroredux_core::string::StringPool;

/// #1495 / REN2-10 — the RT absolute-space precision ceiling guard.
/// Empty cells must not trip (bounds left at ±INF); vanilla-scale
/// extents are clear; a mega-worldspace past 2^20 is flagged with its
/// extent; the bound is inclusive.
#[test]
fn worldspace_extent_ceiling_predicate() {
    // Empty cell — bounds never accumulated, still ±INF → None.
    assert_eq!(
        worldspace_extent_over_rt_ceiling(
            Vec3::splat(f32::INFINITY),
            Vec3::splat(f32::NEG_INFINITY),
        ),
        None,
    );
    // Vanilla exterior corner (~±233k, Skyrim Tamriel) — clear.
    assert_eq!(
        worldspace_extent_over_rt_ceiling(Vec3::splat(-233_000.0), Vec3::splat(233_000.0),),
        None,
    );
    // Mega-worldspace past 2^20 — flagged, returns the max |coord|.
    assert_eq!(
        worldspace_extent_over_rt_ceiling(
            Vec3::new(-1_200_000.0, 0.0, 0.0),
            Vec3::new(50.0, 50.0, 50.0),
        ),
        Some(1_200_000.0),
    );
    // Inclusive at the ceiling itself.
    assert!(worldspace_extent_over_rt_ceiling(
        Vec3::ZERO,
        Vec3::splat(RT_ABSOLUTE_PRECISION_CEILING),
    )
    .is_some());
}

/// Minimal vanilla-shaped `.spt` byte stream: 20-byte magic + one
/// section marker tag + an out-of-range u32 sentinel so the walker
/// stops cleanly at the geometry-tail boundary.
fn minimal_spt_bytes() -> Vec<u8> {
    // Magic header (`E8 03 00 00 0C 00 00 00 __IdvSpt_02_`).
    let mut bytes = vec![0xE8, 0x03, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00];
    bytes.extend_from_slice(b"__IdvSpt_02_");
    // Single bare-marker tag (`1002` is in the bare set).
    bytes.extend_from_slice(&1002u32.to_le_bytes());
    // Tail sentinel — out-of-range u32 so the walker stops cleanly.
    bytes.extend_from_slice(&0x4E25u32.to_le_bytes());
    bytes
}

/// #3076 regression — the renderable placeholder mesh, not its
/// non-renderable placement root, carries the billboard mode.
#[test]
fn parse_and_import_spt_surfaces_billboard_mode_on_mesh() {
    let bytes = minimal_spt_bytes();
    let mut pool = StringPool::new();
    let cached = parse_and_import_spt(&bytes, "trees\\test.spt", None, &mut pool)
        .expect("minimal spt parses through the importer");
    assert_eq!(cached.placement_root_billboard, None);
    assert_eq!(cached.meshes.len(), 1, "single placeholder quad");
    assert_eq!(cached.meshes[0].billboard_mode, Some(5));
}

#[test]
fn parse_and_import_spt_does_not_guess_wind_from_tree_cnam() {
    let bytes = minimal_spt_bytes();
    let tree = byroredux_plugin::esm::records::TreeRecord {
        canopy_params: vec![2.0, 0.25],
        ..Default::default()
    };
    let mut pool = StringPool::new();
    let cached = parse_and_import_spt(&bytes, "trees\\windy.spt", Some(&tree), &mut pool)
        .expect("minimal spt parses through the importer");
    assert_eq!(cached.speedtree_wind, Some((1.0, 0.0)));
}

#[test]
fn malformed_spt_still_produces_placeholder() {
    let mut pool = StringPool::new();
    let cached = parse_and_import_spt(b"not an spt", "trees\\broken.spt", None, &mut pool)
        .expect("parse failure must degrade to a placeholder");
    assert_eq!(cached.meshes.len(), 1);
    assert_eq!(cached.meshes[0].billboard_mode, Some(5));
}

/// #1820 / SPT-NEW-01 — pins the logged sanity check
/// `parse_and_import_spt` now computes via `detect_variant`. The
/// call itself can't be observed without a log-capturing dependency
/// (none exists in this workspace), so this asserts the value the
/// production code path would log for the same fixture bytes
/// `parse_and_import_spt_surfaces_billboard_mode_on_mesh`
/// exercises above — a vanilla `__IdvSpt_02_`-prefixed stream
/// resolves to `V5Fnv` per `detect_variant`'s documented default.
#[test]
fn minimal_spt_fixture_detects_as_v5fnv_variant() {
    let bytes = minimal_spt_bytes();
    assert_eq!(
        byroredux_spt::detect_variant(&bytes),
        byroredux_spt::SpeedTreeVariant::V5Fnv,
        "the same bytes parse_and_import_spt's logged sanity check \
         receives must resolve to V5Fnv, matching MAGIC_HEAD's \
         documented default",
    );
}
