# #4974: REN-D11-2026-09-27-02: a CLI-seeded upscaler (`--upscaler` / `--fsr-quality`) is written to `settings.toml` by the next unrelated settings save, so a one-launch flag becomes the persisted default

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4974
- **Labels**: low,renderer,tech-debt,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D11-2026-09-27-02**._

- **Severity**: LOW (tooling/persistence; owner overlap with `/audit-tooling` settings-io handoff)
- **Dimension**: FSR/Presentation
- **Location**: `byroredux/src/main.rs` `install_universal_settings` (`if explicit_upscaler { settings.set(UPSCALER_SETTING_ID, …) }`); `crates/settings-io/src/lib.rs` `save_to_path` (`for entry in registry.entries() { settings.insert(…) }`); the save trigger in `main.rs`'s debug-UI output loop (`if settings_changed { settings_io::save(…) }`).
- **Status**: NEW
- **Description**:
  - With an explicit flag, the CLI value is written into the live `SettingsRegistry` so the menu shows it.
  - `save_to_path` serialises every registry entry, not just changed ones.
  - Any later settings change in that session (FOV, a key binding, HUD scale) therefore persists the CLI upscaler too.
  - This contradicts the README's "Explicit renderer CLI flags still win **for that launch**". It is a plausible origin of the `fsr3/native-aa` on this machine: `scripts/fsr-bench-matrix.sh` runs `--fsr-quality native-aa`, and a hold session with any menu touch would persist it. Not proven — the file's history is unknown.
- **Evidence**: The code path above. Nothing in `install_universal_settings` marks the seeded value as transient.
- **Impact**: This feeds REN-D11-2026-09-27-01: a bench flag leaks into every later flagless launch.
- **Related**: REN-D11-2026-09-27-01.
- **Suggested Fix**: Keep the CLI override out of the persisted layer. Either don't `set` the registry, or remember the pre-seed persisted value and restore it for `save`. Alternatively, persist only entries changed through the UI.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
