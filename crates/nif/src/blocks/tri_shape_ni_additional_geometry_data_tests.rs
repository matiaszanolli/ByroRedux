//! Tests for `ni_additional_geometry_data_tests` extracted from ../tri_shape.rs (refactor stage A).
//!
//! Same qualified path preserved (`ni_additional_geometry_data_tests::FOO`).

use super::*;
use crate::blocks::parse_block;
use crate::header::NifHeader;

fn fnv_header() -> NifHeader {
    // FNV: 20.2.0.7 with bsver = 34. Matches the corpus where the
    // 4,039 pre-fix NiUnknown blocks came from. #547.
    NifHeader {
        version: NifVersion(0x14020007),
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

/// Build a minimal `NiAdditionalGeometryData` body carrying one
/// tangent-channel descriptor and one 16-byte data block (four
/// vertices × 4-byte f32 tangents, synthetic). Used by both the
/// plain-variant and packed-variant round-trip tests below.
fn minimal_agd_bytes(packed: bool) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&4u16.to_le_bytes()); // num_vertices
    d.extend_from_slice(&1u32.to_le_bytes()); // num_block_infos

    // NiAGDDataStream (25 bytes): synthetic tangent channel.
    d.extend_from_slice(&7u32.to_le_bytes()); // type (NiADT_TANGENTS)
    d.extend_from_slice(&4u32.to_le_bytes()); // unit_size
    d.extend_from_slice(&16u32.to_le_bytes()); // total_size
    d.extend_from_slice(&4u32.to_le_bytes()); // stride
    d.extend_from_slice(&0u32.to_le_bytes()); // block_index
    d.extend_from_slice(&0u32.to_le_bytes()); // block_offset
    d.push(0x02u8); // flags (AGD_MUTABLE default)

    d.extend_from_slice(&1u32.to_le_bytes()); // num_blocks

    // NiAGDDataBlocks: has_data = true.
    d.push(1u8);

    // NiAGDDataBlock:
    d.extend_from_slice(&16u32.to_le_bytes()); // block_size
    d.extend_from_slice(&1u32.to_le_bytes()); // num_blocks (inner)
    d.extend_from_slice(&0u32.to_le_bytes()); // block_offsets[0]
    d.extend_from_slice(&1u32.to_le_bytes()); // num_data
    d.extend_from_slice(&16u32.to_le_bytes()); // data_sizes[0]
    d.extend_from_slice(&[0xAAu8; 16]); // data: num_data * block_size = 1 * 16
    if packed {
        d.extend_from_slice(&42u32.to_le_bytes()); // shader_index
        d.extend_from_slice(&16u32.to_le_bytes()); // total_size
    }
    d
}

/// Regression for #547 — plain `NiAdditionalGeometryData` (FO3+FNV)
/// must dispatch, parse to completion, and preserve the tangent-
/// channel descriptor along with the 16-byte data blob.
#[test]
fn ni_additional_geometry_data_plain_dispatches_and_preserves_channels() {
    let header = fnv_header();
    let bytes = minimal_agd_bytes(false);
    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiAdditionalGeometryData",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("dispatch must produce NiAdditionalGeometryData, not NiUnknown");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "entire block body must be consumed"
    );
    assert_eq!(block.block_type_name(), "NiAdditionalGeometryData");
    let agd = block
        .as_any()
        .downcast_ref::<NiAdditionalGeometryData>()
        .expect("dispatch type must be NiAdditionalGeometryData");
    assert_eq!(agd.kind, NiAgdKind::Plain);
    assert_eq!(agd.num_vertices, 4);
    assert_eq!(agd.block_infos.len(), 1);
    assert_eq!(agd.block_infos[0].ty, 7);
    assert_eq!(agd.block_infos[0].unit_size, 4);
    assert_eq!(agd.block_infos[0].total_size, 16);
    assert_eq!(agd.blocks.len(), 1);
    let inner = agd.blocks[0].as_ref().expect("has_data=true");
    assert_eq!(inner.block_size, 16);
    assert_eq!(inner.data.len(), 16);
    assert!(
        inner.shader_index.is_none(),
        "plain variant must not populate shader_index"
    );
}

/// Regression for #547 — packed variant (`BSPackedAdditionalGeometryData`)
/// populates `shader_index` + `total_size` on each data block (nif.xml
/// arg=1 branch). Only appears in older FNV DLC content.
#[test]
fn bs_packed_additional_geometry_data_dispatches_with_extra_fields() {
    let header = fnv_header();
    let bytes = minimal_agd_bytes(true);
    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block(
        "BSPackedAdditionalGeometryData",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("dispatch must produce packed variant");
    assert_eq!(stream.position() as usize, bytes.len());
    assert_eq!(block.block_type_name(), "BSPackedAdditionalGeometryData");
    let agd = block
        .as_any()
        .downcast_ref::<NiAdditionalGeometryData>()
        .expect("packed and plain share the Rust struct");
    assert_eq!(agd.kind, NiAgdKind::Packed);
    let inner = agd.blocks[0].as_ref().unwrap();
    assert_eq!(inner.shader_index, Some(42));
    assert_eq!(inner.total_size, Some(16));
}

/// Regression for #547 — empty block list (`num_blocks = 0`) must
/// parse without allocating or reading any NiAGDDataBlock. Mirrors
/// the vanilla FO3 pattern where some static props ship a shell
/// block-info array with no attached data.
#[test]
fn ni_additional_geometry_data_with_empty_block_list_parses() {
    let header = fnv_header();
    let mut d = Vec::new();
    d.extend_from_slice(&0u16.to_le_bytes()); // num_vertices
    d.extend_from_slice(&0u32.to_le_bytes()); // num_block_infos
    d.extend_from_slice(&0u32.to_le_bytes()); // num_blocks
    let mut stream = crate::stream::NifStream::new(&d, &header);
    let block = parse_block(
        "NiAdditionalGeometryData",
        &mut stream,
        Some(d.len() as u32),
    )
    .expect("empty AGD must still dispatch");
    assert_eq!(stream.position() as usize, d.len());
    let agd = block
        .as_any()
        .downcast_ref::<NiAdditionalGeometryData>()
        .unwrap();
    assert!(agd.block_infos.is_empty());
    assert!(agd.blocks.is_empty());
}
