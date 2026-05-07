//! Tests for `skin_vertex_tests` extracted from ../tri_shape.rs (refactor stage A).
//!
//! Same qualified path preserved (`skin_vertex_tests::FOO`).

use super::*;
use crate::blocks::parse_block;
use crate::header::NifHeader;
use crate::version::NifVersion;

fn test_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 100, // Skyrim SE
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// Build a minimal valid Skyrim SE BSTriShape body with zero vertices
/// and zero triangles. Used by the BSDynamicTriShape / BSLODTriShape
/// dispatch regression tests (issue #157).
fn minimal_bs_tri_shape_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET: name=-1, extra_data count=0, controller=-1
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // NiAVObject (SSE, no properties): flags u32, transform, collision_ref
    d.extend_from_slice(&0u32.to_le_bytes()); // flags
                                              // NiTransform: translation (3 f32) + rotation (9 f32) + scale (f32)
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes());
    }
    // Identity rotation
    for row in 0..3 {
        for col in 0..3 {
            let v: f32 = if row == col { 1.0 } else { 0.0 };
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
                                                 // BSTriShape: center (3 f32) + radius + 3 refs + vertex_desc u64
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes());
    }
    d.extend_from_slice(&0.0f32.to_le_bytes()); // radius
    d.extend_from_slice(&(-1i32).to_le_bytes()); // skin_ref
    d.extend_from_slice(&(-1i32).to_le_bytes()); // shader_property_ref
    d.extend_from_slice(&(-1i32).to_le_bytes()); // alpha_property_ref
    d.extend_from_slice(&0u64.to_le_bytes()); // vertex_desc (no attrs, stride 0)
                                              // SSE (bsver<130): num_triangles as u16
    d.extend_from_slice(&0u16.to_le_bytes());
    d.extend_from_slice(&0u16.to_le_bytes()); // num_vertices
    d.extend_from_slice(&0u32.to_le_bytes()); // data_size — skip the vertex/tri loops
                                              // SSE (bsver<130): particle_data_size is unconditional (#341).
    d.extend_from_slice(&0u32.to_le_bytes());
    d
}

/// Regression: #359 — a BSTriShape whose stored `data_size`
/// disagrees with the value derived from `vertex_size_quads ·
/// num_vertices · 4 + num_triangles · 6` must still parse
/// successfully (no hard fail). The mismatch fires a `log::warn!`
/// that's visible in `nif_stats` runs and would have caught audit
/// findings S1-01 (FO76 Bound Min Max slip) and S5-01
/// (BSDynamicTriShape misalignment) before manual inspection.
/// Don't hard-fail — some shipped FO4 content has non-standard
/// padding in this field.
#[test]
fn bs_tri_shape_with_mismatched_data_size_still_parses() {
    let header = test_header();
    // Patch the minimal-helper bytes: replace data_size = 0 with
    // a deliberately wrong non-zero value. With num_vertices = 0
    // and num_triangles = 0 the derived value is 0, so any
    // nonzero stored value triggers the mismatch warning.
    // Helper layout (see minimal_bs_tri_shape_bytes): NiObjectNET(12)
    // + flags(4) + transform(52) + collision_ref(4) + center(12)
    // + radius(4) + 3 refs(12) + vertex_desc(8) + num_triangles(2)
    // + num_vertices(2) = 112 bytes before data_size.
    let mut bytes = minimal_bs_tri_shape_bytes();
    let data_size_offset = 112;
    bytes[data_size_offset..data_size_offset + 4].copy_from_slice(&999u32.to_le_bytes());
    // Length unchanged, no trailing data needed because
    // num_vertices == num_triangles == 0 → no vertex/triangle
    // arrays are read regardless of `data_size` value.

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let shape = parse_block("BSTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("data_size mismatch must NOT hard-fail the parse");
    assert!(shape.as_any().downcast_ref::<BsTriShape>().is_some());
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "trailing bytes should still be consumed cleanly even when \
             data_size disagrees with the derived value"
    );
}

/// Regression: #341 — when `data_size == 0` (the BSDynamicTriShape case
/// for facegen heads — real positions live in the trailing dynamic
/// Vector4 array), the SSE `particle_data_size` u32 must still be
/// consumed unconditionally. Previously the read was nested inside
/// `if data_size > 0`, misaligning `parse_dynamic` by 4 bytes so it
/// read `vertex_data_size`/`unknown` from the wrong offsets, dropped
/// every NPC head, and spammed 21,140 "expected N consumed 124"
/// warnings on a Skyrim - Meshes0.bsa scan.
#[test]
fn bs_dynamic_tri_shape_with_zero_data_size_imports_dynamic_vertices() {
    let header = test_header();
    let mut bytes = minimal_bs_tri_shape_bytes();
    // BSDynamicTriShape trailing for 2 dynamic vertices:
    //   dynamic_data_size = 2 * 16 = 32, then 2 × Vector4 (x, y, z, w).
    // Per nif.xml the dynamic-vertex count is `dynamic_data_size / 16`
    // — independent of the base BSTriShape `num_vertices` — so we
    // don't need to patch that field here.
    let dyn_verts: [[f32; 4]; 2] = [[1.0, 2.0, 3.0, 0.0], [4.0, 5.0, 6.0, 0.0]];
    bytes.extend_from_slice(&32u32.to_le_bytes()); // dynamic_data_size
    for v in &dyn_verts {
        for f in v {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
    }

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSDynamicTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("BSDynamicTriShape with data_size==0 should parse");
    let shape = block
        .as_any()
        .downcast_ref::<BsTriShape>()
        .expect("BSDynamicTriShape did not downcast to BsTriShape");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "BSDynamicTriShape (#341): trailing bytes not fully consumed — \
             SSE particle_data_size was probably misaligned again"
    );
    assert_eq!(
        shape.vertices.len(),
        2,
        "dynamic_vertices override should populate shape.vertices"
    );
    assert!((shape.vertices[0].x - 1.0).abs() < 1e-6);
    assert!((shape.vertices[1].x - 4.0).abs() < 1e-6);
}

/// Regression: #157 — BSDynamicTriShape must dispatch to the Dynamic
/// parser and consume its trailing `vertex_data_size` + `unknown`
/// header (even when zero-sized). Previously routed to NiUnknown,
/// making every Skyrim NPC face invisible.
#[test]
fn bs_dynamic_tri_shape_dispatches_and_consumes_trailing_bytes() {
    let header = test_header();
    let mut bytes = minimal_bs_tri_shape_bytes();
    // BSDynamicTriShape trailing: dynamic_data_size=0 (#341 — the
    // bogus `_unknown` u32 was removed; nif.xml only specifies one
    // u32 between the BSTriShape body and the Vector4 array).
    bytes.extend_from_slice(&0u32.to_le_bytes());

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSDynamicTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("BSDynamicTriShape should dispatch through BsTriShape::parse_dynamic");
    assert!(
        block.as_any().downcast_ref::<BsTriShape>().is_some(),
        "BSDynamicTriShape did not downcast to BsTriShape"
    );
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "BSDynamicTriShape trailing extras not fully consumed"
    );
}

