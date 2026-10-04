# Issues 4755–4760 — TOOLING audit batch (AUDIT_TOOLING_2026-09-22.md)

## #4755
[OPEN] TOOL-D2-2026-09-22-01: ActorValues/ActorVitals and five other core gameplay components are never registered with the debug-server component registry

## Description
`ActorValues` is, per its own module doc, the production store shared by every gameplay reader — the backing for `GetActorValue`, health/SPECIAL/skills. Both it and `ActorVitals` are real `SparseSetStorage` components with the `inspect` derive already present (`crates/core/src/ecs/components/actor_values.rs:67-170`, confirmed `impl Component for ActorVitals` at line 90 and `impl Component for ActorValues` at line 169) — simply never added to `register_all`. Confirmed via `grep -c 'register_component::<' crates/debug-server/src/registration.rs` = 42, and `grep -n 'ActorValues\|ActorVitals\|EquippedWeapon\|CreatureAttack\|Perks\|FactionReputation\|Dead' crates/debug-server/src/registration.rs` = no matches. All seven components independently confirmed to `impl Component`:
- `ActorValues`, `ActorVitals` — `crates/core/src/ecs/components/actor_values.rs:169,90`
- `EquippedWeapon` — `crates/core/src/ecs/components/inventory.rs:160`
- `Dead` — `crates/core/src/ecs/components/actor_state.rs:18`
- `CreatureAttack` — `crates/core/src/ecs/components/creature_attack.rs:45`
- `Perks`, `FactionReputation` — `crates/core/src/character/components.rs:132,255`

This is the same gap pattern #4063 (closed) fixed for the six AI-procedure `*Behavior` components, recurring for components more central to routine gameplay debugging than any AI procedure. The only existing access path is write-only-by-formid (`setav`/`modav`) or one-value-at-a-time (`cond`) — there is no generic dump, unlike `Inventory`/`EquipmentSlots` (explicitly wired for the M41 smoke test).

## Evidence
```
$ grep -c 'register_component::<' crates/debug-server/src/registration.rs
42
$ grep -n 'ActorValues\|ActorVitals\|EquippedWeapon\|CreatureAttack' crates/debug-server/src/registration.rs
(no matches)
```

## Impact
"Why is this NPC's health/skill/SPECIAL wrong" has no generic `byro-dbg` path (`inspect <id>` / `entities <Component>` cannot see them). The existing completeness guard (`roster_tests::every_ai_procedure_behavior_component_is_registered`) only derives its roster from `impl Component for <X>Behavior` and structurally cannot catch this class of gap.

## Related
Existing: #4063 (closed) — same gap pattern, different component family (six `*Behavior` components). This is a new instance, not a regression of #4063.

## Suggested Fix
Register the seven listed components in `register_all` (mirrors the M41 `Inventory`/`EquipmentSlots` precedent in the same function). Widen `roster_tests`'s guard from "every `*Behavior` type" to "every `impl Component` type carrying the `inspect` derive", so future gaps in this class fail a test instead of landing silently.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D2-2026-09-22-01)

## Completeness Checks
- [ ] **SIBLING**: All seven listed components registered together, not just `ActorValues`/`ActorVitals` — the same PR that fixes one should fix the whole class
- [ ] **TESTS**: The widened `roster_tests` guard (every `impl Component` carrying the `inspect` derive is registered) is added so this class of gap fails CI going forward


## #4756
[OPEN] TOOL-D2-2026-09-22-02: docs/engine/debug-cli.md's registration counts are stale, and 15 of 92 console commands have no documentation row

## Description
`docs/engine/debug-cli.md` carries two stale counts and 15 undocumented commands, confirmed at HEAD `c3f298a24`:
- Line 179 ("Currently registered (23 components)"), also repeated at lines 950 and 1155, vs. live `grep -c 'register_component::<' crates/debug-server/src/registration.rs` = **42**.
- Line 276 ("Current registered commands (**68**)") vs. live `grep -c 'registry.register(' byroredux/src/commands/mod.rs` = **92**.

Cross-referencing all 92 command names against the full text of the doc (not just the summary block) finds 15 with zero mention anywhere in the file — spot-checked 10 by exact `fn name(&self)` string against `grep -n <name> docs/engine/debug-cli.md`, all confirmed zero hits (`hardcore`, `exposure`, `tonemap`, `depth.stats`, `npc.appearance`, `ragdoll.status`, `sdk.compat`, `rt.masks`, `cell.owners`, `quest.effects`): the entire HUD family (`hud.on`, `hud.off`, `hud.values`, `hud.heading`, `hud.status`, `hud.debug`) plus those 9 (report lists a 15th, `hardcore`, already included above). `exposure` returns 5 hits in the doc, but all 5 are unrelated prose about the rendering concept ("regardless of exposure"), not a documented command row — confirmed by reading each hit. No documented command is missing from the registry (drift is one-directional).

