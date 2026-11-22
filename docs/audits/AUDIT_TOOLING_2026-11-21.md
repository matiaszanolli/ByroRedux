# Tooling and Host-Contract Audit: 2026-11-21

**HEAD**: ecc59bea3 · **Baseline**: `AUDIT_TOOLING_2026-10-31.md` · **Areas swept**: all six dimensions. Every dimension's paths changed in the three weeks since the baseline, through new console commands and components, the bind retry, SDK caps and declarations, biped slots, the new-game targets, the temp-file helpers, the path folds, the GPU-panel split, and lab flags. The baseline's two findings, #6575 and #6576, are closed and their fixes are live: `entity.move` and `place.at` use the finite filter, and `publish_ui_menus` knows the crafting page.

Three review agents (Sonnet) each took two dimensions; the coordinator re-read every kept finding against the code. No engine or launcher window was launched, and no tracked file changed apart from the skill-text correction noted below.

## Test runs

`cargo test` over `byroredux-{sdk,debug-server,debug-protocol,debug-ui,boot-request,settings-io,game-detect}` and `byro-{launcher,detect,texture-upscale}` is all green: 303 passed, 1 ignored (the launcher's Vulkan probe).

| Crate | Tests |
|---|---|
| debug-server | 27 (+1 since 10-31: `a_port_freed_within_the_retry_window_is_bound`) |
| debug-protocol | 8 |
| byro-dbg | 2 |
| sdk + texture-upscale | 19 + 108 |
| boot-request | 17 |
| settings-io | 14 |
| game-detect | 60 |
| byro-launcher | 33 (+1 ignored) |
| byro-detect | 6 |

Binary filters, all green:

| Filter | Tests |
|---|---|
| `debug_cli` | 1 |
| `extensions` | 85 |
| `cli_args` | 15 |
| `boot` | 55 |

`cargo tree -p byro-launcher -i wgpu` is empty, and the SDK's normal dependency closure holds no renderer, UI, platform, archive or Wasmtime crate.

Counts were re-derived from production text:
- **Commands:** 193 registered, and `docs/engine/debug-cli.md` says **193**. The reverse diff (a documented command that no longer exists) is empty.
- **Components:** 85 `register_component::<` calls plus the hand-built faction-reputation descriptor make 86, matching the doc's "(86 components)" at both sites.

Every guard the skill names for these dimensions exists once, is not `#[ignore]`d, and passes. That covers the 27 Dim 1/2 guards, the Dim 3 identity and inventory guards, the Dim 4 durability and precedence guards, and the seven Dim 6 checklist guards.

**Skill text.** `audit-tooling/SKILL.md` said "192 at HEAD"; `terrain.patches` (84f04205f) made it 193. That one line was corrected in the skill file this pass.

## Executive Summary

| Severity | NEW | Existing (open) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 | 0 |
| LOW | 11 | 0 |

- **MEDIUM, TOOL-D3-01: VATS input never reaches extensions.** The SDK, the WIT `input-action` enum, `InputAction::parse` and the engine-to-SDK map `sdk_input_action` all advertise `vats`. But the one list the extension dispatch and the Papyrus input bindings iterate, `InputAction::OBSERVABLE`, has never contained `Vats`. An extension filtering on it, or a script calling `Input.GetMappedKey("VATS")`, gets nothing. Nothing guards the list against the map.
- **LOW, debug and protocol:**
  - `depth.dump` is a third hand-rolled confined writer, and the only one without the canonical-root check (D1-01).
  - The registry-completeness walk sees only `crates/core`, so scripting and physics state holders (`ActiveMagicEffects`, `Bleedout`, `ScriptVariables` and others) are unregistered (D2-01).
- **LOW, SDK and boot:**
  - The #7044 `stored` fold in the actor-value projection is untested (D3-02).
  - The renderer-option reader drops `--flag=value` silently (D4-01).
  - The profile expander's list of game-independent launches misses the godray, shaft and Cornell-reference labs (D4-02).
  - `--new-game` plus an explicit `--cell` does not pin the named cell (D4-03, PLAUSIBLE).
- **LOW, detection:**
  - The cell-form new-game target has no test, and three docs omit `new_game_cell` (D5-01).
  - Games-root precedence is four tiers, documented as three, and the missing-root message does not name `byro-detect --write` (D5-02).
- **LOW, texture-upscale:**
  - Its path check is the `Path::is_absolute` shape that #5016 and #7142 retired (D6-01, Windows-only effect).
  - It folds case ASCII-only (D6-02).
  - `discover` clobbers a hand-edited manifest non-atomically, and `--help` omits the manifest-trust note (D6-03).
- **INFO, not filed:**
  - `create_temp_file` does not assert its stem and extension are bare components. Every caller passes constants.
  - `sdk_input_action`'s doc says "the three spell actions have none", but `Wait` also returns `None`.
  - `terrain.patches amplitude|range` has no upper bound; it is a developer render knob.

## Findings

### MEDIUM

#### TOOL-D3-01: VATS press and release never reach extensions or the Papyrus input view, although every contract layer advertises `vats`
- **Severity**: MEDIUM. An advertised mod-facing contract is not delivered, silently.
- **Dimension**: SDK Contract
- **Exposure**: modder. It affects any extension whose manifest subscribes to `byro.input.action` and filters on `"vats"`, and any Papyrus `Input.GetMappedKey("VATS")` / `IsKeyPressed` path.
- **Location**: `byroredux/src/interaction.rs` `InputAction::OBSERVABLE` (`[Self; 14]`). It is consumed by `extension_input_dispatch_system` (`byroredux/src/extensions/systems.rs`) and by `papyrus_bindings` (`interaction.rs`).
- **Status**: NEW (sibling of #6528, closed, which fixed only the `parse` side)
- **Description**:
  - `InputAction::Vats` is bound to V and read by `systems/vats.rs`.
  - It maps to `SdkInputAction::Vats` in `sdk_input_action` (`extensions/mod.rs`), is in the WIT `input-action` enum, and parses from `"vats"`.
  - `OBSERVABLE` holds the 16 SDK actions minus `Pause` and `Vats`. The source documents why `Pause` is absent (it has no physical source); nothing says why `Vats` is.
  - 818eba7f4 added `Vats` to the map, the key binding and `papyrus_control_name`, but not to `OBSERVABLE`. Sneak (49c92afbc) and ReadyWeapon (533f1f644) were later added to it; Vats never was.
- **Evidence**: re-read by the coordinator. `OBSERVABLE` lists MoveForward…ReadyWeapon (14 actions) with no `Vats`, while `extensions/mod.rs` maps `InputAction::Vats => SdkInputAction::Vats`.
- **Impact**: an extension that waits for the player to open VATS never hears it, and a ported SKSE script's VATS control mapping reads as unbound.
- **Suggested Fix**: add `Self::Vats` to `OBSERVABLE`, or document why it is withheld. Add a test that every engine `InputAction` is either in `OBSERVABLE` or in `sdk_input_action`'s explicit `None` arm (or `Pause`), so a new action cannot compile in unobservable.

### LOW

#### TOOL-D1-01: `depth.dump` writes into `screenshots/` without the canonical-root check the other two confined writers make
- **Severity**: LOW
- **Dimension**: Debug Trust Boundary
- **Exposure**: any local process on a debug build or an opted-in release. The escape needs a pre-planted `screenshots` symlink in the engine's cwd.
- **Location**: `byroredux/src/commands/depth.rs` `DepthDumpCommand::execute` (ac558feb7); contrast `crates/debug-server/src/system.rs` `write_screenshot` and `byroredux/src/commands/assets.rs` `safe_tex_dump_path`
- **Status**: NEW
- **Description**: the name filter is sound (`*.pfm`, ASCII alphanumerics and `_-.`), and the write uses `create_new`. But it writes `Path::new("screenshots").join(name)` after only `create_dir_all`. The screenshot and `tex.dump` writers also canonicalise the output root and require it to sit inside the cwd, so a symlinked `screenshots/` is refused by them and followed by `depth.dump`. It is a third copy of the confinement logic.
- **Evidence**: coordinator re-read `depth.rs:75-76` against `system.rs:260-265` and `assets.rs:515-517`. Its test (`depth_dump_rejects_paths_and_unsupported_devices`) covers the name filter only.
- **Suggested Fix**: one shared "single filename under `<dir>/`, canonical root inside cwd, `create_new`" helper used by all three writers, plus a symlinked-root test (the `confinement_rejects_a_symlink_out_of_the_root` pattern).

#### TOOL-D2-01: the registry-completeness guard sees only `crates/core`, and scripting/physics state holders are unregistered
- **Severity**: LOW
- **Dimension**: Protocol & Registry
- **Exposure**: developer (`byro-dbg` component inspection)
- **Location**: `crates/debug-server/src/registration.rs` `every_inspectable_component_is_registered` (walks `crates/core/src` for the `inspect` derive)
- **Status**: NEW (the "remaining gap class" the skill names; #4755 and #5694 cover core only)
- **Description**: components outside core carry a `save`-feature serde derive instead, the route `SpellList` took to registration. A scan of the ~160 `impl Component` outside core finds these with that derive and no `register_component::<T>`:
  - `crates/scripting`: `ActiveMagicEffects`, `Bleedout`, `ReadiedSpell` (`cast.status` covers it), `ScriptVariables`, `TwoStateActivator` (`door.state` partly covers it), `WeaponDrawn`, `ActorControlState`, `ScriptTimer`, `ActorCinematicState`, `HorseTetherState`, `SpeakingLine`, `IgnoreCrime` and `ScriptPackage`;
  - `crates/physics`: `CharacterController`;
  - the demo: `RumbleOnActivate`.

  Nothing fails when one is added.
- **Evidence**: `ActiveMagicEffects` carries `cfg_attr(feature = "save", derive(serde::Serialize, …))`, like the registered `SpellList`, and has no `register_component::<ActiveMagicEffects>`. Which of them deserve registration is a judgement call.
- **Suggested Fix**: extend the roster scan to `crates/scripting/src` and `crates/physics/src` (serde derive in the `save` window plus `impl Component`), with an explicit allowlist for deliberate omissions (event batches, request markers, demo components). Register the state holders a developer inspects (`ActiveMagicEffects`, `Bleedout`, `ScriptVariables`, `WeaponDrawn`), and update the doc table and count.

#### TOOL-D3-02: the actor-value projection's #7044 `stored` fold has no test
- **Severity**: LOW
- **Dimension**: SDK Contract
- **Exposure**: an extension with `byro.actor-values.read`, on an Oblivion actor carrying stored level-up Health gains (save v136)
- **Location**: `byroredux/src/extensions/capture.rs` (folds `value.base + value.set_override + value.stored` into the SDK `base`); `byroredux/src/extensions/tests.rs` (the #5508 test)
- **Status**: NEW
- **Description**: the #5508 test covers the `set_override` layer only. Dropping `+ value.stored` would make the projection's `current()` fall short of the live `ActorValue::current()` with no test failing.
- **Suggested Fix**: extend the #5508 test with a stored Health gain, asserting that the projection's `current()` equals the live one, or project through `ActorValue::undamaged()` so the layers cannot drift.

#### TOOL-D4-01: the renderer-option reader silently ignores `--flag=value`
- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: a developer or a capture-script author
- **Location**: `byroredux/src/cli_args.rs` `parse_renderer_config` (its `option` closure)
- **Status**: NEW
- **Description**: the closure looks the flag up by exact match and returns `Ok(None)` on a miss. `--upscaler=taa`, `--fsr-quality=…`, `--tonemap=…` and every `--rt-test-*` value flag are therefore dropped with no diagnostic: `--upscaler=taa` silently runs FSR. `parse_string_arg` warns on the equals form for exactly this reason (`the_equals_form_is_reported_rather_than_silently_dropped`).
- **Evidence**: coordinator re-read `cli_args.rs:150-158`.
- **Suggested Fix**: on the `None` path, report the equals form (`equals_form_diagnostic`) as a warning or an error, with one test.

#### TOOL-D4-02: the profile expander does not know the godray, shaft and Cornell-reference labs are game-independent
- **Severity**: LOW (PLAUSIBLE; not run)
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: a developer whose per-user `profiles.toml` sets `[defaults].game` (the shipped file has no `[defaults]`)
- **Location**: `byroredux/src/boot/cli.rs` `expand_game_profile_args` (`has_other_load_flags` list) against `byroredux/src/scene.rs` `diagnostic_scene`
- **Status**: NEW
- **Description**: the list is hand-kept, and gained `--combustion-lab-steam` and `--recede-lab` this window. It still misses `--godray-lab*`, `--shaft-lab*` and `--cornell-reference[-view]`, which `diagnostic_scene` handles. So with a default game set, a bare `--shaft-lab` launch also gets that game's `--esm`, archives and `[defaults].cell`. The lab still wins the scene branch, but the boot does ESM and archive work it was meant to skip.
- **Evidence**: coordinator: `grep` of the `has_other_load_flags` window for `shaft-lab`, `godray-lab` and `cornell-reference` finds none.
- **Suggested Fix**: one shared predicate, `cornell::is_harness_launch(args)`, used by both `diagnostic_scene` and the expander, or a test that compares the two sets.

#### TOOL-D4-03: `--new-game` with an explicit `--cell` / `--grid` does not pin the named location (PLAUSIBLE)
- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: a developer or launcher user
- **Location**: `byroredux/src/boot/cli.rs` (new-game expansion: "explicit location flags override the profile target") against `byroredux/src/scene.rs` (`explicit_location = (cell|grid) && !new_game_flag`)
- **Status**: NEW
- **Description**: the expander keeps the user's location and says it wins. But since `--new-game` is no longer stripped, the scene computes the location as not explicit, so `NewGameStart` stays set and the chargen quest's `MoveTo` can relocate the player away from the cell the user named. The scene cannot tell a location injected from the profile from one the user typed. This may be intended for the profile case only.
- **Suggested Fix**: the expander drops `--new-game`'s pin-skip when an explicit location wins, or marks the profile injection so the scene pins every other location; add a test.

#### TOOL-D5-01: the cell-form new-game target is untested, and three docs omit `new_game_cell`
- **Severity**: LOW
- **Dimension**: Install Detection
- **Exposure**: developer and player. A typo in `new_game_cell` falls through to the empty scene silently.
- **Location**: `crates/core/src/ecs/game_profiles.rs` `has_new_game_target`; `crates/game-detect/src/profiles.rs` (its test asserts only the worldspace form); `byroredux/src/boot/cli.rs` (the cell arm); doc strings in `tools/byro-launcher/src/state.rs` ("Skyrim SE and New Vegas"), `crates/boot-request/src/lib.rs` and `docs/engine/launcher.md` (list only `new_game_worldspace` / `_grid` / `_radius`)
- **Status**: NEW
- **Description**: no test covers the cell arm or the shipped FNV, FO3 and Oblivion values. The reviewer verified the targets against the masters:
  - FNV: `GSDocMitchellHouse` is a CELL, and the cited `VCG00` / `VCG01PlayerStartMarkerREF` exist.
  - FO3: `Vault101d` is CELL 0x28138, and `CG00` / `CG00DadStartMarker` exist.
  - Oblivion: `ImperialDungeon01` is CELL 0x1FBB9, and `Charactergen` exists.
  - Skyrim SE: `Tamriel` 5,-24 was not checked against the master.
- **Suggested Fix**:
  - a test loading the shipped registry and asserting `has_new_game_target()` with the exact values for `fnv`, `fo3`, `oblivion` and `skyrim_se`;
  - an expander test for the cell arm;
  - fixes to the three doc strings.

#### TOOL-D5-02: games-root precedence is four tiers, documented as three, and the missing-root message does not name the fix
- **Severity**: LOW
- **Dimension**: Install Detection
- **Exposure**: a player on a non-developer machine running `--game <key>` without detection
- **Location**: `crates/game-detect/src/profiles.rs` `resolve_games_root` (doc); `byroredux/src/boot/cli.rs` (the `.or_else(defaults.games_root)` and the "resolved data dir does not exist" message); `docs/engine/launcher.md` ("`BYROREDUX_GAMES_ROOT` … read first and treated as authoritative")
- **Status**: NEW
- **Description**: the real order is `--games-root`, then `[defaults].games_root`, then `BYROREDUX_GAMES_ROOT`, then `DEFAULT_GAMES_ROOT` (one developer's path). That contradicts the launcher doc's claim that the env var is authoritative. On a machine with nothing detected, the message names the developer path and the env var, but not `byro-detect --write` or the launcher, which are the actual fix.
- **Suggested Fix**: state the four tiers in the doc comment and `launcher.md`, and append "or run `byro-detect --write`" to the message.

#### TOOL-D6-01: texture-upscale's asset-path check is the `is_absolute` shape #5016 and #7142 retired (PLAUSIBLE, Windows effect)
- **Severity**: LOW
- **Dimension**: Tool CLIs
- **Exposure**: an operator running `discover` on archives with crafted names, then `run`
- **Location**: `tools/texture-upscale/src/lib.rs` `validate_asset_path`, `output_png_path`
- **Status**: NEW
- **Description**: after normalisation it rejects only an absolute path and empty, `.` or `..` segments. A `c:` or `d:x` segment passes, and on Windows `PathBuf::push("c:")` replaces the output root. On Linux it is a literal `c:` directory. `byroredux_core::paths::is_bare_component` rejects `/`, `\` and `:` precisely for this.
- **Suggested Fix**: reject any segment that fails `is_bare_component` (the segments are already split), with tests for `c:/x` and `d:x`.

#### TOOL-D6-02: texture-upscale folds case ASCII-only while the archives and engine caches fold Unicode
- **Severity**: LOW (latent; real texture paths are ASCII)
- **Dimension**: Tool CLIs
- **Exposure**: a source stack mixing loose and archived non-ASCII paths
- **Location**: `tools/texture-upscale/src/lib.rs` `normalize_asset_path` (`to_ascii_lowercase`), which keys `source.rs` `AssetSource::open` / `SourceStack`
- **Status**: NEW (the #7168 / #7180 class; those fixed the engine only)
- **Description**: `Ä.dds` in one source and `ä.dds` in a later one stay distinct, so "later overrides earlier" does not happen and two manifest entries can result.
- **Suggested Fix**: fold through `byroredux_bsa::normalize_path` (the tool already depends on bsa) or `core::paths::fold_case`.

#### TOOL-D6-03: `discover` clobbers an existing hand-edited manifest non-atomically, and `--help` omits the manifest-trust note
- **Severity**: LOW
- **Dimension**: Tool CLIs
- **Exposure**: any operator
- **Location**: `tools/texture-upscale/src/cli.rs` (`Commands::Discover` → `save_manifest`), `lib.rs` `save_manifest` (`std::fs::write`), and the clap help for `run --manifest`
- **Status**: NEW
- **Description**: `run` refuses to overwrite outputs without `--overwrite`, but `discover` overwrites the manifest the README tells users to review and extend by hand, with a plain `fs::write`. The README states that `[upscaler]` is executed, trusted input, but `run --manifest`'s help says only "Reviewed texture-set manifest."
- **Suggested Fix**: refuse an existing manifest without `--overwrite` and write through `byroredux_core::atomic_file::atomic_write`; extend the `--manifest` help with "executes `[upscaler].program`; run only manifests you wrote or reviewed".

## Exposure Matrix

| Surface | Reachable by | Authenticated | Size-capped | Persistence atomic |
|---|---|---|---|---|
| Debug server (TCP 127.0.0.1:9876) | any local process (debug build, or release with `BYRO_DEBUG_SERVER=1`) | no | yes (16 MB frame, 64 queued, 8 clients, walk depth 64) | n/a |
| Debug file writes (screenshot, `tex.dump`, `depth.dump`) | same | no | per-file, count unbounded | `create_new`; `depth.dump` without the root check (D1-01) |
| SDK / extensions | modder | capability grants | yes (owned caps, #6984) | n/a; VATS action not delivered (D3-01) |
| Launcher boot request, settings, `[roots]` merge | launcher user | n/a | n/a | yes (`atomic_write`, clobber scan) |
| `byro-detect` | user | n/a | n/a | yes (`--write` through the merge) |
| texture-upscale | operator | manifest is trusted input | dims and space checked | outputs refuse clobber; manifest is not (D6-03) |

Suggested next step: `/audit-publish docs/audits/AUDIT_TOOLING_2026-11-21.md`. Labels:
- `tech-debt` on every finding;
- `test-gap` on D2-01, D3-02 and D5-01;
- `doc-rot` alongside on D5-01 and D5-02;
- `gameplay` on D3-01.
