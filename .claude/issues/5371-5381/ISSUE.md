## #5371 [OPEN] CONC-D5-2026-10-08-01: `forcegreet_system` and `eat_sleep_system` hold the `PhysicsWorld` guard across storage acquisitions, inverting the order production records
labels: bug, high, physics, concurrency

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-08.md` — `CONC-D5-2026-10-08-01` (HEAD `00f580e09`)

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every `try_resource::<PhysicsWorld>()` site in `byroredux/src/systems/` sits in a block that acquires no storage)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix


## #5372 [OPEN] CONC-D3-2026-10-08-01: `raise_hello_story_event` reads `LoadedCellIndex` under the `StoryEvent` write guard, inside the shadowed `LoadedCellIndex` guards — the cycle #5066 predicted now closes
labels: bug, high, dialogue, concurrency

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-08.md` — `CONC-D3-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Root cause is open #5066 (LOW, `npc_dialogue` shadowed `LoadedCellIndex` guard); this issue tracks the now-closing cycle and its second, independent edge in `story_events.rs`. The report also asks that #5066 be raised to HIGH and widened to the third shadow site `forcegreet_open` (`npc_dialogue.rs:478`) — orchestrator action, not done here.

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (all three `LoadedCellIndex` shadow sites in `npc_dialogue.rs` — `forcegreet_open`, `npc_dialogue_selection_system_inner`, `select_topic_by_form_id`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix


## #5373 [OPEN] ECS-2026-10-08-D6-01: `forcegreet_system` and `eat_sleep_system` write the walk step into `GlobalTransform`, and transform propagation overwrites it, so force-greet and Eat/Sleep NPCs never walk
labels: bug, ecs, high, gameplay, ai

**Source**: `docs/audits/AUDIT_ECS_2026-10-08.md` — `ECS-2026-10-08-D6-01` (HEAD `00f580e09`)

- **Severity**: HIGH. This is ECS state correctness: a derived component is written, and the source-of-truth component is not. Eat and Sleep are common FO3/FNV schedule procedures, so many NPCs are affected. Owner overlap: `/audit-gameplay` (behaviour).
- **Dimension**: 6 — Propagation invariants
- **Location**:
  - `byroredux/src/systems/forcegreet.rs:76-109`
  - `byroredux/src/systems/eat_sleep.rs:116-152`
- **Status**: NEW (introduced by `14cff35ae` and `00f580e09`)
- **Description**: Both systems read the NPC's position from `GlobalTransform`, call `locomotion::step_toward`, and then:
  - write `new_pos` into **`GlobalTransform.translation`**;
  - write only the rotation into `Transform`.

  NPC placement roots are propagation roots, so `Transform` is the authoritative world pose. Two mechanisms undo the step:
  - The `Transform` rotation write uses `get_mut`, which marks the entity dirty unconditionally (`packed.rs:129`). `step_toward` returns `Some(rotation)` on every tick that still has distance to cover.
  - On the next propagation, a dirty root's global is rebuilt from its local (`ecs/systems.rs`, incremental seed: `None => GlobalTransform::new(local.translation, …)`). Every structural change does the same for all roots (Phase 1b). `Transform.translation` was never advanced, so the root snaps back to its old position.

  The effect differs by system:
  - **`forcegreet_system`** (Update) runs *before* PostUpdate propagation, so the step is erased in the same frame, before rendering.
  - **`eat_sleep_system`** (PostUpdate) runs after propagation. Its step moves only the root's global for one frame. The root carries no mesh, and the skeleton and body children are not recomposed. The next propagation resets the root.

  Either way the next tick starts again from the original position. The NPC turns toward its goal but never moves.
  - Eat/Sleep's `ARRIVE_RADIUS` check therefore never passes, so the actor never sits at the marker, unless it spawned within 64 units of its destination.
  - Force-greet never opens unless the player is already inside `radius`.

  Every other M42 mover writes `Transform.translation`:
  - travel's pass 2
  - `wander.rs:424-431`
  - `follow.rs:305-306`
  - the seat snap `apply_seat_assignments`
- **Evidence**:
  ```rust
  // eat_sleep.rs:141 (forcegreet.rs:98 is identical)
  if let Some(mut transforms) = world.query_mut::<GlobalTransform>() {
      if let Some(transform) = transforms.get_mut(npc) { transform.translation = new_pos; }
  }
  if let (Some(new_rotation), Some(mut transforms)) = (new_rotation, world.query_mut::<Transform>()) {
      if let Some(transform) = transforms.get_mut(npc) { transform.rotation = new_rotation; }   // marks Transform dirty
  }
  ```
  `eat_actor_far_from_destination_walks` calls the system once and asserts on `GlobalTransform`, without running propagation, so it cannot see this. `forcegreet.rs` has no tests.
- **Impact**:
  - Every FO3/FNV NPC whose winning package is Eat (procedure 3) or Sleep (procedure 4) stands at its spawn point; `ai_package.rs:389-403` and `:510-523` install these from vanilla schedules.
  - Every Dialogue-procedure force-greet that starts outside its radius never opens.
  - `GlobalTransform` and `Transform` disagree for a frame on every tick, so any consumer that reads `GlobalTransform` in that window sees a pose that is then discarded: the eat_sleep root after PostUpdate, or physics and audio between forcegreet and propagation.
- **Related**: CONC-D5-2026-10-08-01 (the same two write blocks, a lock-order aspect); #4995 (the Transform-writer placement rule).
- **Suggested Fix**: Write `new_pos` into `Transform.translation`, together with the rotation, in a single `query_mut::<Transform>()` scope, the way travel and wander do. Drop the `GlobalTransform` write. Add a test that runs the system and then `make_transform_propagation_system` for N ticks and asserts the actor reaches its destination (Eat/Sleep) or opens the conversation (force-greet).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every NPC mover writes `Transform`, never `GlobalTransform`, on a propagation root — travel, wander, follow, seat snap)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix


## #5374 [OPEN] EXT-D5-2026-10-08-01: Oblivion child worldspaces inherit no default water — `default_water_for_worldspace` models inheritance only through FO3+ `PNAM`, so Bravil's canals, Leyawiin's river, the Imperial City lake shore and New Sheoth ren...
labels: bug, high, game:oblivion, water, terrain-exterior

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-08.md` — `EXT-D5-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: HIGH. A wrong canonical value comes out of an EXAL boundary producer, and the skill's EXAL row sets HIGH. The missing water is in flagship content: the Imperial City plus two major cities.
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `byroredux/src/env_translate.rs:209-238` (`default_water_for_worldspace`). Line 217: `inherit_up_chain(.., pnam::INHERIT_WATER, ..)`. Lines 220-229: the Oblivion arm, whose comment says "Oblivion authors no PNAM either, so the chain walk is a no-op there".
  - `crates/plugin/src/esm/cell/wrld.rs:103-112` (`parent_flags` stays 0 when PNAM is absent).
  - Consumer: `byroredux/src/cell_loader/exterior.rs:1972-1976` and `:71-81` (`resolved_exterior_water_height`).
- **Status**: NEW. It is pre-existing: #2735 recorded "Oblivion is unaffected (it authors no PNAM)", and that is the premise this finding disproves. Searches for "BravilWorld", "Oblivion child worldspace", "Oblivion parent worldspace water", "city worldspace water" and "Oblivion PNAM" found no tracker.
- **Tier Violated**: n/a. The translation itself is wrong: no PNAM is read as "inherit nothing", but in Oblivion it means "inherit everything".
- **Game Affected**: Oblivion (including the Shivering Isles worldspaces in Oblivion.esm)
- **Description**:
  - A child worldspace with no own `NAM2` resolves `(None, None)`. Every cell without `XCLW` then gets no water plane.
  - Oblivion child worldspaces never author `NAM2`, `CNAM` or any PNAM.
  - The data has the same "absent ⟺ inherited" shape that #2735 used to establish PNAM semantics in the later games.
- **Evidence** (Oblivion.esm census):
  - **54/54** root WRLDs author `NAM2`; **0/30** child WRLDs (`WNAM` set) do. 27/54 roots author `CNAM`, 0/30 children do.
  - LAND cells with a VHGT minimum below Z=0 (Tamriel's sea level) and no `XCLW`:

    | Worldspace | Cells below Z=0, no XCLW / LAND cells |
    |---|---|
    | BravilWorld | 33/47 |
    | LeyawiinWorld | 22/38 |
    | ICTalosPlazaDistrict | 29 |
    | ICElvenGardensDistrict | 21 |
    | ICTempleDistrict | 15 |
    | ICImperialPrisonDistrict | 14 |
    | ICArboretumDistrict | 13 |
    | ICMarketDistrict, ICImperialPalace, ICTheArcaneUniversity | 12 each |
    | AnvilCastleCourtyardWorld | 10 |
    | ICArenaDistrict | 9 |
    | AnvilWorld | 7 |
    | SETheFringe (New Sheoth) | 11 |
    | SENSBliss | 4 |
    | SENSCrucible | 3 |
    | SEVitharnWorld | 2 |

  - Bruma, Cheydinhal, Skingrad, Chorrol and Kvatch have 0 such cells and are unaffected.
- **Impact**: Visible missing water in Bravil, Leyawiin, Anvil, all Imperial City districts and New Sheoth. The near field shows exposed canal and lake beds. Tamriel itself is correct (its own `NAM2` gives Z=0).
- **Related**: #2735 (closed; premise disproved); #1305; EXT-D1-2026-10-08-01 (same root cause, climate half); #5335 (Oblivion distant water).
- **Suggested Fix**:
  - At the parse/translate boundary, give Oblivion children (`parent_worldspace` set, no PNAM) an inherit-all parent flag word. One table-shaped rule (e.g. "pre-FO3: child ⟹ inherit LAND|WATER|CLIMATE") lets `inherit_up_chain` resolve `NAM2` from Tamriel / SEWorld.
  - Pin it with an Oblivion-shaped fixture (child without NAM2 → parent's form, Z=0) and correct the #2735 comment at `env_translate.rs:222-224`.

## Completeness Checks
- [ ] **SIBLING**: Same inheritance gap checked for every other `inherit_up_chain` consumer (LAND/CLIMATE bits, `resolve_worldspace_climate`, LOD water) on Oblivion children
- [ ] **TESTS**: A regression test pins this specific fix


## #5375 [OPEN] FNV-2026-10-08-D2-01: A later plugin's DIAL override replaces the master's whole `DialRecord`, INFO list included. Each FNV story DLC drops 8.5–9.9k FalloutNV.esm INFOs, and `GREETING` falls from 5,300 INFOs to 11–129
labels: bug, high, legacy-compat, gameplay, dialogue, game:fnv, game:fo3, game:fo4, game:skyrim, esm-plugin

**Source**: `docs/audits/AUDIT_FNV_2026-10-08.md` — `FNV-2026-10-08-D2-01` (HEAD `00f580e09`)

- **Severity**: HIGH. Authored content is silently dropped under a realistic, documented launch shape, and the topic is the one the new #5367 greeting, force-greet and voice layers depend on. There is no warning and no fallback.
- **Dimension**: ESM Data Slice. The mechanism (load-order merge) belongs to `/audit-esm`. Consumers are `/audit-gameplay` and `/audit-scripting`.
- **Location**:
  - `crates/plugin/src/esm/records/grup_walker.rs:243` and `:393`: each plugin inserts a fresh `DialRecord`.
  - `crates/plugin/src/esm/records/grup_walker.rs:284` and `:403`: INFO children are pushed onto that plugin's copy.
  - `crates/plugin/src/esm/records/index.rs:41-53`: `map_category!` merges with `HashMap::extend`.
  - `crates/plugin/src/esm/records/index.rs:663`: `dialogues` uses that macro.
  - `crates/plugin/src/esm/records/index.rs:1086-1192`: `merge_from`.
- **Status**: NEW. No open or closed issue and no 2026-10-05/-08 report mentions DIAL/INFO merging across plugins. ESM-2026-10-08 covers INFO `DATA`/`QSTI` decode only.
- **Description**:
  - In the Gamebryo/Creation format, INFO records are independent records nested under their DIAL. A plugin that adds or changes INFOs on a master topic ships an override copy of the DIAL record plus a Topic Children group that holds **only its own new or changed INFOs**. The engine composes the topic's INFO set across the load order by INFO FormID.
  - ByroRedux parses each plugin into its own `EsmIndex`, so the DLC's `DialRecord` holds only the DLC's INFOs. `merge_from` then folds `dialogues` with `extend`, which is last-write-wins on the whole record. The master's INFOs vanish.
  - Overrides that carry no Topic Children group leave the topic with **zero** INFOs.
  - Nothing else merges `infos`. A repo-wide grep finds only the two per-plugin `push` sites.
- **Evidence**: a raw GRUP walk over the shipped masters (`/tmp/audit/fnv/py/`).
  - FalloutNV.esm alone: `GREETING` 0x000000C8 has 5,300 INFOs, out of 23,247 INFOs in the file.
  - Per story DLC loaded after FalloutNV.esm:

    | DLC | Master DIALs overridden | Master INFOs replaced | DLC INFOs kept | Topics left with 0 INFOs | `GREETING` after merge |
    |---|---|---|---|---|---|
    | DeadMoney | 93 | 9,914 | 424 | 8 | 73 |
    | HonestHearts | 79 | 8,573 | 631 | 8 | 69 |
    | OldWorldBlues | 24 | 8,715 | 453 | 0 | 129 |
    | LonesomeRoad | 56 | 9,325 | 109 | 5 | 11 |
    | GunRunnersArsenal | 10 | 13 | 10 | 0 | — |

    GunRunnersArsenal hits vendor topics, e.g. `188AlexanderSeeInventory` drops from 3 INFOs to 1.
  - Other topics hit include `RadioHello` (129 INFOs), `Attack` (249), `Hit` (156), `HELLO` (1,074), `Death` (93) and `StartCombatResponse` (53).
  - FO3, same shape: Anchorage keeps 130 `GREETING` INFOs, ThePitt 327, BrokenSteel 301, PointLookout 145 and Zeta 148.
  - Skyrim: `Update.esm` overrides 59 Skyrim.esm DIALs, replacing 790 INFOs with 108 and emptying 1 topic, e.g. `DialogueRiftenHellos` and `DBAstridSecondGreetForcegreet`.
  - Code:
    ```rust
    // index.rs:46-48 (map_category!), used for `dialogues` at :663
    |target: &mut EsmIndex, source: &mut EsmIndex| {
        target.$field.extend(std::mem::take(&mut source.$field));
    },
    ```
- **Impact**:
  - Every FNV DLC launch is affected, including the documented `--master FalloutNV.esm --esm HonestHearts.esm` shape that `m48-5-fnv-hud.sh` boots into `GSDocMitchellHouse`, the same cell as `dt1`.
  - In that session, #5367 Phase G's generic greeting for Doc Mitchell can only select among HonestHearts' 69 Zion-NPC greeting INFOs, all gated to DLC speakers.
  - Force-greet `PKDD` topics that a DLC overrides, Say-Once/Random pools, and Phase V voice (which resolves from the chosen INFO) all see only the last plugin's INFOs.
  - With all four story DLCs loaded, only the last-loaded one's INFOs survive on each shared topic.
  - The same loss affects FO3 DLC launches and every Skyrim load order that includes `Update.esm`. The default `--game skyrim_se` profile loads Skyrim.esm alone. The DLC repro in CLAUDE.md lists `Update.esm`.
  - The single-master floor test `dialogue_greeting_and_forcegreet_fnv_floor` cannot see this.
- **Related**: #5367 (Phases G/F/V), #5271 (QSTI per INFO), #3543 (tombstone merge). Cell children already have a dedicated merge (`merge_cell_references`, `crates/plugin/src/esm/cell/mod.rs:1433`). DIAL children have none.
- **Suggested Fix**:
  - Give `dialogues` a dedicated merge entry in `EsmIndex::categories()`. The override's header fields replace the master's. Its `infos` are folded into the master's by INFO FormID: a later INFO with the same id replaces, a new one appends, and a Deleted INFO is removed.
  - Order the result by INFO `PNAM` (previous INFO) rather than plugin append order.
  - Pin it with a two-plugin synthetic test and a real-data floor: FalloutNV.esm + HonestHearts.esm `GREETING` ≥ 5,300 INFOs.

## Completeness Checks
- [ ] **SIBLING**: Same last-write-wins whole-record merge checked for every other `map_category!` record that owns nested child records (QUST stages/objectives, PACK, any parent+children category) in `EsmIndex::categories()`
- [ ] **TESTS**: A regression test pins this specific fix


## #5376 [OPEN] GAME-D5-2026-10-08-01: Ambient Dialogue-procedure wiring + fail-open package conditions = 60 FNV NPCs force-greet the player unconditionally on every spawn and load; a failed open is never consumed
labels: bug, high, gameplay, ai, dialogue, game:fnv, game:fo3

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D5-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: HIGH
- **Dimension**: NPC Spawn → AI Package Selection → Locomotion (Dim 5) / Interaction (Dim 2)
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:209-219` (the `from_package` Dialogue arm), `:410-418` / `:529-536` (installed at spawn and at runtime), `:36-56` (`package_conditions_pass`); `byroredux/src/systems/forcegreet.rs:69-124`; `crates/scripting/src/condition.rs:197-206`
- **Status**: NEW
- **Trigger**: FNV, any cell holding one of the affected NPCs, e.g. Goodsprings (Sunny Smiles, `GSSunnySmiles` 0x104E84), at any hour, with fresh quest state; repeats on every save load and after every combat.
- **Description**: Before `00f580e09` the only installer of `ForceGreetDirective` was the `dialogue.forcegreet` console door (the `forcegreet.rs` module doc still says vanilla lists these packages on 0 NPC_ defaults). Now any winning PKDT-15 package installs the directive, both at spawn and at re-selection.

  The selector is `active_package`, which takes the first package whose schedule matches and whose conditions pass, and package conditions fail open: any `ConditionFunction::Unknown` makes the whole list pass. The PACK census over FalloutNV.esm, simulated with the engine's own known-function set and schedule rule, gives the same result at hours 3, 12 and 20: 60 NPC_ base records whose first eligible package is a Dialogue package.
  - 56 of them pass only because a condition fails open. The unknown functions are fn 79 `GetQuestVariable` (59 occurrences), then fns 50, 53, 310, 136, 289, 612 and 421.
  - 4 carry no conditions at all.
  - Examples: `VFSGrannyGangerMugPackage`, `VRRCRepConsequenceBark`, `ElDoradoTrooperBarkPackage`, `VHDLegionOliverFightLegateWalkOut`, `VRRCMessengerDialogue`, Sunny's `SunnyMeetPlayerDialoguePackage`.

  The commit adds a CTDA fn 0 → `GetButtonPressed` mapping to stop exactly this for Sunny, but it cannot help:
  - Sunny's package condition is `CTDA 00000000 00000000 4f000000 …`. That is function 79 (`GetQuestVariable` on quest 0x104C66), still `Unknown`, so it still fails open.
  - FalloutNV.esm and Fallout3.esm author zero fn-0 CTDAs anywhere.

  Three further problems:
  - **No consumption on failure.** `forcegreet_open` returns `false` when the topic is missing or no INFO passes. `forcegreet_system` consumes the directive only on success or refusal, so the NPC keeps stepping toward the player (or standing within 128 units) and re-runs `select_on_topic` every frame for as long as it carries the directive.
  - **Re-greets on every load.** Saved state cannot suppress a re-greet: the directive is re-derived by `reseat_ambient_packages_after_restore` on every load and on every combat end (`suspend_ambient_behavior_for_combat` clears the winner).
  - **Wrong target and dialogue type.** The procedure's PTDT target and PKDD Dialogue Type are ignored. Of the 141 NPC-default references, 4 target a non-player reference (e.g. `FollowersVeronicaForceGreetIntercom` → 0xE27FC) and 5 are "Say To". The GECK defines Say To as one spoken line with no dialogue menu. Both still force a player conversation.