## Evidence
```
$ grep -c 'register_component::<' crates/debug-server/src/registration.rs
42   (doc says 23, debug-cli.md:179,950,1155)
$ grep -c 'registry.register(' byroredux/src/commands/mod.rs
92   (doc says 68, debug-cli.md:276)
$ grep -n 'exposure' docs/engine/debug-cli.md
995: ...regardless of exposure)...   (prose, not a command row)
1044,1046,1049,1063: ...           (all prose)
```

## Impact
Discoverability only — `help` lists everything live — but a developer reading the doc to learn the surface will not find these 15 commands or the correct component-registry size.

## Related
None found. Similar doc-drift pattern to `TD4-2026-08-27-05`, `TD4-2026-09-05-01`, `SCR-ORCH-2026-09-06-01` in other docs this audit cycle (per the source report's own cross-reference), but this is a distinct file/finding.

## Suggested Fix
Regenerate both counts and add rows for the 15 missing commands; consider a doc-drift CI guard that asserts the doc mentions every `CommandRegistry`/`ComponentRegistry` name.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D2-2026-09-22-02)

## Completeness Checks
- [ ] **TESTS**: A doc-drift guard test (or CI check) asserts the doc's counts match `grep -c` against the live registries, so this doesn't silently re-drift


## #4757
[OPEN] TOOL-D3-2026-09-22-01: UI.IsMenuOpen always returns false for every real Bethesda menu name

## Description
`UiManager::menu_name` (`crates/ui/src/lib.rs:64,144`) is set from the on-disk movie path (`interface\hudmenu.swf`-shaped) on **both** the `--menu` archive route and the `--swf` dev route — checked both call sites directly:
- `byroredux/src/scene.rs:1292` (`--menu`, production route): `ui.load_swf_from_resource_provider(Arc::new(archive), menu_path, menu_path, None)` — both the `movie_path` and `name` parameters receive the same archive-relative `menu_path` string; `install_player(player, name)` then sets `self.menu_name = name.to_string()` (`crates/ui/src/lib.rs:144`).
- `byroredux/src/scene.rs:1363` (`--swf`, dev route): `ui.load_swf(&swf_data, swf_path)` — `name` = the CLI-supplied file path.

No canonical Bethesda-menu-name table exists anywhere in `crates/ui` — checked `catalog.rs`, `profile.rs`, and `lib.rs` directly (no `canonical`/`HUDMenu`/`InventoryMenu`/menu-name-table hits). The declared, dispatched SDK route `UI.IsMenuOpen("InventoryMenu")` (`crates/sdk/src/compatibility/input_ui.rs:142-149`, `adapt_papyrus_ui_is_menu_open`) compares a real Bethesda name against an archive/file path and can therefore never match for any real caller — the comparison function itself is correctly written; the bug is entirely in what gets fed into `snapshot.active_menu`.

This confirms and files the pointer `AUDIT_UI_2026-09-21.md` § 6 item 4 raised explicitly to `/audit-tooling`, from the SDK-bridge side (the render-side UI audit's own framing was distinct and did not file this).

## Evidence
```rust
// byroredux/src/app_events.rs:546-550
crate::extensions::extension_ui_menu_sync(&self.world, Some(ui.menu_name.as_str()), ui.visible);
```
```rust
// byroredux/src/scene.rs:1292 (--menu, production route)
ui.load_swf_from_resource_provider(Arc::new(archive), menu_path, menu_path, None)
```
```rust
// crates/ui/src/lib.rs:117-132
pub fn load_swf_from_resource_provider(&mut self, provider: ..., movie_path: &str, name: &str, ...) {
    let player = SwfPlayer::from_resource_provider(provider, movie_path, ...)?;
    self.install_player(player, name);   // name == movie_path at both call sites
}
// crates/ui/src/lib.rs:144
self.menu_name = name.to_string();
```

## Impact
Zero real-world blast radius today — no shipped script relies on it, and the UI audit already scoped this under "Pending-Row Readiness" — but the function is fully wired end-to-end (declared, routed, dispatched, unit-tested with synthetic names) and would silently misbehave for every real caller the moment a compat script uses `UI.IsMenuOpen` with a real Bethesda menu name. Other SDK bridge call sites checked for the same pattern: the `Game.Get*Mod*` adapters read real plugin filenames from `ContentCatalog`, and the `Input` adapters compare against the real action-binding table — no other instance found.

## Related
- `AUDIT_UI_2026-09-21.md` § 6 item 4 — the originating pointer to `/audit-tooling`.
- No existing GitHub issue discusses `UiManager::menu_name` or `IsMenuOpen` (checked `/tmp/audit/issues.json` and the live open-issue list by keyword; none match).

## Suggested Fix
Add a vanilla-menu-name lookup (SWF basename, case-insensitive, minus extension → canonical Bethesda name) at the `UiManager`/`install_player` boundary or in `extension_ui_menu_sync` before constructing the snapshot; add a test that loads a real archive menu path and asserts `UI.IsMenuOpen("HUDMenu")` resolves `true`.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D3-2026-09-22-01)

