//! Regression coverage for #980 / NIF-D5-NEW-04 — `bhkPoseArray`,
//! `bhkRagdollTemplate`, and `bhkRagdollTemplateData` must dispatch
//! through their dedicated parsers rather than fall through to
//! `NiUnknown` on FO3/FNV content. Pre-fix the FO3+ death-pose +
//! ragdoll-template system was silently disabled.

use super::*;
use crate::blocks::parse_block;
use crate::blocks::NiUnknown;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;

/// FO3/FNV-style header (NIF v20.2.0.7, BSVER 34). The string table
/// is empty; `read_string()` returns `None` for any non-negative
/// index that's out of range.
fn fo3_header() -> NifHeader {
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

/// Build a synthetic `bhkPoseArray` block:
///   - 2 bones (string-table indices `-1`, `-1` so both resolve to `None`)
///   - 1 pose with 2 bone transforms.
fn build_pose_array_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    // Num Bones
    d.extend_from_slice(&2u32.to_le_bytes());
    // Two bone string-table indices.
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // Num Poses
    d.extend_from_slice(&1u32.to_le_bytes());
    // Pose 0: Num Transforms = 2; two 40-byte BoneTransforms.
    d.extend_from_slice(&2u32.to_le_bytes());
    for _ in 0..2 {
        // Translation
        d.extend_from_slice(&1.0f32.to_le_bytes());
        d.extend_from_slice(&2.0f32.to_le_bytes());
        d.extend_from_slice(&3.0f32.to_le_bytes());
        // Rotation quat (x, y, z, w)
        d.extend_from_slice(&0.0f32.to_le_bytes());
        d.extend_from_slice(&0.0f32.to_le_bytes());
        d.extend_from_slice(&0.0f32.to_le_bytes());
        d.extend_from_slice(&1.0f32.to_le_bytes());
        // Scale
        d.extend_from_slice(&1.0f32.to_le_bytes());
        d.extend_from_slice(&1.0f32.to_le_bytes());
        d.extend_from_slice(&1.0f32.to_le_bytes());
    }
    d
}

#[test]
fn bhk_pose_array_dispatches_through_dedicated_parser() {
    let header = fo3_header();
    let bytes = build_pose_array_bytes();
    let mut stream = NifStream::new(&bytes, &header);
    let block =
        parse_block("bhkPoseArray", &mut stream, Some(bytes.len() as u32)).expect("parse");
    // Must NOT fall through to NiUnknown — that's the entire #980 fix.
    assert!(
        block.as_any().downcast_ref::<NiUnknown>().is_none(),
        "bhkPoseArray must dispatch to BhkPoseArray, not NiUnknown"
    );
    let pa = block
        .as_any()
        .downcast_ref::<BhkPoseArray>()
        .expect("should be BhkPoseArray");
    assert_eq!(pa.bones.len(), 2);
    assert_eq!(pa.poses.len(), 1);
    assert_eq!(pa.poses[0].transforms.len(), 2);
    assert_eq!(pa.poses[0].transforms[0].translation, [1.0, 2.0, 3.0]);
    assert_eq!(pa.poses[0].transforms[0].rotation, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(pa.poses[0].transforms[0].scale, [1.0, 1.0, 1.0]);
    // Parser must consume the full block (no drift into the next block).
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Build a synthetic `bhkRagdollTemplate` (NiExtraData base + 3 bone refs).
fn build_ragdoll_template_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    // Inherited NiExtraData.Name — gated on version >= 10.0.1.0. FO3
    // is v20.2.0.7 so it IS present; string-table index `-1` resolves
    // to `None`.
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // Num Bones
    d.extend_from_slice(&3u32.to_le_bytes());
    // Three BlockRefs (negative = null).
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&5i32.to_le_bytes());
    d.extend_from_slice(&12i32.to_le_bytes());
    d
}

#[test]
fn bhk_ragdoll_template_dispatches_through_dedicated_parser() {
    let header = fo3_header();
    let bytes = build_ragdoll_template_bytes();
    let mut stream = NifStream::new(&bytes, &header);
    let block =
        parse_block("bhkRagdollTemplate", &mut stream, Some(bytes.len() as u32)).expect("parse");
    assert!(
        block.as_any().downcast_ref::<NiUnknown>().is_none(),
        "bhkRagdollTemplate must dispatch to BhkRagdollTemplate, not NiUnknown"
    );
    let rt = block
        .as_any()
        .downcast_ref::<BhkRagdollTemplate>()
        .expect("should be BhkRagdollTemplate");
    assert!(rt.name.is_none());
    assert_eq!(rt.bones.len(), 3);
    // First ref is null (-1 → None when `index()` is called); 2nd /
    // 3rd resolve to block indices 5 and 12 respectively. The struct
    // holds the raw BlockRef — downstream code calls `.index()`.
    assert!(rt.bones[0].index().is_none());
    assert_eq!(rt.bones[1].index(), Some(5));
    assert_eq!(rt.bones[2].index(), Some(12));
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Build a synthetic `bhkRagdollTemplateData` — fixed-layout head
/// (35 bytes: name + 4 floats + material + count) followed by 8
/// bytes of opaque constraint-array payload that the stub must skip.
fn build_ragdoll_template_data_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    // Name (string-table idx -1 → None)
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // Mass / Restitution / Friction / Radius
    d.extend_from_slice(&9.0f32.to_le_bytes());
    d.extend_from_slice(&0.8f32.to_le_bytes());
    d.extend_from_slice(&0.3f32.to_le_bytes());
    d.extend_from_slice(&1.0f32.to_le_bytes());
    // Material (HavokMaterial enum — u32)
    d.extend_from_slice(&7u32.to_le_bytes());
    // Num Constraints (the stub records this but skips the array body)
    d.extend_from_slice(&2u32.to_le_bytes());
    // Opaque constraint-array payload — 8 bytes; the stub skips
    // this via block_size so the stream still lands at byte 36.
    d.extend_from_slice(&[0xABu8; 8]);
    d
}

#[test]
fn bhk_ragdoll_template_data_stub_consumes_block_via_block_size() {
    let header = fo3_header();
    let bytes = build_ragdoll_template_data_bytes();
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "bhkRagdollTemplateData",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("parse");
    assert!(
        block.as_any().downcast_ref::<NiUnknown>().is_none(),
        "bhkRagdollTemplateData must dispatch to BhkRagdollTemplateData, not NiUnknown"
    );
    let td = block
        .as_any()
        .downcast_ref::<BhkRagdollTemplateData>()
        .expect("should be BhkRagdollTemplateData");
    assert_eq!(td.mass, 9.0);
    assert_eq!(td.restitution, 0.8);
    assert_eq!(td.friction, 0.3);
    assert_eq!(td.radius, 1.0);
    assert_eq!(td.material, 7);
    assert_eq!(td.num_constraints, 2);
    // Stub must consume the full block — drift here is the #980
    // regression where the constraint-array tail was left on the
    // stream for the next block to mis-parse.
    assert_eq!(stream.position() as usize, bytes.len());
}
