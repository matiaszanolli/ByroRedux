# #194: D6-1: RT reflection TLAS instance index mismatches SSBO draw index
- **Severity**: HIGH
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/shaders/triangle.frag:174`, `crates/renderer/src/vulkan/acceleration.rs:262-292`
- **Source**: `docs/audits/AUDIT_RENDERER_2026-04-10b.md`
- **Fix**: Encode SSBO index in `instance_custom_index_and_mask`, use `rayQueryGetIntersectionInstanceCustomIndexEXT` in shader