/// FO76 header — BSVER 155. `BS_F76` condition in nif.xml gates the
/// 24-byte `Bound Min Max` AABB between the bounding sphere and the
/// skin ref on BSTriShape. See #342.
fn fo76_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 155, // Fallout 76 — BS_F76
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// Build a minimal valid FO76 BSTriShape body with a non-zero
/// `Bound Min Max` payload. Reads `num_triangles` as u32 (BSVER
/// >= 130) and omits `particle_data_size` (BS_SSE only). Used by
/// the S1-01 / #342 regression test.
fn minimal_fo76_bs_tri_shape_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET: name=-1, extra_data count=0, controller=-1
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // NiAVObject (no properties): flags u32, transform, collision_ref
    d.extend_from_slice(&0u32.to_le_bytes()); // flags
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes());
    }
    for row in 0..3 {
        for col in 0..3 {
            let v: f32 = if row == col { 1.0 } else { 0.0 };
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
                                                 // BSTriShape: center (3 f32) + radius + Bound Min Max (6 f32, F76)
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes()); // center
    }
    d.extend_from_slice(&0.0f32.to_le_bytes()); // radius
                                                // #342 — Bound Min Max payload. Non-zero so a regression that
                                                // skipped past it (or still consumed it as skin_ref) would
                                                // produce a wildly wrong BlockRef index and fail the test's
                                                // skin_ref / shader_ref / alpha_ref assertions.
    for v in [-1.0f32, -2.0, -3.0, 4.0, 5.0, 6.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    // Refs — distinct sentinel values so a byte-slip shows up
    // immediately in the assertions.
    d.extend_from_slice(&7i32.to_le_bytes()); // skin_ref
    d.extend_from_slice(&8i32.to_le_bytes()); // shader_property_ref
    d.extend_from_slice(&9i32.to_le_bytes()); // alpha_property_ref
    d.extend_from_slice(&0u64.to_le_bytes()); // vertex_desc
                                              // FO76 (BSVER >= 130): num_triangles as u32
    d.extend_from_slice(&0u32.to_le_bytes()); // num_triangles
    d.extend_from_slice(&0u16.to_le_bytes()); // num_vertices
    d.extend_from_slice(&0u32.to_le_bytes()); // data_size
                                              // BS_SSE-only particle_data_size is NOT present on FO76.
    d
}

/// Regression: #342 (S1-01) — FO76 BSTriShape must skip the 24-byte
/// `Bound Min Max` AABB between the bounding sphere and the skin
/// ref. Pre-fix every FO76 BSTriShape mis-parsed skin_ref,
/// shader_property_ref, alpha_property_ref, and vertex_desc by 24
/// bytes; per-block `block_size` realignment hid the slip from
/// parse-rate metrics but every block's *contents* were wrong.
#[test]
fn bs_tri_shape_fo76_consumes_bound_min_max() {
    let header = fo76_header();
    let bytes = minimal_fo76_bs_tri_shape_bytes();

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("BSTriShape on FO76 header should parse");
    let shape = block
        .as_any()
        .downcast_ref::<BsTriShape>()
        .expect("BSTriShape did not downcast");

    // The refs must resolve to the sentinel values we wrote into
    // the bytes. A 24-byte slip would shift skin_ref to
    // (-1.0f32 reinterpreted as u32) ≈ 0xBF800000, blowing past
    // any reasonable block index.
    assert_eq!(
        shape.skin_ref.index(),
        Some(7),
        "skin_ref misaligned — Bound Min Max was not consumed"
    );
    assert_eq!(
        shape.shader_property_ref.index(),
        Some(8),
        "shader_property_ref misaligned (#342 cascade)"
    );
    assert_eq!(
        shape.alpha_property_ref.index(),
        Some(9),
        "alpha_property_ref misaligned (#342 cascade)"
    );
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "FO76 BSTriShape must consume exactly the body (no trailing bytes)"
    );
}

/// Sibling — Skyrim SE (BSVER 100) must NOT consume the
/// Bound Min Max bytes. The condition is strict equality on 155,
/// so SkyrimSE / SkyrimLE / FO4 / Starfield stay at the pre-#342
/// layout. Regression guard against a future `>= 155` or
/// `>= 130` typo.
#[test]
fn bs_tri_shape_skyrim_sse_skips_no_bound_min_max() {
    let header = test_header(); // BSVER 100 (SSE)
    let bytes = minimal_bs_tri_shape_bytes();
    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    parse_block("BSTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("SSE BSTriShape must still parse after the FO76 gate lands");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "SSE body length unchanged — BSVER != 155 must not skip Bound Min Max"
    );
}

/// Sibling — Starfield (BSVER 172) also NOT affected. The pre-fix
/// issue description called this out explicitly; test pins the
/// boundary. Reuses `minimal_fo76_bs_tri_shape_bytes` (same FO4+
/// layout: num_triangles u32, no particle_data_size) but patches
/// BSVER to 172 and removes the 24-byte Bound Min Max payload —
/// a strict-equality `BSVER == 155` gate must NOT fire here.
#[test]
fn bs_tri_shape_starfield_skips_no_bound_min_max() {
    let mut header = fo76_header();
    header.user_version_2 = 172;
    // Starfield body is identical to FO76 minus the Bound Min Max.
    // Build from the FO76 bytes and splice out the 24 bytes at the
    // Bound Min Max offset: NiObjectNET(12) + flags(4) + transform(52)
    // + collision_ref(4) + center(12) + radius(4) = 88 → Bound Min Max
    // occupies offsets 88..112.
    let mut sf = minimal_fo76_bs_tri_shape_bytes();
    sf.drain(88..112);
    let mut stream = crate::stream::NifStream::new(&sf, &header);
    parse_block("BSTriShape", &mut stream, Some(sf.len() as u32))
        .expect("Starfield BSTriShape must still parse after the FO76 gate lands");
    assert_eq!(
        stream.position() as usize,
        sf.len(),
        "Starfield body length unchanged — BSVER 172 != 155 must not skip Bound Min Max"
    );
}

