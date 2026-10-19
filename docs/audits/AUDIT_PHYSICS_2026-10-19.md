**HEAD**: `8ed551fe5` · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-18.md` (HEAD `6f44874ac`, itself exterior-area-scoped) · **Audited**: the physics contract with streaming / cell_loader / npc_spawn (cell-spawn collider and body creation, rapier release on unload, NPC bone/KCC spawn and eviction restore, ragdoll start at spawn, cell_loader/water.rs water volumes, fixed-step behaviour under budgeted slices); Dims 2, 3, 4, 5 touched in that slice; scope=streaming-deep AREA-SCOPED · **Unchanged since baseline (skimmed)**: Dim 1 (shape translation: no commits to `convert.rs` / `config.rs` / `nif/import/collision/`), Dim 6 (queries/diagnostics: no commits to `sync.rs` / `commands/physics.rs`)

# PHYSAL / Physics Audit — 2026-10-19 — AREA-SCOPED (streaming-deep)

**This is an AREA-SCOPED audit run for the `--area streaming-deep` suite (`byroredux/src/streaming/`, `npc_spawn/`, `cell_loader/`), not a full `/audit-physics`.** Physics is a neighbour of that area, so only the physics contract with it was audited. The next run must not treat this as a full-codebase baseline. Read-only on code; no engine launch; no sub-agents. The 2026-10-18 report (exterior-area) remains the nearest Dim 1/5/6 baseline.

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 1 |
| LOW | 2 |

- **PHYS-SD-01 (HIGH)**: the #4818 pickup-tombstone sweep despawns parented collision entities without detaching them from their surviving parent. FO4/Starfield clutter carries exactly such a parented collider (the packed-Havok proxy), so the leftover `Children` entry makes `validate_world` reject every later save in that cell.
- **PHYS-SD-02 (MEDIUM)**: a Starts-Dead actor's ragdoll is built awake as soon as its actor job completes, inside a multi-slice exterior cell apply, before the REFRs later in file order have spawned their collision.
- **PHYS-SD-03 (LOW)**: ragdoll activation leaves shape-only bone entities behind, which permanently defeats the #3477 newcomer-scan cache.
- **PHYS-SD-04 (LOW)**: interior water-volume depth is chosen by a `half_extent > 1024` test that large interiors satisfy, giving them a 4096 BU column instead of 200.

**PHYSAL verdict for this slice.** The streaming/spawn side adds no per-game branch to the solver: `cell_loader/` and `npc_spawn/` hand the physics layer only `CollisionShape` + `RigidBodyData` + `GlobalTransform` + markers (`ActorBoneCollider`, `ActorColliderOwner`, `PhysicsSourceForm`). The three named seams (constraint CInfo decode, `havok_scale_for`, collision-object-kind dispatch) are untouched by this area. The one game-keyed physics decision in the area is `missing_collision_fallback` (`cell_loader/spawn.rs:74-93`), which keys on the *layer* and the parse-side `CollisionAuthoringSummary`, not on `GameKind`. Collision data traced statically: Skyrim/FO3/FNV/Oblivion bone bodies via `keyframe_live_ragdoll_bones`, FO4/Starfield packed-Havok proxies via `spawn_packed_collision_proxy`, exterior terrain tiles via `spawn_trimesh_collider_ghost`. No game data was read.

**Test runs (this pass, repo `target/`)**: `cargo test -p byroredux-physics` 261 passed, 0 failed. `cargo test -p byroredux --bin byroredux -- <filter>`: `rapier_release` 9 passed; `ragdoll` 44 passed, 8 ignored (FO3/skeever data gates); `locomotion` 16 passed; `character` 65 passed; `water` 166 passed, 4 ignored; `stream_snapshot` 6 passed. The `#[ignore]` FO3 / skeever gates were not run (need game data / env).

## Solver Invariant Matrix (streaming-deep slice)

