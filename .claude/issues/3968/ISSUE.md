# PHYS-D2-2026-09-06-01: `build_ragdoll`'s collider insert never marks the query pipeline dirty, and the disproof that closed it in 2026-08-20 rests on a premise the crate's own #2856 pin falsifies

Issue: #3968 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.

- **Severity**: LOW · **Dimension**: Step Determinism & Budget · **Status**: NEW (the behaviour was investigated and dismissed on 2026-08-20; the dismissal's stated rationale is wrong at HEAD)
- **Location**: `crates/physics/src/ragdoll.rs:322` (insert) and `:412` (`pw.wake()`); gate at `crates/physics/src/world.rs:633-636`; contract at `crates/physics/src/sync.rs:1011-1017`; the false disproof at `docs/audits/AUDIT_PHYSICS_2026-08-20.md:568-573`
- **Trigger Conditions**: a ragdoll activated on an actor whose skeleton bones have no live `RapierHandles` — activation reaching `activate_ragdoll` before the first `physics_sync_system` tick after spawn — on a frame where `accumulator < PHYSICS_DT` (every frame above 60 fps). Reachable via `byroredux/src/combat.rs:546` (a kill on the actor's spawn frame) and the `ragdoll <id>` console command.
- **Description**: since #2864, `mark_colliders_dirty()` is the sole mechanism by which a collider-set mutation reaches the query BVH on a frame that runs no substep. `register_newcomers` honours it; `build_ragdoll` inserts one collider per part and calls only `pw.wake()`. **`wake()` does not guarantee a substep** — it only makes `step` skip the static-scene early return; the `while accumulator >= PHYSICS_DT` gate is independent. With `pending_wake == true`, `accumulator < PHYSICS_DT` and `colliders_dirty == false`, `steps == 0` and `world.rs:633` evaluates `false || false`.
- **Evidence**: `grep -c mark_colliders_dirty crates/physics/src/ragdoll.rs` → **0**. The gate is `if steps > 0 || self.colliders_dirty`. And the crate's own #2856 pin asserts the exact state: `w.wake(); assert_eq!(w.step(PHYSICS_DT / 2.0), 0, "half a tick cannot step yet"); assert!(w.pending_wake(), "wake must still be armed after a 0-substep frame");`. The 2026-08-20 pass dropped this candidate with *"**Inert**: `build_ragdoll` calls `pw.wake()`, so the next `step` **always** takes the substep path"* — the pre-#2856 mental model of `wake()`, recorded as a settled disproof for future passes to lean on.
- **Impact**: bounded. The behaviour *is* safe today, but for a reason the disproof never states: `activate_ragdoll`'s #1772 keyframed-bone teardown calls `pw.remove_body` per bone, and `remove_body` sets `colliders_dirty = true`. A collider mutation in `crates/physics/src/ragdoll.rs` is covered by a side effect of a loop in `byroredux/src/ragdoll.rs` that exists for an unrelated reason and is guarded by `if !bone_handles.is_empty()`. When that guard is false the window is ≤ one banked tick (~16.7 ms, 2 frames at 120 fps) during which a fresh corpse's colliders are absent from the query BVH. The durable cost is the recorded false disproof.
- **Related**: #2864, #2856, #1772, #2863; sibling shape: PHYS-D1-2026-09-06-01, PHYS-D4-2026-08-30-01.
- **Suggested Fix**: add `pw.mark_colliders_dirty();` beside `pw.wake();` at `ragdoll.rs:412`. Separately, correct the 2026-08-20 disproof — restate it as "safe *because* `activate_ragdoll`'s #1772 teardown marks dirty", which is checkable and reveals the `bone_handles.is_empty()` hole.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
