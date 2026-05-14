//! NiNode subtype dispatch tests.
//!
//! Node subtypes, BSTreeNode, BSMultiBoundNode, groupID-prefix version
//! variants, BSDistantObjectInstancedNode (FO76).

use super::oblivion_header;
use crate::blocks::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;
use std::sync::Arc;

/// Oblivion-era empty NiNode body (no children, no effects, no
/// properties, identity transform). Used as the base bytes for
/// every NiNode subtype test in this module.
fn oblivion_empty_ninode_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET: name (empty inline) + empty extra data list + null controller
    d.extend_from_slice(&0u32.to_le_bytes()); // name len
    d.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                 // NiAVObject: flags (u16 for bsver<=26), identity transform (13 f32),
                                                 // empty properties list, null collision ref.
    d.extend_from_slice(&0u16.to_le_bytes()); // flags
                                              // transform: translation (3 f32)
    d.extend_from_slice(&0.0f32.to_le_bytes());
    d.extend_from_slice(&0.0f32.to_le_bytes());
    d.extend_from_slice(&0.0f32.to_le_bytes());
    // transform: rotation 3x3 identity
    for (i, row) in (0..3).zip([[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]) {
        let _ = i;
        for v in row {
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    // transform: scale
    d.extend_from_slice(&1.0f32.to_le_bytes());
    // properties list: empty
    d.extend_from_slice(&0u32.to_le_bytes());
    // collision_ref: null
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // NiNode children: empty
    d.extend_from_slice(&0u32.to_le_bytes());
    // NiNode effects: empty (Oblivion has_effects_list = true)
    d.extend_from_slice(&0u32.to_le_bytes());
    d
}

/// Build a header for an early-Gamebryo NIF (file version in the
/// `[10.0.0.0, 10.1.0.114)` window). Every block in this range is
/// prefixed with a 4-byte `NiObject.groupID` field per nifly's
/// `NiObject::Get`. Pre-#688 the byte was misread as the first u32
/// of the block payload (typically `NiObjectNET.Name`'s SizedString
/// length), causing 145 / 8032 Oblivion-era files to truncate at
/// root with "failed to fill whole buffer".
fn early_gamebryo_header(packed_version: u32) -> NifHeader {
    NifHeader {
        version: NifVersion(packed_version),
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

/// FO76 header — bsver=155, version=20.2.0.7, with named string slots
/// for the NiObjectNET / texture-array `SizedString` paths used by the
/// BSMultiBoundNode base and BSDistantObjectInstancedNode trailing
/// texture arrays. Strings inside texture arrays are inline
/// `SizedString` (length-prefixed bytes) — they don't look up against
/// the string table, so an empty `strings` field is fine for them.
fn fo76_header_with_name(name: &str) -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 155,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from(name)],
        max_string_length: name.len() as u32,
        num_groups: 0,
    }
}

/// Build the BSMultiBoundNode wire body (NiNode body + multi_bound_ref
/// + culling_mode for bsver >= 83). Returns the byte vector ready to
/// concatenate inside a BSDistantObjectInstancedNode payload.
fn build_bs_multi_bound_node_body() -> Vec<u8> {
    let mut b = Vec::new();
    // NiObjectNET: name (string-table index 0 → name string), 0 extra
    // data refs, controller_ref = -1.
    b.extend_from_slice(&0i32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&(-1i32).to_le_bytes());
    // NiAVObject (FO76/v20.2.0.7 + bsver>=83 layout): u32 flags +
    // transform (3 floats translation + 9 floats rotation + 1 scale) +
    // 0 properties + collision_ref = -1.
    b.extend_from_slice(&0u32.to_le_bytes()); // flags
    for _ in 0..3 {
        b.extend_from_slice(&0.0f32.to_le_bytes());
    }
    for r in &[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        b.extend_from_slice(&r.to_le_bytes());
    }
    b.extend_from_slice(&1.0f32.to_le_bytes()); // scale
                                                // Properties list is gated `bsver <= 34` (FO3/FNV/Oblivion); FO76
                                                // bsver=155 skips it entirely — emitting a `0u32` here would shift
                                                // every downstream field forward 4 bytes and the multi_bound_ref
                                                // (-1) would be misread as a children count of 0xFFFFFFFF.
    b.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
                                                 // NiNode: 0 children. The `effects` array is gated on bsver — FO4+
                                                 // (bsver=130) drops it. FO76 (bsver=155) drops it too.
    b.extend_from_slice(&0u32.to_le_bytes()); // children count
                                              // BSMultiBoundNode: multi_bound_ref (-1) + culling_mode (Skyrim+).
    b.extend_from_slice(&(-1i32).to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes()); // culling_mode
    b
}


/// Regression test for issue #142: NiNode subtypes with trailing fields.
#[test]
fn oblivion_node_subtypes_dispatch_with_correct_payload() {
    use crate::blocks::node::{
        BsRangeNode, NiBillboardNode, NiLODNode, NiSortAdjustNode, NiSwitchNode,
    };

    let header = oblivion_header();
    let base = oblivion_empty_ninode_bytes();

    // NiBillboardNode: base + billboard_mode u16.
    let mut bb = base.clone();
    bb.extend_from_slice(&3u16.to_le_bytes()); // ALWAYS_FACE_CENTER
    let mut stream = NifStream::new(&bb, &header);
    let block = parse_block("NiBillboardNode", &mut stream, Some(bb.len() as u32))
        .expect("NiBillboardNode dispatch");
    let n = block.as_any().downcast_ref::<NiBillboardNode>().unwrap();
    assert_eq!(n.billboard_mode, 3);
    assert_eq!(stream.position(), bb.len() as u64);

    // NiSwitchNode: base + switch_flags u16 + index u32.
    let mut sw = base.clone();
    sw.extend_from_slice(&0x0003u16.to_le_bytes()); // UpdateOnlyActiveChild | UpdateControllers
    sw.extend_from_slice(&7u32.to_le_bytes());
    let mut stream = NifStream::new(&sw, &header);
    let block = parse_block("NiSwitchNode", &mut stream, Some(sw.len() as u32))
        .expect("NiSwitchNode dispatch");
    let n = block.as_any().downcast_ref::<NiSwitchNode>().unwrap();
    assert_eq!(n.switch_flags, 0x0003);
    assert_eq!(n.index, 7);
    assert_eq!(stream.position(), sw.len() as u64);

    // NiLODNode: NiSwitchNode body + lod_level_data ref i32.
    let mut lod = base.clone();
    lod.extend_from_slice(&0u16.to_le_bytes()); // switch_flags
    lod.extend_from_slice(&0u32.to_le_bytes()); // index
    lod.extend_from_slice(&42i32.to_le_bytes()); // lod_level_data
    let mut stream = NifStream::new(&lod, &header);
    let block =
        parse_block("NiLODNode", &mut stream, Some(lod.len() as u32)).expect("NiLODNode dispatch");
    let n = block.as_any().downcast_ref::<NiLODNode>().unwrap();
    assert_eq!(n.lod_level_data.index(), Some(42));
    assert_eq!(stream.position(), lod.len() as u64);

    // NiSortAdjustNode: base + sorting_mode u32 (v20.0.0.5 > 20.0.0.3 → no
    // trailing accumulator ref).
    let mut sa = base.clone();
    sa.extend_from_slice(&1u32.to_le_bytes()); // SORTING_OFF
    let mut stream = NifStream::new(&sa, &header);
    let block = parse_block("NiSortAdjustNode", &mut stream, Some(sa.len() as u32))
        .expect("NiSortAdjustNode dispatch");
    let n = block.as_any().downcast_ref::<NiSortAdjustNode>().unwrap();
    assert_eq!(n.sorting_mode, 1);
    assert_eq!(stream.position(), sa.len() as u64);

    // BSRangeNode (and its subclasses) — base + 3 bytes.
    for type_name in [
        "BSRangeNode",
        "BSBlastNode",
        "BSDamageStage",
        "BSDebrisNode",
    ] {
        let mut r = base.clone();
        r.push(5); // min
        r.push(10); // max
        r.push(7); // current
        let mut stream = NifStream::new(&r, &header);
        let block = parse_block(type_name, &mut stream, Some(r.len() as u32))
            .unwrap_or_else(|e| panic!("{type_name} dispatch: {e}"));
        let n = block.as_any().downcast_ref::<BsRangeNode>().unwrap();
        assert_eq!(n.min, 5);
        assert_eq!(n.max, 10);
        assert_eq!(n.current, 7);
        assert_eq!(stream.position(), r.len() as u64);
    }

    // Pure-alias variants — parse as plain NiNode with no trailing bytes.
    // BSFaceGenNiNode (Starfield, #727) is an unconfirmed-layout stub: the
    // FaceGen-coefficient trailing fields are unknown, so the alias just
    // catches the dispatch and lets `block_size` recovery skip whatever
    // trailing bytes the real wire layout carries. Test asserts the
    // dispatch lands on `NiNode` so the FaceMeshes.ba2 corpus stops
    // demoting all 1,282 face NIFs to NiUnknown.
    for type_name in [
        "AvoidNode",
        "NiBSAnimationNode",
        "NiBSParticleNode",
        "BSFaceGenNiNode",
    ] {
        let mut stream = NifStream::new(&base, &header);
        let block = parse_block(type_name, &mut stream, Some(base.len() as u32))
            .unwrap_or_else(|e| panic!("{type_name} dispatch: {e}"));
        assert!(block
            .as_any()
            .downcast_ref::<crate::blocks::NiNode>()
            .is_some());
        assert_eq!(stream.position(), base.len() as u64);
    }
}

/// Regression test for issue #160: `NiAVObject::parse` and
/// `NiNode::parse` must use the raw `bsver()` for binary-layout
/// decisions so that non-Bethesda Gamebryo files classified as
/// `NifVariant::Unknown` still read the correct fields. Previously
/// the variant-based `has_properties_list` / `has_effects_list`
/// helpers returned `false` for `Unknown`, so an Unknown variant
/// with `bsver <= 34` (pre-Skyrim) would skip the properties list
/// and mis-align the stream on every NiAVObject.
#[test]
fn ni_node_parses_unknown_variant_with_low_bsver() {
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::{NifVariant, NifVersion};
    use std::sync::Arc;

    // Craft a header that detects as `Unknown`: the only path into
    // that variant on `detect()` is `uv >= 11` without matching
    // any specific (uv, uv2) arm. uv=13, uv2=0 lands there and
    // also gives us `bsver() == 0` so the pre-Skyrim binary layout
    // applies.
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 13,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("Root")],
        max_string_length: 4,
        num_groups: 0,
    };
    // Sanity: this combo really does classify as Unknown.
    assert_eq!(
        NifVariant::detect(header.version, header.user_version, header.user_version_2),
        NifVariant::Unknown
    );

    // Build a minimal NiNode body matching the pre-Skyrim layout
    // (has properties list + has effects list). Identity transform,
    // empty children / properties / effects lists, null collision
    // ref with the distinctive sentinel value 0xFFFFFFFF so we can
    // detect a stream misalignment at the `collision_ref` field.
    let mut data = Vec::new();
    // NiObjectNET: name index 0, extra_data count 0, controller -1
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // NiAVObject: flags u16 (bsver <= 26), transform, properties list,
    // collision ref. Note flags is u16 here because bsver=0 < 26.
    data.extend_from_slice(&0u16.to_le_bytes()); // flags
    for _ in 0..3 {
        data.extend_from_slice(&0.0f32.to_le_bytes()); // translation
    }
    for row in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        for v in row {
            data.extend_from_slice(&v.to_le_bytes());
        }
    }
    data.extend_from_slice(&1.0f32.to_le_bytes()); // scale
                                                   // Properties list — this is the field `has_properties_list`
                                                   // gates. Old buggy path would skip it and misread the next
                                                   // 4 bytes as `collision_ref`.
    data.extend_from_slice(&0u32.to_le_bytes()); // properties count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref (null)
                                                    // NiNode children + effects
    data.extend_from_slice(&0u32.to_le_bytes()); // children count
    data.extend_from_slice(&0u32.to_le_bytes()); // effects count

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block("NiNode", &mut stream, Some(data.len() as u32))
        .expect("NiNode must parse under Unknown variant + bsver 0");
    let node = block
        .as_any()
        .downcast_ref::<crate::blocks::NiNode>()
        .expect("downcast to NiNode");
    assert!(
        node.av.collision_ref.is_null(),
        "Unknown variant with bsver=0 must still read properties list \
             so collision_ref lands on the right 4 bytes"
    );
    assert!(node.children.is_empty());
    assert!(node.effects.is_empty());
    assert_eq!(stream.position() as usize, data.len());
}