- **Evidence**:
  ```rust
  } else if package.procedure_type == PROCEDURE_DIALOGUE {
      Some(Self::Dialogue { topic: package.dialogue_topic })   // no PTDT / dialogue-type / condition-confidence check
  ```
  ```rust
  if forcegreet_open(world, npc, directive.topic) { opened.push(npc); }  // false ⇒ directive kept, retried next frame
  ```
- **Impact**: On FNV, the reference title, NPCs whose force-greets vanilla gates on quest state open a conversation with the player at first contact, regardless of quest progress. They do it again after every load and every fight. With ECS-2026-10-08-D6-01 the walk step is currently discarded, so today the conversation opens when the player comes within 128 BU. Once that bug is fixed, the NPC walks to the player. A greet whose INFO pool is empty leaves the NPC glued to the player.
- **Related**: ECS-2026-10-08-D6-01, PHYS-D4-2026-10-08-01 (walk mechanics); #5367; GAME-D5-2026-10-08-05 (the fn-0 mapping); GAME-D2-2026-10-08-01.
- **Suggested Fix**: Install the force-greet only from a Dialogue package whose conditions were *all* evaluable (do not let the fail-open pass arm a player-control takeover), whose PTDT targets the player, and whose PKDD type is Conversation. Consume or cool down the directive when the open fails. Drop or justify the fn-0 mapping.

