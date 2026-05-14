//! Havok collision block parsers (bhk* types).
//!
//! These blocks form the collision geometry tree in Bethesda NIF files.
//! The pipeline: bhkCollisionObject → bhkRigidBody → shape tree.
//! Parsed data feeds a physics-agnostic ECS representation (CollisionShape),
//! which will be converted to Rapier colliders in the physics system (M28).

use super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use crate::version::NifVersion;
use std::any::Any;
use std::io;
use std::sync::Arc;

// ── Collision Object ────────────────────────────────────────────────

/// NiCollisionObject — base class for collision attachments.
///
/// Per nif.xml `NiCollisionObject` has exactly one field, a weak `Target`
/// pointer back at the NiAVObject this collision object is attached to.
/// Bethesda Havok subclasses add `Flags` and `Body` on top (see
/// `BhkCollisionObject` below), but the plain base occasionally appears
/// as a direct block in Oblivion scenes (#125). Because Oblivion NIFs
/// have no `block_sizes`, an unknown-type fallback cascades the whole
/// parse; parsing even the base 4 bytes is enough to keep the loop
/// alive.
#[derive(Debug)]
pub struct NiCollisionObjectBase {
    pub target_ref: BlockRef,
}

impl NiObject for NiCollisionObjectBase {
    fn block_type_name(&self) -> &'static str {
        "NiCollisionObject"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiCollisionObjectBase {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let target_ref = stream.read_block_ref()?;
        Ok(Self { target_ref })
    }
}

/// bhkCollisionObject — attaches a rigid body to a NiAVObject.
/// Concrete subclass of bhkNiCollisionObject (NiCollisionObject base).
#[derive(Debug)]
pub struct BhkCollisionObject {
    pub target_ref: BlockRef,
    pub flags: u16,
    pub body_ref: BlockRef,
}

impl NiObject for BhkCollisionObject {
    fn block_type_name(&self) -> &'static str {
        "bhkCollisionObject"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkCollisionObject {
    pub fn parse(stream: &mut NifStream, is_blend: bool) -> io::Result<Self> {
        let target_ref = stream.read_block_ref()?;
        let flags = stream.read_u16_le()?;
        let body_ref = stream.read_block_ref()?;
        // bhkBlendCollisionObject adds heirGain(f32) + velGain(f32) = 8 bytes.
        if is_blend {
            let _heir_gain = stream.read_f32_le()?;
            let _vel_gain = stream.read_f32_le()?;
            // Pre-Oblivion-mainline content (bsver < 9) ships an
            // additional pair of `Unknown Float 1` + `Unknown Float 2`
            // — see nif.xml line 3428-3429:
            //   <field name="Unknown Float 1" type="float" vercond="#BSVER# #LT# 9" />
            //   <field name="Unknown Float 2" type="float" vercond="#BSVER# #LT# 9" />
            // Pre-#549 the parser ignored this gate; on the boxtest
            // skeleton.nif (v10.2.0.0 / bsver=6) the missing 8 bytes
            // drifted the stream and pushed every downstream block
            // (4 bhkRigidBody, the host NiNode, etc.) into NiUnknown
            // — surfaced by the M33 audit as NIF-04 ("6 Oblivion
            // bhkRigidBody fail"), but the bug is here.
            if stream.bsver() < 9 {
                let _unknown_float_1 = stream.read_f32_le()?;
                let _unknown_float_2 = stream.read_f32_le()?;
            }
        }
        Ok(Self {
            target_ref,
            flags,
            body_ref,
        })
    }
}

/// bhkNPCollisionObject — Fallout 4 / Fallout 76 collision object.
///
/// FO4 replaced the classic bhkRigidBody chain with a new physics
/// subsystem ("NP" physics); NP files reference a `bhkSystem` subclass
/// (physics system or ragdoll system) instead of an individual rigid
/// body, and the actual collision binary lives inside that system's
/// `ByteArray`. Per nif.xml (`#FO4# #F76#`):
///
/// ```text
/// NiCollisionObject : { target_ref: Ptr<NiAVObject> }
/// bhkNPCollisionObject extends NiCollisionObject {
///     flags:   u16  (bhkCOFlags — same encoding as classic bhk)
///     data:    Ref<bhkSystem>
///     body_id: u32
/// }
/// ```
///
/// Until the FO4 NP data is fully consumed by the physics bridge we
/// record the raw references so downstream systems can resolve the
/// physics system on demand. Replaces the earlier skip-only dispatch
/// that dropped all FO4 collision. See #124 / audit NIF-513.
#[derive(Debug)]
pub struct BhkNPCollisionObject {
    pub target_ref: BlockRef,
    pub flags: u16,
    pub data_ref: BlockRef,
    pub body_id: u32,
}

impl NiObject for BhkNPCollisionObject {
    fn block_type_name(&self) -> &'static str {
        "bhkNPCollisionObject"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkNPCollisionObject {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let target_ref = stream.read_block_ref()?;
        let flags = stream.read_u16_le()?;
        let data_ref = stream.read_block_ref()?;
        let body_id = stream.read_u32_le()?;
        Ok(Self {
            target_ref,
            flags,
            data_ref,
            body_id,
        })
    }
}

/// bhkPhysicsSystem / bhkRagdollSystem — FO4 / FO76 NP-physics data.
///
/// Both concrete subclasses of the abstract `bhkSystem` hold a single
/// `ByteArray` (`uint data_size; byte data[data_size]`). The byte blob
/// contains the Havok-serialised physics tree (HKX-like). We store the
/// raw bytes so the physics bridge can hand them off to a Havok parser
/// later without re-parsing the outer NIF; reading them instead of
/// skipping keeps the block-sizes path intact across `block_size = 0`
/// or missing-size corner cases.
#[derive(Debug)]
pub struct BhkSystemBinary {
    pub type_name: &'static str,
    pub data: Vec<u8>,
}

impl NiObject for BhkSystemBinary {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkSystemBinary {
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        let data_size = stream.read_u32_le()? as usize;
        let data = stream.read_bytes(data_size)?;
        Ok(Self { type_name, data })
    }
}

// ── Rigid Body ──────────────────────────────────────────────────────

/// bhkRigidBody / bhkRigidBodyT — Havok rigid body with physics properties.
/// bhkRigidBodyT has active translation/rotation (same binary layout).
#[derive(Debug)]
pub struct BhkRigidBody {
    // bhkWorldObject
    pub shape_ref: BlockRef,
    pub havok_filter: u32,
    // Physics CInfo
    pub translation: [f32; 4],
    pub rotation: [f32; 4],
    pub linear_velocity: [f32; 4],
    pub angular_velocity: [f32; 4],
    pub inertia_tensor: [f32; 12],
    pub center_of_mass: [f32; 4],
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
    pub restitution: f32,
    pub max_linear_velocity: f32,
    pub max_angular_velocity: f32,
    pub penetration_depth: f32,
    pub motion_type: u8,
    pub deactivator_type: u8,
    pub solver_deactivation: u8,
    pub quality_type: u8,
    pub constraint_refs: Vec<BlockRef>,
    pub body_flags: u32,
}

impl NiObject for BhkRigidBody {
    fn block_type_name(&self) -> &'static str {
        "bhkRigidBody"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkRigidBody {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let bsver = stream.bsver();

        // bhkWorldObject: shape ref + havok filter + world object CInfo
        let shape_ref = stream.read_block_ref()?;
        let havok_filter = stream.read_u32_le()?;
        // bhkWorldObjectCInfo: 4 unused + broadphase(1) + 3 unused + 3 property u32s = 20 bytes
        stream.skip(20)?;

        // bhkEntityCInfo: response(1) + unused(1) + callback_delay(2) = 4 bytes
        stream.skip(4)?;

        if bsver <= 34 {
            // bhkRigidBodyCInfo550_660 (Oblivion / FO3 / FNV)
            // Duplicated filter + entity CInfo (since 10.1.0.0).
            // Prefix layout (nif.xml line 2808):
            //   Unused 01[4] + HavokFilter(u32) + Unused 02[4] +
            //   Collision Response(u8) + Unused 03(u8) +
            //   Process Contact Callback Delay(u16) + Unused 04[4] = 20 B.
            stream.skip(4)?; // unused
            let _cinfo_filter = stream.read_u32_le()?;
            stream.skip(4)?; // unused
            stream.skip(4)?; // response + unused + callback_delay
            stream.skip(4)?; // unused
        } else if bsver < 130 {
            // bhkRigidBodyCInfo2010 (Skyrim LE / SE — bsver 83-127).
            // Pre-#546 this prefix was missing entirely and the parser
            // walked straight into `Translation` from 20 bytes early,
            // trashing every subsequent field. All 12,866 vanilla
            // Skyrim SE bhkRigidBody blocks fell into NiUnknown.
            //
            // Prefix layout (nif.xml line 2844):
            //   Unused 01[4] + HavokFilter(u32) + Unused 02[4] +
            //   Unknown Int 1(u32) + Collision Response(u8) +
            //   Unused 03(u8) + Process Contact Callback Delay(u16)
            //   = 20 B. Semantically distinct from 550_660 (Unknown Int 1
            //   replaces 550_660's trailing Unused 04) but same wire size.
            stream.skip(4)?; // Unused 01
            let _cinfo_filter = stream.read_u32_le()?; // duplicated havok filter
            stream.skip(4)?; // Unused 02
            let _unknown_int_1 = stream.read_u32_le()?;
            stream.skip(4)?; // response + unused + callback_delay
        }
        // bsver >= 130 (FO4+): bhkRigidBodyCInfo2014 has a very different
        // layout — motion system / deactivator / quality / penetration
        // depth / time factor are interleaved with callback delay. That
        // path is knowingly incomplete and is tracked separately; we
        // preserve the pre-#546 behaviour of reading straight into
        // Translation here so FO4 doesn't newly regress.

        let translation = read_vec4(stream)?;
        let rotation = read_vec4(stream)?;
        let linear_velocity = read_vec4(stream)?;
        let angular_velocity = read_vec4(stream)?;
        let inertia_tensor = read_matrix3(stream)?;
        let center_of_mass = read_vec4(stream)?;
        let mass = stream.read_f32_le()?;
        let linear_damping = stream.read_f32_le()?;
        let angular_damping = stream.read_f32_le()?;

        if bsver >= 83 {
            // Skyrim+: timeFactor, gravityFactor before friction
            let _time_factor = stream.read_f32_le()?;
            let _gravity_factor = stream.read_f32_le()?;
        }

        let friction = stream.read_f32_le()?;

        if bsver >= 83 {
            let _rolling_friction = stream.read_f32_le()?;
        }

        let restitution = stream.read_f32_le()?;

        let (max_linear_velocity, max_angular_velocity, penetration_depth) = if bsver <= 34 {
            // Oblivion/FO3: max velocities + penetration depth (since 10.1.0.0)
            (
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            )
        } else {
            // Skyrim+: max velocities + penetration depth in different order
            let mlv = stream.read_f32_le()?;
            let mav = stream.read_f32_le()?;
            let pd = stream.read_f32_le()?;
            (mlv, mav, pd)
        };

        let motion_type = stream.read_u8()?;
        // Deactivator Type is present on *every* CInfo per nif.xml — the
        // prior "Skyrim+: removed, hardcoded 0" branch was one of the
        // three root causes of #546. Only the FO4+ CInfo2014 reorders it
        // (and still carries it), so we read it unconditionally here.
        let deactivator_type = stream.read_u8()?;
        let solver_deactivation = stream.read_u8()?;
        let quality_type = stream.read_u8()?;

        if bsver <= 34 {
            // Oblivion/FO3/FNV (CInfo550_660): Unused 05[12] padding.
            stream.skip(12)?;
        } else if bsver < 130 {
            // Skyrim LE/SE (CInfo2010): AutoRemoveLevel(1) +
            // ResponseModifierFlags(1) + NumShapeKeysInContactPoint(1) +
            // ForceCollidedOntoPPU(bool,1) + Unused 04[12] = 16 B.
            // Pre-#546 this skipped only 4 — the 12-byte Unused 04 trailer
            // was consumed by the next block's reads, drifting the stream.
            stream.skip(16)?;
        } else {
            // FO4+ (CInfo2014): different layout — see comment in prefix
            // block above. Preserve pre-#546 4-byte skip to avoid
            // introducing a new regression.
            stream.skip(4)?;
        }

        // Constraint refs
        let num_constraints = stream.read_u32_le()?;
        let mut constraint_refs: Vec<BlockRef> = stream.allocate_vec(num_constraints)?;
        for _ in 0..num_constraints {
            constraint_refs.push(stream.read_block_ref()?);
        }

        // Body flags: u32 in pre-Skyrim, u16 in Skyrim+ per nif.xml
        // (`#SKY_AND_LATER#` resolves to BSVER >= 76 in the niftools
        // schema). No Bethesda title ships in the BSVER 76..=82 gap,
        // so the cutoff is structurally invisible to vanilla content
        // — but the parser doctrine pins to nif.xml's threshold and
        // the previous `bsver < 83` value contradicted that without
        // shipping cause. Boundary tests cover bsver=75 (u32 path)
        // and bsver=76 (u16 path) at the bottom of this file. See
        // NIF-D2-NEW-05 (audit 2026-05-12), original landing #127.
        let body_flags = if bsver < 76 {
            stream.read_u32_le()?
        } else {
            stream.read_u16_le()? as u32
        };

        Ok(Self {
            shape_ref,
            havok_filter,
            translation,
            rotation,
            linear_velocity,
            angular_velocity,
            inertia_tensor,
            center_of_mass,
            mass,
            linear_damping,
            angular_damping,
            friction,
            restitution,
            max_linear_velocity,
            max_angular_velocity,
            penetration_depth,
            motion_type,
            deactivator_type,
            solver_deactivation,
            quality_type,
            constraint_refs,
            body_flags,
        })
    }
}

// ── Primitive Shapes ────────────────────────────────────────────────

/// Read a `HavokMaterial` from the stream — the per-shape material
/// FormID-equivalent on every `bhk*Shape` derived from
/// `bhkSphereRepShape` / `bhkConvexShape`.
///
/// Per nif.xml lines 2293-2299, the `HavokMaterial` struct has an
/// `Unknown Int` (uint) prefix gated `until="10.0.1.2"` followed by the
/// material `Material` u32. Pre-Bethesda NIFs (no Bethesda title ships
/// in that band — vanilla Oblivion is 20.0.0.5, FO3+ is later still)
/// carry the extra 4-byte prefix; modern NIFs drop it. Pre-fix every
/// `bhk*Shape::parse` site read just the final u32 unconditionally,
/// so a pre-Bethesda NIF would attribute the legacy padding bytes to
/// `material` and then misalign every subsequent field by 4 bytes.
/// See #723 / NIF-D1-03.
///
/// The `until=` boundary is inclusive per the version.rs doctrine —
/// at v <= 10.0.1.2 the legacy prefix is read.
fn read_havok_material(stream: &mut NifStream) -> io::Result<u32> {
    if stream.version() <= NifVersion(0x0A000102) {
        let _unknown_int = stream.read_u32_le()?;
    }
    stream.read_u32_le()
}

/// bhkSphereShape — sphere collision primitive.
#[derive(Debug)]
pub struct BhkSphereShape {
    pub material: u32,
    pub radius: f32,
}

impl NiObject for BhkSphereShape {
    fn block_type_name(&self) -> &'static str {
        "bhkSphereShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkSphereShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let material = read_havok_material(stream)?; // bhkSphereRepShape
        let radius = stream.read_f32_le()?; // bhkConvexShape
        Ok(Self { material, radius })
    }
}

