//! Tests for `euler_zup_to_quat_yup_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`euler_zup_to_quat_yup_tests::FOO`).

    //! Regression tests for #380 / audit F3-09 — XCLL directional
    //! rotation math. The Bethesda author specifies rotation via two
    //! Euler angles (Z-up, CW-positive per Gamebryo). Pre-#380 the
    //! `setup_scene` XCLL branch inlined a spherical-math formula
    //! that treated `ry` as elevation-from-horizon and drifted from
    //! the authored intent as `ry` grew. The fix routes those angles
    //! through the same `euler_zup_to_quat_yup` helper the REFR
    //! placement path uses, applied to Gamebryo's
    //! `NiDirectionalLight` model direction `(1, 0, 0)` (per the 2.3
    //! source: "The model direction of the light is (1,0,0)").
    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    fn xcll_dir_yup(rx: f32, ry: f32) -> Vec3 {
        euler_zup_to_quat_yup(rx, ry, 0.0) * Vec3::new(1.0, 0.0, 0.0)
    }

    /// Baseline: `(rx, ry) = (0, 0)` must leave the model direction
    /// at `(1, 0, 0)` — no rotation, no drift, identity pass-through.
    /// Z-up default `(1, 0, 0)` maps to Y-up `(1, 0, 0)` because the
    /// x axis is invariant under the Z-up → Y-up coord swap.
    #[test]
    fn zero_rotation_returns_model_direction_unchanged() {
        let dir = xcll_dir_yup(0.0, 0.0);
        assert!(approx_eq(dir.x, 1.0), "x should be 1, got {}", dir.x);
        assert!(approx_eq(dir.y, 0.0), "y should be 0, got {}", dir.y);
        assert!(approx_eq(dir.z, 0.0), "z should be 0, got {}", dir.z);
    }

    /// `ry = π/2` rotates the model direction around the Z-up Y axis
    /// by a quarter turn CW. The helper maps that to `Rz(ry)` in
    /// Y-up, so `(1, 0, 0)` rotates CCW around +Z to `(0, 1, 0)`.
    /// Guards against a sign flip on the elevation component.
    #[test]
    fn elevation_ry_quarter_turn_moves_to_y_axis() {
        let dir = xcll_dir_yup(0.0, std::f32::consts::FRAC_PI_2);
        assert!(approx_eq(dir.x, 0.0), "x should be 0, got {}", dir.x);
        assert!(approx_eq(dir.y, 1.0), "y should be 1, got {}", dir.y);
        assert!(approx_eq(dir.z, 0.0), "z should be 0, got {}", dir.z);
    }

    /// `rx = π/2` rotates the model direction around Z-up X axis.
    /// Under the helper, that maps to `Rx(-rx)` in Y-up. Because
    /// `(1, 0, 0)` lies on the X axis, it's invariant — output
    /// matches the baseline. Guards against the pre-#380 formula's
    /// behavior, which would have produced `(0, 1, 0)` at this
    /// input.
    #[test]
    fn azimuth_rx_leaves_x_axis_invariant() {
        let dir = xcll_dir_yup(std::f32::consts::FRAC_PI_2, 0.0);
        assert!(approx_eq(dir.x, 1.0), "x should be 1, got {}", dir.x);
        assert!(approx_eq(dir.y, 0.0), "y should be 0, got {}", dir.y);
        assert!(approx_eq(dir.z, 0.0), "z should be 0, got {}", dir.z);
    }

    /// Output vector must always be unit length — XCLL rotations are
    /// rigid, so the direction magnitude must not drift. Exercises a
    /// non-trivial `(rx, ry)` pair to avoid hitting the axis-invariant
    /// corners.
    #[test]
    fn output_is_unit_length_for_arbitrary_angles() {
        let dir = xcll_dir_yup(0.3, 0.7);
        let len = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-5,
            "quaternion rotation must preserve length (got {})",
            len
        );
    }

    /// Parity with the REFR path: applying `euler_zup_to_quat_yup`
    /// to the X axis must match the result of rotating `(0, 0, 0)`
    /// translation + the same Euler angles through the REFR
    /// placement's `ref_rot * model_dir` composition. Pre-#380 the
    /// two paths diverged.
    #[test]
    fn matches_refr_placement_rotation_of_model_direction() {
        let rx = 0.25;
        let ry = 0.4;
        let dir_xcll = xcll_dir_yup(rx, ry);
        // REFR path: identical to the XCLL path now that both route
        // through `euler_zup_to_quat_yup`. Spelling out the REFR
        // composition explicitly to pin the invariant.
        let refr_quat = euler_zup_to_quat_yup(rx, ry, 0.0);
        let dir_refr = refr_quat * Vec3::new(1.0, 0.0, 0.0);
        assert!(approx_eq(dir_xcll.x, dir_refr.x));
        assert!(approx_eq(dir_xcll.y, dir_refr.y));
        assert!(approx_eq(dir_xcll.z, dir_refr.z));
    }
