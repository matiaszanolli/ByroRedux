//! Z-up (Bethesda) → Y-up (engine) Euler-angle → quaternion conversion
//! helpers used by REFR placement and XCLL directional lighting.

use byroredux_core::math::Quat;

/// Convert Euler angles (radians, Z-up Bethesda convention) to a Y-up quaternion.
///
/// Bethesda uses Gamebryo's **clockwise-positive** rotation convention:
///   R_zup = Rx_cw(rx) · Ry_cw(ry) · Rz_cw(rz)
///
/// Note the composition order: **Z applied first, X applied last**
/// (rightmost matrix in the product is applied first to the column
/// vector). This is the `FromEulerAnglesXYZ` ordering Gamebryo's
/// `Matrix3.h` exposes, and matches xEdit's `EulerToM33`
/// (`Core/wbNifMath.pas:387`) decomposition. Pre-2026-05-07 this
/// helper used the opposite (`Rz · Ry · Rx`) ordering, which produced
/// correct results for axis-aligned rotations but rotated combined-
/// Euler REFRs by 90°-quantised orthogonal swaps — the symptom that
/// surfaced as misplaced large statics. See `196dd67` for the
/// `--rotation-mode` triage that pinned this.
///
/// Since glam uses the standard counter-clockwise convention, each
/// CW rotation by angle t equals a CCW rotation by -t:
///   R_zup = Rx_ccw(-rx) · Ry_ccw(-ry) · Rz_ccw(-rz)
///
/// Coordinate change C: (x,y,z)_zup → (x,z,-y)_yup conjugates each:
///   C · Rx(-rx) · C^T = Rx(-rx)     (x → x)
///   C · Ry(-ry) · C^T = Rz(ry)      (y → -z, double negate)
///   C · Rz(-rz) · C^T = Ry(-rz)     (z → y)
///
/// Result: R_yup = Rx(-rx) · Rz(ry) · Ry(-rz)
///
/// `pub(crate)` so non-REFR callers (XCLL directional lighting in
/// `scene.rs`, #380) can route authored Z-up Euler angles through the
/// same CW-convention helper instead of reinventing the spherical
/// math inline and drifting from the authored intent.
pub(crate) fn euler_zup_to_quat_yup(rx: f32, ry: f32, rz: f32) -> Quat {
    Quat::from_rotation_x(-rx) * Quat::from_rotation_z(ry) * Quat::from_rotation_y(-rz)
}

/// Diagnostic switch for the REFR Euler→Y-up quaternion conversion.
///
/// 2026-05-07 triage on `GSDocMitchellHouse` pinned **CW + XYZ-product**
/// as the correct composition (operator A/B compared all four
/// candidates against the in-game pose). The default helper
/// [`euler_zup_to_quat_yup`] now uses that formula directly. The
/// diagnostic stays in tree because empirical sign-off was on a
/// single cell — if a future REFR placement disagrees with FNVEdit
/// or in-game, an operator can re-run the four candidates without
/// rewiring the engine.
///
/// `--rotation-mode N` (default 0 = shipping CW+XYZ) switches the
/// REFR placement path through the alternate compositions:
///   0: CW + XYZ-product  (current ship — `Rx · Ry · Rz` in Z-up)
///   1: CW + ZYX-product  (pre-2026-05-07 ship — `Rz · Ry · Rx`)
///   2: CCW + ZYX-product (no angle negation, ZYX order)
///   3: CCW + XYZ-product (no angle negation, XYZ order)
///
/// Other call sites (XCLL directional lighting in `scene.rs`) go
/// through [`euler_zup_to_quat_yup`] directly, not this dispatcher,
/// so mode changes don't accidentally move the sun.
static REFR_ROTATION_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn set_refr_rotation_mode_diag(mode: u8) {
    REFR_ROTATION_MODE.store(mode, std::sync::atomic::Ordering::Relaxed);
}

/// Diagnostic-mode-aware variant of [`euler_zup_to_quat_yup`] used
/// only on the REFR placement code path.
pub(crate) fn euler_zup_to_quat_yup_refr(rx: f32, ry: f32, rz: f32) -> Quat {
    use std::sync::atomic::Ordering;
    match REFR_ROTATION_MODE.load(Ordering::Relaxed) {
        // Mode 0 — current shipping. CW + XYZ-product (Z applied
        // first). R_zup = Rx_cw · Ry_cw · Rz_cw  ⇒
        // R_yup = Rx(-rx) · Rz(ry) · Ry(-rz).
        0 => Quat::from_rotation_x(-rx) * Quat::from_rotation_z(ry) * Quat::from_rotation_y(-rz),
        // Mode 1 — pre-2026-05-07 ship. CW + ZYX-product (X first,
        // Z last). R_zup = Rz_cw · Ry_cw · Rx_cw  ⇒
        // R_yup = Ry(-rz) · Rz(ry) · Rx(-rx).
        1 => Quat::from_rotation_y(-rz) * Quat::from_rotation_z(ry) * Quat::from_rotation_x(-rx),
        // Mode 2 — CCW + ZYX-product (no angle negation).
        // R_zup = Rz · Ry · Rx ⇒ R_yup = Ry(rz) · Rz(-ry) · Rx(rx).
        2 => Quat::from_rotation_y(rz) * Quat::from_rotation_z(-ry) * Quat::from_rotation_x(rx),
        // Mode 3 — CCW + XYZ-product.
        // R_zup = Rx · Ry · Rz ⇒ R_yup = Rx(rx) · Rz(-ry) · Ry(rz).
        3 => Quat::from_rotation_x(rx) * Quat::from_rotation_z(-ry) * Quat::from_rotation_y(rz),
        // Unknown mode — fall back to shipping default.
        _ => Quat::from_rotation_x(-rx) * Quat::from_rotation_z(ry) * Quat::from_rotation_y(-rz),
    }
}
