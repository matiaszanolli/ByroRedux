//! ExtraData dispatch tests.
//!
//! Strings / integers / bone-LOD / connect-points / FO4 cloth / BSPosition /
//! BSEyeCenter / distant-object-large-ref / collision-query proxy / NiExtraData base.

use super::{fnv_header_bspline, fo4_header, oblivion_header};
use crate::blocks::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;
use std::sync::Arc;

/// Helper: encode a pre-20.1 inline length-prefixed string (u32 len + bytes).
fn inline_string(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
    out
}

/// SSE header with the `BSExtraData.name` slot populated for the
/// `read_extra_data_name` lookup. SSE bsver=100, version=20.2.0.7.
fn sse_header_with_name(name: &str) -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 100,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from(name)],
        max_string_length: name.len() as u32,
        num_groups: 0,
    }
}


/// Regression test for issue #164: array-form extra data.
#[test]
fn oblivion_strings_and_integers_extra_data_roundtrip() {
    use crate::blocks::extra_data::NiExtraData;

    let header = oblivion_header();

    // NiStringsExtraData: name(empty) + count(3) + 3 inline strings.
    let mut strings_bytes = Vec::new();
    strings_bytes.extend_from_slice(&0u32.to_le_bytes()); // name (empty inline str)
    strings_bytes.extend_from_slice(&3u32.to_le_bytes()); // count
    strings_bytes.extend_from_slice(&inline_string("alpha"));
    strings_bytes.extend_from_slice(&inline_string("beta"));
    strings_bytes.extend_from_slice(&inline_string("gamma"));
    let mut stream = NifStream::new(&strings_bytes, &header);
    let block = parse_block(
        "NiStringsExtraData",
        &mut stream,
        Some(strings_bytes.len() as u32),
    )
    .expect("NiStringsExtraData should dispatch");
    let ed = block
        .as_any()
        .downcast_ref::<NiExtraData>()
        .expect("downcast to NiExtraData");
    let arr = ed.strings_array.as_ref().expect("strings_array populated");
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_deref(), Some("alpha"));
    assert_eq!(arr[1].as_deref(), Some("beta"));
    assert_eq!(arr[2].as_deref(), Some("gamma"));

    // NiIntegersExtraData: name(empty) + count(2) + two u32s.
    let mut ints_bytes = Vec::new();
    ints_bytes.extend_from_slice(&0u32.to_le_bytes()); // name
    ints_bytes.extend_from_slice(&2u32.to_le_bytes()); // count
    ints_bytes.extend_from_slice(&42u32.to_le_bytes());
    ints_bytes.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
    let mut stream = NifStream::new(&ints_bytes, &header);
    let block = parse_block(
        "NiIntegersExtraData",
        &mut stream,
        Some(ints_bytes.len() as u32),
    )
    .expect("NiIntegersExtraData should dispatch");
    let ed = block
        .as_any()
        .downcast_ref::<NiExtraData>()
        .expect("downcast to NiExtraData");
    let arr = ed
        .integers_array
        .as_ref()
        .expect("integers_array populated");
    assert_eq!(arr, &vec![42u32, 0xDEADBEEF]);
}

