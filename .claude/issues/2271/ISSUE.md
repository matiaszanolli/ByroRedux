# SAFE-2026-08-03-01: No Miri coverage for the ECS cached-pointer aliasing model

Severity: low
Source audit: docs/audits/AUDIT_SAFETY_2026-08-03.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2271

**Dimension**: 2 (Memory Corruption / UB)
**Source**: `docs/audits/AUDIT_SAFETY_2026-08-03.md` (SAFE-2026-08-03-01)
**Status**: NEW
**Location**: `crates/core/src/ecs/query.rs:23-144` (`QueryRead`/`QueryWrite`/`ComponentRef`)

## Description
`QueryRead`, `QueryWrite`, and `ComponentRef` each cache a raw `*const T`/`*mut T`
resolved once in `new()` from the guard's boxed storage, then deref it in the hot
path without re-touching the guard. The soundness argument is airtight at the type
level today — the guard field is declared before the cached pointer field, is never
re-borrowed after construction, and the borrow checker gates `&`/`&mut` coexistence
— but that is a *convention*, not something the compiler enforces under Stacked
Borrows. A future method that reads `self.guard` after construction would silently
invalidate the cached pointer's tag while still compiling and passing `cargo test`.
There is no `miri` job anywhere in the repo (`grep -rln miri .github/` → empty).

## Evidence
```
$ grep -rln miri .github/
(no output)

crates/core/src/ecs/query.rs:23-35   guard declared before the cached pointer field; no Drop impl touches the pointer
crates/core/src/ecs/query.rs:64      unsafe { &*self.storage } (QueryRead::storage)
crates/core/src/ecs/query.rs:135     unsafe { &*self.storage } (QueryWrite::storage)
crates/core/src/ecs/query.rs:143     unsafe { &mut *self.storage } (QueryWrite::storage_mut)
```

## Impact
Latent only — no current defect. Every SAFETY comment on these four sites correctly
cites the #1367/#35 contract and it holds under today's code. A future refactor
could reintroduce #35/#1367-class unsoundness without any test catching it, because
nothing in CI runs these paths under Stacked Borrows.

## Suggested Fix
Add a `cargo +nightly miri test -p byroredux-core` CI job scoped to the `ecs`
module, or replace the `guard` field with `PhantomData` + a manually-managed drop
so re-deriving a pointer from `self.guard` post-construction becomes structurally
impossible (a compile error, not just a convention).

## Related
None — no open issue overlaps this finding (checked against 47 open issues,
`/tmp/audit/issues.json`).

## Completeness Checks
- [ ] **UNSAFE**: If the fix restructures the cached-pointer field, the SAFETY
      comments on `query.rs:64,135,143` (and `ComponentRef`'s equivalent) still
      state the guard-outlives-pointer invariant accurately
- [ ] **TESTS**: A Miri CI job (or structural guard) actually catches a
      re-introduced #1367/#35-class violation — verify by temporarily reintroducing
      one and confirming the new job fails