/// FO3/FNV header — has_properties_list=true, no shader_alpha_refs.
/// Used by the BSSegmentedTriShape regression test.
fn fo3_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 11,
        user_version_2: 34, // Fallout 3 / NV
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// Build a minimal valid FO3/FNV NiTriShape body: zero materials,
/// null data refs, identity transform. Used as the base for the
/// BSSegmentedTriShape regression test.
fn minimal_fo3_ni_tri_shape_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET: name=-1, extra_data count=0, controller=-1
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // NiAVObject (FO3/FNV, bsver=34): flags u32, transform,
    // properties list (count=0, no entries), collision_ref
    d.extend_from_slice(&0u32.to_le_bytes()); // flags
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes()); // translation
    }
    for row in 0..3 {
        for col in 0..3 {
            let v: f32 = if row == col { 1.0 } else { 0.0 };
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    d.extend_from_slice(&0u32.to_le_bytes()); // properties count
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
                                                 // NiTriShape: data_ref, skin_instance_ref, num_materials,
                                                 // active_material_index, dirty_flag (v >= 20.2.0.7).
    d.extend_from_slice(&(-1i32).to_le_bytes()); // data_ref
    d.extend_from_slice(&(-1i32).to_le_bytes()); // skin_instance_ref
    d.extend_from_slice(&0u32.to_le_bytes()); // num_materials
    d.extend_from_slice(&0u32.to_le_bytes()); // active_material_index
    d.push(0u8); // dirty_flag
                 // FO3/FNV has no shader_alpha_refs branch.
    d
}

/// Regression: #146 — BSSegmentedTriShape must dispatch to the
/// segmented parser and consume its trailing `num_segments` (u32)
/// + 9-byte segment records. Previously aliased to plain NiTriShape,
/// dropping segment metadata and causing block-loop realignment
/// warnings on every FO3/FNV/SkyrimLE body-part mesh.
#[test]
fn bs_segmented_tri_shape_dispatches_and_consumes_segment_table() {
    let header = fo3_header();
    let mut bytes = minimal_fo3_ni_tri_shape_bytes();
    // num_segments = 2 + two 9-byte segment records.
    bytes.extend_from_slice(&2u32.to_le_bytes());
    // Segment 0: flags=0x1, index=0, num_tris=10
    bytes.push(0x1);
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&10u32.to_le_bytes());
    // Segment 1: flags=0x2, index=10, num_tris=5
    bytes.push(0x2);
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.extend_from_slice(&5u32.to_le_bytes());

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSSegmentedTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("BSSegmentedTriShape should dispatch through NiTriShape::parse_segmented");
    assert!(
        block.as_any().downcast_ref::<NiTriShape>().is_some(),
        "BSSegmentedTriShape did not downcast to NiTriShape"
    );
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "BSSegmentedTriShape segment table not fully consumed"
    );
}

/// Regression: #147 — BSMeshLODTriShape shares BSLODTriShape's
/// 3-u32 LOD-size trailing layout. Previously dispatched to the
/// plain BSTriShape arm, leaving 12 bytes unread and spamming the
/// block-loop realignment warning.
#[test]
fn bs_mesh_lod_tri_shape_dispatches_and_consumes_trailing_bytes() {
    let header = test_header();
    let mut bytes = minimal_bs_tri_shape_bytes();
    // BSMeshLODTriShape trailing: 3 × u32 LOD sizes.
    bytes.extend_from_slice(&20u32.to_le_bytes());
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.extend_from_slice(&2u32.to_le_bytes());

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSMeshLODTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("BSMeshLODTriShape should dispatch through BsTriShape::parse_lod");
    assert!(
        block.as_any().downcast_ref::<BsTriShape>().is_some(),
        "BSMeshLODTriShape did not downcast to BsTriShape"
    );
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "BSMeshLODTriShape trailing LOD sizes not fully consumed"
    );
}

