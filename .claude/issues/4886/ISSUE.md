# REN-D5-2026-09-26-08: `recreate_descriptor_sets` is not transactional — a failure after `destroy_descriptor_pool` leaves a destroyed pool handle that the rollback and `destroy()` destroy again

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4886

**Labels**: medium,renderer,vulkan,memory,bug

- **Severity**: MEDIUM (failure path only; needs fault injection)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/texture_registry/mod.rs` — `TextureRegistry::recreate_descriptor_sets`; callers `crates/renderer/src/vulkan/context/resize.rs` (`recreate_texture_ssao_bindings`, the `set_upscaler_mode` rollback), `TextureRegistry::destroy`
- **Status**: NEW
- **Description / Evidence**:
  - I confirmed the order in code: `destroy_descriptor_pool(self.descriptor_pool)`, then later `self.descriptor_pool = create_descriptor_pool(..).context(..)?` and `allocate_descriptor_sets(..).context(..)?`.
  - On `Err`, `self.descriptor_pool` still names the destroyed pool and `bindless_sets` names freed sets. The #2156 rollback in `set_upscaler_mode` re-enters `recreate_swapchain` → `recreate_descriptor_sets` and destroys the same handle again. A fatal resize failure then reaches `TextureRegistry::destroy` for a third destroy.
  - The sampler-replacement half is fine: `create_material_samplers` runs before any destroy.
- **Suggested Fix**: Create the new pool and sets before destroying the old ones, or null `self.descriptor_pool` and clear `bindless_sets` immediately after the destroy. Destroying a null handle is a no-op, which `destroy()` already tolerates from the partial-init path.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl and teardown ordering are still correct
- [ ] **TESTS**: A regression test pins this specific fix