/// Regression test for #615 / SK-D5-04 — `NiStringsExtraData`
/// strings are `SizedString` (always u32-length-prefixed inline)
/// per nif.xml, not the version-aware `string` type. Pre-fix the
/// parser called `read_string`, which on Skyrim+ (v >= 20.1.0.1)
/// reads a 4-byte string-table index. Result: every Skyrim
/// NiStringsExtraData with non-empty contents under-consumed its
/// payload, dropping the entire strings array body.
///
/// Construct a Skyrim-shaped block: name as string-table index
/// (4 bytes) + count + N × SizedString. Pre-fix the parser would
/// read the first 4 bytes of the first SizedString as a string-
/// table index, mis-resolve it, and stop the loop with garbage.
/// Post-fix it must round-trip the strings cleanly.
#[test]
fn skyrim_strings_extra_data_uses_sized_string_not_string_table_index() {
    use crate::blocks::extra_data::NiExtraData;

    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 83, // Skyrim LE
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        // Empty string table — proves the strings array does NOT
        // resolve through it. If the parser still used `read_string`
        // here, the first 4 bytes of "alpha" would be misread as
        // an out-of-bounds string-table index and yield None.
        strings: vec![],
        max_string_length: 0,
        num_groups: 0,
    };

    let mut bytes = Vec::new();
    // Name: string-table index = -1 (None) — exercises the modern
    // header path. 4 bytes.
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // Count: 3.
    bytes.extend_from_slice(&3u32.to_le_bytes());
    // Three SizedStrings.
    bytes.extend_from_slice(&inline_string("alpha"));
    bytes.extend_from_slice(&inline_string("beta"));
    bytes.extend_from_slice(&inline_string("gamma"));

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiStringsExtraData", &mut stream, Some(bytes.len() as u32))
        .expect("NiStringsExtraData should dispatch on Skyrim");
    let ed = block
        .as_any()
        .downcast_ref::<NiExtraData>()
        .expect("downcast to NiExtraData");
    let arr = ed
        .strings_array
        .as_ref()
        .expect("strings_array populated on Skyrim path");
    assert_eq!(arr.len(), 3, "all 3 SizedStrings must round-trip");
    assert_eq!(arr[0].as_deref(), Some("alpha"));
    assert_eq!(arr[1].as_deref(), Some("beta"));
    assert_eq!(arr[2].as_deref(), Some("gamma"));
}

