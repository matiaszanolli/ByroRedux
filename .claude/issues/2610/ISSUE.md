# FO4-D7-05: particle DrawCommands hardcode effect_shader_flags:0, dropping BGEM particle flags

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2610
**Finding ID**: FO4-D7-05

**Severity**: LOW
**Dimension**: 7 (Canonical Material)
**Location**: `byroredux/src/render/particles.rs:234`
**Status**: NEW

## Description
Particle `DrawCommand`s hardcode `effect_shader_flags: 0` instead of
sourcing the value from the canonical `Material`, dropping BGEM particle
palette-remap and `EFFECT_SOFT`/`EFFECT_LIT`/`PBR_BSDF` bits for
particle-driven effects.

## Evidence
`byroredux/src/render/particles.rs:234` — `effect_shader_flags: 0` literal,
not read from the particle's resolved `Material`.

## Impact
BGEM-authored particle effects (palette-remap, soft-particle blending, lit
particles, PBR particle shading) never reach the renderer for particle
draws, even when the source BGEM correctly authors those flags.

## Suggested Fix
Source `effect_shader_flags` from the particle's resolved canonical
`Material` instead of hardcoding zero.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Read from canonical `Material`, don't re-derive from BGEM in the renderer
- [ ] **TESTS**: A regression test with a BGEM particle fixture carrying non-zero effect flags pins the forwarded value
