//! Z-up (Gamebryo / Bethesda) → Y-up (engine) coordinate-flip helpers.
//!
//! Single source of truth for the axis swap every Bethesda import
//! boundary applies: mesh vertices, mesh normals, node translations,
//! REFR / cell placement, animation keys, SpeedTree placeholders.
//! Pre-#1044 (TD3-002 / TD3-003 / TD3-004) the same `(x, z, -y)`
//! transform lived in five places — `nif::import::coord`,
//! `nif::anim::coord`, `byroredux::cell_loader::euler`,
//! `crates::spt::import` — and the matrix-flavoured Shepperd path
//! in `nif::import::coord` had a `#333` normalise-after-extract fix
//! that the array-form sibling in `nif::anim::coord` never picked up.
//!
//! The NIF-flavoured wrappers (`NiPoint3` / `NiMatrix3`) live in
//! `nif::import::coord` and delegate here at the array boundary.
//! REFR / XCLL Euler-triple placement and SpeedTree bounds consume
//! these helpers directly via `byroredux_core::math::coord::…`.
//!
//! ## #333 normalise-quaternion invariant
//!
//! All quaternion outputs from this module are unit-length within
//! `1e-5`. Authored Bethesda keyframes can ship slightly drifted
//! quaternions (`1.0001` / `0.9999`) from export-tool quirks or
//! hand-authored content; `glam::Quat::from_xyzw` does not
//! renormalise, so a non-unit input would propagate shear/scale
//! into the ECS Transform rotation. Re-normalising at the import
//! boundary preserves the invariant downstream consumers rely on.

use glam::{Quat, Vec3};

/// One Bethesda exterior cell spans 4096 world units on each side
/// (32 × 33-vertex landscape grid at 128-unit spacing). This is the
/// spec-defined cell size for every Gamebryo / Creation Engine title
/// shipped to date (Oblivion through Starfield), so it's hard-coded
/// rather than per-game / per-worldspace.
///
/// Sole source of truth post-`#1112` / TD3-202; pre-fix the literal
/// `4096.0` appeared in cell_loader/water.rs, cell_loader/exterior.rs,
/// cell_loader/spawn.rs, cell_loader/terrain.rs, streaming.rs, and
/// crates/plugin/src/esm/cell/mod.rs, with at least one divergent
/// bug-fix history (TD3-110 Z-flip sign disagreement).
pub const EXTERIOR_CELL_UNITS: f32 = 4096.0;

/// Cell-grid `(gx, gy)` → Y-up world-space origin of that cell's
/// south-west corner. Composes the cell-size scale with the Z-up→Y-up
/// flip in one step:
///
/// ```text
/// world_x = gx * EXTERIOR_CELL_UNITS
/// world_y = 0                                     (vertical — unset)
/// world_z = -(gy * EXTERIOR_CELL_UNITS)           (negate-Y for Y-up)
/// ```
///
/// Use this whenever a cell-grid coordinate needs to land in renderer
/// world space directly. For paths that stay in Bethesda Z-up until a
/// later boundary flip (e.g., terrain centering that does its own
/// `Vec3::new(world_x, height, -world_y)`), reference
/// `EXTERIOR_CELL_UNITS` directly instead of this helper.
#[inline]
pub fn cell_grid_to_world_yup(gx: i32, gy: i32) -> Vec3 {
    Vec3::new(
        gx as f32 * EXTERIOR_CELL_UNITS,
        0.0,
        -(gy as f32) * EXTERIOR_CELL_UNITS,
    )
}

/// Convert a Z-up `[x, y, z]` position / translation to Y-up:
/// `(x, y, z) → (x, z, -y)`. Identity for the X axis; new Y is the
/// Bethesda Z (vertical → vertical); new Z is the negated Bethesda Y
/// (north → -south in engine terms).
#[inline]
pub fn zup_to_yup_pos(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[2], -p[1]]
}

