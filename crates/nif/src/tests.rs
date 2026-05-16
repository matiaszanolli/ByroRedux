//! Unit + integration tests for `parse_nif` / `parse_nif_with_options`.
//!
//! Lifted verbatim from the pre-#1118 monolithic `lib.rs::tests` module.
//! The `mod tests` wrapper and trailing brace are gone; everything else
//! is byte-identical.

use super::*;

/// Build a complete minimal NIF file (v20.2.0.7, Skyrim-style)
/// containing a single NiNode block with known field values.
fn build_test_nif_with_node() -> Vec<u8> {
    let mut buf = Vec::new();

    // ── Header ──────────────────────────────────────────────────
    buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    buf.extend_from_slice(&0x14020007u32.to_le_bytes()); // version
    buf.push(1); // little-endian
    buf.extend_from_slice(&11u32.to_le_bytes()); // user_version (FNV)
    buf.extend_from_slice(&1u32.to_le_bytes()); // num_blocks = 1
    buf.extend_from_slice(&34u32.to_le_bytes()); // user_version_2 (FNV)

    // Short strings (author, process, export)
    buf.push(1);
    buf.push(0);
    buf.push(1);
    buf.push(0);
    buf.push(1);
    buf.push(0);

    // Block types: 1 type "NiNode"
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&6u32.to_le_bytes());
    buf.extend_from_slice(b"NiNode");

    // Block type indices: block 0 → type 0
    buf.extend_from_slice(&0u16.to_le_bytes());

    // ── Build NiNode block data first to know its size ──────────
    let mut block = Vec::new();

    // NiObjectNET: name (string table index 0 = "SceneRoot")
    block.extend_from_slice(&0i32.to_le_bytes());
    // extra_data_refs: count=0
    block.extend_from_slice(&0u32.to_le_bytes());
    // controller_ref: -1 (null)
    block.extend_from_slice(&(-1i32).to_le_bytes());

    // NiAVObject: flags (u32 for version >= 20.2.0.7)
    block.extend_from_slice(&14u32.to_le_bytes());
    // transform: translation (1.0, 2.0, 3.0)
    block.extend_from_slice(&1.0f32.to_le_bytes());
    block.extend_from_slice(&2.0f32.to_le_bytes());
    block.extend_from_slice(&3.0f32.to_le_bytes());
    // identity rotation (9 floats)
    for r in &[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        block.extend_from_slice(&r.to_le_bytes());
    }
    // scale: 1.0
    block.extend_from_slice(&1.0f32.to_le_bytes());
    // properties: count=0
    block.extend_from_slice(&0u32.to_le_bytes());
    // collision_ref: -1
    block.extend_from_slice(&(-1i32).to_le_bytes());

    // NiNode: children count=0
    block.extend_from_slice(&0u32.to_le_bytes());
    // effects count=0
    block.extend_from_slice(&0u32.to_le_bytes());

    // ── Back to header: block sizes ─────────────────────────────
    buf.extend_from_slice(&(block.len() as u32).to_le_bytes());

    // String table: 1 string "SceneRoot"
    buf.extend_from_slice(&1u32.to_le_bytes()); // num_strings
    buf.extend_from_slice(&9u32.to_le_bytes()); // max_string_length
    buf.extend_from_slice(&9u32.to_le_bytes()); // "SceneRoot" length
    buf.extend_from_slice(b"SceneRoot");

    // num_groups = 0
    buf.extend_from_slice(&0u32.to_le_bytes());

    // ── Block data ──────────────────────────────────────────────
    buf.extend_from_slice(&block);

    buf
}

/// Regression test for issue #175: `NifScene.truncated` defaults to
/// `false` on a happy-path parse, and can be distinguished from a
/// genuinely-truncated scene by downstream consumers. The full
/// end-to-end "Oblivion block parser errors mid-file" path is
/// exercised by the ignored `parse_rate_oblivion` integration test
/// against real .nif corpora — this unit test just pins the public
/// field surface so that a future refactor of the error path can't
/// silently drop the field.
#[test]
fn nif_scene_truncated_flag_defaults_false_on_clean_parse() {
    let data = build_test_nif_with_node();
    let scene = parse_nif(&data).unwrap();
    assert!(
        !scene.truncated,
        "clean parse must not set the truncated flag"
    );
    assert_eq!(scene.len(), 1);
}

