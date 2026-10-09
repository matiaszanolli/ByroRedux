**HEAD**: 00f580e09 · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-05.md` (HEAD `a2c24b16e`) · **Audited**: Dim 2 Step & Sync, Dim 4 Character & NPC Controller · **Unchanged since baseline (skimmed)**: Dim 1 Shape Translation, Dim 3 Ragdoll & Constraint Seam, Dim 5 Water / Buoyancy (no water-code commits; `late.rs` changed only in a dialogue Access row), Dim 6 Queries & Diagnostics (the query impl moved verbatim, see Dim 2)

# PHYSAL / Physics Audit — 2026-10-08

**Run**: `/audit-physics` (default scope, deep), one leg of `/audit-suite --preset comprehensive`. One auditor
covered all six dimensions; no sub-agents were used. Per-dimension notes are in `/tmp/audit/physics/dim_{1..6}.md`.
The Phase 3 `rm -rf` was skipped on purpose; the suite deletes the directory.

**Delta audited**: `a2c24b16e..00f580e09` (116 commits workspace-wide). Six touch physics scope:

- `12ca34a70` #5311: `crates/physics/src/world.rs` split into `world/{mod,queries,recovery}.rs`.
- `655b317c9` #5100: the physics `source_scan` now re-exports core's `production_text`.
- `8c925ec54` #5095 and `7c7711cff` #5079: `player_body.rs` (PNAM head-part fallback, shared `is_child_race`).
- `14cff35ae` #5367 and `b08a201bf` #5307: `late.rs` Access rows for dialogue (not physics).

Two new callers of the NPC KCC step also landed outside the listed paths: `forcegreet_system` (`14cff35ae`) and
`eat_sleep_system` (`00f580e09`).

**Tests**: run with `-j 4` and `TMPDIR=/mnt/data/tmp`. The bin crate used the 1.96.0 toolchain cargo.

| Lane | Result |
|---|---|
| `cargo test -p byroredux-physics` | **205 passed**, 0 failed, 0 ignored (baseline 206: `source_scan`'s own unit test moved to core in `655b317c9`) |
| `cargo test -p byroredux --bin byroredux -- ragdoll` | 30 passed, 7 ignored (the 6 FO3-data tests and the skeever probe; not run) |
| `… -- character` / `-- locomotion` / `-- water` | 60 / 6 / 140 passed (water: 4 ignored) |
| `… -- scheduler_access` / `-- rapier_release` / `-- player_body` | 26 / 9 / 12 passed |
| `… -- eat_sleep` / `-- forcegreet` | 4 passed / **0 tests exist** |

No engine was launched. The only repo file written is this report.

---

## Executive Summary

| Severity | NEW | Regression | Existing (matched, cited) |
|---|---:|---:|---:|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 1 (#5352) |
| MEDIUM | 0 | 0 | 3 (#5353, #5354, #4772) |
| LOW | **2** | 0 | 5 (#5355, #5356, #5357, #5272, #4134) |

**The #5311 split preserved behaviour.** A line-multiset diff of the old `world.rs` against the three new files
shows one logic change. The substep loop body moved into `run_substep`, which returns `SubstepOutcome`.

- `step` still counts the stopping substep (`steps += 1` before `break`, `world/mod.rs:829-835`), so a wake
  survives a zeroed accumulator.
- The restore arm still zeroes the accumulator and returns before `clamp_explosive_velocities`
  (`world/mod.rs:969-987`), so #5352 is unchanged. It was neither fixed nor made worse.
- The capsule source-scan now covers all three files (`world/mod.rs:1039-1049`).

The move did drop two derives and two doc blocks; see PHYS-D2-2026-10-08-01.

**The new force-greet and Eat/Sleep movers misuse the KCC step** (PHYS-D4-2026-10-08-01, LOW):

- Both call `step_toward`, which discards the KCC `blocked` flag.
- Both walk a straight line with no `NavPath` waypoints. Their module docs claim navmesh routing anyway.
- Both finish only on reaching a fixed radius, so an actor the KCC blocks short of that radius walks into the
  obstacle forever.

This is a different fault from ECS-2026-10-08-D6-01, which is that the step is never committed. That bug masks
this one today, and this one goes live once ECS-D6-01 is fixed.

### PHYSAL doctrine verdict: **HOLDS** (tenth consecutive pass)

```
$ grep -rnE "GameKind|game_kind|bsver|NifVersion|is_skyrim|is_fo4|is_oblivion|is_fo3|is_fnv|game ==|BS_F76|SF_FORM_ID|havok_scale|GameVariant" \
    crates/physics/src/ byroredux/src/ragdoll.rs byroredux/src/systems/{character,locomotion,water,eat_sleep,forcegreet}.rs \
    byroredux/src/commands/{physics,water}.rs
