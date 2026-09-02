//! Controller dispatch tests.
//!
//! NiFlipController, NiBSBoneLODController (Oblivion + Bethesda paths), UV
//! controller, KF-animation blocks, legacy particle-system controller.

use super::oblivion_header;
use crate::blocks::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;
use std::sync::Arc;

/// Regression for #394 — `NiFlipController` on Oblivion (>= 10.1.0.104)
/// gates off `Accum Time` and `Delta` fields, so the disk layout
/// reduces to NiTimeController base (26) + NiSingleInterpController
/// interpolator_ref (4) + texture_slot (4) + num_sources (4) +
/// sources[N] (4 each). Test with N=3 sources → 42 bytes total.
#[test]
fn ni_flip_controller_consumes_full_body_oblivion_layout() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // NiTimeController base: next(i32) + flags(u16) + freq(f32) +
    // phase(f32) + start(f32) + stop(f32) + target(i32) = 26 B
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // stop
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target
                                                     // NiSingleInterpController: interpolator_ref (4 B).
    bytes.extend_from_slice(&5i32.to_le_bytes());
    // NiFlipController: texture_slot(4) + num_sources(4) + sources.
    bytes.extend_from_slice(&4u32.to_le_bytes()); // texture_slot = GLOW_MAP
    bytes.extend_from_slice(&3u32.to_le_bytes()); // num_sources
    bytes.extend_from_slice(&11i32.to_le_bytes());
    bytes.extend_from_slice(&12i32.to_le_bytes());
    bytes.extend_from_slice(&13i32.to_le_bytes());
    assert_eq!(bytes.len(), 26 + 4 + 4 + 4 + 4 * 3);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiFlipController", &mut stream, Some(bytes.len() as u32))
        .expect("NiFlipController must parse on Oblivion");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiFlipController>()
        .unwrap();
    assert_eq!(ctrl.texture_slot, 4);
    assert_eq!(ctrl.sources.len(), 3);
    assert_eq!(ctrl.sources[0].index(), Some(11));
    assert_eq!(ctrl.sources[2].index(), Some(13));
    assert_eq!(ctrl.base.interpolator_ref.index(), Some(5));
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #394 — `NiBSBoneLODController` with one LOD (1
/// bone) + one shape group (1 skin info) + one shape_groups_2
/// entry. Creature-skeleton LOD block on every vanilla Oblivion
/// creature NIF; without this parser, every block after it was
/// truncated.
#[test]
fn ni_bs_bone_lod_controller_consumes_full_body() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // NiTimeController base (26 B).
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // LOD + counts.
    bytes.extend_from_slice(&0u32.to_le_bytes()); // lod
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_lods
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_node_groups (unused)
                                                  // Node Groups: NodeSet { num_nodes=1, nodes=[42] }.
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&42i32.to_le_bytes());
    // Shape Groups 1: SkinInfoSet { num_skin_info=1, [shape_ptr=7, skin_instance=8] }.
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_shape_groups
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_skin_info
    bytes.extend_from_slice(&7i32.to_le_bytes()); // shape_ptr
    bytes.extend_from_slice(&8i32.to_le_bytes()); // skin_instance
                                                  // Shape Groups 2: [ref 99].
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_shape_groups_2
    bytes.extend_from_slice(&99i32.to_le_bytes());
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiBSBoneLODController",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("NiBSBoneLODController must parse on Oblivion");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiBsBoneLodController>()
        .unwrap();
    assert_eq!(ctrl.lod, 0);
    assert_eq!(ctrl.node_groups.len(), 1);
    assert_eq!(ctrl.node_groups[0].nodes.len(), 1);
    assert_eq!(ctrl.node_groups[0].nodes[0].index(), Some(42));
    assert_eq!(ctrl.shape_groups_1.len(), 1);
    assert_eq!(ctrl.shape_groups_1[0].skin_infos.len(), 1);
    assert_eq!(
        ctrl.shape_groups_1[0].skin_infos[0].shape_ptr.index(),
        Some(7)
    );
    assert_eq!(ctrl.shape_groups_2.len(), 1);
    assert_eq!(ctrl.shape_groups_2[0].index(), Some(99));
    assert_eq!(stream.position() as usize, bytes.len());
}

