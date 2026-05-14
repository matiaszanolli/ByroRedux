//! Interpolator dispatch tests.
//!
//! NiPath, NiLookAt, compressed + uncompressed B-spline variants,
//! BSTreadTransf — covers #394 / #936 / #941 / #978.

use super::{fnv_header_bspline, oblivion_header};
use crate::blocks::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;
use std::sync::Arc;

/// Regression for #394 — `NiPathInterpolator` must consume its
/// full 24-byte body. Used by door hinges and environmental spline
/// motion; pre-#394 these tripped the block_sizes-less Oblivion
/// loader.
#[test]
fn ni_path_interpolator_consumes_full_24_bytes() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0x0003u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // bank_dir
    bytes.extend_from_slice(&0.5f32.to_le_bytes()); // max_bank_angle
    bytes.extend_from_slice(&0.2f32.to_le_bytes()); // smoothing
    bytes.extend_from_slice(&1u16.to_le_bytes()); // follow_axis = Y
    bytes.extend_from_slice(&11i32.to_le_bytes()); // path_data_ref
    bytes.extend_from_slice(&22i32.to_le_bytes()); // percent_data_ref
    assert_eq!(bytes.len(), 24);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiPathInterpolator", &mut stream, Some(bytes.len() as u32))
        .expect("NiPathInterpolator must parse on Oblivion");
    let interp = block
        .as_any()
        .downcast_ref::<crate::blocks::interpolator::NiPathInterpolator>()
        .unwrap();
    assert_eq!(interp.flags, 0x0003);
    assert_eq!(interp.bank_dir, -1);
    assert_eq!(interp.follow_axis, 1);
    assert_eq!(interp.path_data_ref.index(), Some(11));
    assert_eq!(interp.percent_data_ref.index(), Some(22));
    assert_eq!(stream.position() as usize, bytes.len());
}

