**HEAD**: fa6be4a6b · **Baseline**: AUDIT_TOOLING_2026-10-21.md (HEAD 4a3e89d56) · **Audited**: Dim 1 (Debug Trust Boundary), Dim 2 (Protocol & Registry), Dim 3 (SDK Contract), Dim 4 (Boot Handoff & Persistence), Dim 5 (Install Detection) · **Unchanged since baseline (skimmed)**: Dim 6 (Tool CLIs; no commits under `tools/texture-upscale`), the durability writers in Dim 4 (`atomic_file.rs`, `boot-request`, `settings-io` save path), `studio_host.rs`, `commands/studio.rs`

# Tooling and Host-Contract Audit - 2026-10-27

Scope: `git log 4a3e89d56..HEAD`, filtered by each dimension's `Paths:`. Run inside `/audit-suite --preset comprehensive`. One auditor, no sub-agents, no engine launch, repo `target/` only. Notes are in `/tmp/audit/tooling/dim_<N>.md`.

| Dim | Commits that touched tooling paths |
|---|---|
| 1 | `05d94dd61` (#6278 metrics on the wire), 11 new console commands (134 → 145: `vats.*` ×6 `7eb4de261`/`1fb7e6f56`, `dialogue.select` + `ui.answer` `6db47b0e4`, `jail.serve` `e161c45f3`, `terrain.parallax` `b81a43443`, `terrain.soften` `e71451d08`), `8dc0a9cad` (#6322 `mesh.info`), `051179b7b` (test literal in `debug_load.rs`) |
| 2 | `05d94dd61` (#6278), the six M42 package-behaviour registrations (`b540b104a`, `10f8cbb91`, `9aaf5e1cf`, `fcd1cce4a`), doc count bumps `30d1b655f` / `28d855ec5` / `3f5f6ffea` |
| 3 | `818eba7f4` (#6350 `InputAction::Vats` in the SDK + WIT), `67722b6e4` (#6362 `PlayerEntity::get` in `extensions/`), `e71451d08` / `b81a43443` / `994620123` / `e9b7bcead` (debug-ui sliders, nested GPU bracket, rad meter colour) |
| 4 | `ad5abcaf1` (#6279), `a355fc8b5` (#6280), `b42f802fa` (#6274 `core::paths`), `e403564c6` (#6331 release gate), `e52d41c8c` (#6168 doc renames) |
| 5 | `b42f802fa` (#6274), `94dab0912` (#6192 `vdf::Value::string_map` test-only) |
| 6 | none |

## Test baseline

`cargo test -j 8 --no-fail-fast` over the eleven tooling crates (repo `target/`). All green.

| Crate | Tests | Delta vs 10-21 |
|---|---|---|
| `byroredux-sdk` | 104 | = |
| `byroredux-debug-server` | 26 | +1 (`metrics_response_mirrors_core_snapshot`, #6278) |
| `byroredux-debug-protocol` | 8 | +1 (`metrics_payload_without_the_breakdowns_still_deserializes`, #6278) |
| `byroredux-debug-ui` | 21 | +1 (`cloud_shadow_is_not_summed_with_the_sky_cube_that_contains_it`) |
| `byroredux-boot-request` | 17 | = |
| `byroredux-settings-io` | 14 | = |
| `byroredux-game-detect` | 60 | = |
| `byro-launcher` | 33 + 1 ignored (Vulkan) | +4 (#6279 pump tests) |
| `byro-detect` | 6 | = |
| `byro-texture-upscale` | 19 (lib) + 0 (bin) | = |
| `byro-dbg` | 2 | = |

## Executive Summary

**2 findings, both NEW: 0 CRITICAL, 0 HIGH, 1 MEDIUM, 1 LOW.**

All three findings from 10-21 are fixed at HEAD:

| Issue | State at HEAD |
|---|---|
| #6278 (TOOL-D2-2026-10-21-01, metrics guard missing, two fields off the wire) | Fixed (`05d94dd61`). `metrics_payload` (`evaluator.rs`) destructures `MetricsSnapshot` with no `..`. `metrics_response_mirrors_core_snapshot` exists and pins every value. `cpu_pass_ms` / `top_systems_ms` are `serde(default)`, and `display.rs` / `tui.rs` show both. |
| #6279 (TOOL-D4-2026-10-21-01, stderr reader closes the pipe) | Fixed (`ad5abcaf1`). `pump_lines` reads bytes, decodes lossily, caps each line at `MAX_LINE_BYTES` and drains into a sink after a read error. |
| #6280 (TOOL-D4-2026-10-21-02, release docs say cwd-only `assets/`) | Fixed (`a355fc8b5`). README and `docs/releasing.md` describe the cwd-then-exe-dir lookup, and the copy-or-symlink note is gone. |

Areas swept with no new defect:
- **Debug trust boundary.** The bind is still the literal `("127.0.0.1", port)` (`listener.rs:173`) and a test pins it. The caps have not changed. None of the 11 new console commands reads or writes a path the client picks: no new `fs::` / `File::` / `OpenOptions` / `Command::new` appeared in `commands/`, `debug-server` or `debug_load.rs`. `terrain.soften` and `terrain.parallax` write through the shared `set_session_number` → `SettingsRegistry::set`, where `validate_value` checks the value is finite and inside the slider range. `set_session_number` then calls `pin_stored`. `apply_debug_ui_outputs` (`main.rs:1791-1794`) releases a pin by `change.id`, so the new ids are covered with no extra code. The `vats.enter` gate gap is **Dup of AUDIT_GAMEPLAY_2026-10-27 GAME-D4-2026-10-27-02** and is not re-reported here.
- **Registry and docs.** There are 79 production `register_component::<` calls plus `register_faction_reputation`, which makes **80**. That matches both count sites in `debug-cli.md` (`:189`, `:1353`). No `inspect` derive exists outside `crates/core/src`. The command count is 145 in both the registry and the doc. I diffed the doc's command block in reverse against the 145 registered `name()` strings and nothing was left over: the 11 names missing from first tokens all appear on combined `a / b` lines. No new `DebugRequest` or `DebugResponse` variant was added.
- **SDK.** The dependency set has not changed (`byroredux-core`, `semver`, `serde`, `thiserror`). The `PlayerEntity::get` refactor in `extensions/{commands,systems}.rs` keeps the same `None` semantics. `sdk_input_action` maps `InputAction::Vats`, and the Rust ↔ WIT case mapping in `mod-runtime/src/runtime/instance.rs` is an exhaustive `match`. The one defect is the manifest-side parser (D3-01).
- **Persistence and paths.** `crates/core/src/paths.rs:25` is now the only `current_exe()` call. `profiles.rs`, `presets.rs` and the launcher reach it through `shipped_asset` and `exe_dir`, and their behaviour is unchanged. No new CLI flag reader was added since the baseline.
- **Install detection.** There is no new reader of the legacy *BYROREDUX_SKYRIM_DATA* name, and no private `~/.byroredux/profiles.toml` path (that string appears only in comments and messages).

## Findings

### TOOL-D3-2026-10-27-01: `InputAction::parse` has no `"vats"` arm, so a manifest that filters input on the new VATS action is refused
- **Severity**: MEDIUM
- **Dimension**: SDK Contract
- **Exposure**: modder (any extension manifest that subscribes to `byro.events.input-action` with a `vats` filter)
- **Location**: `crates/sdk/src/event.rs:711-727` (`InputAction::parse`). Consumers: `crates/sdk/src/manifest.rs:405-406` (validation) and `byroredux/src/extensions/install.rs:187` (`filter_map(SdkInputAction::parse)`). Added by `818eba7f4` (Refs #6350).
- **Status**: NEW (related to open tracker #6350; no open or closed issue covers it)
- **Description**: `818eba7f4` added `InputAction::Vats` to the SDK enum, to `as_str` (`"vats"`), to the WIT `input-action` enum (`crates/mod-runtime/wit/host.wit:93`) and to the host mapping (`byroredux/src/extensions/mod.rs:464`). It did not add `"vats" => Self::Vats` to `parse`. `parse` is the only path from a manifest filter's `equals` string to an action. `Manifest::validate` therefore turns `equals = "vats"` into `ManifestError::InvalidInputAction("vats")`, so the action's own `as_str` spelling fails to parse.
- **Evidence**: `event.rs:713-726` lists 13 arms (`move-forward` … `quickload`, `pause`), while `as_str` at `:691-707` has 14. `serde(rename_all = "kebab-case")` derives `"vats"` for the serde path, so only the hand-written parser is out of step. No test round-trips every variant through `as_str` → `parse`. The only negative-case test is `raw-key-f13` (`manifest.rs:832-835`).
- **Impact**: an extension that wants VATS-key events only cannot declare that. Its manifest is rejected outright, and every other subscription in it goes too. The workaround is an unfiltered `input-action` subscription that filters inside the guest (`dispatch.rs:366` treats an empty filter set as "all actions"). The next action added to the enum will drift the same way.
- **Related**: #6350 (VATS runtime tracker), `818eba7f4`
- **Suggested Fix**: Add the `"vats"` arm. Then write a test that walks every variant through `parse(as_str(v)) == Some(v)`, either with a const `ALL` slice or an exhaustive `match` in the test so a new variant fails to compile. Do the same for `SessionPhase`.

### TOOL-D2-2026-10-27-01: #6278's new wire test was inserted under an existing test's doc comment, so the "Phase 1 lockstep pin" doc now describes the wrong test
- **Severity**: LOW
- **Dimension**: Protocol & Registry
- **Exposure**: developers only
- **Location**: `crates/debug-protocol/src/wire.rs:175-183` (doc block), `:194` (`round_trip_phase1_response_variants`, which the doc block used to sit on)
- **Status**: NEW (same displaced-doc class as closed #5692)
- **Description**: The block "Phase 1 — same lockstep pin for the new response variants. Adding a field to `DebugResponse::Metrics` without updating this test … breaks the build at the missing assertion" documented `round_trip_phase1_response_variants`. `05d94dd61` placed the new `/// #6278 …` lines and `metrics_payload_without_the_breakdowns_still_deserializes` between that block and its function. Rustdoc concatenates the two blocks onto the new serde-default test, and `round_trip_phase1_response_variants` is left with no doc. A reader is now told that a deserialize-only test "breaks the build" when a field is added. That claim belongs to the struct-literal round trip.
- **Evidence**: `wire.rs:175-179` (old doc), `:180-181` (#6278 doc), `:182-183` (`#[test] fn metrics_payload_without_the_breakdowns_still_deserializes`), `:193-194` (`#[test] fn round_trip_phase1_response_variants` with no doc).
- **Impact**: the doc gives wrong guidance on which test enforces payload lockstep. No runtime effect.
- **Related**: #6278, #5692
- **Suggested Fix**: Move lines 175-179 back above `round_trip_phase1_response_variants`, leaving the #6278 lines on the new test.

## Dimension notes

- **Dim 1**: no findings. `debug_load.rs` changed only by a test literal (`root_yaw`). `terrain.height` still reads only engine-owned height-cache DDS paths, now decoded through `byroredux_menuxml::tex::Rgba8::decode_dds`.
- **Dim 2**: finding D2-01. Old-engine → new-client compatibility is pinned. The reverse direction (new engine → old client) relies on serde ignoring unknown fields, and that holds because `MetricsPayload` has no `deny_unknown_fields`.
- **Dim 3**: finding D3-01. An observation I am not filing: the WIT `input-action` enum gained a case before `pause`, and the package stays `byro:mod-host@0.1.0`. The component model type-checks enums by case list, so a component built against the 13-case enum no longer instantiates. This is allowed before v0.1 (`docs/engine/sdk-v0.1-development-plan.md` §10: "prototype names may change with a migration note"), but I found no migration note.
- **Dim 4**: no findings. `pump_lines` handles `Interrupted` with a retry. Any other error is drained to EOF, so the pipe's read end stays open for the child's lifetime. `cache_root` puts caches beside the executable unless `BYRO_CACHE_DIR` is set, so a read-only install needs the env var. This is by design (#6274) and renderer/terrain-owned.
- **Dim 5**: no findings. Both changes preserve behaviour.
- **Dim 6**: unchanged. Guards spot-checked present, 19 tests green.

## Exposure Matrix

| Surface | Reachable by | Authenticated | Size-capped | Persistence atomic |
|---|---|---|---|---|
| Debug TCP console (`127.0.0.1:9876`; 145 commands + `SetField`, incl. `script.exec`/`obscript.cmd` VM doors and `vats.*`) | any local process on a debug build, or a release with `BYRO_DEBUG_SERVER=1`; loopback pinned by test | no | yes (frame 16 MB, 64 queued, 8 clients, depth 64) | n/a |
| `Screenshot` / `tex.dump` file writes | same | no | PNG only; filename-only under `screenshots/` / `texture-dumps/`; `create_new`, unique default names | no (single `create_new` write) |
| Session-only console settings (`terrain.blend` / `.soften` / `.parallax`) | same | no | `validate_value` finite + range | pinned off disk until a menu edit |
| `LoadNif` / `LoadInteriorCell` / `LoadExteriorCell` | same | no | absolute/`..` refused at enqueue; canonicalised into startup roots at drain | n/a |
| SDK extensions (`--extension`, wasm) | modder with a manifest and grants | capability grants (one gate in `deliver`) | `ComponentStoreLimits` | n/a; manifest input filter cannot name `vats` (D3-01) |
| `boot.toml` handoff | launcher user | no (versioned) | n/a | yes (`atomic_write`) |
| Engine stderr → launcher crash tail | launcher-spawned engine | n/a | 200-line tail, 16 KiB per line (#6279) | n/a |
| `settings.toml` | launcher + engine | no (versioned, baseline-merge, session pins) | n/a | yes |
| `profiles.toml` `[roots]` merge | `byro-detect --write`, launcher | no | n/a | yes (`atomic_write`, formatting-preserving) |
| texture-upscale manifest | developer | manifest is trusted executable input | disk-space, read-only and overflow checks | writes new PNGs, `--overwrite` required to replace |

## Next step

`/audit-publish docs/audits/AUDIT_TOOLING_2026-10-27.md` (labels: D3-01 `bug`, `medium`, `tech-debt` (the SDK has no label of its own), `test-gap`, `game:fnv`, `game:fo3`; D2-01 `documentation`, `low`, `tech-debt`, `doc-rot`).
