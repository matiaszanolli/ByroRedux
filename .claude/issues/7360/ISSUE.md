# #7360 — CONC-2026-11-23-01

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_CONCURRENCY_2026-11-23.md` (finding CON-01).

- **Severity**: MEDIUM
- **Dimension**: Compute -> AS -> Fragment Chains
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_terrain_patch.rs:508-523` (`entry.content = Some(...)` and `ready.insert`), planner at `:96-110` (`plan_patch_blas_ops`), call site `context/dispatch_skin_and_cluster.rs:665`, tail-Err sites `context/draw.rs:448`, `:506`, `:554`
- **Status**: NEW. No open or closed issue found (`gh issue list --state all` for "terrain patch BLAS rollback submit", "patch BLAS never built UPDATE", "terrain patch sync validation"; local `issues.json` titles; #7221 / #7245 / #7246 / #7263 do not mention it). `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` "Observations" verified the barriers and the build/refit flags but did not look at the discard path.
- **Description**: `record_patch_blas_frame` commits `entry.content = Some((request.content, refits))` for every recorded op right after it records `cmd_build_acceleration_structures` (`:508-520`). That field is the planner's only input for "refit or build": `plan_patch_blas_ops` returns `Refit` when `previous == now` and `refits < rebuild_after` (`:105`), and a refit records `mode(UPDATE)` with `src_acceleration_structure(entry.accel)` into the same AS (`:458-463`). `draw_frame` has three tail `Err` sites (`end_command_buffer`, `reset_fences`, `queue_submit`) that discard the whole command buffer; each calls `rollback_skin_frame_state(frame)` (`draw.rs:448, 506, 554`). That rollback covers the skinned-BLAS provisional set, the TLAS recording (`invalidate_tlas_recording`) and the bone-world promotion (#3991), but nothing in it, or in `promote_skin_frame_state` / `promote_skip_clear_latches`, names `terrain_patch_blas` (`rg terrain_patch_blas` finds hits only in `acceleration/{mod,tlas,blas_terrain_patch}.rs`, `dispatch_skin_and_cluster.rs`, `telemetry.rs`). The skinned path states the same hazard explicitly: `rollback_provisional_skinned_blas` exists so the next frame does not "UPDATE-refit acceleration structures that were never built (#3991)" (`skinned_blas_refit.rs:105-108`, `blas_skinned.rs:794-816`).
- **Evidence**:
  ```
  // blas_terrain_patch.rs:511-520
  for &(slot, op) in recorded.iter() { ...
      entry.content = Some(match op {
          PatchBlasOp::Refit => { ...; (content, refits + 1) }
          _ => { stats.built += 1; (request.content, 0) }
  ```
  ```
  // blas_terrain_patch.rs:105
  Some((built, refits)) if built == now && refits < rebuild_after => PatchBlasOp::Refit,
  ```
  The `ready` set (`:522`) is cleared at the start of each record, so only `content` survives a discarded frame.
- **Trigger Conditions**: a frame that records at least one first-time patch BUILD (a slot's first use, or a slot reassigned to another square or level) and then loses its command buffer at `end_command_buffer`, `reset_fences` or `queue_submit` (out-of-memory, device error short of loss). The next frame with the same slot content plans `Refit`: an UPDATE whose source AS was never built.
- **Impact**: ray queries traverse an uninitialised BVH for that patch until the slot is reassigned or the 600-refit rebuild fires; the visible result is wrong shadows / GI / reflections on the ground nearest the camera, with a device fault possible if the garbage is traversed. Spec-wise it is the UPDATE-from-unbuilt-source class #3991 closed for skinned BLAS. Not reachable in steady state; the engine already treats the three Err sites as recoverable (`draw.rs:237-243`, the #3837 note that calls the three sites reachable in practice).
- **Verification Path**: `cargo test` can cover it (no GPU): a source-scan guard in the style of `the_groundcover_field_flip_is_promoted_after_submit_not_at_record_time` / `skip_clear_latches_are_promoted_after_submit_not_at_record_time` (both in `context/draw.rs`), plus a pure test on the planner. No validation capture is needed to confirm the latch order; a fault-injected `queue_submit` failure on a patch-bearing exterior would show the UPDATE.
- **Related**: #3991, #5956, #6291 (same record-time-vs-submit-time class), #7221, rider 16 in `sync.rs`.
- **Suggested Fix**: follow the #3991 shape rather than inventing one: stage the content commit (`pending` list on `PatchBlasFrame`, applied by a `commit_` after `queue_submit` returns `Ok` next to `promote_skin_frame_state`), and on the rollback path set the staged entries' `content = None` so the next frame takes `Build`. Pin it with a post-submit source-scan test.
- **Dimension agent ID**: CON-D2-01
- **Also found by**: CON-D1-01 (Dims 1+6), independently.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other record-time latches, other temp stems, other exclusive rows)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
