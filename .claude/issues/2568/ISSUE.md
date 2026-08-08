# OBL-D4-01: Oblivion legacy particle stack (NiParticleSystemController / NiAutoNormalParticles) is fully parsed but produces no ImportedParticleEmitter

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2568
**Finding ID**: OBL-D4-01

**Severity**: HIGH
**Dimension**: Rendering Path for Oblivion Shaders
**Location**: `crates/nif/src/import/walk/mod.rs:531-568`, `crates/nif/src/blocks/legacy_particle.rs:318-361`, `crates/nif/src/blocks/mod.rs:375-400`
**Status**: NEW

## Description
The particle-emission site downcasts only to the modern `NiParticleSystem`/`NiPSysEmitter*` shape. Oblivion's *other* particle stack — `NiParticleSystemController`/`NiBSPArrayController` + `NiAutoNormalParticles`/`NiRotatingParticles`, whose own dispatcher comment reads "Oblivion magic FX, fire, dust, blood" — dispatches to `legacy_particle::*` and matches neither downcast, so it imports with zero emitters. The in-code comment at the emission site claims "the target games all author the modern NiParticleSystem stack" — true for FO3/FNV/Skyrim+, false for Oblivion, which is exactly the title this dimension exists to cover. `legacy_particle::NiParticleSystemController` already decodes a superset of the needed spawn parameters (speed, declination, birth_rate, lifetime, emitter_dimensions, etc.) — it's parsed, just never consumed. `crates/nif/src/import/tests/particle.rs:23,86` pins the current gap as intended behaviour ("deliberately not surfaced (#1327)"), i.e. it's test-locked in.

## Evidence
Confirmed directly: `walk/mod.rs:531-543`'s comment states "The legacy NiParticleSystemController / NiAutoNormalParticles / NiRotatingParticles types dispatch to legacy_particle::*, NOT NiPSysBlock... that dead arm was removed in #1327. The target games all author the modern NiParticleSystem stack." — the emission code only downcasts `NiParticleSystem`.

## Impact
Every Oblivion FX asset using the legacy stack (torch fire, magic-effect shaders, dust, blood, smoke) renders as static geometry with zero particles — no parse error, no log, looks like "Oblivion has no fire." This is the top concrete Oblivion-specific content gap in the render path.

## Suggested Fix
Add a second emission arm downcasting `legacy_particle::NiParticleSystemController` (+ `NiBSPArrayController` alias), map its fields onto `ImportedEmitterParams`, reuse the existing NIFAL-S3 finite/positivity filter verbatim, and update the #1327 tests to assert the new arm.

## Completeness Checks
- [ ] **TESTS**: `crates/nif/src/import/tests/particle.rs:23,86`'s "deliberately not surfaced" assertions are updated to assert the new arm's emission instead
- [ ] **CANONICAL-BOUNDARY**: New emitter mapping reuses the existing NIFAL-S3 finite/positivity filter, not a re-derived one
