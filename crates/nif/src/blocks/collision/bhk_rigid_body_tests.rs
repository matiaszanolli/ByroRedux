use super::*;
use crate::blocks::parse_block;
use crate::header::NifHeader;
use crate::version::NifVersion;

/// Skyrim SE header (NIF 20.2.0.7, user_version 12, bsver 100).
/// Matches the corpus where all 12,866 bhkRigidBody blocks fell into
/// NiUnknown pre-#546.
fn skyrim_se_header() -> NifHeader {
    NifHeader {
        version: NifVersion(0x14020007),
        little_endian: true,
        user_version: 12,
        user_version_2: 100,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// Synthetic `bhkRigidBody` body with zero constraints. Layout mirrors
/// nif.xml `bhkRigidBodyCInfo2010` (line 2844) exactly so any drift
/// surfaces as an unconsumed-bytes assertion failure.
///
/// Total expected size: 28 (bhkWorldObject) + 4 (bhkEntity) + 20
/// (CInfo2010 prefix) + 128 (transforms) + 44 (dynamics) + 20 (motion
/// + trailer) + 4 (num_constraints) + 2 (body_flags) = 250 bytes.
fn minimal_skyrim_bhk_rigid_body_bytes() -> (Vec<u8>, f32) {
    let mut d = Vec::new();
    // bhkWorldObject: Shape ref + Havok Filter + bhkWorldObjectCInfo(20)
    d.extend_from_slice(&(-1i32).to_le_bytes()); // shape_ref
    d.extend_from_slice(&0u32.to_le_bytes()); // havok filter
    d.extend_from_slice(&[0u8; 4]); // bhkWorldObjectCInfo Unused 01
    d.push(0u8); // broad phase type
    d.extend_from_slice(&[0u8; 3]); // Unused 02
    d.extend_from_slice(&[0u8; 12]); // bhkWorldObjCInfoProperty (3 × u32)

    // bhkEntityCInfo: collision_response(1) + unused(1) + callback_delay(2)
    d.push(0u8);
    d.push(0u8);
    d.extend_from_slice(&0xffff_u16.to_le_bytes());

    // bhkRigidBodyCInfo2010 prefix (20 B) — the bytes this fix adds:
    d.extend_from_slice(&[0u8; 4]); // Unused 01
    d.extend_from_slice(&0u32.to_le_bytes()); // duplicated Havok Filter
    d.extend_from_slice(&[0u8; 4]); // Unused 02
    d.extend_from_slice(&0xdead_beef_u32.to_le_bytes()); // Unknown Int 1 — any value
    d.push(0u8); // Collision Response
    d.push(0u8); // Unused 03
    d.extend_from_slice(&0xffff_u16.to_le_bytes()); // Callback Delay

    // Translation (vec4)
    for v in [1.0f32, 2.0, 3.0, 0.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    // Rotation (quat)
    for v in [0.0f32, 0.0, 0.0, 1.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    // Linear + Angular Velocity (vec4 each)
    for _ in 0..2 {
        for v in [0.0f32; 4] {
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    // Inertia Tensor (hkMatrix3 = 48 B)
    for _ in 0..12 {
        d.extend_from_slice(&0.0f32.to_le_bytes());
    }
    // Center of mass (vec4)
    for v in [0.0f32; 4] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    // Mass, linDamp, angDamp
    let mass = 42.0f32;
    d.extend_from_slice(&mass.to_le_bytes());
    d.extend_from_slice(&0.1f32.to_le_bytes());
    d.extend_from_slice(&0.05f32.to_le_bytes());
    // Time Factor + Gravity Factor (Skyrim-only pair)
    d.extend_from_slice(&1.0f32.to_le_bytes());
    d.extend_from_slice(&1.0f32.to_le_bytes());
    // Friction + Rolling Friction Multiplier (Skyrim-only pair) + Restitution
    d.extend_from_slice(&0.5f32.to_le_bytes());
    d.extend_from_slice(&0.0f32.to_le_bytes());
    d.extend_from_slice(&0.4f32.to_le_bytes());
    // Max Linear, Max Angular, Penetration Depth
    d.extend_from_slice(&104.4f32.to_le_bytes());
    d.extend_from_slice(&31.57f32.to_le_bytes());
    d.extend_from_slice(&0.15f32.to_le_bytes());
    // Motion + Deactivator + Solver + Quality (4 × u8)
    d.push(1u8); // motion = MO_SYS_DYNAMIC
    d.push(3u8); // deactivator — non-zero so we can assert the read
    d.push(0u8); // solver_deactivation
    d.push(1u8); // quality
                 // Trailer: auto_remove(1) + response_mod(1) + num_shape_keys(1)
                 //        + force_collided(1) + Unused 04[12] = 16 bytes
    d.extend_from_slice(&[0u8; 16]);
    // Num Constraints + Body Flags (u16 on Skyrim+)
    d.extend_from_slice(&0u32.to_le_bytes());
    d.extend_from_slice(&0u16.to_le_bytes());
    (d, mass)
}

/// Regression for #546 — Skyrim SE bhkRigidBody must parse
/// end-to-end without leaving bytes on the stream. Pre-fix the
/// parser skipped the 20-byte bhkRigidBodyCInfo2010 prefix,
/// hardcoded `deactivator_type = 0`, and left 12 bytes of trailing
/// Unused 04 on the stream — every SE rigid body degraded to
/// NiUnknown.
#[test]
fn bhk_rigid_body_skyrim_se_consumes_full_cinfo2010_body() {
    let header = skyrim_se_header();
    let (bytes, mass) = minimal_skyrim_bhk_rigid_body_bytes();
    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("bhkRigidBody", &mut stream, Some(bytes.len() as u32))
        .expect("Skyrim SE bhkRigidBody must parse cleanly");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "must consume the whole CInfo2010 body — pre-fix left 33 bytes"
    );
    let body = block
        .as_any()
        .downcast_ref::<BhkRigidBody>()
        .expect("dispatch must yield BhkRigidBody, not NiUnknown");
    // Spot-check a few fields that drift to garbage under the old
    // parser: translation[0] and deactivator_type both land in what
    // pre-fix would have been CInfo2010-prefix bytes.
    assert_eq!(body.translation[0], 1.0);
    assert_eq!(body.translation[1], 2.0);
    assert_eq!(body.translation[2], 3.0);
    assert_eq!(body.mass, mass);
    assert_eq!(
        body.deactivator_type, 3,
        "deactivator_type was the second root cause — pre-fix it \
         was hardcoded to 0 on Skyrim+, overwriting the real value"
    );
}

/// Regression for #546 — `bhkRigidBodyT` shares the exact same
/// wire layout as `bhkRigidBody` (nif.xml line 2942), so the SE fix
/// must carry through to the T-variant too. Audit counted 3,094 T
/// blocks in NiUnknown alongside the 9,772 plain rigid bodies.
#[test]
fn bhk_rigid_body_t_skyrim_se_parses_identically() {
    let header = skyrim_se_header();
    let (bytes, mass) = minimal_skyrim_bhk_rigid_body_bytes();
    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("bhkRigidBodyT", &mut stream, Some(bytes.len() as u32))
        .expect("bhkRigidBodyT must share the SE fix");
    assert_eq!(stream.position() as usize, bytes.len());
    let body = block.as_any().downcast_ref::<BhkRigidBody>().unwrap();
    assert_eq!(body.mass, mass);
    assert_eq!(body.deactivator_type, 3);
}
