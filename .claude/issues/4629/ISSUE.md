# NIF-D3-2026-09-21-03: #4154's sibling sweep missed five diagnostics that still filter .nif only

**Issue**: #4629
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: LOW
**Dimension**: 3 (Tooling / CI gates)
**Location**: `crates/nif/examples/{unknown_types,locate_unknowns,full_histogram,parse_sweep,d5_unk_ba2}.rs`
**Status**: NEW

## Description
#4154's sibling sweep missed five diagnostics that still filter `.nif` only. #4154 fixed `d5_coverage` and `block_coverage_baselines` to use `is_nif_entry` (which also recognizes `.bto`/`.btr`), but these five standalone example binaries were not swept and still hard-code `.ends_with(".nif")`:
- `crates/nif/examples/parse_sweep.rs:9`: `if !f.to_ascii_lowercase().ends_with(".nif")`
- `crates/nif/examples/d5_unk_ba2.rs:39`: `.filter(|p| p.to_ascii_lowercase().ends_with(".nif"))`
- `crates/nif/examples/locate_unknowns.rs:26`: same pattern
- `crates/nif/examples/unknown_types.rs:19`: same pattern
- `crates/nif/examples/full_histogram.rs:24,41`: same pattern (two sites)

Confirmed at HEAD `ee6d3fb39`: all five files still use the `.ends_with(".nif")` string filter, not `is_nif_entry`.

## Evidence
On FO76 `GeneratedMeshes02.ba2` (2,055 NIFs, all `.bto`), these five examples report "0 NIFs" since none of the entries match their filter.

## Impact
These are ad-hoc diagnostic tools (not gates that block CI), so no production or test regression results — but they silently under-report on any `.bto`/`.btr`-heavy archive, which could mislead manual investigation the way #4154 already fixed for the two gated harnesses.

## Related
#4154 (closed — fixed the two harnesses that gate CI; this is the same fix left undone for the example binaries)

## Suggested Fix
Switch all five examples to the same `is_nif_entry` helper #4154 introduced.

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D3-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (confirmed here — all five sibling examples share the same fix)
