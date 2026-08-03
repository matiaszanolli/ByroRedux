# SF-D9-2026-08-03-03: Starfield .mat merge forwards zero authored material data, and the CDB Phase-2 deferral has no open tracker

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2359
**Labels**: bug,import-pipeline,medium,legacy-compat

---

**Severity**: MEDIUM (severity is for the untracked deferral, not the closed Phase-1 work)
**Dimension**: 9 — BGSM/BGEM External Material Flow (Starfield audit, 2026-08-03), corroborated by Dimension 3
**Location**: `byroredux/src/asset_provider/material.rs:710-723` (the `.mat` arm), `:331-376` (`has_starfield_cdb`/`register_starfield_cdb`)
**Status**: NEW as a *tracking* finding — the deferral itself is documented in-code; #1289/#1290 are both CLOSED and ROADMAP.md has no Phase-2 row

## Description

The `.mat` arm is a two-statement stub: flips `material.is_pbr = true` and returns. No texture role, metalness/roughness, alpha/blend, or two-sided/decal state is extracted from the CDB. `register_starfield_cdb` deliberately does a header-only `probe_header` — the class/instance tree is never walked, so there is currently no code path from CDB contents to `ImportedMaterial` at all.

## Evidence

- `material.rs:710-723`: `if path.ends_with(".mat") && provider.has_starfield_cdb() { material.is_pbr = true; ... return true; }` — confirmed no further extraction.
- `material.rs:331-376` (`register_starfield_cdb`): confirmed does `ComponentDatabaseFile::probe_header(bytes)` only, never walks the ~1.44M-instance tree, per its own doc comment.
- Verified `#1289` and `#1290` are both `CLOSED` (`gh issue view`).
- Verified `ROADMAP.md` has no "CDB Phase 2" row — the only `#1289` mention (line 179) is a historical note about the `.mat` wiring that shipped, not a Phase-2 tracker.
- Independently corroborated from the reader side by this audit's Dimension 3 (`crates/sfmaterial`): CDB parses cleanly end-to-end (97 classes / 1.44M instances) but no consumer walks it for per-field material data.

## Impact

Every Starfield surface renders with NIF-derived, keyword-classified metalness/roughness under the Disney BSDF lobe and whatever textures the NIF happened to carry — the classic "chrome/posterized" symptom for any surface whose real maps live in the CDB. Blast radius is all Starfield rendering (compounded by SF-D8-02, #2353: ~189,801 of 190,549 surfaces reach the Disney lobe as untextured, matte, fully-dielectric white). This is currently the single highest-value remaining item for Starfield visual fidelity with **no open issue or ROADMAP row tracking it**.

## Suggested Fix

File this issue as the tracker (done). Pin the checklist invariant ("`.mat` paths land in named `MaterialTextureSet` roles, never a CDB slot index") now with a test, so it's enforced before the extraction code exists. Add a "CDB Phase 2" row to ROADMAP.md pointing here.

## Completeness Checks
- [ ] **SIBLING**: N/A — CDB is Starfield-only, no cross-game sibling
- [ ] **CANONICAL-BOUNDARY**: Future extraction work must land as CDB-authored values flowing into `ImportedMaterial` at the same merge boundary (`asset_provider::merge_external_material`), never a render-time fallback. See `/audit-nifal`.
- [ ] **TESTS**: Add the checklist-invariant test now (before Phase-2 extraction code exists) pinning "`.mat` paths never resolve to a raw CDB slot index outside `MaterialTextureSet`"
