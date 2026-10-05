**HEAD**: a2c24b16e · **Baseline**: AUDIT_TOOLING_2026-10-02.md (HEAD e737f06bf) · **Audited**: Dim 1 (Debug Trust Boundary), Dim 2 (Protocol & Registry), Dim 4 (Boot Handoff & Persistence), Dim 5 (Install Detection) · **Unchanged since baseline (skimmed)**: Dim 3 (SDK Contract — no commits), Dim 6 (Tool CLIs — no commits)

# Tooling and Host-Contract Audit — 2026-10-05

The scope was delta-first against the 2026-10-02 report: `git log e737f06bf..HEAD` per dimension `Paths:`. In that range, these commits touched tooling:

| Dim | Commits |
|---|---|
| 1 | `554259315` (#5165 cell-load confinement), `cc5468813` (#5140 timeout pair), `00115f7a5` (#5141 drain under pause; the engine-side hunk landed in `b7bc84722`, the #5120 NVML commit), `6f38b335d` (#5150), `7ead491d7` (unrelated `main.rs` resource) |
| 2 | `977793679` (#4755 registry 49 → 67), `6f38b335d` (#5150), `800e3ed7c` (#5168), `cc5468813`, plus console-output tweaks in `world_info` / `water` / `hud` / `env_health` / `physics` / `time` / `view` |
| 4 | `69fd54fb2` (#5163, #5164), `7f2cee3e8` (#5166), `c9f95283a` (#5162) |
| 5 | `ad6967fa9` (#5167), `7f2cee3e8`, `69fd54fb2` |
| 3, 6 | none |

No sub-agents were used. Notes for each dimension were written to `/tmp/audit/tooling/dim_<N>.md` as the work went, and this report was reconciled against them. The engine, the smoke scripts and every GPU process were left unlaunched.

## Test baseline

`cargo test -j 6` with rustc 1.96.0. Everything was green, with 0 failures.

| Crate | Tests | Δ vs 10-02 |
|---|---|---|
| `byroredux-sdk` | 104 | = |
| `byroredux-debug-server` | 22 | +2 (`cancelled_command_is_answered_without_evaluation`, roster guard) |
| `byroredux-debug-protocol` | 7 | = |
| `byroredux-debug-ui` | 19 | = |
| `byroredux-boot-request` | 17 | −1 (the vacuous positive-only guard was folded into `save_has_no_nonatomic_clobber_fallback`, #5164) |
| `byroredux-settings-io` | 14 | +1 |
| `byroredux-game-detect` | 55 | +3 |
| `byro-launcher` | 24 + 1 ignored (needs Vulkan) | = |
| `byro-detect` | 3 | +3 (#5167) |
| `byro-texture-upscale` | 18 | = |
| `byro-dbg` | 2 | = |
| `byroredux` bin, filtered to `debug_load`, `commands::assets`, `boot::cli`, `settings`, `studio` | 46 | (#5165 guards included) |

## Executive Summary

There are **3 findings: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 3 LOW.** All three are NEW. One is the unaddressed residual of closed #5150.

**All seven findings from 2026-10-02 are fixed and verified at HEAD:**

| Issue | Fix |
|---|---|
| #5162 | Settings writes only the changed ids against a per-frontend baseline. The launcher reloads on opening Settings and on engine exit. |
| #5163 | `atomic_write` removes its temp on every pre-rename failure. |
| #5164 | Structural guard `assert_no_clobber_fallback` over `production_text`. |
| #5165 | Cell-load ESM, master and archive paths go through `confine_to_roots`. |
| #5166 | `toml_edit` edits only the changed `[roots]` keys. |
| #5167 | `byro-detect` parses its arguments strictly. |
| #5168 | Continuation restored. |

**The two 09-29 carry-overs are also fixed.** #5140 now uses a 30 s / 35 s ordered timeout pair. #5141 keeps the drain running while paused and skips cancelled commands.

**Two open issues are already fixed in code and should be closed:**
- **#4753**: the encode side now enforces `MAX_MESSAGE_SIZE` (`wire.rs:15`). `WalkEntity` is clamped to `MAX_WALK_DEPTH` 64.
- **#4754**: `byro-dbg` sets `CLIENT_READ_TIMEOUT` and has two `#[test]`s.

**Still open and unchanged:**
- #5147: default output names collide under `create_new` (`assets.rs:252`, `system.rs:97`).
- #5149: no test pins the loopback bind. A `0.0.0.0` bind would still pass `occupied_port_is_reported_before_a_handle_is_returned`, because Linux reports `EADDRINUSE` across the wildcard and the specific address.

**The three new LOWs:**
- **TOOL-D2-01**: absolute-path guidance survives in `debug-cli.md` and in byro-dbg's TUI and help text, although #4752/#5165 reject absolute paths and the debug loads answer `Ok` before rejecting them. #5150's own addendum was never applied.
- **TOOL-D1-01**: the #5141 paused-frame drain is keyed by a duplicated string literal, and the engine discards the "found" result, so renaming the system silently brings back #5141.
- **TOOL-D5-01**: `byro-launcher --profiles <path>` uses the lax parsing #5167 removed from `byro-detect`. A custom profiles file is also never read by the engine, so the launcher can validate a root that the engine then doesn't use.

## Prior-finding status

| Issue | Finding | State at HEAD |
|---|---|---|
| #5140 | byro-dbg 10 s vs server 30 s | Closed and fixed. `COMMAND_RESPONSE_TIMEOUT` 30 s and `CLIENT_READ_TIMEOUT` 35 s (`crates/debug-protocol/src/lib.rs:18,29`). Guard: `read_timeout_exceeds_the_server_response_timeout`. The TUI treats a timeout as "engine busy" (`tui.rs:416-440`). |
| #5141 | Pause menu stops the drain | Closed and fixed. `app_events.rs:967-982` calls `run_exclusive_named`, and `system.rs:145-156` answers cancelled commands without evaluating them. Residual: TOOL-D1-01. |
| #5147 | `create_new` default-name collisions | Open, unchanged |
| #5149 | No loopback pin | Open, unchanged |
| #5150 | `debug-cli.md` residual drift | Closed, but its 10-02 addendum (the `screenshot /tmp/debug_frame.png` example, now `:1307`) is unfixed. See TOOL-D2-01. |
| #5162–#5168 | 10-02 findings | All closed and fixed (see the Executive Summary) |
| #4753 | Encode side unbounded | **Open but fixed in code.** Recommend closing. |
| #4754 | byro-dbg has no read timeout and no tests | **Open but fixed in code** (`cc5468813` and earlier). Recommend closing. |

## Findings

### TOOL-D2-2026-10-05-01: Absolute-path guidance outlives #4752/#5165 in `debug-cli.md` and byro-dbg, and #5150 closed without its addendum
- **Severity**: LOW
- **Dimension**: Protocol & Registry
- **Exposure**: developers using byro-dbg / the TUI loader. A request that follows the docs is silently dropped.
- **Location**:
  - `docs/engine/debug-cli.md:825-832` (LoadNif / cell-load bullets), `:17-18` (summary), `:1307-1308` (session example)
  - `tools/byro-dbg/src/tui.rs:650` (Loader hint)
  - `tools/byro-dbg/src/display.rs:340` (`.help`)
- **Status**: NEW. It is the residual of closed #5150, whose 2026-10-02 addendum comment named `:1284`, which has since shifted to `:1307`.
- **Description**: #4752 (`63c0aee3b`) and #5165 (`554259315`) made every debug-load file path relative to a startup `--esm`/`--master`/`--bsa` directory, and `write_screenshot` takes only a bare filename under `screenshots/`. Several user-facing texts still teach the old contract:
  - `debug-cli.md:825` says `LoadNif`'s `path` "is either an absolute loose-file path or an archive-relative" one.
  - The session example at `:1307` runs `screenshot /tmp/debug_frame.png` and shows `Screenshot saved: /tmp/debug_frame.png`.
  - The cell-load bullet (`:829`) and the summary (`:17-18`, "Loose `LoadNif` paths stay within configured game data roots") never state that cell loads follow the same rule. Only the protocol rustdoc (`crates/debug-protocol/src/lib.rs:107-111`) does.
  - The TUI Loader shows "Path may be loose absolute or BSA-relative." (`tui.rs:650`).
  - `.help` lists `screenshot <path>  Capture screenshot to specific file` (`display.rs:340`).

  `6f38b335d` ("Fix #5150") reconciled the screenshot *response* contract and deleted the base64 variant. It did not touch these lines, and #5150 was closed with the addendum still open.
- **Evidence**:
  - `confine_to_roots` (`byroredux/src/debug_load.rs`) returns `None` for `!requested.is_relative()`.
  - `exec_load_nif` then falls through to the archive scan, misses, and logs.
  - The request was already answered `DebugResponse::Ok` at enqueue time (`crates/debug-server/src/evaluator.rs:83-130`), so the client never sees the rejection.
- **Impact**: A developer who follows the TUI hint or the doc gets `Ok` and no load, and the reason is only in the engine log. The screenshot example returns an error. No security impact: the confinement itself is correct.
- **Related**: #5150, #4752, #5165, #5009 (same resolver, owned by `/audit-parsers`)
- **Suggested Fix**:
  - Rewrite the `LoadNif` bullet and the `:17-18` summary to state the root rule for all three load requests.
  - Change the example to `screenshot debug_frame.png` → `Screenshot saved: screenshots/debug_frame.png`.
  - Correct the TUI hint and the `.help` line.
  - Optionally, have `eval_request` reject an absolute or `..` path synchronously with an `Error`, so the client sees it.

### TOOL-D1-2026-10-05-01: The #5141 paused-frame drain is keyed by a duplicated string literal and its "found" result is discarded
- **Severity**: LOW
- **Dimension**: Debug Trust Boundary
- **Exposure**: developer only (regression-guard quality)
- **Location**: `byroredux/src/app_events.rs:980-981`; `crates/debug-server/src/system.rs:207-209`; `crates/core/src/ecs/scheduler.rs:545-558` (`run_exclusive_named`)
- **Status**: NEW (test-gap)
- **Description**: The paused path calls `self.scheduler.run_exclusive_named(&self.world, dt, "debug_drain_system")` and ignores the returned `bool`.
  - The name is the hand-typed copy of `DebugDrainSystem::name()`'s `"debug_drain_system"`.
  - The only test, `run_exclusive_named_runs_only_the_named_system`, registers its own `CountingSystem` under a third copy of the literal.
  - No test ties the engine call to the debug-server's system name.
- **Evidence**: `grep -rn debug_drain_system` returns exactly `app_events.rs:981`, `system.rs:208` and two lines inside the scheduler test. There is no shared constant.
- **Impact**: If `DebugDrainSystem::name()` is renamed, `run_exclusive_named` returns `false` and nothing reports it. The #5141 bug comes back unseen: commands queued under the pause menu time out, then fire in a burst on unpause (cancelled ones are now skipped, so the remaining harm is the timeouts). The `false` result is also legitimate in a build without the `debug-server` feature, which is presumably why it is ignored.
- **Related**: #5141, `b7bc84722` (where the engine hunk actually landed)
- **Suggested Fix**: Export `pub const DRAIN_SYSTEM_NAME: &str` from `byroredux-debug-server`, use it in both `name()` and the engine call, and add `debug_assert!` on the result under `#[cfg(feature = "debug-server")]`.

### TOOL-D5-2026-10-05-01: `byro-launcher --profiles` parses loosely, and the engine never reads the file it names
- **Severity**: LOW
- **Dimension**: Install Detection
- **Exposure**: CLI users of the launcher, a documented developer flag (`tools/byro-launcher/README.md:7`). Players launching with no arguments are unaffected.
- **Location**: `tools/byro-launcher/src/main.rs:64-78` (`profiles_path`); `tools/byro-launcher/src/state.rs:72-75,149-158`; `crates/game-detect/src/profiles.rs:273-292,394-398`; `tools/byro-detect/src/main.rs:171-178`
- **Status**: NEW (the launcher sibling of #5167)
- **Description**: The flag has two problems.
  1. **Parsing.** `std::env::args().position(|arg| arg == "--profiles").and_then(|i| std::env::args().nth(i + 1))` has the shape #5167 removed from `byro-detect`:
     - `--profile x` (a typo) and a trailing bare `--profiles` silently select the default `~/.byroredux/profiles.toml`.
     - `--profiles --x` takes `--x` as the path.
     - Unknown arguments are ignored.

     The launcher writes the profiles file on every Play (`remember()` → `merge_into_file`) and writes `boot.toml` beside it, so a typo retargets real writes.
  2. **The engine ignores a custom path.**
     - The launcher reads configured roots from the custom file (`detect_all(&profiles_path)`), validates against them, and records `[roots]` into it.
     - The boot request it hands the engine is a bare profile key (`BootRequest::for_profile`, `state.rs:152`).
     - The engine's loader reads only `home_dir()/.byroredux/profiles.toml` (`profiles.rs:274,288`).

     So a game shown "ready" in a `--profiles` launcher resolves, in the engine, through the default file or `DEFAULT_GAMES_ROOT`. That can be a different root or a missing one. `byro-detect --profiles X --write` has the same gap, while its help text says `--write` "makes `--game <key>` resolve correctly on this machine".

  The per-user path is also computed three times. `game-detect`'s private `home_dir()` returns `None` when `HOME`/`USERPROFILE` are unset, and the engine then skips the file. The launcher's and `byro-detect`'s copies `unwrap_or_default()` to a cwd-relative `.byroredux/profiles.toml`, which the engine never reads.
- **Evidence**: see Location. `docs/engine/launcher.md` has no `--profiles` entry, and the engine has no flag or environment variable that selects a profiles file.
- **Impact**: A developer testing with an alternate profiles file gets a launcher that disagrees with the engine it launches. A typo silently edits the real user file (formatting is now preserved by #5166, so only `[roots]` values change). This is low-likelihood and developer-facing.
- **Related**: #5167, #5166, TOOL-D5-2026-10-02-02
- **Suggested Fix**:
  - Expose one `pub fn user_profiles_path() -> Option<PathBuf>` from `byroredux_game_detect::profiles` and use it in all three places.
  - Parse the launcher's arguments strictly, like `byro-detect`.
  - Either carry the profiles path to the engine (a `BootRequest` field or an environment variable honoured by `load_default`), or document `--profiles` as "detection and validation only; the engine reads `~/.byroredux/profiles.toml`".

## Dimension notes

- **Dim 1**:
  - The bind is still the literal `("127.0.0.1", port)` (`listener.rs:173`). The caps are unchanged: 64 queued commands, 8 clients, 300 s idle read, 16 MB frames enforced on both encode and decode, walk depth 64.
  - No console command was added since baseline (still 99). No new file IO: the client-path writes are still only `write_screenshot` and `safe_tex_dump_path`, and `save`/`load` take `u32` slots.
  - `startup_asset_roots` canonicalises `Path::parent()`. A bare relative startup argument (`--esm FalloutNV.esm`) has parent `""`, which does not canonicalise, so every debug load is then rejected, with a logged reason. That is a usability edge only, not filed.
  - The TUI's "engine busy" path can only fire if the server thread stalls for more than 35 s, because the server always answers by 30 s. The off-by-one response pairing after such a timeout is practically unreachable. Not filed.
- **Dim 2**:
  - The registry has 66 production `register_component::<` calls plus `register_faction_reputation`, for 67. That matches `debug-cli.md:186`. The skill's `grep -c` also yields 67, but only because it counts the test-module literal at `registration.rs:628`.
  - An independent scan found 66 core types carrying both the `inspect` derive and `impl Component`, all registered.
  - The gap class "outside `crates/core/src`" is empty: no other crate's component carries the `inspect` derive.
  - The guard matches only a column-0 `impl Component for X {` in the same file, within a 4-line attribute window. Nothing escapes it today.
  - All 99 registered commands appear in the `debug-cli.md` command block. Ten of them sit on combined `a / b` lines.
  - `display.rs::print_response` and `tui.rs::variant_name` are exhaustive. The remaining `_ =>` arms are keyboard dispatch.
- **Dim 3**: no commits. The SDK dependencies are still core (`math::Vec3` only), semver, serde and thiserror, with `forbid(unsafe_code)`. 104 tests. Cross-reference: open #5239 (SDK `SetBase` on derived pools), owned by `/audit-character`.
- **Dim 4**: all three writers go through `atomic_write` and `assert_no_clobber_fallback`.
  - **Guard blind spot (noted, not filed):** a fallback that writes the in-memory text through `File::create(path)` + `write_all`, or `use std::fs::write; write(..)`, would pass the guard.
  - **Save paths:** every production settings save goes through `settings_io::save` and the shared baseline: `main.rs:899/984/1652`, `extensions/systems.rs:454`, and launcher `settings_screen.rs:65`, including preset apply.
  - **Extension reload at boot:** `extensions/install.rs:795` re-overlays `settings.toml` onto a cloned registry and commits it. That would drop the `--upscaler` CLI seed from the registry, but `record_active_upscaler` (`app_events.rs:188`, after extension load) restores and pins it, so it is benign at HEAD.
  - **Launcher tests:** the launcher reload sites (`app.rs:129,172`) are untested.
  - **Launcher dependencies:** `cargo tree -p byro-launcher` shows `ash` but no renderer or UI crate.
- **Dim 5**:
  - `is_bare_install_dir` is unchanged, and `esm_opens` is still bounded by `take(TES4_PROBE_LIMIT)`.
  - The legacy `BYROREDUX_SKYRIM_DATA` survives only as the `vars.` fallback at `.github/workflows/playable-smoke.yml:45`.
  - `byro-detect --help` matches its strict parser.
- **Dim 6**: no commits. 18 tests, and the #4761/#4762 guards are present.

## Cross-audit references (this suite run; not re-filed)

- **UI-D7-03 (LOW)**: `hud.values` / `hud.heading` accept NaN/±inf. Those are console commands, so they are reachable through the unauthenticated debug port. Exposure is the same as every console command (row 1 below). Owned by `/audit-ui`.
- **SPT-D3-01 (LOW)**: `m-trees.sh` misreads `byro> (no entities)`. The root on the byro-dbg side is that the REPL prints the `byro> ` prompt to **stdout** unconditionally (`tools/byro-dbg/src/main.rs:57-58`), including when stdin is a pipe. Every piped response's first line is therefore prefixed, and each smoke script must strip it. A TTY check (`std::io::IsTerminal`) or printing the prompt on stderr would fix this class at the source. The finding stays filed as SPT-D3-01.
- **CONC-D2-01 / CONC-D3-01 (MEDIUM)**: the vulkan-validation CI lane is red on WARN perf warnings and runs with RT off. Both are owned by `/audit-concurrency`. No tooling surface is involved.
- Debug-server lock order belongs to `/audit-concurrency` Dim 7, and sandbox enforcement to `/audit-safety` Dim 8. Neither was re-audited here.

## Exposure Matrix

| Surface | Reachable by | Authenticated | Size-capped | Persistence atomic |
|---|---|---|---|---|
| Debug TCP port (console incl. `hud.*`, `save`, `engine.quit`, `SetField`) | any local process; debug builds by default, release with `BYRO_DEBUG_SERVER=1`; drained while paused (#5141) | No | Yes: 16 MB frames both ways, 64 queued, 8 clients, walk depth 64, 30 s/35 s timeouts | n/a (save slots are owned by `/audit-save`) |
| Debug loads (`LoadNif`, `LoadInteriorCell`, `LoadExteriorCell`) | same | No | Paths confined to startup roots (#4752/#5165); a rejection is only logged after an `Ok` (TOOL-D2-01) | n/a (read-only) |
| Screenshot / `tex.dump` | same | No | Bare filename under `screenshots/` / `texture-dumps/`, `create_new` (#5147 collisions open) | n/a |
| `settings.toml` | launcher, engine, extensions | n/a | n/a | Yes (`atomic_write`, temp cleaned); writes only changed ids (#5162) |
| `boot.toml` | launcher → engine | n/a | n/a | Yes; contract version refused on mismatch |
| `profiles.toml` | launcher (on Play, only when `[roots]` changed), `byro-detect --write` | n/a | n/a | Yes, format-preserving (#5166); `--profiles` is not honoured by the engine (TOOL-D5-01) |
| texture-upscale manifest | local operator | Trusted executable input (documented) | Disk space checked first | Output collisions rejected (#4761) |

## Next step

Run `/audit-publish docs/audits/AUDIT_TOOLING_2026-10-05.md`. Labels: `tech-debt` on all three findings (debug-server, byro-dbg and the launcher have no label of their own), `doc-rot` on TOOL-D2-01, and `test-gap` on TOOL-D1-01.

Close #4753 and #4754 as already fixed (see Prior-finding status).