/// Regression: #159 — BSTreeNode (Skyrim SpeedTree) must dispatch
/// to its own parser and consume the two trailing NiNode ref lists
/// (`Bones 1` + `Bones 2`). Previously aliased to plain NiNode so
/// the two ref lists were silently dropped.
#[test]
fn bs_tree_node_dispatches_with_both_bone_lists() {
    use crate::blocks::node::BsTreeNode;

    let header = oblivion_header();
    let mut bytes = oblivion_empty_ninode_bytes();
    // bones_1: 3 refs (7, 8, 9)
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&7i32.to_le_bytes());
    bytes.extend_from_slice(&8i32.to_le_bytes());
    bytes.extend_from_slice(&9i32.to_le_bytes());
    // bones_2: 2 refs (10, 11)
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&10i32.to_le_bytes());
    bytes.extend_from_slice(&11i32.to_le_bytes());

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSTreeNode", &mut stream, Some(bytes.len() as u32))
        .expect("BSTreeNode should dispatch through BsTreeNode::parse");
    let tree = block
        .as_any()
        .downcast_ref::<BsTreeNode>()
        .expect("BSTreeNode did not downcast to BsTreeNode");
    assert_eq!(tree.bones_1.len(), 3);
    assert_eq!(tree.bones_1[0].index(), Some(7));
    assert_eq!(tree.bones_1[1].index(), Some(8));
    assert_eq!(tree.bones_1[2].index(), Some(9));
    assert_eq!(tree.bones_2.len(), 2);
    assert_eq!(tree.bones_2[0].index(), Some(10));
    assert_eq!(tree.bones_2[1].index(), Some(11));
    assert_eq!(stream.position(), bytes.len() as u64);
}

