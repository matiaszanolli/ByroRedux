# #4315: REN-2026-09-14-D20-01: the ground-cover interaction and scatter compute dispatches (and their counter clears) run outside every GPU timer bracket — the exact gap 74df367f closed for the sky-cube bake, re-opened one pass later in the frame

- **Labels**: low,renderer,performance,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4315
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/groundcover.rs` (`GroundCover::record_scatter`, `record_interaction`); call site `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (`dispatch_skin_and_cluster`, the `gc.record_scatter(...)` block)
- **Status**: NEW
- **Description**: `record_scatter` records the §12.4 interaction dispatch (`record_interaction`), three `vkCmdFillBuffer` counter clears and one screen-independent scatter `cmd_dispatch`. The scatter traces a ray query per candidate against the TLAS (the placed-geometry cover test). This all runs every frame an exterior has ground cover. The call site sits after `cmd_cluster_cull_end` and before `record_groundcover_bench`. `cmd_main_render_start` is not reached until `geometry_pass.rs`. So this work lands in no bracket, and it is not even misattributed to a neighbour. `groundcover.rs` has no reference to timers at all (`grep -n "timers" crates/renderer/src/vulkan/groundcover.rs` returns zero hits).
- **Evidence**: 74df367f's own message states the rationale: "The bake … ran unmeasured … How the visible sky should adopt the cloud layer … is a cost decision, and it should be made from a number". It added bracket 18 for exactly that reason. The ground-cover scatter is the same shape: per-frame compute with a data-dependent ray-query cost, and density/LOD tuning (#4056 Phase 3, open) is a cost decision. #3676 closed this bug class for `skin_palette.comp` and two others. The blade draw itself is inside the `main_render` bracket (`record_draw` precedes `cmd_end_render_pass` in `geometry_pass.rs`), so only the compute half is unmeasured.
- **Impact**: Observability only. On exterior cells the scatter/interaction GPU time is invisible to `bench:` lines, the metrics map and the debug-UI grid, so a ray-query-heavy density regression shows up as unexplained frame time. No correctness risk.
- **Related**: #3676 (same class, closed), 74df367f (sky-cube bracket), #4052 (bench-only ground-cover bracket, which does not cover the production scatter), #4056.
- **Suggested Fix**: Add a ground-cover bracket around the `record_scatter` call: `QUERIES_PER_FRAME` 36→38, a new `BIT_`, plus `_ms`/`_active` through `GpuTimerSnapshot`, `SkinCoverageStats` and the three consumers. Extend the module-doc table so d28722fb's row-count test stays green. If a bracket is judged not worth it, add a sentence to the table noting the unmeasured scatter.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