## Addendum (from `AUDIT_FO3_2026-10-08.md`) to GAME-D5-2026-10-08-01 (FO3 population, measured independently; not a new finding)
- **Status**: Related: GAME-D5-2026-10-08-01 (HIGH, `/audit-gameplay`). The same mechanism reaches FO3 at the same scale, so publish that finding with `game:fo3` as well as `game:fnv`.
- **Evidence** (`/tmp/audit/fo3/py/fo3_pack_census.py` over `Fallout3.esm`; the full output is in `/tmp/audit/fo3/pack_census.log`). The engine's known-function set and schedule rule were mirrored from `crates/scripting/src/condition.rs:206-227`, and NPC_ TPLT inheritance follows ACBS template flag 0x20:
  ```
  PACK 3266, procedure-15 (Dialogue) 386; on NPC_ PKID lists: 341 packs / 365 refs; on CREA lists 27 (no CREA package runtime)
    PTDT target of those 365 refs: player 186, ANOTHER REFERENCE 179;  PKDD type: Conversation 345, Say-To 15
  always-winning Dialogue package (fail-open or unconditioned), NPC_ base records:
    hour 3: 58 (56 fail-open, 2 no-cond) · hour 12: 59 (57, 2) · hour 20: 60 (58, 2)
    unknown fns gating them: 79 GetQuestVariable x39, 53, 50, 45, 84, 74, 289, 161
    hour-12 split: 48 player-targeted Conversations, 11 target another ref, 5 Say-To total
  placements of the hour-12 set: 221 ACHRs over 51 bases; 160 in exterior cells
    Vault101d: CG02Amata (CG02AmataFindPlayer), CG04Amata, CG04Butch, CG04/CG02 Officer Gomez  · Vault101aMS16: 2 Gomez
    Megaton: HardenSimms (MS11ForcegreetPlayer), MegatonSettler01 (gift), Jericho (DEMOMegJerichoDialogueJennyStahl -> targets Jenny Stahl)
    also ThreeDog (ThreeDogDialogPackageFirstTimePlayer), Betty (MQ04BettyKillPlayerDialogue), Dukov, OldLadyDithers, the 12 FFER79/FFHitSquad encounter actors
  fn-0 CTDAs anywhere in PACK/NPC_/CREA/DIAL/INFO: 0
  ```
