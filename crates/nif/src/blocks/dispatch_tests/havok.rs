//! Havok dispatch tests.
//!
//! All `bhk*` blocks: shape phantoms, actions, P-collision objects, convex
//! list, breakable / orient-hinged constraints, multi-sphere shape, FO4
//! NPCollisionObject family, byte-array physics/ragdoll.

use super::{fo4_header, oblivion_header};
use crate::blocks::*;
use crate::stream::NifStream;

/// Regression: #474 — `bhkSimpleShapePhantom` carries an 8-byte
/// `Unused 01` field between the bhkWorldObjectCInfo block and the
/// Matrix44 transform (nif.xml line 2793). Pre-#474 the parser
/// skipped straight from CInfo to the 4x4 transform, reading only
/// 92 of 100 declared bytes and leaving `block_sizes` recovery to
/// paper over the gap.
#[test]
fn bhk_simple_shape_phantom_consumes_full_100_bytes() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // bhkWorldObject: shape ref + havok filter + 20-byte CInfo.
    bytes.extend_from_slice(&5i32.to_le_bytes()); // shape_ref
    bytes.extend_from_slice(&0x12345678u32.to_le_bytes()); // havok_filter
    bytes.extend_from_slice(&[0u8; 20]); // bhkWorldObjectCInfo
                                         // bhkSimpleShapePhantom: 8-byte Unused 01 + 64-byte Matrix44.
    bytes.extend_from_slice(&[0u8; 8]); // Unused 01
    for i in 0..16 {
        bytes.extend_from_slice(&(i as f32).to_le_bytes());
    }
    assert_eq!(
        bytes.len(),
        100,
        "test fixture must be 100 bytes per nif.xml"
    );
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "bhkSimpleShapePhantom",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("bhkSimpleShapePhantom must parse without block_sizes recovery");
    let prop = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkSimpleShapePhantom>()
        .expect("bhkSimpleShapePhantom must downcast");
    assert_eq!(prop.shape_ref.index(), Some(5));
    assert_eq!(prop.havok_filter, 0x12345678);
    // Transform column 0 should be [0.0, 1.0, 2.0, 3.0] per the fixture.
    assert_eq!(prop.transform[0], [0.0, 1.0, 2.0, 3.0]);
    assert_eq!(prop.transform[3], [12.0, 13.0, 14.0, 15.0]);
}

// ── #557 / NIF-12 Havok tail type round-trips ───────────────────

/// Regression for #557 — `bhkAabbPhantom` must consume its full
/// 68-byte body (28 B bhkWorldObject prefix + 8 B unused + 32 B
/// hkAabb) and surface shape ref + filter + AABB corners.
#[test]
fn bhk_aabb_phantom_consumes_full_68_bytes() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // bhkWorldObject prefix (28 B).
    bytes.extend_from_slice(&7i32.to_le_bytes()); // shape_ref
    bytes.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // havok_filter
    bytes.extend_from_slice(&[0u8; 20]); // bhkWorldObjectCInfo
                                         // Unused 01 (8 B).
    bytes.extend_from_slice(&[0u8; 8]);
    // hkAabb: min (x=1, y=2, z=3, w=0) + max (x=10, y=20, z=30, w=0).
    for v in [1.0f32, 2.0, 3.0, 0.0, 10.0, 20.0, 30.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    assert_eq!(bytes.len(), 68, "fixture must be 68 bytes per nif.xml");
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkAabbPhantom", &mut stream, Some(bytes.len() as u32))
        .expect("bhkAabbPhantom must parse on Oblivion");
    let phantom = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkAabbPhantom>()
        .expect("dispatch must land on BhkAabbPhantom");
    assert_eq!(phantom.shape_ref.index(), Some(7));
    assert_eq!(phantom.havok_filter, 0xDEAD_BEEF);
    assert_eq!(phantom.aabb_min, [1.0, 2.0, 3.0, 0.0]);
    assert_eq!(phantom.aabb_max, [10.0, 20.0, 30.0, 0.0]);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #557 — `bhkLiquidAction` must consume its
/// 28-byte body (12 B unused + 4 × f32 tuning).
#[test]
fn bhk_liquid_action_consumes_full_28_bytes() {
    // FO3+ only, but oblivion_header works for the parse flow
    // since the parser doesn't gate on version. Matches the
    // corpus where FO3/FNV ship these blocks.
    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0u8; 12]); // Unused 01
    bytes.extend_from_slice(&25.0f32.to_le_bytes()); // initial stick force
    bytes.extend_from_slice(&100.0f32.to_le_bytes()); // stick strength
    bytes.extend_from_slice(&128.0f32.to_le_bytes()); // neighbor distance
    bytes.extend_from_slice(&500.0f32.to_le_bytes()); // neighbor strength
    assert_eq!(bytes.len(), 28);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkLiquidAction", &mut stream, Some(bytes.len() as u32))
        .expect("bhkLiquidAction dispatch must parse");
    let action = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkLiquidAction>()
        .unwrap();
    assert_eq!(action.initial_stick_force, 25.0);
    assert_eq!(action.stick_strength, 100.0);
    assert_eq!(action.neighbor_distance, 128.0);
    assert_eq!(action.neighbor_strength, 500.0);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #557 — `bhkPCollisionObject` wire layout is
