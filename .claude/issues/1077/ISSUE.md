# FO4-D6-003: Forward BGSM PBR flags to renderer

**Source**: AUDIT_FO4_2026-05-15.md · MEDIUM  
**Location**: `byroredux/src/asset_provider.rs`, `scene_buffer/gpu_types.rs`  
pbr / translucency / model_space_normals / glowmap / tessellate parsed but never forwarded.
