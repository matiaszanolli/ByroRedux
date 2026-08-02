# TD1-079: material.rs crossed 2000 LOC — mostly inline test growth, no oversized production function

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2257

**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/renderer/src/vulkan/material.rs` (2015 LOC)
**Status**: NEW

**Description**: Grew from 1931 LOC (07-25 boundary) to 2015 (+84 LOC) — a marginal crossing, ~60% of the file is `#[cfg(test)]` content. No single production function is anywhere near 200 LOC; the production code (`GpuMaterial`, `MaterialTable::intern`/`intern_by_hash`, preset constructors) is unchanged in shape from prior audits. This crossing is purely test accumulation — the tests were never split into a sibling file to begin with, unlike `texture_registry.rs`/`texture_registry_tests.rs`, the established convention elsewhere in this same directory.

**Evidence**: `git show 2cb86be5:...material.rs | wc -l` → 1931; current → 2015. Longest production function is well under 100 LOC; the file's length is dominated by ~35 `#[test]` fns plus two large GPU-layout pinning tests (~280 combined lines).

**Impact**: Maintainability only, lowest-urgency finding in this batch — no logic is hard to follow, just file length.

**Suggested Fix**: Extract the `#[cfg(test)] mod tests { ... }` block into a sibling `material_tests.rs`, mirroring the already-established `texture_registry.rs`/`texture_registry_tests.rs` split in the same directory. Purely mechanical, lowest-risk of any finding in this batch.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
