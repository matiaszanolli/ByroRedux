**HEAD**: 9fcfdc3fc · **Baseline**: AUDIT_TOOLING_2026-09-22.md (HEAD ee6d3fb39) · **Audited**: Dim 1 (Debug Trust Boundary), Dim 2 (Protocol & Registry), Dim 3 (SDK Contract), Dim 4 (Boot Handoff & Persistence), Dim 5 (Install Detection), Dim 6 (Tool CLIs) · **Unchanged since baseline (skimmed)**: none. Every dimension has commits since 2026-09-22, most of them from `63c0aee3b`. Dims 3 and 6 came back clean.

# Tooling and Host-Contract Audit — 2026-09-29

The scope was delta-first against the 2026-09-22 report. Every dimension's `Paths:` had commits, so all six were audited. The largest change is `63c0aee3b` (2026-09-27, titled "Refactor environment variable usage for Skyrim SE data path"). It fixed eight of the nine open 2026-09-22 tooling findings without citing their issue numbers, so none of them closed. See **Prior-finding status**.

No sub-agents were used. Each dimension's findings were written to `/tmp/audit/tooling/dim_<N>.md` before the next dimension started, and this report was reconciled against all six files.

## Test baseline

`cargo test -j 6` over the tooling crates: everything is green, 0 failures.

| Crate | Tests | Δ vs 09-22 |
|---|---|---|
| `byroredux-sdk` | 104 | = |
| `byroredux-debug-server` | 20 | +4 |
| `byroredux-debug-protocol` | 7 | +1 |
| `byroredux-debug-ui` | 19 | +3 |
| `byroredux-boot-request` | 17 | +1 |
| `byroredux-settings-io` | 9 | +1 |
| `byroredux-game-detect` | 49 | +4 |
| `byro-launcher` | 24 + 1 ignored (needs Vulkan) | = |
| `byro-detect` | 0 | = |
| `byro-texture-upscale` | 18 (lib) | +3 |
| `byro-dbg` | 2 | +2 (was 0) |

The engine binary, the smoke scripts and every GPU process were deliberately left unlaunched, per the suite constraints.

## Executive Summary

