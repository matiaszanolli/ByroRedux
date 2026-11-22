# REN-D2-02 / REN-D6-02: the normal-alpha env-mask fallback is now a per-kind special case, and its remaining Skyrim kind-1 population is large (measure before deciding)

Labels: low, renderer, shaders, nifal, game:skyrim, enhancement
URL: https://github.com/matiaszanolli/ByroRedux/issues/7023

**Severity**: LOW
**Source**: `docs/audits/AUDIT_RENDERER_2026-11-16.md` (renderer audit, delta since 2026-11-09)
**Location**: `environmentReflectionMask` and `hasExplicitEnvironment` in `crates/renderer/shaders/triangle.frag`; `normal_alpha_spec_applies` in `byroredux/src/material_translate.rs`; guard `normal_alpha_masks_specular_intensity_not_roughness` (`crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`)
**Status**: NEW (validated against HEAD; no matching open issue)

## Description
The shader decides "is this normal alpha an env mask?" at render time by `materialKind`; only kind 11 (MultiLayerParallax) is exempt since `d9d11dc34`. Measured on vanilla Skyrim SE (22,047 NIFs, 78,146 meshes): kind 1 with a cubemap = 6,624 meshes; 3,924 of those have no env mask + a normal map + no gloss map, and 3,915 carry normal alpha (1,445 NIFs). 1x1-mip alpha proxy over those: mean 0.335, median 0.182, p90 1.0; 2,298 are below 0.25. Kind 11 with no cubemap and metalness > 0.3 reaches the block with `environmentStrength = 1.0` and an unmasked reflection (population unmeasured). The guard is a whitespace-exact `contains` of the ternary text; it does not pin that other kinds keep the fallback or that kind 11's local specular is still masked.

**Impact**: latent; possible residual under-reflection on kind 1 (visual effect unmeasured — this is an observation, not a defect claim); brittle guard.

## Suggested Fix
Measure first: masked vs unmasked A/B of kind-1 surfaces with a low-alpha normal and no env mask. Candidates (most affected NIFs in the probe): DLC01 castle portal, Dwemer astrolabe lens/hub, DLC01 snow-elf-ruins blast roof. If kind 1 shows under-reflection, carry an explicit "env mask source: none / authored / normal alpha" signal from `translate_material` rather than adding shader kind arms. Independently, relax the guard to a token match on `MATERIAL_KIND_MULTI_LAYER_PARALLAX` inside the initializer plus a second assertion that `normalAlphaSpecMask` remains in the fallback arm.

Related: open #6816 (FO3/FNV env-mask classifier). The ice fix itself shipped in `d9d11dc34`.
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
