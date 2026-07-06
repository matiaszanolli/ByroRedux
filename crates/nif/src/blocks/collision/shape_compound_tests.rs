//! Version-gate regression tests for compound/BV-tree collision shapes.

use super::*;
use crate::header::NifHeader;
use crate::version::{NifVariant, NifVersion};

/// Hybrid header `(20.2.0.7, user_version=11, bsver)`. For bsver outside
/// every game's fan-out this detects as the `Unknown` variant — the corner
/// where a `variant()` feature helper disagrees with the file's raw BSVER.
fn hybrid_header(bsver: u32) -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 11,
        user_version_2: bsver,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// Regression: #1839 / NIF-D2-02 — `bhkMoppBvTreeShape.Build Type` must be
/// gated on the file's raw BSVER (nif.xml `#BS_GT_FO3#`, `bsver > 34`), not
/// on a `variant()` helper. The #1277 refactor substituted
/// `variant().has_shader_alpha_refs()` here — a NiGeometry helper answering
/// a collision-era question (#1511) — which returns `false` on the
/// `Unknown` hybrid corner even though `Build Type` IS authored there.
/// With the raw-bsver gate the 1-byte field is consumed; a revert leaves it
/// unread and `mopp_data` reads one byte early, ending 1 byte short.
#[test]
fn mopp_bv_tree_reads_build_type_on_hybrid_unknown_bsver_over_34() {
    let header = hybrid_header(50); // Unknown variant, bsver > 34
    // `Unknown` is the hybrid corner where a game-variant helper would answer
    // `false` and drop the field; the production gate reads raw bsver (#1839 / #1840).
    let variant = NifVariant::detect(header.version, 11, 50);
    assert_eq!(variant, NifVariant::Unknown);

    let mut d = Vec::new();
    d.extend_from_slice(&7i32.to_le_bytes()); // shape_ref
    d.extend_from_slice(&[0u8; 12]); // unused (skipped)
    d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    d.extend_from_slice(&3u32.to_le_bytes()); // data_size = 3
    for _ in 0..4 {
        d.extend_from_slice(&0.0f32.to_le_bytes()); // origin vec4 (has_mopp_offset)
    }
    d.push(0u8); // build_type (bsver > 34)
    d.extend_from_slice(&[0xAA, 0xBB, 0xCC]); // mopp_data (data_size = 3)
    let expected_len = d.len();

    let mut stream = crate::stream::NifStream::new(&d, &header);
    let shape = BhkMoppBvTreeShape::parse(&mut stream)
        .expect("hybrid-header bhkMoppBvTreeShape should parse");
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "bsver 50 > 34 must consume the 1-byte Build Type (raw-bsver gate); \
         a revert to the variant helper reads mopp_data 1 byte early"
    );
    assert_eq!(
        shape.mopp_data,
        vec![0xAA, 0xBB, 0xCC],
        "mopp_data must start AFTER Build Type"
    );
}