/// `bhkMultiSphereShape` — compound Havok shape made of up to 8
/// spheres. nif.xml line 3124. Inherits `bhkSphereRepShape` (which
/// supplies `Material`) then adds a `bhkWorldObjCInfoProperty` block
/// and a `NiBound`-per-sphere array. Appears in creature skeleton /
/// ragdoll NIFs across all Bethesda games; was a MEDIUM-count block
/// on Oblivion where no `block_sizes` table exists to skip it.
/// See audit OBL-D5-H2 / #394.
///
/// Byte layout on Oblivion (20.0.0.5; HavokMaterial drops its pre-
/// 10.0.1.2 `Unknown Int` prefix): material(4) +
/// bhkWorldObjCInfoProperty(12) + num_spheres(4) + N × (Vector3 + f32)
/// = 20 + 16 × N bytes.
#[derive(Debug)]
pub struct BhkMultiSphereShape {
    pub material: u32,
    /// Bethesda's `bhkWorldObjCInfoProperty` — 3 × u32 opaque CInfo
    /// bookkeeping. Preserved verbatim for future physics bridges.
    pub shape_property: [u32; 3],
    /// Up to 8 spheres making up the collision approximation. Each is
    /// (center_x, center_y, center_z, radius).
    pub spheres: Vec<[f32; 4]>,
}

impl NiObject for BhkMultiSphereShape {
    fn block_type_name(&self) -> &'static str {
        "bhkMultiSphereShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkMultiSphereShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let material = read_havok_material(stream)?;
        let shape_property = [
            stream.read_u32_le()?,
            stream.read_u32_le()?,
            stream.read_u32_le()?,
        ];
        let num_spheres = stream.read_u32_le()?;
        let mut spheres = stream.allocate_vec::<[f32; 4]>(num_spheres)?;
        for _ in 0..num_spheres {
            let cx = stream.read_f32_le()?;
            let cy = stream.read_f32_le()?;
            let cz = stream.read_f32_le()?;
            let r = stream.read_f32_le()?;
            spheres.push([cx, cy, cz, r]);
        }
        Ok(Self {
            material,
            shape_property,
            spheres,
        })
    }
}

/// bhkBoxShape — axis-aligned box collision primitive.
#[derive(Debug)]
pub struct BhkBoxShape {
    pub material: u32,
    pub radius: f32,
    pub dimensions: [f32; 3],
}

impl NiObject for BhkBoxShape {
    fn block_type_name(&self) -> &'static str {
        "bhkBoxShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkBoxShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let material = read_havok_material(stream)?;
        let radius = stream.read_f32_le()?;
        stream.skip(8)?; // unused
        let dx = stream.read_f32_le()?;
        let dy = stream.read_f32_le()?;
        let dz = stream.read_f32_le()?;
        let _w = stream.read_f32_le()?; // padding
        Ok(Self {
            material,
            radius,
            dimensions: [dx, dy, dz],
        })
    }
}