There are **10 findings: 0 CRITICAL, 0 HIGH, 6 MEDIUM, 4 LOW.** Nine are NEW and one is a residual of an open issue (#4752).

Three of the MEDIUM findings come from the fixes `63c0aee3b` shipped for the 2026-09-22 findings:

- **The #4754 fix gave byro-dbg a 10 s timeout** (TOOL-D1-01). One day earlier the server's response wait had been raised to 30 s, so a response that takes 10–30 s now closes the REPL and the TUI.
- **The #4758 fix moved the three config writers onto `atomic_write`** (TOOL-D4-01), but each copy kept a "Windows rename" fallback that runs on any error on every OS. On disk-full, that fallback overwrites the user's good file with a partial temp file.
- **The #4759 fix checks the ESM header by reading the whole ESM** (TOOL-D5-01). That is 1.46 GB for Starfield, read on the launcher's UI thread at startup and on every Rescan.

The fourth MEDIUM found in the fixes is harness fallout. The sibling reports filed m-exteriors, m34 and m-trees as broken by `63c0aee3b`'s screenshot-path rule and release opt-in (EXT-D7-2026-09-29-01, SPT-D3-01). Eight more harnesses and three developer docs are broken the same way, and no report had filed them (TOOL-D1-03).

Two MEDIUM findings are older gaps the baseline missed:

- **The native pause menu stops the debug drain.** The pause menu skips `scheduler.run`, so the debug port stops answering. Commands the client already gave up on then run when the menu closes (TOOL-D1-02).
- **A settings file that fails to parse is silently discarded on the next save.** That takes the player's input bindings with it (TOOL-D4-02).

Areas swept:
- the debug-server trust boundary: the release opt-in, path confinement, timeouts, and pause/drain liveness;
- every smoke/eval harness that drives byro-dbg;
- registry and console-doc completeness, and wire compatibility for the new `Metrics` fields;
- the SDK dependency surface, capability constants, determinism, and the new dialogue panel;
- atomic-write adoption and its fallback, and settings merge semantics;
- ESM pre-flight validation and the env-var unification;
- texture-upscale collision and per-set failure fixes.

## Prior-finding status (2026-09-22 → HEAD)

`63c0aee3b` carries no `Fix #N` keywords, so the issues below are still OPEN even though the code fixes them. I checked each one against HEAD. The *perf_fix_status_unclosed_issues* memory records the same pattern elsewhere.

| Issue | Finding | State at HEAD | Recommended action |
|---|---|---|---|
| #4752 | debug port file read/write | **Mostly fixed.** Release needs `BYRO_DEBUG_SERVER=1` (`main.rs:78-80,1022`, test `release_debug_server_requires_explicit_opt_in`). Screenshot is confined to `screenshots/` + `create_new`. `tex.dump` is confined to `texture-dumps/` + configured archives. `LoadNif` is relative-only under the configured roots. **Residual:** TOOL-D1-05 | Add D1-05, then close |
| #4753 | encode-side cap / walk depth | Fixed. `encode` enforces `MAX_MESSAGE_SIZE`, `MAX_WALK_DEPTH` 64 + visited set, and the `InvalidData` fallback sits in `handle_client` | Close |
| #4754 | byro-dbg timeout + zero tests | Fixed: 10 s timeout, 2 tests. **It introduced TOOL-D1-01** | Close with a pointer to D1-01 |
| #4755 | unregistered gameplay components | Fixed. All 7 are registered, guarded by `requested_gameplay_components_are_registered` | Close |
| #4756 | debug-cli.md counts / 15 commands | Fixed: 49 components and 97 commands, every name documented, and `debug_cli_component_counts_match_the_registry` guards it. The residual drift is TOOL-D2-01 | Close |
| #4757 | `UI.IsMenuOpen` | Partial. Owned by `/audit-ui` as UI-D1-2026-09-29-01 | Keep open |
| #4758 | bare `fs::write` writers | Fixed (`atomic_write`). **The retained fallback is TOOL-D4-01** | Close with a pointer to D4-01 |
| #4759 | ESM not validated | Fixed: `esm_opens` plus the test `a_present_but_truncated_main_plugin_fails_validation`. **It introduced TOOL-D5-01** | Close with a pointer to D5-01 |
| #4760 | `BYROREDUX_SKYRIM_DATA` readers | Fixed. The only remaining reference is the deliberate repo-variable fallback at `.github/workflows/playable-smoke.yml:45` | Close, or keep for the CI var only |
| #4761 / #4762 | texture-upscale | Closed by `4286e55ba`. Verified in Dim 6 | — |

## Findings

### TOOL-D1-2026-09-29-01: byro-dbg's new 10 s read timeout is shorter than the server's 30 s response timeout, so 10–30 s commands close the REPL and TUI
- **Severity**: MEDIUM
- **Dimension**: Debug Trust Boundary
- **Exposure**: developers and every byro-dbg smoke harness
- **Location**:
  - `tools/byro-dbg/src/main.rs:163-165` (`configure_read_timeout`, applied before both REPL and TUI) and `:95-101` (`Receive error` → `break`)
  - `tools/byro-dbg/src/tui.rs:408-420` (`Err(_) => break` → "engine disconnected", quit)
  - `crates/debug-server/src/listener.rs:17-22` (`COMMAND_RESPONSE_TIMEOUT` = 30 s)
- **Status**: NEW. The #4754 fix in `63c0aee3b` introduced it.
- **Description**:
  - `dd99cd0f3` (2026-09-26) raised the server's wait from 5 s to 30 s. Its comment says why: "The previous five-second limit dropped the initial water diagnostics on FO3's exterior fixture."
  - The next day, `63c0aee3b` gave the client a fixed 10 s socket read timeout.
  - Any response that arrives after 10 s but within 30 s now fails in the client with `WouldBlock`/`TimedOut`. The REPL prints `Receive error` and exits, and every remaining line of a piped heredoc is dropped. The engine still runs the command, then logs a write error to the closed socket.
  - The TUI's net thread breaks, and the dashboard quits with "engine disconnected".
  - The TUI can trigger this itself. A Loader-tab cell load runs synchronously in `step_debug_loads` between frames, so the next `Metrics` poll waits out the whole load.
- **Evidence**: `stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))` (byro-dbg) against `const COMMAND_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);` (server). Neither side references the other.
- **Impact**:
  - The same cold-stall class `dd99cd0f3` fixed is broken again on the client side: first-command-after-bench-hold, exterior water diagnostics, and cell loads.
  - Smoke heredocs end in `|| true`, so the truncation is silent. Later gates read missing output as FAIL, or as a parsed `0`.
- **Related**: #4754, `dd99cd0f3`, #1007. TOOL-D1-02 makes it certain while the game is paused.
- **Suggested Fix**: Put one timeout constant in `debug-protocol` and set the client read timeout above the server's response timeout (e.g. 35 s). On a timeout, the TUI should report "engine busy" instead of quitting.

### TOOL-D1-2026-09-29-02: the native pause menu stops the debug-server drain; timed-out commands then run in a burst when the game resumes
- **Severity**: MEDIUM
- **Dimension**: Debug Trust Boundary
- **Exposure**: developers (byro-dbg while the game is paused), and any smoke that opens a native menu page
- **Location**:
  - `byroredux/src/app_events.rs:688-697`, `:919-922` (`if !simulation_paused { self.scheduler.run(&self.world, dt); }`)
  - `crates/debug-ui/src/lib.rs:299` (`simulation_paused`)
  - `crates/debug-server/src/lib.rs:28-38` (`DebugDrainSystem` is a `Stage::Late` exclusive inside that scheduler)
  - `crates/debug-server/src/system.rs:136-190` (drain loop; only the screenshot path checks `cancel`)
- **Status**: NEW. The gating has existed since `5d47f0735` (2026-08-15), and the 09-22 baseline missed it.
- **Description**:
  - While the Pause, Settings or Inventory page is open, `about_to_wait` skips `scheduler.run` entirely, so the drain system never runs.
  - The listener still accepts and queues requests, up to 64. Each client gets "timeout waiting for engine response" after 30 s, or its REPL exits after 10 s (TOOL-D1-01).
  - When the menu closes, the drain runs every queued command at once. Only screenshots honour the abandonment flag.
  - A `SetField`, `cell.load`, `quest.setstage` or `inv.add` that the user retried after a timeout therefore fires once per attempt, long after the client was told it failed.
  - `766e1746e` deliberately kept the dialogue page out of `simulation_paused` for this reason: "a running world keeps the debug-server drain alive so a route smoke can assert on the presented response".
- **Evidence**: `simulation_paused = game_menu.visible && page != Dialogue` → `if !simulation_paused { self.scheduler.run(..) }`. `evaluate(world, &self.registry, &cmd.request)` runs for every drained command, with no `cmd.cancel` check.
- **Impact**:
  - The pause menu is the most natural moment to inspect the world, and the debugger is dead exactly then.
  - Duplicate, delayed mutations corrupt the state the developer is trying to debug.
- **Related**: TOOL-D1-01, #1007 (cancel flag, screenshot-only), `766e1746e`.
- **Suggested Fix**: Run the debug drain even while the simulation is paused, for example by running `DebugDrainSystem` (or a paused-mode drain) outside the gated `scheduler.run`. Separately, skip any drained command whose `cancel` flag is already set before calling `evaluate`.

### TOOL-D1-2026-09-29-03: Eight more byro-dbg harnesses and three developer docs broke on `63c0aee3b`'s screenshot rule and release opt-in (siblings of EXT-D7-2026-09-29-01 / SPT-D3-01)
- **Severity**: MEDIUM
- **Dimension**: Debug Trust Boundary (harness fallout)
- **Exposure**: developers and CI-adjacent acceptance gates (all manual)
- **Location**:
  - **Absolute screenshot path.** The server rejects it ("screenshot path must be a filename inside screenshots/"):
    - `docs/smoke-tests/m48-4-oblivion-hud.sh:81,84-86`
    - `docs/smoke-tests/m48-5-fnv-hud.sh:104-108`: edited today in `db8351587`, and still broken
    - `docs/smoke-tests/m48-5-fo3-hud.sh:86-90`
    - `docs/smoke-tests/m48-6-skyrim-hud.sh:118,122`
    - `docs/smoke-tests/m48-7-fo4-hud.sh:112,116`
    - `scripts/material-provider-matrix.sh:222`: the `:263` "screenshot missing" check then hard-fails
  - **Release engine without `BYRO_DEBUG_SERVER=1`.** byro-dbg cannot attach:
    - `docs/smoke-tests/m43-quest-runtime.sh:54` (byro-dbg at `:81`)
    - `docs/smoke-tests/m47-triggers.sh:169,306` (byro-dbg at `:204`)
    - `docs/smoke-tests/r6a_stale_15_bench.sh:147`
    - `scripts/material-provider-matrix.sh:151-158`
  - **Docs that teach "release `--bench-hold` → attach byro-dbg" with no opt-in:**
    - `CLAUDE.md:337,341`
    - `README.md:251`: `cargo run --release -- --studio --bench-hold`, "Scriptable over byro-dbg"
    - `docs/engine/testing.md:206-208`
- **Status**: NEW. These are siblings of EXT-D7-2026-09-29-01 (m-exteriors, m34, `docs/smoke-tests/README.md`) and SPT-D3-01 (m-trees), which are not re-filed here. The EXT report routed m43/m47 to `/audit-scripting` and m48-* to `/audit-ui`. No scripting report ran today, and `AUDIT_UI_2026-09-29.md` does not file them, so they are filed here.
- **Description**: `63c0aee3b` (the #4752 fix) made two changes. `write_screenshot` now accepts only a bare filename. `debug_server_allowed(false, None)` is now false in release builds. The fixed harnesses use one of two patterns:
  - bare-name capture, then `mv "$SMOKE_DATA/screenshots/$name"` (`p3-hud.sh:144-150`, `p3-player-body.sh:151-161`);
  - `env … BYRO_DEBUG_SERVER=1` (the p0–p5, w1, m41 and m48-menu-load harnesses).
  - The five HUD smokes do export `BYRO_DEBUG_SERVER=1`, which their debug binaries do not even need. They still pass `$OUT_DIR/*.png` from `mktemp -d`, an absolute `/tmp` path, so every capture is refused. The pixel gates then find no PNG.
  - `r6a_stale_15_bench.sh` is also broken independently. It runs `cd "$game_data" && cargo run --release`, which is outside the workspace, so there is no `Cargo.toml`. It also passes `byro-dbg -p "$PORT"`, and byro-dbg ignores every argument except `--tui`.
- **Evidence**:
  - `write_screenshot` (`crates/debug-server/src/system.rs:202-229`) rejects any path with more than one component. Its test is `rejects_absolute_and_parent_paths`.
  - The m48 scripts send `screenshot $OUT_DIR/full.png`.
  - `grep -c BYRO_DEBUG_SERVER` returns 0 for m43, m47, r6a and material-provider-matrix, and each of them launches `--release`.
- **Impact**:
  - The M48.4–M48.7 HUD pixel gates (Oblivion, FO3, FNV, Skyrim, FO4) are permanently red.
  - So are the M43 quest-runtime and M47 trigger gates, and the R5.5 material-provider capture matrix.
  - A developer who follows CLAUDE.md's usage line gets a byro-dbg "Failed to connect" and no hint why.
- **Related**: EXT-D7-2026-09-29-01, SPT-D3-01, #4752, #4724 (HUD smokes `exit 1` on missing data)
- **Suggested Fix**:
  - Use the `p3-hud.sh` capture pattern in the six screenshot sites.
  - Add `BYRO_DEBUG_SERVER=1` to the four release launches.
  - Add the opt-in to the CLAUDE.md, README.md and testing.md examples.
  - Add the static guard EXT-D7-01 proposes. It would scan `docs/smoke-tests/*.sh` and `scripts/*.sh` for `screenshot <path containing />` and for a release byro-dbg harness that lacks the opt-in.

### TOOL-D4-2026-09-29-01: The shared "Windows rename" fallback runs on any `atomic_write` error on every OS and overwrites the good file with the temp's possibly-partial bytes
- **Severity**: MEDIUM
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: players and launcher users. Their files are user-edited `~/.byroredux/profiles.toml`, `settings.toml` (input bindings), and the per-Play `boot.toml`.
- **Location**:
  - `crates/boot-request/src/lib.rs:307-330`
  - `crates/game-detect/src/overrides.rs:133-156`
  - `crates/settings-io/src/lib.rs:206-218`
  - `crates/core/src/atomic_file.rs:38-70`
- **Status**: NEW. It came in with the #4758 fix. #4758 is related, and #3472 introduced the original settings-io copy.
- **Description**:
  - `atomic_write` returns `Err` from any of these steps: create, `write_all`, `sync_all`, read-back, `rename`, or the parent-directory fsync.
  - All three callers handle it the same way: `Err(_) if path.exists() => fs::write(path, fs::read(&temp)?)`. That branch has no `cfg(windows)` and no check of which step failed. The comment calls it Windows-only; the code is not.
  - Concrete cases:
    1. **Disk full.** `write_all` fails with a partial temp on disk. `fs::read(&temp)` succeeds, and `fs::write(path, …)` truncates the user's good file and writes the partial bytes. The next settings load fails to parse, and TOOL-D4-02 then drops everything. A `profiles.toml` loses its hand-authored `[profiles.*]` blocks.
    2. **EIO on `sync_all`.** The unsynced bytes are copied over the destination non-atomically. That is exactly what `atomic_write` exists to prevent.
    3. **Parent-directory fsync fails after a successful rename.** The temp is gone, so the fallback's `fs::read` returns NotFound, and a write that completed is reported as failed.
  - The fallback's premise is also wrong. Rust's `std::fs::rename` on Windows already replaces an existing destination (`MOVEFILE_REPLACE_EXISTING`, or `FILE_RENAME_FLAG_REPLACE_IF_EXISTS`). Where rename does fail there (a sharing violation), the fallback's `fs::write` would usually fail too.
  - The fallback and `atomic_temp_path` are copied verbatim three times instead of living in `core::atomic_file`.
- **Evidence**:
  ```rust
  // crates/boot-request/src/lib.rs:308-313 (same shape in overrides.rs:134-137, settings-io:206-212)
  match byroredux_core::atomic_file::atomic_write(path, &temp_path, text.as_bytes()) {
      Ok(()) => Ok(()),
      Err(rename_error) if path.exists() => {
          // Windows does not replace an existing destination with rename.
          fs::write(path, fs::read(&temp_path)...?)
  ```
- **Impact**: The durability contract #3472/#4758 set out to establish is voided on the one failure (disk full) it most needs to survive. Data at risk: user-edited profiles and bindings (MEDIUM, per the skill's Exposure guidance). `boot.toml` alone would be LOW.
- **Related**: #4758, #3472, TOOL-D4-02. The save ring (`crates/save/src/disk.rs`) uses `atomic_write` without this fallback.
- **Suggested Fix**: Delete the fallback. If a Windows sharing-violation retry is really wanted, move it into `atomic_file` and gate it to `cfg(windows)` and an error from the `rename` step only. Keep one shared `atomic_temp_path`.

### TOOL-D4-2026-09-29-02: `settings-io` silently discards the stored map when the existing file fails to parse, so the next save erases input bindings
- **Severity**: MEDIUM
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: players and launcher users. It can also be triggered by an extension's settings write, with no user action.
- **Location**:
  - `crates/settings-io/src/lib.rs:170-175` (`save_to_path`: `.unwrap_or_default()`, `Err(_) => BTreeMap::new()`)
  - contract doc `:151-164`
  - save triggers: `tools/byro-launcher/src/settings_screen.rs:55-57`, `byroredux/src/extensions/systems.rs:451-455`, `byroredux/src/main.rs:875,1591`
- **Status**: NEW. It is on the skill checklist but was not filed at baseline.
- **Description**:
  - `save_to_path` is documented as "**preserving stored keys it does not know** … required by the launcher", because the launcher's registry has no input bindings.
  - That contract only holds when the existing file parses. For a hand-edit typo, a torn file (TOOL-D4-01) or a future `SETTINGS_VERSION` with a changed shape, `toml::from_str` fails. `unwrap_or_default()` then substitutes an empty map, and the file is rewritten with only the saving registry's keys.
  - Nothing is logged at save time. `load` warned once at startup, but that says the file was skipped, not that it is about to be erased.
  - A parseable newer-version file is also silently re-stamped `version = 1`: `load` logs, `save` does not.
- **Evidence**:
  ```rust
  Ok(existing) => toml::from_str::<StoredSettings>(&existing)
      .map(|stored| stored.settings)
      .unwrap_or_default(),
  Err(_) => BTreeMap::new(),
  ```
- **Impact**:
  - Opening the launcher's settings screen and pressing Save with a malformed `settings.toml` destroys every key rebinding irreversibly.
  - An extension writing a setting (`systems.rs:451-455`) does the same inside the engine, with no user action.
- **Related**: TOOL-D4-01, #3472, #4974
- **Suggested Fix**: When the existing file does not parse, refuse to save with a warning, or first copy it aside (`settings.toml.bad`) and log that. Treat a read error other than NotFound the same way.

### TOOL-D5-2026-09-29-01: `esm_opens` reads the entire ESM to check a 24-byte header, so the launcher's startup and Rescan read about 3.2 GB on the UI thread
- **Severity**: MEDIUM
- **Dimension**: Install Detection
- **Exposure**: every launcher user, at startup and on Rescan; `byro-detect`
- **Location**:
  - `crates/game-detect/src/validate.rs:117-123` (`std::fs::read(path)` → `EsmReader::read_file_header`), `:166`
  - callers: `tools/byro-launcher/src/state.rs:72-75` (`LauncherState::load`), `:85-91` (`refresh`, from the egui `update` closure at `app.rs:160-162`), `tools/byro-detect/src/main.rs:63`
- **Status**: NEW. The #4759 fix (`63c0aee3b`) introduced it.
- **Description**:
  - `validate()` runs once per detected game, and each run slurps the whole main plugin to parse its TES4 record. Installed sizes on the dev box:
    - Starfield.esm: 1,457,098,709 B
    - SeventySix.esm: 925,469,444 B
    - Fallout4.esm: 330,776,576 B
    - Skyrim.esm: 249,752,131 B
    - FalloutNV.esm: 245,650,747 B
  - The launcher therefore reads about 3.2 GB synchronously before its first frame, and again on every Rescan click, with peak RSS around 1.46 GB.
  - The full read buys nothing. The header parse cannot detect mid-file truncation, only a bad or short first record.
- **Evidence**: `let bytes = std::fs::read(path)…?; EsmReader::new(&bytes).read_file_header()`
- **Impact**:
  - A cold-cache launcher start, or a Rescan, stalls the UI for seconds on an SSD and far longer on an HDD.
  - A 1.5 GB allocation spike is a poor fit for a tool whose stated job is to run when the engine cannot.
- **Related**: #4759
- **Suggested Fix**: Read the 24-byte record header, bound the TES4 payload (e.g. ≤ 1 MiB), and parse from that prefix only.

### TOOL-D1-2026-09-29-04: `create_new` with non-unique default names: a second default `tex.dump` always fails, and two auto-named screenshots in the same second collide
- **Severity**: LOW
- **Dimension**: Debug Trust Boundary
- **Exposure**: developers
- **Location**:
  - `byroredux/src/commands/assets.rs:249-252` (default `"tex_dump.png"`), `:308-316` (`create_new`)
  - `crates/debug-server/src/system.rs:93-106` (`screenshot_<unix secs>.png`), `:219-225` (`create_new`)
- **Status**: NEW
- **Description**: `63c0aee3b` correctly switched both writers to `create_new`, so neither can overwrite a file. Their default names were not made unique, though:
  - `tex.dump <bsa> <tex>` without an `out.png` fails from the second call on, with "write '…/texture-dumps/tex_dump.png': File exists".
  - Two bare `screenshot` commands in the same wall-clock second get the same name, and the second one fails.
- **Impact**: This is a minor usability regression. The user has to clean up `texture-dumps/` or pass explicit names.
- **Suggested Fix**: Suffix the default with a counter or nanoseconds, or pick the next free `_N` name when the default already exists.

### TOOL-D1-2026-09-29-05: The two cell-load debug requests still open client-chosen absolute `esm` / `masters` / `bsas` paths (#4752 residual)
- **Severity**: LOW
- **Dimension**: Debug Trust Boundary
- **Exposure**: any local process when the debug server is on. It is on by default in debug builds, and opt-in in release.
- **Location**:
  - `crates/debug-server/src/evaluator.rs:91-125` (`LoadInteriorCell` / `LoadExteriorCell` queued verbatim)
  - `byroredux/src/debug_load.rs` (`exec_load_interior` / `exec_load_exterior` → `DebugLoadSource`)
- **Status**: Existing: #4752 (residual after `63c0aee3b`)
- **Description**:
  - `63c0aee3b` confined `LoadNif` to relative paths under the configured `--esm`/`--bsa` parents. It left `Load*Cell` alone, and those requests open and parse any ESM and archive path the engine user can read.
  - This is read and parse only; nothing is written. `debug-cli.md:16-19` says "Loose `LoadNif` paths stay within configured game data roots" and is silent on cell loads.
- **Impact**: Small, since it is same-user and read-only. It does keep an untrusted-input parser reachable from any local process on a debug build. #4752's own checklist lists "all independent paths fixed together".
- **Related**: #4752, PAR-D2-2026-09-29-02 (same file: `resolve_nif_bytes` still uses a BSA-only, first-listed-wins loop, which is tooling-owned. It is cross-referenced here, not re-filed).
- **Suggested Fix**: Apply the `resolve_nif_bytes` root rule to the cell-load paths, or restrict them to the startup archive set, before closing #4752.

### TOOL-D1-2026-09-29-06: No test pins the debug listener to loopback
- **Severity**: LOW
- **Dimension**: Debug Trust Boundary
- **Exposure**: all (a guard gap, not a live defect)
- **Location**: `crates/debug-server/src/listener.rs:179` (`TcpListener::bind(("127.0.0.1", port))`)
- **Status**: NEW (test-gap)
- **Description**: The whole trust model rests on the literal `127.0.0.1`: there is no authentication, and the path confinement assumes a local caller. No test asserts `handle.local_addr().ip().is_loopback()`. A change to `0.0.0.0` (for example "to reach it from a VM") would pass every existing guard.
- **Impact**: A future edit could silently expose world mutation to the network. The severity table puts a "network-reachable … without an explicit opt-in" surface at a HIGH floor, so the regression this guard would catch is HIGH.
- **Suggested Fix**: In `listener.rs` tests, `let (_, h) = spawn(0).unwrap(); assert!(h.local_addr().ip().is_loopback());`.

### TOOL-D2-2026-09-29-01: `debug-cli.md` residual drift after the 63c0aee3b reconcile: base64 screenshots, the `/tmp` tex.dump default, and the "5 s" timeout
- **Severity**: LOW
- **Dimension**: Protocol & Registry
- **Exposure**: developers
- **Location**:
  - `docs/engine/debug-cli.md:123`, `:342-345`, `:938-942`, `:21`
  - `crates/debug-protocol/src/lib.rs` (`DebugResponse::Screenshot`)
  - `tools/byro-dbg/src/display.rs:70`, `tools/byro-dbg/src/tui.rs:383`
- **Status**: NEW. It is related to #4756, whose scope (counts and missing rows) is now fixed.
- **Description**:
  1. `:123`: `Screenshot { path? }` "…else returns base64 PNG". The server never does this. With `path: None` it writes `screenshot_<secs>.png` and answers `ScreenshotSaved` (`system.rs:93-106`). `DebugResponse::Screenshot` is never constructed anywhere, which makes it a dead variant that byro-dbg still matches.
  2. `:342-345`: `tex.dump` "default `/tmp/tex_dump.png`". It is now `texture-dumps/tex_dump.png` and needs a startup-configured archive, which contradicts the file's own header at `:16-19`.
  3. `:938-942`: "The per-client thread's 5 s `recv_timeout`". It is 30 s (`dd99cd0f3`), and the new 10 s client timeout (TOOL-D1-01) is not documented.
  4. `:21`: "Last reconciled 2026-08-25".
- **Impact**: Doc rot only. Readers get the wrong behaviour for two commands.
- **Suggested Fix**: Correct the three statements and bump the reconcile stamp. Then either implement the documented base64 return for `path: None` or delete the `Screenshot` variant (#4374 removed `ListLoadedAssets` for the same reason).

## Dimensions with no findings

- **Dim 3 (SDK Contract):**
  - The dependency set is `byroredux-core` (only `math::Vec3`), `semver`, `serde` and `thiserror`.
  - All 28 `*_CAPABILITY` constants have real call sites.
  - The storage_util split (#4768) is a pure move.
  - #4702's death commit iterates a `BTreeMap`, so it is deterministic.
  - The new dialogue panel exchanges plain data with the binary and leaks no engine types.
  - Its `simulation_paused` carve-out is what exposed TOOL-D1-02.
- **Dim 6 (Tool CLIs):**
  - The #4761/#4762 fixes are verified by tests.
  - The README now states that `[upscaler]` is trusted executable input.
  - `SourceStack` is last-wins, matching the engine.
  - A partially written set leaves its reference PNG behind, and a rerun needs `--overwrite`. That is recoverable and reported, so it is not filed.
- **Dim 2 registry (noted, not filed):** components defined in the `byroredux` binary crate cannot be registered, because `register_all` has no hook for them: `NpcDialogueTopic`, `CombatDisposition`, `AmbientEngagement`, `PlacedItemCount` and `PlayerBodyRoot`. Dedicated commands cover them today (`dialogue.status`, `combat.status`, `player.body`, `inventory.status`).
- **Wire compat:** `0925f7926`'s six new `Option<>` `Metrics` fields deserialize as `None` when absent. Unknown fields are ignored, so an old byro-dbg and a new engine interoperate in both directions.

## Exposure Matrix

| Surface | Reachable by | Authenticated | Size-capped | Persistence atomic |
|---|---|---|---|---|
| Debug TCP server (`crates/debug-server`) | Any local process. Default-on in debug builds; release needs `BYRO_DEBUG_SERVER=1` | No (loopback only, not test-pinned: D1-06) | Yes, both directions (16 MB, walk depth 64) | N/A. Writes are confined to `screenshots/` and `texture-dumps/` with `create_new` (D1-04). Cell-load paths are unconfined (D1-05). Unresponsive while paused (D1-02) |
| byro-dbg client | Developer | N/A | Decode 16 MB | N/A. 10 s timeout < server 30 s (D1-01) |
| SDK / extensions bridge | In-process, via the mod-runtime sandbox | Capability-gated (28 constants) | N/A | Settings writes go through settings-io (D4-02) |
| Studio host (`studio_host.rs`) | In-process: egui panel, console, SDK | Single validated choke point | N/A | N/A |
| Boot handoff (`boot-request`) | Launcher → engine | Local trust | N/A | `atomic_write`, except the any-error fallback (D4-01) |
| Settings (`settings-io`) | Launcher, engine, extensions | Local trust | N/A | `atomic_write`, except the fallback (D4-01). A file that fails to parse is discarded on save (D4-02) |
| Root overrides (`game-detect::overrides`) | `byro-detect --write`, launcher | Local trust | N/A | `atomic_write`, except the fallback (D4-01). Comments are still dropped by the `toml::Table` round-trip |
| Install detection (`game-detect`, `byro-detect`) | Local CLI and launcher | N/A | ESM read unbounded (D5-01) | N/A |
| texture-upscale | Local CLI with an operator manifest | Manifest is trusted executable input (documented) | Disk space checked before writes | Output collisions rejected up front (#4761) |

## Cross-audit dedup notes

- **Not re-filed:** EXT-D7-2026-09-29-01 (m-exteriors, m34, smoke README) and SPT-D3-01 (m-trees). TOOL-D1-03 covers the remaining siblings.
- **PAR-D2-2026-09-29-02** (`resolve_nif_bytes`: BSA-only, first-listed-wins, errors dropped) is tooling-owned. It is cross-referenced from TOOL-D1-05 and not duplicated.
- **UI-D1-2026-09-29-01** (Existing #4757, the `canonical_menu_name` partial fix) belongs to `/audit-ui`.
- **Skill drift noticed.** For the next skill sync; no skill was edited:
  - Dim 1's "Facts" still say every stock build exposes the port and that `LoadNif` reads absolute paths.
  - Dim 1 says `safe_tex_dump_path` "has no test". It has `output_path_is_confined_to_the_dump_directory` at `assets.rs:432-436`.
  - Dim 2 says `every_ai_procedure_behavior_component_is_registered` is "the only completeness guard". There are now three.
  - The Dim 2 First-step `grep -c 'register_component::<'` counts the test module's own literal. It reports 49 only because it misses the manual `register_faction_reputation`.
- **Lock order** is `/audit-concurrency` Dim 7 and **sandbox enforcement** is `/audit-safety` Dim 8. Neither was re-audited here.

---

*Next*: `/audit-publish docs/audits/AUDIT_TOOLING_2026-09-29.md`. Suggested labels:
- `tech-debt` on all of them (debug-server, launcher and tools have no label of their own);
- `doc-rot` on D2-01 and D1-03 (its doc part);
- `test-gap` on D1-06.

Also close #4753, #4754, #4755, #4756, #4758, #4759 and #4760 with a pointer to `63c0aee3b`. That is a manual step, not part of publishing. Close #4752 after D1-05 lands.
