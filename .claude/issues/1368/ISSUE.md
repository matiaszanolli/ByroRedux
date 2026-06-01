# PERF-D3-NEW-07: SipHash on render hot path (per-draw material_hash + whole-buffer dirty-gates)

**Severity**: MEDIUM · **Dimension**: Draw Call / Material Table (merged PERF-D3-NEW-07 + PERF-D8-NEW-01/02)
**Location**: `crates/renderer/src/vulkan/context/mod.rs:503-620` (`DrawCommand::material_hash`, ~75 write_u32), called `byroredux/src/render/static_meshes.rs:647` + `render/particles.rs:204`; dirty-gates `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:218-231` + `:243-254`; `MaterialTable::index` probe
**Status**: NEW

Every render-hash site uses `DefaultHasher` (SipHash-1-3, DoS-resistant — irrelevant for an internal key). Per-draw `material_hash` is computed unconditionally before `intern_by_hash` probes (so the ~97% dedup-hit draws still pay the full 75-field walk) ≈ 0.3-0.6ms/frame; the full-buffer instance dirty-gate ≈ 0.45ms/frame on a 7359-instance scene. Combined ~0.8-1.5ms/frame.

**Fix**: (a) immediate — add `rustc-hash` (FxHash) or `ahash`, swap `DefaultHasher` across `material_hash`, `hash_gpu_material_fields`, `hash_material_slice`, `hash_instance_slice`, and the `index` map. ~5-10× faster, behavior-identical (collision resistance irrelevant; the debug collision assert at material.rs:1055 stays; lockstep test `material_hash_matches_gpu_material_field_hash` guards it). (b) deferred (M40) — carry a stable interned material id on the `Material` component so the per-draw walk is skipped except on first sight.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
