# Audit: Renderer — Dimension 4: Render Pass & G-Buffer

**Date**: 2026-05-11
**Depth**: deep (single-dimension, `/audit-renderer 4` → `--focus 4`)
**Scope**:
- `crates/renderer/src/vulkan/context/helpers.rs` — `create_render_pass`, `find_depth_format`, `create_main_framebuffers`, `create_depth_resources`
- `crates/renderer/src/vulkan/gbuffer.rs` (full)
- `crates/renderer/src/vulkan/context/draw.rs:200-290` — clear values + render pass begin
- `crates/renderer/src/vulkan/context/resize.rs` (full)

## Executive Summary

The render-pass + G-buffer subsystem is in solid shape. Attachment formats, load/store ops, layout transitions, framebuffer lockstep with the render pass, and resize teardown are all coherent. Three LOW findings on hygiene — two are subpass-dependency stage-mask asymmetries the validator would prefer tightened, one is a depth-format edge case on devices that fall back to packed depth-stencil formats. **No CRITICAL/HIGH issues.** The 2026-05-09 sweep's findings in this area (#906 `render_finished` per-image, #870 shared-depth-across-frames) are either fixed today (#906 in commit 913f804) or protected by a const-assert (#870 at `sync.rs:32-36`).

## RT Pipeline Assessment

Not in scope for D4 — see D8 (Acceleration Structures), D9 (RT Ray Queries), D10 (Denoiser & Composite).

## Rasterization Assessment

**Healthy.** All 6 color attachments + 1 depth attachment are correctly cleared, stored, transitioned to `SHADER_READ_ONLY_OPTIMAL` for downstream consumers, and recreated in correct order on swapchain resize. The 15-bit-id + bit-15-flag mesh_id invariant (#ALPHA_BLEND_NO_HISTORY for SVGF) is consistent across the writer (triangle.frag) and all three readers (svgf_temporal.comp, taa.comp, caustic_splat.comp), with the 32767-cap `debug_assert!` in `draw.rs:1149` as the safety net. Per-frame-in-flight ownership of G-buffer images is preserved; the SVGF "previous frame" sampling correctly references the OTHER slot (`1 - frame`).

## Findings

### REN-D4-NEW-01: Outgoing subpass dep omits `EARLY_FRAGMENT_TESTS` in `src_stage_mask`

**Severity**: LOW
**Status**: NEW
**Location**: `crates/renderer/src/vulkan/context/helpers.rs:149-159`
**Evidence**: `dependency_out.src_stage_mask = COLOR_ATTACHMENT_OUTPUT | LATE_FRAGMENT_TESTS`. The incoming dep covers both `EARLY` + `LATE` depth stages, but outgoing covers only `LATE`.
**Why it's a bug**: Depth writes occur in both EARLY and LATE fragment tests stages depending on whether the fragment shader uses `discard` / writes `gl_FragDepth`. With early-fragment-tests enabled (no discard branch hitting depth-test), writes that complete in EARLY are not necessarily ordered against the downstream SSAO compute read by a barrier that only synchronises FROM LATE. Spec allows it (LATE is logically-later), but Synchronization2 validation and several IHV implementations treat the missing EARLY as an under-synchronisation hint. Symmetric with the incoming dep would be cleaner.
**Fix sketch**: Add `EARLY_FRAGMENT_TESTS` to `dependency_out.src_stage_mask` for symmetry with the incoming dep at lines 129-133.

### REN-D4-NEW-02: Packed depth-stencil fallback uses DEPTH-only view with combined-layout final_layout

**Severity**: LOW
**Status**: NEW
**Location**: `crates/renderer/src/vulkan/context/helpers.rs:336-342` (view aspect) + `helpers.rs:98` (final layout)
**Evidence**: `find_depth_format` accepts `D32_SFLOAT_S8_UINT` / `D24_UNORM_S8_UINT` as fallbacks. The view is created with `aspect_mask: DEPTH` only (line 337), but the render pass's `final_layout: DEPTH_STENCIL_READ_ONLY_OPTIMAL` (line 98) is the combined layout.
**Why it's a bug**: On a device that falls back to a packed depth-stencil format, the stencil aspect of the image is never transitioned by the render pass's depth-only view binding, and downstream samplers (SSAO compute) read through a depth-aspect view that may not match `DEPTH_STENCIL_READ_ONLY_OPTIMAL` strictly. `VK_KHR_separate_depth_stencil_layouts` permits `DEPTH_READ_ONLY_OPTIMAL` for this case but the code uses the combined layout unconditionally. RTX 4070 Ti exposes D32_SFLOAT first so this doesn't fire in practice, but it'll bite a portability test on AMD/Intel pre-Vulkan 1.2 stacks.
**Fix sketch**: Either restrict `find_depth_format` candidates to `D32_SFLOAT` + `D16_UNORM` (pure depth — Skyrim/FNV/FO4 don't author stencil-tested decals so the fallback isn't load-bearing), or detect `has_stencil_component(format)` and switch the render pass's depth `final_layout` to `DEPTH_READ_ONLY_OPTIMAL` (requires VK_KHR_separate_depth_stencil_layouts or Vulkan 1.2) when the aspect view is DEPTH-only.

### REN-D4-NEW-03: `initialize_layouts` uses deprecated-style `TOP_OF_PIPE` source stage with non-empty `dst_access_mask`

**Severity**: LOW
**Status**: NEW
**Location**: `crates/renderer/src/vulkan/gbuffer.rs:298-326`
**Evidence**: Pipeline barrier with `src_stage = TOP_OF_PIPE`, `dst_stage = FRAGMENT_SHADER | COMPUTE_SHADER`, `src_access = empty`, `dst_access = SHADER_READ`. Old layout `UNDEFINED`.
**Why it's a bug**: Strictly correct (UNDEFINED transitions don't need source synchronisation), but Synchronization2 validation prefers explicit `ALL_COMMANDS` or `NONE` source with empty src_access; `TOP_OF_PIPE` in `src_stage_mask` is technically deprecated terminology in newer specs. The `dst_access = SHADER_READ` is unnecessary because the layout transition itself is the read-availability operation. Cosmetic — won't break, but it's a future-cleanup flag for the eventual Sync2 migration.
**Fix sketch**: Either switch to Synchronization2 (`VkImageMemoryBarrier2` with `srcStageMask = NONE`), or leave as-is and accept the deprecated-style note. Lowest-priority finding.

