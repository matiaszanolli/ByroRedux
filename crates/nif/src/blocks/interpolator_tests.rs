//! Unit tests for the interpolator block family. Extracted from
//! `interpolator.rs` to keep the production code coherent; pulled in
//! via `#[cfg(test)] mod tests;` so the rest of the parser sits in one
//! contiguous block.

use super::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;

fn make_header_fnv() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 11,
        user_version_2: 34,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("TestName"), Arc::from("start")],
        max_string_length: 8,
        num_groups: 0,
    }
}

#[test]
fn parse_transform_interpolator() {
    let header = make_header_fnv();
    let mut data = Vec::new();
    // NiQuatTransform: translation (1, 2, 3)
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&2.0f32.to_le_bytes());
    data.extend_from_slice(&3.0f32.to_le_bytes());
    // rotation: identity quat (w=1, x=0, y=0, z=0)
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    // scale: 1.0
    data.extend_from_slice(&1.0f32.to_le_bytes());
    // data_ref: 5
    data.extend_from_slice(&5i32.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let interp = NiTransformInterpolator::parse(&mut stream).unwrap();
    assert_eq!(interp.transform.translation.x, 1.0);
    assert_eq!(interp.transform.translation.y, 2.0);
    assert_eq!(interp.transform.translation.z, 3.0);
    assert_eq!(interp.transform.rotation, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(interp.transform.scale, 1.0);
    assert_eq!(interp.data_ref.index(), Some(5));
    // 3 + 4 + 1 = 8 floats (32 bytes) + 4 byte ref = 36 bytes
    assert_eq!(stream.position(), 36);
}

#[test]
fn parse_transform_data_linear_rotation() {
    let header = make_header_fnv();
    let mut data = Vec::new();

    // 2 rotation keys, type=Linear(1)
    data.extend_from_slice(&2u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes()); // Linear

    // Key 0: time=0.0, quat=(1,0,0,0)
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());

    // Key 1: time=1.0, quat=(0,0,1,0)
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());

    // 0 translation keys
    data.extend_from_slice(&0u32.to_le_bytes());
    // 0 scale keys
    data.extend_from_slice(&0u32.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let td = NiTransformData::parse(&mut stream).unwrap();

    assert_eq!(td.rotation_type, Some(KeyType::Linear));
    assert_eq!(td.rotation_keys.len(), 2);
    assert_eq!(td.rotation_keys[0].time, 0.0);
    assert_eq!(td.rotation_keys[0].value, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(td.rotation_keys[1].time, 1.0);
    assert_eq!(td.rotation_keys[1].value, [0.0, 0.0, 1.0, 0.0]);
    assert!(td.xyz_rotations.is_none());
    assert!(td.translations.keys.is_empty());
    assert!(td.scales.keys.is_empty());
}

#[test]
fn parse_transform_data_with_translation_keys() {
    let header = make_header_fnv();
    let mut data = Vec::new();

    // 0 rotation keys
    data.extend_from_slice(&0u32.to_le_bytes());

    // 2 translation keys, type=Linear(1)
    data.extend_from_slice(&2u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes()); // Linear

    // Key 0: time=0.0, pos=(0,0,0)
    data.extend_from_slice(&0.0f32.to_le_bytes());
    for _ in 0..3 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }

    // Key 1: time=1.0, pos=(10,20,30)
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&10.0f32.to_le_bytes());
    data.extend_from_slice(&20.0f32.to_le_bytes());
    data.extend_from_slice(&30.0f32.to_le_bytes());

    // 0 scale keys
    data.extend_from_slice(&0u32.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let td = NiTransformData::parse(&mut stream).unwrap();

    assert!(td.rotation_keys.is_empty());
    assert_eq!(td.translations.keys.len(), 2);
    assert_eq!(td.translations.key_type, KeyType::Linear);
    assert_eq!(td.translations.keys[1].value, [10.0, 20.0, 30.0]);
}

