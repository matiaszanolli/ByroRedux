# REN-D16-04: froxel temporal neighbourhood clamp is computed from the history volume itself; emissive fast time-constant keys only on the current sample

- **Severity**: MEDIUM
- **Dimension**: 16 — Volumetrics
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp` — the `params.prev_camera_pos.w > 0.5` / `reprojectHistory` block (`historyMean`, `historySecondMoment`, `historySigma`, `emissionFraction`, `emissionAgreement`, `historyWeight`); constants `DEFAULT_TEMPORAL_HISTORY_WEIGHT`, `DEFAULT_EMISSIVE_HISTORY_WEIGHT`, `DEFAULT_DENSITY_REJECTION` in `crates/renderer/src/vulkan/volumetrics.rs`
- **Description**: (1) The 3×3 statistics that clamp `history.rgb` are gathered from `previousFroxel` — the history volume itself, not the current frame — unlike `taa.comp`/`svgf_temporal.comp`. Clamping a value to a neighbourhood that includes that value places no bound on disagreement with the current frame. (2) Genuine current-vs-history rejection is only the density term and `emissionAgreement`, which is multiplied by `emissionFraction` (current-sample-only). On the trailing edge of a departing flame, `emissionFraction → 0`, so `emissionAgreement → 1` and `mix` selects `steadyWeight = 0.92` (~12-frame decay) instead of `emissiveWeight = 0.75` (~3.5 frames) — fast on, slow off.
- **Evidence**: Clamp statistics sample `previousFroxel`; `emissionFraction`/`emissionAgreement`/`mix(steadyWeight, emissiveWeight, emissionFraction) * emissionAgreement` confirmed in `volumetrics_inject.comp`. Constants confirmed in `volumetrics.rs`. Partially self-disproved: when the departing volume also dominated extinction, the density term suppresses the trail; the surviving case is ambient/global-medium-dominated froxels (fogged exteriors, thin flames).
- **Impact**: Emissive smear trailing moving fire/explosion fronts in fogged exteriors. Visual only, bounded.
- **Related**: `edbed7a3` (mechanism landed, never audited), #2470 (different Z-convention issue, reprojectHistory's slice convention is correct here), #2241 (OPEN, sibling integration-side over-brightening).
- **Suggested Fix**: Gather clamp statistics from the current frame's computed inscatter/extinction neighbourhood, or derive `emissionFraction` from `max(current, reprojected-history)` so trailing edge inherits the same short time constant.

## Completeness Checks
- [ ] TESTS: A regression test pins this specific fix; needs RenderDoc/visual verification since quality-only and bounded
- [ ] SIBLING: cross-check against #2241 before landing (both touch the reproject/history path)

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2809
