//! Havok collision block parsers (bhk* types).
//!
//! These blocks form the collision geometry tree in Bethesda NIF files.
//! The pipeline: bhkCollisionObject → bhkRigidBody → shape tree.
//! Parsed data feeds a physics-agnostic ECS representation (CollisionShape),
//! which will be converted to Rapier colliders in the physics system (M28).

use super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::any::Any;
use std::io;

// ── Collision Object ────────────────────────────────────────────────

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
        }
        Ok(Self {
            target_ref,
            flags,
            body_ref,
        })
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
        stream.skip(20);

        // bhkEntityCInfo: response(1) + unused(1) + callback_delay(2) = 4 bytes
        stream.skip(4);

        if bsver <= 34 {
            // bhkRigidBodyCInfo550_660 (Oblivion / FO3)
            // Duplicated filter + entity CInfo (since 10.1.0.0)
            stream.skip(4); // unused
            let _cinfo_filter = stream.read_u32_le()?;
            stream.skip(4); // unused
            stream.skip(4); // response + unused + callback_delay
            stream.skip(4); // unused
        }

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
        let deactivator_type = if bsver <= 34 {
            stream.read_u8()?
        } else {
            // Skyrim+: deactivator type removed, use simulated
            0
        };
        let solver_deactivation = stream.read_u8()?;
        let quality_type = stream.read_u8()?;

        if bsver <= 34 {
            // Oblivion/FO3: 12 bytes unused padding after quality type
            stream.skip(12);
        } else if bsver < 130 {
            // Skyrim: autoRemoveLevel(1) + responseModifierFlags(1) + numShapeKeysInContactPoint(1)
            // + forceCollidedOntoPPU(1) = 4 bytes
            stream.skip(4);
        } else {
            // FO4+: different padding
            stream.skip(4);
        }

        // Constraint refs
        let num_constraints = stream.read_u32_le()?;
        let mut constraint_refs = Vec::with_capacity(num_constraints as usize);
        for _ in 0..num_constraints {
            constraint_refs.push(stream.read_block_ref()?);
        }

        // Body flags
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
        let material = stream.read_u32_le()?; // bhkSphereRepShape
        let radius = stream.read_f32_le()?; // bhkConvexShape
        Ok(Self { material, radius })
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
        let material = stream.read_u32_le()?;
        let radius = stream.read_f32_le()?;
        stream.skip(8); // unused
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
        let material = stream.read_u32_le()?;
        let radius = stream.read_f32_le()?;
        stream.skip(8); // unused
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
        let material = stream.read_u32_le()?;
        let radius = stream.read_f32_le()?;
        stream.skip(8); // unused
        let point1 = read_vec4(stream)?;
        let point2 = read_vec4(stream)?;
        let cylinder_radius = stream.read_f32_le()?;
        stream.skip(12); // unused padding
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
        let material = stream.read_u32_le()?;
        let radius = stream.read_f32_le()?;
        // Two bhkWorldObjCInfoProperty structs (12 bytes each)
        stream.skip(24);
        let num_vertices = stream.read_u32_le()? as usize;
        let mut vertices = Vec::with_capacity(num_vertices);
        for _ in 0..num_vertices {
            vertices.push(read_vec4(stream)?);
        }
        let num_normals = stream.read_u32_le()? as usize;
        let mut normals = Vec::with_capacity(num_normals);
        for _ in 0..num_normals {
            normals.push(read_vec4(stream)?);
        }
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
        let num_sub_shapes = stream.read_u32_le()? as usize;
        let mut sub_shape_refs = Vec::with_capacity(num_sub_shapes);
        for _ in 0..num_sub_shapes {
            sub_shape_refs.push(stream.read_block_ref()?);
        }
        let material = stream.read_u32_le()?;
        // Two bhkWorldObjCInfoProperty structs (12 bytes each)
        stream.skip(24);
        let num_filters = stream.read_u32_le()? as usize;
        let mut filters = Vec::with_capacity(num_filters);
        for _ in 0..num_filters {
            filters.push(stream.read_u32_le()?);
        }
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
        let material = stream.read_u32_le()?;
        let radius = stream.read_f32_le()?;
        stream.skip(8); // unused
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
        stream.skip(12); // unused
        let scale = stream.read_f32_le()?;
        let data_size = stream.read_u32_le()? as usize;
        let origin = read_vec4(stream)?; // since 10.1.0.0 (always present)
        // Build Type: only for BSVER > 34 (FO3+ only)
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
        let material = stream.read_u32_le()?;
        let radius = stream.read_f32_le()?;
        stream.skip(20); // unused
        let _grow_by = stream.read_u32_le()?;
        // Scale: since 10.1.0.0 (always present for Oblivion+)
        let _scale = read_vec4(stream)?;
        let num_data = stream.read_u32_le()? as usize;
        let mut data_refs = Vec::with_capacity(num_data);
        for _ in 0..num_data {
            data_refs.push(stream.read_block_ref()?);
        }
        let num_filters = stream.read_u32_le()? as usize;
        let mut filters = Vec::with_capacity(num_filters);
        for _ in 0..num_filters {
            filters.push(stream.read_u32_le()?);
        }
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
            let count = stream.read_u16_le()? as usize;
            let mut subs = Vec::with_capacity(count);
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
        stream.skip(4); // unused
        let _radius = stream.read_f32_le()?;
        stream.skip(4); // unused
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
        let num_triangles = stream.read_u32_le()? as usize;
        let mut triangles = Vec::with_capacity(num_triangles);
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

        let num_vertices = stream.read_u32_le()? as usize;
        // FO3+ (since 20.2.0.7): compressed bool + optional half-float vertices
        if version >= crate::version::NifVersion::V20_2_0_7 {
            let _compressed = stream.read_byte_bool()?;
        }
        let mut vertices = Vec::with_capacity(num_vertices);
        for _ in 0..num_vertices {
            let x = stream.read_f32_le()?;
            let y = stream.read_f32_le()?;
            let z = stream.read_f32_le()?;
            vertices.push([x, y, z]);
        }

        // FO3+ (since 20.2.0.7): sub-shapes at the end
        if version >= crate::version::NifVersion::V20_2_0_7 {
            let num_sub_shapes = stream.read_u16_le()? as usize;
            for _ in 0..num_sub_shapes {
                stream.skip(12); // HkSubPartData: filter(4) + numVerts(4) + material(4)
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
        stream.skip(20); // bhkWorldObjectCInfo

        // bhkPhantom / bhkShapePhantom / bhkSimpleShapePhantom: 4x4 transform
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
        stream.skip(num_mat32 as u64 * 4);
        let num_mat16 = stream.read_u32_le()? as usize;
        stream.skip(num_mat16 as u64 * 4);
        let num_mat8 = stream.read_u32_le()? as usize;
        stream.skip(num_mat8 as u64 * 4);

        // Chunk materials: (SkyrimHavokMaterial, HavokFilter) = 2×u32 = 8 bytes each
        let num_chunk_materials = stream.read_u32_le()? as usize;
        let mut chunk_materials = Vec::with_capacity(num_chunk_materials);
        for _ in 0..num_chunk_materials {
            chunk_materials.push([stream.read_u32_le()?, stream.read_u32_le()?]);
        }

        let _num_named_materials = stream.read_u32_le()?;

        // Chunk transforms
        let num_transforms = stream.read_u32_le()? as usize;
        let mut chunk_transforms = Vec::with_capacity(num_transforms);
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
        let num_big_verts = stream.read_u32_le()? as usize;
        let mut big_verts = Vec::with_capacity(num_big_verts);
        for _ in 0..num_big_verts {
            big_verts.push([
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ]);
        }

        // Big tris
        let num_big_tris = stream.read_u32_le()? as usize;
        let mut big_tris = Vec::with_capacity(num_big_tris);
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
        let num_chunks = stream.read_u32_le()? as usize;
        let mut chunks = Vec::with_capacity(num_chunks);
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

            // Vertices: count * 3 u16 values
            let num_vertices = stream.read_u32_le()? as usize;
            let mut vertices = Vec::with_capacity(num_vertices);
            for _ in 0..num_vertices {
                vertices.push([
                    stream.read_u16_le()?,
                    stream.read_u16_le()?,
                    stream.read_u16_le()?,
                ]);
            }

            // Indices
            let num_indices = stream.read_u32_le()? as usize;
            let mut indices = Vec::with_capacity(num_indices);
            for _ in 0..num_indices {
                indices.push(stream.read_u16_le()?);
            }

            // Strips
            let num_strips = stream.read_u32_le()? as usize;
            let mut strips = Vec::with_capacity(num_strips);
            for _ in 0..num_strips {
                strips.push(stream.read_u16_le()?);
            }

            // Welding info
            let num_welding = stream.read_u32_le()? as usize;
            stream.skip(num_welding as u64 * 2);

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