/// Regression: #404 — BSSubIndexTriShape now decodes its segmentation
/// block into [`BsSubIndexTriShapeData`] instead of skipping past it
/// via `block_size`. The recovered segment table carries the
/// per-segment bone-slot flags (SSE) / parent-array indices + cut
/// offsets (FO4+) needed for dismemberment / locational damage.
///
/// SSE-flavoured fixture (`bsver == 100` from `test_header()`): each
/// segment is `byte flags + uint start_index + uint num_primitives`
/// (9 bytes/segment, no parent_array_index, no sub-segments).
#[test]
fn bs_sub_index_tri_shape_sse_decodes_segment_table() {
    let header = test_header();
    let mut bytes = minimal_bs_tri_shape_bytes();
    // SSE segmentation: u32 num_segments = 2, then 2 × (u8 flags + u32 start + u32 num_prims).
    bytes.extend_from_slice(&2u32.to_le_bytes()); // num_segments
    bytes.extend_from_slice(&0x42u8.to_le_bytes()); // flags
    bytes.extend_from_slice(&0u32.to_le_bytes()); // start_index
    bytes.extend_from_slice(&12u32.to_le_bytes()); // num_primitives
    bytes.extend_from_slice(&0x7Fu8.to_le_bytes()); // flags
    bytes.extend_from_slice(&36u32.to_le_bytes()); // start_index
    bytes.extend_from_slice(&8u32.to_le_bytes()); // num_primitives

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSSubIndexTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("BSSubIndexTriShape SSE path should structured-decode");
    let shape = block
        .as_any()
        .downcast_ref::<BsTriShape>()
        .expect("BSSubIndexTriShape did not downcast to BsTriShape");

    let sub = match &shape.kind {
        BsTriShapeKind::SubIndex(data) => data,
        other => panic!("expected SubIndex kind, got {:?}", other),
    };
    assert_eq!(sub.num_segments, 2);
    // SSE doesn't carry total_segments / num_primitives.
    assert_eq!(sub.total_segments, 0);
    assert_eq!(sub.num_primitives, 0);
    assert_eq!(sub.segments.len(), 2);
    assert_eq!(sub.segments[0].flags, Some(0x42));
    assert_eq!(sub.segments[0].start_index, 0);
    assert_eq!(sub.segments[0].num_primitives, 12);
    assert!(sub.segments[0].parent_array_index.is_none());
    assert!(sub.segments[0].sub_segments.is_empty());
    assert_eq!(sub.segments[1].flags, Some(0x7F));
    assert_eq!(sub.segments[1].start_index, 36);
    assert_eq!(sub.segments[1].num_primitives, 8);
    assert!(sub.shared.is_none());
    // All bytes consumed — no `block_size` realignment.
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression: #404 — BSSubIndexTriShape FO4+/FO76 path. `bsver >= 130`
/// adds `num_primitives + num_segments + total_segments` plus the
/// sub-segment list per segment, and a trailing
/// `BSGeometrySegmentSharedData` (segment_starts, per-segment shared
/// data with cut offsets, SSF filename via SizedString16) when
/// `num_segments < total_segments`.
///
/// The FO4 BSStreamHeader user_version_2 is 130 — fixture builds the
/// minimal viable BSTriShape body for that bsver and appends a single
/// segment with one sub-segment so the shared trailer is exercised.
#[test]
fn bs_sub_index_tri_shape_fo4_decodes_segments_subsegments_and_ssf() {
    // FO4 header: user_version_2 (BSVER) = 130.
    let header = NifHeader {
        user_version_2: 130,
        ..test_header()
    };
    // Build a minimal FO4 BSTriShape body. `parse()` reads
    // num_triangles as u32 on bsver>=130, num_vertices as u16,
    // data_size as u32. Set data_size=1 to flip the FO4+
    // `Data Size > 0` gate without having to ship real geometry
    // (vertex/tri loops are gated separately on data_size > 0
    // and num_vertices/num_triangles > 0; with both counts at 0
    // the loops run zero iterations regardless of data_size).
    let mut bytes = Vec::new();
    // NiObjectNET: name + extra_data count + controller
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // NiAVObject: flags u32, transform (3 + 9 + 1 floats), collision_ref
    bytes.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..3 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    for row in 0..3 {
        for col in 0..3 {
            let v: f32 = if row == col { 1.0 } else { 0.0 };
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // BSTriShape: center + radius + 3 refs + vertex_desc u64
    for _ in 0..3 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    // FO4: num_triangles u32, num_vertices u16, data_size u32 (>0 to
    // open the segmentation gate but with zero counts so vertex /
    // triangle loops are skipped).
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num_triangles
    bytes.extend_from_slice(&0u16.to_le_bytes()); // num_vertices
    bytes.extend_from_slice(&1u32.to_le_bytes()); // data_size > 0

    // FO4+ segmentation: num_primitives, num_segments, total_segments
    bytes.extend_from_slice(&20u32.to_le_bytes()); // num_primitives
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_segments
    bytes.extend_from_slice(&2u32.to_le_bytes()); // total_segments (1 seg + 1 subseg)
                                                  // Segment 0: start_index, num_prims, parent_array_index, num_sub_segments=1
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&20u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    // Sub-segment: start_index, num_prims, parent_array_index, unused
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&20u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
    // BSGeometrySegmentSharedData (num_segments < total_segments → present)
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_segments
    bytes.extend_from_slice(&2u32.to_le_bytes()); // total_segments
    bytes.extend_from_slice(&0u32.to_le_bytes()); // segment_starts[0]
                                                  // per_segment_data[0]: user_index, bone_id, num_cut_offsets=2, [f32; 2]
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&0xCAFEBABEu32.to_le_bytes());
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&0.25f32.to_le_bytes());
    bytes.extend_from_slice(&0.75f32.to_le_bytes());
    // per_segment_data[1]: user_index, bone_id, num_cut_offsets=0
    bytes.extend_from_slice(&7u32.to_le_bytes());
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    // SSF filename — SizedString16
    let ssf = b"actor.ssf";
    bytes.extend_from_slice(&(ssf.len() as u16).to_le_bytes());
    bytes.extend_from_slice(ssf);

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSSubIndexTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("BSSubIndexTriShape FO4+ path should structured-decode");
    let shape = block
        .as_any()
        .downcast_ref::<BsTriShape>()
        .expect("BSSubIndexTriShape did not downcast to BsTriShape");

    let sub = match &shape.kind {
        BsTriShapeKind::SubIndex(data) => data,
        other => panic!("expected SubIndex kind, got {:?}", other),
    };
    assert_eq!(sub.num_primitives, 20);
    assert_eq!(sub.num_segments, 1);
    assert_eq!(sub.total_segments, 2);
    assert_eq!(sub.segments.len(), 1);
    assert!(sub.segments[0].flags.is_none());
    assert_eq!(sub.segments[0].parent_array_index, Some(0));
    assert_eq!(sub.segments[0].sub_segments.len(), 1);
    assert_eq!(sub.segments[0].sub_segments[0].unused, 0xDEADBEEF);
    let shared = sub.shared.as_ref().expect("FO4+ shared trailer expected");
    assert_eq!(shared.num_segments, 1);
    assert_eq!(shared.total_segments, 2);
    assert_eq!(shared.segment_starts, vec![0]);
    assert_eq!(shared.per_segment_data.len(), 2);
    assert_eq!(shared.per_segment_data[0].user_index, 3);
    assert_eq!(shared.per_segment_data[0].bone_id, 0xCAFEBABE);
    assert_eq!(shared.per_segment_data[0].cut_offsets, vec![0.25, 0.75]);
    assert_eq!(shared.per_segment_data[1].cut_offsets, Vec::<f32>::new());
    assert_eq!(shared.ssf_filename, "actor.ssf");
    // Every byte consumed — no `block_size`-driven realignment.
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression: #404 — when the segmentation trailer parse fails
/// mid-stream, the BSTriShape body must still be preserved (the
/// renderer consumes geometry, not segmentation). Pre-fix behaviour
/// was a wholesale `block_size` skip that always succeeded; the
/// post-fix structured decode must never degrade below that level
/// of robustness.
///
/// Fixture: a FO4 BSSubIndexTriShape whose segmentation block runs
/// off the end of the supplied bytes (truncated mid-segment). The
/// parser must catch the read error, skip to `block_size`, and hand
/// back a `BsTriShape` with `SubIndex(default)` so geometry survives.
#[test]
fn bs_sub_index_tri_shape_truncated_segmentation_preserves_body() {
    let header = NifHeader {
        user_version_2: 130,
        ..test_header()
    };
    // Build the same FO4 BSTriShape body as the happy-path test...
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..3 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    for row in 0..3 {
        for col in 0..3 {
            let v: f32 = if row == col { 1.0 } else { 0.0 };
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    for _ in 0..3 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
    }
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes()); // data_size > 0
                                                  // ...then claim 1000 segments but supply only enough bytes for
                                                  // the first segment header — `allocate_vec` admits the count
                                                  // (under cap) but the stream runs dry mid-segment.
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num_primitives
    bytes.extend_from_slice(&1000u32.to_le_bytes()); // num_segments
    bytes.extend_from_slice(&1000u32.to_le_bytes()); // total_segments
                                                     // Truncate here — first segment read will fail.

    // Round up the block size to cover the body + the truncated
    // segmentation header bytes only. Pad with garbage so the skip
    // path has somewhere to land.
    let body_end = bytes.len();
    bytes.extend_from_slice(&[0xFFu8; 32]);
    let block_size = bytes.len() as u32;

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSSubIndexTriShape", &mut stream, Some(block_size))
        .expect("truncated segmentation must NOT take down the BsTriShape body");
    let shape = block
        .as_any()
        .downcast_ref::<BsTriShape>()
        .expect("BSSubIndexTriShape did not downcast to BsTriShape");
    // Geometry preserved; segmentation defaulted (signal to consumers
    // that the trailer wasn't recovered).
    match &shape.kind {
        BsTriShapeKind::SubIndex(data) => {
            assert_eq!(
                data.num_segments, 0,
                "default segmentation expected on fallback"
            );
            assert!(data.segments.is_empty());
        }
        other => panic!("expected SubIndex kind even on fallback, got {:?}", other),
    }
    // Stream advanced to the end of the block — no realignment by the
    // outer block-loop required.
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "block_size skip should land exactly at block end (body_end={body_end})"
    );
}

/// Build a minimal valid Skyrim SE NiTriShape body (NiTriBasedGeom
/// inheritance, 97 B). Used by the BSLODTriShape regression below
/// — pre-#838 BSLODTriShape was incorrectly assembled from a
/// `BSTriShape` body even though nif.xml says it inherits
/// `NiTriBasedGeom`.
fn minimal_sse_ni_tri_shape_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET: name=-1, extra_data count=0, controller=-1
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // NiAVObject (SSE bsver=100, no properties): flags u32 + transform + collision_ref
    d.extend_from_slice(&0u32.to_le_bytes()); // flags
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes()); // translation
    }
    for row in 0..3 {
        for col in 0..3 {
            let v: f32 = if row == col { 1.0 } else { 0.0 };
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    // bsver=100 > 34, so properties_list is omitted (Vec::new()).
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
    // NiTriShape body fields:
    d.extend_from_slice(&(-1i32).to_le_bytes()); // data_ref
    d.extend_from_slice(&(-1i32).to_le_bytes()); // skin_instance_ref
    d.extend_from_slice(&0u32.to_le_bytes()); // num_materials = 0
    d.extend_from_slice(&0u32.to_le_bytes()); // active_material_index
    d.push(0u8); // dirty_flag (v >= 20.2.0.7)
    // SSE has has_shader_alpha_refs = true.
    d.extend_from_slice(&(-1i32).to_le_bytes()); // shader_property_ref
    d.extend_from_slice(&(-1i32).to_le_bytes()); // alpha_property_ref
    debug_assert_eq!(d.len(), 97, "SSE NiTriShape body must be exactly 97 B");
    d
}

/// Regression: #838 (SK-D5-NEW-07) — BSLODTriShape inherits from
/// `NiTriBasedGeom` (NiTriShape body, not BSTriShape body). Pre-fix
/// the dispatcher routed it through `BsTriShape::parse_lod`, which
/// over-consumed by 23 bytes per block on real Skyrim tree LODs
/// (`expected 109 bytes, consumed 132`); `block_size` recovery
/// silently realigned the stream so the bug only surfaced as
/// per-block WARN noise. Routing through `NiLodTriShape` parses the
/// 97-byte NiTriShape body + 12-byte LOD trailer correctly.
///
/// Replaces the prior #157 regression which built BSLODTriShape from
/// a `minimal_bs_tri_shape_bytes()` body — that fixture passed only
/// because the test built the same wrong body the parser expected;
/// real Skyrim NIFs ship the NiTriShape format and drifted.
#[test]
fn bs_lod_tri_shape_skyrim_consumes_ni_tri_shape_body_plus_3u32_trailer() {
    let header = test_header();
    let mut bytes = minimal_sse_ni_tri_shape_bytes();
    // 3 × u32 LOD sizes — matches nif.xml `BSLODTriShape` definition.
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.extend_from_slice(&5u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    // 97 (NiTriShape body) + 12 (trailer) = 109 — the exact size the
    // Skyrim Meshes0 nif_stats run reports for real BSLODTriShape blocks.
    assert_eq!(bytes.len(), 109);

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSLODTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("BSLODTriShape should dispatch through NiLodTriShape::parse");
    assert_eq!(block.block_type_name(), "BSLODTriShape");
    let lod = block
        .as_any()
        .downcast_ref::<crate::blocks::tri_shape::NiLodTriShape>()
        .expect("BSLODTriShape must downcast to NiLodTriShape");
    assert_eq!(lod.lod0_size, 10);
    assert_eq!(lod.lod1_size, 5);
    assert_eq!(lod.lod2_size, 1);
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "BSLODTriShape trailing LOD sizes not fully consumed",
    );
}

/// Regression: #560 — each wire-distinct BsTriShape subtype must stamp
/// the matching `kind` discriminator and report its original type name
/// via `block_type_name()`. Pre-fix every variant returned
/// `"BSTriShape"` and downstream consumers (facegen head detection,
/// distant-LOD batch importer, dismember segmentation) could not tell
/// a head from a static prop from a segmented body from a LOD shell.
#[test]
fn bs_tri_shape_variants_stamp_their_kind() {
    let header = test_header();

    // 1. Plain BSTriShape → Plain.
    {
        let bytes = minimal_bs_tri_shape_bytes();
        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let block = parse_block("BSTriShape", &mut stream, Some(bytes.len() as u32)).unwrap();
        let shape = block.as_any().downcast_ref::<BsTriShape>().unwrap();
        assert_eq!(shape.kind, BsTriShapeKind::Plain);
        assert_eq!(block.block_type_name(), "BSTriShape");
    }

    // 2. BSLODTriShape — covered by `bs_lod_tri_shape_skyrim_consumes_ni_tri_shape_body_plus_3u32_trailer`
    //    above. Per #838 / nif.xml, BSLODTriShape inherits from
    //    NiTriBasedGeom (NiTriShape body), NOT BSTriShape — it
    //    downcasts to `NiLodTriShape`, not `BsTriShape`. BSMeshLODTriShape
    //    (FO4) IS a BSTriShape subclass and stays in this matrix below.

    // 3. BSMeshLODTriShape → MeshLOD (same wire format as LOD but
    //    different kind so importers can branch — Skyrim SE DLC
    //    doesn't consult the cutoffs).
    {
        let mut bytes = minimal_bs_tri_shape_bytes();
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(&5u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let block =
            parse_block("BSMeshLODTriShape", &mut stream, Some(bytes.len() as u32)).unwrap();
        let shape = block.as_any().downcast_ref::<BsTriShape>().unwrap();
        assert_eq!(shape.kind, BsTriShapeKind::MeshLOD);
        assert_eq!(block.block_type_name(), "BSMeshLODTriShape");
    }

    // 4. BSSubIndexTriShape → SubIndex(_) carrying a structured
    //    segmentation payload. SSE wire format (test_header bsver=100):
    //    `u32 num_segments` followed by per-segment 9-byte rows.
    //    Empty segment table is the simplest valid fixture (consumes
    //    exactly 4 bytes of trailer).
    {
        let mut bytes = minimal_bs_tri_shape_bytes();
        bytes.extend_from_slice(&0u32.to_le_bytes()); // num_segments = 0
        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let block =
            parse_block("BSSubIndexTriShape", &mut stream, Some(bytes.len() as u32)).unwrap();
        let shape = block.as_any().downcast_ref::<BsTriShape>().unwrap();
        assert!(matches!(shape.kind, BsTriShapeKind::SubIndex(_)));
        if let BsTriShapeKind::SubIndex(data) = &shape.kind {
            assert_eq!(data.num_segments, 0);
            assert!(data.segments.is_empty());
            assert!(data.shared.is_none());
        }
        assert_eq!(block.block_type_name(), "BSSubIndexTriShape");
    }

    // 5. BSDynamicTriShape → Dynamic. Append dynamic_data_size=0 so
    //    the facegen-vertex loop runs zero iterations.
    {
        let mut bytes = minimal_bs_tri_shape_bytes();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let block =
            parse_block("BSDynamicTriShape", &mut stream, Some(bytes.len() as u32)).unwrap();
        let shape = block.as_any().downcast_ref::<BsTriShape>().unwrap();
        assert_eq!(shape.kind, BsTriShapeKind::Dynamic);
        assert_eq!(block.block_type_name(), "BSDynamicTriShape");
    }
}

/// IEEE-754 half-float for 1.0 is 0x3C00; for 0.5 is 0x3800; for 0.0 is 0x0000.
/// These are the constants the read_vertex_skin_data helper will decode.
#[test]
fn read_vertex_skin_data_weights_and_indices() {
    let header = test_header();
    let mut data = Vec::new();
    // Weights: 1.0, 0.5, 0.0, 0.0 as half-floats.
    data.extend_from_slice(&0x3C00u16.to_le_bytes()); // 1.0
    data.extend_from_slice(&0x3800u16.to_le_bytes()); // 0.5
    data.extend_from_slice(&0x0000u16.to_le_bytes()); // 0.0
    data.extend_from_slice(&0x0000u16.to_le_bytes()); // 0.0
                                                      // Indices: 0, 1, 0, 0
    data.extend_from_slice(&[0u8, 1, 0, 0]);

    let mut stream = NifStream::new(&data, &header);
    let (weights, indices) = read_vertex_skin_data(&mut stream).unwrap();

    assert!((weights[0] - 1.0).abs() < 1e-4);
    assert!((weights[1] - 0.5).abs() < 1e-4);
    assert_eq!(weights[2], 0.0);
    assert_eq!(weights[3], 0.0);
    assert_eq!(indices, [0, 1, 0, 0]);
    // All 12 bytes consumed.
    assert_eq!(stream.position() as usize, data.len());
}

#[test]
fn read_vertex_skin_data_four_bones_normalized() {
    let header = test_header();
    let mut data = Vec::new();
    // Four equal weights of 0.25 as half-floats (0x3400).
    for _ in 0..4 {
        data.extend_from_slice(&0x3400u16.to_le_bytes());
    }
    // Four distinct bone indices.
    data.extend_from_slice(&[3u8, 7, 12, 42]);

    let mut stream = NifStream::new(&data, &header);
    let (weights, indices) = read_vertex_skin_data(&mut stream).unwrap();

    let sum: f32 = weights.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-3,
        "weights should sum to 1, got {}",
        sum
    );
    for w in &weights {
        assert!((w - 0.25).abs() < 1e-3);
    }
    assert_eq!(indices, [3, 7, 12, 42]);
}

/// Regression: #711 / NIF-D5-05 — FO4 precombined LOD chunks
/// (`Fallout4 - MeshesExtra.ba2`) ship `data_size = 0` together with
/// nontrivial `num_vertices` / `num_triangles` because the actual
/// vertex / index payload lives in a sidecar precombined-mesh buffer.
/// Pre-fix the parser ran `allocate_vec(num_vertices)` BEFORE the
/// `data_size > 0` gate, causing 45,521 `BSMeshLODTriShape` and
/// 18,073 `BSTriShape` blocks to error out with "claims N elements
/// but only M bytes remain" and fall to NiUnknown via err-recovery —
/// every FO4 distant-LOD piece rendered as an empty bounding box.
///
/// The fixture is the byte-exact 134-byte body of block 2 from
/// `meshes\precombined\0000e163_135b94fb_oc.nif`. It carries
/// `num_vertices = 15102`, `num_triangles = 13588`, `data_size = 0`,
/// no inline geometry — the canonical precombined-LOD pattern. The
/// 3-u32 LOD-size trailer brings the total to 146 bytes (134 BSTriShape
/// body + 12 LOD trailer). Post-fix the parser must consume the full
/// block, recognise the `data_size == 0` sentinel, leave the inline
/// vertex / index Vecs empty, and route through `parse_lod` to the
/// LOD trailer.
#[test]
fn fo4_precombined_lod_chunk_with_zero_data_size_parses_clean() {
    // FO4 header (BSVER 130).
    let header = NifHeader {
        user_version_2: 130,
        ..test_header()
    };

    // Captured byte-exact from `Fallout4 - MeshesExtra.ba2` block 2 of
    // meshes\precombined\0000e163_135b94fb_oc.nif via trace_block.rs.
    // 134 bytes total. Layout walkthrough (offsets in block):
    //   0-15:    NiObjectNET (name=1, extra_data count=1, extra[0]=3, ctrl=-1)
    //   16-19:   AV flags = 0x120e
    //   20-31:   translation (precombined cell origin — large values)
    //   32-67:   rotation 3x3 (effectively identity, sanitized downstream)
    //   68-71:   scale = 1.0
    //   72-75:   collision_ref = -1
    //   76-91:   bounding sphere (center 0,0,0 + radius ≈ 884.64)
    //   92-103:  skin_ref=-1, shader_ref=4, alpha_ref=6
    //   104-111: vertex_desc u64 = 0x0043_b000_0765_0408
    //   112-115: num_triangles = 13588 (u32 on bsver>=130)
    //   116-117: num_vertices = 15102 (u16, raw 0x3afe)
    //   118-121: data_size = 0  ← the precombined-LOD sentinel
    //   122-125: lod0 = 9668     ← BSLODTriShape 3-u32 trailer
    //   126-129: lod1 = 0
    //   130-133: lod2 = 3920
    let bytes: [u8; 134] = [
        0x01, 0x00, 0x00, 0x00, // NET name idx = 1
        0x01, 0x00, 0x00, 0x00, // NET extra_data count = 1
        0x03, 0x00, 0x00, 0x00, // NET extra_data[0] = 3
        0xff, 0xff, 0xff, 0xff, // NET controller_ref = -1
        0x0e, 0x12, 0x00, 0x00, // AV flags = 0x120e
        // Translation (3 f32) — large precombined-cell offset.
        0xf4, 0x78, 0x91, 0x47, // tx ≈ 74393.91
        0xc2, 0x6b, 0x24, 0xc7, // ty ≈ -42091.76
        0x98, 0x90, 0x30, 0xc5, // tz ≈ -2825.04
        // Rotation 3x3 (effectively identity, sanitized downstream).
        0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f,
        0x00, 0x00, 0x80, 0x3f, // scale = 1.0
        0xff, 0xff, 0xff, 0xff, // collision_ref = -1
        // Bounding sphere: center (0,0,0) + radius ≈ 884.64.
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x46, 0x29, 0x5d, 0x44,
        // Refs.
        0xff, 0xff, 0xff, 0xff, // skin_ref = -1
        0x04, 0x00, 0x00, 0x00, // shader_property_ref = 4
        0x06, 0x00, 0x00, 0x00, // alpha_property_ref = 6
        // vertex_desc u64.
        0x08, 0x04, 0x65, 0x07, 0x00, 0xb0, 0x43, 0x00,
        0x14, 0x35, 0x00, 0x00, // num_triangles = 13588
        0xfe, 0x3a, // num_vertices = 15102
        0x00, 0x00, 0x00, 0x00, // data_size = 0  ← canonical FO4 LOD sentinel
        // BSLODTriShape 3-u32 LOD-size trailer (consumed by parse_lod).
        0xc4, 0x25, 0x00, 0x00, // lod0 = 9668
        0x00, 0x00, 0x00, 0x00, // lod1 = 0
        0x50, 0x0f, 0x00, 0x00, // lod2 = 3920
    ];

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSMeshLODTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("FO4 precombined-LOD chunk must parse cleanly with data_size = 0");
    let shape = block
        .as_any()
        .downcast_ref::<BsTriShape>()
        .expect("BSMeshLODTriShape did not downcast to BsTriShape");

    // Metadata round-tripped from the wire.
    assert_eq!(shape.num_triangles, 13588);
    assert_eq!(shape.num_vertices, 15102);
    assert_eq!(shape.data_size, 0);
    // No inline geometry on this branch — the precombined-LOD pattern
    // points the renderer at a sidecar buffer.
    assert!(shape.vertices.is_empty());
    assert!(shape.uvs.is_empty());
    assert!(shape.normals.is_empty());
    assert!(shape.triangles.is_empty());
    // Bounding sphere recovered for spatial dispatch.
    assert!((shape.radius - 884.64).abs() < 0.1);
    // The 3-u32 LOD trailer is consumed by `parse_lod`. The
    // `BSMeshLODTriShape` dispatch arm calls `with_kind(MeshLOD)` to
    // overwrite the LOD-sizes-bearing `Kind::LOD` variant — Skyrim SE
    // DLC's `BSMeshLODTriShape` doesn't consult the cutoffs, so they
    // are intentionally discarded at the wire-type-discriminator level.
    // See blocks/mod.rs `BSMeshLODTriShape` arm + the `kind` doc on
    // `BsTriShape` (#157, #560). What matters here is that the trailer
    // bytes were consumed (asserted by the position check below) and
    // the kind ends up as `MeshLOD`.
    assert!(
        matches!(shape.kind, BsTriShapeKind::MeshLOD),
        "expected MeshLOD kind, got {:?}",
        shape.kind,
    );
    // Block consumed exactly — no `block_size`-driven realignment.
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "BSMeshLODTriShape with data_size=0 must consume the block exactly"
    );
}