/// Regression test for #614 / SK-D5-03 — `BSBoneLODExtraData`
/// must dispatch through `NiExtraData::parse` and populate the
/// `bone_lods` field with the array of `(distance, bone_name)`
/// pairs. Pre-fix the type name had no dispatch arm so every
/// Skyrim SE skeleton.nif (52 files in vanilla Meshes0.bsa) fell
/// into `NiUnknown` and dropped the parse rate from 100% to
/// ~99.7%.
///
/// The block carries the inherited `Name` field (string-table
/// index = -1 for `None`), then `BoneLOD Count: u32`, then N ×
/// `BoneLOD { Distance: u32, Bone Name: NiFixedString }`. The
/// string table here resolves indices 0/1/2 to `bone_a`, `bone_b`,
/// `bone_c` so the parsed names round-trip.
#[test]
fn skyrim_bs_bone_lod_extra_data_dispatches_and_parses() {
    use crate::blocks::extra_data::NiExtraData;

    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 83, // Skyrim LE — SKY_AND_LATER gate
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![
            Arc::from("bone_a"),
            Arc::from("bone_b"),
            Arc::from("bone_c"),
        ],
        max_string_length: 6,
        num_groups: 0,
    };

    let mut bytes = Vec::new();
    // Inherited Name: -1 (None) — 4 bytes.
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // BoneLOD Count: 3.
    bytes.extend_from_slice(&3u32.to_le_bytes());
    // 3 × (u32 distance + i32 string_table_index).
    bytes.extend_from_slice(&100u32.to_le_bytes());
    bytes.extend_from_slice(&0i32.to_le_bytes()); // bone_a
    bytes.extend_from_slice(&500u32.to_le_bytes());
    bytes.extend_from_slice(&1i32.to_le_bytes()); // bone_b
    bytes.extend_from_slice(&2000u32.to_le_bytes());
    bytes.extend_from_slice(&2i32.to_le_bytes()); // bone_c

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSBoneLODExtraData", &mut stream, Some(bytes.len() as u32))
        .expect("BSBoneLODExtraData should dispatch (#614)");
    let ed = block
        .as_any()
        .downcast_ref::<NiExtraData>()
        .expect("downcast to NiExtraData");
    let arr = ed
        .bone_lods
        .as_ref()
        .expect("bone_lods populated for BSBoneLODExtraData");
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].0, 100);
    assert_eq!(arr[0].1.as_deref(), Some("bone_a"));
    assert_eq!(arr[1].0, 500);
    assert_eq!(arr[1].1.as_deref(), Some("bone_b"));
    assert_eq!(arr[2].0, 2000);
    assert_eq!(arr[2].1.as_deref(), Some("bone_c"));
    // Stream must be fully consumed — `block_size` recovery would
    // otherwise mask any drift introduced by a future field add.
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression #158 / #365: BSPackedCombined[Shared]GeomDataExtra
/// must dispatch to its own parser and fully decode the
/// variable-size per-object tail (not just skip-via-block_size).
///
/// Constructs a valid wire payload with `num_data = 1` per
/// variant — one `BSPackedGeomData` (baked) or one
/// `BSPackedGeomObject` + one `BSPackedSharedGeomData` (shared) —
/// and checks that counts, per-instance combined records, vertex
/// bytes (for the baked variant), and triangle indices all
/// round-trip.
#[test]
fn bs_packed_combined_geom_data_extra_fully_parses_variable_tail() {
    use crate::blocks::extra_data::{BsPackedCombinedGeomDataExtra, BsPackedCombinedPayload};

    let header = oblivion_header();

    // Fixed header — identical between the two variants except for
    // what follows the top-level `num_data`.
    let mut fixed = Vec::new();
    fixed.extend_from_slice(&0u32.to_le_bytes()); // name: empty inline string
                                                  // vertex_desc: low nibble = 4 → 16-byte per-vertex stride.
    let outer_desc: u64 = 0x0000_0000_0000_0004;
    fixed.extend_from_slice(&outer_desc.to_le_bytes());
    fixed.extend_from_slice(&42u32.to_le_bytes()); // num_vertices
    fixed.extend_from_slice(&24u32.to_le_bytes()); // num_triangles
    fixed.extend_from_slice(&1u32.to_le_bytes()); // unknown_flags_1
    fixed.extend_from_slice(&2u32.to_le_bytes()); // unknown_flags_2
    fixed.extend_from_slice(&1u32.to_le_bytes()); // num_data = 1

    // One `BSPackedGeomDataCombined` — 72 bytes: f32 + NiTransform STRUCT + NiBound.
    // NiTransform STRUCT (nif.xml line 1808) ships rotation FIRST (9 f32),
    // then translation (3 f32), then scale (1 f32) — opposite to
    // NiAVObject's inline Translation→Rotation→Scale layout.
    //
    // #767 / 2026-04-30 — non-identity rotation chosen so scrambled
    // field order would assert-fail. Pre-fix the parser used
    // `read_ni_transform()` (NiAVObject order), reading the first 3
    // floats of the rotation matrix as `translation`, the next 3 as
    // rotation row 0, etc. Identity rotation (1,0,0,0,1,0,0,0,1) hides
    // the scrambling because the bytes happen to be plausible. A
    // 90°-Z rotation matrix `[[0,-1,0], [1,0,0], [0,0,1]]` plus
    // distinguishable translation makes any field misorder visible.
    let mut combined = Vec::new();
    combined.extend_from_slice(&0.5f32.to_le_bytes()); // grayscale_to_palette_scale
                                                       // rotation 90° around Z (CCW): row 0=(0,-1,0), row 1=(1,0,0), row 2=(0,0,1)
    for f in [0.0f32, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] {
        combined.extend_from_slice(&f.to_le_bytes());
    }
    for f in [10.0f32, 20.0, 30.0] {
        // translation — distinguishable from rotation values
        combined.extend_from_slice(&f.to_le_bytes());
    }
    combined.extend_from_slice(&2.5f32.to_le_bytes()); // scale (non-unity)
    for f in [5.0f32, 6.0, 7.0, 42.0] {
        // bounding sphere
        combined.extend_from_slice(&f.to_le_bytes());
    }
    assert_eq!(combined.len(), 72);

    // Baked variant tail: one BSPackedGeomData with num_verts=2,
    // one combined record, vertex_desc (stride 16), 2×16 vertex
    // bytes, and tri_count_lod0=1 triangle.
    let mut baked_tail = Vec::new();
    baked_tail.extend_from_slice(&2u32.to_le_bytes()); // num_verts
    baked_tail.extend_from_slice(&1u32.to_le_bytes()); // lod_levels
    baked_tail.extend_from_slice(&1u32.to_le_bytes()); // tri_count_lod0
    baked_tail.extend_from_slice(&0u32.to_le_bytes()); // tri_offset_lod0
    baked_tail.extend_from_slice(&0u32.to_le_bytes()); // tri_count_lod1
    baked_tail.extend_from_slice(&0u32.to_le_bytes()); // tri_offset_lod1
    baked_tail.extend_from_slice(&0u32.to_le_bytes()); // tri_count_lod2
    baked_tail.extend_from_slice(&0u32.to_le_bytes()); // tri_offset_lod2
    baked_tail.extend_from_slice(&1u32.to_le_bytes()); // num_combined
    baked_tail.extend_from_slice(&combined);
    // Per-vertex stride comes from low nibble of `inner_desc` (4 quads = 16 bytes).
    let inner_desc: u64 = 0x0000_0000_0000_0004;
    baked_tail.extend_from_slice(&inner_desc.to_le_bytes());
    // 2 vertices × 16 bytes = 32 bytes of vertex data.
    baked_tail.extend_from_slice(&[0xAAu8; 32]);
    // 1 triangle: u16 indices [0, 1, 0]
    for idx in [0u16, 1, 0] {
        baked_tail.extend_from_slice(&idx.to_le_bytes());
    }

    // Shared variant tail: one BSPackedGeomObject (8 bytes) then
    // one BSPackedSharedGeomData (header-only, same shape as baked
    // but no vertex / triangle arrays).
    let mut shared_tail = Vec::new();
    shared_tail.extend_from_slice(&0xCAFEBABEu32.to_le_bytes()); // filename_hash
    shared_tail.extend_from_slice(&0x10u32.to_le_bytes()); // data_offset
    shared_tail.extend_from_slice(&2u32.to_le_bytes()); // num_verts
    shared_tail.extend_from_slice(&1u32.to_le_bytes()); // lod_levels
    shared_tail.extend_from_slice(&1u32.to_le_bytes());
    shared_tail.extend_from_slice(&0u32.to_le_bytes());
    shared_tail.extend_from_slice(&0u32.to_le_bytes());
    shared_tail.extend_from_slice(&0u32.to_le_bytes());
    shared_tail.extend_from_slice(&0u32.to_le_bytes());
    shared_tail.extend_from_slice(&0u32.to_le_bytes());
    shared_tail.extend_from_slice(&1u32.to_le_bytes()); // num_combined
    shared_tail.extend_from_slice(&combined);
    shared_tail.extend_from_slice(&inner_desc.to_le_bytes());

    // ---- Baked ----
    let mut baked_bytes = fixed.clone();
    baked_bytes.extend_from_slice(&baked_tail);
    {
        let mut stream = NifStream::new(&baked_bytes, &header);
        let block = parse_block(
            "BSPackedCombinedGeomDataExtra",
            &mut stream,
            Some(baked_bytes.len() as u32),
        )
        .expect("baked parse");
        let extra = block
            .as_any()
            .downcast_ref::<BsPackedCombinedGeomDataExtra>()
            .expect("baked downcast");
        assert_eq!(extra.num_data, 1);
        let baked = match &extra.payload {
            BsPackedCombinedPayload::Baked(v) => v,
            _ => panic!("baked variant should produce Baked payload"),
        };
        assert_eq!(baked.len(), 1);
        assert_eq!(baked[0].num_verts, 2);
        assert_eq!(baked[0].tri_count_lod0, 1);
        assert_eq!(baked[0].combined.len(), 1);
        let c = &baked[0].combined[0];
        assert!((c.grayscale_to_palette_scale - 0.5).abs() < 1e-6);
        // #767 regression: NiTransform STRUCT field order (Rotation →
        // Translation → Scale). With the pre-fix `read_ni_transform()`
        // (NiAVObject order), translation would read the first 3
        // rotation floats (0, -1, 0) and rotation row 0 would shift
        // into rotation row 1's slot — these assertions fail in that
        // case.
        assert!((c.transform.translation.x - 10.0).abs() < 1e-6);
        assert!((c.transform.translation.y - 20.0).abs() < 1e-6);
        assert!((c.transform.translation.z - 30.0).abs() < 1e-6);
        assert!((c.transform.scale - 2.5).abs() < 1e-6);
        // 90°-Z rotation: rows[0] = (0, -1, 0), rows[1] = (1, 0, 0)
        assert!((c.transform.rotation.rows[0][0] - 0.0).abs() < 1e-6);
        assert!((c.transform.rotation.rows[0][1] - (-1.0)).abs() < 1e-6);
        assert!((c.transform.rotation.rows[1][0] - 1.0).abs() < 1e-6);
        assert_eq!(baked[0].vertex_data.len(), 32);
        assert_eq!(baked[0].triangles, vec![[0, 1, 0]]);
        assert_eq!(stream.position() as usize, baked_bytes.len());
    }

    // ---- Shared ----
    let mut shared_bytes = fixed.clone();
    shared_bytes.extend_from_slice(&shared_tail);
    {
        let mut stream = NifStream::new(&shared_bytes, &header);
        let block = parse_block(
            "BSPackedCombinedSharedGeomDataExtra",
            &mut stream,
            Some(shared_bytes.len() as u32),
        )
        .expect("shared parse");
        let extra = block
            .as_any()
            .downcast_ref::<BsPackedCombinedGeomDataExtra>()
            .expect("shared downcast");
        assert_eq!(extra.num_data, 1);
        let (objects, data) = match &extra.payload {
            BsPackedCombinedPayload::Shared { objects, data } => (objects, data),
            _ => panic!("shared variant should produce Shared payload"),
        };
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].filename_hash, 0xCAFEBABE);
        assert_eq!(objects[0].data_offset, 0x10);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].num_verts, 2);
        assert_eq!(data[0].combined.len(), 1);
        assert_eq!(stream.position() as usize, shared_bytes.len());
    }
}