/// `NiLookAtInterpolator` — surfaced by the R3 histogram (18
/// instances per FNV mesh sweep). Layout for our targets (NIF
/// version <= 20.4.0.12 includes the `Transform` field):
/// 2 (flags) + 4 (look_at) + 4 (look_at_name string ref) +
/// 32 (NiQuatTransform) + 4×3 (TRS interp refs) = 54 B.
///
/// Uses a v20.2.0.7 FNV-shaped header so the `look_at_name` field
/// goes through the string-table path (`>= 0x14010001`) — the
/// failing real-world content is FNV-era and uses table indices,
/// not the legacy inline length-prefixed strings.
#[test]
fn ni_look_at_interpolator_consumes_full_54_bytes() {
    let header = NifHeader {
        version: NifVersion(0x14020007),
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
    // Flags: LOOK_FLIP | LOOK_Y_AXIS = 0x0003.
    bytes.extend_from_slice(&0x0003u16.to_le_bytes());
    // Look At Ptr → NiNode index 7.
    bytes.extend_from_slice(&7i32.to_le_bytes());
    // Look At Name → string-table index 0 ("SkyProp" in
    // oblivion_header).
    bytes.extend_from_slice(&0i32.to_le_bytes());
    // NiQuatTransform: translation (1,2,3), rotation (w,x,y,z) =
    // (1,0,0,0), scale = 1.0. 32 bytes.
    for v in [1.0f32, 2.0, 3.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    for v in [1.0f32, 0.0, 0.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    // Three sub-interpolator refs.
    bytes.extend_from_slice(&11i32.to_le_bytes());
    bytes.extend_from_slice(&12i32.to_le_bytes());
    bytes.extend_from_slice(&13i32.to_le_bytes());
    assert_eq!(bytes.len(), 54);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiLookAtInterpolator",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("NiLookAtInterpolator must parse on Oblivion");
    let interp = block
        .as_any()
        .downcast_ref::<crate::blocks::interpolator::NiLookAtInterpolator>()
        .unwrap();
    use crate::blocks::interpolator::look_at_flags;
    assert_eq!(interp.flags, 0x0003);
    assert_ne!(interp.flags & look_at_flags::LOOK_FLIP, 0);
    assert_ne!(interp.flags & look_at_flags::LOOK_Y_AXIS, 0);
    assert_eq!(interp.flags & look_at_flags::LOOK_Z_AXIS, 0);
    assert_eq!(interp.look_at.index(), Some(7));
    assert_eq!(interp.look_at_name.as_deref(), Some("SkyProp"));
    assert_eq!(interp.transform.translation.x, 1.0);
    assert_eq!(interp.transform.translation.z, 3.0);
    assert_eq!(interp.transform.scale, 1.0);
    assert_eq!(interp.interp_translation.index(), Some(11));
    assert_eq!(interp.interp_roll.index(), Some(12));
    assert_eq!(interp.interp_scale.index(), Some(13));
    assert_eq!(stream.position() as usize, bytes.len());
}

#[test]
fn ni_bspline_comp_float_interpolator_round_trip() {
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    // NiBSplineInterpolator: start_time, stop_time, spline_data_ref,
    // basis_data_ref.
    data.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
    data.extend_from_slice(&1.5f32.to_le_bytes()); // stop_time
    data.extend_from_slice(&7i32.to_le_bytes()); // spline_data_ref
    data.extend_from_slice(&8i32.to_le_bytes()); // basis_data_ref
                                                 // NiBSplineFloatInterpolator: value, handle.
    data.extend_from_slice(&0.25f32.to_le_bytes()); // fallback value
    data.extend_from_slice(&0u32.to_le_bytes()); // handle
                                                 // NiBSplineCompFloatInterpolator: float_offset, float_half_range.
    data.extend_from_slice(&0.5f32.to_le_bytes()); // offset
    data.extend_from_slice(&0.5f32.to_le_bytes()); // half_range

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiBSplineCompFloatInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("NiBSplineCompFloatInterpolator must dispatch");
    assert_eq!(block.block_type_name(), "NiBSplineCompFloatInterpolator");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::NiBSplineCompFloatInterpolator>()
        .expect("dispatch must produce NiBSplineCompFloatInterpolator");

    assert_eq!(interp.start_time, 0.0);
    assert_eq!(interp.stop_time, 1.5);
    assert_eq!(interp.spline_data_ref.index(), Some(7));
    assert_eq!(interp.basis_data_ref.index(), Some(8));
    assert_eq!(interp.value, 0.25);
    assert_eq!(interp.handle, 0);
    assert_eq!(interp.float_offset, 0.5);
    assert_eq!(interp.float_half_range, 0.5);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "32-byte body must be consumed exactly"
    );
}

#[test]
fn ni_bspline_comp_point3_interpolator_round_trip() {
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    // NiBSplineInterpolator base.
    data.extend_from_slice(&0.5f32.to_le_bytes()); // start_time
    data.extend_from_slice(&2.5f32.to_le_bytes()); // stop_time
    data.extend_from_slice(&3i32.to_le_bytes()); // spline_data_ref
    data.extend_from_slice(&4i32.to_le_bytes()); // basis_data_ref
                                                 // NiBSplinePoint3Interpolator: Vector3 value + handle.
    data.extend_from_slice(&0.1f32.to_le_bytes());
    data.extend_from_slice(&0.2f32.to_le_bytes());
    data.extend_from_slice(&0.3f32.to_le_bytes());
    data.extend_from_slice(&12u32.to_le_bytes()); // handle (non-invalid)
                                                  // NiBSplineCompPoint3Interpolator: position_offset, position_half_range.
    data.extend_from_slice(&1.0f32.to_le_bytes()); // offset
    data.extend_from_slice(&2.0f32.to_le_bytes()); // half_range

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiBSplineCompPoint3Interpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("NiBSplineCompPoint3Interpolator must dispatch");
    assert_eq!(block.block_type_name(), "NiBSplineCompPoint3Interpolator");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::NiBSplineCompPoint3Interpolator>()
        .expect("dispatch must produce NiBSplineCompPoint3Interpolator");

    assert_eq!(interp.start_time, 0.5);
    assert_eq!(interp.stop_time, 2.5);
    assert_eq!(interp.spline_data_ref.index(), Some(3));
    assert_eq!(interp.basis_data_ref.index(), Some(4));
    assert_eq!(interp.value, [0.1, 0.2, 0.3]);
    assert_eq!(interp.handle, 12);
    assert_eq!(interp.position_offset, 1.0);
    assert_eq!(interp.position_half_range, 2.0);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "36-byte body must be consumed exactly"
    );
}

#[test]
fn ni_bspline_comp_float_interpolator_invalid_handle_static_fallback() {
    // handle = 0xFFFFFFFF + a non-FLT_MAX `value` means the channel is
    // static — pin that the body still parses cleanly so the anim
    // emitter's static-key path has data to read.
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes()); // null spline_data_ref
    data.extend_from_slice(&(-1i32).to_le_bytes()); // null basis_data_ref
    data.extend_from_slice(&0.75f32.to_le_bytes()); // value
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // INVALID handle
    data.extend_from_slice(&0.0f32.to_le_bytes()); // float_offset
    data.extend_from_slice(&0.0f32.to_le_bytes()); // float_half_range

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiBSplineCompFloatInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("static-handle NiBSplineCompFloatInterpolator must dispatch");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::NiBSplineCompFloatInterpolator>()
        .unwrap();
    assert_eq!(interp.handle, u32::MAX);
    assert_eq!(interp.value, 0.75);
}