/// Sibling check from #711: the same `data_size = 0 + non-zero
/// num_vertices` pattern occurs on plain `BSTriShape` blocks (18,073
/// hits in `MeshesExtra.ba2`). `parse_lod` calls `parse` first, so the
/// fix lands in the shared base — pin the plain dispatch arm with the
/// same precombined-LOD shape minus the 3-u32 LOD trailer.
#[test]
fn fo4_bs_tri_shape_with_zero_data_size_and_nonzero_counts_parses_clean() {
    let header = NifHeader {
        user_version_2: 130,
        ..test_header()
    };

    // BSTriShape body proper is 122 bytes (no LOD trailer on the plain
    // dispatch arm). Drop the 12 trailing bytes the LOD test included
    // — for the plain `BSTriShape` arm the `data_size = 0` sentinel
    // means the body ends exactly at the data_size u32.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[
        0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
        0xff, 0xff, 0xff, 0xff, 0x0e, 0x12, 0x00, 0x00, 0xf4, 0x78, 0x91, 0x47,
        0xc2, 0x6b, 0x24, 0xc7, 0x98, 0x90, 0x30, 0xc5, 0x00, 0x00, 0x80, 0x3f,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
        0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x46, 0x29, 0x5d, 0x44, 0xff, 0xff, 0xff, 0xff,
        0x04, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x08, 0x04, 0x65, 0x07,
        0x00, 0xb0, 0x43, 0x00, 0x14, 0x35, 0x00, 0x00, 0xfe, 0x3a, 0x00, 0x00,
        0x00, 0x00,
    ]);
    assert_eq!(bytes.len(), 122);

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("plain FO4 BSTriShape with data_size = 0 must parse cleanly");
    let shape = block
        .as_any()
        .downcast_ref::<BsTriShape>()
        .expect("BSTriShape did not downcast to BsTriShape");
    assert_eq!(shape.num_vertices, 15102);
    assert_eq!(shape.data_size, 0);
    assert!(shape.vertices.is_empty());
    assert!(matches!(shape.kind, BsTriShapeKind::Plain));
    assert_eq!(stream.position() as usize, bytes.len());
}