/// `NiBSBoneLODController` on Bethesda content (bsver != 0) must
/// stop after `node_groups` and skip the `#NISTREAM#`-gated
/// shape-group tail. Pre-fix the parser ate 4+ extra bytes past
/// the block, hit `0xFFFFFFFF` reading the next block's data as
/// `Num Shape Groups`, and bailed via `allocate_vec`. Surfaced by
/// the R3 per-block histogram on FNV creature skeletons (34
/// instances all advertising as `NiUnknown`). Sized to mirror the
/// failing block 6 from `meshes/characters/_male/skeleton.nif`:
/// 26 (base) + 4 (lod) + 4 (num_lods=1) + 4 (num_node_groups) +
/// 4 (num_nodes=5) + 5×4 (ptrs) = 62 bytes total.
#[test]
fn ni_bs_bone_lod_controller_skips_shape_groups_on_bethesda() {
    // FNV header — bsver=34, the BSVER on every creature skeleton
    // that R3 surfaced.
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 11,
        user_version_2: 34,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("SkyProp")],
        max_string_length: 8,
        num_groups: 0,
    };
    let mut bytes = Vec::new();
    // NiTimeController base (26 B).
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // LOD + counts.
    bytes.extend_from_slice(&0u32.to_le_bytes()); // lod
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_lods
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_node_groups (unused)
                                                  // Node Groups: NodeSet { num_nodes=5, nodes=[10,11,12,13,14] }.
    bytes.extend_from_slice(&5u32.to_le_bytes()); // num_nodes
    for ptr in 10i32..15 {
        bytes.extend_from_slice(&ptr.to_le_bytes());
    }
    // No shape-group fields — Bethesda content stops here.
    assert_eq!(bytes.len(), 62);
    // Pre-fix tripwire: a sentinel u32 right after the body so a
    // regressed parser that keeps reading past `bytes.len()` would
    // hit `0xFFFFFFFF` and bail in `allocate_vec`. The
    // `Some(bytes.len() as u32)` block-size cap below already
    // bounds the parser; this is belt-and-braces.
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiBSBoneLODController",
        &mut stream,
        // Block-size hint covers only the real body — pre-fix
        // parser ignored block_size for end-of-block detection
        // and read past it anyway.
        Some(62),
    )
    .expect("NiBSBoneLODController must parse on Bethesda BSVER!=0");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiBsBoneLodController>()
        .unwrap();
    assert_eq!(ctrl.lod, 0);
    assert_eq!(ctrl.node_groups.len(), 1);
    assert_eq!(ctrl.node_groups[0].nodes.len(), 5);
    assert_eq!(ctrl.node_groups[0].nodes[0].index(), Some(10));
    assert_eq!(ctrl.node_groups[0].nodes[4].index(), Some(14));
    // Shape-groups are absent on Bethesda content per #NISTREAM#.
    assert!(ctrl.shape_groups_1.is_empty());
    assert!(ctrl.shape_groups_2.is_empty());
    // Stream must stop exactly at end of body — no overshoot into
    // the sentinel u32 we stamped past byte 62.
    assert_eq!(stream.position(), 62);
}