// ── #941 / NIF-D5-NEW-02 — BSTreadTransfInterpolator (FO3+) ──────

#[test]
fn fnv_bs_tread_transf_interpolator_round_trip_two_tread_transforms() {
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    // num_tread_transforms = 2
    data.extend_from_slice(&2u32.to_le_bytes());

    // Tread 0: name string-table index = -1 (None), two NiQuatTransforms.
    data.extend_from_slice(&(-1i32).to_le_bytes()); // name
    // T1: translation (1,2,3) + rotation (w=1,x=0,y=0,z=0 identity) + scale=1
    for v in [1.0f32, 2.0, 3.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // T2: translation (4,5,6) + rotation identity + scale=2
    for v in [4.0f32, 5.0, 6.0, 1.0, 0.0, 0.0, 0.0, 2.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }

    // Tread 1: same shape, different values.
    data.extend_from_slice(&(-1i32).to_le_bytes());
    for v in [10.0f32, 20.0, 30.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    for v in [40.0f32, 50.0, 60.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }

    // data_ref → block 5
    data.extend_from_slice(&5i32.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSTreadTransfInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("BSTreadTransfInterpolator must dispatch");
    assert_eq!(block.block_type_name(), "BSTreadTransfInterpolator");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::BsTreadTransfInterpolator>()
        .expect("dispatch must produce BsTreadTransfInterpolator");

    assert_eq!(interp.tread_transforms.len(), 2);
    assert_eq!(interp.tread_transforms[0].transform_1.translation.x, 1.0);
    assert_eq!(interp.tread_transforms[0].transform_2.scale, 2.0);
    assert_eq!(interp.tread_transforms[1].transform_1.translation.x, 10.0);
    assert_eq!(interp.tread_transforms[1].transform_2.translation.z, 60.0);
    assert_eq!(interp.data_ref.index(), Some(5));
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "BSTreadTransfInterpolator must consume the payload exactly — \
         each tread is 68 bytes (4 + 32 + 32), 2 treads + 4-byte count + \
         4-byte data ref = 144 bytes"
    );
    assert_eq!(data.len(), 4 + 2 * 68 + 4, "fixture size sanity check");
}

#[test]
fn fnv_bs_tread_transf_interpolator_empty_array() {
    // num_tread_transforms = 0 — zero-tread case. Edge case for the
    // allocate_vec(0) path and the immediately-following data_ref.
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes()); // count
    data.extend_from_slice(&(-1i32).to_le_bytes()); // null data_ref

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "BSTreadTransfInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("zero-tread BSTreadTransfInterpolator must dispatch");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::BsTreadTransfInterpolator>()
        .unwrap();
    assert!(interp.tread_transforms.is_empty());
    assert!(interp.data_ref.is_null());
    assert_eq!(stream.position() as usize, data.len());
}

// ── #978 / NIF-D5-NEW-02 — uncompressed B-spline interpolator dispatch ──

#[test]
fn ni_bspline_transform_interpolator_uncompressed_round_trip() {
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    // NiBSplineInterpolator base.
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&5i32.to_le_bytes());
    data.extend_from_slice(&6i32.to_le_bytes());
    // NiQuatTransform (32 bytes).
    data.extend_from_slice(&0.0f32.to_le_bytes()); // tx
    data.extend_from_slice(&0.0f32.to_le_bytes()); // ty
    data.extend_from_slice(&0.0f32.to_le_bytes()); // tz
    data.extend_from_slice(&1.0f32.to_le_bytes()); // qw (identity)
    data.extend_from_slice(&0.0f32.to_le_bytes()); // qx
    data.extend_from_slice(&0.0f32.to_le_bytes()); // qy
    data.extend_from_slice(&0.0f32.to_le_bytes()); // qz
    data.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    // Three handles.
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&u32::MAX.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiBSplineTransformInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("NiBSplineTransformInterpolator must dispatch");
    assert_eq!(block.block_type_name(), "NiBSplineTransformInterpolator");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::NiBSplineTransformInterpolator>()
        .expect("dispatch must produce NiBSplineTransformInterpolator");
    assert_eq!(interp.spline_data_ref.index(), Some(5));
    assert_eq!(interp.basis_data_ref.index(), Some(6));
    assert_eq!(interp.translation_handle, 0);
    assert_eq!(interp.rotation_handle, 1);
    assert_eq!(interp.scale_handle, u32::MAX);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "60-byte uncompressed body must be consumed exactly"
    );
}

