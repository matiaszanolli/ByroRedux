**HEAD**: 9fcfdc3fc · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-09-21.md` (HEAD `f97775ca8`) · **Audited**: Dim 1 Shape Translation, Dim 2 Step & Sync, Dim 3 Ragdoll & Constraint Seam, Dim 4 Character & NPC Controller, Dim 5 Water / Buoyancy, Dim 6 Queries & Diagnostics · **Unchanged since baseline (skimmed)**: none at dimension level (every dimension had commits). Dim 1 (two commits: one test, one NIF-side guard) and Dim 3 (index push + ECS-owned walk guards) were light deltas, spot-checked rather than re-traced.

# PHYSAL / Physics Audit — 2026-09-29

**Run**: `/audit-physics` (default scope, deep), one leg of `/audit-suite --preset comprehensive`. A single
auditor analysed all six dimensions in sequence, with no sub-agents (per the suite constraint). The per-dimension
scratch notes are in `/tmp/audit/physics/dim_{1..6}.md`. They are kept for the orchestrator, so the skill's
Phase 3 `rm -rf` was deliberately skipped.

**Delta audited**: `f97775ca8..9fcfdc3fc`, about 30 commits in physics scope. The load-bearing ones:

- **Fixes for the 2026-09-21 findings**:
  - `fa6235775` #4682: the recovery snapshot reads a dynamic-body index.
  - `15ad2455c` #4685: incremental query pipeline in the step.
  - `6ca10bf4b` #4687: the three recovery edges.
  - `fb8fae288` #4683: recovery counter.
  - `004e9befb` #4684: census scope.
  - `45a0b8b6b` #4686: debug-lane clamp pin.
  - `e9ae939a5` / `3f7f361a6` / `0e607cbac` / `7e942371f`: #4689 / #4690 / #4691 / #4688.
- **Performance work**:
  - `fe80f4d76` `FixedPairFilterBroadPhase`.
  - `ad1d53a11`: newcomer shape conversion runs on rayon.
  - `078f650ec`: read-only `get` before `get_mut`.
  - `32f774c01` #4614: stack capsule.
- **Water**: `17c01a4e5` `WaterSurfaceMesh` / `surface_y_at`; `0963d675d` #4791.
- **The P3 player-body series**: `a070baaad` (body attach + third-person boom), `db8351587` (walk/idle),
  `0182fc5e8` (mid-life gear import).
- **Guards**: `6d05c2bc0` (#4997 rayon-section guard, #4995).

**Tests** (all via `cargo test`, `-j 4`, `TMPDIR=/mnt/data/tmp`):

| Lane | Result |
|---|---|
| `cargo test -p byroredux-physics` | **192 passed**, 0 failed, 0 ignored (baseline 175) |
| `cargo test -p byroredux --bin byroredux -- ragdoll` | 27 passed, **6 ignored**. `ragdoll_installed_tests.rs` needs FO3 data and was not run (RAM budget). |
| `… -- character` / `-- locomotion` / `-- water` | 56 / 5 / 136 passed (water: 3 ignored) |
| `… -- scheduler_access` / `-- rapier_release` / `-- player_body` | 23 / 9 / 12 passed |
| `cargo test -p byroredux-nif --lib -- collision` | 155 passed |

**Scratch probe** for PHYS-D2-2026-09-29-01: a standalone crate in the session scratchpad, depending on
`byroredux-physics` by path, target dir `/mnt/data/tmp/physaudit-target`. It drives a real
`PhysicsWorld::step` recovery and then casts rays. The engine was not launched. The only repo file written is
this report.

---

## Executive Summary

| Severity | NEW | Regression | Existing (matched, open) |
|---|---:|---:|---:|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | **1** | 0 | 0 |
| LOW | **3** | **1** (#4687) | 2 (#4134, #4772) |

### The result that matters

**All ten 2026-09-21 findings were fixed in code, and nine of those fixes hold.** The exception is #4687 part (b),
the same-frame query refresh after an explosion restore. It is a no-op, and its guard is vacuous
(PHYS-D2-2026-09-29-01):

- The fix calls `update_incremental(…, false)`. In rapier 0.22 that only marks the QBVH leaf dirty and does not
  refit it.
- The step itself had just refit the leaf to the EXPLODED pose.
- Measured with a real recovery: the ray through the restored body hits the floor below it for the rest of the
  frame, and the body becomes visible again on the next frame.
- The guard fakes the "exploded" indexing with `false` as well, so the tree never held the exploded AABB.

The impact is one frame after a rare event, so this is LOW. It matters because the issue is closed, and the skill
states the invariant as true.

**The new P3 third-person view breaks every camera-origin gameplay ray** (PHYS-D4-2026-09-29-01, MEDIUM). The
180 BU boom moves the origin of the interaction, melee and occlusion casts behind the player:

- Melee reach (`MELEE_REACH_BU = 180`) now ends at the player's own head.
- Activation keeps 12 BU of its 192 BU reach.
- With a wall behind the player, the unswept boom starts the ray behind or inside that wall.

The p3 smoke gate checks "no self-targeting", not "can still reach a target", so it stays green.

The remaining LOWs:

- #4691's parity fix left the marker-only current case asymmetric (D5-29-01).
- The recovery counter mixes events and bodies (D2-29-02).
- A doc comment was split by the #4997 insert (D2-29-03).

### PHYSAL doctrine verdict — **HOLDS, eighth consecutive pass**

```
$ grep -nE "GameKind|game_kind|bsver|NifVersion|is_skyrim|is_fo4|is_oblivion|is_fo3|is_fnv|game ==|BS_F76|SF_FORM_ID|havok_scale|GameVariant" \
    crates/physics/src/*.rs byroredux/src/ragdoll.rs byroredux/src/systems/{character,locomotion,water}.rs byroredux/src/commands/physics.rs
(no matches)
```

The three named seams stay at the parse→canonical boundary: constraint CInfo decode, `havok_scale_for`, and
collision-object-kind dispatch. The widened grep has one new hit, `byroredux/src/player_body.rs:406`. It uses
`GameKind::Fallout3NV` to read the child-race flag for the walk-clip choice. That is animation-clip selection, not
the solver side, so it is outside the doctrine.

### Games traced

No game's collision data was traced end-to-end. There was no engine launch, and the FO3 data tests were skipped
for RAM. The real-data evidence comes from project records instead:

- `0182fc5e8` was verified live on Skyrim SE: gear import, player visual-only.
- `fe80f4d76` records FO4 Commonwealth 0,0 r1 numbers: 15,941 fixed bodies.
- #4772 is the FO3 restore record.

The solver side is game-agnostic by construction (doctrine grep above).

### Per-dimension counts

| Dimension | CRIT | HIGH | MED | LOW | Notes |
|---|---:|---:|---:|---:|---|
| 1 — Shape Translation | 0 | 0 | 0 | 0 | #4686 fixed; #4134 now one site (Existing) |
| 2 — Step & Sync | 0 | 0 | 0 | 3 | one is Regression of #4687; #4682/#4683/#4685 confirmed; broad phase + rayon sound |
| 3 — Ragdoll & Constraint Seam | 0 | 0 | 0 | 0 | doctrine holds; #4772 Existing |
| 4 — Character & NPC Controller | 0 | 0 | 1 | 0 | #4689/#4690/#4614 confirmed; player body visual-only confirmed |
| 5 — Water / Buoyancy | 0 | 0 | 0 | 1 | #4691/#4791 confirmed; surface_y_at in all three samplers |
| 6 — Queries & Diagnostics | 0 | 0 | 0 | 0 | #4684 confirmed |

---

## Solver Invariant Matrix

| Invariant | Verdict | Note |
|---|---|---|
| Fixed step: accumulator clamped before the loop; `frame_dt.max(0.0)` | **HOLDS** | `world.rs:773-777` |
| Anti-spiral wall-clock budget (#1698) | **HOLDS** | checked after each substep (`:940`) |
| Static-scene fast path (`active_dynamic_bodies` empty && `!pending_wake`) | **HOLDS** | `:839-846`. Spawn stays wake-exempt (#3969). `recover_pre_broken_bodies` runs before the gate. |
| Wake survives a zeroed accumulator | **HOLDS** | the recovery branch counts its substep before `break` (`:931`) |
| Collider-set mutation reaches the query pipeline | **HOLDS** | step: incremental + final refit (rapier `physics_pipeline.rs:632-639`); stepless dirty frame: full rebuild (`:997-1002`) |
| Query BVH cost (#4685) | **FIXED** | `Some(&mut query_pipeline)` into `pipeline.step` (`:910`) |
| Explosion recovery | **HOLDS, one edge drifted** | Snapshot precedes every substep from the dynamic index (#4682). Delta bound holds. Pre-broken bodies are parked (#4687a). All articulations are detached (#4687c). Counted (#4683; units: D2-29-02). **Same-frame query refresh is a no-op (D2-29-01, #4687b).** |
| Fixed-on-fixed broad-phase filter (fe80f4d76) | **HOLDS** | the only production `set_body_type` is `set_motion_type` → `body_left_fixed` (`:623-629`) |
| Registration determinism (ad1d53a11) | **HOLDS** | parallel conversion, serial insert in newcomer order; no World guard in the rayon section (#4997 guard) |
| Lock ordering (Phase 1 read → release → write) | **HOLDS** | unchanged; `/audit-concurrency` owns the rule |
| Phase order collect → push kinematic → buoyancy → step → pull | **HOLDS** | `sync.rs:131-168` |
| `physics_sync_system` Access declares every acquisition | **HOLDS** | `WaterSurfaceMesh` declared; `scheduler_access` 23/23 |
| Scale applied exactly once, at the sink | **HOLDS** | `convert.rs` unchanged apart from the test |
| Teardown completeness | **HOLDS** | `rapier_release` 9 green; `dynamic_bodies` tolerates stale handles (generational) |
| Player physics presence = one capsule | **HOLDS** | body attach and gear import strip `CollisionShape`/`RigidBodyData` (`player_body.rs:276-295`, `loot_appearance.rs:628-636`) |
| The two water samplers agree | **DRIFTED (narrowed)** | co-located case fixed (#4691); marker-only case diverges (D5-29-01) |
| Diagnostics do not lie | **HOLDS, minor** | #4684 fixed; counter units mixed (D2-29-02) |
| PHYSAL doctrine (no solver-side game branch) | **HOLDS** | eighth consecutive pass |

---

## Findings — MEDIUM

### PHYS-D4-2026-09-29-01: The third-person boom moves every camera-ray gameplay cast 180 BU behind the head — melee reach ends at the player's own head, activation keeps 12 BU of 192
- **Severity**: MEDIUM
- **Dimension**: Character & NPC Controller. The consumers are owned by `/audit-gameplay` (interaction/combat).
- **Location**:
  - `byroredux/src/systems/character.rs:610-613, 699-712`
  - `byroredux/src/interaction.rs:33, 871, 1055, 1117, 1156-1168`
  - `byroredux/src/combat.rs:27, 215, 495-500`
- **Status**: NEW
- **Trigger Conditions**: Character mode, `player.view third` / V key, then attack or activate anything. Worse with
  a wall behind the player.
- **Description**: `a070baaad` added `THIRD_PERSON_BOOM_BU = 180.0`. In third person, `camera_follow_system` sets
  `cam_pos = head_pos - forward * 180`. Every player gameplay ray takes its origin from the camera
  (`interaction::camera_ray` reads the `ActiveCamera` Transform):
  - activation target selection, with `INTERACTION_REACH_BU = 192`;
  - the interaction occlusion casts;
  - the player swing (`combat.rs:215`), with `attack_reach_bu` = `MELEE_REACH_BU` (180) × weapon reach;
  - the studio host and `commands/view.rs` rays.

  None of them compensates for the boom. The camera-collision probe is documented as missing, so the ray origin
  can also sit behind or inside the wall at the player's back.
- **Evidence**:
  ```rust
  // character.rs:708-711
  crate::player_body::PlayerCameraView::ThirdPerson => {
      head_pos - (cam_rot * -Vec3::Z) * THIRD_PERSON_BOOM_BU
  }
  // interaction.rs:1156 — pub(crate) fn camera_ray(world) -> (camera Transform.translation, forward)
  // combat.rs:27   — pub(crate) const MELEE_REACH_BU: f32 = 180.0;
  // interaction.rs:33 — pub(crate) const INTERACTION_REACH_BU: f32 = 192.0;
  ```
- **Impact**: In third person:
  - A reach-1.0 or unarmed swing's ray stops at the player's head and hits nothing in front of the player.
  - Activation reaches only 12 BU past the head, so doors, containers, NPCs, loot and dialogue are effectively out
    of reach.
  - With a wall behind the player, the first solid the ray meets is that wall. A convex collider gives toi = 0,
    because `solid = true`; a TriMesh gives its back face. Either way the wall occludes every target.

  Only the player capsule is excluded. Switching back to first person restores behaviour, which is the
  workaround. The `p3-player-body.sh` gate asserts "no self-targeting in `interaction.status`" and so cannot see
  lost reach.
- **Related**: CONC-D4-2026-09-28-02 / #4995 (same boom, body-yaw lag, closed). The slice doc's "no wall collision
  yet" follow-up.
- **Suggested Fix**: in third person, cast gameplay rays from the eye (the capsule position + `eye_height`) along
  the camera forward. Alternatively, advance the origin by the boom length and keep the camera pose for rendering
  only. Extend the p3 gate to activate a target about 100 BU away in third person.

---

## Findings — LOW

### PHYS-D2-2026-09-29-01: #4687(b)'s same-frame query refresh is a no-op — a restored body is invisible to queries for the rest of the frame, and the guard is vacuous
- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**:
  - `crates/physics/src/world.rs:679-690` (`refresh_query_geometry_after_restore`)
  - `world.rs:~2472-2528` (the guard `restored_pose_is_visible_to_ray_queries_same_frame`)
- **Status**: Regression of #4687. Part (b) never took effect.
- **Trigger Conditions**: any per-substep explosion recovery (`restored > 0`). The FO3 restored-corpse path (#4772)
  hits it on every restore.
- **Description**: the fix propagates the restored poses to the colliders, which is correct. It then calls
  `query_pipeline.update_incremental(&colliders, &touched, &[], false)`. In rapier 0.22,
  `refit_and_rebalance = false` runs only `qbvh.pre_update_or_insert` (`pipeline/query_pipeline/mod.rs:330-342`),
  which marks the leaf dirty. Leaf AABBs are refit only when the flag is `true`.
  `PhysicsPipeline::step` passes `true` on its final CCD substep (`physics_pipeline.rs:632-639`). So right after
  `pipeline.step`, the tree holds the EXPLODED AABB. After the restore, BVH culling never reaches the collider at
  its restored pose. The guard reproduces the "explosion" with `update_incremental(…, false)` too, so its tree
  never holds the exploded AABB. It proves only the collider-position propagation, and it stays green with the
  `update_incremental` line deleted.
- **Evidence** (scratch probe with a real recovery, `step` + `cast_ray` through the public API; three explosion
  kinds: linvel 3e5, angvel `f32::MAX`, linvel `f32::MAX`):
  ```
  [finite jump] steps=1 recovery=(1, 1, 0) body_y=50 finite_rot=true
  [finite jump] same frame: ray@restored body -> Some(1.0); ray@floor tile x=300 -> Some(1.0)
  [finite jump] next frame: ray@restored body -> Some(52.0)
  ```
  The ray straight down through the restored body (crown at y=52) returns the floor below it (1.0). The stale
  active set forces one more step next frame, and that step's final refit consumes the dirty leaf.
- **Impact**: for one frame after a recovery, rays, KCC sweeps and LOS pass through the restored body.
  Pre-#4687 they hit it at the exploded pose; now they miss it entirely. Neighbouring static colliders were not
  poisoned (checked). The effect is small, but #4687 is closed as fixed, and the skill's Dim 2 text asserts the
  invariant.
- **Related**: #4687 (closed), #4685 (the incremental design this depends on), #4772.
- **Suggested Fix**: pass `true` (it refits and rebalances only on a recovery frame), or `mark_colliders_dirty()`
  and let the post-loop fallback rebuild. Make the guard index the exploded pose with
  `update_incremental(…, true)`, as rapier does, or drive a real recovery through `step` the way the probe did.
  Then fix the skill sentence.

### PHYS-D2-2026-09-29-02: `recovery_counts()` mixes units — the lifetime total counts events, `last_frame` counts bodies
- **Severity**: LOW
- **Dimension**: Step & Sync (also Queries & Diagnostics)
- **Location**: `crates/physics/src/world.rs:244-252, 925-926`; printed at `byroredux/src/commands/physics.rs:199-208` and `byroredux/src/commands/ragdoll_status.rs:89-100`
- **Status**: NEW
- **Trigger Conditions**: any recovery that restores more than one body (a multi-bone ragdoll always does).
- **Description**: the two counters count different things:
  - `recoveries_total += 1` once per recovery event.
  - `recoveries_last_frame += restored as u32` adds the number of bodies restored.

  The field docs call both "recoveries". The step `break`s after a recovery, so the event count per frame is only
  ever 0 or 1. An 18-bone corpse restore prints `recoveries: total=1 last_frame=18`, so `total >= last_frame` does
  not hold.
- **Evidence**: `world.rs:925` `self.recoveries_total = self.recoveries_total.saturating_add(1);` · `:926` `self.recoveries_last_frame = self.recoveries_last_frame.saturating_add(restored as u32);`
- **Impact**: a gate or operator comparing the two numbers gets inconsistent semantics. #4683 exists precisely so
  that gates can assert on these counters.
- **Related**: #4683 (closed), #4772.
- **Suggested Fix**: rename the second counter to `bodies_restored_last_frame` and update both command labels.
  Alternatively, count events in both and add a separate body count.

### PHYS-D5-2026-09-29-01: The two water samplers still disagree on marker-only currents — the player feels a `WaterCurrentVolume` only inside a submerged `WaterPlane` column
- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**: `byroredux/src/systems/character.rs:1080-1190` (`player_water_state`); compare `crates/physics/src/water.rs:848-863, 1050-1086`
- **Status**: NEW. This is the remainder of the #4691 parity rule; #4691 fixed the co-located case.
- **Trigger Conditions**: a placed current box (XWCU/XPRM → `WaterCurrentVolume`) that extends beyond its plane's
  XZ footprint or surface-mesh coverage. Placed-water surface meshes are partial strips (17c01a4e5). The
  occurrence in shipped content was not censused this pass.
- **Description**: the dynamic path resolves `current_flow` independently of `surface`. It applies the marker drag
  at `frac = 1.0` whenever the AABB centre is inside the marker box and the union XZ footprint, including a body
  "in a marker that overlaps no plane in XZ" (the #3114/#3268 comments name this case). `player_water_state` looks
  the marker up only inside `for (entity, plane) in wq.iter()`, after three `continue`s:
  - `surface_y_at` returns `None`;
  - the column is outside `[volume.min.y, surface_y]`;
  - `fraction <= 0`.

  So in the marker-only region a barrel drifts and the swimmer beside it does not. The skill rule is "every water
  input added to one must exist in the other".
- **Evidence**: the guard `player_water_state_falls_back_to_a_placed_current_volume` (`character.rs:1720-1800`)
  places its marker entirely inside the lake, so the marker-only branch is never exercised.
- **Impact**: the player gets no drift in rapids whose authored current box outruns the water surface mesh, while
  clutter there drifts.
- **Related**: #4691, #3974 (closed), #4911 (XWCU scroll/physics divergence, EXAL).
- **Suggested Fix**: resolve the marker before the plane loop, as the dynamic path does. The flow applies even when
  no plane column matched (a swim state needs a plane; a current does not). Alternatively, record the asymmetry as
  intended in `watal.md` and pin it with a marker-outside-plane test.

### PHYS-D2-2026-09-29-03: The #4997 guard was inserted inside the #3266 test's doc comment
- **Severity**: LOW
- **Dimension**: Step & Sync (test hygiene)
- **Location**: `crates/physics/src/sync.rs:2451-2462, 2506`
- **Status**: NEW
- **Description**: `6d05c2bc0` inserted `register_newcomers_parallel_section_holds_no_world_guard` between the
  `/// #3266 regression guard: …` doc lines and `physics_diagnostics_resolve_forms_after_storage_guards_drop`. The
  #3266 rationale now documents the wrong test, and the #3266 test has none.
- **Impact**: doc-only; a reader deleting "the #4997 test" takes the #3266 rationale with it.
- **Suggested Fix**: move the three `/// #3266 …` lines down to `physics_diagnostics_resolve_forms_after_storage_guards_drop`.

---

## Existing (matched to an open issue; cited, not re-counted)

### PHYS-D1-2026-09-11-02 (#4134): Character-controller/ground-probe capsule shapes use a floor-only `.max(1e-3)` — now ONE site
- **Severity**: LOW · **Dimension**: Shape Translation · **Status**: Existing: #4134
- **Location**: `crates/physics/src/world.rs:1634-1636` (`character_capsule`)
- **Update**: #4614 (`32f774c01`) consolidated the three sites into `character_capsule()`. Its own doc names it
  "the one place the degenerate-extent floor lives, for #4134's clamp to extend". The issue checklist collapses to
  this one line: route both arguments through `clamp_shape_extent`.

### #4772: FO3 copied-save restore's first ragdoll solve jumps the root to ~1e12 BU
- **Severity**: (issue) · **Dimension**: Ragdoll & Constraint Seam · **Status**: Existing: #4772
- Unchanged in code this window. PHYS-D2-2026-09-29-01 (one-frame query gap) and D2-29-02 (counter units) both
  fire on this path.

---

## Confirmed-Fixed Register (re-derived from code this pass)

| Issue | Fixed by | Re-derivation |
|---|---|---|
| #4682 (snapshot walked the arena) | `fa6235775` | Snapshot iterates `dynamic_bodies` (`world.rs:868-882`). Indexed at `sync.rs:1091`, `ragdoll.rs:297`, `world.rs:638-640`. Guard `recovery_snapshot_index_tracks_dynamics_not_the_arena` green. |
| #4683 (recovery invisible) | `fb8fae288` | Counters surfaced in `phys.stats` and `ragdoll.status` with live joints. Units: see D2-29-02. |
| #4684 (census scope) | `004e9befb` | Verdict states both scopes and refuses the drop claim; `collision_authoring_totals_registry_wide`. |
| #4685 (full QBVH rebuild) | `15ad2455c` | `Some(&mut query_pipeline)`. Verified against rapier source: incremental with final refit. |
| #4686 (release-only clamp pin) | `45a0b8b6b` | `non_finite_extents_clamp_in_the_debug_lane_too` has no cfg gate; green. |
| #4687 (recovery edges) | `6ca10bf4b` | (a) parking ✓, (c) all articulations ✓, **(b) no-op → D2-29-01** |
| #4688 (sync.rs doc pointer) | `7e942371f` | not re-verified beyond the module doc test |
| #4689 (NPC constants copied) | `e9ae939a5` | derived from `CharacterController::HUMAN` / `ContactConfig::DEFAULT` (`locomotion.rs:55-88`); guard green |
| #4690 (M42.10 sweep docs) | `3f7f361a6` | `world.rs:1069-1079` states ghost-through-NPCs / clutter blocks, never shoved |
| #4691 (plane-wins current) | `0e607cbac` | additive composition (`character.rs:1158-1175`); marker-only remainder → D5-29-01 |
| #4614 (per-call capsule Arc) | `32f774c01` | stack `Capsule` via `character_capsule`; source-scan guard over the production prefix |
| #4791 (missing current storage) | `0963d675d` | `current_q` optional (`character.rs:1098`) |
| #4997 (rayon section unpinned) | `6d05c2bc0` | guard scans the production prefix; non-vacuous (doc placement: D2-29-03) |

## Known-Open Register (what this pass changed)

- **#4134** OPEN: collapsed to one site (above).
- **#3477** OPEN: fixed in code (the `registered_shape_generations` cache, `sync.rs:941-990`;
  `CollisionShape::TRACK_CHANGES = true`). Still open; not re-filed.
- **#4772** OPEN: unchanged. The new one-frame query gap (D2-29-01) rides on it.
- **#4797** OPEN (perf): the grid index is implemented per `PERFORMANCE_FIX_STATUS_2026-09-26.md`, but the issue
  is still open. Cited.
- *tes_grounding_zero_mass_dynamic_fix* and *interior_spawn_point_fix*: untouched this window.
- Ball-and-socket / stiff-spring / chain constraints: still decoded but declined by design. No occupancy census was
  run, so nothing was filed.
- WATAL water-walking and freezing are still unbuilt per `docs/engine/watal.md`; not filed.
- Third-person camera collision (boom unswept) is a documented slice follow-up. D4-29-01 is its un-documented
  gameplay consequence.

## Cross-Audit Deduplication

| Topic | Owner | Note |
|---|---|---|
| Third-person reach loss in `interaction.rs` / `combat.rs` | `/audit-gameplay` | Root cause is in Dim 4's `camera_follow_system`; reported once here (D4-29-01) |
| Body-yaw one-frame lag behind the boom | `/audit-concurrency` | CONC-D4-2026-09-28-02 / #4995, closed |
| Npc interaction arm per-frame cost (same `interaction.rs` path) | `/audit-ecs` | ECS-2026-09-29-D6-01; no overlap with reach |
| `ragdoll_writeback_system` walk guards (std `HashSet` per frame) | `/audit-ecs` | #4572 / #4769 closed; outside the render/skinning hot-path hashing rule |
| NPC-worn gear keeps bhk collision like spawn-time outfits | `/audit-gameplay` / `/audit-nifal` | behaviour unchanged by `0182fc5e8`; content-dependent; not filed |
| `WaterSurfaceMesh` build / XWCU scroll | `/audit-exterior` | #4911, #4797 cited |
| Lock order of the new physics paths | `/audit-concurrency` | AUDIT_CONCURRENCY_2026-09-28 covered the water sampler and rayon section |

## Verification notes

- Each finding was re-read at HEAD, and a disproof was attempted:
  - **D2-29-01**: rapier 0.22 `update_incremental` and `PhysicsPipeline::step` were read from the vendored source,
    then the effect was measured with a real recovery through the public API.
  - **D4-29-01**: every `camera_ray` caller and both reach constants were traced. The p3 gate text was read in
    `playable-vertical-slice.md:1282-1292`.
  - **D5-29-01**: both samplers were read side by side, and the guard fixture's geometry was checked.
  - **D2-29-02 / 03**: read directly.
- Dedup:
  - Open issues come from `/tmp/audit/issues.json` (163 open, up to #4989). Physics-keyword titles were reviewed.
  - `gh` state was checked for #4134, #3477, #4572, #4574, #4614, #4682-#4691, #4769, #4772, #4791 and #4997.
  - Closed-issue search: "third person" (#4995, #3180 — not the same defect) and "convex hull non-finite" (the hull
    sink-guard asymmetry was already disposed below the floor in AUDIT_PHYSICS_2026-08-27b; not re-filed).
  - Sibling reports dated 2026-09-29 (CONCURRENCY, ECS, NIFAL, NIF, PERFORMANCE, RENDERER, SAFETY) plus
    AUDIT_CONCURRENCY_2026-09-28 were grepped for every finding's key symbols. The only touch is ECS D6-01 (see the
    table above).
- The skill's `_audit-validate.sh` run shows no path errors and no advisories for `audit-physics/SKILL.md`. One
  skill sentence is now false: the Dim 2 claim that #4687 propagates restored poses to "the query pipeline
  same-frame". Correct it together with the D2-29-01 fix.

Next step: `/audit-publish docs/audits/AUDIT_PHYSICS_2026-09-29.md`. Suggested labels:

- `physics` on all findings;
- `gameplay` + `character` on D4-29-01;
- `water` on D5-29-01;
- `test-gap` on D2-29-01 and D5-29-01;
- `doc-rot` on D2-29-02 and D2-29-03;
- `game:fo3` optional on D2-29-01 (#4772 path).
