# REN-MEM-C3: Descriptor pools have no overflow/growth policy — hard crash on large cells

**Issue**: #425 — https://github.com/matiaszanolli/ByroRedux/issues/425
**Labels**: bug, renderer, vulkan, critical

---

## Finding

`crates/renderer/src/vulkan/descriptors.rs` — descriptor pools are sized at `VulkanContext` construction using hardcoded `max_sets` / per-type counts derived from an assumed upper bound (textures + meshes + UI + RT).

Once `MeshRegistry` or `TextureRegistry` crosses that bound — trivially possible in an exterior cell with LOD chains — `vkAllocateDescriptorSets` returns `VK_ERROR_OUT_OF_POOL_MEMORY`.

There is no second pool, no grow path, and callers treat the allocation as infallible (`.unwrap()` / `.expect()`).

## Impact

- **Hard crash** on large cells.
- Not a memory *leak* but a hard residency-ceiling cliff that users will hit long before VRAM exhaustion.
- Blocks any scene larger than the hardcoded bound from loading at all.

## Fix

Two acceptable approaches:

**(a) `UPDATE_AFTER_BIND` + bindless arrays (preferred)**: pairs with current `runtimeDescriptorArray` usage in `triangle.frag`. Let textures/meshes register into a single large bindless pool sized at device-property maxima rather than per-resource sets.

```rust
// In DescriptorPool creation:
let pool_flags = vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND
    | vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET;
```

Pool size set to `maxPerStageDescriptorUpdateAfterBind*` device limits.

**(b) Pool vector with grow-on-OOM**:
```rust
struct DescriptorPoolChain {
    pools: Vec<vk::DescriptorPool>,
    current_pool_idx: usize,
}
impl DescriptorPoolChain {
    fn allocate(&mut self, layout: vk::DescriptorSetLayout) -> Result<vk::DescriptorSet, _> {
        loop {
            match try_alloc(self.pools[self.current_pool_idx], layout) {
                Ok(set) => return Ok(set),
                Err(OUT_OF_POOL_MEMORY) => {
                    self.grow()?;  // push a new pool, bump index
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }
}
```

Option (a) is the architecturally right answer and aligns with where the rest of the renderer is going (bindless textures). Option (b) is a smaller migration and unblocks tonight.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: TextureRegistry (`crates/renderer/src/texture_registry.rs`) allocates per-texture sets — verify its pool has the same overflow handling. SVGF, composite, SSAO each own a pool; their fixed sizes are fine (they allocate a small, known count).
- [ ] **DROP**: When the pool chain grows, the old pools must still be destroyed in Drop.
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Load a cell with more than the hardcoded bound (e.g. inject 1000 dummy meshes into a test). Expect graceful grow, not crash.

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 2 C3. Part of the memory-shape trilogy.
