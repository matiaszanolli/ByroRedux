# UI-D7-2026-09-21-05: prefix literal duplicated, stale command descriptions, per-game choices outside the profile tables

**Issue**: #4725
**Severity**: LOW
**Labels**: low,ui,tech-debt,bug

## Description
Five small maintainability issues in the HUD driver code, grouped as one finding:
1. `byroredux/src/scaleform_hud.rs:403` re-types the literal `"__byro"` instead of importing and using the exported `byroredux_ui::ENGINE_NAME_PREFIX` (`crates/ui/src/host.rs:73`) — a drift risk if the prefix ever changes.
2. `hud.on`/`hud.off`/`hud.status` (`byroredux/src/commands/hud.rs`) describe themselves as "MenuXml HUD" in their `description()` text, but have driven both the MenuXml and Scaleform backends since `62fc0bf22`.
3. `byroredux/src/hud.rs:158` picks the HUD's default texture archive by comparing the display label (`label == "Fallout 3"`) rather than a data field on the profile.
4. `byroredux/src/scaleform_hud.rs:265` registers the `updateStats`/`RequestPlayerInfo` poll handlers under an `if game == ScaleformGame::Skyrim` branch inside the launch function, instead of as data on `ScaleformGame` itself.
5. `crates/ui/tests/fallout4_hudmenu_protocol.rs:36` builds an unused `ScaleformHostBridge` (the UI test build's only compiler warning) and passes `Some(Fallout4Avm2)` where production passes `None`.

## Evidence
Verified at HEAD `ee6d3fb39` — all five sites present as described:
- `scaleform_hud.rs`: `if name.starts_with("__byro")` (literal, not `ENGINE_NAME_PREFIX`).
- `commands/hud.rs`: `"Show the MenuXml HUD overlay"` / `"Hide the MenuXml HUD overlay"` / `"Report the MenuXml HUD state"`.
- `hud.rs`: `default_textures_bsa: if label == "Fallout 3" { ... }`.
- `scaleform_hud.rs`: `if game == ScaleformGame::Skyrim { ... bridge.set_response_handler("updateStats", ...) ... }`.

## Impact
Maintainability only — no runtime behavior is incorrect. Left as-is, these are the kind of drift source that eventually produces a real bug (e.g. the prefix literal silently diverging from `ENGINE_NAME_PREFIX`, or a per-game `if` growing unbounded as more profile-specific poll handlers are added).

## Related
- CHAR-2026-09-21-D1-02 — the same "per-game choice belongs on the profile type, not an inline `if`" shape, in the native HUD's `GameKind` pool roster.
- UI-D7-2026-09-21-03 (this report) — item 3 above (`label == "Fallout 3"`) is the same comparison pattern implicated in that bug.

## Suggested Fix
Use `byroredux_ui::ENGINE_NAME_PREFIX` in `scaleform_hud.rs`. Reword the three `hud.*` command descriptions to name both backends. Move the texture-archive choice and the poll-handler set onto `ScaleformGame`/`HudGameProfile` as data. Delete the unused `ScaleformHostBridge` in the FO4 protocol pin test and pass `None` to match production.

## Completeness Checks
- [ ] **SIBLING**: Grep for other `"__byro"` literals that should use `ENGINE_NAME_PREFIX`
- [ ] **TESTS**: The FO4 protocol pin's unused-bridge warning is gone after the cleanup (compiler check, not a new test)

Source: docs/audits/AUDIT_UI_2026-09-21.md (UI-D7-2026-09-21-05)
