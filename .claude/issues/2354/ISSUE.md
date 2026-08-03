# SF-D8-03: NIFAL particle slice is structurally unreachable on Starfield — not documented as N/A

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2354
**Labels**: bug,nif-parser,medium,legacy-compat

---

**Severity**: MEDIUM
**Dimension**: 8 — NIFAL Canonical Material Translation (Starfield audit, 2026-08-03)
**Location**: `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params`/`extract_emitter_rate`, currently ~line 721 — the audit report's cited 536-563 has drifted with recent edits), `docs/engine/nifal.md`
**Status**: NEW, CONFIRMED against current code

## Description

The NIFAL particle slice (`extract_emitter_params`/`extract_emitter_rate` in `walk/mod.rs`, dispatching on `NiPSysEmitter`/`NiPSys*FieldModifier` blocks) is structurally unreachable on Starfield content — the full Meshes01 per-block histogram (31,058 files, 22 distinct block types) contains zero `NiPSys*`/`NiParticleSystem` blocks. Starfield authors particle systems entirely outside the NIF container (confirmed: Dimension 2's BSGeometry-only import path has no NiNode/NiParticleSystem hierarchy for Starfield content).

This isn't a silent drop of translatable data — it's a structural inapplicability — but nothing in `docs/engine/nifal.md` or the compat matrix states the slice is inapplicable to Starfield, so the NIFAL particle regression suite (#1411/#1434/#1445/#1771/#1775, all Oblivion/FO3/FNV/Skyrim-driven) silently says nothing about Starfield coverage.

## Evidence

- `walk/mod.rs`: `extract_emitter_params`/`extract_emitter_rate` downcast against `NiPSysEmitter`/`NiPSysGravityFieldModifier`/etc. — confirmed present and gated on those block types.
- Dimension 2 of this audit's block-type histogram over the 31,058-file Meshes01 corpus found zero `NiPSys*` blocks.
- `docs/engine/nifal.md` has no "Starfield: particle slice N/A" note.

## Impact

Documentation/tracking gap only — no functional defect. Risk is a future format discovery (e.g. Starfield DLC content authoring particles differently) silently passing the existing test suite instead of flipping a corpus-baseline assertion red.

## Suggested Fix

Record "Starfield: particle slice N/A (particles authored outside the NIF container)" in `docs/engine/nifal.md` and the compat matrix, and add a corpus-baseline assertion (zero `NiPSys*` blocks across the Starfield corpus) so a future format discovery flips a test red instead of passing silently.

## Completeness Checks
- [ ] **SIBLING**: Confirm the same is true for FO76 (also BA2/BGSM/BSGeometry-era) if/when an FO76 audit runs
- [ ] **CANONICAL-BOUNDARY**: This is a documentation-only fix to `docs/engine/nifal.md`; no code path change. See `/audit-nifal`.
- [ ] **TESTS**: Corpus-baseline assertion added (zero `NiPSys*` on Starfield corpus) so future format drift is caught