/// bhkCapsuleShape — capsule (line segment + radius) collision primitive.
#[derive(Debug)]
pub struct BhkCapsuleShape {
    pub material: u32,
    pub radius: f32,
    pub point1: [f32; 3],
    pub radius1: f32,
    pub point2: [f32; 3],
    pub radius2: f32,
}

impl NiObject for BhkCapsuleShape {
    fn block_type_name(&self) -> &'static str {
        "bhkCapsuleShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkCapsuleShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let material = read_havok_material(stream)?;
        let radius = stream.read_f32_le()?;
        stream.skip(8)?; // unused
        let p1x = stream.read_f32_le()?;
        let p1y = stream.read_f32_le()?;
        let p1z = stream.read_f32_le()?;
        let radius1 = stream.read_f32_le()?;
        let p2x = stream.read_f32_le()?;
        let p2y = stream.read_f32_le()?;
        let p2z = stream.read_f32_le()?;
        let radius2 = stream.read_f32_le()?;
        Ok(Self {
            material,
            radius,
            point1: [p1x, p1y, p1z],
            radius1,
            point2: [p2x, p2y, p2z],
            radius2,
        })
    }
}

/// bhkCylinderShape — cylinder collision primitive.
#[derive(Debug)]
pub struct BhkCylinderShape {
    pub material: u32,
    pub radius: f32,
    pub point1: [f32; 4],
    pub point2: [f32; 4],
    pub cylinder_radius: f32,
}

impl NiObject for BhkCylinderShape {
    fn block_type_name(&self) -> &'static str {
        "bhkCylinderShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkCylinderShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let material = read_havok_material(stream)?;
        let radius = stream.read_f32_le()?;
        stream.skip(8)?; // unused
        let point1 = read_vec4(stream)?;
        let point2 = read_vec4(stream)?;
        let cylinder_radius = stream.read_f32_le()?;
        stream.skip(12)?; // unused padding
        Ok(Self {
            material,
            radius,
            point1,
            point2,
            cylinder_radius,
        })
    }
}

// ── Convex Hull ─────────────────────────────────────────────────────

/// bhkConvexVerticesShape — convex hull from vertex set.
#[derive(Debug)]
pub struct BhkConvexVerticesShape {
    pub material: u32,
    pub radius: f32,
    pub vertices: Vec<[f32; 4]>,
    pub normals: Vec<[f32; 4]>,
}

impl NiObject for BhkConvexVerticesShape {
    fn block_type_name(&self) -> &'static str {
        "bhkConvexVerticesShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkConvexVerticesShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let material = read_havok_material(stream)?;
        let radius = stream.read_f32_le()?;
        // Two bhkWorldObjCInfoProperty structs (12 bytes each)
        stream.skip(24)?;
        // #981 — bulk-read `[f32; 4]` arrays via `read_ni_color4_array`
        // (POD `[f32; 4]` is the same on-disk layout `read_vec4` used).
        let num_vertices = stream.read_u32_le()? as usize;
        let vertices = stream.read_ni_color4_array(num_vertices)?;
        let num_normals = stream.read_u32_le()? as usize;
        let normals = stream.read_ni_color4_array(num_normals)?;
        Ok(Self {
            material,
            radius,
            vertices,
            normals,
        })
    }
}

// ── Compound / Transform Shapes ─────────────────────────────────────

/// bhkListShape — compound shape with sub-shapes.
#[derive(Debug)]
pub struct BhkListShape {
    pub sub_shape_refs: Vec<BlockRef>,
    pub material: u32,
    pub filters: Vec<u32>,
}

impl NiObject for BhkListShape {
    fn block_type_name(&self) -> &'static str {
        "bhkListShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkListShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_sub_shapes = stream.read_u32_le()?;
        let mut sub_shape_refs = stream.allocate_vec(num_sub_shapes)?;
        for _ in 0..num_sub_shapes {
            sub_shape_refs.push(stream.read_block_ref()?);
        }
        let material = read_havok_material(stream)?;
        // Two bhkWorldObjCInfoProperty structs (12 bytes each)
        stream.skip(24)?;
        // #981 — bulk-read filter u32 array.
        let num_filters = stream.read_u32_le()? as usize;
        let filters = stream.read_u32_array(num_filters)?;
        Ok(Self {
            sub_shape_refs,
            material,
            filters,
        })
    }
}

/// bhkTransformShape / bhkConvexTransformShape — wraps a shape with a 4x4 transform.
#[derive(Debug)]
pub struct BhkTransformShape {
    pub shape_ref: BlockRef,
    pub material: u32,
    pub radius: f32,
    pub transform: [[f32; 4]; 4],
}

impl NiObject for BhkTransformShape {
    fn block_type_name(&self) -> &'static str {
        "bhkTransformShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkTransformShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let shape_ref = stream.read_block_ref()?;
        let material = read_havok_material(stream)?;
        let radius = stream.read_f32_le()?;
        stream.skip(8)?; // unused
        let mut transform = [[0.0f32; 4]; 4];
        for row in &mut transform {
            for val in row.iter_mut() {
                *val = stream.read_f32_le()?;
            }
        }
        Ok(Self {
            shape_ref,
            material,
            radius,
            transform,
        })
    }
}

// ── BV Tree ─────────────────────────────────────────────────────────

/// bhkMoppBvTreeShape — MOPP BVH wrapping a shape collection.
/// MOPP bytecode is stored opaquely (Rapier builds its own BVH).
#[derive(Debug)]
pub struct BhkMoppBvTreeShape {
    pub shape_ref: BlockRef,
    pub mopp_data: Vec<u8>,
    pub origin: [f32; 4],
    pub scale: f32,
}

impl NiObject for BhkMoppBvTreeShape {
    fn block_type_name(&self) -> &'static str {
        "bhkMoppBvTreeShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkMoppBvTreeShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let shape_ref = stream.read_block_ref()?; // bhkBvTreeShape
        stream.skip(12)?; // unused
        let scale = stream.read_f32_le()?;
        let data_size = stream.read_u32_le()? as usize;
        let origin = read_vec4(stream)?; // since 10.1.0.0 (always present)
                                         // Build Type: only for BSVER > 34 (Skyrim+; FO3/FNV is 34)
        if stream.bsver() > 34 {
            let _build_type = stream.read_u8()?;
        }
        let mopp_data = stream.read_bytes(data_size)?;
        Ok(Self {
            shape_ref,
            mopp_data,
            origin,
            scale,
        })
    }
}

// ── Mesh Shapes ─────────────────────────────────────────────────────

/// bhkNiTriStripsShape — collision mesh referencing NiTriStripsData blocks.
#[derive(Debug)]
pub struct BhkNiTriStripsShape {
    pub material: u32,
    pub radius: f32,
    pub data_refs: Vec<BlockRef>,
    pub filters: Vec<u32>,
}

impl NiObject for BhkNiTriStripsShape {
    fn block_type_name(&self) -> &'static str {
        "bhkNiTriStripsShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkNiTriStripsShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let material = read_havok_material(stream)?;
        let radius = stream.read_f32_le()?;
        stream.skip(20)?; // unused
        let _grow_by = stream.read_u32_le()?;
        // Scale: since 10.1.0.0 (always present for Oblivion+)
        let _scale = read_vec4(stream)?;
        let num_data = stream.read_u32_le()?;
        let mut data_refs = stream.allocate_vec(num_data)?;
        for _ in 0..num_data {
            data_refs.push(stream.read_block_ref()?);
        }
        // #981 — bulk-read filter u32 array.
        let num_filters = stream.read_u32_le()? as usize;
        let filters = stream.read_u32_array(num_filters)?;
        Ok(Self {
            material,
            radius,
            data_refs,
            filters,
        })
    }
}

/// bhkPackedNiTriStripsShape — packed triangle mesh with sub-shapes.
#[derive(Debug)]
pub struct BhkPackedNiTriStripsShape {
    pub sub_shapes: Vec<HkSubPartData>,
    pub data_ref: BlockRef,
    pub scale: [f32; 4],
}

/// Sub-shape info for packed tri strips.
#[derive(Debug)]
pub struct HkSubPartData {
    pub havok_filter: u32,
    pub num_vertices: u32,
    pub material: u32,
}

impl NiObject for BhkPackedNiTriStripsShape {
    fn block_type_name(&self) -> &'static str {
        "bhkPackedNiTriStripsShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkPackedNiTriStripsShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let version = stream.version();
        let sub_shapes = if version <= crate::version::NifVersion::V20_0_0_5 {
            // Oblivion: sub-shapes inline (until="20.0.0.5")
            let count = stream.read_u16_le()? as u32;
            let mut subs = stream.allocate_vec(count)?;
            for _ in 0..count {
                let havok_filter = stream.read_u32_le()?;
                let num_vertices = stream.read_u32_le()?;
                let material = stream.read_u32_le()?;
                subs.push(HkSubPartData {
                    havok_filter,
                    num_vertices,
                    material,
                });
            }
            subs
        } else {
            // FO3+: sub-shapes are in hkPackedNiTriStripsData
            Vec::new()
        };

