# OB-D4-02: scene-level first-match particle-emitter attribution is wrong for 67% of Oblivion's multi-emitter NIFs (regression of #1402)

**Issue**: #4261 — https://github.com/matiaszanolli/ByroRedux/issues/4261
**Labels**: medium,nif-parser,nif,game:oblivion,legacy-compat,bug

**Severity**: MEDIUM
**Dimension**: Dimension 4 — Rendering Path for Oblivion Shaders
**Location**: `crates/nif/src/import/walk/emitter.rs (extract_emitter_params, extract_emitter_max_particles, extract_emitter_rate)`
**Status**: NEW

## Description
The three particle-emitter parameter extractors in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params`, `extract_emitter_max_particles`, `extract_emitter_rate`) scan the whole parsed NIF scene with `find_map` and use the first matching block for every `ImportedParticleEmitter` instance in the file, rather than pairing each emitter instance with its own emitter block. This is the exact limitation #1402 ("ANIM-08") was filed and closed against — but that fix only added a documenting comment ("deferred until a regression surfaces"), not a behavior change, and the underlying first-match scope is unchanged in the current code.

## Evidence
Measured against the real Oblivion + DLC mesh corpus: 208 NIFs carry a `NiPSysEmitter`; 140 of those (67.3%) carry more than one, and 139 import multiple `ImportedParticleEmitter`s that all receive identical kinematics from the first emitter block in the file. Real content hit: `transformation.nif` (13 emitters), `obgatemini01.nif` (11), `restoration.nif` (11), `skeleton.nif` (10), `drain.nif` (9), `invisibility.nif` (8). This is exactly the "regression surfaces" condition #1402's deferred-fix comment named as the trigger to revisit.

## Impact
Spell VFX and Oblivion-gate effects lose their authored multi-emitter layering (e.g. fast core + slow haze collapse to a single set of kinematics) on 67% of the emitter-bearing Oblivion corpus — a widespread, measurable visual-fidelity gap.

## Related
Regression of #1402 (closed 2026-06-01 with a comment-only fix, not a behavior change; the code comments at `emitter.rs:238-245,340-352` still explicitly document the first-match limitation as deferred).

## Suggested Fix
Rework the three extractors to pair each `NiPSysEmitter` instance with its own parameter blocks (color curve, rate, max particles) by walking from the specific emitter rather than scanning the whole scene for the first match, so each `ImportedParticleEmitter` gets its own authored kinematics.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_OBLIVION_2026-09-11.md — findings verified against live code during this publish run.*
