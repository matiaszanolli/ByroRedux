//! Coverage for the typed `bhkRagdollConstraint` /
//! `bhkLimitedHingeConstraint` CInfo decode that feeds PHYSAL (the
//! physics abstraction layer — see `docs/engine/physal.md`). Pre-M41.x
//! these were name-only stubs (16 bytes read, the rest skipped); now the
//! per-variant body is decoded in the era-correct field order so the same
//! `RagdollCInfo` / `LimitedHingeCInfo` — and therefore one ragdoll path —
//! is reached from every game.
//!
//! Both eras are pinned: Oblivion (`#NI_BS_LTE_16#`: 6 Vec4 ragdoll /
//! 7 Vec4 hinge, no motors, pivots-first order), FO3/FNV
//! (`!#NI_BS_LTE_16#`: 8 Vec4 + trailing motor), and Skyrim (same FO3+
//! layout, gated by NIF version not bsver). Field order + sizes are from
//! nif.xml, cross-checked against the FNV prefix sizes the sibling
//! `BhkBreakableConstraint` already encodes (Ragdoll 152, LimitedHinge
//! 140). The FO3+ typed decode reads exactly the fixed prefix and leaves
//! the trailing motor to `block_size` recovery — so those tests assert the
//! stream advanced by `16 + prefix`, with no motor bytes in the buffer.
use super::*;
use crate::header::NifHeader;
use crate::version::NifVersion;

fn fnv_header() -> NifHeader {
    NifHeader {
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
    }
}

fn oblivion_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_0_0_5,
        little_endian: true,
        user_version: 11,
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

/// Shared `bhkConstraintCInfo` base — 16 bytes:
/// num_entities + entity_a + entity_b + priority.
fn base() -> Vec<u8> {
    let mut d = Vec::with_capacity(16);
    d.extend_from_slice(&2u32.to_le_bytes());
    d.extend_from_slice(&1u32.to_le_bytes()); // entity_a
    d.extend_from_slice(&2u32.to_le_bytes()); // entity_b
    d.extend_from_slice(&3u32.to_le_bytes()); // priority
    d
}

fn vec4(x: f32, y: f32, z: f32, w: f32) -> Vec<u8> {
    let mut d = Vec::with_capacity(16);
    for c in [x, y, z, w] {
        d.extend_from_slice(&c.to_le_bytes());
    }
    d
}

