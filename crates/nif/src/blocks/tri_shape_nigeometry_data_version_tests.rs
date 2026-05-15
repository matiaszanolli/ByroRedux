//! Tests for `nigeometry_data_version_tests` extracted from ../tri_shape.rs (refactor stage A).
//!
//! Same qualified path preserved (`nigeometry_data_version_tests::FOO`).

use super::*;
use crate::header::NifHeader;

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

/// Minimal NiGeometryData body with zero vertices/normals/UVs/colors.
/// Each optional pair matches one nif.xml version gate:
///  - `include_group_id`      — `Group ID` (since 10.1.0.114, 4 B)
///  - `include_keep_compress` — `Keep/Compress Flags` (since 10.1.0.0, 2 B)
///  - `include_consistency`   — `Consistency Flags` (since 10.0.1.0, 2 B)
fn nigeometry_data_bytes(
    include_group_id: bool,
    include_keep_compress: bool,
    include_consistency: bool,
) -> Vec<u8> {
    let mut d = Vec::new();
    if include_group_id {
        d.extend_from_slice(&0i32.to_le_bytes()); // group_id
    }
    // num_vertices = 0
    d.extend_from_slice(&0u16.to_le_bytes());
    if include_keep_compress {
        d.push(0u8); // keep_flags
        d.push(0u8); // compress_flags
    }
    d.push(0u8); // has_vertices = false
                 // data_flags (u16) — version >= 10.0.1.0 branch.
    d.extend_from_slice(&0u16.to_le_bytes());
    d.push(0u8); // has_normals = false
                 // bounding sphere: center(3 f32) + radius(f32)
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes());
    }
    d.extend_from_slice(&0.0f32.to_le_bytes());
    d.push(0u8); // has_vertex_colors = false
                 // (data_flags=0 ⇒ num_uv_sets=0 ⇒ has_uv=false, no UV arrays)
    if include_consistency {
        d.extend_from_slice(&0u16.to_le_bytes()); // consistency_flags
    }
    d
}

/// Regression: #327 / audit N1-02 — at NIF 10.0.1.0 the parser must
/// NOT consume `keep_flags` / `compress_flags`. Those fields
/// appear only from 10.1.0.0 per nif.xml. Previously this branch
/// stole 2 bytes from `has_vertices` + `data_flags`, corrupting
/// every downstream field. With #326 applied, `Group ID` is also
/// absent (since 10.1.0.114).
#[test]
fn nigeometry_data_at_10_0_1_0_skips_keep_compress_flags() {
    let header = header_at(NifVersion::V10_0_1_0); // 10.0.1.0 — in the gap.
    let bytes = nigeometry_data_bytes(
        /*include_group_id=*/ false, /*include_keep_compress=*/ false,
        /*include_consistency=*/ true,
    );
    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let _ = parse_geometry_data_base(&mut stream)
        .expect("NiGeometryData base should parse at 10.0.1.0");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "at 10.0.1.0 NiGeometryData must NOT consume group_id or keep/compress"
    );
}

/// At NIF 10.1.0.0 (the corrected keep/compress threshold) the 2
/// flags bytes ARE consumed. `Group ID` is still absent — it only
/// appears from 10.1.0.114.
#[test]
fn nigeometry_data_at_10_1_0_0_reads_keep_compress_flags() {
    let header = header_at(NifVersion::V10_1_0_0); // 10.1.0.0 — threshold.
    let bytes = nigeometry_data_bytes(
        /*include_group_id=*/ false, /*include_keep_compress=*/ true,
        /*include_consistency=*/ true,
    );
    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let _ = parse_geometry_data_base(&mut stream)
        .expect("NiGeometryData base should parse at 10.1.0.0");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "at 10.1.0.0 NiGeometryData MUST consume keep/compress flags"
    );
}

/// Regression: #326 / audit N1-01 — `Group ID` is only serialized
/// from 10.1.0.114 onward per nif.xml. Previously read from 10.0.1.0,
/// stealing 4 bytes in the [10.0.1.0, 10.1.0.114) window (non-Bethesda
/// Gamebryo pre-Civ IV era).
#[test]
fn nigeometry_data_at_10_1_0_113_skips_group_id() {
    let header = header_at(NifVersion::V10_1_0_113); // 10.1.0.113 — one below.
    let bytes = nigeometry_data_bytes(
        /*include_group_id=*/ false, /*include_keep_compress=*/ true,
        /*include_consistency=*/ true,
    );
    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let _ = parse_geometry_data_base(&mut stream)
        .expect("NiGeometryData base should parse at 10.1.0.113");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "at 10.1.0.113 NiGeometryData must NOT consume group_id"
    );
}

