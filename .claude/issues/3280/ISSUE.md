# 3280: SAVE-D6-2026-08-24-01: exterior live-load's delta overlay runs before the streaming radius resolves - saved state outside the arrival cell silently dropped

**Severity**: HIGH · **Report**: `docs/audits/AUDIT_SAVE_2026-08-24.md` (SAVE-D6-2026-08-24-01)

## Description

`reload_exterior_session` reconstructs a saved exterior session via `assemble_exterior_streaming(..., ExteriorBootstrapMode::ForegroundFirst)`, which deliberately blocks only for the arrival cell, leaving peripheral cells to the steady-state per-frame streaming budget. `execute_pending_save_loads` immediately calls `build_form_id_remap` (a synchronous full-`World` `FormIdComponent` scan) and `apply_deltas` right after `reload_exterior_session` returns — at which point only the arrival cell's entities exist. Every entity from every other cell in the load radius doesn't exist yet, so it can't appear in the remap, and `apply_deltas`'s per-column `apply` closure silently drops any row absent from the remap. Nothing in the streaming pipeline re-applies the saved deltas once a peripheral cell finishes streaming in later.

## Location

`byroredux/src/save_io.rs` (`reload_exterior_session`, `execute_pending_save_loads`'s call to `apply_deltas` immediately after it), `crates/save/src/driver.rs` (`build_form_id_remap`), `crates/save/src/registry.rs:135-151` (silent `filter_map`), `byroredux/src/scene/world_setup.rs` (`stream_initial_radius`, `ExteriorBootstrapMode::ForegroundFirst`)

## Evidence

```rust
let remap = byroredux_save::build_form_id_remap(world, &registry, &snapshot);
match byroredux_save::apply_deltas(world, &registry, &snapshot, &remap, MUTABLE_DELTA_COLUMNS) { ... }
```
```rust
let remapped: Vec<(u32, T)> = rows
    .into_iter()
    .filter_map(|(old, comp)| remap.get(&old).map(|&live| (live, comp)))
    .collect();
```
No test exercises the actual drain for an exterior context — only queueing is covered.

## Impact

Fires on the ordinary path of F9/console-loading while playing outdoors — likely the common case. Every mutable delta column (`Dead`, `ActorValues`, `Inventory`/`EquipmentSlots`, AI-procedure state, `RigidBodyData.collidable`, `RumbleOnActivate`) silently reverts to ESM defaults for every actor outside the arrival cell, permanently. Completely silent — no log, no validation-gate signal.

## Related

`0a847910` (the feature that introduced the surface).

## Suggested Fix

(a) Make `reload_exterior_session` block for the full load radius (`FullRadius`) on a live load, or (b) defer `build_form_id_remap`/`apply_deltas` until `state.pending` is empty, re-running incrementally as peripheral cells stream in. At minimum, log a warning naming how many delta rows were dropped for remap-miss reasons.

## Completeness Checks
- [ ] **TESTS**: A test driving `execute_pending_save_loads` on an exterior context with entities in peripheral (still-streaming) cells, asserting their delta columns are NOT silently dropped
