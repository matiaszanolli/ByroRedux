# #7002: UI-D7-2026-11-15-02: The detection indicator is not counted as "something to draw" in `DebugUiState::run`'s hidden-overlay early return

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/7002
- **Labels**: low,ui,bug
- **Source**: docs/audits/AUDIT_UI_2026-11-15.md

From the UI audit `docs/audits/AUDIT_UI_2026-11-15.md` (HEAD 8473ae994).

- **Severity**: LOW
- **Dimension**: MenuXml & HUD Drivers (native HUD consumer of the same indicator)
- **Profile**: n/a
- **Location**: `crates/debug-ui/src/lib.rs:385-406` (the early-return condition), `crates/debug-ui/src/panels/hud.rs:37-39` (the draw)
- **Status**: NEW (014e64ef1 added `PanelSnapshot::stealth_indicator` and the draw, not the gate term)
- **Description**: `run` skips the whole egui pass when the overlay and menu are hidden and nothing "visible" is in the snapshot: no prompt, no crosshair, no vitals content, no objectives content, no loading tip, no player message. The new `stealth_indicator` is drawn by `draw_hud`, which only runs after `begin_pass`, but it is not one of the terms. With the crosshair setting off, vitals off or empty and no objectives (the settings registry has `SHOW_CROSSHAIR`, `SHOW_VITALS`, `SHOW_OBJECTIVES`, `SHOW_PROMPTS`, `lib.rs:59-60`; there is no setting for the indicator), a sneaking player's `[HIDDEN]` / `[DETECTED]` label never draws, although the snapshot carries it (`app_frame.rs:352`, `main.rs:1443`).
- **Evidence**: read of the condition and the draw site. The unit test `the_stealth_indicator_draws_in_each_look_and_not_without_a_view` (`panels/hud.rs:361-395`) calls `draw_hud` directly, so it cannot see the gate. Not run end to end (needs a window); this is a code reading.
- **Impact**: a user who turns the crosshair, vitals and objectives off (a HUD-less preset) loses the stealth indicator while sneaking. With any of those on, it draws.
- **Related**: 014e64ef1; SNK-HUD-001.
- **Suggested Fix**: add `&& snapshot.stealth_indicator.is_none()` to the condition, and give the indicator its own `SHOW_*` setting if HUD-less play is meant to be honoured (then gate on it like the other layers).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