(no matches)
```

The three named seams (constraint CInfo decode, `havok_scale_for`, collision-object-kind dispatch) remain the only
per-game branches. No game's collision data was traced this pass: there was no engine launch, and the FO3 installed
tests were not run.

---

## Solver Invariant Matrix

| Invariant | Verdict | Note |
|---|---|---|
| Fixed step: accumulator clamped before the loop; `frame_dt.max(0.0)` | **HOLDS** | `world/mod.rs:744-748` |
| Anti-spiral wall-clock budget (#1698) | **HOLDS** | checked after the clamp in `run_substep` (`:993-996`) |
| Static-scene fast path; spawn is exempt from wake (#3969) | **HOLDS** | `:808-817`; the `wake_contract_tests` were re-pointed at `mod.rs` and `../sync.rs`, and pass |
| Wake survives a zeroed accumulator | **HOLDS** | `Stop` → `steps += 1; break` |
| Collider-set mutation reaches the query pipeline | **HOLDS** | incremental `Some(&mut self.query_pipeline)` (`:958`); stepless dirty frames rebuild (`:892-895`) |
| Explosion recovery: snapshot before every substep, delta bound, parking | **HOLDS** | `run_substep` `:918-930`; `MAX_DYNAMIC_SUBSTEP_DISPLACEMENT` moved to `recovery.rs` unchanged |
| Pre-explosion clamp at the end of every substep | **DRIFTED (Existing #5352)** | still skipped on restore substeps; DOF cap still applies to the free root (#5353) |
| Fixed-on-fixed broad-phase filter | **HOLDS** | the only production `set_body_type` is `set_motion_type` (`:652`) → `body_left_fixed` |
| Lock ordering (`PhysicsWorld` is a sink) | **DRIFTED, owned elsewhere** | the new movers hold `PhysicsWorld` across component guards: CONC-D5-2026-10-08-01 |
| Phase order collect → push kinematic → buoyancy → step → pull | **HOLDS** | `sync.rs` unchanged |
| Teardown completeness | **HOLDS for Rapier sets; side tables Existing** | #5272 (`remove_body` prunes `body_labels` only, `:430-453`); #5355 (`articulation_joints`) |
| Player physics presence = one capsule | **HOLDS** | the #5095 head parts are parented under the placement root before `attach_assembled_root` strips collision from all descendants (`player_body.rs:279-298`) |
| NPC KCC callers consume the step contract | **DRIFTED** | PHYS-D4-2026-10-08-01 |
| Water samplers agree | **DRIFTED (Existing)** | #5354, #5357 unchanged |
| PHYSAL doctrine | **HOLDS** | tenth pass |

---

## Findings — LOW

### PHYS-D4-2026-10-08-01: The force-greet and Eat/Sleep movers drive the NPC KCC through `step_toward`, discarding `blocked` and skipping navmesh waypoints, so a KCC-blocked actor never completes its procedure
- **Severity**: LOW. Latent: ECS-2026-10-08-D6-01 currently stops these actors moving at all.
- **Dimension**: Character & NPC Controller
- **Location**:
  - `byroredux/src/systems/forcegreet.rs:44-48` (the doc claims routing), `:80-97`
  - `byroredux/src/systems/eat_sleep.rs:17-19` (doc), `:38` (`ARRIVE_RADIUS`), `:121-140`
  - The contract: `byroredux/src/systems/locomotion.rs:134-152` (`step_toward`), `:154-158` (`blocked`), `:17-26`
    (`step_toward` "knows nothing about NAVM")
- **Status**: NEW. Introduced by `14cff35ae` and `00f580e09`. Distinct from ECS-2026-10-08-D6-01 (the step is
  written to `GlobalTransform`) and CONC-D5-2026-10-08-01 (lock hold); it cross-references both.
- **Trigger Conditions**: either condition below, once ECS-D6-01 is fixed so the step is committed:
  - a force-greet or Eat/Sleep actor whose straight line to its goal crosses solid architecture;
  - a destination the KCC capsule cannot reach within the arrival radius.

  The radius is 64 BU for Eat/Sleep and the directive radius (default 128) for force-greet. A `NearReference`
  `PLDT` resolves to the referenced REFR's origin (`travel::resolve_near_reference_target`). If that REFR is solid
  furniture whose collider extends more than about 40 BU from its origin along the approach, and is too tall to
  autostep, the actor can never reach 64 BU: the capsule radius is 20 BU, the KCC offset is 4 BU, and the step
  height is 32 BU. Real furniture extents were not measured.
- **Description**: M42.10 gives the KCC step two outputs that callers must handle.
  - `step_toward_detailed` reports `blocked` (less than 25% of the requested XZ move delivered). The oscillating
    walkers consume it through `advance_stuck_repick`.
  - Every frozen-goal mover routes through `step_along_waypoints`, which steps toward the next resident-tile
    `NavPath` waypoint and pins `target.y = current.y`. That mover family is Travel, Guard, Escort and Follow.

  The two new movers call bare `step_toward`, so they get neither:
  - `blocked` is discarded. Neither system has a stuck timer, a give-up or a re-pick.
  - They follow no waypoints and walk straight at the goal. The docs nevertheless say "navmesh-routing when a
    resident tile covers the actor" (`forcegreet.rs:47-48`) and "KCC-backed, single resident NAVM tile"
    (`eat_sleep.rs:18`), while `locomotion.rs:17` says `step_toward` "knows nothing about NAVM".
  - Completion is a pure distance test (`flat.length() > ARRIVE_RADIUS` / `> directive.radius`). The KCC can hold
    an actor outside that radius forever, either at a wall on the straight line or at the destination furniture's
    own collider. The actor then grinds in place every frame, and Eat/Sleep never reaches `seat_at_marker`.

  A minor point: `eat_sleep` passes the resolved `destination`, whose `.y` is not `current.y`. This violates
  `step_toward`'s documented contract (`locomotion.rs:134-137`). On the KCC path it is harmless, because the move is
  computed from XZ only. On the no-physics fallback, `move_towards` drifts Y toward the authored or hash-picked
  height, and the XZ stride shrinks by the Y component.
- **Evidence**:
  ```rust
  // eat_sleep.rs:130 (forcegreet.rs:87 is the same shape)
  let (new_pos, new_rotation) = crate::systems::locomotion::step_toward(
      current, /* rotation */, destination /* .y != current.y */, dt, speed, physics);
  // vs travel.rs:291 — step_along_waypoints(p.current, p.rotation, p.waypoints, p.destination, …)
  // vs wander.rs:212 — step_toward_detailed(…) → advance_stuck_repick(…, blocked, dt)
  ```
  `forcegreet.rs` has no tests. The `eat_sleep` tests run with no `PhysicsWorld`, so none of them exercises the KCC.
- **Impact**: after ECS-D6-01 is fixed, any FO3/FNV Eat/Sleep actor whose path or destination is obstructed walks
  into the obstacle indefinitely and never sits. A force-greet that starts behind a wall or counter never opens.
  PERF-D1-2026-10-08-03 (per-frame furniture gather for an arrived-but-unseatable actor) is the cost-side sibling.
  Here the actor never arrives at all.
- **Related**: ECS-2026-10-08-D6-01 (masks this finding; fix it first); CONC-D5-2026-10-08-01; PERF-D1-2026-10-08-03.
  `/audit-gameplay` owns the behaviour policy (give-up versus re-pick); the KCC contract misuse is reported here.
- **Suggested Fix**:
  - Route both movers through `step_along_waypoints` with a cached `NavPath`, as Travel does, which also fixes the
    Y contract. Then correct the two module docs.
  - Consume `blocked` through `step_toward_detailed`. For Eat/Sleep, attempt the seat pick once the actor is
    blocked within the seat-search radius. For force-greet, drop the directive or open in place after
    `LOCOMOTION_STUCK_REPICK_SECS`.
  - Add a `PhysicsWorld`-backed test with a wall between the actor and its goal.

### PHYS-D2-2026-10-08-01: The #5311 "move-only" world split dropped `CharacterMoveResult`'s `Debug`/`Clone`/`Copy` derive and two doc blocks, and left the step comment pointing at a `None` that moved into another function
- **Severity**: LOW
- **Dimension**: Step & Sync (doc and API hygiene)
- **Location**:
  - `crates/physics/src/world/queries.rs:12` (`pub struct CharacterMoveResult`, re-exported at
    `crates/physics/src/lib.rs:51`)
  - `crates/physics/src/world/recovery.rs:38-42` (`DynamicBodySnapshot`)
  - `crates/physics/src/world/mod.rs:761-764`
- **Status**: NEW. The derive and doc loss is a regression introduced by `12ca34a70` (#5311). The stale comment is
  pre-existing since #4685 (`"None" is passed`).
- **Description**: a line-multiset diff of `12ca34a70^:crates/physics/src/world.rs` against the three new files
  finds only these non-mechanical differences, besides `run_substep` and the re-pointed `include_str!`s:
  - Removed: `#[derive(Debug, Clone, Copy)]` and the "Result of a `move_character` step. Mirrors Rapier's
    `EffectiveCharacterMovement` … See M28.5" doc on the public `CharacterMoveResult`. The type is now not
    `Debug`, `Clone` or `Copy`.
  - Removed: `DynamicBodySnapshot`'s seven-line rationale. It recorded why the snapshot is per-substep, not
    per-frame ("a long catch-up frame must not roll a body back farther than the one solve that corrupted it").
    That invariant is load-bearing for the #4687 recovery design and now appears nowhere in the code.
  - Still present: the step rationale says the per-substep rebuild "is removed forty lines below (`None` is passed
    for the query pipeline)". Since #4685 the step passes `Some(&mut self.query_pipeline)`, and since #5311 that
    call is in `run_substep`, about 190 lines below. The guard
    `step_cost_rationale_is_scoped_to_history_and_names_the_real_cost_centre` does not check this sentence.
