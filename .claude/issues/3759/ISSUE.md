# #3759 — SAFE-2026-08-30-D5-01: `is_device_suitable` admits non-RT GPUs while the main triangle pipeline unconditionally creates SPIR-V declaring `RayQueryKHR` + `PhysicalStorageBufferAddresses` — #1561 covered only `water.frag`

**Labels**: bug, renderer, high, vulkan, safety

---

- **Severity**: HIGH
- **Dimension**: 5 — Vulkan Spec Compliance
- **Location**: `crates/renderer/src/vulkan/device.rs` (`REQUIRED_EXTENSIONS`, `RT_EXTENSIONS`, `is_device_suitable`, `create_logical_device`); `crates/renderer/src/vulkan/context/init.rs` (`create_triangle_pipeline` call site); `crates/renderer/shaders/triangle.frag.spv`, `triangle.vert.spv`, `caustic_splat.comp.spv`, `volumetrics_inject.comp.spv`, `skin_vertices.comp.spv`
- **Status**: sibling of CLOSED **#1561**, which fixed only the `water.frag` half
- **Source**: `docs/audits/AUDIT_SAFETY_2026-08-30.md` (`SAFE-D5-01`), HEAD `64f64480`

## Description

`VK_KHR_ray_query` / `VK_KHR_acceleration_structure` are **optional** in device selection —
`REQUIRED_EXTENSIONS` is just `[ash::khr::swapchain::NAME]`, and `is_device_suitable`
merely records `let ray_query_supported = RT_EXTENSIONS.iter().all(|ext| has_extension(ext));`
**without rejecting the device**. `create_logical_device` then correctly *withholds* the
matching features when that flag is false: `.buffer_device_address(caps.ray_query_supported)`,
`.acceleration_structure(caps.ray_query_supported)`, `.ray_query(caps.ray_query_supported)`.

But the **main geometry pipeline is created unconditionally** — `init.rs`'s
`pipeline::create_triangle_pipeline(…)` has **no `ray_query_supported` gate** (verified at
HEAD: every other `ray_query_supported` use in `init.rs` is an `if`-gated resource; the
triangle pipeline is not among them), and `pipeline.rs` loads `TRIANGLE_VERT_SPV` +
`TRIANGLE_FRAG_SPV` verbatim.

Per the Vulkan SPIR-V environment appendix, `RayQueryKHR` requires
`VkPhysicalDeviceRayQueryFeaturesKHR::rayQuery` and `PhysicalStorageBufferAddresses`
requires `VkPhysicalDeviceVulkan12Features::bufferDeviceAddress` to be **enabled**;
creating a shader module / pipeline from such SPIR-V without them violates
**VUID-VkShaderModuleCreateInfo-pCode-08740**.

**The codebase already knows this argument and applies it elsewhere**: `device.rs` rejects
a device lacking `shaderInt64` with the comment *"a device without shaderInt64 cannot
legally create the renderer's shader modules (VUID-VkShaderModuleCreateInfo-pCode-08740)"*.
The identical reasoning is simply not applied to the RT capabilities. And `init.rs` already
asserts *"RT-capable hardware (the only configuration this engine targets — RT is
mandatory)"* while gating **only** `WaterPipeline` on the flag — that is the #1561 fix.

## Evidence — independently decoded from the committed SPIR-V

`OpCapability` (opcode 17) stream decode of the committed `.spv` files:

```
triangle.frag.spv    [Shader, ImageQuery, RayQueryKHR(4472), RuntimeDescriptorArray,
                      InputAttachmentArrayDynamicIndexing, StorageBufferArrayDynamicIndexing,
                      PhysicalStorageBufferAddresses(5347)]
triangle.vert.spv    [Shader, ImageQuery, PhysicalStorageBufferAddresses(5347)]
caustic_splat.comp   [Shader, ImageQuery, RayQueryKHR, PhysicalStorageBufferAddresses]
volumetrics_inject   [Shader, DerivativeControl, RayQueryKHR]
skin_vertices.comp   [Shader, ImageQuery, PhysicalStorageBufferAddresses]
water.frag.spv       [Shader, ImageQuery, RayQueryKHR, …]   ← the only one whose pipeline IS gated
```