/// byte-identical to `bhkCollisionObject` (target + u16 flags +
/// body ref = 10 B) but must surface as its own type so consumers
/// can tell it wraps a phantom, not a rigid body.
#[test]
fn bhk_p_collision_object_consumes_full_10_bytes() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&9i32.to_le_bytes()); // target_ref
    bytes.extend_from_slice(&0x0081u16.to_le_bytes()); // flags (SYNC_ON_UPDATE + SET_LOCAL)
    bytes.extend_from_slice(&3i32.to_le_bytes()); // body_ref (bhkAabbPhantom)
    assert_eq!(bytes.len(), 10);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkPCollisionObject", &mut stream, Some(bytes.len() as u32))
        .expect("bhkPCollisionObject must parse");
    let pco = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkPCollisionObject>()
        .expect("dispatch must land on BhkPCollisionObject, not the sibling bhkCollisionObject");
    assert_eq!(pco.target_ref.index(), Some(9));
    assert_eq!(pco.flags, 0x0081);
    assert_eq!(pco.body_ref.index(), Some(3));
    assert_eq!(pco.block_type_name(), "bhkPCollisionObject");
}

/// Regression for #557 — `bhkConvexListShape` (FO3 only) with a
/// two-sub-shape body. Total size = 37 + 4*N = 45 bytes for N=2.
#[test]
fn bhk_convex_list_shape_consumes_variable_body() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&2u32.to_le_bytes()); // num_sub_shapes
    bytes.extend_from_slice(&11i32.to_le_bytes()); // sub_shape[0]
    bytes.extend_from_slice(&22i32.to_le_bytes()); // sub_shape[1]
    bytes.extend_from_slice(&7u32.to_le_bytes()); // material (FO3 = no Unknown Int prefix)
    bytes.extend_from_slice(&0.5f32.to_le_bytes()); // radius
    bytes.extend_from_slice(&0u32.to_le_bytes()); // Unknown Int 1
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // Unknown Float 1
    bytes.extend_from_slice(&[0u8; 12]); // bhkWorldObjCInfoProperty
    bytes.push(1u8); // use_cached_aabb = true
    bytes.extend_from_slice(&42.0f32.to_le_bytes()); // closest_point_min_distance
    assert_eq!(bytes.len(), 37 + 4 * 2);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkConvexListShape", &mut stream, Some(bytes.len() as u32))
        .expect("bhkConvexListShape dispatch must parse");
    let shape = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkConvexListShape>()
        .unwrap();
    assert_eq!(shape.sub_shapes.len(), 2);
    assert_eq!(shape.sub_shapes[0].index(), Some(11));
    assert_eq!(shape.sub_shapes[1].index(), Some(22));
    assert_eq!(shape.material, 7);
    assert_eq!(shape.radius, 0.5);
    assert!(shape.use_cached_aabb);
    assert_eq!(shape.closest_point_min_distance, 42.0);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #557 — `bhkBreakableConstraint` with a Hinge