/// Regression test for issue #108: `BSConnectPoint::Children.Skinned`
/// is a `byte` per nif.xml, not a `uint`. The previous parser read
/// 4 bytes instead of 1, eating the first 3 bytes of the following
/// count field. Verifies the byte read preserves the subsequent
/// count and string fields exactly.
#[test]
fn bs_connect_point_children_reads_skinned_as_byte() {
    use crate::blocks::extra_data::BsConnectPointChildren;

    let header = oblivion_header(); // inline-string path (pre-20.1.0.1)
    let mut data = Vec::new();
    // NiExtraData base: empty inline name
    data.extend_from_slice(&0u32.to_le_bytes());
    // Skinned: 1 (true) — ONE byte, not four.
    data.push(1u8);
    // Num Connect Points: u32 = 2
    data.extend_from_slice(&2u32.to_le_bytes());
    // Two sized-string entries.
    let s1 = b"HEAD";
    data.extend_from_slice(&(s1.len() as u32).to_le_bytes());
    data.extend_from_slice(s1);
    let s2 = b"CAMERA";
    data.extend_from_slice(&(s2.len() as u32).to_le_bytes());
    data.extend_from_slice(s2);

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSConnectPoint::Children",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("BSConnectPoint::Children should dispatch");
    let cp = block
        .as_any()
        .downcast_ref::<BsConnectPointChildren>()
        .expect("downcast to BsConnectPointChildren");
    assert!(cp.skinned, "skinned byte should decode to true");
    assert_eq!(cp.point_names.len(), 2);
    assert_eq!(cp.point_names[0], "HEAD");
    assert_eq!(cp.point_names[1], "CAMERA");
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "BSConnectPoint::Children over-read the skinned flag"
    );
}

