**HEAD**: 4a3e89d56 · **Baseline**: AUDIT_TOOLING_2026-10-12.md (HEAD 5d31fd637) · **Audited**: Dim 1 (Debug Trust Boundary), Dim 2 (Protocol & Registry), Dim 3 (SDK Contract), Dim 4 (Boot Handoff & Persistence), Dim 5 (Install Detection) · **Unchanged since baseline (skimmed)**: Dim 6 (Tool CLIs; the only commit is `e7913ec21`, the #5695 test), the durability writers in Dim 4 (`atomic_file.rs`, `boot-request`, `settings-io` have no commits), `crates/sdk` outside `component.rs`, `studio_host.rs`

# Tooling and Host-Contract Audit - 2026-10-21

Scope: `git log 5d31fd637..HEAD` (784 commits), filtered by each dimension's `Paths:`.

| Dim | Commits that touched tooling paths |
|---|---|
| 1 | `cff99166e` (#5290 reject absolute load paths at enqueue), `44219e881` (#5009 provider-backed debug NIF loads), `13877d3ce` (Linux alpha release pipeline, `--version`/`--help`), `dcc527528` (#5692), `044fa8031` (#5327), 33 new console commands (101 → 134) |
| 2 | `f64a299ec` (#5822 `Metrics(MetricsPayload)`), `3ad23b3e4` (#5694 doc table names guard), `ab34039b8` (#6056 doc count), `dbe562c1f`/`f0ca6eda1` (+`CharacterLevel`, +`LevelUpPoints`) |
| 3 | `91a28a4c9` (#5592 one `deliver` body), `78cfa46df` (#5810 `with_extension_host`), `e634fbf9b` (#5816 `field_declaration`/`stage_row`), `6f7dd2213` (#5994 NativeActivation), `106f636be` (#5813) |
| 4 | `da3f62128` (launcher crash tail waits for stderr reader), `b3ca11cf1` (#5911 non-UTF-8 `--profiles`), `0f07131a8` (#5739 shared strict parser), `d3225bb2f` (#5717 bounded preflight names), `2a8b8e505` (rotation-mode default), `13877d3ce` (release packaging docs) |
| 5 | `21538a9d0` (#5910 explicit roots file), `fa7591ca2` (#5526 Oblivion voice archives), `4a0431073` |
| 6 | `e7913ec21` (#5695) |

Method: one auditor, no sub-agents, no engine launch. Notes in `/tmp/audit/tooling/`.

## Test baseline

`cargo test -j 8 --no-fail-fast` over the eleven tooling crates (repo `target/`). All green.

| Crate | Tests | Delta vs 10-12 |
|---|---|---|
| `byroredux-sdk` | 104 | = |
| `byroredux-debug-server` | 25 | +1 (`absolute_load_paths_are_rejected_before_queueing`, #5290) |
| `byroredux-debug-protocol` | 7 | = |
| `byroredux-debug-ui` | 20 | +1 |
| `byroredux-boot-request` | 17 | = |
| `byroredux-settings-io` | 14 | = |
| `byroredux-game-detect` | 60 | +1 (`explicit_profile_file_also_supplies_root_overrides`, #5910) |
| `byro-launcher` | 29 + 1 ignored (Vulkan) | +4 (crash-tail drain pair, non-UTF-8 / empty `--profiles`) |
| `byro-detect` | 6 | +2 (non-UTF-8 / empty `--profiles`) |
| `byro-texture-upscale` | 19 (lib) + 0 (bin) | +1 (`read_only_output_or_scratch_is_refused_by_role`, #5695) |
| `byro-dbg` | 2 | = |

## Executive Summary

**3 findings, all NEW: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 3 LOW.**

All three findings from 10-12 are fixed at HEAD:

| Issue | State at HEAD |
|---|---|
| #5692 (TOOL-D1-2026-10-12-01, displaced doc comments) | Fixed (`dcc527528`). `unique_screenshot_name` and `set_visible` carry their own docs. |
| #5694 (TOOL-D2-2026-10-12-01, doc table missing rows) | Fixed (`3ad23b3e4`). The guard now requires a table row per registered name; `debug-cli.md` describes the `_N` suffix defaults at `:127` and `:380`. |
| #5695 (TOOL-D6-2026-10-12-01, read-only refusal untested) | Fixed (`e7913ec21`). Pure `refuse_read_only` plus `read_only_output_or_scratch_is_refused_by_role`. |
| #5290, #5327 | Closed, verified. Absolute and `..` load paths now get an `Error` naming the path at enqueue; `debug_server_allowed` is `cfg(any(test, feature = "debug-server"))`. |

Areas swept with no new defect:
- **Debug trust boundary.** The bind is still the literal `("127.0.0.1", port)` (`listener.rs:173`) and is test-pinned. The caps are unchanged. Every guard the skill names exists. 33 console commands were added since 10-12 (134 now). None of them reads or writes a client-chosen path. The only file reads, in `terrain.height`, go to the engine's own height-cache paths. `script.exec`, `script.call`, `script.qcall` and `obscript.cmd` give the whole Papyrus/ObScript VM to any client of the debug port. That is the documented capability surface, and a release build only opens the port with `BYRO_DEBUG_SERVER=1`. `terrain.blend` input goes through `SettingsRegistry::set` → `validate_value`, which checks the value is finite and in range, and the command pins the band session-only (#6204). The new release tarball ships `byro-dbg`, and `packaging/linux/README.txt` documents the opt-in.
- **Registry and docs.** 73 production `register_component::<` calls plus `register_faction_reputation` = **74**, which matches both count sites in `debug-cli.md`. No `inspect` derive exists outside `crates/core` (only a save-guard test file mentions the string). The doc's command count (134) matches the registry. A reverse diff of every dotted command name in the doc's command block against the 134 registered `name()` strings comes back empty.
- **SDK.** The dependency set is unchanged (`byroredux-core` for `math::Vec3` only, `semver`, `serde`, `thiserror`). The #5816 refactor keeps the type check before row staging and the per-principal row budget. The #5592 `deliver` helper applies the `EVENTS_SUBSCRIBE_CAPABILITY` + `Active` gate once for every family; input delivery still also requires `INPUT_ACTIONS_SUBSCRIBE_CAPABILITY`. Extension activation reads `papyrus_activations`, so it honours `NativeActivation` (#5994). All 28 `*_CAPABILITY` constants have a non-test call site.
- **Persistence.** There is no `fs::write` in any production writer. `cargo tree -p byro-launcher -i wgpu` prints nothing. `parse_profiles_arg` takes `OsString` (#5911), and an explicit profiles file also supplies `[roots]` (#5910).
- **Install detection.** The Oblivion profile now lists `Oblivion - Voices1.bsa` / `Voices2.bsa` (#5526). `numeric_sibling_paths` treats a trailing non-zero digit as explicit, so neither archive is opened twice. Shivering Isles voices are already in the DLC tier. `byro-detect` usage text matches its strict parser.

Cross-audit: `CONC-2026-10-21-02` (shared height-job temp file name, `asset_provider/terrain_height.rs:203`) is the only new production `fs::write` + `fs::rename` pair seen in this sweep, and `/audit-concurrency` already reports it. It is not re-filed here.

## Findings

### TOOL-D2-2026-10-21-01: The `metrics_response_mirrors_core_snapshot` guard cited by `DebugResponse::Metrics` does not exist, and two core metrics fields never reach the wire
- **Severity**: LOW
- **Dimension**: Protocol & Registry
- **Exposure**: developers only (`byro-dbg metrics` / TUI dashboard)
- **Location**: `crates/debug-protocol/src/lib.rs:238-244` (variant doc), `crates/debug-protocol/src/lib.rs:257-278` (`MetricsPayload`), `crates/debug-server/src/evaluator.rs:168-198` (`eval_metrics`), `crates/core/src/ecs/metrics.rs` (`MetricsSnapshot.cpu_pass_ms`, `.top_systems_ms`)
- **Status**: NEW. The skill has carried "report it until a test lands" since #5822; no earlier report filed it, and there is no issue (open or closed) for the guard name.
- **Description**: The variant doc says the field set is pinned so "a one-sided drift breaks the build", and the `f64a299ec` (#5822) commit message says the evaluator's conversion is "pinned as before by ... the server's metrics_response_mirrors_core_snapshot guard". That test exists nowhere in the tree, and `git log -S` finds no trace of it ever existing. The drift it was meant to catch is already present. Core's `MetricsSnapshot` has two more fields than `MetricsPayload`: `cpu_pass_ms` (Phase 8 CPU frame breakdown, `8d4c14522`, 2026-05-24) and `top_systems_ms` (Phase 11 per-system scheduler timings, `1138a5b19`). `eval_metrics` maps neither. The in-engine debug-ui panel shows them (`crates/debug-ui/src/panels.rs:1571-1601`), but the wire client cannot.
- **Evidence**: `grep -rn metrics_response_mirrors_core_snapshot crates byroredux tools` → only the doc comment at `lib.rs:242`. Field list of `MetricsSnapshot` (17 `pub` fields) vs `MetricsPayload` (15) → missing `cpu_pass_ms`, `top_systems_ms`.
- **Impact**: someone triaging a frame-time gap from `byro-dbg` (the "atw_scheduler is 500 ms, which system?" case Phase 11 was built for) sees GPU pass times only, and the doc tells them the payload is complete. The next field added to core will be dropped the same way, with nothing to catch it.
- **Related**: #5822, #2513
- **Suggested Fix**: Add `cpu_pass_ms` / `top_systems_ms` to `MetricsPayload` (serde-default for older clients) and map them in `eval_metrics`, then write the guard the doc cites. One way is an exhaustive destructure of `MetricsSnapshot` in the evaluator (`let MetricsSnapshot { sampled_at_secs, .., top_systems_ms } = &*snap;` with no `..`), which makes a new core field a compile error.

### TOOL-D4-2026-10-21-01: The launcher's stderr reader stops at the first non-UTF-8 line and closes the pipe, after which the engine's next `eprintln!` panics
- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence (launcher supervision)
- **Exposure**: launcher user (any engine started by `byro-launcher`)
- **Location**: `tools/byro-launcher/src/engine.rs:105-116` (reader thread; loop body from `c53527ff7`, 2026-08-30, wrapped by `da3f62128` this window)
- **Status**: NEW
- **Description**: The reader loop is `for line in BufReader::new(stderr).lines().map_while(Result::ok)`. `BufRead::lines` yields `Err(InvalidData)` for a line that is not valid UTF-8, and `map_while` then ends the loop for good. The thread exits and drops the `ChildStderr`, which closes the read end of the pipe. The engine's stderr is that pipe, and the Rust runtime ignores `SIGPIPE`. Its next stderr write therefore fails with `EPIPE`, and `eprintln!` panics on a write error ("failed printing to stderr"). There are 53 production `eprintln!` sites in `byroredux/src`. `env_logger` drops write errors, so logging alone survives, but the first `eprintln!` takes the engine down with exit 101. The launcher then shows a crash whose tail ends at the line before the bad bytes, so the report hides both the trigger and the panic.
- **Evidence**: `engine.rs:107-114`; the `da3f62128` change only waits for this thread to finish, so it does not change how the loop ends. First-party Rust code does not emit non-UTF-8 text: there are no raw `stderr()` writes in `byroredux/src` or `crates/*/src`, and paths are printed through `display()`. The ffmpeg `.fuz` decoder pipes its own stderr (`crates/audio/src/fuz.rs:241`). The realistic trigger is a native component that writes straight to fd 2 inside the process, such as a driver, a Vulkan layer or the FSR3 runtime, emitting a non-UTF-8 byte.
- **Impact**: when it triggers, the engine crashes over a diagnostic line and the crash report loses its cause. Before that, nothing records what the engine writes. The same gap applies to a single very long line with no newline, which buffers without bound in the reader. The likelihood is low and the fix is cheap.
- **Related**: `da3f62128`, #5294 (launcher-spawned engine contract)
- **Suggested Fix**: Read with `read_until(b'\n', &mut buf)` and push `String::from_utf8_lossy(&buf)`, capping the line length. On any non-EOF read error, keep draining into a sink instead of returning, so the pipe stays open for the life of the child. A test helper that writes `\xff\n` and then more lines should see the later lines in the tail.

### TOOL-D4-2026-10-21-02: The release README and `docs/releasing.md` say `assets/` is read only from the current directory, but both loaders fall back to the executable's directory
- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence (release packaging docs)
- **Exposure**: players using the alpha tarball
- **Location**: `packaging/linux/README.txt:15-16`, `:31-32` and `:45-47` ("REQUIRED, read relative to the current directory"; "the engine and launcher look for assets/ in the current directory"; and the NOTE under "Running": "byroredux cannot find assets/debug_profiles.toml unless your current directory contains an assets/ folder. Copy or symlink this folder's assets/ ..."), `docs/releasing.md:19` ("Read **relative to the current directory**, so the binaries are run from this folder"). Both were added by `13877d3ce`.
- **Status**: NEW
- **Description**: `crates/game-detect/src/profiles.rs` `load_default_from` and `ordered_config_paths_with` try `assets/debug_profiles.toml` relative to the current directory first, then `current_exe().parent()/assets/...` (since `85c22964a`, 2026-08-30; the code comment says "binary-parent-relative (release ships)"). `crates/settings-io/src/presets.rs` `PresetFile::load_default` does the same for `graphics_presets.toml`, and its doc says it does this "so a release build beside its `assets/` works without a working-directory convention". The tarball puts `byroredux` beside `assets/`, so the documented workflow (run from the game's `Data` directory) finds both files with no copy or symlink. On Linux `current_exe` resolves symlinks, so a symlink to the binary in `~/bin` works too.
- **Evidence**: `profiles.rs:308-321` and `:403-416`; `presets.rs:81-95`.
- **Impact**: players are told to copy or symlink `assets/` into each game's Data directory. A copy taken that way wins over the shipped file (the cwd copy is tried first), and it goes stale on the next release, so following the instruction produces old profiles or presets.
- **Related**: `13877d3ce`, #5294
- **Suggested Fix**: Say the binaries find `assets/` beside themselves, and that an `assets/` in the current directory overrides it. Drop the "copy or symlink" NOTE.

## Dimension notes

- **Dim 1**: `eval_queue_load` now rejects through `PendingDebugLoad::first_non_root_relative_path` (`crates/core/src/ecs/debug_load.rs:78`). `confine_to_roots` still canonicalises and checks `starts_with(root)` plus `is_file()`, so the drain-time half is intact. `13877d3ce` adds `early_info` (`--version`/`--help` as the first argument only); it does not touch the debug-server gate. Observation, not filed (UI-owned, pre-baseline, env-gated): `byroredux/src/hud.rs:699` writes the fixed path `/tmp/hud_engine_frame.rgba` when `BYRO_HUD_DUMP=1`.
- **Dim 2**: No new `DebugRequest`/`DebugResponse` variant. `Metrics` became a newtype, and serde keeps the JSON shape. `tui.rs` `variant_name` is exhaustive, and `handle_response`'s `other =>` arm reports through it. Finding D2-01.
- **Dim 3**: The SDK crate's only change is the behaviour-preserving #5816 extraction (it is not rustfmt-formatted, and CI runs no fmt step). The host validators (`valid_transform`/`valid_material`) and the panel sanitizers are unchanged.
- **Dim 4**: The writers and version policies are unchanged. `rotation_mode_arg` reads through `cli_args::parse_string_arg`. The remaining hand-rolled `args.iter().position` readers (`--studio`, `--menu`, `--swf`, `--bsa`, `--hud-textures`, `--cmd`, `--sf-smoke`, `--list-cells`) are the raw-by-design set. Findings D4-01 and D4-02.
- **Dim 5**: `fa7591ca2` adds the Oblivion voice archives (present in the local install). `root_overrides_path` prefers the explicit file. `is_bare_install_dir` and the 1 MiB `esm_opens` probe are unchanged.
- **Dim 6**: unchanged apart from the #5695 test, which is present and green.

## Exposure Matrix

| Surface | Reachable by | Authenticated | Size-capped | Persistence atomic |
|---|---|---|---|---|
| Debug TCP console (`127.0.0.1:9876`; 134 commands + `SetField`, incl. `script.exec`/`obscript.cmd` VM doors) | any local process on a debug build, or a release with `BYRO_DEBUG_SERVER=1` (release tarball documents it); loopback pinned by test | no | yes (frame 16 MB, 64 queued, 8 clients, depth 64) | n/a |
| `Screenshot` / `tex.dump` file writes | same | no | PNG only; filename-only under `screenshots/` / `texture-dumps/`; `create_new`, unique default names | no (single `create_new` write, removed on write error) |
| `LoadNif` / `LoadInteriorCell` / `LoadExteriorCell` | same | no | absolute/`..` refused at enqueue; canonicalised into startup roots at drain | n/a |
| SDK extensions (`--extension`, wasm) | modder with a manifest and grants | capability grants (one gate in `deliver`) | `ComponentStoreLimits` (row budget in `stage_row`) | n/a |
| `boot.toml` handoff | launcher user | no (versioned) | n/a | yes (`atomic_write`) |
| Engine stderr → launcher crash tail | launcher-spawned engine | n/a | 200-line tail; line length unbounded (D4-01) | n/a |
| `settings.toml` | launcher + engine | no (versioned, baseline-merge, session pins) | n/a | yes |
| `profiles.toml` `[roots]` merge | `byro-detect --write`, launcher | no | n/a | yes (`atomic_write`, formatting-preserving) |
| texture-upscale manifest | developer | manifest is trusted executable input | disk-space, read-only and overflow checks | writes new PNGs, `--overwrite` required to replace |

## Next step

`/audit-publish docs/audits/AUDIT_TOOLING_2026-10-21.md` (labels: `tech-debt` for all three, since debug-server and launcher have no label of their own; add `test-gap` + `doc-rot` for D2-01 and `doc-rot` for D4-02).
