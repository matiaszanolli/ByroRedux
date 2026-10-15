# OBL-D5-2026-10-14-01: the Oblivion branch of the `f67d82b3c` flip (GetQuestVariable evaluated, quest locals persisted, `is_modeled` game split) has no test

**Labels**: low,game:oblivion,legacy-compat,scripting,test-gap,bug
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D5-2026-10-14-01`. Re-verified against current code before filing.

- **Severity**: LOW (test gap; code behaves correctly on every probe)
- **Dimension**: 5 (routes the mechanism to `/audit-scripting`)
- **Location**: `byroredux_scripting::condition::{uses_obscript_quest_variables, is_modeled}`, the `ConditionFunction::GetQuestVariable` arm of `evaluate_function`, `obscript_quests::{ObScriptQuestLocals, run_quest_game_mode}`
- **Status**: NEW
- **Description**: `f67d82b3c` turns index 79 from "uncatalogued → fail-closed everywhere" into "evaluated against `ObScriptQuestLocals` when `ScriptGame` is Oblivion / Fallout3NV", and makes quest locals survive between GameMode runs. Neither is pinned. The only tests that mention fn 79 (`npc_spawn/ai_package.rs`) run with no `ScriptGame` and pin the pass-through branch (the opposite one). `run_quest_game_mode` has no test caller outside the system registration.
- **Evidence**: `grep -rn "ObScriptQuestLocals|uses_obscript_quest_variables|is_modeled" crates byroredux` finds definitions and uses, no `#[test]`; `grep -rn "function_index: 79"` gives two hits, both the no-game fixture. On real data 3 774 Oblivion INFO + 403 QUST conditions are fn 79 (all 4 177 `param_2` values match an SLSD index field); 1 011 of them (898 INFO + 113 QUST) are satisfied by an all-zero local, so wherever the variable is only written by a path the engine does not run (remote `set Quest.var` is declined per #5630, result scripts, object scripts) the gate now opens where it used to stay closed. That is the documented phase-2 limit, not a regression.
- **Impact**: a later refactor of `ScriptGame` install, `is_modeled` or the `of` / `store` seeding could silently return Oblivion / FNV dialogue and package gates to fail-closed (or Skyrim / FO4 to misread index 79) with every suite green.
- **Related**: `f67d82b3c`; `m42-eat-sleep.sh` is the only (device-bound, FNV) guard; save-side shape is covered by `saved_type_shape_changes_require_format_major_bump` (FORMAT 97).
- **Suggested Fix**: three small unit tests — (a) `evaluate` of a fn-79 condition under `ScriptGame(Oblivion)` with a seeded `ObScriptQuestLocals`, under `ScriptGame(Skyrim|Fallout4)` (passes), and with no resource (0.0); (b) `is_modeled` false for 79 under Skyrim / FO4 and true under Oblivion / FNV; (c) run a synthetic quest script twice through `run_quest_game_mode` asserting a `short` set on tick 1 is read on tick 2 (the `doonce` idiom).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, sibling parsers/tests)
- [ ] **TESTS**: A regression test pins this specific fix