#[test]
fn parse_float_interpolator() {
    let header = make_header_fnv();
    let mut data = Vec::new();
    data.extend_from_slice(&42.0f32.to_le_bytes()); // value
    data.extend_from_slice(&7i32.to_le_bytes()); // data_ref

    let mut stream = NifStream::new(&data, &header);
    let fi = NiFloatInterpolator::parse(&mut stream).unwrap();
    assert_eq!(fi.value, 42.0);
    assert_eq!(fi.data_ref.index(), Some(7));
}

#[test]
fn parse_float_data_linear() {
    let header = make_header_fnv();
    let mut data = Vec::new();

    // 2 keys, Linear
    data.extend_from_slice(&2u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes()); // Linear

    data.extend_from_slice(&0.0f32.to_le_bytes()); // time
    data.extend_from_slice(&0.0f32.to_le_bytes()); // value

    data.extend_from_slice(&1.0f32.to_le_bytes()); // time
    data.extend_from_slice(&1.0f32.to_le_bytes()); // value

    let mut stream = NifStream::new(&data, &header);
    let fd = NiFloatData::parse(&mut stream).unwrap();
    assert_eq!(fd.keys.keys.len(), 2);
    assert_eq!(fd.keys.key_type, KeyType::Linear);
}

#[test]
fn parse_text_key_extra_data() {
    let header = make_header_fnv();
    let mut data = Vec::new();

    // name: string table index 0 = "TestName"
    data.extend_from_slice(&0i32.to_le_bytes());
    // num_text_keys: 1
    data.extend_from_slice(&1u32.to_le_bytes());
    // key 0: time=0.0, text=string table index 1 = "start"
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1i32.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let tk = NiTextKeyExtraData::parse(&mut stream).unwrap();
    assert_eq!(tk.name.as_deref(), Some("TestName"));
    assert_eq!(tk.text_keys.len(), 1);
    assert_eq!(tk.text_keys[0].0, 0.0);
    assert_eq!(tk.text_keys[0].1, "start");
}

#[test]
fn parse_text_key_extra_data_rejects_malicious_count() {
    // Regression #388 / OBL-D5-C1: a corrupt/drifted u32 used to
    // OOM the process via `Vec::with_capacity(num_text_keys as usize)`.
    // The reproducer was Oblivion's `upperclassdisplaycaseblue01.nif`
    // — a 218 KB file claimed 4.24 G text keys, prompting a
    // 135 GB allocation that aborted the process. The new
    // `allocate_vec` bound rejects any count larger than the bytes
    // remaining in the stream.
    let header = make_header_fnv();
    let mut data = Vec::new();
    // name: empty string-table index
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // num_text_keys = u32::MAX → must be rejected, not OOM
    data.extend_from_slice(&u32::MAX.to_le_bytes());
    // No payload follows; if the bound check were missing, parse
    // would call `Vec::with_capacity(u32::MAX as usize)` and
    // potentially abort the process before any read fails.

    let mut stream = NifStream::new(&data, &header);
    let result = NiTextKeyExtraData::parse(&mut stream);
    assert!(result.is_err(), "expected Err on malicious count");
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("only") && msg.contains("bytes remain"),
        "expected allocate_vec budget message, got: {msg}"
    );
}

#[test]
fn parse_transform_data_empty() {
    let header = make_header_fnv();
    let mut data = Vec::new();
    // 0 rotation, 0 translation, 0 scale
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let td = NiTransformData::parse(&mut stream).unwrap();
    assert!(td.rotation_keys.is_empty());
    assert!(td.translations.keys.is_empty());
    assert!(td.scales.keys.is_empty());
    assert_eq!(stream.position(), 12);
}

