# #7187 — TOOL-D4-2026-11-21-03: `--new-game` with an explicit `--cell` / `--grid` does not pin the named location (PLAUSIBLE)

Labels: low,tech-debt,bug

- **Severity**: LOW
- **Dimension**: Boot Handoff & Persistence
- **Exposure**: a developer or launcher user
- **Location**: `byroredux/src/boot/cli.rs` (new-game expansion: "explicit location flags override the profile target") against `byroredux/src/scene.rs` (`explicit_location = (cell|grid) && !new_game_flag`)
- **Status**: NEW
- **Description**: the expander keeps the user's location and says it wins. But since `--new-game` is no longer stripped, the scene computes the location as not explicit, so `NewGameStart` stays set and the chargen quest's `MoveTo` can relocate the player away from the cell the user named. The scene cannot tell a location injected from the profile from one the user typed. This may be intended for the profile case only.
- **Suggested Fix**: the expander drops `--new-game`'s pin-skip when an explicit location wins, or marks the profile injection so the scene pins every other location; add a test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / readers / folds named above)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_TOOLING_2026-11-21.md`
