# REN-D1-01: stale line-number and phase-number references in `blas_static.rs` comments after the #6709 split

Labels: low,renderer,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6739

**Severity**: LOW · **Dimension**: AS Correctness

## Location
`crates/renderer/src/vulkan/acceleration/blas_static.rs`, `build_blas_batched` (alloc_compact rollback comment) and `prepare_static_blas_batch` (geometry SAFETY comment)

## Description
The #6709 split moved the phases into methods but three comments still point at the old layout: "Mirrors the build/copy-phase cleanup pattern at lines 733-745 / 815-832"; "`PreparedBlas::geometry` (line ~610)"; and the geometry SAFETY comment says the geometry is stored "for the Phase 2 batched build submission", but the build submission is Phase 4 (`record_batch_builds`; Phase 2 is `ensure_batch_scratch`).

## Evidence
`grep -n "lines 733\|line ~610\|Phase 2 batched" crates/renderer/src/vulkan/acceleration/blas_static.rs` hits all three.

## Impact
None at runtime. It misleads a reader auditing the `'static` geometry claim (SAFE-21), the one invariant that leans on that comment.

## Suggested Fix
Replace the line numbers with symbol names (`the copy_result failure arm below`, `PreparedBlas::geometry`) and say Phase 4. No code change.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Same wording checked in related files (docs, other comments)
- [ ] **TESTS**: Covered by existing guards / `.claude/commands/_audit-validate.sh`; no new test needed for pure comment edits