#[test]
fn nif_scene_struct_carries_truncated_field() {
    // Hand-constructed marker: verifies the field exists on the
    // struct surface so consumers like `cell_loader` can branch on
    // it without fear of the field being silently removed.
    let scene = NifScene {
        blocks: Vec::new(),
        root_index: None,
        truncated: true,
        dropped_block_count: 3,
        recovered_blocks: 0,
        link_errors: 0,
        drift_histogram: std::collections::BTreeMap::new(),
        stubbed_drift_histogram: std::collections::BTreeMap::new(),
    };
    assert!(scene.truncated);
    assert_eq!(scene.dropped_block_count, 3);
    assert_eq!(scene.recovered_blocks, 0);
    assert_eq!(scene.link_errors, 0);
    assert!(scene.drift_histogram.is_empty());
    assert!(scene.is_empty());
}

/// Regression: #568 (SK-D5-06). A NIF whose header advertises a
/// block type the dispatch table doesn't know lands on
/// `parse_block`'s unknown-type fallback, which returns
/// `Ok(NiUnknown)`. Pre-fix the outer loop silently counted that
/// as a clean parse; the `record_success` path on `nif_stats`
/// kept the headline rate at 100% and hid under-consuming parser
/// bugs like #546. Post-fix `NifScene.recovered_blocks` increments
/// for every such placeholder, and the integration gate routes
/// these scenes through `record_truncated`.
#[test]
fn recovered_blocks_flagged_for_unknown_dispatch_fallback() {
    // Build a minimal NIF whose single block advertises a type
    // name that's NOT in the dispatch table. The parser's
    // dispatch-level unknown-type recovery seeks past via
    // block_size and substitutes an `NiUnknown` placeholder.
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    buf.extend_from_slice(&0x14020007u32.to_le_bytes());
    buf.push(1); // little-endian
    buf.extend_from_slice(&11u32.to_le_bytes()); // user_version (FNV-like)
    buf.extend_from_slice(&1u32.to_le_bytes()); // num_blocks = 1
    buf.extend_from_slice(&34u32.to_le_bytes()); // user_version_2

    // Short strings (author / process / export, each 1 byte empty).
    for _ in 0..3 {
        buf.push(1);
        buf.push(0);
    }

    // 1 block type — "NiImaginaryBlockFromSK-D5-06".
    const UNKNOWN: &str = "NiImaginaryBlockFromSK-D5-06";
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&(UNKNOWN.len() as u32).to_le_bytes());
    buf.extend_from_slice(UNKNOWN.as_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // block 0 → type 0

    // Block payload: 4 arbitrary bytes the parser will skip.
    let block_payload = [0xAAu8, 0xBB, 0xCC, 0xDD];
    buf.extend_from_slice(&(block_payload.len() as u32).to_le_bytes()); // block_size

    // String table: empty.
    buf.extend_from_slice(&0u32.to_le_bytes()); // num_strings
    buf.extend_from_slice(&0u32.to_le_bytes()); // max_string_length
    buf.extend_from_slice(&0u32.to_le_bytes()); // num_groups

    buf.extend_from_slice(&block_payload);

    let scene = parse_nif(&buf).expect("unknown-type fallback must produce Ok");
    assert_eq!(
        scene.len(),
        1,
        "placeholder block still lives at its original index"
    );
    assert_eq!(
        scene.recovered_blocks, 1,
        "unknown dispatch fallback must bump recovered_blocks"
    );
    assert!(
        !scene.truncated,
        "truncated is reserved for blocks dropped past the abort point"
    );
    assert_eq!(
        scene.blocks[0].block_type_name(),
        "NiUnknown",
        "placeholder is an NiUnknown"
    );
}

