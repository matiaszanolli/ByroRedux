# Issue #1074: renderer dds.rs missing 7 DXGI format mappings

**Domain**: renderer  
**Severity**: MEDIUM  
**Location**: `crates/renderer/src/vulkan/dds.rs:183-205` — `map_dxgi_format`

7 DXGI formats crash texture upload: B8G8R8A8_UNORM (87), B8G8R8A8_SRGB (91),
R16_UNORM (56), R8_UNORM (61), BC4_SNORM (81), BC6H_UF16 (95), BC6H_SF16 (96).
Fix: add constants + match arms. B8G8R8A8/R16/R8 need no Vulkan feature flag;
BC6H requires textureCompressionBC (already used by existing BC1-BC7).