        let _user_data = stream.read_u32_le()?;
        stream.skip(4)?; // unused
        let _radius = stream.read_f32_le()?;
        stream.skip(4)?; // unused
        let scale = read_vec4(stream)?;
        let _radius_copy = stream.read_f32_le()?;
        let _scale_copy = read_vec4(stream)?;
        let data_ref = stream.read_block_ref()?;

        Ok(Self {
            sub_shapes,
            data_ref,
            scale,
        })
    }
}

/// hkPackedNiTriStripsData — packed triangle and vertex arrays.
#[derive(Debug)]
pub struct HkPackedNiTriStripsData {
    pub triangles: Vec<PackedTriangle>,
    pub vertices: Vec<[f32; 3]>,
}

/// A single triangle in packed collision data.
#[derive(Debug)]
pub struct PackedTriangle {
    pub v0: u16,
    pub v1: u16,
    pub v2: u16,
    pub welding_info: u16,
    pub normal: Option<[f32; 3]>,
}

impl NiObject for HkPackedNiTriStripsData {
    fn block_type_name(&self) -> &'static str {
        "hkPackedNiTriStripsData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl HkPackedNiTriStripsData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let version = stream.version();
        let num_triangles = stream.read_u32_le()?;
        let mut triangles = stream.allocate_vec(num_triangles)?;
        for _ in 0..num_triangles {
            let v0 = stream.read_u16_le()?;
            let v1 = stream.read_u16_le()?;
            let v2 = stream.read_u16_le()?;
            let welding_info = stream.read_u16_le()?;
            // Oblivion (until="20.0.0.5"): normal as 3 floats
            let normal = if version <= crate::version::NifVersion::V20_0_0_5 {
                let nx = stream.read_f32_le()?;
                let ny = stream.read_f32_le()?;
                let nz = stream.read_f32_le()?;
                Some([nx, ny, nz])
            } else {
                // FO3+ has i16 normal in the welding info (different encoding)
                None
            };
            triangles.push(PackedTriangle {
                v0,
                v1,
                v2,
                welding_info,
                normal,
            });
        }

        let num_vertices = stream.read_u32_le()?;
        // FO3+ (since 20.2.0.7) — nif.xml lines 3962-3967:
        //   `Compressed: bool` gates the trailing `Vertices` array
        //   between `Vector3[]` (12 B/vertex, IEEE f32) and
        //   `HalfVector3[]` (6 B/vertex, IEEE half-float). Vanilla
        //   Bethesda content ships `Compressed == 0`, but flipping
        //   the bit is legal — and if we always read f32 the per-
        //   vertex over-read scrambles the following `Num Sub Shapes`
        //   u16 and turns every collider vertex into NaN downstream.
        //   See issue #975 (NIF-D1-NEW-01).
        let compressed = if version >= crate::version::NifVersion::V20_2_0_7 {
            stream.read_byte_bool()?
        } else {
            false
        };
        let mut vertices: Vec<[f32; 3]> = stream.allocate_vec(num_vertices)?;
        for _ in 0..num_vertices {
            let (x, y, z) = if compressed {
                (
                    crate::blocks::tri_shape::half_to_f32(stream.read_u16_le()?),
                    crate::blocks::tri_shape::half_to_f32(stream.read_u16_le()?),
                    crate::blocks::tri_shape::half_to_f32(stream.read_u16_le()?),
                )
            } else {
                (
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                    stream.read_f32_le()?,
                )
            };
            vertices.push([x, y, z]);
        }

        // FO3+ (since 20.2.0.7): sub-shapes at the end
        if version >= crate::version::NifVersion::V20_2_0_7 {
            let num_sub_shapes = stream.read_u16_le()? as usize;
            for _ in 0..num_sub_shapes {
                stream.skip(12)?; // HkSubPartData: filter(4) + numVerts(4) + material(4)
            }
        }

        Ok(Self {
            triangles,
            vertices,
        })
    }
}

// ── Simple Phantom ──────────────────────────────────────────────────

/// bhkSimpleShapePhantom — non-physical trigger volume with shape + transform.
#[derive(Debug)]
pub struct BhkSimpleShapePhantom {
    pub shape_ref: BlockRef,
    pub havok_filter: u32,
    pub transform: [[f32; 4]; 4],
}

impl NiObject for BhkSimpleShapePhantom {
    fn block_type_name(&self) -> &'static str {
        "bhkSimpleShapePhantom"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkSimpleShapePhantom {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // bhkWorldObject: shape ref + filter + world CInfo (20 bytes)
        let shape_ref = stream.read_block_ref()?;
        let havok_filter = stream.read_u32_le()?;
        stream.skip(20)?; // bhkWorldObjectCInfo

        // bhkSimpleShapePhantom adds 8 unused bytes (nif.xml `Unused 01`,
        // `byte length="8" binary="true"`) before the transform. Pre-#474
        // this 8-byte slot was skipped along with the trailing transform
        // via block_sizes recovery, costing 8 bytes per phantom block.
        stream.skip(8)?;

        // bhkSimpleShapePhantom: 4x4 transform (Matrix44).
        let mut transform = [[0.0f32; 4]; 4];
        for row in &mut transform {
            for val in row.iter_mut() {
                *val = stream.read_f32_le()?;
            }
        }
        Ok(Self {
            shape_ref,
            havok_filter,
            transform,
        })
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn read_vec4(stream: &mut NifStream) -> io::Result<[f32; 4]> {
    Ok([
        stream.read_f32_le()?,
        stream.read_f32_le()?,
        stream.read_f32_le()?,
        stream.read_f32_le()?,
    ])
}

fn read_matrix3(stream: &mut NifStream) -> io::Result<[f32; 12]> {
    let mut m = [0.0f32; 12];
    for val in &mut m {
        *val = stream.read_f32_le()?;
    }
    Ok(m)
}

// ── bhkCompressedMeshShape (Skyrim+) ───────���───────────────────────

/// Thin wrapper pointing to bhkCompressedMeshShapeData.
#[derive(Debug)]
pub struct BhkCompressedMeshShape {
    pub target_ref: BlockRef,
    pub user_data: u32,
    pub radius: f32,
    pub scale: [f32; 4],
    pub data_ref: BlockRef,
}

impl NiObject for BhkCompressedMeshShape {
    fn block_type_name(&self) -> &'static str {
        "bhkCompressedMeshShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkCompressedMeshShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let target_ref = stream.read_block_ref()?;
        let user_data = stream.read_u32_le()?;
        let radius = stream.read_f32_le()?;
        let _unknown_float = stream.read_f32_le()?;
        let scale = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let _radius_copy = stream.read_f32_le()?;
        let _scale_copy = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let data_ref = stream.read_block_ref()?;
        Ok(Self {
            target_ref,
            user_data,
            radius,
            scale,
            data_ref,
        })
    }
}

// ── bhkCompressedMeshShapeData (Skyrim+) ────────────────────────────

/// A single quantized collision chunk within bhkCompressedMeshShapeData.
#[derive(Debug)]
pub struct CmsChunk {
    /// Chunk-local origin in Havok coordinates.
    pub translation: [f32; 4],
    pub material_index: u32,
    pub transform_index: u16,
    /// Quantized vertex positions: (x, y, z) as u16. Dequantize: translation + vertex/1000.
    pub vertices: Vec<[u16; 3]>,
    /// Triangle strip indices into the vertices array.
    pub indices: Vec<u16>,
    /// Strip lengths — if empty, indices are a plain triangle list.
    pub strips: Vec<u16>,
}

/// Full-precision triangle referencing BigVerts.
#[derive(Debug)]
pub struct CmsBigTri {
    pub v1: u16,
    pub v2: u16,
    pub v3: u16,
    pub material: u32,
}

/// Chunk transform (translation + rotation as quaternion).
#[derive(Debug)]
pub struct CmsTransform {
    pub translation: [f32; 4],
    pub rotation: [f32; 4],
}

/// Compressed mesh collision data — Skyrim's primary collision format.
#[derive(Debug)]
pub struct BhkCompressedMeshShapeData {
    pub bits_per_index: u32,
    pub bits_per_w_index: u32,
    pub mask_w_index: u32,
    pub mask_index: u32,
    pub error: f32,
    pub aabb_min: [f32; 4],
    pub aabb_max: [f32; 4],
    pub chunk_materials: Vec<[u32; 2]>,
    pub chunk_transforms: Vec<CmsTransform>,
    /// Full-precision vertices for oversized triangles.
    pub big_verts: Vec<[f32; 4]>,
    /// Oversized triangles indexing into big_verts.
    pub big_tris: Vec<CmsBigTri>,
    /// Quantized collision geometry chunks.
    pub chunks: Vec<CmsChunk>,
}

impl NiObject for BhkCompressedMeshShapeData {
    fn block_type_name(&self) -> &'static str {
        "bhkCompressedMeshShapeData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkCompressedMeshShapeData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let bits_per_index = stream.read_u32_le()?;
        let bits_per_w_index = stream.read_u32_le()?;
        let mask_w_index = stream.read_u32_le()?;
        let mask_index = stream.read_u32_le()?;
        let error = stream.read_f32_le()?;