- **What is FO3-distinctive**:
  1. The force-greets sit in **Vault 101**, FO3's opening (Sunny Smiles is FNV's opening equivalent). Its tutorial actors force-greet on cell load whatever the CG02/CG04 stage is.
  2. **Non-player targets are far more common**: 179 of 365 FO3 Dialogue-package refs (49%) name another reference in `PTDT`, against 4 of 141 on FNV per GAME-D5-01. GAME-D5-01's "wrong target" bullet is therefore a large class on FO3. Examples are Jericho→Jenny Stahl and Officer Gomez→Butch, which both become player conversations.
  3. `forcegreet.rs:7-9` still says vanilla FO3/FNV lists these packages on "0 references across both masters". On FO3 it is 365 NPC_ references, so that premise is false for both masters.
- **Fixture reach**: none of the 51 bases is placed in `MegatonPlayerHouse` or `MegatonMoriartysSaloon`, so the declared `fo3.env` gates (P0 / P2 Nova / P5) should not be hijacked. This was not live-verified.

## Completeness Checks
- [ ] **SIBLING**: other intrusive procedures installed from fail-open package conditions (Escort/Follow/Dialogue) checked
- [ ] **TESTS**: A regression test pins this specific fix


## #5377 [OPEN] PAR-D2-2026-10-08-01: menuxml `decode_uncompressed` underflows a `u32` on any zero or narrow channel mask, which panics in dev/test builds; release decodes L8 as red
labels: bug, import-pipeline, high, safety, ui

