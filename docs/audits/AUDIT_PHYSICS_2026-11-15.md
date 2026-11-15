**HEAD**: `e11de827c` · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-31.md` (HEAD `670f81917`) · **Audited**: all six dimensions (each has commits since the baseline): Dim 1 (#6555), Dim 2 (#6487, #6656 `movable_body_blocks_line`, #6689), Dim 3 (#6490, #6065 FO4 creature rigs), Dim 4 (M65 sneak stance, VATS limb slow, live-load body rebuild), Dim 5 (#6488, #6489, lock-order and Late-stage registration), Dim 6 (new query call sites) · **Unchanged since baseline (skimmed)**: `crates/physics/src/ragdoll.rs`, `byroredux/src/ragdoll.rs`, `crates/nif/src/import/collision/ragdoll.rs`, `crates/nif/src/blocks/collision/{constraints,np_ragdoll}.rs`, `convert.rs`, `config.rs`, `boot/schedule/physics.rs`, `commands/physics.rs`

# PHYSAL / Physics Audit — 2026-11-15

838 commits since the baseline; 6 touch `crates/physics/**`. The main delta is the four fixes for the 10-27 findings (#6487, #6488, #6489, #6490), which no physics audit had reviewed. Dims 2, 3 and 5 were traced by Sonnet sub-agents (read-only, no cargo). Dims 1, 4 and 6 were done inline. Every agent finding was re-checked against the code before inclusion. No engine launch.

## Executive Summary

| Severity | Count (new) |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 9 |

**All four 10-27 fixes hold.**
- #6487: a link impulse is now queued and delivered as `impulse / PHYSICS_DT` on the first substep that runs. It survives sub-tick frames, recovery substeps and despawn.
- #6488: the player and dynamic bodies share one `local_current`.
- #6489: the rescan gate counts the swell.
- #6490: `parks_authored_pose` has no `GameKind` term, and the rig gate refuses Oblivion's rig.

The nine LOWs are side effects of those fixes and of new code (the #6487 restore wipes user torque, a NaN impulse reaches a free body, `movable_body_blocks_line` does not exclude a ragdolled target, the rescan gate's ripple term drifts from the sampler), plus guard gaps and doc rot. Nothing breaks a simulation.

**PHYSAL verdict.** There is no `GameKind` / `bsver` / version branch in `crates/physics/**`, `byroredux/src/ragdoll.rs` or `byroredux/src/ragdoll_pose.rs`. The 10-27 `ragdoll_pose.rs` `Fallout3NV` branch is gone (a53173780). The remaining per-game terms are:
- The named parse-side seams: constraint CInfo `bsver` gates in `constraints.rs`, and `havok_scale_for`.
- The record-boundary corpse flag in `crates/plugin`.
- `zero_is_authored_stone` (`shape.rs`, importer-side, #5549). It is not one of the three named seams, but it never reaches the solver.

Collision data traced: FO3 (six environments) and FO4 (six-rig free fall, XRGD-posed ghoul, eight-creature XRGD census) through the real-data gate, plus Skyrim SE (skeever) and the FNV/FO3 XRGD censuses. Oblivion's #6555 change was traced statically.

**Test runs (this pass, pinned rustc 1.100.0, `CARGO_INCREMENTAL=0`, TMPDIR on /mnt/data)**:
- `cargo test -p byroredux-physics`: 276 passed (270 at the baseline).
- `cargo test -p byroredux --bin byroredux -- ragdoll character locomotion water`: 315 passed, 17 ignored (307 passed at the baseline).
- **Data gate** (`-- --ignored ragdoll::installed_tests skeever_ragdoll xrgd_corpse`, plus `fo4_xrgd`), with `BYROREDUX_{FO3,FO4,SKYRIMSE,FNV}_DATA` set: 13 of 13 pass. Those are 6 FO3 environments, 2 FO4 rigs, skeever 600 floor ticks, the FNV and FO3 XRGD censuses, and the new `fo4_xrgd_poses_resolve_against_the_real_skeletons`. Note: the skill's `xrgd_corpse` filter does not match the FO4 census; add `fo4_xrgd` at the next sync.

## Solver Invariant Matrix

| Invariant | Status | Evidence |
|---|---|---|
| Fixed step (accumulator, `MAX_SUBSTEPS` clamp, NaN guard, budget) | verified | `world/mod.rs:1437-1443, 1529-1541, 1759-1766`. The #6487 restructure keeps the step-count semantics. |
| Wake (`pending_wake`, `substep_wake_requested`, spawn-sleep exemption) | verified | Link `apply_impulse` wakes, calls `forget_spawn_sleep` and arms `pending_wake`. A queued impulse holds the fast path off until a real tick. |
| Lock order / scheduler access | verified | dfcbcafd5 drops `WaterBedField` before the `WaterVolume` write in `submersion_system` (guard `submersion_with_a_bed_field_takes_its_locks_in_one_order`). `WaterBedField` reads are declared on all three consumers. |
| Phase order (register → kinematic → buoyancy → step → pull) | verified | `boot/schedule/physics.rs` unchanged. Inside `step`, the link-damping pass runs, then link-impulse delivery, then the substeps. |
| Explosion recovery (snapshot, restore, ladder, contact-event truncation) | verified | The impulse force is released on a recovery substep too. PHYS-D2-04 routes a script NaN through recovery instead of refusing it. |
| Teardown (handle release, per-body maps) | verified | `pending_link_impulses` is not pruned in `remove_body`, but it is self-cleaning (stale handles skipped, drained at the next tick). |
| Link force/torque ownership (#5698) | **drifted (minor)** | PHYS-D2-01. |
| Late water chain (submersion → damage → reconcile → interaction → audio) | verified | 22e0f71fc keeps every named `run_after` (`water_hazards.rs`, `scripted_combat.rs`, `audio.rs`). |

## Known-Open Register (changes this pass)

- **Closed and verified in code**: #6487 (eac3f73f9), #6488 (6390bfb90), #6489 (b76b674df), #6490 (a53173780). None regressed.
- **Still open, unchanged**:
  - #6065: eight creature rigs are now admitted, and cat / Handy / FEV hound are still refused by absence from the list.
  - #6677: tilted FO4 corpses. The eight new rigs inherit the same placement composition.
  - #6549: the FNV opaque sky planes. #6555's rule is keyed on `has_alpha` and does not touch them.
  - #6632 and #6048.
- **Skill-text drift for `/audit-sync`**:
  - (a) The Dim 5 line "calm planes still settle and sleep" holds only for non-wind-driven kinds. Every `Calm` plane is wind-driven (`env_translate/water.rs:668`), including interior pools, and `WaterMaterial::swell` gives a 1.5 BU minimum swell at zero wind. That is documented WATAL design.
  - (b) Phase 1 should add `fo4_xrgd` to the data-gate filter.
  - (c) `movable_body_blocks_line`'s doc and its two sibling probes still say the caller must call `update_query_pipeline`. That is stale: production relies on `step` plus queued-leaf insertion (Dim 2 note).
- **Open questions (not filed; need a source)**:
  - The sneak stance and crippled-leg multipliers also scale the swim stroke, through `speed_mul` → `swim_motion`. Sprint ×2 already did. The M65 PRD is silent on swimming.
  - A third actor's ragdoll corpse between a shooter and its target counts as a "movable object" for Hold Fire When Blocked. Check against the GECK text.

## Findings

### PHYS-D2-01: A delivered link impulse also wipes the link's user TORQUE, so the angular-damping torque is lost for the rest of the frame's substeps
- **Severity**: LOW
- **Dimension**: Dim 2 (Step & Wake) / link-damping interaction
- **Location**: `crates/physics/src/world/mod.rs:1235-1262` (`deliver_link_impulses` / `release_link_impulse_forces`); `crates/physics/src/world/mod.rs:1302-1306` (`reset_forces`)
- **Trigger Conditions**: A multibody link with nonzero angular damping receives `apply_impulse` (e.g. `PushActorAway`, `ApplyHavokImpulse`), and the same stepping frame runs two or more substeps (catch-up at <60 fps, or a sub-tick accumulator carry-over).
- **Status**: NEW (introduced by `eac3f73f9`)
- **Description**: `deliver_link_impulses` snapshots only `body.user_force()` into `before`. `release_link_impulse_forces` then calls `self.reset_forces(handle, false)`, which clears forces AND torques (`b.reset_forces(..); b.reset_torques(..)` at 1304-1305), and re-adds only the saved force. The commit and doc say "restores the body's previous user force exactly". That is true for the force, but the user torque is not part of the snapshot and is silently zeroed. The torque is real state. `apply_multibody_link_damping` (1137-1175) puts the authored angular drag `-c·I·ω` (#5697) on every dry awake link through `b.add_torque`, and the water scan does the same for wet links (`pw.add_torque`, water.rs ~1409).
- **Evidence**: `release_link_impulse_forces`: `self.reset_forces(handle, false); self.add_force(handle, force, false);`. The torque is never saved or re-added. The tests `a_link_impulse_is_a_single_kick_and_leaves_other_forces_alone` only assert `user_force()`, never `user_torque()`.
- **Impact**: Small. After substep 0 of the impulse frame, the link has no angular damping for substeps 1..N of that frame (N ≤ MAX_SUBSTEPS-1, only on catch-up frames). Both the dry pass (top of the next stepping frame) and the water scan (each full scan) re-derive the torque, so nothing persists past the frame. The cost is a slightly under-damped spin during a knock-down, the case where the angular damping is wanted most.
- **Related**: #6487, #5697, #5540, #5698 (force ownership: "anything else added to a dry link's force is replaced each frame").
- **Suggested Fix**: Capture `body.user_torque()` beside `user_force()` in `deliver_link_impulses` and re-add it in the release (`b.add_torque`), or restore with `b.reset_forces(false)` plus the force only, leaving torques untouched. Extend the single-kick test with a held torque.

### PHYS-D2-02: `movable_body_blocks_line` does not exclude the target's (or shooter's) multi-body ragdoll, so a knocked-down target blocks its own shot
- **Severity**: LOW
- **Dimension**: Dim 2 (scene-query contract, `47207ecb0`) / caller `byroredux/src/systems/faction_hostility.rs:113-124`
- **Location**: `crates/physics/src/world/queries.rs:428-451`; caller `byroredux/src/systems/faction_hostility.rs:117-123`
- **Trigger Conditions**: A UseWeapon package with Hold Fire When Blocked (`options.hold_fire_when_blocked`, `byroredux/src/systems/script_requests.rs:122`) whose target is a ragdolled actor (knocked down by `push_actor_system`, or any actor with `RagdollActive`).
- **Status**: NEW
- **Description**: `excluded_bodies` is built from ONE handle per actor, `RapierHandles.body` of the shooter and target entities (the stand-in capsule or placement body). A live actor's bone bodies are keyframed (kinematic), so they are not "movable" and the test is correct for them. When an actor is ragdolled, its bones are separate DYNAMIC bodies on their own entities and none are in `excluded_bodies`. `sight_points` aims at `placement root + FALLBACK_ACTOR_CAPSULE_CENTRE_HEIGHT` (`detection/sight.rs:121-133`), which lies inside the ragdoll's torso. The ray therefore first-hits one of the target's own dynamic bone colliders, and `is_dynamic()` is true. The doc comment promises "a dynamic ragdoll target would otherwise block its own shot", but the exclusion only covers the single capsule body. The unit test uses a one-body target and cannot see this.
- **Evidence**: queries.rs 437-445 filter `!collider.parent().is_some_and(|body| excluded_bodies.contains(&body))`; the caller passes `[shooter, target].filter_map(handles.get(entity).map(|h| h.body))`. Ragdoll bodies live on `Ragdoll.bodies` entities (`knockdown.rs:106`). Nothing maps them back to the owning actor, although `ActorColliderOwner` stamps every ragdoll body with the placement root (#6255).
- **Impact**: The `UseWeaponRun` "waits without giving up" (script_requests.rs:122-125), so a shooter holds fire for as long as its target is down, normally the get-up window, then resumes. Behaviourally wrong for a few seconds per knockdown. Also, a third actor's ragdoll corpse between shooter and target counts as a "movable object". That may be intended; confirm against the GECK text.
- **Related**: #6656 (AIP-UW-003), #6255 (`ActorColliderOwner`).
- **Suggested Fix**: Resolve exclusion by owner, not by handle. In the caller, add the `Ragdoll.bodies` handles of shooter and target to `own_bodies`, or key the predicate on `ActorColliderOwner`. Add a test with a two-body (jointed or dynamic) target.

### PHYS-D2-03: #6689 removed the only guard that tied each profile accessor to its own environment variable
- **Severity**: LOW
- **Dimension**: Dim 2 (guard coverage, `8124d9594`)
- **Location**: `crates/physics/src/sync.rs:250-272`, tests at `crates/physics/src/sync.rs:2625-2680`
- **Trigger Conditions**: A copy-paste edit of `profile_fallers` (e.g. `env_flag_once(&PROFILE_FALLERS, PROFILE_VAR)`, or sharing one `OnceLock`).
- **Status**: NEW (test gap only; the production code is correct)
- **Description**: The deleted `profile_accessors_read_their_own_variables` asserted `var_os("BYRO_PROFILE")` and `var_os("BYRO_PROFILE_FALLERS")` each appear in production text. The replacement `the_two_profile_flags_name_different_variables` only compares the two `const`s to string literals and calls each accessor twice, so it cannot see which constant or cell an accessor passes. `an_env_flag_is_read_once_and_then_cached` exercises `env_flag_once` in isolation. The caching guard (`profile_flags_are_read_once_per_process_not_per_tick`) is not vacuous. It scans the `physics_sync_system` body through whole-file `include_str!` and `find("pub fn physics_sync_system")` hits the production fn before the test module. I checked the Methodology rule here: no guard in this commit became vacuous, but this wiring check was dropped.
- **Impact**: A wiring mistake would make `BYRO_PROFILE_FALLERS` inert or tied to `BYRO_PROFILE`. It is a diagnostic flag, so the cost is a misleading profile run.
- **Related**: #6689, #2881.
- **Suggested Fix**: Make each accessor a thin wrapper over a single `static` + `const` pair (e.g. a small macro), or assert on `production_text` that `profile_phases` contains `PROFILE_VAR` and `profile_fallers` contains `PROFILE_FALLERS_VAR`.

### PHYS-D2-04: The non-finite-impulse refusal added by #6487 covers multibody links only; a free body still takes a NaN impulse
- **Severity**: LOW
- **Dimension**: Dim 2 (wake/impulse contract)
- **Location**: `crates/physics/src/world/mod.rs:1209-1222`
- **Trigger Conditions**: `apply_impulse` on a non-link dynamic body with a non-finite `impulse` (script-supplied `ApplyHavokImpulse` vector, `cinematic.rs:511`).
- **Status**: NEW (asymmetry exposed by `eac3f73f9`)
- **Description**: For a link, a non-finite impulse returns `false` before any state changes. For a free body it goes straight to `b.apply_impulse(impulse, true)`, which writes NaN into the body's velocity. `forget_spawn_sleep` and `wake()` still run and the call returns `true`. The next `run_substep` snapshot filters the body out (`body_state_is_finite`), rapier quarantines it, and `restore_invalid_dynamic_bodies` parks it. A bad script argument therefore counts as an explosion-recovery event (error log, `recoveries_total`) instead of a refused call.
- **Evidence**: 1209-1222: `if is_link { if !impulse.is_finite() { return false; } ... } else { b.apply_impulse(impulse, true); }`.
- **Impact**: One parked clutter body and a spurious error log per bad input. Recovery contains it.
- **Related**: #6487, #4687 (non-finite parking).
- **Suggested Fix**: Hoist the `is_finite` refusal above the `is_link` branch so both paths behave alike.

---

### PHYS-D3-01: The eight FO4 creature rigs' entry→body rule is pinned only by an offline probe; the Rust census and tests accept any permutation, and no posed-creature physics check exists
- **Severity**: LOW
- **Dimension**: Dim 3 — Ragdoll Articulation (authored corpse pose, #5015/#6065)
- **Location**: `byroredux/src/ragdoll_pose.rs:734-760` (`entries_by_tree_order`), `:762-805` (`body_tree_ranks`), `byroredux/src/ragdoll_pose_tests.rs:942-975` (`imported_body_ranks`), `:1035-1050` and `:1086-1096` (census), `byroredux/src/ragdoll_installed_tests.rs:391-398` and `:518` (only physics checks), `scripts/xrgd-fo4-rigs.py:83-88,167-170`
- **Status**: NEW (adjacent to open #6065, which tracks "rigs still refused"; this is about the rigs now admitted). Related open: #6677 (tilted FO4 corpses)
- **Trigger Conditions**: an FO4 `Starts Dead` ACHR whose race is Brahmin / RadStag / YaoGuai / Mirelurk / RadRoach / Molerat / ViciousDog (Dogmeat) / Stingwing and which authors an `XRGD` pose with exactly N (+ optional marker) entries, spawned through `spawn_nif_nodes`. Latent: it only bites if the node order the loader gives `Children`, or the DFS convention, ever changes.
- **Description**: The whole admission rests on one empirical rule: entry k poses the k-th ragdoll body bone in the skeleton's depth-first order. The evidence for that rule is `scripts/xrgd-fo4-rigs.py`, which builds its `tree` mapping from `order`, the line order of `skeleton_tree_dump`'s JSONL, i.e. NIF **block** order (`:83-88`, `:167`). The Rust side walks `Children` (pre-order DFS, `body_tree_ranks`). The commit asserts DFS == block order on all eight rigs (checked with the new `block`/`ci` dump fields), but nothing in Rust pins that. `entries_by_tree_order` accepts *any* permutation with body 0 first (`:744-755`), and the real-data census `fo4_xrgd_poses_resolve_against_the_real_skeletons` computes ranks with a test-local reimplementation (`imported_body_ranks`, over `imported.nodes`, not over spawned entities) and then asserts only per-race resolve counts (`:1086-1096`). Those counts are entry-count and permutation checks. A change to child ordering in the importer/loader, to sibling order in `add_child`, or to the DFS direction would leave every census number green while posing every creature bone from the wrong entry. The only physics-level check of a posed corpse is the feral-ghoul one (`ragdoll_installed_tests.rs:518`); the six-rig free-fall test (`:391-398`) is bind pose and includes only the molerat of the eight. Three of the eight (molerat, dog, stingwing: 27 vanilla corpses by the commit's census) were admitted although their FK rank is 7th/11th/11th of 24, below the bar the module doc (`ragdoll_pose.rs:50-61`) and the audit skill both state ("a rig is added only when ... passes ... both checks"); the module doc records it as the owner's call (2026-11-08), which is honest, but leaves no guard behind it.
- **Evidence**: `entries_by_tree_order` returns `Ok(ranks.to_vec())` for any permutation (`ragdoll_pose.rs:744-756`); the census calls it with `imported_body_ranks(..)` output (`ragdoll_pose_tests.rs:1035,1046`); the script's offset check keys on `bones[k]` from `order` (`xrgd-fo4-rigs.py:167-179`). The loader does append children in node-index order (`byroredux/src/scene/nif_loader.rs:1576-1581`, `add_child` pushes), so the live path agrees with the test-local ranks today; that agreement is not asserted anywhere.
- **Impact**: No wrong behaviour today. If the order assumption silently broke, creature corpses would seed with permuted bone rotations: contorted, possibly past joint limits, with the soft-limit pull-in and the `seed_pose_is_sane` bound (1e5 BU root offset, `crates/physics/src/ragdoll.rs:54`) as the only backstops (cf. #6677's thrown corpse). Bounded to the 8 FO4 creature races (59 vanilla posed corpses per the census table at `ragdoll_pose_tests.rs:1086-1096`; the commit message says 53 of 56, which does not match its own per-race table).
- **Related**: #6065, #6677, #6070; `docs/smoke-tests/fo4-ragdoll.sh:258` exercises one posed molerat corpse (ShamrockTaphouse01 REFR 0x0F2A45) for stability only, not for pose correctness.
- **Suggested Fix**: Extend the ignored census to assert, per admitted rig, that the median of `|entry offset| / |NIF bone offset|` under `body_tree_ranks`-equivalent ranks stays within the 3 % bar (the check the script does offline), computed from spawned-entity ranks or by asserting `imported_body_ranks` equals the loader's `Children` order; and add a posed-corpse joint-limit residual check (the `fo4_xrgd_posed_ghoul_corpse_holds_together_in_free_fall` shape) for at least one of the three below-bar rigs.

### PHYS-D3-02: a3a36f1c7 left four statements of the admission rule stale — the rule is now three mappings and eight more rigs
- **Severity**: LOW
- **Dimension**: Dim 3 — doc rot
- **Location**: `docs/engine/physal.md:233-241`, `docs/engine/physics.md:312-318`, `byroredux/src/ragdoll_pose.rs:194-199` (module "Scope"), `docs/smoke-tests/fo4-ragdoll.sh:17-19` and `:225-226`
- **Status**: NEW
- **Trigger Conditions**: documentation only
- **Description**: a53173780 rewrote physal.md §3 to name the per-rig maps ("Skyrim's humanoid and creature sets and the FO3/FNV humanoid map entry k to body k; FO4's human, ghoul and super-mutant sets map by `pose_bone_id`"). Six days later a3a36f1c7 added a third mapping (skeleton node order) and eight rigs, touching four files but not these:
  - `physal.md:233-241` still lists two maps, and "a rig joins by adding its *verified* body set" now overstates (three of eight were admitted on the owner's call, not the verified bar).
  - `physics.md:312-318` still says pose interpretation "is pinned to the humanoid skeleton's named body order".
  - The `ragdoll_pose.rs` module **Scope** paragraph (`:196-199`) lists "the FO4 human, feral-ghoul and super-mutant rigs" only, contradicting the FO4 section ~120 lines above it in the same doc; `:194` ("Creatures stay refused until each rig is verified") sits in the FO3/FNV section and reads as a global statement.
  - `fo4-ragdoll.sh:17-19` and `:225` still describe the molerat as "a creature whose rig is not posable"; since a3a36f1c7 it is a posed corpse (the census pins Molerat 12 of 12 resolving), so the gate's comment misdescribes what it now covers.
- **Evidence**: `git show --stat a3a36f1c7` touches `ragdoll_pose.rs`, `ragdoll_pose_tests.rs`, `workspace_hygiene_tests.rs`, `skeleton_tree_dump.rs` only; `git log -3 -- docs/engine/physal.md` last entry is a53173780.
- **Impact**: A reader (or the next audit) takes the FO4 rule as bone-id-only and the creature rigs as refused; the smoke gate's pass is read as "creature collapse" when it is a posed-corpse stability check.
- **Related**: #6065, PHYS-D3-01
- **Suggested Fix**: One sentence each: name the node-order map and the eight sets in physal.md §3 and physics.md, mark the molerat/dog/stingwing exception, update the module Scope paragraph, and reword the smoke script's molerat comment.

---

### PHYS-D5-01: `waves_require_contact_rescan` sizes its reach from a different formula than the crest sampler, and the sized band is unreachable in production
- **Severity**: LOW
- **Dimension**: Dim 5 — WATAL physics sink (wake discipline / quiescence gate)
- **Location**: `crates/physics/src/water.rs:785-819` (gate and `surface_wave_reach`), compare the sampler at `crates/physics/src/water.rs:574-577`; gate call site `crates/physics/src/water.rs:947-948,976`
- **Status**: NEW (latent regression class of #6489; not observable today)
- **Description**: #6489 made the gate's reach `wave_amplitude.abs() + swell.amplitude * WATER_SWELL_SHOAL_GAIN_MAX`. The swell half is a true upper bound of what `authored_wave_height_over_bed` adds. `WaterSwell::amplitude_over` caps the gain at 1.6 and the breaking cap only lowers it. It also takes the same kind-damped scroll, `surface.kind.weather_scroll_for(weather_scroll)`, as the sampler at `water.rs:1198`. The ripple half is not the sampler's formula. The sampler uses `(wave_amplitude * wind_wave_scale).clamp(0, WATER_WAVE_AMPLITUDE_MAX)`, and `weather_wave_adjustment` returns `wind_wave_scale` in 1.0..=1.5 (`water.rs:644-649`). The gate uses the raw authored amplitude, so in a gale it under-reports the ripple by up to 50%. It also over-reports any authored amplitude above 32. The `|depth| <= reach + WATERLINE_HYSTERESIS` disjunct is the only place the reach is used as a size rather than as an on/off threshold (`> 1e-4`).
- **Evidence**: The two producers of a `WaterContact` with `surface_entity: Some(..)` are `crates/physics/src/water.rs:1416` (written only under `if frac > 0.0`; the `else if t.prior_wet` and `None` arms write `WaterContact::default()`) and `byroredux/src/systems/character.rs:1440` (`player_water_state` `continue`s when `fraction <= 0.0`). Every live contact with a surface therefore has `submerged_fraction > 0.0`, so the first disjunct `contact.submerged_fraction > 0.0` fires whenever the band disjunct could. The band is reachable only from a test that hand-builds a `submerged_fraction: 0.0` contact (`default_authored_waves_only_rescan_nearby_live_contacts`, `water.rs:~1665-1700`, and the second half of `a_swell_only_plane_keeps_the_contact_rescan_armed`). In production the gate is effectively `max reach > 1e-4 && any wet contact exists`.
- **Impact**: None today. #6489's own rationale "sized the band too narrow wherever the swell outreached the ripple" describes a path that cannot trigger, and the half of the new test that proves the band reaches a gale swell pins unreachable behaviour. The first assertion (a wet float on a swell-only plane keeps the rescan armed) is the one that matters and holds. The risk is future: a producer that keeps a `WaterContact` with fraction 0 (for example a hysteresis-band contact) would silently get the under-counted ripple reach. Minor cost note: `waves_require_rescan` is evaluated eagerly at `water.rs:947` before the cheap `active_island_counts()/pending_wake()/had_newcomers` terms at `:976`. That is one `WaterContact` read-lock plus a contact scan per tick even when bodies are awake. The result is only needed when everything else is quiet, so it could be lazy.
- **Related**: #6489 (b76b674df), #3135 (original gate), #5561 (settled-depth re-wake), #5582 (shared wave tuning).
- **Suggested Fix**: Either drop the band disjunct (and its half-test) and document the gate as "any wet contact on a plane with a non-zero animated reach", or take the ripple term from one shared helper that applies `wind_wave_scale` and `WATER_WAVE_AMPLITUDE_MAX` exactly as `authored_wave_height_over_bed` does, so the two cannot drift. Move the gate behind the cheap quiescence terms.
- **Trigger Conditions**: A `WaterContact` with `surface_entity` set and `submerged_fraction == 0` (not produced today), on a plane whose ripple amplitude is non-zero during wind above the calm baseline.

### PHYS-D5-02: Stale water-current prose after #5129/#6488 (early-stage access comment and watal.md river paragraph)
- **Severity**: LOW
- **Dimension**: Dim 5 — documentation of the two-samplers contract
- **Location**: `byroredux/src/boot/schedule/early.rs:67-69`; `docs/engine/watal.md:671-674`
- **Status**: NEW (doc rot)
- **Description**: The player controller's access row says `// #3974 — player_water_state falls back to a placed WaterCurrentVolume marker when the plane has no flow of its own`. That arm was replaced: since #5129/#5357 the marker is resolved by `placed_current_flow_at` (`character.rs:312`), independent of any plane, and composed additively in `player_current_drift` (`character.rs:1214`). `player_water_state` no longer reads the marker. The `watal.md` river paragraph says `WaterFlow::over_bed` is "the physics current on floating bodies (`apply_buoyancy`) and the `WaterContact` it publishes". It does not name the `local_current` seam or the swimming player, which is what #6488 added and what the two-samplers rule rests on.
- **Evidence**: `character.rs:1272-1279` and `physics/src/water.rs:499-512` already document the current behaviour. The two sites above are the only ones that still describe the old one.
- **Impact**: A reader following the early-stage comment would look for a plane-less fallback inside `player_water_state` that does not exist. The `watal.md` page does not tell the next author that a new reader of a plane's `WaterFlow` must go through `byroredux_physics::local_current`.
- **Related**: #4691, #5129, #5357, #6488.
- **Suggested Fix**: Reword the early.rs comment to "`placed_current_flow_at` resolves the marker, independent of the plane". Add one sentence to the watal.md river paragraph: "`byroredux_physics::local_current` is the one entry point; the dynamic path and `player_water_state` both call it (#6488)".
- **Trigger Conditions**: n/a (documentation).

---

### PHYS-D4-01: `probe_spawn_ground`'s rustdoc opens with half of `yaw_pitch_from_forward`'s doc
- **Severity**: LOW
- **Dimension**: Character & NPC Controller
- **Location**: `byroredux/src/scene/character_spawn.rs:255-273`; the other half at `byroredux/src/scene.rs:148-151`
- **Status**: NEW
- **Description**: The file split in e9aeb2706 (2026-09-21) cut the rustdoc of `yaw_pitch_from_forward` in two. Its head ("Decompose a forward vector into the `(yaw, pitch)` pair that `fly_camera_system` would need… so `pitch = asin(y)` and `yaw = atan2(-x, -z)`…") now sits directly above "Probe for walkable ground beneath `cam_pos`" with no blank line, so it is part of `probe_spawn_ground`'s doc. The function it describes lives in `scene.rs:153`, whose doc now starts mid-thought at "Looking exactly along ±Y is the degenerate case".
- **Evidence**: `git blame -L 255,273 byroredux/src/scene/character_spawn.rs` → all e9aeb2706; `git grep -n 'fn yaw_pitch_from_forward'` → `byroredux/src/scene.rs:153` only.
- **Trigger Conditions**: none (documentation).
- **Impact**: A reader of the player-spawn ground probe (the Character-vs-FlyCam gate, #2375/#3002) is told it decomposes a camera vector; the real decomposition's #2383 rationale (why not a shortest-arc quaternion) is missing where it applies.
- **Related**: #2383, #6814 (last edit to this function).
- **Suggested Fix**: Move lines 255-269 back above the "Looking exactly along ±Y" paragraph on `yaw_pitch_from_forward` in `scene.rs`.

## Dimension Verification Notes

### Dim 1 — Shape Translation

Delta: one commit, `9a0ae0817` (Fix #6555). `convert.rs` / `config.rs` have no commits since the baseline; the 10-31 variant-vs-arm trace stands.

#### Verified
- `9a0ae0817`: `render_geometry_can_collide` (`byroredux/src/cell_loader/spawn.rs:105-118`) adds `blended_overlay = has_alpha && authoring.authors_no_body()`. `authors_no_body` (`crates/nif/src/import/collision/mod.rs:115`) = `classic == 0 && new_physics == 0 && external == 0`, so a phantom-only NIF is an overlay (trigger, not body) and a NIF with classic collision the importer failed to translate keeps the trimesh stand-in. The fallback mode is chosen only when `collisions` is empty (`missing_collision_fallback`), so the new term cannot remove a collider from a NIF whose bodies were translated.
- Every `PlacementCtx` constructor passes the summary (`spawn.rs:911`, `spawn/authored_cover.rs:151` — whose fallback is `None` anyway, `spawn/precombined.rs:93`). FO4 precombines stamp `external: 1` (`cell_loader/precombined.rs:1089`), so their blended surfaces keep the stand-in; `external` is not read by the packed-proxy gate or the census sums (`nif_import_registry.rs:743-754`) — counts unchanged as the commit claims.
- The SpeedTree placeholder's `Default` summary (`references/import.rs:524`) is all-zero (`authors_no_body`), but its meshes are alpha-tested and already excluded by `!material.alpha_test`; no behaviour change.
- Open #6549 (FNV sky planes / fake sky dome / waterfall sheet) is the *opaque* `NO_LIGHTING` residue; #6555 keys on `has_alpha`, so it does not touch #6549. Carried.

### Dim 2: Step, Wake & the 4(+1)-Phase Sync Tick

**HEAD**: `e11de827c` · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-31.md` (HEAD `670f81917`) · **Scope**: delta commits `eac3f73f9` (#6487), `47207ecb0` (Refs #6656), `8124d9594` (#6689) traced end to end; lighter pass over the rest of the Dim 2 checklist. Read-only; no cargo, no engine launch.

Dedup: `/tmp/audit/issues.json` (123 open) and `gh issue list --state all` searches (impulse / multibody link / torque / reset_forces / Hold Fire / UseWeapon movable / profile flag). No open or closed duplicate for the four findings below. #6487 (closed by `eac3f73f9`) is the fix under review. #5697 / #5934 are the closed link-damping and water-ownership fixes the new code interacts with.

Summary: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 4 LOW. The #6487 fix delivers correctly in every case I traced. The Solver Invariant Matrix row is verified except for one small gap, the user-torque wipe in PHYS-D2-01.

---


#### Verified

##### `eac3f73f9` — Fix #6487 (impulse on a multibody link delivered as a one-substep force)

All of the following hold in HEAD `e11de827c`.

- **Force magnitude (`impulse / PHYSICS_DT`) is right.** Rapier 0.36.1's staged island solver recomputes the multibody acceleration from the user force at the start of every internal substep with `dt/num_substeps` (`staged_island_solver/helpers.rs:109-114,141-149`; `init.rs:77-119`). The force is constant across the pipeline step, so the total Δv is `F · PHYSICS_DT / m = impulse / m`. `integration_parameters.dt = PHYSICS_DT` at `world/mod.rs:496`. The test pins this (free pair 150 vs jointed pair 150 at 300 BU/s over 30 ticks; one-kick total 100 within 5 %).
- **Delivered on the first substep that runs.** `deliver_link_impulses` (1235-1253) returns early, consuming nothing, when `accumulator < PHYSICS_DT`. This is the same predicate that gates the loop (`while self.accumulator >= PHYSICS_DT`, 1531), and nothing between 1520 and 1531 changes the accumulator. A sub-tick frame therefore leaves the queue intact and no stray force behind.
- **Zero-substep and static-fast-path frames.** `apply_impulse` calls `self.wake()` (1220), so `pending_wake` is true. `step` clears `pending_wake` only when `steps > 0` (1556-1563), so a retained impulse keeps the fast path (1509-1512) off until the first real tick. `remove_body` also calls `wake()` (665), so a body removed with an impulse queued still lets the next step drain the queue.
- **Order inside the step.** `apply_multibody_link_damping` (1518) runs before `deliver_link_impulses` (1520), so the damping `reset_forces` cannot erase the kick. Water targets are skipped by the damping pass and own their force. The water scan runs before `step` (phase 2.5), so the kick is added after it. `refresh_water_lift` (1622) runs after delivery as a delta-only `add_force` (water.rs:250-271). In substep 0 its delta is ≈0, because the scan seeded `applied = f` from the same pose and velocity (water.rs:1346-1361). The release therefore drops nothing of it (the lift is re-seeded by the next scan).
- **Force restore is exact for forces.** `before` is captured once per handle before any add (`!before.iter().any(..)`, 1244-1246), so two impulses on one body in the same frame sum correctly and restore to the true prior force. The torque is the exception (PHYS-D2-01).
- **Release is unconditional on the first substep.** The `steps == 0` release at 1533-1535 runs after `run_substep` whatever it returned, so a recovery or rollback substep (restored > 0, accumulator zeroed, `Stop`) and a budget-stop still release. The restructured loop (1531-1541) keeps the old step-count semantics: `steps` increments once, and `Stop` breaks after the increment.
- **Body removed while queued.** `bodies.get(handle)` returning `None` is skipped (1242). The generational arena makes a reused slot a different handle. A queued impulse for a body that stopped being Dynamic is skipped too (1245).
- **Link detached before delivery** (explosive-ladder `remove_multibody_articulations`, `stand_up`). The impulse is delivered as a force to a now-free body, which is still `impulse / m`. This is correct and needs no special case.
- **Non-multibody body.** Keeps the immediate `b.apply_impulse(impulse, true)` path (1217).
- **Wake and spawn-sleep.** Link path: `b.wake_up(true)` (1214), `forget_spawn_sleep` (1219) and `wake()` (1220) all run. `resleep_spawned_newcomers` therefore cannot swallow a same-frame kick (`spawn_sleep_pending` is consumed from the same list `forget_spawn_sleep` edits, 1043/1081). The non-finite early return at 1211 precedes all of these, so a refused impulse leaves nothing half-applied.
- **Contract with consumers.** `knock_down` (`byroredux/src/systems/knockdown.rs:106-113`) and `cinematic.rs:511` call the same public method. Neither depends on immediate velocity.
- **Residual**: the quarantine re-enable path (`body.sleep()` in `run_substep`) can put a link to sleep between queue and delivery, which makes the kick a no-op. This is an explosion-recovery corner and not worth a finding. Tests do not cover the sub-tick retention, removal and double-impulse cases I traced (all correct by reading); worth adding alongside the PHYS-D2-01 fix.

##### `47207ecb0` — `PhysicsWorld::movable_body_blocks_line`

- **Query contract.** Uses `self.queries(filter)` and `cast_ray(.., solid = true)` on the broad phase's BVH, the same path as `line_of_sight_blocked` (queries.rs:393-417). Zero or non-finite length returns `false`. Sensors are excluded. It does not call `update_query_pipeline()` (the doc's "caller must have called `update_query_pipeline`" is boilerplate shared by three probes at queries.rs:395/426/467 and stale: production relies on `step` plus the queued-leaf insertion, per the Dim 2 `wake()` note). The caller does not call it either, which is correct for per-frame use.
- **"Movable" semantic.** `Body::is_dynamic()` on the first non-excluded solid hit. A fixed wall or prop returns false, as does a kinematic actor body. That matches AIP-UW-003 and the comment in `faction_hostility.rs:108-112`. Note that a kinematic actor in front of a crate also hides the crate, which is acceptable ("first solid thing").
- **Gap**: the ragdolled-target exclusion (PHYS-D2-02).

##### `8124d9594` — Fix #6689 (source-scan reclassification, `crates/physics/src/sync.rs`)

- No guard became vacuous. `profile_flags_are_read_once_per_process_not_per_tick` still scans the production `physics_sync_system` body (the production fn precedes the test module, so `find` lands on it; the scan window is closed at the first `\n}\n`). The dropped `OnceLock` text scan is replaced by a behavioural `an_env_flag_is_read_once_and_then_cached`. The only loss is accessor-to-variable wiring (PHYS-D2-03).
- The `set_var` / `remove_var` in the new test use a unique variable name, which is safe within a Rust 2021 test process.
- The other 60 scans in the classification file were not touched by this commit (`git show --stat`: `sync.rs` plus a `.claude/issues/6689` doc).

##### Rest of the Dim 2 checklist (light pass, nothing disturbed by the delta)

- **Accumulator clamp and NaN guard.** `frame_dt.max(0.0)` plus the `MAX_SUBSTEPS * PHYSICS_DT` clamp (1438-1443) are unchanged and precede the loop. The budget is checked after the substep (1759-1766). `steps < MAX_SUBSTEPS` is intact in the restructured loop.
- **Static fast path.** Unchanged (1509-1512). `pending_wake` is cleared only when `steps > 0` (1556), and the substep-armed `substep_wake_requested` re-arm follows at 1559-1562. The writers at 1696 and 1726 are intact.
- **Spawn-sleep contract.** The four explicit-wake entry points (`add_force`/`add_torque` with wake at 984/1008, `reset_forces` with wake at 1307, `set_body_type` at 1355) and the new link `apply_impulse` path all call `forget_spawn_sleep`. `register_newcomers` still adds no `wake()`.
- **Explosion recovery.** The snapshot precedes `pipeline.step` in every substep (1625-1646). `restored > 0` zeroes the accumulator (1746), runs `clamp_explosive_velocities` and returns `Stop`. The substep-armed wake survives the post-loop clear.
- **Contact-force buffer.** `events_before_substep` is captured before the drain (1669). The truncate on rollback is at 1735. The buffer is cleared once at `step` entry (`pending_force_events.clear()`), and the new code does not touch it.
- **Lock discipline / phase order.** `boot/schedule/physics.rs`, `crates/physics/src/components.rs` and `byroredux/src/cell_loader/rapier_release_tests.rs` have no commits since baseline. `git log 670f81917..HEAD -- crates/physics/src/world/ crates/physics/src/sync.rs crates/physics/src/components.rs byroredux/src/boot/schedule/physics.rs byroredux/src/cell_loader/rapier_release_tests.rs` lists only the three delta commits. The access-declaration guards are not affected, since no new component or resource type is read by the physics systems.

##### Solver Invariant Matrix row (Dim 2)

| Invariant | Status | Evidence |
|---|---|---|
| Fixed step (PHYSICS_DT accumulator, clamp, NaN guard, budget) | verified | `world/mod.rs:1437-1443,1529-1541,1759-1766`; `integration_parameters.dt = PHYSICS_DT` at 496; impulse force scaling matches rapier's per-internal-substep integration |
| Wake (pending_wake, substep_wake_requested, spawn-sleep exemption, same-frame wake not swallowed) | verified | `1220`, `1556-1563`, `1214/1219`; sub-tick retention of queued link impulses holds the fast path off |
| Lock order / scheduler access | verified (unchanged; no new types read) | no commits touch the schedule or access declarations |
| Phase order (collect/register, kinematic, buoyancy, step, pull) | verified (unchanged) | `boot/schedule/physics.rs` unchanged since baseline; within `step`, damping pass then link-impulse delivery then substeps (1518-1534) |
| Explosion recovery (snapshot, restore, ladder, contact-event truncation) | verified | `1625-1646`, `1735-1766`; the impulse release also runs on a recovery substep |
| Teardown (handle release, per-body maps pruned) | verified, with a note | `remove_body` (635-668) prunes `body_labels`, `explosion_offences` and `keyframe_refusals_logged`. `pending_link_impulses` is not pruned there but is self-cleaning (stale handles skipped and the queue drained at the next full tick), so it is bounded and needs no change |
| Link force/torque ownership (#5698) | drifted (minor) | PHYS-D2-01: the delivery release resets the user torque as well as the force |

### Dim 3 — Ragdoll Articulation & the Per-Game Constraint Seam

**HEAD**: `e11de827c` · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-31.md` (HEAD `670f81917`) · **Audited**: Dim 3 delta (a53173780, a3a36f1c7, 9a0ae0817, 3c9949ec9, plus 68d3a834e and eac3f73f9 touching ragdoll-adjacent paths) · **Unchanged since baseline (spot-checked)**: `crates/physics/src/ragdoll.rs`, `byroredux/src/ragdoll.rs`, `crates/nif/src/import/collision/ragdoll.rs`, `crates/nif/src/blocks/collision/{constraints,np_ragdoll}.rs`, `byroredux/src/ragdoll_installed_tests.rs` (zero diff `670f81917..HEAD`; every named guard test still exists, `RAGDOLL_EXTRA_SOLVER_ITERATIONS = 0` at `crates/physics/src/ragdoll.rs:608`, `seed_pose_is_sane` at `:100`, `RAGDOLL_JOINT_SOFTNESS` at `:692`).

Method: read-only. No cargo, no engine, no real-data run (the ignored census / installed tests were NOT re-executed; every "measured" number below is the commit message's or the module doc's claim, flagged as such).

#### Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 2 |

Both delta commits that this dimension was asked to judge are correct as written. a53173780 does what it says and the rig gate keeps Oblivion out by design. a3a36f1c7's `entries_by_tree_order` / `body_tree_ranks` are internally sound (pre-order DFS in stored child order, visited set, budget, permutation + root-first check, atomic refusal before any write). The two surviving findings are about how well the new creature-rig admission is *pinned* and *documented*, not about wrong code.

---


#### Verified (delta commits)

##### a53173780 — Fix #6490 (corpse-pose parking has no `GameKind` term)
- `parks_authored_pose` is now `!pose.is_empty() && (placement_starts_dead || base_dead)` (`byroredux/src/ragdoll_pose.rs:703-709`); no `GameKind` import remains in `ragdoll_pose.rs`, `ragdoll.rs`, `ragdoll_pose_tests.rs` or `ragdoll_installed_tests.rs` (grep clean). Single production caller `byroredux/src/cell_loader/references/mod.rs:891-896` updated; the only other callers are the test census.
- Dead-base translation is at the record boundary, per game, and nowhere else sets it: Oblivion = header bit `0x80000` gated `game == Oblivion` (`crates/plugin/src/esm/records/dispatch_actor.rs:15-17`, applied with `|=` at `:45` NPC_ and `:85` CREA); FO3/FNV = DATA health <= 0, gated `GameKind::Fallout3NV` (`crates/plugin/src/esm/records/actor/npc.rs:~714` CREA 17-byte, `:~737` NPC_ 11/25-byte); Skyrim/FO4/FO76/Starfield: the NPC_ `DATA` arms are Fallout3NV-gated so `NpcRecord::starts_dead` stays false, the corpse marker is the placement flag (`crates/plugin/src/esm/cell/walkers.rs:1019-1021`, ACHR only, `Tes5Plus` header variant). The `starts_dead` doc (`npc.rs:163-188`) matches.
- The only behaviour change is Oblivion's 125 posed dead-base corpses now parking their pose. Consumer `combat.rs:1429-1442` takes the component (consumed whether or not it applies), logs at debug, and falls through to `activate_ragdoll`. Refusal is `pose_skeleton`'s final `else` (`ragdoll_pose.rs:898-901`): the body names must equal one of the exact verified sets, compared in template order, case-insensitive (`is_body_set`, `:668-676`). Oblivion's rig (18 bodies, spine-first, no `Bip01 Neck1`, `SideWeapon`) matches none; pinned by `the_unverified_oblivion_humanoid_rig_is_refused_by_the_rig_gate`, which also asserts the skeleton is untouched. So the exclusion is by the gate, not by accident. **Residual**: the pin uses the hand-transcribed `OBLIVION_HUMANOID_RAGDOLL_BODIES` (`#[cfg(test)]`, `:405`), not the real skeleton; a real-data pin like the FO3/FNV ones does not exist. Not filed (the gate fails closed: any other body set is refused too).
- FO3/FNV unchanged by construction (their base term was already game-true). The commit's 202/202/1 and 310/307/0 census was not re-run here.

##### a3a36f1c7 — Refs #6065 (eight FO4 creature rigs by node order)
- **`entries_by_tree_order`** (`ragdoll_pose.rs:734-760`): returns mapping[b] = rank[b], i.e. body b is posed by entry `ranks[b]`; matches the unit test (bodies 1,2,3 at ranks 3,1,2 -> `[0,3,1,2]`). Refuses: entry count != body count after `body_entries` strips a trailing marker; ranks not a permutation; body 0 not first in the order. `at_placement: k == 0` in `bone_poses_mapped` then lands on body 0 exactly because the first-body check guarantees entry 0 poses body 0. The marker keeps indices stable (it is last), so passing the unstripped `entries` to `bone_poses_mapped` is safe.
- **`body_tree_ranks`** (`:762-805`): pre-order DFS, `stack.extend(children.rev())` so pops follow stored order; visited-set check precedes `guard.step()`; budget is `world.next_entity_id()` (+1), identical to `collect_subtree` (`crates/core/src/ecs/hierarchy.rs:50-58`), so a cyclic `Children` graph terminates; every body must be reached or the walk errors (`next != bones.len()`); duplicate bone entities leave a rank `usize::MAX`, so also an error. All validation precedes any `Transform` write in `pose_skeleton`, so a refusal is atomic (except an impossible missing-`Transform` mid-loop, `:917`).
- **Order evidence**: stored child order == NIF child order on the live path (`nif_loader.rs:1576-1581`, children appended in node-index order, skeleton root = `node_entities.first()`, `:624`); gear/part attaches only append non-body children. The offset evidence (brahmin 37/37, radstag 29/29, yaoguai 16/16, mirelurk 22/22, molerat 17/17, stingwing 30/30, dog 21/21, radroach 20/21) lives in `scripts/xrgd-fo4-rigs.py` and the commit message, not in a Rust test (-> PHYS-D3-01).
- **Rig admission gate**: `is_fo4_tree_ordered_ragdoll` (`:725-727`) is an exact, in-order, case-insensitive match against eight name lists; `pose_skeleton` falls to the `Err` arm for everything else. Cat, Handy, FEV hound are refused by absence from the list, not by luck. `the_fo4_tree_ordered_creature_sets_are_recognised_by_name_only_in_full` pins one-short, swapped-pair and creature-set-is-not-id-mapped; the id-mapped branch is tested first (`:880`), and the sets are disjoint from the human/ghoul/super-mutant ones. Brahmin's 41/42-entry poses fail the count check (census: BrahminRace 10 of 13).
- **Creature rigs with non-zero bone ids**: routing is by body names alone and never reads `pose_bone_id` for the eight; a rig whose ids vary (human/ghoul/super mutant) is caught by the id-mapped branch first because its names are disjoint. A new rig with informative ids would have to be added to the right list; nothing cross-checks the two mappings against each other on one rig. Not a defect.
- **Hand-rolled walk allowlist** (`byroredux/src/workspace_hygiene_tests.rs:1298-1303`): scoped to the exact file `byroredux/src/ragdoll_pose.rs` with an exact count of 1 (`count != allowed` fails on a second walk), same needle, `assert_eq!(seen_allowed, ALLOWED.len())` fails if the file moves. Narrow enough. (Duplication note, not a finding: `body_tree_ranks` re-spells `collect_subtree`'s loop because that collector pushes children un-reversed; flipping the collector to push in reverse would let the allowlist entry go. Callers of `collect_subtree` document "order unspecified".)
- Observed, not filed: `is_trailing_marker` (`:602-610`) tests `bone_id == 0`, which is vacuous on creature rigs that author id 0 almost everywhere; a creature rig whose last real entry had x = 0, nonzero (y, z) and zero rotation would be stripped and then refused by the count check (a lost pose, never a wrong one). The census (56 of 59 creature corpses resolve per the pinned per-race table; the 3 misses are brahmin 41/42) shows no such corpse in vanilla.

##### 9a0ae0817 — Fix #6555 (ragdoll-relevant side only)
- `CollisionAuthoringSummary.external` and `authors_no_body` (`crates/nif/src/import/collision/mod.rs:100-119`) are additive; `needs_packed_collision_fallback` is untouched, so "authored but opaque" is still reported for FO4+ packed Havok. A skeleton NIF with `bhkRigidBody`/`bhkNPCollisionObject` bones has `classic`/`new_physics` > 0, so `authors_no_body` is false and the ragdoll path is not reachable by the new term. Rest is Dim 1.

##### 3c9949ec9 — toolchain/clippy
- Only ragdoll-path change: `ragdoll_pose_tests.rs` `for body in 0..20` -> `bones.iter().enumerate().take(20)` (identical iteration for the 20-bone molerat fixture) and an iterator rewrite in `crates/nif/examples/ragdoll_components.rs`. No behaviour change.

##### Also skimmed (touch Dim 3 paths indirectly)
- **68d3a834e (#6954)**: `crates/nif/src/blocks/collision/havok_packfile.rs` now reads the container through `byroredux_hkx::container`. Fixup offsets map back to section-relative identically (`absolute - section.start`); the sentinel stop, the 64-byte v11 section stride and the `0x3E` predicate area are handled. New strictness (version 8/11 only, `contents_version` must match, reuse-padding byte 0, section count 1..=64, ordered sections) could refuse a blob the old reader accepted; its only live consumer is `np_ragdoll.rs:309` (FO4 skeleton ragdoll blobs). The commit states all five `np_ragdoll_corpus` blobs still decode; not re-run here. Tagfile refusal (#6302) kept (`is_havok_tagfile_error` still keys on `Unsupported`).
- **eac3f73f9 (#6487)**: link impulses queued and delivered as `impulse/PHYSICS_DT` for substep 0, user force restored afterwards (`crates/physics/src/world/mod.rs` `deliver_link_impulses` / `release_link_impulse_forces`). Traced the interaction with `refresh_water_lift`: `before` is snapshotted after the water scan and link-damping pass, and substep 0's lift refresh sees the pre-impulse velocity, so the restore does not drop a meaningful lift delta. Non-finite impulses refused; despawned handles skipped; sub-tick frames keep the queue. No finding.

#### PHYSAL doctrine grep (`GameKind` / `bsver` / version constants)

Scope: `crates/physics/`, `byroredux/src/ragdoll.rs`, `byroredux/src/ragdoll_pose.rs` (+ `ragdoll_pose_tests.rs`, `ragdoll_installed_tests.rs`, `crates/nif/src/import/collision/ragdoll.rs`).

| Hit | Classification |
|---|---|
| `GameKind` / `bsver` / `NifVariant` in `crates/physics/**`, `byroredux/src/ragdoll.rs`, `byroredux/src/ragdoll_pose.rs`, its two test files, `import/collision/ragdoll.rs` | **None.** The `GameKind::Fallout3NV` hit from the 10-31 report (`ragdoll_pose.rs:575-578`) is gone (a53173780). |
| `crates/physics/src/sync.rs:820` ("FO4+ packed Havok" in a message string), `:2174` (`"Skyrim.esm"` in a test plugin id) | Prose / test fixture, not branches. (Plain "version" hits in `sync.rs:2702,2724` are the word "conversion".) |
| `crates/nif/src/blocks/collision/constraints.rs:674`, `:1011` (`stream.bsver() <= NI_BS_LTE_16`) | Parse-side: seam 1 (constraint CInfo decode), the one named in physal.md §3; correct place. |
| `crates/nif/src/lib.rs:92` `havok_scale_for` | Parse-side: seam 2; correct place. |
| `crates/nif/src/import/collision/shape.rs:93` `zero_is_authored_stone` (`scene.bsver < SKYRIM_LE`) | Parse-side (importer, material value 0 semantics, #5549), belongs upstream and does not reach the solver. It is not one of physal.md's three named seams; Dim 1 owns it. Not filed. |
| Collision-object-kind dispatch (seam 3) | Importer-side; not re-traced (Dim 1). |

Verdict: the solver side (`crates/physics/**`, `byroredux/src/ragdoll.rs`, `byroredux/src/ragdoll_pose.rs`) contains **no** per-game branch. The pose path's per-game knowledge is entirely (a) record-boundary translation of the corpse flag in the plugin crate and (b) the name-keyed rig sets.

#### Dedup notes
- Open, not re-filed: #6065 (refused rigs), #6677 (tilted FO4 corpses; the eight new rigs inherit the same untested placement composition), #6632 (physics body reclaim, outside this dimension).
- Closed checked: #6490 (fixed in a53173780, verified above), #6252, #5015, #6070, #5938 (no regression found).
- No open issue covers either finding (`/tmp/audit/issues.json` title scan for ragdoll / XRGD / corpse / pose / physal / smoke; closed search "XRGD creature").

#### Not verified here

*Merge note: the orchestrator ran the real-data gate in this pass, including `fo4_xrgd_poses_resolve_against_the_real_skeletons`, `fo4_xrgd_posed_ghoul_corpse_holds_together_in_free_fall` and the FNV/FO3 XRGD censuses (all green; see Executive Summary). The Oblivion skeleton was not extracted.*
Real-data runs (`-- --ignored ragdoll::installed_tests`, the FO4/FNV/FO3 XRGD censuses, `fo4-ragdoll.sh`) were not executed, so the commit-message counts (1,035 of 1,053; FNV 202/202/1; FO3 310/307/0; np_ragdoll_corpus 5) are unconfirmed. The Oblivion skeleton's real body list was not extracted (no prebuilt `skeleton_tree_dump`/`ragdoll_components` binary).

### Dim 4 — Character Controller, Grounding & the NPC KCC

Delta: `character.rs` (bda7fab73 crippled-leg mult, 49c92afbc sneak stance mult, bd7d634ba `PlayerEntity::get`, e8fdab51a `player_identity_sync_system`, 6390bfb90 river current → Dim 5), `player_body.rs` (a0b255b77 live-load rebuild, b05218f5d), `locomotion.rs` (no physics-path diff). `world/queries.rs` additions are Dim 2/6 (`movable_body_blocks_line`).

#### Verified
- `character_controller_system` (`byroredux/src/systems/character.rs:~357-370`): `speed_mul = sprint × encumbrance × stance_mult × limbs::locomotion_mult`. Both new factors are bounded at their source: `SneakMovement::resolve` (`crates/core/src/character/stance.rs:91`) admits a SPED ratio only for `d > 0 && s.is_finite()` and falls back to 1.0; `stance_speed_mult` returns 1.0 when not sneaking or with no resource; `leg_speed_mult` is the authored GMST row (CHARAL's lane). Diagonal normalisation still precedes the multiply.
- NPC movers take the crippled-leg factor through `limbs::actor_walk_speed` (`byroredux/src/limbs.rs:97`); the physics-side `step_toward_detailed` contract (actor-bone mask, owned vertical, `LOCOMOTION_NPC_SNAP_TO_GROUND` = 0) is untouched.
- a0b255b77: `sync_body_after_load` only raises `PlayerBodyRebuildRequest`; the rebuild is the existing exclusive `rebuild_player_body` (`byroredux/src/player_body.rs`): `collect_subtree` → `release_entities` → `attach_player_body` (strips `CollisionShape`/`RigidBodyData`) in one `&mut World` call, so no physics sync sees the unstripped body, and the capsule is untouched.
- New query call sites since baseline: `scene/character_spawn.rs:295` `cast_ray_down(.., None)` is the pre-existing spawn probe (only its guard scope moved, #6814; runs before the capsule exists, so `None` is correct); `faction_hostility` → `movable_body_blocks_line` passes the shooter/target bodies (Dim 2 verifies the query).
- Bin filter `ragdoll character locomotion water`: 315 passed, 17 ignored.

### Dim 5: WATAL Physics Sink (Buoyancy, Damping, Current, Swim)

**HEAD**: `e11de827c` · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-31.md` (HEAD `670f81917`) · **Scope**: delta since baseline (6390bfb90, b76b674df, dfcbcafd5, 22e0f71fc, f131c5275, 02453c0c3) plus a lighter pass over the rest of the Dim 5 checklist. Read-only; no cargo, no engine launch.

**Result**: 0 CRITICAL / 0 HIGH / 0 MEDIUM / 2 LOW. Both fixes under review (#6488, #6489) hold. The "two samplers must agree" rule for the river current now holds end to end. The two LOW findings are a latent gate-vs-sampler formula mismatch that production cannot currently observe, and doc rot.

Dedup: `/tmp/audit/issues.json` (123 open) has no water / buoyancy / swim / current / swell / drown title among the physics-relevant rows (only #6844 FO4 shallow creek render, #6486 god-ray perf, unrelated). Closed-issue searches (`gh ... --state closed`) for "contact rescan swell", "river current swimmer bed", "waves_require_contact_rescan" and "wind_wave_scale" return only #6489, #6488 (the delta fixes, verified in code below), #3122 (gate reachability, a different property) and the #5245/#5354/#3207 weather-damping family (already folded into the sampler). Neither finding below matches any of them.

---


#### Verified (delta commits)

##### 6390bfb90 — Fix #6488: swimming player's river current follows the bed
- `byroredux_physics::local_current(kind, flow, bed, x, z)` is defined once at `crates/physics/src/water.rs:507-514` and re-exported at `crates/physics/src/lib.rs:48`. It reshapes only `(River | Rapids, Some(bed))` when `sample_baked(x, z)` covers the point. Every other kind, a bed-less plane and a point outside the bed return the plane's flow verbatim.
- It has exactly two production callers. The dynamic path is `water.rs:1284-1286`, which feeds both the current force (`water.rs:1396-1402`, `current_force(flow, ..., frac, ...)`) and `WaterContact.flow` (`water.rs:1421`) from the one `local_flow`. The player path is `character.rs:1369-1375` inside `player_water_state`. `rg` finds no other reader of `WaterFlow` that feeds a force or a contact. `render/water.rs:148` is the GPU upload, `commands/water.rs:55` is the `water.dump` listing, and `cell_loader/water.rs:886` is placed-water merge. None needs the local form.
- Sample point: the player passes `pos.x, pos.z` (capsule centre = `Transform` translation, `character.rs:285-295`), and the same `pos` drives the surface containment and wave height. The dynamic path passes the collider-AABB-centre `reference_point`. For the capsule these coincide, so the "one reference point" rule holds for both.
- Marker: `placed_current_flow_at` (`character.rs:1246`) is independent of the plane. `player_current_drift` (`character.rs:1214-1240`) composes plane flow × `state.fraction` plus marker × 1.0 in the swim arm, and marker × 1.0 alone in the wade arm. The dynamic marker arm is `current_force(flow, vel, mass, 1.0, drag)` (`water.rs:1388-1397`), additive after the single force reset. The per-source scales agree.
- Published convention: `PlayerWaterState.flow` is the plane-only local flow and `sync_player_water_contact` writes it to `WaterContact.flow` (`character.rs:1447`), matching the dynamic `flow: local_flow`. The marker never enters either.
- Tests: `local_current_reshapes_only_rivers_over_a_covering_bed` (`water.rs` tests) and `a_river_current_over_a_shallow_bed_matches_the_dynamic_bodys_flow` (`character.rs:~2128`). The second compares the player's published flow with a `local_current` call, so a revert to the raw plane flow fails it.
- Nothing else regressed. `over_bed` keeps `self.direction[1]` and renormalises only XZ, so a river with a vertical component yields a non-unit vector. The player drift projects onto XZ and is unaffected. This predates the delta.

##### f131c5275 — river-current test lock order
- Test-only change (`character.rs:~2159-2163`): resolves the plane entity first, drops that read, then reads `WaterBedField`.
- Production order in `player_water_state` is `WaterPlane`, `WaterVolume`, `WaterFlow`, `WaterSurfaceMesh`, `WaterBedField` (`character.rs:1311-1317`). `collect_water_surfaces` has the same order (`water.rs:658-662`). `submersion_system` is `WaterPlane`, `WaterVolume`, `WaterSurfaceMesh`, `WaterBedField` (`systems/water.rs:~224-226`), a consistent subsequence. The test now follows the production order.

##### b76b674df — Fix #6489: contact-rescan gate counts the swell
- Reach uses the plane's kind-damped scroll (`surface.kind.weather_scroll_for(weather_scroll)`, `water.rs:813-819`), the same policy as the sampler at `water.rs:1198`. The swell term uses `WATER_SWELL_SHOAL_GAIN_MAX` (1.6), which is the maximum `amplitude_over` can return (`core/.../water.rs:739-746`), so it is a valid upper bound.
- A still plane (`wind_driven: false` and `wave_amplitude == 0`) gives reach 0, hence `<= 1e-4`, hence no rescan, hence it still quiesces. A swell-only plane with a wet contact keeps the rescan armed (`a_swell_only_plane_keeps_the_contact_rescan_armed`).
- The gate only enables the scan; it never wakes a body. Wake stays on `!prior_wet` or the settled-depth drift test (`water.rs:1301-1318`, `WAVE_WAKE_DEPTH_DELTA` 0.1). The documented "awake dynamic count never exceeds the wet floats" invariant is unchanged.
- Exception: see PHYS-D5-01 for the ripple half (sampler formula mismatch, band unreachable in production).

##### dfcbcafd5 — close five lock-order cycles
- Only `systems/water.rs` is in scope: `drop(bed_q)` before `drop(surface_q)`, `drop(wq)`, `drop(vq)`, and before the `query_2_mut::<WaterVolume, ParticleEmitter>` write (`systems/water.rs:~288-293`). `submersion_system` now holds no `WaterBedField` guard across a `WaterVolume` write.
- Guard added: `submersion_with_a_bed_field_takes_its_locks_in_one_order` (`systems/water.rs:~807`).
- The access rows declare the `WaterBedField` read for all three consumers: player controller (`boot/schedule/early.rs:65`), physics sync (`boot/schedule/physics.rs:50`) and submersion (`boot/schedule/late/water_hazards.rs:41`).

##### 22e0f71fc — Late-stage systems register from per-subsystem files
- `late.rs` is a flat call list (`boot/schedule/late.rs:16-31`): camera, footsteps, ragdoll, water_hazards, scripted_combat, audio, ...
- `water_hazards.rs` registers `submersion_system` as a Late exclusive and `water_damage_system` after it. It names `run_after(submersion, ragdoll_writeback)` and `run_after(water_damage, submersion)`.
- `scripted_combat.rs:90-96` names `reconcile_pending_dead_actors_system` `run_after` both `water_damage_system` and `scripted_kill_system` (#6702, #3119). There is one reconcile site, so the "second inline teardown" regression class is absent.
- `audio.rs` registers `make_water_interaction_system`, then `water_audio_system`, then `audio_system`, with `run_after(audio_system, water_audio_system)` named.
- Order holds: ragdoll → submersion → water_damage → (kill) → reconcile → make_water_interaction → water_audio → audio_system.
- Guard `scheduler_access_tests.rs::submersion_runs_after_camera_follow_and_before_water_audio` (`:763-845`) still pins camera_follow < submersion (exclusive after the parallel batch), ragdoll < submersion, submersion < water_audio < audio_system, and no earlier-stage copy.
- Observation, not a finding: `water_damage → reconcile → make_water_interaction → water_audio` is held by registration order plus the named `run_after` edges above. `make_water_interaction_system` is a closure, so it cannot be named, and no guard pins `interaction < water_audio` directly. Reordering lines in `audio.rs` would pass the current guard but lag splash audio by a frame.

##### 02453c0c3 — deleted pub items
- The only water item removed is `WaterKind::refracts` (`crates/core/src/ecs/components/water.rs`, 8 lines). A tree-wide grep finds no remaining reference. Nothing the Dim 5 checklist relies on was deleted: `WaterBedField`, `WaterSwell`, `WATER_SWELL_*`, `WaterFlow::over_bed`, `WaterKind::weather_scroll_for`, `WaterContact` and `WaterCurrentVolume` are all present.

#### Verified (other changes in the player paths since baseline; not water-specific)
- 49c92afbc (M65 sneak stance), bda7fab73 (limb slow) and 995ed089b feed `speed_mul` (`character.rs:363-366`). The swim stroke uses `controller.move_speed * speed_mul` (`character.rs:~380`), so the stance and limb multipliers also scale swim speed, as sprint (×2) and encumbrance already did. The swim damping, the surface clamp, `advance_breath` and the zero-dt breath behaviour are untouched (bd7d634ba / 039ff243c only change the `PlayerEntity` accessor; no lock scope moves). See Observation 1.
- Guards named in the checklist all still exist in the tree: `water_and_animation_parallel_accesses_are_complete`, `swim_damping_is_frame_rate_independent`, `zero_dt_tick_while_submerged_does_not_refill_breath`, `swimming_drift_scales_the_plane_by_fraction_and_the_marker_at_full_strength`, `the_marker_reaches_the_swimmer_through_drift_not_the_column_state`, `a_marker_outside_every_plane_still_drifts_the_player`, `swimlevel_predicate_agrees_between_pose_and_depth_forms`, `placed_current_volume_is_collected_without_becoming_a_water_surface`, `water_scan_latches_are_not_siphash`, `a_despawned_sleeper_leaves_no_rest_depth_latch`, `leaving_a_marker_only_current_releases_its_force`, `river_current_on_a_floating_body_follows_the_bed`.
- Volume depth is still the cell kind's (`cell_loader/water.rs:74-80`; the one production caller, `:630`, passes `interior`). `cell_loader/water.rs`, `commands/water.rs` and `components/water/` have no commits since baseline apart from the toolchain chore (3c9949ec9).
- One reference point: the union prefilter, current-volume containment and surface containment all read `reference_point` (`water.rs:1126-1175`).
- Two target sources (RigidBodyData scan + Ragdoll scan) are intact (`water.rs:~1018-1062`).
- Single force reset ahead of the surface and marker arms (`water.rs:1236-1247`).
- The marker-only link-damping arm and the `in_current_prev` release are intact.
- Surface height for mesh water goes through `volume.surface_y_at` in all three samplers (buoyancy, `player_water_state`, `submersion_system`).

#### Observations (not findings; need a decision or a source)
1. **Sneak stance and limb multipliers scale the swim stroke.** `stance_speed_mult` (M65) and `limbs::locomotion_mult` multiply `speed_mul`, which `swim_motion` also uses. The M65 PRD (`docs/prd/sneak-and-detection.md`) is silent on swimming, and I found no engine source for whether a sneak stance persists in water. Per the no-guessing policy this is recorded, not asserted. If the answer is "sneak does not apply while swimming", `stance_mult` should be gated on `swim.is_none()`. The pre-existing sprint ×2 on the swim stroke has the same shape.
2. **Every Calm-kind plane is wind-driven, including interior pools.** `env_translate/water.rs:668` sets `wind_driven = kind == Calm || still_river` with no interior check, and `WaterMaterial::swell` returns a swell at the 1.5 BU minimum even at zero wind (asserted by `swell_follows_the_wind_shoals_and_breaks`). A float in an interior pond therefore keeps riding a ~3 s swell and does not sleep. This agrees with `watal.md` §Phase 2 ("with the swell they ride a moving surface and stay awake"), so it is documented design. It means the SKILL line "calm planes still settle and sleep" is only true for the non-wind-driven kinds (authored rivers, falls). Consider rewording the SKILL line at the next sync.

### Dim 6 — Queries, Diagnostics & Cost

Delta: `47207ecb0` (`movable_body_blocks_line`) and `8124d9594` (#6689 `sync.rs` source-scan classification) — both traced by Dim 2 (see dim_2.md). `commands/physics.rs` has no commits since the baseline.

#### Verified
- New query call sites since the baseline (diffed `670f81917..HEAD`): only `faction_hostility::movable_object_blocks_fire` → `movable_body_blocks_line` (excludes its shooter/target body list) and the pre-existing `character_spawn.rs` spawn probe (pre-capsule, `None` exclusion correct). No new `cast_ray_corridor*`, `colliders_near_xz`, `surface_material_under` or `capsule_overlaps_solid` caller, so the self-exclusion and per-frame-allocation checks are unchanged from 10-31.
