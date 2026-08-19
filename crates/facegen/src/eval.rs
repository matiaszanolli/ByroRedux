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
        if !coeff.is_finite() {
            // #3048 — `finite × finite` can still overflow to ±inf
            // (huge slider weight × huge morph scale). Catching it
            // here is a cheap per-morph skip, not the safety net —
            // the mandatory per-vertex finiteness check after the
            // loop below is what actually guarantees no non-finite
            // vertex escapes this function, since `coeff * d[k]` or
            // the running accumulation could still overflow even
            // when `coeff` itself is finite.
            continue;
        }
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
    // #3048 (SAFE-2026-08-16-01) — guard the OUTPUT, not only the
    // inputs. Every guard above checks an input (`w`, `scale`,
    // `coeff`, `d`); none of them can prove the accumulated sum
    // stays finite, since `coeff * d[k]` or the running per-component
    // total across many morphs can overflow even when every guarded
    // term along the way was individually finite. This single
    // O(vertices) pass is the actual safety net: it's the last stop
    // before `out` reaches the vertex SSBO and `build_blas_for_mesh`,
    // and the Vulkan spec requires finite BLAS vertex data — feeding
    // it ±inf/NaN is driver-dependent undefined behaviour, not a
    // clean error.
    //
    // Reject-per-vertex, not clamp: a non-finite result means at
    // least one contributing morph was corrupt or malicious, and we
    // have no principled bound to clamp to. Falling back to the
    // undeformed base position is the same "treat corruption
    // conservatively" posture the weight/scale/delta guards above
    // already use — this NPC's face just doesn't apply that vertex's
    // deformation, instead of crashing the driver.
    for (out_vertex, base_vertex) in out.iter_mut().zip(base_positions.iter()) {
        if !out_vertex[0].is_finite() || !out_vertex[1].is_finite() || !out_vertex[2].is_finite() {
            *out_vertex = *base_vertex;
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

    // ── #3048 (SAFE-2026-08-16-01) — output-finiteness regression ──

    #[test]
    fn overflowing_coeff_is_rejected_before_the_vertex_loop() {
        // weight * scale overflows to +inf even though both inputs
        // individually pass the existing is_finite() input guards.
        // Caught by the cheap per-morph coeff check, not the output
        // safety net — this pins that early-out specifically.
        let base = vec![[5.0, 5.0, 5.0]];
        let morphs = vec![morph(f32::MAX, vec![[1.0, 1.0, 1.0]])];
        let out = apply_morphs(&base, &morphs, &[f32::MAX]);
        assert_eq!(out, base, "overflowing coeff must not deform the vertex");
    }

    #[test]
    fn overflow_from_coeff_times_finite_delta_falls_back_to_base() {
        // `coeff` alone is finite (1.0) and `d` alone is finite
        // (f32::MAX) — every existing INPUT guard passes. The product
        // `coeff * d[k]` is what overflows, which only the mandatory
        // post-loop output check catches. This is the exact gap the
        // issue describes: input-only guarding lets this through.
        let base = vec![[0.0, 0.0, 0.0]];
        let morphs = vec![morph(1.0, vec![[f32::MAX, 0.0, 0.0]])];
        let out = apply_morphs(&base, &morphs, &[2.0]);
        assert_eq!(
            out, base,
            "coeff*delta overflow must fall back to the base position, \
             not leak ±inf into the returned vertex array"
        );
    }

    #[test]
    fn accumulation_overflow_across_many_finite_terms_falls_back_to_base() {
        // Every individual morph's contribution is finite and
        // in-range on its own; only the RUNNING SUM across many
        // morphs overflows. Neither the input guards nor the
        // per-morph coeff check can see this — only the accumulated
        // per-vertex output check does.
        let base = vec![[0.0, 0.0, 0.0]];
        let big = f32::MAX / 2.0;
        let morphs = vec![
            morph(1.0, vec![[big, 0.0, 0.0]]),
            morph(1.0, vec![[big, 0.0, 0.0]]),
            morph(1.0, vec![[big, 0.0, 0.0]]),
        ];
        let out = apply_morphs(&base, &morphs, &[1.0, 1.0, 1.0]);
        assert_eq!(
            out, base,
            "accumulated overflow across finite-only terms must still \
             fall back to the base position"
        );
    }

    #[test]
    fn finite_result_is_returned_unchanged() {
        // Sanity check alongside the overflow tests above: a normal,
        // fully-finite deformation must NOT be touched by the new
        // output-finiteness pass.
        let base = vec![[1.0, 2.0, 3.0]];
        let morphs = vec![morph(2.0, vec![[0.5, -0.25, 0.1]])];
        let out = apply_morphs(&base, &morphs, &[3.0]);
        assert!((out[0][0] - 4.0).abs() < 1e-4);
        assert!((out[0][1] - 0.5).abs() < 1e-4);
        assert!((out[0][2] - 3.6).abs() < 1e-4);
    }

    #[test]
    fn overflow_in_one_vertex_does_not_affect_siblings() {
        // Only vertex 0's delta overflows; vertex 1's deformation
        // must still land normally — the fallback is per-vertex, not
        // "reject the whole morph".
        let base = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
        let morphs = vec![morph(1.0, vec![[f32::MAX, 0.0, 0.0], [1.0, 0.0, 0.0]])];
        let out = apply_morphs(&base, &morphs, &[2.0]);
        assert_eq!(out[0], base[0], "overflowing vertex falls back to base");
        assert_eq!(
            out[1],
            [2.0, 0.0, 0.0],
            "sibling vertex still deforms normally"
        );
    }
}
