# PHYS-D3-2026-09-06-01: Phase 1's fresh-`GlobalTransform` precondition is enforced only by scheduler stage order, and the out-of-schedule bootstrap walks around it — NIF-node colliders register at the world origin, permanently

Issue: #3960 · Filed from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`)

Reported by `/audit-physics` (full 7-dimension pass) — `docs/audits/AUDIT_PHYSICS_2026-09-06.md`, base `229306ce`.

Every claim below was independently re-derived from the code by the audit orchestrator before filing, not accepted from the dimension agent.


- **Severity**: HIGH
- **Dimension**: ECS Sync
- **Location**: `crates/physics/src/sync.rs:200-219` (the bootstrap) and `:807-866` / `:880-940` (the Phase 1 it re-enters); callers `byroredux/src/scene.rs:1263` and `byroredux/src/systems/character.rs:822`; producer `byroredux/src/scene/nif_loader.rs:519-542`
- **Status**: NEW
- **Trigger Conditions**: any collision-carrying entity spawned by `load_nif_bytes_with_skeleton` collected by `register_newcomers_and_refresh_queries` before a `transform_propagation_system` pass has run. Two live routes: (a) `cargo run -- <mesh>.nif` — the default loose-NIF viewer invocation, no flags; (b) any interior cell transition in Character mode where the reference phase spawned an NPC.
- **Description**: the tick's module doc opens with the precondition — *"Runs after `transform_propagation_system` so that `GlobalTransform` is fresh for Phase 1 spawning"*. That is true for `physics_sync_system` (`Stage::PostUpdate` = 2 precedes `Stage::Physics` = 3). But `register_newcomers_and_refresh_queries` is a **second entry into the same Phase 1** that runs outside the scheduler. Its doc is careful about the *exclusivity* half of the contract (#3267) and says nothing about the *freshness* half. Neither caller satisfies it.
- **Evidence**:
  - The producer seeds identity: `world.insert(entity, GlobalTransform::IDENTITY);` (`nif_loader.rs:520`), with the collision shape and body inserted on the same entity at `:540-541`. The correctly-composed `rest_pose` **is computed 8 lines earlier** (`:512-516`) and used only for a name map.
  - Route (a): `scene.rs:944` runs propagation only under `if studio_mode && has_nif_content`. `diagnostic_scene = combustion_lab || cornell_glass_dragon || cornell_oracle.is_some() || cornell_sun.is_some() || studio_mode` (`:700-704`) — all false for a bare NIF run. So `spawn_plan`'s `else` branch at `:1259` calls `register_newcomers_and_refresh_queries(world)` (`:1263`) with no propagation having occurred.
  - Route (b): `transition.rs:559` → `ground_character_body_at` → `register_newcomers_and_refresh_queries(world)` (`character.rs:822`), firing the instant the reference phase reports `Complete` — and NPC spawn is part of that phase, so every bone entity just created sits at `GlobalTransform::IDENTITY`.
  - Phase 1 reads it as world truth: `collision_shape_to_parts(&n.shape, n.global.scale, &cfg)` (`sync.rs:907`) and `.position(iso_from_trs(n.global.translation, n.global.rotation))` (`:938`).
  - Registration is **one-shot** — the collect gate `if handles_q.contains(entity) { continue; }` (`sync.rs:849`) means the body is never rebuilt, and the collider *shape* (where the scale lives) is never rebuilt for any motion type.
- **Impact**, by motion type: **Static** — permanently stranded at the world origin, identity rotation, scale 1.0; nothing re-poses a fixed body; silent. **Keyframed** (every NPC ragdoll bone after `keyframe_live_ragdoll_bones`) — `push_kinematic` re-targets the pose next tick, so for one frame ~18 bone colliders per NPC stack at the origin and Rapier derives an enormous implied kinematic velocity from the one-step correction; the baked **scale** is never revisited, so a bone collider on a non-unit-scaled actor is permanently the wrong size. **Dynamic** — worst: spawned asleep, `pull_dynamic`'s sleeping-skip compares the ECS local against the origin-derived local, they differ, and **the ECS `Transform` is overwritten from the bad Rapier pose** — the visual sub-object teleports toward the origin. In all cases the BVH the bootstrap refreshes (the entire point of both call sites) is the one the ground probe immediately queries.
- **Related**: **#2866 fixed the *writeback* half of this same entity's local/world contract and left the registration half unexamined** — the same partial-close shape; #2867 (the gate that makes registration one-shot); #3267 (narrowed these call sites to this bootstrap); #1698 (awake-faller storms — the Keyframed origin excursion is a plausible unlisted contributor).
- **Suggested Fix**: make the precondition explicit rather than accidental. Preferred: have `register_newcomers_and_refresh_queries` run a propagation pass over the pending set before collecting — it already owns exclusive access, which is what makes that legal. Alternative: state the precondition in its doc beside the exclusivity contract, add a `debug_assert` that no collected newcomer carries a `Parent` whose composed global disagrees with its own, and give both callers an explicit `propagate(world, 0.0)`. Independently, having the producer seed its already-computed `rest_pose` instead of `GlobalTransform::IDENTITY` removes the sharp edge for every consumer.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — this audit's headline result is that *every* recent physics fix closed on fewer sites than its own evidence named; enumerate the full defect class before closing
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, the canonical acquisition order (`docs/engine/ecs.md`) is preserved and `PhysicsWorld` stays a sink
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser→canonical boundary; the solver side carries no `GameKind`/`bsver` branch (PHYSAL doctrine, `docs/engine/physal.md` §1)
- [ ] **TESTS**: A regression test pins this specific fix