/// Regression test for issue #144: Oblivion-era KF animation roots
/// must dispatch through the right parsers.
#[test]
fn oblivion_kf_animation_blocks_route_correctly() {
    // NiKeyframeController: parses as NiPreSplitDataController (#2562) —
    // 26-byte NiTimeControllerBase + 4-byte interpolator ref (Oblivion
    // retail v20.0.0.5 is far above the 10.1.0.104 gate, so the
    // interpolator ref IS read) + no Data ref (that field is `until
    // 10.1.0.103`, also gated off at this version — NULL).
    let header = oblivion_header();
    let mut kf_bytes = Vec::new();
    // NiTimeControllerBase: next_controller, flags, frequency, phase,
    // start_time, stop_time, target_ref.
    kf_bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    kf_bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    kf_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    kf_bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    kf_bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
    kf_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // stop_time
    kf_bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref
    kf_bytes.extend_from_slice(&7i32.to_le_bytes()); // interpolator_ref
    let mut stream = NifStream::new(&kf_bytes, &header);
    let block = parse_block(
        "NiKeyframeController",
        &mut stream,
        Some(kf_bytes.len() as u32),
    )
    .expect("NiKeyframeController should dispatch through NiPreSplitDataController");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiPreSplitDataController>()
        .expect("NiKeyframeController did not downcast to NiPreSplitDataController");
    assert_eq!(ctrl.base.interpolator_ref.index(), Some(7));
    assert!(
        ctrl.data_ref.is_null(),
        "Data ref is `until 10.1.0.103` — must be NULL on Oblivion retail (v20.0.0.5)"
    );
    assert_eq!(
        block.block_type_name(),
        "NiKeyframeController",
        "#2562 — the real RTTI must survive, not erase to \"NiSingleInterpController\""
    );
    assert_eq!(stream.position(), kf_bytes.len() as u64);

    // NiSequenceStreamHelper: NiObjectNET with no extra fields.
    // name (string table index 0) + extra_data count (0) + controller ref (-1)
    let mut ssh_bytes = Vec::new();
    ssh_bytes.extend_from_slice(&0i32.to_le_bytes()); // name
    ssh_bytes.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
    ssh_bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller
    let mut stream = NifStream::new(&ssh_bytes, &header);
    let block = parse_block(
        "NiSequenceStreamHelper",
        &mut stream,
        Some(ssh_bytes.len() as u32),
    )
    .expect("NiSequenceStreamHelper should dispatch to its own parser");
    assert!(block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiSequenceStreamHelper>()
        .is_some());
}

/// Header at an exact version — for the `#2562` / `#2563` pre-split
/// (`until="10.1.0.103"`) boundary tests below, where `oblivion_header`
/// (v20.0.0.5, far above the gate) can't exercise the Data-ref read at
/// all. No supported game ships content this old (Oblivion retail is
/// the floor), so these are synthetic byte-stream fixtures at exactly
/// the version nif.xml declares — same convention as
/// `legacy_particle.rs`'s `header_at`.
fn header_at(version: NifVersion) -> NifHeader {
    NifHeader {
        version,
        little_endian: true,
        user_version: 0,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// #2562 — at `v10.1.0.103` (the last version the `Data` ref is
/// present), `NiKeyframeController` must read it instead of the
/// (not-yet-present, `since="10.1.0.104"`) interpolator ref. Pre-fix,
/// `NiKeyframeController` parsed as a bare `NiSingleInterpController`,
/// which unconditionally read the interpolator ref at every version and
/// never read `Data` at all — at this exact version that reads 4 bytes
/// belonging to the NEXT block as a bogus interpolator ref, then leaves
/// the real `Data` ref unconsumed, desyncing the stream by 4 bytes for
/// every block that follows (marker_map.nif's 8-of-13-blocks drop).
#[test]
fn ni_keyframe_controller_reads_data_ref_below_10_1_0_104() {
    let header = header_at(NifVersion::V10_1_0_103);
    let mut bytes = Vec::new();
    // `has_object_group_id()` — [10.0.0.0, 10.1.0.114) prefixes every
    // non-Havok-serializable NiObject with a 4-byte groupID (#1337).
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // NiTimeControllerBase (26 B).
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // No interpolator ref at this version (since=10.1.0.104). Data ref
    // (until=10.1.0.103) instead.
    bytes.extend_from_slice(&42i32.to_le_bytes()); // Data ref
    assert_eq!(bytes.len(), 4 + 26 + 4);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiKeyframeController",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("NiKeyframeController must parse below v10.1.0.104");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiPreSplitDataController>()
        .expect("downcast NiPreSplitDataController");
    assert!(
        ctrl.base.interpolator_ref.is_null(),
        "interpolator_ref is since=10.1.0.104 — must not be read below it"
    );
    assert_eq!(
        ctrl.data_ref.index(),
        Some(42),
        "Data ref (until=10.1.0.103) must be read at this exact version"
    );
    assert_eq!(stream.position() as usize, bytes.len());
}

/// #2562 — `NiTransformController` is nif.xml's bare rename of
/// `NiKeyframeController` (zero fields of its own), so it must parse
/// identically, including the `Data` ref, and report its own real RTTI.
#[test]
fn ni_transform_controller_is_the_keyframe_controller_alias() {
    let header = header_at(NifVersion::V10_1_0_103);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u32.to_le_bytes()); // groupID
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&7i32.to_le_bytes()); // Data ref
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiTransformController",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("NiTransformController must parse below v10.1.0.104");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiPreSplitDataController>()
        .expect("downcast NiPreSplitDataController");
    assert_eq!(ctrl.data_ref.index(), Some(7));
    assert_eq!(block.block_type_name(), "NiTransformController");
    assert_eq!(stream.position() as usize, bytes.len());
}

