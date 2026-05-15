# REN-D8-NEW-13: instance_custom_index 24-bit overflow has no guard

**State**: OPEN
**Labels**: renderer, low, vulkan, safety

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM8_v2.md`
**Dimension**: Acceleration Structures
**Severity**: LOW
**Confidence**: HIGH

## Observation

`crates/renderer/src/vulkan/acceleration.rs:2047`:

```rust
instances.push(vk::AccelerationStructureInstanceKHR {
    transform,
    instance_custom_index_and_mask: vk::Packed24_8::new(ssbo_idx, 0xFF),
    // ...
});
```

`ssbo_idx: u32` is fed from `build_instance_map` (monotonic `0, 1, 2, …` per surviving draw). `vk::Packed24_8::new` silently truncates to 24 bits (max 16 777 215). `padded_count = max(2× count, 8192)` (`acceleration.rs:2102`) is not a hard cap upstream.

## Why it's a bug

Vulkan spec: `VkAccelerationStructureInstanceKHR.instanceCustomIndex` is 24 bits. The SSBO indexing in `triangle.frag` reads it as the GpuInstance array index via `rayQueryGetIntersectionInstanceCustomIndexEXT`. A future exterior with > 2^24 surviving draws writes the wrong `ssbo_idx` and silently corrupts every RT hit's material / transform lookup.

Unreachable today — the R16_UINT mesh_id ceiling (Dim 4, `helpers.rs:54-62`) caps visible instances at 32 767, and `debug_assert!` at `draw.rs:1154` enforces that. But the 24-bit invariant lives in a different file and isn't tied to the upstream cap.

## Suggested fix

`debug_assert!(ssbo_idx < (1 << 24), …)` at the push site, plus a `log::warn!` once-per-second telemetry if `instance_count` is within 10% of 2^24. Mirrors the existing `debug_assert!` at `draw.rs:1154` for the R16_UINT mesh_id ceiling.

## Completeness Checks
- [ ] **UNSAFE**: No new unsafe.
- [ ] **SIBLING**: Cross-check against Dim 4 R16_UINT mesh_id ceiling (`helpers.rs:54-62`) and Dim 5 `debug_assert!` (`draw.rs:1154`) — same invariant family.
- [ ] **DROP**: N/A.
- [ ] **LOCK_ORDER**: N/A.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: Optional — unit test pinning the 24-bit cap at the call site.

## Dedup

- No existing OPEN issue matches.
- Adjacent to `#647` / RP-1 (mesh_id R16→R32 upgrade plan) and `#956` (Dim 5 `debug_assert!` in active recording).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
Status: Closed (108450e)
