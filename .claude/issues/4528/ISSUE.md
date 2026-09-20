# REN-6-2026-09-20-01: an explicit Height flipbook leaves the parallax .a-vs-.r gate keyed on the normal's alpha — active_has_alpha(FlipTextureRole::Height) is recorded but never read

- **ID**: REN-6-2026-09-20-01
- **Labels**: low,renderer,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: NIFAL Material
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-6-2026-09-20-01)

**Location**: `byroredux/src/anim_convert.rs:272` (records `active_has_alpha(FlipTextureRole::Height)` per frame) — zero readers; gate consumer in material_translate/render

**Description**
The #3562/#4260/#4301 flipbook family closed the Normal half; the Height half is dormant: it needs parallax_height_in_alpha (0 of 35,322 vanilla Oblivion NIFs) plus a TexType-7 flip to fire, but then both polarity failures are possible (wrong-channel swim or spurious POM zeroing).

**Evidence**
Audit D6, 2026-09-20: the recorded lane has no reader (grep).

**Impact**
Latent wrong-parallax class on modded content only.

**Suggested Fix**
Read the recorded lane when a Height flipbook is active and key the gate on it; pin with a flip fixture.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