/// Regression: #148 — BSMultiBoundNode must dispatch to its own
/// parser and read the trailing `multi_bound_ref` (BlockRef, always)
/// + `culling_mode` (u32, Skyrim+ only). Previously aliased to plain
/// NiNode so the multi-bound linkage was silently dropped.
#[test]
fn bs_multi_bound_node_dispatches_with_multi_bound_ref() {
    use crate::blocks::node::BsMultiBoundNode;

    let header = oblivion_header(); // bsver 0 — no culling_mode field
    let mut bytes = oblivion_empty_ninode_bytes();
    // multi_bound_ref = 42
    bytes.extend_from_slice(&42i32.to_le_bytes());

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSMultiBoundNode", &mut stream, Some(bytes.len() as u32))
        .expect("BSMultiBoundNode should dispatch through BsMultiBoundNode::parse");
    let node = block
        .as_any()
        .downcast_ref::<BsMultiBoundNode>()
        .expect("BSMultiBoundNode did not downcast to BsMultiBoundNode");
    assert_eq!(node.multi_bound_ref.index(), Some(42));
    assert_eq!(node.culling_mode, 0); // default when bsver < 83
    assert_eq!(stream.position(), bytes.len() as u64);
}

/// O5-3 / #688: a v10.0.1.0 NiNode with the leading `groupID` u32
/// must parse all of NiObjectNET (name + extra_data + controller)
/// + NiAVObject (flags + transform + properties + collision) +
/// NiNode (children + effects). Pre-fix the parser swallowed the
/// 4-byte groupID as the start of `Name.length`, then drifted by
/// 4 bytes through every downstream field — eventually failing
/// the buffer-fill check far past the actual layout.
#[test]
fn ni_node_v10_0_1_0_consumes_groupid_prefix_and_full_payload() {
    let header = early_gamebryo_header(0x0A000100);
    let mut bytes = Vec::new();
    // NiObject.groupID — vanilla Bethesda content always ships zero.
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // NiObjectNET.Name (SizedString = u32 length + bytes).
    bytes.extend_from_slice(&6u32.to_le_bytes());
    bytes.extend_from_slice(b"HornsA");
    // NiObjectNET.NumExtraDataList + Extra Data List.
    bytes.extend_from_slice(&1u32.to_le_bytes()); // count
    bytes.extend_from_slice(&1i32.to_le_bytes()); // ref[0]
                                                  // NiObjectNET.Controller — NULL.
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // NiAVObject.Flags (u16, BSVER == 0 ≤ 26).
    bytes.extend_from_slice(&0x0010u16.to_le_bytes());
    // NiAVObject.Transform: translation (3×f32) + rotation (9×f32) + scale.
    for _ in 0..3 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // scale
                                                    // NiAVObject.NumProperties + Properties (count=0 → empty).
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // NiAVObject.CollisionObject (since 10.0.1.0).
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // NiNode.NumChildren + Children + NumEffects + Effects (all 0).
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());

    let consumed_expected = bytes.len();
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiNode", &mut stream, None)
        .expect("v10.0.1.0 NiNode with groupID prefix must parse");
    let node = block
        .as_any()
        .downcast_ref::<crate::blocks::node::NiNode>()
        .expect("downcast to NiNode");
    assert_eq!(node.av.net.name.as_deref(), Some("HornsA"));
    assert_eq!(node.av.flags, 0x0010);
    assert_eq!(node.av.net.extra_data_refs.len(), 1);
    assert_eq!(stream.position() as usize, consumed_expected);
}