/// Regression for #722 (NIF-D5-07): BSClothExtraData inherits
/// BSExtraData, which nif.xml line 3222 explicitly excludes from the
/// `Name` field via `excludeT="BSExtraData"`. Pre-fix the parser
/// called `read_extra_data_name` here, consuming 4 bytes (string-
/// table index) of the cloth payload as a name reference and then
/// reading the next 4 bytes as the length. 1,523 / 1,523 cloth-
/// bearing FO4 / FO76 / SF NIFs failed through `block_size` recovery
/// — capes, flags, curtains, hair fell back to rigid geometry.
#[test]
fn fo4_bs_cloth_extra_data_omits_name_field() {
    use crate::blocks::extra_data::BsClothExtraData;

    let header = fo4_header();
    // BSExtraData omits the NiExtraData Name. Wire layout reduces to
    // `length: u32 + data: u8[length]`. Use a sentinel payload so an
    // off-by-4 (pre-fix) consumes the length bytes as part of the
    // payload and trips the consume-exact assertion below.
    let payload: &[u8] = b"CLOTHBLOB";
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);

    let expected_len = bytes.len();
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSClothExtraData", &mut stream, Some(bytes.len() as u32))
        .expect("BSClothExtraData must dispatch on FO4 without consuming a phantom Name field");
    let cloth = block.as_any().downcast_ref::<BsClothExtraData>().unwrap();
    assert!(
        cloth.name.is_none(),
        "BSExtraData explicitly excludes Name (nif.xml line 3222 `excludeT`); name must stay None"
    );
    assert_eq!(cloth.data.as_slice(), payload);
    assert_eq!(stream.position(), expected_len as u64);
}

