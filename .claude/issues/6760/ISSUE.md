# CHAR-2026-11-07-L-01: Most #5921 rule-row readers have no test that varies the profile independently of the game; kill-XP install has no test at all

**Issue**: #6760
**Labels**: bug, low, character, test-gap
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding L-01, Dim 1). Game: all (fo3, fnv, skyrim for the affected rules).

## Description
#5921 (`3162b21b8`) moved the encumbrance, barter, kill-XP, bleedout and perk-dialect tables onto profile rows. The profile table itself is pinned by value (`per_game_rule_rows_are_stated_on_every_profile`), but the consumers are not pinned the way the #4448 and #6567 consumers are. Each consumer's test builds an `EsmIndex` whose `game` and `character_rules` are a matching pair, so a regression back to `match index.game { .. }` would still pass. `install_experience_rules` has no test at all.

## Location
`byroredux/src/combat.rs:558-607` (`install_experience_rules`), `byroredux/src/encumbrance/tests.rs:6-20`, `crates/scripting/src/bleedout/tests.rs:171-205`, `crates/scripting/src/perk_entry_points/tests.rs:44-55`, `byroredux/src/barter/tests.rs:121-124`.

## Evidence
`git grep -n 'install_experience_rules('` returns the definition and the single caller `scene/scene_runtime.rs:226`. `encumbrance/tests.rs` `index(game)` maps game to profile 1:1 and `unreachable!`s on other games. Contrast `obscript_dialect_follows_the_profile_not_the_game_kind`, `fn586_condition_gate_is_profile_scoped_not_game_scoped`, `no_armor_row_means_no_model_and_no_book` and `rank_after_scripted_adds`, which build a pair where game and profile disagree (or vary only the profile).

## Impact
None today (the code reads the rows). The doctrine regression #5921 fixed, a new per-game branch in a consumer, is the one most likely to return, and these consumers cannot detect it.

## Related
#5921, #6567, #4448.

## Suggested Fix
One mismatched-index install test per consumer (e.g. `game: Skyrim` + `FALLOUT_NEW_VEGAS` profile asserting the profile's rule wins), a direct `install_experience_rules` test (FO3/FNV installs, Skyrim/FO4 removes a prior resource), optionally the `production_text` `GameKind::` scan from `armor.rs`.

## Completeness Checks
- [ ] **SIBLING**: all five #5921 consumers covered
- [ ] **TESTS**: each new test varies the profile independently of `GameKind`
