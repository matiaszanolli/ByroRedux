//! FaceGen morph evaluation — turns slider values + `.egm` deltas
//! into a deformed copy of the base race head's vertex array.
//!
//! Phase 3b ships the symmetric (FGGS) path; Phase 3c layers the
//! asymmetric (FGGA) deformation on top through the same evaluator
//! (the math is identical, only the slider table and morph subset
//! change).
//!
//! ## Math
//!
//! For each vertex `i` in the base mesh:
//!
//! ```text
//! v_i' = v_i + Σ_j  weights[j] * morphs[j].scale * morphs[j].deltas[i]
//! ```
//!
//! Where `j` indexes the morph table (0..50 sym, 0..30 asym) and
//! `weights[j]` is the matching slider value from
//! `NpcRecord.runtime_facegen.fggs[j]` / `.fgga[j]`.
//!
//! ## NaN guard
//!
//! Vanilla FNV `headhuman.egm` carries non-finite half-float bit
//! patterns on some delta entries — verified empirically on
//! 2026-04-29 (see the `parse_real_facegen` integration test).
//! Multiplying any non-finite component by a slider weight propagates
//! NaN to the deformed vertex, then to the GPU. The evaluator skips
//! non-finite contributions silently — the assumption is that
//! FaceGen used NaN as a "no displacement" sentinel rather than
//! authoring intent. If a delta entry is finite, it gets applied
//! verbatim.

use crate::EgmMorph;

