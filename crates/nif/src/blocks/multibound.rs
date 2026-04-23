//! BSMultiBound blocks — spatial bounding volumes for streaming/culling.
//!
//! BSMultiBoundNode (already parsed as NiNode) references a BSMultiBound,
//! which in turn references a BSMultiBoundAABB or BSMultiBoundOBB.

use super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use std::any::Any;
use std::io;

/// BSMultiBound — container referencing a BSMultiBoundData subclass.
#[derive(Debug)]
pub struct BsMultiBound {
    pub data_ref: BlockRef,
}

impl NiObject for BsMultiBound {
    fn block_type_name(&self) -> &'static str {
        "BSMultiBound"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsMultiBound {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let data_ref = stream.read_block_ref()?;
        Ok(Self { data_ref })
    }
}

/// BSMultiBoundAABB — axis-aligned bounding box.
#[derive(Debug)]
pub struct BsMultiBoundAABB {
    pub position: [f32; 3],
    pub extent: [f32; 3],
}

impl NiObject for BsMultiBoundAABB {
    fn block_type_name(&self) -> &'static str {
        "BSMultiBoundAABB"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsMultiBoundAABB {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let position = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let extent = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        Ok(Self { position, extent })
    }
}

/// BSMultiBoundOBB — oriented bounding box.
#[derive(Debug)]
pub struct BsMultiBoundOBB {
    pub center: [f32; 3],
    pub size: [f32; 3],
    pub rotation: [[f32; 3]; 3],
}

impl NiObject for BsMultiBoundOBB {
    fn block_type_name(&self) -> &'static str {
        "BSMultiBoundOBB"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsMultiBoundOBB {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let center = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let size = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let mut rotation = [[0.0f32; 3]; 3];
        for row in &mut rotation {
            for val in row.iter_mut() {
                *val = stream.read_f32_le()?;
            }
        }
        Ok(Self {
            center,
            size,
            rotation,
        })
    }
}

/// BSMultiBoundSphere — spherical bounding volume (FO3+).
#[derive(Debug)]
pub struct BsMultiBoundSphere {
    pub center: [f32; 3],
    pub radius: f32,
}

impl NiObject for BsMultiBoundSphere {
    fn block_type_name(&self) -> &'static str {
        "BSMultiBoundSphere"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BsMultiBoundSphere {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let center = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let radius = stream.read_f32_le()?;
        Ok(Self { center, radius })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::NifVersion;

    fn make_fnv_header() -> NifHeader {
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

    #[test]
    fn parse_bs_multi_bound() {
        let header = make_fnv_header();
        let data: Vec<u8> = 5i32.to_le_bytes().to_vec();
        let mut stream = NifStream::new(&data, &header);
        let mb = BsMultiBound::parse(&mut stream).unwrap();
        assert_eq!(mb.data_ref.index(), Some(5));
    }

    #[test]
    fn parse_bs_multi_bound_aabb() {
        let header = make_fnv_header();
        let mut data = Vec::new();
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut stream = NifStream::new(&data, &header);
        let aabb = BsMultiBoundAABB::parse(&mut stream).unwrap();
        assert!((aabb.position[0] - 1.0).abs() < 1e-6);
        assert!((aabb.extent[2] - 6.0).abs() < 1e-6);
    }

    #[test]
    fn parse_bs_multi_bound_obb() {
        let header = make_fnv_header();
        let mut data = Vec::new();
        // center (3) + size (3) + rotation (9) = 15 floats
        for i in 0..15 {
            data.extend_from_slice(&(i as f32).to_le_bytes());
        }
        let mut stream = NifStream::new(&data, &header);
        let obb = BsMultiBoundOBB::parse(&mut stream).unwrap();
        assert!((obb.center[0] - 0.0).abs() < 1e-6);
        assert!((obb.size[0] - 3.0).abs() < 1e-6);
        assert!((obb.rotation[0][0] - 6.0).abs() < 1e-6);
        assert_eq!(stream.position(), 60); // 15 * 4 bytes
    }

    #[test]
    fn parse_bs_multi_bound_sphere() {
        let header = make_fnv_header();
        let mut data = Vec::new();
        // center (3) + radius (1) = 4 floats = 16 bytes
        for v in [10.0f32, 20.0, 30.0, 5.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut stream = NifStream::new(&data, &header);
        let sphere = BsMultiBoundSphere::parse(&mut stream).unwrap();
        assert!((sphere.center[0] - 10.0).abs() < 1e-6);
        assert!((sphere.center[1] - 20.0).abs() < 1e-6);
        assert!((sphere.center[2] - 30.0).abs() < 1e-6);
        assert!((sphere.radius - 5.0).abs() < 1e-6);
        assert_eq!(stream.position(), 16);
    }
}
