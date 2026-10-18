# UI-D6-2026-10-17-01: ExplicitStartPin::release fires before the loading-cover, bench-harness and modal-UI input quarantines

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/5968 (snapshot as filed 2026-10-17; GitHub is authoritative for current state)

**Source**: `docs/audits/AUDIT_UI_2026-10-17.md` (UI-D6-2026-10-17-01) · **Severity**: LOW · **Dimension**: Engine Wiring & Input Routing · **Profile**: n/a (matters for Skyrim-style Start Game Enabled quests) · **Status**: NEW (`c1c5e8d8c`, 2026-10-13; no open or closed issue mentions `ExplicitStartPin`)

## Description
`ExplicitStartPin` (`byroredux/src/components/explicit_start.rs`) keeps a `--cell` / `--grid` launch where it asked to be by ignoring scripted player relocation. It is documented as held "until the player takes control (the first key or mouse press)", and a benchmark or debug-server session "never presses a window key, so its pin stays".

`ExplicitStartPin::release` is called at the top of `window_event` (`byroredux/src/app_events.rs:362-365`), before the `loading_screen.active()` return (`:366`, which discards key and mouse input) and before the `bench::harness_owns_input` return (`:379`, whose own comment says matrix runs create focus-stealing windows and a coincident press must not act). So any pressed key or mouse button releases the pin, including presses that the loading cover discards, presses the bench harness quarantines, and presses that egui, the F3 overlay or a native page (pause, inventory, barter, message box) consumes. None of those is the player taking control of the world.

## Evidence
`window_event` order at HEAD: `Focused(false)` handling -> `ExplicitStartPin::release` on a pressed key / mouse -> `loading_screen.active()` return -> `harness_owns_input` return -> egui forward -> modal / overlay handling. `step_player_move_request` (`byroredux/src/app_step.rs`, ~`:911`) reads `ExplicitStartPin::active`. Orchestrator re-read the block and confirmed the order. Not reproduced (no engine launched), so reachability on a real Skyrim `--cell` launch is unconfirmed.

## Impact
A stray press during a bench or a loading cover, or a key used only to dismiss a modal, can let the first MQ101 start-up `MoveTo` run and relocate a player who launched with `--cell` / `--grid`; a bench's world state is then no longer the one the harness measures. Narrow: Skyrim-class start quests plus an explicit location, first scripted move only.

## Suggested fix
Move the release below the loading-cover, bench, egui and modal-surface early returns (for example into the existing world-input arms for `KeyboardInput` / `MouseInput`, after `route_scaleform_window_event`). Add a source-order test next to `bench_camera_startup_order_tests`.

Related: `bench::harness_owns_input`, the #5409 modal teardown family.

## Completeness Checks
- [ ] **SIBLING**: Other first-input latches in `window_event` (grep for state changes above the loading-cover return) checked for the same ordering
- [ ] **TESTS**: A source-order or behaviour test pins that a press swallowed by the loading cover / bench harness / modal does not release the pin
