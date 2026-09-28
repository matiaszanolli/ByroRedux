# #4940: REN-D2-2026-09-27-01: ReSTIR initial-candidate normalisation divides the shadow-casting direct term by the number of enumerated lights

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4940
- **Labels**: high,renderer,shaders,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D2-2026-09-27-01**._

- **Severity**: HIGH
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag` (fn `main`, ReSTIR stream `restirWSum += w_i; restirM += 1.0;` and finalize `restirW = min(restirWSum / (restirM * restirPHat), RESERVOIR_W_CLAMP)`, consumed by `frameContribution = rad * restirW * visibility`)
- **Status**: NEW. Not filed; searched `ReSTIR normalization`, `restirM` and `ReSTIR initial candidate M light count`. It is recorded as an open "estimator concern" in `docs/audits/PERFORMANCE_OPAQUE_EARLY_TESTS_2026-09-26.md` §"Separate normalization experiment, not retained", and noted "unchanged" in `PERFORMANCE_RESTIR_LIGHT_HISTORY_2026-09-26.md`.
- **Description**: Every shadow-casting light in the fragment's cluster is **enumerated** once as a candidate. The code adds weight `w_i = pHat_i` (luminance of `shadowableRadiance`) and increments `M` by 1 per candidate, then finalises `W = ΣpHat / (M · pHat_y)`. In RIS, the candidate weight is `pHat/p`, where `p` is the proposal pdf. Enumerating N lights is the uniform proposal `p = 1/N` with M = N, so the weight must be `N·pHat`. With the weight as written, `E[rad_y · W] = Σ_y (pHat_y/ΣpHat) · rad_y · ΣpHat/(N·pHat_y) = (1/N) Σ rad`. The estimator returns the **mean** of the shadowable lights' radiance, not the sum. Lights with `needsVisibility` contribute *only* through this estimate under ReSTIR (`if (!useRestir || !needsVisibility) Lo += shadowableRadiance;`), so nothing adds the rest back. The legacy-WRS arm, which the code's own tests cite as "the reference semantics being matched", normalises correctly: `W = resWSum / (K · w_sel)`, where K is the reservoir count and there is no candidate-count factor. Temporal and spatial combines add `M_r` in the standard way, so they carry the same 1/N scale forward rather than cancelling it.
- **Evidence**:
  - Two equal lights of radiance r: wSum = 2p, M = 2, W = 2p/(2p) = 1. Estimate r; truth 2r.
  - One light: W = 1. Correct, so single-light scenes and the Cornell single-emitter checks cannot see it.
  - The perf report's normalisation experiment "passed an added analytic total-energy test … It changed global lighting and was removed."
- **Impact**: In every cell with more than one shadow-casting light per cluster, the direct term of those lights is scaled by 1/N. The default `render/lights.rs` routes imported cell lights through full visibility, so this covers interiors in all games. Two effects follow:
  - Multi-light rooms render too dark on the direct term.
  - Because N is per-cluster, a light shared by adjacent clusters with different candidate counts contributes 1/N₁ versus 1/N₂. That is a potential cluster-grid-aligned brightness step (expected symptom, not observed; no engine run in this audit).
  - Any lighting tuning done since ReSTIR became default has been calibrated against the biased value.
- **Related**: #1369 (reservoir recompute), #2554 (far-field fade semantics), `restir_far_field_converges_to_unshadowed_radiance` (source-shape; no energy assertion).
- **Suggested Fix**: Weight fresh candidates by `pHat · N_candidates`, or equivalently treat the enumerated list as one initial sample with M = 1, per Bitterli Alg. 3/4. Add a CPU mirror test asserting the expected value equals `Σ rad` for N equal and unequal lights. As the perf report warns, gate the change on a transport-oracle A/B (Cornell `--cornell` multi-light plus one multi-light interior) because it rebrightens the whole ReSTIR term.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