## Completeness Checks
- [ ] **SIBLING**: Both call sites (`--menu` archive route and `--swf` dev route) fixed together, not just one
- [ ] **TESTS**: A test loads a real archive menu path and asserts `UI.IsMenuOpen("HUDMenu")` (or equivalent) resolves `true`


## #4758
[OPEN] TOOL-D4-2026-09-22-01: BootRequest::save and RootOverrides::merge_into_file still use bare fs::write, not the project's atomic-write doctrine

## Description
`atomic_write` (temp → fsync → read-back → rename → parent fsync, `crates/core/src/atomic_file.rs:38`) is the project's documented durability contract; `settings-io` was moved onto it by #3472 specifically because "there is no reason for two writers in one binary to have two different durability contracts." Two more writers remain un-migrated, confirmed at HEAD:
- `BootRequest::save` (`crates/boot-request/src/lib.rs:293-309`) — writes the launcher→engine `boot.toml` handoff via bare `fs::write(path, text)` (line 307).
- `RootOverrides::merge_into_file` (`crates/game-detect/src/overrides.rs:96-136`) — rewrites the user's hand-edited `~/.byroredux/profiles.toml`, merge-preserving hand-authored `[profiles.*]`/`[defaults]` blocks per its own doc comment, via bare `std::fs::write(path, text)` (line 133).

`boot-request` deliberately has no `byroredux-core` dependency — confirmed by reading `crates/boot-request/Cargo.toml`'s own comment ("Deliberately dependency-free beyond serde + toml... anything heavier here would leak a GPU/engine dependency into the launcher") and its `[dependencies]` block (`serde`, `toml`, `thiserror` only) — so it cannot call `atomic_write` without a dependency decision.

## Evidence
```rust
// crates/boot-request/src/lib.rs:307
fs::write(path, text).map_err(|source| BootRequestError::Write { ... })
```
```rust
// crates/game-detect/src/overrides.rs:133
std::fs::write(path, text).map_err(|source| OverrideError::Write { ... })
```

## Impact
A crash/power-loss mid-write leaves a truncated file. For `boot.toml`, low value at risk (the launcher regenerates it fresh every `Play`, so a torn file only fails the *next* attempt with a clear parse error). For `profiles.toml`, the file carries hand-authored user content that a torn `byro-detect --write` could destroy alongside the freshly-detected `[roots]` table — this is the higher-value path.

## Related
#3472 (closed) fixed this exact class for `settings-io`; its own rename-failure fallback (`fs::write` over the destination on Windows rename failure) is separately, deliberately non-atomic per an in-place comment — not part of this finding.

## Suggested Fix
Add `byroredux-core` as a dependency of `boot-request` and reuse `atomic_write` directly, or extract it into a dependency-light shared crate both `boot-request` and `game-detect` can use.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D4-2026-09-22-01)

## Completeness Checks
- [ ] **SIBLING**: Both writers (`BootRequest::save` and `RootOverrides::merge_into_file`) migrated together, given they share the same root cause and fix shape
- [ ] **TESTS**: A regression test simulates a torn write (or at minimum asserts the new path uses `atomic_write`) for both call sites


## #4759
[OPEN] TOOL-D5-2026-09-22-01: game-detect validate() never structurally checks the main ESM plugin, unlike the archive check

## Description
`crates/game-detect/src/validate.rs`'s archive-validation loop (`:165-206`, `archive_opens` at `:100-114`) explicitly reasons that "a truncated download or a mod manager that mangled the file... the engine would fail mid-load rather than at startup" and calls the real archive header parser (`BsaArchive::open`/`Ba2Archive::open`) accordingly, reporting `Severity::Fail` on a parse error. The main-plugin check (`:143-163`) applies none of that reasoning: confirmed by reading the full branch — `esm.is_file()` plus a byte-count-as-MB informational `Severity::Ok` line (`:157-162`), with no attempt to open or structurally validate the ESM. Even the crate's own test fixtures never exercise a real header here — every fixture writes the literal 4 bytes `b"TES4"` with no further structure (not independently re-verified byte-for-byte in this pass, but the validate.rs code path itself confirms no parse call exists to exercise). `crates/plugin/src/esm/reader.rs` has a real TES4/record header parser available (`read_record_header`, per-game `EsmVariant`/`record_header_size`), and `game-detect`'s `Cargo.toml` has no `byroredux-plugin` dependency today — this is a wiring gap, not a missing capability.

