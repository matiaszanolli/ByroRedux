**HEAD**: 670f81917 · **Baseline**: AUDIT_TOOLING_2026-10-27.md (HEAD fa6be4a6b) · **Audited**: Dim 1 (Debug Trust Boundary, new console commands), Dim 2 (Protocol & Registry counts), Dim 3 (SDK Contract, `relationships.rs`), plus the `GameMenuPage` host panels in `crates/debug-ui` · **Unchanged since baseline (skimmed)**: Dim 4 (Boot Handoff & Persistence: no commits under `boot-request`, `settings-io`, `atomic_file.rs`, `paths.rs`, `tools/byro-launcher`, `boot/cli.rs`, `cli_args.rs`), Dim 5 (Install Detection: no commits under `game-detect`, `game_profiles.rs`, `debug_profiles.toml`, `tools/byro-detect`), Dim 6 (Tool CLIs: no commits under `tools/texture-upscale`), `debug-server`, `debug-protocol`, `byro-dbg`, `debug_load.rs`, `studio_host.rs`, `commands/studio.rs`, `extensions/`

# Tooling and Host-Contract Audit - 2026-10-31

Scope: `git log fa6be4a6b..HEAD` (136 commits), filtered by each dimension's `Paths:`. Run inside `/audit-suite --preset comprehensive`. One auditor, no sub-agents, no engine launch, repo `target/` only (`CARGO_INCREMENTAL=0`). Notes are in `/tmp/audit/tooling/dim_<N>.md`.