(Re-decoded at HEAD for `triangle.frag.spv`, `triangle.vert.spv` and `water.frag.spv`:
`triangle.frag` and `water.frag` carry identical capability sets; `triangle.vert` carries
5347.)

- `RT_EXTENSIONS` is labelled *"Optional RT extensions (enabled when available)"*.
- No `return Ok(None)` follows the `ray_query_supported` computation.
- `grep -rn 'ray_query_supported' byroredux/src crates/renderer/src` yields only
  per-feature `if` / flag reads, **never a startup rejection**.

## Impact

On any GPU that passes `is_device_suitable` but lacks `VK_KHR_ray_query` — the suitability
message itself names *"RDNA1 or newer"*, and RDNA1 (RX 5000) has **no** ray-query support —
startup runs `vkCreateShaderModule` / `vkCreateGraphicsPipelines` with capabilities whose
features are disabled. That is undefined behaviour: the observable outcome ranges from a
wall of validation errors (*"SPIR-V Capability RayQueryKHR was declared, but one of the
following requirements is required (VkPhysicalDeviceRayQueryFeaturesKHR::rayQuery)"*) to
driver-dependent pipeline-creation failure to a hard fault.

Because `bufferDeviceAddress` is **also** withheld, even `triangle.vert` (which has no ray
query at all, only `PhysicalStorageBufferAddresses`) is illegal on that path — **so there
is no partial-render fallback; the entire main pass is affected.** RT-capable hardware (the
dev 4070 Ti, and everything the project actually targets) is completely unaffected, which
is why this has never been observed.

## Suggested Fix — device selection, NOT a pipeline restructure

> This is deliberately scoped to device selection. Per
> `feedback_speculative_vulkan_fixes.md`, render-pass / pipeline / barrier changes whose
> failure modes are invisible to `cargo test` are not to be shipped speculatively. **Do not
> restructure any pipeline for this issue.**

Make device selection match the documented policy. Either move
`ash::khr::acceleration_structure::NAME` + `ash::khr::ray_query::NAME` +
`ash::khr::deferred_host_operations::NAME` into `REQUIRED_EXTENSIONS`, or — matching the
`shaderInt64` shape exactly — add, immediately after the `ray_query_supported` computation
in `is_device_suitable`:

```rust
// The committed shader set declares RayQueryKHR (triangle.frag, caustic_splat.comp,
// volumetrics_inject.comp, water.frag) and PhysicalStorageBufferAddresses (triangle.vert,
// triangle.frag, skin_vertices.comp). Both require features this chain only enables when
// ray_query_supported is true, so an RT-less device cannot legally create these shader
// modules (VUID-VkShaderModuleCreateInfo-pCode-08740).
if !ray_query_supported {
    return Ok(None);
}
```

and let the existing `anyhow::bail!("No suitable GPU found …")` carry the diagnostic
(extend its text with the RT requirement). That single change removes the illegal path and
makes `init.rs`'s "RT is mandatory" comment true. The now-dead `ray_query_supported == false`
branches (the `accel_manager`, `skin_compute`, `skin_palette`, `water` gates, the
rt-disabled descriptor-layout permutation, `buffer_device_address` gating) can be simplified
in a **follow-up**, not in the same commit.

**Verification note**: the *static* premise above (which capabilities the shipped SPIR-V
declares, and that the pipeline is created without a gate) is proven from the binaries and
the source. The *runtime* symptom on an RT-less device would need a validation-layer run on
such a GPU (or `VK_LAYER_KHRONOS_validation` with a device-simulation layer) to observe
directly.

## Related

#1561 (CLOSED — REN-D2-NEW-01, the `water.frag` half of exactly this problem);
#1636 / #1478 (the same "feature gated on the wrong flag" shape for `host_query_reset`);
the `shaderInt64` rejection in `device.rs` that cites the very same VUID.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every pipeline whose SPIR-V declares an RT or BDA capability, not just the triangle pipeline
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix — a unit test over the committed `.spv` capability sets asserting that every declared capability is covered by a `REQUIRED_EXTENSIONS` / suitability rejection
