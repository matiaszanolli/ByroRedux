# TOOL-D3-2026-09-22-01: UI.IsMenuOpen always returns false for every real Bethesda menu name

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4757

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
