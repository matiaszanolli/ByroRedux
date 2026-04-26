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

/// Regression: #157 — BSLODTriShape must dispatch to the LOD parser
/// and consume its 3 trailing LOD-size u32s. Previously routed to
/// NiUnknown, breaking FO4 distant LOD.
#[test]
fn bs_lod_tri_shape_dispatches_and_consumes_trailing_bytes() {
    let header = test_header();
    let mut bytes = minimal_bs_tri_shape_bytes();
    // BSLODTriShape trailing: 3 × u32 LOD sizes.
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.extend_from_slice(&5u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("BSLODTriShape", &mut stream, Some(bytes.len() as u32))
        .expect("BSLODTriShape should dispatch through BsTriShape::parse_lod");
    assert!(
        block.as_any().downcast_ref::<BsTriShape>().is_some(),
        "BSLODTriShape did not downcast to BsTriShape"
    );
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "BSLODTriShape trailing LOD sizes not fully consumed"
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

    // 2. BSLODTriShape → LOD { lod0, lod1, lod2 } (values preserved).
    {
        let mut bytes = minimal_bs_tri_shape_bytes();
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(&5u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        let mut stream = crate::stream::NifStream::new(&bytes, &header);
        let block = parse_block("BSLODTriShape", &mut stream, Some(bytes.len() as u32)).unwrap();
        let shape = block.as_any().downcast_ref::<BsTriShape>().unwrap();
        assert_eq!(
            shape.kind,
            BsTriShapeKind::LOD {
                lod0: 10,
                lod1: 5,
                lod2: 1,
            }
        );
        assert_eq!(block.block_type_name(), "BSLODTriShape");
    }

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
