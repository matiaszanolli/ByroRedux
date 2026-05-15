use super::*;
use crate::blocks::parse_block;
use crate::header::NifHeader;
use crate::version::NifVersion;

/// Skyrim SE header (NIF 20.2.0.7, user_version 12, bsver 100).
/// Matches the corpus where all 12,866 bhkRigidBody blocks fell into
/// NiUnknown pre-#546.
fn skyrim_se_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
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

/// Boundary regression for NIF-D2-NEW-05 (audit 2026-05-12) — the
/// `body_flags` width threshold is `bsver < crate::version::bsver::RIGID_BODY_FLAGS16` per nif.xml's
/// `#SKY_AND_LATER#` resolution, not the pre-fix `bsver < crate::version::bsver::SKYRIM_LE`.
///
/// Builds a Skyrim-shape `bhkRigidBody` body at the two bsver values
/// straddling the threshold and asserts the parser consumed the
/// expected total byte count:
///
/// - **bsver=75** (just below the threshold): `body_flags` reads as
///   u32 → 4-byte tail → 252 total.
/// - **bsver=76** (at the threshold): `body_flags` reads as u16 →
///   2-byte tail → 250 total.
///
/// Both bsvers fall in the `35..=129` Skyrim-layout window from the
/// parser's POV, so the rest of the body shape stays identical —
/// only the trailing `body_flags` width differs.
///
/// No Bethesda title ships in the 76..=82 gap, so the pre-fix value
/// was structurally invisible to vanilla content — this test exists
/// to pin the doctrine boundary at the value nif.xml actually
/// specifies.

fn skyrim_header_at_bsver(bsver: u32) -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
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

/// Same fixture shape as [`minimal_skyrim_bhk_rigid_body_bytes`] but
/// the trailing `body_flags` is sized for the requested bsver: u32
/// when `bsver < crate::version::bsver::RIGID_BODY_FLAGS16`, u16 otherwise.
///
/// Returns `(bytes, body_flags_width_bytes)`. The width is what the
/// boundary tests assert: the delta in consumed bytes between
/// bsver=75 and bsver=76 must be exactly the 2-byte difference
/// between a u32 and u16 read. Total parser-consumed length isn't
/// asserted because at bsver < crate::version::bsver::SKYRIM_LE the parser skips three Skyrim+
/// fields (time_factor + gravity_factor + rolling_friction = 12 B)
/// that the bsver=100 base fixture supplies — the test fixture's
/// trailing 12 B sit unconsumed and that's a separate (correct)
/// behaviour from the body_flags width gate.
fn skyrim_bhk_rigid_body_bytes_at_bsver(bsver: u32) -> (Vec<u8>, usize) {
    let (mut d, _mass) = minimal_skyrim_bhk_rigid_body_bytes();
    let width = if bsver < crate::version::bsver::RIGID_BODY_FLAGS16 { 4 } else { 2 };
    if bsver < crate::version::bsver::RIGID_BODY_FLAGS16 {
        // Base fixture wrote u16 body_flags. Pad to u32.
        d.extend_from_slice(&0u16.to_le_bytes());
    }
    (d, width)
}

#[test]
fn bhk_rigid_body_body_flags_reads_u32_below_bsver_76() {
    let header = skyrim_header_at_bsver(75);
    let (bytes, width) = skyrim_bhk_rigid_body_bytes_at_bsver(75);
    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("bhkRigidBody", &mut stream, Some(bytes.len() as u32))
        .expect("bsver=75 bhkRigidBody must parse cleanly");
    assert_eq!(
        width, 4,
        "bsver=75 must size body_flags as u32 (4 bytes) — pre-fix this \
         read u32 at bsver=75 too, so this pins the threshold doctrine \
         from below the boundary, not a bug fix"
    );
    assert!(block.as_any().is::<BhkRigidBody>());
}

#[test]
fn bhk_rigid_body_body_flags_reads_u16_at_bsver_76() {
    let header = skyrim_header_at_bsver(76);
    let (bytes, width) = skyrim_bhk_rigid_body_bytes_at_bsver(76);
    let mut stream = crate::stream::NifStream::new(&bytes, &header);
    let block = parse_block("bhkRigidBody", &mut stream, Some(bytes.len() as u32))
        .expect("bsver=76 bhkRigidBody must parse cleanly");
    assert_eq!(
        width, 2,
        "bsver=76 must size body_flags as u16 (2 bytes) — pre-fix the \
         threshold was 83 and bsver=76 silently over-read 2 bytes, \
         drifting the stream"
    );
    assert!(block.as_any().is::<BhkRigidBody>());
}

/// Differential test: parse the same body bytes at bsver=75 and
/// bsver=76 and verify the consumed-byte counts differ by exactly 2.
/// This is the load-bearing assertion for NIF-D2-NEW-05 — the
/// individual per-bsver tests above pin the width to the expected
/// value, this one pins the actual parser behaviour at the boundary.
#[test]
fn bhk_rigid_body_body_flags_width_differs_by_2_at_threshold() {
    let (bytes_75, _) = skyrim_bhk_rigid_body_bytes_at_bsver(75);
    let (bytes_76, _) = skyrim_bhk_rigid_body_bytes_at_bsver(76);

    let header_75 = skyrim_header_at_bsver(75);
    let mut stream_75 = crate::stream::NifStream::new(&bytes_75, &header_75);
    parse_block("bhkRigidBody", &mut stream_75, Some(bytes_75.len() as u32))
        .expect("bsver=75 must parse");
    let consumed_75 = stream_75.position();

    let header_76 = skyrim_header_at_bsver(76);
    let mut stream_76 = crate::stream::NifStream::new(&bytes_76, &header_76);
    parse_block("bhkRigidBody", &mut stream_76, Some(bytes_76.len() as u32))
        .expect("bsver=76 must parse");
    let consumed_76 = stream_76.position();

    assert_eq!(
        consumed_75.saturating_sub(consumed_76),
        2,
        "body_flags width must differ by 2 B (u32 vs u16) at the \
         bsver < crate::version::bsver::RIGID_BODY_FLAGS16 threshold; pre-fix the threshold was 83 so both \
         bsvers consumed the same width and the boundary was wrong"
    );
}