**Source**: `docs/audits/AUDIT_PARSERS_2026-10-08.md` — `PAR-D2-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: HIGH
- **Dimension**: Error Semantics
- **Location**:
  - `crates/menuxml/src/tex.rs:300-319`: `shift_of` returns `(0, 1)` for a zero mask, and `scale` computes `(v >> (2 * bits - 8).min(bits))`.
  - `crates/menuxml/src/tex.rs:332-343`: the luminance test `r_mask == b_mask && g_mask == r_mask`.
  - Production callers:
    - `crates/menuxml/src/menu.rs:459` (menu art)
    - `crates/menuxml/src/font.rs:88` (font-atlas DDS fallback)
    - `byroredux/src/hud.rs:574` (compass strip)
    - `byroredux/src/commands/assets.rs:421` (`tex.dump`, which decodes any archive texture)
- **Status**: NEW
- **Trigger Input**: an uncompressed DDS header (`DDPF_FOURCC` clear) with `RGBBitCount` 8, 24 or 32 where any of R, G or B is 0, or any non-zero mask is under 4 bits wide. Examples:
  - The standard `DDSPF_L8` header: flags `0x20000`, masks `0xFF, 0, 0, 0`.
  - `DDSPF_A8`: masks `0, 0, 0, 0xFF`.
  - A2R10G10B10: the 2-bit alpha mask `0xC0000000`.
- **Description**:
  1. `bits` is a `u32`. For `bits < 4`, `2 * bits - 8` underflows. The dev profile keeps overflow checks on (`[profile.dev]` sets `opt-level = 1` only), so this panics with "attempt to subtract with overflow".
  2. `scale` runs unconditionally for R, G and B. A zero colour mask, reported as `bits = 1`, therefore always reaches it. Only alpha is guarded, by `a_mask == 0`.
  3. In release the subtraction wraps and `.min(bits)` hides it. The result is still wrong:
     - The luminance branch requires all three colour masks to be equal and non-zero. The standard L8 header has G = B = 0, so it never matches, and an L8 texel decodes as `(L, 0, 0)`.
     - A 2-bit alpha of 3 expands to 192 instead of 255.
  4. Neither the HUD driver nor the debug-command path runs under `catch_unwind`, so in a dev build (`cargo run`, the documented default) the panic takes down the engine's main thread.
  5. The strict real-data lane runs `--release`, so overflow checks are off there. No unit fixture covers an 8-bit or zero-mask header.
- **Evidence**:
  - **Probe** (`menuprobe --bin dds`, real `Rgba8::decode_dds`):
    ```
    (dev)     L8 grey 0x80: PANIC "attempt to subtract with overflow"
    (dev)     A8 0x80: PANIC
    (dev)     A2R10G10B10 white opaque: PANIC
    (release) L8 grey 0x80: Some([128, 0, 0, 255])      # expected [128,128,128,255]
    (release) A8 0x80: Some([0, 0, 0, 128])
    (release) A2R10G10B10 white opaque: Some([255, 255, 255, 192])   # expected alpha 255
    ```
  - **Vanilla census** (`menuprobe --bin ddscensus`, every uncompressed `.dds` header in the textures and misc BSAs):
    - Oblivion has **470** L8 files (flags `0x20000`, masks `000000ff,0,0,0`), for example `textures\clutter\voidessence_g.dds`.
    - No vanilla menu path in Oblivion, FO3 or FNV carries an 8-bit DDS. The vanilla HUD is therefore unaffected, and the vanilla trigger is `tex.dump` on any of those 470 files.
- **Impact**:
  - **Dev/test builds:** an engine abort from one texture. It is reachable from:
    - `tex.dump` on 470 vanilla Oblivion textures;
    - any mod menu art, font atlas or compass strip authored as L8, A8 or a narrow-mask format.
  - **Release builds:** silently wrong colours for the same files: red-tinted luminance and under-opaque 2-bit alpha.
  - The skill's severity rule applies: a panic in an untrusted-input reader is HIGH.
- **Related**:
  - REN-D5-2026-10-08-02: the renderer's `parse_dds` rejects L8; this decoder accepts and then mis-decodes it.
  - REN-D5-2026-10-08-01: the mask gap is not shared.
  - #1542, PAR-D3-2026-10-08-01.
- **Suggested Fix**:
  - Treat a zero mask as "channel absent" and skip `scale`, the way alpha is already handled.
  - Compute the bit replication in signed or saturating arithmetic. Bits replicate as `v8 = v << (8 - b)`, then OR in `v8 >> b` repeatedly until 8 bits are filled.
  - Key luminance off `DDPF_LUMINANCE` (`0x20000`), or off `g_mask == 0 && b_mask == 0 && r_mask != 0` when `bit_count == 8`, and replicate L into R, G and B.
  - Check the payload length against `width * height * bpp` before `Rgba8::new`. Today a 128-byte header claiming 8192² reserves 256 MiB before the first bounds check.
  - Add fixtures for L8, A8 and A2R10G10B10 that run in the debug unit lane.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other DDS decoders (`parse_dds` in the renderer, REN-D5-2026-10-08-01/02))
- [ ] **TESTS**: A regression test pins this specific fix


## #5378 [OPEN] REN-D5-2026-10-08-01: `parse_dds` ignores the channel masks of 32-bpp `DDPF_RGB` headers, so A8R8G8B8 / X8R8G8B8 files upload as R8G8B8A8 with red and blue swapped — including every Skyrim SE distant-terrain diffuse atlas
labels: bug, renderer, high, game:skyrim

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-08.md` — `REN-D5-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: the `bpp == 32` arm of `parse_dds` (`crates/renderer/src/vulkan/dds.rs` ~:500) still returns `R8G8B8A8_SRGB, expand: None` without reading the masks. Also affects Oblivion / FO3 / FNV assets (see table); labelled by the headline Skyrim SE impact.

- **Severity**: HIGH
- **Dimension**: Memory/Lifecycle (texture decode)
- **Location**:
  - `crates/renderer/src/vulkan/dds.rs`, `parse_dds`, the `bpp == 32` arm of the `DDPF_RGB` branch ("32-bpp R8G8B8A8 uploads directly — zero-copy").
  - Consumers: `TextureRegistry::load_dds_with_clamp_and_color_space` / `enqueue_dds_with_clamp_and_color_space` (`crates/renderer/src/texture_registry/upload.rs`).
  - The affected Skyrim consumer: `spawn_btr_block` (`byroredux/src/cell_loader/terrain_lod_btr.rs`), which resolves `btr_diffuse_path`.
- **Status**: NEW. No open or closed issue matches the keyword searches listed under Process notes (#1542, #1074 and #4830 are adjacent but cover other arms). `AUDIT_FO3_2026-07-02.md` Dim 6 records the parser as handling "32-bpp uncompressed RGBA/BGRA", which was never checked against the masks.
- **Description**:
  - The 16-/24-bpp arm reads `dwRBitMask`…`dwABitMask` (file offsets 92..108) and decodes through `RgbExpand`.
  - The 32-bpp arm never reads them. It sets `format: R8G8B8A8_SRGB, expand: None` and borrows the file bytes.
  - A DDS with `R=0x00FF0000, G=0x0000FF00, B=0x000000FF` stores bytes in B,G,R,(A|X) order. Uploaded as R8G8B8A8, the sampler's `.r` is the file's blue.
  - For X8R8G8B8 (`A=0`) the fourth byte is the unused X byte, so alpha is sampled from it. It averages 0 across the 2,916 Tamriel atlases measured. `format_has_alpha(R8G8B8A8_SRGB)` is true, so `INSTANCE_FLAG_DIFFUSE_ALPHA` is set and the shader does not force `a = 1`. The LOD terrain draw is opaque with no alpha test, so the zero alpha is harmless today; the channel swap is the visible part.
  - The sibling DX10 path handles the same layouts correctly: DXGI 87/88/91 map to `B8G8R8A8_*`.
- **Evidence**:
  - Code: the `bpp == 32` arm in `parse_dds` (above).
  - Test gap: the only 32-bpp fixture, `make_uncompressed_header`, never sets the masks.
  - No compensation downstream: no `.bgr`/`.zyx`/`ComponentMapping` swizzle exists in the renderer shaders or image-view creation (grep).
  - Measured on the vanilla archives (header scan of every `.dds`; masks other than the RGBA order `R=0xFF, G=0xFF00, B=0xFF0000`):

    | Archive set | Layout | Count | What it is |
    |---|---|---|---|
    | Skyrim SE `Textures5/6/7.bsa` | X8R8G8B8 (`A=0`) | 9,326 | every terrain-LOD diffuse atlas under `textures/terrain/<world>/` (Tamriel 3,040; Solstheim 3,060; Soul Cairn 910; Apocrypha 990; Sovngarde 495; Skuldafn 340; the rest smaller) |
    | Skyrim SE | A8R8G8B8 | 6 + 11 + 2 + 3 + 1 (+ 641 flow maps, see below) | 6 tree-LOD atlases (`…/trees/<world>treelod.dds`), 11 DLC1 cubemaps, `sky/sun.dds` and `sky/sunglare.dds`, 3 lens-flare maps, `effects/highfrequencynormals.dds`; the 641 are `textures/water/skyrim.esm/flow.*.dds` |
    | Oblivion / Shivering Isles | A8R8G8B8 | 72 / 3 | lock-picking meshes, gate effects, 1×1 placeholders |
    | FO3 / FNV | A8R8G8B8 | 13 / 13 | including the WATR noise/foam maps `testwaternoisegrant.dds`, `wastelandmuckpoolnoise01.dds`, `toxicdumpwater01.dds`, `waterfoam01.dds` |
    | Skyrim LE | A8R8G8B8 | 1 | `effects/highfrequencynormals.dds` |

  - The masks, not the bytes, are what is right. On 2,916 Tamriel diffuse atlases the per-file mean of byte 2 minus byte 0 has p50 = +14.1 and p90 = +14.2 (of 255), and 2,554 files have byte 2 > byte 0. Decoded per the masks (R = byte 2), that is the warm, earthy tint Skyrim terrain has. Decoded the way the engine does (R = byte 0), it is a uniform 5 % shift toward blue.
  - Cross-check: the 21 level-32 `tamriel.32.*` atlases are authored with RGBA-order masks (so they decode correctly today), and their R is also above their B (73 vs 64 on the sample).
  - The 641 water flow maps are **not** consumed by the engine today: only the NIF-water `flow_map_index` is loaded, from the mesh, not these per-cell files (grep of `byroredux/src`). They are listed for completeness, not as impact.
- **Impact**:
  - Every Skyrim SE exterior that draws distant terrain from vanilla `.btr` LOD renders it with red and blue swapped, plus an alpha channel read from an unused byte. The magnitude per tile is modest (a few percent), but it is systematic, silent and affects a headline feature.
  - The tree-LOD atlases (6) and DLC1 environment cubemaps (11) are swapped the same way.
  - The WATR noise maps (FO3/FNV) and the Oblivion lock-picking/effect meshes are swapped too, with lower visibility.
  - Rated HIGH under the decision tree's "rendering correctness → at least HIGH" line. The per-tile hue shift is small, so a reviewer who reads this as "visual artifacts only" can down-rate it to MEDIUM.
- **Related**: #1542 (16-/24-bpp arm), #1074 (DXGI BGRA mappings), #4830 (mask hardening), REN-D5-2026-10-08-02.
- **Suggested Fix**:
  - Read the four masks in the 32-bpp arm.
  - Keep the zero-copy path only for the exact RGBA order with `A = 0xFF000000`.
  - Send every other combination (A8R8G8B8, X8R8G8B8, X8B8G8R8) through the existing `RgbExpand` machinery, which already accepts arbitrary masks and forces `A = 255` when the alpha mask is zero. `validate_expand_masks` already admits `bpp == 32`.
  - Alternatively map the exact BGRA masks to `B8G8R8A8_*` and force opaque alpha when `a_mask == 0`.
  - Pin it with fixtures that carry real masks (an X8R8G8B8 case asserting the channels land in the right slots and `A == 255`).

## Completeness Checks
- [ ] **SIBLING**: `average_rgb`'s uncompressed arm (`swap_rb = false` for `R8G8B8A8_*`) is fixed by the same change; DX10 BGRA mappings stay consistent
- [ ] **TESTS**: A regression test pins this specific fix


## #5379 [OPEN] SAVE-D5-2026-10-08-01: #3817's cinematic re-adoption stamps `CellRoot` on the process-lifetime player, so the next save-load teardown despawns the player; pending un-rooted convoy entities survive the load as `FormIdPair` ghost twins
labels: bug, high, gameplay, game:skyrim, save-load

**Source**: `docs/audits/AUDIT_SAVE_2026-10-08.md` — `SAVE-D5-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `unplaced` filters only on missing `CellRoot`, `retry_cinematic_readoption` inserts `CellRoot` with no player check, and `purge_cinematic_retention_state` never touches `CinematicReAdoption`. Reopens hazard (2) of closed #5056 through the new #3817 path; the player-despawn half is new.

