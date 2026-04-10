# #200: SYNC-07: AO image sampled in UNDEFINED layout on first frame
- **Severity**: MEDIUM — **Domain**: renderer — **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/ssao.rs:71,260`, `draw.rs:465`
- **Fix**: One-time command to transition AO image to SHADER_READ_ONLY_OPTIMAL + clear to white