// ── #621 / SK-D1-LOW — BsTriShape parser hardening ──────────────────────

/// SK-D1-04 regression. `parse_dynamic` overwrites the BSTriShape
/// positions with the trailing Vector4 array — an authoritatively
/// full-precision f32 source — but pre-fix it left `vertex_desc`
/// untouched. Downstream consumers reading
/// `vertex_attrs & VF_FULL_PRECISION` thought positions were still
/// half-precision, despite the override. Latent today (no consumer
/// cross-checks); a future GPU-skinning path that re-uploads from the
/// packed buffer would read stale half-precision metadata.
///
/// Pin the post-overwrite invariant: `vertex_attrs` must include
/// `VF_FULL_PRECISION` (bit 10 of the high u16) after `parse_dynamic`
/// successfully copies a non-empty dynamic-vertex array.
#[test]
fn bs_dynamic_tri_shape_sets_full_precision_flag_after_position_overwrite() {
    let header = test_header();
    let mut bytes = minimal_bs_tri_shape_bytes();
    // dynamic_data_size = 1 vertex × 16 = 16 bytes, then one Vector4.
    bytes.extend_from_slice(&16u32.to_le_bytes());
    for f in [3.5f32, -1.25, 7.0, 0.0] {
        bytes.extend_from_slice(&f.to_le_bytes());
    }

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSDynamicTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("BSDynamicTriShape parse");
    let shape = block
        .as_any()
        .downcast_ref::<BsTriShape>()
        .expect("BSDynamicTriShape did not downcast to BsTriShape");

    // Sanity: the dynamic Vector4 actually overwrote the (empty)
    // packed positions — that's the precondition for the descriptor
    // patch to fire.
    assert_eq!(shape.vertices.len(), 1);
    assert!((shape.vertices[0].x - 3.5).abs() < 1e-6);

    // VF_FULL_PRECISION is bit 10 of the vertex_attrs field (bits
    // 44..56 of the u64 vertex_desc per nif.xml `BSVertexDesc` line
    // 2092). 0x400 << 44 = 0x0040_0000_0000_0000.
    let attrs = ((shape.vertex_desc >> 44) & 0xFFF) as u16;
    assert_eq!(
        attrs & 0x400,
        0x400,
        "VF_FULL_PRECISION must be set on vertex_desc after parse_dynamic \
         override (raw vertex_desc = 0x{:016x})",
        shape.vertex_desc,
    );
}

