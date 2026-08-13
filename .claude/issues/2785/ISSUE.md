# REN-D15-04: WaterMaterial::fog_near travels EXAL water arm but nothing reads it

## Description
`WaterMaterial::fog_near` travels the whole EXAL water arm and **nothing reads it**. Absorption is keyed exclusively on `fog_far` through a hard-coded `exp(-2t)` curve identical for every water body in every game, so authored per-WATR clarity is ignored. The dead slot sits in a block whose own doc says 128 B is "exactly the Vulkan 1.1 spec minimum … no further growth is possible", so a WATAL §5.1 promotion will have to displace something. Same shape as the already-fixed `wave_amplitude` gap (#1936/#1969).

## Location
`byroredux/src/env_translate.rs` → `crates/renderer/src/vulkan/water.rs` (`WaterPush::shallow.a`) → `crates/renderer/shaders/water.frag`

## Severity / Domain / Type
low / renderer / bug

https://github.com/matiaszanolli/ByroRedux/issues/2785

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D15-04).
