# #577 MEM-1: GpuInstance doc comment says 192 bytes — struct is 320 bytes since #562

**Severity**: LOW  
**Audit**: AUDIT_RENDERER_2026-04-22  
**File**: `crates/renderer/src/vulkan/scene_buffer.rs:111`

## Summary

Doc comment reads "Layout: 192 bytes per instance, 16-byte aligned (12×16)" but struct ends at offset 316+4 = 320 bytes (20×16) after Skyrim+ variant payloads added in #562.

## Fix

```rust
/// Layout: 320 bytes per instance, 16-byte aligned (20×16).
```

Also verify/add `assert_eq!(std::mem::size_of::<GpuInstance>(), 320)` test.