- **Impact**: none at runtime. A caller can no longer `{:?}`-log or copy a `CharacterMoveResult`, a public type in
  the controller API. The deleted snapshot rationale is the kind of doc an auditor re-derives from first principles
  each pass, and the stale `None` sentence contradicts the paragraph directly below it.
- **Related**: #5311 (closed), #4685, #4687.
- **Suggested Fix**: restore the derive and both doc blocks from `12ca34a70^`. Reword the stale sentence to "removed
  by `6e55b492`; today the step passes `Some(&mut self.query_pipeline)` (see `run_substep`)", and extend the
  existing rationale guard with `!rationale.contains("`None` is passed")`.

---

## Existing (matched; cited, not re-counted)

All re-confirmed at HEAD; the code is unchanged since 2026-10-05 except for file location.

| Issue | Severity | Location now | Note |
|---|---|---|---|
| #5352 restore substep skips the clamp | HIGH | `world/mod.rs:969-987` | the comment "skipping the clamp there loses nothing" moved verbatim into `run_substep` |
| #5353 DOF cap catches the free root | MEDIUM | `world/recovery.rs:400` loop | unchanged |
| #5354 flowing-water damping missing in buoyancy/player | MEDIUM | `crates/physics/src/water.rs`, `systems/character.rs` | no commits |
| #4772 FO3 restore first solve to about 1e12 | MEDIUM | — | unchanged |
| #5355 `articulation_joints` never pruned | LOW | `world/mod.rs:345`, `ragdoll.rs:529` | unchanged |
| #5356 clamp doc pre-#5246; detach over-count | LOW | `world/recovery.rs:281+` | unchanged |
| #5357 swimming marker drift × fraction | LOW | `systems/character.rs` | no commits |
| #5272 `remove_body` prunes only `body_labels` | LOW | `world/mod.rs:430-453` | unchanged (`/audit-safety` owns it) |
| #4134 floor-only `.max(1e-3)` capsule | LOW | `world/queries.rs:689` (`character_capsule`) | moved by #5311, one site |

