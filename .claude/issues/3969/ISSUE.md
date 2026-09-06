# PHYS-D2-2026-09-06-02: the wake-discipline contract says spawning a body arms `pending_wake`; the production spawn path deliberately does not, and the `had_newcomers` escape hatch exists because it doesn't

Issue: #3969 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.

- **Severity**: LOW · **Dimension**: Step Determinism & Budget · **Status**: NEW
- **Location**: `crates/physics/src/world.rs:287-292` (the `wake` docstring) and `:519-520` (the fast-path comment), contradicted by `crates/physics/src/sync.rs:932-946` + `:1011-1017` and by `crates/physics/src/water.rs:668-671`
- **Trigger Conditions**: latent — fires when a maintainer adds a body-creating path, or "restores" the missing wake in `register_newcomers`, on the authority of the two comments.
- **Description**: `wake`'s docstring states the subsystem contract — *"Must be called by every mutation that can introduce motion — **spawning a body**, pushing a kinematic target, setting a velocity"*. Two of the three are true. The third is false for the path that spawns essentially every body in the engine: `register_newcomers` calls **only** `mark_colliders_dirty()`, and its dynamic bodies are built `sleeping(true)` on purpose (the EXTERIOR-FREEZE FIX, whose comment records `atw_scheduler=3005ms` with ~3000 awake dynamics on a Skyrim exterior streaming frame).
- **Evidence**: the crate has exactly 8 production `.wake()` call sites and **none is in the spawn path**. The strongest counter-evidence is in the buoyancy sink, which grew a parameter to work around it: *"The `had_newcomers` term is load-bearing: a body that streams in already submerged spawns ASLEEP and Phase 1 does NOT wake it (`register_newcomers`), so without this term its first-frame dry→wet float-up would be skipped here."* (`water.rs:668-671`), fed by `apply_buoyancy(world, n_new > 0)`. The code contains both the false claim and its own refutation.
- **Impact**: no incorrect behaviour today; the hazard is asymmetric. (a) A future body-creating path added on the strength of "spawn arms it" inherits a body that never moves and produces no error — the silent-failure mode this dimension's checklist exists for. (b) A maintainer reconciling comment with code in the **wrong** direction reintroduces a measured multi-second streaming stall and simultaneously makes `had_newcomers` look redundant, inviting its removal.
- **Related**: #2856, #2889, #2890, #3121, `docs/engine/watal.md` §0.
- **Suggested Fix**: in both comments replace "spawning a body" with the true rule and its reason — spawn is exempt (dynamics spawn asleep by design), announces itself with `mark_colliders_dirty`, and hands first-frame visibility to consumers through the `n_new > 0` argument. One sentence each, cross-referencing `water.rs:668-671`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
