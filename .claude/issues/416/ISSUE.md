# REN-AS-H2: TRIANGLE_FACING_CULL_DISABLE applied unconditionally — RT sees backfaces of every mesh

**Issue**: #416 — https://github.com/matiaszanolli/ByroRedux/issues/416
**Labels**: bug, renderer, high, vulkan

---

## Finding

`crates/renderer/src/vulkan/acceleration.rs:984-987` sets `TRIANGLE_FACING_CULL_DISABLE` on every TLAS instance unconditionally:

```rust
instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
    0,
    vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() as u8,
),
```

`DrawCommand` has `two_sided: bool` (`context/mod.rs:45`) which the rasterizer pipeline consumes. The RT path ignores it and forces cull-disable for every instance.

## Impact

- Ray queries hit backfaces on single-sided opaque geometry — closed meshes (rooms, buildings) have their interior-facing walls contribute shadow/GI when the ray traverses them from the outside.
- Shadow + GI rays see ~2× the geometry on closed single-sided meshes → perf regression.
- Self-shadow on walls that should cull backfaces.

Worst on interiors where a room's "outside" backfaces become shadow casters.

## Fix

Gate the flag on `draw_cmd.two_sided`:

```rust
let instance_flags = if draw_cmd.two_sided {
    vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw()
} else {
    0
};
```

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify the flag's interaction with `VK_GEOMETRY_OPAQUE_BIT_KHR` set on the BLAS (acceleration.rs:322, 552). Also check caustic/GI ray traversal doesn't assume two-sidedness.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Render test with a closed single-sided mesh (box interior) — shadow from outside should not bleed through the far wall onto the near wall's back.

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 8 H2.