/// Regression: #436 — XYZ_ROTATION_KEY mode stores euler angles
/// across three independent `KeyGroup<float>` blocks (one per axis).
/// Per nif.xml `NiKeyframeData`, `Num Rotation Keys` MUST be 1 when
/// rotation_type==4; the actual per-axis key counts live in the
/// three KeyGroups. The audit claimed the parser read only the X
/// channel and left Y/Z bytes in the stream — a stale observation;
/// `interpolator.rs:224-229` already reads all three. This test
/// pins that behavior so a future rewrite can't regress to the
/// imagined bug without failing loudly.
/// Regression for #431 — the canonical color-animation chain
/// (`NiColorInterpolator` → `NiColorData`) must parse end-to-end
/// instead of landing as `NiUnknown`. Covers the value default
/// field, the data_ref, and two linear RGBA keys with distinct
/// alpha so a future regression that drops the 4th component
/// fails loudly.
#[test]
fn parse_color_interpolator_and_color_data() {
    let header = make_header_fnv();

    // NiColorInterpolator: value (r, g, b, a) + data_ref.
    let mut data = Vec::new();
    data.extend_from_slice(&0.5f32.to_le_bytes());
    data.extend_from_slice(&0.25f32.to_le_bytes());
    data.extend_from_slice(&0.125f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&9i32.to_le_bytes()); // data_ref

    let mut stream = NifStream::new(&data, &header);
    let ci = NiColorInterpolator::parse(&mut stream).unwrap();
    assert_eq!(ci.value, [0.5, 0.25, 0.125, 1.0]);
    assert_eq!(ci.data_ref.index(), Some(9));
    assert_eq!(stream.position(), 20);

    // NiColorData with 2 Linear RGBA keys (fade from opaque red →
    // half-alpha blue across one second).
    let mut data = Vec::new();
    data.extend_from_slice(&2u32.to_le_bytes()); // num keys
    data.extend_from_slice(&1u32.to_le_bytes()); // Linear
                                                 // Key 0: t=0, (1, 0, 0, 1)
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    // Key 1: t=1, (0, 0, 1, 0.5)
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.5f32.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let cd = NiColorData::parse(&mut stream).unwrap();
    assert_eq!(cd.keys.key_type, KeyType::Linear);
    assert_eq!(cd.keys.keys.len(), 2);
    assert_eq!(cd.keys.keys[0].value, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(cd.keys.keys[1].value, [0.0, 0.0, 1.0, 0.5]);
    assert_eq!(cd.keys.keys[1].time, 1.0);
}

#[test]
fn parse_color_data_empty_keygroup() {
    let header = make_header_fnv();
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes()); // zero keys → no key_type follows
    let mut stream = NifStream::new(&data, &header);
    let cd = NiColorData::parse(&mut stream).unwrap();
    assert!(cd.keys.keys.is_empty());
    assert_eq!(stream.position(), 4);
}

#[test]
fn parse_transform_data_xyz_rotation_reads_all_three_axes() {
    let header = make_header_fnv();
    let mut data = Vec::new();

    // Num Rotation Keys = 1 (spec requires this for XYZ mode).
    data.extend_from_slice(&1u32.to_le_bytes());
    // Rotation Type = XyzRotation (4).
    data.extend_from_slice(&4u32.to_le_bytes());

    // KeyGroup X: 2 Linear keys (time, value).
    //   num_keys, interpolation_type, then (time, value) pairs.
    data.extend_from_slice(&2u32.to_le_bytes()); // num keys
    data.extend_from_slice(&1u32.to_le_bytes()); // Linear
    data.extend_from_slice(&0.0f32.to_le_bytes()); // time
    data.extend_from_slice(&0.1f32.to_le_bytes()); // value
    data.extend_from_slice(&1.0f32.to_le_bytes()); // time
    data.extend_from_slice(&0.2f32.to_le_bytes()); // value

    // KeyGroup Y: 3 Linear keys — different count than X to verify
    // the parser doesn't apply the X count to Y.
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes()); // Linear
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.5f32.to_le_bytes());
    data.extend_from_slice(&1.5f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&2.0f32.to_le_bytes());

    // KeyGroup Z: 1 Linear key — smallest to prove Y's larger
    // count didn't over-consume Z's header.
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes()); // Linear
    data.extend_from_slice(&0.5f32.to_le_bytes());
    data.extend_from_slice(&3.0f32.to_le_bytes());

    // Translations: 0 keys.
    data.extend_from_slice(&0u32.to_le_bytes());
    // Scales: 0 keys.
    data.extend_from_slice(&0u32.to_le_bytes());

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let td = NiTransformData::parse(&mut stream).unwrap();
    assert_eq!(
        stream.position() as usize,
        expected_len,
        "parser must consume every byte of X + Y + Z KeyGroups (audit premise: bytes left in stream → downstream drift)"
    );
    assert_eq!(td.rotation_type, Some(KeyType::XyzRotation));
    assert!(td.rotation_keys.is_empty());

    let xyz = td
        .xyz_rotations
        .as_ref()
        .expect("xyz_rotations must be populated in XyzRotation mode");

    // Each axis has its own distinct key count — proves Y/Z weren't
    // silently skipped or overwritten with X's data.
    assert_eq!(xyz[0].keys.len(), 2, "X axis (2 keys)");
    assert_eq!(
        xyz[1].keys.len(),
        3,
        "Y axis (3 keys) — audit imagined this was missed"
    );
    assert_eq!(
        xyz[2].keys.len(),
        1,
        "Z axis (1 key) — audit imagined this was missed"
    );

    // Spot-check authored values so a future parser that reads
    // three KeyGroups but at the wrong offsets still fails.
    assert_eq!(xyz[0].keys[1].value, 0.2);
    assert_eq!(xyz[1].keys[2].value, 2.0);
    assert_eq!(xyz[2].keys[0].value, 3.0);
}