/// inner (type=1, 80 B payload). Oblivion-sized so no block_sizes
/// recovery is needed. Total = 16 (outer CInfo) + 4 (wrapped type)
/// + 16 (inner CInfo) + 80 (Hinge payload) + 4 (threshold) + 1
/// (remove_when_broken) = 121 bytes.
#[test]
fn bhk_breakable_constraint_hinge_inner_consumes_121_bytes() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // Outer bhkConstraintCInfo
    bytes.extend_from_slice(&2u32.to_le_bytes()); // num_entities
    bytes.extend_from_slice(&5i32.to_le_bytes()); // entity_a
    bytes.extend_from_slice(&6i32.to_le_bytes()); // entity_b
    bytes.extend_from_slice(&1u32.to_le_bytes()); // priority
                                                  // Wrapped type = Hinge.
    bytes.extend_from_slice(&1u32.to_le_bytes());
    // Inner bhkConstraintCInfo (16 B) — unused in this parse.
    bytes.extend_from_slice(&[0u8; 16]);
    // Hinge payload (80 B).
    bytes.extend_from_slice(&[0u8; 80]);
    // Threshold + Remove When Broken.
    bytes.extend_from_slice(&256.0f32.to_le_bytes());
    bytes.push(1u8);
    assert_eq!(bytes.len(), 121);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "bhkBreakableConstraint",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("bhkBreakableConstraint must parse");
    let bc = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkBreakableConstraint>()
        .unwrap();
    assert_eq!(bc.entity_a.index(), Some(5));
    assert_eq!(bc.entity_b.index(), Some(6));
    assert_eq!(bc.priority, 1);
    assert_eq!(bc.wrapped_type, 1);
    assert_eq!(bc.threshold, 256.0);
    assert!(bc.remove_when_broken);
    assert_eq!(stream.position() as usize, bytes.len());
}

// ── #394 / OBL-D5-H2 Oblivion-skippable block parsers ──────────

/// Regression for #394 — `bhkMultiSphereShape` with 2 spheres
/// must consume its full 20 + 16*2 = 52-byte body on Oblivion
/// (no block_sizes table to fall back on). Validates material +
/// shape_property + per-sphere (center, radius).
#[test]
fn bhk_multi_sphere_shape_consumes_full_52_bytes_for_2_spheres() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&7u32.to_le_bytes()); // material
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shape_property[0]
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shape_property[1]
    bytes.extend_from_slice(&0u32.to_le_bytes()); // shape_property[2]
    bytes.extend_from_slice(&2u32.to_le_bytes()); // num_spheres
    for v in [1.0f32, 2.0, 3.0, 0.5, 10.0, 20.0, 30.0, 2.5] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    assert_eq!(bytes.len(), 52);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkMultiSphereShape", &mut stream, Some(bytes.len() as u32))
        .expect("bhkMultiSphereShape must parse on Oblivion");
    let sphere = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkMultiSphereShape>()
        .unwrap();
    assert_eq!(sphere.material, 7);
    assert_eq!(sphere.spheres.len(), 2);
    assert_eq!(sphere.spheres[0], [1.0, 2.0, 3.0, 0.5]);
    assert_eq!(sphere.spheres[1], [10.0, 20.0, 30.0, 2.5]);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #557 — `bhkOrientHingedBodyAction` must consume
