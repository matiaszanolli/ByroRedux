# PHYS-D4-2026-08-20-03: the two water death sites insert Dead without reconcile_dead_actor — a drowned actor keeps its AI, keeps its AnimationPlayer, and never ragdolls

**Issue**: #3119 — https://github.com/matiaszanolli/ByroRedux/issues/3119
**Finding**: `PHYS-D4-2026-08-20-03`
**Labels**: bug, high, legacy-compat
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_PHYSICS_2026-08-20.md` — Dimension 4 (Ragdoll Articulation)
**Severity**: HIGH · **Status**: NEW

## Cross-audit ownership (2026-08-20 comprehensive suite)
This defect was **independently surfaced by three audits in the same sweep** and is filed **once**, here, from the physics side, because the ragdoll-handoff contract is the load-bearing half:

- `/audit-physics` (this report, HIGH) — owns it: `activate_ragdoll` is the only path that frees the actor's keyframed per-bone Rapier bodies.
- `/audit-ecs` corroborated it independently **from the component side** (its own MEDIUM), reaching the same two call sites via the derived-component removals that never run.
- `/audit-save` verified the save round-trip actually **repairs** the live inconsistency: `reconcile_dead_actor_runtime_state` runs on load, so a save taken after a drowning and reloaded produces a *different* (correct) world state from the live one.

That last point is the reason this is classified as a **combat / ECS-runtime defect, not a persistence one** — persistence is the thing that works. Do not re-file the ECS or save copies.

## Location
- `byroredux/src/systems/water.rs:60-68` (`water_damage_system`)
- `byroredux/src/systems/character.rs:1027-1044` (`apply_player_drowning_damage`)
- Contract owner: `byroredux/src/combat.rs:376-389` (`reconcile_dead_actor`)

## Trigger conditions
An actor killed by water rather than by combat — FO3/FNV authored harmful water (`WaterPlane::damage_per_second`, the `06f84f0d` / `93851ecd` path) on an NPC with an active `WaterContact`, or the player's breath reserve reaching zero (`advance_breath` → `DROWNING_DAMAGE_PER_SECOND = 12.0`).

## Description
`reconcile_dead_actor` is documented as the single reconciler that rebuilds "the runtime consequences of the persisted `Dead` fact", and both pre-existing death transitions route through it — `apply_hit_damage` (`combat.rs:242`) and the save-load drain (`save_io.rs:1014` via `reconcile_dead_actor_runtime_state`). The two water death sites added this cycle insert the marker directly and stop:

```rust
// systems/water.rs:64-67
if killed {
    if let Some(mut dead_q) = world.query_mut::<Dead>() { dead_q.insert(entity, Dead); }
}
// systems/character.rs:1039-1043 — identical shape
```

Three derived state changes are therefore skipped:
- `clear_ambient_behavior` (removes 16 behavior/state components — `ai_package.rs:416-436`)
- `remove_component::<AnimationPlayer>` on the skeleton root
- `activate_ragdoll`

Verified at HEAD: `grep -rn reconcile_dead_actor byroredux/src` returns exactly two production callers (`combat.rs:242`, `combat.rs:399` via the load drain) plus `save_io.rs:1014`. Neither water site appears.

## Evidence
The AI gate added by #3030 is on the *evaluation* path only — `ai_package.rs:472` and `:543` skip re-selecting a package for a `Dead` actor, but nothing removes an already-installed `WanderBehavior` / `TravelBehavior` / `SandboxBehavior`, and their driver systems do not consult `Dead`.

From the physics side the decisive one is `activate_ragdoll`: it is the only path that frees the actor's keyframed per-bone Rapier bodies and replaces them with the dynamic articulated rig (`byroredux/src/ragdoll.rs:392-427`, the #1772 discipline). Skipping it leaves the corpse's bones as `Keyframed` bodies that `push_kinematic` continues to drive from an `AnimationPlayer` that also was not removed.

## Impact
A drowned NPC keeps walking its package and playing its idle, with its skeleton still kinematically pushed into the solver every frame — a walking corpse, not a body.

The player case is softer (the controller now early-returns on `Dead`, `character.rs:157-159`) but equally inconsistent: no death ragdoll, and any `HavokAnimationTarget`-driven animation keeps running.

Because `Dead` is persisted and `reconcile_dead_actor_runtime_state` runs on load, a save taken after a drowning and then reloaded produces a *different* world state from the live one — the reload finally ragdolls the actor. The persistence layer is repairing a live-runtime inconsistency, which is the diagnostic that localises the defect here rather than in save/load.

## Related
#3022 (created the single reconciler), #3030 (CLOSED — gated AI *re-installation* on `Dead`, not the already-installed behavior), #1772, #2882.

## Suggested fix
Widen `reconcile_dead_actor` to `pub(crate)` and call it from both water death sites immediately after the `Dead` insert, exactly as `combat.rs:242` does.

`water_damage_system` is already an exclusive `Stage::Late` system so the structural removals are legal there. The character path needs the call hoisted out of `character_controller_system`'s parallel window, or routed through a `Stage::Late` sink. Both sites' `Access` declarations must then be widened to the union `reconcile_dead_actor` touches.

A single test — kill an actor by water damage, assert `Ragdoll` is present — pins the contract for whichever third death site lands next.

## Completeness Checks
- [ ] **SIBLING**: Every site that inserts `Dead` routes through the one reconciler (grep `insert(.*Dead)` — this fix must leave zero unrouted producers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved — the character path's call must not run inside the parallel window
- [ ] **TESTS**: A regression test pins this specific fix