Also open, with no labels: #5155 (P5 soak: `grounded=false` after the 10th F5→door→F9 cycle). It is physics-adjacent
and was not re-investigated here because it needs an engine run.

## Known-Open Register (what this pass changed)

- Nothing closed in the window. #5352–#5357 (this audit's 2026-10-05 findings) are all still open and unchanged.
- #4134 moved to `world/queries.rs:689`. The skill's Dim 1 text ("`world/mod.rs` and `world/queries.rs`") is
  accurate.
- The upstream instability of mass-inverted rigs, the decoded-but-declined ball-and-socket / stiff-spring / chain
  constraints, and WATAL water-walking and freezing are all unchanged and not filed.
- Skill drift for the next `/audit-sync`:
  - Dim 4's caller list should name `forcegreet.rs` and `eat_sleep.rs` as `step_toward` consumers.
  - Dim 2 should note that the substep body is `run_substep`.

## Cross-Audit Deduplication

| Topic | Owner | Note |
|---|---|---|
| Force-greet and Eat/Sleep step written to `GlobalTransform` | `/audit-ecs` ECS-2026-10-08-D6-01 | not re-filed; it masks PHYS-D4-2026-10-08-01 |
| Same movers hold `PhysicsWorld` across component guards | `/audit-concurrency` CONC-D5-2026-10-08-01 | not re-filed |
| Arrived-but-unseatable Eat/Sleep actor re-gathers furniture every frame | `/audit-performance` PERF-D1-2026-10-08-03 | cost of the arrived case; PHYS-D4-01 is the never-arrives case |
| `remove_body` side tables | `/audit-safety` #5272 | cited |