/// #2562 — `NiVisController` / `NiAlphaController` siblings, same shape.
#[test]
fn ni_vis_and_alpha_controllers_read_data_ref_below_10_1_0_104() {
    for type_name in ["NiVisController", "NiAlphaController"] {
        let header = header_at(NifVersion::V10_1_0_103);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0u32.to_le_bytes()); // groupID
        bytes.extend_from_slice(&(-1i32).to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&(-1i32).to_le_bytes());
        bytes.extend_from_slice(&99i32.to_le_bytes()); // Data ref
        let mut stream = NifStream::new(&bytes, &header);
        let block = parse_block(type_name, &mut stream, Some(bytes.len() as u32))
            .unwrap_or_else(|e| panic!("{type_name} must parse below v10.1.0.104: {e}"));
        let ctrl = block
            .as_any()
            .downcast_ref::<crate::blocks::controller::NiPreSplitDataController>()
            .unwrap_or_else(|| panic!("{type_name} did not downcast to NiPreSplitDataController"));
        assert_eq!(ctrl.data_ref.index(), Some(99), "{type_name}");
        assert_eq!(block.block_type_name(), type_name);
        assert_eq!(stream.position() as usize, bytes.len(), "{type_name}");
    }
}

/// #2563 — `NiFlipController`'s `Accum Time` / `Delta` floats,
/// representative sibling of the same `until="10.1.0.103"` shape as the
/// `Data`-ref siblings above (two plain floats here instead of a Ref).
/// The pre-fix code comment asserted "nothing to read here" for every
/// supported Bethesda NIF — true for retail content, but wrong for this
/// exact band, which the comment then failed to gate at all.
#[test]
fn ni_flip_controller_reads_accum_time_and_delta_below_10_1_0_104() {
    let header = header_at(NifVersion::V10_1_0_103);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u32.to_le_bytes()); // groupID
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // No interpolator ref at this version. Accum Time + Delta instead.
    bytes.extend_from_slice(&2.5f32.to_le_bytes()); // Accum Time
    bytes.extend_from_slice(&0.125f32.to_le_bytes()); // Delta
    bytes.extend_from_slice(&4u32.to_le_bytes()); // texture_slot
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_sources
    bytes.extend_from_slice(&11i32.to_le_bytes());
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiFlipController", &mut stream, Some(bytes.len() as u32))
        .expect("NiFlipController must parse below v10.1.0.104");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiFlipController>()
        .expect("downcast NiFlipController");
    assert!(ctrl.base.interpolator_ref.is_null());
    assert!((ctrl.accum_time - 2.5).abs() < 1e-6);
    assert!((ctrl.delta - 0.125).abs() < 1e-6);
    assert_eq!(ctrl.texture_slot, 4);
    assert_eq!(ctrl.sources.len(), 1);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// #2563 — `NiTextureTransformController`'s `Data` ref, the shader.rs
