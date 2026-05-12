# #959 — REN-D8-NEW-15: refit_skinned_blas scratch-serialize barrier is caller-emitted

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM8_v2.md`
**Dimension**: Acceleration Structures
**Severity**: LOW
**Confidence**: MED
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/959

## Locations

- `crates/renderer/src/vulkan/acceleration.rs:1098-1107` — function docstring requires caller to emit barrier
- `crates/renderer/src/vulkan/context/draw.rs:735` — caller honors the contract

## Summary

`refit_skinned_blas` documents the scratch-serialize-barrier precondition in its `# Safety` block but doesn't emit it. Contract is preserved today; a future second call site that forgets the precondition would corrupt shared scratch.

## Fix (preferred)

Make `record_scratch_serialize_barrier` the first statement of `refit_skinned_blas` itself. Barrier is idempotent — extra emission is harmless compared to a missing one. Caller-side emission becomes a no-op safety net.

## Tests

N/A — correctness checked via Vulkan validation layer / RenderDoc, not unit tests.
