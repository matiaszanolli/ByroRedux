//! Regression coverage for #549 / NIF-04 — `bhkBlendCollisionObject`
//! ships two extra `Unknown Float` fields on `bsver < 9` per
//! nif.xml line 3428-3429. Pre-fix, those 8 bytes were unread and
//! the stream drifted; downstream blocks (host `NiNode`, every
//! later `bhkRigidBody`) fell into `NiUnknown`. The vanilla
//! Oblivion BSA's `meshes\creatures\boxtest\skeleton.nif`
//! (`v10.2.0.0` / `bsver=6`) was the surfaced victim — 4
//! `bhkRigidBody` failures + a host of cascade misalignments.

use super::*;
use crate::blocks::parse_block;
use crate::header::NifHeader;
use crate::version::NifVersion;

/// Pre-Oblivion-mainline header — NIF v10.2.0.0 / `bsver=6`,
/// matching the corpus where the 4 (-of-6) reported `bhkRigidBody`
/// failures actually originated.
fn pre_oblivion_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V10_2_0_0,
        little_endian: true,
        user_version: 10,
        user_version_2: 6,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// Oblivion-mainline header — NIF v20.0.0.5 / `bsver=11`. Sibling
/// fixture so the test pins both the new `bsver < 9` arm AND the
/// existing `bsver >= 9` arm against the same parser body.
fn oblivion_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_0_0_5,
        little_endian: true,
        user_version: 11,
        user_version_2: 11,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// Write a synthetic `bhkBlendCollisionObject` payload. `with_pre_oblivion_pad`
/// adds the two `Unknown Float` fields the `bsver < 9` schema
/// emits — the parser MUST consume them on the pre-Oblivion path
/// and MUST NOT on the Oblivion+ path, otherwise the stream drifts.
fn bhk_blend_collision_object_bytes(with_pre_oblivion_pad: bool) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref
    d.extend_from_slice(&0x89_u16.to_le_bytes()); // flags (Sky+ default)
    d.extend_from_slice(&(-1i32).to_le_bytes()); // body_ref
    d.extend_from_slice(&1.0f32.to_le_bytes()); // heir_gain
    d.extend_from_slice(&1.0f32.to_le_bytes()); // vel_gain
    if with_pre_oblivion_pad {
        d.extend_from_slice(&0.0f32.to_le_bytes()); // unknown_float_1
        d.extend_from_slice(&0.0f32.to_le_bytes()); // unknown_float_2
    }
    d
}

/// `bsver < 9` payload (26 B) must consume the full body — the
/// parse must not leave the trailing 8 B for the next block to
/// misread. Pre-fix this test would expose 8 B of slack.
#[test]
fn bsver_lt_9_consumes_unknown_float_pair() {
    let header = pre_oblivion_header();
    let bytes = bhk_blend_collision_object_bytes(true);
    assert_eq!(bytes.len(), 26, "bsver<9 wire size is 26 B");

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block(
        "bhkBlendCollisionObject",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("bsver<9 bhkBlendCollisionObject must parse");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "must consume the full 26 B — pre-fix left 8 B of trailing pad"
    );
    let _ = block
        .as_any()
        .downcast_ref::<BhkCollisionObject>()
        .expect("dispatch must yield BhkCollisionObject, not NiUnknown");
}

/// Oblivion-mainline `bsver >= 9` payload (18 B) must NOT consume
/// the legacy pad — over-reading would over-consume the next
/// block's bytes.
#[test]
fn bsver_gte_9_does_not_read_unknown_float_pair() {
    let header = oblivion_header();
    let bytes = bhk_blend_collision_object_bytes(false);
    assert_eq!(bytes.len(), 18, "bsver>=9 wire size is 18 B");

    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let _ = parse_block(
        "bhkBlendCollisionObject",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("Oblivion bhkBlendCollisionObject must parse with the same body");
    assert_eq!(stream.position() as usize, bytes.len());
}

/// `bhkCollisionObject` (the non-blend base) must NOT read the
/// pre-Oblivion pad regardless of `bsver` — only the blend
/// variant carries the trailer per nif.xml.
#[test]
fn bsver_lt_9_non_blend_skips_unknown_float_pair() {
    let header = pre_oblivion_header();
    let mut d = Vec::new();
    d.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref
    d.extend_from_slice(&0x1_u16.to_le_bytes()); // flags
    d.extend_from_slice(&(-1i32).to_le_bytes()); // body_ref
    assert_eq!(d.len(), 10);

    let mut stream = crate::stream::NifStream::new(&d, &header);
    let _ = parse_block("bhkCollisionObject", &mut stream, Some(d.len() as u32))
        .expect("bsver<9 non-blend bhkCollisionObject must parse");
    assert_eq!(
        stream.position() as usize,
        d.len(),
        "non-blend variant must not pull the blend-only trailer"
    );
}
