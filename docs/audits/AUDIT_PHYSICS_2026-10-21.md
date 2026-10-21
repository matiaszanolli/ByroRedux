**HEAD**: `4a3e89d56` · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-16.md` (HEAD `d099eb68f`, the last full run). The two area-scoped runs since, `AUDIT_PHYSICS_2026-10-18.md` (exterior) and `AUDIT_PHYSICS_2026-10-19.md` (streaming-deep), were used for dedup only. · **Audited**: Dim 2 (Step & Sync: #5936 / #5937 / #5711 / #5697 deltas), Dim 3 (Ragdoll & Constraint Seam: the FO4 `bhkRagdollSystem` wave, the XRGD rig gates, stand-in capsule retirement, `stand_up`), Dim 5 (WATAL sink: #5933 / #5934) · **Unchanged since baseline (skimmed)**: Dim 1 (Shape Translation: `convert.rs` has a one-line doc change; `config.rs` and the classic `shape.rs` arms are unchanged), Dim 4 (Character & NPC Controller: only the `place_character_body` sanity gate and the `actor_is_swimming` helper changed; both checked), Dim 6 (Queries & Diagnostics: the corridor unification and the trap cache; both checked)

# PHYSAL / Physics Audit — 2026-10-21

Suite run (`--preset comprehensive`). Read-only on code, solo (no sub-agents), no engine launch. 352 commits since the baseline. 25 of them touch physics scope; the bulk is the FO4 ragdoll wave (#5015 → #6065–#6070).

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 5 |

- **PHYS-D3-2026-10-21-01 (MEDIUM)**: FO4 decoded-ragdoll corpses cannot be targeted at their body. The decoded bodies drive skeleton bones that carry no `ActorColliderOwner`. #6070 retires the only collider that did carry one, the stand-in capsule. So `interaction.rs` maps a hit on an FO4 corpse body to a bare bone entity. The corpse falls back to the placement-root bound, which the code itself calls stale once the ragdoll moves. The LOS check can also reject the corpse because its own body blocks the ray.
- **PHYS-D3-2026-10-21-02 (LOW)**: `stand_up` (revive and knockdown get-up) re-creates each bone's keyframed body from `RigidBodyData::default()`. That drops the authored `collidable` flag, `havok_layer`, surface material, friction and restitution. On FO4 it also adds stray rows to bones that never had a body.
- **PHYS-D2-2026-10-21-01 (LOW)**: `set_linear_velocity` is the one motion starter #5936 missed. A velocity given to a dynamic newcomer before its first step is still re-slept and zeroed.
- **PHYS-D2-2026-10-21-02 (LOW)**: contact-force subscription (#5711) depends on when the toggle happened. Ragdoll links and `set_motion_type` flips that exist at the toggle frame get tagged; later ones never do.
- **PHYS-D3-2026-10-21-03 (LOW)**: the #6070 guard `the_stand_in_ragdoll_template_never_replaces_a_decoded_one` scans a whole-file `include_str!` window that includes its own literals (the #4604 rule).
- **PHYS-D3-2026-10-21-04 (LOW)**: `combat.rs` still says "FO4 ragdolls do not activate yet".

**PHYSAL verdict.** The three named seams are still the only per-game branches. A grep of `crates/physics/src/**` and `byroredux/src/ragdoll{,_pose}.rs` for `GameKind` / `bsver` / game matches is empty. FO4's articulation enters through the constraint-CInfo seam: `decode_np_ragdoll` produces the same `RagdollCInfo` / `LimitedHingeCInfo` records the classic path does, and `crates/physics/src/ragdoll.rs` has had no logic change since the baseline. `ragdoll_pose.rs` picks a rig by its bodies' bone-name sets (`is_skyrim_posable_ragdoll` / `is_fo4_posable_ragdoll`), which is data, not `GameKind`. Collision data traced statically: the FO4 decoded skeleton path (`extract_np_ragdoll` → `template_from_imported` → `activate_ragdoll`), the classic FO3/FNV/Skyrim bone path, and the shape-less stand-in capsule path.

**Test runs (this pass, repo `target/`)**: `cargo test -p byroredux-physics`: 262 passed. `cargo test -p byroredux --bin byroredux -- <filter>`: `ragdoll` 57 passed and 11 ignored; `character` 65 passed; `locomotion` 16 passed; `water` 166 passed and 4 ignored. **Data gate**: `-- --ignored ragdoll::installed_tests` with `BYROREDUX_FO3_DATA` + `BYROREDUX_FO4_DATA` set: 8 of 8 pass (six FO3 environments, FO4 six-rig bind-pose free fall, and the MedTek XRGD-posed ghoul; 17 s). Not run: the skeever probe (needs `BYRO_SKEEVER_NIF`) and the two `ragdoll_pose` corpus tests.

## Solver Invariant Matrix

| Invariant | Verdict | Evidence |
|---|---|---|
| Fixed step / catch-up | **Verified (unchanged)** | `step` / `run_substep` have no logic delta. The accumulator clamp and the `SUBSTEP_TIME_BUDGET` drop are as baselined. |
| Wake discipline | **Verified, one gap** | #5936 (`00ec063ec`): `add_force` / `add_torque` / `apply_impulse` / `reset_forces` / `set_motion_type` with wake now call `forget_spawn_sleep` (`world/mod.rs:1054`). Gap: the public `set_linear_velocity` helper does not (PHYS-D2-2026-10-21-01). Spawn stays asleep-until-processed (`sync.rs:1100-1113`). |
| Lock order | **Verified** | The new stand-in block in `activate_ragdoll` (`ragdoll.rs:555-585`) takes `RapierHandles` under a read guard, drops it, frees the bodies under the `PhysicsWorld` write lock, then removes the rows. Same two-phase pattern as the #1772 bone teardown above it. All four activators (`push_actor_system`, `get_up_system`, `revive_system`, `reconcile_pending_dead_actors_system`) are exclusive systems, so the new `FallbackActorCollider` / `ActorColliderOwner` reads need no access declaration. |
| Phase order | **Verified (unchanged)** | `boot/schedule/physics.rs` has no delta. Buoyancy runs before the step. |
| Explosion recovery | **Verified** | `recovery.rs` unchanged. The FO4 decoded rigs hold together in the data gate (0 recoveries reported by the installed tests). |
| Teardown | **Verified, one gap** | Activation retires the stand-in capsule and `stand_up` restores it (#6070, `activation_retires_the_stand_in_capsule_and_stand_up_restores_it`). `stand_up`'s bone restore synthesises default rows (PHYS-D3-2026-10-21-02). |
| Damping (#5697) | **Verified** | `angular_damping_torque` = −c·I_world·ω, built in one place and used by the dry link pass and both wet arms (`water.rs:1224`, `:1376`). The explicit torque is held over at most `MAX_SUBSTEPS` = 5 substeps. Water c = 1.5 + authored, so c·Δt ≤ ~0.25: stable. |

## Findings

### PHYS-D3-2026-10-21-01: FO4 decoded-ragdoll corpses have no collider that resolves to the actor, so the body cannot be targeted and can occlude itself
- **Severity**: MEDIUM
- **Dimension**: Ragdoll & Constraint Seam
- **Location**: `byroredux/src/ragdoll.rs:555-585` (stand-in retirement), `byroredux/src/npc_spawn.rs:353-383` (`keyframe_live_ragdoll_bones`: owner only on bones with a `CollisionShape`), `byroredux/src/interaction.rs:891-935` (`select_interaction_target`), `:999-1062` (`corpse_collider_actors`), `:1064-1079` (`ray_hit_actor`), `:1081-1105` (`ray_hit_entity`), `:1150-1172` (`collider_belongs_to_target`)
- **Status**: NEW. Introduced by `7c6e6c3e1` (#6070) on top of `e1bd40bcf`. No match in open or closed issues ("corpse ActorColliderOwner", "ragdoll corpse loot target").
- **Trigger Conditions**: an FO4 actor whose skeleton carries a decoded `bhkRagdollSystem` template (human, feral ghoul, super mutant, molerat, …) is dead and ragdolled. Its body has moved off its placement-root bound, or pokes out of that bound toward the player.
- **Description**: Every interaction and combat path maps a ray hit to an actor through `ActorColliderOwner`. `keyframe_live_ragdoll_bones` stamps that marker only on skeleton bones that carry a `CollisionShape`. FO4 bones carry none, because the articulation lives in the blob. That is exactly why the stand-in capsule (`install_fallback_actor_collider`) is installed, and the capsule is the actor's only owner-marked collider. At death, #6070 frees the capsule's body and `RigidBodyData`, and the decoded bodies take over. They hang on bone entities that have no owner marker. The results:
  1. `corpse_collider_actors` keeps only owners whose marked entity has a live collider. The capsule no longer has one, and the bones are not marked, so the FO4 corpse never enters `physical_corpses`.
  2. `ray_hit_actor` resolves a hit on a ragdoll body through `ray_hit_entity`'s `Ragdoll` fallback to the bone entity. `get::<ActorColliderOwner>(bone)` is `None`, so the "actor" is the bone, which is not a candidate. The physical-hit path never fires.
  3. The corpse is then targeted only through the placement-root bound. The code documents that bound as wrong for a ragdoll: "Collider-backed corpses must be targeted at the body, not also at the stale placement-root bound after the ragdoll has moved away" (`:906-908`) and "A ragdoll may fall away from the placement root's bound" (`:919-921`).
  4. `target_has_line_of_sight` then casts toward that bound. If the corpse's own body is the first thing hit, `collider_belongs_to_target(bone, actor)` fails: the bone is not the actor, has no owner marker and no `FormIdComponent` / `PhysicsSourceForm`. The corpse is reported as occluded by itself.

  Classic rigs are unaffected: their bones were owner-marked at spawn and keep the marker through activation. Before #6070 the FO4 corpse body was the capsule (the one-body template), which was owner-marked.
- **Evidence**: `npc_spawn.rs:372-377` (owner inserted only under `has_shape`). `ragdoll.rs:575-583` (the capsule's `RapierHandles` and `RigidBodyData` removed). `interaction.rs:1012-1020` (owners filtered to marked entities) and `:1037-1050` (only marked entities among the live colliders count). `:1076-1078`: `ActorColliderOwner(collider).map_or(collider, …)` returns the bone.
- **Impact**: An FO4 corpse that a hit or a fall has moved cannot be looted by aiming at it. It is only reachable by aiming at where it was placed, and it can even be rejected there. The same unresolvable mapping affects every `ActorColliderOwner` consumer that meets a decoded FO4 ragdoll body (`combat.rs:414` `resolve_actor_root`, `projectiles.rs:388`, `commands/view.rs:343`). Those mostly matter for corpses, because no FO4 path knocks a living actor down: `PushActorRequest` is raised only by ObScript.
- **Related**: #6070, #5015, #6134 (the retired capsule also becomes another shape-only entity of that class).
- **Suggested Fix**: When a template is installed (or at activation), insert `ActorColliderOwner(placement_root)` on every template bone that lacks one, so all body producers share the classic invariant. Alternatively, have `ray_hit_entity`'s `Ragdoll` fallback return the ragdoll's owner. The first fix also covers combat and projectiles. Add an interaction test with a decoded multi-body template and no shaped bones.

### PHYS-D3-2026-10-21-02: `stand_up` rebuilds bone bodies from `RigidBodyData::default()` instead of the authored rows activation removed
- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam
- **Location**: `byroredux/src/systems/revive.rs:82-90` (bones), `:92-101` (stand-in, which matches its install recipe, so it is fine); compare `byroredux/src/ragdoll.rs:549-553` (activation removes the authored rows) and `crates/core/src/ecs/components/collision.rs:102-167` (fields)
- **Status**: NEW. `revive.rs` was added after the baseline (`d1b298921`, 2026-10-18). GAME-D4-2026-10-21-01/-02 cover the animation and idle half of `knock_down` / `stand_up`, not the collision rows.
- **Trigger Conditions**: a revived actor (`Resurrect`) or a knocked-down actor getting up (`get_up_system`) whose skeleton bones had authored bodies (FO3/FNV/Oblivion via `PushActorAway`, any game via `Resurrect`).
- **Description**: Activation deletes each body bone's `RigidBodyData`. That row is the imported row with only `motion_type` flipped to Keyframed (`keyframe_live_ragdoll_bones`). `stand_up` re-inserts `RigidBodyData { motion_type: Keyframed, ..Default::default() }`. That loses `collidable` (a bone authored on the Havok non-collidable layer comes back solid instead of as a sensor), `havok_layer` (→ 0 "Unidentified", which `ApplyHavokImpulse` scaling reads), `surface_material`, `part_surface_materials`, friction, restitution and mass. It also inserts the row for every template bone. On FO4 those bones never had a row: they have no `CollisionShape` and never register, so the rows are inert strays.
- **Evidence**: `revive.rs:84-88`. `collision.rs:149-167` (`static_body()`: friction 0.5, restitution 0.3, `havok_layer` 0, `collidable` true).
- **Impact**: After a get-up or revive, a live actor's keyframed bone colliders differ from the ones it spawned with. The visible effects are small (contact coefficients against clutter, the sensor/solid flag, the layer-scaled impulse). The state is still not what the content authored.
- **Related**: GAME-D4-2026-10-21-01, GAME-D4-2026-10-21-02, #6134.
- **Suggested Fix**: Have activation park the removed rows (for example a `RetiredBoneBodies(Vec<(EntityId, RigidBodyData)>)` component on the skeleton root) and have `stand_up` restore exactly those. Bones that had no row stay without one.

### PHYS-D2-2026-10-21-01: `set_linear_velocity` wakes a newcomer but does not take it off the spawn re-sleep list
- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/sync.rs:55-77`; compare `crates/physics/src/world/mod.rs:1054-1057` (`forget_spawn_sleep`) and its five callers (`:958`, `:982`, `:1172`, `:1195`, `:1232`)
- **Status**: NEW. A residual of #5936 (closed by `00ec063ec`, which covered the `PhysicsWorld` methods but not the crate's free helper).
- **Trigger Conditions**: a dynamic body registered this frame (on `spawn_sleep_pending`) that gets a nonzero `set_linear_velocity` before its first stepping frame, and moves less than `SPAWN_SLEEP_DRIFT_BU` (4 BU) on that frame.
- **Description**: `set_linear_velocity` calls `body.set_linvel(v, true)` and `pw.wake()`. It does not drop the handle from `spawn_sleep_pending`. After the stepping frame, `resleep_spawned_newcomers` finds the body awake, within 4 BU of its spawn, and calls `body.sleep()`, which zeroes the requested velocity. This is the #5936 swallow, reached through the one wake path the fix did not touch. The guard `explicit_impulse_keeps_newcomer_awake_through_first_step` covers impulses only.
- **Evidence**: `sync.rs:68-75` (no `forget_spawn_sleep`). `world/mod.rs:1037-1046` (re-sleep on `!drifted && is_dynamic && !is_sleeping`).
- **Impact**: There is one live caller today, `fly_camera_system`'s physics branch (`systems/camera.rs:121`), and only on the camera body's first frame. The risk is to the next producer that launches a fresh body by velocity (thrown or dropped items, scripted `SetVelocity`).
- **Related**: #5936, #5544.
- **Suggested Fix**: Route the helper through a `PhysicsWorld::set_linear_velocity(handle, v)` method that calls `forget_spawn_sleep` on a nonzero velocity, beside the other motion starters. Extend the #5936 test to the velocity path.

### PHYS-D2-2026-10-21-02: Which bodies emit contact-force events depends on when the subscription toggle flipped
- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/world/mod.rs:734-768` (`set_contact_force_events_enabled`), `crates/physics/src/sync.rs:1155-1172` (registration rule), `crates/physics/src/ragdoll.rs:446-457` (no `active_events`), `world/mod.rs:1236-1242` (`set_motion_type` → `dynamic_bodies`)
- **Status**: NEW (#5711 / `e3a4d5e18` follow-up)
- **Trigger Conditions**: `ImpactConfig` gains sound content. When it does, ragdolls and script-flipped dynamics exist at different times relative to the toggle.
- **Description**: The registration rule tags a collider only if its recipe is `MotionType::Dynamic`. The toggle instead retags every collider whose parent is in `dynamic_bodies`. That index also holds ragdoll links (`ragdoll.rs:408`) and bodies flipped to Dynamic by `set_motion_type`. So a ragdoll or flipped body that exists on the toggle frame starts emitting impacts. One built or flipped a frame later never does, because `build_ragdoll` and `set_motion_type` set no `active_events`. Disabling and then re-enabling changes the set again.
- **Evidence**: `world/mod.rs:740-755`; `ragdoll.rs:446-457`.
- **Impact**: Dormant today (`ImpactConfig` ships without content, `systems/audio.rs:372-377`). Once content lands, corpse-bone and scripted-body impact sounds will be inconsistent and depend on history.
- **Related**: #5711, #5557 (ragdoll colliders carry no surface material either).
- **Suggested Fix**: Pick one population (recipe-Dynamic, or `dynamic_bodies`) and apply it in all three producers. That means retagging in `set_motion_type` and having `build_ragdoll` read `contact_force_events_enabled`.

### PHYS-D3-2026-10-21-03: The #6070 stand-in-template guard scans a whole-file window that contains its own needles
- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam (guard hygiene)
- **Location**: `byroredux/src/npc_spawn/resumable/mod.rs:1166-1180` (`the_stand_in_ragdoll_template_never_replaces_a_decoded_one`)
- **Status**: NEW
- **Description**: The test takes `include_str!("mod.rs")` from `fn spawn_skeleton_unit(` to the end of the file. That window includes the test module and the test's own two needle literals, which the source-scan rule (#4604 / #4842 / #5164; `_audit-common.md` Methodology) forbids. If the production install call is renamed or reformatted while the guard text survives elsewhere in production, `find` matches the test's literal for the install, and `guard < install` passes vacuously. An ordering check also does not show that the guard wraps the call.
- **Evidence**: `resumable/mod.rs:1168-1179`. `no_source_scan_cuts_with_the_weak_cfg_test_needle` does not catch it, because the test makes no cut at all.
- **Impact**: The #6070 regression guard can go blind silently. The behaviour itself is also covered indirectly by the live smoke, but not in `cargo test`.
- **Suggested Fix**: Scan `byroredux_core::source_scan::production_text(include_str!("mod.rs"))`. Better: replace it with a behavioural test that runs `spawn_skeleton_unit`'s template step on a skeleton root that already holds a decoded `RagdollTemplate`.

### PHYS-D3-2026-10-21-04: Stale "FO4 ragdolls do not activate yet" in the death teardown
- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam (doc rot)
- **Location**: `byroredux/src/combat.rs:1051-1057`
- **Status**: NEW (made false by `e1bd40bcf` / `7c6e6c3e1`)
- **Description**: The #5861 comment justifies removing `ObjectAnimationGraph` with "FO4 ragdolls do not activate yet, so nothing overrides the restored pose and the corpse stands and idles". FO4 decoded ragdolls now activate at death, and `docs/smoke-tests/fo4-ragdoll.sh` gates them. The removal is still right, because the graph would keep driving the skeleton under the writeback. The stated reason is wrong.
- **Suggested Fix**: Say the graph is removed because, while it lives, its clips fight the ragdoll writeback on every rig.

## Checked, no finding

- **#5937 (`b3b473487`)**: `contact_event_position` now falls to the dynamic side when the pair is gone, before the origin midpoint. The fallback order is correct and pinned.
- **#5933 (`eab6b131f`)**: the reset gate includes the `in_current_prev` latch (sorted at `water.rs:1412`, so `binary_search` is valid). The no-marker early return releases through `release_current_forces` before it clears the latch. The `targets.is_empty()` return clears without releasing, which is safe because there is no dynamic body to hold a force. The quiesced fast path keeps the latch on purpose.
- **#5934 / #5697**: links in a marker-only current get authored damping through `angular_damping_torque`. The dry pass walks multibody links only.
- **FO4 template carry**: `template_from_imported` passes `pose_bone_id` through (`ragdoll.rs:171-173`, `fc2b32d9e`). `extract_np_ragdoll` maps each body to its own host node (one `BhkNPCollisionObject` per node), so two bodies never write one bone's `GlobalTransform`. Two bodies can share a *pose* id (COM and first spine). `entries_by_bone_id` claims them in order and refuses gaps and stray entries.
- **Stand-in recipe**: `stand_up`'s capsule row equals `install_fallback_actor_collider`'s (Keyframed default). `fallback_colliders_of` keys on the skeleton root's `Parent`, which matches `parent_part(placement_root, root)` and `ActorColliderOwner(placement_root)`.
- **#5960 motor tail** (Dim 3 byte side): owned by `/audit-nif`, which re-verified drift 0.
- **Corridor unification (`b4bff7784`)**: a zero-radius corridor is the same filtered ray as `cast_ray`. A NaN projectile radius is mapped to 0 upstream (`projectiles.rs:165`, `.max(0.0)`).
- **`place_character_body` (`267ccee0e`)**: refuses non-finite or out-of-world targets via `translation_is_sane` before any store.
- **`actor_is_swimming` (`4b99e2393`)**: uses the controller's own `depth_reaches_swimlevel` with the same half-span.
- **Compound ragdoll bodies (#6067)**: `build_ragdoll` splits a body's mass equally across its `collision_shape_to_parts` parts. That approximation predates the delta and has no vanilla evidence of harm. Noted, not filed.

## Known-Open Register (what this pass changed)

- FO4 ragdolls: live, and the data gate is green this pass (8/8 installed tests). New: PHYS-D3-2026-10-21-01 (corpse body not attributable to its actor). Still open: #6065 (other FO4 rigs) and the tilted-reference composition (466 of 971 posed FO4 corpses; `ragdoll_pose.rs` module doc).
- #6133 / #6134 / #6135 (PHYS-SD-02/03/04) are still open. The retired FO4 stand-in capsule (CollisionShape, no `RigidBodyData`) is one more shape-only entity of the #6134 class.
- #6048 (terrain LTEX Havok data) is still open.
- Already filed by this suite and not repeated here: GAME-D4-2026-10-21-01/-02 (knock_down / stand_up animation side), NIF-D5-2026-10-21-01/-02/-03 (FO76/SF tagfile blobs, `node_world` duplication, ragdoll-wave doc rot), PERF-D1-01 (trap contact residual).

## Not checked / limits

- No engine launch. The live FO4 smoke (`docs/smoke-tests/fo4-ragdoll.sh`) was not run. PHYS-D3-2026-10-21-01 is a static trace; no in-engine loot attempt was made.
- `crates/nif/src/blocks/collision/np_ragdoll.rs` byte offsets are `/audit-nif`'s.

Suggest: `/audit-publish docs/audits/AUDIT_PHYSICS_2026-10-21.md` (labels `physics`; add `game:fo4` + `gameplay` on PHYS-D3-2026-10-21-01, `test-gap` on -03, `doc-rot` on -04).
