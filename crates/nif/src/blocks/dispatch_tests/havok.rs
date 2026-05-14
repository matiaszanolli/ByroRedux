//! Havok dispatch tests.
//!
//! All `bhk*` blocks: shape phantoms, actions, P-collision objects, convex
//! list, breakable / orient-hinged constraints, multi-sphere shape, FO4
//! NPCollisionObject family, byte-array physics/ragdoll.

use super::{fo4_header, oblivion_header};
use crate::blocks::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;
use std::sync::Arc;

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

/// Starfield header (bsver=172, uv=12). Per
/// `crates/nif/src/version.rs::NifVariant::detect`. Skyrim+ string-table
/// shape — `read_string` resolves `0` to `strings[0]`.
fn starfield_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 172,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("BSGeometry_Test")],
        max_string_length: 16,
        num_groups: 0,
    }
}

/// Build the NiAVObject (no-properties) prefix every BSGeometry shares.
/// `flags` lands on the parent NiAVObject; bit 0x200 is the
/// internal-geom-data gate.
fn starfield_av_prefix(flags: u32) -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET — name index, extra-data ref count, controller ref.
    d.extend_from_slice(&0i32.to_le_bytes()); // name = strings[0]
    d.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                 // NiAVObject (parse_no_properties): flags(u32) + transform + collision_ref
    d.extend_from_slice(&flags.to_le_bytes());
    // NiTransform: rotation 3×3 matrix (9×f32) + translation (3×f32) + scale (f32)
    for v in [
        1.0f32, 0.0, 0.0, // row 0
        0.0, 1.0, 0.0, // row 1
        0.0, 0.0, 1.0, // row 2
        0.0, 0.0, 0.0, // translation
        1.0, // scale
    ] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
    d
}

/// Append the BSGeometry trailer (bounds + boundMinMax + 3 refs) and
/// `mesh_count` external-mesh slots (each: 3×u32 + sized-string).
fn starfield_external_geometry_bytes(flags: u32, mesh_names: &[&str]) -> Vec<u8> {
    assert!(mesh_names.len() <= 4);
    let mut d = starfield_av_prefix(flags);
    // bounds: Vector3 center + f32 radius
    for v in [0.0f32, 0.0, 0.0, 1.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    // boundMinMax: 6 × f32
    for v in [-1.0f32, -1.0, -1.0, 1.0, 1.0, 1.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    // 3 refs: skin / shader / alpha
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // 4 mesh slots — `mesh_names.len()` populated, rest absent.
    for i in 0..4 {
        if i < mesh_names.len() {
            d.push(1u8); // present
            d.extend_from_slice(&123u32.to_le_bytes()); // tri_size
            d.extend_from_slice(&456u32.to_le_bytes()); // num_verts
            d.extend_from_slice(&64u32.to_le_bytes()); // flags (nifly: "often 64")
                                                       // sized string: u32 length + bytes
            let name = mesh_names[i].as_bytes();
            d.extend_from_slice(&(name.len() as u32).to_le_bytes());
            d.extend_from_slice(name);
        } else {
            d.push(0u8); // absent
        }
    }
    d
}