/// its full 68-byte body (12 B bhkUnaryAction + 8 + 16 + 16 + 4 +
/// 4 + 8 = 56 B self).
#[test]
fn bhk_orient_hinged_body_action_consumes_full_68_bytes() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // bhkUnaryAction: Entity Ptr + Unused 01[8].
    bytes.extend_from_slice(&4i32.to_le_bytes()); // entity_ref
    bytes.extend_from_slice(&[0u8; 8]); // Unused 01
                                        // Self body: Unused 02[8] + Hinge Axis LS + Forward LS + S + D + Unused 03[8].
    bytes.extend_from_slice(&[0u8; 8]); // Unused 02
    for v in [1.0f32, 0.0, 0.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    for v in [0.0f32, 1.0, 0.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // strength
    bytes.extend_from_slice(&0.1f32.to_le_bytes()); // damping
    bytes.extend_from_slice(&[0u8; 8]); // Unused 03
    assert_eq!(bytes.len(), 68);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "bhkOrientHingedBodyAction",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("bhkOrientHingedBodyAction must parse");
    let action = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkOrientHingedBodyAction>()
        .unwrap();
    assert_eq!(action.entity_ref.index(), Some(4));
    assert_eq!(action.hinge_axis_ls, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(action.forward_ls, [0.0, 1.0, 0.0, 0.0]);
    assert_eq!(action.strength, 1.0);
    assert_eq!(action.damping, 0.1);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression test for issue #125: `NiCollisionObject` (the non-Havok
/// base class) must dispatch to its own parser so Oblivion NIFs that
/// reference it directly don't cascade-fail on the unknown-block
/// fallback. The block is trivially small — a single target ref —
/// and we only need to prove the parser consumes exactly 4 bytes and
/// downcasts cleanly.
#[test]
fn oblivion_ni_collision_object_base_dispatches() {
    use crate::blocks::collision::NiCollisionObjectBase;

    let header = oblivion_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&42i32.to_le_bytes()); // target ref (i32 -> BlockRef(42))

    let expected_len = bytes.len();
    let mut stream = NifStream::new(&bytes, &header);
    // Pass block_size=None to mimic Oblivion where the header has
    // no block_sizes table. Before the fix this arm returned Err.
    let block = parse_block("NiCollisionObject", &mut stream, None)
        .expect("NiCollisionObject must dispatch without block_size on Oblivion");
    let co = block
        .as_any()
        .downcast_ref::<NiCollisionObjectBase>()
        .expect("downcast to NiCollisionObjectBase");
    assert_eq!(co.target_ref.index(), Some(42));
    assert_eq!(stream.position() as usize, expected_len);
}

/// Regression test for issue #117: the 7 Havok constraint types must
/// dispatch to byte-exact parsers on Oblivion so a constraint block
/// on an Oblivion .nif no longer cascade-fails the parse loop.
/// Builds a 16-byte `bhkConstraintCInfo` base + a zero-filled
/// type-specific payload for each constraint type and asserts the
/// parser consumes exactly the expected number of bytes.
#[test]
fn oblivion_havok_constraints_dispatch_byte_exact() {
    use crate::blocks::collision::BhkConstraint;

    let header = oblivion_header();

    /// Construct a valid bhkConstraintCInfo base (16 bytes) with
    /// known entity refs and a non-trivial priority.
    fn base_bytes() -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&2u32.to_le_bytes()); // num_entities
        d.extend_from_slice(&7i32.to_le_bytes()); // entity_a
        d.extend_from_slice(&11i32.to_le_bytes()); // entity_b
        d.extend_from_slice(&1u32.to_le_bytes()); // priority
        d
    }

    // (type_name, payload_size_after_base) — Oblivion sizes per
    // nif.xml with #NI_BS_LTE_16# active. Total = 16 + payload.
    let cases: [(&'static str, usize); 6] = [
        ("bhkBallAndSocketConstraint", 32),
        ("bhkHingeConstraint", 80),
        ("bhkRagdollConstraint", 120),
        ("bhkLimitedHingeConstraint", 124),
        ("bhkPrismaticConstraint", 140),
        ("bhkStiffSpringConstraint", 36),
    ];

    for (type_name, payload) in cases {
        let mut bytes = base_bytes();
        bytes.resize(bytes.len() + payload, 0u8);
        let expected_len = bytes.len();

        let mut stream = NifStream::new(&bytes, &header);
        let block = parse_block(type_name, &mut stream, None)
            .unwrap_or_else(|e| panic!("{type_name} dispatch failed: {e}"));
        let c = block
            .as_any()
            .downcast_ref::<BhkConstraint>()
            .unwrap_or_else(|| panic!("{type_name} didn't downcast to BhkConstraint"));
        assert_eq!(c.type_name, type_name);
        assert_eq!(c.entity_a.index(), Some(7));
        assert_eq!(c.entity_b.index(), Some(11));
        assert_eq!(c.priority, 1);
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "{type_name} consumed {} bytes, expected {}",
            stream.position(),
            expected_len,
        );
    }

    // Malleable constraint — runtime dispatch on the wrapped type.
    // Layout on Oblivion: base(16) + wrapped_type u32(4) + nested
    // bhkConstraintCInfo(16) + inner CInfo(N) + tau+damping(8).
    // Total = 44 + inner. Wrapped type 2 is LimitedHinge (inner=124).
    let mut mbytes = base_bytes();
    mbytes.extend_from_slice(&2u32.to_le_bytes()); // wrapped type = LimitedHinge
    mbytes.extend_from_slice(&2u32.to_le_bytes()); // nested num_entities
    mbytes.extend_from_slice(&3i32.to_le_bytes()); // nested entity_a
    mbytes.extend_from_slice(&4i32.to_le_bytes()); // nested entity_b
    mbytes.extend_from_slice(&0u32.to_le_bytes()); // nested priority
    mbytes.resize(mbytes.len() + 124, 0u8); // inner LimitedHinge CInfo
    mbytes.resize(mbytes.len() + 8, 0u8); // tau + damping
    let expected_len = mbytes.len();

    let mut stream = NifStream::new(&mbytes, &header);
    let block = parse_block("bhkMalleableConstraint", &mut stream, None)
        .expect("bhkMalleableConstraint dispatch failed");
    let c = block
        .as_any()
        .downcast_ref::<BhkConstraint>()
        .expect("malleable didn't downcast to BhkConstraint");
    assert_eq!(c.type_name, "bhkMalleableConstraint");
    assert_eq!(stream.position() as usize, expected_len);
}