/// sibling family (field order differs from the mod.rs siblings above:
/// `Data` trails `shader_map`/`texture_slot`/`operation` instead of
/// immediately following the interpolator-ref prologue).
#[test]
fn ni_texture_transform_controller_reads_data_ref_below_10_1_0_104() {
    let header = header_at(NifVersion::V10_1_0_103);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u32.to_le_bytes()); // groupID
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // No interpolator ref at this version.
    bytes.push(1); // shader_map = true
    bytes.extend_from_slice(&0u32.to_le_bytes()); // texture_slot
    bytes.extend_from_slice(&2u32.to_le_bytes()); // operation
    bytes.extend_from_slice(&55i32.to_le_bytes()); // Data ref
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiTextureTransformController",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("NiTextureTransformController must parse below v10.1.0.104");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiTextureTransformController>()
        .expect("downcast NiTextureTransformController");
    assert!(ctrl.interpolator_ref.is_null());
    assert!(ctrl.shader_map);
    assert_eq!(ctrl.operation, 2);
    assert_eq!(
        ctrl.data_ref.index(),
        Some(55),
        "Data ref (until=10.1.0.103) must be read at this exact version"
    );
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression test for issue #154: NiUVController + NiUVData.
#[test]
fn oblivion_uv_controller_and_data_roundtrip() {
    use crate::blocks::controller::NiUVController;
    use crate::blocks::interpolator::NiUVData;

    let header = oblivion_header();

    // NiUVController: NiTimeControllerBase (26 bytes) + u16 target + i32 data ref.
    let mut uvc = Vec::new();
    uvc.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    uvc.extend_from_slice(&0u16.to_le_bytes()); // flags
    uvc.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    uvc.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    uvc.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
    uvc.extend_from_slice(&2.5f32.to_le_bytes()); // stop_time
    uvc.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref
    uvc.extend_from_slice(&0u16.to_le_bytes()); // target_attribute
    uvc.extend_from_slice(&42i32.to_le_bytes()); // data ref
    let mut stream = NifStream::new(&uvc, &header);
    let block = parse_block("NiUVController", &mut stream, Some(uvc.len() as u32))
        .expect("NiUVController dispatch");
    let c = block.as_any().downcast_ref::<NiUVController>().unwrap();
    assert_eq!(c.target_attribute, 0);
    assert_eq!(c.data_ref.index(), Some(42));
    assert!((c.base.stop_time - 2.5).abs() < 1e-6);
    assert_eq!(stream.position(), uvc.len() as u64);

    // NiUVData: four KeyGroup<FloatKey>. First group has 2 linear
    // keys scrolling U from 0→1; the rest are empty.
    let mut uvd = Vec::new();
    // Group 0: num_keys=2, key_type=Linear(1), key (time, value)×2
    uvd.extend_from_slice(&2u32.to_le_bytes());
    uvd.extend_from_slice(&1u32.to_le_bytes()); // KeyType::Linear
    uvd.extend_from_slice(&0.0f32.to_le_bytes()); // t=0
    uvd.extend_from_slice(&0.0f32.to_le_bytes()); // v=0
    uvd.extend_from_slice(&1.0f32.to_le_bytes()); // t=1
    uvd.extend_from_slice(&1.0f32.to_le_bytes()); // v=1
                                                  // Groups 1-3: num_keys=0 (no key_type field when empty).
    for _ in 0..3 {
        uvd.extend_from_slice(&0u32.to_le_bytes());
    }
    let mut stream = NifStream::new(&uvd, &header);
    let block =
        parse_block("NiUVData", &mut stream, Some(uvd.len() as u32)).expect("NiUVData dispatch");
    let d = block.as_any().downcast_ref::<NiUVData>().unwrap();
    assert_eq!(d.groups[0].keys.len(), 2);
    assert_eq!(d.groups[0].keys[1].value, 1.0);
    assert!(d.groups[1].keys.is_empty());
    assert!(d.groups[2].keys.is_empty());
    assert!(d.groups[3].keys.is_empty());
    assert_eq!(stream.position(), uvd.len() as u64);
}

