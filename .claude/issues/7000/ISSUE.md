# #7000: UI-D6-2026-11-15-01: Keyboard, mouse and save hotkeys still reach the world and the native menus while a Bink movie covers the game

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/7000
- **Labels**: low,ui,bug
- **Source**: docs/audits/AUDIT_UI_2026-11-15.md

From the UI audit `docs/audits/AUDIT_UI_2026-11-15.md` (HEAD 8473ae994).

- **Severity**: LOW
- **Dimension**: Engine Wiring & Input
- **Profile**: n/a (engine input gate; affects every game's start movie)
- **Location**: `byroredux/src/app_events.rs:1152-1157` (window_event discard), `:1458` (device_event), `byroredux/src/main.rs:1255` (`capture_world_input`); the helper that should be used is `cover_holds_game` at `main.rs:1235-1237`
- **Status**: NEW (no matching issue; searched "movie input", "Bink input cover")
- **Description**: 6e64c453b added a second full-screen cover, the Bink movie, and a helper `App::cover_holds_game()` (`loading_screen.active() || video.active()`) whose doc says "simulation time is held still and world input is released". It was applied only in `about_to_wait` (`app_events.rs:1522` release and `:1543` dt). The three *event-time* quarantines the loading cover has were left on `loading_screen.active()` alone: the `window_event` discard of keyboard / mouse / wheel / touch / IME events (`app_events.rs:1152-1157`), the `device_event` mouse-look release (`:1458`) and `capture_world_input` (`main.rs:1255`). The only movie-specific input handling is Escape (`app_events.rs:1206-1210`, `video::VideoPlayer::request_skip`). So while a movie plays, every other press runs the full dispatch below the `video.active()` Escape check.
- **Evidence**: with `video.active()` true and no loading cover, a window event reaches: the inventory binding -> `open_inventory_menu()` (`app_events.rs:1372`) which sets `game_menu.visible` and `release_world_input_for_ui()`; `F` -> `toggle_player_mode` (`:1392`); `V` -> `toggle_third_person` (`:1400`); the quicksave / quickload binding -> `queue_player_save_action` (`:1290-1305`), so a quickload keypress during the intro queues a load of another timeline; a left click -> `capture_world_input()` which grabs and hides the cursor (`main.rs:1254-1268`), which `about_to_wait` then releases next tick (cursor flicker). The comment at `app_events.rs:1348-1354` lists "the loading cover, the bench harness, egui, the native menus and a focused Scaleform menu" as the things that return above the world-input arm; the movie is not in that list, so a key press during a movie also reaches `ExplicitStartPin::release` (`:1359`; only relevant for a named-location launch, which does not play start movies, so a note not a separate defect). Once the inventory page is open over a movie, Escape is owned by the movie (`:1204`) and cannot close it until the movie ends. The held-key leak the helper was written for is bounded (`about_to_wait` clears `InputState` every tick); the discrete one-shot actions above are not.
- **Impact**: pressing the inventory key, F, V or a save hotkey during an intro movie (the FNV new-game route plays `FNVIntro.bik` first, `docs/engine/video-playback.md`) acts on the world or opens a native page under a cover that is supposed to hold the game. Not a crash; the movie is short and (for the start movies) interruptable by Escape.
- **Related**: 6e64c453b; `docs/engine/video-playback.md` (BIK-PRES-002); the loading-cover discard is pinned only by `the_pin_is_released_after_every_input_quarantine` (`app_events.rs:2003-2030`), whose needle list says `self.loading_screen.active()`, so a fix must extend that pin too.
- **Suggested Fix**: replace the three `loading_screen.active()` event-time checks with `cover_holds_game()` (keeping Escape-to-skip ahead of the discard, as it already is), and add `self.cover_holds_game()` to the needle table in `the_pin_is_released_after_every_input_quarantine`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
