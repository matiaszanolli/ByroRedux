//! Z-up (Bethesda) → Y-up (engine) Euler-angle → quaternion conversion
//! helpers used by REFR placement and XCLL directional lighting.
//!
//! The shipping default ([`euler_zup_to_quat_yup`]) is a thin re-export
//! of [`byroredux_core::math::coord::euler_zup_to_quat_yup`] — the
//! single source of truth post-#1044 / TD3-003 — so non-REFR callers
//! (XCLL directional in `scene.rs`, #380) route through one canonical
//! formula. The diagnostic-mode dispatcher stays in this module
//! because the four-candidate triage is REFR-placement-specific and
//! the `2026-05-07 GSDocMitchellHouse` sign-off was on a single cell.

use byroredux_core::math::Quat;

/// Convert Euler angles (radians, Z-up Bethesda convention) to a Y-up
/// quaternion. See [`byroredux_core::math::coord::euler_zup_to_quat_yup`]
/// for the full derivation; this is `pub(crate)` so the existing
/// REFR / XCLL call sites keep their qualified path.
pub(crate) fn euler_zup_to_quat_yup(rx: f32, ry: f32, rz: f32) -> Quat {
    byroredux_core::math::coord::euler_zup_to_quat_yup(rx, ry, rz)
}

/// Diagnostic switch for the REFR Euler→Y-up quaternion conversion.
///
/// 2026-05-26: ship default flipped from mode 0 (XYZ-product) to mode 1
/// (ZYX-product) after cross-referencing OpenMW's canonical static-REFR
/// placement at `apps/openmw/mwrender/objectpaging.cpp:853-855`. The
/// previous default was empirically picked from a single-cell sign-off
/// on `GSDocMitchellHouse` (2026-05-07) — that cell's REFRs are
/// dominated by Z-only rotations, where XYZ and ZYX products produce
/// identical results. Multi-axis REFRs (slope-tilted exterior walls /
/// sloped architecture) exposed the difference: walls displaced or
/// rotated 90°. ZYX-product matches OpenMW + Bethesda CK / xEdit
/// documented "rotate around X first, then Y, then Z in object-local
/// axes" convention.
///
/// The diagnostic stays in tree as a runtime A/B knob in case any
/// scene disagrees with FNVEdit / in-game — an operator can re-run
/// the four candidates without rewiring the engine.
///
/// `--rotation-mode N` (default 1 = shipping CW+ZYX, OpenMW-derived):
///   0: CW + XYZ-product  (pre-2026-05-26 ship — `Rx · Ry · Rz` in Z-up; preserved for A/B)
///   1: CW + ZYX-product  (current ship — `Rz · Ry · Rx` in Z-up; matches OpenMW)
///   2: CCW + ZYX-product (no angle negation, ZYX order)
///   3: CCW + XYZ-product (no angle negation, XYZ order)
///
/// Other call sites (XCLL directional lighting in `scene.rs`) go
/// through [`euler_zup_to_quat_yup`] directly, not this dispatcher,
/// so the helper is the single source of truth; this dispatcher's
/// mode-0/2/3 paths are kept for diagnostic-only triage.
static REFR_ROTATION_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

pub fn set_refr_rotation_mode_diag(mode: u8) {
    REFR_ROTATION_MODE.store(mode, std::sync::atomic::Ordering::Relaxed);
}

/// Diagnostic-mode-aware variant of [`euler_zup_to_quat_yup`] used
/// only on the REFR placement code path.
pub(crate) fn euler_zup_to_quat_yup_refr(rx: f32, ry: f32, rz: f32) -> Quat {
    use std::sync::atomic::Ordering;
    match REFR_ROTATION_MODE.load(Ordering::Relaxed) {
        // Mode 0 — pre-2026-05-26 ship (deprecated; kept for A/B
        // triage only). CW + XYZ-product (Z applied first).
        // R_zup = Rx_cw · Ry_cw · Rz_cw ⇒
        // R_yup = Rx(-rx) · Rz(ry) · Ry(-rz). Agrees with mode 1
        // for Z-only rotations (why GSDocMitchellHouse passed
        // sign-off); diverges for multi-axis REFRs.
        0 => Quat::from_rotation_x(-rx) * Quat::from_rotation_z(ry) * Quat::from_rotation_y(-rz),
        // Mode 1 — current ship. CW + ZYX-product (X applied first,
        // Z applied last). R_zup = Rz_cw · Ry_cw · Rx_cw ⇒
        // R_yup = Ry(-rz) · Rz(ry) · Rx(-rx). Matches OpenMW's
        // canonical formula at objectpaging.cpp:853-855 and
        // Bethesda CK convention.
        1 => Quat::from_rotation_y(-rz) * Quat::from_rotation_z(ry) * Quat::from_rotation_x(-rx),
        // Mode 2 — CCW + ZYX-product (no angle negation).
        // R_zup = Rz · Ry · Rx ⇒ R_yup = Ry(rz) · Rz(-ry) · Rx(rx).
        2 => Quat::from_rotation_y(rz) * Quat::from_rotation_z(-ry) * Quat::from_rotation_x(rx),
        // Mode 3 — CCW + XYZ-product.
        // R_zup = Rx · Ry · Rz ⇒ R_yup = Rx(rx) · Rz(-ry) · Ry(rz).
        3 => Quat::from_rotation_x(rx) * Quat::from_rotation_z(-ry) * Quat::from_rotation_y(rz),
        // Unknown mode — fall back to shipping default (mode 1).
        _ => Quat::from_rotation_y(-rz) * Quat::from_rotation_z(ry) * Quat::from_rotation_x(-rx),
    }
}
