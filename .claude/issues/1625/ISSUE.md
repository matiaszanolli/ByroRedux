# TD4-001: FNV skill cites non-existent function predicates.rs::blas_budget_bytes

_Filed as #1625 from `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md`. Immutable snapshot as-filed; GitHub is authoritative for live state._

**Severity**: LOW · **Dimension**: Audit-Finding Rot · **Effort**: trivial
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md` (TD4-001)
**Status**: NEW

## Description
The FNV skill (`.claude/commands/audit-fnv/SKILL.md:75`) attributes the BLAS budget formula (`device_local_bytes / 3` floored at `MIN_BLAS_BUDGET_BYTES`) to `predicates.rs::blas_budget_bytes`. That is **not** a function — the formula lives in `compute_blas_budget` (`crates/renderer/src/vulkan/acceleration/predicates.rs:547`); `blas_budget_bytes` is only the struct *field* (`acceleration/mod.rs:153`) that caches the result.

## Evidence
`SKILL.md:75` — "`predicates.rs::blas_budget_bytes` = `device_local_bytes / 3` floored at `MIN_BLAS_BUDGET_BYTES`". Live code: `predicates.rs:547 pub(super) fn compute_blas_budget(`; `mod.rs:153 pub(super) blas_budget_bytes: vk::DeviceSize`; `mod.rs:203 let blas_budget_bytes = compute_blas_budget(instance, physical_device);`.

## Impact
A future audit grepping `predicates.rs` for `blas_budget_bytes` finds only the field reference (or nothing in `predicates.rs`) and may conclude the budget logic was removed. The path-validation gate cannot see symbol-anchor drift.

## Suggested Fix
Change the skill reference to `predicates.rs::compute_blas_budget`.

## Completeness Checks
- [ ] **SIBLING**: No other skill cites `blas_budget_bytes` as the budget *function* (the field name is fine where it means the cached value)
