# #5476: TOOL-D4-2026-10-08-01: #5294 is incomplete — `[defaults]` and the launcher's own registry ignore the selected profiles file, and a relative `--profiles` breaks Play

**Labels**: low,tech-debt,bug,test-gap
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5476

**Source**: `docs/audits/AUDIT_TOOLING_2026-10-08.md` — `TOOL-D4-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Incomplete-fix remainder of CLOSED #5294 (not a duplicate). Re-verified at HEAD: `ordered_config_paths` still hard-codes `home.join(".byroredux")` (`crates/game-detect/src/profiles.rs:353`); launcher `state.rs:74,90` and `byro-detect` `main.rs:55` call `load_default()`; `parse_args` stores `PathBuf::from(value)` unabsolutised.

- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence (it also touches Install Detection)
- **Exposure**: Launcher and `byro-detect` users who pass `--profiles`, or who set `BYRO_PROFILES` by hand. That is a documented developer flag (`tools/byro-launcher/README.md`, `docs/engine/launcher.md` §2.6). Players who launch with no arguments are unaffected.
- **Location**:
  - `crates/game-detect/src/profiles.rs:333-358` (`ordered_config_paths`), `:366` (`load_launch_defaults`)
  - `tools/byro-launcher/src/state.rs:74,90`
  - `tools/byro-launcher/src/app.rs:93,103-108`
  - `tools/byro-launcher/src/engine.rs:85,89`
  - `tools/byro-launcher/src/main.rs:67-86` (`parse_args`)
- **Status**: NEW (what #5294 left unfinished; #5294 is closed)
- **Description**: #5294 says, in the commit, in the `PROFILES_ENV` rustdoc and in `launcher.md` §2.6, that `$BYRO_PROFILES` selects "*every* per-user layer" so that a custom file "stays self-consistent". Three paths don't follow that rule:
  1. **The `[defaults]` layer is not redirected.** `load_launch_defaults()` iterates `ordered_config_paths()`, which still hard-codes `home.join(".byroredux").join("profiles.toml")` (`:353`). That is a fourth copy of the per-user path that #5294 set out to unify. Its doc comment says "Both `load_default` (profiles) and `load_launch_defaults` consume this so the two stay in lockstep", but `load_default` stopped using it and now calls `selected_user_path()`. The result: an engine started with `BYRO_PROFILES=X` takes its profiles and `[roots]` from X, but takes `[defaults]` (`game`, `cell`, `games_root`, `light_atten_*`) from `~/.byroredux/profiles.toml`. The consumers are `boot/cli.rs:310` (`expand_game_profile_args`: the default game and the `games_root` fallback), `boot/world.rs:91` and `studio_host.rs:546`.
  2. **The launcher validates against a different registry from the one the engine loads.** `LauncherState::load` and `refresh` build the registry with `detect::profiles::load_default()` inside the launcher's own process (`state.rs:74,90`). That call follows the launcher's own environment or home file, not the `--profiles` path. Only `detect_all(&profiles_path)` and `remember()` use the selected file. So a `[profiles.<key>]` block that exists only in the `--profiles` file is a "no profile named" Fail in the launcher, while the engine it spawns, given `BYRO_PROFILES`, knows that profile. The reverse also happens: an archive-list override is validated against the home file's version of the profile. `byro-detect` has the same split (`load_default()` at `main.rs:55` versus `--profiles` for `detect_all` and `--write`).
  3. **A relative `--profiles` path is never made absolute.** `parse_args` stores `PathBuf::from(value)` as given. `boot_request_path()` derives `boot.toml` from its parent, which is `""` for `--profiles alt.toml`, giving a bare `boot.toml`. `EngineProcess::spawn` then runs the engine with `.current_dir(engine.parent())`, passing `--boot boot.toml` and `BYRO_PROFILES=alt.toml`. Both now resolve against the engine's directory instead of the launcher's cwd. The engine fails at `BootRequest::load` ("could not load the launcher boot request"). Had it got past that, it would have read the wrong profiles file. The `boot.toml` half predates #5294. #5294 added the same defect for `BYRO_PROFILES`, and its spawn test uses only absolute tempdir paths.
- **Evidence**: `grep -n 'join(".byroredux")' crates/game-detect/src/profiles.rs` finds `:353` (`ordered_config_paths`) and `:406` (`user_profiles_path`). Only the second is behind `selected_user_path()`. The launcher has no `set_var(PROFILES_ENV, …)` and no `load_from(profiles_path)` (`grep -rn 'set_var\|PROFILES_ENV' tools/byro-launcher/src`).
- **Impact**: A developer who uses an alternate profiles file still gets a launcher and engine that disagree on default-game, `games_root` and light-tuning defaults, and on any custom profile block. A relative `--profiles` makes every Play fail with a confusing boot-request error. No data loss: all writes still go through `atomic_write` and land in the file the user named.
- **Related**: #5294, #5167, TOOL-D5-2026-10-05-01
- **Suggested Fix**:
  - Route `ordered_config_paths` through `selected_user_path()`, and fix its doc comment.
  - In the launcher and `byro-detect`, load the registry from the selected file. For example, add a `load_with_user_path(&Path)` that `load_default()` delegates to.
  - Canonicalise or absolutise `--profiles` in `parse_args`, or in `main` before use.
  - Add a relative-path case to the spawn test.

## Completeness Checks
- [ ] **SIBLING**: `byro-detect` gets the same selected-file registry load as the launcher
- [ ] **TESTS**: A regression test pins this specific fix (including a relative `--profiles` spawn case)
