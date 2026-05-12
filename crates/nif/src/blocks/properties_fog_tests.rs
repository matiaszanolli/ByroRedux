//! Regression tests for NiFogProperty parsing (FO3/FNV/Oblivion).

use super::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;
use std::sync::Arc;

fn make_fo3_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 11,
        user_version_2: 21,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("")],
        max_string_length: 0,
        num_groups: 0,
    }
}

#[test]
fn parse_ni_fog_property_fo3() {
    let header = make_fo3_header();
    let mut data = Vec::new();
    // NiObjectNET: name (string index u32) + num_extra u32 + controller_ref i32
    data.extend_from_slice(&0u32.to_le_bytes()); // name index 0
    data.extend_from_slice(&0u32.to_le_bytes()); // num extra data
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller ref = null
                                                    // No NiProperty.Flags (v20.2.0.7 > 10.0.1.2)
                                                    // FogFlags: 1 (enabled)
    data.extend_from_slice(&1u16.to_le_bytes());
    // fog_depth: 0.5
    data.extend_from_slice(&0.5f32.to_le_bytes());
    // fog_color: grey
    data.extend_from_slice(&0.5f32.to_le_bytes());
    data.extend_from_slice(&0.5f32.to_le_bytes());
    data.extend_from_slice(&0.5f32.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let prop = NiFogProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.flags, 1);
    assert!((prop.fog_depth - 0.5).abs() < 1e-6);
    assert!((prop.fog_color[0] - 0.5).abs() < 1e-6);
    assert_eq!(stream.position() as usize, data.len());
}