#[test]
fn fo4_bhk_np_collision_object_dispatches_and_consumes() {
    let header = fo4_header();
    // NiCollisionObject::target_ref (i32) + flags (u16) + data_ref (i32) + body_id (u32).
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0x01020304i32.to_le_bytes()); // target_ref
    bytes.extend_from_slice(&0x0080u16.to_le_bytes()); // flags (default 0x80)
    bytes.extend_from_slice(&0x00000005i32.to_le_bytes()); // data_ref = 5
    bytes.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // body_id
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "bhkNPCollisionObject",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("bhkNPCollisionObject should dispatch through a real parser");
    let obj = block
        .as_any()
        .downcast_ref::<collision::BhkNPCollisionObject>()
        .expect("bhkNPCollisionObject did not downcast");
    assert_eq!(obj.flags, 0x0080);
    assert_eq!(obj.body_id, 0xDEADBEEF);
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "bhkNPCollisionObject must consume the stream exactly"
    );
}

#[test]
fn fo4_bhk_physics_system_keeps_byte_array_verbatim() {
    let header = fo4_header();
    // ByteArray: u32 size + raw bytes.
    let payload: &[u8] = b"PHYSICS-BLOB-123";
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkPhysicsSystem", &mut stream, Some(bytes.len() as u32))
        .expect("bhkPhysicsSystem dispatch");
    let sys = block
        .as_any()
        .downcast_ref::<collision::BhkSystemBinary>()
        .expect("bhkPhysicsSystem downcast");
    assert_eq!(sys.type_name, "bhkPhysicsSystem");
    assert_eq!(sys.data.as_slice(), payload);
    assert_eq!(stream.position() as usize, bytes.len());
}

#[test]
fn fo4_bhk_ragdoll_system_keeps_byte_array_verbatim() {
    let header = fo4_header();
    let payload: &[u8] = b"RAGDOLL";
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkRagdollSystem", &mut stream, Some(bytes.len() as u32))
        .expect("bhkRagdollSystem dispatch");
    let sys = block
        .as_any()
        .downcast_ref::<collision::BhkSystemBinary>()
        .expect("bhkRagdollSystem downcast");
    assert_eq!(sys.type_name, "bhkRagdollSystem");
    assert_eq!(sys.data.as_slice(), payload);
    assert_eq!(stream.position() as usize, bytes.len());
}

// ── #708 / NIF-D5-01 + NIF-D5-02 + NIF-D5-08 — Starfield BSGeometry triple ──

