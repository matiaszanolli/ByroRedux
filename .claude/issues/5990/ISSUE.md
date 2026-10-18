# SCR-D3-2026-10-17-02: The CTDA catalog is game-agnostic for six more colliding indices; `is_modeled` only special-cases fn 79

Issue: #5990
Labels: high, scripting, bug, game:fo3, game:skyrim, game:fo4

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-02: The CTDA catalog is game-agnostic for six more colliding indices; `is_modeled` only special-cases fn 79

- **Severity**: HIGH
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/condition/catalog.rs:296,298,299,300,303,340` (rows for 449, 533, 550, 555, 630, 403);
  `crates/scripting/src/condition/eval.rs:69-75` (`is_modeled`) and `:36-43` (the only per-game branch, fn 79)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `ConditionFunction::from_index` maps an index to one function regardless of game; only `GetQuestVariable` (79) is gated by game
  (`uses_obscript_quest_variables`). xEdit (`wbDefinitions*.pas`) shows these catalog indices are a *different function* on other games:
  | idx | catalog (where valid) | other game(s) | evaluated as |
  |---|---|---|---|
  | 403 | `GetRelationshipRank` (Skyrim/FO4) | FO3/FNV `HasFriendDisposition` | GetRelationshipRank(actor 0) |
  | 449 | `HasPerk` (FO3/FNV) | Skyrim/FO4 `GetFactionRelation(ptActor)` | HasPerk(actor FormID) = 0 |
  | 533 | `GetXPForNextLevel` (FNV) | Skyrim/FO4 `GetInfamy` | ruleset XP |
  | 550 | `IsSceneActionComplete` (Skyrim/FO4) | FO3/FNV `IsGoreDisabled` | scene history |
  | 555 | `HasLoaded3D` (Skyrim/FO4) | FO3/FNV `GetSpellUsageNum` | GlobalTransform probe |
  | 630 | `GetVMScriptVariable` (Skyrim) | FO4 `GetCombatAudioDetection` (FO4's is 660) | VM script variable |
  (`FO4.pas:4087`, `FO3.pas:2324/2375/2376`, `TES5.pas:3310/3349`.)
- **Evidence**: vanilla CTDA census - Fallout3.esm fn 403 x8 (all INFO: 0x1d7ac, 0x1d7ad, 0x1fda2, 0x1fda3, 0x1fda5, 0x1fda6, 0x8726f, 0x1e374;
  comparators `== 1` / `== 0`, no parameter); Skyrim.esm fn 449 x21 (INFO 16, QUST 5); Fallout4.esm fn 449 x35 (QUST 29, INFO 3, ENCH 2, SCEN 1),
  fn 630 x25 (INFO). The evaluator computes a real-looking value for each (e.g. `get_relationship_rank` returns 0 for a null actor -> a
  `HasFriendDisposition == 0` FO3 line passes for everyone, the `== 1` twin never does), and `is_modeled` is `true` for them, so the
  fail-closed gates added by #5380/#5376/#5734/bd85de5ea accept the result.
- **Impact**: wrong dialogue variant selection on FO3, wrong quest-level INFO gating (`#5734`) on Skyrim/FO4 through fn 449, and a wrong VM
  variable read on FO4 INFOs (fn 630) - the exact class the fn-79 special case was added to stop. Escalated to HIGH under the skill rule (an
  intrusive consumer accepts a term the evaluator does not actually model for the loaded game).
- **Related**: `f67d82b3c` (fn 79 game gate - the precedent), #5380, #5752 (closed by bd85de5ea), `condition.rs` doc ("180/610 stay unmapped").
- **Suggested Fix**: give each `ROWS` row a game set (`valid_for`) next to the existing 79 branch and have both `evaluate_condition` and `is_modeled`
  treat a collision on the wrong game as `Unknown`; add a drift test that walks `CATALOG` against a checked-in per-game index-name table.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
