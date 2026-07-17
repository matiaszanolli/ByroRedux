# TD2-110: EFID/EFIT to MagicEffectItem decode duplicated verbatim between parse_spel and parse_ench

**GitHub Issue**: #2069
**Labels**: low,import-pipeline,legacy-compat,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `crates/plugin/src/esm/records/misc/magic.rs:522-544,680-702` (`parse_spel`/`parse_ench`)

## Description
Both implement an identical ~20-line manual EFID/EFIT decode, despite the same file having a schema-decoder convention used immediately adjacent.

## Evidence
Confirmed live: `parse_spel` (line 506) and `parse_ench` (line 661) both contain matching `b"EFID" if sub.data.len() >= 4 => { current_efid = ... }` / `b"EFIT" if sub.data.len() >= 12 && current_efid != 0 => { ... push MagicEffectItem ... }` arms at the claimed line ranges.

## Suggested Fix
Extract `accumulate_efid_efit()`, call from both.

**Effort**: small

## Completeness Checks
- [ ] **TESTS**: Existing `parse_spel_with_two_effects` / `parse_ench_with_one_effect` / `parse_spel_efit_without_efid_is_skipped` tests cover both call sites — purely mechanical extraction
