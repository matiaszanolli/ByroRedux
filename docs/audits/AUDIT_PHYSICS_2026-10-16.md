**HEAD**: `d099eb68f` (agents 1-2 read `fa7591ca2`; the intervening commits do not touch physics scope) · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-12.md` (HEAD `5d31fd637`) · **Audited**: all six dimensions (Dim 1 Shape Translation, Dim 2 Step & Sync, Dim 3 Ragdoll & Constraint Seam, Dim 4 Character & NPC Controller, Dim 5 Water / Buoyancy, Dim 6 Queries & Diagnostics) · **Unchanged since baseline (skimmed)**: Dim 1 translation core (`convert.rs`, `config.rs`), Dim 3 solver/constraint code (only the XRGD corpse-pose commit changed)

# PHYSAL / Physics Audit — 2026-10-16

**Run**: `/audit-physics`, orchestrator + 3 sub-agents (Dims 2+6, 5+1, 3+4). Read-only except this report; no windowed engine launch. Dim 5/1 used a throwaway probe crate in the session scratchpad (binary removed, tree clean).

**Delta audited**: `5d31fd637..HEAD` (431 commits repo-wide; the physics-scope ones are #5698/#5699/#5700 `a2af867d9`, #5675 `0d8ea5d6e`, #5709 `5f01d1a7f`, #5591 `946310eb9`, #5530 `d28545728`, XRGD corpse pose `18b3d87a7`, encumbrance `ed59c81f7`, the M47.2 projectile/trap/`ApplyHavokImpulse` commits, and the new node-collider producer `fa1473e25`).

## Executive Summary

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | |
| HIGH | 1 | PHYS-D1-2026-10-16-01 |
| MEDIUM | 1 | PHYS-D5-2026-10-16-01 |
| LOW | 12 | D2-01, D2-02, D3-01, D3-02, D4-01, D4-02, D4-03, D5-02, D5-03, D6-01, D6-02, D6-03 (D4-03 cosmetic, D5-03 doc rot) |

14 findings in total. All are NEW; no open or closed issue title matched (title search only).

- **The HIGH is a content-loss regression from `fa1473e25` (2026-10-14)**: a collider authored on the root NiNode of a behavior-graph prop is paired with the root, which is never spawned, so neither spawn path creates it. A census over Skyrim SE `Meshes0/1.bsa` found 41 affected NIFs (pull chains, bear/Dwemer traps, ruin pieces, magic stones). The orchestrator spot-checked the code path (`pair_collisions` has no `parent.is_some()` filter; `spawn_nodes` only makes entities for nodes with a parent; `spawn_node_collisions` `continue`s on `node_entity == None`).
- **The MEDIUM is latent since #3114**: a dynamic body leaving a marker-only `WaterCurrentVolume` keeps the marker's last user force for life (probe: velocity climbs 7 → 27 past the authored 8 and still rising).
- **Prior findings closed**: PHYS-D2-2026-10-12-01/-02/-03 (#5698/#5699/#5700) are fixed by `a2af867d9`, verified in code with non-vacuous pin tests.
- **PHYSAL verdict**: the three named seams (constraint CInfo decode, `havok_scale_for`, collision-object-kind dispatch) remain the only per-game branches. A grep of `crates/physics` and `byroredux/src/ragdoll.rs` finds no `GameKind`/`bsver`/version branch. The one other per-game item is the XRGD skeleton-name table in `ragdoll_pose.rs`, keyed on bone names, not game version.
- **Games whose collision data was traced**: Skyrim SE (behavior-graph NIF census, XRGD humanoid rig), FO3 (real-data ragdoll gate). FNV/Oblivion/FO4/Starfield: not traced this pass (unchanged code).

**Lane counts at HEAD** (toolchain 1.96.0):
- `cargo test -p byroredux-physics`: **253 passed** (baseline 244).
- `cargo test -p byroredux --bin byroredux`, by filter: `ragdoll` 40 passed / 8 ignored; `character` 63; `locomotion` 14; `water` 166 / 4 ignored; `scheduler_access` 29; `workspace_hygiene` 13; `rapier_release` 9; `physics` 34 / 1 ignored; `footstep` 19; `submersion` 5.
- `cargo test -p byroredux-nif --lib -- collision bhk`: 183 passed.
- **FO3 real-data ragdoll gate** (`-- --ignored ragdoll::installed_tests`): **6 passed, 0 failed** (17.6 s). Still green, so the #5536/#5538 claim in the register holds.
- Not run: the `#[ignore]`d skeever probe, plugin `--ignored` tests.

## Solver Invariant Matrix

