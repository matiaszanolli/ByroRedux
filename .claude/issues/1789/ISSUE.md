# CONC-D6-01: Stale context/mod.rs line-number citations in acceleration/mod.rs::destroy() comments

_Filed as #1789 from `docs/audits/AUDIT_CONCURRENCY_2026-07-02.md`._

**Severity**: LOW · **Dimension**: Resource Lifecycle (stale comment) · Source: `AUDIT_CONCURRENCY_2026-07-02` (CONC-D6-01)

## Location
`crates/renderer/src/vulkan/acceleration/mod.rs:251-252,292-293`.

## Description
`AccelerationManager::destroy()`'s doc comments cite `context/mod.rs:1300`, `context/mod.rs:1859`, and `context/mod.rs:2093` as the locations of the `device_wait_idle()` calls that make the immediate (non-deferred) destroys in this function safe. Those line numbers predate the #1670/#1671 (`0409b6d6`) and #1749 (`26439046`) refactors; the actual `device_wait_idle()` calls in the current tree are at `context/mod.rs:2521` (`flush_pending_destroys`) and `context/mod.rs:2836` (`Drop::drop`). The referenced invariant itself (drain `pending_destroy_blas` + `skinned_blas` unconditionally, because an upstream `device_wait_idle` already covers any in-flight reference) is still correct and still held by both call sites — only the citation is stale.

## Evidence
`grep -n "device_wait_idle" crates/renderer/src/vulkan/context/mod.rs` confirms only two call sites, at 2521 and 2836, neither matching the cited line numbers.

## Impact
None functionally — documentation/traceability defect. A future reader chasing the comment lands on unrelated code (pipeline creation), which could cost review time or lead someone to "fix" a correctly-documented invariant redundantly.

## Related
CONC-D1-01 (same file family; the immediate-destroy hazard there is the *code* issue, this is the *comment* issue).

## Suggested Fix
Update the two comment blocks in `acceleration/mod.rs` to cite `context/mod.rs::flush_pending_destroys` / `context/mod.rs::Drop::drop` by name/anchor rather than by line number (refactor-resistant). Bundle with the next touch of this file.

## Completeness Checks
- [ ] **SIBLING**: Both comment blocks (:251-252 and :292-293) re-cited by name/anchor, not line number
- [ ] **TESTS**: N/A — documentation-only; verify with `grep -n "device_wait_idle" context/mod.rs`