/// Regression test for issue #143: NiParticleSystemController with
/// zero particles. Verifies the huge scalar field chain consumes
/// the expected byte count.
#[test]
fn oblivion_legacy_particle_system_controller_roundtrip() {
    use crate::blocks::legacy_particle::NiParticleSystemController;

    let header = oblivion_header();

    // NiTimeControllerBase: 26 bytes.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
    bytes.extend_from_slice(&3.0f32.to_le_bytes()); // stop_time
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref

    // Controller body scalar soup — mostly zeros, non-zero marker
    // values to verify specific field offsets.
    for v in [
        50.0f32,               // speed
        5.0,                   // speed_variation
        0.0,                   // declination
        0.5,                   // declination_variation
        0.0,                   // planar_angle
        std::f32::consts::TAU, // planar_angle_variation
    ] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    // initial_normal (vec3)
    for v in [0.0f32, 0.0, 1.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    // initial_color (RGBA)
    for v in [1.0f32, 0.5, 0.25, 1.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&1.5f32.to_le_bytes()); // initial_size
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // emit_start_time
    bytes.extend_from_slice(&10.0f32.to_le_bytes()); // emit_stop_time
    bytes.push(0u8); // reset_particle_system
    bytes.extend_from_slice(&25.0f32.to_le_bytes()); // birth_rate
    bytes.extend_from_slice(&2.0f32.to_le_bytes()); // lifetime
    bytes.extend_from_slice(&0.5f32.to_le_bytes()); // lifetime_variation
    bytes.push(1u8); // use_birth_rate
    bytes.push(0u8); // spawn_on_death
    for v in [0.0f32; 3] {
        bytes.extend_from_slice(&v.to_le_bytes());
    } // emitter_dimensions
    bytes.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // emitter ptr hash
    bytes.extend_from_slice(&1u16.to_le_bytes()); // num_spawn_generations
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // percentage_spawned
    bytes.extend_from_slice(&1u16.to_le_bytes()); // spawn_multiplier
    bytes.extend_from_slice(&0.1f32.to_le_bytes()); // spawn_speed_chaos
    bytes.extend_from_slice(&0.1f32.to_le_bytes()); // spawn_dir_chaos

    bytes.extend_from_slice(&0u16.to_le_bytes()); // num_particles
    bytes.extend_from_slice(&0u16.to_le_bytes()); // num_valid
                                                  // No particle records.
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // unknown_ref
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num_emitter_points
    bytes.extend_from_slice(&0u32.to_le_bytes()); // trailer_emitter_type
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // unknown_trailer_float
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // trailer_emitter_modifier

    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block(
        "NiParticleSystemController",
        &mut s,
        Some(bytes.len() as u32),
    )
    .expect("NiParticleSystemController dispatch");
    let c = b
        .as_any()
        .downcast_ref::<NiParticleSystemController>()
        .unwrap();
    assert!((c.speed - 50.0).abs() < 1e-6);
    assert!((c.birth_rate - 25.0).abs() < 1e-6);
    assert!((c.lifetime - 2.0).abs() < 1e-6);
    assert_eq!(c.emitter, 0xDEADBEEF);
    assert_eq!(c.num_particles, 0);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiBSPArrayController aliases to the same parser with the
    // identical payload — verify it dispatches.
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiBSPArrayController", &mut s, Some(bytes.len() as u32))
        .expect("NiBSPArrayController dispatch");
    assert!(b
        .as_any()
        .downcast_ref::<NiParticleSystemController>()
        .is_some());
}

/// Regression for #2000 / NIF-D1-03 — `NiGeomMorpherController`'s
/// `Morpher Flags` (`since=10.0.1.2`) and `Num Interpolators`/
/// `Interpolators` (`since=10.1.0.106`) were read unconditionally. On
/// old Gamebryo content below both gates (v10.0.1.0), the disk layout
/// has neither field: base(26) + data_ref(4) + always_update(1) = 31 B.
/// Pre-fix the parser walked 6 phantom bytes into the next block.
#[test]
fn ni_geom_morpher_controller_v10_0_1_0_has_no_flags_or_interpolators() {
    let header = NifHeader {
        version: NifVersion::V10_0_1_0,
        little_endian: true,
        user_version: 0,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    };
    let mut bytes = Vec::new();
    // v10.0.1.0 NiObject groupID (4 B) — present on every non-Havok
    // NiObject in [10.0.0.0, 10.1.0.114); NiGeomMorpherController isn't
    // a bhk* serializable so it carries this like the rest.
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // NiTimeController base (26 B) — no Manager Controlled bool this old.
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // stop
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target
                                                     // data_ref(4) + always_update(1) — no Morpher Flags, no Num Interpolators.
    bytes.extend_from_slice(&9i32.to_le_bytes()); // data_ref
    bytes.push(1); // always_update
    assert_eq!(bytes.len(), 4 + 31);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiGeomMorpherController",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("NiGeomMorpherController must parse at v10.0.1.0");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiGeomMorpherController>()
        .expect("downcast NiGeomMorpherController");
    assert_eq!(ctrl.morpher_flags, 0);
    assert_eq!(ctrl.data_ref.index(), Some(9));
    assert_eq!(ctrl.always_update, 1);
    assert!(ctrl.interpolator_weights.is_empty());
    assert_eq!(stream.position() as usize, bytes.len());
}

// ── #3174 — NiPSys*Ctlr family until=10.1.0.103 / since=10.1.0.104 split ──
//
// #2562/#2563 fixed this same `Data` ref / interpolator-ref split on nine
// `NiSingleInterpController` subclasses but never reached the
// `NiPSysModifierCtlr` chain (`parse_modifier_ctlr` / `parse_emitter_ctlr` /
// `parse_multi_target_emitter_ctlr` in `blocks/particle.rs`), which
// open-code the same base instead of delegating to
// `NiSingleInterpController::parse`. Below v10.1.0.104 those three read a
// nonexistent interpolator ref unconditionally and never read the
// complementary legacy `Data` ref, desyncing every block that follows.

/// `NiPSysModifierActiveCtlr` / `NiPSysModifierFloatCtlr` family (dispatched
/// through `parse_modifier_ctlr`) at `v10.1.0.103`: no interpolator ref
/// (since=10.1.0.104), `modifier_name` immediately after the base, then the
/// legacy `Data` ref (until=10.1.0.103).
#[test]
fn ni_psys_modifier_ctlr_family_reads_data_ref_below_10_1_0_104() {
    for type_name in ["NiPSysGravityStrengthCtlr", "NiPSysRotDampeningCtlr"] {
        let header = header_at(NifVersion::V10_1_0_103);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0u32.to_le_bytes()); // groupID
        bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
        bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
        bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start
        bytes.extend_from_slice(&1.0f32.to_le_bytes()); // stop
        bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target
                                                         // No interpolator ref at this version (since=10.1.0.104).
        bytes.extend_from_slice(&0u32.to_le_bytes()); // modifier_name (empty)
        bytes.extend_from_slice(&77i32.to_le_bytes()); // Data ref (until=10.1.0.103)
        let mut stream = NifStream::new(&bytes, &header);
        parse_block(type_name, &mut stream, Some(bytes.len() as u32))
            .unwrap_or_else(|e| panic!("{type_name} must parse below v10.1.0.104: {e}"));
        assert_eq!(
            stream.position() as usize,
            bytes.len(),
            "{type_name}: Data ref must be consumed, not left desyncing later blocks"
        );
    }
}

