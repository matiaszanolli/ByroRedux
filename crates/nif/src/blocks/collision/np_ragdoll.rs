//! FO4+ `bhkRagdollSystem` payload: the `hknpRagdollData` articulation
//! (#5015).
//!
//! A Fallout 4 skeleton NIF hosts its ragdoll in a `bhkRagdollSystem` block
//! whose [`BhkSystemBinary`](super::BhkSystemBinary) payload is a classic
//! Havok packfile ([`parse_havok_packfile`]). The packfile's object table
//! names the classes; this module reads the fields of the ones that make a
//! ragdoll — `hknpRagdollData` (the bodies and the joint list), the
//! `hknpCapsuleShape` / `hknpSphereShape` / `hknpConvexPolytopeShape` /
//! `hknpDynamicCompoundShape` bodies and the `hkpRagdollConstraintData` /
//! `hkpLimitedHingeConstraintData` joints (also behind an
//! `hknpBreakableConstraintData` wrapper) — into
//! the same [`RagdollCInfo`] / [`LimitedHingeCInfo`] records the classic
//! (`bhkConstraint`) path produces, so the existing joint conversion serves
//! both generations.
//!
//! ## Clean-room posture
//!
//! `__types__` is empty in every blob, so the file ships no reflection
//! metadata, and no SDK header or documentation is consulted: every offset
//! below was recovered from the vanilla `Fallout4 - Meshes.ba2` corpus and is
//! pinned by an independent consistency check, listed with the field:
//!
//! - **Body mass** is `1 / inverse mass` of the body's motion row, and that
//!   row's density times the capsule's volume reproduces it — 18 of 18 bodies
//!   of the human skeleton to 1e-7 (`hknp` stores density beside inverse
//!   mass).
//! - **Body pose** (position, orientation) is the body's bind pose in the
//!   skeleton NIF's root space; it equals the host bone's world transform
//!   (COM at 68.91 units on both sides).
//! - **Joint frames** put each joint's pivot at the same world point from both
//!   bodies (largest disagreement 6 mm over 17 joints) and their axes
//!   coincide at the bind pose.
//! - **Polytope vertices** (#6067) reproduce the authored mass of every
//!   8-vertex box once the convex radius is folded into the volume.
//! - **Compound instance transforms** (#6067) are rotation *columns*: the
//!   compound data object stores each child's padded bounds, and only the
//!   column reading reproduces them for the rotated children.
//!
//! The per-body material and motion-property rows (friction, restitution,
//! damping) are present but their field identities are not established, so
//! they are not read.
//!
//! ## Layout (little-endian, 64-bit pointers, all offsets within `__data__`)
//!
//! `hknpRagdollData` at the first object. Its `hkArray` members (pointer at
//! `+0`, element count at `+8`) sit at `+16 + 16·i`; the ones used are
//! `+48` (motion rows, 112 B), `+64` (body rows, 96 B) and `+80` (joint rows,
//! 24 B). A body row holds the shape pointer (`+0`), the motion-row index
//! (`+12`), the skeleton bone id (`+21`, a `u8`), the position (`+48`, three `f32`) and the orientation (`+64`, an
//! `x y z w` quaternion). A motion row holds the inverse mass (`+4`) and the
//! density (`+8`). A joint row holds the constraint-data pointer (`+0`) and
//! the two body indices (`+8`, `+12`); the data object holds each body's
//! frame as three column vectors plus a pivot (A: `+48/+64/+80` and `+96`,
//! B: `+112/+128/+144` and `+160`). The limits follow as typed atoms.

use std::io;

use byroredux_core::bytes::{le_f32_at, le_u16_at, le_u32_at};

use super::constraints::{LimitedHingeCInfo, RagdollCInfo};
use super::havok_packfile::{parse_havok_packfile, HavokPackfile};

/// `hknpRagdollData` header: `hkArray` member offsets.
const MOTIONS_ARRAY: usize = 48;
const BODIES_ARRAY: usize = 64;
const JOINTS_ARRAY: usize = 80;
const BODY_ROW: usize = 96;
const MOTION_ROW: usize = 112;
const JOINT_ROW: usize = 24;

/// `hknpConvexPolytopeShape`: the vertex-array member and its row size.
const HULL_VERTICES: usize = 48;
const HULL_VERTEX_ROW: usize = 16;
/// `hknpDynamicCompoundShape`: the instance array (`hkArray`, pointer at `+96`,
/// count at `+104`) and its 128-byte rows: three rotation columns at `+0/+16/+32`,
/// the translation at `+48`, a per-axis scale at `+64` and the child-shape pointer
/// at `+80`.
const COMPOUND_INSTANCES: usize = 96;
const INSTANCE_ROW: usize = 128;
const INSTANCE_TRANSLATION: usize = 48;
const INSTANCE_SCALE: usize = 64;
const INSTANCE_SHAPE: usize = 80;
/// `hknpBreakableConstraintData`: the pointer to the wrapped constraint data.
const BREAKABLE_INNER: usize = 24;
/// The largest vanilla compound has four children; only stops a corrupt count.
const MAX_COMPOUND_CHILDREN: usize = 64;
/// More vertices than any vanilla hull carries (corpus maximum is recorded
/// beside the corpus test); only stops a corrupt count from allocating.
const MAX_HULL_VERTICES: usize = 1024;