- **Severity**: HIGH
- **Dimension**: Live Load-Apply & Frame Boundary (teardown completeness)
- **Data-Loss Class**: corruption-on-load / reference-break
- **Location**:
  - `byroredux/src/systems/cinematic.rs:489-498` (riders = every actor whose `ActorCinematicState.vehicle` is the cart);
  - `byroredux/src/systems/cinematic.rs:524-561` (`unplaced` = every released entity without `CellRoot`, queued on `CinematicReAdoption`);
  - `byroredux/src/systems/cinematic.rs:574-640` (`retry_cinematic_readoption`: no player exclusion; `world.insert(*entity, CellRoot(*root))` at `:619` plus a `CellRootIndex` push);
  - `byroredux/src/app_step.rs:83-90` (the retry runs every exterior streaming frame);
  - `byroredux/src/cell_loader/unload.rs:77-86` (`purge_cinematic_retention_state` removes only the two state components and never touches `CinematicReAdoption`);
  - `byroredux/src/save_io.rs:1581-1589` (exterior reload: purge, then `drain_streaming_state`);
  - `byroredux/src/save_io/registry_completeness_tests.rs:476` (allowlist reason).
- **Status**: NEW. This is a regression introduced by the #3817 fix (`63bf3347f`). It is distinct from ECS-2026-10-08-D7-01, which covers subtree nodes adopted by *local* Transform. It reopens hazard (2) of #5056 for released entities.
- **Trigger**: Skyrim MQ101 opening convoy. The player is a `SetVehicle` rider; the `unload.rs:1046` test and #5056 both model `vehicle = Some(cart)` on the player. The tether releases at the authored route terminal while riders are still attached; per the release doc, riders keep `cart_seat` for the later scripted exit. After that, the player does any in-process load (F9, the pause menu, console `load`) or a cell transition.
- **Description**:
  1. **The player is adopted into a cell.**
     - `release_finished_tethers` collects riders without excluding the player.
     - `release_set` walks their `Children`, which include the player body root.
     - `unplaced` keeps every member without a `CellRoot`. The player never has one: it is process-lifetime, and the body root is "never `CellRoot`-owned" (`player_body.rs:74`). So the player and its subtree are queued every time.
     - On the next streaming frame, `retry_cinematic_readoption` finds the player's world `Transform` in a loaded grid cell (by definition the player's own cell). It inserts `CellRoot(cell_root)` and registers the player in `CellRootIndex`.
  2. **The save load then destroys the player.**
     - `reload_exterior_session` runs `drain_streaming_state`, which unloads every loaded cell through `CellRootIndex` victims. The player is now one of them and is despawned, along with its physics body and GPU handles.
     - Nothing respawns it, because the live path relies on the player outliving the reload.
     - `PlayerEntity` dangles, and `build_form_id_remap` finds no live `PLAYER_FORM_ID_PAIR`. The player's saved `Inventory`, `ActorValues`, `CharacterController` and the rest go unresolved, and #5054's `park_unresolved_snapshot_rows` parks them as a `ReferenceState` that nothing will ever respawn.
     - `apply_player_pose` has no body.
     - The same despawn happens without any load, as soon as the player walks out of the arrival cell's ring or takes a door.
  3. **Pending entities survive the load.**
     - Released entities that are still pending have no `CellRoot`, and no state component, since release cleared it. Examples: the horse that drove past the loaded ring (the #3817 test asserts it "stays pending"), and subtree nodes per ECS-D7-01.
     - The purge does not touch them and the teardown cannot enumerate them, so they survive the load.
     - The reload spawns fresh copies of the same REFR/ACHR, and ghost and fresh copy share a `FormIdPair`. `build_form_id_remap`'s `HashMap` collect then keeps one at random, which is #5056's hazard (2).
     - The un-purged pending list then adopts the ghost into the loaded session.
     - The allowlist reason ("a save/session replacement tears the whole world down anyway (purge_cinematic_retention_state)") is false on both counts.
- **Evidence**: `let unplaced: Vec<EntityId> = { let roots = world.query::<CellRoot>(); release_set.iter().filter(|e| roots…get(**e).is_none())… }` → `pending.pending.extend(unplaced)`. The retry runs `world.insert(*entity, CellRoot(*root)); idx.map.entry(*root).or_default().push(*entity)` with no `PlayerEntity` check. `rg 'Player' cell_loader/unload.rs` finds nothing, so the unload has no player guard. `tether_releases_at_the_authored_route_terminal_and_detaches_riders` uses a non-player rider, so it cannot see this.
- **Impact**: after the Helgen convoy arrives, every later in-process load produces a session with no player. Workaround: restart the process and fresh-load. Separately, every later streaming unload of the arrival cell deletes the player, which is the gameplay-breaking half and is owned by `/audit-gameplay` or `/audit-scripting`. Saves already on disk are intact.
- **Related**: #3817 and #5056 (both closed); ECS-2026-10-08-D7-01; #3254.
- **Suggested Fix**:
  - Exclude the player and its body subtree from `unplaced`; the process-lifetime player must never be cell-owned. Add a player-rider case to the #3817 test that asserts the player has no `CellRoot` after the retry.
  - In `purge_cinematic_retention_state`, also despawn (or hand to the teardown) every entity on `CinematicReAdoption.pending`, then clear the list.
  - Fix the allowlist reason.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other process-lifetime entities (player body subtree, camera) reachable from a cinematic release set)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix


