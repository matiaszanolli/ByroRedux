# NAVM-01: stale test comment claims FNV NAVM count is 0 (pre-#1272), live run shows 4771

Source: `docs/audits/AUDIT_FNV_2026-08-03.md`, Dimension 4 (ESM Record Parser — Coverage & Accuracy), finding NAVM-01.
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2341
Labels: low, import-pipeline, legacy-compat, documentation

**Severity**: LOW
**Dimension**: Dimension 4 — ESM Record Parser — Coverage & Accuracy
**Location**: `crates/plugin/src/esm/records/tests.rs:364-367`

## Description

A stale test comment still describes FNV's `NAVM` (navmesh) count as `0`:

```rust
// Observed on FalloutNV.esm:
//   WATR=78, NAVI=1, NAVM=0 (NAVM entries live nested under
//   CELL children groups on FO3/FNV, not at top level — a
//   follow-up can walk those if needed), REGN=276, ECZN=17,
//   LGTM=31, HDPT=61, EYES=12, HAIR=67.
```

This reflects the pre-#1272 state, before the per-cell NAVM drain landed. A
live run of `parse_real_fnv_esm_record_counts` now shows 4771 navmeshes
(`index.navmeshes.len()`), which is only surfaced via an `eprintln!` a few
lines below — there is no assertion pinning the real count, so nothing
catches the comment being wrong.

## Impact

No functional impact — no assertion reads the stale claim, so this is purely
a documentation-drift risk. But it risks wasting a future auditor's time
re-investigating already-fixed behavior.

## Suggested Fix

Update the comment to reflect the current observed count and, optionally, add
a floor assertion (e.g. `index.navmeshes.len() >= 4000`) alongside the
existing `waters`/`factions`/`globals`/`game_settings` floor assertions in the
same test, converting the stale comment into a real regression guard.

## Validation

CONFIRMED — verified directly (comment text matches, no nearby assertion on
navmesh count, `#1272` per-cell drain confirmed already landed), independently
re-confirmed by a background validation pass. No open-issue duplicate found.