/// Upper bound on bodies/joints a ragdoll blob may declare — the largest
/// vanilla creature has 41 bodies; the cap only stops a corrupt count from
/// reserving gigabytes.
const MAX_ELEMENTS: usize = 256;

/// Constraint atom type ids the layout expects, found by reading the id at
/// the expected offset (a mismatch means a layout this decoder does not know,
/// and the joint is refused rather than misread).
const ATOM_ANGULAR_FRICTION: u16 = 0x11;
const ATOM_HINGE_LIMIT: u16 = 0x0e;
const ATOM_TWIST_LIMIT: u16 = 0x0f;
const ATOM_CONE_LIMIT: u16 = 0x10;

/// A ragdoll body's collision shape, in the body's local frame (Havok metres,
/// Z-up).
#[derive(Debug, Clone, PartialEq)]
pub enum NpShape {
    /// Segment `a`–`b` swept by `radius`.
    Capsule {
        a: [f32; 3],
        b: [f32; 3],
        radius: f32,
    },
    Sphere {
        radius: f32,
    },
    /// `hknpConvexPolytopeShape`: the hull's vertices (the face planes and index
    /// lists that follow them in the object are redundant with the vertices) and
    /// the convex radius the hull is rounded by.
    ConvexHull {
        vertices: Vec<[f32; 3]>,
        radius: f32,
    },
    /// `hknpDynamicCompoundShape`: child shapes, each placed in the body's frame.
    Compound { children: Vec<NpCompoundChild> },
}

/// One instance of a compound shape: a leaf shape and where it sits.
#[derive(Debug, Clone, PartialEq)]
pub struct NpCompoundChild {
    /// The instance transform's three columns (the images of the child's X, Y
    /// and Z axes), so a child point `v` sits at `columns · v + translation`.
    pub columns: [[f32; 3]; 3],
    pub translation: [f32; 3],
    /// A capsule, sphere or convex hull — never another compound.
    pub shape: Box<NpShape>,
}

/// One ragdoll body (`hknpBodyCinfo` row).
#[derive(Debug, Clone, PartialEq)]
pub struct NpRagdollBody {
    /// Index in the blob's body array. A skeleton's `bhkNPCollisionObject`
    /// names its host bone by this id.
    pub index: usize,
    pub mass: f32,
    /// The motion row's density (`+8`, beside the inverse mass): the mass the
    /// authoring tool derived from the shape's volume is `volume × density`.
    pub density: f32,
    /// The ragdoll skeleton's bone this body drives (a `u8` at `+21` of the
    /// body row; two bodies may share one — the human's COM and SPINE1 are both
    /// bone 2). An `XRGD` corpse pose names its entries by this id.
    pub bone_id: u8,
    /// `None` for a shape class this decoder does not read (compounds); the
    /// body cannot be built.
    pub shape: Option<NpShape>,
    /// Bind-pose position in skeleton-root space (Havok metres, Z-up).
    pub position: [f32; 3],
    /// Bind-pose orientation `[x, y, z, w]` in skeleton-root space.
    pub rotation: [f32; 4],
}

/// A joint's decoded geometry, in the classic CInfo records.
#[derive(Debug, Clone)]
pub enum NpConstraintData {
    Ragdoll(RagdollCInfo),
    LimitedHinge(LimitedHingeCInfo),
}

/// One joint between two ragdoll bodies (indices into [`NpRagdoll::bodies`]).
#[derive(Debug, Clone)]
pub struct NpRagdollConstraint {
    pub body_a: usize,
    pub body_b: usize,
    pub data: NpConstraintData,
}

/// A decoded `hknpRagdollData`.
#[derive(Debug, Clone, Default)]
pub struct NpRagdoll {
    pub bodies: Vec<NpRagdollBody>,
    pub constraints: Vec<NpRagdollConstraint>,
    /// Joint rows whose constraint class or atom layout is not decoded
    /// (unknown atom ids, or a class other than a ragdoll or limited hinge).
    pub skipped_constraints: usize,
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

/// Bounds-checked reads inside the `__data__` section.
struct Data<'a> {
    bytes: &'a [u8],
}

impl Data<'_> {
    fn f32(&self, at: usize) -> io::Result<f32> {
        le_f32_at(self.bytes, at).ok_or_else(|| invalid("np ragdoll: truncated"))
    }
    fn u32(&self, at: usize) -> io::Result<u32> {
        le_u32_at(self.bytes, at).ok_or_else(|| invalid("np ragdoll: truncated"))
    }
    fn u16(&self, at: usize) -> io::Result<u16> {
        le_u16_at(self.bytes, at).ok_or_else(|| invalid("np ragdoll: truncated"))
    }
    fn vec3(&self, at: usize) -> io::Result<[f32; 3]> {
        Ok([self.f32(at)?, self.f32(at + 4)?, self.f32(at + 8)?])
    }
    /// A `vec3` widened to the `[f32; 4]` the classic CInfo records use.
    fn vec4(&self, at: usize) -> io::Result<[f32; 4]> {
        let v = self.vec3(at)?;
        Ok([v[0], v[1], v[2], 0.0])
    }
}

/// The resolved targets of a section's fixups, indexed by source offset.
struct Fixups<'a> {
    pack: &'a HavokPackfile,
}

