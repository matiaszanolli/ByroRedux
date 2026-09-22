# PERF-D8-2026-09-21-01: The exposure meter is a per-frame GPU pass with no timer bracket

**Labels**: bug, renderer, low, performance

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: LOW (telemetry blind spot) · **Dimension**: 8 — Telemetry & Origin
**Location**:
- `crates/renderer/src/vulkan/context/post_passes.rs:290-294`: the call, between `record_bloom_pass` and TAA / the upscale
- `crates/renderer/src/vulkan/context/post_passes.rs:1030-1051`: `record_exposure_meter_pass`, with no `gpu_timers` calls
- `crates/renderer/src/vulkan/exposure_meter.rs:235-313`: `dispatch`, which runs `cmd_dispatch(cmd, 1, 1, 1)` between two `cmd_pipeline_barrier`s
- `crates/renderer/src/vulkan/gpu_timers.rs:1-49`: 19 brackets / 38 queries (`QUERIES_PER_FRAME = 38`), none of them for the meter
- `gpu_timers.rs:39-40` and `:226`: the presentation label "exposure + ACES" predates the ACES|AgX switch and the meter-produced exposure texel

**Status**: NEW (`c5663fe39`, 2026-09-21)
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

- The meter dispatch and its two stage-wide barriers sit after bloom's end timestamp and before the start of TAA or the upscale. Its GPU time therefore falls in no bracket, the #3676 / #4315 class.
- By the report's estimate, fixed mode (the default) costs a few µs. Auto mode does 4,096 scattered fetches on one workgroup between two barriers (*est.* tens of µs). Neither mode can be measured today, just as auto-exposure and AgX are being evaluated as defaults.
- The renderer report lists "the exposure meter has no GPU-timer bracket" only in its stale-skill-premise notes, not as a finding.

## Impact

The pass's GPU cost is silently folded into unbracketed time. The missing bracket costs no frame time itself; it only hides the pass from `gpu_timers`, the metrics map and the `bench:` line.

## Related

- #4586 (REN-D4-2026-09-21-01): the matching `shader-pipeline.md` / `memory-budget.md` doc gaps for this pass. This report defers the doc side there.
- Other open defects on the same pass: #4597 (SAFE-D7-2026-09-21-01, meter averaging math), #4590 (REN-D11-2026-09-21-02, adaptation history), #4591 (REN-D11-2026-09-21-03, missing raw-debug gate).
- #3676 and #4315 (closed): earlier unbracketed passes. #4541 and #4210 (closed): the bracket-count doc rot that has followed each bump.

## Suggested Fix

Add a `BIT_EXPOSURE_METER` bracket around the dispatch and its barriers, taking the pool from 38 to 40 queries. Plumb it through `GpuTimerSnapshot`, the metrics map and the `bench:` line; append it last so existing parsers keep their column positions. Update the module doc's bracket table and count, and the presentation label, which is now tone-map (ACES | AgX) × the exposure texel.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D8-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Every post pass `record_post_passes` calls is either bracketed or deliberately unbracketed (with a comment)
- [ ] **TESTS**: A `*_bracket_reports_measured_duration` test is added for the new bit, and `doc_table_slot_count_matches_queries_per_frame` / `prose_outside_the_module_header_carries_no_bracket_counts` stay green after the 38 → 40 bump (the #4541 / #4210 doc rot)

