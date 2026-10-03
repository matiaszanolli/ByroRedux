**HEAD**: e737f06bf · **Baseline**: AUDIT_TOOLING_2026-09-29.md (HEAD 9fcfdc3fc) · **Audited**: Dim 1 (Debug Trust Boundary), Dim 2 (Protocol & Registry), Dim 4 (Boot Handoff & Persistence), Dim 5 (Install Detection) · **Unchanged since baseline (skimmed)**: Dim 3 (SDK Contract — clippy-only commits), Dim 6 (Tool CLIs — no commits)

# Tooling and Host-Contract Audit — 2026-10-02

The scope was delta-first against the 2026-09-29 report. Since then, these commits touched tooling `Paths:`:

- `2e95f0bbf`: #5143, #5144 (durable writers)
- `d6b495f2a`: #5145 (bounded TES4 probe)
- `aad5dfd75`: #5016 (Windows-shaped `installdir`)
- `bae84e54b`: `engine.quit`
- `e60911864`: `loadscreen.census`
- Four `main.rs` edits that add resources or the upscaler-persistence pin (#5030)

Dims 3 and 6 had no behavioural change and were spot-checked only. No sub-agents were used. Each dimension's notes were written to `/tmp/audit/tooling/dim_<N>.md` before the next dimension started, and this report was reconciled against them.

## Test baseline

`cargo test -j 6` (rustc 1.96.0) over the tooling crates: all green, 0 failures.

| Crate | Tests | Δ vs 09-29 |
|---|---|---|
| `byroredux-sdk` | 104 | = |
| `byroredux-debug-server` | 20 | = |
| `byroredux-debug-protocol` | 7 | = |
| `byroredux-debug-ui` | 19 | = |
| `byroredux-boot-request` | 18 | +1 |
| `byroredux-settings-io` | 13 | +4 |
| `byroredux-game-detect` | 52 | +3 |
| `byro-launcher` | 24 + 1 ignored (needs Vulkan) | = |
| `byro-detect` | 0 | = |
| `byro-texture-upscale` | 18 | = |
| `byro-dbg` | 2 | = |

The engine, the smoke scripts and every GPU process were left unlaunched.

## Executive Summary

There are **7 findings: 0 CRITICAL, 0 HIGH, 1 MEDIUM, 6 LOW.** Six are NEW. The seventh is an unfixed residual of #4752, which was closed while the residual was still open.

- **The MEDIUM is a lost update of settings between the launcher and the engine (TOOL-D4-01).** The launcher loads `settings.toml` once, when the launcher starts. Every later launcher Settings change writes back its whole stale registry. A field-of-view, sensitivity or upscaler change made in-game is therefore reverted by the next launcher Settings click. The same happens in reverse while both processes are running.
- **#5143's fix left two weaknesses (TOOL-D4-02, TOOL-D4-03).** First, `atomic_write` still leaves its temp file behind when a step fails. Settings temps now get unique names, so each failed save leaves a new hidden partial file behind, where the old fixed name was simply overwritten. Second, the "pinned statically" guards are partly vacuous: the needle each one checks for also appears in the test's own source.
- **#4752 was closed with its own residual still open (TOOL-D1-01).** The interior and exterior cell-load debug requests still pass client-chosen absolute ESM and archive paths through unconfined. Only `LoadNif` got the root rule.
- **The rest are LOW:**
  - `profiles.toml` loses its comments on every Play (TOOL-D5-01).
  - `byro-detect` silently ignores unknown flags (TOOL-D5-02).
  - The new `loadscreen.census` line contains a 14-space whitespace run (TOOL-D2-01).

Verified fixed since baseline: #5142, #5143, #5144, #5145 and #5016. The #5145 probe has a large safety margin: the largest TES4 payload on disk is 31.6 KB (Starfield.esm), against a 1 MiB probe.

## Prior-finding status (2026-09-29 report)

| Issue | Finding | State at HEAD |
|---|---|---|
| #5140 | byro-dbg 10 s vs server 30 s | Open, unchanged (`tools/byro-dbg/src/main.rs:164,191`) |
| #5141 | pause menu stops the debug drain | Open, unchanged |
| #5142 | harness breakage from `63c0aee3b` | Closed, fixed |
| #5143 | "Windows rename" clobber fallback | Closed, fixed in `2e95f0bbf`. Residuals: TOOL-D4-02 and TOOL-D4-03 |
| #5144 | unparseable settings file erased | Closed, fixed: refuses, copies the file aside to `.bad`, and refuses newer versions |
| #5145 | `esm_opens` full-file read | Closed, fixed (1 MiB `Read::take`) |
| #5147 | `create_new` default-name collisions | Open, unchanged |
| #5149 | no loopback pin | Open, unchanged (no test asserts `127.0.0.1`) |
| #5150 | debug-cli.md residual drift | Open, unchanged. One more instance belongs in it: the session example at `docs/engine/debug-cli.md:1284` (`screenshot /tmp/debug_frame.png` → "Screenshot saved") shows an absolute path that `write_screenshot` now rejects. Add it to #5150 rather than filing it separately |
| #4752 / D1-05 | cell-load absolute paths | #4752 closed, residual **unfixed**. See TOOL-D1-01 |

## Findings

### TOOL-D4-2026-10-02-01: The launcher writes back a settings snapshot taken when the launcher started, reverting settings changed in-game
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

### TOOL-D4-2026-10-02-02: `atomic_write` leaves its temp on failure, and #5143's unique temp names turn that into accumulation
- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: any user whose config disk fails a write (disk full, EIO)
- **Location**: `crates/core/src/atomic_file.rs:19-25` (`atomic_temp_path`), `:55-84` (`atomic_write`); callers `crates/settings-io/src/lib.rs:243-259`, `crates/boot-request/src/lib.rs:307`, `crates/game-detect/src/overrides.rs:133-137`
- **Status**: NEW. It was introduced by the #5143 fix (`2e95f0bbf`).
- **Description**: `atomic_write` removes the temp only in its read-back-mismatch arm. If create, `write_all`, `flush`, `sync_all` or `rename` fails, the temp is left behind holding whatever bytes reached it. Before `2e95f0bbf`, two things limited the damage:
  - settings-io staged through the fixed name `settings.toml.tmp`, which the next attempt overwrote.
  - The (now removed) fallback branch deleted the temp.

  Now every writer uses `.{name}.{pid}.{counter}.tmp`, and the callers do not clean up. Each failed save leaves one more hidden file.
- **Evidence**: `atomic_file.rs:62-66` uses `?`-propagation on every step before the read-back. The only `remove_file` is at `:72`, inside the mismatch branch. The old helper is visible in `git show 2e95f0bbf^:crates/settings-io/src/lib.rs` (`temporary_path`, `:274-278`).
- **Impact**: This happens on the disk-full path #5143 was written for. Every settings change in the in-game menu saves, and each failed save leaves another partial `.settings.toml.<pid>.<n>.tmp` in the config directory. That consumes space on a disk that is already full. There is no data loss.
- **Related**: #5143, TOOL-D4-03
- **Suggested Fix**: In `atomic_write`, best-effort `remove_file(tmp_path)` on every error before the rename returns, and after a failed rename. A guard test can inject the failure with a temp path under a missing directory.

### TOOL-D4-2026-10-02-03: The #5143 "pinned statically" guards are vacuous on their positive half, and boot-request has no negative pin
- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: developer only (regression-guard quality)
- **Location**: `crates/settings-io/src/lib.rs:668-680` (`save_has_no_nonatomic_clobber_fallback`); `crates/game-detect/src/overrides.rs:159-172` (`merge_uses_the_shared_atomic_file_writer`); `crates/boot-request/src/lib.rs:475-477` (`save_uses_the_shared_atomic_file_writer`)
- **Status**: NEW (test-gap)
- **Description**: The tests scan their own file with whole-file `include_str!`. That is the #4604/#4842 pattern `_audit-common.md` warns about: the scan should use `production_text` instead.
  - The positive needles, `"atomic_file::atomic_write"` and `"byroredux_core::atomic_file::atomic_write"`, appear verbatim inside each test's own `assert!`. Deleting the production call does not fail the test.
  - The negative needle is assembled at run time, but it is only the old binding name `rename_error`. A fallback reintroduced as `Err(e) if path.exists() => fs::write(path, fs::read(&temp_path)?)` passes.
  - The boot-request guard is entirely the vacuous positive half. It has no negative check, although `2e95f0bbf` says the removal is pinned in each writer.
- **Evidence**: `boot-request/src/lib.rs:476`: `assert!(include_str!("lib.rs").contains("byroredux_core::atomic_file::atomic_write"));`. The needle is the literal on that same line.
- **Impact**: The three guards cannot catch the regression they were written for: the clobber fallback coming back.
- **Related**: #5143, #4604, TOOL-D4-02
- **Suggested Fix**: Strip `#[cfg(test)]` blocks before scanning, using the `source_scan::production_text` pattern. Make the negative check structural, for example "no `fs::write` whose argument reads `temp_path`", and add it to boot-request.

### TOOL-D1-2026-10-02-01: The cell-load debug requests still open client-chosen absolute ESM and archive paths after #4752 closed (residual of TOOL-D1-2026-09-29-05)
- **Severity**: LOW
- **Dimension**: Debug Trust Boundary
- **Exposure**: any local process on a debug build, or on a release build with `BYRO_DEBUG_SERVER=1`. It is read-only and same-user.
- **Location**: `crates/debug-server/src/evaluator.rs:91-130`; `byroredux/src/debug_load.rs:52-99` (dispatch), `:293+` (`exec_load_interior` → `cell_loader::load_cell_with_masters`), and the exterior twin
- **Status**: NEW. This is the unfixed residual of closed #4752. The 09-29 report recorded it as D1-05 under #4752 and did not file it separately, so it was lost when #4752 closed.
- **Description**: `LoadInteriorCell` and `LoadExteriorCell` copy the client's `esm`, `masters`, `bsas` and `textures_bsas` strings into `PendingDebugLoad`. They are then opened as given. Only `LoadNif` goes through `resolve_nif_bytes`' rule: relative paths, confined under `allowed_roots` derived from the startup `--esm`/`--bsa` args (`debug_load.rs:151-185`). The #4752 close comment says `63c0aee3b` "covers all three independent paths". The cell-load paths are a fourth.
- **Evidence**: `evaluator.rs:98-104` builds `PendingDebugLoad::InteriorCell { esm: esm.clone(), masters: masters.clone(), bsas: bsas.clone(), … }` with no path check. `debug_load.rs` passes `DebugLoadSource { esm: &esm, … }` straight to the loader.
- **Impact**: Any local process can point the ESM, BSA and BA2 parsers (untrusted-input code) at any file the user can read. It is read-only, but it keeps a parser attack surface reachable without authentication on every debug build.
- **Related**: #4752, TOOL-D1-2026-09-29-05, #5009 (same file, `resolve_nif_bytes` archive order)
- **Suggested Fix**: Apply `resolve_nif_bytes`' root rule to every cell-load path: relative only, canonicalized under the startup roots. Alternatively, restrict cell loads to the startup archive set, as `tex.dump` does.

### TOOL-D5-2026-10-02-01: Every Play rewrites `~/.byroredux/profiles.toml` through `toml::Table`, dropping the user's comments and formatting
- **Severity**: LOW
- **Dimension**: Install Detection
- **Exposure**: every launcher user with a hand-curated profiles file
- **Location**: `tools/byro-launcher/src/app.rs:79` (`self.state.remember()` before each Play); `tools/byro-launcher/src/state.rs:156-158`; `crates/game-detect/src/overrides.rs:88-121`
- **Status**: NEW. It is on the skill checklist but was never filed.
- **Description**: `merge_into_file` parses the file into a `toml::Table`, replaces `[roots]`, and serializes it again. Values are preserved. Comments, key order and formatting in the user's `[profiles.*]` and `[defaults]` blocks are lost. The launcher does this on every Play, not only after detection changes. The doc comment says those blocks "survive verbatim — including comments' absence being the only casualty", which contradicts itself.
- **Evidence**: `overrides.rs:98` `let mut document: toml::Table = … toml::from_str(&text)`, and `:121` `document.insert(ROOTS_TABLE…)`, followed by re-serialization.
- **Impact**: Users lose their annotations without any message. Severity is LOW because no value is lost.
- **Related**: TOOL-D4-01 (another write-on-every-action), #4758
- **Suggested Fix**: Edit only `[roots]` with `toml_edit::DocumentMut`, which is already in `Cargo.lock` transitively. Skip the write when `[roots]` is unchanged, and correct the doc comment.

### TOOL-D5-2026-10-02-02: `byro-detect` ignores unknown flags, so a mistyped `--profiles` sends `--write` to the default file
- **Severity**: LOW
- **Dimension**: Install Detection
- **Exposure**: CLI users of `byro-detect`
- **Location**: `tools/byro-detect/src/main.rs:26-34`, `:131-136` (`value_of`), `:147-158` (`print_usage`)
- **Status**: NEW. It is on the skill checklist but was never filed.
- **Description**: Flags are parsed with `args.iter().any(..)` and a positional `value_of`.
  - `byro-detect --write --profile /tmp/p.toml` (singular) ignores `--profile` and merges into the real `~/.byroredux/profiles.toml`. That hits TOOL-D5-01's comment loss too.
  - `--profiles --write` takes `"--write"` as the path, and the write is off.

  `--help` documents neither behaviour.
- **Evidence**: `value_of` returns `args.get(index + 1)` without checking that the value is not itself a flag. No branch rejects an unrecognised argument.
- **Impact**: A typo silently changes which file gets rewritten.
- **Related**: TOOL-D5-01
- **Suggested Fix**: Reject unknown arguments, and values that start with `--`, with a usage error and a non-zero exit.

### TOOL-D2-2026-10-02-01: The `loadscreen.census` output line has a 14-space whitespace run in the middle (a lost line continuation)
- **Severity**: LOW
- **Dimension**: Protocol & Registry
- **Exposure**: console and byro-dbg users; the p6 smoke greps this line
- **Location**: `byroredux/src/commands/world_info.rs:89-140` (`e60911864`); the same commit at `:146` (`EntitiesCommand::description`)
- **Status**: NEW
- **Description**: The `format!` string reads `"…locations={},              malformed={}…"`. A multi-line literal lost its trailing `\`, so the indentation became part of the output. The same commit also left `"Show entity count and component breakdown"    }` on one line. Neither change went through rustfmt.
- **Evidence**: `git show e60911864 -- byroredux/src/commands/world_info.rs`
- **Impact**: The console output is cosmetically broken. `docs/smoke-tests/p6-loading-model.sh:126` greps only `eligible=[1-9]`, so the smoke is unaffected today.
- **Related**: none
- **Suggested Fix**: Restore the `\` continuation and run `cargo fmt` on the file.

## Dimension notes

- **Dim 1**: re-verified the loopback bind (`listener.rs:179`) and every cap: 64 queued commands, 8 clients, 300 s read timeout, 30 s response timeout, 16 MB frames enforced on both encode and decode, walk depth 64.
  - The new `engine.quit` is reachable through the same unauthenticated console port as every other command. That is an exposure-matrix note, not a new class of finding.
- **Dim 2**: 49 registered components and 99 registered commands, both equal to `debug-cli.md`. `every_registered_command_is_named_in_debug_cli_docs` is green.
  - New components since baseline: `LoadingModelStage`, `PendingGearRelease`, `AmbientEngagement`. All are transient and none is inspect-derived, so the registry has no gap.
  - No new `DebugRequest`/`DebugResponse` variant.
- **Dim 3**: the SDK dependency tree is unchanged: core (`math::Vec3` only), semver, serde, thiserror. The `compatibility/tests.rs` churn is clippy's module-inception unwrap, and the test count is unchanged at 104.
- **Dim 5**:
  - `for_data_dir` is the only release selector (validate, boot/cli, studio_host, evaluator).
  - The legacy Skyrim env var survives only at `.github/workflows/playable-smoke.yml:45` (open #4760).
  - The `DEFAULT_GAMES_ROOT` developer path is announced with remedies when `--game` misses (`boot/cli.rs:376-384`), so it is not a finding.
- **Dim 6**: unchanged. 18 tests, and the #4761/#4762 guards are present.

## Exposure Matrix

| Surface | Reachable by | Authenticated | Size-capped | Persistence atomic |
|---|---|---|---|---|
| Debug TCP port (console, `SetField`, `engine.quit`) | any local process; debug builds by default, release with `BYRO_DEBUG_SERVER=1` | No | Yes: 16 MB frames, 64 queued, 8 clients, walk depth 64 | n/a |
| Debug cell loads (`LoadInteriorCell`/`LoadExteriorCell`) | same | No | No path confinement (TOOL-D1-01) | n/a (read-only) |
| Screenshot / `tex.dump` | same | No | Bare filename under `screenshots/` / `texture-dumps/`, `create_new` | n/a |
| `settings.toml` | launcher + engine + extensions | n/a | n/a | Yes (`atomic_write`), but writes back a stale whole registry (TOOL-D4-01) and orphans temps on failure (TOOL-D4-02) |
| `boot.toml` | launcher → engine | n/a | n/a | Yes; the contract version is refused on mismatch |
| `profiles.toml` | launcher (every Play), `byro-detect --write` | n/a | n/a | Yes, but comments are dropped (TOOL-D5-01) |
| texture-upscale manifest | local operator | Trusted executable input (documented) | Disk space checked first | Output collisions rejected (#4761) |

## Cross-audit routing

- Debug-server lock order: `/audit-concurrency` Dim 7. Sandbox capability enforcement: `/audit-safety` Dim 8. Neither was re-audited here.
- #5009 (debug-load NIF resolver archive order) is owned by `/audit-parsers`. It is cross-referenced from TOOL-D1-01.

## Next step

`/audit-publish docs/audits/AUDIT_TOOLING_2026-10-02.md`. Use `tech-debt` for the debug-server, launcher and tools findings, since those areas have no label of their own, and add `test-gap` on TOOL-D4-03. The #5150 addendum (the `debug-cli.md:1284` example) is a comment on the existing issue, not a new one.