/// O5-3 / #688: same payload at v10.1.0.106 — the upper edge of the
/// reported failing bucket (77 of 145 files). The fix uses an
/// inclusive-low / exclusive-high gate, so 10.1.0.106 (= 0x0A01006A,
/// just below the 10.1.0.114 = 0x0A010072 cap) must still consume
/// the prefix.
#[test]
fn ni_node_v10_1_0_106_consumes_groupid_prefix() {
    let header = early_gamebryo_header(0x0A01006A);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u32.to_le_bytes()); // groupID
    bytes.extend_from_slice(&0u32.to_le_bytes()); // name length 0
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num extra data
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    for _ in 0..13 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // transform
    }
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num properties
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // collision
    bytes.extend_from_slice(&0u32.to_le_bytes()); // children
    bytes.extend_from_slice(&0u32.to_le_bytes()); // effects

    let consumed_expected = bytes.len();
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiNode", &mut stream, None)
        .expect("v10.1.0.106 NiNode with groupID prefix must parse");
    assert!(block.as_any().is::<crate::blocks::node::NiNode>());
    assert_eq!(stream.position() as usize, consumed_expected);
}

/// O5-3 / #688: above the 10.1.0.114 cap (e.g. v20.0.0.5 / Oblivion
/// mainstream) the groupID prefix is gone — the parser must NOT
/// consume an extra 4 bytes. This pins the upper bound; without the
/// gate every Oblivion / FO3 / FNV / Skyrim block would lose 4 bytes
/// at the head.
#[test]
fn ni_node_v20_0_0_5_does_not_consume_groupid_prefix() {
    // Use the existing oblivion_header() (V20_0_0_5 / user_version=11).
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // No groupID — name index goes first.
    bytes.extend_from_slice(&0i32.to_le_bytes()); // name index = 0
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num extra data
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller
    bytes.extend_from_slice(&0u32.to_le_bytes()); // flags (u32 since BSVER=0… wait, header sets user_version=11, bsver=0)
    for _ in 0..13 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num properties
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // collision
    bytes.extend_from_slice(&0u32.to_le_bytes()); // children
    bytes.extend_from_slice(&0u32.to_le_bytes()); // effects
                                                  // The oblivion_header has bsver=0, so flags is u16 not u32 — fix:
                                                  // re-build with u16 flags.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0i32.to_le_bytes()); // name index = 0
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num extra data
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags (u16, bsver=0 ≤ 26)
    for _ in 0..13 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num properties
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // collision
    bytes.extend_from_slice(&0u32.to_le_bytes()); // children
    bytes.extend_from_slice(&0u32.to_le_bytes()); // effects

    let consumed_expected = bytes.len();
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiNode", &mut stream, None)
        .expect("v20.0.0.5 NiNode without groupID prefix must parse");
    assert!(block.as_any().is::<crate::blocks::node::NiNode>());
    assert_eq!(
        stream.position() as usize,
        consumed_expected,
        "v20.0.0.5 must NOT consume a phantom groupID — pre-#688 it \
         stopped at the right offset because the byte was \
         (mis-)read into NiObjectNET.Name.length"
    );
}

