# PERF-D6-NEW-01: draw_frame's Err(e) return arm never checks skin_dispatch_ran -- the #1791/#1796 pose-hash/first-sight-upload corruption is still reachable through four early-Err paths

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2522
**Finding ID**: PERF-D6-NEW-01

**Severity**: HIGH
**Dimension**: Skinning & BLAS Cost (M29.x)
**Location**: `byroredux/src/main.rs:784-897` (the `match ctx.draw_frame(...)` — rollback only lives in the `Ok(needs_recreate) =>` arm; the `Err(e) =>` arm has no rollback) vs. `crates/renderer/src/vulkan/context/draw.rs:1347,1442,1458,1655` (four `return Err(e)` sites that execute strictly before `record_skinned_blas_refit` — the call that flips `skin_dispatch_ran` true)
**Status**: NEW (Regression of #1791 / #1796 — both CLOSED. The `skin_dispatch_ran` flag + `main.rs` rollback check those issues shipped was wired into only one of the two `match` arms `draw_frame`'s `Result` can produce; no open issue currently tracks this specific gap.)

## Description
#1791's own issue text explicitly named the loss vectors this fix needed to close: "empty framebuffers (`Ok(false)`)... `ERROR_OUT_OF_DATE_KHR`... **and fence/reset/begin error arms**." The landed fix (`skin_dispatch_ran`, reset `false` at the top of `draw_frame`, flipped `true` only once `record_skinned_blas_refit` runs) does cover the first two — but the consumer in `main.rs` only reads the flag inside the `Ok(needs_recreate) => { ... }` match arm. `draw_frame` can also return `Err(e)` — and does, from at least four sites that execute *before* `record_skinned_blas_refit` (i.e. while `skin_dispatch_ran` is still `false`): `wait_for_fences` on the image fence, `reset_command_buffer`, `begin_command_buffer`, and `build_fsr_frame_parameters`. On any of these, `main.rs`'s `Err(e) => { log::error!(...); event_loop.exit(); }` arm runs — and never calls `rollback_pending_pose_commits()` or `requeue_pending()`, regardless of what `ctx.skin_dispatch_ran` reads.

## Evidence
Confirmed directly:
```rust
// byroredux/src/main.rs — rollback only checked in the Ok arm
Ok(needs_recreate) => {
    if !ctx.skin_dispatch_ran {                       // <- only checked here
        self.skin_slot_pool.rollback_pending_pose_commits();
        self.skin_slot_pool.requeue_pending(std::mem::take(&mut pending_for_requeue));
    }
    ...
}
Err(e) => {                                            // <- never checked here
    log::error!("Draw failed: {e:#}");
    event_loop.exit();
}
```
All four `return Err(e)` sites in `draw.rs` run before `self.skin_dispatch_ran = true;` (reached via `record_skinned_blas_refit`). Before every one of these, `self.skin_slot_pool.drain_pending(...)` has already irrevocably removed the first-sight `(slot, entity)` pairs from `SkinSlotPool::pending_uploads` — exactly the precondition #1791 described as "irrevocably drained... before invoking `ctx.draw_frame`." And the CPU-side pose-hash commit (`try_mark_pose_dirty`, called before `ctx.draw_frame` is even invoked) has already advanced `last_pose_hash` — exactly the precondition #1796 described. The regression test added for #1796 (`skin_dispatch_ran_ordering_tests`) only pins the *order* of the reset vs. the two `Ok`-path guards inside `draw_frame` itself — it says nothing about whether `main.rs`'s caller-side consumption of the flag covers the `Err` arm, so it doesn't (and can't) catch this gap.

## Impact
On any of these four (rare but real: driver/allocator pressure, not necessarily a fatal device-loss) failures, `draw_frame` returns `Err`. `event_loop.exit()` is *queued*, not synchronous — one or more further frames can still render before the process actually terminates. On such a frame: (1) drained first-sight `bind_inverses` for any entity that allocated a slot this frame are permanently lost, corrupting that entity's raster + RT skinning for its remaining lifetime in the cell (identical blast radius to #1791, HIGH); (2) any entity whose pose changed this frame has its `last_pose_hash` baseline advanced against a dispatch that never happened, freezing GPU output/BLAS one-plus frames stale if the pose then goes idle (identical to #1796, MEDIUM). Severity taken at the higher of the two component bugs.

## Related
#1791 (CLOSED, same root cause), #1796 (CLOSED, same root cause), #1194/#1195/#1196/#1197 (adjacent, unaffected guards, verified intact this sweep).

## Suggested Fix
Move the `if !ctx.skin_dispatch_ran { rollback... }` block out of the `Ok(needs_recreate) =>` arm so it runs after `ctx.draw_frame(...)` regardless of which arm matched — restructure to call `draw_frame`, capture the `Result`, do the `skin_dispatch_ran` check unconditionally, *then* match on the result for the rest of the per-arm handling. `ctx` is still valid and owned by `self` after an `Err` return.

## Completeness Checks
- [ ] **TESTS**: A regression test forces one of the four early-`Err` paths (e.g. via a fault-injection hook) and confirms rollback still runs
- [ ] **SIBLING**: Confirm no other caller of `draw_frame` (if any exist) has the same Ok-arm-only gap