        // AABB
        let aabb_min = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let aabb_max = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];

        let _welding_type = stream.read_u8()?;
        let _material_type = stream.read_u8()?;

        // Material arrays (unused but must be consumed)
        let num_mat32 = stream.read_u32_le()? as usize;
        stream.skip(num_mat32 as u64 * 4)?;
        let num_mat16 = stream.read_u32_le()? as usize;
        stream.skip(num_mat16 as u64 * 4)?;
        let num_mat8 = stream.read_u32_le()? as usize;
        stream.skip(num_mat8 as u64 * 4)?;

        // Chunk materials: (SkyrimHavokMaterial, HavokFilter) = 2×u32 = 8 bytes each
        let num_chunk_materials = stream.read_u32_le()?;
        let mut chunk_materials = stream.allocate_vec(num_chunk_materials)?;
        for _ in 0..num_chunk_materials {
            chunk_materials.push([stream.read_u32_le()?, stream.read_u32_le()?]);
        }

        let _num_named_materials = stream.read_u32_le()?;

        // Chunk transforms
        let num_transforms = stream.read_u32_le()?;
        let mut chunk_transforms = stream.allocate_vec(num_transforms)?;
        for _ in 0..num_transforms {
            let translation = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let rotation = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            chunk_transforms.push(CmsTransform {
                translation,
                rotation,
            });
        }

        // Big verts (full-precision)
        let num_big_verts = stream.read_u32_le()?;
        let mut big_verts = stream.allocate_vec(num_big_verts)?;
        for _ in 0..num_big_verts {
            big_verts.push([
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ]);
        }

        // Big tris
        let num_big_tris = stream.read_u32_le()?;
        let mut big_tris = stream.allocate_vec(num_big_tris)?;
        for _ in 0..num_big_tris {
            let v1 = stream.read_u16_le()?;
            let v2 = stream.read_u16_le()?;
            let v3 = stream.read_u16_le()?;
            let material = stream.read_u32_le()?;
            let _welding_info = stream.read_u16_le()?;
            big_tris.push(CmsBigTri {
                v1,
                v2,
                v3,
                material,
            });
        }

        // Chunks (variable-size)
        let num_chunks = stream.read_u32_le()?;
        let mut chunks = stream.allocate_vec(num_chunks)?;
        for _ in 0..num_chunks {
            let translation = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            let material_index = stream.read_u32_le()?;
            let _reference = stream.read_u16_le()?;
            let transform_index = stream.read_u16_le()?;

            // Vertices: nif.xml Num Vertices is the count of u16 values (not triples).
            // Divide by 3 to get the number of (x, y, z) vertex positions.
            // Confirmed via Havok source: Chunk::m_vertices is hkArray<hkUint16>,
            // count = actual_vertices * 3. #981 — bulk reads via
            // `read_u16_triple_array` / `read_u16_array`.
            let num_vertex_components = stream.read_u32_le()?;
            let num_vertices = (num_vertex_components / 3) as usize;
            let vertices = stream.read_u16_triple_array(num_vertices)?;

            // Indices
            let num_indices = stream.read_u32_le()? as usize;
            let indices = stream.read_u16_array(num_indices)?;

            // Strips
            let num_strips = stream.read_u32_le()? as usize;
            let strips = stream.read_u16_array(num_strips)?;

            // Welding info
            let num_welding = stream.read_u32_le()? as usize;
            stream.skip(num_welding as u64 * 2)?;

            chunks.push(CmsChunk {
                translation,
                material_index,
                transform_index,
                vertices,
                indices,
                strips,
            });
        }

        // Num convex piece A (unused)
        let _num_convex_piece_a = stream.read_u32_le()?;

        Ok(Self {
            bits_per_index,
            bits_per_w_index,
            mask_w_index,
            mask_index,
            error,
            aabb_min,
            aabb_max,
            chunk_materials,
            chunk_transforms,
            big_verts,
            big_tris,
            chunks,
        })
    }
}

// ── Havok constraint stubs (#117) ──────────────────────────────────────
//
// Minimal parsers for the 7 Havok constraint types
// (bhkBallAndSocket / bhkHinge / bhkLimitedHinge / bhkRagdoll /
// bhkPrismatic / bhkStiffSpring / bhkMalleable). They capture the
// shared `bhkConstraintCInfo` base — entity refs + priority — and
// then skip the type-specific CInfo payload by its known byte size.
// The physics system (M28) will eventually parse the full CInfo
// structs; for now we just need enough to advance the stream past
// these blocks so they stop cascading the parse loop on Oblivion.
//
// **Oblivion path** (`version <= 20.0.0.5`, aka the `#NI_BS_LTE_16#`
// branch in nif.xml): sizes are hand-computed from nif.xml and must
// be byte-exact because Oblivion NIFs have no `block_sizes` table
// and any drift takes down every subsequent block.
//
// **FO3+ path** (`version >= 20.2.0.7`): layouts add Motor blobs and
// extra vectors — several of them are variable-size. Rather than
// duplicate the full layout here, the stub reads only the 16-byte
// base and relies on the outer `parse_nif` loop's `block_size`
// reconciliation to seek past the remainder. Zero risk: on FO3+ the
// header always has a `block_sizes` table, so the recovery path is
// guaranteed to run. This keeps the constraint code short without
// sacrificing parse completeness.

/// Opaque stub for a Havok constraint block.
///
/// Holds just the shared `bhkConstraintCInfo` base (two entity refs
/// + priority); everything else is skipped. The concrete constraint
/// type is preserved in `type_name` so downstream consumers and
/// telemetry can identify it. See #117.
#[derive(Debug)]
pub struct BhkConstraint {
    /// RTTI class name — one of the seven constraint types.
    pub type_name: &'static str,
    pub entity_a: BlockRef,
    pub entity_b: BlockRef,
    pub priority: u32,
}

impl NiObject for BhkConstraint {
    fn block_type_name(&self) -> &'static str {
        self.type_name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkConstraint {
    /// Read the shared `bhkConstraintCInfo` prefix — 16 bytes:
    /// `num_entities u32 + entity_a i32 + entity_b i32 + priority u32`.
    /// Returns `(entity_a, entity_b, priority)`.
    fn parse_base(stream: &mut NifStream) -> io::Result<(BlockRef, BlockRef, u32)> {
        let _num_entities = stream.read_u32_le()?;
        let entity_a = stream.read_block_ref()?;
        let entity_b = stream.read_block_ref()?;
        let priority = stream.read_u32_le()?;
        Ok((entity_a, entity_b, priority))
    }

    /// Parse a constraint block by type name. On Oblivion, reads the
    /// exact byte layout and returns a `BhkConstraint`. On FO3+, reads
    /// the 16-byte base and returns early; the caller seeks past the
    /// remainder via `block_size`.
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        let (entity_a, entity_b, priority) = Self::parse_base(stream)?;

        // Oblivion byte-exact payload sizes (post-base bytes). Derived
        // from nif.xml with `#NI_BS_LTE_16#` active. A zero means
        // "drop through to the FO3+ short-stub path".
        let is_oblivion = stream.version() <= NifVersion::V20_0_0_5;
        if is_oblivion {
            let payload_size: Option<u64> = match type_name {
                // 2 × Vec4
                "bhkBallAndSocketConstraint" => Some(32),
                // 5 × Vec4
                "bhkHingeConstraint" => Some(80),
                // 6 × Vec4 + 6 × f32
                "bhkRagdollConstraint" => Some(120),
                // 7 × Vec4 + 3 × f32
                "bhkLimitedHingeConstraint" => Some(124),
                // 8 × Vec4 + 3 × f32
                "bhkPrismaticConstraint" => Some(140),
                // 2 × Vec4 + f32
                "bhkStiffSpringConstraint" => Some(36),
                // Malleable wrapper has a runtime-dispatched inner
                // CInfo — handle separately below.
                "bhkMalleableConstraint" => None,
                _ => None,
            };

            if let Some(size) = payload_size {
                stream.skip(size)?;
                return Ok(Self {
                    type_name,
                    entity_a,
                    entity_b,
                    priority,
                });
            }

            if type_name == "bhkMalleableConstraint" {
                // Oblivion layout: type u32 + nested bhkConstraintCInfo
                // (16) + wrapped CInfo + tau f32 + damping f32.
                let wrapped_type = stream.read_u32_le()?;
                let _nested_entities = stream.read_u32_le()?;
                let _nested_a = stream.read_block_ref()?;
                let _nested_b = stream.read_block_ref()?;
                let _nested_priority = stream.read_u32_le()?;
                let inner_size: u64 = match wrapped_type {
                    0 => 32,  // Ball and Socket
                    1 => 80,  // Hinge
                    2 => 124, // Limited Hinge
                    6 => 140, // Prismatic
                    7 => 120, // Ragdoll
                    8 => 36,  // Stiff Spring
                    other => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "bhkMalleableConstraint: unknown inner type {other} — \
                                 stream position unreliable"
                            ),
                        ));
                    }
                };
                stream.skip(inner_size)?;
                // Tau + Damping (Oblivion trailer).
                stream.skip(8)?;
                return Ok(Self {
                    type_name,
                    entity_a,
                    entity_b,
                    priority,
                });
            }
        }

        // FO3+ (or unknown pre-Oblivion content): return after the
        // 16-byte base. The outer parse_nif loop seeks past the rest
        // using the header's block_sizes table, which is always present
        // on v >= 20.2.0.7. The stub still preserves the RTTI name
        // for telemetry.
        Ok(Self {
            type_name,
            entity_a,
            entity_b,
            priority,
        })
    }
}

