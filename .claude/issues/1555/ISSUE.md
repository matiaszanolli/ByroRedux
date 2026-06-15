# Issue #1555: OBL-D7-NEW-01: ROADMAP Oblivion clean-parse rate understated

_Snapshot as filed via /audit-publish from docs/audits/AUDIT_OBLIVION_2026-06-14.md. GitHub is authoritative for current state._

**Severity**: LOW · **Dimension**: 7 (Doc Staleness) — tech-debt / documentation · **Status**: NEW

**Location**: `ROADMAP.md:197`, `ROADMAP.md:733`

## Description
ROADMAP states Oblivion "96.24% (7730/8032)"; live `nif_stats` over `Oblivion - Meshes.bsa` (2026-06-15) gives **8024/8032 = 99.90% clean, 0 failures, 8 truncated**. The #1506–#1509 family fixes landed since the last sweep. ROADMAP understates the rate and is internally inconsistent (line 73 already quotes a 99.99% recover rate). (Merges the duplicate OBL-D6-NEW-02 reported by Dimension 6.)

## Evidence
Live `nif_stats` run (Dimensions 1 + 6).

## Impact
Doc only; misleads anyone gauging Oblivion readiness.

## Suggested Fix
Refresh the ROADMAP Oblivion compat-matrix row and project-stats line to the live 99.90% (and 99.92% archive-aggregate). Per CLAUDE.md, ROADMAP is the authoritative source — fix it there.

## Completeness Checks
- [ ] **SIBLING**: Both ROADMAP sites (compat matrix + project stats) updated consistently
- [ ] **TESTS**: N/A (doc-only change)
