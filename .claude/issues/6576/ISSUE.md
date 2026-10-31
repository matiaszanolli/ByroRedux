# TOOL-D1-2026-10-31-02: `publish_ui_menus` does not know the new crafting page, so a workbench reports the Pause menu as open

Labels: low,tech-debt,bug

Source: `docs/audits/AUDIT_TOOLING_2026-10-31.md` (TOOL-D1-2026-10-31-02). Severity LOW. Dimension: Debug Trust Boundary (debug-ui `GameMenuPage` host seam). Exposure: player / modder (script menu-mode queries `GetPCMenuMode`/`IsPCMenuMode` kind 1013, `menu_open`).

**Location**: `byroredux/src/systems/script_queries.rs:32-48` (`publish_ui_menus`), `:55-70` (`menu_open`); page added by `851636a13` in `crates/debug-ui/src/lib.rs:316-324`

## Description
`publish_ui_menus` derives `pause` as `ui.game_menu_visible() && !(inventory || dialogue || message_box || barter)`. `851636a13` added `GameMenuPage::Crafting` (and `DebugUiState::crafting_menu_visible`, which has no caller), but the exclusion list and `UiMenuState` were not extended. With the crafting page up, `pause` is `true`, so `menu_open(1013)` ("Pause" menu) reports open while the player is at a bench. `menu_open(2)` and `menu_open(0)` are true either way, so only the specific Pause query is wrong. The engine has no mapped kind for the crafting page (a "menu the engine has no screen for is never open"), so the right outcome is `pause = false` and `other = true`, with an explicit `crafting` flag.

## Evidence
`script_queries.rs:38-46` omits `crafting`; `grep crafting_menu_visible byroredux/src` finds no caller.

## Impact
A script gating on "player is in the pause menu" (e.g. a quest stage waiting for the pause menu to close) misfires at crafting benches. Narrow; the crafting page is new and no vanilla script targets it.

## Suggested Fix
Add `crafting: ui.crafting_menu_visible()` to `UiMenuState`, exclude it from `pause`, include it in `other`. Add a test that publishes with each `open_*_menu` and asserts exactly one page flag.

Related: `851636a13`, `UiMenuState` in the save registry completeness list.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