| Invariant | Verdict | Notes |
|---|---|---|
| Fixed step / accumulator | VERIFIED | NaN-safe `frame_dt.max(0.0)`, clamp before loop, budget bail-out drops backlog; sole wall-clock read is the budget timer. Fx sets are probed, never iterated into solver order. |
| Wake discipline | VERIFIED, one narrow gap | Spawn exemption intact. D2-01: the #5544 re-sleep can swallow a deliberate `ApplyHavokImpulse` on an un-stepped newcomer (LOW). |
| Lock order | VERIFIED | New `trap_contact_system` / `projectile_flight_system` take only a `PhysicsWorld` read. `place_character_body` uses one short scope per store. |
| Phase order | VERIFIED | collect/register → push kinematic → buoyancy → step → pull dynamic; pull writes `Transform`. |
| Explosion recovery | VERIFIED | Ladder unchanged; `accept_keyframe_target` now shares `translation_is_sane`. |
| Teardown / handle release | VERIFIED | `rapier_release_tests` green. `spawn_sleep_pending` is not pruned by `remove_body` (stale handles skipped, bounded; no finding). |
| Contact-force events | VERIFIED, one gap | D2-02: dynamic-side fallback unreachable once the pair is gone. |
| Water two-samplers rule | VERIFIED | No water input added to either side in the window. |
| Water latches (#5709) | VERIFIED | Scan-generation stamp and Fx latches correct; guards non-vacuous. D5-01/-02 are the exceptions on the force side. |

## Findings (deduplicated, ordered by severity)


### PHYS-D1-2026-10-16-01: A collider authored on the NIF root node of an animated prop is paired with the root, which is never spawned, and is silently dropped
- **Severity**: HIGH
- **Dimension**: Shape Translation (collider producer, cell loader)
- **Location**: `byroredux/src/cell_loader/graph_nodes.rs:132-155` (`pair_collisions`), `:268-270` (`spawn_nodes` skips `parent.is_none()`), `byroredux/src/cell_loader/spawn.rs:1428` (`spawn_node_collisions`), `:1481-1483` (`spawn_collision_shapes` skip)
- **Status**: NEW (regression introduced by fa1473e25, 2026-10-14; the detached collider spawned before it)
- **Description**: `pair_collisions` pairs each flat collision with the first node that carries a body and stands at the collision's pose. Its candidate set is every node with `collision.is_some()`, including the root. `GraphNodes::build` requires an identity root, so a root-hosted body's pose (identity) matches the root and `collision_nodes[i] = Some(root)`. Then (a) `spawn_collision_shapes` skips every collision whose `collision_nodes` entry is `Some`, and (b) `spawn_node_collisions` needs `spawned.node_entity(root)`, but `spawn_nodes` only creates entities for nodes with a parent (the root IS the placement root) and `entities` never holds the root, so it hits `else { continue }`. The collider is spawned by neither path. `collisions` is non-empty, so `missing_collision_fallback` does not engage either. The mesh side avoids the same trap explicitly ("when it hangs under one below the root"); the collision side lacks that exclusion. The only test (`a_nodes_own_body_rides_the_node_as_a_keyframed_child`) puts the body on a child node and builds the graph with `GraphNodes::for_test`, bypassing `pair_collisions`.
- **Evidence**: census probe over Skyrim SE `Meshes0/1.bsa` (parse + `import_nif_scene` + `import_nif_with_collision`, same conditions as `build`): 1199 NIFs carry `BSBehaviorGraphExtraData`; 411 have collision only on child nodes, 41 have it on the ROOT node, and all 41 pass `build`'s mesh-alignment and identity-root conditions, so `graph_nodes` is `Some` for all of them. Examples: `meshes\dungeons\nordic\levers\pullchain\genpullchain01.nif` and `norpullchain01.nif`, `meshes\traps\beartrap\beartrap01.nif`, `trapdwefirepillar01.nif`, `trapmagiccaster01.nif`, `dweobservatoryarmillary01.nif`, `dwebrentrance01.nif` (14 colliders), `dwekagrumezgauntlet.nif` (9), `dlc01sebf_room.nif`, `seruinstemplepuzzle01.nif`, `dragonmounddirtcap.nif`, the four `magicstone*01.nif`, `dragonbones*.nif`, `smelter01.nif`, `eyeofmagnus01.nif` (57).
- **Impact**: those props lose their root body entirely, and any child body at identity pose pairs with the root too. Players walk through pull-chain mounts, trap bases, ruin pieces and room shells; an unrelated physics-driven behaviour (hit/trap events keyed on these bodies, `ApplyHavokImpulse`) finds no body. No log line announces it (the debug log in `spawn_node_collisions` is after the `continue`).
- **Trigger Conditions**: any placed reference whose NIF has a `BSBehaviorGraphExtraData` root and a `bhkCollisionObject` on the root NiNode, with an identity root transform.
- **Related**: fa1473e25, `graph_nodes::build` mesh-vs-collision asymmetry, #5686 / 696da4f47 (same area, no overlap).
- **Suggested Fix**: restrict `pair_collisions` to nodes with a parent (`nodes[index].parent.is_some()`), exactly like the mesh pairing, so root-hosted bodies stay on the flat detached spawn; add a `pair_collisions`-level test (root + child, body on root) and a spawn test asserting one detached collider. Alternatively map the root to `placement_root` in `SpawnedNodes::node_entity`, but then the root body would be re-keyframed onto the placement root, which is not what the flat spawn did.



### PHYS-D5-2026-10-16-01: A body that leaves a marker-only current keeps the marker's last force forever
- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy
- **Location**: `crates/physics/src/water.rs:1070` (reset gate), `:1290-1337` (marker arm), `:786-795` (no-marker early return)
- **Status**: NEW (latent since #3114's gate; sits in the function #5698 reworked)
- **Description**: rapier user forces persist across steps until `reset_forces`. The #3114 fix scopes the per-frame reset to `current_flow.is_some() || surface.is_some() || t.prior_wet`. A placed `WaterCurrentVolume` that overlaps no water plane never sets `WaterContact`, so `prior_wet` is false for its occupants. The frame a body stops being inside any marker (it moves out of the box, or the marker streams out and the scan takes the `surfaces.is_empty() && current_volumes.is_empty()` early return) none of the three terms holds, nothing resets, and the last `add_force` stays on the body for the rest of its life. `in_current_prev` already records exactly who was in a marker last scan but is only used for the wake latch.
- **Evidence**: probe, zero gravity, 20 kg ball, +X marker of speed 8, no plane, marker despawned after 30 frames:
  ```
  in marker  frame 29: user_force=(80.78,0,0) linvel=(7.06,0,0)
  after exit frame 59 : user_force=(80.78,0,0) linvel=(11.10,0,0)
  after exit frame 299: user_force=(80.78,0,0) linvel=(27.25,0,0)   // past the authored 8, still rising
  ```
  The force is the last proportional term `m*c*(s - v.d)`; with no further evaluation it becomes a constant push, so the body overshoots the authored current speed without bound (air / low friction). Multibody links are immune (the dry-link pass resets them), plain dynamic bodies are not.
- **Impact**: crates / barrels / corpses-before-activation leaving a river marker region that is not covered by a plane band get a permanent phantom thrust (up to `m*c*s`, 640 N for 20 kg at the defaults). It survives sleep and resumes on the next wake. Friction usually pins a resting body, so the visible cases are airborne or sliding bodies.
- **Trigger Conditions**: an FO3/FNV/Skyrim cell with an XWCU/XPRM current marker whose box extends beyond its plane's footprint or above its water band (the #5129 configuration), and a dynamic body that is awake inside it and then leaves it (pushed, carried by the current out of the far end, or the marker's reference unloads while the body persists).
- **Related**: #3114 (introduced the gated reset), #3268 (`in_current_prev` latch), #5698 (same function, link side only).
- **Suggested Fix**: add `|| in_current_prev.binary_search(&t.entity).is_ok()` to the reset gate (the latch is sorted and survives the quiesced path), and in the no-marker early return reset the forces of the bodies still listed in `in_current_prev` before clearing it (needs an entity -> handle lookup through `RapierHandles`). Add a leave-the-marker test next to `current_volume_without_a_water_plane_does_not_wind_up_user_force`.

### PHYS-D5-2026-10-16-02: A multibody link inside a marker-only current is "water-owned" but receives no authored damping
- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**: `crates/physics/src/water.rs:1290-1337` (marker arm), `crates/physics/src/world/mod.rs:1033-1057` (`apply_multibody_link_damping` skip)
- **Status**: NEW (follow-on of the #5698 fix; before the fix the link was damped but the current was wiped)
- **Description**: #5698 registers the body in `water_force_targets` from the current arm so the dry pass stops wiping the marker force. The same registration also makes the dry pass skip the link, and the marker arm adds only `current_force` (a drag toward the flow speed along the flow axis). Unlike the surface arm it folds in no `-c*m*v` / torque for the link's own authored coefficients, which rapier never applies to links (#5540). Net: authored linear and angular damping vanish for a link while it is inside a marker that no plane surface claims.
- **Evidence**: probe, two-body damped ragdoll (linear damping 1.0, mass 10) free-falling for 1 s under default gravity:
  ```
  no marker (dry damping owns)       vy = -436.2
  marker-only current, speed 0       vy = -683.8   // ~ undamped free fall (-686.7)
  ```
  A zero-speed marker isolates the effect: it contributes no force off the flow axis.
- **Impact**: a ragdoll (corpse) falling or tumbling through a placed current volume that overlaps no plane is un-damped for as long as it is in the box (about 57% faster after one second at damping 1.0). Cosmetic-to-moderate fidelity loss; no instability seen. Narrow trigger (marker-only region, awake link).
- **Trigger Conditions**: awake ragdoll link, centre inside a `WaterCurrentVolume` box, no plane surface within its hysteresis band at that XZ.
- **Related**: #5698, #5540, #5697 (angular link damping stand-in).
- **Suggested Fix**: in the marker arm, when `pw.is_multibody_link(body)`, add the same `-c*m*v` force and `-c_ang*m*w` torque the surface arm builds from `b.linear_damping()` / `b.angular_damping()` (without the water coefficients), or let the dry pass run additively for marker-only owners instead of skipping them.

### PHYS-D5-2026-10-16-03: Two comments left false by the delta
- **Severity**: LOW
- **Dimension**: Water / Buoyancy (doc rot, reported once for Dim 2/6 owners)
- **Location**: `crates/physics/src/world/mod.rs:1043` and `:1089-1093`
- **Status**: NEW
- **Description**: (a) `apply_multibody_link_damping` still says "Cheap body flags before the SipHash set lookup" after #5709 made `water_force_targets` an `FxHashSet`. (b) `PhysicsWorld::apply_impulse` still says "No production consumer yet (#2889)", but `scripted_havok_impulse_system` (`byroredux/src/systems/cinematic.rs:376`, 696da4f47) now calls it; only the WATAL splash-kick consumer the comment names is still unbuilt.
- **Impact**: misleads the next reader about hashing cost and about who exercises `apply_impulse`.
- **Related**: #5709, 696da4f47, #2889.
- **Suggested Fix**: drop "SipHash"; reword (b) to name the scripted consumer and keep the splash-kick note.



### PHYS-D2-2026-10-16-01: The #5544 spawn re-sleep swallows a user wake (and zeroes the velocity) of a newcomer that has not yet had a stepping frame
- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/world/mod.rs:968-1002` (`resleep_spawned_newcomers`, drift test at 984, `body.sleep()` at 987); producer `crates/physics/src/sync.rs:1108-1111`; consumer path `byroredux/src/systems/cinematic.rs:376` (`ApplyHavokImpulse` -> `PhysicsWorld::apply_impulse`, `world/mod.rs:1094-1105`)
- **Status**: NEW (the skill's "confirm a same-frame wake cannot be swallowed" check; read from code and rapier source, not probe-measured)
- **Trigger Conditions**: a dynamic prop that streamed in but has had no stepping frame since (a quiescent cell: no animated actor pushing kinematic targets, nothing else armed `pending_wake`), then a script `ApplyHavokImpulse` (M47.2, 696da4f47) whose resulting speed moves the body less than `SPAWN_SLEEP_DRIFT_BU` (4 BU) during that frame, i.e. below roughly 240 BU/s over one 1/60 s substep.
- **Description**: `spawn_sleep_pending` is filled at registration and drained only by a stepping frame. `apply_impulse` wakes the body and arms `pending_wake`, so the next step runs and integrates the impulse. After the step, `resleep_spawned_newcomers` re-sleeps every pending body that is dynamic, awake and within 4 BU of its spawn pose. That test cannot tell rapier's begin-touch strong-wake (the thing it exists to undo) from a deliberate engine wake. Only the water pass's targets are exempt (`water_force_targets`). Rapier's `RigidBody::sleep()` also zeroes the linear and angular velocity (rapier3d-0.36.1 `rigid_body.rs:915-918`), so the impulse is erased, not merely paused.
- **Evidence**: `grep spawn_sleep_pending` shows exactly one push (sync.rs:1110) and one `take` (mod.rs:969); neither `apply_impulse` nor `set_motion_type` removes the handle. The new unit test `spawn_resleep_drops_only_settled_newcomers_force_events` covers settled-vs-drifted only, with no engine-wake case.
- **Impact**: a gentle scripted nudge on the first stepping frame after an object streams in is lost once (the pending list is then consumed, so a repeat works). Strong impulses (above ~3.4 m/s) are unaffected. No instability, no state corruption.
- **Related**: #5544, #5700, #3969 (spawn wake exemption), M47.2 `ApplyHavokImpulse`.
- **Suggested Fix**: drop the handle from `spawn_sleep_pending` (or its drift reference) whenever an engine path deliberately wakes it (`apply_impulse`, `set_motion_type(wake_up)`, `add_force(wake_up=true)`), or exempt bodies whose velocity was set by the engine in the same frame.

### PHYS-D2-2026-10-16-02: `contact_event_position` falls to the old origin midpoint when the narrow-phase pair is already gone, skipping its own dynamic-side fallback
- **Severity**: LOW
- **Dimension**: Step & Sync (contact events)
- **Location**: `crates/physics/src/world/mod.rs:748-785` (the dynamic-body loop at 770-778 sits inside `if let Some(pair) = ... contact_pair(...)`; the origin-midpoint tail at 779-786)
- **Status**: NEW (partial gap in #5675's fix; code-read only)
- **Trigger Conditions**: a `started` force-crossing recorded in an earlier substep of a multi-substep frame (catch-up frames), the body then bounces away far enough that the broad phase drops the pair before `impact_audio_system` resolves the event, with one parent body parked at an origin far from the contact (static kit ghost, exterior terrain tile).
- **Description**: the doc says the order is deepest solver contact, then the dynamic side's translation, then the origin midpoint. In code the dynamic-side fallback is only reachable while `contact_pair` still returns the pair. If the pair has been removed, control goes straight to the origin midpoint, which is exactly the #5675 symptom (impact sound ~150 m away for a terrain tile).
- **Evidence**: structure of the function: `if let Some(pair) = ... { manifold loop; if deepest {return}; for handle in [a,b] { if dynamic {return translation} } }` then `pa/pb` midpoint.
- **Impact**: rare mis-placed (inaudible) impact sound on catch-up frames with fast, high-restitution bounces. Cosmetic.
- **Related**: #5675, `contact_event_position_is_the_contact_not_the_body_origins` (single-substep drop, does not cover the removed-pair case).
- **Suggested Fix**: move the dynamic-side loop out of the `if let Some(pair)` block so it precedes the origin midpoint unconditionally.



Severity summary: 0 CRITICAL / 0 HIGH / 0 MEDIUM / 2 LOW (both non-behavioural: a test gap and doc rot). No new solver, seed, or seam defect survived disproof.

### PHYS-D3-2026-10-16-01: The XRGD corpse-pose wiring (spawn parking, consume-before-activate, one-shot) has no test
- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam
- **Location**: `byroredux/src/combat.rs:1040-1063` (reconcile wiring), `byroredux/src/cell_loader/references/mod.rs:765-785` (parking), tests in `byroredux/src/ragdoll_pose_tests.rs`
- **Status**: NEW (feature itself tracked by Existing #5015, still open for FO4/creatures)
- **Description**: `ragdoll_pose_tests.rs` covers `bone_poses`, `body_entries`, `is_humanoid_ragdoll` and `pose_skeleton` in isolation, plus an ignored corpus-count test. Nothing exercises the two call sites that make the feature work: (a) the spawn-side condition `placed_ref.starts_dead && !placed_ref.ragdoll_pose.is_empty()` that inserts `AuthoredRagdollPose`, and (b) `reconcile_dead_actor` taking the component, calling `pose_skeleton` strictly BEFORE `activate_ragdoll`, consuming it whether or not it applies, and surfacing the "posed N bones from XRGD" / "XRGD pose not applied" note. `grep AuthoredRagdollPose byroredux/src` hits only `combat.rs`, `ragdoll_pose.rs`, `boot/world.rs` (register), the spawn site, and the save-registry exclusion list.
- **Evidence**: `rg -l AuthoredRagdollPose byroredux/src` -> combat.rs, ragdoll_pose.rs, boot/world.rs, registry_completeness_tests.rs, cell_loader/references/mod.rs; no `#[test]` in combat.rs or the loader references the component. Swapping the order of the two calls in `reconcile_dead_actor` (seed then pose), or dropping the `remove_component_taking`, would keep every current test green.
- **Impact**: Code works today (the commit message reports a live RannveigsFast01 check). The risk is regression: the ordering is the whole point (the seed validator and `activate_ragdoll` read the posed globals) and is currently pinned only by prose.
- **Trigger Conditions**: A future refactor of `reconcile_dead_actor` or of the starts-dead branch in `load_references_budgeted`.
- **Related**: #5015, `registry_completeness_tests.rs` (documents the non-saved status)
- **Suggested Fix**: Add one headless test using the `humanoid_chain` fixture: insert `AuthoredRagdollPose` + `Dead` on the root, run `reconcile_dead_actor`, assert the COM `Transform.translation` is zero, the component is gone, and the returned note starts with "; posed 18 bones" (or "XRGD pose not applied" for a renamed body); plus a source-order guard that `pose_skeleton(` precedes `activate_ragdoll(` inside `reconcile_dead_actor`.

### PHYS-D3-2026-10-16-02: `physal.md` / `physics.md` still describe ragdoll activation as the console trigger only and say nothing of the XRGD pose or the death-reconciliation path
- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam
- **Location**: `docs/engine/physal.md:213-219` ("On the `ragdoll <id>` console trigger, `activate_ragdoll` reads each bone's current `GlobalTransform`..."), `docs/engine/physics.md` (no mention)
- **Status**: NEW
- **Description**: Production activation is `combat.rs::reconcile_dead_actor` (live kills, Starts-Dead actors, save-load sweep); the console `ragdoll` command is the debug path. Since `18b3d87a7` that path first poses a Skyrim humanoid corpse from its XRGD via `ragdoll_pose.rs`, a fourth data interpretation (entry k = body k, Gamebryo XYZ negated-angle Euler, COM at placement) that is neither one of the three PHYSAL §3 seams nor described anywhere under `docs/engine/` (`grep -i "xrgd\|ragdoll_pose\|authored corpse" docs/engine/*.md` is empty).
- **Evidence**: `docs/engine/physal.md:218`; `byroredux/src/combat.rs:1040-1063`; `byroredux/src/ragdoll_pose.rs:10-50` (module docs hold the only description).
- **Impact**: The "ground truth" doc used by this audit under-describes where per-skeleton knowledge lives, so a future audit could either miss the fifth entry point for ragdoll seeding or mis-flag the bone-name table as a seam violation.
- **Related**: #5701 (previous physal.md sync, closed), #5015
- **Suggested Fix**: Add a short "Authored corpse pose (#5015)" paragraph to physal.md's Translate section (death-reconciliation activation, the humanoid-only pin, refusal -> collapse, not saved) and note that it is skeleton-keyed data, not a `GameKind` branch.



Severity summary: 0 CRITICAL / 0 HIGH / 0 MEDIUM / 3 LOW (defence-in-depth and cosmetic). No rule in the skill text regressed.

### PHYS-D4-2026-10-16-01: The over-encumbrance speed multiplier is taken raw from a GMST and reaches the player translation unclamped
- **Severity**: LOW
- **Dimension**: Character & NPC Controller
- **Location**: `byroredux/src/encumbrance.rs:63-69,84-90` (source), `byroredux/src/systems/character.rs:331` (`speed_mul = (..) * encumbrance_speed`)
- **Status**: NEW
- **Description**: For Fallout 3/NV the multiplier is `index.game_setting_float("fMoveEncumEffect")` (any `f32` the load order authors, including NaN/negative/huge) and `PlayerEncumbrance::movement()` returns it verbatim when over capacity. `character_controller_system` multiplies it into `controller.move_speed` for both walking and swimming. Nothing between there and `move_character` (which does no sanitising of `desired_translation`) or the `Transform` write at `character.rs:608` checks finiteness, and `integrate_vertical`'s `v < terminal` comparison does not launder NaN.
- **Evidence**: `GameSetting` float -> `OverEncumbered::SpeedMult(mult)` -> `(mult, true)` -> `horizontal_motion(.., controller.move_speed * speed_mul, dt)` -> `new_pos = current_pos + result.translation` -> `t.translation = new_pos`. A negative multiplier reverses the controls; NaN/inf poisons the player `Transform` (and every consumer of it) until another teleport.
- **Impact**: Only a hostile/broken plugin GMST triggers it (vanilla is 0.4), and only while over-encumbered. Hardening gap, same class as #5530/#4134 (which sanitised a saved pose and capsule extents).
- **Trigger Conditions**: FO3/NV load order whose `fMoveEncumEffect` is non-finite or negative, and a player carrying more than Carry Weight.
- **Related**: #5530, #4134
- **Suggested Fix**: Sanitise at install: `.filter(|m| m.is_finite() && *m >= 0.0).unwrap_or(FALLOUT_MOVE_ENCUM_EFFECT)` in `install_encumbrance_rules` (and an upper clamp if a speed-up is not meant to be authorable).

### PHYS-D4-2026-10-16-02: Console movers accept non-finite coordinates: `actor.walk` (NaN duration never expires) and `cam.pos` via `place_character_body`
- **Severity**: LOW
- **Dimension**: Character & NPC Controller
- **Location**: `byroredux/src/commands/anim_event.rs` (`ActorWalkCommand::execute`), `byroredux/src/systems/graph_locomotion.rs:73-90` (`console_walk_system`), `byroredux/src/commands/view.rs:1190-1215` (`cam.pos`), `byroredux/src/systems/character.rs:921-950` (`place_character_body`)
- **Status**: NEW
- **Description**: `actor.walk` parses `speed`/`seconds` with `f32::parse` (accepts `nan`/`inf`) and stores them with no bounds. In `console_walk_system`, `step = dt.min(walk.remaining).max(0.0)` with `remaining = NaN` is `dt`, and `remaining -= step` stays NaN, so `remaining <= 0.0` is never true: the actor walks forever (until it dies, `reconcile_dead_actor` removes `ConsoleWalk`). An `inf` speed writes an infinite translation straight into the actor's `Transform` (no KCC, no `translation_is_sane`). Likewise `cam.pos nan 0 0` flows camera -> `snap_character_body_to_camera` -> `place_character_body`, which, now being the single home of the player teleport, writes the position to `Transform`/`GlobalTransform` unvalidated (`apply_player_pose` validates with `PhysicsWorld::translation_is_sane` only for the save path).
- **Evidence**: see Description; `cargo test` pins none of this. `console_walk_system` and `ActorWalkCommand` have no non-finite test.
- **Impact**: Debug/console surface only (same exposure as every other console mutator); the failure is a NaN/inf player or actor transform or a perpetual forced walk, recoverable by another teleport/kill. Reported because #5591 created the single point where one guard would cover every player teleport.
- **Trigger Conditions**: An operator (or `byro-dbg` script) passing `nan`/`inf` to those commands.
- **Related**: #5530 (apply_player_pose guard), `PhysicsWorld::translation_is_sane`
- **Suggested Fix**: In `ActorWalkCommand`, reject non-finite `speed`/`seconds`/`turns` and cap `seconds`; in `place_character_body`, return `None` (and log) when `!PhysicsWorld::translation_is_sane(pos)` so `apply_player_pose`'s check becomes defence in depth rather than the only guard.

### PHYS-D4-2026-10-16-03: `FOOTSTEP_SURFACE_PROBE_REACH` is declared mis-indented at module scope between two structs
- **Severity**: LOW
- **Dimension**: Queries & Diagnostics (path owned by Dim 4/6)
- **Location**: `crates/physics/src/world/queries.rs:30-38`
- **Status**: NEW
- **Description**: The #5743 constant and its doc comment are indented four spaces at module scope (as if inside an `impl`) and wedged between `CharacterMoveResult` and the doc comment of `CharacterMoveParams`. `cargo fmt` would re-indent it, so the file is not fmt-clean and the const reads as belonging to the neighbouring struct. Behaviour is unaffected (it is re-exported at `world/mod.rs:20` and used by `systems/audio.rs:281`; `footstep_probe_reach_clears_the_player_capsule_centre` pins the 64 BU vs 128 BU margin).
- **Impact**: Cosmetic / diff noise for the next `cargo fmt`.
- **Suggested Fix**: Re-indent and move it next to `PhysicsWorld::surface_material_under`, its only consumer-facing API.



### PHYS-D6-2026-10-16-01: `trap_contact_system` pays an O(all rigid bodies) scan, an O(all physics entities) SipHash map and an unfiltered shape-overlap every tick
- **Severity**: LOW
- **Dimension**: Queries & Diagnostics (cost)
- **Location**: `byroredux/src/systems/trap_hits.rs:86-98` (full `query::<RigidBodyData>()` filter on layer 14), `:146-150` (`owners` `HashMap` over every `RapierHandles`), `:170-177` (`bodies_overlapping_body` per trap), `crates/physics/src/world/queries.rs:577-601`; registered unconditionally at `byroredux/src/boot/schedule/update.rs:567`
- **Status**: NEW (hand to `/audit-performance` for the number; not measured here)
- **Trigger Conditions**: every tick, every game and cell. The scan runs even when no trap exists (the early return at `:143` comes after it). The map and the overlaps are built only while a scripted Trap-layer body exists or existed last tick (Skyrim dungeons with dart/swing traps, some exteriors).
- **Description**: (a) the Trap-layer discovery iterates every `RigidBodyData` (statics included, tens of thousands in an exterior) each frame to find a handful of layer-14 bodies that only change at cell load. (b) When traps exist, `owners` inserts every `(body, entity)` of the whole world into a std `HashMap` each tick, only to look up the few `others` returned by the overlap. (c) `bodies_overlapping_body` intersects the trap's colliders with every solid collider in its AABB, including the wall or floor it is mounted in, then the caller discards all but live actors; an actor-only `QueryFilter` predicate (interaction groups) would skip the trap-vs-world-mesh tests entirely.
- **Evidence**: code structure above; `trap_layer` is collected before the `traps.is_empty()` return, `owners` is built before the overlap block, `bodies_overlapping_body` filters only `exclude_sensors().exclude_rigid_body(body)`.
- **Impact**: a steady per-frame CPU tax scaling with loaded body count (order of 0.1-0.5 ms scan on a 95 k-body exterior by analogy to the #4682 / PHYS-D2-2026-10-12-02 numbers; several ms for the SipHash map when a trap is present in an exterior). CPU bound is the project's stated bug class, but this is small in interiors.
- **Related**: #4682, #2923 (hot-path hashing), PHYS-D2-2026-10-12-02 (same walk-the-arena shape, fixed).
- **Suggested Fix**: maintain the trap-body list incrementally (rebuild only when `RapierHandles` / `RigidBodyData` structure changes, or on cell load), resolve `owners` lazily for the few overlap results, and give the overlap query an actor-group predicate.

### PHYS-D6-2026-10-16-02: `projectile_flight_system` rebuilds per-entity maps over the whole world every frame a projectile is in flight
- **Severity**: LOW
- **Dimension**: Queries & Diagnostics (cost)
- **Location**: `byroredux/src/systems/projectiles.rs:196-223` (`owners` Vec, `launcher_bodies` per shooter, `actor_bodies` set), `:303-346` (`launcher_bodies` up to 16 `Parent` lookups per physics entity), `:259-266` (linear `owners.iter().find_map`)
- **Status**: NEW
- **Trigger Conditions**: any `Weapon.Fire` projectile in flight (script-driven: traps, scripted launchers); cost scales with loaded physics entities.
- **Description**: every frame, for each distinct shooter, `launcher_bodies` walks all `RapierHandles` entities with up to 16 `world.get::<Parent>` lock acquisitions each plus a `PhysicsSourceForm` lookup, and `actor_bodies` calls `resolve_actor` (up to 3 `world.get`) for every entity. The launcher's collider set and the actor set are stable across the flight but are recomputed per frame.
- **Evidence**: code structure above (no caching across frames; `launchers` is a local `HashMap`).
- **Impact**: on a 20-95 k-entity exterior, a few hundred thousand storage lock acquisitions per frame for the ~1-2 s of an arrow's flight. Interior cost is small. Not measured.
- **Related**: PHYS-D6-2026-10-16-01 (same pattern), #2923.
- **Suggested Fix**: cache the launcher body list on the `Projectile` (or `ActiveProjectiles`) at launch, and resolve actor-ness only for the bodies a cast returns (the predicate can test `resolve_actor` on demand) instead of precomputing a set over all bodies.

### PHYS-D6-2026-10-16-03: `cast_ray_corridor_where` duplicates `cast_ray_corridor`'s shape-cast body and differs on two edge cases
- **Severity**: LOW
- **Dimension**: Queries & Diagnostics (tech debt)
- **Location**: `crates/physics/src/world/queries.rs:353-390` vs `:304-345`
- **Status**: NEW
- **Description**: the new function is a copy of `cast_ray_corridor` with a predicate in place of `excluded_body`. Divergences: it has no `corridor_radius <= 0` fallback to `cast_ray` (it clamps to a 0.01 BU ball), and its predicate (`collider.parent().is_some_and(allow)`) excludes parentless colliders while `cast_ray_corridor` includes them (none exist in production today, every collider is inserted with a parent). The user's rule is to extend existing code rather than duplicate logic.
- **Impact**: none today; future edits (e.g. the ShapeCastOptions flags, which carry the "start inside a wall" semantics) can drift between the two.
- **Related**: #5160.
- **Suggested Fix**: implement `cast_ray_corridor` as `cast_ray_corridor_where` with an `exclude_rigid_body` predicate (keeping its zero-radius `cast_ray` fallback), or share a private helper taking a `QueryFilter`.


## Known-Open Register (what this pass changed)

- **Closed this pass**: PHYS-D2-2026-10-12-01/-02/-03 → #5698/#5699/#5700 fixed (`a2af867d9`).
- **Newly opened here**: D1-01 (HIGH), D5-01 (MEDIUM), the LOWs above. Not yet issues — `/audit-publish` will file them.
- **Still open, unchanged**: #5015 (FO4 and creature corpse poses; Skyrim humanoid pose shipped in `18b3d87a7`), #4772, #5697 (angular link damping stand-in), the mass-inverted-rig upstream root cause (no issue; FO3 gate green), ball-and-socket/stiff-spring/chain constraints declined by design, water-walking/freezing unbuilt.
- **Register edits suggested for the skill** (`/audit-sync`): add `fa1473e25`'s node-collider producer (`graph_nodes::pair_collisions`, `spawn_node_collisions`) to Dim 1 scope; add `ragdoll_pose.rs` XRGD wiring checks to Dim 3 (already partly present); note the `contact_event_position` fallback order and the `water_force_targets`-skips-link-damping interaction in Dim 2/5.
- **Handoffs**: D6-01/-02 (per-tick O(world) scans in `trap_contact_system` / `projectile_flight_system`, structural, unmeasured) → `/audit-performance`. `water_damage_system`'s `Access` list omits the `PapyrusPlayerEntity` read and `StoryEvent` write (exclusive system, so no conflict) → `/audit-concurrency`.

---

# Per-dimension notes (status, checklist verdicts, carry-overs)

## PHYSICS AUDIT — Dim 1: Shape Translation

**HEAD**: `fa7591ca2` · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-12.md` (HEAD `5d31fd637`)
**Status**: translation core UNCHANGED (`crates/physics/src/convert.rs`, `config.rs` have zero diff since baseline); importer and producers CHANGED. Commits reviewed: `8bd4f3b8b` (weak `cfg(test)` cut migration in `nif/src/import/collision/mod.rs`), `696da4f47` (adds `RigidBodyData::havok_layer`, set in `extract_from_classic`), `4e0c962c4` (#5686 `part_surface_materials` overlay), and, because it is a NEW collider producer the path list did not name, `fa1473e25` (`spawn_node_collisions` / `graph_nodes::pair_collisions`, M47.2, 2026-10-14). Also read: `sync.rs::register_newcomers` (diff is a comment only), `world/queries.rs` (`character_capsule`, unchanged routing).

**Guards run** (`cargo test -p byroredux-physics`, filtered): `trimesh_flag_bits_match_rapier_definitions`, all four `huge_finite_*`, `non_finite_extents_clamp_in_the_debug_lane_too`, `sanitize_child_pose_neutralizes_non_finite_lanes_and_keeps_finite_ones`, `character_capsule_sanitises_non_finite_and_absurd_extents`, `every_registered_collider_carries_the_havok_contact_combine_rule`, `every_ragdoll_collider_carries_the_havok_contact_combine_rule`, `restitution_zero_body_does_not_bounce_on_restitution_floor`: 16 passed. Orchestrator lanes: physics crate 253 passed, `nif --lib -- collision bhk` 183 passed.

### Checklist verdicts

| Item | Verdict |
|---|---|
| Every `CollisionShape` variant has a translation arm | 7 variants (`Ball`, `Cuboid`, `Capsule`, `Cylinder`, `ConvexHull`, `TriMesh`, `Compound`), 7 arms in `flatten_to_parts`; no new variant in the window. `BhkPlaneShape` still produces none by design (census-counted). OK |
| Compound children compose in authored order; scale | Depth-first, `parent_iso * pose_from_trs(t*scale, r)`; unchanged. OK |
| Scale applied exactly once, at the Rapier sink | `collision_shape_to_parts(.., n.global.scale, ..)` is the only consumer; importer keeps shapes local. The new node-collider producer inserts `coll.shape.clone()` unscaled and carries `compose_trs(.., coll.scale)` in `GlobalTransform` (and the node's propagated scale is the same value `pair_collisions` checks), so no scale². OK |
| Every primitive through `clamp_shape_extent` | Ball/Cuboid/Capsule/Cylinder all clamp after scaling; `character_capsule` routes through it (`queries.rs:840-843`). Hull / TriMesh vertices are finite-checked, not extent-clamped (pre-existing, producer-side REFR scale clamp is the bound). OK |
| u32 surface material end to end | `RigidBodyData.surface_material: u32`; `register_newcomers` packs `u128::from(surface_material)` per part; `collider_surface_material` reads `user_data as u32`; `build_ragdoll` packs nothing (0/STONE, #5557). `resolve_shape_with_materials` returns shape + leaf materials from one walk with a `debug_assert_eq!` on `collision_shape_leaf_count`. `part_surface_materials` is `serde(skip)` and now preserved across the live-load overlay (#5686); `havok_layer` rides the same hook (`registry.rs:240`). OK |
| Havok contact combine rule on both producers | `register_newcomers` (`sync.rs:1165-1166`) and `build_ragdoll` (`ragdoll.rs:454-455`) both set `CONTACT_COEFFICIENT_COMBINE_RULE` on friction and restitution. `ColliderBuilder` appears in no other production code (`spawn_fixed_cuboid*` are test fixtures). OK |
| `sanitize_child_pose` | Present, tested in every profile. OK |
| `default_contact_skin_bu` on every collider kind; degenerate input rejected pre-BVH | `contact_skin` set per part in `register_newcomers`; TriMesh empty / non-finite / out-of-range-index fallbacks unchanged. OK |
| `havok_layer` import | `(body.havok_filter & 0xFF) as u8`, same byte `havok_filter_is_collidable` tests; only `extract_from_classic` builds a `RigidBodyData` (NP / phantom return `None`). Pinned by `the_authored_layer_byte_is_carried_to_the_body`. OK |
| Source-scan cut migration (8bd4f3b8b) | `dispatch_coverage_tests` now use `production_text_or_whole`; `every_dispatched_bhk_shape_has_resolve_arm` and the resolve-scan negative tests still run (183 passed). OK |

### Observations (not findings)
- Rapier user-data packing, `part_surface_materials` ordering for a degenerate hull / mesh fallback (still consumes one `materials.next()` and one importer leaf) and the empty-compound tiny-ball fallback remain aligned.
- The node-collider producer rewrites `Static` to `Keyframed` unconditionally for paired bodies; with the drop above fixed it matches the Havok "fixed bodies driven by animation" intent. Dynamic bodies are left alone.

## PHYSICS audit, Dim 2: Step, Wake & the 4(+1)-Phase Sync Tick

**HEAD** fa7591ca2 · **Baseline** docs/audits/AUDIT_PHYSICS_2026-10-12.md (5d31fd637) · **Status**: CHANGED.

Commits reviewed (scope paths): a2af867d9 (#5698/#5699/#5700), 0d8ea5d6e (#5675), 5f01d1a7f (#5709), d28545728 (#5530), 946310eb9 (#5591), 6dc948ac4 / 9e6c44cea / f6a64fe20 (query + `BodyMotion` additions), d06ac5844 / 251d491ef (naming waves), 9896b1cf7 (clippy), 4e0c962c4 (comment only in `sync.rs`). Full `git diff 5d31fd637..HEAD` of `world/mod.rs`, `world/recovery.rs`, `world/queries.rs`, `sync.rs`, `components.rs`, `water.rs` was read.
Unchanged since 2026-10-12: `byroredux/src/boot/schedule/physics.rs`, `byroredux/src/scheduler_access_tests.rs`, `cell_loader/rapier_release_tests.rs` (test-only touch, #5626), `world/broad_phase_tests.rs`, `commands/physics.rs`, `recovery.rs` clamp / quarantine / escalation ladder (only `accept_keyframe_target` now delegates to `translation_is_sane`, same predicate).

Tests run (targeted, all green): `spawn_resleep_drops_only_settled_newcomers_force_events`, `link_damping_pass_leaves_unjointed_bodies_alone`, `contact_event_position_is_the_contact_not_the_body_origins`, `marker_only_current_pushes_an_awake_damped_ragdoll_link`, `an_early_returning_scan_clears_the_water_force_target_set`, `a_despawned_sleeper_leaves_no_rest_depth_latch`, `translation_is_sane_matches_the_keyframe_bound`, `step_cost_rationale_is_scoped_to_history_and_names_the_real_cost_centre`, `wake_doc_records_the_spawn_exemption...`, plus bin `scheduler_access_tests::{physics_sync_declaration_reads_all_phase_and_diagnostic_types, contact_config_read_is_declared_on_both_physics_systems}`.

### Prior findings closed by a2af867d9 (verified in code)
- PHYS-D2-2026-10-12-01 (#5698): FIXED. `water.rs` clears `water_force_targets` at the top of the scan before the early returns, and the current arm calls `note_water_force_target` (`water.rs` ~1328). Pins: `marker_only_current_pushes_an_awake_damped_ragdoll_link`, `an_early_returning_scan_clears_the_water_force_target_set`.
- PHYS-D2-2026-10-12-02 (#5699): FIXED. `apply_multibody_link_damping` (`world/mod.rs:1033-1078`) walks `multibody_joints.multibodies()` links, cheap body flags before the set lookup; the set is `FxHashSet`.
- PHYS-D2-2026-10-12-03 (#5700): FIXED. `resleep_spawned_newcomers` (`world/mod.rs:968-1002`) uses an `FxHashSet`, and extends it only from bodies actually re-slept (drift test precedes `body.sleep()`).

### Solver invariant verdicts
| Invariant | Verdict | Evidence |
|---|---|---|
| Fixed step / accumulator | VERIFIED | `world/mod.rs:1243-1248`: `self.accumulator += frame_dt.max(0.0)` (NaN-safe), clamp to `MAX_SUBSTEPS * PHYSICS_DT` before the loop; budget bail-out at end of `run_substep` (~1535) zeroes the backlog; the only wall-clock read is `Instant::now()` at 1328; the new Fx sets are only probed (`contains`/`retain`), never iterated into solver order. |
| Wake discipline | VERIFIED, one narrow gap (PHYS-D2-2026-10-16-01) | Fast path (1310-1314) gates on `has_awake_dynamics() / pending_wake`; wake consumed only after `steps > 0` (1357-1366) with the `substep_wake_requested` re-arm; every motion starter calls `wake()` (`apply_impulse` 1102, `set_motion_type` 1156, `add_force(wake_up)` 912, `set_kinematic_translation` sync.rs:108, `set_linear_velocity` sync.rs:74); spawn remains the documented exemption (no `wake()` in `register_newcomers`). Gap: the #5544 re-sleep can swallow a user wake delivered to a still-pending newcomer. |
| Lock order | VERIFIED | `physics_sync_system` (sync.rs:114-169) collects newcomers under read locks, `register_newcomers` takes the `PhysicsWorld` write after the rayon section (guard `register_newcomers_parallel_section_holds_no_world_guard`), `step` under a single `resource_mut`. New exclusive consumers `trap_contact_system` and `projectile_flight_system` take only a `PhysicsWorld` read, release component guards first, and hold no ECS lock inside the physics guard. `place_character_body` (character.rs:921-949) writes `Transform`, `GlobalTransform`, `CharacterController` each in its own scope, then `PhysicsWorld`. |
| Phase order | VERIFIED | sync.rs:131-168: collect/register, push kinematic, buoyancy (2.5), step, pull dynamic. `pull_dynamic` writes `Transform` only. Absent `PhysicsWorld` early-returns (115). Scheduler access declaration unchanged and guards green. |
| Explosion recovery | VERIFIED | Snapshot built from the maintained `dynamic_bodies` index at the top of every `run_substep` (1432-1446) before `pipeline.step`; displacement bound is a delta (recovery.rs:70); quarantine cooldown re-enable happens after the NEXT step and re-arms via `substep_wake_requested` (~1480-1525); `restored > 0` truncates the force-event buffer, zeroes the accumulator, still runs `clamp_explosive_velocities` (~1538-1551). Lifetime offence ladder untouched. |
| Teardown / handle release | VERIFIED (unchanged) | `rapier_release_tests` green path unchanged; `remove_body` pruning untouched. Note: `spawn_sleep_pending` is not pruned by `remove_body`; stale handles are skipped by `bodies.get_mut`, and the list drains on the next stepping frame (bounded, no finding). |
| Contact-force events | VERIFIED | Cleared once at `step` entry (1309), appended per substep, rolled back with the substep; `contact_event_position` resolves the deepest solver contact (guard green), see PHYS-D2-2026-10-16-02 for a fallback-order inconsistency. |
| Kill plane | VERIFIED (unchanged) | `KILL_PLANE_Y = -25000`, scans `islands.active_bodies()` only after `steps > 0`. |

### Checked, no finding
- `apply_multibody_link_damping` after the rewrite: iteration order is arena order (deterministic), liveness checked via `bodies.get`, wet links excluded by the set, unjointed bodies never touched.
- `resleep_spawned_newcomers` event filtering is linear and exact; a drifted newcomer keeps both its wake and its first-impact event.
- `water_force_targets` clear moved to the scan top; `step`'s re-sleep exemption still sees the current frame's targets (buoyancy runs before step).
- `spawn_fixed_cuboid_handles` calls `update_query_pipeline()` but is a test-fixture helper only (callers are `#[cfg(test)]` modules in `trap_hits.rs`/`projectiles.rs`); not a per-frame path.

## Dim 3: Ragdoll Articulation & the Per-Game Constraint Seam

**Status**: CHANGED since baseline (AUDIT_PHYSICS_2026-10-12, HEAD 5d31fd637) by one feature commit; the rest of the dimension is UNCHANGED (guards spot-checked).
**Audited at**: HEAD d099eb68f (tree clean).
**Commits touching Dim 3 paths since 5d31fd637**:
- `18b3d87a7` Pose a Skyrim humanoid corpse from its XRGD before its ragdoll activates (#5015 partial) - `byroredux/src/ragdoll_pose.rs`, `ragdoll_pose_tests.rs`, wiring in `combat.rs::reconcile_dead_actor` and `cell_loader/references/mod.rs`. Reviewed in full.
- `8bd4f3b8b` (#5824 source-scan needle migration) and `696da4f47` (M47.2 ApplyHavokImpulse) - touch `crates/nif/src/import/collision/mod.rs` / tests only; no ragdoll logic change.
- `crates/physics/src/ragdoll.rs`, `byroredux/src/ragdoll.rs`, `crates/nif/src/blocks/collision/constraints.rs`, `crates/nif/src/import/collision/ragdoll.rs`: no diff since baseline.

**Test evidence (this pass)**:
- `cargo test -p byroredux --bin byroredux -- ragdoll_pose ...` + `ragdoll::`: green (ragdoll:: 20 passed, 7 ignored; the 10 non-ignored `ragdoll_pose` tests (11 total, 1 ignored needing Skyrim data) are in the 66-pass character/locomotion/pose run).
- `cargo test -p byroredux-physics -- ragdoll kcc_ capsule_clearance a_ragdoll`: 49 passed.
- FO3 real-data gate, run once: `BYROREDUX_FO3_DATA=".../Fallout 3 goty/Data" cargo test -p byroredux --bin byroredux -- --ignored ragdoll::installed_tests` -> **6 passed, 0 failed** (17.6 s). The #5536/#5538 claim in the Known-open register holds.

### Checklist verdicts

| Item | Verdict |
|---|---|
| XRGD entry-count bound | OK. Decode (`plugin/esm/cell/helpers.rs::decode_ragdoll_pose`) uses `as_chunks::<28>`, so it is bounded by the u16 sub-record length (<= 2340 entries) and drops a trailing partial entry. `bone_poses` requires `body_entries(..).len() == body_count` (18, after dropping one `(0,17,10)` zero-rotation marker), so anything else is refused before any write. |
| Bone-name pin | OK. `is_humanoid_ragdoll` requires exactly 18 template bodies whose bone names equal `HUMANOID_RAGDOLL_BODIES` in order (case-insensitive; an unnamed body fails). This also protects against `template_from_imported` silently dropping an unresolved bone (which would shift every index): the count/name pin refuses it. Tests: `the_humanoid_body_set_is_matched_by_name_in_order`, `a_non_humanoid_ragdoll_is_not_posed`. |
| Finite / sanity of decoded rotations | OK. Position and rotation of every entry are `is_finite`-checked; rotations go through `euler_zup_to_quat_yup_mode(0, ..)` (a unit-quaternion product, periodic, so a huge finite angle cannot produce a non-unit/non-finite result). Positions are never applied (only the COM's local translation is zeroed), so the only translation written is `Vec3::ZERO`. |
| Interaction with `seed_pose_is_sane` | OK. The pose is rotation-only plus a zeroed COM translation on NIF-length bones, so a posed seed cannot exceed `SEED_SANE_MAX_ROOT_OFFSET_BU` / `SEED_SANE_ABS_BOUND_BU`; the validator still runs on the lying pose inside `activate_ragdoll` (it reads the recomputed `GlobalTransform`s). Even a refusal there leaves a static lying pose, not an explosion. |
| Save / refusal behaviour | OK. `AuthoredRagdollPose` is derived from plugin data at every spawn, is listed (with rationale) in `registry_completeness_tests.rs`, and is consumed once (`remove_component_taking` before `pose_skeleton`). All refusal reasons (no template, non-humanoid, count mismatch, non-finite) return before the first write; the only mid-loop `Err` ("ragdoll bone has no Transform") is unreachable for name-resolved bones. A refused/failed pose falls back to today's collapse. The early returns in `reconcile_dead_actor` ("no ragdoll target", "ragdoll already active") leave the component parked, which is harmless. |
| Lock / hierarchy discipline of `pose_skeleton` | OK. Template read guard dropped inside the `and_then` closure; the Transform write guard is scoped before `recompute_globals`; `recompute_globals` takes Transform(R) -> Parent(R) -> GlobalTransform(W), the same order as `ragdoll_writeback_system`. `collect_subtree` is a pre-order DFS, so parents are composed before children. The `GlobalTransform::get_mut` dirty-marking (TRACK_CHANGES) is once per subtree node and the values do change, so it is correct, not the #4062 spurious-dirty pattern. |
| Constraint coverage vs the doc table | Unchanged since baseline; `constraints.rs` / `import/collision/ragdoll.rs` have no diff. |
| `RAGDOLL_EXTRA_SOLVER_ITERATIONS == 0` | OK (`crates/physics/src/ragdoll.rs:603`); `a_ragdoll_does_not_change_an_unrelated_bodys_solve` and `ragdoll_solver_budget_preserves_the_world_default_mass_and_limits` pass. |
| Havok limit convention | OK. `hinge_flexion_seed_holds_and_hyperextension_is_corrected` passes and the FO3 articulation gate is green. |
| Seed validation | OK at both sites (`seed_pose_is_sane` in `activate_ragdoll` and the `build_ragdoll` absolute backstop); `activation_refuses_a_seed_beyond_actor_reach` / `..._non_finite_seed` pass. |
| Bone->body index mapping / activation strips recipes | Unchanged; covered by the passing `ragdoll::tests`. |

### PHYSAL doctrine (grep verdict)

`grep -E "GameKind|bsver|BSVER|NifVariant|NifVersion|version|user_version"` over `crates/physics/src/**` and `byroredux/src/ragdoll.rs` / `ragdoll_pose.rs` (code, not comments): **zero hits** (the only text match is `conversion` in `sync.rs`). Per-game logic in `crates/nif/src/import/collision/`: only the parse-side `scene.bsver < SKYRIM_LE` surface-material rule (`shape.rs:93`, a parse/import boundary item, documented in the skill) and `havok_scale_for` (`crates/nif/src/lib.rs:92`, an exhaustive `NifVariant` match: Morrowind/Oblivion/FO3/FNV 7.0, Skyrim+/FO4/FO76/Starfield 69.99125). So the three named seams (constraint CInfo decode, `havok_scale_for`, collision-object-kind dispatch) are still the only version/`GameKind` branches, and the solver side is clean.

One new thing the doctrine does not name: `ragdoll_pose.rs` carries a Skyrim-skeleton-specific table (`HUMANOID_RAGDOLL_BODIES`) and an empirical XRGD layout. It is keyed on skeleton identity (bone names), not on `GameKind`/version, is scoped to the engine-side corpse reconciliation (not the solver), and is refused for every other rig. I judge it a documented data-interpretation, not a seam violation, but `physal.md` should record it (see PHYS-D3-2026-10-16-02).

### Carry-overs verified unchanged (no re-filing)
- #5015 (open): XRGD for FO4 and creature rigs; the Skyrim humanoid half is the partial fix reviewed above.
- #4772 (open): FO3 copied-save restore first-solve jump; recovery nets still mask it, code unchanged.
- #5697 (open): multibody-link angular damping stand-in; `crates/physics/src/ragdoll.rs` has no diff since the report that raised it.

## Dim 4: Character Controller, Grounding & the NPC KCC (M42.10)

**Status**: CHANGED since baseline (AUDIT_PHYSICS_2026-10-12, HEAD 5d31fd637). Audited at HEAD d099eb68f (tree clean).
**Commits reviewed** (touching `byroredux/src/systems/**`, `player_body.rs`, `crates/physics/src/world/queries.rs`, `components.rs`):
- `946310eb9` Fix #5591 `place_character_body` / `CharacterController::clear_momentum` (character.rs, commands/view.rs, save_io, physics components) - reviewed in full.
- `ed59c81f7` M47.2 encumbrance in `character_controller_system` - reviewed.
- `d28545728` Fix #5530 corrupt saved pose refused (`save_io/player.rs::apply_player_pose`) - reviewed.
- `746ba8dfd` actor walking command + `graph_locomotion.rs` (`ConsoleWalk`, `console_walk_system`, `graph_locomotion_system`, later `graph_root_motion_system`) - reviewed in full. **It does not touch the KCC**: it is a Transform-writing stand-in mover plus an observer that reads displacement; there is no `move_character` call, so the NPC-step rules (group mask, own-vertical, derived constants) are not applicable to it. It is a different class of mover, covered by finding PHYS-D4-2026-10-16-02 below.
- `f6a64fe20` projectiles (`systems/projectiles.rs`, `cast_ray_corridor_where`, queries.rs) - reviewed: thin ray vs solids, wider corridor vs actor bodies, launcher bodies excluded via a body predicate, `max_distance`/direction guarded (`normalize_or_zero`), sensors excluded. No finding.
- `locomotion.rs` delta is only `AI_PACKAGE_DEFAULT_RADIUS` (#5587) + its source-shape test; `forcegreet.rs`/`eat_sleep.rs`/`navmesh_path.rs` deltas are non-physics (`cached_walk_step` gained only a clippy allow).
- `queries.rs` delta: `FOOTSTEP_SURFACE_PROBE_REACH`, `cast_ray_corridor_where`, `bodies_overlapping_body`, `body_motion` (script-facing queries; correct, `exclude_rigid_body` + `exclude_sensors`).

**Test evidence (this pass)**: `cargo test -p byroredux --bin byroredux -- ragdoll_pose systems::character systems::locomotion place_character momentum_is_cleared npc_locomotion kcc_` -> 66 passed, 0 failed, 1 ignored; `player_body` / `attach_reparents` -> 12 passed; `cargo test -p byroredux-physics -- kcc_ capsule_clearance ragdoll a_ragdoll` -> 49 passed. `kcc_filter_groups_mask_actor_bone_colliders` is non-vacuous (asserts the unmasked sweep is blocked, `blocked > -30`, and the masked one passes through).

### Checklist verdicts

#### NPC KCC rules (`systems/locomotion.rs`) - all hold
- Group mask: `move_character` is called with `filter_groups: Some(actor_move_interaction_groups())`; `actor_move_interaction_groups()` is literally `ground_probe_groups()` (`world/mod.rs:185-187`), which masks `ACTOR_BONE_GROUP`. Both NPC `cast_ray_down` calls pass `None` for the exclude collider, correct because the solid-probe filter masks the actor bones.
- `LOCOMOTION_NPC_SNAP_TO_GROUND == 0.0` (`locomotion.rs:115`) and `move_character` maps `snap_to_ground <= 0` to `None`.
- Step owns its vertical: pre-move `cast_ray_down(.., lift + STEP_HEIGHT + kcc_offset, None)` and `correction.clamp(-STEP_HEIGHT, kcc_offset)`; airborne fallback ray origin-lifted by `LOCOMOTION_GROUND_RAY_UP_OFFSET` (256) and capped at `LOCOMOTION_MAX_DROP` (256).
- `dt = 0` boot tick: `a_zero_dt_boot_step_matches_a_normal_first_step_for_embedded_npcs` passes. (Residual: a request is exactly zero only if the correction is exactly 0 and no horizontal move; a measure-zero case that merely triggers rapier's benign depenetration.)
- Derived constants: `LOCOMOTION_NPC_{CAPSULE_HALF_HEIGHT,CAPSULE_RADIUS,STEP_HEIGHT,STEP_MIN_WIDTH,MAX_SLOPE_DEG,KCC_OFFSET_BU}` still read `npc_spawn::FALLBACK_ACTOR_CAPSULE_*`, `CharacterController::HUMAN.*`, `ContactConfig::DEFAULT.kcc_offset_bu`; `npc_locomotion_constants_match_their_sources` passes.
- `blocked` flag (#5452) consumed via `step_along_waypoints_detailed` / `cached_walk_step`.
- `forcegreet_system` and `eat_sleep_system` both step through `navmesh_path::cached_walk_step` -> `step_along_waypoints_detailed`; both write `Transform` (never `GlobalTransform`, #5373), with `PhysicsWorld` scoped inside the helper only (#5371). Unchanged in substance by the delta.

#### Player rules (`systems/character.rs`) - all hold
- `resolve_ground_contact` ORs `kcc_grounded || probe_found_support` (swim short-circuits to false); the probe half is gated on the walkable normal (`min_walkable_normal_y`). `no_support_probe_and_no_kcc_contact_stays_airborne`, `the_ground_probe_screens_its_grounded_half_on_the_walkable_normal`, `stationary_capsule_stays_grounded_instead_of_flickering` pass.
- `integrate_vertical`: accumulation then terminal clamp (`v < terminal -> terminal`), jump replaces velocity; tests pass.
- `MAX_FRAME_DT = 1/30` applied right after the `dt <= 0` early return (NaN dt is laundered by `f32::min`).
- Diagonal input normalised in `horizontal_motion` (`move_dir.normalize()` before the speed multiply); test `horizontal_motion_diagonal_does_not_exceed_speed`.
- `player_accepts_movement_input` honours `PlayerControlState.movement_enabled && !ai_driven` and `ActorControlState.restrained`. The `unconscious` flag is not consulted, but that is correct: `PlayerRef.SetUnconscious` is filtered out of the player's `ActorControlState` (`fragment/effects/resolve.rs`, #5298) and the player never enters `Bleedout` (`spare_from_death` returns false for the player).
- Player passes no actor-bone mask (`filter_groups: None`, #5559 comment intact) and excludes its own collider; `kcc_offset` read from `ContactConfig` with the DEFAULT fallback.
- Camera boom single source: `THIRD_PERSON_BOOM_BU` is read only through `player_camera_boom`; `camera_follow_system` and `interaction::camera_ray` use it; `third_person_gameplay_ray_starts_at_the_eye_not_the_camera` passes. Remaining direct casts (`app_step.rs` bench-subject probe, `render/mod.rs` fog-height ray with the player body excluded, `scene/character_spawn.rs` pre-spawn probe) are not player gameplay rays.
- Third-person body: `attach_player_body` collision stripping guard `attach_reparents_offsets_retargets_and_strips_collision` passes; the `player_body.rs` delta is a `collect_subtree` reuse (#5713).

#### #5591 teleport funnel
All four named sites route through `systems::place_character_body`: camera snap (`character.rs:904`), transition arrival grounding (`character.rs:1060`), save restore (`save_io/player.rs:249`), `combat.approach` (`commands/view.rs:461`). `cam.pos`/`cam.tp` reach it via `carry_player_body_to_camera` -> `snap_character_body_to_camera`. No hand-written `wants_jump = false` / `vertical_velocity = 0.0` / `is_grounded = false` outside `clear_momentum`; `momentum_is_cleared_through_one_method` scans `production_text_or_whole` of the three files (non-vacuous: `place_character_body_moves_both_transforms_and_clears_momentum` also passes). Each store is in its own lock scope (Transform, GlobalTransform, CharacterController), no nesting.
Observation (not filed): `cinematic.rs:1296-1311` (MQ101 vehicle attachment, `ActorCinematicState`) and `cinematic.rs:1158` (scene-trigger approach) move the player body each frame by writing `Transform` + `set_kinematic_translation` by hand, without `clear_momentum` or a `GlobalTransform` write. It is a per-frame carry that predates #5591 (introduced 0ff8612bf, 2026-08-01) and is not on the skill's teleport list; the controller runs in Early and the attachment in Update, and I could not establish a concrete failure without a live run (a body with no support under it while attached would accumulate terminal `vertical_velocity` until release). Left as a watch item for `/audit-runtime`, not a finding.

#### Encumbrance (`ed59c81f7`)
`(encumbrance_speed, sprint_allowed)` is read once per tick from `PlayerEncumbrance::movement()`; sprint is gated, `speed_mul` multiplied into both the swim and walk motion. Logic is correct for sane inputs; see PHYS-D4-2026-10-16-01 for the unvalidated GMST value.

### Not filed (disproved or out of scope)
- "Unconscious player can still walk" - disproved (SetUnconscious never reaches the player's `ActorControlState`, #5298; player cannot bleed out).
- Projectile flight cost: `projectile_flight_system` rebuilds `owners` / `actor_bodies` over every `RapierHandles` entity each tick while any projectile is in flight (O(bodies) with 2-3 sparse lookups each). Only active during flights; belongs to `/audit-performance` if ever measured.
- Cinematic vehicle attachment pose writes - see the observation under "#5591 teleport funnel".
- `graph_root_motion_system` moves graph-driven actors by extracted root motion without a KCC sweep; this is by design (seat entry / turn-on-spot transitions are animation-authored) and is documented in its header.

## PHYSICS AUDIT — Dim 5: WATAL Physics Sink (Buoyancy, Damping, Current, Swim)

**HEAD**: `fa7591ca2` · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-12.md` (HEAD `5d31fd637`)
**Status**: CHANGED. Commits reviewed (water/sink scope): `a2af867d9` (#5698/#5699/#5700), `5f01d1a7f` (#5709), `0d8ea5d6e` (#5675, contact position; Dim 6 owner, read for water interaction only), `946310eb9` (#5591 `place_character_body`), `8bd4f3b8b` (source-scan cut migration in `water.rs`), `275fc103f` / `001de8741` / `696da4f47` / M47.2 batch (character.rs encumbrance, `water_damage_system` `spare_from_death`, `late.rs` access lists). Delta read in full: `git diff 5d31fd637..HEAD -- crates/physics/src/water.rs crates/physics/src/world/mod.rs byroredux/src/systems/{character,water}.rs byroredux/src/boot/schedule/late.rs`.

**Lanes (orchestrator log `/tmp/audit/physics/tests.log`)**: physics crate 253 passed; bin filters `character` 63, `water` 166 (+4 ignored), `scheduler_access` 29, `submersion` 5: all green.

**Probes**: a throwaway crate in the session scratchpad (path-deps on `byroredux-physics`, built into the repo `target/`, binary removed afterwards; no repo file edited) measured both findings below.

### Checklist verdicts

| Item | Verdict |
|---|---|
| `submerged_fraction` clamp, zero-height AABB, lift sign/units | Unchanged since baseline; guards green. OK |
| Lift re-derived every substep (#5547), heave damping (#5618) | Unchanged. OK |
| **FxHashMap latches** (`rest_depth`, `water_force_targets`) + **scan-generation stamp** (#5709) | Verified. Every arm of the per-body loop either removes the latch (`!prior_wet`, awake, drifted, dry, exit) or refreshes the stamp (asleep + wet); the one `retain(stamp == generation)` after the loop sweeps only bodies the loop did not touch (despawned / non-dynamic / missing collider). `EntityId` is monotonic (`World::spawn` never recycles), so a stale latch cannot be inherited by a new entity. `u32` wrap needs 2^32 scans. Guards `a_despawned_sleeper_leaves_no_rest_depth_latch` and `water_scan_latches_are_not_siphash` are non-vacuous (`production_text`; the former forces a full scan via a newcomer, asserts the latch survives 5 asleep scans and is swept after despawn). OK |
| **`water_force_targets` handling** (#5698) | Clear moved above every early return: correct, and the move above the quiesced-scene return is safe — `step`'s own fast path (`!has_awake_dynamics && !pending_wake`) is the same predicate as the scan's, and a wake armed by the scan keeps `pending_wake` set across sub-tick frames, so the next scan is full and refills the set before `resleep_spawned_newcomers` consults it. Current arm now notes the body (`water.rs:1334`); `marker_only_current_pushes_an_awake_damped_ragdoll_link` and `an_early_returning_scan_clears_the_water_force_target_set` are non-vacuous (probe-equivalent reasoning: pre-fix dry pass `reset_forces` wipes the current). Two residual gaps found below (D5-01 stale force on marker exit, D5-02 damping lost while noted) |
| Link damping pass walks links, not arena (#5699) | `multibodies().links()` is the same set as the old `rigid_body_link(h).is_some()` filter. OK (stale comment, D5-03) |
| Spawn re-sleep (#5700) | Now a `FxHashSet`; settle crossings dropped only for bodies actually re-slept. Test pins it. OK |
| Wake discipline (`n_new > 0` escape, `pending_wake` one-shot) | Unchanged. OK |
| Two target sources + matching `Ragdoll` arm in `clear_stale_water_contacts` | Unchanged and present. OK |
| Current drag bounded; one `reference_point` | Unchanged (AABB centre on all axes for prefilter, marker, surface). OK |
| `WaterContact` zero-transition frame | Unchanged. OK |
| **Two samplers must agree** (`apply_buoyancy_with_scratch` vs `player_water_state` / `player_current_drift`) | Re-read both against current code. Plane flow: dynamic `flow: s.flow` x fraction, player `state.flow` x fraction; marker: dynamic `current_force(.., 1.0)`, player `drift(marker, 1.0)` added in both swim and wade arms; both go through `weather_wave_adjustment` + `WaterKind::weather_scroll_for` + `authored_wave_height_with_weather`; both use `WaterVolume::surface_y_at` with the surface mesh; hazard damage only in Late `water_damage_system`; missing `WaterCurrentVolume` storage tolerated on the player side (`placed_current_flow_at` returns `?`). No water input was added to either side in this window (core water components and the sampling code are untouched). Only intentional asymmetries remain (no wind force on the player, hysteresis band on dynamic bodies only; the player skips `fraction <= 0` columns). OK |
| Swim/drown: dt-correct damping, zero-dt breath, single death reconcile | Unchanged code; `place_character_body` (#5591) clears momentum through `CharacterController::clear_momentum` (same three fields) and does not touch swim state, which is re-derived every tick. M47.2 encumbrance scales `speed_mul`, which feeds `swim_motion` too (design call, not a sampler input). `water_damage_system` now routes a killed actor through `spare_from_death` (the player is never spared, so drowning/hazard death of the player is unchanged; essential NPCs would bleed out). OK |
| Late adapters order (submersion -> damage -> reconcile -> interaction -> audio) | `late.rs` diff only adds access declarations and new exclusives; `submersion_runs_after_camera_follow_and_before_water_audio` green. OK |

### Observations (not findings)
- `water_damage_system`'s `Access` (late.rs:207) lists the `spare_from_death` reads but not the `PapyrusPlayerEntity` resource it reads nor the `StoryEvent` storage `raise_kill_story_event` writes. It is an exclusive system, so no parallel-batch conflict arises; `/audit-concurrency` owns the declaration-completeness question.
- Unfixed-by-design hysteresis effect: for a link in the 4 BU `WATERLINE_HYSTERESIS` band above a surface (`surface = Some`, `frac == 0`) the surface arm registers the body but applies nothing; same ownership gap as D5-02 but for at most a few frames, so folded into D5-02's fix rather than filed separately.

## PHYSICS audit, Dim 6: Queries, Diagnostics & Cost

**HEAD** fa7591ca2 · **Baseline** docs/audits/AUDIT_PHYSICS_2026-10-12.md (5d31fd637) · **Status**: CHANGED in `world/queries.rs` and its new consumers; `sync.rs` (census) and `commands/physics.rs` unchanged since 2026-10-12 (sync.rs: one comment edit, 4e0c962c4); guards spot-checked green.

Commits reviewed: 6dc948ac4 (`bodies_overlapping_body`, `body_motion`, `BodyMotion`), f6a64fe20 (`cast_ray_corridor_where`), d06ac5844 (`FOOTSTEP_SURFACE_PROBE_REACH`, #5743), 0d8ea5d6e, 4e0c962c4. Consumers read: `byroredux/src/systems/{trap_hits,projectiles,audio}.rs`.

Tests run green: `census_excluded_body_prevents_a_self_hit_on_the_players_own_capsule`, `census_reports_full_extent_and_scale_not_just_the_y_range`, `corridor_cast_hits_a_bone_the_zero_width_ray_threads`, `surface_material_under_skips_the_walkers_own_capsule`, `step_cost_rationale_is_scoped_to_history_and_names_the_real_cost_centre`.

### Invariant verdicts
| Invariant | Verdict | Evidence |
|---|---|---|
| Caster excluded from every cast | VERIFIED | `footstep_system` passes the walker's body to `surface_material_under` (`systems/audio.rs:281`, `FOOTSTEP_SURFACE_PROBE_REACH` = 128 > 1.5x the 64 BU capsule centre height, pinned by `footstep_probe_reach_clears_the_player_capsule_centre`); live `phys.census` resolves `excluded_body` from the player's `RapierHandles` (`commands/physics.rs:120-157`); the only `None` census caller is the boot-time all-rungs-missed path where no capsule exists yet (`scene/character_spawn.rs:598`, #3965, unchanged). `cast_ray_corridor_where` is a predicate cast: projectiles pass `!excluded.contains(body)` for both the thin solid sweep and the wide actor corridor (`projectiles.rs:234-243`), so the launcher exclusion the skill requires is present. |
| Census distinguishes authored / dropped / not walkable, full extent + scale | VERIFIED (unchanged) | guards green. |
| `colliders_near_xz` not per-frame | VERIFIED | callers are the census report (`sync.rs:689`) and tests only. |
| Cost model (no separate query tree, queued leaves only, `update_query_pipeline` not per frame) | VERIFIED | `step_cost_rationale...` green; `update_query_pipeline` is only called from `register_newcomers_and_refresh_queries` (cold-start bootstrap), `spawn_fixed_cuboid_handles` (test fixture) and tests. |
| `phys.stats` scope-labelled counters | VERIFIED (unchanged) | `commands/physics.rs` untouched since baseline. |
| Query surface lock discipline | VERIFIED | `trap_contact_system` and `projectile_flight_system` resolve component maps before taking the `PhysicsWorld` read and release it before touching ECS storages (comments cite the melee sweep's lock order). |

### Checked, no finding
- `bodies_overlapping_body` / `body_motion` correctness: sensors and the body itself excluded, motion-type mapping Dynamic 1 / Keyframed 4 / Fixed 5; the Update-stage caller reads a BVH current as of the previous Physics stage (one-frame lag for moving traps, by design).
- `cast_ray_corridor_where` with `stop_at_penetration: false` deliberately ignores a start-inside overlap, matching the wall-mounted-launcher rationale in f6a64fe20.
- `surface_material_under` still takes `Option<RigidBodyHandle>` (not a mandatory type), as at baseline; its one production caller passes the walker's body.

