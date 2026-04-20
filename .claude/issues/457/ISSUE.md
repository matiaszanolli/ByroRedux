# Issue #457

FO3-6-04: ROADMAP Tier-1 table lists FO3 as 'Interior only' — masks exterior gap or understates state

---

## Severity: Medium (doc drift)

**Location**: `ROADMAP.md:874`

## Problem

Tier-1 compatibility table row for FO3 reads `Interior ✓` with no exterior column entry. FNV row says `Interior + exterior ✓`.

`load_exterior_cells` is game-agnostic — nothing code-side stops FO3 exterior loading *except* FO3-6-01 (hardcoded worldspace preferred list missing `wasteland`) and the absence of positive validation.

## Impact

Either:
- FO3 exterior has been tested and works — ROADMAP is undercounting, and FO3-6-01 is the only prerequisite to updating the row.
- FO3 exterior has not been tested — the Tier-1 ranking is overstated and there may be additional unknown gaps.

## Fix

1. Fix FO3-6-01 first.
2. Run `cargo run --release -- --esm Fallout3.esm --grid 0,0 --bsa 'Fallout - Meshes.bsa' --textures-bsa 'Fallout - Textures.bsa'`.
3. If it renders, update `ROADMAP.md:874` to `Interior + exterior ✓`.
4. If it doesn't, capture the failure mode as a new issue and update the row to `Interior ✓ / exterior broken via <issue>`.

## Completeness Checks

- [ ] **TESTS**: Exterior cell load attempt logged, outcome documented
- [ ] **DOCS**: ROADMAP Tier-1 row reflects reality
- [ ] **SIBLING**: Same check for Oblivion and Skyrim Tier-1 rows — don't leave stale entries

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-6-04)
