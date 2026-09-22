# REN-D4-2026-09-21-01: `shader-pipeline.md` and `memory-budget.md` did not move with the Stage-1 exposure meter — no meter step between bloom and TAA, presentation still "exposure + ACES"

**Labels**: low, renderer, documentation, doc-rot

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: LOW (reference-doc drift) · **Dimension**: Pipeline/RenderPass
**Location**:
- `docs/engine/shader-pipeline.md`: Per-Frame Submission Order steps 17-20 (~:199-233); the compute-shader table and the `presentation.frag` row (~:33-50); the FSR-tail note (~:241-245).
- `docs/engine/memory-budget.md`: § FSR 3.1 Upscaler (~:341-343).
- Recording site: `crates/renderer/src/vulkan/context/post_passes.rs` `record_exposure_meter_pass` (~:1030), called from `record_post_passes` (~:294).

**Status**: NEW (Stage-1 commits `c5663fe39` / `d54382415`)
**Verified against**: HEAD `f97775ca8`

## Description

The Stage-1 exposure pipeline added a per-frame compute pass, `record_exposure_meter_pass` (`exposure_meter.comp`). It runs between bloom and TAA/upscale and writes the per-FIF 1×1 exposure texel that FSR and presentation read. Presentation now applies `tonemap(graded * exposure)`, with an ACES|AgX switch.

Neither reference doc moved:
- **`shader-pipeline.md`**:
  - The "authoritative" submission order goes 17 bloom → 18 `taa.comp`, with no meter step.
  - Step 20 still reads "exposure + ACES tone-map".
  - `exposure_meter.comp` is missing from the compute-shader table.
  - The `presentation.frag` row still says "applies ACES tone-mapping".
- **`memory-budget.md`**: the FSR section still names a single exposure producer (`exposure.rs`). The meter's 2 per-FIF slots, 2 UBOs and pipeline are not ledgered. They are tiny, but the ledger claims to be complete.

This is the third renderer audit in a row where the docs did not move with a post-pass change, despite the doc-moves-with-pass rule added after 2026-09-20.

## Evidence

- `grep -n -i "exposure" docs/engine/shader-pipeline.md` finds only step 20 ("exposure + ACES tone-map"), the `presentation.frag` table row, and the FSR-tail note naming `exposure.rs`. Nothing names the meter.
- `grep -n -i "exposure" docs/engine/memory-budget.md` finds one mention, `exposure.rs` in the FSR section header.
- `record_post_passes` calls `record_exposure_meter_pass` after `record_bloom_pass` and before `record_taa_pass` / `record_upscale_pass`. `taa_resolves_the_post_bloom_scene_tap` pins that order.

## Impact

Anyone reasoning about barriers or VRAM from the reference docs misses a per-frame compute dispatch, its two stage-wide barriers, and its resources. The performance audit's GPU-timer finding on this pass had to cite the gap. No runtime effect.

## Related

- #4308 (open): the same submission-order list is also missing the ground-cover passes. Sweep both in one edit.
- #4525 and #4526 (closed): the previous round of this doc drift (2026-09-20).
- PERF-D8-2026-09-21-01 (`docs/audits/AUDIT_PERFORMANCE_2026-09-21.md`): the exposure meter has no GPU-timer bracket. That report defers the doc gaps to this finding.
- REN-D7-2026-09-21-01 (#4587): the same frame-flow drift in `docs/engine/renderer.md`.

## Suggested Fix

In `shader-pipeline.md`:
- Add a numbered exposure-meter step between bloom and TAA (dispatch + barriers, fixed vs auto mode).
- Reword step 20 and the `presentation.frag` row to `tonemap(graded * exposureTex)` with the ACES|AgX switch.
- Add `exposure_meter.comp` to the compute table.

In `memory-budget.md`, ledger the meter's slots, UBOs and pipeline.

To stop a fourth round, consider a source-shape test that every `record_*_pass` called from `record_post_passes` is named in `shader-pipeline.md`.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D4-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: the `docs/engine/renderer.md` frame flow (REN-D7-2026-09-21-01 (#4587)) and #4308's ground-cover steps updated in the same sweep
- [ ] **TESTS**: optional doc-coverage pin (`record_post_passes` callees ↔ `shader-pipeline.md` step names)
