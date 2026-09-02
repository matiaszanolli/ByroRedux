# Batch: REN-D6/D8 material-boundary + presentation nits

## #3590 — REN-D6-03: particle boundary drops `MaterialInfo.greyscale_lut_map`

`extract_particle_material` (`crates/nif/src/import/walk/mod.rs`) harvests
`texture_path`/`src_blend`/`dst_blend`/`effect_shader` from `MaterialInfo`
but not `greyscale_lut_map`. `pack_effect_shader_flags` still sets the
`MAT_FLAG_EFFECT_PALETTE_COLOR`/`_ALPHA` bits on `ParticleEmitter::
effect_shader_flags` from the effect-shader payload, but with no LUT handle
carried through, `emit_particles` (`byroredux/src/render/particles.rs`)
hardcodes `greyscale_lut_index: 0`, so the forwarded palette bits are
structurally inert (gated `!= 0u` in the shader, correctly, but nothing is
ever non-zero). Fix: thread `greyscale_lut_map` through `ParticleMaterial`
→ `ImportedParticleEmitter{,Flat}` → `ParticleEmitter`, resolve + forward
the handle at both spawn sites.

## #3591 — REN-D6-04: `Material::parallax_height_in_alpha` not covered by the canonical-completeness harness

`byroredux/src/material_translate.rs`'s `kitchen_sink_source()` /
`translate_material_copies_every_canonical_field` harness is supposed to
fail if any `translate_material` copy line is reverted. #3530 added
`parallax_height_in_alpha` (the 60th `Material` field) without adding it
to the harness — deleting that copy line currently leaves the harness
green. Sibling of #3462 (4 other uncovered fields); this field postdates
that issue. Fix: add `parallax_height_in_alpha: true` to
`kitchen_sink_source()` + an assertion, ideally alongside #3462's fix.

**STALE — already fixed.** Verified at HEAD: `kitchen_sink_source()` already
sets `parallax_height_in_alpha: true` (with a `#3462` comment) and
`translate_material_copies_every_canonical_field` already asserts
`assert!(material.parallax_height_in_alpha, "#3462");`. Landed in
`847426fa` ("Close four test-coverage gaps whose guards overstated their
own coverage", `Fix #3462: pin the five unasserted translate_material
copies, mechanically... water_shader_flags, is_water_shader, ior,
parallax_height_in_alpha, and two of effect_shader_flags' three
contributors") — exactly what this issue's own body suggested ("Fold the
same edit into #3462's fix so the harness closes on all five at once").
`847426fa` only carried a `Fix #3462` keyword, not `Fix #3591`, so GitHub
never auto-closed this one despite the fix landing. Confirmed the harness
genuinely catches a revert: temporarily changed the copy line to a
literal `false` and reran `translate_material_copies_every_canonical_field`
— it fails at the `"#3462"` assertion, as expected — then restored it via
`git checkout`. No code change needed; closing with a comment pointing at
`847426fa`.

## #3594 — REN-D8-04: two transcription defects from the `b28acb0c`/#3426 relocation

1. `crates/renderer/src/vulkan/presentation.rs:645-651`
   (`_UI_OVERLAY_DEFENSIVE_STATE_INVARIANT`): the assertion message lost its
   `\` line-continuation when the block moved out of `geometry_pass.rs`,
   leaving 18 literal mid-sentence spaces in the compiled string.
2. `crates/renderer/src/vulkan/context/post_passes.rs:1095-1104`: the
   relocated warn-once tests a strictly wider condition
   (`overlay.is_none() && ui_instance_idx.is_some()`) than its message
   describes (only the "global-only" cause) — it now also fires when
   `mesh_registry.get(ui_quad)` misses, a different cause the surrounding
   comment already documents but the log text doesn't. Cosmetic only (a
   panic string nobody has hit, and a warning that would name the wrong of
   two adjacent causes).

## #3595 — REN-D8-05: sRGB swapchain-format fallback has no warning

`crates/renderer/src/vulkan/swapchain.rs`'s `choose_surface_format` prefers
`B8G8R8A8_SRGB` + `SRGB_NONLINEAR` but silently falls back to `formats[0]`
with no log. #3426 documented an exact-colour-round-trip invariant (Ruffle
UI overlay) that depends on this pairing holding; nothing enforces or
reports it if it doesn't. No live impact (the pair is present on every
targeted desktop driver). Fix: one `log::warn!` on the fallback arm.

## Domain

#3590 spans `byroredux-nif` (import/walk), `byroredux-core` (ParticleEmitter
component) and `byroredux` binary (render/particles.rs) — classified
**nif** as the primary boundary-owning crate, `byroredux`/`byroredux-core`
as secondary test targets. #3591 → `byroredux` binary (material_translate.rs
lives there, despite the "renderer" issue label). #3594/#3595 →
`byroredux-renderer`.
