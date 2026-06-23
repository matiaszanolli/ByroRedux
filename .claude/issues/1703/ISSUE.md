# TD3-002: feature-matrix "What Doesn't Work Yet" table still lists M45/M47.2 as gaps

Issue: #1703 · Labels: low, tech-debt, documentation
Source: docs/audits/AUDIT_TECH_DEBT_2026-06-23.md

**Severity**: LOW
**Dimension**: 3 (Stale Documentation)
**Location**: `docs/feature-matrix.md` ("What Doesn't Work Yet (live gaps as of 2026-06-02)" table)

## Description
The "What Doesn't Work Yet" gaps table lists two shipped milestones as live gaps "as of 2026-06-02": `Papyrus transpiler (M47.2) | Script execution on real content | M47.2 (Tier 3)` and `Save / load (M45) | Game sessions persist | M45 (unstarted)`. The `.pex` recognizer slice and M45/M45.1 both shipped after that date.

## Evidence
Same commit set as TD3-001: `bd2d0de2`/`48e18c4f` (M45/M45.1, `crates/save/`) and `fcd46e90`/`92560525`/`f1a00e89` (M47.2, `crates/pex/` + `crates/scripting/`). Verified against current `docs/feature-matrix.md` — both rows still present in the gaps table.

## Impact
Same surface as TD3-001; redundant doc-rot in the same file. A reader treats finished work as still-blocked.

## Suggested Fix
- Remove the `Save / load (M45)` row from the gaps table.
- Reframe the transpiler row as "full Papyrus transpiler deferred — `.pex` recognizer slice shipped (M47.2)".

## Completeness Checks
- [ ] **SIBLING**: Keep consistent with the TD3-001 status-row fixes in the same file
- [ ] **TESTS**: N/A (doc-only change)
