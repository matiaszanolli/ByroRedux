**HEAD**: 3bcf6c8e8 · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-08.md` (HEAD `00f580e09`) · **Audited**: Dim 2 Step & Sync (streaming-deep focus: collider insert/remove across cell load/unload, ragdoll body lifecycle, the 4-phase sync around streamed entities, and the live Skyrim SE `sap_axis.rs` panic), Dim 4 Character & NPC Controller (mover callers changed), Dim 5 Water / Buoyancy (#5357), Dim 6 Queries & Diagnostics (#5356 counters) · **Unchanged since baseline (skimmed)**: Dim 1 Shape Translation, Dim 3 Ragdoll & Constraint Seam (no commits; the field panic is a Dim 2 containment fault)

# PHYSAL / Physics Audit — 2026-10-09

**Run**: `/audit-physics`, one leg of `/audit-suite --preset streaming-deep`. One auditor covered all six
dimensions; no sub-agents were used. Per-dimension notes are in `/tmp/audit/physics/dim_{1..6}.md`. The suite
deletes that directory, so the Phase 3 `rm -rf` was skipped on purpose.

**Delta audited**: `00f580e09..3bcf6c8e8` (81 commits workspace-wide). These touch physics scope:

- `26a1277bc` #5355/#5356/#5272: the joint index is swept at the top of `clamp_explosive_velocities`,
  `remove_body` prunes the offence and refusal maps, and the detach counter counts only live articulations.
- `dd3a506e9` #5357: the swimming player's placed-current drift keeps the marker at full strength.
- `f0c683ef1` #5373/#5371: the force-greet and Eat/Sleep movers write `Transform`, and the `PhysicsWorld` guard
  is scoped to the step. This unmasks #5452 (see Existing).
- `8410ad1e5`, `5b68792b7`, `9d8fd4cc9`: dialogue-voice Access rows in `late.rs` only.

The in-area streaming commits (#5358, #5359, #5387, #5391, #5393/#5395, #5418, #5421–#5424, #5376, #5379) touch
no physics code. The #5379 purge's Rapier gap is CONC-D5-2026-10-09-01; see Cross-Audit.

**Field evidence**: the engine log
`/tmp/claude-1000/-mnt-data-src-gamebyro-redux/1d2e5fa5-…/scratchpad/c5482/engine2.log` comes from a Skyrim SE
`--grid 0,0 --radius 2` run at HEAD `3bcf6c8e8`. It was read by grep only.

**Tests**: run with `-j 4` and `TMPDIR=/mnt/data/tmp`, using the 1.96.0 toolchain.

| Lane | Result |
|---|---|
| `cargo test -p byroredux-physics` | **208 passed**, 0 failed (baseline 205; +3 from `26a1277bc`) |
| `cargo test -p byroredux --bin byroredux -- ragdoll` | 30 passed, 7 ignored (the 6 FO3-data tests and the skeever probe were not run) |
| `… -- character` / `-- locomotion` / `-- water` | 60 / 6 / 140 passed (water: 4 ignored) |
| `… -- scheduler_access` / `-- rapier_release` / `-- player_body` | 28 / 9 / 12 passed |
| `… -- eat_sleep` / `-- forcegreet` | 10 / 5 passed |

No engine was launched. A scratch probe crate (`/tmp/audit/physics/probe`, raw rapier 0.22) was built and run
outside the repo. The only repo file written is this report.

---

## Executive Summary

| Severity | NEW | Regression | Existing (matched, cited) |
|---|---:|---:|---:|
| CRITICAL | 0 | 0 | 0 |
| HIGH | **1** | 0 | 1 (#5352) |
| MEDIUM | 0 | 0 | 3 (#5353, #5354, #4772) |
| LOW | **2** | 0 | 4 (#5452, #5453, #4134, #5155) |

### The live Skyrim `sap_axis.rs:61` panic is not #5352

**#5352 does not explain the panic.** The panic exposes a separate, larger containment hole, reported as
PHYS-D2-2026-10-09-01 (HIGH).

- **#5352 cannot be on this path.** #5352's hole is the restore arm skipping the clamp, so it needs a restore
  substep. The run logged exactly one restore: 18 bodies of actor 2443 at 00:50:40 (log `:1697-1698`). That
  restore rolled back the entire rig and detached its articulation. The panic came 2 min 47 s later, on a
  different rig (actor 16468, log `:4112-4133`). No restore or park line appears in between.
- **The clamp ran.** The substep before the panic logged a first-offence body clamp on all 18 of actor 16468's
  bones, plus "clamped 40 exploding articulation DOF velocities". The next log line is the panic.
- **Positions were not already huge going in.** `restore_invalid_dynamic_bodies` runs before the clamp in
  `run_substep` (`world/mod.rs:974-998`). It claimed nothing, so every bone was finite and within 2,048 BU of its
  pre-substep pose.
- **The broken AABB was created and broad-phased inside the very next `pipeline.step` call.**
  - Rapier 0.22 runs 4 + 12 = 16 internal TGS substeps for a ragdoll island.
  - Each internal substep integrates positions.
  - The call then runs `detect_collisions` (broad phase and narrow phase) itself, before returning.
  - The restore and the clamp only run after the call returns.
- **The containment's own model of the failure is wrong.** It assumes an explosion is only integrated by the
  *next* call. The "≈2.68e11 grid boundary" is the saturation of rapier's `point_key` `as i32` cast, not a clamp.

A scratch probe reproduced the mechanism: a rig entering the step with every engine cap satisfied moved 1.6e11 to
2.2e16 BU in one call. Two probe configurations panicked inside the call, at a second rapier site (the narrow
phase). The `sap_axis` site was not reproduced.

**Streaming verdict: holds.** The streamed-entity physics lifecycle is intact:

- Newcomers spawn asleep with no wake.
- Terrain colliders are spawned before the REFR walk.
- Starts-Dead corpses activate in the Late sink, after PostUpdate propagation.
- Cell unload releases both `RapierHandles` and `Ragdoll` bodies.
- `remove_body` now prunes every per-body map (#5272 fixed).

The one streaming-adjacent gap is that newcomer registration inserts colliders at an unchecked translation
(PHYS-D2-2026-10-09-02, LOW).

### PHYSAL doctrine verdict: **HOLDS** (eleventh consecutive pass)

```
$ grep -rnE "GameKind|game_kind|bsver|NifVersion|is_skyrim|is_fo4|is_oblivion|is_fo3|is_fnv|game ==|BS_F76|SF_FORM_ID|havok_scale|GameVariant" \
    crates/physics/src/ byroredux/src/ragdoll.rs byroredux/src/systems/{character,locomotion,water,eat_sleep,forcegreet}.rs \
    byroredux/src/commands/{physics,water}.rs
