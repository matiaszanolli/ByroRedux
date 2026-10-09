# #5431: GAME-D5-2026-10-08-05: The new `GetButtonPressed` (CTDA fn 0) mapping rests on a false premise, and its doc comment swallowed `GetDistance`'s

**Labels**: low,gameplay,ai,scripting,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5431

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D5-2026-10-08-05` (HEAD `00f580e09`)

**Publish note**: Code shape re-verified at HEAD (fn-0 arm at `from_index`, `GetButtonPressed` inserted between `GetDistance`'s doc comment and the `GetDistance` variant). The ESM census numbers (zero fn-0 CTDAs in FO3/FNV, Sunny's gate = fn 79, which `from_index` still leaves `Unknown`) are taken from the audit's `/tmp/audit/gameplay/ctda_fn0_census.py` run and were not re-run here.

- **Severity**: LOW
- **Dimension**: Dim 5 (package conditions; evaluator owned by `/audit-scripting`)
- **Location**: `crates/scripting/src/condition.rs:83-89, 197-206, 711-717`
- **Status**: NEW
- **Description**: `from_index`'s comment says the FNV corpus authors fn 0 on Sunny Smiles' packages. The census says otherwise:
  - FalloutNV.esm and Fallout3.esm contain zero fn-0 CTDAs.
  - Sunny's gate is fn 79.
  - Skyrim.esm's 19 fn-0 CTDAs all sit on IDLE records (`PowerAttack`, `BashFail`, `WispLeftAttack`, …), combat predicates rather than a MessageBox query. They now evaluate to a constant -1 if IDLE conditions are ever evaluated.

  `GetButtonPressed` is a script-only function (the CS raw function list does not type it "Condition"). Separately, the variant was inserted between `GetDistance`'s doc comment and `GetDistance`. Rustdoc now attaches "GetDistance(target_form_id) → f32 … index 1" to `GetButtonPressed`, and `GetDistance` is left undocumented (the same defect class as #5269).
- **Suggested Fix**: Remove the fn-0 arm, or re-derive what fn 0 is from xEdit's table, and restore the `GetDistance` doc placement.

## Completeness Checks
- [ ] **SIBLING**: other from_index arms re-checked against xEdit's function table
- [ ] **TESTS**: A regression test pins this specific fix