/// Regression for #714 — nif.xml specifies an `Order` float
/// (4-byte phantom) between `Rotation Type` and the three `XYZ
/// Rotations` KeyGroups when (a) rotation_type == XyzRotation and
/// (b) version <= 10.1.0.0.  Without the fix the stream under-reads
/// by 4 bytes and all subsequent blocks walk 4 bytes early.
#[test]
fn parse_transform_data_pre10_xyz_rotation_consumes_order_float() {
    // Build a header with version 10.0.1.0 (pre-10.1.0.0 boundary)
    let header = NifHeader {
        version: NifVersion(0x0A000100), // 10.0.1.0
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
    };

    let mut data = Vec::new();
    // Num Rotation Keys = 1
    data.extend_from_slice(&1u32.to_le_bytes());
    // Rotation Type = XyzRotation (4)
    data.extend_from_slice(&4u32.to_le_bytes());
    // Order float (only present on pre-10.1): a sentinel 1.23456 so
    // a test that silently skips it would sample the next field wrong
    data.extend_from_slice(&1.23456f32.to_le_bytes());
    // KeyGroup X: 1 Linear key
    data.extend_from_slice(&1u32.to_le_bytes()); // num_keys
    data.extend_from_slice(&1u32.to_le_bytes()); // Linear
    data.extend_from_slice(&0.0f32.to_le_bytes()); // time
    data.extend_from_slice(&0.5f32.to_le_bytes()); // value=0.5 (X)
                                                   // KeyGroup Y: 1 Linear key
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes()); // value=1.0 (Y)
                                                   // KeyGroup Z: 1 Linear key
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&2.0f32.to_le_bytes()); // value=2.0 (Z)
                                                   // Translations: 0 keys
    data.extend_from_slice(&0u32.to_le_bytes());
    // Scales: 0 keys
    data.extend_from_slice(&0u32.to_le_bytes());

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let td = NiTransformData::parse(&mut stream).unwrap();

    assert_eq!(
        stream.position() as usize,
        expected_len,
        "pre-10.1: Order float must be consumed so stream position is exact"
    );
    let xyz = td.xyz_rotations.as_ref().unwrap();
    // If Order were not consumed the first value (1.23456 interpreted
    // as f32 bytes) would land here and the check would fail.
    assert_eq!(xyz[0].keys[0].value, 0.5, "X axis value");
    assert_eq!(xyz[1].keys[0].value, 1.0, "Y axis value");
    assert_eq!(xyz[2].keys[0].value, 2.0, "Z axis value");
}