/// Convert a Gamebryo `(w, x, y, z)` Z-up quaternion to a glam-ordered
/// `(x, y, z, w)` Y-up quaternion. Normalises before returning so
/// drifted-unit-length inputs (`±1e-4` off unity) don't propagate
/// scale into downstream `glam::Quat::from_xyzw`. See #333 (matrix
/// sibling) and #1044 / TD3-002 (this array path).
///
/// Derivation: the coord-change quaternion is a 90° rotation around
/// X (`q_conv = (sin45°, 0, 0, cos45°)`). Conjugating the source by
/// `q_conv` collapses to "swap y↔z and negate the new z" for the
/// vector part, leaving the scalar `w` untouched. Re-order WXYZ to
/// glam's XYZW at the end.
#[inline]
pub fn zup_to_yup_quat_wxyz(wxyz: [f32; 4]) -> [f32; 4] {
    let [w, x, y, z] = wxyz;
    let q = [x, z, -y, w];
    normalize_quat(q)
}

/// Convert Bethesda Z-up Euler angles (CW convention, XYZ-product
/// — Z applied first, X applied last) to a glam Y-up rotation
/// quaternion.
///
/// Bethesda uses Gamebryo's clockwise-positive rotation convention:
///   `R_zup = Rx_cw(rx) · Ry_cw(ry) · Rz_cw(rz)`
///
/// Each CW rotation by angle `t` equals a CCW rotation by `-t` under
/// glam's standard convention. The coord change `C: (x,y,z)_zup →
/// (x,z,-y)_yup` conjugates each axis rotation:
///   `C · Rx(-rx) · Cᵀ = Rx(-rx)`     (x → x)
///   `C · Ry(-ry) · Cᵀ = Rz(ry)`      (y → -z, double negate)
///   `C · Rz(-rz) · Cᵀ = Ry(-rz)`     (z → y)
///
/// Result: `R_yup = Rx(-rx) · Rz(ry) · Ry(-rz)`.
///
/// 2026-05-07 triage on `GSDocMitchellHouse` pinned CW + XYZ-product
/// as the correct composition (operator A/B compared all four
/// candidates against the in-game pose). The diagnostic mode
/// dispatcher lives in `byroredux::cell_loader::euler` for the REFR
/// placement path where empirical sign-off was tight; XCLL
/// directional lighting + this default helper go through here.
/// See `196dd67` for the `--rotation-mode` triage that pinned this.
#[inline]
pub fn euler_zup_to_quat_yup(rx: f32, ry: f32, rz: f32) -> Quat {
    Quat::from_rotation_x(-rx) * Quat::from_rotation_z(ry) * Quat::from_rotation_y(-rz)
}