/// Regression for #710 / NIF-D5-03. `BSPositionData` is an FO4 / FO76
/// extra-data block carrying a per-vertex blend factor array (half-
/// floats). Pre-fix it had no dispatch arm — 2,961 vanilla instances
/// (372 in `Fallout4 - Meshes.ba2`, 2,589 in `SeventySix - Meshes.ba2`)
/// fell into NiUnknown and lost their per-vertex morph data. This
/// test builds a synthetic 4-vertex payload, dispatches via
/// `parse_block`, asserts the downcast succeeds, and pins the
/// half-float decode (0x3C00 ↔ 1.0, 0x0000 ↔ 0.0, 0xBC00 ↔ -1.0).
#[test]
fn bs_position_data_dispatches_and_decodes_half_float_array() {
    use crate::blocks::extra_data::BsPositionData;

    // FO4 header: v20.2.0.7, user_version=12, user_version_2=130 (FO4 BSVER).
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        // BSPositionData reads NiObjectNET's name via the string
        // table on v >= 20.1.0.1; supply slot 0 so the read succeeds.
        strings: vec![Arc::from("ClothBlend")],
        max_string_length: 16,
        num_groups: 0,
    };

    let mut data = Vec::new();
    // NiObjectNET name string-table index = 0 → "ClothBlend".
    data.extend_from_slice(&0i32.to_le_bytes());
    // num_vertices = 4
    data.extend_from_slice(&4u32.to_le_bytes());
    // 4 half-float blend factors: 1.0, 0.5, 0.0, -1.0
    // 1.0   = 0x3C00
    // 0.5   = 0x3800
    // 0.0   = 0x0000
    // -1.0  = 0xBC00
    for h in [0x3C00u16, 0x3800, 0x0000, 0xBC00] {
        data.extend_from_slice(&h.to_le_bytes());
    }

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block("BSPositionData", &mut stream, Some(data.len() as u32))
        .expect("BSPositionData must dispatch (#710)");
    assert_eq!(block.block_type_name(), "BSPositionData");
    let pos = block
        .as_any()
        .downcast_ref::<BsPositionData>()
        .expect("dispatch must produce BsPositionData");

    assert_eq!(pos.name.as_deref(), Some("ClothBlend"));
    assert_eq!(pos.vertex_data.len(), 4);
    assert!((pos.vertex_data[0] - 1.0).abs() < 1e-6);
    assert!((pos.vertex_data[1] - 0.5).abs() < 1e-3);
    assert!((pos.vertex_data[2] - 0.0).abs() < 1e-6);
    assert!((pos.vertex_data[3] - (-1.0)).abs() < 1e-6);

    assert_eq!(
        stream.position() as usize,
        data.len(),
        "BSPositionData must consume exactly {} bytes",
        data.len()
    );
}

/// Companion: hostile `num_vertices = 0xFFFFFFFF` must error out via
/// the `allocate_vec` budget guard, not OOM-allocate ~12 GB before
/// the inner half-float reads fail. Per the issue's ALLOCATE_VEC
/// completeness check (#764 sweep).
#[test]
fn bs_position_data_hostile_num_vertices_returns_err_not_panic() {
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("Hostile")],
        max_string_length: 8,
        num_groups: 0,
    };

    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes()); // name index
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // hostile num_vertices

    let mut stream = NifStream::new(&data, &header);
    let result = parse_block("BSPositionData", &mut stream, Some(data.len() as u32));
    assert!(
        result.is_err(),
        "hostile num_vertices must error gracefully, not panic"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("only") && msg.contains("bytes remain"),
        "expected `allocate_vec` budget rejection, got: {msg}"
    );
}