/// Counterpart to the above: on a v20 NIF there is no Order float.
/// The existing `parse_transform_data_xyz_rotation_reads_all_three_axes`
/// already covers post-10.1 correctness, but this test uses identical
/// key values so the two can be compared side-by-side.
#[test]
fn parse_transform_data_post20_xyz_rotation_has_no_order_float() {
    // post-10.1 header (FNV)
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

    let mut data = Vec::new();
    // Num Rotation Keys = 1
    data.extend_from_slice(&1u32.to_le_bytes());
    // Rotation Type = XyzRotation (4)
    data.extend_from_slice(&4u32.to_le_bytes());
    // NO Order float on v20+
    // KeyGroup X: 1 Linear key
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.5f32.to_le_bytes());
    // KeyGroup Y
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    // KeyGroup Z
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&2.0f32.to_le_bytes());
    // Translations: 0, Scales: 0
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let td = NiTransformData::parse(&mut stream).unwrap();

    assert_eq!(stream.position() as usize, expected_len);
    let xyz = td.xyz_rotations.as_ref().unwrap();
    assert_eq!(xyz[0].keys[0].value, 0.5);
    assert_eq!(xyz[1].keys[0].value, 1.0);
    assert_eq!(xyz[2].keys[0].value, 2.0);
}

/// Regression for #548 — plain `NiBoolInterpolator` keeps the
/// `Plain` discriminator (the pre-fix default) and reports its
/// original wire type name. Guards against the Timeline variant
/// accidentally widening to include plain blocks.
#[test]
fn ni_bool_interpolator_plain_stamps_plain_kind() {
    let header = make_header_fnv();
    let mut data = Vec::new();
    data.push(1u8); // value = true
    data.extend_from_slice(&7i32.to_le_bytes()); // data_ref
    let mut stream = NifStream::new(&data, &header);
    let interp = NiBoolInterpolator::parse(&mut stream).unwrap();
    assert!(interp.value);
    assert_eq!(interp.data_ref.index(), Some(7));
    assert_eq!(interp.kind, BoolInterpolatorKind::Plain);
    assert_eq!(interp.block_type_name(), "NiBoolInterpolator");
    assert_eq!(stream.position() as usize, data.len());
}

/// Regression for #548 — `NiBoolTimelineInterpolator` shares the
/// wire layout of `NiBoolInterpolator` per nif.xml line 3287 (no
/// additional fields), so `parse_timeline` consumes exactly the
/// same 5 bytes (1 byte bool + 4 byte BlockRef). The discriminator
/// is what lets downstream importers tell the two apart — pre-fix
/// 8,450 blocks across FO3 + FNV + Skyrim SE went to NiUnknown.
#[test]
fn ni_bool_timeline_interpolator_parses_identical_wire_layout() {
    let header = make_header_fnv();
    let mut data = Vec::new();
    data.push(0u8); // value = false
    data.extend_from_slice(&12i32.to_le_bytes()); // data_ref
    let mut stream = NifStream::new(&data, &header);
    let interp = NiBoolInterpolator::parse_timeline(&mut stream).unwrap();
    assert!(!interp.value);
    assert_eq!(interp.data_ref.index(), Some(12));
    assert_eq!(interp.kind, BoolInterpolatorKind::Timeline);
    assert_eq!(
        interp.block_type_name(),
        "NiBoolTimelineInterpolator",
        "block_type_name must dispatch on the wire-type discriminator"
    );
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "timeline wire layout is identical to plain — no extra fields per nif.xml line 3287"
    );
}

