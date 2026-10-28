# RT-10: Baselines README schema block lacks `light_count_spot`

Labels: low, tech-debt, documentation, doc-rot
Source: docs/audits/AUDIT_RUNTIME_2026-10-27.md

**Report**: `docs/audits/AUDIT_RUNTIME_2026-10-27.md` · **Finding**: RT-10 · **Severity**: LOW (doc-rot) · **Dimension**: Harness and baseline integrity
**Location**: `.claude/audit-baselines/runtime/README.md:22-46` (the `## Schema` sample block)

## Description
The README's schema block says "The key set above mirrors the committed TSVs exactly", but the sample lacks `light_count_spot`. Stale since `aa075ed35` (#5132), which added the row to all five TSVs, `REQUIRED_METRICS` and the skill, but not to the README. (Verified at HEAD: all five TSVs carry `light_count_spot`; the README block lists `light_count_point` and `light_count_directional` only.)

## Evidence
`grep light_count_spot .claude/audit-baselines/runtime/*.tsv` hits all five files; the README sample block (lines 22-46) has no such row.

## Impact
Documentation only: someone hand-authoring a TSV from the README would fail the schema check.

## Related
#5132 (closed), `aa075ed35`.

## Suggested Fix
Add `light_count_spot` to the sample block (between `light_count_point` and `light_count_directional`, matching TSV row order).

## Completeness Checks
- [ ] **SIBLING**: README prose around the schema (row descriptions) checked for other missing rows

