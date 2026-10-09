**HEAD**: 00f580e09 · **Baseline**: AUDIT_TOOLING_2026-10-05.md (HEAD a2c24b16e) · **Audited**: Dim 1 (Debug Trust Boundary), Dim 2 (Protocol & Registry), Dim 3 (SDK Contract), Dim 4 (Boot Handoff & Persistence), Dim 5 (Install Detection), plus two CI checks routed from this suite · **Unchanged since baseline (skimmed)**: Dim 6 (Tool CLIs — no commits)

# Tooling and Host-Contract Audit — 2026-10-08

This audit is scoped to the changes since the last one: `git log a2c24b16e..HEAD` (116 commits), filtered by each dimension's `Paths:`.

| Dim | Commits that touched tooling paths |
|---|---|
| 1 | `8a9989070` (#5292, fix of TOOL-D1-2026-10-05-01), `14cff35ae` (`main.rs`: dialogue close) |
| 2 | `00f580e09` (registry +3), `287214103`, `17fed2565`, `14cff35ae` (console commands `dialogue.forcegreet`, `sm.event`), `2464a52d7`, `b3f73a363` |
| 3 | `2464a52d7` (#5239 extension `SetBase`), `655b317c9` (#5100 source-scan delegation), `b24cb46b6` (`panels.rs` literals) |
| 4 | `322c36626` (#5294, fix of TOOL-D5-2026-10-05-01) |
| 5 | `f8950e7cc` (FNV `Voices1.bsa` added to the sounds pool), `322c36626` |
| 6 | none |

Method: one auditor, no sub-agents. Notes for each dimension are in `/tmp/audit/tooling/dim_<N>.md` and `dim_ci.md`. Nothing that needs a GPU was launched: no engine, no smoke scripts.

## Test baseline

Run with `cargo test -j 6 --no-fail-fast`, rustc 1.96.0 (the `rustup which` toolchain).

| Crate | Tests | Δ vs 10-05 |
|---|---|---|
| `byroredux-sdk` | 104 | = |
| `byroredux-debug-server` | 21 pass, **1 fail** | 1 now red: `debug_cli_component_counts_match_the_registry`. This is the already-filed CONC-D3-2026-10-08-04. |
| `byroredux-debug-protocol` | 7 | = |
| `byroredux-debug-ui` | 19 | = |
| `byroredux-boot-request` | 17 | = |
| `byroredux-settings-io` | 14 | = |
| `byroredux-game-detect` | 56 | +1 (`selected_user_path_prefers_the_env_override`) |
| `byro-launcher` | 25 + 1 ignored (needs Vulkan) | +1 (`profiles_parsing_is_strict`) |
| `byro-detect` | 3 | = |
| `byro-texture-upscale` | 18 | = |
| `byro-dbg` | 2 | = |
| `byroredux` bin, filtered to `debug_load`, `commands::assets`, `boot::cli`, `settings`, `studio`, `extensions`, `paused_frame_drain` | 128 | — |

## Executive Summary

**4 findings, all NEW: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 4 LOW.**

**Prior findings:**
- **#5292**: fixed and verified.
- **#5294**: the parser and spawn halves are fixed, but three gaps remain (TOOL-D4-01).
- **#4753 and #4754**: now closed, as the 10-05 report recommended.
- **Still open and unchanged at HEAD**: #5147 (non-unique default output names), #5149 (no loopback pin) and #5290 (absolute-path guidance in `debug-cli.md`:827/:1309, `tui.rs:650`, `display.rs:340`).

**Routed checks:**
- **The red workspace clippy step is a separate failure from the red test.** It is the single `clippy::type_complexity` error in `byroredux/src/systems/eat_sleep.rs:54`, also introduced by 00f580e09. No other audit filed it, so it is recorded here as TOOL-CI-01.
- **The parsers job's `dtolnay/rust-toolchain@stable` versus the 1.96.0 pin is real but harmless.** The CI log shows that rustup applies the `rust-toolchain.toml` override, and the rust-cache key uses 1.96.0. The `@stable` step only installs an unused 1.99 and prints its version, which misleads anyone reading the log. It is recorded as TOOL-CI-02 because it affects 10 sites, not only the parsers job.

**Areas swept with no new defect:**
- Debug trust boundary: the bind, the caps and the new world-mutating console commands. These have the same exposure as every other console command and do no file IO.
- Registry and console: all 101 registered command names are documented.
- SDK: all 28 `*_CAPABILITY` constants are checked at real call sites, and the dependency set is unchanged.
- Atomic writers.
- Install-detection guards.

## Prior-finding status

| Issue | Finding | State at HEAD |
|---|---|---|
| #5292 | Paused-frame drain keyed by a duplicated literal | **Closed, fixed.** `DRAIN_SYSTEM_NAME` is exported from `crates/debug-server/src/system.rs` and used by `name()`. The engine call is at `byroredux/src/app_events.rs:1001-1010` and has `debug_assert!(drained \|\| self.debug_server.is_none())`. The guard is `paused_frame_drain_names_the_system_through_the_shared_constant` (`scheduler_access_tests.rs:1082`). That guard reads `app_events.rs` with `include_str!`, but its needle lives in the test file, so it is not vacuous. |
| #5294 | Launcher `--profiles` lax; engine ignores custom file | **Closed, partly fixed.** Parsing is strict, `user_profiles_path()` is shared, and `BYRO_PROFILES` is exported on spawn and honoured by `load_default`. The remaining gaps are TOOL-D4-01. |
| #5290 | Absolute-path guidance in docs/byro-dbg | Open, unchanged |
| #5147 | `create_new` default-name collisions | Open, unchanged |
| #5149 | No loopback pin (`listener.rs:173` is still the literal `("127.0.0.1", port)`) | Open, unchanged |
| #4753, #4754 | Encode-side bound; byro-dbg timeout | Closed (as recommended 10-05) |

## Findings

### TOOL-D4-2026-10-08-01: #5294 is incomplete — `[defaults]` and the launcher's own registry ignore the selected profiles file, and a relative `--profiles` breaks Play
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

### TOOL-CI-2026-10-08-01: The workspace clippy step is red at HEAD on a `type_complexity` error, separate from the red registry-count test
- **Severity**: LOW
- **Dimension**: Tool CLIs / CI gate (`.github/` belongs to tech-debt and runtime in `_audit-owners.md`. The offending code is in `byroredux/src/systems/`, owned by ecs and performance, and no audit in this suite filed it.)
- **Exposure**: developers only (CI signal)
- **Location**: `byroredux/src/systems/eat_sleep.rs:54`; `.github/workflows/ci.yml:185-190`
- **Status**: NEW. It is distinct from CONC-D3-2026-10-08-04, which is the `debug_cli_component_counts_match_the_registry` test failure in the same job's `cargo test` step.
- **Description**: 00f580e09 added `let mut actors: Vec<(EntityId, EatOrSleep, Option<f32>, Option<u32>, u32)> = Vec::new();`. Clippy 1.96.0 (the pinned toolchain CI actually uses, see TOOL-CI-02) rejects it under `-D warnings` with `clippy::type_complexity`.
- **Evidence**: CI run 37848932617, job 113556725530, step "cargo clippy":
  - `error: very complex type used … --> byroredux/src/systems/eat_sleep.rs:54:21`
  - `error: could not compile byroredux (bin "byroredux") due to 1 previous error`

  This is the only clippy error in the log, and every other crate passed. The dedicated renderer unsafe-block gate step (`ci.yml:199-201`) was green.
- **Impact**: The workspace clippy gate is red. Because of `--keep-going` and because `byroredux` is the leaf crate, no other crate goes unlinted. But while the step stays red, the next real lint in the `byroredux` crate lands without a new signal. #5308 and #4595 show this repo has paid for multi-day red clippy boards before. Fixing CONC-D3-04 alone will not turn the job green.
- **Related**: CONC-D3-2026-10-08-04, #5308, #5121, #4595
- **Suggested Fix**: Name the tuple as a type alias or a small struct (for example, `type EatSleepCandidate = (…)`) in `eat_sleep.rs`, then re-run `cargo clippy -p byroredux --no-deps -- -D warnings` on 1.96.0.

### TOOL-CI-2026-10-08-02: `dtolnay/rust-toolchain@stable` survives in 10 CI steps after the 1.96.0 pin, and its version output names a toolchain no step uses
- **Severity**: LOW
- **Dimension**: Tool CLIs / CI gate (`.github/`, `rust-toolchain.toml` → tech-debt)
- **Exposure**: developers only (CI log readers; time on the self-hosted runners)
- **Location**:
  - `.github/workflows/ci.yml:138,222,317,354`
  - `.github/workflows/real-data-gates.yml:112,161` (corpus, parsers)
  - `.github/workflows/rt-correctness.yml:28,72`
  - `.github/workflows/playable-smoke.yml:38`
  - `rust-toolchain.toml`
- **Status**: NEW. The parsers audit routed it here. The intent is #5308, which is closed.
- **Description**: #5308 pinned `channel = "1.96.0"` (plus clippy) in `rust-toolchain.toml` to stop surprise reds when stable moves. The workflows still run `dtolnay/rust-toolchain@stable` first. That step installs current stable, sets it as rustup's *default*, and reports its version and cache key. Inside the checkout, rustup's toolchain-file override wins, so every `cargo` call runs on 1.96.0. The pin works. The step is dead weight that looks authoritative. `ci.yml:289-290` already documents this override for the Miri job.
- **Evidence**: CI run 37848932617, Test+Check+Clippy:
  - dtolnay step: `stable-x86_64-unknown-linux-gnu unchanged - rustc 1.99.0`, then `info: note that the toolchain '1.96.0-x86_64-unknown-linux-gnu' is currently in use (overridden by '…/rust-toolchain.toml')`. Its version step prints `rustc 1.99.0`.
  - Swatinem/rust-cache keys on `1.96.0 x86_64-unknown-linux-gnu ac68faa20…`.
- **Impact**: No effect on correctness: builds, tests, clippy and the cache key all use 1.96.0. Two costs remain. A log reader, or the parsers audit as happened here, concludes the lane is unpinned. Each self-hosted game-data runner (`corpus`, `parsers`, `playable-smoke`) also installs and keeps updating a stable toolchain it never uses. Someone who deletes `rust-toolchain.toml` believing CI pins elsewhere would silently re-open #5308.
- **Related**: #5308, `4a6a6cdd7`, `10f02bac1`
- **Suggested Fix**: Do one of the following, and drop the now-redundant `components: clippy` input:
  - Replace the step with `dtolnay/rust-toolchain@1.96.0` (or `@master` with `toolchain: 1.96.0`).
  - Remove it and rely on the toolchain file, adding a `rustc --version` step so the log names the toolchain actually used.

### TOOL-D2-2026-10-08-01: The new `sm.event` / `dialogue.forcegreet` commands widen the unauthenticated world-mutation surface; noted for the exposure matrix only
- **Severity**: LOW
- **Dimension**: Debug Trust Boundary
- **Exposure**: Any local process on a debug build, or on a release build with `BYRO_DEBUG_SERVER=1` set.
- **Location**: `byroredux/src/commands/quest.rs:862-1000` (`DialogueForceGreetCommand` :862, `SmEventCommand` :930); `byroredux/src/commands/mod.rs:91-92`
- **Status**: NEW (informational; the same class as every console command, no defect in the commands themselves)
- **Description**: Both commands mutate gameplay state through the real dispatchers. `dialogue.forcegreet` installs a force-greet package on any NPC. `sm.event` raises a Story Manager event, which can start quests. Both are reachable unauthenticated over the debug port, like every console command. Their argument parsing is panic-free: it uses `split_whitespace`, `split_once('=')` and `parse_console_u32`, each with explicit error returns. Neither does file IO.
- **Evidence**: `git diff a2c24b16e..HEAD -- byroredux/src/commands/quest.rs` contains no `unwrap`, `expect`, slice indexing or `fs::` calls in the new code.
- **Impact**: None beyond the existing exposure posture (row 1 of the matrix). The finding is recorded so the next run's matrix reflects that the count went from 99 to 101.
- **Related**: #5149 (loopback pin), #5366, #5367
- **Suggested Fix**: None required. If the player-build posture ever tightens (a separate allow-list for mutating commands), include these two.

## Dimension notes

- **Dim 1**:
  - The caps are unchanged: 64 queued commands, 8 clients, 300 s idle timeout, 30 s / 35 s response timeouts, and 16 MB frames enforced on both encode and decode.
  - No new path-taking request was added. File writes are still only `write_screenshot` and `safe_tex_dump_path`.
  - The #5292 `debug_assert` is gated on `self.debug_server.is_none()`. The drain system is registered only on `start()`'s success path, so a refused bind does not trip it.
  - The #5292 guard slices `APP_EVENTS_RS[call..call + 400]` by byte offset. That is safe today (the window is ASCII), but a future multi-byte character at the boundary would panic the test. Fragile, not filed.
- **Dim 2**:
  - The live registry has 69 production `register_component::<` calls plus `register_faction_reputation`, for 70. `debug-cli.md` still says 67. That is CONC-D3-04, not re-filed.
  - The three new components are in `crates/core/src/ecs/components/eat_sleep.rs`, and their registered field names match the struct fields. The "inspect derive outside core" gap class is still empty.
  - All 101 registered console names appear in `debug-cli.md`, and its count of 101 is current.
  - There were no protocol or byro-dbg changes.
- **Dim 3**:
  - `crates/sdk` was not touched. `extensions/commands.rs` now routes `SetBase` on the player's derived pools to `set_permanent`. That change is owned and filed by `/audit-character` (CHAR-2026-10-08-D4-01) and is not re-reported.
  - `extensions/mod.rs` now delegates to `byroredux_core::source_scan::strip_test_modules`, which suits that file's interleaved test modules.
- **Dim 4**:
  - All three writers still go through `atomic_write`. No `fs::write` or `fs::rename` was found in production writer text.
  - The launcher's `cargo tree` posture is unchanged.
  - New finding: TOOL-D4-01.
- **Dim 5**:
  - FNV `default_sounds_bsas` now ends with `Fallout - Voices1.bsa`. That archive is vanilla on every FNV release and present locally, so the validator requiring it is correct.
  - `is_bare_install_dir` and the 1 MiB `esm_opens` probe are unchanged.
  - The legacy `BYROREDUX_SKYRIM_DATA` survives only as the `vars.` fallback at `playable-smoke.yml:45`.
- **Dim 6**: no commits. 18 tests pass.

## Cross-audit references (not re-filed)

- **CONC-D3-2026-10-08-04**: `debug_cli_component_counts_match_the_registry` is red. This audit reproduced it locally (21/22).
- **CHAR-2026-10-08-D4-01**: extension and console `SetBase` on Health/AP overwrites the permanent-modifier layer.
- **Ownership**: debug-server locks belong to `/audit-concurrency` Dim 7 and sandbox enforcement to `/audit-safety` Dim 8. Neither was re-audited.

## Exposure Matrix

| Surface | Reachable by | Authenticated | Size-capped | Persistence atomic |
|---|---|---|---|---|
| Debug TCP port (101 console commands incl. `sm.event`, `dialogue.forcegreet`, `hud.*`, `save`, `engine.quit`, `SetField`) | any local process; debug builds by default, release with `BYRO_DEBUG_SERVER=1`; drained while paused (#5141/#5292) | No | Yes: 16 MB frames both ways, 64 queued, 8 clients, walk depth 64, 30 s / 35 s timeouts | n/a (save slots are owned by `/audit-save`) |
| Debug loads (`LoadNif`, `LoadInteriorCell`, `LoadExteriorCell`) | same | No | Paths confined to startup roots; a rejection is only logged after an `Ok` (#5290) | n/a (read-only) |
| Screenshot / `tex.dump` | same | No | Bare filename under `screenshots/` / `texture-dumps/`, `create_new` (#5147 collisions open) | n/a |
| `settings.toml` | launcher, engine, extensions | n/a | n/a | Yes (`atomic_write`); only changed ids are written (#5162) |
| `boot.toml` | launcher → engine | n/a | n/a | Yes; version refused on mismatch. A relative `--profiles` misplaces it (TOOL-D4-01) |
| `profiles.toml` | launcher (on Play), `byro-detect --write` | n/a | n/a | Yes, format-preserving (#5166). `BYRO_PROFILES` is honoured by the engine's profile and `[roots]` layers but not by `[defaults]` or the launcher's registry (TOOL-D4-01) |
| texture-upscale manifest | local operator | Trusted executable input (documented) | Disk space checked first | Output collisions rejected (#4761) |

## Summary

| Severity | NEW | Already tracked (seen, not re-filed) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 0 | 0 |
| LOW | 4 | 5 (#5147, #5149, #5290, CONC-D3-2026-10-08-04, CHAR-2026-10-08-D4-01) |

## Next step

Run `/audit-publish docs/audits/AUDIT_TOOLING_2026-10-08.md`. Labels:
- `tech-debt` on all four (the launcher, CI and the debug server have no label of their own).
- `test-gap` as well on TOOL-D4-01 (the spawn test covers only absolute paths).
- TOOL-D2-08-01 is informational and may be folded into the exposure matrix instead of being published.