// ── #1329 — Oblivion v10.0.1.0 ("old Oblivion") Havok cascade ───────
//
// These rare early-Oblivion meshes (handscythe01 / oar01 /
// ungrdltraphingedoor) are sizeless (no block_sizes) and differ from
// the v20.0.0.5 Oblivion layout in three ways verified byte-exact
// against the vanilla files and openmw `physics.cpp`:
//   1. bhk*/hk* blocks do NOT carry the v10.0.x NiObject `groupID`
//      (parse_block must not consume it for Havok serializables).
//   2. bhkConvexSweepShape / bhkMeshShape need dedicated parsers.
//   3. bhkRigidBody drops the `since=10.1.0.0` duplicated-CInfo prefix
//      and the max-velocity / penetration triple.

/// v10.0.1.0 header (bsver=0). Distinct from `oblivion_header`
/// (v20.0.0.5) — the groupID gate and the rigid-body layout both key
/// off the file version being `< 10.1.0.0`.
fn oblivion_old_header() -> crate::header::NifHeader {
    crate::header::NifHeader {
        version: crate::version::NifVersion::V10_0_1_0,
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
    }
}

/// #1329 root-cause guard: at v10.0.1.0 a `bhk*` block must NOT have the
/// 4-byte NiObject `groupID` consumed by `parse_block_inner`. The fixture
/// places bhkBoxShape data at byte 0 (no groupID prefix); if the gate
/// regresses and the groupID is consumed, material/radius shift into
/// garbage instead of the sane 5 / 0.1.
#[test]
fn oblivion_old_bhk_box_shape_skips_groupid() {
    let header = oblivion_old_header();
    let mut bytes = Vec::new();
    // HavokMaterial (v10.0.x): Unknown Int (4) + Material (4).
    bytes.extend_from_slice(&0u32.to_le_bytes()); // unknown int
    bytes.extend_from_slice(&5u32.to_le_bytes()); // material = 5
    bytes.extend_from_slice(&0.1f32.to_le_bytes()); // radius
    bytes.extend_from_slice(&[0u8; 8]); // unused
    for v in [4.0f32, 8.0, 12.0, 0.0] {
        bytes.extend_from_slice(&v.to_le_bytes()); // extents x/y/z + pad
    }
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkBoxShape", &mut stream, None)
        .expect("bhkBoxShape must parse at v10.0.1.0 without a groupID");
    let b = block
        .as_any()
        .downcast_ref::<collision::BhkBoxShape>()
        .expect("downcast bhkBoxShape");
    assert_eq!(b.material, 5, "groupID wrongly consumed → material shifted");
    assert_eq!(b.radius, 0.1);
    assert_eq!(b.dimensions, [4.0, 8.0, 12.0]);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// bhkConvexSweepShape (nif.xml 3117) — Shape ref + HavokMaterial +
/// Radius + Unknown(Vector3). 28 bytes at v10.0.1.0, no groupID.
#[test]
fn oblivion_old_bhk_convex_sweep_shape() {
    let header = oblivion_old_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&2i32.to_le_bytes()); // Shape ref → block 2
    bytes.extend_from_slice(&0u32.to_le_bytes()); // HavokMaterial unknown int
    bytes.extend_from_slice(&5u32.to_le_bytes()); // HavokMaterial material
    bytes.extend_from_slice(&0.1f32.to_le_bytes()); // Radius
    bytes.extend_from_slice(&[0u8; 12]); // Unknown Vector3
    assert_eq!(bytes.len(), 28);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkConvexSweepShape", &mut stream, None)
        .expect("bhkConvexSweepShape must parse");
    let s = block
        .as_any()
        .downcast_ref::<collision::BhkConvexSweepShape>()
        .expect("downcast bhkConvexSweepShape");
    assert_eq!(s.shape_ref.index(), Some(2));
    assert_eq!(s.material, 5);
    assert_eq!(s.radius, 0.1);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// bhkMeshShape (nif.xml 3179) — skip8 + Radius + skip8 + Scale(Vec4) +
/// ShapeProperties(count + N×12) + skip12 + StripsData(count + N×Ref).
#[test]
fn oblivion_old_bhk_mesh_shape() {
    let header = oblivion_old_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0u8; 8]); // Unknown 01 (2 × u32)
    bytes.extend_from_slice(&0.25f32.to_le_bytes()); // Radius
    bytes.extend_from_slice(&[0u8; 8]); // Unknown 02 (2 × u32)
    for v in [1.0f32, 2.0, 3.0, 4.0] {
        bytes.extend_from_slice(&v.to_le_bytes()); // Scale Vec4
    }
    bytes.extend_from_slice(&1u32.to_le_bytes()); // Num Shape Properties = 1
    bytes.extend_from_slice(&[0u8; 12]); // one bhkWorldObjCInfoProperty
    bytes.extend_from_slice(&[0u8; 12]); // Unknown 03 (3 × u32)
    bytes.extend_from_slice(&1u32.to_le_bytes()); // Num Strips Data = 1
    bytes.extend_from_slice(&2i32.to_le_bytes()); // Strips Data ref → block 2
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkMeshShape", &mut stream, None).expect("bhkMeshShape must parse");
    let m = block
        .as_any()
        .downcast_ref::<collision::BhkMeshShape>()
        .expect("downcast bhkMeshShape");
    assert_eq!(m.radius, 0.25);
    assert_eq!(m.scale, [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(m.data_refs.len(), 1);
    assert_eq!(m.data_refs[0].index(), Some(2));
    assert_eq!(stream.position() as usize, bytes.len());
}

/// bhkRigidBody v10.0.1.0 dedicated path: no `since=10.1.0.0` duplicated
/// CInfo prefix, no max-velocity/penetration triple, plus the
/// bhkWorldObject 4-byte Unknown after the shape ref.
#[test]
fn oblivion_old_bhk_rigid_body() {
    let header = oblivion_old_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&3i32.to_le_bytes()); // shape_ref → block 3
    bytes.extend_from_slice(&[0u8; 4]); // bhkWorldObject Unknown (VER_OB_OLD)
    bytes.extend_from_slice(&0xABCDu32.to_le_bytes()); // havok_filter
    bytes.extend_from_slice(&[0u8; 20]); // bhkWorldObjCInfo
    bytes.extend_from_slice(&[0u8; 4]); // bethVer<83 Unused
    for _ in 0..(4 + 4 + 4 + 4) {
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // translation/rotation/lin/ang vel (4×Vec4)
    }
    for _ in 0..12 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // inertia 3×4
    }
    for _ in 0..4 {
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // center Vec4
    }
    bytes.extend_from_slice(&7.5f32.to_le_bytes()); // mass
    bytes.extend_from_slice(&0.1f32.to_le_bytes()); // linear_damping
    bytes.extend_from_slice(&0.05f32.to_le_bytes()); // angular_damping
    bytes.extend_from_slice(&0.3f32.to_le_bytes()); // friction
    bytes.extend_from_slice(&0.4f32.to_le_bytes()); // restitution
                                                    // NO max velocities / penetration depth at v10.0.x.
    bytes.push(1); // motion_type
    bytes.push(2); // deactivator_type
    bytes.push(3); // solver_deactivation
    bytes.push(4); // quality_type
    bytes.extend_from_slice(&[0u8; 12]); // Unused (bethVer<83)
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num_constraints = 0
    bytes.extend_from_slice(&0u32.to_le_bytes()); // body_flags (u32 pre-Skyrim)
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkRigidBody", &mut stream, None)
        .expect("bhkRigidBody must parse on the v10.0.1.0 path");
    let rb = block
        .as_any()
        .downcast_ref::<collision::BhkRigidBody>()
        .expect("downcast bhkRigidBody");
    assert_eq!(rb.shape_ref.index(), Some(3));
    assert_eq!(rb.havok_filter, 0xABCD);
    assert_eq!(rb.mass, 7.5);
    assert_eq!(rb.friction, 0.3);
    assert_eq!(rb.restitution, 0.4);
    assert_eq!(rb.motion_type, 1);
    assert_eq!(rb.quality_type, 4);
    // v10.0.x marker: max-velocity / penetration fields are absent → 0.
    assert_eq!(rb.max_linear_velocity, 0.0);
    assert_eq!(rb.penetration_depth, 0.0);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// #1337 — `BSKeyframeController` on old Oblivion (v10.0.1.x). It is