## #5380 [OPEN] SCR-D5-2026-10-08-01: Story Manager node conditions on uncatalogued CTDA functions evaluate as 0.0 and pass vacuously, so the dispatcher starts quests on unmodeled terms. Both of `sm1`'s gated boot starts are such passes
labels: bug, high, scripting, quests, game:skyrim

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-10-08.md` — `SCR-D5-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `walk_siblings` still gates on `evaluate(&node.conditions, …)` and `ConditionFunction::Unknown` still returns `0.0` (`condition.rs` ~995). Census numbers are the report's (Skyrim.esm), not re-run.

- **Severity**: HIGH (skill escalation: a quest-start effect fires on an **unmodeled** term instead of declining)
- **Dimension**: Scene/Package/Dialogue (Story Manager); evaluator side Dim 3
- **Untrusted-Input**: No (vanilla data triggers it)
- **Location**:
  - `crates/scripting/src/story_manager.rs:659-668`: `walk_siblings` evaluates `node.conditions` through the shared
    evaluator and starts on `true`.
  - `crates/scripting/src/condition.rs:191-229`: `from_index`. Anything outside the 22-entry catalog is `Unknown`.
  - `crates/scripting/src/condition.rs:995-1001`: `Unknown` → `0.0`.
- **Status**: NEW. It is the same policy class as GAME-D5-2026-10-08-01 (fail-open FNV Dialogue packages → force-greet),
  but that finding covers a different consumer. Here the consumer starts quests.
- **Description**: the M47.1 "unknown function → 0.0" default was designed for display and selection gating. The SM
  dispatcher reuses it to decide quest **starts**. Skyrim's SM vocabulary is mostly outside the catalog:
  `GetInCurrentLoc` 359, fn 576, 606, 565, `GetGlobalValue` 74, `GetQuestRunning` 56, 629, `GetRandomPercent` 77 and
  others. Any of these compared against a value that 0.0 satisfies (`== 0`, `< 1`, `!= 1`, `<= x`) passes no matter what
  the world state is.
