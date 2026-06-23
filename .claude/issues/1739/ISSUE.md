# SCR-D5-02: fragment lowerer (effects.rs) implemented but unwired

Filed as: matiaszanolli/ByroRedux#1739
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness
- **Location**: `crates/scripting/src/translate/effects.rs` vs `translate/mod.rs:34-39` (`RECOGNIZERS`)
- **Labels**: low, legacy-compat, tech-debt, bug

## Description
`lower_fragment` + `EFFECT_PRIMITIVES` are complete and tested but no `RECOGNIZERS` entry calls them, so no decompiled quest-fragment `.pex` (69.5% of corpus) is lowered via the live boundary — reachable only from its own tests. Designed Phase-3 gap; the feature is dead from the engine today.

## Suggested Fix
Wire a fragment recognizer into `RECOGNIZERS` when it lands; until then a doc note that the table is staged-not-wired.