/// NiObject-derived (NOT a Havok serializable), so `parse_block` consumes
/// the groupID; the body is NiTimeController base + (no interpolator
/// below 10.1.0.104) + `Data` ref (until 10.1.0.103) + `Data 2` ref.
/// Pre-#1337 it fell through to the base-only stub and truncated the
/// per-bone controller chain of old-Oblivion creatures (minotaurold.nif).
#[test]
fn oblivion_old_bs_keyframe_controller() {
    let header = oblivion_old_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u32.to_le_bytes()); // groupID (consumed by parse_block)
                                                  // NiTimeController base (26 B).
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    bytes.extend_from_slice(&8u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
    bytes.extend_from_slice(&1.33f32.to_le_bytes()); // stop_time
    bytes.extend_from_slice(&3i32.to_le_bytes()); // target
                                                  // No interpolator (< 10.1.0.104).
    bytes.extend_from_slice(&7i32.to_le_bytes()); // Data ref (until 10.1.0.103)
    bytes.extend_from_slice(&9i32.to_le_bytes()); // Data 2 ref (always)
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSKeyframeController", &mut stream, None)
        .expect("BSKeyframeController must parse on the v10.0.1.x path");
    let c = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::BsKeyframeController>()
        .expect("downcast BsKeyframeController");
    assert_eq!(c.base.base.target_ref.index(), Some(3));
    assert!(
        c.base.interpolator_ref.is_null(),
        "no interpolator ref below 10.1.0.104"
    );
    assert_eq!(c.data_ref.index(), Some(7));
    assert_eq!(c.data2_ref.index(), Some(9));
    // groupID(4) + base(26) + data(4) + data2(4) = 38.
    assert_eq!(stream.position() as usize, bytes.len());
    assert_eq!(bytes.len(), 38);
}

