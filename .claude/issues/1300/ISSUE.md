# #1300 — D12B-1: first-sight skinned BLAS build (i==0) reuses shared scratch with no AS_WRITE/AS_WRITE serialize barrier

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_RENDERER_2026-05-28_DIM12). GitHub is authoritative for current state._

**Severity**: MEDIUM · **Dimension**: GPU Skinning / Acceleration Structures
**Source**: `docs/audits/AUDIT_RENDERER_2026-05-28_DIM12.md` (finding D12B-1)

**Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:242-266` (build loop — `record_scratch_serialize_barrier` is gated `if i > 0`).

**Issue**: `build_skinned_blas_batched_on_cmd` records an `AS_WRITE→AS_WRITE` scratch-serialize barrier only for `i > 0`. The first build (`i == 0`) has no preceding barrier on the shared `blas_scratch_buffer`. When a cell-load static-BLAS batch (`build_blas_batched`) wrote the same scratch earlier — even in a prior submission — the first skinned build can touch scratch before the prior build's writes are visible. The refit path already self-emits this barrier as its first statement (the #983 pattern); the batched-build path does not.

**Risk**: Scratch-buffer WAR/WAW hazard across builds sharing one scratch allocation → intermittent BLAS corruption on the first skinned mesh of a cell, under driver scheduling that overlaps the two builds. Timing-dependent (hasn't surfaced in steady-state bench).

**Suggested fix**: Self-emit `self.record_scratch_serialize_barrier(device, cmd)` once at the top of the Phase-3 record loop (before the `i == 0` build), mirroring `refit_skinned_blas`. Idempotent with the existing `i > 0` barriers.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in the related path (refit vs batched-build; raster vs compute)
- [ ] **DROP**: if Vulkan objects change, Drop impl still correct
- [ ] **CANONICAL-BOUNDARY**: if the fix touches `material_translate.rs::translate_material` / `Material::resolve_pbr` / import-walk emitter params, per-game logic stays at the NIFAL parser→Material boundary (see /audit-nifal). _(N/A for these skinning/accel findings)_
- [ ] **TESTS**: regression test added for this specific fix
- [x] **SIBLING**: confirmed — `refit_skinned_blas` (blas_skinned.rs:371) self-emits the barrier; batched-build is the asymmetric one.
