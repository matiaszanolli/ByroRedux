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
/// cell_loader/spawn.rs, cell_loader/terrain.rs, the `streaming/` module, and
/// crates/plugin/src/esm/cell/mod.rs, with at least one divergent
/// bug-fix history (TD3-110 Z-flip sign disagreement).
pub const EXTERIOR_CELL_UNITS: f32 = 4096.0;

/// Vertices per side of an exterior cell's `LAND` heightmap grid: 33,
/// spanning 32 quads at [`LAND_VERTEX_SPACING`] units each.
///
/// Promoted out of `cell_loader/terrain.rs`'s function-local `const GRID`
/// (#4052) because it stopped being one consumer's private detail: the
/// ground-cover scatter pass has to locate a cell's terrain vertices in the
/// renderer's global vertex SSBO, which means the *shader* needs this number
/// too. It reaches GLSL through `shader_constants_data.rs` →
/// `include/shader_constants.glsl`, so there is one definition rather than a
/// Rust one and a GLSL one drifting apart — a drift that would not fail any
/// test, it would silently read a neighbouring vertex.
pub const LAND_GRID_VERTS: usize = 33;

/// Spacing between adjacent `LAND` grid vertices, in world units (128.0).
///
/// Derived rather than typed: `EXTERIOR_CELL_UNITS / (LAND_GRID_VERTS - 1)`.
pub const LAND_VERTEX_SPACING: f32 = EXTERIOR_CELL_UNITS / (LAND_GRID_VERTS as f32 - 1.0);

/// Texture tiles per cell side for `LAND` diffuse/splat sampling (12.0).
///
/// Per openmw's ESM4 (Oblivion+) terrain (`Storage::getTextureTileCount` →
/// `2 * ESM4::Land::sQuadTexturePerSide`, with `sQuadTexturePerSide = 6`):
/// 2 quadrants per cell side × 6 texture tiles per quadrant. The host
/// authors terrain vertex UVs from it (`cell_loader/terrain.rs`), and
/// §12.3's ground-colour coupling reconstructs the same UV in the blade
/// vertex shader from world position — so the *shader* needs this number
/// too. It reaches GLSL through `shader_constants_data.rs` →
/// `include/shader_constants.glsl`, one definition rather than a Rust one
/// and a GLSL one drifting apart. The drift would be invisible in isolation
/// (the grass would just take its ground colour from a slightly wrong texel)
/// and only shows as a colour mismatch against the terrain behind the blade.
pub const LAND_TEXTURE_TILES_PER_CELL: f32 = 12.0;

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

