//! Z-up (Bethesda) → Y-up (engine) position and Euler-angle → quaternion
//! conversion helpers used by REFR placement (#6713: lifted out of
//! `cell_loader` so the loading screen, scripts and light animation can use
//! the math without depending on the cell loader).
//!
//! XCLL directional lighting deliberately does not use these helpers: its
//! two fields are a spherical azimuth/elevation pair, not a REFR Euler triple.

use byroredux_core::math::{Quat, Vec3};

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

/// Convert a Bethesda Z-up world-space position to engine Y-up.
/// Mirrors the convention used at REFR placement in `references.rs`:
/// `(x, z, -y)`. #1617 — delegates to the coord SoT so this stays in
/// lockstep with the canonical swap (bit-identical to the old inline form).
pub(crate) fn position_zup_to_yup(p: [f32; 3]) -> Vec3 {
    Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos(p))
}

/// Convert a Bethesda Z-up Euler rotation triple to an engine Y-up
/// quaternion. Wrapper over [`euler_zup_to_quat_yup_refr`] —
/// same convention REFR placements use, so the camera's orientation
/// matches what the destination REFR would render at.
///
/// #2435 / COORD-2 — this used to call the plain (non-dispatcher)
/// `euler_zup_to_quat_yup`, contradicting this doc comment. Zero
/// effect at the shipping default (`--rotation-mode 0`, CW + XYZ ≡ the
/// plain helper's formula), but under `--rotation-mode 1/2/3` the player
/// would land at a door with an orientation from a DIFFERENT
/// convention than the surrounding geometry — exactly the scenario
/// the diagnostic flag exists to triage, and XTEL was the one REFR-
/// family site that couldn't be triaged with it.
pub(crate) fn rotation_zup_to_yup_quat(rot: [f32; 3]) -> Quat {
    euler_zup_to_quat_yup_refr(rot[0], rot[1], rot[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirror of the Z-up → Y-up convention REFR placements use
    /// (`cell_loader/references/mod.rs`, via `zup_to_yup_pos`): `(x, y, z)_zup → (x, z, -y)_yup`.
    /// Pin the helper against three canonical orientations.
    #[test]
    fn position_zup_to_yup_matches_refr_convention() {
        // Forward (Bethesda +Y axis) → engine -Z.
        assert_eq!(
            position_zup_to_yup([0.0, 100.0, 0.0]),
            Vec3::new(0.0, 0.0, -100.0)
        );
        // Up (Bethesda +Z axis) → engine +Y.
        assert_eq!(
            position_zup_to_yup([0.0, 0.0, 50.0]),
            Vec3::new(0.0, 50.0, 0.0)
        );
        // Right (Bethesda +X axis) stays +X.
        assert_eq!(
            position_zup_to_yup([25.0, 0.0, 0.0]),
            Vec3::new(25.0, 0.0, 0.0)
        );
        // Mixed.
        assert_eq!(
            position_zup_to_yup([10.0, 20.0, 30.0]),
            Vec3::new(10.0, 30.0, -20.0)
        );
    }

    /// #2435 / COORD-2 regression — `rotation_zup_to_yup_quat`'s doc
    /// comment promises it wraps `euler_zup_to_quat_yup_refr` (the
    /// `--rotation-mode` dispatcher every other REFR-family site uses),
    /// but it used to call the plain, non-dispatcher `euler_zup_to_quat_yup`
    /// instead. A value-based test can't discriminate the two at the
    /// shipping default (`--rotation-mode 0`'s formula is bit-identical
    /// to the plain helper's), and mutating the dispatcher's shared
    /// `AtomicU8` mode from a test would race every other test in this
    /// crate that exercises REFR placement concurrently — so this is a
    /// static source check, the same shape this codebase already uses
    /// for logic a value-based unit test can't safely pin.
    #[test]
    fn rotation_zup_to_yup_quat_calls_the_refr_dispatcher_not_the_plain_helper() {
        let src = include_str!("zup_yup.rs");
        let fn_start = src
            .find("pub(crate) fn rotation_zup_to_yup_quat")
            .expect("rotation_zup_to_yup_quat must still exist");
        let fn_body = &src[fn_start..fn_start + 400.min(src.len() - fn_start)];
        assert!(
            fn_body.contains("euler_zup_to_quat_yup_refr("),
            "rotation_zup_to_yup_quat must call the `--rotation-mode` \
             dispatcher (euler_zup_to_quat_yup_refr), matching its own doc \
             comment and every other REFR-family call site — calling the \
             plain euler_zup_to_quat_yup instead silently opts XTEL \
             transitions out of `--rotation-mode` triage (#2435)"
        );
    }
}
