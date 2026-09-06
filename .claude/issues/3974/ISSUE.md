# PHYS-D6-2026-09-06-04: the kinematic player's water sampler has no `WaterCurrentVolume` arm, so a swimmer feels nothing from an authored XWCU rapids marker that every dynamic body in it drifts on

Issue: #3974 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.

- **Severity**: LOW · **Dimension**: Water / Buoyancy · **Status**: NEW (not covered by the 09-04 pass — its declared `character.rs` entry points are the four `c7561d74` functions; `player_water_state` is not among them)
- **Location**: `byroredux/src/systems/character.rs:939-1004` (`player_water_state`) · consumed at `:251-263` (the swim current drift)
- **Trigger Conditions**: a placed REFR carrying `XWCU` + `XPRM` (a rapids / current marker) whose volume overlaps swimmable water, where the containing `WaterPlane` carries no `WaterFlow` of its own. The plane's flow comes from the CELL's own velocity; the marker's comes from the REFR — independent authoring channels.
- **Description**: `player_water_state`'s own doc says it *"mirrors the dynamic-body `WaterContact` calculation without creating a transient component for the kinematic player."* That calculation has had a placed `WaterCurrentVolume` branch since #3114/#3268. The player sampler queries `WaterPlane`, `WaterVolume` and `WaterFlow` only, so the mirror is incomplete: the swimmer's horizontal drift can only ever come from the plane's own `WaterFlow`.
- **Evidence**: `grep -n WaterCurrentVolume byroredux/src/systems/character.rs` → no hits. The dynamic-body counterpart is `collect_water_current_volumes` feeding the `current_flow` branch. `WaterCurrentVolume` is data-driven, built from `placed_ref.water_velocity` + `placed_ref.primitive`.
- **Impact**: cosmetic-to-gameplay divergence at authored rapids — a barrel dropped next to the player drifts downstream while the player does not. Bounded: the player is never pushed *wrongly*, only not pushed, and the kinematic path never touches `user_force`.
- **Related**: #3114, #3268, `docs/engine/watal.md` (the physics-half status paragraph, which describes the character path as consuming "horizontal flow" without distinguishing the two sources).
- **Suggested Fix**: give `player_water_state` the same current-volume lookup the dynamic path uses (capsule centre against `WaterCurrentVolume.volume`), returning the marker's flow when the plane has none — or, if the omission is deliberate for controller feel, say so in the doc comment instead of claiming the calculation is mirrored.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
