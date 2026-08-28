# #3438 — SAFE-2026-08-27b-03: sanitize_finite's whole-struct pin is a hand-typed field list, so it cannot catch the defect class its doc-comment claims

- **Source**: `docs/audits/AUDIT_SAFETY_2026-08-27b.md`
- **Severity**: LOW
- **Labels**: `low,safety,nifal,test-gap,bug`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3438

---

From `docs/audits/AUDIT_SAFETY_2026-08-27b.md` (Dimension 9 — NIFAL boundary, NaN/Inf on the GPU).

- **Severity**: LOW (test-coverage gap; the code is correct today)
- **Location**: `crates/core/src/ecs/components/material.rs:1970-2070` (`sanitize_finite_leaves_no_non_finite_float_anywhere`)
- **Status**: NEW — the residual of #3373 (CLOSED, fixed in `59b85565`)

## Description

#3373's fix is complete: a mechanised diff of `struct Material`'s float fields against the `fix_scalar!`/`fix_vec!` calls now shows **33 float fields, 33 covered, 0 missing**. The prior report also asked for a durable guard, and one was added — but its doc-comment overstates what it does:

> This is the guard that catches the #3373 defect *class* — a float field added to `Material` without a matching `fix_scalar!`/`fix_vec!` line — rather than only the four fields that were missing this time. **Extend the literal below whenever `Material` gains a float.**

The two halves of that sentence contradict each other. The test poisons a hand-written list of 33 field initialisers and then re-reads a hand-written list of 33 accessors. A field added to `Material` without a `fix_scalar!` line is added without a test line by the identical omission — the test's own instruction admits the maintenance burden it was supposed to remove. #3373 was exactly that omission (four fields added on 2026-08-25, sanitiser not extended), so the guard does not close the loop that produced it.

This codebase already has the right instrument for a Rust-side structural invariant: `crates/renderer/src/shader_constants.rs` and `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` both use `include_str!` source scans to pin properties invisible to ordinary unit tests.

## Evidence

Verified mechanically — a script extracting `pub <name>: f32 | [f32; N]` from `struct Material` and `fix_(scalar|vec)!\((\w+)\)` from `sanitize_finite` reports `float fields: 33 / MISSING: [] / covered-but-not-a-float-field: []`, i.e. the *code* is complete while the *test* remains a literal transcription of that same list.

## Impact

None today. The next `Material` float — the BGEM/Bethesda material surface has grown four times in the last month (300 → 348 → 364 → 396 → 432 B) — can silently reopen the same hole, and the report that closed #3373 will read as if it were guarded.

## Related

#3373, #2687 (the finding that created `sanitize_finite`).

## Suggested Fix

Replace the literal with an `include_str!("material.rs")` source scan: extract every `f32` / `[f32; N]` field name from the `struct Material` block, extract every `fix_scalar!`/`fix_vec!` argument from `sanitize_finite`, and assert set equality. That is what the doc-comment already promises, and it needs no maintenance.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other hand-transcribed field lists pinning a struct — e.g. the `GpuMaterial` layout tests)
- [ ] **CANONICAL-BOUNDARY**: the fix touches `crates/core/src/ecs/components/material.rs` — per-game logic stays at the NIFAL parser→`Material` boundary, never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
