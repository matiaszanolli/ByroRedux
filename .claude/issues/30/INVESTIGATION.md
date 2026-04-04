# Investigation: Issue #30

## Root Cause
Line 23: `const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT` — hardcoded
without querying `get_physical_device_format_properties`. D32_SFLOAT is universal
on desktop but not spec-mandated for all implementations.

## Usage Sites
- Line 960: `create_render_pass` — depth attachment format
- Line 1060: `create_depth_resources` — image creation format
- Line 1103: `create_depth_resources` — image view format
- Line 1119: log message

## Fix
1. Add `find_depth_format(instance, physical_device)` that queries format properties
   for OPTIMAL_TILING + DEPTH_STENCIL_ATTACHMENT feature, with fallback chain:
   D32_SFLOAT → D32_SFLOAT_S8_UINT → D24_UNORM_S8_UINT → D16_UNORM
2. Store chosen format as `depth_format: vk::Format` field on VulkanContext
3. Pass it to `create_render_pass()` and `create_depth_resources()`
4. Remove the const

## Scope
1 file (context.rs): remove const, add field, add function, update 2 call sites.