/// Same family at `v10.1.0.104`: `Manager Controlled` bool + interpolator
/// ref now present, legacy `Data` ref gone.
#[test]
fn ni_psys_modifier_ctlr_family_reads_interpolator_ref_at_10_1_0_104() {
    let header = header_at(NifVersion::V10_1_0_104);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u32.to_le_bytes()); // groupID
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.push(0); // Manager Controlled (10.1.0.104–108 band, #1506)
    bytes.extend_from_slice(&5i32.to_le_bytes()); // interpolator_ref (since=10.1.0.104)
    bytes.extend_from_slice(&0u32.to_le_bytes()); // modifier_name (empty)
    let mut stream = NifStream::new(&bytes, &header);
    parse_block(
        "NiPSysGravityStrengthCtlr",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("NiPSysGravityStrengthCtlr must parse at v10.1.0.104");
    assert_eq!(stream.position() as usize, bytes.len());
}

/// `NiPSysEmitterCtlr` at `v10.1.0.103`: no base interpolator ref, and the
/// trailing ref is the legacy `Data` slot rather than the (not-yet-present)
/// Visibility Interpolator.
#[test]
fn ni_psys_emitter_ctlr_reads_data_ref_below_10_1_0_104() {
    let header = header_at(NifVersion::V10_1_0_103);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u32.to_le_bytes()); // groupID
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes()); // modifier_name (empty)
    bytes.extend_from_slice(&88i32.to_le_bytes()); // Data ref (legacy, until=10.1.0.103)
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiPSysEmitterCtlr", &mut stream, Some(bytes.len() as u32))
        .expect("NiPSysEmitterCtlr must parse below v10.1.0.104");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::particle::NiPSysEmitterCtlr>()
        .expect("downcast NiPSysEmitterCtlr");
    assert!(
        ctrl.interpolator_ref.is_null(),
        "base interpolator_ref is since=10.1.0.104 — must not be read below it"
    );
    assert_eq!(stream.position() as usize, bytes.len());
}

