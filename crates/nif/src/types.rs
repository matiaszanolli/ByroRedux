//! Intermediate math types for NIF data.
//!
//! These mirror Gamebryo's internal representations (NiPoint3, NiMatrix3, etc.)
//! and are converted to glam types during the ECS import phase.

/// 3D point/vector — maps to Gamebryo's NiPoint3.
#[derive(Debug, Clone, Copy, Default)]
pub struct NiPoint3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// RGB color (0.0–1.0) — maps to Gamebryo's NiColor.
#[derive(Debug, Clone, Copy)]
pub struct NiColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Default for NiColor {
    fn default() -> Self {
        Self { r: 1.0, g: 1.0, b: 1.0 }
    }
}

/// RGBA color (0.0–1.0) — maps to Gamebryo's NiColorA.
#[derive(Debug, Clone, Copy)]
pub struct NiColorA {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for NiColorA {
    fn default() -> Self {
        Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }
    }
}

/// 3x3 rotation matrix — maps to Gamebryo's NiMatrix3.
/// Row-major storage: rows[0] = first row = (m00, m01, m02).
#[derive(Debug, Clone, Copy)]
pub struct NiMatrix3 {
    pub rows: [[f32; 3]; 3],
}

impl Default for NiMatrix3 {
    fn default() -> Self {
        Self {
            rows: [
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
        }
    }
}

/// Combined transform: rotation + translation + uniform scale.
/// Maps to Gamebryo's NiTransform.
#[derive(Debug, Clone, Copy)]
pub struct NiTransform {
    pub rotation: NiMatrix3,
    pub translation: NiPoint3,
    pub scale: f32,
}

impl Default for NiTransform {
    fn default() -> Self {
        Self {
            rotation: NiMatrix3::default(),
            translation: NiPoint3::default(),
            scale: 1.0,
        }
    }
}

/// A reference to another block in the NIF file, by index.
/// `u32::MAX` (0xFFFFFFFF) is the null reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockRef(pub u32);

impl BlockRef {
    pub const NULL: Self = Self(u32::MAX);

    pub fn is_null(self) -> bool {
        self.0 == u32::MAX
    }

    pub fn index(self) -> Option<usize> {
        if self.is_null() { None } else { Some(self.0 as usize) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_ref_null() {
        assert!(BlockRef::NULL.is_null());
        assert!(BlockRef(u32::MAX).is_null());
        assert_eq!(BlockRef::NULL.index(), None);
    }

    #[test]
    fn block_ref_valid() {
        let r = BlockRef(5);
        assert!(!r.is_null());
        assert_eq!(r.index(), Some(5));
    }

    #[test]
    fn block_ref_zero_is_valid() {
        let r = BlockRef(0);
        assert!(!r.is_null());
        assert_eq!(r.index(), Some(0));
    }

    #[test]
    fn ni_transform_default_is_identity() {
        let t = NiTransform::default();
        assert_eq!(t.scale, 1.0);
        assert_eq!(t.translation.x, 0.0);
        assert_eq!(t.rotation.rows[0], [1.0, 0.0, 0.0]);
        assert_eq!(t.rotation.rows[1], [0.0, 1.0, 0.0]);
        assert_eq!(t.rotation.rows[2], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn ni_color_default_is_white() {
        let c = NiColor::default();
        assert_eq!(c.r, 1.0);
        assert_eq!(c.g, 1.0);
        assert_eq!(c.b, 1.0);
    }
}