impl Fixups<'_> {
    fn data_section(&self) -> io::Result<&super::havok_packfile::PackfileSection> {
        self.pack
            .section("__data__")
            .ok_or_else(|| invalid("np ragdoll: no __data__ section"))
    }

    /// Where the pointer stored at `src` points inside `__data__`.
    fn pointer(&self, src: usize) -> Option<usize> {
        let data = self.data_section().ok()?;
        let src = u32::try_from(src).ok()?;
        data.local_fixups
            .iter()
            .find(|f| f.src_offset == src)
            .map(|f| f.dst_offset as usize)
            .or_else(|| {
                data.global_fixups
                    .iter()
                    .find(|f| f.src_offset == src && f.dst_section == self.section_index())
                    .map(|f| f.dst_offset as usize)
            })
    }

    fn section_index(&self) -> u32 {
        self.pack
            .sections
            .iter()
            .position(|s| s.name == "__data__")
            .unwrap_or(0) as u32
    }

    /// The class name of the object that starts at `offset`.
    fn class_at(&self, offset: usize) -> Option<&str> {
        let data = self.data_section().ok()?;
        data.virtual_fixups
            .iter()
            .find(|v| v.data_offset as usize == offset)
            .and_then(|v| self.pack.class_name_at(v.class_name_offset))
    }
}

/// Decode a `bhkRagdollSystem` payload.
///
/// `Err` when the blob is not a packfile, has no `hknpRagdollData`, or its
/// arrays disagree; individual joints or bodies the decoder cannot read are
/// reported through [`NpRagdoll::skipped_constraints`] / a `None` shape, not
/// as an error.
pub fn decode_np_ragdoll(blob: &[u8]) -> io::Result<NpRagdoll> {
    let pack = parse_havok_packfile(blob)?;
    let fixups = Fixups { pack: &pack };
    let section = fixups.data_section()?;
    let start = section.absolute_data_start as usize;
    let bytes = blob
        .get(start..start + section.local_fixups_offset as usize)
        .ok_or_else(|| invalid("np ragdoll: __data__ outside the blob"))?;
    let data = Data { bytes };

    let root = pack
        .objects()
        .into_iter()
        .find(|o| o.class_name == "hknpRagdollData")
        .ok_or_else(|| invalid("np ragdoll: no hknpRagdollData object"))?
        .section_offset as usize;

    let array = |member: usize, row: usize| -> io::Result<(usize, usize)> {
        let count = data.u32(root + member + 8)? as usize;
        if count == 0 {
            return Ok((0, 0));
        }
        if count > MAX_ELEMENTS {
            return Err(invalid("np ragdoll: implausible element count"));
        }
        let base = fixups
            .pointer(root + member)
            .ok_or_else(|| invalid("np ragdoll: array pointer is unbound"))?;
        if base + count * row > bytes.len() {
            return Err(invalid("np ragdoll: array runs past __data__"));
        }
        Ok((base, count))
    };
    let (motions, motion_count) = array(MOTIONS_ARRAY, MOTION_ROW)?;
    let (body_rows, body_count) = array(BODIES_ARRAY, BODY_ROW)?;
    let (joint_rows, joint_count) = array(JOINTS_ARRAY, JOINT_ROW)?;

    let mut bodies = Vec::with_capacity(body_count);
    for index in 0..body_count {
        let row = body_rows + index * BODY_ROW;
        let motion = data.u32(row + 12)? as usize;
        if motion >= motion_count {
            return Err(invalid("np ragdoll: body names a motion row past the array"));
        }
        let inverse_mass = data.f32(motions + motion * MOTION_ROW + 4)?;
        if !(inverse_mass.is_finite() && inverse_mass > 0.0) {
            return Err(invalid("np ragdoll: non-positive body mass"));
        }
        bodies.push(NpRagdollBody {
            index,
            mass: 1.0 / inverse_mass,
            density: data.f32(motions + motion * MOTION_ROW + 8)?,
            bone_id: *bytes
                .get(row + 21)
                .ok_or_else(|| invalid("np ragdoll: truncated"))?,
            shape: fixups.pointer(row).and_then(|at| read_shape(&fixups, &data, at)),
            position: data.vec3(row + 48)?,
            rotation: [
                data.f32(row + 64)?,
                data.f32(row + 68)?,
                data.f32(row + 72)?,
                data.f32(row + 76)?,
            ],
        });
    }

    let mut constraints = Vec::with_capacity(joint_count);
    let mut skipped_constraints = 0;
    for k in 0..joint_count {
        let row = joint_rows + k * JOINT_ROW;
        let body_a = data.u32(row + 8)? as usize;
        let body_b = data.u32(row + 12)? as usize;
        let decoded = (body_a < body_count && body_b < body_count && body_a != body_b)
            .then(|| fixups.pointer(row))
            .flatten()
            .and_then(|at| read_constraint(&fixups, &data, at));
        match decoded {
            Some(data) => constraints.push(NpRagdollConstraint {
                body_a,
                body_b,
                data,
            }),
            None => skipped_constraints += 1,
        }
    }
    Ok(NpRagdoll {
        bodies,
        constraints,
        skipped_constraints,
    })
}

/// Read a body's shape object, or `None` for a class this decoder does not
/// read. Capsule: radius at `+20`, end points at `+80` / `+96`. Sphere:
/// radius at `+20`. Polytope: radius at `+20`, vertices per
/// [`read_hull_vertices`]. Compound: instances per [`read_compound`].
fn read_shape(fixups: &Fixups<'_>, data: &Data<'_>, at: usize) -> Option<NpShape> {
    read_shape_at(fixups, data, at, true)
}

