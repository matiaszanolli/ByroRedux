# REN-SHADER-H1: GpuInstance _pad1 drift in caustic_splat.comp — missed #344 lockstep rename

**Issue**: #417 — https://github.com/matiaszanolli/ByroRedux/issues/417
**Labels**: bug, renderer, high

---

## Finding

`crates/renderer/shaders/caustic_splat.comp:74` declares `uint _pad1;` at byte offset 156 of the `GpuInstance` struct. The three canonical shader sites declare that same slot as `uint materialKind;`:

- `crates/renderer/shaders/triangle.vert:39` — `uint materialKind;`
- `crates/renderer/shaders/triangle.frag:53` — `uint materialKind;`
- `crates/renderer/shaders/ui.vert:33` — `uint materialKind;`

## Why this happened

Per `feedback_shader_struct_sync.md`, `GpuInstance` is a cross-shader contract requiring lockstep updates. The memory note names **three** shaders as canonical consumers. `caustic_splat.comp` was added after issue #344 closed the original 3-shader sync, and the sync list was never extended.

Byte offset (156, 4 bytes) and std430 layout are identical — this is a **name-level drift only** today.

## Impact

- No runtime misbehavior (offsets align, field is unused in caustic_splat.comp).
- Future hazard: the next developer adding material-kind-aware code in caustic_splat.comp will either read `_pad1` as padding and skip the field, or reintroduce a divergent rename.
- Elevates risk of a real ABI break later.

Related to #344 (SK-D3-02 — material_kind dispatch in triangle.frag): SK-D3-02 is the producer side; this issue extends the consumer side to the 4th shader.

## Fix

1. Rename `_pad1` → `materialKind` at `caustic_splat.comp:74`.
2. Extend the sync list in `feedback_shader_struct_sync.md` memory from 3 shaders to 4.
3. Recompile SPIR-V for caustic_splat.comp.
4. (Optional) Add a CI grep check: any file declaring `struct GpuInstance` must appear in the sync doc.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check for any other compute shaders declaring `struct GpuInstance` (`grep -r 'struct GpuInstance' crates/renderer/shaders/`).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Update the `scene_buffer.rs` struct-sync regression test (around `scene_buffer.rs:820-857`) to include caustic_splat.comp.

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 6 H1.
