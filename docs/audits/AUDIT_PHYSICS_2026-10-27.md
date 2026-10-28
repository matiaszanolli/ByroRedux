**HEAD**: `fa6be4a6b` · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-21.md` (HEAD `4a3e89d56`) · **Audited**: Dim 2 (Step & Sync: #6324 / #6325 deltas), Dim 3 (Ragdoll & Constraint Seam: #6255 / #6134 / #6326 / #6303 / #6302 / #6160 / #6252 / #6185, plus the PushActorAway knockdown path), Dim 4 (Character & NPC Controller: #5155 fall_blocked, VATS freeze), Dim 5 (WATAL sink: bed field, swell, river current, #6135), Dim 6 (Queries & Diagnostics: census shape label) · **Unchanged since baseline (skimmed)**: Dim 1 (Shape Translation: no commits to `convert.rs` / `config.rs`; `crates/nif/src/import/collision/mod.rs` only got the #6144 `blocks/dispatch.rs` path rename in its coverage scan)

# PHYSAL / Physics Audit — 2026-10-27

Suite run (`--preset comprehensive`). Read-only on code, solo (no sub-agents), no engine launch. 395 commits since the baseline. 40 of them touch physics scope. The bulk is the WATAL bed-field / swell / river-current wave (7dd294ee3, 8647c1128, 4427ce44f) and the ragdoll follow-ups from the 2026-10-21 audit (#6324, #6325, #6326, #6255, #6134).

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 3 |

- **PHYS-D3-2026-10-27-01 (MEDIUM)**: `PushActorAway` knocks the actor down but never pushes it. `knockdown.rs` gives every ragdoll body its push through `PhysicsWorld::apply_impulse`. That writes the rigid body's `linvel`. For a multibody link, rapier 0.36 overwrites that velocity from the multibody's generalized velocities before each solve. A probe with the same impulse measured a 150 BU slide on a free pair and **0.0** on a jointed pair. `apply_impulse` still returns `true`.
- **PHYS-D5-2026-10-27-01 (LOW)**: on a river with a bed field, a swimming player drifts with the plane's uniform current. Floating bodies, and the `WaterContact` they publish, use `WaterFlow::over_bed`: slower over the shallows and turned along the bank. This breaks the two-samplers rule (same class as #4691 and #5357).
- **PHYS-D5-2026-10-27-02 (LOW)**: `waves_require_contact_rescan` only looks at the ripple amplitude. The swell is ignored, both for arming the rescan and for its hysteresis band. On a wind-driven plane whose authored ripple is zero, a float that has gone to sleep is never rescanned, so it cannot re-wake when the swell grows.
- **PHYS-D3-2026-10-27-02 (LOW)**: `parks_authored_pose` adds a `GameKind::Fallout3NV` branch on the ragdoll side. `docs/engine/physal.md` §3 still says the XRGD path is "skeleton-keyed, not a `GameKind` branch". Its entry-mapping sentence is also stale for FO4 (bone-id map) and FO3/FNV (root body placed at the skeleton root).

**PHYSAL verdict.** `crates/physics/**` and `byroredux/src/ragdoll.rs` still contain no `GameKind` / `bsver` / version branch. The three named seams (constraint CInfo decode, `havok_scale_for`, collision-object-kind dispatch) are still the only per-game branches in the solver path. The exception is new and sits next to the solver: `byroredux/src/ragdoll_pose.rs::parks_authored_pose` decides corpse-pose admission by `GameKind` (PHYS-D3-2026-10-27-02). That is admission policy, not translation, and it should move to the record boundary or be documented. FO4's articulation still enters through the constraint seam (`decode_np_ragdoll` produces the same CInfo records). #6302 now declines FO76/Starfield tagfile blobs at that same seam rather than reporting them as corrupt. #6160 was checked byte for byte: `LimitedHingeCInfo::parse_fo3` now reads through `parse_hinge_fo3`, the same eight Vector4s in the same order, then the three limits. Collision data was traced statically through the classic FO3/FNV/Skyrim bone path, the FO4 decoded path (`extract_np_ragdoll` → `node_world` via `compose_transforms` + `any_ni_node`, #6303), and the FO3/FNV XRGD pose path (#6252).

**Test runs (this pass, repo `target/`)**:
- `cargo test -p byroredux-physics`: 270 passed.
- `cargo test -p byroredux --bin byroredux -- <filter>`: `ragdoll` 61 passed, 12 ignored; `character` 66 passed; `locomotion` 17 passed; `water` 171 passed, 4 ignored.
- **Data gate**: `-- --ignored ragdoll::installed_tests skeever_ragdoll`, run with `BYROREDUX_FO3_DATA`, `BYROREDUX_FO4_DATA` and `BYROREDUX_SKYRIMSE_DATA` set: 9 of 9 pass in 17.6 s. That covers the six FO3 environments, the FO4 six-rig bind-pose free fall, the MedTek XRGD-posed ghoul, and the #5161 skeever replay, now pinned as `skeever_ragdoll_survives_six_hundred_floor_ticks`.
- **Probe**: a scratch crate in the scratchpad (built into the repo `target/` with the repo `Cargo.lock`, rapier 0.36.1) measured `PhysicsWorld::apply_impulse` on free bodies against multibody links. Results are under PHYS-D3-2026-10-27-01.

## Solver Invariant Matrix

| Invariant | Verdict | Evidence |
|---|---|---|
| Fixed step + accumulator clamp | Verified (unchanged) | `world/mod.rs` `step` / `run_substep` have no delta besides #6324/#6325. VATS freeze (818eba7f4) feeds `frame_dt = 0` through `app_events.rs` `world_time_scale`, which is the zero-dt path the boot tick already covers. |
| Wake discipline / spawn re-sleep | Verified, prior gap closed | #6324 moves `set_linear_velocity` onto `PhysicsWorld`: a nonzero velocity calls `forget_spawn_sleep` and `wake`, a zero velocity does neither. The `sync.rs` helper delegates to it. No other production `set_linvel` / `set_body_type` exists outside the `PhysicsWorld` methods and recovery (checked by grep). Guards: `explicit_velocity_keeps_newcomer_awake_through_first_step`, `zero_velocity_leaves_newcomer_on_the_settle_list`. |
| Contact-force subscription | Verified, prior gap closed | #6325: `contact_force_active_events` is now the one rule. `register_newcomers` keys on the Rapier body type, `build_ragdoll` always passes dynamic, the toggle walks every collider by its live parent type, and `set_motion_type` retags on every flip. Guard: `contact_force_subscription_follows_the_body_type_not_history`. |
| Lock order | Not re-derived here | VATS `vats_scan` / `visible_fraction` physics-lock ordering = Dup of AUDIT_ECS_2026-10-27 ECS-02 / AUDIT_CONCURRENCY_2026-10-27 CONC-01. `retire_body_rows` takes `RigidBodyData` then `CollisionShape` then `RetiredBodyRows` serially, one guard at a time. |
| Phase order (collect → kinematic → buoyancy → step → pull) | Verified (unchanged) | `boot/schedule/physics.rs` only gained the `WaterBedField` read (a9cd22d71). |
| Explosion recovery | Verified (unchanged) | `recovery.rs` has no commits. FO3/FO4/skeever real-data gate green. |
| Teardown / retired rows | Verified | #6134/#6326: activation parks the bones' `RigidBodyData` and `CollisionShape` on the skeleton root's `RetiredBodyRows`, which is registered in `boot/world.rs:336`. `stand_up` (`systems/revive.rs:76-92`) restores shapes first, then rows, and only fills slots that are empty. #6255: activation stamps `ActorColliderOwner(Parent(skeleton_root))` on every body that lacks one, which matches the spawn-side `actor_root` owner. |
| Ragdoll impulse API | **Drifted** | PHYS-D3-2026-10-27-01: `apply_impulse` is a silent no-op on multibody links. |
| Two water samplers agree | **Drifted** | PHYS-D5-2026-10-27-01 (river current over the bed). Wave height agrees: every production CPU crest sampler passes the plane's bed. |

## Findings

### PHYS-D3-2026-10-27-01: PushActorAway's push is discarded — `apply_impulse` on a multibody link is overwritten by rapier's solver
- **Severity**: MEDIUM
- **Dimension**: Ragdoll & Constraint Seam
- **Location**: `byroredux/src/systems/knockdown.rs:107-115` (`knock_down`); `crates/physics/src/world/mod.rs:1185-1199` (`PhysicsWorld::apply_impulse`)
- **Status**: NEW (knockdown.rs landed 8d32b3fe9, 2026-10-18; the 2026-10-21 audit did not cover it)
- **Description**: `knock_down` activates the ragdoll and then gives every body `direction * speed * mass` through `PhysicsWorld::apply_impulse`. That calls `RigidBody::apply_impulse`, which adds `impulse * inv_mass` to `vels.linvel` (rapier 0.36.1 `rigid_body.rs:1451-1459`). Every ragdoll body is a multibody link, and that includes the dynamic root through its free joint. At the start of each island solve, rapier rebuilds every link's `vels` from the multibody's generalized velocities (`staged_island_solver/helpers.rs:108` → `Multibody::update_velocities`, `multibody.rs:735-766`). Nothing ever reads `rb.vels` back into those generalized velocities, so the impulse is lost. The physics crate already records this fact in a test comment (`world/mod.rs:2429-2431`: "a multibody LINK's velocity is solver-owned — `set_linvel` on it is replaced by forward kinematics"), and #5540 already routes link damping through force/torque for the same reason. `apply_impulse` has no such route, and it returns `true`.
- **Evidence**: Probe (scratch crate, rapier 0.36.1, the repo's `PhysicsWorld`). Two dynamic balls get `apply_impulse(+X * 300 * mass)` each, then 30 steps at 1/60 s:
  - Free pair: `dx = (150.0, 150.0)`.
  - The same pair joined by a multibody `SphericalJoint`: `dx = (0.0, 0.0)`.
  - Both fall the same distance under gravity (y 100 → 7.6), so the step does run.
- **Impact**: Every ObScript `PushActorAway` (FO3/FNV quests and traps; the GECK says "Fred will slide past Buddy") collapses the actor in place instead of throwing it. A negative force (a pull) also does nothing. The wake side effect still happens, so the bug is invisible in logs. `knock_down` even logs "knocked down … at {speed} u/s". Any future caller that pushes a ragdoll this way (impact or explosion knockback on a corpse) inherits the same silent no-op. The knockdown tests only cover direction maths and the refusal paths; none asserts that a body moved.
- **Trigger Conditions**: A `PushActorAway` on an actor whose ragdoll activates (any actor with a `RagdollTemplate`, i.e. every humanoid).
- **Related**: #5540 (link damping as force/torque), #6230 (knockdown builds the ragdoll first).
- **Suggested Fix**: Give `PhysicsWorld` a link-aware impulse. For a multibody link, convert the impulse to a force applied for one substep (`impulse / PHYSICS_DT`, then cleared), or write the root's free-joint DOFs (`FREE_ROOT_DOFS`, linear first) through the multibody's generalized velocity. Make `apply_impulse` return `false` or delegate on links. Add a test that a jointed ragdoll actually moves.

### PHYS-D5-2026-10-27-01: The swimming player's river current ignores the bed; dynamic bodies and their `WaterContact` use `WaterFlow::over_bed`
- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**: `byroredux/src/systems/character.rs:1351` (`player_water_state` takes the plane `WaterFlow` verbatim), `:1205-1226` (`player_current_drift`), `:1260-1265` (stale doc); vs `crates/physics/src/water.rs:1236-1243` (`local_flow`), `:1335` (current force), `:1378` (published `flow: local_flow`)
- **Status**: NEW (introduced by 8647c1128, 2026-10-26)
- **Description**: 8647c1128 made the dynamic path evaluate a River/Rapids current at the body's position over the plane's `WaterBedField` (`flow.over_bed(sample)`: down to `WATER_RIVER_SPEED_MIN` 0.35 at the waterline, turned along the bank). It uses that local flow both for the current force and for the published `WaterContact.flow`. The player sampler was not changed. `player_water_state` still copies the plane's `WaterFlow`, and `player_current_drift` drifts the swimmer on it at the plane's full speed and authored heading. The `PlayerWaterState` doc still says the dynamic contact write publishes `flow: s.flow`. The skill's two-samplers rule ("every water input added to one must exist in the other") is broken. Same class as #4691 and #5357.
- **Evidence**: `water.rs:1236` `let local_flow = s.flow.map(|flow| match (s.kind, s.bed.as_ref()) { (WaterKind::River | WaterKind::Rapids, Some(bed)) => bed.sample(..).map_or(flow, |sample| flow.over_bed(sample)), _ => flow })`. On the player side, `character.rs:1351` `let flow = flow_q.as_ref().and_then(|q| q.get(entity).copied());` has no bed lookup, even though `bed_q` is already in scope two dozen lines up for the wave height.
- **Impact**: In a river's shallows or near a bank, a swimming player is pushed up to ~3x harder than the barrel beside it, and in the authored direction rather than along the bank. The published player `WaterContact.flow` disagrees with every other wet body's in the same column, which matters for FX/audio consumers. Small: the player drift is `speed * 0.35 * fraction`.
- **Trigger Conditions**: An exterior River/Rapids plane with a LAND-derived bed field (interior, LOD and placed-mesh water carry none) and a swimming player.
- **Related**: #4691, #5357, #5354 (same rule for wave inputs).
- **Suggested Fix**: In `player_water_state`, apply the same `(River | Rapids, Some(bed)) => flow.over_bed(bed.sample(pos.x, pos.z))` to the stored `flow`. Better, factor one `local_current(kind, flow, bed, point)` helper into `crates/physics/src/water.rs` and call it from both sites. Fix the struct doc.

### PHYS-D5-2026-10-27-02: `waves_require_contact_rescan` keys on the ripple amplitude only; a swell-only wind-driven plane never re-arms the scan
- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**: `crates/physics/src/water.rs:754-772` (gate), `:900` (use), `:922-935` (quiesced fast path)
- **Status**: NEW. The skill asked for this check explicitly; it fails.
- **Description**: The gate takes `max(material.wave_amplitude)` over the surfaces and returns `false` below 1e-4. The same value sizes the waterline hysteresis band. Since 7dd294ee3, a `wind_driven` plane also carries a travelling swell (`WaterMaterial::swell`, 1.5 BU in calm up to 28 BU in a gale). `authored_wave_height_over_bed` adds the swell whatever the ripple amplitude ("a plane with none still rolls in the wind", `water.rs:531-532`). The gate does not count it. When every surface in range has a zero authored ripple, the quiesced fast path (`active_island_counts().0 == 0 && !pending_wake && !had_newcomers && !waves_require_rescan`) skips the scan entirely. The #5561 settled-depth re-wake lives inside that scan, so a sleeping float is never woken by the swell. Can a float sleep on a calm swell? At the 1.5 BU floor, the surface heave is about A·ω ≈ 1.5 × 2.08 ≈ 3 BU/s. Rapier's linear sleep threshold scales with `length_unit = BU_PER_METER`: 0.4 m/s ≈ 28 BU/s. So such a float can fall asleep, and the `w2-dynamic-water.sh` contract allows that ("on calm water they settle and sleep"). After that, it stays frozen while the wind raises the swell. Separately, when a ripple is authored but smaller than the swell, the band `max_amplitude + WATERLINE_HYSTERESIS` is too narrow for contacts the swell reaches.
- **Evidence**: `water.rs:755-761` reads only `surface.material.wave_amplitude`. `WaterMaterial::swell` (`crates/core/src/ecs/components/water.rs:764-784`) depends only on `wind_driven` and the weather scroll. `wave_amplitude` is copied straight from WATR (`env_translate/water.rs:269`). The only gate test is `default_authored_waves_only_rescan_nearby_live_contacts`, which uses the non-zero default ripple and no swell.
- **Impact**: A float on a calm lake whose WATR authors no ripple stays parked through a wind change. Content-dependent. The size of the affected set (WATR records with zero wave amplitude on Calm, or still-River, kinds) was not measured in this pass.
- **Trigger Conditions**: Every water surface in range has `wave_amplitude ≤ 1e-4`, at least one is `wind_driven`, and a dynamic float has gone to sleep on it.
- **Related**: #3122 / #3135 (why the gate exists), #5561 (settled-depth re-wake).
- **Suggested Fix**: Fold the swell's current amplitude (`material.swell(weather_scroll).map(|s| s.amplitude)`, capped by `WATER_SWELL_AMPLITUDE_MAX * gain`) into `max_amplitude`. Pass the weather scroll in. Add a test with `wave_amplitude: 0.0, wind_driven: true` and a sleeping submerged contact.

### PHYS-D3-2026-10-27-02: `parks_authored_pose` adds a `GameKind` branch on the ragdoll side; `physal.md` §3 still calls the XRGD path skeleton-keyed
- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam (PHYSAL doctrine)
- **Location**: `byroredux/src/ragdoll_pose.rs:571-578`; caller `byroredux/src/cell_loader/references/mod.rs:869-874`; doc `docs/engine/physal.md:225-232`
- **Status**: NEW (107cc1d79, #6252)
- **Description**: `parks_authored_pose(placement_starts_dead, base_dead, game, pose)` parks a pose for `placement_starts_dead || (base_dead && game == GameKind::Fallout3NV)`. The base flag is already translated per game at the record boundary (`base_actor_starts_dead` in `crates/plugin/src/esm/records/dispatch_actor.rs`, plus #5005's FO3/FNV DATA-health rule). So the runtime branch exists only to keep Oblivion's dead-base poses out, because Oblivion's rig is unverified. Rig verification is otherwise skeleton-keyed: `is_skyrim_posable_ragdoll`, `is_fo4_posable_ragdoll`, `is_fallout_posable_ragdoll`. The baseline audit recorded `ragdoll_pose.rs` as "data, not `GameKind`", and `physal.md` §3 still says so. That paragraph's mapping sentence ("entry *k* maps to body *k* in the 18-body humanoid set … entry zero places the COM at the reference") is also stale: FO4 maps by `pose_bone_id`, and FO3/FNV place the root body at the skeleton root's position in its parent frame (`placement_local`).
- **Evidence**: `ragdoll_pose.rs:577` `!pose.is_empty() && (placement_starts_dead || (base_dead && game == GameKind::Fallout3NV))`; `physal.md:227` "This mapping is skeleton-keyed, not a `GameKind` branch".
- **Impact**: There is no wrong behaviour today. The risk is doctrine drift: the next rig admission (Oblivion) has to edit a game switch in the runtime instead of adding a verified body set. Whether Oblivion's skeleton body names would pass `is_fallout_posable_ragdoll` was not checked in this pass. If they do, this branch is the only thing keeping Oblivion out, and that should be written down.
- **Trigger Conditions**: N/A (doctrine / doc rot).
- **Related**: #6252, #5013, #5005.
- **Suggested Fix**: Either carry the decision on the record: "a base-dead flag marks a posable corpse", set where `base_actor_starts_dead` / the DATA rule run. Or replace the game check with a rig gate: keep the base-dead path and let `pose_skeleton` refuse unverified rigs, as it already does. Then update `physal.md` §3's XRGD paragraph to name the per-rig maps and the admission rule.

## Known-Open Register (changes this pass)

- **Closed since baseline, verified in code**: PHYS-D2-2026-10-21-01 (#6324), PHYS-D2-2026-10-21-02 (#6325), PHYS-D3-2026-10-21-01 (#6255: decoded FO4 bodies get `ActorColliderOwner`), PHYS-D3-2026-10-21-02 (#6326 + #6134: rows and shapes parked and restored as authored), PHYS-D3-2026-10-21-03 (#6327: guard uses `production_text`), PHYS-D3-2026-10-21-04 (the stale "FO4 ragdolls do not activate yet" comment is gone).
- **Still open, unchanged**: #6048 (terrain LTEX friction/restitution), #6065 (FO4 XRGD rigs past human/ghoul/super mutant), #6133 (Starts-Dead ragdoll built before late-ordered statics). The FO4 tilted-reference corpse pose still has no issue of its own. Ball-and-socket / stiff-spring / chain constraints are still declined (by design). The upstream instability of mass-inverted rigs is unchanged; the real-data gate is green (9/9).
- **New**: PHYS-D3-2026-10-27-01 (ragdoll impulse no-op), PHYS-D5-2026-10-27-01 (player river current), PHYS-D5-2026-10-27-02 (swell-blind rescan gate), PHYS-D3-2026-10-27-02 (GameKind admission branch + physal.md drift).
- **Skill notes for the next sync**: Dim 3 should list `byroredux/src/systems/knockdown.rs` (a ragdoll producer and impulse caller) and say that link velocities are solver-owned. Dim 5 should record that `player_current_drift` / `player_water_state` and `local_flow` must share `over_bed`.

Suggested next step: `/audit-publish docs/audits/AUDIT_PHYSICS_2026-10-27.md` (labels `physics`; add `water` for the two D5 findings and `doc-rot` for D3-02).
