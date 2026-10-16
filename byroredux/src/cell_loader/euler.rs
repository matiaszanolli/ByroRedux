//! Z-up (Bethesda) → Y-up (engine) Euler-angle → quaternion conversion
//! helpers used by REFR placement.
//!
//! XCLL directional lighting deliberately does not use these helpers: its
//! two fields are a spherical azimuth/elevation pair, not a REFR Euler triple.

use byroredux_core::math::Quat;

/// Diagnostic switch for the REFR Euler→Y-up quaternion conversion.
///
/// 2026-10-16: ship default restored to mode 0 (CW + XYZ-product,
/// Gamebryo `FromEulerAnglesXYZ`). The 2026-05-26 flip to mode 1 read
/// OpenMW's `osg::Quat` expression as a Hamilton product; OSG
/// multiplies in reverse order, so OpenMW actually agrees with mode 0 —
/// as do the Gamebryo `efd.lib` disassembly and the CK wiki. Mode 1
/// misplaced every multi-axis REFR (~30% of FNV interior REFRs; the
/// `GSProspectorSaloonInterior` window walls). Full citations on
/// [`byroredux_core::math::coord::euler_zup_to_quat_yup`].
///
/// The diagnostic stays in tree as a runtime A/B knob in case any
/// scene disagrees with FNVEdit / in-game — an operator can re-run
/// the four candidates without rewiring the engine.
///
/// `--rotation-mode N` (default 0 = shipping CW+XYZ, Gamebryo):
///   0: CW + XYZ-product  (current ship — `Rx · Ry · Rz` in Z-up)
///   1: CW + ZYX-product  (2026-05-26 → 2026-10-16 ship — `Rz · Ry · Rx` in Z-up; preserved for A/B)
///   2: CCW + ZYX-product (no angle negation, ZYX order)
///   3: CCW + XYZ-product (no angle negation, XYZ order)
///
/// The mode-1/2/3 paths are kept for diagnostic-only triage.
static REFR_ROTATION_MODE: std::sync::atomic::AtomicU8 =
    std::sync::atomic::AtomicU8::new(byroredux_core::math::coord::REFR_ROTATION_MODE_SHIP);

pub fn set_refr_rotation_mode_diag(mode: u8) {
    REFR_ROTATION_MODE.store(mode, std::sync::atomic::Ordering::Relaxed);
}

/// Diagnostic-mode-aware Euler conversion used only on the REFR placement
/// code path.
///
/// The four formulas themselves live in
/// [`byroredux_core::math::coord::euler_zup_to_quat_yup_mode`] — this
/// function is only the atomic-backed mode selection. #2438 / COORD-5:
/// `crates/plugin/examples/cell_rot_sweep.rs` used to hand-copy the
/// match arms, so retuning the dispatcher would silently desync the
/// triage tool that exists to arbitrate exactly these disputes.
pub(crate) fn euler_zup_to_quat_yup_refr(rx: f32, ry: f32, rz: f32) -> Quat {
    use std::sync::atomic::Ordering;
    byroredux_core::math::coord::euler_zup_to_quat_yup_mode(
        REFR_ROTATION_MODE.load(Ordering::Relaxed),
        rx,
        ry,
        rz,
    )
}
