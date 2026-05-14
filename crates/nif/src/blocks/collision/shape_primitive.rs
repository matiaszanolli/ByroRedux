//! Primitive collision shapes.
//!
//! Sphere, MultiSphere, Box, Capsule, Cylinder — single-volume shapes with
//! material + bounding metadata.

use super::super::NiObject;
use crate::stream::NifStream;
use std::any::Any;
use std::io;

use super::{read_havok_material, read_vec4};

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
