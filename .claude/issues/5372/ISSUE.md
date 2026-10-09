# #5372: CONC-D3-2026-10-08-01: `raise_hello_story_event` reads `LoadedCellIndex` under the `StoryEvent` write guard, inside the shadowed `LoadedCellIndex` guards — the cycle #5066 predicted now closes

**Labels**: high,concurrency,dialogue,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5372

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
