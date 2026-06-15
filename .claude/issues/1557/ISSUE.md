# Issue #1557: OBL-D7-NEW-03: ROADMAP #688 narrative (~149 files) stale vs live 8

_Snapshot as filed via /audit-publish from docs/audits/AUDIT_OBLIVION_2026-06-14.md. GitHub is authoritative for current state._

**Severity**: LOW · **Dimension**: 7 (Doc Staleness) — tech-debt / documentation · **Status**: NEW

**Location**: `ROADMAP.md:197`, `ROADMAP.md:716`

## Description
The #688 closeout narrative still says ~149 NetImmerse-era Oblivion files truncate; live reality is **8** (6 markers + the 2 OBL-D1-NEW-01 files). The number is ~18× overstated after the #1506–#1509 fixes.

## Evidence
Live `nif_stats` + `recovery_trace`.

## Impact
Doc only.

## Suggested Fix
Refresh the #688 narrative number to 8 (and note OBL-D1-NEW-01 will take it to 6 once fixed).

## Completeness Checks
- [ ] **SIBLING**: Both ROADMAP #688 references updated; cross-checked against OBL-D7-NEW-01's rate refresh for consistency
- [ ] **TESTS**: N/A (doc-only change)
