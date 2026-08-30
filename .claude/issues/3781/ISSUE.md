# #3781: SF-D3-01: the CDB Phase 2 spike sizes against one 105 MB CDB, but two full-size CDBs ship — 13 CDBs / 3.08M chunks corpus-wide, projecting ~18 GB not 9.19 GB

**Labels**: documentation, import-pipeline, low, legacy-compat, game:starfield, doc-rot
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_STARFIELD_2026-08-30.md` — SF-D3-01 (LOW)
**Dimension**: 3 — CDB material database
**Location**: `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md:190` (the 9.19 GB sizing paragraph)

## Description

The Phase 2 spike scopes the work against *"a 105 MB CDB"* peaking at **9.19 GB RSS** — singular. There are **two** full-size CDBs, and thirteen in total.

## Evidence

Measured across all 129 installed Starfield archives:

- `SFBGS007 - Main.ba2` carries a **second** `materialsbeta.cdb`: **104,868,172 B / 1,458,383 chunks / 97 classes** — within 0.2% of the base CDB on every axis.
- Corpus total: **3,077,172 chunks across ~232 MB in 13 CDBs**, versus the 1,457,575 chunks the 9.19 GB figure was measured on.

A Phase-2 reader reusing the current `parse` across the discovered set would peak north of **18 GB** on a 29 GB machine.

## Impact

Planning-accuracy correction. Nothing calls `parse` today, so there is no runtime defect — but Phase 2's memory budget, which is the whole reason the spike exists, is scoped against roughly half the real input. Anyone sizing an incremental/streaming reader from the spike doc will under-provision by ~2×.

**Scope note carried forward from the audit**: this run deliberately did **not** attempt the 9.19 GB full parse (stated scope limit, not a defect). The chunk/class counts above come from header + chunk-table walks, not a full materialisation.

## Suggested Fix

Amend the spike doc's sizing paragraph with the per-CDB table (Dimension 3 of the audit report): 13 CDBs, two of them full-size, 3,077,172 chunks, ~232 MB, and the resulting ~18 GB projection for a naive whole-corpus `parse`. State explicitly that the 9.19 GB figure is a per-CDB measurement on the base CDB only.

## Related

- #3398 — Starfield CDB Phase 2 (the work this doc scopes)
- `sf_cdb_phase2_unblocked` project memory note

## Completeness Checks
- [ ] **SIBLING**: Check whether `ROADMAP.md` or the `/audit-starfield` skill restate the single-CDB sizing
- [ ] **TESTS**: N/A — documentation-only