// ── Havok tail types (#557 / NIF-12) ────────────────────────────────
//
// Six leaf types that appeared in `NiUnknown` buckets across all four
// Bethesda games before they were registered. Each is a thin parser
// with nif.xml-derived byte counts — the block_sizes recovery covers
// FO3+ / Skyrim / FO4, but Oblivion (v20.0.0.5, no block_sizes) needs
// every byte read exactly or the outer walker cascades off the rails.

/// `bhkAabbPhantom` — non-physical broad-phase-only phantom (trigger
/// volumes, region queries). nif.xml line 2778.
///
/// Inheritance chain: bhkAabbPhantom → bhkPhantom → bhkWorldObject.
/// Layout on disk (Bethesda, 20.0.0.5 and later, no `Unknown Int`
/// since the `until 10.0.1.2` gate excludes every Bethesda game):
/// ```text
///   bhkWorldObject     : shape_ref(4) + havok_filter(4) + CInfo(20) = 28 B
///   (bhkPhantom adds nothing)
///   bhkAabbPhantom     : Unused 01[8] + hkAabb (2 × Vec4 = 32) = 40 B
///   --------------------------------------------------------
///   Total                                                  = 68 B
/// ```
#[derive(Debug)]
pub struct BhkAabbPhantom {
    pub shape_ref: BlockRef,
    pub havok_filter: u32,
    /// World-space AABB min corner (x, y, z, w) — w unused per hkAabb.
    pub aabb_min: [f32; 4],
    /// World-space AABB max corner (x, y, z, w) — w unused per hkAabb.
    pub aabb_max: [f32; 4],
}

impl NiObject for BhkAabbPhantom {
    fn block_type_name(&self) -> &'static str {
        "bhkAabbPhantom"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkAabbPhantom {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // bhkWorldObject prefix (28 B): shape + filter + bhkWorldObjectCInfo.
        let shape_ref = stream.read_block_ref()?;
        let havok_filter = stream.read_u32_le()?;
        stream.skip(20)?; // bhkWorldObjectCInfo
                          // bhkAabbPhantom: 8 unused + 2 × Vec4 hkAabb.
        stream.skip(8)?;
        let aabb_min = read_vec4(stream)?;
        let aabb_max = read_vec4(stream)?;
        Ok(Self {
            shape_ref,
            havok_filter,
            aabb_min,
            aabb_max,
        })
    }
}

/// `bhkLiquidAction` — FO3+ custom `bhkUnaryAction`-flavoured Havok
/// action that applies surface-tension forces to a body of liquid.
/// nif.xml line 6893.
///
/// The base class is `bhkAction` (abstract, no fields), so the on-disk
/// body starts immediately. Layout:
/// ```text
///   Unused 01[12] + Initial Stick Force(f32) + Stick Strength(f32)
///   + Neighbor Distance(f32) + Neighbor Strength(f32) = 28 B
/// ```
/// The `Unused 01` slot explicitly differs from a `bhkUnaryAction` —
/// per nif.xml, `bhkLiquidAction` does NOT carry the Entity Ptr even
/// though the class heritage looks like it should.
#[derive(Debug)]
pub struct BhkLiquidAction {
    pub initial_stick_force: f32,
    pub stick_strength: f32,
    pub neighbor_distance: f32,
    pub neighbor_strength: f32,
}

impl NiObject for BhkLiquidAction {
    fn block_type_name(&self) -> &'static str {
        "bhkLiquidAction"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkLiquidAction {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        stream.skip(12)?; // Unused 01
        let initial_stick_force = stream.read_f32_le()?;
        let stick_strength = stream.read_f32_le()?;
        let neighbor_distance = stream.read_f32_le()?;
        let neighbor_strength = stream.read_f32_le()?;
        Ok(Self {
            initial_stick_force,
            stick_strength,
            neighbor_distance,
            neighbor_strength,
        })
    }
}

/// `bhkPCollisionObject` — Havok collision object wrapping a phantom
/// (typically `bhkAabbPhantom`) rather than a rigid body. Concrete
/// subclass of `bhkNiCollisionObject`. nif.xml line 3432.
///
/// Wire layout is byte-identical to the standard `bhkCollisionObject`
/// (target_ref + flags u16 + body Ref, 10 B total); the only runtime
/// difference is that `body` references a `bhkPhantom` subclass
/// instead of a `bhkEntity`. We expose it as its own struct so
/// consumers can pattern-match on "this is a phantom, not a body."
#[derive(Debug)]
pub struct BhkPCollisionObject {
    pub target_ref: BlockRef,
    pub flags: u16,
    /// Reference to the `bhkPhantom` subclass (e.g. `bhkAabbPhantom`,
    /// `bhkSimpleShapePhantom`) that supplies the collision volume.
    pub body_ref: BlockRef,
}

impl NiObject for BhkPCollisionObject {
    fn block_type_name(&self) -> &'static str {
        "bhkPCollisionObject"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkPCollisionObject {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let target_ref = stream.read_block_ref()?;
        let flags = stream.read_u16_le()?;
        let body_ref = stream.read_block_ref()?;
        Ok(Self {
            target_ref,
            flags,
            body_ref,
        })
    }
}

/// `bhkConvexListShape` — FO3-only composite shape holding N convex
/// sub-shapes with a shared padding radius. nif.xml line 6835.
/// Constraints enforced by the Havok engine (not by this parser):
/// sub-shapes must all be convex and share the same `Radius` value.
///
/// Layout:
/// ```text
///   num_sub_shapes(u32) + sub_shapes[Ref × N]
///   + HavokMaterial(4, since FO3 strips the pre-10.0.1.2 Unknown Int)
///   + radius(f32) + unknown_int_1(u32) + unknown_float_1(f32)
///   + child_shape_property(12)  [bhkWorldObjCInfoProperty]
///   + use_cached_aabb(u8) + closest_point_min_distance(f32)
///   = 37 + 4×N bytes
/// ```
#[derive(Debug)]
pub struct BhkConvexListShape {
    pub sub_shapes: Vec<BlockRef>,
    pub material: u32,
    pub radius: f32,
    pub use_cached_aabb: bool,
    pub closest_point_min_distance: f32,
}

impl NiObject for BhkConvexListShape {
    fn block_type_name(&self) -> &'static str {
        "bhkConvexListShape"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkConvexListShape {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_sub_shapes = stream.read_u32_le()?;
        let mut sub_shapes = stream.allocate_vec::<BlockRef>(num_sub_shapes)?;
        for _ in 0..num_sub_shapes {
            sub_shapes.push(stream.read_block_ref()?);
        }
        let material = stream.read_u32_le()?;
        let radius = stream.read_f32_le()?;
        let _unknown_int_1 = stream.read_u32_le()?;
        let _unknown_float_1 = stream.read_f32_le()?;
        stream.skip(12)?; // bhkWorldObjCInfoProperty (3 × u32)
        let use_cached_aabb = stream.read_u8()? != 0;
        let closest_point_min_distance = stream.read_f32_le()?;
        Ok(Self {
            sub_shapes,
            material,
            radius,
            use_cached_aabb,
            closest_point_min_distance,
        })
    }
}

/// `bhkBreakableConstraint` — wrapper around another constraint that
/// can "break" (stop applying force) once a force threshold is
/// exceeded. nif.xml line 7027.
///
/// Byte-accurate parse is critical on Oblivion (no block_sizes
/// recovery). The wrapped payload size depends on the inner
/// `hkConstraintType` enum, which maps identically to the sizes
/// [`BhkConstraint::parse`] hard-codes; we reuse that same table here.
/// On FO3+ the outer walker seeks via `block_size` if the inner type
/// is one we haven't sized (e.g. `Malleable`, which carries nested
/// CInfo dispatch).
#[derive(Debug)]
pub struct BhkBreakableConstraint {
    /// Outer `bhkConstraintCInfo` — the two entities this wrapper
    /// constrains.
    pub entity_a: BlockRef,
    pub entity_b: BlockRef,
    pub priority: u32,
    /// `hkConstraintType` enum value identifying the inner data
    /// layout (0 = Ball and Socket, 1 = Hinge, …, 13 = Malleable).
    pub wrapped_type: u32,
    /// Force magnitude above which the constraint releases.
    pub threshold: f32,
    /// When `true`, the constraint is destroyed once the threshold
    /// is hit; when `false`, it stops applying force but stays
    /// present so the game can re-enable it.
    pub remove_when_broken: bool,
}

