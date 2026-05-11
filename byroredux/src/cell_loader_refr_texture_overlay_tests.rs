//! Tests for `refr_texture_overlay_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`refr_texture_overlay_tests::FOO`).

//! Regression tests for #584 — REFR override sub-records (XATO,
//! XTNM, XTXR) must resolve against `EsmCellIndex.texture_sets`
//! and build a `RefrTextureOverlay` the spawn path consumes.
//! The MNAM-only TXST path preserves the material_path so the
//! cell_loader can chain-resolve via `MaterialProvider` (the BGSM
//! resolution is covered separately — here we verify the overlay
//! builder passes the path through).
use super::*;
use byroredux_core::string::{FixedString, StringPool};
use byroredux_plugin::esm::cell::{EsmCellIndex, PlacedRef, TextureSet, TextureSlotSwap};

/// Resolve a path handle through the `StringPool`. The pool lowercases
/// on intern (Gamebryo `GlobalStringTable` semantic) so test assertions
/// compare against the canonical lowercase form. See #609.
fn resolved<'a>(pool: &'a StringPool, sym: Option<FixedString>) -> Option<&'a str> {
    sym.and_then(|s| pool.resolve(s))
}

fn empty_placed_ref(base_form_id: u32) -> PlacedRef {
    PlacedRef {
        base_form_id,
        position: [0.0; 3],
        rotation: [0.0; 3],
        scale: 1.0,
        enable_parent: None,
        teleport: None,
        primitive: None,
        linked_refs: Vec::new(),
        rooms: Vec::new(),
        portals: Vec::new(),
        radius_override: None,
        alt_texture_ref: None,
        land_texture_ref: None,
        texture_slot_swaps: Vec::new(),
        emissive_light_ref: None,
        ownership: None,
    }
}

#[test]
fn build_overlay_returns_none_when_refr_has_no_overrides() {
    let index = EsmCellIndex::default();
    let placed = empty_placed_ref(0x0100_0001);
    let mut pool = StringPool::new();
    assert!(build_refr_texture_overlay(&placed, &index, None, &mut pool).is_none());
}

#[test]
fn build_overlay_carries_mnam_only_txst_material_path() {
    let mut index = EsmCellIndex::default();
    // MNAM-only TXST: no TX00..TX07, only material_path.
    index.texture_sets.insert(
        0x0020_0001,
        TextureSet {
            material_path: Some(r"materials\fo4\vault\sign.bgsm".to_string()),
            ..TextureSet::default()
        },
    );
    let mut placed = empty_placed_ref(0x0100_0001);
    placed.alt_texture_ref = Some(0x0020_0001);

    let mut pool = StringPool::new();
    let ov = build_refr_texture_overlay(&placed, &index, None, &mut pool)
        .expect("XATO with MNAM TXST must produce an overlay");
    // Direct slots stay None — the MNAM-only TXST authored none.
    assert!(ov.diffuse.is_none());
    assert!(ov.normal.is_none());
    // material_path propagates unchanged; BGSM chain resolve is a
    // separate stage (mat_provider = None here).
    assert_eq!(
        resolved(&pool, ov.material_path),
        Some(r"materials\fo4\vault\sign.bgsm")
    );
}

#[test]
fn build_overlay_full_txst_fills_every_authored_slot() {
    let mut index = EsmCellIndex::default();
    index.texture_sets.insert(
        0x0020_0001,
        TextureSet {
            diffuse: Some(r"textures\a\diff.dds".to_string()),
            normal: Some(r"textures\a\nrm.dds".to_string()),
            glow: Some(r"textures\a\glow.dds".to_string()),
            specular: Some(r"textures\a\spec.dds".to_string()),
            ..TextureSet::default()
        },
    );
    let mut placed = empty_placed_ref(0x0100_0001);
    placed.alt_texture_ref = Some(0x0020_0001);

    let mut pool = StringPool::new();
    let ov = build_refr_texture_overlay(&placed, &index, None, &mut pool)
        .expect("XATO with populated TXST must produce an overlay");
    assert_eq!(resolved(&pool, ov.diffuse), Some(r"textures\a\diff.dds"));
    assert_eq!(resolved(&pool, ov.normal), Some(r"textures\a\nrm.dds"));
    assert_eq!(resolved(&pool, ov.glow), Some(r"textures\a\glow.dds"));
    assert_eq!(resolved(&pool, ov.specular), Some(r"textures\a\spec.dds"));
    // Unauthored slots stay None so the base mesh's textures ride through.
    assert!(ov.env.is_none());
    assert!(ov.material_path.is_none());
}

#[test]
fn build_overlay_xtxr_swaps_only_the_named_slot() {
    let mut index = EsmCellIndex::default();
    // Source TXST for the XTXR — normal slot populated.
    index.texture_sets.insert(
        0x0020_0002,
        TextureSet {
            normal: Some(r"textures\swap\nrm.dds".to_string()),
            ..TextureSet::default()
        },
    );
    let mut placed = empty_placed_ref(0x0100_0001);
    placed.texture_slot_swaps.push(TextureSlotSwap {
        texture_set: 0x0020_0002,
        slot_index: 1, // normal
    });

    let mut pool = StringPool::new();
    let ov = build_refr_texture_overlay(&placed, &index, None, &mut pool)
        .expect("XTXR alone must produce an overlay");
    assert_eq!(resolved(&pool, ov.normal), Some(r"textures\swap\nrm.dds"));
    // Every other slot stays None.
    assert!(ov.diffuse.is_none());
    assert!(ov.glow.is_none());
    assert!(ov.specular.is_none());
}

#[test]
fn build_overlay_xtxr_later_swap_wins_for_same_slot() {
    let mut index = EsmCellIndex::default();
    index.texture_sets.insert(
        0x0020_0003,
        TextureSet {
            normal: Some(r"textures\first\nrm.dds".to_string()),
            ..TextureSet::default()
        },
    );
    index.texture_sets.insert(
        0x0020_0004,
        TextureSet {
            normal: Some(r"textures\second\nrm.dds".to_string()),
            ..TextureSet::default()
        },
    );
    let mut placed = empty_placed_ref(0x0100_0001);
    placed.texture_slot_swaps.push(TextureSlotSwap {
        texture_set: 0x0020_0003,
        slot_index: 1,
    });
    placed.texture_slot_swaps.push(TextureSlotSwap {
        texture_set: 0x0020_0004,
        slot_index: 1,
    });

    let mut pool = StringPool::new();
    let ov = build_refr_texture_overlay(&placed, &index, None, &mut pool)
        .expect("XTXR swaps must produce an overlay");
    // Authoring-order: later XTXR wins.
    assert_eq!(resolved(&pool, ov.normal), Some(r"textures\second\nrm.dds"));
}

#[test]
fn build_overlay_out_of_range_slot_index_is_silently_dropped() {
    let mut index = EsmCellIndex::default();
    index.texture_sets.insert(
        0x0020_0005,
        TextureSet {
            normal: Some(r"textures\x\nrm.dds".to_string()),
            ..TextureSet::default()
        },
    );
    let mut placed = empty_placed_ref(0x0100_0001);
    placed.texture_slot_swaps.push(TextureSlotSwap {
        texture_set: 0x0020_0005,
        slot_index: 99, // garbage
    });

    let mut pool = StringPool::new();
    let ov = build_refr_texture_overlay(&placed, &index, None, &mut pool)
        .expect("XTXR with bad slot still returns an overlay (empty slots)");
    assert!(ov.diffuse.is_none());
    assert!(ov.normal.is_none());
    assert!(ov.specular.is_none());
}
