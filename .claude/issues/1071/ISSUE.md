# #1071 — F-WAT-11: Water pipeline static CULL_MODE NONE — no post-water guard

**Severity**: LOW  
**Audit**: `docs/audits/AUDIT_RENDERER_2026-05-14_DIM17.md`  
**Location**: `crates/renderer/src/vulkan/water.rs:377-384`

## Summary

Water pipeline has CULL_MODE as static (baked NONE); opaque/blend pipelines have it dynamic. Currently safe (UI after water is also static-cull). No assertion or guard prevents a future pipeline inserted post-water from reading undefined cull state.

## Fix (Option A — preferred)

Add `vk::DynamicState::CULL_MODE` to water `dynamic_states` and issue `cmd_set_cull_mode(NONE)` at the start of the water draw section in `draw.rs`. Removes special-casing.
