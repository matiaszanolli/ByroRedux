# NIFAL-D5-2026-09-29-01: nifal.md §2 Particles still calls the legacy-data and #3329 sequence rate tiers "whole-scene" after #4560 and #4620 made both per-instance

**Labels**: low,documentation,doc-rot,nifal,nif-parser

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-29.md`
**Severity**: LOW
**Dimension**: Particles · **Tier Violated**: doc / record-keeping · **Game Affected**: Oblivion, FO3, FNV, Skyrim
**Location**: `docs/engine/nifal.md` §2 Particles, Attribution paragraph: "Still whole-scene: the legacy `NiParticleSystemController` fallback and the #3329 sequence tier (a sequence's controlled block names an emitter controller, not an emitter instance)". Also `.claude/commands/audit-nifal/SKILL.md` Dim 5.

## Description
Both tiers now resolve through `find_own_emitter_ctlr_refs` (`crates/nif/src/import/walk/emitter.rs`):
- per #4560, the legacy data is `NiPSysEmitterCtlr`'s own `Data` ref, and an unlinked block is attributed to nobody;
- per #4620, a non-null `cb.controller_ref` must equal the system's own controller, and a null ref falls back to the system name.

The paragraph's stated reason ("names an emitter controller, not an emitter instance") is exactly what #4620 disproved. The only remaining whole-scene piece is the budget fallback when the own `data_ref` does not resolve. The audit-nifal skill's Dim 5 text carries the same stale claim. Introduced by `a6bcec6e2` + `b7491072f`, neither of which touched nifal.md.

## Impact
The next particle change is planned against a boundary that no longer exists — the same failure class as #4409.

## Related
#4560, #4620, #4409, #4261.

## Suggested Fix
Replace the sentence with "Still whole-scene: only the emitter-budget scan when the own `data_ref` does not resolve", citing #4560 and #4620, and sync the audit-nifal skill Dim 5 text.

Validated at HEAD 9fcfdc3fc: nifal.md still reads "Still whole-scene: the legacy `NiParticleSystemController` …" in §2 Particles.

## Completeness Checks
- [ ] **SIBLING**: audit-nifal SKILL.md Dim 5 synced in the same change
