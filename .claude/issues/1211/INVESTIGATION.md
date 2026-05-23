# #1211 — Investigation

## Code path

- Panic site: `crates/renderer/src/vulkan/context/draw.rs:379` —
  `.framebuffer(self.framebuffers[frame])` (issue cited line 356 of the
  pre-split version; the file was reorganised but the offending line is
  unchanged in spirit).
- `self.framebuffers` is owned by `VulkanContext` (`mod.rs:911`),
  populated during init (`mod.rs:1873`) and rebuilt on resize
  (`resize.rs:564`).

## Why it can be empty when `draw_frame` runs

`recreate_swapchain` (`resize.rs`) destroys then rebuilds in this order:

1. `destroy_main_framebuffers` (line 63) — Vec is cleared.
2. `destroy_depth_resources` (65).
3. `mem::take` old image views (98).
4. `swapchain::create_swapchain(...)?` (103) — first fallible step;
   if Err returns here, `self.framebuffers` is empty, depth handles are
   null, `image_views` is empty.
5. Many more fallible steps (depth recreate, render pass, SSAO,
   gbuffer, SVGF, caustic, bloom, composite, TAA, scene descriptors,
   `create_main_framebuffers` at 564, `frame_sync.recreate_for_swapchain`
   at 583).

Any `?`-propagated failure between step 1 and step 564 leaves
`self.framebuffers` at `len == 0`.

The caller (`byroredux/src/main.rs:1333-1336` for `WindowEvent::Resized`,
`:1562-1568` for the `needs_recreate` post-draw path) logs and calls
`event_loop.exit()`. Exit is *queued* — the current event-loop tick
continues, and the very next `RedrawRequested` (which can already be
queued or fire in the same iteration when winit batches) drives
`draw_frame` → panic at line 379.

## Sibling check

- `swapchain_state.images[img]` at `draw.rs:2622`: reachable only
  AFTER `acquire_next_image` returned a valid `image_index`. Invariant
  holds: if `framebuffers` is non-empty, `images` is non-empty (both
  are rebuilt by the same `recreate_swapchain` body, with `images`
  rebuilt first). Gating on `framebuffers.is_empty()` at the top of
  `draw_frame` therefore covers this site transitively.
- `frame_sync.images_in_flight[img]` at `draw.rs:253/268`: sized to
  the swapchain's image count, rebuilt at `resize.rs:584`. Same
  invariant — if framebuffers is non-empty, this Vec was just rebuilt
  too. Skipping above keeps us out of this path.
- `command_buffers[frame]` at `draw.rs:314`: sized by
  `MAX_FRAMES_IN_FLIGHT` (compile-time const, not swapchain-derived);
  not at risk.
- `composite_framebuffers` (composite.rs:804): indexed by
  `swapchain_image_index` returned by `acquire_next_image`. Same
  transitive coverage — if the main framebuffers Vec is non-empty, the
  composite rebuild at `resize.rs:494-510` ran successfully too.

## Fix design

**Layer 1 (load-bearing)**: early-return at the very top of
`draw_frame`, before `wait_for_fences` and `acquire_next_image`, when
`self.framebuffers.is_empty()`. Returning before `acquire_next_image`
avoids signalling `image_available[frame]` semaphore without a paired
wait — no semaphore-leak recovery needed.

Return value: `Ok(false)`. `Ok(true)` would prompt the caller to retry
`recreate_swapchain` immediately, looping if the underlying issue is
sticky (surface-lost on a still-invalid window). `Ok(false)` lets the
next `Resized` / focus event drive recovery naturally.

**Layer 2 (optional, deferred)**: a `swapchain_invalid` flag in the
app gating `draw_frame`. Skipped for now — the inner guard is enough
and additive bookkeeping at the call site duplicates state already
implicit in `framebuffers.is_empty()`.

## Test

Following the precedent set by `resize.rs::tests` (the #654
ordering check), a source-string assertion verifies the guard is
present at the top of `draw_frame`. A live unit test that mocks
`VulkanContext` is impractical — the struct holds 70+ Vulkan-loader
fields that don't have safe default values.
