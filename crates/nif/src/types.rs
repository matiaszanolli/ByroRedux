//! Intermediate math types for NIF data.
//!
//! These mirror Gamebryo's internal representations (NiPoint3, NiMatrix3, etc.)
//! and are converted to glam types during the ECS import phase.

/// 3D point/vector — maps to Gamebryo's NiPoint3.
///
/// `#[repr(C)]` documents the 12-byte tightly-packed layout that the
/// bulk readers in `stream.rs::read_ni_point3_array` rely on for the
/// direct byte-slice `read_exact` fast path (#833). Practical layout
/// is unchanged from the default `repr(Rust)` for three same-size /
/// same-alignment fields — no padding either way — but the annotation
/// pins the contract so a future field reorder or padding insertion
/// can't silently corrupt the bulk-read path.
#[repr(C)]
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
        Self {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        }
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
        Self {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        }
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
            rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
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

/// Quaternion transform: translation + quaternion rotation + scale.
/// Used by NiTransformInterpolator (animation keyframe poses).
/// Quaternion stored as (w, x, y, z) matching Gamebryo's NiQuatTransform serialization.
#[derive(Debug, Clone, Copy)]
pub struct NiQuatTransform {
    pub translation: NiPoint3,
    pub rotation: [f32; 4], // w, x, y, z
    pub scale: f32,
}

impl Default for NiQuatTransform {
    fn default() -> Self {
        Self {
            translation: NiPoint3::default(),
            rotation: [1.0, 0.0, 0.0, 0.0], // identity quaternion (w=1)
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
        if self.is_null() {
            None
        } else {
            Some(self.0 as usize)
        }
    }
}

/// Bitangent handedness sign for the renderer's `B = w * cross(N, T)`
/// reconstruction. `t` is the tangent we store in `Vertex.tangent.xyz`
/// (∂P/∂U); `b` is the surface's other tangent derivative (∂P/∂V).
/// Returns `+1.0` for a standard right-handed UV winding (and for the
/// degenerate zero case) and `-1.0` for a mirrored shell.
///
/// **Operand order is load-bearing.** The scalar triple product is
/// antisymmetric, so `sign(dot(b, cross(n, t)))` and
/// `sign(dot(t, cross(n, b)))` are negatives of each other — passing the
/// stored tangent and the other derivative in the wrong slots silently
/// inverts every normal-map V channel (see #1516). All three tangent
/// producers — `extract_tangents_from_extra_data`, the BSTriShape inline
/// decode, and the SSE skin reconstruction — must call through here so the
/// convention stays pinned in one place.
///
/// Coordinate-frame agnostic: `n`, `t`, `b` must share a frame, but the
/// result is invariant under the proper Z-up → Y-up rotation, so callers
/// may pass raw Z-up or converted Y-up triples interchangeably.
pub(crate) fn bitangent_sign(n: [f32; 3], t: [f32; 3], b: [f32; 3]) -> f32 {
    let cross_nt = [
        n[1] * t[2] - n[2] * t[1],
        n[2] * t[0] - n[0] * t[2],
        n[0] * t[1] - n[1] * t[0],
    ];
    let dot = b[0] * cross_nt[0] + b[1] * cross_nt[1] + b[2] * cross_nt[2];
    if dot < 0.0 {
        -1.0
    } else {
        1.0
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

    // #1516 — pins the bitangent-sign convention shared by all three
    // tangent producers. Textbook right-handed basis (N=+Z, T=∂P/∂U=+X,
    // B=∂P/∂V=+Y) must yield +1; the operand-swapped call must yield -1,
    // guarding against re-introducing the antisymmetric inversion the
    // BSTriShape inline + SSE-recon paths shipped before #1516.
    #[test]
    fn bitangent_sign_right_handed_is_positive() {
        let n = [0.0, 0.0, 1.0];
        let t = [1.0, 0.0, 0.0]; // ∂P/∂U (stored tangent)
        let b = [0.0, 1.0, 0.0]; // ∂P/∂V
        assert_eq!(bitangent_sign(n, t, b), 1.0);
    }

    #[test]
    fn bitangent_sign_swapped_operands_invert() {
        let n = [0.0, 0.0, 1.0];
        let t = [1.0, 0.0, 0.0];
        let b = [0.0, 1.0, 0.0];
        // Passing ∂P/∂V as the tangent and ∂P/∂U as the other derivative
        // is exactly the pre-#1516 bug — antisymmetry flips the sign.
        assert_eq!(bitangent_sign(n, b, t), -1.0);
    }

    #[test]
    fn bitangent_sign_mirrored_uv_is_negative() {
        // V flipped: ∂P/∂V points -Y, so cross(N, T)=+Y opposes it.
        let n = [0.0, 0.0, 1.0];
        let t = [1.0, 0.0, 0.0];
        let b = [0.0, -1.0, 0.0];
        assert_eq!(bitangent_sign(n, t, b), -1.0);
    }

    #[test]
    fn bitangent_sign_degenerate_defaults_positive() {
        assert_eq!(bitangent_sign([0.0; 3], [0.0; 3], [0.0; 3]), 1.0);
    }

    #[test]
    fn bitangent_sign_rotation_invariant_zup_vs_yup() {
        // Same basis expressed Z-up and after the (x,z,-y) Y-up swap must
        // give the same sign — callers pass either frame interchangeably.
        let zup = bitangent_sign([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let yup = bitangent_sign([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
        assert_eq!(zup, yup);
    }
}
