# Issue #444

FO3-6-01: Exterior worldspace auto-pick list omits FO3 Wasteland EDID

---

## Severity: High

**Location**: `byroredux/src/cell_loader.rs:346-360`

## Problem

`let preferred = ["wastelandnv", "tamriel", "skyrim"];` — FO3's Capital Wasteland uses EDID `Wasteland` (no `nv` suffix). `--esm Fallout3.esm --grid 0,0` falls through to the `max_by_key(cell_count)` fallback.

Today the fallback happens to land on Wasteland (it has the most cells). But DLC masters (PointLookout, Zeta, Pitt, Anchorage) add their own worldspaces — any one of them could tie or outvote on a multi-plugin load.

## Impact

CLI UX — `--grid` works by luck for single-plugin FO3, silently wrong for multi-plugin. The comment at `scene.rs:39` references a `--wrld` override that doesn't exist.

## Fix

1. Add `"wasteland"` to the preferred list in front of `"tamriel"`.
2. Expose `--wrld <name>` flag in `byroredux/src/main.rs` + `scene.rs`, thread through to `cell_loader`.
3. When user passes `--grid` without `--wrld`, prefer a worldspace that actually contains the requested coord over raw cell count.

## Completeness Checks

- [ ] **TESTS**: Regression: `load_exterior_cells(Fallout3.esm, grid=0,0)` lands on `Wasteland` even with PointLookout.esm loaded
- [ ] **DOCS**: Update CLI usage block in `CLAUDE.md` with `--wrld` option
- [ ] **SIBLING**: Consider the same for Oblivion (`Tamriel`) and Skyrim (`Skyrim`) edge cases — ensure all three Tier-1 names in the list

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-6-01)