fn read_shape_at(
    fixups: &Fixups<'_>,
    data: &Data<'_>,
    at: usize,
    allow_compound: bool,
) -> Option<NpShape> {
    let radius = || data.f32(at + 20).ok().filter(|r| r.is_finite() && *r >= 0.0);
    match fixups.class_at(at)? {
        "hknpCapsuleShape" => Some(NpShape::Capsule {
            a: data.vec3(at + 80).ok()?,
            b: data.vec3(at + 96).ok()?,
            radius: radius()?,
        }),
        "hknpSphereShape" => Some(NpShape::Sphere { radius: radius()? }),
        "hknpConvexPolytopeShape" => Some(NpShape::ConvexHull {
            vertices: read_hull_vertices(data, at)?,
            radius: radius()?,
        }),
        "hknpDynamicCompoundShape" if allow_compound => read_compound(fixups, data, at),
        _ => None,
    }
}

/// A compound's instances. A child that is not a capsule, sphere or hull, or
/// whose instance scale is not 1, refuses the whole compound rather than
/// building a body with a part missing.
fn read_compound(fixups: &Fixups<'_>, data: &Data<'_>, at: usize) -> Option<NpShape> {
    let count = data.u32(at + COMPOUND_INSTANCES + 8).ok()? as usize;
    if count == 0 || count > MAX_COMPOUND_CHILDREN {
        return None;
    }
    let first = fixups.pointer(at + COMPOUND_INSTANCES)?;
    let mut children = Vec::with_capacity(count);
    for i in 0..count {
        let row = first + i * INSTANCE_ROW;
        if row + INSTANCE_ROW > data.bytes.len() {
            return None;
        }
        if data.vec3(row + INSTANCE_SCALE).ok()? != [1.0; 3] {
            return None;
        }
        let child = fixups.pointer(row + INSTANCE_SHAPE)?;
        children.push(NpCompoundChild {
            columns: [data.vec3(row).ok()?, data.vec3(row + 16).ok()?, data.vec3(row + 32).ok()?],
            translation: data.vec3(row + INSTANCE_TRANSLATION).ok()?,
            shape: Box::new(read_shape_at(fixups, data, child, false)?),
        });
    }
    Some(NpShape::Compound { children })
}

/// A polytope's vertices: a `u16` count at `+48` and a `u16` byte offset at
/// `+50` (relative to `+48`) to rows of 16 bytes, three `f32` and a word that
/// carries the vertex's own index in its low bits and is not read.
fn read_hull_vertices(data: &Data<'_>, at: usize) -> Option<Vec<[f32; 3]>> {
    let count = data.u16(at + HULL_VERTICES).ok()? as usize;
    let first = at + HULL_VERTICES + data.u16(at + HULL_VERTICES + 2).ok()? as usize;
    if !(4..=MAX_HULL_VERTICES).contains(&count) || first + count * HULL_VERTEX_ROW > data.bytes.len() {
        return None;
    }
    (0..count)
        .map(|i| data.vec3(first + i * HULL_VERTEX_ROW).ok().filter(|v| v.iter().all(|c| c.is_finite())))
        .collect()
}

/// The two body frames of a constraint object: columns and pivot per side.
fn read_frames(data: &Data<'_>, at: usize) -> io::Result<[[[f32; 4]; 4]; 2]> {
    let side = |cols: usize, pivot: usize| -> io::Result<[[f32; 4]; 4]> {
        Ok([
            data.vec4(at + cols)?,
            data.vec4(at + cols + 16)?,
            data.vec4(at + cols + 32)?,
            data.vec4(at + pivot)?,
        ])
    };
    Ok([side(48, 96)?, side(112, 160)?])
}

fn read_constraint(fixups: &Fixups<'_>, data: &Data<'_>, at: usize) -> Option<NpConstraintData> {
    read_constraint_at(fixups, data, at, true)
}

fn read_constraint_at(
    fixups: &Fixups<'_>,
    data: &Data<'_>,
    at: usize,
    allow_wrapper: bool,
) -> Option<NpConstraintData> {
    if allow_wrapper && fixups.class_at(at)? == "hknpBreakableConstraintData" {
        // The wrapper holds the constraint it breaks at `+24` (and a threshold
        // float at `+32`, which is not read: a joint stays unbreakable).
        return read_constraint_at(fixups, data, fixups.pointer(at + BREAKABLE_INNER)?, false);
    }
    let [a, b] = read_frames(data, at).ok()?;
    match fixups.class_at(at)? {
        // Atoms after the frames: stabilisation (0x17 @176), motors (0x13 @192),
        // angular friction (0x11 @288), twist (0x0f @304), cone (0x10 @336),
        // plane (0x10 @368).
        "hkpRagdollConstraintData" => {
            let ids = [
                (288, ATOM_ANGULAR_FRICTION),
                (304, ATOM_TWIST_LIMIT),
                (336, ATOM_CONE_LIMIT),
                (368, ATOM_CONE_LIMIT),
            ];
            if !ids.iter().all(|&(off, id)| data.u16(at + off).ok() == Some(id)) {
                return None;
            }
            Some(NpConstraintData::Ragdoll(RagdollCInfo {
                // Frame columns are (twist, plane, third); the classic CInfo
                // calls the third axis the motor axis.
                twist_a: a[0],
                plane_a: a[1],
                motor_a: a[2],
                pivot_a: a[3],
                twist_b: b[0],
                plane_b: b[1],
                motor_b: b[2],
                pivot_b: b[3],
                cone_max_angle: data.f32(at + 348).ok()?,
                plane_min_angle: data.f32(at + 376).ok()?,
                plane_max_angle: data.f32(at + 380).ok()?,
                twist_min_angle: data.f32(at + 312).ok()?,
                twist_max_angle: data.f32(at + 316).ok()?,
                max_friction: data.f32(at + 296).ok()?,
            }))
        }
        // Atoms after the frames: stabilisation (0x17 @176), motor (0x12 @192),
        // angular friction (0x11 @232), hinge limit (0x0e @248).
        "hkpLimitedHingeConstraintData" => {
            let ids = [(232, ATOM_ANGULAR_FRICTION), (248, ATOM_HINGE_LIMIT)];
            if !ids.iter().all(|&(off, id)| data.u16(at + off).ok() == Some(id)) {
                return None;
            }
            Some(NpConstraintData::LimitedHinge(LimitedHingeCInfo {
                axis_a: a[0],
                perp_axis_in_a1: a[1],
                perp_axis_in_a2: a[2],
                pivot_a: a[3],
                axis_b: b[0],
                perp_axis_in_b1: b[1],
                perp_axis_in_b2: b[2],
                pivot_b: b[3],
                min_angle: data.f32(at + 252).ok()?,
                max_angle: data.f32(at + 256).ok()?,
                max_friction: data.f32(at + 240).ok()?,
            }))
        }
        _ => None,
    }
}

