# TD8-024: SKINNED_BLAS_FLAGS doc comment references deleted build_skinned_blas function

## Source Audit
`docs/audits/AUDIT_TECH_DEBT_2026-05-17.md` — Dimension 8 (Backwards-Compat Cruft / orphaned function reference)

## Severity
**LOW** — comment-only stale reference left over from today's #1141 cleanup.

## Location
`crates/renderer/src/vulkan/acceleration/constants.rs:86-88`

## Description
The doc comment for `SKINNED_BLAS_FLAGS` reads:
```rust
/// Build flags for the skinned-BLAS BUILD + UPDATE call sites in
/// `blas_skinned.rs` (`build_skinned_blas`, `build_skinned_blas_batched_on_cmd`,
/// `refit_skinned_blas`). ...
```

`build_skinned_blas` (sync) was deleted in today's #1141 (commit `96cb6ab8`). Only the other two functions remain live. The orphaned name creates confusion for readers chasing call sites.

## History
#1141 explicitly cleaned up 6 stale cross-references; this one in `constants.rs:87` was missed in that sweep.

## Proposed Fix
Edit the doc comment to list only the two live functions:
```rust
/// Build flags for the skinned-BLAS BUILD + UPDATE call sites in
/// `blas_skinned.rs` (`build_skinned_blas_batched_on_cmd`,
/// `refit_skinned_blas`). ...
```

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Grep all renderer-side files for any other lingering `build_skinned_blas` references not caught by #1141: `grep -rn 'build_skinned_blas[^_]' crates/renderer`
- [ ] **DROP**: N/A
- [ ] **TESTS**: N/A (doc only)
