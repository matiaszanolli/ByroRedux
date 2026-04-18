# REN-AS-H1: TLAS instance_custom_index vs SSBO index parity is fragile across two filter predicates

**Issue**: #419 — https://github.com/matiaszanolli/ByroRedux/issues/419
**Labels**: bug, renderer, high, vulkan

---

## Finding

Two independent pieces of code filter `draw_commands` and produce different index spaces that are assumed to line up:

- **`crates/renderer/src/vulkan/acceleration.rs:958-991`** — `build_tlas` encodes `instance_custom_index = i` where `i` is the enumerate index over the full `draw_commands` slice, filtering on `!in_tlas` or `blas_entries.get(...).is_none()`.
- **`crates/renderer/src/vulkan/context/draw.rs:425-430`** — SSBO builder iterates the same slice but filters on `mesh_registry.get(draw_cmd.mesh_handle).is_some()` and writes `instance_idx = gpu_instances.len()` (a **compacted** index).

The shader (`triangle.frag:228-233, 542-545, 893-910`) indexes the SSBO with the TLAS custom index:

```glsl
instances[rayQueryGetIntersectionInstanceCustomIndexEXT(rq)]
```

## Why this works today

Comment at `acceleration.rs:950-954` asserts "the mesh_registry.get() guard in draw.rs always succeeds for submitted draw commands". No enforced invariant backs that.

## Impact — time-bomb

A single draw_cmd with a stale or evicted `mesh_handle` causes SSBO indices to shift by one from that point forward while TLAS custom indices stay put. Every subsequent RT hit indexes the wrong material/transform in the SSBO, producing **silent visual corruption** on shadows / reflections / GI for every frame after the divergence.

Also affects `caustic_splat.comp:120` and the primary-hit path at `triangle.frag:184, 322, 545, 910`.

Races with:
- `MeshRegistry::drop_mesh`
- Future mesh eviction strategies (the BLAS LRU eviction from M31 is adjacent territory)
- Any draw-command rejection for reasons other than `in_tlas = false`

## Fix

Two options, pick one:

**(a) Shared index table (recommended)**: build a `Vec<u32>` mapping `draw_idx → gpu_instance_idx` in one pass. Feed that to the TLAS loop as `instance_custom_index = map[draw_idx]`. Single source of truth.

**(b) Enforced invariant**: promote the "mesh_registry.get() always succeeds for draw_commands" claim to `debug_assert!(mesh_registry.get(h).is_some())` at BOTH the acceleration.rs filter and the draw.rs SSBO builder, plus a comment block documenting that the two filters MUST stay identical. Cheaper but keeps the fragility.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check caustic_splat.comp:120 and all `triangle.frag` `rayQueryGetIntersectionInstanceCustomIndexEXT` sites — they all share the same assumption and must stay consistent.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Build a scene with a draw_command whose mesh_handle has been evicted (simulating the race). With fix (a), expect an explicit "missing" instance. With fix (b), expect `debug_assert!` to fire immediately.

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 8 H1. The single highest-priority correctness time-bomb in the RT pipeline.
