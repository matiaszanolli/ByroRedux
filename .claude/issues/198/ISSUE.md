# #198: SYNC-4: Scene descriptor sets partially stale after resize (latent)
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/resize.rs`
- **Source**: `docs/audits/AUDIT_RENDERER_2026-04-10b.md`
- **Related**: #193 (SYNC-1), #195 (LIFE-2)
- **Fix**: Add `write_ao_texture()` calls when implementing SSAO resize fix; consider `rebind_all_scene_descriptors()` helper