/// Regression: #440 / FO3-5-01. Bethesda streams (BSVER > 0,
/// version 20.2.0.7) interpret `dataFlags` as `BSGeometryDataFlags`,
/// where bit 0 is a `Has UV` bool — exactly 0 or 1 UV sets. The
/// non-Bethesda `NiGeometryDataFlags` decode (bits 0..5 = count)
/// would read bits 1..5 as additional UV slots, over-reading N ×
/// `num_vertices × 8` bytes. On a real FO3 FaceGen head
/// (`headfemalefacegen.nif`, 1307 vertices, `data_flags = 0x1003`)
/// the pre-fix decode asked for 3 UV sets and over-read enough to
/// demote every FO3 NPC face to `NiUnknown`.
#[test]
fn bs_geometry_data_flags_decodes_has_uv_bit0_only() {
    // FO3/FNV header: NIF 20.2.0.7, user_version=11, bsver=34.
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 11,
        user_version_2: 34,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    };
    // Build a minimal NiGeometryData body for 2 vertices, no normals,
    // no vcolor, 1 UV set, data_flags = 0x1003 (bits 0, 1, 12 set).
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes()); // group_id
    data.extend_from_slice(&2u16.to_le_bytes()); // num_vertices
    data.push(0u8); // keep_flags
    data.push(0u8); // compress_flags
    data.push(1u8); // has_vertices
                    // Two vertices, 12 bytes each.
    for _ in 0..2 {
        for _ in 0..3 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
    }
    // data_flags: bit 0 = HasUV, bit 1 = unused noise, bit 12 = tangents
    data.extend_from_slice(&0x1003u16.to_le_bytes());
    data.push(0u8); // has_normals = false (no NBT payload to read)
                    // bounding sphere
    for _ in 0..3 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.push(0u8); // has_vertex_colors = false
                    // Exactly 1 UV set (per BS decode) × 2 vertices × 8 bytes = 16 bytes
    for _ in 0..2 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }
    data.extend_from_slice(&0u16.to_le_bytes()); // consistency
    data.extend_from_slice(&(-1i32).to_le_bytes()); // additional_data_ref
    let expected_len = data.len();

    let mut stream = crate::stream::NifStream::new(&data, &header);
    let (verts, flags, _norms, _c, _r, _vc, uvs) = parse_geometry_data_base(&mut stream)
        .expect("FO3 NiGeometryData should parse with BS data flag decode");
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "BS decode must consume exactly 1 UV set; got position {} expected {}",
        stream.position(),
        expected_len
    );
    assert_eq!(flags, 0x1003);
    assert_eq!(verts.len(), 2);
    assert_eq!(uvs.len(), 1, "BS decode: bit 0 = 1 UV set, bit 1 is noise");
}

/// Non-Bethesda Gamebryo streams (bsver = 0) keep the
/// `NiGeometryDataFlags` decode where bits 0..5 encode a 6-bit
/// count. `data_flags = 0x0003` must still mean 3 UV sets on that
/// path — the BS fix must not break vanilla Gamebryo content.
#[test]
fn ni_geometry_data_flags_decodes_count_on_non_bethesda() {
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 0,
        user_version_2: 0, // bsver=0 → NiGeometryDataFlags path
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    };
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes()); // group_id
    data.extend_from_slice(&1u16.to_le_bytes()); // num_vertices = 1
    data.push(0u8); // keep
    data.push(0u8); // compress
    data.push(1u8); // has_vertices
    for _ in 0..3 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }
    // data_flags = 3 → NiGeometryDataFlags count = 3 UV sets
    data.extend_from_slice(&0x0003u16.to_le_bytes());
    data.push(0u8); // has_normals
    for _ in 0..3 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.push(0u8); // has_vertex_colors
                    // 3 UV sets × 1 vertex × 8 bytes = 24
    for _ in 0..3 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }
    data.extend_from_slice(&0u16.to_le_bytes()); // consistency
    data.extend_from_slice(&(-1i32).to_le_bytes()); // additional_data

    let mut stream = crate::stream::NifStream::new(&data, &header);
    let (_v, _f, _n, _c, _r, _vc, uvs) = parse_geometry_data_base(&mut stream)
        .expect("non-Bethesda NiGeometryData should parse with count decode");
    assert_eq!(uvs.len(), 3, "non-Bethesda: bits 0..5 encode UV count");
}

/// Dual-side for #326: at 10.1.0.114 the `group_id` i32 IS consumed.
#[test]
fn nigeometry_data_at_10_1_0_114_reads_group_id() {
    let header = header_at(NifVersion::V10_1_0_114); // 10.1.0.114 — threshold.
    let bytes = nigeometry_data_bytes(
        /*include_group_id=*/ true, /*include_keep_compress=*/ true,
        /*include_consistency=*/ true,
    );
    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let _ = parse_geometry_data_base(&mut stream)
        .expect("NiGeometryData base should parse at 10.1.0.114");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "at 10.1.0.114 NiGeometryData MUST consume group_id"
    );
}
