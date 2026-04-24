# #575 SH-1: triangle.frag reads vertex SSBO as float[] — bone/splat fields are not floats

**Severity**: MEDIUM  
**Audit**: AUDIT_RENDERER_2026-04-22  
**File**: `crates/renderer/shaders/triangle.frag:174-176`

## Summary

Global vertex SSBO declared as `float vertexData[]`. Works for UV access (offsets 9-10 = genuine f32). But bone_indices (bytes 44-59 = u32×4) and splat weights (bytes 68-75 = u8×8) would be silently reinterpreted as f32 if read via the same pattern in RT hit shaders.

## Fix

Short-term: add warning comment.  
Long-term: dedicated `vec2 uvData[]` SSBO for RT hit UV lookups.
