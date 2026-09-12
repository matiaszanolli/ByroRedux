# PERF-D9-2026-09-11-01: The CPU-phase nesting contract for `atw_post_ms` is documented wrong in both places an operator reads it

Labels: medium,performance,doc-rot,documentation

**Description**: `atw_post_ms`'s field doc lists only its Phase-10-era contents (`step_streaming`, `step_debug_loads`, `step_cell_transition`, window title). Phase 14 moved `render_one_frame` *inside* that same bracket, so in steady state `atw_post` contains `rof_pre_draw + rof_draw_call + rof_post_draw` — i.e. the entire render path, `draw_frame`, fence wait, submit. `cpu_breakdown`'s paragraph then asserts the opposite and false containment ("`atw_post` contains `atw_pre`/`atw_scheduler`'s sibling work" — the three `atw_*` brackets are actually strictly disjoint and sequential), occupying the exact sentence where the true `atw_post` superset-of-`rof_*` containment should be.

**Evidence**:
Bracket line numbers in `byroredux/src/app_events.rs` (805/808/812/819/892-894/1328) directly contradict the doc text quoted verbatim in `crates/core/src/ecs/resources/mod.rs:835-840` and `byroredux/src/systems/debug.rs:114-120`.

**Impact**: Stall mis-attribution — this bucket's entire purpose. The `cpu_breakdown` triage rule stays accidentally safe because it conditions on `rof_*` being small, but the field doc read alone (as the debug-UI Metrics panel and `byro-dbg` do) would point an investigator at exterior streaming for a frame whose cost is actually `draw_frame`. No runtime cost — the numbers are right, the contract describing them is not.

**Related**: #3692, #3674, #2731, Phase 14.

**Suggested Fix**: Two doc-only edits — extend `atw_post_ms`'s doc to state it also contains `render_one_frame` in its entirety; replace `cpu_breakdown`'s false clause with the true containment statement and note the three `atw_*` brackets are disjoint siblings. Optionally pin with a source-order assertion like `between_frames_capture_ordering_tests`.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
