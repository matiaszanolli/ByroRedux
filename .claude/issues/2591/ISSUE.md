# SKY-D7-03: EmissiveSource::None's documented contract is contradicted by the unconditional Lighting tag -- on Skyrim the discriminator degenerates to has a BSLightingShaderProperty

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2591
**Finding ID**: SKY-D7-03

**Severity**: LOW
**Dimension**: NIFAL Canonical Material Translation (Skyrim slice)
**Location**: contract `crates/core/src/ecs/components/material.rs:452-457`; Skyrim set-site `dedicated_shader.rs:298-300`
**Status**: NEW

## Description
`apply_bs_lighting_shader` sets `EmissiveSource::Lighting` unconditionally regardless of whether `emissive_color`/`emissive_multiple` are actually non-zero. Vanilla Skyrim ships the overwhelming majority of BSLSP blocks with an unauthored `[0,0,0]`/`1.0` emissive, all tagged `Lighting` anyway — the discriminator carries no emissive-authoring information on Skyrim, contrary to its own doc's parenthetical.

## Evidence
Confirmed directly: `dedicated_shader.rs:298-300` — `info.emissive_source = ...EmissiveSource::Lighting;` set with no check on the emissive values.

## Impact
None at runtime today (no `GpuMaterial` field, no shader branch reads it yet). Cost is that the #1280 discriminator doesn't yet answer the question its doc promises.

## Related
#1280, #166

## Suggested Fix
Either amend the doc to describe actual behavior, or gate the three set-sites on a non-zero emissive contribution.

## Completeness Checks
- [ ] **TESTS**: If gated, a regression test confirms `EmissiveSource::Lighting` is only set for non-zero emissive authoring
