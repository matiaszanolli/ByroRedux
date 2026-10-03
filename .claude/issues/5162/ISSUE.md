# #5162 — TOOL-D4-2026-10-02-01: The launcher writes back a settings snapshot taken when the launcher started, reverting settings changed in-game

Labels: medium,tech-debt,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/5162

From `docs/audits/AUDIT_TOOLING_2026-10-02.md` (HEAD `e737f06bf`).

- **Severity**: MEDIUM
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: every launcher user. User preferences are lost silently.
- **Location**: `tools/byro-launcher/src/app.rs:65` (`SettingsState::load()` at construction), `:112-128` (`poll_engine`, which does not reload); `tools/byro-launcher/src/settings_screen.rs:34-57,101,110-111`; `crates/settings-io/src/lib.rs:225-230`; engine side `byroredux/src/main.rs:1630-1646`
- **Status**: NEW
- **Description**:
  - The launcher builds its `SettingsRegistry` and overlays `settings.toml` once, in `LauncherApp::new`. Nothing reloads it: not when the engine process exits, and not when the user opens the Settings screen.
  - Any control change sets `dirty`, and the same frame calls `save()`. `save_to_path` re-reads the file, but then inserts **every** registry entry over it (`for entry in registry.entries() { settings.insert(...) }`), not just the changed one.
  - The engine does the same thing in the other direction. Each in-game universal-setting change saves the engine's whole registry, which was loaded at engine boot.
- **Evidence**:
  1. Start the launcher, press Play, and change FOV in the in-game Settings. The engine saves `camera.fov` = new.
  2. Quit to the launcher, open Settings, and toggle anything. The launcher's registry still holds the old FOV, so `save_to_path` writes it back.
  3. The in-game FOV change is gone.

  The reverse case: with the engine running, change a value in the launcher. The engine's next save writes its boot-time value back over it.
- **Impact**: Built-in settings changed in one frontend are silently reverted by the other. These include FOV, mouse sensitivity, invert-Y, HUD visibility, UI scale and the upscaler. Key bindings survive only because the launcher's registry does not know those keys. This is exactly the "one model, two skins" sharing that `docs/engine/launcher.md` §4 describes.
- **Related**: #5144 (same file, different failure), #3472, TOOL-D4-02
- **Suggested Fix**: Make the save write only the entries this frontend changed (track dirty ids, or diff against the loaded snapshot) instead of the whole registry. Also reload the launcher's registry when the engine exits and when the Settings screen opens.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / frontends / request variants named above)
- [ ] **TESTS**: A regression test pins this specific fix

