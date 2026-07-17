# TD1-009: cell_loader/references/mod.rs — load_references is a 1015-line function (69% of the file)

**GitHub Issue**: #2058
**Labels**: low,import-pipeline,legacy-compat,tech-debt,bug

**Severity**: LOW
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `byroredux/src/cell_loader/references/mod.rs:92-1106` (`load_references`)

## Description
The #1877 split reduced the *file* below threshold but not the *function* below the 200-line guidance. Not a regression of #1877 — a follow-on that continues the same split one level deeper.

## Evidence
Confirmed live: `byroredux/src/cell_loader/references/mod.rs` is 1480 LOC total; `pub(super) fn load_references(` starts at line 92, matching the report's claimed location — 1015/1480 ≈ 69% of the file, matching the report's stated proportion.

## Related
Existing: #1877 (closed, file-size fix) — not a regression, a follow-on.

## Suggested Fix
Continue the #1877 split one level deeper — extract per-record-kind dispatch (static mesh / light / door+teleport / precombined-skip).

**Effort**: medium

## Completeness Checks
- [ ] **SIBLING**: Same "function still monolithic after a file-level split" shape as TD1-008/TD1-010/TD1-011
- [ ] **TESTS**: A regression test pins that the per-record-kind dispatch split produces identical spawned/loaded output for a representative reference set
