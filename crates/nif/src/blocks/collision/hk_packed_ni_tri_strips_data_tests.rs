//! Regression tests for `hkPackedNiTriStripsData` parsing.
//!
//! Pins the `Compressed` bool dispatch (nif.xml lines 3962-3967):
//! `Compressed == 0` → `Vector3[]` (12 B/vertex f32), `Compressed != 0`
//! → `HalfVector3[]` (6 B/vertex IEEE half-float). See issue #975
//! (NIF-D1-NEW-01) for the silent-scramble failure mode the fix
//! prevents.

use super::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;

/// FO3+ header (NIF 20.2.0.7) — `Compressed` byte present.
fn fo3_header() -> NifHeader {
    NifHeader {
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
    }
}

/// Write the per-triangle FO3+ shape: 3 × u16 indices + u16 welding.
fn push_triangle(buf: &mut Vec<u8>, v0: u16, v1: u16, v2: u16, welding: u16) {
    buf.extend_from_slice(&v0.to_le_bytes());
    buf.extend_from_slice(&v1.to_le_bytes());
    buf.extend_from_slice(&v2.to_le_bytes());
    buf.extend_from_slice(&welding.to_le_bytes());
}

/// Build the trailing `Num Sub Shapes` u16 + zero entries — present
/// since 20.2.0.7. Used to confirm the parser lands at the right
/// offset after vertices (the scramble symptom from #975).
fn push_zero_sub_shapes(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&0u16.to_le_bytes());
}

#[test]
fn parses_uncompressed_f32_vertices() {
    let mut d = Vec::new();
    // num_triangles = 1
    d.extend_from_slice(&1u32.to_le_bytes());
    push_triangle(&mut d, 0, 1, 2, 0);
    // num_vertices = 2
    d.extend_from_slice(&2u32.to_le_bytes());
    // compressed = 0
    d.push(0u8);
    // Two f32 vertices @ 12 B each.
    for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    push_zero_sub_shapes(&mut d);

    let header = fo3_header();
    let mut stream = NifStream::new(&d, &header);
    let parsed = HkPackedNiTriStripsData::parse(&mut stream).expect("parse should succeed");

    assert_eq!(parsed.triangles.len(), 1);
    assert_eq!(parsed.vertices.len(), 2);
    assert_eq!(parsed.vertices[0], [1.0, 2.0, 3.0]);
    assert_eq!(parsed.vertices[1], [4.0, 5.0, 6.0]);
    // Sub-shape header consumed: stream should be at end of buffer.
    assert_eq!(
        stream.position() as usize,
        d.len(),
        "parser must consume sub-shape trailer; pre-#975 over-read by 6 B \
         per vertex would land short of EOF"
    );
}

#[test]
fn parses_compressed_half_float_vertices() {
    let mut d = Vec::new();
    // num_triangles = 1
    d.extend_from_slice(&1u32.to_le_bytes());
    push_triangle(&mut d, 0, 1, 2, 0);
    // num_vertices = 2
    d.extend_from_slice(&2u32.to_le_bytes());
    // compressed = 1
    d.push(1u8);
    // IEEE half-float bit patterns: 1.0 = 0x3C00, 2.0 = 0x4000,
    // 3.0 = 0x4200, 4.0 = 0x4400, 5.0 = 0x4500, 6.0 = 0x4600.
    for h in [0x3C00u16, 0x4000, 0x4200, 0x4400, 0x4500, 0x4600] {
        d.extend_from_slice(&h.to_le_bytes());
    }
    push_zero_sub_shapes(&mut d);

    let header = fo3_header();
    let mut stream = NifStream::new(&d, &header);
    let parsed = HkPackedNiTriStripsData::parse(&mut stream).expect("parse should succeed");

    assert_eq!(parsed.vertices.len(), 2);
    assert_eq!(parsed.vertices[0], [1.0, 2.0, 3.0]);
    assert_eq!(parsed.vertices[1], [4.0, 5.0, 6.0]);
    // Stream must land exactly at EOF — pre-#975 the parser read
    // 12 B/vertex (f32) here and over-read, scrambling the sub-shape
    // count and dragging arbitrary downstream bytes into the vertex
    // buffer.
    assert_eq!(stream.position() as usize, d.len());
}

#[test]
fn pre_v20_2_0_7_skips_compressed_byte() {
    // Oblivion era — no `Compressed` byte on disk and no sub-shape
    // trailer. The parser must NOT consume the byte that follows
    // num_vertices.
    let mut d = Vec::new();
    d.extend_from_slice(&1u32.to_le_bytes());
    push_triangle(&mut d, 0, 1, 2, 0);
    // Oblivion era: per-triangle normal is 3 × f32 in the data block
    // (FO3+ folds the normal into the welding info u16 instead).
    for v in [0.0f32, 0.0, 1.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&1u32.to_le_bytes()); // num_vertices = 1
    for v in [9.0f32, 8.0, 7.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }

    let mut header = fo3_header();
    header.version = NifVersion::V20_0_0_5; // 20.0.0.5 (Oblivion)
    header.user_version = 11;
    header.user_version_2 = 11;

    let mut stream = NifStream::new(&d, &header);
    let parsed = HkPackedNiTriStripsData::parse(&mut stream).expect("parse should succeed");

    assert_eq!(parsed.vertices, vec![[9.0, 8.0, 7.0]]);
    assert_eq!(stream.position() as usize, d.len());
}
