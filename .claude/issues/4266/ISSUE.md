# OB-D7-01: docs/feature-matrix.md's Cell Loading table is stale — still shows Oblivion exterior bench as pending, a month after it landed

**Issue**: #4266 — https://github.com/matiaszanolli/ByroRedux/issues/4266
**Labels**: medium,doc-rot,game:oblivion,legacy-compat,documentation

**Severity**: MEDIUM
**Dimension**: Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks
**Location**: `docs/feature-matrix.md:20,23,25-28`
**Status**: NEW

## Description
`docs/feature-matrix.md`'s Cell Loading table reads "bench pending" (line 20) / "device check pending" (line 23) for Oblivion's exterior grid and confirmed-bench rows, and its own explanatory note (lines 25-28) says "only an on-device exterior render bench is pending." This has been false since commit `f90e4eec` (2026-08-12, Fix #2368), which recorded a real on-device measurement and updated `ROADMAP.md` and `docs/engine/exterior-readiness-plan.md` — but never touched `feature-matrix.md`, which has since been edited four more times for unrelated FNV/Skyrim/FO4 bench-number updates in the same table without ever correcting the Oblivion row.

## Evidence
Live confirmed during this audit: `docs/feature-matrix.md:20` reads `| **Exterior grid (7×7)** | bench pending | ✓ | ✓ | ✓ | ✓ | — | ✓ |` and line 23 reads `| **Confirmed bench** | device check pending | device check pending | 3 146 ent · 74.0 FPS TAA | ... |` for the Oblivion column — both stale. The real 2026-08-12 measurement: Tamriel `(0,0)` radius 1, 6,043 entities / 2,355 draws, image-health + environment-value gates both clean.

## Impact
Discoverability gap: a reader consulting `feature-matrix.md` (rather than `ROADMAP.md` or `docs/engine/exterior-readiness-plan.md`) concludes Oblivion's exterior-cell rendering is still unverified, when it has been closed for a month.

## Related
Adjacent to the already-closed #2368 (the fix that produced the real measurement) and #2377 (the ongoing readiness-matrix tracker).

## Suggested Fix
Change line 20's Oblivion cell to "✓", line 23's to "6,043 ent / 2,355 draws (image-health + env-value gates clean, 2026-08-12, #2368)" (entities/draws, not FPS — do not fabricate an FPS figure to match the row's other cells), and rewrite lines 25-28's note accordingly. FO3 remains genuinely pending and should stay as-is.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_OBLIVION_2026-09-11.md — findings verified against live code during this publish run.*