(no matches)
```

The only per-game branches are the three named seams: constraint CInfo decode, `havok_scale_for`, and
collision-object-kind dispatch. Collision data traced this pass:

- Skyrim SE, from the field log only: humanoid, canine, mudcrab, deer, hare, mammoth, sabrecat and giant
  `RagdollTemplate`s attached.
- On every Skyrim skeleton, `bhkRagdollConstraint`/`bhkLimitedHingeConstraint` log `consumed != block_size`. This
  is the deliberate FO3+ motor-trailer under-read (`constraints.rs:755-765`), not misalignment. See Skill drift.

---

## Solver Invariant Matrix

| Invariant | Verdict | Note |
|---|---|---|
| Fixed step: accumulator clamped before the loop; `frame_dt.max(0.0)` | **HOLDS** | `world/mod.rs:755-759` |
| Anti-spiral wall-clock budget (#1698) | **HOLDS** | `run_substep` `:1004-1007` |
| Static-scene fast path; spawn exempt from wake (#3969) | **HOLDS** | `:821-828`; `register_newcomers` still builds dynamics `sleeping(true)` (`sync.rs`) |
| Wake survives a zeroed accumulator | **HOLDS** | `Stop` → `steps += 1; break` (`:840-846`) |
| Collider-set mutation reaches the query pipeline | **HOLDS** | `Some(&mut self.query_pipeline)` (`:969`); stepless dirty frames rebuild (`:903-905`) |
| Explosion recovery: snapshot before every substep, delta bound, parking | **HOLDS between calls** | `:929-941`, `recovery.rs:51-55`; blind inside a call (D2-01) |
| Pre-explosion containment (#5161/#5246) | **DRIFTED** | in-call integration bypasses it (D2-01, NEW HIGH); restore substep skips the clamp (#5352); DOF cap catches the free root (#5353) |
| Position gate on every body-placement entry | **DRIFTED** | only `push_kinematic` is gated; `register_newcomers` / `set_kinematic_translation` are not (D2-02) |
| Fixed-on-fixed broad-phase filter | **HOLDS** | `set_motion_type` (`:646-682`) is still the only production `set_body_type` → `body_left_fixed` |
| Lock ordering (`PhysicsWorld` is a sink) | **HOLDS** | CONC-D5-2026-10-08-01 fixed by `f0c683ef1` (movers scope the guard to the step) |
| Phase order collect → push kinematic → buoyancy → step → pull | **HOLDS** | `sync.rs` unchanged |
| Teardown completeness (cell unload) | **HOLDS** | `release_victim_rapier_bodies` (`unload.rs:854-887`) covers `RapierHandles` + `Ragdoll`; `remove_body` prunes labels, offences, refusals (`:445-456`); joint index swept (`recovery.rs:290-291`) |
| Teardown completeness (session-replace purge) | **DRIFTED, owned elsewhere** | CONC-D5-2026-10-09-01 (`unload.rs:97` bare `despawn_batch`, phantom collider) |
| NPC KCC callers consume the step contract | **DRIFTED (Existing #5452, now live)** | the masking bug ECS-D6-01 was fixed by `f0c683ef1` |
| Water samplers agree | **HOLDS for currents (#5357 fixed); DRIFTED for damping (#5354)** | `player_current_drift` = plane × fraction + marker × 1.0, matching `water.rs:978-1073` |
| PHYSAL doctrine | **HOLDS** | eleventh pass |

---

## Findings — HIGH

### PHYS-D2-2026-10-09-01: A ragdoll explosion is integrated and broad-phased inside ONE `pipeline.step`, so no #5161/#5246 guard can run before rapier panics (live Skyrim SE `sap_axis.rs:61` crash)
- **Severity**: HIGH. A process-killing panic under ordinary exterior play (Skyrim SE grid 0,0 radius 2, about
  3.5 min in). Nothing in the engine can intercept it today. This is the same class #5352 is rated for.
- **Dimension**: Step & Sync
- **Location**:
  - `crates/physics/src/world/mod.rs:942-972`: the `pipeline.step` call. Hooks are `&()` at `:970`.
  - `crates/physics/src/world/mod.rs:974-998`: the restore and clamp, which run only after the call returns.
  - `crates/physics/src/world/mod.rs:74-101`: the cap docs that state the wrong failure model.
  - `crates/physics/src/world/recovery.rs:284-441`: `clamp_explosive_velocities`. The wrong NaN model is at
    `:301-309`.
  - `crates/physics/src/broad_phase.rs:44-63`: `FixedPairFilterBroadPhase::update` delegates with no AABB check.
  - `crates/physics/src/ragdoll.rs:391`: `.additional_solver_iterations(12)`.
- **Status**: NEW. This is a residual of the closed #5161/#5246 class, and it is distinct from #5352. No issue
  matched in the open snapshot. Closed-issue searches for "in-call explosion pipeline.step broad phase" and
  "sap_axis" returned only #5161.
- **Trigger Conditions**: an activated ragdoll whose solve blows up on any internal TGS substep except the last.
  The input state can be fully sane: finite, within the restore bound, and inside every cap. In the field this was
  a streamed Skyrim humanoid corpse (actor 16468, `meshes\Actors\Character\Character Assets\skeleton.nif`,
  18 bodies), 52 s after its cell streamed in.
- **Description**: the #5161 containment assumes that an explosion born in one `pipeline.step` call reaches
  positions only in the *next* call. It is built on that assumption: `VELOCITY_SANITY_CAP_BU_PER_S`'s doc says
  "the *next* call's position integration then places its AABB near … the grid boundary". So it clamps
  velocities, and restores displaced bodies, between calls. Rapier 0.22 does not behave that way.
  - **Many integrations per call.** `island_solver.rs:45-49` runs
    `num_solver_iterations + additional_solver_iterations` internal substeps at `dt / n`. That is 4 (the default,
    which the engine never overrides) + 12 for a ragdoll island, giving 16.
  - `velocity_solver.rs:161-221` integrates positions in every one of them, including multibody forward
    kinematics (`:247-258`), and then runs the stabilization pass.
  - A velocity that blows up in internal substep *k* is therefore integrated in substeps *k+1…15* of the same
    call. The #5161 premise holds only when the blow-up is born in the last substep's stabilization pass.
  - **Collision detection inside the same call.** `physics_pipeline.rs:613-640` runs
    `advance_to_final_positions` and then `detect_collisions` (broad phase, then the narrow phase's
    `compute_contacts`) before `step` returns. The engine supplies no code at that point: hooks are `&()`, and
    `FixedPairFilterBroadPhase::update` forwards to the multi-SAP without looking at AABBs.
  - **First-rung response is too weak.** Rapier overwrites a multibody link's `RigidBody` velocity from the
    generalized velocities at every solve (`velocity_solver.rs:137` → `multibody.rs:503,521`). So the body-level
    clamp on the 18 bones was a no-op, which the test comment at `world/mod.rs:1576-1578` already concedes. Only the
    DOF clamp bounded the rig. Offence rung 1 also leaves the exploding constraint configuration (the pose is not
    rolled back) to be re-solved on the very next call. In the field, that next call was fatal.
  - **The failure model in the docs is wrong.** Two statements need correcting:
    - "≈2.68e11 multi-SAP grid boundary" is not a clamp. `clamp_point` bounds at ±`f32::MAX/4`
      (`sap_utils.rs:22-24`). The 2.68e11 figure is `point_key`'s saturating `floor() as i32` at the 125-BU region
      width: `i32::MAX × 125` = 2.684e11. The threshold is about 2.1e9 for a 1-BU layer.
    - `recovery.rs:301-309` says a NaN AABB "clamps to finite grid corners". In fact
      `handle_modified_collider` rejects non-finite AABBs before `clamp_point`
      (`broad_phase_multi_sap.rs:376-385`).

    The panicking proxy fits the real model: it is a degenerate point AABB
    `[2.684e11, 1.193e11, 2.684e11]` with mins equal to maxs. That is a region created at the saturated key, which
    trips `batch_insert`'s bounds assert when it propagates to the larger layer.
- **Evidence**:
  ```
  :1697 ERROR physics: invalid-solve evidence (first of 18): … [actor 2443 bone npc com] at |t|=1.947e4 |v|=2.958e6 …
  :1698 ERROR physics: restored 18 dynamic body/bodies after an invalid solve …        ← the run's ONLY restore
  :4111 WARN  physics: clamped 1 exploding articulation DOF velocities …               (00:53:27)
  :4112-4129 WARN physics: clamped explosive velocity on … [actor 16468 bone npc com / l thigh / … / l hand] (18 bones, 1st offence)
  :4130 WARN  physics: clamped 40 exploding articulation DOF velocities to the sanity cap (#5161)
  :4132 thread 'main' panicked at rapier3d-0.22.0/src/geometry/broad_phase_multi_sap/sap_axis.rs:61:13:
        proxy.aabb.maxs 268435460000 (in Aabb { mins: [268435460000.0, 119289000000.0, 268435460000.0],
        maxs: [268435460000.0, 119289000000.0, 268435460000.0] }) >= min_bound 268435470000
  ```
  Scratch probe (`/tmp/audit/physics/probe/src/main.rs`):
  - Setup: raw rapier 0.22 with the engine's `IntegrationParameters`, 686.7 gravity and a fixed ground cuboid.
    The rig is a multibody chain of spherical joints with `additional_solver_iterations(12)`.
  - Between calls the probe applies the engine's body and DOF clamp, and it stops at the first call that moves any
    link more than 2,048 BU. So every call it runs starts from a state the engine's guards accept.
  - Results:
    - With 12 extra iterations, single calls moved a link **1.6e11 to 2.2e16 BU** on frames 1-3. Examples: `n=6 ratio=2000 pen=20 → 5.6e11`;
      `n=12 ratio=20 pen=20 → 1.4e15`.
    - `n=12 ratio=2000 pen=60` (balls) **panicked inside the call** at `parry3d-0.17.6
      clip_aabb_line.rs:141` ("Matrix index out of bounds"). The path was `physics_pipeline.rs:616` →
      `narrow_phase.rs:941`, the same-call narrow phase on an existing contact pair.
  - The probe rig is synthetic (tight limits, inverted masses). It proves the mechanism, not the content trigger.
    The `sap_axis` assert itself was not reproduced; it depends on a rounding window.
- **Impact**:
  - Any activated ragdoll in any game can kill the process. Two Skyrim humanoid rigs exploded within 3.5 min of
    one exterior session; the second crashed it.
  - #5161's and #5246's closing A/Bs held only on their cells. The whole guard family (velocity cap, DOF cap,
    offence ladder, restore) acts between calls and cannot see this path.
  - Fixing #5352 would not close it.
- **Related**: #5352 (separate restore-substep hole, still open); #5353 (the free-root DOF cap; field log
  `:781-923`, `:3779-3855` and `:4096-4111` show its per-substep floods on Skyrim corpses; a possible contributor,
  not established); #5161, #5246 (closed); #4772; PHYS-D6-2026-10-09-01 (the DOF log could not attribute the
  pre-panic floods). The rig's root cause (upstream mass-inverted instability) still has no issue: see the
  Known-Open Register.
- **Suggested Fix**: contain the explosion at the two engine-owned points rapier calls *inside* `step`, so the
  existing restore can roll the body back afterwards.
  - **Broad phase**: in `FixedPairFilterBroadPhase::update`, pass the inner multi-SAP a copy of
    `modified_colliders` that leaves out every collider whose `compute_collision_aabb` has a coordinate beyond a
    sane bound. Reusing `KEYFRAME_TARGET_SANE_BOUND_BU` (1e8, below every layer's saturation point) works. The
    proxy keeps its last sane AABB.
  - **Narrow phase**: replace the `&()` hooks with a `PhysicsHooks` whose `filter_contact_pair` returns `None`
    for any pair with a collider past that bound. Set `ActiveHooks::FILTER_CONTACT_PAIRS` on ragdoll colliders in
    `build_ragdoll`. Rapier ORs the two colliders' flags (`narrow_phase.rs:880-897`), so ground pairs are covered
    without hooking every static collider.
  - **Escalation**: escalate an articulation's first burst straight to park or detach, with a snapshot rollback,
    instead of clamp-only.
  - **Docs**: correct the `:74-101` and `recovery.rs:301-309` failure-model docs.
  - **Test**: pin the probe's panicking configuration as a `PhysicsWorld` test that asserts no panic and a counted
    restore.
  - **Longer term**: a later rapier release with a BVH broad phase would remove the `sap_axis` class entirely.
    Verify the exact version before planning on it.

## Findings — LOW

### PHYS-D2-2026-10-09-02: `accept_keyframe_target` gates only `push_kinematic`; newcomer registration and `set_kinematic_translation` place Rapier bodies at unchecked translations
- **Severity**: LOW. A hardening gap in the same panic class. No content evidence: the field panic did not come
  through these entries.
- **Dimension**: Step & Sync (streamed-collider insert)
- **Location**:
  - `crates/physics/src/sync.rs:1066`: `register_newcomers`, `.position(iso_from_trs(n.global.translation, …))`.
  - `crates/physics/src/sync.rs:87-111`: `set_kinematic_translation`. Its callers are
    `byroredux/src/save_io.rs:781` (`apply_player_pose`, a save-file pose with no finiteness or range check),
    `byroredux/src/systems/character.rs:635,907,1036` and `byroredux/src/commands/view.rs:482`
    (`combat.approach`).
  - Compare `sync.rs:1243-1255`: the only gate, `accept_keyframe_target`, at `recovery.rs:227-254`.
- **Status**: NEW
- **Trigger Conditions**: any of the following:
  - a streamed entity whose `GlobalTransform` at first registration is finite but beyond the multi-SAP saturation
    window (from about 2.1e9 BU for small colliders up to 2.68e11 BU for bone-sized ones), for example a broken
    first animation sample on a live actor's bone, or a corrupt placement;
  - a save whose player pose is corrupt;
  - a debug teleport to such a coordinate.
- **Description**: #5161 refused insane keyframe targets because "the substep recovery only snapshots `Dynamic`
  bodies, and live actor bones are keyframed — this boundary check is the only guard they have". Two other entry
  points put a body into the broad phase at an arbitrary position:
  - **Newcomer registration** builds every streamed body, keyframed bones included, at
    `n.global.translation` with no check. The body is never in the snapshot; a fixed or keyframed one never will
    be.
  - **`set_kinematic_translation`** drives the player capsule. `apply_player_pose` passes it the saved position
    verbatim.

  NaN is harmless here, because rapier rejects non-finite AABBs. A finite out-of-range value is the panic
  precondition analysed in D2-01.
- **Evidence**: `sync.rs:1063-1070` builds the body straight from the newcomer's transform. `save_io.rs:720-781`
  performs no check between `Vec3::from_array(pose.position)` and `set_kinematic_translation`.
- **Impact**: a corrupt transform or save crashes the process instead of being refused or parked. Low
  likelihood; same blast radius as D2-01.
- **Related**: PHYS-D2-2026-10-09-01, whose broad-phase withholding fix also closes both entries; #5161.
- **Suggested Fix**: apply the `accept_keyframe_target` bound in `register_newcomers` (skip the entity and log
  once) and in `set_kinematic_translation` (refuse and return `false`). `apply_player_pose` should reject a
  non-finite or out-of-range pose and fall back to the cell's spawn point. Alternatively, rely on D2-01's
  broad-phase filter as the single backstop.

### PHYS-D6-2026-10-09-01: The articulation-DOF clamp logs no articulation, so its per-substep floods (and the pre-panic DOF clamps) cannot be attributed to an actor
- **Severity**: LOW. A diagnostics gap.
- **Dimension**: Queries & Diagnostics
- **Location**: `crates/physics/src/world/recovery.rs:413-440`
- **Status**: NEW
- **Trigger Conditions**: any activated ragdoll whose generalized velocity crosses `ARTICULATION_DOF_SANITY_CAP`.
  Because of #5353, that is every falling corpse.
- **Description**:
  - The body clamp logs a label (`body_labels`, `actor <id> bone <name>`). On a multibody link, though, that
    clamp is a no-op: rapier overwrites the link velocity from the generalized DOFs.
  - The DOF clamp is the one that actually bounds a ragdoll. Its single summary line ("clamped N exploding
    articulation DOF velocities") carries no handle and no label.
  - The field log has dozens of anonymous "clamped 1 …" lines (#5353 falling roots). It cannot show whether actor
    16468 was the corpse flooding them in the seconds before D2-01's panic.
- **Evidence**: the `log::warn!` at `recovery.rs:436-439` formats only `clamped_dofs`. The loop holds the
  multibody (`:419`), and rapier 0.22 exposes `Multibody::root().rigid_body_handle()`.
- **Impact**: the explosion telemetry cannot be attributed, which slowed this triage. #5353's floods are
  indistinguishable from a real articulation explosion.
- **Related**: #5353, PHYS-D2-2026-10-09-01, #4683 (counted recovery).
- **Suggested Fix**: tally per multibody and log `body_labels[multibody.root().rigid_body_handle()]` with each
  count, deduplicating by multibody: `articulation_joints` holds about 17 handles that resolve to the same
  multibody.

---

## Existing (matched; cited, not re-counted)

| Issue | Severity | Location now | Note |
|---|---|---|---|
| #5352 restore substep skips the clamp | HIGH | `world/mod.rs:980-998` | Unchanged and still valid. **Not** the cause of the field panic (no restore that frame); see D2-01 |
| #5353 DOF cap catches the free root | MEDIUM | `world/recovery.rs:418-433` | Unchanged. Field corroboration: per-substep anonymous DOF-clamp floods on Skyrim corpses |
| #5354 flowing-water damping missing in buoyancy/player | MEDIUM | `crates/physics/src/water.rs`, `systems/character.rs` | No change to the damping path |
| #4772 FO3 restore first solve to about 1e12 | MEDIUM | — | Unchanged |
| #5452 movers discard KCC `blocked`, skip NavPath | LOW | `eat_sleep.rs:170`, `forcegreet.rs:132` | **Now live**: ECS-2026-10-08-D6-01 was fixed by `f0c683ef1`, so the step is committed. Re-triage recommended |
| #5453 #5311 split dropped derives and docs | LOW | `world/queries.rs:12`, `world/mod.rs:773-775` | Unchanged |
| #4134 floor-only `.max(1e-3)` capsule | LOW | `world/queries.rs` (`character_capsule`) | Unchanged |
| #5155 P5 soak `grounded=false` after the 10th cycle | — (unlabelled) | — | Not re-investigated; needs an engine run |

Closed in the window and verified at HEAD:

- #5355: `articulation_joints` retain at `recovery.rs:290-291`.
- #5356: the detach is counted only when `rigid_body_link(handle).is_some()` (`:381`), and the clamp doc is
  corrected.
- #5272: `remove_body` prunes `explosion_offences` and `keyframe_refusals_logged` (`world/mod.rs:455-456`).
- #5357: `player_current_drift` composition, checked against `water.rs:978/1013/1047-1073`.

## Streaming-area verification (suite emphasis)

- **Collider insert on cell load**:
  - Newcomers are collected under read locks, converted on rayon, then inserted serially. Dynamics spawn
    `sleeping(true)` with `mark_colliders_dirty` and no `wake()` (`sync.rs:1005-1194`).
  - Terrain is spawned in the cell's non-reference phase, before the REFR walk (`exterior.rs:2007-2010, 2090`),
    so actors never land before their floor's collider.
  - Starts-Dead corpses are queued by `apply_starts_dead` (`reference_state.rs:364-370`) to the Late sink.
    Ragdolls therefore seed from post-PostUpdate world bones.
- **Collider remove on unload**:
  - `release_victim_rapier_bodies` (`unload.rs:854-887`) runs before the despawn on every `release_entities_timed`
    path, and it covers both `RapierHandles` and `Ragdoll`.
  - `remove_ragdoll` → `remove_body` cascades colliders and joints, and now prunes every per-body map.
  - `rapier_release` passes 9/9.
  - The one uncovered despawn is the session-replace purge (CONC-D5-2026-10-09-01, not re-filed).
- **Ragdoll lifecycle on activation**: `activate_ragdoll` frees the keyframed follower bodies and strips both
  `RapierHandles` and `RigidBodyData` (`byroredux/src/ragdoll.rs:515-544`), so no duplicate followers reach the
  solver.
- **4-phase sync**: phase order and lock discipline are unchanged. The only position-sanity gap is D2-02.

## Known-Open Register (what this pass changed)

- **#5161/#5246 containment is incomplete.** D2-01 adds the in-call hole. The register line "contain ragdoll
  solver explosions at the physics boundary" is now false for any explosion born before the last internal
  substep.
- **The upstream instability of mass-inverted rigs** still has no issue of its own. This session saw two Skyrim
  humanoid rigs (`character assets\skeleton.nif`) explode within 3.5 min. Publishing D2-01 should carry a pointer
  to filing the root cause.
- **#5452** went from latent to live.
- **Closed in the window**: #5355, #5356, #5357, #5272.
- Unchanged: ball-and-socket / stiff-spring / chain joints are still declined; WATAL water-walking and freezing
  are still unbuilt.

## Cross-Audit Deduplication

| Topic | Owner | Note |
|---|---|---|
| Session-replace purge despawns without the Rapier release | `/audit-concurrency` CONC-D5-2026-10-09-01 | Phantom collider; not re-filed. SAVE-D4-2026-10-09-01 is the save-side consequence |
| Eat/Sleep anchor and mover behaviour | `/audit-gameplay` GAME-D5-2026-10-09-02 | That report cites #5452 for the KCC contract |
| Per-frame mover lookups | `/audit-performance` | Not physics-correctness |
| bhk constraint byte parsing | `/audit-nif` | The Skyrim `consumed != block_size` lines are the by-design motor under-read |

## Verification notes

- **D2-01**:
  - Read rapier 0.22 at these points:
    - `physics_pipeline.rs:408-640`: the step order, and the second `detect_collisions` at `:616`;
    - `island_solver.rs:30-110` and `velocity_solver.rs:95-272`: the internal substeps, position integration, the
      multibody `update_dynamics` velocity overwrite, and writeback;
    - `broad_phase_multi_sap.rs:358-460`, `sap_layer.rs:215-260`, `sap_axis.rs:44-75` and `sap_utils.rs`: the
      finite rejection, `clamp_point`, the `point_key` i32 saturation, region creation, and the asserts;
    - `narrow_phase.rs:878-945`: the filter-hook placement.
  - Attempted disproofs, none of which held:
    - "#5352 caused it": there was no restore that frame.
    - "positions were already huge": the restore precedes the clamp on the same substep and claimed nothing.
    - "a newcomer or keyframe insert caused it": there is no refusal or registration evidence, and all 18 of
      actor 16468's bones exploded at that moment.
- **D2-02**: read every `set_translation` / `set_position` / `set_next_kinematic_*` / body-builder `.position`
  site in `crates/physics/src` and `byroredux/src`. Only `push_kinematic` routes through `accept_keyframe_target`.
- **Dedup**:
  - `/tmp/audit/issues.json` (147 open) was filtered by physics, constraint and recovery keywords.
  - Closed-issue searches for `sap_axis`, `bhkRagdollConstraint`, the DOF-clamp label and `register_newcomers`
    bounds matched nothing beyond #5161 and the closed constraint-stub history.
  - Today's sibling reports were grepped for Rapier, ragdoll and `PhysicsWorld`; only CONC-D5 and its SAVE
    sibling overlap, and both are cited.

## Skill drift (for the next `/audit-sync`)

- Dim 2's containment text ("`clamp_explosive_velocities` runs at the end of every substep … irrecoverable once an
  AABB nears its ≈2.68e11 grid boundary") should record two things:
  - the in-call path (D2-01);
  - that 2.68e11 is `point_key` i32 saturation at the 125-BU width, not a clamp.
- Dim 3's claim that "Byte consumption is exact for the decoded types, so drift telemetry is live for them" is
  false on FO3+/Skyrim. Ragdoll, LimitedHinge and Prismatic deliberately leave the motor trailer to `block_size`
  (`constraints.rs:755-765`), so every skeleton logs drift.
- Dim 5 names the guard `player_water_state_falls_back_to_a_placed_current_volume`, which `dd3a506e9` deleted. Its
  successors are:
  - `swimming_drift_scales_the_plane_by_fraction_and_the_marker_at_full_strength`;
  - `the_marker_reaches_the_swimmer_through_drift_not_the_column_state`;
  - `a_marker_outside_every_plane_still_drifts_the_player`.

  The additive composition now lives in `player_current_drift`, not `player_water_state`.
- Dim 4 should list `eat_sleep.rs` and `forcegreet.rs` as `step_toward` consumers (carried over from 2026-10-08).

Next step: `/audit-publish docs/audits/AUDIT_PHYSICS_2026-10-09.md`. Suggested labels:

- `physics` on all three findings;
- `game:skyrim` and `test-gap` on D2-01 (the panic is live on Skyrim SE; the mechanism is game-agnostic);
- `save-load` on D2-02 (the pose path).
