# Issue #30: Renderer: hardcoded D32_SFLOAT depth format without support query

- **State**: OPEN
- **Labels**: enhancement, renderer, low, vulkan
- **Location**: `crates/renderer/src/vulkan/context.rs:23`

Depth format is `const D32_SFLOAT` with no `get_physical_device_format_properties` call.
Universal on desktop but not spec-mandated.

**Fix**: Query format properties at device selection, fallback D32→D32S8→D24S8→D16.
