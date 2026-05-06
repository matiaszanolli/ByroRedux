## REN-D4-NEW-01: shared depth image safe under MAX_FRAMES_IN_FLIGHT=2 only — add static_assert regression guard

## Source Audit
`docs/audits/AUDIT_RENDERER_2026-05-06.md` — Dim 4 cross-dim notes

## Severity / Dimension
LOW / Render Pass — Vulkan Sync (regression guard)

## Location
- `crates/renderer/src/vulkan/context/mod.rs:580-582` (depth_image / depth_image_view / depth_allocation declarations — single VkImage, NOT per-frame-in-flight)
- `crates/renderer/src/vulkan/sync.rs:6` (`MAX_FRAMES_IN_FLIGHT: usize = 2`)
- `crates/renderer/src/vulkan/context/draw.rs:108-120` (the double-fence wait that makes the shared depth image safe)

## Description
The depth image is a single `vk::Image` (not per-frame-in-flight, unlike the G-buffer attachments which all live in `Vec<vk::Image>` indexed by frame_index). Frame N+1's main-render-pass `LOAD_OP_CLEAR` on depth would race against frame N's compute consumers (SSAO / SVGF) which sample depth — UNLESS frame N's compute work has retired before frame N+1 begins.

**Today this is safe** because:
- `MAX_FRAMES_IN_FLIGHT = 2`, AND
- `draw.rs:108-120` waits on **both** `in_flight[frame]` AND `in_flight[(frame+1) % MAX_FRAMES_IN_FLIGHT]` per #282.

Under `MAX_FRAMES_IN_FLIGHT = 2`, waiting on both fences is equivalent to device-idle for prior frames, so frame N's SSAO + SVGF compute have retired before frame N+1's render-pass clear touches the depth image.

**The hazard is real** if `MAX_FRAMES_IN_FLIGHT` is ever bumped to 3+: the both-fences pattern would only wait on 2 of N slots. Frame N-2's compute might still be in-flight when frame N+1's render pass clears depth.

## Evidence
```rust
// crates/renderer/src/vulkan/context/mod.rs:580-582
depth_image_view: vk::ImageView,
depth_image: vk::Image,
depth_allocation: Option<vk_alloc::Allocation>,
// (scalars, not Vec — single shared depth image)

// crates/renderer/src/vulkan/sync.rs:6
pub const MAX_FRAMES_IN_FLIGHT: usize = 2;
```

The double-fence wait in `draw.rs:108-120` (#282) is the safety contract. Nothing in the depth declaration or `MAX_FRAMES_IN_FLIGHT` constant points at the contract.

## Impact
None today — current behaviour is spec-compliant. **Latent regression risk**: a future `MAX_FRAMES_IN_FLIGHT` bump (e.g. enabling triple-buffered frames-in-flight for higher GPU utilisation) would silently violate the contract. The first symptom would be SVGF / SSAO sampling stale depth from frame N-2's clear, producing flickering AO / ghosted denoised indirect.

## Suggested Fix
Add a `const_assert!` (or `static_assertions::const_assert_eq!` if the dependency exists) at the depth-image declaration and at `MAX_FRAMES_IN_FLIGHT`'s definition cross-referencing the safety contract:

```rust
// crates/renderer/src/vulkan/context/mod.rs near line 580
// SAFETY: the depth image is a single VkImage shared across all frames
// in flight. This is safe ONLY because draw.rs:108-120 waits on both
// in_flight fences (#282), which under MAX_FRAMES_IN_FLIGHT = 2 is
// equivalent to waiting on every in-flight slot. If MAX_FRAMES_IN_FLIGHT
// is ever raised, either:
//   (a) make depth per-frame-in-flight (Vec<vk::Image>) to match the
//       G-buffer pattern, OR
//   (b) extend the fence wait to cover all slots.
const _: () = assert!(crate::vulkan::sync::MAX_FRAMES_IN_FLIGHT == 2,
    "shared depth image requires MAX_FRAMES_IN_FLIGHT == 2; see #REN-D4-NEW-01");
depth_image_view: vk::ImageView,
depth_image: vk::Image,
depth_allocation: Option<vk_alloc::Allocation>,
```

Or equivalently a `#[cfg(...)]` gate, but `const _: () = assert!(...)` works at workspace-build time without a feature.

## Related
- #282 — original double-fence wait that introduced the safety contract.
- #573 / SY-2 — Sync2 spec-compliance pass (BOTTOM_OF_PIPE removal). The shared-depth pattern was unaffected.
- The G-buffer attachments use the per-frame-in-flight pattern (Vec<vk::Image>) and are immune to this hazard.

## Completeness Checks
- [ ] **UNSAFE**: N/A — `const _: () = assert!()` is safe Rust.
- [ ] **SIBLING**: Verify other `vk::Image` fields on VulkanContext that are NOT per-frame-in-flight (e.g. SSAO output, caustic accumulator slots) — do they share the same MAX_FRAMES_IN_FLIGHT==2 safety contract? Audit each.
- [ ] **DROP**: N/A — no resource lifecycle change.
- [ ] **LOCK_ORDER**: N/A.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: Optional — a build-time `const _: () = assert!()` is its own regression test (workspace fails to compile if MAX_FRAMES_IN_FLIGHT ≠ 2, surfacing the contract).
