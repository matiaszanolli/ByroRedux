# #5409: UI-D6-2026-10-08-01: Escape (or the Inventory key) on the native dialogue page closes it without `end_open_conversation`, so OnEnd is skipped and the conversation outlives its page

**Labels**: medium,ui,gameplay,dialogue,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5409

**Source**: `docs/audits/AUDIT_UI_2026-10-08.md` — `UI-D6-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: MEDIUM. Incorrect behaviour on the most natural way to leave dialogue. The workaround is the page's
  Close button.
- **Dimension**: Engine Wiring & Input
- **Profile**: n/a (the native egui game-menu modal; all games)
- **Location**:
  - `byroredux/src/app_events.rs:400-417`: the `game_menu_open` keyboard branch
  - `byroredux/src/main.rs:1330-1340` (`toggle_game_menu`)
  - `byroredux/src/main.rs:1342-1347` (`open_inventory_menu`)
  - `byroredux/src/main.rs:1370-1385` (`resume_from_game_menu`)
- **Status**: NEW. It is not in today's AUDIT_GAMEPLAY report, and #5152 (closed) scoped OnEnd dispatch "on
  selection-change/conversation-close".
- **Description**:
  - `end_open_conversation` has exactly one app-side caller, `resume_from_game_menu` (`main.rs:1379`). That function
    is reached only through the egui `resume_game` output (`app_frame.rs:283`), which is the page's Close/Resume
    button.
  - The keyboard router handles Escape first. `if game_menu_open { if pressed_key == Some(KeyCode::Escape) {
    self.toggle_game_menu(); return; } … }` (`app_events.rs:402-405`). `toggle_game_menu` flips `visible` off and
    recaptures input, but runs no dialogue teardown.
  - The Inventory key in the same branch calls `open_inventory_menu`. That replaces the dialogue page outright and
    pauses the simulation, because `simulation_paused` is "visible and not Dialogue".
  - The comment at `main.rs:1371` says "the dialogue page closing (its Close button **or Escape**) ends the open line",
    which the routing contradicts.
- **Evidence**:
  - `grep -rn "end_open_conversation(" byroredux/src` finds three call sites:
    - `main.rs:1379`
    - `systems/npc_dialogue.rs:553`, the Goodbye timer
    - a test
  - The dialogue page draws no Escape handler of its own (`crates/debug-ui/src/panels.rs:487-492`: "Closing is the
    pause menu's own resume path").
- **Impact**:
  - **OnEnd skipped.** After Escape, the outgoing INFO's OnEnd fragment never dispatches. Skyrim has 3,773 OnEnd-only
    INFO fragments (#5152's census). A quest stage that advances on OnEnd does not advance.
  - **Stale conversation state.** `NpcDialogueTopic` stays on the NPC and `DialogueSurfaceState` keeps its selection.
  - **Late Goodbye close.** For a Goodbye line, the `close_after` timer still fires later. It then runs OnEnd late and
    raises `close_requested`, which calls `close_dialogue_menu` on a page that is already closed. That call is benign,
    because `capture_world_input` keeps its guards.
  - **Inventory key.** The same loss happens, and the page is not restored when the inventory closes.
- **Related**: #5152 (closed), #5367 Phase L (14cff35ae), GAME-D2-2026-10-08-01 (a force-greet hijacking a live
  conversation is the sibling path)
- **Suggested Fix**:
  - Route Escape, and the Inventory key, through `resume_from_game_menu` whenever `dialogue_menu_visible()`. A simpler
    option is to make `toggle_game_menu` and `open_inventory_menu` call `end_open_conversation` when leaving the
    Dialogue page.
  - Pin the behaviour with a test that drives the keyboard path.

## Completeness Checks
- [ ] **SIBLING**: Every native page close path (Escape, Inventory key, any future page switch) goes through one teardown entry point
- [ ] **TESTS**: A regression test pins this specific fix
