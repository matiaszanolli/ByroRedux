# FNV-D4-01: feature-matrix.md mislabels its SCPT record count as FO3/FNV -- that figure is FO3-only

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2557
**Finding ID**: FNV-D4-01

**Severity**: LOW
**Dimension**: ESM Record Parser
**Location**: `docs/feature-matrix.md:152`
**Status**: NEW

## Description
`docs/feature-matrix.md:152` mislabels its SCPT record count ("1,257") as "FO3/FNV" — that figure is FO3-only; real `FalloutNV.esm` ships 2,576 SCPT records (parser correctly captures all of them; no functional gap).

## Evidence
Confirmed directly: `feature-matrix.md:152` reads "ESM SCPT record parse (FO3/FNV, 1 257 records)".

## Impact
Documentation only. The parser itself is correct and unaffected.

## Suggested Fix
Split the row into separate FO3 (1,257) and FNV (2,576) counts, or clarify the figure is FO3-specific.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