/// SK-D1-05 regression. When `data_size` disagrees with the
/// descriptor-derived expected value, the parser must trust
/// `data_size` (the on-disk authority) and adopt the data_size-derived
/// stride for the per-vertex loop. Pre-fix the parser logged the
/// mismatch but plowed ahead with the suspect `vertex_size_quads * 4`
/// stride, silently misaligning every vertex past the first.
///
/// Fixture: SSE BSTriShape (bsver < 130 → full-precision implicit) with
/// `VF_VERTEX` set, `vertex_size_quads = 3` (deliberately wrong — only
/// covers 12 of the 16 bytes the field actually consumes), 2 vertices,
/// 0 triangles. data_size = 32 (= 2 × 16) implies the correct stride.
/// Pre-fix the per-vertex loop would have hard-erred at the
/// `consumed > vertex_size_bytes` assertion (16 > 12). Post-fix the
/// derived stride lifts the loop to 16 bytes/vertex and the read
/// completes cleanly.
#[test]
fn bs_tri_shape_data_size_mismatch_uses_derived_stride() {
    let header = test_header();
    let mut bytes = minimal_bs_tri_shape_bytes();

    // Patch vertex_desc (offset 100, 8 bytes): set
    // vertex_size_quads = 3 in low nibble, set VF_VERTEX (bit 0 of
    // the high u16, raw value 0x001 — see tri_shape.rs:331).
    let vertex_desc: u64 = 3 | ((/* VF_VERTEX = */ 0x001u64) << 44);
    let vd_offset = 100;
    bytes[vd_offset..vd_offset + 8].copy_from_slice(&vertex_desc.to_le_bytes());

    // Patch num_vertices (offset 110, u16) = 2.
    let nv_offset = 110;
    bytes[nv_offset..nv_offset + 2].copy_from_slice(&2u16.to_le_bytes());

    // Patch data_size (offset 112, u32) = 32 (= 2 × 16, no triangles).
    // Descriptor-derived expected = vertex_size_quads * 4 * num_vertices +
    //                               num_triangles * 6
    //                             = 3 * 4 * 2 + 0 = 24
    // → mismatch fires; derived stride = (32 - 0) / 2 = 16.
    let ds_offset = 112;
    bytes[ds_offset..ds_offset + 4].copy_from_slice(&32u32.to_le_bytes());

    // Splice the 32 bytes of vertex data BEFORE the trailing 4-byte
    // particle_data_size that minimal_bs_tri_shape_bytes appends. The
    // helper's particle_data_size starts at offset 116 (= ds_offset + 4),
    // so insert at offset 116.
    let particle_size_offset = 116;
    let mut vertex_payload = Vec::with_capacity(32);
    for v in [
        [10.0f32, 20.0, 30.0, 0.0],
        [-1.5f32, -2.5, -3.5, 0.0],
    ] {
        for f in v {
            vertex_payload.extend_from_slice(&f.to_le_bytes());
        }
    }
    bytes.splice(particle_size_offset..particle_size_offset, vertex_payload);
    // Sanity: 120 (helper) + 32 (vertex payload) = 152 bytes total.
    assert_eq!(bytes.len(), 152);

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("data_size-mismatch BSTriShape must parse via the derived stride");
    let shape = block
        .as_any()
        .downcast_ref::<BsTriShape>()
        .expect("BSTriShape did not downcast");

    assert_eq!(shape.vertices.len(), 2, "both vertices must be read");
    assert!((shape.vertices[0].x - 10.0).abs() < 1e-6);
    assert!((shape.vertices[1].x + 1.5).abs() < 1e-6);
    assert!((shape.vertices[1].z + 3.5).abs() < 1e-6);
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "block must consume exactly — derived stride must align the \
         per-vertex loop with the data_size payload"
    );
}