/// Regression for #611 / SK-D5-02 — `parse_nif` must pick a
/// NiNode-subclass-rooted scene as root, not skip past it to a
/// plain-NiNode child. Pre-fix the predicate was the literal
/// `matches!(block_type_name(), "NiNode")`; this guarantees every
/// dedicated subclass with its own dispatch arm in `parse_block`
/// is also recognised.
///
/// The list mirrors the dedicated subclass dispatch arms in
/// `crate::blocks::parse_block` (around line 144-216). Update both
/// sites when adding a new NiNode-derived block type.
#[test]
fn is_ni_node_subclass_recognises_every_dedicated_subclass() {
    // Plain NiNode + the aliased ones (BSFadeNode, BSLeafAnimNode,
    // RootCollisionNode, AvoidNode, NiBSAnimationNode,
    // NiBSParticleNode) all parse as `NiNode` and report their
    // type as `"NiNode"`, so the single arm covers them all.
    assert!(is_ni_node_subclass("NiNode"));

    // Dedicated subclass parsers — each has its own dispatch arm
    // and reports its own block_type_name. These were the
    // regression surface in #611.
    for name in [
        "BSOrderedNode",
        "BSValueNode",
        "BSMultiBoundNode",
        "BSDistantObjectInstancedNode",
        "BSTreeNode",
        "NiBillboardNode",
        "NiSwitchNode",
        "NiLODNode",
        "NiSortAdjustNode",
        "BSRangeNode",
    ] {
        assert!(
            is_ni_node_subclass(name),
            "{name} must be recognised as a NiNode-subclass for root \
             selection in is_ni_node_subclass()"
        );
    }

    // Negative controls — block types that are NOT NiNode subclasses.
    // A scene rooted at one of these (extremely unusual) would fall
    // back to `Some(0)` (block at index 0) regardless. None of these
    // should match the helper.
    for name in [
        "NiCamera",
        "NiTriShape",
        "BsTriShape",
        "NiSkinPartition",
        "BSLightingShaderProperty",
        "NiAlphaProperty",
        "NiUnknown",
    ] {
        assert!(
            !is_ni_node_subclass(name),
            "{name} must not be recognised as a NiNode-subclass"
        );
    }
}

/// End-to-end regression for #611 / SK-D5-02. The recogniser test
/// above pins the predicate; this one pins the actual root-pick
/// against a synthesised NIF that mirrors vanilla Skyrim tree LODs:
/// `BSTreeNode` at block 0 followed by a plain `NiNode` at block 1.
/// Pre-fix the predicate matched only the literal `"NiNode"` and
/// returned `Some(1)`, causing the importer to descend from a leaf
/// bone container and import 0 of N geometry shapes.
#[test]
fn root_pick_prefers_bstreenode_root_over_plain_ninode_child() {
    // NiNode body — same wire layout used by `build_test_nif_with_node`.
    let mut ninode_body = Vec::new();
    ninode_body.extend_from_slice(&(-1i32).to_le_bytes()); // name index — none
    ninode_body.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    ninode_body.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
    ninode_body.extend_from_slice(&0u32.to_le_bytes()); // flags (u32 @ v20.2.0.7)
    ninode_body.extend_from_slice(&0.0f32.to_le_bytes()); // tx
    ninode_body.extend_from_slice(&0.0f32.to_le_bytes()); // ty
    ninode_body.extend_from_slice(&0.0f32.to_le_bytes()); // tz
    for r in &[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        ninode_body.extend_from_slice(&r.to_le_bytes());
    }
    ninode_body.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    ninode_body.extend_from_slice(&0u32.to_le_bytes()); // properties count
    ninode_body.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
    ninode_body.extend_from_slice(&0u32.to_le_bytes()); // children count
    ninode_body.extend_from_slice(&0u32.to_le_bytes()); // effects count

    // BSTreeNode = NiNode body + two empty bone-ref lists.
    let mut bstreenode_body = ninode_body.clone();
    bstreenode_body.extend_from_slice(&0u32.to_le_bytes()); // num_bones_1
    bstreenode_body.extend_from_slice(&0u32.to_le_bytes()); // num_bones_2

    let mut buf = Vec::new();
    // Header — FNV-style configuration (user_version_2 = bsver = 34).
    // The #611 root-pick bug is bsver-agnostic, but matching FNV keeps
    // the wire layout aligned with `build_test_nif_with_node` (the
    // properties list at bsver<=34 is part of NiAVObject body).
    buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    buf.extend_from_slice(&0x14020007u32.to_le_bytes());
    buf.push(1); // little-endian
    buf.extend_from_slice(&11u32.to_le_bytes()); // user_version (FNV)
    buf.extend_from_slice(&2u32.to_le_bytes()); // num_blocks = 2
    buf.extend_from_slice(&34u32.to_le_bytes()); // user_version_2 (FNV)

    // Three short strings (author / process / export, empty)
    for _ in 0..3 {
        buf.push(1);
        buf.push(0);
    }

    // Block types: 2 — "BSTreeNode" (idx 0), "NiNode" (idx 1)
    buf.extend_from_slice(&2u16.to_le_bytes());
    buf.extend_from_slice(&10u32.to_le_bytes());
    buf.extend_from_slice(b"BSTreeNode");
    buf.extend_from_slice(&6u32.to_le_bytes());
    buf.extend_from_slice(b"NiNode");

    // Block type indices: block 0 → type 0 (BSTreeNode), block 1 → type 1 (NiNode)
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());

    // Block sizes
    buf.extend_from_slice(&(bstreenode_body.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(ninode_body.len() as u32).to_le_bytes());

    // String table — empty
    buf.extend_from_slice(&0u32.to_le_bytes()); // num_strings
    buf.extend_from_slice(&0u32.to_le_bytes()); // max_string_length
    buf.extend_from_slice(&0u32.to_le_bytes()); // num_groups

    // Block data
    buf.extend_from_slice(&bstreenode_body);
    buf.extend_from_slice(&ninode_body);

    let scene = parse_nif(&buf).expect("two-block scene must parse cleanly");
    assert_eq!(scene.len(), 2, "both blocks landed in the scene");
    assert_eq!(
        scene.root_index,
        Some(0),
        "root must be the BSTreeNode at block 0, not the plain NiNode at block 1"
    );
    let root = scene.root().expect("root must resolve");
    assert_eq!(
        root.block_type_name(),
        "BSTreeNode",
        "root_index points at a NiNode subclass, not the trailing plain NiNode"
    );
}