/// Normalise a quaternion to unit length. Zero-length input is
/// returned unchanged to avoid NaN propagation — the Shepperd matrix
/// path never hits this case on a non-NaN matrix, and the array-form
/// `zup_to_yup_quat_wxyz` guards against it explicitly. Public so the
/// NIF-matrix consumer in `nif::import::coord::zup_matrix_to_yup_quat`
/// can share the same `#333` invariant. See #1044.
#[inline]
pub fn normalize_quat(q: [f32; 4]) -> [f32; 4] {
    let len_sq = q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3];
    if len_sq == 0.0 {
        return q;
    }
    let inv = len_sq.sqrt().recip();
    [q[0] * inv, q[1] * inv, q[2] * inv, q[3] * inv]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pos_flips_z_into_y_and_negates_y_into_z() {
        // Canonical Bethesda case: a 1 m vertical (Z=1) becomes engine
        // Y=1; a 1 m "north" (Y=1) becomes engine Z=-1.
        assert_eq!(zup_to_yup_pos([0.0, 0.0, 1.0]), [0.0, 1.0, 0.0]);
        assert_eq!(zup_to_yup_pos([0.0, 1.0, 0.0]), [0.0, 0.0, -1.0]);
        assert_eq!(zup_to_yup_pos([1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn identity_quat_wxyz_maps_to_glam_identity_xyzw() {
        // Bethesda (w=1, x=0, y=0, z=0) is the identity rotation;
        // expect glam (x=0, y=0, z=0, w=1).
        let out = zup_to_yup_quat_wxyz([1.0, 0.0, 0.0, 0.0]);
        assert_eq!(out, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn quat_swaps_y_and_z_and_negates_new_z() {
        // Pure Z-axis rotation in Bethesda (90° around vertical) should
        // become a pure Y-axis rotation in engine space. Vector part
        // (x, y, z) = (0, 0, sin45°), w = cos45°.
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let out = zup_to_yup_quat_wxyz([s, 0.0, 0.0, s]);
        // glam (x, y, z, w) = (0, sin45°, 0, cos45°)
        assert!((out[0] - 0.0).abs() < 1e-6);
        assert!((out[1] - s).abs() < 1e-6);
        assert!((out[2] - 0.0).abs() < 1e-6);
        assert!((out[3] - s).abs() < 1e-6);
    }

    #[test]
    fn drifted_unit_length_quat_normalises_after_swap() {
        // Regression for #1044 / TD3-002: pre-fix the array-form
        // sibling skipped the #333 normalise step, so a drifted
        // authored quaternion would propagate scale into the ECS
        // Transform rotation.
        let drifted = [1.0001_f32, 0.0, 0.0, 0.0];
        let out = zup_to_yup_quat_wxyz(drifted);
        let len =
            (out[0] * out[0] + out[1] * out[1] + out[2] * out[2] + out[3] * out[3]).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-5,
            "drifted quat must normalise to unit length, got len {len}",
        );

        let drifted_short = [0.9999_f32, 0.0, 0.0, 0.0];
        let out = zup_to_yup_quat_wxyz(drifted_short);
        let len =
            (out[0] * out[0] + out[1] * out[1] + out[2] * out[2] + out[3] * out[3]).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-5,
            "shrunk quat must normalise to unit length, got len {len}",
        );
    }

    #[test]
    fn zero_length_quat_returns_unchanged() {
        // Should not produce NaN — matching `nif::import::coord`'s
        // matrix-path behaviour for the degenerate-zero case.
        assert_eq!(zup_to_yup_quat_wxyz([0.0, 0.0, 0.0, 0.0]), [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn euler_identity_returns_identity_quat() {
        let q = euler_zup_to_quat_yup(0.0, 0.0, 0.0);
        assert!((q.length() - 1.0).abs() < 1e-6);
        assert!((q.x).abs() < 1e-6 && (q.y).abs() < 1e-6 && (q.z).abs() < 1e-6);
        assert!((q.w - 1.0).abs() < 1e-6);
    }

    /// TD3-202 / #1112 — `EXTERIOR_CELL_UNITS` is a Bethesda spec
    /// constant; if this test ever needs to change, every consumer
    /// in cell_loader / streaming / plugin::esm::cell needs auditing.
    #[test]
    fn exterior_cell_units_matches_bethesda_spec() {
        assert_eq!(EXTERIOR_CELL_UNITS, 4096.0);
    }

    /// TD3-202 — origin formula pins the Z-up→Y-up flip into one
    /// place so callers can't accidentally re-roll the sign.
    #[test]
    fn cell_grid_origin_negates_y_for_yup() {
        // (0, 0) → origin
        assert_eq!(cell_grid_to_world_yup(0, 0), Vec3::ZERO);
        // (+1, 0) → +X
        assert_eq!(cell_grid_to_world_yup(1, 0), Vec3::new(4096.0, 0.0, 0.0));
        // (0, +1) → -Z (Bethesda +Y is renderer -Z)
        assert_eq!(cell_grid_to_world_yup(0, 1), Vec3::new(0.0, 0.0, -4096.0));
        // (-2, -3) → (-8192, 0, 12288)
        assert_eq!(
            cell_grid_to_world_yup(-2, -3),
            Vec3::new(-8192.0, 0.0, 12288.0)
        );
    }
}