/// Regression for #548 — the dispatcher must route
/// `NiBoolTimelineInterpolator` through `parse_timeline` (not the
/// plain `parse`). Pre-fix the dispatch arm was absent and the
/// block fell into the `NiUnknown` fallback at `blocks/mod.rs:705`.
#[test]
fn ni_bool_timeline_interpolator_dispatches_via_parse_block() {
    let header = make_header_fnv();
    let mut data = Vec::new();
    data.push(1u8); // value = true
    data.extend_from_slice(&99i32.to_le_bytes()); // data_ref
    let mut stream = NifStream::new(&data, &header);
    let block = crate::blocks::parse_block(
        "NiBoolTimelineInterpolator",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("dispatch must route Timeline variant — pre-fix it was NiUnknown");
    assert_eq!(block.block_type_name(), "NiBoolTimelineInterpolator");
    let interp = block
        .as_any()
        .downcast_ref::<NiBoolInterpolator>()
        .expect("Timeline and plain share the Rust struct");
    assert_eq!(interp.kind, BoolInterpolatorKind::Timeline);
    assert_eq!(interp.data_ref.index(), Some(99));
}

/// Boundary regression for #935 (post-#769 doctrine flip). nif.xml
/// gates `Order` with `until="10.1.0.0"` which is **inclusive** per
/// niftools/nifly (see version.rs doctrine). The field IS present
/// at v10.1.0.0 exactly; the first version that drops it is
/// v10.1.0.1.
#[test]
fn parse_transform_data_xyz_order_at_v10_1_0_0_exactly() {
    let header = NifHeader {
        version: NifVersion::V10_1_0_0,
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
    };
    let mut data = Vec::new();
    data.extend_from_slice(&1u32.to_le_bytes()); // num_rotation_keys = 1
    data.extend_from_slice(&4u32.to_le_bytes()); // KeyType::XyzRotation
    data.extend_from_slice(&0u32.to_le_bytes()); // Order = 0 (XYZ) — IS read at v10.1.0.0 (inclusive)
    data.extend_from_slice(&0u32.to_le_bytes()); // X KeyGroup num_keys = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // Y KeyGroup num_keys = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // Z KeyGroup num_keys = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // translations num_keys = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // scales num_keys = 0

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let td = NiTransformData::parse(&mut stream)
        .expect("v10.1.0.0 NiTransformData must consume Order under inclusive doctrine");
    assert_eq!(stream.position() as usize, expected_len);
    assert_eq!(td.rotation_type, Some(KeyType::XyzRotation));
    assert!(td.xyz_rotations.is_some());
}

/// Boundary above the inclusive `until="10.1.0.0"` — at v10.1.0.1
/// the Order field is finally absent.
#[test]
fn parse_transform_data_xyz_no_order_at_v10_1_0_1() {
    let header = NifHeader {
        version: NifVersion(0x0A010001),
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
    };
    let mut data = Vec::new();
    data.extend_from_slice(&1u32.to_le_bytes()); // num_rotation_keys = 1
    data.extend_from_slice(&4u32.to_le_bytes()); // KeyType::XyzRotation
                                                 // NO Order at v10.1.0.1 (just above the inclusive until= boundary)
    data.extend_from_slice(&0u32.to_le_bytes()); // X KeyGroup num_keys = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // Y KeyGroup num_keys = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // Z KeyGroup num_keys = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // translations num_keys = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // scales num_keys = 0

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let td = NiTransformData::parse(&mut stream)
        .expect("v10.1.0.1 NiTransformData must skip Order under inclusive doctrine");
    assert_eq!(stream.position() as usize, expected_len);
    assert_eq!(td.rotation_type, Some(KeyType::XyzRotation));
    assert!(td.xyz_rotations.is_some());
}

/// Pre-boundary spot check: at v10.0.1.0 (below the boundary) the
/// `Order` field IS still present and must be consumed.
#[test]
fn parse_transform_data_xyz_with_order_below_v10_1_0_0() {
    let header = NifHeader {
        version: NifVersion(0x0A000100), // v10.0.1.0 — below the until= boundary
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
    };
    let mut data = Vec::new();
    data.extend_from_slice(&1u32.to_le_bytes()); // num_rotation_keys = 1
    data.extend_from_slice(&4u32.to_le_bytes()); // KeyType::XyzRotation
    data.extend_from_slice(&0.0f32.to_le_bytes()); // Order (present pre-10.1.0.0)
    data.extend_from_slice(&0u32.to_le_bytes()); // X KeyGroup num_keys = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // Y KeyGroup num_keys = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // Z KeyGroup num_keys = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // translations num_keys = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // scales num_keys = 0

    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);
    let td = NiTransformData::parse(&mut stream)
        .expect("v10.0.1.0 NiTransformData with XYZ rotation must consume Order");
    assert_eq!(stream.position() as usize, expected_len);
    assert!(td.xyz_rotations.is_some());
}