/// The inverse of [`zup_to_yup_pos`]: engine Y-up `(x, y, z)` back to
/// Gamebryo Z-up `(x, -z, y)`. For fixtures that author in engine space
/// and need the on-disk Z-up form (navmesh vertices), so the Z-up knowledge
/// stays in this module instead of being re-derived per test (#5597).
#[inline]
pub fn yup_to_zup_pos(p: [f32; 3]) -> [f32; 3] {
    [p[0], -p[2], p[1]]
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
/// — Z applied first to the vertex, X applied last, about world axes)
/// to a glam Y-up rotation quaternion.
///
/// Bethesda uses Gamebryo's `FromEulerAnglesXYZ`: clockwise-positive
/// axis matrices composed as an XYZ product under column vectors
/// (`v' = M · v`):
///   `R_zup = Rx_cw(rx) · Ry_cw(ry) · Rz_cw(rz)`
/// i.e. extrinsic Z, then Y, then X — equivalently intrinsic
/// (object-local) X, then Y, then Z.
///
/// Each CW rotation by angle `t` equals a CCW rotation by `-t` under
/// glam's standard convention. The coord change `C: (x,y,z)_zup →
/// (x,z,-y)_yup` conjugates each axis rotation:
///   `C · Rx(-rx) · Cᵀ = Rx(-rx)`     (x → x)
///   `C · Ry(-ry) · Cᵀ = Rz(ry)`      (y → -z, double negate)
///   `C · Rz(-rz) · Cᵀ = Ry(-rz)`     (z → y)
///
/// Composing under the axis swap (composition order preserved):
///   `R_yup = Rx(-rx) · Rz(ry) · Ry(-rz)`
///
/// **References** (four independent, storage-convention-free):
/// - Gamebryo 3.2 `efd.lib` (Debug, `Matrix3_Win32.obj`):
///   `FromEulerAnglesXYZ` builds X/Y/Z via `MakeXRotation` etc., then
///   calls `operator*(Y, Z)` followed by `operator*(X, tmp)` —
///   `RX · (RY · RZ)`. `efd/Matrix3.h` pins column vectors + CW: "a
///   90-degree rotation of [0,1,0] about [0,0,1] gives [1,0,0]".
/// - CK wiki `SetAngle - ObjectReference`: "The Z-axis rotation is
///   applied first, followed by the Y-axis and finally the X-axis …
///   extrinsic left-handed".
/// - OpenMW `components/misc/convert.hpp::makeOsgQuat`:
///   `Quat(z,-Z) * Quat(y,-Y) * Quat(x,-X)`. `osg::Quat::operator*`
///   multiplies in reverse Hamilton order (`a*b` = "a, then b" —
///   compare OpenMW's actor look direction `Quat(pitch,-X) *
///   Quat(yaw,-Z)` in `mwworld/worldimp.cpp`, correct only under that
///   reading), so the expression is Z first, then Y, then X.
/// - `GSProspectorSaloonInterior` A/B render: the multi-axis
///   `NVGSRmWindowSeparate` / `NVCraftsmanRmWindowSeparate` walls
///   close the room under this formula and leave dark slabs / sky
///   gaps under the ZYX product.
///
/// **History**: shipped as XYZ on 2026-05-07, flipped to the ZYX
/// product (`--rotation-mode 1`) on 2026-05-26 (`20074410`) by reading
/// the OpenMW expression as a Hamilton product — which broke ~30% of
/// FNV interior REFRs (every multi-axis one). Restored here. Modes 0
/// and 1 agree whenever only `rz` is non-zero, which is why Z-only
/// cells could never arbitrate the dispute.
#[inline]
pub fn euler_zup_to_quat_yup(rx: f32, ry: f32, rz: f32) -> Quat {
    euler_zup_to_quat_yup_mode(REFR_ROTATION_MODE_SHIP, rx, ry, rz)
}

/// The inverse of [`euler_zup_to_quat_yup`]: the Z-up Euler triple `(rx, ry, rz)` in radians —
/// the one a REFR's `DATA` rotation holds, and the one an ObScript `GetAngle` reads — of a Y-up
/// quaternion under the shipping convention `Rx(-rx) · Rz(ry) · Ry(-rz)`.
///
/// From the product's matrix (`a = -rx`, `b = ry`, `c = -rz`): `R[0][1] = -sin b`,
/// `R[1][1] = cos a cos b`, `R[2][1] = sin a cos b`, `R[0][0] = cos b cos c`,
/// `R[0][2] = cos b sin c`. So `ry` is the middle angle, in `[-pi/2, pi/2]`; `rx` and `rz`
/// come back in `(-pi, pi]`. At `ry = ±pi/2` (gimbal lock) `rz` is taken as zero and `rx` carries
/// the rest.
pub fn quat_yup_to_euler_zup(q: Quat) -> (f32, f32, f32) {
    let m = glam::Mat3::from_quat(q.normalize());
    // glam is column-major: `m.col(j)[i]` is R[i][j].
    let r = |i: usize, j: usize| m.col(j)[i];
    let sin_b = (-r(0, 1)).clamp(-1.0, 1.0);
    let b = sin_b.asin();
    let (a, c) = if sin_b.abs() > 0.999_999 {
        // cos b = 0: the first and third axes coincide.
        ((-r(1, 2)).atan2(r(2, 2)), 0.0)
    } else {
        (r(2, 1).atan2(r(1, 1)), r(0, 2).atan2(r(0, 0)))
    };
    (-a, b, -c)
}

/// The shipping REFR rotation mode — CW angle convention, XYZ product
/// (Gamebryo `FromEulerAnglesXYZ`). [`euler_zup_to_quat_yup`] is exactly
/// this mode; the other three are diagnostic-only. See
/// [`euler_zup_to_quat_yup_mode`].
pub const REFR_ROTATION_MODE_SHIP: u8 = 0;

/// The four candidate Euler→Y-up conversions the REFR-placement A/B
/// triage sweeps over. **Single source of truth** for the formulas —
/// `byroredux::cell_loader::euler`'s `--rotation-mode N` dispatcher and
/// the `byroredux-plugin` `cell_rot_sweep` example both route here
/// (#2438 / COORD-5; pre-fix the example hand-copied all four arms, so a
/// retune of the dispatcher would have left the triage tool reporting
/// conclusions about a convention the engine no longer used).
///
///   0: CW + XYZ-product  — `Rx(-rx) · Rz(ry) · Ry(-rz)`
///      (current ship; Gamebryo `FromEulerAnglesXYZ` — see
///      [`euler_zup_to_quat_yup`])
///   1: CW + ZYX-product  — `Ry(-rz) · Rz(ry) · Rx(-rx)`
///      (2026-05-26 → 2026-10-16 ship, from a Hamilton misreading of
///      OpenMW's `osg::Quat` product; kept for A/B triage only)
///   2: CCW + ZYX-product — `Ry(rz) · Rz(-ry) · Rx(rx)` (no negation)
///   3: CCW + XYZ-product — `Rx(rx) · Rz(-ry) · Ry(rz)` (no negation)
///
/// Modes 0 and 1 agree whenever only `rz` is non-zero — which is why
/// Z-only cells (the 2026-05-07 `GSDocMitchellHouse` sign-off) cannot
/// tell them apart. Any mode outside `0..=3` falls back to the shipping
/// default so a bad `--rotation-mode` argument can't produce garbage
/// placement.
#[inline]
pub fn euler_zup_to_quat_yup_mode(mode: u8, rx: f32, ry: f32, rz: f32) -> Quat {
    match mode {
        1 => Quat::from_rotation_y(-rz) * Quat::from_rotation_z(ry) * Quat::from_rotation_x(-rx),
        2 => Quat::from_rotation_y(rz) * Quat::from_rotation_z(-ry) * Quat::from_rotation_x(rx),
        3 => Quat::from_rotation_x(rx) * Quat::from_rotation_z(-ry) * Quat::from_rotation_y(rz),
        // Mode 0 and any out-of-range value: the shipping formula.
        _ => Quat::from_rotation_x(-rx) * Quat::from_rotation_z(ry) * Quat::from_rotation_y(-rz),
    }
}

/// Normalise a quaternion to unit length. Zero-length input is
/// returned unchanged to avoid NaN propagation — the Shepperd matrix
/// path never hits this case on a non-NaN matrix, and the array-form
/// `zup_to_yup_quat_wxyz` guards against it explicitly. Public so the
/// NIF-matrix consumer in `nif::import::coord::zup_matrix_to_yup_quat`
/// can share the same `#333` invariant. See #1044.
///
/// Known leniency (deliberate — this is a pure math helper): an input
/// whose components square past `f32::MAX` yields the zero quaternion
/// here (`len_sq` inf → `inv` 0), and zero-length input passes through
/// unchanged. Animation keys never reach that — `nif::anim::keys::
/// normalized_rotation_sample` (#4396) rejects both classes before any
/// conversion calls this — but a new caller must not rely on this
/// function alone to sanitize authored quaternions.
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

    /// The inverse recovers what the REFR formula was given: every triple with the middle angle
    /// inside `(-pi/2, pi/2)` round-trips through the quaternion, and a gimbal-locked one lands on
    /// the same rotation.
    #[test]
    fn the_euler_extraction_inverts_the_refr_convention() {
        let angles = [-2.9f32, -1.2, -0.3, 0.0, 0.4, 1.1, 2.5, 3.0];
        for &rx in &angles {
            for &rz in &angles {
                for &ry in &[-1.4f32, -0.7, 0.0, 0.5, 1.3] {
                    let q = euler_zup_to_quat_yup(rx, ry, rz);
                    let (ex, ey, ez) = quat_yup_to_euler_zup(q);
                    let again = euler_zup_to_quat_yup(ex, ey, ez);
                    assert!(q.dot(again).abs() > 0.9999, "({rx}, {ry}, {rz}) -> ({ex}, {ey}, {ez})");
                    assert!((ey - ry).abs() < 1e-3, "middle angle {ry} came back {ey}");
                }
            }
        }
        // Gimbal lock still reproduces the rotation.
        let q = euler_zup_to_quat_yup(0.8, std::f32::consts::FRAC_PI_2, 0.3);
        let (ex, ey, ez) = quat_yup_to_euler_zup(q);
        assert!(q.dot(euler_zup_to_quat_yup(ex, ey, ez)).abs() > 0.9999);
        // A pure Z turn is a pure `rz`.
        let (ex, ey, ez) = quat_yup_to_euler_zup(euler_zup_to_quat_yup(0.0, 0.0, 1.0));
        assert!(ex.abs() < 1e-5 && ey.abs() < 1e-5 && (ez - 1.0).abs() < 1e-5);
    }

    #[test]
    fn pos_flips_z_into_y_and_negates_y_into_z() {
        // Canonical Bethesda case: a 1 m vertical (Z=1) becomes engine
        // Y=1; a 1 m "north" (Y=1) becomes engine Z=-1.
        assert_eq!(zup_to_yup_pos([0.0, 0.0, 1.0]), [0.0, 1.0, 0.0]);
        assert_eq!(zup_to_yup_pos([0.0, 1.0, 0.0]), [0.0, 0.0, -1.0]);
        assert_eq!(zup_to_yup_pos([1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn yup_to_zup_pos_is_the_inverse_of_zup_to_yup_pos() {
        for p in [[1.0, 2.0, 3.0], [0.0, 0.0, 1.0], [-4.5, 0.25, 7.0]] {
            assert_eq!(zup_to_yup_pos(yup_to_zup_pos(p)), p);
            assert_eq!(yup_to_zup_pos(zup_to_yup_pos(p)), p);
        }
        assert_eq!(yup_to_zup_pos([0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]);
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
        let len = (out[0] * out[0] + out[1] * out[1] + out[2] * out[2] + out[3] * out[3]).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-5,
            "drifted quat must normalise to unit length, got len {len}",
        );

        let drifted_short = [0.9999_f32, 0.0, 0.0, 0.0];
        let out = zup_to_yup_quat_wxyz(drifted_short);
        let len = (out[0] * out[0] + out[1] * out[1] + out[2] * out[2] + out[3] * out[3]).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-5,
            "shrunk quat must normalise to unit length, got len {len}",
        );
    }

    #[test]
    fn zero_length_quat_returns_unchanged() {
        // Should not produce NaN — matching `nif::import::coord`'s
        // matrix-path behaviour for the degenerate-zero case.
        assert_eq!(
            zup_to_yup_quat_wxyz([0.0, 0.0, 0.0, 0.0]),
            [0.0, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn euler_identity_returns_identity_quat() {
        let q = euler_zup_to_quat_yup(0.0, 0.0, 0.0);
        assert!((q.length() - 1.0).abs() < 1e-6);
        assert!((q.x).abs() < 1e-6 && (q.y).abs() < 1e-6 && (q.z).abs() < 1e-6);
        assert!((q.w - 1.0).abs() < 1e-6);
    }

    /// Applies the Gamebryo `efd::Matrix3` CW axis matrices (copied from
    /// the `efd/Matrix3.h` header comment, column vectors `M · v`) to a
    /// Z-up vector as `RX · (RY · (RZ · v))` — `FromEulerAnglesXYZ`'s
    /// product as disassembled from `efd.lib`. Independent of glam and of
    /// the dispatcher, so it can arbitrate the composition order.
    fn gamebryo_from_euler_xyz_apply(rx: f32, ry: f32, rz: f32, v: [f32; 3]) -> [f32; 3] {
        let mul = |m: [[f32; 3]; 3], v: [f32; 3]| {
            [
                m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
                m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
                m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
            ]
        };
        let (sx, cx) = rx.sin_cos();
        let (sy, cy) = ry.sin_cos();
        let (sz, cz) = rz.sin_cos();
        let m_x = [[1.0, 0.0, 0.0], [0.0, cx, sx], [0.0, -sx, cx]];
        let m_y = [[cy, 0.0, -sy], [0.0, 1.0, 0.0], [sy, 0.0, cy]];
        let m_z = [[cz, sz, 0.0], [-sz, cz, 0.0], [0.0, 0.0, 1.0]];
        mul(m_x, mul(m_y, mul(m_z, v)))
    }

    /// The header's own worked example pins the CW sign: "a 90-degree
    /// rotation of [0,1,0] about [0,0,1] gives the vector [1,0,0]".
    #[test]
    fn gamebryo_reference_matrices_match_header_example() {
        let out = gamebryo_from_euler_xyz_apply(0.0, 0.0, std::f32::consts::FRAC_PI_2, [0.0, 1.0, 0.0]);
        assert!((out[0] - 1.0).abs() < 1e-6 && out[1].abs() < 1e-6 && out[2].abs() < 1e-6, "{out:?}");
    }

    /// Regression pin for the Gamebryo XYZ-product convention against
    /// the independent reference above, over multi-axis triples where
    /// the XYZ and ZYX products diverge (they agree whenever only one
    /// angle is non-zero — the trap behind both the 2026-05-07 and the
    /// 2026-05-26 flips). Includes the `GSProspectorSaloonInterior`
    /// window-wall REFR `00108729` at (-90°, -90°, 270°).
    #[test]
    fn euler_matches_gamebryo_from_euler_angles_xyz() {
        use std::f32::consts::{FRAC_PI_2, PI};
        let triples = [
            (FRAC_PI_2, FRAC_PI_2, 0.0),
            (FRAC_PI_2, 0.0, FRAC_PI_2),
            (-FRAC_PI_2, -FRAC_PI_2, 1.5 * PI),
            (0.3, -1.2, 2.4),
            (-0.7, 0.9, -0.15),
        ];
        let probes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.3, -0.5, 0.8]];
        for (rx, ry, rz) in triples {
            let q = euler_zup_to_quat_yup(rx, ry, rz);
            for v in probes {
                let want = zup_to_yup_pos(gamebryo_from_euler_xyz_apply(rx, ry, rz, v));
                let got = q * Vec3::from(zup_to_yup_pos(v));
                assert!(
                    (got - Vec3::from(want)).length() < 1e-5,
                    "({rx}, {ry}, {rz}) applied to {v:?}: got {got:?}, Gamebryo gives {want:?}"
                );
            }
        }
    }

    /// The ZYX product (`--rotation-mode 1`, the 2026-05-26 ship) must
    /// disagree with Gamebryo on a multi-axis triple — otherwise the pin
    /// above couldn't tell the two apart.
    #[test]
    fn zyx_product_disagrees_with_gamebryo_on_multi_axis() {
        use std::f32::consts::FRAC_PI_2;
        let q = euler_zup_to_quat_yup_mode(1, FRAC_PI_2, 0.0, FRAC_PI_2);
        let v = [0.0, 1.0, 0.0];
        let want = Vec3::from(zup_to_yup_pos(gamebryo_from_euler_xyz_apply(FRAC_PI_2, 0.0, FRAC_PI_2, v)));
        let got = q * Vec3::from(zup_to_yup_pos(v));
        assert!((got - want).length() > 0.5, "mode 1 unexpectedly matched Gamebryo: {got:?}");
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

    /// Angle between two unit quaternions, sign-insensitive (`q == -q`).
    /// Mirrors the `cell_rot_sweep` example's comparison.
    fn quat_angle(a: Quat, b: Quat) -> f32 {
        2.0 * a.dot(b).abs().clamp(-1.0, 1.0).acos()
    }

    /// #2438 / COORD-5 — the shipping helper must stay byte-identical to
    /// the dispatcher's shipping mode, so the default `--rotation-mode`
    /// and every non-REFR caller agree by construction rather than by
    /// convention.
    #[test]
    fn ship_helper_equals_ship_mode() {
        assert_eq!(REFR_ROTATION_MODE_SHIP, 0);
        for (rx, ry, rz) in [
            (0.0_f32, 0.0, 0.0),
            (0.3, -1.2, 2.4),
            (-0.7, 0.9, -0.15),
            (1.5707964, 0.5, -2.0),
        ] {
            let ship = euler_zup_to_quat_yup(rx, ry, rz);
            let ship_mode = euler_zup_to_quat_yup_mode(REFR_ROTATION_MODE_SHIP, rx, ry, rz);
            assert_eq!(
                ship.to_array(),
                ship_mode.to_array(),
                "ship helper diverged from the ship mode at ({rx}, {ry}, {rz})"
            );
        }
    }

    /// The historical trap the 2026-05-07 `GSDocMitchellHouse` sign-off
    /// fell into: modes 0 and 1 are indistinguishable on Z-only REFRs
    /// and diverge only once a second axis is non-zero. Pinning both
    /// halves keeps the sweep example's premise honest.
    #[test]
    fn modes_zero_and_one_agree_only_on_z_only_rotations() {
        let z_only = quat_angle(
            euler_zup_to_quat_yup_mode(0, 0.0, 0.0, 1.1),
            euler_zup_to_quat_yup_mode(1, 0.0, 0.0, 1.1),
        );
        assert!(z_only < 1e-4, "Z-only modes 0/1 must agree, got {z_only}");

        let multi_axis = quat_angle(
            euler_zup_to_quat_yup_mode(0, 0.4, 0.0, 1.1),
            euler_zup_to_quat_yup_mode(1, 0.4, 0.0, 1.1),
        );
        assert!(
            multi_axis > 1e-3,
            "multi-axis modes 0/1 must diverge, got {multi_axis}"
        );
    }

    /// An out-of-range `--rotation-mode` must land on the shipping
    /// formula, not on garbage placement.
    #[test]
    fn out_of_range_mode_falls_back_to_ship() {
        for mode in [4u8, 9, 255] {
            assert_eq!(
                euler_zup_to_quat_yup_mode(mode, 0.4, -0.8, 1.1).to_array(),
                euler_zup_to_quat_yup(0.4, -0.8, 1.1).to_array(),
                "mode {mode} must fall back to the shipping formula"
            );
        }
    }
}
