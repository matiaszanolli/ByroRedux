# PHYS-D6-2026-09-06-03: #3492 added a `Ragdoll` storage read to `physics_sync_system` and updated neither the `Access` declaration nor the guard test that exists to catch that

Issue: #3964 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.


- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy (ECS access declaration)
- **Location**: `byroredux/src/boot.rs:1452-1494` (the declaration) · `crates/physics/src/water.rs:487` + `:747` (the two undeclared reads) · `byroredux/src/scheduler_access_tests.rs:148-172` (`water_and_animation_parallel_accesses_are_complete`, the #3121 guard)
- **Status**: NEW — **fourth** instance of a class already filed and fixed three times, twice on this exact declaration: #1787 / CONC-D4-01, #2676 / CONC-D3-NEW-02, PHYS-D3-2026-08-20-05 (fixed `5428e872`, MEDIUM)
- **Trigger Conditions**: none today — `physics_sync_system` is currently the only system in `Stage::Physics` and nothing else declares `Ragdoll`. Becomes live the moment any system reading or writing `Ragdoll` joins a parallel batch that can overlap `Stage::Physics`.
- **Description**: `1e4d83a7` gave the buoyancy sink a second target source keyed on `Ragdoll`. Both `apply_buoyancy_with_scratch` (`water.rs:747`) and `clear_stale_water_contacts` (`:487`) now take a `world.query::<Ragdoll>()` read guard on the default path. The declaration lists every other storage the buoyancy phase touches — `WaterPlane`, `WaterVolume`, `WaterFlow`, `WaterCurrentVolume`, `WaterContact`, `RapierHandles`, `RigidBodyData` — and even declares three components read only behind an env-var-gated diagnostic, with a comment explaining that *"the analyzer can't see the runtime gate"*.
- **Evidence**: `git show --stat 1e4d83a7` → four files (`crates/physics/src/{components,ragdoll,water}.rs`, `docs/engine/physics.md`); neither `boot.rs` nor `scheduler_access_tests.rs` is among them. `grep -n Ragdoll byroredux/src/boot.rs` → only the three `world.register` lines at `:622-624`; **no `Access` declaration anywhere mentions it**. The guard test — whose doc reads *"#3121 — WATAL buoyancy reads live time/weather/current state … Keep those reads on the exact parallel-system declarations so the scheduler can reject future conflicts"* — still asserts its pre-#3492 needles and passes.
- **Impact**: `scheduler.access_report().known_conflict_count() == 0` is computed from an incomplete surface, and `sys.accesses` under-reports what `physics_sync_system` acquires. Same impact statement PHYS-D3-2026-08-20-05 carried when it was filed and fixed at MEDIUM.
- **Related**: #3492, #1787, #2676, PHYS-D3-2026-08-20-05, #3121, `/audit-concurrency` Dim 4.
- **Suggested Fix**: add `.reads::<byroredux_physics::Ragdoll>()` to the `physics_sync_system` declaration with the same one-line "#3492 — the buoyancy phase's second target source" comment the neighbouring water entries carry, and extend `water_and_animation_parallel_accesses_are_complete`'s needle list so the guard covers the read it was written to guard.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
