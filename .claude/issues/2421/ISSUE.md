# TD4-002: _audit-common.md disagrees with itself on the shader count — '21 GLSL sources' vs. 'All 19 shaders'

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2421
**Finding ID**: TD4-002 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 4 — Audit-Finding Rot
**Location**: `.claude/commands/_audit-common.md:63` vs. `:104`
**Status**: NEW

## Description
Same file, three lines apart in different tables, two different numbers for the same shader set. `ls crates/renderer/shaders/*.{vert,frag,comp}` confirms 21, matching line 63's explicit list; line 104 undercounts by 2 and doesn't correspond to any count `shader-pipeline.md` itself asserts.

## Related
Same self-inconsistency class as closed #2194 (table omission vs. this being a table undercounted total).

## Suggested Fix
Change line 104 to "All current shaders" (no number) or update to 21, with a re-count-don't-trust-prose note.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