## Evidence
```rust
// crates/game-detect/src/validate.rs:157-162 — size only, never opened
let size_mb = esm.metadata().map(|m| m.len()).unwrap_or(0) / (1024 * 1024);
checks.push(Check::new(Severity::Ok, "Main plugin", format!("{} ({size_mb} MB)", entry.esm)));
```
```rust
// crates/game-detect/src/validate.rs:100-114 (archive branch, contrast)
fn archive_opens(path: &Path) -> Result<(), String> {
    ...
    "ba2" => Ba2Archive::open(path).map(|_| ()).map_err(|e| e.to_string()),
    _ => BsaArchive::open(path).map(|_| ()).map_err(|e| e.to_string()),
```

## Impact
`byro-detect`/the launcher pre-flight reports a broken install as "ready" when the corruption is in the ESM rather than an archive, defeating the tool's stated purpose.

## Related
None found. Distinct from #4673 (open — game-detect VDF recursion / ACF installdir containment), which is about Steam-manifest parsing, not main-plugin structural validation.

## Suggested Fix
Call the existing ESM header parser (or at minimum check the leading `TES4`/`TES3` magic and a sane minimum size) in the main-plugin branch, mirroring the archive branch's `Severity::Fail` on parse error.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D5-2026-09-22-01)

## Completeness Checks
- [ ] **TESTS**: A test fixture with a truncated/corrupt (but existent, correctly-sized-on-disk) ESM asserts `validate()` reports `Severity::Fail`, mirroring the existing archive-corruption test


## #4760
[OPEN] TOOL-D5-2026-09-22-02: BYROREDUX_SKYRIM_DATA (not canonical BYROREDUX_SKYRIMSE_DATA) is still read by five Rust files and shell scripts

## Description
The canonical Skyrim SE data-dir env var is `BYROREDUX_SKYRIMSE_DATA` (`crates/plugin/src/esm/test_paths.rs:83`, `SKYRIM_SE_ENV`). `BYROREDUX_SKYRIM_DATA` (missing the `SE`) is still the only variable read at five Rust sites, confirmed via `grep -rln 'BYROREDUX_SKYRIM_DATA\b'`: `byroredux/src/asset_provider/animation.rs`, `byroredux/src/npc_spawn/ai_package.rs`, `crates/hkx/src/animation.rs`, `crates/scripting/examples/mq101_conformance.rs`, and `crates/ui/tests/hudmenu_protocol.rs` (5 files total, matching the report's count), plus a double-digit number of `docs/smoke-tests/*.sh` / `scripts/*.sh` shell scripts. Every site checked is opt-in test/tooling code — `#[ignore]`-gated `#[test]` functions or a standalone `cargo run --example` probe / shell smoke scripts — never the running engine or a player-facing code path.

## Evidence
```
$ grep -rln 'BYROREDUX_SKYRIM_DATA\b' --include='*.rs' .
byroredux/src/asset_provider/animation.rs
byroredux/src/npc_spawn/ai_package.rs
crates/scripting/examples/mq101_conformance.rs
crates/ui/tests/hudmenu_protocol.rs
crates/hkx/src/animation.rs
```
```rust
// crates/plugin/src/esm/test_paths.rs:83 (canonical)
pub const SKYRIM_SE_ENV: &str = "BYROREDUX_SKYRIMSE_DATA";
```

## Impact
Confined to opt-in developer workflows; never a player or production-engine defect. A developer who configures the documented canonical `BYROREDUX_SKYRIMSE_DATA` will find these specific opt-in tests/scripts silently fall back to a hardcoded path that only resolves on the maintainer's own dev machine.

## Related
#3741 (closed) addressed helper visibility, not this spelling split — not a regression of it.

## Suggested Fix
Rename all five `.rs` sites (ideally by importing `SKYRIM_SE_ENV` instead of a string literal, so the existing completeness guard covers them) and the shell scripts that reference the non-canonical name.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D5-2026-09-22-02)

## Completeness Checks
- [ ] **SIBLING**: All five `.rs` sites and the shell scripts renamed together, not just one, so the split doesn't partially persist
- [ ] **TESTS**: Import `SKYRIM_SE_ENV` instead of a string literal at each `.rs` site so the existing completeness guard covers them going forward


