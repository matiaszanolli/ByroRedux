# PERF-D6-2026-09-11-02: The per-entity skin dispatch re-binds the same compute pipeline once per skinned entity

Labels: low,performance,renderer,bug

**Description**: `SkinComputePipeline::dispatch` opens with `cmd_bind_pipeline` and is called once per non-skipped entity in the dispatch loop; the pipeline handle is immutable for the renderer's lifetime, so every bind after the first is redundant (descriptor set + push constants genuinely vary per slot; the pipeline bind does not).

**Evidence**:
`crates/renderer/src/vulkan/skin_compute.rs:701-728`, called from `skinned_blas_refit.rs:501-570`.

**Impact**: One extra recorded command per dirty skinned entity per frame; sub-microsecond each, zero GPU cost — a small constant, not a scaling hazard.

**Related**: Bundle with PERF-D6-2026-09-11-01 (same loop).

**Suggested Fix**: Split a `bind()` off `dispatch()`, hoist above the loop; keep the timer bracket ordering unchanged.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
