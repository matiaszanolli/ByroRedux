**HEAD**: `a97d2123c` · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-11-15.md` (HEAD `e11de827c`) · **Audited**: all six dimensions, since each has commits since the baseline. Dim 1: refactors, #6116, #6549's docs. Dim 2: #7012–#7015, refactors, 256cf3d16. Dim 3: #7016, #7017, #7133, #7155. Dim 4: #7056, #7227, AIP-WALK-003, cam.fly. Dim 5: #7018, the wind-drift-per-layer change, #7033, the water shader contracts. Dim 6: new query callers. · **Unchanged since baseline (skimmed)**: `crates/physics/src/{convert,config}.rs`, `crates/physics/src/ragdoll.rs`

# PHYSAL / Physics Audit — 2026-11-22

## Executive summary

| Severity | New |
|---|---|
| CRITICAL / HIGH / MEDIUM | 0 |
| LOW | 6 |

Process:
- The six dimensions were traced by four read-only Sonnet sub-agents, which ran no cargo; Dims 1 and 6 shared an agent.
- Every agent finding was re-checked against the code before it was included here. One was dropped (see *Checked, not filed*).
- The engine was not launched.

Baseline tests at HEAD:

| Suite | Passed | Ignored |
|---|---|---|
| `byroredux-physics` | 278 | 0 |
| bin `ragdoll` | 67 | 14 (real data) |
| bin `character` | 69 | 0 |
| bin `locomotion` | 20 | 0 |
| bin `water` | 174 | 4 |

All nine findings from the 2026-11-15 audit are fixed:
- Dim 2: PHYS-D2-01..04, by #7012 / #7013 / #7014 / #7015.
- Dim 3: PHYS-D3-01 / -02, by #7016 / #7017.
- Dim 4: PHYS-D4-01, by #7020.
- Dim 5: PHYS-D5-01, by #7018; PHYS-D5-02, by `ebc2a3623`.

Each fix was traced and found correct, and they now serve as regression checks.

**PHYSAL verdict**: the three source-boundary seams are still the only per-game branches. No `GameKind` / `bsver` term appears on the solver side (`crates/physics/`, `byroredux/src/ragdoll.rs`). The new FO4 decoder gate (#7133, `file_version == 11` and 8-byte pointers) sits at the parse boundary.

## Solver invariant matrix

| Invariant | State | Evidence |
|---|---|---|
| Fixed step, accumulator clamp, substep budget | verified | `world/mod.rs` `step`; unchanged by the let-chain refactors (153f611a7, 607e2e9a1) |
| Wake discipline / spawn-asleep / impulse delivery | verified | #7012 release now clears only the user force (`reset_forces(false)`); #7015 refusal precedes the link/free split (`world/mod.rs:1226`) |
| Lock order / access declarations | verified | re-derived from call sites; the controller's accesses are declared in `early.rs` |
| Phase order (register → push → buoyancy → step → pull) | verified | `sync.rs` |
| Explosion recovery ladder | verified | constants and guard tests present at HEAD |
| Teardown / row retirement | verified | activation / `stand_up` / `remove_ragdoll` unchanged |
| Player teleports via `place_character_body` | verified | camera snap, door arrival, MoveTo, save restore, `combat.approach` / `cam.pos`, `cam.fly` toggle; #7227 resets `FallTracker` there |

## Findings

### PHYS-D2-01: Bodies registered through `register_newcomers_and_refresh_queries` never reach the buoyancy phase's newcomer escape
- **Severity**: LOW
- **Dimension**: Step & Sync
- **Location**: `crates/physics/src/sync.rs:220-239` (the exclusive helper); `crates/physics/src/sync.rs:143` (`apply_buoyancy(world, n_new > 0)`); `crates/physics/src/water.rs:987` (`… && !had_newcomers`)
- **Status**: NEW
- **Description**: The buoyancy phase skips its scan in a quiesced scene unless `had_newcomers` is set. That flag is only `n_new > 0` computed inside the scheduled tick. Bodies registered earlier by the exclusive helper are already registered when the tick runs, so the tick sees `n_new == 0`. The helper is used by scene setup (`byroredux/src/scene.rs:1082`, before the first tick) and by `combat.approach` (`commands/view.rs:356`). A dynamic body that streams in already submerged and spawned asleep, the case the escape exists for, misses its first-frame float-up when it arrives through that helper.
- **Evidence**: `register_newcomers_and_refresh_queries` calls `collect_newcomers` → `register_newcomers` → `update_query_pipeline()` and returns. Nothing records that newcomers were registered. Buoyancy then decides `quiescent = pw.active_island_counts().0 == 0 && !pw.pending_wake() && !had_newcomers`.
- **Trigger Conditions**: boot into a cell with submerged dynamic clutter, with no other newcomer or wake on the first tick. The player capsule is often a newcomer, and a teleport arms `pending_wake`, so this is usually masked.
- **Impact**: Submerged clutter can sit at its spawn pose underwater until something else wakes the scene.
- **Related**: #3969 (spawn-asleep contract), #5561
- **Suggested Fix**: Have the helper arm the same escape, for example `queue_wake` or a "registered outside the tick" latch that the next tick ORs into `had_newcomers`, and pin it with a test that registers a submerged body through the helper.

### PHYS-D5-01: The player's own `WaterContact` keeps the per-body buoyancy rescan armed
- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**: `crates/physics/src/water.rs:792-815` (`waves_require_contact_rescan`); `byroredux/src/systems/character.rs:1461-1471` (`sync_player_water_contact`)
- **Status**: NEW
- **Description**: `waves_require_contact_rescan` returns true when any `WaterContact` has a surface and is submerged or within the wave band. The kinematic player writes such a row while wading or swimming, so in an otherwise settled cell the full per-dynamic-body buoyancy scan runs every frame for as long as the player is in water. The player's water state comes from `player_water_state`, not that scan.
- **Evidence**: `contacts.iter().any(|(_, contact)| contact.surface_entity.is_some() && (contact.submerged_fraction > 0.0 || contact.depth.abs() <= max_amplitude + WATERLINE_HYSTERESIS))`, with no filter that excludes the player's row.
- **Trigger Conditions**: the player stands in, or swims in, wave-carrying water in a cell whose dynamic bodies have all settled.
- **Impact**: A per-frame cost that scales with the cell's dynamic bodies, paid only because of the player. Not measured.
- **Related**: #6489, #7018
- **Suggested Fix**: Skip contacts whose entity has no dynamic `RapierHandles` (or skip the player entity) in the rescan gate, and measure the scan cost before and after on a wading exterior.

### PHYS-D5-02: `make_water_interaction_system` → `water_audio_system` is ordered by registration order alone
- **Severity**: LOW
- **Dimension**: Water / Buoyancy (Late-stage chain), test gap
- **Location**: `byroredux/src/boot/schedule/late/audio.rs:22-40`
- **Status**: NEW
- **Description**: The skill's Late chain is submersion → `water_damage_system` → death reconcile → `make_water_interaction_system` → water audio. Only the first links are named `run_after` constraints. The interaction system is registered as an anonymous closure (`make_water_interaction_system()`), so it cannot be named, and the audio system follows it only because it is registered after it. `submersion_runs_after_camera_follow_and_before_water_audio` does not check this pair.
- **Evidence**: Two consecutive `scheduler.add_exclusive_with_access(Stage::Late, …)` calls: the interaction system, which writes `SplashEvent` / `RippleEvent`, then `water_audio_system`, which reads them. There is no `run_after` between them.
- **Trigger Conditions**: any reordering of these registrations, or a scheduler change to how same-stage exclusives are ordered.
- **Impact**: If the order flipped, the splash and ripple markers would be cleaned up before the audio read them, and water sounds would go silent with no failing test.
- **Related**: #5434, #6702
- **Suggested Fix**: Give the interaction system a stable registered name and a `run_after`, or extend the scheduler-order guard to assert this pair.

### PHYS-D5-03: `stance_use_system` banks Athletics / Sneak skill use while the player is dead, input-locked, or in the fly camera
- **Severity**: LOW
- **Dimension**: Character & NPC Controller (handoff: skill-use leveling, `/audit-character`)
- **Location**: `byroredux/src/skill_use_emit.rs:191-235`
- **Status**: NEW
- **Description**: The stance is derived from the raw `ActionState` keys, `Sneaking`, detection and `actor_is_swimming`. None of the gates the character controller applies are checked:
  - `Dead`;
  - `PlayerMode` (fly camera);
  - `player_accepts_movement_input` (`PlayerControlState`, `ActorControlState.restrained`).
  Holding a move key therefore advances the run and swim timers and queues `ATHLETICS` uses while the body cannot move.
- **Evidence**: `let moving = [MoveForward, MoveBackward, StrafeLeft, StrafeRight].into_iter().any(|action| actions.is_held(action));` feeds `stance_of(moving, …)`. There is no `Dead` / mode / input-acceptance check before `stance_step`.
- **Trigger Conditions**: Oblivion, with the player dead, restrained or script-locked, or in fly-cam mode, while holding a movement key.
- **Impact**: Unearned Athletics (and swim Athletics) skill progress.
- **Related**: #7051 (skill-use emitters, 72a73d765), #7056
- **Suggested Fix**: Gate the stance on the controller's own predicate. Moving should mean the controller actually accepted movement this frame, for example its post-move horizontal displacement, or `player_accepts_movement_input` plus Character mode plus not `Dead`.

### PHYS-D3-01: The tilted-corpse gap cites the closed #6070, not its open tracker #6677
- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam (doc rot)
- **Location**: `byroredux/src/ragdoll_pose.rs:96`; `docs/engine/physal.md:447`; `.claude/commands/audit-physics/SKILL.md` (Known-open register, "Open with no issue of its own"); `crates/nif/src/import/collision/np_ragdoll.rs:324` (`decode_np_ragdoll` doc)
- **Status**: NEW
- **Description**: Several places are stale:
  - The module doc heads the gap "Placement tilt (#6070, open)". #6070 is closed (it was the FO4 ragdoll smoke-test issue).
  - The gap is tracked by #6677 (OPEN: "FO4 tilted authored corpses … burst on load").
  - `physal.md` and the audit skill's register say the gap has no issue of its own.
  - `decode_np_ragdoll`'s doc does not mention the #7133 refusal (a packfile that is not 64-bit Havok 2014 returns `None`).
- **Evidence**: `//! **Placement tilt (#6070, open).**`; `gh issue view 6070` → CLOSED; `gh issue view 6677` → OPEN.
- **Trigger Conditions**: n/a (documentation)
- **Impact**: A reader following the citation lands on a closed smoke-test issue. The audit register invites a duplicate filing.
- **Related**: #6677, #6070, #7133
- **Suggested Fix**: Cite #6677 at all three sites, and add the #7133 refusal to `decode_np_ragdoll`'s doc.