## Verification notes

- **PHYS-D4-01**:
  - Read both movers, `step_toward` / `step_toward_detailed` / `step_along_waypoints`, and every other
    `step_toward*` caller: Travel, Follow, Escort and Guard use waypoints; Wander and Patrol consume `blocked`;
    combat and cinematic use bare `step_toward` with their own live goals.
  - Read `resolve_destination` → `resolve_near_reference_target` (it returns the REFR's `GlobalTransform` origin).
  - Disproof attempted: a mover that cannot complete is a gameplay problem only if blocking is reachable. KCC
    blocking against fixed colliders is the whole point of M42.10, and the radius arithmetic uses code constants
    only. The real-furniture footprint was not measured, so the finding is LOW and the trigger is stated as a
    condition.
- **PHYS-D2-01**: line-multiset diff of the pre-split file against the three new files (scratch
  `/tmp/audit/physics/{old_world,new_*}.rs`), confirmed with `grep` at HEAD.
- Dedup:
  - `/tmp/audit/issues.json` (113 open) was filtered by physics keywords and labels.
  - Closed-issue searches for "CharacterMoveResult derive" and "eat_sleep OR forcegreet step_toward" returned
    nothing.
  - Today's sibling reports were grepped for `step_toward`, `eat_sleep` and `forcegreet` (ECS, concurrency and
    performance hits are cited above).

Next step: `/audit-publish docs/audits/AUDIT_PHYSICS_2026-10-08.md`. Suggested labels:

- `physics` on both findings;
- `ai` and `test-gap` on PHYS-D4-01;
- `doc-rot` on PHYS-D2-01.