impl NiObject for BhkBreakableConstraint {
    fn block_type_name(&self) -> &'static str {
        "bhkBreakableConstraint"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkBreakableConstraint {
    /// Payload size (in bytes, past the 16-byte outer bhkConstraintCInfo
    /// and 4-byte wrapped type discriminator) for the wrapped CInfo,
    /// keyed on both the wrapped-type discriminator AND the parser's
    /// version branch. `None` means "wrapped-payload size depends on a
    /// runtime-dispatched motor type byte; rely on `block_size`
    /// recovery for the trailer."
    ///
    /// nif.xml sizes per `#NI_BS_LTE_16#` (Oblivion until 20.0.0.5) vs
    /// `!#NI_BS_LTE_16#` (FNV/FO3+ since 20.2.0.7):
    ///
    /// | type | Oblivion | FNV | Notes |
    /// |-----:|---------:|----:|-------|
    /// | 0 BallAndSocket | 32 | 32 | size attr on struct, no version diff |
    /// | 1 Hinge | 80 | **128** | FNV adds Axis A + Perp Axis In B1 + Pivot A (3 × Vec4 = +48) |
    /// | 2 LimitedHinge | 124 | None | FNV adds Perp Axis In B1 (Vec4) + variable Motor |
    /// | 6 Prismatic | 140 | None | FNV adds variable Motor |
    /// | 7 Ragdoll | 120 | None | FNV adds Motor A + Motor B (2 × Vec4) + variable Motor |
    /// | 8 StiffSpring | 36 | 36 | size attr on struct, no version diff |
    /// | 13 Malleable | None | None | nested-CInfo dispatch — outside this table |
    ///
    /// Pre-#633 the table was Oblivion-only and only consulted on the
    /// Oblivion branch — so FNV constraint blocks fell into the FO3+
    /// short-stub and silently zeroed `threshold` / `remove_when_broken`.
    /// Post-fix, the FNV-derivable rows let the parser fully consume
    /// the wrapped payload and read the trailer fields. Motor-bearing
    /// FNV constraints still rely on `block_size` recovery — no
    /// regression vs the old behaviour, just a wider correct path.
    fn wrapped_payload_size(wrapped_type: u32, is_oblivion: bool) -> Option<u64> {
        match (wrapped_type, is_oblivion) {
            // BallAndSocket — 2 × Vec4 = 32 B regardless of version.
            (0, _) => Some(32),
            // Hinge — Oblivion 5 × Vec4, FNV 8 × Vec4.
            (1, true) => Some(80),
            (1, false) => Some(128),
            // LimitedHinge — Oblivion 7 × Vec4 + 3 × f32 = 124. FNV adds
            // 1 × Vec4 + variable Motor; size depends on motor type.
            (2, true) => Some(124),
            (2, false) => None,
            // Prismatic — Oblivion 8 × Vec4 + 3 × f32 = 140. FNV adds
            // variable Motor; size depends on motor type.
            (6, true) => Some(140),
            (6, false) => None,
            // Ragdoll — Oblivion 6 × Vec4 + 6 × f32 = 120. FNV adds
            // 2 × Vec4 (Motor A/B) + 6 × f32 + variable Motor; size
            // depends on motor type.
            (7, true) => Some(120),
            (7, false) => None,
            // StiffSpring — 2 × Vec4 + f32 = 36 B regardless of version,
            // no Motor field.
            (8, _) => Some(36),
            // 13 Malleable wraps another CInfo with its own type dispatch.
            _ => None,
        }
    }

    /// Fixed-prefix byte count (positions + scalars, no motor) for
    /// FNV motor-bearing constraints. Returns `None` for any wrapped
    /// type that doesn't carry a runtime motor on FNV.
    ///
    /// Layouts per nif.xml (`!#NI_BS_LTE_16#` branch):
    ///   - LimitedHinge: 8 × Vec4 + 3 × f32 = 140 B (then Motor)
    ///   - Prismatic:    8 × Vec4 + 3 × f32 = 140 B (then Motor)
    ///   - Ragdoll:      8 × Vec4 + 6 × f32 = 152 B (then Motor)
    fn fnv_motor_prefix_size(wrapped_type: u32) -> Option<u64> {
        match wrapped_type {
            2 => Some(140), // LimitedHinge
            6 => Some(140), // Prismatic
            7 => Some(152), // Ragdoll
            _ => None,
        }
    }

    /// Consume a `bhkConstraintMotorCInfo` from the stream — 1 byte
    /// `hkMotorType` discriminator + conditional payload. Sizes per
    /// nif.xml:
    ///   - 0 NONE: 0 B
    ///   - 1 POSITION: 25 B
    ///   - 2 VELOCITY: 18 B
    ///   - 3 SPRING:   17 B
    ///
    /// Errors on an unknown motor type — the stream position would be
    /// unreliable past the byte we just read.
    fn consume_motor(stream: &mut NifStream) -> io::Result<()> {
        let motor_type = stream.read_u8()?;
        let payload: u64 = match motor_type {
            0 => 0,
            1 => 25,
            2 => 18,
            3 => 17,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "bhkConstraintMotorCInfo: unknown motor type {other} — \
                         stream position unreliable"
                    ),
                ));
            }
        };
        stream.skip(payload)?;
        Ok(())
    }

    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // Outer bhkConstraintCInfo (16 bytes).
        let (entity_a, entity_b, priority) = BhkConstraint::parse_base(stream)?;
        // Wrapped constraint: type(u32) + inner bhkConstraintCInfo(16)
        // + variable inner data.
        let wrapped_type = stream.read_u32_le()?;
        // Inner bhkConstraintCInfo — always 16 bytes.
        stream.skip(16)?;
        let is_oblivion = stream.version() <= NifVersion::V20_0_0_5;

        // #633: lift the Oblivion-only gate. When the wrapped CInfo size
        // is derivable for the parser's version (Hinge / BallAndSocket /
        // StiffSpring on either; LimitedHinge / Prismatic / Ragdoll on
        // Oblivion), read the trailer fields directly. Pre-#633 every
        // FNV/FO3 instance returned `threshold = 0.0,
        // remove_when_broken = false` even when the bytes were on disk.
        let trailer = if let Some(size) = Self::wrapped_payload_size(wrapped_type, is_oblivion) {
            stream.skip(size)?;
            Some(())
        } else if !is_oblivion {
            // FNV motor-bearing types (LimitedHinge / Prismatic /
            // Ragdoll): consume the fixed prefix + motor inline so the
            // trailer is reachable. Pre-#633 these all hit the short
            // stub and the motor + trailer bytes were skipped via
            // `block_size` recovery.
            if let Some(prefix) = Self::fnv_motor_prefix_size(wrapped_type) {
                stream.skip(prefix)?;
                Self::consume_motor(stream)?;
                Some(())
            } else {
                None
            }
        } else {
            None
        };

        if trailer.is_some() {
            let threshold = stream.read_f32_le()?;
            let remove_when_broken = stream.read_u8()? != 0;
            return Ok(Self {
                entity_a,
                entity_b,
                priority,
                wrapped_type,
                threshold,
                remove_when_broken,
            });
        }

        // Malleable (wrapped_type == 13) wraps another CInfo with its
        // own type dispatch — outside this table on either version.
        // `block_size` recovery in the outer walker handles the byte
        // skip; trailer fields default to zero.
        Ok(Self {
            entity_a,
            entity_b,
            priority,
            wrapped_type,
            threshold: 0.0,
            remove_when_broken: false,
        })
    }
}

/// `bhkOrientHingedBodyAction` — `bhkUnaryAction` that re-orients a
/// rigid body to keep its `Forward LS` axis pointing at a target,
/// pivoting around `Hinge Axis LS`. Used for articulation-driven
/// pieces like pendulums, doors that want to settle open, and some
/// of the Skyrim+ ragdoll "aim" bones. nif.xml line 7035.
///
/// Layout:
/// ```text
///   bhkUnaryAction: Entity(Ptr=4) + Unused 01[8] = 12 B
///   bhkOrientHingedBodyAction:
///     Unused 02[8] + Hinge Axis LS (Vec4) + Forward LS (Vec4)
///     + Strength(f32) + Damping(f32) + Unused 03[8]
///     = 8 + 16 + 16 + 4 + 4 + 8 = 56 B
///   Total = 68 B
/// ```
#[derive(Debug)]
pub struct BhkOrientHingedBodyAction {
    /// Rigid body this action is attached to.
    pub entity_ref: BlockRef,
    /// Local-space axis the body pivots around.
    pub hinge_axis_ls: [f32; 4],
    /// Local-space axis the body tries to keep aimed at the target.
    pub forward_ls: [f32; 4],
    /// Torque multiplier. Larger values re-orient faster.
    pub strength: f32,
    /// Angular damping on the reorientation torque.
    pub damping: f32,
}

impl NiObject for BhkOrientHingedBodyAction {
    fn block_type_name(&self) -> &'static str {
        "bhkOrientHingedBodyAction"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkOrientHingedBodyAction {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let entity_ref = stream.read_block_ref()?;
        stream.skip(8)?; // Unused 01
        stream.skip(8)?; // Unused 02
        let hinge_axis_ls = read_vec4(stream)?;
        let forward_ls = read_vec4(stream)?;
        let strength = stream.read_f32_le()?;
        let damping = stream.read_f32_le()?;
        stream.skip(8)?; // Unused 03
        Ok(Self {
            entity_ref,
            hinge_axis_ls,
            forward_ls,
            strength,
            damping,
        })
    }
}