/// Regression for #720 / NIF-D5-04. `BSEyeCenterExtraData` is an FO4
/// / FO76 extra-data block carrying eye-pivot positions consumed by
/// FaceGen + dialogue-camera framing. Pre-fix it had no dispatch arm
/// — 625 vanilla instances (623 in `Fallout4 - Meshes.ba2`, 2 in
/// `SeventySix - Meshes.ba2`) fell into NiUnknown, so dialogue
/// eye-tracking pointed at the NIF origin instead of the eye centroid.
/// Synthetic FO4-shaped header + the canonical 4-float payload
/// (left+right eye XY) round-trip through dispatch + decode.
#[test]
fn bs_eye_center_extra_data_dispatches_and_decodes_4_floats() {
    use crate::blocks::extra_data::BsEyeCenterExtraData;

    // FO4 header: v20.2.0.7, user_version=12, user_version_2=130 (FO4 BSVER).
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        // BSEyeCenterExtraData reads NiObjectNET's name via the string
        // table on v >= 20.1.0.1; supply slot 0 so the read succeeds.
        strings: vec![Arc::from("EyeCenter")],
        max_string_length: 16,
        num_groups: 0,
    };

    let mut data = Vec::new();
    // NiObjectNET name string-table index = 0 → "EyeCenter".
    data.extend_from_slice(&0i32.to_le_bytes());
    // num_floats = 4 (canonical: left.x, left.y, right.x, right.y).
    data.extend_from_slice(&4u32.to_le_bytes());
    for f in [-2.5f32, 4.0, 2.5, 4.0] {
        data.extend_from_slice(&f.to_le_bytes());
    }

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block("BSEyeCenterExtraData", &mut stream, Some(data.len() as u32))
        .expect("BSEyeCenterExtraData must dispatch (#720)");
    assert_eq!(block.block_type_name(), "BSEyeCenterExtraData");
    let eye = block
        .as_any()
        .downcast_ref::<BsEyeCenterExtraData>()
        .expect("dispatch must produce BsEyeCenterExtraData");

    assert_eq!(eye.name.as_deref(), Some("EyeCenter"));
    assert_eq!(eye.floats, vec![-2.5, 4.0, 2.5, 4.0]);

    assert_eq!(
        stream.position() as usize,
        data.len(),
        "BSEyeCenterExtraData must consume exactly {} bytes",
        data.len()
    );
}

/// Companion: hostile `num_floats = 0xFFFFFFFF` must error out via
/// the `allocate_vec` budget guard, not OOM-allocate ~16 GB before
/// the inner `read_f32_le` fails. Per the issue's ALLOCATE_VEC
/// completeness check (#764 sweep).
#[test]
fn bs_eye_center_extra_data_hostile_num_floats_returns_err_not_panic() {
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("Hostile")],
        max_string_length: 8,
        num_groups: 0,
    };

    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes()); // name index
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // hostile num_floats

    let mut stream = NifStream::new(&data, &header);
    let result = parse_block("BSEyeCenterExtraData", &mut stream, Some(data.len() as u32));
    assert!(
        result.is_err(),
        "hostile num_floats must error gracefully, not panic"
    );
    let msg = result.unwrap_err().to_string();
    // Post-#981 the parser routes through `read_pod_vec` (which calls
    // `check_alloc` internally) instead of the previous `allocate_vec`
    // + per-element loop. Both paths share the same budget gate but
    // produce slightly different wording: the hard-cap branch reads
    // "exceeds hard cap" and the EOF branch reads "bytes remain[ing]".
    // The 0xFFFFFFFF × 4-byte request from this fixture trips the
    // hard-cap branch on the new path; either rejection is acceptable
    // — the contract is "clean error, not OOM panic."
    assert!(
        msg.contains("bytes remain") || msg.contains("exceeds hard cap"),
        "expected budget-rejection error, got: {msg}"
    );
}

// ── #942 / NIF-D5-NEW-03 — BSDistantObjectLargeRefExtraData (SSE) ──

#[test]
fn sse_bs_distant_object_large_ref_extra_data_round_trips_true() {
    let header = sse_header_with_name("LargeRefMarker");
    let mut data = Vec::new();
    // NiExtraData.name: string-table index 0 → "LargeRefMarker".
    data.extend_from_slice(&0i32.to_le_bytes());
    // Large Ref bool — single byte at v20.2.0.7.
    data.push(1);

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSDistantObjectLargeRefExtraData",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("BSDistantObjectLargeRefExtraData must dispatch");
    assert_eq!(block.block_type_name(), "BSDistantObjectLargeRefExtraData");
    let large = block
        .as_any()
        .downcast_ref::<extra_data::BsDistantObjectLargeRefExtraData>()
        .expect("dispatch must produce BsDistantObjectLargeRefExtraData");
    assert!(large.large_ref);
    assert_eq!(large.name.as_deref(), Some("LargeRefMarker"));
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "must consume the 5-byte body exactly"
    );
}

