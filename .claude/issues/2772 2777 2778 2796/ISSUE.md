# Batch fix: #2772, #2777, #2778, #2796

All renderer, from AUDIT_RENDERER_2026-08-12b.md. Domain: **renderer** →
`byroredux-renderer`.

## #2772 — REN-D13-05: TAA/FSR write GpuCamera.jitter.xy under opposite Y-sign
TAA (`draw.rs::taa_jitter`) and FSR (`upscaling.rs::fsr_pixel_jitter_to_ndc`)
write jitter.xy under opposite Y-sign conventions. Consumers are sign-agnostic
so not a live rendering bug, but `triangle.frag`'s `DBG_VIZ_FSR_TEMPORAL`
debug view hard-codes the FSR convention with no gate on active upscaler —
sign-wrong in TAA mode. Needs investigation of intent + a fix/gate + doc.

## #2777 — REN-D2-01: ReSTIR spatial reuse inert past ~66841 BU
Reservoir depth lane clamped to 65504 (f16 max) on write, compared against
unclamped `worldDist` f32 on read in `spatialDepthCompatible` — comparison
always fails past that distance, wasting 5 reservoir fetches/pixel with zero
effect. Fix: clamp the read side to match, or widen the write-side encoding.

## #2778 — REN-D2-02: RESERVOIR_LIGHT_MASK unguarded vs MAX_LIGHTS
`RESERVOIR_LIGHT_MASK` (GLSL literal in triangle.frag) has no lockstep test
against Rust `MAX_LIGHTS` (`pub(super)` in scene_buffer/constants.rs) — only
correct today because 511 < 1023. Need: import from the shared constants
table (like the other REN-D2 issues fixed for MATERIAL_KIND) + a pin test.

## #2796 — REN-D16-01: bloom source has no sky (MEDIUM, needs a decision)
Bloom reads `composite.hdr_image_views` (pre-composite HDR G-buffer), which
never contains sky (synthesized only inside composite.frag). Two effects:
(1) sun/sky can never bloom despite #2233's stated intent: (2) exterior clear
color (CORNFLOWER_BLUE) leaks into the bloom source as sky-shaped constant
radiance, producing an analytically-estimated ~0.3-0.7 linear HDR lift +
horizon bleed. Issue offers two fix directions with different scope/risk;
explicitly flags wanting a capture before sizing the real fix. Needs
investigation + likely a user decision per project's no-speculative-Vulkan-
fix policy.
