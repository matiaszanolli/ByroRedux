# NIF-D4-2026-10-27-02: `dark_role_has_one_writer_and_carries_its_census_doc` cannot fail on its doc half: it `include_str!`s its own file and its needles match themselves

Labels: low, nif-parser, nif, test-gap, bug
Source: docs/audits/AUDIT_NIF_2026-10-27.md

**Audit report**: `docs/audits/AUDIT_NIF_2026-10-27.md` -- finding `NIF-D4-2026-10-27-02` (LOW)
**Location (re-mapped to current code)**: `crates/nif/src/import/types/material.rs` (test `dark_role_has_one_writer_and_carries_its_census_doc`, ~:264-273; the doc it pins at ~:58-59). Premise re-read at HEAD: `let types_src = include_str!("material.rs")` still whole-file.
**Status at filing**: NEW (vacuous since 0faa0b107 / #4523, 2026-09-20; #6141 / 5559074be moved it from `types.rs` and kept the defect)

## Description
The test asserts `types_src.contains("#4523 census, 2026-09-20") && types_src.contains("8 meshes in 6 files")` where `types_src = include_str!("material.rs")`. That whole-file text includes the `#[cfg(test)] mod tests` that holds those same literals, so each needle occurs twice and deleting or rewording the census doc still passes. This is the #4604/#4842/#5164 class described in `.claude/commands/_audit-common.md` (source-scan guards must scan `source_scan::production_text`). The test's second half (`legacy_properties.rs` must contain `info.dark_map = ` exactly once) is sound.

## Evidence
A scripted scan of every `include_str!` + `.contains` / `.matches` pair in `crates/nif/src` found only these two needles with a whole-file count (2) above their production-only count (1).

## Impact
None at runtime. The pin that should force a census re-derivation when the dark-role doc changes is not enforced.

## Related
#4523, #4604, #4842, #5164, #6141. Not the REN-D1-01 shape (call-order); this one is self-matching needles.

## Suggested Fix
Scan `byroredux_core::source_scan::production_text(include_str!("material.rs"))` (already used in `crates/nif/src/import/{collision/mod.rs,walk/lights.rs}`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

