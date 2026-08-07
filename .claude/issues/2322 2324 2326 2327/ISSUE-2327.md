title:	SKY-D7-02: Authored refraction_strength discarded for every Skyrim material that isn't fire-refraction
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, legacy-compat, medium, nif-parser
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2327
--
**Severity**: MEDIUM
**Location**: `byroredux/src/material_translate.rs:34-44` (`material_optical_scalar`) and `:180`; producer `crates/nif/src/import/material/dedicated_shader.rs:307,333-350`

## Description

`material_optical_scalar` only returns the authored `refraction_strength`
when `material_kind == MATERIAL_KIND_FIRE_REFRACTION` (synthesized only when
both SLSF1 `REFRACTION` and `FIRE_REFRACTION` bits are set); every other
kind gets a constant `DEFAULT_DIELECTRIC_IOR` (1.5), silently discarding the
authored scalar. Ordinary Skyrim refractive-glass/ice/crystal authoring
(SLSF1 `Refraction` alone, without `FIRE_REFRACTION`) hits this — the flag
isn't packed into any `material_flag::*` bit either. This is distinct from
open issue #2232 (which documents `GpuMaterial.ior`'s triple-meaning-overload
being undocumented) and from open issue #2284 (which covers 6 other
authored-but-dropped scalars — `lighting_effect_1/2`,
`subsurface_rolloff`, `rimlight_power`, `backlight_power`,
`fresnel_power` — explicitly excluding `refraction_strength`, which #2284
states already "completes the translate step" for the fire-refraction case
only).

## Evidence

```rust
// byroredux/src/material_translate.rs:34
fn material_optical_scalar(material_kind: u32, refraction_strength: f32) -> f32 {
    if material_kind == byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION {
        if refraction_strength.is_finite() {
            refraction_strength.clamp(0.0, 1.0)
        } else {
            ...
        }
    } else {
        byroredux_core::ecs::components::material::DEFAULT_DIELECTRIC_IOR
    }
}
...
// :180
ior: material_optical_scalar(source.material_kind, source.refraction_strength),
```

Confirmed present at HEAD (1ae86f62): for any `material_kind` other than
`MATERIAL_KIND_FIRE_REFRACTION`, the authored `refraction_strength` captured
at `crates/nif/src/import/material/dedicated_shader.rs:307` is read into
`ImportedMaterial` but never reaches `ior` — it is discarded in favor of the
hardcoded `DEFAULT_DIELECTRIC_IOR`.

## Impact

Skyrim refractive surfaces render as ordinary dielectrics (IOR 1.5, no
authored distortion) or, if a glass texture-keyword happens to fire, at the
engine's fixed glass IOR (1.45) regardless of what the artist authored.
Shading fidelity only — no wrong material *kind*, no fabrication — hence
MEDIUM not HIGH.

## Suggested Fix

Either pack a `MAT_FLAG_REFRACTION` bit and let the scalar ride an
un-overloaded canonical field, or explicitly document the non-`FIRE_REFRACTION`
discard as deliberate in both `material_translate.rs` and `nifal.md` — today
it reads as an oversight at the call site.

## Completeness Checks
- [ ] **SIBLING**: Check FO4/FO76/Starfield's equivalent refraction-kind dispatch for the same discard pattern
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins whichever fix is chosen (either the new flag path, or the documented-discard invariant)