#[test]
fn parse_nif_minimal_node() {
    let data = build_test_nif_with_node();
    let scene = parse_nif(&data).unwrap();

    assert_eq!(scene.len(), 1);
    assert_eq!(scene.root_index, Some(0));

    let root = scene.root().unwrap();
    assert_eq!(root.block_type_name(), "NiNode");

    // Downcast and verify fields
    let node = scene.get_as::<blocks::node::NiNode>(0).unwrap();
    assert_eq!(node.av.net.name.as_deref(), Some("SceneRoot"));
    assert_eq!(node.av.flags, 14);
    assert_eq!(node.av.transform.translation.x, 1.0);
    assert_eq!(node.av.transform.translation.y, 2.0);
    assert_eq!(node.av.transform.translation.z, 3.0);
    assert_eq!(node.av.transform.scale, 1.0);
    assert!(node.children.is_empty());
    assert!(node.effects.is_empty());
    assert!(node.av.net.controller_ref.is_null());
    assert!(node.av.collision_ref.is_null());
}

#[test]
fn parse_nif_empty_file() {
    // Build a NIF with 0 blocks
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    buf.extend_from_slice(&0x14020007u32.to_le_bytes());
    buf.push(1);
    buf.extend_from_slice(&12u32.to_le_bytes()); // user_version
    buf.extend_from_slice(&0u32.to_le_bytes()); // num_blocks = 0
    buf.extend_from_slice(&83u32.to_le_bytes()); // user_version_2

    buf.push(1);
    buf.push(0); // author
    buf.push(1);
    buf.push(0); // process
    buf.push(1);
    buf.push(0); // export

    buf.extend_from_slice(&0u16.to_le_bytes()); // num_block_types
    buf.extend_from_slice(&0u32.to_le_bytes()); // num_strings
    buf.extend_from_slice(&0u32.to_le_bytes()); // max_string_length
    buf.extend_from_slice(&0u32.to_le_bytes()); // num_groups

    let scene = parse_nif(&buf).unwrap();
    assert!(scene.is_empty());
    assert_eq!(scene.root_index, None);
}