| Invariant | Verdict | Evidence |
|---|---|---|
| Fixed step / catch-up | **Verified** | The scheduler runs first in `about_to_wait`, streaming steppers run after it (`app_events.rs:1010` then `:1071`), so a slice never interleaves with a substep. A slice-induced hitch reaches `PhysicsWorld::step` as a large `frame_dt`, clamped to `MAX_SUBSTEPS * PHYSICS_DT` (`world/mod.rs:1318-1321`), then bounded by `SUBSTEP_TIME_BUDGET` (slow-motion, not a jump). `dt` is pinned to 0.0 while the loading cover is up (`app_events.rs:744-747`), so door-transition applies never step. The player controller clamps with `MAX_FRAME_DT` (`character.rs:192`); the NPC KCC does not clamp `dt` but sweeps (no tunnelling). |
| Wake discipline | **Verified** | Spawn stays asleep-until-processed (`register_newcomers`, `sync.rs:1100-1113`); `remove_body` re-arms the pipeline (`world/mod.rs:638`), so eviction cleanup and neighbour wake-ups are not stranded by the static-scene fast path. Corpse ragdolls are the one awake-at-spawn class (PHYS-SD-02). |
| Registration freshness | **Verified** | Streaming spawns after the scheduler (frame F); PostUpdate propagates before Physics (frame F+1), so the one-shot `GlobalTransform` read in `collect_newcomers` sees the world pose. The out-of-scheduler paths (`ground_character_body_at`, `scene.rs:1017`) go through `register_newcomers_and_refresh_queries`, which propagates itself (#3960). |
| Lock order / phase order | **Verified** | `release_victim_rapier_bodies` collects under component read guards, drops them, then takes `PhysicsWorld` (`unload.rs:948-981`); same shape in `remove_collision_bodies` (`loot_appearance.rs:158-178`). Phase order collect/register -> push kinematic -> buoyancy -> step -> pull (`sync.rs:131-168`). |
| Explosion recovery | **Verified (tests)** | Recovery / clamp / quarantine guards sit in the 261-test lane. Not exercised by streaming code directly. |
| Teardown | **Verified, one drift** | Cell unload releases every `RapierHandles` body and every `Ragdoll` (bodies, colliders, multibody joints) before the despawn (`unload.rs:613`, `rapier_release_tests.rs`, 9 pass, incl. duplicated-victim idempotence). `remove_body` also prunes the per-body offence/label maps. The pickup-tombstone sweep releases the Rapier side correctly but not the hierarchy side (PHYS-SD-01). |
| Query BVH vs eviction | **Verified** | A removed collider's stale leaf resolves to nothing, or to the collider now occupying the slot, whose own leaf is refreshed at registration (`queries.rs:136-167`, `world/mod.rs:630-637`); the door-arrival probe runs after `update_query_pipeline`. |

## Findings

### PHYS-SD-01: The #4818 pickup-tombstone sweep despawns parented collision entities without detaching them, so a respawned FO4/Starfield picked-up item leaves a dangling `Children` entry and every later save is refused

- **Severity**: HIGH
- **Dimension**: Teardown (Dim 2 contract with the streaming restore path)
- **Location**: `byroredux/src/cell_loader/reference_state.rs:358-370` (the sweep), `:268` (`restore`), `:655-668` (`restore_resident`); parented producers at `byroredux/src/cell_loader/spawn.rs:301-302` (`spawn_packed_collision_proxy`) and `:1463-1464` (graph-node collision); the missing step lives privately at `byroredux/src/cell_loader/unload.rs:658` (`detach_victims_from_surviving_parents`)
- **Status**: NEW (an incompleteness in the #4818 fix; same bug class as the already-closed #5310 / #5490)
- **Trigger Conditions**: FO4 or Starfield. A Clutter- or Actor-layer item whose NIF authors only an opaque packed-Havok body (`needs_packed_collision_fallback()`, `spawn.rs:87`) is picked up; its cell then streams out (radius eviction or a door) and back in, or a save containing the pickup is loaded. Any game, for a pickup that also carries a behaviour-graph node collider.
- **Description**: On a respawn whose parked row says `picked_up`, `restore` removes the placement's collision entities with `collision_entities_of` (all entities tagged `PhysicsSourceForm` with the placement's FormId), then `release_victim_rapier_bodies` followed by a raw `world.despawn_batch(colliders)`. The comment at `:358-361` and the unit test at `:1848-1853` both assume those entities are "standalone ... never descendants". That holds for the bhk ghosts and the F3 trimesh ghosts, but not for the packed-Havok proxy (`Parent(placement_root)` plus `add_child`, `spawn.rs:301-302`) nor for graph-node colliders (`Parent(node_entity)` plus `add_child`, `spawn.rs:1463-1464`). `World::despawn_batch` removes component rows only and never edits another entity's `Children`; that is exactly why `release_entities_timed` calls `detach_victims_from_surviving_parents` before its own `despawn_batch` (`unload.rs:630-631`, #5310). The tombstone sweep skips that step, and the placement root survives (it stays as the hidden `PickedUp` placement), so its `Children` keeps the dead id.
- **Evidence**: `validate_hierarchy` (`crates/save/src/validate.rs:155-171`) reports `Children lists {child}, but its Parent != {parent}` for a child whose `Parent` row is gone (`parent_of.get(&child)` is `None`). `SaveCommand::execute` aborts on any issue: `save ABORTED: N referential-integrity issue(s) — refusing to write a poisoned save` (`save_io/commands.rs:250-267`). The post-load diagnostic only logs (`live_load.rs:699-700`), so the first symptom is the refused save, not the load.
- **Impact**: after the pickup is restored, every quicksave / manual save is refused until the placement's cell is evicted again (a second eviction despawns the placement and its stale `Children` together). Because `restore_resident` re-runs the same sweep after a save load (#4695), loading a save that contains such a pickup poisons the session the same way. The duplicated `Children` entry also lingers in propagation and the child-reference budget walk, as #5310 documents. Item loot is still correct; the damage is the save gate.
- **Related**: #4818 (the sweep), #5310 (the same dangling-`Children` leak on the gear path), #5490, SAVE-D4-2026-10-09-01 (validate_hierarchy arm).
- **Suggested Fix**: call `detach_victims_from_surviving_parents(world, &colliders)` (widen it to `pub(super)`) before the `despawn_batch` at `reference_state.rs:369`, and extend the `save_reload_window_tombstone_is_applied_by_restore_resident` fixture with one parented collider plus a `validate_world` assertion (the current fixture's collider is unparented, so it cannot see this).

### PHYS-SD-02: A Starts-Dead actor's ragdoll is built (awake) before the cell's later-in-file collision exists, so a corpse resting on late-ordered statics falls past them

- **Severity**: MEDIUM
- **Dimension**: Ragdoll start at NPC spawn (Dim 3 contract with the streaming apply)
- **Location**: `byroredux/src/cell_loader/references/mod.rs:495-499` (REFRs applied in file order, actors inline), `:772-788` (`apply_starts_dead`); `byroredux/src/cell_loader/reference_state.rs:391-397`; `byroredux/src/combat.rs:1279-1288` and `:1124` (`reconcile_pending_dead_actors_system` -> `activate_ragdoll`); `crates/physics/src/ragdoll.rs:355-570` (`RigidBodyBuilder::dynamic()`, `pw.wake()`)
- **Status**: NEW (adjacent to GAME-SD-03 and #5015, but a different mechanism)
- **Trigger Conditions**: an exterior cell applied in budgeted slices (no loading cover, so `dt != 0`) with a Starts-Dead ACHR whose supporting statics (a platform, table, bed, rampart piece, rock outcrop) come LATER in the cell's REFR order than the ACHR, and are not part of the terrain. The same applies to a corpse restored from a parked `dead` row (`reference_state.rs:325-341`).
- **Description**: `apply_starts_dead` queues the reconciliation as soon as the actor job completes (`references/mod.rs:787`); the Late sink runs on the next scheduler pass (`combat.rs:1279`) and `activate_ragdoll` builds the ragdoll from the bone `GlobalTransform`s. The bodies are dynamic and awake and the world is woken (`ragdoll.rs:568`). Nothing gates the activation on the rest of the cell's collision being registered: the cell apply job is still pending (references remaining, `budget.should_yield()` at `:496`), so statics later in the list spawn on later frames and register one frame after that. The corpse integrates gravity meanwhile (686.7 BU/s², so about 10 BU of fall after 10 frames and about 340 BU after one second; a cell apply routinely spans several frames). Terrain is always present first (terrain and water are spawned before references, `exterior.rs:2043-2121`), so the body ends on the terrain or on whatever registered earlier; the late static then appears under or through it. Interior transitions are not affected: `dt` is 0.0 while the cover is up (`app_events.rs:744-747`) and the interior apply completes before the cover lifts.
- **Evidence**: the awake/wake facts are in `ragdoll.rs` as cited; the ordering fact is the single `while job.next_ref < refs.len()` loop at `references/mod.rs:495`, which spawns actors and statics in one pass. `rapier`'s begin-touch wake and the sleep re-settle only help once something is touching; a body that already passed the late floor stays below it.
- **Impact**: visible mis-placement of authored corpses (lying under or beside the structure they were placed on) in exterior ring cells, and for interpenetration cases a depenetration impulse on the ragdoll. Not a crash: the explosion ladder bounds it. Not measured; the frequency depends on authored REFR order, which was not censused (needs game data).
- **Related**: #5015 (XRGD decode, open), GAME-SD-03 (corpse pose not carried across eviction), #4814, #5267.
- **Suggested Fix**: hold the queued reconciliation for an actor until its cell's apply job has completed (the owning `ExteriorCellApplyJob` / `ReferenceLoadJob` knows when), or build the ragdoll asleep and wake it on the first contact the way placed clutter spawns.

### PHYS-SD-03: Ragdoll activation leaves shape-only bone entities behind, which keeps the #3477 newcomer-scan cache permanently off

- **Severity**: LOW
- **Dimension**: Step & Sync (Dim 2)
- **Location**: `byroredux/src/ragdoll.rs:518-547` (activation removes `RapierHandles` and `RigidBodyData` but not `CollisionShape`); `crates/physics/src/sync.rs:949-995` (`collect_newcomers`; `all_registered = false` at `:974` for a handle-less shape row)
- **Status**: NEW (the fix for closed #3477 is in place and works for the case it targeted; this is a case it does not cover)
- **Trigger Conditions**: any ragdoll activation in the session (any killed NPC, or any Starts-Dead corpse, which is why 1,445 Skyrim ones matter). The effect lasts while the corpse's bone entities are resident.
- **Description**: `collect_newcomers` caches "nothing new" in `registered_shape_generations` only when every `CollisionShape` row has a `RapierHandles` row (`all_registered`). `activate_ragdoll` strips the bones' `RapierHandles` and `RigidBodyData` (`ragdoll.rs:518-547`) but leaves their `CollisionShape` (the keyframed bone bodies are replaced by the `Ragdoll` bodies). Each bone is then an entity with a shape and neither a body nor handles, i.e. permanently "incomplete" (the test `newcomer_cache_does_not_hide_late_body_or_transform_components`, `sync.rs:2066`, pins that an incomplete row must never be cached). One corpse (about 18 such bones) therefore re-enables the full `CollisionShape` storage walk on every physics tick, which #3477 was written to remove.
- **Evidence**: `grep` shows no `remove::<CollisionShape>` on the ragdoll path (`player_body.rs:295` and `loot_appearance.rs:1007` are the only strippers, both for the player's visual-only parts).
- **Impact**: one sparse-set probe per collider row per frame. #3477 measured about 10 us at a few thousand rows (3-5 ns per row); a radius-12 exterior with 50-100k colliders would cost roughly 0.2-0.5 ms per frame (derived, not measured). Streaming frames already invalidate the cache every time they add or drop colliders, so this only affects steady state.
- **Related**: #3477 (closed), #2867.
- **Suggested Fix**: have `activate_ragdoll` also remove the spec bodies' `CollisionShape` (the template keeps its own shapes), or let `collect_newcomers` skip rows that carry no `RigidBodyData` for the cache decision.

### PHYS-SD-04: Interior water volume depth flips from 200 BU to 4096 BU when the cell's REFR spread exceeds about 1984 BU

- **Severity**: LOW
- **Dimension**: Water / Buoyancy (Dim 5 contract with `cell_loader/water.rs`)
- **Location**: `byroredux/src/cell_loader/water.rs:524-529` (`volume_depth`), `:119-150` (`interior_water_placement`, `MAX_HALF_EXTENT = 2048`), `byroredux/src/cell_loader/load.rs:590-605`
- **Status**: NEW
- **Trigger Conditions**: an interior with a real `XCLW` (Oblivion, FO3, FNV; Skyrim/FO4 author an inert 0.0 that is filtered) whose placed references span more than about 1984 BU in X or Z.
- **Description**: `spawn_water_plane` decides interior versus exterior from `half_extent > 1024.0`, and says so in a comment ("captures the spawn caller contract without an explicit flag"). `interior_water_placement` can return up to `MAX_HALF_EXTENT = 2048` for exactly the large cells (`half_extent = span/2 + 32`, clamped), so those interiors take the exterior branch: `EXTERIOR_CELL_UNITS` (4096) of volume below the surface, where small pools get `DEFAULT_INTERIOR_VOLUME_DEPTH` (200). The constant's own comment (`:59-66`) names the 4096 default as the cause of an earlier "spuriously submerged" regression.
- **Evidence**: no test pins the depth selection; the only interior-placement tests (`water.rs:1770-1790`) check the extent and the empty fallback.
- **Impact**: the same pool gets a 20x different swim/buoyancy column depending on how far apart unrelated REFRs sit. Which side is closer to the original engine was not established; the inconsistency itself is the defect. Not measured against a real cell census.
- **Related**: #3548 (the interior `XCLW` gate), `docs/engine/watal.md` (interior volume).
- **Suggested Fix**: pass an explicit `interior: bool` (or the depth) from the two callers (`load.rs:594`, `exterior.rs:2115`) instead of inferring it from `half_extent`.

## Checked, no finding

- **Unload release** (`unload.rs:604-613`, `:948-981`): both component classes (`RapierHandles`, `Ragdoll`) are swept before the despawn; duplicated victims are idempotent; a missing `PhysicsWorld` is a no-op. Retained and protected victims keep their bodies by design (they are removed from the victim list first). The loading-cover stage retires through the same `release_entities` since #5193; the lone `ctx == None` arm (`loading_screen.rs:727-729`) is a teardown race and skips the Rapier release by design.
- **Cell-spawn collider creation**: `spawn_collision_shapes` / `spawn_trimesh_collider_ghost` / `spawn_packed_collision_proxy` / node-collision all seed a correct world `GlobalTransform` and the placement `FormId` tag; ghosts are stamped by the apply job's entity-range stamping.
- **NPC bone spawn** (`npc_spawn.rs:353-385`): bones with a shape are flipped Keyframed and tagged `ActorBoneCollider` + `ActorColliderOwner` before the first sync; a shape-less rig gets the fallback capsule (`:413-445`). The restore of an evicted actor's position is written to `Transform` before registration (`stream_snapshot.rs:339-381`), so the bodies register at the restored pose.
- **NPC KCC under streaming** (`locomotion.rs:177-304`): with no support within a step of the feet the vertical request is zero and the clamped fallback ray may find nothing, so an NPC stepping onto a not-yet-registered tile holds its height rather than falling; the `cast_ray_down` / `move_character` calls use the actor-bone mask.
- **Ragdoll activation vs registration**: activation removes the keyframed followers and the `RigidBodyData` recipes even when `physics_sync_system` has not yet registered them (`ragdoll.rs:539-547`); the queued-Late reconciliation (#5267) runs after PostUpdate propagation.
- **Water**: the multi-pond component volumes (`water.rs:530-582`) and the render grid use the same wet criterion (any corner at or below the surface), so a non-empty grid with an empty component list is unreachable except for malformed LAND (both conservatively fall back to the full cell); the LOD water plane carries no `WaterVolume` (pinned by `water.rs:1688`). Planes precede bodies in both the interior prefix and the exterior begin unit, so the quiesced-scene fast path in `apply_buoyancy` cannot miss a streamed-in wet sleeper.

## Known-Open Register (what this pass changed)

- New: PHYS-SD-01 (HIGH), PHYS-SD-02 (MEDIUM), PHYS-SD-03 (LOW), PHYS-SD-04 (LOW).
- Cited, not re-reported: GAME-SD-03 (corpse position not carried across eviction; the ragdoll never moves the actor root, so the death spot is lost); #5015 (XRGD decode for Skyrim+/FO4); #3477 and #4818 remain closed and their fixes are in place (PHYS-SD-01 and PHYS-SD-03 are gaps beside them).
- Unchanged: the Skyrim mass=0 reclassification note, capsule-clamp (#4134), contact-combine rule, solver budget; none is reachable from this area's code.

## Not checked / limits

- No engine launch, no Vulkan device, no game data: the corpse-ordering frequency (PHYS-SD-02), the interior-span census (PHYS-SD-04) and the cache cost (PHYS-SD-03) are code-derived, not measured.
- Watch item, not a finding: NPC wearables (armor, body, head, hair) are loaded through `spawn_nif_nodes`, which attaches any authored bhk body (`scene/nif_loader.rs:1604-1615`); only the player's visual-only parts are stripped (`player_body.rs:295`, `loot_appearance.rs:1005-1010`). Whether any shipped wearable NIF authors a collision body was not censused.
- `#[ignore]` FO3 real-data ragdoll gates and the skeever probe were not run.

Suggest `/audit-publish docs/audits/AUDIT_PHYSICS_2026-10-19.md` (labels `physics`; PHYS-SD-04 also `water`; PHYS-SD-01 also `save`; PHYS-SD-01 `game:fallout4` / `game:starfield`).
