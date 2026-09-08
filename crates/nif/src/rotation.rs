//! Rotation matrix sanitization.
//!
//! Gamebryo NIF files occasionally contain degenerate rotation matrices
//! (rank-deficient, det≈0, or sheared/scaled) from bad exports or zeroed
//! BSFadeNode transforms. We repair these ONCE at parse time via SVD
//! ("nearest orthogonal matrix"), so downstream code (compose_transforms,
//! zup_matrix_to_yup_quat) can skip per-composition checks. See #277.

use crate::types::NiMatrix3;
use std::sync::atomic::{AtomicU32, Ordering};

/// Check if a rotation matrix is degenerate (det far from 1.0).
#[inline]
pub fn is_degenerate_rotation(m: &NiMatrix3) -> bool {
    let r = &m.rows;
    let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
        - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
        + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
    (det - 1.0).abs() >= 0.1
}

/// Check whether a matrix is an orthonormal *reflection* — a valid
/// orthogonal matrix with `det ≈ -1` rather than `+1` (a mirrored subtree,
/// e.g. `diag(-1, 1, 1)`).
///
/// #3532 — this class was silently folded into the scale/shear path.
/// [`is_non_orthonormal`] returns `false` for a reflection (its columns
/// ARE orthonormal), but [`is_degenerate_rotation`] returns `true`
/// (`|det - 1| = 2`), so it took the SVD branch and logged
/// "baked scale/shear … the singular value information is discarded",
/// which is factually wrong for it: every singular value is 1 and there is
/// no scale/shear to discard.
///
/// The distinction is load-bearing because SVD does not *repair* a
/// reflection. `repair_rotation_svd_or_identity`'s `det < 0` column flip
/// turns `diag(-1, 1, 1)` into `diag(-1, 1, -1)` — a 180° rotation about
/// Y, i.e. a **different orientation**, not an un-mirrored one. That is a
/// legitimate policy (a renderer cannot draw a mirrored basis without
/// flipping winding), but it changes orientation rather than losing
/// magnitude, and the log must say so.
#[inline]
pub fn is_reflection(m: &NiMatrix3) -> bool {
    if is_non_orthonormal(m) {
        return false;
    }
    let r = &m.rows;
    let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
        - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
        + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
    det < 0.0
}