#[test]
fn parse_nif_unknown_block_skipped() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    buf.extend_from_slice(&0x14020007u32.to_le_bytes());
    buf.push(1);
    buf.extend_from_slice(&12u32.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes()); // 1 block
    buf.extend_from_slice(&83u32.to_le_bytes());

    buf.push(1);
    buf.push(0);
    buf.push(1);
    buf.push(0);
    buf.push(1);
    buf.push(0);

    // 1 block type: "BSUnknownFutureType"
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&19u32.to_le_bytes());
    buf.extend_from_slice(b"BSUnknownFutureType");

    // Block 0 → type 0
    buf.extend_from_slice(&0u16.to_le_bytes());

    // Block size: 8 bytes of dummy data
    buf.extend_from_slice(&8u32.to_le_bytes());

    // String table: 0 strings
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    // num_groups = 0
    buf.extend_from_slice(&0u32.to_le_bytes());

    // Block data: 8 bytes of garbage
    buf.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE]);

    let scene = parse_nif(&buf).unwrap();
    assert_eq!(scene.len(), 1);
    // Unknown block is preserved as NiUnknown
    assert_eq!(scene.get(0).unwrap().block_type_name(), "NiUnknown");
}

#[test]
fn scene_get_as_wrong_type_returns_none() {
    let data = build_test_nif_with_node();
    let scene = parse_nif(&data).unwrap();

    // Block 0 is NiNode, not NiTriShape
    let result = scene.get_as::<blocks::tri_shape::NiTriShape>(0);
    assert!(result.is_none());
}

/// Build a minimal Oblivion (v20.0.0.5) NIF with `num_unknown` blocks of
/// a registered unknown type, each `payload_size` bytes of garbage.
/// v20.0.0.5 has no `block_sizes` table and no string table, which is
/// exactly the configuration that exercises the `oblivion_skip_sizes`
/// recovery path in the main parse loop.
fn build_oblivion_nif_with_unknowns(
    type_name: &str,
    num_unknown: usize,
    payload_size: usize,
) -> Vec<u8> {
    let mut buf = Vec::new();

    // ASCII header line.
    buf.extend_from_slice(b"Gamebryo File Format, Version 20.0.0.5\n");

    // Binary header.
    buf.extend_from_slice(&0x14000005u32.to_le_bytes()); // version
    buf.push(1); // little_endian
    buf.extend_from_slice(&11u32.to_le_bytes()); // user_version (Oblivion)
    buf.extend_from_slice(&(num_unknown as u32).to_le_bytes()); // num_blocks

    // BSStreamHeader (triggered by user_version >= 3).
    buf.extend_from_slice(&11u32.to_le_bytes()); // user_version_2
    buf.push(0); // author short_string: length 0
    buf.push(0); // process_script (user_version_2 < 131)
    buf.push(0); // export_script

    // Block types table.
    buf.extend_from_slice(&1u16.to_le_bytes()); // num_block_types
    buf.extend_from_slice(&(type_name.len() as u32).to_le_bytes());
    buf.extend_from_slice(type_name.as_bytes());

    // Block type indices — all blocks point at type 0.
    for _ in 0..num_unknown {
        buf.extend_from_slice(&0u16.to_le_bytes());
    }

    // No block_sizes (version < 20.2.0.7).
    // No string table (version < 20.1.0.1).

    // num_groups = 0.
    buf.extend_from_slice(&0u32.to_le_bytes());

    // Block data: each block is `payload_size` bytes of 0xAB.
    for _ in 0..num_unknown {
        buf.extend(std::iter::repeat(0xABu8).take(payload_size));
    }

    buf
}

/// Regression test for issue #224: on Oblivion NIFs (no block_sizes) the
/// caller can register `oblivion_skip_sizes` hints that let the parser
/// skip past unknown block types instead of truncating the scene.
#[test]
fn oblivion_skip_sizes_hint_recovers_unknown_blocks() {
    let type_name = "BSUnknownOblivionSkipTest";
    let payload = 24;
    let data = build_oblivion_nif_with_unknowns(type_name, 3, payload);

    // Default options: no hints → parse should truncate after the first
    // failing block, keeping 0 blocks.
    let default_scene = parse_nif(&data).unwrap();
    assert!(
        default_scene.truncated,
        "unknown-type Oblivion NIF must truncate without a hint"
    );
    assert_eq!(default_scene.dropped_block_count, 3);
    assert!(default_scene.blocks.is_empty());

    // With a registered hint the parser should skip past all 3 blocks.
    let mut options = ParseOptions::default();
    options
        .oblivion_skip_sizes
        .insert(type_name.to_string(), payload as u32);
    let scene = parse_nif_with_options(&data, &options).unwrap();

    assert!(!scene.truncated, "hint must prevent truncation");
    assert_eq!(scene.dropped_block_count, 0);
    assert_eq!(scene.len(), 3);
    for i in 0..3 {
        assert_eq!(scene.get(i).unwrap().block_type_name(), "NiUnknown");
    }
}