#[test]
fn fo76_bs_distant_object_instanced_node_round_trips_two_instances() {
    let header = fo76_header_with_name("DistantRoot");

    let mut data = Vec::new();
    data.extend_from_slice(&build_bs_multi_bound_node_body());

    // num_instances = 2.
    data.extend_from_slice(&2u32.to_le_bytes());

    // Instance 0: resource_id (file_hash=0xCAFEBABE, ext="nif\0",
    // dir_hash=0xDEADBEEF), 1 unknown_data entry, 2 transforms.
    data.extend_from_slice(&0xCAFEBABEu32.to_le_bytes()); // file_hash
    data.extend_from_slice(b"nif\0"); // extension
    data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // dir_hash
    data.extend_from_slice(&1u32.to_le_bytes()); // num_unknown_data
    data.extend_from_slice(&0x0102030405060708u64.to_le_bytes()); // unknown 1
    data.extend_from_slice(&0x11223344u32.to_le_bytes()); // unknown 2
    data.extend_from_slice(&2u32.to_le_bytes()); // num_transforms
                                                 // Two diagnostic matrices (16 f32 each) — first element differs so
                                                 // round-trip checks can distinguish them.
    for tag in [10.0f32, 20.0] {
        for j in 0..16 {
            data.extend_from_slice(&(tag + j as f32).to_le_bytes());
        }
    }

    // Instance 1: empty unknown_data, single transform.
    data.extend_from_slice(&0x00000001u32.to_le_bytes()); // file_hash
    data.extend_from_slice(b"bgs\0"); // extension
    data.extend_from_slice(&0x00000002u32.to_le_bytes()); // dir_hash
    data.extend_from_slice(&0u32.to_le_bytes()); // num_unknown_data
    data.extend_from_slice(&1u32.to_le_bytes()); // num_transforms
    for j in 0..16 {
        data.extend_from_slice(&(100.0f32 + j as f32).to_le_bytes());
    }

    // 3 BSShaderTextureArray slots — each is unknown_byte + count.
    // Slot 0: 1 BSTextureArray with width=2 ("foo", "bar").
    data.push(1u8); // unknown_byte
    data.extend_from_slice(&1u32.to_le_bytes()); // num_texture_arrays
    data.extend_from_slice(&2u32.to_le_bytes()); // width
    data.extend_from_slice(&3u32.to_le_bytes()); // SizedString length
    data.extend_from_slice(b"foo");
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(b"bar");
    // Slots 1 + 2: empty (count=0).
    for _ in 0..2 {
        data.push(1u8);
        data.extend_from_slice(&0u32.to_le_bytes());
    }

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSDistantObjectInstancedNode",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("BSDistantObjectInstancedNode must dispatch");
    assert_eq!(block.block_type_name(), "BSDistantObjectInstancedNode");
    let inst = block
        .as_any()
        .downcast_ref::<node::BsDistantObjectInstancedNode>()
        .expect("dispatch must produce BsDistantObjectInstancedNode");

    // Multi-bound base intact.
    assert_eq!(inst.base.culling_mode, 0);
    assert!(inst.base.multi_bound_ref.is_null());
    assert_eq!(inst.base.base.av.net.name.as_deref(), Some("DistantRoot"));

    // Per-instance payload intact.
    assert_eq!(inst.instances.len(), 2);
    let a = &inst.instances[0];
    assert_eq!(a.resource_file_hash, 0xCAFEBABE);
    assert_eq!(a.resource_extension, *b"nif\0");
    assert_eq!(a.resource_dir_hash, 0xDEADBEEF);
    assert_eq!(a.unknown_data, vec![(0x0102030405060708, 0x11223344)]);
    assert_eq!(a.transforms.len(), 2);
    assert_eq!(a.transforms[0][0], 10.0);
    assert_eq!(a.transforms[1][0], 20.0);

    let b = &inst.instances[1];
    assert_eq!(b.resource_file_hash, 1);
    assert_eq!(b.unknown_data.len(), 0);
    assert_eq!(b.transforms.len(), 1);
    assert_eq!(b.transforms[0][0], 100.0);

    // Whole payload consumed — texture arrays are parsed-and-consumed,
    // so the drift detector stays silent on this fixture.
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "BSDistantObjectInstancedNode must consume the entire payload"
    );
}

#[test]
fn fo76_bs_distant_object_instanced_node_root_recognised_by_is_ni_node_subclass() {
    // SK-D5-02 / #611 — the root-selection helper must include the new
    // subclass so a NIF rooted at BSDistantObjectInstancedNode picks
    // block 0 instead of skipping past it to the first plain NiNode
    // child.
    assert!(crate::is_ni_node_subclass("BSDistantObjectInstancedNode"));
}

// ── #936 / NIF-D5-NEW-01 — NiBSplineComp{Float,Point3}Interpolator ──

