# SAFE-2026-08-03-03: Stale field-count in MaterialTable::intern's collision-policy comment

Severity: low
Source audit: docs/audits/AUDIT_SAFETY_2026-08-03.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2273

**Dimension**: 6 (R1 Material Table Layout Soundness) / doc-rot
**Source**: `docs/audits/AUDIT_SAFETY_2026-08-03.md` (SAFE-2026-08-03-03)
**Status**: NEW
**Location**: `crates/renderer/src/vulkan/material.rs:1143` (comment above
`MaterialTable`'s collision-policy), field list at `material.rs:76-291`

## Description
The doc comment describing hash-collision odds reads "rare on FxHash's 64-bit
output over 75 scalar fields, #1368" — but `GpuMaterial` has carried 87 fields
(348 bytes) since the 2026-07-27 growth that added twelve supplemental
texture-role indices. The size/offset pins themselves
(`gpu_material_size_is_348_bytes`, `gpu_material_field_offsets_match_shader_contract`)
are correct and up to date — only this prose comment drifted. Not a functional
bug; this is a documentation-only fix analogous to the closed #2132/#2133
SKILL-doc-drift findings from the prior audit pass.

## Evidence
```
crates/renderer/src/vulkan/material.rs:1143   "(rare on FxHash's 64-bit output over 75 scalar fields, #1368)"
crates/renderer/src/vulkan/material.rs:76-291  GpuMaterial struct — 87 pub fields, #[repr(C)], 348 bytes
```
`gpu_material_size_is_348_bytes` and `gpu_material_field_offsets_match_shader_contract`
(same file) both currently pass, confirming 348 B / 87-field layout is the live
contract — only the "75 scalar fields" prose is stale.

## Impact
Cosmetic; no functional effect. Flagged per the repo's path/symbol-reference
hygiene convention — stale numbers in load-bearing comments are worth catching
even when harmless, since a future reader could miscount collision probability
using the wrong field count.

## Suggested Fix
Update the comment to 87 fields, or drop the specific count so it can't go stale
again on the next field addition (e.g. reference `size_of::<GpuMaterial>()` by
name rather than restating the field count in prose).

## Related
None — no open issue overlaps this finding (checked against 47 open issues,
`/tmp/audit/issues.json`). Same class as closed #2132/#2133 (stale-citation
doc-rot from the prior `AUDIT_SAFETY_2026-07-25` pass).

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix, no code path affected
