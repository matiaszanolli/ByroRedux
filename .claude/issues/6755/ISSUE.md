# REN-D12-02: stale comments in the local-sim timer / telemetry code (bracket order, misplaced doc paragraph, wrong field name)

Labels: low,renderer,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6755

**Severity**: LOW · **Dimension**: Debug/Telemetry

## Location
`crates/renderer/src/vulkan/gpu_timers.rs` (`Q_VOLUMETRICS_LOCAL_SIM_*` doc and the `TimerPass::row` comment); `crates/renderer/src/vulkan/volumetrics/local_grid.rs` (`record_frame` SAFETY comment); `crates/renderer/src/vulkan/context/telemetry.rs` (`fill_texture_residency_telemetry` / `fill_upscaler_telemetry` docs); `crates/renderer/src/vulkan/taa.rs` (`TAA_VARIANCE_GAMMA` doc)

## Description
(a) The sim is recorded BEFORE the inject bracket ("Stage B2" precedes "Stage C" in `VolumetricsPipeline::dispatch`), but the timer docs say "recorded right after it" and the SAFETY text says "the froxel inject dispatch recorded just before". (b) The M39 telemetry fns were inserted between `fill_upscaler_telemetry`'s doc paragraph and its fn, so the paragraph documents the wrong item. (c) `TAA_VARIANCE_GAMMA`'s doc cites `RendererConfig::rt_test_taa_gamma`; the field is `rt_test_taa_gamma_bits`.

## Evidence
Call order in `VolumetricsPipeline::dispatch`: `grid.record_frame` then `cmd_start(VolumetricsInject)`; the two doc blocks in `telemetry.rs` run together with no item between them.

## Impact
Misleads about barrier / bracket order; no runtime effect (the bracket is correctly nested inside `volumetrics`).

## Suggested Fix
Correct the three statements; move the misplaced paragraph back onto `fill_upscaler_telemetry`; fix the field name.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Same wording checked in related files (docs, other comments)
- [ ] **TESTS**: Covered by existing guards / `.claude/commands/_audit-validate.sh`; no new test needed for pure comment edits