/// A two-body, one-joint synthetic blob for the extraction tests in
/// `import::collision::ragdoll` (body 0 at (0, 0, 1) m, body 1 at (0.5, 0, 1) m,
/// 4 kg and 2 kg, one ragdoll joint).
#[cfg(test)]
pub(crate) fn synthetic_two_body_blob() -> Vec<u8> {
    tests::good()
}

#[cfg(test)]
mod tests {
    use super::super::havok_packfile::tests::{
        build_synthetic_packfile_with_fixups, synthetic_class_name_offset,
    };
    use super::super::havok_packfile::{GlobalFixup, LocalFixup, VirtualFixup};
    use super::*;

    const CLASSES: [&str; 6] = [
        "hknpRagdollData",
        "hknpCapsuleShape",
        "hknpConvexPolytopeShape",
        "hkpRagdollConstraintData",
        "hknpDynamicCompoundShape",
        "hknpBreakableConstraintData",
    ];

    /// What a fixture hook adds to the base fixture.
    #[derive(Default)]
    struct Extras {
        locals: Vec<LocalFixup>,
        globals: Vec<GlobalFixup>,
        virtuals: Vec<VirtualFixup>,
        /// Where the joint row points instead of the constraint object.
        joint_at: Option<usize>,
    }

    // Synthetic `__data__` layout (offsets within the section).
    const ROOT: usize = 0;
    const MOTIONS: usize = 192;
    const BODIES: usize = MOTIONS + 2 * MOTION_ROW;
    const JOINTS: usize = BODIES + 2 * BODY_ROW;
    const SHAPE_0: usize = 640;
    const SHAPE_1: usize = SHAPE_0 + 432;
    const JOINT: usize = SHAPE_1 + 432;
    /// Scratch space past the joint, for the objects only some tests add.
    const EXTRA: usize = JOINT + 416;
    const END: usize = EXTRA + 512;