#[test]
fn fnv_ragdoll_decodes_typed_cinfo() {
    let mut bytes = base();
    // 8 × Vec4 in FO3+ order: TwistA, PlaneA, MotorA, PivotA, TwistB,
    // PlaneB, MotorB, PivotB. Encode the first component as the index so
    // each field is individually identifiable.
    bytes.extend(vec4(0.0, 0.0, 1.0, 0.0)); // twist_a
    bytes.extend(vec4(1.0, 0.0, 0.0, 0.0)); // plane_a
    bytes.extend(vec4(2.0, 0.0, 0.0, 0.0)); // motor_a
    bytes.extend(vec4(3.0, 10.0, 20.0, 1.0)); // pivot_a
    bytes.extend(vec4(4.0, 0.0, 1.0, 0.0)); // twist_b
    bytes.extend(vec4(5.0, 0.0, 0.0, 0.0)); // plane_b
    bytes.extend(vec4(6.0, 0.0, 0.0, 0.0)); // motor_b
    bytes.extend(vec4(7.0, -10.0, 5.0, 1.0)); // pivot_b
    // 6 × f32 limits.
    for v in [0.5f32, -0.25, 0.75, -1.5, 1.5, 100.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    assert_eq!(bytes.len(), 16 + 152, "base + 8×Vec4 + 6×f32");

    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    let c = BhkConstraint::parse(&mut stream, "bhkRagdollConstraint").unwrap();

    assert_eq!(c.priority, 3);
    let BhkConstraintData::Ragdoll(r) = c.data else {
        panic!("expected Ragdoll data, got {:?}", c.data);
    };
    assert_eq!(r.twist_a, [0.0, 0.0, 1.0, 0.0]);
    assert_eq!(r.plane_a, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(r.motor_a, [2.0, 0.0, 0.0, 0.0]);
    assert_eq!(r.pivot_a, [3.0, 10.0, 20.0, 1.0]);
    assert_eq!(r.twist_b, [4.0, 0.0, 1.0, 0.0]);
    assert_eq!(r.plane_b, [5.0, 0.0, 0.0, 0.0]);
    assert_eq!(r.motor_b, [6.0, 0.0, 0.0, 0.0]);
    assert_eq!(r.pivot_b, [7.0, -10.0, 5.0, 1.0]);
    assert_eq!(r.cone_max_angle, 0.5);
    assert_eq!(r.plane_min_angle, -0.25);
    assert_eq!(r.plane_max_angle, 0.75);
    assert_eq!(r.twist_min_angle, -1.5);
    assert_eq!(r.twist_max_angle, 1.5);
    assert_eq!(r.max_friction, 100.0);
    // Fixed prefix consumed exactly; the trailing motor is left for
    // block_size recovery.
    assert_eq!(stream.position() as usize, 16 + 152);
}

#[test]
fn fnv_limited_hinge_decodes_typed_cinfo() {
    let mut bytes = base();
    // 8 × Vec4 in FO3+ order: AxisA, PerpA1, PerpA2, PivotA, AxisB,
    // PerpB1, PerpB2, PivotB.
    bytes.extend(vec4(1.0, 0.0, 0.0, 0.0)); // axis_a
    bytes.extend(vec4(0.0, 1.0, 0.0, 0.0)); // perp_axis_in_a1
    bytes.extend(vec4(0.0, 0.0, 1.0, 0.0)); // perp_axis_in_a2
    bytes.extend(vec4(11.0, 22.0, 33.0, 1.0)); // pivot_a
    bytes.extend(vec4(1.0, 0.0, 0.0, 0.0)); // axis_b
    bytes.extend(vec4(0.0, 1.0, 0.0, 0.0)); // perp_axis_in_b1
    bytes.extend(vec4(0.0, 0.0, 1.0, 0.0)); // perp_axis_in_b2
    bytes.extend(vec4(44.0, 55.0, 66.0, 1.0)); // pivot_b
    // 3 × f32: min, max, friction.
    for v in [-1.0f32, 1.0, 10.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    assert_eq!(bytes.len(), 16 + 140, "base + 8×Vec4 + 3×f32");

    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    let c = BhkConstraint::parse(&mut stream, "bhkLimitedHingeConstraint").unwrap();

    let BhkConstraintData::LimitedHinge(h) = c.data else {
        panic!("expected LimitedHinge data, got {:?}", c.data);
    };
    assert_eq!(h.axis_a, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(h.perp_axis_in_a1, [0.0, 1.0, 0.0, 0.0]);
    assert_eq!(h.perp_axis_in_a2, [0.0, 0.0, 1.0, 0.0]);
    assert_eq!(h.pivot_a, [11.0, 22.0, 33.0, 1.0]);
    assert_eq!(h.pivot_b, [44.0, 55.0, 66.0, 1.0]);
    assert_eq!(h.min_angle, -1.0);
    assert_eq!(h.max_angle, 1.0);
    assert_eq!(h.max_friction, 10.0);
    assert_eq!(stream.position() as usize, 16 + 140);
}

/// FNV `bhkMalleableConstraint` wrapping a Ragdoll (Type 7) — the
/// dominant joint form in the vanilla humanoid skeleton (14 of 17
/// joints). The inner CInfo's entities are −1/−1; the real bodies are
/// the outer base. Layout: base(16) + Type(4) + inner CInfo(16) + Ragdoll
/// prefix(152). The trailing Strength + inner motor recover via block_size.
#[test]
fn fnv_malleable_wrapping_ragdoll_surfaces_as_ragdoll() {
    let mut bytes = base(); // outer base: real bodies (entity_a=1, entity_b=2)
    bytes.extend_from_slice(&7u32.to_le_bytes()); // wrapped Type = 7 (Ragdoll)
    // inner bhkConstraintCInfo: num=2, entity_a=-1, entity_b=-1, priority=1
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    // inner Ragdoll CInfo: 8 × Vec4 + 6 × f32.
    for i in 0..8 {
        bytes.extend(vec4(i as f32, 0.0, 0.0, 0.0));
    }
    for v in [0.5f32, -0.25, 0.75, -1.5, 1.5, 100.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }

    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    let c = BhkConstraint::parse(&mut stream, "bhkMalleableConstraint").unwrap();

    // Outer entities are the constrained bodies.
    assert_eq!(c.entity_a.index(), Some(1));
    assert_eq!(c.entity_b.index(), Some(2));
    let BhkConstraintData::Ragdoll(r) = c.data else {
        panic!("malleable-wrapped Ragdoll must surface as Ragdoll, got {:?}", c.data);
    };
    assert_eq!(r.twist_a, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(r.pivot_b, [7.0, 0.0, 0.0, 0.0]);
    assert_eq!(r.max_friction, 100.0);
    // base(16) + Type(4) + inner CInfo(16) + Ragdoll prefix(152) = 188.
    assert_eq!(stream.position() as usize, 16 + 4 + 16 + 152);
}

/// Non-decoded FO3+ types stay name-only stubs (16-byte base read; the
/// rest recovered via block_size) — unchanged from pre-M41.x behaviour.
#[test]
fn fnv_other_constraint_type_stays_stub() {
    let bytes = base();
    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    let c = BhkConstraint::parse(&mut stream, "bhkHingeConstraint").unwrap();
    assert!(matches!(c.data, BhkConstraintData::Other));
    assert_eq!(stream.position() as usize, 16, "only the base is read");
}

/// Oblivion ragdoll decodes its own (`#NI_BS_LTE_16#`) field order:
/// 6 × Vec4 (pivot/plane/twist, A then B — no Motor A/B) + 6 × f32.
/// The absent motors zero out; the common subset the importer reads
/// (twist/plane/pivot + angle limits) must match the FNV decode so the
/// same `RagdollCInfo` funnels every game into one ragdoll path (PHYSAL).
#[test]
fn oblivion_ragdoll_decodes_typed_cinfo() {
    let mut bytes = base();
    // 6 × Vec4 in Oblivion order: PivotA, PlaneA, TwistA, PivotB, PlaneB,
    // TwistB. First component encodes the field index.
    bytes.extend(vec4(0.0, 10.0, 20.0, 1.0)); // pivot_a
    bytes.extend(vec4(1.0, 0.0, 0.0, 0.0)); // plane_a
    bytes.extend(vec4(2.0, 0.0, 1.0, 0.0)); // twist_a
    bytes.extend(vec4(3.0, -10.0, 5.0, 1.0)); // pivot_b
    bytes.extend(vec4(4.0, 0.0, 0.0, 0.0)); // plane_b
    bytes.extend(vec4(5.0, 0.0, 1.0, 0.0)); // twist_b
    // 6 × f32 limits (same shared trailer as FNV).
    for v in [0.5f32, -0.25, 0.75, -1.5, 1.5, 10.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    assert_eq!(bytes.len(), 16 + 120, "base + 6×Vec4 + 6×f32");

    let header = oblivion_header();
    let mut stream = NifStream::new(&bytes, &header);
    let c = BhkConstraint::parse(&mut stream, "bhkRagdollConstraint").unwrap();

    let BhkConstraintData::Ragdoll(r) = c.data else {
        panic!("Oblivion Ragdoll must decode, got {:?}", c.data);
    };
    // Fields the PHYSAL translate boundary actually reads.
    assert_eq!(r.pivot_a, [0.0, 10.0, 20.0, 1.0]);
    assert_eq!(r.plane_a, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(r.twist_a, [2.0, 0.0, 1.0, 0.0]);
    assert_eq!(r.pivot_b, [3.0, -10.0, 5.0, 1.0]);
    assert_eq!(r.plane_b, [4.0, 0.0, 0.0, 0.0]);
    assert_eq!(r.twist_b, [5.0, 0.0, 1.0, 0.0]);
    assert_eq!(r.cone_max_angle, 0.5);
    assert_eq!(r.twist_min_angle, -1.5);
    assert_eq!(r.twist_max_angle, 1.5);
    assert_eq!(r.max_friction, 10.0);
    // FO3-only motors don't exist pre-FO3 → zeroed, invisible downstream.
    assert_eq!(r.motor_a, [0.0; 4]);
    assert_eq!(r.motor_b, [0.0; 4]);
    assert_eq!(stream.position() as usize, 16 + 120);
}

/// Oblivion limited hinge: 7 × Vec4 (pivot/axis/perp, no Perp Axis In B1)
/// + 3 × f32. Absent perp axis zeroes out.
#[test]
fn oblivion_limited_hinge_decodes_typed_cinfo() {
    let mut bytes = base();
    // 7 × Vec4 in Oblivion order: PivotA, AxisA, PerpA1, PerpA2, PivotB,
    // AxisB, PerpB2.
    bytes.extend(vec4(11.0, 22.0, 33.0, 1.0)); // pivot_a
    bytes.extend(vec4(1.0, 0.0, 0.0, 0.0)); // axis_a
    bytes.extend(vec4(0.0, 1.0, 0.0, 0.0)); // perp_axis_in_a1
    bytes.extend(vec4(0.0, 0.0, 1.0, 0.0)); // perp_axis_in_a2
    bytes.extend(vec4(44.0, 55.0, 66.0, 1.0)); // pivot_b
    bytes.extend(vec4(1.0, 0.0, 0.0, 0.0)); // axis_b
    bytes.extend(vec4(0.0, 0.0, 1.0, 0.0)); // perp_axis_in_b2
    for v in [-1.0f32, 1.0, 10.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    assert_eq!(bytes.len(), 16 + 124, "base + 7×Vec4 + 3×f32");

    let header = oblivion_header();
    let mut stream = NifStream::new(&bytes, &header);
    let c = BhkConstraint::parse(&mut stream, "bhkLimitedHingeConstraint").unwrap();

    let BhkConstraintData::LimitedHinge(h) = c.data else {
        panic!("Oblivion LimitedHinge must decode, got {:?}", c.data);
    };
    assert_eq!(h.axis_a, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(h.pivot_a, [11.0, 22.0, 33.0, 1.0]);
    assert_eq!(h.axis_b, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(h.pivot_b, [44.0, 55.0, 66.0, 1.0]);
    assert_eq!(h.min_angle, -1.0);
    assert_eq!(h.max_angle, 1.0);
    assert_eq!(h.max_friction, 10.0);
    assert_eq!(h.perp_axis_in_b1, [0.0; 4]); // FO3+ only
    assert_eq!(stream.position() as usize, 16 + 124);
}

/// Oblivion `bhkMalleableConstraint` wrapping a Ragdoll (Type 7) must
/// surface as Ragdoll just like the FNV form, using the Oblivion inner
/// layout (120 B) + the Oblivion tau/damping trailer (8 B).
#[test]
fn oblivion_malleable_wrapping_ragdoll_surfaces_as_ragdoll() {
    let mut bytes = base(); // outer base: real bodies (entity_a=1, entity_b=2)
    bytes.extend_from_slice(&7u32.to_le_bytes()); // wrapped Type = 7 (Ragdoll)
    // nested bhkConstraintCInfo: num=2, entity_a=-1, entity_b=-1, priority=1
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    // inner Oblivion Ragdoll CInfo: 6 × Vec4 + 6 × f32 = 120.
    for i in 0..6 {
        bytes.extend(vec4(i as f32, 0.0, 0.0, 0.0));
    }
    for v in [0.5f32, -0.25, 0.75, -1.5, 1.5, 10.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    // Oblivion malleable trailer: Tau + Damping.
    bytes.extend_from_slice(&0.8f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());

    let header = oblivion_header();
    let mut stream = NifStream::new(&bytes, &header);
    let c = BhkConstraint::parse(&mut stream, "bhkMalleableConstraint").unwrap();

    assert_eq!(c.entity_a.index(), Some(1));
    assert_eq!(c.entity_b.index(), Some(2));
    let BhkConstraintData::Ragdoll(r) = c.data else {
        panic!("Oblivion malleable-wrapped Ragdoll must surface as Ragdoll, got {:?}", c.data);
    };
    // Oblivion order: index 0 = pivot_a, index 2 = twist_a.
    assert_eq!(r.pivot_a, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(r.twist_a, [2.0, 0.0, 0.0, 0.0]);
    assert_eq!(r.max_friction, 10.0);
    // base(16) + Type(4) + nested CInfo(16) + Ragdoll(120) + trailer(8).
    assert_eq!(stream.position() as usize, 16 + 4 + 16 + 120 + 8);
}

/// Skyrim LE/SE share the FNV (`!#NI_BS_LTE_16#`) constraint layout —
/// they're distinguished from Oblivion by NIF version (20.2.0.7), not
/// bsver, so a Skyrim-header Ragdoll must decode byte-identically to FNV.
/// This pins the version gate so a future change can't silently route
/// Skyrim down the Oblivion arm. (havok_scale ×69.99 is applied later, at
/// the import boundary, from `havok_scale_for(header)`.)
#[test]
fn skyrim_ragdoll_uses_fo3_layout() {
    let mut bytes = base();
    // FO3+ order, 8 × Vec4 + 6 × f32 (same as FNV).
    bytes.extend(vec4(0.0, 0.0, 1.0, 0.0)); // twist_a
    bytes.extend(vec4(1.0, 0.0, 0.0, 0.0)); // plane_a
    bytes.extend(vec4(2.0, 0.0, 0.0, 0.0)); // motor_a
    bytes.extend(vec4(3.0, 10.0, 20.0, 1.0)); // pivot_a
    bytes.extend(vec4(4.0, 0.0, 1.0, 0.0)); // twist_b
    bytes.extend(vec4(5.0, 0.0, 0.0, 0.0)); // plane_b
    bytes.extend(vec4(6.0, 0.0, 0.0, 0.0)); // motor_b
    bytes.extend(vec4(7.0, -10.0, 5.0, 1.0)); // pivot_b
    for v in [0.5f32, -0.25, 0.75, -1.5, 1.5, 100.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }

    // Skyrim SE header: NIF 20.2.0.7 (== FNV), bsver 100 (> 16 → FO3+ arm).
    let mut header = fnv_header();
    header.user_version = 12;
    header.user_version_2 = 100;
    let mut stream = NifStream::new(&bytes, &header);
    let c = BhkConstraint::parse(&mut stream, "bhkRagdollConstraint").unwrap();

    let BhkConstraintData::Ragdoll(r) = c.data else {
        panic!("Skyrim Ragdoll must decode via the FO3+ path, got {:?}", c.data);
    };
    assert_eq!(r.pivot_a, [3.0, 10.0, 20.0, 1.0]);
    assert_eq!(r.pivot_b, [7.0, -10.0, 5.0, 1.0]);
    assert_eq!(r.motor_a, [2.0, 0.0, 0.0, 0.0]); // FO3+ motors present
    assert_eq!(stream.position() as usize, 16 + 152);
}
