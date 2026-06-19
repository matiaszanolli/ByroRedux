# D4-1: Worldspace selection runs preferred-default list before grid-containment

**Issue**: #1655
**Source audit**: docs/audits/AUDIT_FO3_2026-06-18.md (HEAD `2aac5351`) — re-confirmed
still-open; first reported AUDIT_FO3_2026-06-16.md (D4-1), never previously filed.
**Severity**: LOW · **Labels**: low, import-pipeline, legacy-compat, bug
**Dimension**: 4 — Cell Loading
**Location**: `byroredux/src/cell_loader/exterior.rs:113-139`

## Description
The preferred-default list `["wastelandnv","wasteland","tamriel","skyrim"]` (line 114) runs
in `.or_else()` order before the grid-containment check (line 120). With Fallout3.esm + a
DLC master, `--grid X,Y` targeting an FO3 DLC worldspace (Anchorage, Point Lookout, Zeta)
without `--wrld` silently lands on Capital Wasteland. FNV-shared.

## Impact
Low — only multi-master DLC exterior loads via `--grid` without `--wrld`.

## Suggested Fix
Run grid-containment before the preferred list when `--grid` is supplied.