/// Regression: #887 / SK-D1-NN-01 — when `VF_TANGENTS` is clear, the
/// 4-byte trailing slot after the position triplet is `Unused W` per
/// nif.xml `BSVertexData`, NOT `Bitangent X`. Pre-fix the parser
/// unconditionally read the slot into `bitangent_x`. Output was
/// fortunately gated on `bitangent_z` (only set under
/// `VF_TANGENTS && VF_NORMALS`), so the stray value never reached the
/// `tangents` Vec — but the dual semantic was invisible to the code,
/// and any future consumer that read `bitangent_x` outside the
/// assembly gate would silently take garbage on non-tangented meshes.
///
/// Fixture: SSE BSTriShape (bsver < 130 → full-precision implicit)
/// with `VF_VERTEX` set, `VF_TANGENTS` clear, 1 vertex. Stuff a
/// sentinel f32 (NaN-encoded `0xDEAD_BEEF`) in the trailing slot
/// where pre-fix `bitangent_x` would have absorbed it. Post-fix the
/// slot is consumed via `stream.skip(4)` and the assembler never sees
/// it. Pin: `shape.tangents` must remain empty (no spurious tangent
/// reconstruction).
#[test]
fn bs_tri_shape_unused_w_slot_does_not_pollute_tangents() {
    let header = test_header();
    let mut bytes = minimal_bs_tri_shape_bytes();

    // Patch vertex_desc (offset 100, 8 bytes): vertex_size_quads = 4
    // (16 bytes/vertex), VF_VERTEX = 0x001 (no tangents, no normals).
    let vertex_desc: u64 = 4 | (0x001u64 << 44);
    let vd_offset = 100;
    bytes[vd_offset..vd_offset + 8].copy_from_slice(&vertex_desc.to_le_bytes());
    // num_vertices = 1.
    let nv_offset = 110;
    bytes[nv_offset..nv_offset + 2].copy_from_slice(&1u16.to_le_bytes());
    // data_size = 16 (1 × 16-byte vertex, 0 triangles).
    let ds_offset = 112;
    bytes[ds_offset..ds_offset + 4].copy_from_slice(&16u32.to_le_bytes());

    // Splice the 16-byte vertex BEFORE the trailing particle_data_size
    // (offset 116) — same pattern as the data_size-mismatch test above.
    let particle_size_offset = 116;
    let mut vertex_payload = Vec::with_capacity(16);
    vertex_payload.extend_from_slice(&7.0f32.to_le_bytes()); // x
    vertex_payload.extend_from_slice(&8.0f32.to_le_bytes()); // y
    vertex_payload.extend_from_slice(&9.0f32.to_le_bytes()); // z
    // `Unused W` sentinel — pre-fix this would have been absorbed
    // into `bitangent_x` and (if `VF_TANGENTS` were also set
    // somewhere downstream) propagated into the tangent buffer.
    let sentinel = f32::from_bits(0xDEAD_BEEF);
    vertex_payload.extend_from_slice(&sentinel.to_le_bytes());
    bytes.splice(particle_size_offset..particle_size_offset, vertex_payload);

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("BSTriShape with VF_VERTEX (no tangents) must parse");
    let shape = block
        .as_any()
        .downcast_ref::<BsTriShape>()
        .expect("BSTriShape did not downcast");

    assert_eq!(shape.vertices.len(), 1);
    assert!((shape.vertices[0].x - 7.0).abs() < 1e-6);
    assert!((shape.vertices[0].y - 8.0).abs() < 1e-6);
    assert!((shape.vertices[0].z - 9.0).abs() < 1e-6);
    assert!(
        shape.tangents.is_empty(),
        "no-tangents BSTriShape must not produce tangent data; \
         pre-fix the Unused W sentinel was leaking into bitangent_x"
    );
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "stream must consume exactly — the Unused W byte must be \
         skipped (not silently captured)"
    );
}
