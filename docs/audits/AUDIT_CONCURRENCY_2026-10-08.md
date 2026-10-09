**HEAD**: `00f580e09` · **Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-10-05.md` (@ `a2c24b16e`, 116 commits ago) · **Audited**: Dims 3, 4, 5 in full (the new Story Manager, dialogue force-greet, Eat/Sleep and dialogue-voice code, plus the lock-order lanes); Dims 1, 2, 7 read for their deltas (6, 8 and 7 commits; no barrier, layout or thread-shape change) · **Unchanged since baseline (skimmed)**: Dim 6 (its `Paths:` changed only in two renderer test-source pins; `teardown.rs`, `resize.rs`, `buffer.rs`, `image.rs`, `egui_pass.rs` and `scene_buffer/` have zero commits). Zero-commit sub-paths, guard spot-checked only: `crates/core/src/ecs/{world,lock_tracker,access,scheduler}.rs`, `byroredux/src/render/`, `crates/bsa/src/read_at.rs`, `byroredux/src/asset_provider/texture_prefetch.rs`, `crates/ui/src/player.rs`.

# Concurrency & Synchronization Audit — 2026-10-08

**Command**: `/audit-concurrency` (all dimensions, depth `deep`), run inside `/audit-suite --preset comprehensive`.
**Severity scale**: `.claude/commands/_audit-severity.md`.

## Method

- **Delta scope.** `git log a2c24b16e..HEAD -- <Paths>` per dimension. All seven dimensions were analysed synchronously in this
  session (no sub-agents). Per-dimension notes are in `/tmp/audit/concurrency/dim_{1..7}.md`; this report was reconciled against
  each of them.
- **No engine launch** (suite rule). The Vulkan evidence is the CI `vulkan-validation` job log of run 37833008690, read with `gh`.
- **One scratch experiment, outside the tree.** To turn the two HIGH findings from code reading into measured panics, I copied the
  source (no `.git`, no `target/`) to a scratch directory, applied three test-only edits there, and built it with a separate
  `CARGO_TARGET_DIR`. The tracked tree was never touched; the copy is deleted. The edits are saved in
  `/tmp/audit/concurrency/experiment_edits.diff` and the raw log in `/tmp/audit/concurrency/experiment.log`. Details are under
  each finding.
- **Dedup.** Sources checked:
  - open issues (`/tmp/audit/issues.json`, 113);
  - closed-issue searches for each finding (`PhysicsWorld GlobalTransform lock order`, `forcegreet lock`, `debug-cli.md component count`);
  - the baseline report `AUDIT_CONCURRENCY_2026-10-05.md`.

### Guard runs at HEAD (local, rustc 1.96.0, `TMPDIR=/mnt/data/tmp`)

| Command | Result |
|---|---|
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --no-fail-fast` | bin: **2705 passed, 0 failed**, 55 ignored. The five baseline failures (ECS-2026-10-05-D1-01) are gone (#5305). |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-scripting --no-fail-fast` | **524 passed, 0 failed**, 3 ignored |
| the scheduler-proof tests inside the bin run (`scheduler_access_report_tests`, `system_access_declaration_tests`, `scheduler_access_tests`) | 33 ok, 0 not-ok, none ignored |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-physics --no-fail-fast` | **205 passed, 0 failed** (206 at baseline; the −1 is the `source_scan` self-test #5100 moved to core) |
| `cargo test -p byroredux-renderer --lib -- <the skill's guard filters + tlas_scratch + one_time_failure>` | 164 passed, 0 failed |
| `cargo test -p byroredux-bsa --lib -- concurrent threaded parallel` | 3 passed |
| `cargo test -p byroredux-debug-server --lib -- roster` | **1 failed**: `debug_cli_component_counts_match_the_registry` ("debug-cli.md component heading must report 70"). See CONC-D3-2026-10-08-04. |
| Scratch copy: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux --no-fail-fast` with the three test-only edits | 2697 passed, **9 failed**, all nine lock-order-cycle panics at `lock_tracker.rs:476`. See F1 and F2. |

CI evidence (read with `gh`):

| Run | Commit | Lane result |
|---|---|---|
| 37804953450, 37806461022 | `f8950e7cc`, `5c81a2f90` | **ABBA lane red**: 8 `story_manager::tests::*` panics, `SceneActorBindings → QuestStageState → SceneActorBindings`. Fixed by `cbaf782db`. |
| 37807728116 | `cbaf782db` | ABBA green |
| 37833008690 | `287214103` | all 10 jobs green. The vulkan-validation lane printed `Selected GPU: "llvmpipe …"`, `rt-integrity: … rt_flag=1 tlas_build=1 tlas_eligible=4 tlas_emitted=4`, zero `[Vulkan]` lines, bench exit 0. |
| 37848932617 | `00f580e09` (HEAD) | ABBA lane **red** and Test+Check+Clippy **red**, both on the same non-lock test (CONC-D3-2026-10-08-04). Vulkan-validation lane green. |

## Summary

| Severity | NEW | Regression | Existing |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 2 | 0 | 0 |
| MEDIUM | 0 | 0 | 0 |
| LOW | 4 | 0 | 2 (#5066 status, #5069 unchanged) |

**Headline.**
- **All four baseline findings are fixed and confirmed live**: CONC-D1-2026-10-05-01 (#5250), CONC-D2-2026-10-05-01 (#5261),
  CONC-D3-2026-10-05-01 (#5263), CONC-D1-2026-10-05-02 (#5265). The vulkan-validation lane is green, has RT back on, and prints no
  validation message.
- **The Vulkan side is quiet.** Dims 1, 2 and 6 saw only shader math, constants, doc fixes, a file split and one fault-arm change
  in the one-time helper. No barrier, layout, descriptor or destroy ordering moved.
- **The new gameplay systems are where the lock hazards are.** The M42 / #5366 / #5367 wave added four systems and a new SM
  producer, and two of them break documented lock rules in ways no test exercises:
  1. `forcegreet_system` and `eat_sleep_system` hold the `PhysicsWorld` guard across storage acquisitions, against the documented
     "`PhysicsWorld` is a sink" rule and the `Transform/GlobalTransform → PhysicsWorld` edges production already records.
  2. `raise_hello_story_event` (SM Phase 4) takes `LoadedCellIndex` while holding the `StoryEvent` write guard. It is called
     under the shadowed `LoadedCellIndex` guards that open issue #5066 describes, so the cycle #5066 only predicted now closes on
     the first dialogue opened in a detector build.

  Both were reproduced as detector panics in the scratch copy. Neither is a runtime deadlock today: every participant is an
  exclusive system. Neither is visible to the lane today either: `forcegreet_system` has no unit test at all (only the live `dt1`/`dt2` smoke
  scripts, which the lane does not run), the three `eat_sleep_system` tests install no `PhysicsWorld`, and the dialogue fixtures do
  not register `StoryEvent`.

| ID | Sev | Dim | Status | Title |
|---|---|---|---|---|
| CONC-D5-2026-10-08-01 | HIGH | D5 | NEW | `forcegreet_system` and `eat_sleep_system` hold the `PhysicsWorld` guard across storage acquisitions — the sink rule #2134/#3262/#4325 enforce — and record `PhysicsWorld → Transform/GlobalTransform` against production's `Transform/GlobalTransform → PhysicsWorld` |
| CONC-D3-2026-10-08-01 | HIGH | D3 | NEW (activates #5066) | `raise_hello_story_event` reads `LoadedCellIndex` under the `StoryEvent` write guard, inside the shadowed `LoadedCellIndex` guards of the dialogue entry points: `LoadedCellIndex → StoryEvent → LoadedCellIndex` closes on the first conversation |
| CONC-D4-2026-10-08-01 | LOW | D4 | NEW | `npc_dialogue_selection`'s Access row omits the `StoryEvent` write and the dialogue-voice resources; the declaration scan cannot follow the two cross-file hops |
| CONC-D4-2026-10-08-02 | LOW | D4 | NEW | The new Update-stage ordering contracts (SM dispatch before the alias refresh, CLOC producer before dispatch, force-greet between ambient and Late selection) are comments only |
| CONC-D3-2026-10-08-03 | LOW | D3 | NEW | The #5261 `rt-integrity` assertion in the vulkan-validation lane has no source pin, unlike every other assertion in that job |
| CONC-D3-2026-10-08-04 | LOW | D3 | NEW | At HEAD the ABBA lane (and the main test job) is red on `debug_cli_component_counts_match_the_registry`: `00f580e09` registered three components and did not update `debug-cli.md` |

---

## Findings

### CONC-D5-2026-10-08-01: `forcegreet_system` and `eat_sleep_system` hold the `PhysicsWorld` guard across storage acquisitions, inverting the order production records
- **Severity**: HIGH. ECS deadlock potential has a floor of HIGH (`_audit-severity.md`). There is **no runtime deadlock today**: both
  systems are exclusive and the participants in the reverse order run elsewhere. The closest precedent, #4325 (`npc_combat_ai_system`,
  same shape), was graded MEDIUM on exactly that reasoning, whereas #2134 (HIGH) and the recent #5305 (HIGH) were graded at the floor.
  I followed the table; the orchestrator may reasonably downgrade to MEDIUM.
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics)
- **Location**:
  - `byroredux/src/systems/forcegreet.rs:66-125` (`forcegreet_system`; guard bound at `:66`, never dropped; nested acquisitions at `:72`, `:76`, `:83`, `:90`, `:98`, `:104`, `:112`, `:119`).
  - `byroredux/src/systems/eat_sleep.rs:128-153` (`eat_sleep_system`; guard bound at `:128`, nested acquisitions at `:133`, `:141`, `:147`).
  - Registered at `byroredux/src/boot/schedule/update.rs:442` (Update, exclusive) and `byroredux/src/boot/schedule/post_update.rs:129` (PostUpdate, exclusive).
- **Status**: NEW. The same class as closed #2134 / #3262 / #3655 / #4325; none of those covers these two sites (both added after the baseline: `14cff35ae`, `00f580e09`).
- **Verification Path**: `cargo test` under `BYRO_LOCK_ORDER_CHECK=1` — reproduced in the scratch copy (below).
- **Description**: `docs/engine/ecs.md` § Lock-ordering policy states the rule: *"`PhysicsWorld` last, with nothing taken under it … it
  is a sink with no outgoing edges"*; `ragdoll_writeback_system`'s comment (`ragdoll.rs:607-614`) calls it "the crate-wide 'no storage
  under a `PhysicsWorld` guard' rule". Every other locomotion system follows it with the same three-pass shape — collect under
  storage guards, drop them, take `PhysicsWorld` for the `step_toward` / `step_along_waypoints` loop alone, then apply writes
  (`travel.rs:262-340`, `patrol.rs:191-195`, `wander.rs:378-382`, `follow.rs:262-267`, `guard.rs:235-239`, `escort.rs:368-375`,
  `combat_ai.rs:249-256`). The two new systems skip that shape:
  - **`forcegreet_system`**: binds `physics_guard` at `:66` before the loop and keeps it to the end of the function. Under it the loop reads `Dead` / `AiCombatState` /
    `ActorControlState` (`npc_refuses_dialogue`), `GlobalTransform` (`get`, `query_mut`), `WalkSpeed`, `Transform` (`get`, `query_mut`), and then calls
    **`forcegreet_open`** — the whole dialogue-open stack (`LoadedCellIndex`, the CTDA evaluator's read set, `apply_selection`'s `DialogueRegistry` /
    `NpcDialogueTopic` / `DialogueSurfaceState` writes, the spoken-INFO fragment effects, `raise_hello_story_event`) — and finally `query_mut::<ForceGreetDirective>()`.
  - **`eat_sleep_system`**: inside the walk branch, `physics_guard` (`:128`) is held across `world.get::<Transform>` (an argument of `step_toward`), `query_mut::<GlobalTransform>` and
    `query_mut::<Transform>` until the `continue` at `:153`. The module doc says the walk is "the same straight-line `step_toward` locomotion the force-greet bridge uses", so it
    inherited the shape.
- **Evidence**: Production already records the reverse edges. `ragdoll_writeback_system` (Late exclusive) holds `Transform`, `Parent`, `Children`,
  `GlobalTransform` (write), `LocalBound`, `WorldBound` and *then* takes `PhysicsWorld` (`ragdoll.rs:593-615`). (`push_kinematic` no longer contributes: it snapshots its storage guards before taking the resource, `sync.rs:1209-1228`, #2404.)
  Measured in the scratch copy (edits: `world.insert_resource(PhysicsWorld::new())` in `eat_actor_far_from_destination_walks`, and a new 14-line `forcegreet_system` test with a `PhysicsWorld`):
  ```
  systems::eat_sleep::tests::eat_actor_far_from_destination_walks … FAILED
    attempted acquisition of `…::transform::Transform` while holding `byroredux_physics::world::PhysicsWorld` on this thread — that closes a cycle …:
    `Transform` → `PhysicsWorld` → `Transform`
  systems::forcegreet::scratch_experiment::scratch_forcegreet_with_physics_world … FAILED
    attempted acquisition of `…::global_transform::GlobalTransform` while holding `byroredux_physics::world::PhysicsWorld` …:
    `GlobalTransform` → `PhysicsWorld` → `GlobalTransform`
  ```
  The first edge in each chain was already in the process-wide graph from tests that ship in the binary (the only production site that holds `Transform` / `GlobalTransform` across the `PhysicsWorld` acquisition is `ragdoll_writeback_system`; I did not attribute the edge to one specific test), exactly as it would be in a real run once the Late ragdoll pass has executed.
  Why the lane does not see it: `forcegreet_system` has no unit test, and the three `eat_sleep_system` tests run **without** a `PhysicsWorld` resource (`try_resource` returns `None`, no edge) — the same blind spot #2134 documents.
  The follow / guard / travel systems each have a "with a real `PhysicsWorld` installed" regression test (`follow.rs:561`, `guard.rs:487`, `travel.rs:565`); these two have none.
- **Trigger Conditions**: A debug build with `BYRO_LOCK_ORDER_CHECK=1`, a loaded cell (any `PhysicsWorld`), and one actor carrying a `ForceGreetDirective` far from the player, or an `EatBehavior` / `SleepBehavior` actor outside `ARRIVE_RADIUS` (64 u) of its destination. Any cell where an actor's Eat/Sleep or Dialogue package wins ambient selection qualifies. The panic fires once `ragdoll_writeback_system` has also run (it runs every Late frame).
- **Impact**: The detector aborts the debug session at the first walking step, and — more importantly — it blinds the tool that gates promoting a system to a parallel lane. Promoting `forcegreet_system`
  or `eat_sleep_system` (or putting any parallel `GlobalTransform` / `Transform` writer in their stage window) turns it into a real `PhysicsWorld(R) → X(W)` against `X(W) → PhysicsWorld(R)` ABBA. No release-build effect today.
- **Related**: #2134, #3262, #3655, #4325 (same class), `docs/engine/ecs.md` § Lock-ordering policy, CONC-D3-2026-10-08-01 (the `forcegreet_open` nest also records `PhysicsWorld → LoadedCellIndex …`).
- **Suggested Fix**: Use the three-pass shape the sibling systems share. Pass 1a gathers `(npc, current, rotation, speed, target)` into an owned `Vec` under storage guards (and, for force-greet, partitions "refuses", "walks", "opens" and
  resolves `forcegreet_open` **after** the physics scope); Pass 1b takes `PhysicsWorld` alone for the `step_toward` calls; Pass 2 applies the `GlobalTransform` / `Transform` writes. Add a "with a real `PhysicsWorld`" test per system under the detector, like `travel.rs:565`.

### CONC-D3-2026-10-08-01: `raise_hello_story_event` reads `LoadedCellIndex` under the `StoryEvent` write guard, inside the shadowed `LoadedCellIndex` guards — the cycle #5066 predicted now closes
- **Severity**: HIGH. ECS lock-order cycle on the common dialogue path (every opened conversation, activation or force-greet), floor HIGH. No runtime deadlock today (single thread, exclusive systems; the recursive `LoadedCellIndex` read only blocks behind a queued writer, and its writers need `&mut World`). It aborts the first conversation of a detector session.
- **Dimension**: ECS Lock Ordering (system level)
- **Location**:
  - The second edge: `byroredux/src/systems/story_events.rs:101-113` (`raise_hello_story_event`; `StoryEvent` write at `:101`, `resolve_current_lctn(world)` evaluated as a struct-field expression at `:110`, which takes `LoadedCellIndex` at `:65` and `CurrentCellContext` at `:66`).
  - The first edge: the shadowed guards at `byroredux/src/systems/npc_dialogue.rs:478-481` (`forcegreet_open`), `:652-655` (`npc_dialogue_selection_system_inner`) and `:731-734` (`select_topic_by_form_id`; no hello call under it).
  - The call sites under the shadow: `npc_dialogue.rs:505` (`forcegreet_open`) and `:702` (selection Pass 2).
- **Status**: NEW — activated by `006905d74` (#5366 Phase 4), which added the call. The root cause is the open issue **#5066** (LOW), which says *"any future site that holds one of those and then reads `LoadedCellIndex` closes a cycle rooted here"*. This is that site. Recommend raising #5066 to HIGH, widening it to the third shadow site `forcegreet_open` (added by `14cff35ae`, not in #5066's list of two), and fixing both ends (below).
- **Verification Path**: `cargo test` under `BYRO_LOCK_ORDER_CHECK=1` — reproduced in the scratch copy.
- **Description**: `let Some(index) = world.try_resource::<LoadedCellIndex>() …; let index = index.0.clone();` shadows the read guard; it lives to the end of the function.
  Under it, Pass 2 calls `raise_hello_story_event`, which takes `query_mut::<StoryEvent>()` and — while holding that write guard — evaluates `resolve_current_lctn(world)` as the value of the `location_1` field. That re-reads `LoadedCellIndex`.
  The detector records `LoadedCellIndex → StoryEvent` at the first acquisition and then sees `StoryEvent → LoadedCellIndex` close the cycle.
  (`record_and_check` excludes the incoming type from `held_others`, so the recursive read adds no self-edge, but the *other* held lock, `StoryEvent`, is checked.)
- **Evidence**:
  ```rust
  // story_events.rs:101-113
  let Some(mut events) = world.query_mut::<StoryEvent>() else { return; };
  events.insert(greeter, StoryEvent {
      mnemonic: *b"AHEL", reference_1: greeter, reference_2: Some(greeted),
      location_1: resolve_current_lctn(world),   // takes LoadedCellIndex / CurrentCellContext
      location_2: None,
  });
  // npc_dialogue.rs:652-655, 699-702
  let Some(index) = world.try_resource::<LoadedCellIndex>() else { return; };
  let index = index.0.clone();                    // guard shadowed, not dropped
  …
  apply_selection(world, selection.npc, …);
  crate::systems::story_events::raise_hello_story_event(world, selection.npc, player);
  ```
  Measured in the scratch copy (edit: one line, `byroredux_scripting::story_manager::register(&mut world);` in the `bound_world` fixture, so the fixture registers the `StoryEvent` storage as the real boot does): **7 dialogue tests fail**, all
  `attempted acquisition of byroredux::cell_loader::index::LoadedCellIndex while holding byroredux_scripting::story_manager::StoryEvent … : LoadedCellIndex → StoryEvent → LoadedCellIndex`
  (`a_combatant_npc_refuses_dialogue`, `a_second_conversation_presents_the_second_npc`, `an_unconscious_npc_refuses_dialogue`, the three `branches::*` tests, `the_spoken_lines_fragments_advance_the_stage`).
  In the unmodified tree they pass because `bound_world` never registers `StoryEvent`, so `raise_hello_story_event` returns at its first line.
- **Trigger Conditions**: `byroredux_scripting::register` has run (always, at boot), a plugin index is loaded, and any NPC conversation opens (activation, or force-greet). Under `BYRO_LOCK_ORDER_CHECK=1` the very first one panics.
- **Impact**: Debug-detector sessions (and any future test that registers `StoryEvent` and opens a conversation, including the CI lane the moment a dialogue test gains the storage) abort. Release builds are unaffected. It also records `LoadedCellIndex → {SoundArchiveProvider, SoundCache, AudioWorld, GlobalTransform, StoryEvent, …}` edges through `play_line_voice` (`f8950e7cc`) for the same reason: a second site that holds one of those and reads `LoadedCellIndex` closes another cycle.
- **Related**: #5066 (root cause), #4982/#5305 (same class, different pairs), CONC-D5-2026-10-08-01 (`forcegreet_open` is also called under the `PhysicsWorld` guard), the baseline's "Existing issues re-checked" entry for #5066.
- **Suggested Fix**: Both ends, each sufficient to break this cycle and cheap:
  1. Replace the three shadows with a scoped clone: `let index = { let Some(r) = world.try_resource::<LoadedCellIndex>() else { return …; }; r.0.clone() };` (this is the #5066 fix).
  2. In `raise_hello_story_event`, call `resolve_current_lctn(world)` **before** `query_mut::<StoryEvent>()`, then insert; no edge leaves the marker storage.
  Add a regression test that registers `StoryEvent` in `bound_world` (the experiment's one-line edit) so the lane sees it.

### CONC-D4-2026-10-08-01: `npc_dialogue_selection`'s Access row omits the `StoryEvent` write and the dialogue-voice resources; the declaration scan cannot follow the cross-file hops
- **Severity**: LOW (informational for an exclusive today; the row is the comparison basis for a future parallel promotion, the same reasoning as #5069 and #5307).
- **Dimension**: Scheduler Access Declarations
- **Location**: `byroredux/src/boot/schedule/late.rs:439-495` (the row); the scan `npc_dialogue_selection_declares_everything_the_spoken_fragment_path_acquires`, `byroredux/src/boot/schedule/mod.rs:1007-1018`; the assert helper `:813-893` and `fn_body` lookup `:785-808`.
- **Status**: NEW. Searched "Access row", "npc_dialogue", "StoryEvent write"; only the closed #5307 (spoken-fragment writes) and open #5069 (a different system) match.
- **Description**: The row declares the quest/fragment writes #5307 added, but not what the two newest hops acquire:
  - `raise_hello_story_event` (`006905d74`, called at `npc_dialogue.rs:702`): `StoryEvent` **write**, plus `CurrentCellContext` / `CurrentExteriorContext` reads (via `resolve_current_lctn`).
  - `play_line_voice` (`f8950e7cc`, called from `apply_selection` at `npc_dialogue.rs:344`): `SoundArchiveProvider` read, `SoundCache` **write**, `AudioWorld` **write**, `LoadedPluginSet` read, `GlobalTransform` read.

  The scan follows callees by `fn_body(src, callee)` within each listed source file only; the listed pairs are `npc_dialogue.rs`, the fragment systems, the fragment effects and `scene.rs`. `dialogue_voice.rs` and `story_events.rs` are not listed, so both hops are invisible and the guard stays green.
- **Impact**: `sys.accesses` understates this system, and a parallel-lane promotion of the row would look safe when it is not (the #5307 failure mode, one wave later).
- **Related**: #5307, #5069 (same class, different row), CONC-D3-2026-10-08-01 (the `StoryEvent` write).
- **Suggested Fix**: Add the declarations, and add `(DIALOGUE_VOICE_SRC, "play_line_voice")` and `(STORY_EVENTS_SRC, "raise_hello_story_event")` to the scan's source list.

### CONC-D4-2026-10-08-02: The new Update-stage ordering contracts are comments only
- **Severity**: LOW (hardening; mis-ordering costs a one-frame delay, not a hazard).
- **Dimension**: Scheduler Proof Soundness
- **Location**: `byroredux/src/boot/schedule/update.rs:408-409` (`story_change_location_dispatch`, `story_manager_dispatch`) against `:412` (`quest_alias_refresh_system`); `:442` (`forcegreet_system`) against `ambient_ai_package_system` (`:434`) and the Late `npc_dialogue_selection_system`.
- **Status**: NEW. `grep` for `story_manager_dispatch` / `story_change_location` / `forcegreet` in `scheduler_access_tests.rs` and `boot/schedule/mod.rs` finds no test.
- **Description**: The skill asks for a pin for every new single-writer / multi-reader hand-off. Three exist: the SM dispatcher writes `StoryEventAliasFill` and flips `SceneActorBindings` dirty so that the alias refresh "scheduled right after this system" fills `FromEvent` aliases the same frame (`story_manager.rs:526-538`); the CLOC producer must precede the dispatcher; the force-greet system must sit between the ambient package system (installs the directive) and the Late selection (serials the surface once). All three are registration order plus a comment. Siblings are pinned (`billboard_runs_after_camera_follow_in_late`, `footstep_runs_after_camera_follow_in_late`, `player_body_facing_runs_in_update_before_propagation`, …), each added after a real one-frame-stale bug.
- **Impact**: A reorder delays alias fill / greeting by a frame; nothing in the suite would notice.
- **Suggested Fix**: One test in the style of `player_body_facing_runs_in_update_before_propagation` asserting the three relative positions in `access_report()`.

### CONC-D3-2026-10-08-03: The #5261 `rt-integrity` assertion in the vulkan-validation lane has no source pin
- **Severity**: LOW (test gap; the assertion itself works — see below).
- **Dimension**: ECS Lock Ordering (the CI-lane half of Dim 3)
- **Location**: `.github/workflows/ci.yml:477-481` (the assertion); the sibling pins in `byroredux/src/scheduler_access_tests.rs:424-445` (`vulkan_validation_job_requires_a_selected_device`, `vulkan_validation_job_fails_on_a_panic`, `vulkan_validation_job_enables_the_lock_order_detector`, `…resolves_lavapipe_and_fails_on_init_failure`).
- **Status**: NEW. `grep -rn 'rt_flag=1\|tlas_build=1'` over `byroredux/` and `crates/` matches only a comment in `scene.rs` and a unit-test string in `ecs/resources/mod.rs`.
- **Description**: `59115f54c` (#5261) restored RT in the lane and added `grep -qE 'rt-integrity:.*rt_flag=1 .*tlas_build=1'`. The assertion works: the run 37833008690 log carries `rt-integrity: frame=7 … rt_flag=1 tlas_build=1 tlas_eligible=4 tlas_emitted=4`, and the `println!` that produces it is on stdout so `RUST_LOG` cannot hide it. But every other assertion in that job is pinned by a `vulkan_validation_job_*` test, and this one, the only guard against the lane silently going blind to RT again (the #4596 / #4987 / #5261 class), is not. A workflow edit that drops it passes `cargo test`.
- **Impact**: The class of regression the last two baseline findings were about can return silently.
- **Related**: #5261, #4987, #4596.
- **Suggested Fix**: Add a `vulkan_validation_job_requires_live_rt` test beside the others asserting the job contains `rt_flag=1 .*tlas_build=1`.

### CONC-D3-2026-10-08-04: At HEAD the ABBA lane and the main test job are red on a documentation-count test
- **Severity**: LOW (trivial to fix and HEAD-only; graded under the baseline's CONC-D3-2026-10-05-01 reasoning — a red lane masks the next real cycle — but that one persisted five days, this one is one commit old).
- **Dimension**: ECS Lock Ordering (the CI-lane half of Dim 3)
- **Location**: `crates/debug-server/src/registration.rs:633-651` (`debug_cli_component_counts_match_the_registry`); `docs/engine/debug-cli.md:186` and `:1256` (both still say "67 components"); the registrations `00f580e09` added to `registration.rs`.
- **Status**: NEW (the guard is #4756's; this is fresh drift). Likely also reported by the tooling audit.
- **Description**: `00f580e09` registered `EatBehavior`, `SleepBehavior` and `EatSleepState` (registry count 67 → 70) without touching `debug-cli.md`. CI run 37848932617: the "ABBA lock-order detector" job (workspace-wide `--no-fail-fast`) and "Test + Check + Clippy" both fail on that one test; no `lock-order cycle` panic appears in the ABBA log. Reproduced locally (`cargo test -p byroredux-debug-server --lib -- roster`).
- **Impact**: Until fixed, a real cycle in the next commit would be hidden behind an already-red lane.
- **Suggested Fix**: Update the two counts in `docs/engine/debug-cli.md` to 70.

---

## Existing issues re-checked

- **#5066 (LOW, open) — escalate.** The shadowed `LoadedCellIndex` guard is unchanged at `npc_dialogue.rs:652` and `:731`, and a **third** copy now exists at `:478` (`forcegreet_open`, `14cff35ae`). The "no cycle closes today" premise in the baseline no longer holds: see CONC-D3-2026-10-08-01. Suggest `/audit-publish` post this as a comment on #5066 and raise its severity to HIGH.
- **#5069 (LOW, open).** `late.rs:416-429` (`equipment_appearance_system`) is unchanged: still no `ActorBodyClass`, `PendingGearImport` or `LoadedCellIndex`.
- **Verified fixed since the baseline** (each read in code and exercised by a named test or the CI log):
  - **#5250** (CONC-D1-2026-10-05-01): `tlas.rs:1139-1157` records `build_scratch_size.max(update_scratch_size)`; `fresh_build_records_peak_unconditionally_of_scratch_regrow` ok.
  - **#5261** (CONC-D2-2026-10-05-01): `scene.rs:835-870` uploads through `upload_scene_mesh`; the lane log shows `rt_flag=1 tlas_build=1`.
  - **#5263** (CONC-D3-2026-10-05-01): `ci.yml` exports `RUST_LOG=error,byroredux_renderer::vulkan::device=info`; zero `[Vulkan]` lines in the lane log; `vulkan_validation_job_requires_a_selected_device` rejects the crate-wide form.
  - **#5265** (CONC-D1-2026-10-05-02): `sync.rs:78-82` and the contract test's `counter_readback` needle (`sync.rs:720-725`).
  - **#5305** (ECS-2026-10-05-D1-01): `audio.rs:150-182` resolves the body gate to an owned `(body, gated)` before any guard; the five baseline panics are gone.
  - **#5270** (the one-time helper's wait-failure arm): read in full (`texture.rs:931-995`). Only `ERROR_DEVICE_LOST` frees `cmd` / destroys the fence; any other code leaks both and poisons the reusable-fence mutex through a *caught* panic raised while the guard is alive (`panic = "unwind"` is pinned in `Cargo.toml:294-311`, so `catch_unwind` works). The queue lock is `lock_recovering` and released before the wait. `wait_failure_arm_disposes_only_on_device_loss` and `one_time_commands_free_cmd_buffer_on_every_error_path` ok.
  - **#5365** (open, filed from #5117): the all-slots fence wait is documented, not narrowed; `sync_and_acquire_frame.rs` only gained a comment.
  - **cbaf782db** (the SM ABBA): phase B no longer takes `SceneActorBindings` under the `QuestStageState` write (`story_manager.rs:476-539`); scripting suite green under the detector.

## Dimension notes (clean areas)

- **D1 — Queue & AS.** `draw.rs` still binds the graphics-queue guard across `queue_submit` (dropped on both arms) and the present guard across `queue_present`. The `MAX_FRAMES_IN_FLIGHT == 2` const-assert (`sync.rs:142`) and the contract test are live. No new immediate `destroy_acceleration_structure`; `teardown.rs` / `resize.rs` untouched.
- **D2 — Compute chains.** No barrier, layout, descriptor or dispatch changed. `34c3adf14` (#5249) adds bounded ray-query loops in the fragment shader (`MAX_TRANSMISSION_SELF_SKIPS`, #5368) and re-pins `restir.rs` source needles; `bc1233207` single-sources the meter constant; `a4ef215d4` splits CPU-only files out of `volumetrics.rs` and widens the source-scan lists. `skin_publish_barrier_consumer_tests`, `taa_resolves_the_post_bloom_scene_tap`, `every_skipped_frame_drops_the_temporal_history_not_just_the_first` ok. Lane scene has `lights_submitted=0` in every run I read (including ones from before the baseline), so light-driven consumers are not exercised — unchanged coverage.
- **D3 — other new code read.**
  - `story_manager_dispatch_system` phase A: `SmTree` / `QuestDefinitionRegistry` / `QuestStageState` (R) / `StoryManagerNodeState` (R) / `StoryManagerRng` (W) across condition evaluation; consistent with phase B. `RunOn::QuestAlias` reads `SceneActorBindings`, but returns first when `ctx.quest` is `None`, as in SM dispatch, so no `QuestStageState → SceneActorBindings` edge.
  - `apply_pending_actor_value_writes` (#5239): `CharacterRuleset → ActorValues → GlobalFormIdResolver`, canonical.
  - `release_finished_tethers` / `retry_cinematic_readoption` (#3817): scoped one-storage passes. `PackageTargetRegistry` is held for the whole route system but nothing records an edge *into* it.
  - `dialogue_voice::play_line_voice`: `AudioWorld::play_oneshot` is a bounded queue push (`audio/src/lib.rs:602-630`), so no kira call under the guard. It does a synchronous BSA read and OGG decode under the `SoundCache` write guard on the main thread — a frame-hitch concern for `/audit-performance`, not a lock hazard.
  - The `extensions/` host: one commit (#5239), a canonical-order read only.
- **D4 — Scheduler proof.** 9 `add_to_with_access` == `PARALLEL_SYSTEMS` (`the_parallel_system_table_covers_every_parallel_registration` ok), 38 exclusives-with-access (unchanged), 50 bare exclusives (46 at baseline: `story_change_location_dispatch`, `story_manager_dispatch`, `forcegreet_system`, `eat_sleep_system`). The non-vacuity floors (≥ 9 parallel systems, ≥ 7 pairs) hold (`scheduler_access_tests.rs:294-306`). `ambient_ai_package_system`'s row gained the four new component writes (`update.rs:353-359`).
- **D5 — Physics.** The #5310 detach (`unload.rs`, a `Parent` read per victim, then one `Children` write scope) and the #3817 retention release hold no nested guards; the `world/` split (#5311) is a move. Every named snapshot-then-acquire guard is present and green.
- **D6 — Lifecycle.** No source change; the three load-bearing teardown orderings are still commented at their sites (`teardown.rs:22-24`, `:85`, `:184-204`).
- **D7 — Workers.** No new thread, rayon section or channel. The streaming split (#5092) is a pure move: `shutdown` still takes the worker, drops `request_tx`, then `join_with_timeout` (`streaming/mod.rs:680-704`), `Drop` re-enters it with 1 s, and the worker still runs under `catch_unwind` (`pre_parse.rs:215`, `:256`). `build_render_data`'s `rayon::join` branches (`render/mod.rs:1116-1180`) are unchanged (zero commits in `render/`).

## Skill drift (fold into the next `/audit-concurrency` sync)

- **Dim 5 should list the PhysicsWorld-sink check as a sweep, not just the named guards.** Add a first-step grep: `grep -rn 'try_resource::<.*PhysicsWorld>()' byroredux/src/systems` and require each hit to sit in a block that acquires no storage. Both new offenders were found this way; none of the existing named guards covers new locomotion systems.
- **Dim 3 should say that a lane green with the dialogue / force-greet / Eat-Sleep tests in it proves little**: `bound_world` registers no `StoryEvent`, `forcegreet_system` has no test, and the Eat/Sleep tests install no `PhysicsWorld`. A "register what boot registers" step in those fixtures is the cheap fix.
- **Dim 3 checklist: add "`rt_flag=1 tlas_build=1` on the lane's `rt-integrity:` line"** — the baseline's drift item; #5261 added the CI assertion but the skill text does not yet mention it (and CONC-D3-2026-10-08-03 asks for its pin).
- **`crates/renderer/src/vulkan/restir.rs` is still in no dimension's `Paths:`** (flagged in the baseline). Dim 2's `First step:` glob covers it; `Paths:` does not.
- **Dim 4 should note** that `assert_declares_everything_it_acquires` follows callees only inside the files it is given; any new cross-file hop from a declared system needs its file added (CONC-D4-2026-10-08-01).

## Out-of-scope observations (not findings)

- `crates/scripting/src/condition.rs:82-92`: `00f580e09` inserted the `GetButtonPressed` variant and its doc comment between `GetDistance`'s doc comment and `GetDistance`, so the `GetDistance` doc now sits on `GetButtonPressed` and `GetDistance` has none. Doc-rot for `/audit-tech-debt`.
- `ambient_ai_package_system` now reads `std::env::var_os("BYRO_M42_DEBUG")` once per due tick and once per evaluated actor, and logs `[m42-reselect]` at info for every reselect; debugging residue for `/audit-tech-debt`.

---

Publish with: `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-10-08.md`. Domain labels:
- **concurrency** (+ `physics` for CONC-D5-2026-10-08-01; + `dialogue` for CONC-D3-2026-10-08-01): CONC-D5-2026-10-08-01, CONC-D3-2026-10-08-01, CONC-D4-2026-10-08-01, CONC-D4-2026-10-08-02.
- **concurrency** + `test-gap`: CONC-D3-2026-10-08-03; `doc-rot` + `tech-debt`: CONC-D3-2026-10-08-04.
- Also post the #5066 escalation as a comment on #5066 (and raise its severity label).
