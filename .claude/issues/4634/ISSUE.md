# NIFAL-D1-2026-09-21b-02: effect_shader_flags is now a four-way union but docs/tests still describe three contributors

**Issue**: #4634
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIFAL_2026-09-21b.md)

**Severity**: LOW
**Dimension**: Material
**Tier Violated**: no-fabrication (record-keeping)
**Game Affected**: all (FO4 in practice)
**Location**: `byroredux/src/material_translate.rs:522-527` (doc), `:670-674` ("All three contributors"), `:2946-2953` (`translate_material_unions_all_three_effect_shader_flag_contributors`); `docs/engine/asset-pipeline.md:374-386`; `docs/engine/nifal.md:618-640`.

## Description
`c0b740ce7` (#4548) made `effect_shader_flags` a four-way union by adding the `_msn`-name classifier (`material_translate.rs:593-601`), but every doc site and the test name still say three. Confirmed unchanged at HEAD.

## Suggested Fix
Update the function doc, the "All three contributors" prose, and the test name/doc to four contributors. Record the `_msn`-name rule and its census in `nifal.md` §Material and `asset-pipeline.md`.

## Source
docs/audits/AUDIT_NIFAL_2026-09-21b.md (NIFAL-D1-2026-09-21b-02)