/// Check if a rotation matrix's columns fail to form an orthonormal set —
/// i.e. the matrix encodes scale and/or shear beyond pure rotation, even
/// when its determinant lands inside [`is_degenerate_rotation`]'s "valid
/// rotation" window. A uniform-or-non-uniform scale whose singular values
/// multiply out to ≈1 (`diag(2, 0.5, 1)`, det = 1.0 exactly) is invisible
/// to a determinant-only check — this catches it via the columns'
/// lengths and pairwise dot products instead. See #2456.
///
/// The 0.1 tolerance (on squared column length / dot product, so ~10%)
/// deliberately sits at or above the numerical drift #333 already
/// treats as ordinary export-tool noise — e.g. a uniform 1.03× scale
/// (see `zup_to_yup_drifted_rotation_returns_unit_quaternion`) squares
/// to a 0.06 deviation and stays silent here. Only a matrix that
/// actually encodes deliberate, code-visible scale/shear content trips
/// this check.
#[inline]
pub fn is_non_orthonormal(m: &NiMatrix3) -> bool {
    let r = &m.rows;
    let col = |j: usize| [r[0][j], r[1][j], r[2][j]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (c0, c1, c2) = (col(0), col(1), col(2));
    const TOL: f32 = 0.1;
    (dot(c0, c0) - 1.0).abs() > TOL
        || (dot(c1, c1) - 1.0).abs() > TOL
        || (dot(c2, c2) - 1.0).abs() > TOL
        || dot(c0, c1).abs() > TOL
        || dot(c0, c2).abs() > TOL
        || dot(c1, c2).abs() > TOL
}

/// Caps how many "baked scale/shear discarded" warnings
/// [`sanitize_rotation`] logs per process. Real NIF corpora can carry
/// thousands of nodes; without a cap, one badly-exported model would
/// flood the log and drown out everything else.
///
/// #2456 asked this instrumentation to measure real corpus incidence
/// before committing to the larger "decompose into `NiTransform.scale`"
/// fix. **#3532 has that answer, and it is "not warranted".** Over 55 949
/// vanilla NIFs — `Oblivion - Meshes.bsa`, FO3 and FNV
/// `Fallout - Meshes.bsa`, `Skyrim - Meshes0.bsa` + `Meshes1.bsa` —
/// yielding **642 589** `NiTransform` rotation matrices:
///
/// | branch | hits |
/// |---|---|
/// | `is_degenerate_rotation` (SVD) | **1** |
/// | `is_non_orthonormal` pass-through | **0** |
///
/// The `diag(2, 0.5, 1)`-shaped baked-scale case the deferred
/// decomposition was designed for does not occur in any shipped Bethesda
/// title. The warning is kept — not as open instrumentation but as a
/// diagnostic for the non-Bethesda / mod content that is live scope
/// (#2383) — and it changes no parsed geometry or transform output.
static SCALE_DISCARD_WARNINGS: AtomicU32 = AtomicU32::new(0);
const MAX_SCALE_DISCARD_WARNINGS: u32 = 20;

/// #3532 — a reflection is a different defect from baked scale/shear and
/// gets its own message. Saying "the singular value information is
/// discarded" here would be false (all three singular values are 1), and it
/// would hide the part that actually matters: the subtree's ORIENTATION is
/// being changed, not its magnitude.
fn warn_reflection_reoriented(m: &NiMatrix3, repaired: &NiMatrix3) {
    let count = SCALE_DISCARD_WARNINGS.fetch_add(1, Ordering::Relaxed);
    if count >= MAX_SCALE_DISCARD_WARNINGS {
        return;
    }
    log::warn!(
        "NiTransform.rotation is a mirrored basis (orthonormal, det = -1) — SVD cannot \
         un-mirror it, so this subtree is REORIENTED, not rescaled: the repair flips the \
         third column and yields a different rotation (#3532). No scale/shear information \
         is involved. Matrix: {m:?} -> {repaired:?}"
    );
    if count + 1 == MAX_SCALE_DISCARD_WARNINGS {
        log::warn!("further rotation-sanitisation warnings suppressed for this process (#2456)");
    }
}

fn warn_scaled_rotation_discarded(m: &NiMatrix3, mode: &str) {
    let count = SCALE_DISCARD_WARNINGS.fetch_add(1, Ordering::Relaxed);
    if count >= MAX_SCALE_DISCARD_WARNINGS {
        return;
    }
    log::warn!(
        "NiTransform.rotation is non-orthonormal (baked scale/shear, {mode}) — the singular \
         value information is discarded rather than folded into NiTransform.scale (#2456); \
         this subtree's placement may be offset by the discarded factor. Matrix: {m:?}"
    );
    if count + 1 == MAX_SCALE_DISCARD_WARNINGS {
        log::warn!(
            "further \"baked scale/shear discarded\" rotation warnings suppressed for this \
             process (#2456)"
        );
    }
}

/// Longest column vector of `m` — used to tell a genuinely-scaled matrix
/// (worth measuring/warning about) apart from a truly degenerate one
/// (zeroed BSFadeNode transform, garbage export) that
/// [`repair_rotation_svd_or_identity`] already collapses to identity.
/// Mirrors that function's own `max_sv < 0.01` zeroed-matrix threshold.
#[inline]
fn max_column_length(m: &NiMatrix3) -> f32 {
    let r = &m.rows;
    (0..3)
        .map(|j| {
            let v = [r[0][j], r[1][j], r[2][j]];
            (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
        })
        .fold(0.0f32, f32::max)
}

/// SVD-repair a degenerate rotation matrix, or return identity if the matrix
/// has no meaningful orientation (all singular values near zero).
pub fn repair_rotation_svd_or_identity(m: &NiMatrix3) -> NiMatrix3 {
    use nalgebra::Matrix3;

    let r = &m.rows;
    let mat = Matrix3::new(
        r[0][0], r[0][1], r[0][2], r[1][0], r[1][1], r[1][2], r[2][0], r[2][1], r[2][2],
    );

    let svd = mat.svd(true, true);

    let max_sv = svd.singular_values.max();
    if max_sv < 0.01 {
        return NiMatrix3::default();
    }

    let u = svd.u.unwrap();
    let vt = svd.v_t.unwrap();
    let mut nearest = u * vt;

    if nearest.determinant() < 0.0 {
        let mut u_fixed = u;
        u_fixed.column_mut(2).scale_mut(-1.0);
        nearest = u_fixed * vt;
    }

    NiMatrix3 {
        rows: [
            [nearest[(0, 0)], nearest[(0, 1)], nearest[(0, 2)]],
            [nearest[(1, 0)], nearest[(1, 1)], nearest[(1, 2)]],
            [nearest[(2, 0)], nearest[(2, 1)], nearest[(2, 2)]],
        ],
    }
}

/// Sanitize a rotation matrix: pass-through for valid rotations (~99.9% of
/// NIF content), SVD-repair for degenerate ones. Call once at parse time
/// so downstream code can assume the matrix is a valid rotation.
///
/// #2456 — either branch can discard genuine scale/shear information
/// baked into the matrix by an exporter (the SVD branch orthogonalizes
/// it away; the pass-through branch admits `det≈1`-but-non-orthonormal
/// matrices like `diag(2, 0.5, 1)` untouched, and that information is
/// lost further downstream when `#333`'s unit-quaternion guard runs).
/// Neither branch folds the discarded factor into `NiTransform.scale`.
/// #3532 measured the incidence that decision was waiting on — 1 SVD hit
/// and 0 pass-through hits across 642 589 matrices in 55 949 vanilla
/// Oblivion / FO3 / FNV / Skyrim NIFs — so the decomposition is **not
/// warranted for any shipped Bethesda title** and is no longer "pending
/// data". The rate-limited warning stays as a diagnostic for the
/// non-Bethesda / mod content that is live scope (#2383).
///
/// A third case rides the SVD branch and is NOT scale/shear: an
/// orthonormal reflection (`det = -1`). It is classified first by
/// [`is_reflection`] and warned about separately, because SVD reorients
/// it rather than repairing it — see that function's doc.
#[inline]
pub fn sanitize_rotation(m: NiMatrix3) -> NiMatrix3 {
    if is_degenerate_rotation(&m) {
        // #3532 — classify BEFORE the scale/shear wording. A reflection is
        // orthonormal with det = -1: it trips `is_degenerate_rotation`
        // (|det - 1| = 2) but has no scale/shear to discard, and the repair
        // changes its orientation rather than its magnitude.
        if is_reflection(&m) {
            let repaired = repair_rotation_svd_or_identity(&m);
            warn_reflection_reoriented(&m, &repaired);
            return repaired;
        }
        if max_column_length(&m) >= 0.01 {
            warn_scaled_rotation_discarded(&m, "SVD-orthogonalized");
        }
        repair_rotation_svd_or_identity(&m)
    } else {
        if is_non_orthonormal(&m) {
            warn_scaled_rotation_discarded(
                &m,
                "determinant in valid-rotation window, passed through unchanged",
            );
        }
        m
    }
}

#[cfg(test)]
mod reflection_classification_tests {
    use super::*;

    fn m(rows: [[f32; 3]; 3]) -> NiMatrix3 {
        NiMatrix3 { rows }
    }

    /// #3532 — a pure reflection is orthonormal, so the scale/shear
    /// classifier must NOT claim it, while the determinant classifier does.
    /// That combination is what silently routed it into a message about
    /// discarded singular values it does not have.
    #[test]
    fn a_pure_reflection_is_orthonormal_but_still_trips_the_determinant_check() {
        let mirrored = m([[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        assert!(
            !is_non_orthonormal(&mirrored),
            "diag(-1, 1, 1) has unit, mutually perpendicular columns — there is no \
             scale or shear here to discard"
        );
        assert!(
            is_degenerate_rotation(&mirrored),
            "|det - 1| = 2, so it takes the SVD branch"
        );
        assert!(is_reflection(&mirrored));
    }

    /// The three classes stay disjoint where it matters: a genuine
    /// scale/shear matrix must not be claimed by the reflection arm, or it
    /// would get the wrong message in the other direction.
    #[test]
    fn scale_shear_and_ordinary_rotations_are_not_classified_as_reflections() {
        // det = 1 exactly, but columns are scaled — the case #2456 was
        // written for, and the one the corpus never produced.
        let baked_scale = m([[2.0, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, 0.0, 1.0]]);
        assert!(is_non_orthonormal(&baked_scale));
        assert!(!is_reflection(&baked_scale));

        let identity = m([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        assert!(!is_reflection(&identity));
        assert!(!is_degenerate_rotation(&identity));

        // 90° about Z — an ordinary rotation, det = +1.
        let rot_z = m([[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);
        assert!(!is_reflection(&rot_z));
        assert!(!is_degenerate_rotation(&rot_z));

        // A mirrored basis that ALSO carries scale is scale/shear first:
        // `is_reflection` defers to `is_non_orthonormal` so the more
        // specific message wins.
        let mirrored_and_scaled = m([[-2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        assert!(is_non_orthonormal(&mirrored_and_scaled));
        assert!(!is_reflection(&mirrored_and_scaled));
    }

    /// The reason the classification matters: SVD does not un-mirror a
    /// reflection. It produces a DIFFERENT ORIENTATION — here a 180°
    /// rotation about Y — which is a legitimate policy (a renderer cannot
    /// draw a mirrored basis without flipping winding) but is not what
    /// "the singular value information is discarded" describes.
    #[test]
    fn repairing_a_reflection_reorients_it_rather_than_un_mirroring_it() {
        let mirrored = m([[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        let repaired = sanitize_rotation(mirrored);

        let r = &repaired.rows;
        let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
            - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
            + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
        assert!(
            (det - 1.0).abs() < 1e-5,
            "the repair must hand downstream a proper rotation, det = {det}"
        );
        assert!(
            (r[0][0] + 1.0).abs() < 1e-5 && (r[2][2] + 1.0).abs() < 1e-5,
            "diag(-1, 1, 1) comes back as diag(-1, 1, -1) — a 180° turn about Y, \
             NOT an un-mirrored identity. Got {repaired:?}"
        );
        assert!(
            !is_degenerate_rotation(&repaired),
            "whatever it is, downstream must see a valid rotation (#277)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> NiMatrix3 {
        NiMatrix3::default()
    }

    #[test]
    fn identity_is_orthonormal() {
        assert!(!is_non_orthonormal(&identity()));
    }

    #[test]
    fn pure_rotation_is_orthonormal() {
        // 90deg rotation around Z — a genuine rotation, no scale.
        let rot_z90 = NiMatrix3 {
            rows: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        };
        assert!(!is_non_orthonormal(&rot_z90));
    }

    #[test]
    fn ordinary_export_drift_is_not_flagged() {
        // #333's documented ~3-7% per-axis drift case — must stay silent
        // so this diagnostic doesn't fire on every ordinary export.
        let drift = 1.03f32;
        let drifted = NiMatrix3 {
            rows: [[drift, 0.0, 0.0], [0.0, drift, 0.0], [0.0, 0.0, drift]],
        };
        assert!(!is_non_orthonormal(&drifted));
    }

    /// #2456 — the case `is_degenerate_rotation` cannot see: singular
    /// values multiply out to det=1.0 exactly, but the matrix is not a
    /// rotation.
    #[test]
    fn anisotropic_scale_with_unit_determinant_is_flagged_non_orthonormal() {
        let scaled = NiMatrix3 {
            rows: [[2.0, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, 0.0, 1.0]],
        };
        assert!((det(&scaled) - 1.0).abs() < 1e-6, "fixture must have det=1");
        assert!(
            !is_degenerate_rotation(&scaled),
            "det-only check must miss this"
        );
        assert!(is_non_orthonormal(&scaled), "column check must catch it");
    }

    #[test]
    fn uniform_scaled_identity_is_flagged_non_orthonormal() {
        let scaled_identity = NiMatrix3 {
            rows: [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 2.0]],
        };
        assert!(is_non_orthonormal(&scaled_identity));
    }

    #[test]
    fn zeroed_matrix_is_not_flagged_non_orthonormal() {
        // Zeroed BSFadeNode-style garbage — sanitize_rotation's det branch
        // handles this via repair_rotation_svd_or_identity's own
        // max_sv < 0.01 threshold; is_non_orthonormal is never consulted
        // for it (is_degenerate_rotation already catches det=0), but it
        // should not itself misreport a zero matrix as "scaled".
        let zero = NiMatrix3 {
            rows: [[0.0; 3]; 3],
        };
        assert!(is_degenerate_rotation(&zero));
    }

    #[test]
    fn sanitize_rotation_leaves_pure_rotations_unchanged() {
        let rot_z90 = NiMatrix3 {
            rows: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        };
        let out = sanitize_rotation(rot_z90);
        for i in 0..3 {
            for j in 0..3 {
                assert!((out.rows[i][j] - rot_z90.rows[i][j]).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn sanitize_rotation_orthogonalizes_far_from_unit_determinant() {
        // det = 8, well outside the fast-path window — SVD branch fires
        // and must return an orthonormal (near-identity) matrix.
        let scaled_identity = NiMatrix3 {
            rows: [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 2.0]],
        };
        let out = sanitize_rotation(scaled_identity);
        assert!(!is_degenerate_rotation(&out));
        assert!(!is_non_orthonormal(&out));
    }

    /// #2456 — the anisotropic-scale-with-unit-determinant case passes
    /// through `sanitize_rotation` UNCHANGED today (this pins the current,
    /// not-yet-fixed behaviour so a future decomposition fix has to
    /// deliberately update this test rather than silently pass).
    #[test]
    fn sanitize_rotation_does_not_yet_fold_unit_determinant_scale_into_output() {
        let scaled = NiMatrix3 {
            rows: [[2.0, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, 0.0, 1.0]],
        };
        let out = sanitize_rotation(scaled);
        for i in 0..3 {
            for j in 0..3 {
                assert!((out.rows[i][j] - scaled.rows[i][j]).abs() < 1e-6);
            }
        }
    }

    fn det(m: &NiMatrix3) -> f32 {
        let r = &m.rows;
        r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
            - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
            + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0])
    }
}