/// A too-large hint (past EOF) must NOT crash or advance the stream —
/// the parser falls back to the truncation path gracefully.
#[test]
fn oblivion_skip_sizes_oversized_hint_falls_back_to_truncation() {
    let type_name = "BSUnknownOblivionOversize";
    let data = build_oblivion_nif_with_unknowns(type_name, 1, 16);

    let mut options = ParseOptions::default();
    // Hint is 9999 bytes but the payload is only 16 — skip would go
    // past EOF, so the parser should log a warning and truncate.
    options
        .oblivion_skip_sizes
        .insert(type_name.to_string(), 9999);
    let scene = parse_nif_with_options(&data, &options).unwrap();

    assert!(scene.truncated);
    assert_eq!(scene.dropped_block_count, 1);
    assert!(scene.blocks.is_empty());
}

/// Regression test for #324: Oblivion NIFs (no block_sizes) recover
/// from a corrupted block using the runtime size cache built from
/// earlier successful parses of the same type.
#[test]
fn oblivion_runtime_size_cache_recovers_corrupted_block() {
    // Build an Oblivion-style NIF (v20.0.0.5, no block_sizes) with
    // 3 NiNode blocks. Block 0 and 2 are valid; block 1 is truncated
    // (data too short → parse error). The runtime cache should learn
    // the NiNode size from block 0, use it to skip block 1, and
    // successfully parse block 2.
    let mut buf = Vec::new();

    // ── Header (Oblivion v20.0.0.5) ────────────────────────────
    buf.extend_from_slice(b"Gamebryo File Format, Version 20.0.0.5\n");
    buf.extend_from_slice(&0x14000005u32.to_le_bytes()); // version
    buf.push(1); // little-endian
    buf.extend_from_slice(&11u32.to_le_bytes()); // user_version
    buf.extend_from_slice(&3u32.to_le_bytes()); // num_blocks = 3
    buf.extend_from_slice(&21u32.to_le_bytes()); // user_version_2

    // Short strings
    buf.push(1);
    buf.push(0);
    buf.push(1);
    buf.push(0);
    buf.push(1);
    buf.push(0);

    // Block types: 1 type "NiNode"
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&6u32.to_le_bytes());
    buf.extend_from_slice(b"NiNode");

    // Block type indices: all 3 blocks → type 0
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());

    // NO block_sizes (v20.0.0.5 < 20.2.0.5 threshold).
    // NO string table (v20.0.0.5 < 20.1.0.1 threshold).
    // num_groups (v >= 5.0.0.6)
    buf.extend_from_slice(&0u32.to_le_bytes());

    // ── Build a valid NiNode block (v20.0.0.5 layout) ─────────
    fn build_ninode_block() -> Vec<u8> {
        let mut b = Vec::new();
        // NiObjectNET: name (u32 length-prefixed string, 0 = empty)
        b.extend_from_slice(&0u32.to_le_bytes());
        // extra_data_refs: count=0
        b.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1
        b.extend_from_slice(&(-1i32).to_le_bytes());
        // NiAVObject: flags (u16 for v20.0.0.5)
        b.extend_from_slice(&14u16.to_le_bytes());
        // translation
        b.extend_from_slice(&0.0f32.to_le_bytes());
        b.extend_from_slice(&0.0f32.to_le_bytes());
        b.extend_from_slice(&0.0f32.to_le_bytes());
        // rotation (3×3 identity)
        for &v in &[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        // scale
        b.extend_from_slice(&1.0f32.to_le_bytes());
        // properties: count=0
        b.extend_from_slice(&0u32.to_le_bytes());
        // collision_ref: -1
        b.extend_from_slice(&(-1i32).to_le_bytes());
        // NiNode: children count=0
        b.extend_from_slice(&0u32.to_le_bytes());
        // effects count=0
        b.extend_from_slice(&0u32.to_le_bytes());
        b
    }

    let good_block = build_ninode_block();
    let block_len = good_block.len();

    // Block 0: valid
    buf.extend_from_slice(&good_block);

    // Block 1: corrupted — write a huge string length (0xDEADBEEF)
    // as the first field (name), which will fail with an I/O error
    // when read_string tries to read 3.7 billion bytes. The rest is
    // valid block data padded to block_len so the cache skip lands
    // at the correct offset for block 2.
    buf.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // poison name length
    buf.extend_from_slice(&vec![0xAA; block_len - 4]);

    // Block 2: valid
    buf.extend_from_slice(&good_block);

    let scene = parse_nif(&buf).unwrap();

    // Block 0 parsed successfully, block 1 recovered via cache → NiUnknown,
    // block 2 parsed successfully. No truncation.
    assert!(
        !scene.truncated,
        "scene should NOT be truncated — cache recovery should work"
    );
    assert_eq!(scene.len(), 3, "all 3 blocks should be present");
    assert_eq!(scene.blocks[0].block_type_name(), "NiNode");
    assert_eq!(scene.blocks[1].block_type_name(), "NiUnknown");
    assert_eq!(scene.blocks[2].block_type_name(), "NiNode");
}