#[test]
fn sse_bs_distant_object_large_ref_extra_data_round_trips_false() {
    let header = sse_header_with_name("");
    let mut data = Vec::new();
    data.extend_from_slice(&(-1i32).to_le_bytes()); // name index = -1 (None)
    data.push(0); // large_ref = false

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSDistantObjectLargeRefExtraData",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("BSDistantObjectLargeRefExtraData with false flag must dispatch");
    let large = block
        .as_any()
        .downcast_ref::<extra_data::BsDistantObjectLargeRefExtraData>()
        .unwrap();
    assert!(!large.large_ref);
    assert!(large.name.is_none());
}

// ── #942 / NIF-D5-NEW-03 — BSDistantObjectInstancedNode (FO76) ──────

// ── #728 / NIF-D5-10 — BSCollisionQueryProxyExtraData (FO76) ─────

#[test]
fn fo76_bs_collision_query_proxy_extra_data_round_trips_byte_array() {
    let header = fnv_header_bspline(); // wire layout doesn't depend on bsver — ByteArray only
    let payload: &[u8] = b"\xDE\xAD\xBE\xEF\xCA\xFE";
    let mut data = Vec::new();
    data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    data.extend_from_slice(payload);

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSCollisionQueryProxyExtraData",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("BSCollisionQueryProxyExtraData must dispatch");
    assert_eq!(block.block_type_name(), "BSCollisionQueryProxyExtraData");
    let proxy = block
        .as_any()
        .downcast_ref::<extra_data::BsCollisionQueryProxyExtraData>()
        .expect("dispatch must produce BsCollisionQueryProxyExtraData");
    assert_eq!(proxy.data.as_slice(), payload);
    assert_eq!(stream.position() as usize, data.len());
}

/// Regression test for #1073 / FO4-D5-002 — bare `"NiExtraData"` RTTI name
/// must dispatch through `NiExtraData::parse` and produce a name-only struct.
///
/// 100 FO4 facial morph NIFs (`meshes\actors\character\characterassets\morphs\*.nif`)
/// each carry one block with RTTI string `"NiExtraData"` (the abstract base class
/// used literally). Pre-fix these fell to NiUnknown, classifying every affected
/// file as Truncated and pulling the FO4 clean-parse rate from 100% to 99.71%.
///
/// The block wire layout at FO4 (v20.2.0.7, string-table):
///   Name: i32 string-table index (-1 = None)
///
/// nif.xml defines `NiExtraData` as a concrete block with only the inherited
/// `Name` field; no additional data follows. The parser correctly reads the
/// Name and returns a name-only struct via the default `_ =>` arm.
#[test]
fn fo4_ni_extra_data_bare_base_dispatches_and_parses() {
    use crate::blocks::extra_data::NiExtraData;

    // FO4 header (bsver=130, version=20.2.0.7) with string-table format.
    // Name field = -1 (i32) → None. No additional data follows.
    let header = fo4_header();
    let mut data = Vec::new();
    data.extend_from_slice(&(-1i32).to_le_bytes()); // Name: string-table index -1 = None

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block("NiExtraData", &mut stream, Some(data.len() as u32))
        .expect("bare 'NiExtraData' RTTI name must dispatch — pre-#1073 this fell to NiUnknown");

    assert_eq!(
        block.block_type_name(),
        "NiExtraData",
        "block_type_name must reflect the dispatched type name"
    );
    let extra = block
        .as_any()
        .downcast_ref::<NiExtraData>()
        .expect("dispatch must produce NiExtraData");
    assert!(
        extra.name.is_none(),
        "Name at string-table index -1 must resolve to None"
    );
    // Verify no phantom bytes were consumed beyond the Name field.
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "parser must consume exactly the Name field (4 bytes) with no trailing reads"
    );
    // All payload fields must be None (no additional data for the base class).
    assert!(extra.string_value.is_none());
    assert!(extra.integer_value.is_none());
    assert!(extra.float_value.is_none());
    assert!(extra.binary_data.is_none());
    assert!(extra.bone_lods.is_none());
}
