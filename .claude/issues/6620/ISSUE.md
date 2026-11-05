# PRDR2-2026-11-03-02: `MenuMode 1008` never answers on Oblivion: `menu_open` reads the FO3/FNV menu codes for every game

Labels: bug,low,scripting,game:oblivion,ui

**Severity**: LOW (one menu code answers wrong on one game)
**Dimension**: Scripting / UI
**Source**: found during the game-ui-integration PRD review round 2 (2026-11-03, rev 3), re-verified at HEAD `eba979dfb`. Sibling of #6576 (crafting page reports Pause), same function family.
**Status**: NEW

## Location
`byroredux/src/systems/script_queries.rs:55-73` (`menu_open`, the `MenuMode <n>` answer):

```rust
match kind {
    ...
    1013 => ui.pause,
    1053 => barter,
    _ => false,
}
```

## Description
`menu_open` answers the FO3/FNV GECK *MenuTypes* codes for every game. On Oblivion the barter/container screen is code **1008**: xEdit `wbMenuModeEnum` (`wbDefinitionsCommon.pas:7861`, entry `1008, IsTES4('Container/Barter', …)`), and the CS wiki uses `MenuMode 1008` for the inventory/container screen (`cs-uesp-wiki/(main)/OnActivate.wiki:18`). So `MenuMode 1008` is always false on Oblivion while barter is open, and `MenuMode 1053` is answered with FNV's meaning.

## Impact
Oblivion scripts that gate on `MenuMode 1008` (merchant and container scripts) never see the menu open.

## Suggested Fix
Make the code table per game, read from the same source the parser already uses for menu codes (xEdit `wbMenuModeEnum` is game-conditional), at the translation boundary, not as a branch in the query. The game-ui-integration PRD (`docs/prd/game-ui-integration.md`, GUI-REG-002 / GUI-REG-004 per-game menu tables) owns the full table; this issue is only the 1008 answer.

## Completeness Checks
- [ ] **OBLIVION**: `MenuMode 1008` true while the barter/container screen is open on Oblivion
- [ ] **FNV_UNCHANGED**: FO3/FNV codes answer as today (existing `menu_open` tests stay green)
- [ ] **NO_GAME_BRANCH**: the code table is data selected once per game, not a `match` on game kind inside `menu_open`
- [ ] **TESTS**: unit test for 1008 on an Oblivion-profile world