// Real-game NIF parse coverage lives in `tests/parse_real_nifs.rs`, which
// walks entire mesh archives and asserts a per-game success-rate threshold.
// The old /tmp-based single-file smoke tests were removed in N23.10.

// ── #395: stream-position drift detector ─────────────────────────

#[test]
fn drift_warning_silent_with_too_few_samples() {
    // Need at least two prior samples to characterise the type.
    assert!(super::drift_warning(100, &[]).is_none());
    assert!(super::drift_warning(100, &[42]).is_none());
}

#[test]
fn drift_warning_silent_when_consumed_matches_cache() {
    // Fixed-size type, new sample matches → no fire.
    assert!(super::drift_warning(48, &[48, 48, 48]).is_none());
    // Within ±2 byte tolerance — still considered a match.
    assert!(super::drift_warning(50, &[48, 48, 48]).is_none());
    assert!(super::drift_warning(46, &[48, 48, 48]).is_none());
}

#[test]
fn drift_warning_silent_for_high_variance_types() {
    // NiTriShapeData / NiSkinData / NiNode-with-children all have
    // wildly varying consumed sizes legitimately. The detector
    // recognises this from the cache spread (> 2 bytes) and stays
    // silent regardless of the new sample.
    let prior = [40, 200, 1024];
    assert!(super::drift_warning(48, &prior).is_none());
    assert!(super::drift_warning(99999, &prior).is_none());
}

#[test]
fn drift_warning_fires_on_fixed_size_disagreement() {
    // Cache has 3 prior samples all = 48 (clearly a fixed-size
    // type). New sample 68 differs by 20 — > 2 byte tolerance,
    // unambiguous drift.
    let msg = super::drift_warning(68, &[48, 48, 48])
        .expect("drift warning should fire on +20 byte deviation from fixed-size cache");
    assert!(
        msg.contains("consumed 68 bytes"),
        "warning must report the offending consumed count, got: {msg}"
    );
    assert!(
        msg.contains("median 48"),
        "warning must report the cached median, got: {msg}"
    );
    assert!(
        msg.contains("3 prior parse(s)"),
        "warning must report sample count, got: {msg}"
    );
}

#[test]
fn drift_warning_fires_on_short_consumed_too() {
    // Drift can be backward as well as forward — a parser that
    // under-consumed leaves bytes for the next reader to overshoot.
    let msg = super::drift_warning(40, &[48, 48, 48])
        .expect("drift warning should fire on -8 byte deviation");
    assert!(msg.contains("consumed 40 bytes"));
}

#[test]
fn drift_warning_uses_min_distance_not_first_sample() {
    // Cache has slight variance ([46, 47, 48], range = 2 → still
    // considered fixed-size). New sample 50 is 2 away from 48
    // (within tolerance) → no fire. New sample 60 is 12 away from
    // the closest sample (48) → fire.
    assert!(super::drift_warning(50, &[46, 47, 48]).is_none());
    assert!(super::drift_warning(60, &[46, 47, 48]).is_some());
}

// ── #939: per-block-type drift histogram ─────────────────────────