/// `NiPSysEmitterCtlr` at `v10.1.0.104`: base interpolator ref present,
/// trailing ref is the Visibility Interpolator (#1544).
#[test]
fn ni_psys_emitter_ctlr_reads_visibility_interpolator_at_10_1_0_104() {
    let header = header_at(NifVersion::V10_1_0_104);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u32.to_le_bytes()); // groupID
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.push(0); // Manager Controlled
    bytes.extend_from_slice(&9i32.to_le_bytes()); // base interpolator_ref
    bytes.extend_from_slice(&0u32.to_le_bytes()); // modifier_name (empty)
    bytes.extend_from_slice(&10i32.to_le_bytes()); // Visibility Interpolator
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiPSysEmitterCtlr", &mut stream, Some(bytes.len() as u32))
        .expect("NiPSysEmitterCtlr must parse at v10.1.0.104");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::particle::NiPSysEmitterCtlr>()
        .expect("downcast NiPSysEmitterCtlr");
    assert_eq!(ctrl.interpolator_ref.index(), Some(9));
    assert_eq!(stream.position() as usize, bytes.len());
}

/// `BSPSysMultiTargetEmitterCtlr` (FO3+) at `v10.1.0.103`: same
/// `NiPSysEmitterCtlr`-shaped prologue (legacy `Data` ref, not Visibility
/// Interpolator), then its own `max_emitters` + `master_ref` tail.
#[test]
fn bs_psys_multi_target_emitter_ctlr_reads_data_ref_below_10_1_0_104() {
    let header = header_at(NifVersion::V10_1_0_103);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u32.to_le_bytes()); // groupID
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes()); // modifier_name (empty)
    bytes.extend_from_slice(&88i32.to_le_bytes()); // Data ref (legacy)
    bytes.extend_from_slice(&3u16.to_le_bytes()); // max_emitters
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // master_ref
    let mut stream = NifStream::new(&bytes, &header);
    parse_block(
        "BSPSysMultiTargetEmitterCtlr",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("BSPSysMultiTargetEmitterCtlr must parse below v10.1.0.104");
    assert_eq!(stream.position() as usize, bytes.len());
}

/// #3327 — `BSMaterialEmittanceMultController` / `BSRefractionStrengthController`
/// / `BSFrustumFOVController` must dispatch through `BsNamedFloatInterpController`
/// and report their real RTTI via `block_type_name()`, not the shared
/// "NiSingleInterpController" label these three erased into pre-fix.
#[test]
fn bs_named_float_interp_controller_family_preserves_rtti() {
    for type_name in [
        "BSMaterialEmittanceMultController",
        "BSRefractionStrengthController",
        "BSFrustumFOVController",
    ] {
        // FO3/FNV representative version — >= 10.1.0.104 so the
        // interpolator ref is present and the Manager Controlled bool
        // gate (10.1.0.104-108) doesn't apply.
        let header = header_at(NifVersion::V20_2_0_7);
        let mut bytes = Vec::new();
        // NiTimeController base (26 B).
        bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
        bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
        bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start
        bytes.extend_from_slice(&1.0f32.to_le_bytes()); // stop
        bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target
                                                         // NiSingleInterpController: interpolator_ref (4 B) — the only
                                                         // field any of these three carry beyond the base.
        bytes.extend_from_slice(&42i32.to_le_bytes());
        assert_eq!(bytes.len(), 30);
        let mut stream = NifStream::new(&bytes, &header);
        let block = parse_block(type_name, &mut stream, Some(bytes.len() as u32))
            .unwrap_or_else(|e| panic!("{type_name} must parse: {e}"));
        assert_eq!(
            block.block_type_name(),
            type_name,
            "RTTI must survive dispatch, not erase to NiSingleInterpController"
        );
        let ctrl = block
            .as_any()
            .downcast_ref::<crate::blocks::controller::BsNamedFloatInterpController>()
            .unwrap_or_else(|| {
                panic!("{type_name} did not downcast to BsNamedFloatInterpController")
            });
        assert_eq!(ctrl.base.interpolator_ref.index(), Some(42), "{type_name}");
        assert_eq!(stream.position() as usize, bytes.len(), "{type_name}");
    }
}

// ── #124 / audit NIF-513 — bhkNPCollisionObject family ──────────
