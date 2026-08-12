# CONC-D3-NEW-02: camera_follow_system reads PlayerMode undeclared

**Issue**: #2676
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`


- **Severity**: MEDIUM
- **Dimension**: 3 — ECS Lock Ordering & Deadlock (declaration-backed deadlock guard)
- **Location**: declaration at [boot.rs](byroredux/src/boot.rs)
  (`scheduler.add_to_with_access(Stage::Late, crate::systems::camera_follow_system, …)`); body at
  [character.rs](byroredux/src/systems/character.rs) — `camera_follow_system`, first statement.
- **Status**: NEW (explicitly *not* covered by #2389, whose body names only `log_stats_system`
  and `metrics_sample_system`)
- **Description**: `camera_follow_system`'s very first statement acquires a read guard on the
  `PlayerMode` resource as an early-out gate. Its `Access` declaration lists `PlayerEntity`,
  `ActiveCamera`, `InputState` (resources) and `CharacterController`, `GlobalTransform`,
  `Transform` (components) — `PlayerMode` is absent. `Stage::Late` is the largest parallel batch
  in the engine (4 systems, 6 analyzed pairs), and the analyzer therefore reports
  `AccessConflict::None` for pairings it has not proved disjoint on this resource.

  This matters more here than in #2389's two cases: those are telemetry systems whose entire
  effect is writing snapshot resources. `camera_follow_system` writes `Transform` and
  `GlobalTransform` on the active camera — the pose the renderer, the audio listener, and
  `submersion_system` all consume later in the frame.
- **Evidence**:
  ```rust
  // byroredux/src/systems/character.rs — camera_follow_system, first statement
  let mode = world
      .try_resource::<PlayerMode>()      // ← undeclared
      .map(|r| *r)
      .unwrap_or_default();
  if mode != PlayerMode::Character { return; }
  ```
  versus the registration in `boot.rs`, whose `Access::new()` chain runs
  `reads_resource::<PlayerEntity>` / `ActiveCamera` / `InputState` and then goes straight to the
  component list — no `PlayerMode` entry. Note the Early batch's `player_controller_system`
  *does* declare `reads_resource::<PlayerMode>()`, so the omission is asymmetric within the same
  file.
- **Impact**: No live race today, and the reason was confirmed rather than assumed: the only
  writer of `PlayerMode` is `toggle_player_mode`, whose signature is `(&mut World)`
  ([main.rs](byroredux/src/main.rs) key handler) — a `&mut World` cannot coexist with the
  `&World` the scheduler hands systems, so the write is structurally excluded from the parallel
  window. The defect is that the `known_conflict_count() == 0` invariant asserted in
  `install_runtime_registries` — the thing that makes cross-thread ABBA structurally unreachable
  among parallel systems (see CONC-D3-NEW-03) — is computed from an incomplete declaration. The
  moment `PlayerMode` acquires a system-level writer in `Stage::Late` (a mode-switch script
  effect, a save-load apply, a debug command promoted out of the exclusive drain), the analyzer
  will not see the pair, `sys.accesses` will keep printing 0 conflicts, and the resulting
  read/write overlap gets no diagnostic.
- **Trigger Conditions**: Detection gap: present on every frame the schedule is built. Realized
  race requires a second `Stage::Late` *parallel* system that writes `PlayerMode` while
  `camera_follow_system` holds its read guard — i.e. the two co-scheduled on different rayon
  workers inside the same `data.parallel.par_iter_mut()` batch. Not reachable with today's
  registration set.
- **Verification Path**: **`cargo test`-observable.** Run the `byro-dbg` `sys.accesses` command
  (or `byroredux/src/scheduler_access_tests.rs`) — the Late-stage report shows
  `camera_follow_system ↔ *` as `None` on `PlayerMode`. Direct proof: add
  `.reads_resource::<PlayerMode>()` to the declaration and confirm `known_conflict_count()` stays
  0 (no real conflict exists today), which is the same shape of fix #1787 applied to
  `physics_sync_system`'s `ContactConfig`.
- **Related**: #2389 (same class, the other two Late-parallel systems), #1787 / CONC-D4-01
  (closed, `physics_sync_system`'s `ContactConfig`), #2393 (zero-conflict invariant
  near-vacuous).
- **Suggested Fix**: Add `.reads_resource::<crate::systems::PlayerMode>()` to
  `camera_follow_system`'s `Access` in `boot.rs`. Fix alongside #2389 so all four Late-parallel
  declarations are complete in one pass.

---

### LOW


---
*Filed from [`docs/audits/AUDIT_CONCURRENCY_2026-08-12.md`](docs/audits/AUDIT_CONCURRENCY_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `CONC-D3-NEW-02`.*

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related systems
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