### PHYS-D1-01: `physics.md` still says the architecture stand-in covers every collision-less model
- **Severity**: LOW
- **Dimension**: Shape Translation (doc rot)
- **Location**: `docs/engine/physics.md:563-565`
- **Status**: NEW
- **Description**: The doc says the `ArchitectureTriMesh` path "remains available for collision-less architecture even when the NIF does not contain a packed object". Since #6549 (`0c35505ba`), `missing_collision_fallback` returns `None` for a classic-dialect model that authored no body (`authoring.walk_through_by_authoring()`). Those models get no stand-in: FNV sky planes and cave fungus stalks, Skyrim water planes. The stand-in remains only for authored-but-declined collision and for FO4+ / unknown provenance.
- **Evidence**: `byroredux/src/cell_loader/spawn.rs` `missing_collision_fallback`: `if authoring.walk_through_by_authoring() { return MissingCollisionFallback::None; }`.
- **Trigger Conditions**: n/a (documentation)
- **Impact**: A reader expects a solid stand-in under every collision-less architecture mesh and would misread the #6549 walk-through as a regression.
- **Related**: #6549, #6555
- **Suggested Fix**: Restate the stand-in's scope as the `missing_collision_fallback` rule (classic dialect with nothing authored → none).

## Checked, not filed

