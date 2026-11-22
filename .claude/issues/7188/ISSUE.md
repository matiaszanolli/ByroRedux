# #7188 — TOOL-D5-2026-11-21-01: the cell-form new-game target is untested, and three docs omit `new_game_cell`

Labels: low,tech-debt,bug,test-gap,doc-rot

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / readers / folds named above)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_TOOLING_2026-11-21.md`
