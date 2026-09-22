# NIF-D1-2026-09-21-02: is_havok_constraint_stub / stubbed_drift_histogram are dead after #4212, and doc sites still describe stub under-reads

**Issue**: #4626
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: LOW
**Dimension**: 1 (Stream Position Integrity)
**Location**: `crates/nif/src/lib.rs:163-198, 341-355, 509-535`; `crates/nif/src/scene.rs:106-123`
**Status**: NEW

## Description
`is_havok_constraint_stub` / `stubbed_drift_histogram` are dead after #4212, and doc sites still describe the old "~45 stub under-reads per actor" behaviour.
- `is_havok_constraint_stub` (`lib.rs:189`) now matches only `"bhkGenericConstraint"` — #4212 gave `bhkRagdollConstraint`, `bhkLimitedHingeConstraint`, `bhkHingeConstraint`, `bhkMalleableConstraint` and `bhkPrismaticConstraint` typed CInfo decoders and removed them from the stub list (per the code's own #3713 comment); a later pass removed `bhkBallAndSocketConstraint`, `bhkStiffSpringConstraint` and `bhkBallSocketConstraintChain` too (`lib.rs:191-194` comment, "#4212").
- `bhkGenericConstraint`, the one type left on the list, has **no dispatch arm** anywhere in `crates/nif/src/blocks/mod.rs` or `blocks/collision/*.rs` (confirmed via grep — the only occurrence outside `lib.rs`/`scene.rs` is a test comment in `bhk_constraint_tests.rs` calling `BhkConstraint::parse` directly, not through block dispatch).
- It therefore always falls through to the `_` fallback and becomes `NiUnknown` with an exact `block_size` skip, so the drift branch that consults `is_havok_constraint_stub` (`lib.rs:521`, feeding `stubbed_drift_histogram` at `:534`) can never fire for it.
- `bhkGenericConstraint` is also absent from nif.xml and from every audited corpus, and `stubbed_drift_histogram` is empty on every corpus this audit measured.

## Evidence
`crates/nif/src/lib.rs:189-198` (`is_havok_constraint_stub` body); `:521,534` (the now-unreachable consult site); `grep -rn "bhkGenericConstraint" crates/nif/src` returns no dispatch-table entry, only the test file.

## Impact
Dead telemetry code and a stale set of struct fields (`stubbed_drift_histogram` threaded through `scene.rs` at 7 sites) carrying no signal. No parse-correctness impact.

## Related
#4212 (closed — removed the five typed-decoder types from the stub list, leaving only the unreachable one)

## Suggested Fix
Remove `is_havok_constraint_stub`, `stubbed_drift_histogram` and its plumbing through `lib.rs`/`scene.rs`, or add a dispatch arm for `bhkGenericConstraint` if it's ever needed. Update the audit-nif skill's Dim 5 checklist (see NIF-D3-2026-09-21-04) which still says its "drift is suppressed".

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D1-2026-09-21-02)

## Completeness Checks
- [ ] **TESTS**: If removed, confirm no test depends on the dead fields
