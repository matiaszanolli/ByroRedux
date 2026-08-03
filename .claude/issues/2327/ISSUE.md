# SKY-D7-02: Authored refraction_strength discarded for every Skyrim material that isn't fire-refraction

**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-08-03.md` (Dimension 7)
**GitHub issue**: #2327

**Severity**: MEDIUM
**Location**: `byroredux/src/material_translate.rs:34-44` (`material_optical_scalar`) and `:180`; producer `crates/nif/src/import/material/dedicated_shader.rs:307,333-350`

## Description

`material_optical_scalar` only returns the authored `refraction_strength`
when `material_kind == MATERIAL_KIND_FIRE_REFRACTION`; every other kind gets
a constant `DEFAULT_DIELECTRIC_IOR` (1.5), silently discarding the authored
scalar. Ordinary Skyrim refractive-glass/ice/crystal authoring (SLSF1
`Refraction` alone, without `FIRE_REFRACTION`) hits this. Distinct from open
#2232 (`GpuMaterial.ior`'s triple-meaning overload being undocumented) and
from open #2284 (6 other authored-but-dropped scalars, which explicitly
excludes `refraction_strength`).

## Evidence

Confirmed at HEAD (1ae86f62): for any `material_kind` other than
`MATERIAL_KIND_FIRE_REFRACTION`, `material_optical_scalar` returns
`DEFAULT_DIELECTRIC_IOR`, discarding the authored `refraction_strength`
captured at `crates/nif/src/import/material/dedicated_shader.rs:307`.

## Impact

Skyrim refractive surfaces render as ordinary dielectrics (IOR 1.5, no
authored distortion) or, if a glass texture-keyword happens to fire, at the
engine's fixed glass IOR (1.45) regardless of what the artist authored.
Shading fidelity only — no wrong material *kind*, no fabrication — hence
MEDIUM not HIGH.

## Suggested Fix

Either pack a `MAT_FLAG_REFRACTION` bit and let the scalar ride an
un-overloaded canonical field, or explicitly document the
non-`FIRE_REFRACTION` discard as deliberate in both `material_translate.rs`
and `nifal.md`.

## Completeness Checks
- [ ] **SIBLING**: Check FO4/FO76/Starfield's equivalent refraction-kind dispatch for the same discard pattern
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins whichever fix is chosen