    fn put_f32(buf: &mut [u8], at: usize, v: f32) {
        buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn put_u32(buf: &mut [u8], at: usize, v: u32) {
        buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn put_u16(buf: &mut [u8], at: usize, v: u16) {
        buf[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn put_vec3(buf: &mut [u8], at: usize, v: [f32; 3]) {
        for (i, c) in v.iter().enumerate() {
            put_f32(buf, at + 4 * i, *c);
        }
    }

    /// Two capsule bodies 0.5 m apart joined by one ragdoll joint, built to
    /// the corpus-derived layout: body 0 at the origin, body 1 half a metre
    /// along +X; the joint's pivot is body 1's origin from body 0 and the
    /// origin of body 1 from itself, so both sides name the same world point.
    /// `second_shape` names the class of body 1's shape; `friction_atom` the
    /// atom id at the ragdoll friction slot.
    fn fixture(second_shape: &str, friction_atom: u16, counts: [u32; 3]) -> Vec<u8> {
        fixture_with(second_shape, friction_atom, counts, |_| Extras::default())
    }

    /// [`fixture`] with a hook that edits the `__data__` bytes before the packfile is
    /// built and returns the fixups its edit needs.
    fn fixture_with(
        second_shape: &str,
        friction_atom: u16,
        counts: [u32; 3],
        edit: impl FnOnce(&mut Vec<u8>) -> Extras,
    ) -> Vec<u8> {
        let mut d = vec![0u8; END];
        // hknpRagdollData header: arrays (motions, bodies, joints).
        put_u32(&mut d, ROOT + MOTIONS_ARRAY + 8, counts[0]);
        put_u32(&mut d, ROOT + BODIES_ARRAY + 8, counts[1]);
        put_u32(&mut d, ROOT + JOINTS_ARRAY + 8, counts[2]);
        // Motion rows: inverse mass 0.25 (4 kg) and 0.5 (2 kg), density beside.
        put_f32(&mut d, MOTIONS + 4, 0.25);
        put_f32(&mut d, MOTIONS + 8, 400.0);
        put_f32(&mut d, MOTIONS + MOTION_ROW + 4, 0.5);
        put_f32(&mut d, MOTIONS + MOTION_ROW + 8, 400.0);
        // Body rows: motion index (+12), position (+48), orientation (+64).
        for (i, x) in [0.0f32, 0.5].into_iter().enumerate() {
            let row = BODIES + i * BODY_ROW;
            put_u32(&mut d, row + 12, i as u32);
            put_vec3(&mut d, row + 48, [x, 0.0, 1.0]);
            put_f32(&mut d, row + 76, 1.0); // identity quaternion (w)
        }
        // Capsule shapes: radius (+20), end points (+80, +96).
        for (shape, radius, len) in [(SHAPE_0, 0.1f32, 0.2f32), (SHAPE_1, 0.05, 0.3)] {
            put_f32(&mut d, shape + 20, radius);
            put_vec3(&mut d, shape + 80, [0.0, 0.0, -len / 2.0]);
            put_vec3(&mut d, shape + 96, [0.0, 0.0, len / 2.0]);
        }
        // Joint row: bodies (+8, +12) = (0, 1).
        put_u32(&mut d, JOINTS + 8, 0);
        put_u32(&mut d, JOINTS + 12, 1);
        // Constraint object: identity frames, pivots, then the atoms.
        for (cols, pivot, p) in [(48, 96, [0.5f32, 0.0, 0.0]), (112, 160, [0.0, 0.0, 0.0])] {
            put_vec3(&mut d, JOINT + cols, [1.0, 0.0, 0.0]);
            put_vec3(&mut d, JOINT + cols + 16, [0.0, 1.0, 0.0]);
            put_vec3(&mut d, JOINT + cols + 32, [0.0, 0.0, 1.0]);
            put_vec3(&mut d, JOINT + pivot, p);
        }
        put_u16(&mut d, JOINT + 288, friction_atom);
        put_f32(&mut d, JOINT + 296, 3.0);
        put_u16(&mut d, JOINT + 304, ATOM_TWIST_LIMIT);
        put_f32(&mut d, JOINT + 312, -0.25);
        put_f32(&mut d, JOINT + 316, 0.3);
        put_u16(&mut d, JOINT + 336, ATOM_CONE_LIMIT);
        put_f32(&mut d, JOINT + 344, -100.0);
        put_f32(&mut d, JOINT + 348, 0.5);
        put_u16(&mut d, JOINT + 368, ATOM_CONE_LIMIT);
        put_f32(&mut d, JOINT + 376, -0.4);
        put_f32(&mut d, JOINT + 380, 0.2);

        let extras = edit(&mut d);
        let locals = [
            LocalFixup { src_offset: (ROOT + MOTIONS_ARRAY) as u32, dst_offset: MOTIONS as u32 },
            LocalFixup { src_offset: (ROOT + BODIES_ARRAY) as u32, dst_offset: BODIES as u32 },
            LocalFixup { src_offset: (ROOT + JOINTS_ARRAY) as u32, dst_offset: JOINTS as u32 },
        ]
        .into_iter()
        .chain(extras.locals)
        .collect::<Vec<_>>();
        let global = |src: usize, dst: usize| GlobalFixup {
            src_offset: src as u32,
            dst_section: 2,
            dst_offset: dst as u32,
        };
        let globals = [
            global(BODIES, SHAPE_0),
            global(BODIES + BODY_ROW, SHAPE_1),
            global(JOINTS, extras.joint_at.unwrap_or(JOINT)),
        ]
        .into_iter()
        .chain(extras.globals)
        .collect::<Vec<_>>();
        let virtual_at = |offset: usize, class: &str| VirtualFixup {
            data_offset: offset as u32,
            class_section: 0,
            class_name_offset: synthetic_class_name_offset(&CLASSES, class),
        };
        let virtuals = [
            virtual_at(ROOT, "hknpRagdollData"),
            virtual_at(SHAPE_0, "hknpCapsuleShape"),
            virtual_at(SHAPE_1, second_shape),
            virtual_at(JOINT, "hkpRagdollConstraintData"),
        ]
        .into_iter()
        .chain(extras.virtuals)
        .collect::<Vec<_>>();
        build_synthetic_packfile_with_fixups(&CLASSES, &d, &locals, &globals, &virtuals)
    }

    pub(super) fn good() -> Vec<u8> {
        fixture("hknpCapsuleShape", ATOM_ANGULAR_FRICTION, [2, 2, 1])
    }

    #[test]
    fn decodes_bodies_shapes_and_a_ragdoll_joint() {
        let r = decode_np_ragdoll(&good()).expect("decodes");
        assert_eq!(r.bodies.len(), 2);
        assert_eq!(r.skipped_constraints, 0);
        assert_eq!(r.bodies[0].index, 0);
        assert!((r.bodies[0].mass - 4.0).abs() < 1e-5, "1 / inverse mass");
        assert!((r.bodies[1].mass - 2.0).abs() < 1e-5);
        assert_eq!(r.bodies[1].position, [0.5, 0.0, 1.0]);
        assert_eq!(r.bodies[1].rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            r.bodies[0].shape,
            Some(NpShape::Capsule { a: [0.0, 0.0, -0.1], b: [0.0, 0.0, 0.1], radius: 0.1 })
        );
        let [c] = r.constraints.as_slice() else { panic!("one joint") };
        assert_eq!((c.body_a, c.body_b), (0, 1));
        let NpConstraintData::Ragdoll(j) = &c.data else { panic!("a ragdoll joint") };
        // The frame columns are (twist, plane, third axis).
        assert_eq!(j.twist_a, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(j.plane_a, [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(j.motor_a, [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(j.pivot_a, [0.5, 0.0, 0.0, 0.0]);
        assert_eq!(j.pivot_b, [0.0; 4]);
        assert_eq!((j.twist_min_angle, j.twist_max_angle), (-0.25, 0.3));
        assert_eq!(j.cone_max_angle, 0.5);
        assert_eq!((j.plane_min_angle, j.plane_max_angle), (-0.4, 0.2));
        assert_eq!(j.max_friction, 3.0);
        // The corpus invariant the layout was derived from: both sides name
        // the same world point at the bind pose.
        let world = |body: usize, pivot: [f32; 4]| {
            let p = r.bodies[body].position;
            [p[0] + pivot[0], p[1] + pivot[1], p[2] + pivot[2]]
        };
        assert_eq!(world(0, j.pivot_a), world(1, j.pivot_b));
    }

    /// A polytope whose vertex table is empty (here: capsule bytes under a
    /// polytope class) leaves the body decoded with `shape: None` (the extractor
    /// then drops it), never a misread shape.
    #[test]
    fn an_unread_shape_class_leaves_the_body_without_a_shape() {
        let r = decode_np_ragdoll(&fixture("hknpConvexPolytopeShape", ATOM_ANGULAR_FRICTION, [2, 2, 1]))
            .expect("decodes");
        assert!(r.bodies[0].shape.is_some());
        assert_eq!(r.bodies[1].shape, None);
    }

    /// A constraint whose atom ids are not the ones the layout expects is
    /// counted and skipped rather than read at the wrong offsets.
    #[test]
    fn an_unexpected_atom_layout_skips_the_joint() {
        let r = decode_np_ragdoll(&fixture("hknpCapsuleShape", 0x99, [2, 2, 1])).expect("decodes");
        assert!(r.constraints.is_empty());
        assert_eq!(r.skipped_constraints, 1);
    }

    fn put_u16_at(buf: &mut [u8], at: usize, v: u16) {
        buf[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }

    /// A unit cube's eight corners at `at`, written as a polytope's vertex table
    /// (count and relative offset at `+48`, 16-byte rows from `+80`), the last
    /// word of each row carrying the vertex index the way the corpus does.
    fn write_cube_hull(d: &mut [u8], at: usize, half: f32, radius: f32) {
        put_f32(d, at + 20, radius);
        put_u16_at(d, at + HULL_VERTICES, 8);
        put_u16_at(d, at + HULL_VERTICES + 2, 32);
        for i in 0..8usize {
            let row = at + 80 + i * HULL_VERTEX_ROW;
            let corner = [i & 1, (i >> 1) & 1, (i >> 2) & 1].map(|bit| if bit == 1 { half } else { -half });
            put_vec3(d, row, corner);
            put_u32(d, row + 12, 0x3f00_0000 | i as u32);
        }
    }

    #[test]
    fn a_polytope_body_decodes_its_vertices_and_radius() {
        let blob = fixture_with("hknpConvexPolytopeShape", ATOM_ANGULAR_FRICTION, [2, 2, 1], |d| {
            write_cube_hull(d, SHAPE_1, 0.5, 0.01);
            Extras::default()
        });
        let r = decode_np_ragdoll(&blob).expect("decodes");
        let Some(NpShape::ConvexHull { vertices, radius }) = &r.bodies[1].shape else {
            panic!("a hull, got {:?}", r.bodies[1].shape)
        };
        assert_eq!(*radius, 0.01);
        assert_eq!(vertices.len(), 8);
        assert_eq!(vertices[0], [-0.5, -0.5, -0.5]);
        assert_eq!(vertices[7], [0.5, 0.5, 0.5], "the index word is not read as a coordinate");
        assert!(r.bodies[0].shape.is_some(), "the capsule body is unaffected");
    }

    #[test]
    fn a_polytope_whose_table_leaves_the_object_or_is_degenerate_is_refused() {
        for (count, offset) in [(8u16, 0xff00u16), (3, 32), (u16::MAX, 32)] {
            let blob = fixture_with("hknpConvexPolytopeShape", ATOM_ANGULAR_FRICTION, [2, 2, 1], |d| {
                write_cube_hull(d, SHAPE_1, 0.5, 0.01);
                put_u16_at(d, SHAPE_1 + HULL_VERTICES, count);
                put_u16_at(d, SHAPE_1 + HULL_VERTICES + 2, offset);
                Extras::default()
            });
            let r = decode_np_ragdoll(&blob).expect("decodes");
            assert_eq!(r.bodies[1].shape, None, "count {count} offset {offset:#x}");
        }
    }

    /// A compound of two capsule instances: the second turned a quarter about Z and
    /// shifted. The columns are the images of the child's axes.
    fn compound_blob(scale: f32) -> Vec<u8> {
        fixture_with("hknpDynamicCompoundShape", ATOM_ANGULAR_FRICTION, [2, 2, 1], |d| {
            put_u32(d, SHAPE_1 + COMPOUND_INSTANCES + 8, 2);
            let columns = [([1.0, 0.0, 0.0], [0.0, 0.0, 0.0]), ([0.0, 1.0, 0.0], [0.25, 0.0, 0.0])];
            let turned = [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
            for (i, (x, shift)) in columns.into_iter().enumerate() {
                let row = EXTRA + i * INSTANCE_ROW;
                let cols = if i == 0 { [x, [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] } else { turned };
                for (k, c) in cols.iter().enumerate() {
                    put_vec3(d, row + 16 * k, *c);
                }
                put_vec3(d, row + INSTANCE_TRANSLATION, shift);
                put_vec3(d, row + INSTANCE_SCALE, [scale; 3]);
            }
            let locals = vec![LocalFixup {
                src_offset: (SHAPE_1 + COMPOUND_INSTANCES) as u32,
                dst_offset: EXTRA as u32,
            }];
            let globals = (0..2)
                .map(|i| GlobalFixup {
                    src_offset: (EXTRA + i * INSTANCE_ROW + INSTANCE_SHAPE) as u32,
                    dst_section: 2,
                    dst_offset: SHAPE_0 as u32,
                })
                .collect();
            Extras { locals, globals, ..Extras::default() }
        })
    }

    #[test]
    fn a_compound_body_decodes_its_instances() {
        let r = decode_np_ragdoll(&compound_blob(1.0)).expect("decodes");
        let Some(NpShape::Compound { children }) = &r.bodies[1].shape else {
            panic!("a compound, got {:?}", r.bodies[1].shape)
        };
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].columns, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        assert_eq!(children[1].columns[0], [0.0, 1.0, 0.0]);
        assert_eq!(children[1].columns[1], [-1.0, 0.0, 0.0]);
        assert_eq!(children[1].translation, [0.25, 0.0, 0.0]);
        assert!(matches!(*children[1].shape, NpShape::Capsule { radius, .. } if radius == 0.1));
    }

    /// An instance scale other than 1 is not modelled, so the whole compound is
    /// refused rather than built at the wrong size.
    #[test]
    fn a_scaled_compound_instance_refuses_the_compound() {
        let r = decode_np_ragdoll(&compound_blob(2.0)).expect("decodes");
        assert_eq!(r.bodies[1].shape, None);
    }

    /// The joint row points at an `hknpBreakableConstraintData` whose `+24` pointer names
    /// the ragdoll joint; `wrap_twice` puts a second wrapper in front of the first.
    fn breakable_blob(wrap_twice: bool) -> Vec<u8> {
        fixture_with("hknpCapsuleShape", ATOM_ANGULAR_FRICTION, [2, 2, 1], |_| {
            let wrapper = VirtualFixup {
                data_offset: EXTRA as u32,
                class_section: 0,
                class_name_offset: synthetic_class_name_offset(&CLASSES, "hknpBreakableConstraintData"),
            };
            let inner = |wrapper_at: usize, target: usize| GlobalFixup {
                src_offset: (wrapper_at + BREAKABLE_INNER) as u32,
                dst_section: 2,
                dst_offset: target as u32,
            };
            let (virtuals, globals, joint_at) = if wrap_twice {
                let outer = VirtualFixup { data_offset: (EXTRA + 64) as u32, ..wrapper };
                (vec![wrapper, outer], vec![inner(EXTRA + 64, EXTRA), inner(EXTRA, JOINT)], EXTRA + 64)
            } else {
                (vec![wrapper], vec![inner(EXTRA, JOINT)], EXTRA)
            };
            Extras { globals, virtuals, joint_at: Some(joint_at), ..Extras::default() }
        })
    }

    #[test]
    fn a_breakable_wrapper_yields_the_joint_it_wraps() {
        let r = decode_np_ragdoll(&breakable_blob(false)).expect("decodes");
        assert_eq!(r.skipped_constraints, 0);
        let [c] = r.constraints.as_slice() else { panic!("one joint") };
        assert_eq!((c.body_a, c.body_b), (0, 1));
        let NpConstraintData::Ragdoll(j) = &c.data else { panic!("a ragdoll joint") };
        assert_eq!(j.pivot_a, [0.5, 0.0, 0.0, 0.0]);
        assert_eq!(j.cone_max_angle, 0.5);
    }

    /// A wrapper around a wrapper is a layout this decoder does not know: the joint is
    /// counted as skipped instead of recursed into.
    #[test]
    fn a_wrapper_around_a_wrapper_skips_the_joint() {
        let r = decode_np_ragdoll(&breakable_blob(true)).expect("decodes");
        assert!(r.constraints.is_empty());
        assert_eq!(r.skipped_constraints, 1);
    }

    #[test]
    fn corrupt_blobs_are_errors_not_panics() {
        let good = good();
        assert!(decode_np_ragdoll(&[]).is_err());
        assert!(decode_np_ragdoll(&good[..good.len() / 2]).is_err());
        // A body row naming a motion row past the array.
        assert!(decode_np_ragdoll(&fixture("hknpCapsuleShape", ATOM_ANGULAR_FRICTION, [1, 2, 1])).is_err());
        // An implausible element count must not reserve memory for it.
        assert!(decode_np_ragdoll(&fixture("hknpCapsuleShape", ATOM_ANGULAR_FRICTION, [2, u32::MAX, 1])).is_err());
        // A joint count with no joint array behind it.
        let r = decode_np_ragdoll(&fixture("hknpCapsuleShape", ATOM_ANGULAR_FRICTION, [2, 2, 0])).unwrap();
        assert!(r.constraints.is_empty());
    }
}