- **Evidence** (census of Skyrim.esm SM node CTDAs):
  - Overall, 939 CTDAs: 759 uncatalogued (81%), 98 passing at 0.0.
  - Under the three events that have live producers (KILL/CLOC/AHEL): 328 / 256 / 62, and **40 quest nodes carry at least
    one vacuously-passing term**.
  - Seven quest nodes pass on nothing but such terms. Two of them are exactly the two starts `sm1-story-manager.sh`
    asserts:
    - `WIGreetingNodeSHARES` (`0x000C791B`): sole condition fn **145** `== 0`. The smoke's own comment says it is
      "passing trivially at boot".
    - `CWChangeLocationScenes` (`0x000D5176`): sole condition fn **56** = `GetQuestRunning(CWFinale 0x000D1444) == 0`.
  - `docs/engine/story-manager.md` §7 describes these as "genuinely condition-gated nodes".
- **Impact**:
  - Every CLOC/KILL/AHEL event can start quests whose authored gate was never evaluated. Some conditions also fail
    closed in the same way: the other 661 unknown terms block their nodes regardless of state, which silently loses
    coverage.
  - In the CLOC chain, `CWChangeLocationScenes` starts a Solitude/Windhelm map-table scene quest on the first location
    change anywhere (the boot cell is Whiterun Dragonsreach), and SCR-D5-02 then makes it consume the event.
  - The quest-state corruption is persistent, because SM-started quests are saved.
- **Related**: GAME-D5-2026-10-08-01, GAME-D5-2026-10-08-05 (the fn-0 mapping), ESM-2026-10-08-D4-02 (field decode),
  SCR-D5-2026-10-08-02.
- **Suggested Fix**: give the evaluator a tri-state result (`Known(bool)` / `Unmodeled`), and have SM dispatch treat
  any `Unmodeled` term as a decline. A node whose gate cannot be evaluated does not start or consume. Catalog the cheap,
  state-backed functions first: 56 (`QuestStageState`), 74 (`Globals`) and 77 (an RNG). Re-derive the `sm1` expected set
  afterwards, and pin a census test asserting that no live-event quest node starts through an `Unknown` term.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other consumers of the `Unknown → 0.0` default that gate effects rather than display — e.g. FNV Dialogue package conditions (GAME-D5-2026-10-08-01))
- [ ] **TESTS**: A regression test pins this specific fix


## #5381 [OPEN] SF-2026-10-08-D6-01: CDB slot 6 (`_height`) lands in the canonical `height` role, which the renderer consumes as a parallax-occlusion map, so Starfield materials get an unauthored POM ray-march at the engine default scale
labels: bug, import-pipeline, high, legacy-compat, game:starfield, nifal

**Source**: `docs/audits/AUDIT_STARFIELD_2026-10-08.md` — `SF-2026-10-08-D6-01` (HEAD `00f580e09`)

- **Severity**: HIGH (the `_audit-severity.md` NIFAL row: a wrong Material out of the NIFAL boundary)
- **Dimension**: Material Flow (NIFAL boundary) / CDB Material Database
- **Location**: `byroredux/src/asset_provider/material/merge.rs:265-271` (`SLOT_HEIGHT => fill(&mut material.textures.height, …)` in `apply_cdb_material`); consumer `byroredux/src/render/static_meshes.rs:657` (`parallax_map_index = texture_indices.height`); `crates/renderer/shaders/triangle.frag:292-305`; defaults `crates/core/src/ecs/components/material.rs:17-21`
- **Status**: NEW. It arrived with `224a19372` / `18fce7e43` (#3398 Phase 2). A search of open and closed issues and of the 10-03 to 10-08 reports found no mention.
- **Description**: `apply_cdb_material` fills the canonical `height` role from `MRTextureFile` slot 6. The render path binds `height` to `parallaxMapIndex`, and `triangle.frag` runs parallax-occlusion displacement whenever that index is non-zero. The CDB arm authors no parallax scale or step count, so POM runs at `DEFAULT_PARALLAX_HEIGHT_SCALE = 0.04` and `DEFAULT_PARALLAX_MAX_PASSES = 4`.
  - Starfield does not use slot 6 as a POM input. In the vanilla CDB's 97-class schema, the only parallax-occlusion fields are on `BSMaterial::ProjectedDecalSettings`: `UseParallaxOcclusionMapping`, `SurfaceHeightMap: BSMaterial::TextureFile` (a nested field, not an `MRTextureFile` slot), `ParallaxOcclusionScale`, `ParallaxOcclusionShadows` and `MaxParralaxOcclusionSteps`. That is, decals only.
  - Height is consumed elsewhere by `BSMaterial::AlphaBlenderSettings` (`HeightBlendThreshold`, `HeightBlendFactor`; layer height-blending) and `BSMaterial::TerrainSettingsComponent` (`MaxDisplacement`, `DisplacementMidpoint`).
  - Ordinary layered materials have no POM toggle or scale.
- **Evidence**:
  - A read-only CLAS dump of `Starfield - Materials.ba2:materials\materialsbeta.cdb` (field lists quoted above).
  - The base CDB carries 1,521 `*_height.dds` FileName strings (394 unique), against 24,827 unique `*_color.dds`.
  - Sampled height maps are layer-blend landscape and architecture sets: `SnowScalloped01_height`, `DirtForerstRoots01_height`, `CaveRoughMacro01_height`, `NAStone01Mossy01_height`, `NATechPatternConcrete02_height`.
  - The synthetic CDB fixture carries slots 0/1/3 only (`index.rs` `synthetic_cdb_chunks`), so no test exercises the slot-6 route.
- **Impact**:
  - POM displaces `sampleUV` before every later fetch (base, normal, detail, emissive…). Each affected Starfield surface swims and distorts with view angle, from a height field authored for blending layers, not for displacing UVs.
  - This hits the landscape, cave and New Atlantis architecture materials that carry height maps.
  - It is a fabricated shading effect at the single translation boundary, the same near-miss class nifal.md forbids for the five parked kinds ("the near-miss roles are wrong, not merely imperfect").
- **Related**: #3398 (Phase 2), #4429 (the parked-kinds contract), #5283 (sibling: the CDB emissive role is zero-weighted), `docs/engine/nifal.md` § Starfield single-channel kinds.
- **Suggested Fix**:
  - Stop forwarding `SLOT_HEIGHT` into `height`. Park it with the single-channel kinds until a Starfield layer height-blend consumer exists.
  - If decal POM is wanted, translate it from `ProjectedDecalSettings` (toggle + scale + steps + `SurfaceHeightMap`).
  - Add a slot-6 fixture that pins the decision, and fix `apply_cdb_material`'s "6=height" doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CDB slots routed by `apply_cdb_material`; the parked single-channel kinds)
- [ ] **CANONICAL-BOUNDARY**: The fix stays at the NIFAL parser→`Material` boundary (`apply_cdb_material` / `translate_material`) — never pushed into `triangle.frag` or re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix (synthetic CDB fixture with a slot-6 texture)


