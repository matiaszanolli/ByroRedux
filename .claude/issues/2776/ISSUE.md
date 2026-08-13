# REN-D14-NEW-03: caustic.rs tune.x upload side unpinned against CAUSTIC_FIXED_SCALE

## Description
Nothing pins that the uploaded `tune.x` equals the `CAUSTIC_FIXED_SCALE` composite divides by — the value travels by two channels (runtime UBO lane vs. compile-time `#define`) and the comment claiming otherwise names two `shader_constants` tests that check neither. The composite side *is* pinned; the **upload** side is the unpinned link. Failure mode if `tune.x` ever becomes a live tunable: silent global brightness error on every caustic pixel.

## Location
`crates/renderer/src/vulkan/caustic.rs` (`CausticParams` in `dispatch`)

## Severity / Domain / Type
low / renderer / bug

https://github.com/matiaszanolli/ByroRedux/issues/2776

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D14-NEW-03).
