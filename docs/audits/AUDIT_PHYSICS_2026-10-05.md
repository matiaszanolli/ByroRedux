**HEAD**: a2c24b16e · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-09-29.md` (HEAD `9fcfdc3fc`) · **Audited**: Dim 2 Step & Sync, Dim 3 Ragdoll & Constraint Seam, Dim 4 Character & NPC Controller, Dim 5 Water / Buoyancy, Dim 6 Queries & Diagnostics · **Unchanged since baseline (skimmed)**: Dim 1 Shape Translation. It had commits, but they were one comment rename and clippy test-literal rewrites with no production change, so its guards were spot-checked rather than re-traced.

# PHYSAL / Physics Audit — 2026-10-05

**Run**: `/audit-physics` (default scope, deep), one leg of `/audit-suite --preset comprehensive`. One auditor
covered all six dimensions in order. No sub-agents were used, as the suite requires. Per-dimension notes are in
`/tmp/audit/physics/dim_{1..6}.md`. The skill's Phase 3 `rm -rf` was skipped on purpose; the suite deletes the
directory.

**Delta audited**: `9fcfdc3fc..a2c24b16e` (313 commits workspace-wide; about 25 touch physics scope). The
load-bearing ones:

- **Fixes for the 2026-09-29 findings**:
  - `ea04f2689` #5126 / #5127: the recovery refit now passes `true`, and the per-frame counter is renamed.
  - `60cd7028c` #5130: test docs moved back onto their own tests.
  - `69157c7cd` #5124: third-person gameplay rays start at the eye.
  - `1512c5a12` #5129: the player's placed current is resolved apart from the plane columns.
- **The #5161 / #5246 containment wave**:
  - `44f7bab55`: keyframe-target gate and evidence labels.
  - `5ae7f8ad4`: per-substep velocity / spin / articulation-DOF clamp, seed-pose gate, `articulation_joints`
    index.
  - `e8de9f8c8`: NaN velocities zeroed, lifetime offence count, third-offence articulation detach.
- **Other**:
  - `483776fe5` #5160: `cast_ray_corridor` for the melee swing.
  - `37cc637e9` #5245 / #4929: flowing-water weather transport damping.
  - `98061ec58` #5243: per-cell distant water.
  - `3c08cfe50` #5017: `unconscious` control state.
  - `c9254beb8` #5039: player derived-stat refresh.
  - Clippy sweeps `4ad847a81`, `3f0852e08`, `fd26378da`.

**Tests**: all ran under `cargo test` with `-j 4` and `TMPDIR=/mnt/data/tmp`. The bin crate used the 1.96.0
toolchain cargo.

| Lane | Result |
|---|---|
| `cargo test -p byroredux-physics` | **206 passed**, 0 failed, 0 ignored (baseline 192) |
| `cargo test -p byroredux --bin byroredux -- ragdoll` | 30 passed, **7 ignored**: the 6 FO3-data tests in `ragdoll_installed_tests.rs` and the skeever probe in `ragdoll_skeever_probe_tests.rs`. None of them was run. |
| `… -- character` / `-- locomotion` / `-- water` | 59 / 6 / 139 passed (water: 4 ignored) |
| `… -- scheduler_access` / `-- rapier_release` / `-- player_body` | 24 / 9 / 12 passed |
| `… -- combat` / `-- interaction` | 70 (2 ignored) / 34 passed |
| `cargo test -p byroredux-nif --lib -- collision` | 155 passed |

**Scratch probes**: one standalone crate in the session scratchpad, with two binaries. It depends on
`byroredux-physics` by path and uses only its public API, with target dir `/mnt/data/tmp/physaudit-target`. The
probes drive a real `PhysicsWorld::step` (PHYS-D2-2026-10-05-01) and a production `build_ragdoll` articulation in
free fall (PHYS-D2-2026-10-05-02). The engine was not launched. The only repo file written is this report.

---

## Executive Summary

| Severity | NEW | Regression | Existing (matched, cited) |
|---|---:|---:|---:|
| CRITICAL | 0 | 0 | 0 |
| HIGH | **1** | 0 | 0 |
| MEDIUM | **2** | 0 | 1 (#4772) |
| LOW | **3** | 0 | 2 (#4134, SAFE-D3-2026-10-05-01) |

### What matters

**All four 2026-09-29 findings are fixed, and the fixes hold:**

- D2-29-01 (#5126): the restore refit passes `true`, and the guard now drives a real recovery through `step`.
- D2-29-02 (#5127): the counter is renamed `bodies_restored_last_frame`.
- D2-29-03 (#5130): the doc comments are back on their own tests.
- D4-29-01 (#5124): `player_camera_boom` is the single source for the boom, and `camera_ray` adds it back.

**The #5161/#5246 explosion containment has two holes, found by probe:**

1. **HIGH, PHYS-D2-2026-10-05-01.** On a substep where *any* body is restored, `step` `break`s before
   `clamp_explosive_velocities`. The code comment says that loses nothing because the restored bodies were
   sanitised. Every other body, and every surviving articulation's DOFs, keeps its post-solve velocity unclamped.
   That velocity reaches next frame's first `pipeline.step`. This is rapier 0.22's exact explosion class: the
   stabilization pass changes velocities after positions are integrated. A sibling link whose velocity explodes in
   the same substep that restores another link is therefore integrated next frame before any clamp. That is the
   multi-SAP panic path that #5161 and #5246 exist to close. Measured: an unrelated body at 1e5 BU/s keeps that
   speed through a recovery substep (clamp count 0) and moves 1,667 BU the next frame. The cap bound is 333.
2. **MEDIUM, PHYS-D2-2026-10-05-02.** The new articulation-DOF clamp caps *every* generalized velocity at
   100. A production ragdoll's root is a dynamic body, so rapier gives its multibody a **free root joint** whose
   first three DOFs are linear velocity in BU/s. Every ragdoll therefore falls at a terminal speed of 100 BU/s
   (about 1.4 m/s). The probe measured 204 BU dropped in 2 s, against 1,376 BU for a free body. Every substep of
   that fall also adds one `velocity clamps` count to `phys.stats` and logs one `warn`. Corpses fall in slow
   motion in every game, and the telemetry that #5161/#5246 rely on reports an "explosion" for every falling body.

**The flowing-water weather damping (#5245) reached only half of the wave samplers** (PHYS-D5-2026-10-05-01,
MEDIUM):

- The renderer and the camera `submersion_system` damp the weather scroll on River, Rapids and Waterfall.
- Dynamic-body buoyancy and `player_water_state` still pass the raw scroll.
- The weather scroll is added *before* the crest direction and rate are derived. On flowing water with any wind,
  the physics crests therefore travel in a different direction and at a different speed from the visible surface
  and the camera's waterline.
- The constant's own doc says it is "applied identically by the renderer upload and the CPU crest sampler". It is
  applied by only one of the three CPU samplers.

**The remaining LOWs:**

- `articulation_joints` is never pruned, and the clamp walks it every substep (D2-03; sibling of
  SAFE-D3-2026-10-05-01).
- `clamp_explosive_velocities` still documents the pre-#5246 "clean substep resets" semantics, and it counts a
  "detach" on every later burst, including for bodies with no articulation (D2-04).
- The swimming player feels the placed current scaled by submerged fraction, while the dynamic path applies it at
  1.0. The drift drops by about 32% at the swimlevel threshold (D5-02).

### PHYSAL doctrine verdict — **HOLDS, ninth consecutive pass**

```
$ grep -nE "GameKind|game_kind|bsver|NifVersion|is_skyrim|is_fo4|is_oblivion|is_fo3|is_fnv|game ==|BS_F76|SF_FORM_ID|havok_scale|GameVariant" \
    crates/physics/src/*.rs byroredux/src/ragdoll.rs byroredux/src/systems/{character,locomotion,water}.rs \
    byroredux/src/commands/{physics,water}.rs
(no matches)
```

The three named seams stay at the parse→canonical boundary: constraint CInfo decode, `havok_scale_for`, and
collision-object-kind dispatch. The #5161 seed and keyframe bounds are absolute world-space constants. They are
not per-game branches.

### Games traced

No game's collision data was traced end-to-end. There was no engine launch, and the FO3 installed tests and the
skeever probe were not run. The containment wave's own commits record live evidence:

- Skyrim p2-melee-core (#5161).
- FNV SLscorpionBurrowINT (#5246).
- FNV `GSSettlercm` (#5160).

D2-01 and D2-02 were reproduced with the public API rather than with game data. The solver side is game-agnostic
by construction, as the doctrine grep shows.

### Per-dimension counts

| Dimension | CRIT | HIGH | MED | LOW | Notes |
|---|---:|---:|---:|---:|---|
| 1 — Shape Translation | 0 | 0 | 0 | 0 | comment and test-literal delta only; #4134 still one site (Existing) |
| 2 — Step & Sync | 0 | 1 | 1 | 2 | #5126/#5127/#5130 confirmed; containment holes D2-01/D2-02 |
| 3 — Ragdoll & Constraint Seam | 0 | 0 | 0 | 0 | seed gate sound; doctrine holds; the ragdoll-visible D2-02 is filed under Dim 2, where the code is |
| 4 — Character & NPC Controller | 0 | 0 | 0 | 0 | #5124 confirmed; KCC unchanged |
| 5 — Water / Buoyancy | 0 | 0 | 1 | 1 | #5129 confirmed; #5245 parity hole |
| 6 — Queries & Diagnostics | 0 | 0 | 0 | 0 | corridor cast sound; the counter pollution is under D2-02/D2-04 |

---

## Solver Invariant Matrix

| Invariant | Verdict | Note |
|---|---|---|
| Fixed step: accumulator clamped before the loop; `frame_dt.max(0.0)` | **HOLDS** | `world.rs:1147-1151` |
| Anti-spiral wall-clock budget (#1698) | **HOLDS** | checked after each substep (`:1321`) |
| Static-scene fast path (`active_dynamic_bodies` empty && `!pending_wake`) | **HOLDS** | `:1212-1220`. `recover_pre_broken_bodies` runs before the gate. Spawn stays wake-exempt (#3969). |
| Wake survives a zeroed accumulator | **HOLDS** | the recovery branch does `steps += 1` before `break` (`:1306-1307`) |
| Collider-set mutation reaches the query pipeline | **HOLDS** | step is incremental with a final refit; stepless dirty frames do a full rebuild (`:1214-1216`); `build_ragdoll` marks dirty |
| Explosion recovery: snapshot precedes every substep, delta bound, parking, all articulations detached | **HOLDS** | dynamic index (#4682); #4687(a)/(c) intact |
| Recovery is visible to queries the same frame (#4687b / #5126) | **FIXED** | `refresh_query_geometry_after_restore` passes `update_incremental(…, true)`; the guard drives a real `step` |
| Pre-explosion containment: clamp at the end of every substep (#5161) | **DRIFTED** | Skipped on every recovery substep (**D2-01**). The DOF cap also caps a free root's linear velocity (**D2-02**). |
| Keyframe-target gate (#5161) | **HOLDS** | `push_kinematic` refuses targets via `accept_keyframe_target` (`sync.rs:1253`); the absolute 1e8 bound keeps the derived AABB about 2600× under the grid |
| Ragdoll seed gate (#5161) | **HOLDS** | all-or-nothing before any insert; the bin-side activator check and the builder backstop are mutually exclusive, so each refusal counts once |
| Fixed-on-fixed broad-phase filter (fe80f4d76) | **HOLDS** | the only production `set_body_type` is still `set_motion_type` → `body_left_fixed` |
| Registration determinism (ad1d53a11) | **HOLDS** | unchanged in window; #4997 guard green |
| Lock ordering (Phase 1 read → release → write) | **HOLDS** | `activate_ragdoll` drops ECS guards before the `PhysicsWorld` write (seed-rejection path included); `/audit-concurrency` owns the rule |
| Phase order collect → push kinematic → buoyancy → step → pull | **HOLDS** | `sync.rs` unchanged apart from the keyframe gate |
| `physics_sync_system` Access declaration | **HOLDS** | `boot/schedule/physics.rs` untouched; `scheduler_access` 24/24 |
| Scale applied exactly once, at the sink | **HOLDS** | `convert.rs` comment-only delta |
| Teardown completeness | **HOLDS for Rapier sets, DRIFTED for side tables** | `rapier_release` 9/9. `remove_body` prunes `body_labels` only. `explosion_offences` and `keyframe_refusals_logged` are covered by SAFE-D3-2026-10-05-01; `articulation_joints` is D2-03. |
| Player physics presence = one capsule; gameplay rays from the eye | **HOLDS** | #5124 |
| Water samplers agree | **DRIFTED** | marker-only case fixed (#5129). Flowing-kind weather damping is missing in buoyancy and in the player sampler (**D5-01**). The swimming marker drift is scaled by fraction (**D5-02**). |
| Diagnostics do not lie | **DRIFTED** | `velocity clamps` is flooded by falling ragdolls (D2-02); `explosive detaches` over-counts (D2-04) |
| PHYSAL doctrine (no solver-side game branch) | **HOLDS** | ninth consecutive pass |

---

## Findings — HIGH

### PHYS-D2-2026-10-05-01: A recovery substep skips `clamp_explosive_velocities`, so every body the restore did not claim carries its exploded velocity into the next frame's integration unclamped
- **Severity**: HIGH
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/world.rs:1295-1314` (the `if restored > 0 { …; break; }` arm and the comment
  above `self.clamp_explosive_velocities()`); compare `:846-878` (`recover_pre_broken_bodies`, non-finite only)
- **Status**: NEW. This is a third hole in the #5161 containment, after #5246's wake-rearm hole.
- **Trigger Conditions**: one substep in which the invalid-solve restore claims at least one body, while another
  dynamic body or articulation ends the same substep with a finite velocity above the cap but has not yet moved
  past `MAX_DYNAMIC_SUBSTEP_DISPLACEMENT`. The natural case is an exploding ragdoll: some links have already
  moved more than 2,048 BU and are restored, while sibling links have exploded only in velocity.
- **Description**: `step` runs the restore after each `pipeline.step`. When `restored > 0` it zeroes the
  accumulator and `break`s, and the clamp never runs. The comment justifies this: "The restore branch above already
  sanitises its bodies (rolled back + slept), so skipping the clamp there loses nothing". That is true only of the
  restored bodies themselves (`RigidBody::sleep` zeroes their velocities).
  - The restore claims a body only if it is non-finite or has moved more than 2,048 BU (`body_needs_recovery`).
  - In rapier 0.22, `VelocitySolver::solve_constraints` integrates positions and *then* runs the stabilization
    pass (`solve_wo_bias`, `solve_restitution_wo_bias`). That pass rewrites velocities that are only integrated on
    the next call. A body can therefore leave a substep with a finite, exploded velocity and a still-sane pose.
    This is exactly the mechanism the `VELOCITY_SANITY_CAP_BU_PER_S` doc describes ("explodes … within ONE
    `pipeline.step` call …; the *next* call's position integration then places its AABB near … the grid
    boundary").
  - Next frame, `recover_pre_broken_bodies` parks only non-finite state. The first `pipeline.step` then integrates
    the unclamped velocity and runs `detect_collisions` on the result inside the same call.
  - `remove_multibody_articulations` detaches the articulations that contain a restored body. Their un-restored
    links become free bodies that keep their exploded velocities. Articulations with no restored link keep their
    exploded generalized velocities, which forward kinematics integrates.
- **Evidence**: scratch probe, `PhysicsWorld::step` through the public API, two balls, indexed via
  `set_motion_type(Dynamic)`:
  ```
  [control]      steps=1 B |v|=20000 clamps=1 recovery=(0, 0, 0)
  [with restore] steps=1 B |v|=100000 sleeping=false clamps=0 recovery=(1, 1, 0) B.x=1667
  [next frame]   steps=1 B moved 1667 BU this frame (cap-bounded would be <= 333); clamps=1
  ```
  - B (1e5 BU/s) sits in the clamp-only window.
  - A (1e9 BU/s, unrelated) is restored in the same substep.
  - B's velocity survives the recovery substep untouched, and the next frame integrates it at full speed before
    the clamp runs.
  - With the stabilization-pass explosion magnitudes recorded in #5161 (|v| ≈ 7.7e15 BU/s), that next-frame
    integration is the multi-SAP boundary crossing (≈2.68e11) that panics `sap_axis.rs`. The panic itself was not
    reproduced: the public API cannot stage a stabilization-only explosion.
- **Impact**: the class #5161 and #5246 were filed to close returns as a process panic. It needs a restore and a
  velocity explosion in the same substep, which is the common shape of a ragdoll explosion rather than an exotic
  one. The skill's Dim 2 text says the clamp "runs at the end of every substep", which is false on exactly the
  substeps where an explosion is in progress.
- **Related**: #5161, #5246 (closed); #4687 (restore detach); #4772 (the FO3 restore path fires the restore);
  PHYS-D2-2026-10-05-02.
- **Suggested Fix**: run `clamp_explosive_velocities()` in the restore arm too, before the `break`. It is
  idempotent on the restored bodies, which are already zeroed and asleep. Then correct the comment. Pin the fix
  with the probe's two-body shape as a `world.rs` test: one body restored and one in the clamp window in the same
  substep, then assert the second body is capped and counted.

---

## Findings — MEDIUM

### PHYS-D2-2026-10-05-02: The articulation-DOF clamp caps a dynamic ragdoll root's free-joint LINEAR velocity at 100 BU/s, so every ragdoll falls at about 1.4 m/s and floods the explosion telemetry
- **Severity**: MEDIUM. The behaviour is wrong in every game with ragdolls, and the containment's diagnostics are
  corrupted. No crash.
- **Dimension**: Step & Sync (user-visible in Ragdoll & Constraint Seam; diagnostics in Queries & Diagnostics)
- **Location**: `crates/physics/src/world.rs:91-99` (`ARTICULATION_DOF_SANITY_CAP` and its doc),
  `:1071-1097` (the DOF loop); `crates/physics/src/ragdoll.rs:383-398` (every ragdoll body
  `RigidBodyBuilder::dynamic()`)
- **Status**: NEW (introduced by `5ae7f8ad4`, #5161)
- **Trigger Conditions**: any activated ragdoll whose root's speed along one world axis exceeds 100 BU/s. That
  includes any fall longer than about 0.15 s (gravity is 686.7 BU/s²), a corpse dropping off a ledge, and an actor
  killed mid-air.
- **Description**: the cap's doc lists the DOFs it is meant for: "rad/s for the ragdoll/hinge joints' angular axes,
  BU/s for the prismatic rail; every authored class moves far slower than this". The doc omits the root.
  - `build_ragdoll` makes every body dynamic, and nothing in production pins the root. A grep of
    `byroredux/src/ragdoll.rs` finds no `set_body_type` or `Fixed`.
  - Rapier's `Multibody::with_root` / `update_root_type` therefore gives the multibody a `MultibodyJoint::free`
    root with `SPATIAL_DIM` = 6 DOFs at the front of `generalized_velocity`. The first three are the root's linear
    velocity in BU/s.
  - The loop clamps every entry to ±100 regardless of which joint owns it.
- **Evidence**: scratch probe. A production `build_ragdoll` with three ball links, ragdoll joints and
  `ContactConfig::DEFAULT`, seeded at y = 10,000 BU with no floor, run for 120 frames:
  ```
  [free body]  2 s fall: dropped 1376 BU, |v|=1373 BU/s, clamps=0
  [ragdoll] frame  10: root dropped 10 BU,  root DOFs[0..6]=[~0, -100.0, 0, 0, 0, ~0], clamps=2
  [ragdoll] frame  30: root dropped 45 BU,  root DOFs[0..6]=[~0, -100.0, 0, 0, 0, ~0], clamps=22
  [ragdoll] frame 120: root dropped 204 BU, root DOFs[0..6]=[~0, -100.0, 0, 0, 0, ~0], clamps=112
  free-fall reference: 2 s => 1373 BU, v=1373 BU/s
  ```
  The existing guards do not see this:
  - `exploding_articulation_dofs_are_capped_before_forward_kinematics` asserts only an upper bound.
  - `ragdoll_chain_swings_but_stays_jointed` pins the root `Fixed`, so its multibody has no free joint.
  - No test asserts that a ragdoll falls at gravity.
- **Impact**:
  - Every ragdoll in every game descends at 100 BU/s or less per axis. A corpse falling 1,000 BU takes about 10 s
    instead of about 1.7 s.
  - Every substep of that descent adds one to `velocity_clamps_total`, the `phys.stats` "velocity clamps" line
    that #5161/#5246 rely on, and logs `warn` "clamped N exploding articulation DOF velocities". That is about 60
    lines per second per falling corpse, and a normal death now looks like a solver explosion in the telemetry.
  - Ragdoll seeding passes no velocity from the live animation, so horizontal momentum is not affected today.
- **Related**: #5161, PHYS-D2-2026-10-05-01, PHYS-D2-2026-10-05-04.
- **Suggested Fix**: skip the root free joint's six DOFs. A dynamic-root multibody's first `SPATIAL_DIM` entries
  belong to `links[0]`; read `link(0).joint().ndofs()` rather than hard-coding 6. Those DOFs are already covered by
  the body-level clamp at 20k BU/s and 100 rad/s, because the root is a dynamic body in `dynamic_bodies`.
  Alternatively, clamp the root's linear DOFs at `VELOCITY_SANITY_CAP_BU_PER_S`. Add a free-fall test on a
  `build_ragdoll` articulation that asserts a gravity-rate drop and `velocity_clamps_total() == 0`.

### PHYS-D5-2026-10-05-01: #5245's flowing-water weather damping reached the renderer and `submersion_system` but not dynamic buoyancy or `player_water_state`, so physics crests on rivers diverge from the visible surface
- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy
- **Location**:
  - `crates/physics/src/water.rs:650` (scroll sampled), `:888-897` (buoyancy crest sample, raw scroll)
  - `byroredux/src/systems/character.rs:1205-1235` (`player_water_state`, raw scroll)
  - Compare `byroredux/src/systems/water.rs:212-230` and `byroredux/src/render/water.rs:236-247` (damped)
  - Constant: `crates/core/src/ecs/components/water.rs:150-163`
- **Status**: NEW (regression of the #3207 phase-coherence contract, introduced by `37cc637e9` #5245/#4929)
- **Trigger Conditions**: a `WaterPlane` whose `kind.is_flowing()` (River, Rapids, Waterfall), with a non-zero
  `WindField` gust, and a dynamic body or the player near its surface.
- **Description**: `FLOWING_WATER_WEATHER_TRANSPORT` (0.35) scales the weather scroll on flowing kinds. Its doc
  says it is "Applied identically by the renderer upload and the CPU crest sampler so the #3207 phase-coherence
  contract holds".
  - `grep -rn "FLOWING_WATER_WEATHER_TRANSPORT\|is_flowing()"` finds it in only two places: `render/water.rs` and
    `systems/water.rs` (camera submersion).
  - `apply_buoyancy_with_scratch` and `player_water_state` call `authored_wave_height_with_weather` with the raw
    `weather_wave_adjustment` scroll.
  - That function adds the weather scroll to `scroll_a`/`scroll_b` *before* deriving `dir_a`/`dir_b` and
    `rate_a`/`rate_b` (`water.rs:362-379`). The two paths therefore produce different crest directions and speeds,
    not just different phases.
  - On the #5245 White River fixture, the undamped weather is about 0.33 UV/s against a downstream flow term of
    about 0.13. The physics crests travel with the wind, while the rendered crests, and the waterline the camera's
    underwater state is tested against, travel downstream.
- **Evidence**:
  ```rust
  // byroredux/src/systems/water.rs:216 — damped (camera submersion)
  let weather_scroll = if plane.kind.is_flowing() { [ws[0] * WaterKind::FLOWING_WATER_WEATHER_TRANSPORT, …] } else { ws };
  // crates/physics/src/water.rs:890 — buoyancy: `weather_scroll` passed straight through
  // byroredux/src/systems/character.rs:1228 — player: `weather_scroll` passed straight through
  ```
- **Impact**:
  - Floating debris and ragdolls on rivers bob against a crest pattern the player cannot see.
  - The player's swimlevel and `WaterContact` depth are computed against a different crest than the camera's
    `SubmersionState`. Near the waterline, the swim state and the underwater tint and audio can disagree by up to
    one wave amplitude × `wind_wave_scale`.
  - The skill's rule, "Every water input added to one [sampler] must exist in the other", is violated for a whole
    class of water.
- **Related**: #5245 / #4929 (closed); #3207; PHYS-D5-2026-09-29-01 / #5129 (the previous sampler-parity
  instance). `/audit-exterior` owns the policy constant; the sampler parity is reported here, as the skill
  requires.
- **Suggested Fix**: move the damping into one helper, for example a `WaterKind`-aware
  `weather_scroll_for(kind, scroll)` next to `weather_wave_adjustment`. Call it from all four samplers: renderer,
  submersion, buoyancy (via `WaterSurface`, which would need the plane's `kind`) and the player. Add a parity test
  that samples all three CPU paths on a River plane with wind.

---

## Findings — LOW

### PHYS-D2-2026-10-05-03: `PhysicsWorld::articulation_joints` is never pruned, and `clamp_explosive_velocities` walks it every substep
- **Severity**: LOW
- **Dimension**: Step & Sync (cost)
- **Location**: `crates/physics/src/world.rs:348-353` (field doc), `:1075`; `crates/physics/src/ragdoll.rs:529`
  (push), `:863-867` (`remove_ragdoll` does not prune)
- **Status**: NEW. This is a sibling of SAFE-D3-2026-10-05-01, which covers `explosion_offences` and
  `keyframe_refusals_logged` but not this vector.
- **Description**: every `build_ragdoll` pushes one handle per joint, about 17 for a humanoid. The field doc
  claims "Stale handles … are skipped, so the vector never needs sweeping". The handles are skipped, but they are
  never removed. Nothing removes entries: not `remove_ragdoll`, not `remove_body`, and not the restore or detach
  paths. Every substep of a stepping frame pays one `MultibodyJointSet::get_mut` miss per lifetime ragdoll joint.
- **Impact**: per-substep cost grows linearly over the session with the number of corpses ever activated. In
  absolute terms it stays small (thousands of misses after hundreds of deaths), and memory is a few bytes per
  joint.
- **Suggested Fix**: `retain` live joints at the top of `clamp_explosive_velocities`, the same way the
  `dynamic_bodies` compaction does at the snapshot. Alternatively, drop a ragdoll's `joints` in `remove_ragdoll`.
  Fix it together with SAFE-D3-2026-10-05-01.

### PHYS-D2-2026-10-05-04: `clamp_explosive_velocities` still documents pre-#5246 semantics, and its third-offence arm counts a "detach" on every later burst, including for bodies with no articulation
- **Severity**: LOW
- **Dimension**: Step & Sync / Queries & Diagnostics
- **Location**: `crates/physics/src/world.rs:949-955` (fn doc), `:1032-1048` (`*offences >= 3` arm)
- **Status**: NEW
- **Description**: the function doc still says "A body clamped on consecutive substeps is parked; a clean substep
  returns it to watch-list absence". That describes the set-cleared-on-clean-substeps design that #5246 removed in
  favour of a lifetime count, and the field doc and constant doc now say the opposite. Separately:
  - The `>= 3` arm calls `remove_multibody_articulations`, adds one to `explosive_detaches_total` and logs "detached
    … articulation" on the third burst *and every burst after it*.
  - It does this for any dynamic body, including clutter or a link whose articulation was already removed, where
    the removal is a no-op.
  - The guard `the_third_burst_detaches_the_articulation` drives a plain dynamic body with no joint and asserts
    `explosive_detaches_total() == 1`. It therefore pins a detach that detached nothing.
- **Impact**: a gate or operator reading `phys.stats` "explosive detaches" counts bursts past the third, not rigs
  detached. A reader trusting the fn doc misreads the escalation ladder.
- **Suggested Fix**: rewrite the doc to state the lifetime ladder. Count a detach only when
  `multibody_joints.rigid_body_link(handle).is_some()` before the removal, or only on the transition to offence 3.
  Make the test assert on a real articulation.

### PHYS-D5-2026-10-05-02: The swimming player's drift scales the placed current by submerged fraction; the dynamic path applies it at 1.0
- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**: `byroredux/src/systems/character.rs:1120-1137` (`player_current_drift`), `:1255-1275` (#4691
  vector composition); compare `crates/physics/src/water.rs:1050-1079` (`current_force(…, 1.0, …)`)
- **Status**: NEW. This is the remainder of the #4691 / #5129 parity rule. The marker-only case is fixed.
- **Trigger Conditions**: the player swims, meaning depth is past `SWIM_HEIGHT_SCALE`, inside a `WaterCurrentVolume`
  box.
- **Description**: the two paths scale the marker differently:
  - The dynamic path applies plane drag × fraction plus marker drag × **1.0**.
  - While not swimming, the player applies the marker × 1.0, which matches.
  - Once swimming, `player_water_state` composes plane + marker into one vector, and `player_current_drift`
    scales *the whole vector* by `state.fraction`. At the swimlevel threshold the fraction is about 0.675
    ((0.35h + h) / 2h), so stepping into the swim state drops the marker's drift by about 32%. It returns to
    full strength only when the player is fully submerged.
  - The guard `swimming_drift_uses_the_composed_column_flow_only` pins the composite × fraction form.

  The player ignoring plane flow while wading is a different matter: it is documented design (the doc at
  `:1111-1113`) and is not filed.
- **Impact**: a small, discontinuous change in drift at the walk→swim boundary inside rapids. A barrel next to the
  swimmer feels the full marker current.
- **Related**: #4691, #5129 (closed); PHYS-D5-2026-10-05-01.
- **Suggested Fix**: compose `plane × fraction + marker × 1.0` in the swim arm, mirroring the dynamic path. Or
  record the intended scaling in `docs/engine/watal.md` and keep the test.

---

## Existing (matched; cited, not re-counted)

### SAFE-D3-2026-10-05-01 (today's `/audit-safety`): `remove_body` prunes `body_labels` but not `explosion_offences` / `keyframe_refusals_logged`
- **Status**: cross-referenced, not re-filed. Re-confirmed at `world.rs:536-560`. Two consequences are relevant
  here:
  - `explosion_offences` is keyed by the full handle, so a stale entry cannot alias a new body.
  - The leak is bounded per removed body.

  The physics-side companion is D2-03 (`articulation_joints`). All three fixes belong in one change.

### #4134: Character-controller / ground-probe capsule uses a floor-only `.max(1e-3)`
- **Status**: Existing, unchanged. Still the one site, `character_capsule` at `world.rs:2076-2078`.

### #4772: FO3 copied-save restore's first ragdoll solve jumps the root to about 1e12 BU
- **Status**: Existing, unchanged in code. This is the live path that fires the invalid-solve restore, and so it
  is the most likely real-content trigger for PHYS-D2-2026-10-05-01.

---

## Confirmed-Fixed Register (re-derived from code this pass)

| Issue | Fixed by | Re-derivation |
|---|---|---|
| #5126 (D2-29-01, refit no-op) | `ea04f2689` | `update_incremental(…, true)` at `world.rs:832-833`. The guard `restored_pose_is_visible_to_ray_queries_same_frame` drives a real `step` recovery. |
| #5127 (D2-29-02, counter units) | `ea04f2689` | `bodies_restored_last_frame` field and accessor doc; `phys.stats` label updated; `multi_body_recovery_counts_one_event_and_every_restored_body` |
| #5130 (D2-29-03, doc splice) | `60cd7028c` | the #3266 doc sits on `physics_diagnostics_resolve_forms_after_storage_guards_drop` again |
| #5124 (D4-29-01, third-person ray origin) | `69157c7cd` | `player_camera_boom` drives both the camera placement and `camera_ray`; the guard covers third person, first person and fly cam |
| #5129 (D5-29-01, marker-only current) | `1512c5a12` | `placed_current_flow_at` resolved before the plane loop; non-swimming drift uses the marker at 1.0 (remainder: D5-02) |
| #5161 (keyframe and seed insanity) | `44f7bab55`, `5ae7f8ad4` | `accept_keyframe_target` in `push_kinematic`; `seed_pose_is_sane` in both activator and builder; labels; counters in `phys.stats` (containment holes: D2-01/D2-02) |
| #5246 (NaN pass-through, offence reset) | `e8de9f8c8` | non-finite velocity zeroed (`nan_velocities_are_zeroed_on_the_first_clamp`); lifetime offence map (`a_second_burst_parks_even_after_a_clean_substep`) |

## Known-Open Register (what this pass changed)

- **#4134** OPEN: unchanged, one site.
- **#4772** OPEN: unchanged. It is now the natural trigger for D2-01.
- **SAFE-D3-2026-10-05-01**: cross-referenced. D2-03 extends it to `articulation_joints`.
- Upstream mass-inverted rig instability (palms heavier than the spine, authored restitution 0.8) remains the root
  cause behind #5161/#5246 and has no issue of its own. D2-01 matters most for exactly these rigs, because
  restitution drives the stabilization-pass velocity explosions.
- Ball-and-socket / stiff-spring / chain constraints: still decoded and declined by design. No occupancy census
  was run, so nothing was filed.
- WATAL water-walking and freezing: still unbuilt per `docs/engine/watal.md`; not filed.
- Third-person camera collision (boom unswept): still a documented slice follow-up. #5124 removed its gameplay
  consequence.
- Skill drift, to fold into the next `/audit-sync`:
  - Dim 2 says `clamp_explosive_velocities` "runs at the end of every substep". It does not on recovery substeps
    (D2-01).
  - The DOF-cap description does not mention the free root joint (D2-02).

## Cross-Audit Deduplication

| Topic | Owner | Note |
|---|---|---|
| `remove_body` side-table growth | `/audit-safety` | SAFE-D3-2026-10-05-01; D2-03 is the physics-side sibling |
| `FLOWING_WATER_WEATHER_TRANSPORT` policy value | `/audit-exterior` | D5-01 covers only the sampler parity. AUDIT_RENDERER_2026-10-05 routes the constant to exterior and raises no parity finding. |
| Corridor-cast melee behaviour (12 BU ball may meet the floor before a very low target when pitched down) | `/audit-gameplay` Dim 4 | not filed; combat behaviour, speculative |
| Lock order of `player_derived_stats_system` (`CharacterRuleset` → `ActorValues`) | `/audit-concurrency` / `/audit-character` | documented canonical direction (#3441); not re-checked here |
| Buoyancy wave sample uses the body origin `pos` while `surface_y_at` uses `reference_point` (`water.rs:884` vs `:892`) | this audit | pre-existing (`06fa77e387`), outside the delta; noted, not filed. Fix it with D5-01 if that sampler is touched. |

## Verification notes

- Each finding was re-read at HEAD, and a disproof was attempted:
  - **D2-01**: read rapier 0.22 `VelocitySolver::solve_constraints` (position integration *before*
    stabilization), `PhysicsPipeline::step` ordering (`advance_to_final_positions` then `detect_collisions`),
    `RigidBody::sleep` (zeroes velocities) and `remove_multibody_articulations` (detaches the whole multibody;
    links keep their velocities). Measured with the probe.
  - **D2-02**: read `Multibody::with_root` / `update_root_type` (free root joint, `SPATIAL_DIM` DOFs first).
    Confirmed that no production root pin exists. Measured with the probe on the production builder.
  - **D5-01**: grepped every `authored_wave_height_with_weather` call site and every user of the constant.
  - **D5-02**: read both drift paths and the swimlevel constant (`SWIM_HEIGHT_SCALE = 0.35`).
  - **D2-03 / D2-04**: read directly.
- Dedup:
  - Open issues come from `/tmp/audit/issues.json` (97 open, up to #5242); physics-keyword titles were reviewed.
  - `gh` state was checked for #5161 and #5246 (both closed).
  - Closed-issue searches: "articulation DOF", "free joint", "ragdoll fall slow", "articulation_joints",
    "FLOWING_WATER_WEATHER_TRANSPORT", "marker current fraction swim", "clamp skipped recovery". None matches a
    finding.
  - Sibling 2026-10-0x reports were grepped for the key symbols. The only touches are SAFE-D3-01 (cited) and the
    renderer's routing note.

Next step: `/audit-publish docs/audits/AUDIT_PHYSICS_2026-10-05.md`. Suggested labels:

- `physics` on all findings;
- `water` on D5-01 and D5-02;
- `test-gap` on D2-01 and D2-02;
- `doc-rot` on D2-04;
- `performance` on D2-03.
