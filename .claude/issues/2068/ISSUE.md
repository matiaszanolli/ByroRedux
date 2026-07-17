# TD2-109: misc/world.rs's parse_acti/parse_term hand-roll the common EDID/FULL/MODL/SCRI(/VMAD) bundle instead of CommonNamedFields::from_subs

**GitHub Issue**: #2068
**Labels**: low,import-pipeline,legacy-compat,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `crates/plugin/src/esm/records/misc/world.rs:283-310,347-376` (`parse_acti`/`parse_term`) vs. `common.rs:268-296` (`CommonNamedFields::from_subs`)

## Description
`CommonNamedFields::from_subs` is used correctly at 9 other sites; `misc/world.rs` reimplements it byte-for-byte, including the VMAD decode call.

## Evidence
Confirmed live: `crates/plugin/src/esm/records/common.rs:272` defines `CommonNamedFields::from_subs`. `misc/world.rs::parse_acti` (line 283) and `parse_term` (line 347) each hand-roll a `match &sub.sub_type { b"EDID" => ..., b"FULL" => ..., b"MODL" => ..., b"SCRI" => ..., b"VMAD" => ScriptInstanceData::parse(...), ... }` loop instead of calling `CommonNamedFields::from_subs(subs)`; grep for `CommonNamedFields::from_subs` across `misc/*.rs` returns zero hits in `world.rs`.

## Suggested Fix
`let common = CommonNamedFields::from_subs(subs);` in both functions, keep only the ACTI/TERM-specific arms.

**Effort**: small

## Completeness Checks
- [ ] **SIBLING**: 9 other sites correctly use `CommonNamedFields::from_subs` — `parse_acti`/`parse_term` are the outliers
- [ ] **TESTS**: Existing `parse_acti_extracts_scri_and_model` / `parse_term_extracts_password_footer_menu` tests cover this path — purely mechanical swap
