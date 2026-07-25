**Severity**: MEDIUM
**Dimensions**: 4, 8, 11, 12, 20 (five independent `/audit-renderer` dimension agents surfaced pieces of this same underlying drift)
**Source**: `docs/audits/AUDIT_RENDERER_2026-07-25.md` (M-1)
**Status**: NEW (consolidated from `REN-11-01`, `REN-11-02`, `REN-12-01`, `REN-20-01`, and matching notes in the Dim 4 and Dim 8 scratch reports — filed as ONE issue per project convention for a single underlying story surfaced by multiple sub-passes)

## Description

The 2026-07-22→24 FSR 3.1 work (`crates/fsr3-sys`, `crates/renderer/src/vulkan/presentation.rs`, `frame_upscaler.rs`, `exposure.rs`) restructured the tail of the frame. The code side is internally consistent and fully test-pinned; only the reference docs and the audit checklist itself lag:

- **G-buffer grew 6 → 8 color attachments.** `docs/engine/shader-pipeline.md`'s "G-Buffer Layout" table still lists six (HDR, normal, motion, mesh_id, raw_indirect, albedo). `gbuffer.rs::GBuffer` now also owns `reactive` and `transparency` attachments (`FSR_MASK_FORMAT = R8_UNORM`), and `context/helpers.rs::create_render_pass` explicitly builds an 8-entry `color_refs` array; all three graphics pipelines' blend-attachment arrays are correctly 8-wide in lockstep (`reflect::tests::triangle_frag_declares_eight_color_outputs` passes).
- **ACES tone-mapping moved out of `composite.frag` into `presentation.frag`.** Composite now emits render-resolution linear HDR (`HDR_FORMAT = R16G16B16A16_SFLOAT`, `final_layout = SHADER_READ_ONLY_OPTIMAL`) with no tone-map step; `presentation.frag` applies ACES to the *upscaled* image using its own `PresentationPushConstants.exposure`.
- **Composite no longer writes the swapchain; `presentation.rs` does.** Composite's render pass targets an intermediate HDR image; `presentation.rs`'s render pass owns `final_layout(PRESENT_SRC_KHR)` and binds `swapchain_views`. Two stale comments still credit composite with this: `context/draw.rs`'s egui-pass comment ("Composite already wrote the swapchain image and left it in PRESENT_SRC_KHR" — flagged independently by both Dim 8 and Dim 20 as the exact same line) and the Dim-4 checklist's "composite's outgoing dstStage = NONE" wording (that property now belongs to `presentation.rs`'s `outgoing` dependency).
- **Submission order gained two steps.** `shader-pipeline.md`'s "Per-Frame Submission Order" stops at step 16–17 (composite → egui). The actual order (`context/post_passes.rs::record_post_passes`) is: SVGF → caustic splat → volumetrics → TAA (gated) → SSAO → bloom → composite → **`frame_upscaler.record`** (FSR 3.1 SDK dispatch or native-blit fallback) → **`presentation.dispatch`** (exposure + ACES + underwater → swapchain) → egui → screenshot copy.
- **`CompositeParams.underwater` and `depth_params.y` (exposure) are dead uploads.** Both are populated every frame in `draw.rs` and `composite.frag`'s UBO declares both, but `composite.frag`'s `main()` reads neither — the real consumers are `presentation.frag`'s independently-sourced `params.underwater`/`params.exposure` push constants. Doc comments on both fields still assert composite consumes them; contrast `fog_color`/`fog_params`, which *are* correctly annotated reserved-and-unconsumed (#1926/#1927).
- **`_audit-common.md`'s `VulkanContext` file-listing row is stale.** It lists only `(mod.rs, draw.rs, resize.rs, resources.rs, helpers.rs, screenshot.rs)`; the directory also now contains `geometry_pass.rs`, `post_passes.rs`, and `skinned_blas_refit.rs` (all part of the same #1857/FSR3-era file split).

Verified independently against current code during publish:
- `gbuffer.rs:14-15,64-67,240-241,260-261` — `reactive`/`transparency` `Attachment` fields confirmed live.
- `crates/renderer/src/vulkan/presentation.rs`, `frame_upscaler.rs`, `exposure.rs` confirmed to exist.
- `composite.frag:46` — `depth_params.y` (comment: "y = exposure") grepped; only `.x`/`.z` are read anywhere in `main()`.
- `crates/renderer/src/vulkan/context/` listing confirmed to include `geometry_pass.rs`, `post_passes.rs`, `skinned_blas_refit.rs` alongside the six files `_audit-common.md` still lists.

## Evidence

`gbuffer.rs:64-72,234-244`; `context/helpers.rs:86-122`; `pipeline.rs:336-359,640-649,822-850`; `context/post_passes.rs:537-608`; `presentation.rs:136-150`; `context/draw.rs` egui-pass comment (~line 2192-2196); `composite.frag` (grep `depth_params` → only `.x`/`.z` read, never `.y`).

## Impact

Zero runtime impact — every piece of this is internally consistent, correctly synced (each new pass declares its own `SubpassDependency` pair, mirroring the pre-existing #1433 egui pattern), correctly torn down in reverse order (`context/mod.rs` Drop, ~line 3285-3300), and correctly instrumented (GPU telemetry: `gpu_timers.rs`'s `QUERIES_PER_FRAME = 28` already includes `cmd_upscale_start/_end` and `cmd_presentation_start/_end`, correctly wired into `post_passes.rs` — checked and came back clean). The impact is entirely for a future contributor or auditor: tracing a swapchain-write bug by looking in `composite.rs`, or adding a new G-buffer consumer against the stale 6-attachment table, would send them to the wrong place.

## Suggested Fix

One doc pass covering `docs/engine/shader-pipeline.md` (G-Buffer table → 8 attachments incl. `reactive`/`transparency`; submission-order list extended with upscale + presentation steps; note ACES's new home), `_audit-common.md`'s `VulkanContext` file row, and the two stale "composite wrote the swapchain" comments (`context/draw.rs`, and the Dim-4/SKILL.md checklist wording). Either drop `CompositeParams.underwater`/`depth_params.y` or re-annotate them "reserved (moved to presentation.frag)" to match the `fog_*` precedent.

## Completeness Checks
- [ ] **SIBLING**: Same drift pattern checked across all doc/comment sites that reference the pre-FSR3 frame tail (shader-pipeline.md, _audit-common.md, context/draw.rs comment, audit-renderer SKILL.md Dim 4/11/12/20 wording)
- [ ] **TESTS**: N/A — documentation-only fix; the code side is already test-pinned (`reflect::tests::triangle_frag_declares_eight_color_outputs`, GPU timer coverage)
