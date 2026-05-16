# CONC-D3-NEW-02: refit_skinned_blas safety docstring contradiction (cosmetic)

**GitHub**: #1139
**Severity**: INFO (cosmetic)
**Audit**: AUDIT_CONCURRENCY_2026-05-16.md
**Status**: CONFIRMED

## Location
- `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:523-534` (safety docstring)

## Summary
Docstring at lines 523-534 says the barrier "is now emitted as the first statement of this
function" (correct) but then says "Pre-fix this was a caller-side precondition documented but
unenforced" using tense that reads as if the fix is still pending. Cosmetic only — no code issue.

## Fix
Reword to past tense: "Before #983 / REN-D8-NEW-15 this was a caller-side precondition..."
No code change to the barrier call itself.