## What's NOT a bug

- **Color attachment load/store**: All six G-buffer color attachments use `LOAD_OP_CLEAR + STORE_OP_STORE` via the shared `make_color` closure. `helpers.rs:70-80`.
- **Depth load/store**: `LOAD_OP_CLEAR + STORE_OP_STORE` (stored for SSAO compute consumer). `helpers.rs:90-98`.
- **Color final_layout**: All six set to `SHADER_READ_ONLY_OPTIMAL`, matching downstream composite/SVGF sampling. `helpers.rs:79`.
- **Depth final_layout**: `DEPTH_STENCIL_READ_ONLY_OPTIMAL`, matching SSAO compute READ_ONLY sampling. `helpers.rs:98`.
- **Initial_layout UNDEFINED**: Correct for first-frame and post-resize use; render pass handles the transition. `helpers.rs:78, 97`.
- **Incoming subpass dep**: Covers `COLOR_ATTACHMENT_OUTPUT + EARLY + LATE_FRAGMENT_TESTS` in both src and dst. `helpers.rs:126-143`.
- **Outgoing dst stages**: `FRAGMENT_SHADER + COMPUTE_SHADER`, paired with `SHADER_READ` dst_access. Composite samples color, SSAO compute samples depth. `helpers.rs:172-175`. `BOTTOM_OF_PIPE` comment at lines 160-171 documents the prior fix.
- **Format choices**: normal `RG16_SNORM` (octahedral, Schied 2017 — `gbuffer.rs:37`), motion `R16G16_SFLOAT` (`gbuffer.rs:38`), mesh_id `R16_UINT` (`gbuffer.rs:39`), raw_indirect + albedo `B10G11R11_UFLOAT_PACK32` (`gbuffer.rs:44, 48`).
- **Mesh_id 15-bit cap + bit-15 flag**: `0x7FFFu` mask consistent across writer (`triangle.frag:943`) and all readers (`svgf_temporal.comp:142`, `taa.comp:116`, `caustic_splat.comp:122`). `debug_assert!(gpu_instances.len() <= 0x7FFF)` at `draw.rs:1149`.
- **Usage flags**: All G-buffer images `COLOR_ATTACHMENT | SAMPLED` at `gbuffer.rs:88`. Depth `DEPTH_STENCIL_ATTACHMENT | SAMPLED` at `helpers.rs:272`.
- **Recreate ordering**: `GBuffer::recreate_on_resize` destroys views → images → allocs per attachment via `Attachment::destroy` at `gbuffer.rs:154-173`. Framebuffers destroyed first in `resize.rs:43` before depth (`resize.rs:45-53`).
- **Clear values count + order**: 7 entries at `draw.rs:249-271`, matching render pass's 6 color + 1 depth (HDR, normal, motion, mesh_id, raw_indirect, albedo, depth). Mesh_id clears to `uint32:[0,0,0,0]` (background = 0, shader writes id+1). `draw.rs:257-262`.
- **Per-frame ownership**: `Attachment::images: Vec<vk::Image>` length = `MAX_FRAMES_IN_FLIGHT` at `gbuffer.rs:75`. SVGF / TAA "previous frame" reads sample slot `1 - frame` via the descriptor sets rebuilt in `resize.rs:296-308`.
- **Shared depth across MAX_FRAMES_IN_FLIGHT**: Tracked at #870, protected by const-assert at `sync.rs:32-36`.
- **render_finished per-image vs per-frame**: Tracked at #906, fixed in commit 913f804.

## Prioritized Fix Order

All three findings are LOW. If addressed, recommend in this order:

1. **REN-D4-NEW-02** — restrict `find_depth_format` to pure-depth formats (3-line change, prevents the portability foot-gun before any non-NVIDIA bring-up).
2. **REN-D4-NEW-01** — add `EARLY_FRAGMENT_TESTS` to outgoing subpass dep src_stage_mask (1-line change, satisfies the validator hint).
3. **REN-D4-NEW-03** — Sync2 migration of `initialize_layouts` (defer; bundle with a future Sync2 sweep if/when other initialization barriers move).

None are urgent. The subsystem can stay shipped as-is.
