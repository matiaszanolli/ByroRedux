# #5207 — REN-D5-2026-10-03-03: #4886's "transactional" `recreate_descriptor_sets` still leaks the replacement samplers when pool creation or set allocation fails

**Labels**: low,renderer,vulkan,memory,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `TextureRegistry::recreate_descriptor_sets` (`crates/renderer/src/texture_registry/mod.rs`).
- **Status**: NEW. Residual of CLOSED #4886; the fix is otherwise in place.
- **Description**:
  - When the mip bias changed (an upscaler/preset switch or a resize under FSR), `replacement_samplers = Some(create_material_samplers(...)?)` creates four `VkSampler`s *first*.
  - The following `create_descriptor_pool(...)` returns through bare `?`. The `allocate_descriptor_sets` error arm destroys only `new_pool`.
  - On either failure the four new samplers are never destroyed: they are a local `Option<[vk::Sampler; 4]>` with no Drop. The doc's claim, "an `Err` return leaves the registry exactly as it was", holds for the registry's fields but not for the device.
  - The pin `recreate_descriptor_sets_allocates_the_replacement_before_the_old_pool_dies` checks pool/set ordering only. `create_material_samplers` itself unwinds correctly on a partial failure.
- **Evidence**: The sampler creation precedes `let new_pool = unsafe { device.create_descriptor_pool(&pool_info, None).context(...)? };`. The `Err(e)` arm of `allocate_descriptor_sets` destroys `new_pool` only.
- **Impact**:
  - Four sampler handles leak per failed recreate.
  - The #2156 `set_upscaler_mode` rollback re-enters this function, so a persistent descriptor-pool OOM can leak 4 more per retry.
  - Validation reports live samplers at device destroy.
  - Reaching it requires a descriptor-pool allocation failure.
- **Suggested Fix**: Create the replacement samplers after the pool and sets succeed, or destroy `replacement_samplers` in both error arms. Extend the #4886 pin to assert that one of these holds.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
