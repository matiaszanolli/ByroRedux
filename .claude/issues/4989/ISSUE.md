# #4989: CONC-D2-2026-09-28-02: The narrowed palette dispatch records back-to-back dispatches that write one SSBO with no barrier between them (sync-validation WAW noise; HYPOTHESIS)

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,sync,renderer,bug
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW, HYPOTHESIS. It is not a real data race, because the ranges are disjoint. The risk is validation noise that can hide real hazards.
- **Dimension**: Compute → AS → Fragment Chains (skin chain, M29 palette)
- **Location**:
  - `crates/renderer/src/vulkan/skin_compute.rs:1275-1289`: the per-range `cmd_push_constants` + `cmd_dispatch` loop, with no barrier inside it.
  - `skin_compute.rs:907-953`: `plan_palette_dispatch`, which sorts and merges the ranges so they are disjoint.
  - Caller: `context/dispatch_skin_and_cluster.rs:248-290`.
- **Status**: NEW. The loop was introduced by eb7c82043 (#4204, 2026-09-16), before the baseline. The 09-21 audit verified the loop's per-FIF behavior but did not raise this. No issue exists.
- **Description**:
  - Every range dispatch writes `palette_buffer` through binding 2, which is bound at `range = palette_buffer_size` (the whole buffer).
  - `skin_palette.comp:77` early-returns outside `[bone_base, bone_end)`, and the plan merges overlapping ranges. So invocations of different dispatches never write the same address, and there is no race under the Vulkan memory model.
  - Sync validation, however, treats a descriptor access as touching its whole bound range. It should therefore report `SYNC-HAZARD-WRITE-AFTER-WRITE` on the second and later dispatch whenever the plan has two or more runs. That happens whenever the dirty skinned slots are non-contiguous, for example several NPCs with only some of them moving.
- **Evidence**: No `cmd_pipeline_barrier` appears between loop iterations. The only palette barrier is the post-loop COMPUTE→COMPUTE|VERTEX|FRAGMENT one at `dispatch_skin_and_cluster.rs:270-288`.
- **Trigger Conditions**: A frame whose palette plan has two or more disjoint runs, under `BYRO_VALIDATION`.
- **Impact**: Validation noise only. This project decides barrier changes from sync-validation output (#4293's WAW capture), so persistent false WAWs on every skinned frame dilute that signal.
- **Verification Path**: Run `BYRO_VALIDATION=1` on a cell with several skinned actors, some of them idle. Confirm that a `SYNC-HAZARD-WRITE-AFTER-WRITE` names `vkCmdDispatch` / `skin_palette` on the palette buffer. If none appears, close this as not reproducing.
- **Related**: #4204, #4205, #4293.
- **Suggested Fix**: Only if confirmed. Prefer one dispatch that reads the run list from a small SSBO, or bind each run's sub-range through a dynamic offset so the validator sees disjoint ranges. Avoid adding a per-range COMPUTE→COMPUTE barrier: it would serialize the tiny dispatches that #4204 split out.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D2-2026-09-28-02) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in the other one-time/partial texture paths (texture_registry, dynamic_rgba, overlay uploads)
- [ ] **DROP**: If Vulkan objects change, teardown ordering is still correct (see the three load-bearing orderings in `context/teardown.rs`)
- [ ] **TESTS**: A regression test (or a recorded `BYRO_VALIDATION=1` capture) pins this specific fix
