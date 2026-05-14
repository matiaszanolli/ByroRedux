//! Compound / wrapper collision shapes.
//!
//! ConvexVerts, List, Transform, MoppBvTree, ConvexList — shapes that wrap or
//! aggregate other shapes.

use super::super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::any::Any;
use std::io;

use super::{read_havok_material, read_vec4};

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