#[test]
fn ni_bspline_float_interpolator_uncompressed_round_trip() {
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    data.extend_from_slice(&0.0f32.to_le_bytes()); // start
    data.extend_from_slice(&1.0f32.to_le_bytes()); // stop
    data.extend_from_slice(&7i32.to_le_bytes()); // spline_data
    data.extend_from_slice(&8i32.to_le_bytes()); // basis_data
    data.extend_from_slice(&0.42f32.to_le_bytes()); // value
    data.extend_from_slice(&3u32.to_le_bytes()); // handle

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiBSplineFloatInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("NiBSplineFloatInterpolator must dispatch");
    assert_eq!(block.block_type_name(), "NiBSplineFloatInterpolator");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::NiBSplineFloatInterpolator>()
        .expect("dispatch must produce NiBSplineFloatInterpolator");
    assert_eq!(interp.value, 0.42);
    assert_eq!(interp.handle, 3);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "24-byte uncompressed body must be consumed exactly"
    );
}

#[test]
fn ni_bspline_point3_interpolator_uncompressed_round_trip() {
    let header = fnv_header_bspline();
    let mut data = Vec::new();
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&9i32.to_le_bytes());
    data.extend_from_slice(&10i32.to_le_bytes());
    data.extend_from_slice(&0.5f32.to_le_bytes()); // vx
    data.extend_from_slice(&0.6f32.to_le_bytes()); // vy
    data.extend_from_slice(&0.7f32.to_le_bytes()); // vz
    data.extend_from_slice(&15u32.to_le_bytes()); // handle

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiBSplinePoint3Interpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("NiBSplinePoint3Interpolator must dispatch");
    assert_eq!(block.block_type_name(), "NiBSplinePoint3Interpolator");
    let interp = block
        .as_any()
        .downcast_ref::<interpolator::NiBSplinePoint3Interpolator>()
        .expect("dispatch must produce NiBSplinePoint3Interpolator");
    assert_eq!(interp.value, [0.5, 0.6, 0.7]);
    assert_eq!(interp.handle, 15);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "32-byte uncompressed body must be consumed exactly"
    );
}
