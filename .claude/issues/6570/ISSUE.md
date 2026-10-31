# SCR-2026-10-31-01: CTDA 659 `EPTemperingItemIsEnchanted` is modeled in every game, but Fallout 4 authors 659 as `HasVMScript`

Labels: low,scripting,bug,game:fo4

Source: `docs/audits/AUDIT_SCRIPTING_2026-10-31.md` (SCR-2026-10-31-01). Severity LOW. Dimension: Event Runtime (condition catalog). Introduced by the tempering commits `2c55546f4` / `31aca901d`.

**Location**: `crates/scripting/src/condition/catalog.rs:475` (row), `:620` (`FOREIGN_INDICES`, no 659 entry), `crates/scripting/src/condition/eval.rs:95` (`is_modeled`: `_ => true`), `:595` (arm)

## Description
- xEdit `wbDefinitionsTES5.pas:3435`: `(Index: 659; Name: 'EPTemperingItemIsEnchanted')`. `wbDefinitionsFO4.pas:4110`: `(Index: 659; Name: 'HasVMScript'; ParamType1: ptString)`.
- The row is Skyrim's but is not listed in `FOREIGN_INDICES`, which exists for exactly this (#5990: an index another game reuses is unmodeled there). On an FO4 load order `function_of` returns `EpTemperingItemIsEnchanted`, `is_modeled` says true, and the evaluator answers `TemperingItem.enchanted`, always `false` outside the Skyrim improve action.
- Every fail-closed gate (dialogue INFO / quest gates, Story Manager nodes, perk entries, scene starts, quest-log entries) therefore evaluates a `HasVMScript` term as 0.0 instead of declining. This is the only row added since the table without an entry. FO76 and Starfield were not read.

## Evidence
Raw CTDA census of `Fallout4.esm` (101 882 conditions): exactly two use function 659, one `QUST` and one `PERK`. DLC masters were not scanned.

## Impact
Small today: one quest condition and one perk condition on FO4 wrongly evaluated as "HasVMScript == 0"; any FO4 mod using `HasVMScript` is affected.

## Suggested Fix
Add `(659, &[GameKind::Fallout4])` to `FOREIGN_INDICES` with a `| 659 | EPTemperingItemIsEnchanted (Skyrim) | FO4: HasVMScript |` doc row. Extend the real-data foreign-index test with the pair.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