/// A known-good NIF must produce an empty drift histogram —
/// `block_size` matches `consumed` for every block, so the
/// reconciliation branch never fires.
#[test]
fn drift_histogram_empty_on_clean_parse() {
    let data = build_test_nif_with_node();
    let scene = parse_nif(&data).expect("clean parse");
    assert!(
        scene.drift_histogram.is_empty(),
        "clean parse must produce an empty drift histogram, got: {:?}",
        scene.drift_histogram
    );
}

/// Build a Skyrim-SE-style NIF with one NiNode whose header-declared
/// `block_size` is intentionally `inflate_by` bytes larger than the
/// parser actually consumes. The parser returns `Ok`, the drift
/// reconciliation branch fires, and `scene.drift_histogram["NiNode"]`
/// ends up with one entry at `drift = +inflate_by`. Used by the
/// synthetic-drift regression test below.
fn build_drifted_nif(inflate_by: u32) -> Vec<u8> {
    let mut buf = Vec::new();

    // Header — same layout as build_test_nif_with_node.
    buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    buf.extend_from_slice(&0x14020007u32.to_le_bytes());
    buf.push(1); // little-endian
    buf.extend_from_slice(&11u32.to_le_bytes()); // user_version (FNV-style)
    buf.extend_from_slice(&1u32.to_le_bytes()); // num_blocks = 1
    buf.extend_from_slice(&34u32.to_le_bytes()); // user_version_2

    // Short strings.
    for _ in 0..3 {
        buf.push(1);
        buf.push(0);
    }

    // Block types: 1 type "NiNode".
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&6u32.to_le_bytes());
    buf.extend_from_slice(b"NiNode");

    // Block type indices: block 0 → type 0.
    buf.extend_from_slice(&0u16.to_le_bytes());

    // NiNode body (same wire layout as build_test_nif_with_node).
    let mut block = Vec::new();
    block.extend_from_slice(&0i32.to_le_bytes()); // name index
    block.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    block.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
    block.extend_from_slice(&14u32.to_le_bytes()); // flags (u32 @ v20.2.0.7)
    block.extend_from_slice(&1.0f32.to_le_bytes());
    block.extend_from_slice(&2.0f32.to_le_bytes());
    block.extend_from_slice(&3.0f32.to_le_bytes());
    for r in &[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        block.extend_from_slice(&r.to_le_bytes());
    }
    block.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    block.extend_from_slice(&0u32.to_le_bytes()); // properties count
    block.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
    block.extend_from_slice(&0u32.to_le_bytes()); // children count
    block.extend_from_slice(&0u32.to_le_bytes()); // effects count

    // Block sizes — declared = actual + inflate_by, so the drift
    // reconciliation branch fires with drift = +inflate_by.
    let declared = block.len() as u32 + inflate_by;
    buf.extend_from_slice(&declared.to_le_bytes());

    // String table: 1 string "SceneRoot".
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&9u32.to_le_bytes());
    buf.extend_from_slice(&9u32.to_le_bytes());
    buf.extend_from_slice(b"SceneRoot");

    // num_groups = 0.
    buf.extend_from_slice(&0u32.to_le_bytes());

    // Block data + tail padding so `stream.set_position(start_pos +
    // declared)` lands within the buffer.
    buf.extend_from_slice(&block);
    buf.extend(std::iter::repeat(0u8).take(inflate_by as usize));

    buf
}

#[test]
fn drift_histogram_records_synthetic_drift() {
    // 10-byte declared-vs-consumed gap on a single NiNode block.
    let data = build_drifted_nif(10);
    let scene = parse_nif(&data).expect("synthetic drift NIF parses");
    let per_type = scene
        .drift_histogram
        .get("NiNode")
        .expect("NiNode drift bucket must exist");
    assert_eq!(
        per_type.get(&10).copied(),
        Some(1),
        "synthetic NIF inflated NiNode block_size by 10 — expected one entry at drift=+10, \
         got histogram: {:?}",
        scene.drift_histogram
    );
    // No other drift entries, single drift event total.
    let total_events: u32 = scene
        .drift_histogram
        .values()
        .flat_map(|inner| inner.values())
        .sum();
    assert_eq!(total_events, 1);
}

/// Drift histogram surface is keyed on the public NifScene field —
/// pin its existence + Default-initialisation so a future refactor
/// can't silently drop it.
#[test]
fn nif_scene_default_carries_empty_drift_histogram() {
    let scene = NifScene::default();
    assert!(scene.drift_histogram.is_empty());
}