- **The `update_query_pipeline` caller clause on `line_of_sight_blocked`, `movable_body_blocks_line` and the support ray** (`world/queries.rs:392-397`, `:424-427`, `:464-468`): an agent read it as telling per-frame callers to run the full refresh. Each clause defers to the base probe contract ("the same contract as the other probes" / as `cast_ray_down`), which reads "`update_query_pipeline` (or stepped)", so it is not misleading.
- **`set_linear_velocity` does not refuse a non-finite vector**: its only production caller is finite by construction.
- **`walk_through_by_authoring` ignores `scene.truncated` / `recovered_blocks`**: there is no corpus evidence that this fires.
- **The ragdoll census computes node-order ranks with a test-local function**: only the molerat free-fall test runs production `body_tree_ranks` end to end. Coverage note only.
- **Dim 4 observations** (an airborne ray starting inside a fixed collider; the ~24 BU fall-damage threshold against the 32 BU step-down; the ramp retry's second sweep per blocked tick): each was checked and dropped for lack of evidence, or because it sits inside a declared gap.

## Known-open register (this pass)

- Unchanged and still open: #6065 (FO4 XRGD rigs still refused), #6677 (tilted FO4 corpses, now named as the tilt tracker, see PHYS-D3-01), #6632 (17 FO4 soak bodies), #6048 (terrain LTEX Havok data).
- The register's "open with no issue of its own" line for the tilted FO4 corpse is wrong: #6677 tracks it.
- Skill drift for `/audit-sync`:
  - Dim 3 `Paths` omits `ragdoll_pose_tests.rs`, `ragdoll_skeever_tests.rs` and `havok_packfile.rs`.
  - Dim 6 should list the Touch-spell caller of `cast_ray_corridor` (`combat::actor_in_reach`).
  - `movable_body_blocks_line` now takes `&[RigidBodyHandle]`.
  - Only the first links of the Dim 5 Late chain are named `run_after` constraints (PHYS-D5-02).