// ── FO3+ ragdoll extensions ─────────────────────────────────────────
//
// `.psa` files (death-pose libraries — `idleanims/deathposes.psa`)
// carry a `bhkPoseArray` root block; `.rdt` ragdoll templates
// (`meshes/ragdollconstraint/*.rdt`) carry a `bhkRagdollTemplate` +
// child `bhkRagdollTemplateData`. Pre-#980 all three fell through
// to `NiUnknown` on FO3+ — NPCs ragdolled into the generic spine-
// collapse pose instead of the authored "shot in the chest" /
// "stumble-and-drop" canned poses.
//
// Schemas from nif.xml (`module="BSHavok"`, `versions="#FO3_AND_LATER#"`).

/// One bone's pose-frame inside a [`BonePose`]. Translation + quaternion
/// rotation + scale = 40 bytes, matching nif.xml's `size="40"` pin.
#[derive(Debug, Clone, Copy)]
pub struct BoneTransform {
    pub translation: [f32; 3],
    /// `hkQuaternion` storage order is `(x, y, z, w)` per nif.xml's
    /// `hkQuaternion` struct — same on-disk layout as `Quaternion`.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl BoneTransform {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let tx = stream.read_f32_le()?;
        let ty = stream.read_f32_le()?;
        let tz = stream.read_f32_le()?;
        let rx = stream.read_f32_le()?;
        let ry = stream.read_f32_le()?;
        let rz = stream.read_f32_le()?;
        let rw = stream.read_f32_le()?;
        let sx = stream.read_f32_le()?;
        let sy = stream.read_f32_le()?;
        let sz = stream.read_f32_le()?;
        Ok(Self {
            translation: [tx, ty, tz],
            rotation: [rx, ry, rz, rw],
            scale: [sx, sy, sz],
        })
    }
}

/// One full skeletal pose snapshot — `Num Transforms` × [`BoneTransform`].
/// In `.psa` files there's exactly one entry per bone listed in the
/// containing [`BhkPoseArray::bones`], but the on-disk count is parsed
/// independently so a malformed file can't reach into the next pose's
/// memory.
#[derive(Debug, Clone)]
pub struct BonePose {
    pub transforms: Vec<BoneTransform>,
}

impl BonePose {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let n = stream.read_u32_le()? as usize;
        // Each BoneTransform is exactly 40 bytes — bound the alloc
        // against the remaining stream so a junk count can't OOM.
        stream.check_alloc(n.saturating_mul(40))?;
        let mut transforms = Vec::with_capacity(n);
        for _ in 0..n {
            transforms.push(BoneTransform::parse(stream)?);
        }
        Ok(Self { transforms })
    }
}

/// `bhkPoseArray` — FO3/FNV death-pose library. Found at the root of
/// `meshes/idleanims/deathposes.psa` etc. The game samples a random
/// `poses[i]` entry and applies it to the ragdoll the moment the NPC
/// dies. See #980 / NIF-D5-NEW-04 and nif.xml.
#[derive(Debug)]
pub struct BhkPoseArray {
    /// Bone names this pose array targets — one entry per bone in the
    /// skeleton, matched by name at runtime so the file can be reused
    /// across creatures with different bone counts.
    pub bones: Vec<Option<Arc<str>>>,
    /// Pose snapshots — the engine picks one at random per death event.
    pub poses: Vec<BonePose>,
}

impl NiObject for BhkPoseArray {
    fn block_type_name(&self) -> &'static str {
        "bhkPoseArray"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkPoseArray {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_bones = stream.read_u32_le()? as usize;
        // String-table indices are 4 bytes on disk.
        stream.check_alloc(num_bones.saturating_mul(4))?;
        let mut bones = Vec::with_capacity(num_bones);
        for _ in 0..num_bones {
            bones.push(stream.read_string()?);
        }
        let num_poses = stream.read_u32_le()? as usize;
        // BonePose carries at least 4 bytes for the inner count. Real
        // bound enforcement happens inside `BonePose::parse`.
        stream.check_alloc(num_poses.saturating_mul(4))?;
        let mut poses = Vec::with_capacity(num_poses);
        for _ in 0..num_poses {
            poses.push(BonePose::parse(stream)?);
        }
        Ok(Self { bones, poses })
    }
}

/// `bhkRagdollTemplate` — FO3/FNV per-creature ragdoll constraint
/// template. Inherits `NiExtraData` (parsed inline since the
/// `NiExtraData` machinery returns its own struct and this block
/// type re-uses only the trailing fields). The `bones` list holds
/// references to companion [`BhkRagdollTemplateData`] blocks describing
/// each constraint hierarchy. See #980 / NIF-D5-NEW-04 and nif.xml.
#[derive(Debug)]
pub struct BhkRagdollTemplate {
    /// Inherited `NiExtraData.Name`. `None` on pre-10.0.1.0 content
    /// (gated by `read_extra_data_name`) — `.rdt` files are FO3+ only
    /// so this is always `Some` in practice but the gate is kept for
    /// uniformity with the rest of the NiExtraData family.
    pub name: Option<Arc<str>>,
    /// Refs to constraint-data blocks. nif.xml types these as
    /// `Ref<NiObject>` — in vanilla content they always resolve to
    /// [`BhkRagdollTemplateData`], but we preserve the polymorphic
    /// `Ref<NiObject>` shape so future modders' substitutions still
    /// parse cleanly.
    pub bones: Vec<BlockRef>,
}

impl NiObject for BhkRagdollTemplate {
    fn block_type_name(&self) -> &'static str {
        "bhkRagdollTemplate"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkRagdollTemplate {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiExtraData base — `Name` only, gated on stream version.
        let name = stream.read_extra_data_name()?;
        let num_bones = stream.read_u32_le()? as usize;
        // BlockRefs are 4 bytes each.
        stream.check_alloc(num_bones.saturating_mul(4))?;
        let mut bones = Vec::with_capacity(num_bones);
        for _ in 0..num_bones {
            bones.push(stream.read_block_ref()?);
        }
        Ok(Self { name, bones })
    }
}

/// `bhkRagdollTemplateData` — companion data block for
/// [`BhkRagdollTemplate`]. nif.xml schema carries a mass / restitution /
/// friction / radius / material block plus an array of polymorphic
/// `bhkWrappedConstraintData` entries — the inner constraint variant
/// switches on a `Type` discriminator and re-uses six different
/// constraint-info layouts (BallAndSocket / Hinge / LimitedHinge /
/// Prismatic / Ragdoll / StiffSpring / Malleable). The wrapper is
/// shared with `bhkBlendCollisionObject`'s constraint stack and would
/// need the full constraint-data parser family to expand.
///
/// First-pass stub per #980 / NIF-D5-NEW-04: parse the leading
/// scalars (Name / Mass / Restitution / Friction / Radius / Material)
/// + the constraint count, then skip the constraint array via
/// `block_size` so the stream stays aligned. The fixed-layout head
/// covers what we'd actually consume today (ragdoll mass tuning);
/// the polymorphic tail is left for a follow-up.
#[derive(Debug)]
pub struct BhkRagdollTemplateData {
    pub name: Option<Arc<str>>,
    pub mass: f32,
    pub restitution: f32,
    pub friction: f32,
    pub radius: f32,
    pub material: u32,
    /// Count of `bhkWrappedConstraintData` entries that follow on
    /// disk. The entries themselves are skipped through `block_size`
    /// (polymorphic constraint-CInfo expansion is a follow-up).
    pub num_constraints: u32,
}

impl NiObject for BhkRagdollTemplateData {
    fn block_type_name(&self) -> &'static str {
        "bhkRagdollTemplateData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BhkRagdollTemplateData {
    pub fn parse(stream: &mut NifStream, block_size: Option<u32>) -> io::Result<Self> {
        // Pin the start so we can compute how many bytes of constraint
        // payload to skip after the fixed-layout head.
        let start = stream.position();
        let name = stream.read_string()?;
        let mass = stream.read_f32_le()?;
        let restitution = stream.read_f32_le()?;
        let friction = stream.read_f32_le()?;
        let radius = stream.read_f32_le()?;
        let material = stream.read_u32_le()?;
        let num_constraints = stream.read_u32_le()?;
        // Skip the constraint array using block_size when available
        // (FO3+ has it; pre-Bethesda content never reaches this code
        // because the type is `#FO3_AND_LATER#`). Falls back to the
        // unknown-block-type dispatch handler if block_size is missing,
        // which would have happened anyway pre-#980.
        if let Some(sz) = block_size {
            let consumed = stream.position().saturating_sub(start);
            let remaining = (sz as u64).saturating_sub(consumed);
            if remaining > 0 {
                stream.skip(remaining)?;
            }
        }
        Ok(Self {
            name,
            mass,
            restitution,
            friction,
            radius,
            material,
            num_constraints,
        })
    }
}

#[cfg(test)]
mod bhk_rigid_body_tests;
#[cfg(test)]
mod bhk_blend_collision_object_tests;
#[cfg(test)]
mod bhk_breakable_constraint_tests;
#[cfg(test)]
mod bhk_ragdoll_tests;
#[cfg(test)]
mod hk_packed_ni_tri_strips_data_tests;