| Dim | Commits that touched tooling paths |
|---|---|
| 1 | M43 console doors in `byroredux/src/commands/` (`story_actions.rs` new, `gameplay.rs`, `quest.rs`, `vats.rs`, `mod.rs`): `d467b074c`, `851636a13`, `2a7121ff2`, `b54bc65a8`, `542f3de7b`, `31aca901d`, `4dd6197dc`, `7c9414845`, `93428cba1`, `834e83822`, `9d82dc616` (Fix #6469, `vats.enter` shares the key's gates). `debug-server`, `debug-protocol`, `debug_load.rs`, `byro-dbg`: none. |
| 2 | Doc/registry count only (`docs/engine/debug-cli.md` 145 -> 160 commands). No `DebugRequest`/`DebugResponse` change, no new component registration. |
| 3 | `407730df3` (Fix #6504, SDK faction catalog keeps -200 edges). `studio_host.rs`, `commands/studio.rs`, `extensions/`: none. |
| debug-ui | `c2742a901`, `851636a13`, `d467b074c`, `041d57333` (crafting page, Drop button, dialogue speaker/cue/numbering). |
| 4, 5, 6 | none |

## Test baseline

`cargo test -j 8 --no-fail-fast` over the eleven tooling crates (repo `target/`). All green.

| Crate | Tests | Delta vs 10-27 |
|---|---|---|
| `byroredux-sdk` | 105 | +1 (`modifiers_beyond_one_hundred_are_kept_as_authored`, #6504) |
| `byroredux-debug-server` | 26 | = |
| `byroredux-debug-protocol` | 8 | = |
| `byroredux-debug-ui` | 22 | +1 (`dialogue_page_marks_speaking_replies_and_said_topics`) |
| `byroredux-boot-request` | 17 | = |
| `byroredux-settings-io` | 14 | = |
| `byroredux-game-detect` | 60 | = |
| `byro-launcher` | 33 + 1 ignored (Vulkan) | = |
| `byro-detect` | 6 | = |
| `byro-texture-upscale` | 19 (lib) + 0 (bin) | = |
| `byro-dbg` | 2 | = |

## Executive Summary

**2 findings, both NEW: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 2 LOW.** Plus the two 10-27 findings, which are still present and still open (see "Existing").

Existing, not re-filed:
- **#6528** (TOOL-D3-2026-10-27-01, MEDIUM): `InputAction::parse` (`crates/sdk/src/event.rs:711-727`) still has 13 arms against 14 in `as_str`; no `"vats"` arm. Unchanged at HEAD.
- **#6529** (TOOL-D2-2026-10-27-01, LOW): the displaced "Phase 1 lockstep pin" doc is still on `metrics_payload_without_the_breakdowns_still_deserializes` (`crates/debug-protocol/src/wire.rs:175-183`).

Areas swept with no new defect:
- **Debug trust boundary.** `debug-server`, `debug-protocol`, `byro-dbg` and `debug_load.rs` have no commits since the baseline, so the loopback bind, the caps (frame 16 MB, 64 queued, 8 clients, depth 64) and the confinement helpers are unchanged. The 15 new console commands add no `fs::`, `File::`, `OpenOptions`, `Command::new` or `Path` use (diff-grepped over `commands/`), so no new client-chosen file path. The one defect in the new commands is the missing finite check in D1-01.
- **Registry and docs.** The live registry has 160 `registry.register(` calls and `debug-cli.md:343` says **160**. Components: 82 whole-file `register_component::<` lines minus the 2 test needles = 80 production (79 + `register_faction_reputation` = 80), matching `debug-cli.md:189` "(80 components)". No component was added, no `DebugRequest`/`DebugResponse` variant was added, and the reverse doc diff shows every one of the 15 new names documented (`craft`, `lock.*`, `word.teach`, `jail.*`, `place.at`, `sm.nodes`, `conv.status`, `entity.move`, `start.release`, `armor.status`, `mine.status`, `attraction.status`, `inv.drop`).
- **SDK.** `407730df3` removed the `-100..=100` range check and the `ModifierOutOfRange` variant from `FactionRelationship::new`. The WIT record carries `modifier: s32`, no other reader of the variant exists (grep over `crates`, `byroredux`, `docs/engine`), and the new test covers `i32::MIN`/`MAX` and the `truncated` flag. The SDK dependency set is unchanged. The `ObjectId`/`FormRef` surface leaks no engine type.
- **Persistence, install detection, tool CLIs.** No commits; skimmed per the header. The guards named in the skill were present in the green test run (launcher 33, boot-request 17, settings-io 14, game-detect 60, texture-upscale 19).

## Findings

### TOOL-D1-2026-10-31-01: `entity.move` and `place.at` parse `f32` offsets with no finite check, so `NaN`/`inf` is written into live transforms or a created placement
- **Severity**: LOW
- **Dimension**: Debug Trust Boundary
- **Exposure**: any local process on a debug build, or a release launched with `BYRO_DEBUG_SERVER=1` (the console is unauthenticated); developer-only in practice
- **Location**: `byroredux/src/commands/story_actions.rs:256-285` (`EntityMoveCommand::execute`), `:165-197` (`PlaceAtCommand::execute`, offset parse at `:171`). Added by `93428cba1` / `7c9414845` (M43 live gates).
- **Status**: NEW
- **Description**: `str::parse::<f32>` accepts `NaN`, `inf` and `-inf`. `entity.move <e> NaN 0 0` adds a NaN vector to `Transform.translation` **and** `GlobalTransform.translation` of any entity, and nothing downstream (transform propagation, physics sync, BLAS/TLAS instance upload) re-validates it. `place.at <base> NaN` builds `position = zup + offset` and hands it to `placement_at_position` and `CreatedReferences::place`, so the persistent created-reference registry then holds a non-finite position. `place.at` also uses `filter_map(|part| part.parse().ok())`, so `place.at 1F abc 5` silently reads `5` as `dx`: a typo shifts every later axis instead of being refused. The rest of the console follows the finite-check convention (`time.rs:38,195,275`, `terrain.rs:206`, `view.rs:570`, `hud.rs:95,143`, `physics.rs:68`, `anim_event.rs:161-216`, `scene.rs:962`; `studio_host::valid_transform` for the SDK path).
- **Evidence**: `story_actions.rs:259-262` and `:171`; no `is_finite` anywhere in `story_actions.rs`.
- **Impact**: one console line from a local process, or a stray typo in a live gate script, can leave a NaN transform on an entity (renderer instance data, a Rapier body sync) or a non-finite created reference that a save then persists. Recoverable only by reloading the cell. Developer tooling, hence LOW.
- **Related**: the console-as-whole-surface note in Dim 1; `studio_host::valid_transform`
- **Suggested Fix**: reject non-finite deltas in `entity.move` and non-finite or unparsable offsets in `place.at` with the usage error, as `terrain.rs:206` does. A one-line test per command (`entity.move 1 NaN 0 0` leaves the transform unchanged and returns an error).

### TOOL-D1-2026-10-31-02: `publish_ui_menus` does not know the new crafting page, so a workbench reports the Pause menu as open
- **Severity**: LOW
- **Dimension**: Debug Trust Boundary (debug-ui `GameMenuPage` host seam)
- **Exposure**: player / modder (script menu-mode queries: `GetPCMenuMode`/`IsPCMenuMode` kind 1013, `menu_open`)
- **Location**: `byroredux/src/systems/script_queries.rs:32-48` (`publish_ui_menus`), `:55-70` (`menu_open`); page added by `851636a13` in `crates/debug-ui/src/lib.rs:316-324`
- **Status**: NEW
- **Description**: `publish_ui_menus` derives `pause` as `ui.game_menu_visible() && !(inventory || dialogue || message_box || barter)`. `851636a13` added `GameMenuPage::Crafting` (and `DebugUiState::crafting_menu_visible`, which has no caller), but the exclusion list and `UiMenuState` were not extended. With the crafting page up, `pause` is `true`, so `menu_open(1013)` ("Pause" menu) reports open while the player is at a bench. `menu_open(2)` and `menu_open(0)` are true either way, so only the specific Pause query is wrong. The engine has no mapped kind for the crafting page (a "menu the engine has no screen for is never open"), so the right outcome is `pause = false` and `other = true`, with an explicit `crafting` flag.
- **Evidence**: `script_queries.rs:38-46` omits `crafting`; `grep crafting_menu_visible byroredux/src` finds no caller.
- **Impact**: a script that gates on "player is in the pause menu" (for example a quest stage that waits for the pause menu to close) misfires at crafting benches. Narrow; the crafting page is new and no vanilla script targets it.
- **Related**: `851636a13`, `UiMenuState` in the save registry completeness list
- **Suggested Fix**: add `crafting: ui.crafting_menu_visible()` to `UiMenuState`, exclude it from `pause`, and include it in `other`. A test that publishes with each `open_*_menu` and asserts exactly one page flag.

## Dimension notes

- **Dim 1**: finding D1-01. The `vats.enter` gate gap from 10-27 was fixed by `9d82dc616` (Fix #6469), which only touches `commands/vats.rs`. `craft`, `inv.drop`, `lock.open`, `word.teach`, `jail.*` call the same engine functions as the gameplay paths and take a numeric entity or hex form id, never a path. `parse_console_form_id` rejects non-hex.
- **Dim 2**: no findings new. Counts re-derived (160 commands, 80 components). The 10-27 D2-01 doc-displacement (#6529) persists.
- **Dim 3**: no new findings. #6504 is a behaviour change only (a modder reading `FactionRelationship::modifier` can now see values beyond +-100 up to `i32`). `docs/engine/sdk-v0.1-*.md` do not state the old range.
- **debug-ui**: the crafting page's `Craft { recipe }` and the inventory `Drop { index, form_id }` actions are applied in `apply_debug_ui_outputs` (`main.rs:1751`, `inventory::apply_action`), which re-validate against the open bench / the player's inventory (`crafting::apply_action` fails with "no bench is open"). `simulation_paused` still stops the scheduler for the crafting page and keeps the debug drain on (`run_exclusive_named`); the dialogue page is still the only exemption. `draw_crafting_page` and `draw_barter_page` have no panel-level test (the barter page had none before either), so nothing new in that class is filed.
- **Dim 4**: unchanged. **Dim 5**: unchanged. **Dim 6**: unchanged (19 tests green).

## Exposure Matrix

| Surface | Reachable by | Authenticated | Size-capped | Persistence atomic |
|---|---|---|---|---|
| Debug TCP console (`127.0.0.1:9876`; 160 commands + `SetField`, incl. `script.exec`/`obscript.cmd` VM doors, `vats.*`, `lock.open`, `place.at`, `entity.move`) | any local process on a debug build, or a release with `BYRO_DEBUG_SERVER=1`; loopback pinned by test | no | yes (frame 16 MB, 64 queued, 8 clients, depth 64); `entity.move`/`place.at` accept non-finite floats (D1-01) | n/a |
| `Screenshot` / `tex.dump` file writes | same | no | PNG only; filename-only under `screenshots/` / `texture-dumps/`; `create_new`, unique default names | no (single `create_new` write) |
| Session-only console settings (`terrain.blend` / `.soften` / `.parallax`) | same | no | `validate_value` finite + range | pinned off disk until a menu edit |
| `LoadNif` / `LoadInteriorCell` / `LoadExteriorCell` | same | no | absolute/`..` refused at enqueue; canonicalised into startup roots at drain | n/a |
| Native menu pages (pause / inventory / dialogue / barter / crafting / message box) | player | n/a | actions re-validated host-side | n/a |
| SDK extensions (`--extension`, wasm) | modder with a manifest and grants | capability grants (one gate in `deliver`) | `ComponentStoreLimits` | n/a; manifest input filter cannot name `vats` (#6528) |
| `boot.toml` handoff | launcher user | no (versioned) | n/a | yes (`atomic_write`) |
| Engine stderr -> launcher crash tail | launcher-spawned engine | n/a | 200-line tail, 16 KiB per line (#6279) | n/a |
| `settings.toml` | launcher + engine | no (versioned, baseline-merge, session pins) | n/a | yes |
| `profiles.toml` `[roots]` merge | `byro-detect --write`, launcher | no | n/a | yes (`atomic_write`, formatting-preserving) |
| texture-upscale manifest | developer | manifest is trusted executable input | disk-space, read-only and overflow checks | writes new PNGs, `--overwrite` required to replace |

## Next step

`/audit-publish docs/audits/AUDIT_TOOLING_2026-10-31.md` (labels: D1-01 `bug`, `low`, `tech-debt`, `test-gap`; D1-02 `bug`, `low`, `tech-debt`). Existing #6528 and #6529 are still open.
