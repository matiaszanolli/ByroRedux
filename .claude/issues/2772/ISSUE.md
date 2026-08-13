# REN-D13-05: TAA/FSR write GpuCamera.jitter.xy under opposite Y-sign conventions

## Description
TAA and FSR write `GpuCamera.jitter.xy` under **opposite Y-sign conventions** through the same match arm. The applying consumers are sign-agnostic, so this is not a rendering bug — but `triangle.frag`'s `DBG_VIZ_FSR_TEMPORAL` branch *inverts* the mapping and hard-codes the FSR convention with no gate on the active upscaler, so that view is sign-wrong in TAA mode. An unlabelled trap for any future analytic jitter cancellation.

## Location
`crates/renderer/src/vulkan/context/draw.rs` (`taa_jitter`), `crates/renderer/src/vulkan/upscaling.rs` (`fsr_pixel_jitter_to_ndc`), `crates/renderer/shaders/triangle.frag`

## Severity / Domain / Type
low / renderer / bug

https://github.com/matiaszanolli/ByroRedux/issues/2772

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D13-05).
