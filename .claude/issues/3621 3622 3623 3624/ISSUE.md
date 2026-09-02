# Batch: OBL-D1 NIF version-gating gaps + REN-D19 POM/shader-contract findings

## #3621 — OBL-D1-01: `NiTexturingProperty::Apply Mode` missing `since=3.3.0.13`

`crates/nif/src/blocks/properties.rs`, `NiTexturingProperty::parse`. `apply_mode`
is read for every `version <= STRING_TABLE_THRESHOLD` with no lower bound;
nif.xml gates it `since="3.3.0.13" until="20.1.0.1"`. Zero exposure on the
Oblivion corpus (lowest version present is exactly 3.3.0.13). Fix: add the
lower bound, default to `APPLY_MODULATE` below it.

## #3622 — REN-D19-03: POM marchers agree on channel, disagree on mip

`crates/renderer/shaders/include/material_sampling.glsl`
(`sampleParallaxHeight`) samples with implicit-LOD `texture()` inside a
data-dependent-`break` loop (undefined derivatives, pre-existing, not
observably broken); `ray_hit.glsl`'s secondary-ray marcher uses explicit
`textureLod(..., 0.0)`. Suggested fix: compute LOD once before the loop and
switch to `textureLod`, but the issue itself flags this needs to land behind
an A/B capture per the project's "no speculative shader changes" rule.

## #3623 — OBL-D1-02: dark/detail/gloss/glow `TexDesc` gated on `texture_count`, nif.xml gates none of them

Same function as #3621. `dark_texture`/`detail_texture`/`gloss_texture`/
`glow_texture` are gated `texture_count > 1..4`; nif.xml's `Has …` bools for
these four are unconditional — only Bump/Normal/Parallax/Decals carry a
`Texture Count` condition. Zero exposure on Oblivion (`texture_count == 7`
on all 30,121 instances in the corpus). Fix: read the four bools
unconditionally, keep the count-gates only on Bump/Normal/Parallax/Decals.

## #3624 — REN-D19-04: `parallax_alpha_height_bit_is_masked_and_honoured_by_every_reader` doesn't enforce "every reader"

`crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:2012-2067`.
The test is a 3-file whitelist with an at-least-once substring check —
doesn't catch a 4th file with an unmasked read, or a 4th unmasked read added
to an already-listed file. Currently true by inspection (4 real read sites,
all masked). Fix: enumerate all shader files at test time and assert every
`parallaxMapIndex` occurrence (outside the `bindings.glsl` declaration) is
immediately followed by the mask. Issue also asks the same treatment for the
sibling `NORMAL_ALPHA_SPEC_BIT`.

## Domain

#3621/#3623 → `byroredux-nif` (nif). #3622/#3624 → `byroredux-renderer`
(renderer) — #3624 is a pure-Rust test file (no Vulkan risk); #3622 touches
live shader source and is handled per the project's no-speculative-shader-
changes constraint.