/// #1334 — `bhkPlaneShape` (bhkHeightFieldShape-derived) had no dispatch arm,
/// so the one vanilla SSE instance (slaughterfish egg-cluster ground plane)
/// hit the NiUnknown fallback. Layout (nif.xml): material(4) + 12 unused +
/// Plane Normal(Vector3=12) + Plane Constant(f32=4) + AABB Half Extents
/// (Vector4=16) + AABB Center(Vector4=16) = 64 bytes. Pins byte-exact
/// consumption + field decode.
#[test]
fn bhk_plane_shape_consumes_full_64_bytes() {
    let header = fo4_header(); // any version > 10.0.1.2 → 4-byte HavokMaterial
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&7u32.to_le_bytes()); // material
    bytes.extend_from_slice(&[0u8; 12]); // Unused 01
    for c in [0.0f32, 0.0, 1.0] {
        bytes.extend_from_slice(&c.to_le_bytes()); // Plane Normal (+Z)
    }
    bytes.extend_from_slice(&128.5f32.to_le_bytes()); // Plane Constant
    for c in [10.0f32, 20.0, 30.0, 0.0] {
        bytes.extend_from_slice(&c.to_le_bytes()); // AABB Half Extents
    }
    for c in [1.0f32, 2.0, 3.0, 0.0] {
        bytes.extend_from_slice(&c.to_le_bytes()); // AABB Center
    }
    assert_eq!(bytes.len(), 64, "fixture must be 64 bytes per nif.xml");

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("bhkPlaneShape", &mut stream, Some(bytes.len() as u32))
        .expect("bhkPlaneShape must parse without NiUnknown fallback");
    assert_eq!(
        stream.position() as usize,
        64,
        "must consume the full block, not fall through to NiUnknown",
    );
    let prop = block
        .as_any()
        .downcast_ref::<crate::blocks::collision::BhkPlaneShape>()
        .expect("must downcast to BhkPlaneShape");
    assert_eq!(prop.material, 7);
    assert_eq!(prop.plane_normal, [0.0, 0.0, 1.0]);
    assert_eq!(prop.plane_constant, 128.5);
    assert_eq!(prop.aabb_half_extents, [10.0, 20.0, 30.0, 0.0]);
    assert_eq!(prop.aabb_center, [1.0, 2.0, 3.0, 0.0]);
}
