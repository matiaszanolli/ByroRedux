# #7278: PHYS-D2-01: Bodies registered through `register_newcomers_and_refresh_queries` never reach the buoyancy phase's newcomer escape

- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/sync.rs:220-239` (the exclusive helper); `crates/physics/src/sync.rs:143` (`apply_buoyancy(world, n_new > 0)`); `crates/physics/src/water.rs:987` (`… && !had_newcomers`)
- **Status**: NEW
- **Description**: The buoyancy phase skips its scan in a quiesced scene unless `had_newcomers` is set. That flag is only `n_new > 0` computed inside the scheduled tick. Bodies registered earlier by the exclusive helper are already registered when the tick runs, so the tick sees `n_new == 0`. The helper is used by scene setup (`byroredux/src/scene.rs:1082`, before the first tick) and by `combat.approach` (`commands/view.rs:356`). A dynamic body that streams in already submerged and spawned asleep, the case the escape exists for, misses its first-frame float-up when it arrives through that helper.
- **Evidence**: `register_newcomers_and_refresh_queries` calls `collect_newcomers` → `register_newcomers` → `update_query_pipeline()` and returns. Nothing records that newcomers were registered. Buoyancy then decides `quiescent = pw.active_island_counts().0 == 0 && !pw.pending_wake() && !had_newcomers`.
- **Trigger Conditions**: boot into a cell with submerged dynamic clutter, with no other newcomer or wake on the first tick. The player capsule is often a newcomer, and a teleport arms `pending_wake`, so this is usually masked.
- **Impact**: Submerged clutter can sit at its spawn pose underwater until something else wakes the scene.
- **Related**: #3969 (spawn-asleep contract), #5561
- **Suggested Fix**: Have the helper arm the same escape, for example `queue_wake` or a "registered outside the tick" latch that the next tick ORs into `had_newcomers`, and pin it with a test that registers a submerged body through the helper.

**Found by**: `docs/audits/AUDIT_PHYSICS_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: every registration path outside the scheduled tick checked for the escape
- [ ] **LOCK_ORDER**: a new latch keeps the helper exclusive-only
- [ ] **TESTS**: a body registered submerged through the helper floats on the next tick
