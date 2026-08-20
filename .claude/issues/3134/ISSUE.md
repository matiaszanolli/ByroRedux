# SAFE-2026-08-20-02: the water params UBO is 65,472 B and its guard test names 64 KiB as the portable maxUniformBufferRange floor — the spec floor is 16 KiB and nothing queries the device limit

**Issue**: #3134 — https://github.com/matiaszanolli/ByroRedux/issues/3134
**Finding**: `SAFE-2026-08-20-02`
**Labels**: bug, medium, vulkan, memory, safety
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_SAFETY_2026-08-20.md` — Dimension 5 (Vulkan spec compliance) + Dimension 6 (GPU table layout soundness)
**Severity**: MEDIUM · **Status**: NEW

## Location
- `crates/renderer/src/vulkan/water.rs:169-172` — `MAX_WATER_DRAWS` + its rationale comment
- `crates/renderer/src/vulkan/water.rs:433-434` — buffer size
- `crates/renderer/src/vulkan/water.rs:459-466` — the descriptor write that carries the range
- `crates/renderer/src/vulkan/water.rs:911-915` — the guard assertion

## Description
`MAX_WATER_DRAWS = 186` × `size_of::<GpuWaterParams>() = 352` = **65,472 bytes**, uploaded as a `UNIFORM_BUFFER` and bound with `range = param_buffer_size`.

Both the constant's doc comment and the guard test justify that figure by calling 64 KiB the *portable* `maxUniformBufferRange` floor. **It is not.** The Vulkan specification's Required Limits table sets `maxUniformBufferRange` at **16384 bytes**. 64 KiB is the common *reported* value on desktop drivers (and the D3D11 constant-buffer size), not the guarantee. The buffer is therefore **4× the spec-guaranteed maximum**, and `grep -rn "max_uniform_buffer_range" crates/ byroredux/` returns **nothing** — the device limit is never queried, so there is no runtime clamp, no fallback, and no diagnostic.

This is the renderer's only large UBO. Every other bulk per-draw array in the tree is a `STORAGE_BUFFER` (`scene_buffer/buffers.rs:510`/`:566` are single-record camera/DALC UBOs; volumetrics' fog-volume, cluster and index arrays are all `STORAGE_BUFFER` at `volumetrics.rs:1091-1113`). The water path is the one place that put a 186-element array in a uniform block.

## Evidence
```rust
// crates/renderer/src/vulkan/water.rs:169-172
/// Fixed UBO capacity: 186 × 352 B = 65,472 B, below Vulkan's portable
/// `maxUniformBufferRange` floor while leaving room for the
/// handful of water bodies normally visible in one cell.
pub const MAX_WATER_DRAWS: usize = 186;
```
```rust
// crates/renderer/src/vulkan/water.rs:911-915 — the guard asserts the wrong floor
assert!(
    MAX_WATER_DRAWS * std::mem::size_of::<GpuWaterParams>() <= 64 * 1024,
    "water UBO must fit Vulkan's portable maxUniformBufferRange floor"
);
```
```rust
// crates/renderer/src/vulkan/water.rs:459-466 — the range that must satisfy the limit
let info = [vk::DescriptorBufferInfo { buffer: buffer.buffer, offset: 0, range: param_buffer_size }];
let write = write_uniform_buffer(water_caustic_descriptor_sets[frame], 1, &info);
unsafe { device.update_descriptor_sets(&[write], &[]) };
```

On a conforming device that reports the spec minimum, that write violates **VUID-VkDescriptorBufferInfo-range-00342** (`range` must be ≤ `maxUniformBufferRange`), and the corresponding shader-side block exceeds `maxUniformBufferRange` at draw time.

All four sites verified present at HEAD, and the `max_uniform_buffer_range` grep is still empty.

Per the No-Speculative-Vulkan-Fixes rule this claim is derived from the spec's Required Limits table and the code, not from an observed VUID — the validation-layer channel was not exercised this run (25-agent target-lock contention, plus the no-parallel-engine-launch rule).

## Impact
Latent on the dev GPU (RTX 4070 Ti reports 65,536) and on every mainstream RT-capable desktop part, which is why this is MEDIUM and not HIGH — the `ray_query` gate on water pipeline creation (`context/mod.rs:2233`) narrows the device set considerably.

What makes it worth filing is the *headroom arithmetic the wrong comment invites*: the true remaining margin against the real-world 64 KiB ceiling is **64 bytes**. Adding one `vec4` to `GpuWaterParams` (352 → 368) puts the buffer at 68,448 B and breaks essentially every device — and someone reading "below Vulkan's portable floor" reasonably concludes they have room. The assertion would catch the growth, but the reader has been told the wrong reason it exists, which is the same failure mode as `GpuMaterial` being documented at 300 B after it grew to 348.

## Related
Dimension 6's `GpuMaterial` pins are the model to copy — `MAX_MATERIALS = 16384` is an SSBO precisely because uniform blocks cannot carry that. **#2688** (OPEN) is the sibling "the pin exists but pins the wrong property" finding on `GpuMaterial`.

## Suggested fix
Correct both the constant's doc comment and the assertion message to say 16 KiB is the spec floor and 64 KiB is the assumed-desktop ceiling this design deliberately targets. Then either:

(a) query `VkPhysicalDeviceLimits::maxUniformBufferRange` at `WaterPipeline::new` and clamp `MAX_WATER_DRAWS` — with the geometry pass already reading the constant via `.take()`, a runtime value threads through cleanly; or

(b) move the array to a `STORAGE_BUFFER` like every other bulk per-draw array in the renderer, which removes the limit question entirely and costs one `layout(std430)` change in `water.vert` / `water.frag`.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct (`WaterPipeline::destroy` owns the param UBOs and its own `SharedAllocator` clone — see SAFE-2026-08-20-05)
- [ ] **SIBLING**: Every other `UNIFORM_BUFFER` descriptor range in the renderer checked against the 16 KiB spec floor, not just water
- [ ] **TESTS**: A regression test pins this specific fix — the guard must assert the *real* floor it intends, and the shader-side block size must move with the Rust struct