/// Apply a slider-weighted sum of `.egm` deltas to a base-mesh
/// vertex array.
///
/// Returns a new `Vec<[f32; 3]>` with the deformed positions. The
/// base array is left untouched (so the caller can keep using it for
/// other NPCs of the same race).
///
/// `morphs.len()` and `weights.len()` MUST agree. Excess weights
/// past `morphs.len()` are ignored (defensive against
/// `runtime_facegen.fggs` arrays sized for the legacy 50-slot table
/// when a mod-modified `.egm` ships fewer morphs); excess morphs
/// past `weights.len()` are silently treated as zero-weighted.
///
/// ## Coordinate frame
///
/// Deltas in the `.egm` file live in the same NIF-local coordinate
/// frame as the base vertices. The Z-up→Y-up conversion that the
/// renderer applies happens at the placement-root level
/// (`cell_loader.rs:864-877`), not at the vertex level — so this
/// evaluator does not touch axis ordering.
///
/// ## Performance
///
/// Inner loop is `O(num_morphs × num_vertices)`. For vanilla FNV
/// (1449 vertices, 50 sym + 30 asym morphs combined), that's
/// ~115 800 multiply-adds per NPC. Negligible at cell-load time;
/// not a hot path.
pub fn apply_morphs(
    base_positions: &[[f32; 3]],
    morphs: &[EgmMorph],
    weights: &[f32],
) -> Vec<[f32; 3]> {
    let mut out: Vec<[f32; 3]> = base_positions.to_vec();
    let n = morphs.len().min(weights.len());
    for j in 0..n {
        let w = weights[j];
        if w == 0.0 || !w.is_finite() {
            // Weight=0 contributes nothing; weight=NaN means the
            // ESM record carries a non-finite slider value — almost
            // certainly content corruption, but treating as zero is
            // the conservative recovery.
            continue;
        }
        let scale = morphs[j].scale;
        if !scale.is_finite() {
            continue;
        }
        let coeff = w * scale;
        let m = &morphs[j].deltas;
        // Defensive bound: applying past `out.len()` would index
        // out of range. Caller should pass matching base + morph
        // sizes, but malformed content shouldn't panic the cell
        // loader. Truncates to the shorter array.
        let limit = m.len().min(out.len());
        for i in 0..limit {
            let d = m[i];
            // Skip non-finite delta components (vanilla EGM authoring
            // sentinel — see module-level NaN guard).
            if !d[0].is_finite() || !d[1].is_finite() || !d[2].is_finite() {
                continue;
            }
            out[i][0] += coeff * d[0];
            out[i][1] += coeff * d[1];
            out[i][2] += coeff * d[2];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn morph(scale: f32, deltas: Vec<[f32; 3]>) -> EgmMorph {
        EgmMorph { scale, deltas }
    }

    #[test]
    fn zero_weights_leave_base_unchanged() {
        let base = vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let morphs = vec![morph(1.0, vec![[10.0, 0.0, 0.0], [0.0, 10.0, 0.0]])];
        let out = apply_morphs(&base, &morphs, &[0.0]);
        assert_eq!(out, base);
    }

    #[test]
    fn unit_weight_unit_scale_adds_delta_directly() {
        let base = vec![[1.0, 2.0, 3.0]];
        let morphs = vec![morph(1.0, vec![[0.5, -0.25, 0.1]])];
        let out = apply_morphs(&base, &morphs, &[1.0]);
        assert!((out[0][0] - 1.5).abs() < 1e-6);
        assert!((out[0][1] - 1.75).abs() < 1e-6);
        assert!((out[0][2] - 3.1).abs() < 1e-6);
    }

    #[test]
    fn scale_multiplies_delta() {
        let base = vec![[0.0, 0.0, 0.0]];
        let morphs = vec![morph(2.0, vec![[1.0, 0.0, 0.0]])];
        let out = apply_morphs(&base, &morphs, &[3.0]);
        // 0 + 3 * 2 * 1 = 6.0
        assert_eq!(out[0][0], 6.0);
    }

    #[test]
    fn multiple_morphs_sum_linearly() {
        let base = vec![[0.0, 0.0, 0.0]];
        let morphs = vec![
            morph(1.0, vec![[1.0, 0.0, 0.0]]),
            morph(1.0, vec![[0.0, 2.0, 0.0]]),
            morph(1.0, vec![[0.0, 0.0, 3.0]]),
        ];
        let out = apply_morphs(&base, &morphs, &[1.0, 1.0, 1.0]);
        assert_eq!(out[0], [1.0, 2.0, 3.0]);
    }

    #[test]
    fn nan_delta_skipped() {
        let base = vec![[5.0, 5.0, 5.0]];
        let morphs = vec![morph(1.0, vec![[f32::NAN, f32::NAN, f32::NAN]])];
        let out = apply_morphs(&base, &morphs, &[1.0]);
        // NaN delta means "no displacement"; vertex stays at base.
        assert_eq!(out, base);
    }

    #[test]
    fn nan_weight_skipped() {
        let base = vec![[5.0, 5.0, 5.0]];
        let morphs = vec![morph(1.0, vec![[1.0, 1.0, 1.0]])];
        let out = apply_morphs(&base, &morphs, &[f32::NAN]);
        assert_eq!(out, base);
    }

    #[test]
    fn weight_morph_count_mismatch_truncates() {
        let base = vec![[0.0, 0.0, 0.0]];
        let morphs = vec![
            morph(1.0, vec![[1.0, 0.0, 0.0]]),
            morph(1.0, vec![[0.0, 1.0, 0.0]]),
        ];
        // Only one weight; second morph silently zero-weighted.
        let out = apply_morphs(&base, &morphs, &[1.0]);
        assert_eq!(out[0], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn delta_length_shorter_than_base_doesnt_panic() {
        let base = vec![[0.0, 0.0, 0.0]; 5];
        let morphs = vec![morph(1.0, vec![[1.0, 0.0, 0.0]; 3])];
        let out = apply_morphs(&base, &morphs, &[1.0]);
        // First 3 vertices deformed; last 2 unchanged.
        assert_eq!(out[0], [1.0, 0.0, 0.0]);
        assert_eq!(out[2], [1.0, 0.0, 0.0]);
        assert_eq!(out[3], [0.0, 0.0, 0.0]);
        assert_eq!(out[4], [0.0, 0.0, 0.0]);
    }
}
