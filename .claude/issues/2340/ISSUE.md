# FNV-D8-01: --grid 0,0 worldspace auto-pick is non-deterministic across multiple containing worldspaces

Source: `docs/audits/AUDIT_FNV_2026-08-03.md`, Dimension 8 (Real-Data Validation & Bench-of-Record), finding FNV-D8-01.
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2340
Labels: high, import-pipeline, legacy-compat, bug

**Severity**: HIGH
**Dimension**: Dimension 8 — Real-Data Validation & Bench-of-Record
**Location**: `byroredux/src/cell_loader/exterior.rs:317-362` (`build_exterior_world_context`, grid-containment step ~lines 344-353)
**Related**: Builds on / extends the tie-break rule fixed for #444 and #1655 (single-DLC-worldspace omission) — this is a distinct new multi-candidate failure mode in the same selector, not a regression of those fixes.

## Description

`--grid 0,0` worldspace auto-pick is non-deterministic. The selection order in
`build_exterior_world_context` is: `--wrld` override → "worldspace containing
the requested grid" → hardcoded preferred list → most-cells fallback. The
grid-containment step resolves multiple candidates via `HashMap::iter().find()`,
whose iteration order is unspecified/randomized for Rust's std `HashMap`.

FNV ships many small worldspaces (`ffencounterworld`, `freesidefortworld`,
`WastelandNVmini`) whose own local grid also spans the origin, so at
`--grid 0,0` every candidate satisfies the containment predicate and `.find()`
returns whichever happens to come first in iteration order. The preferred-list
tie-break (`wastelandnv`, `wasteland`, `tamriel`, `skyrim`) only runs as a
fallback *after* containment fails to match anything, so it never gets a
chance to break a tie among multiple containment matches.

## Evidence

Three back-to-back runs of the byte-identical command — the exact invocation
this project's own `CLAUDE.md` documents (`cargo run -- --esm FalloutNV.esm
--grid 0,0 --radius 3 --bsa …`) — returned three different results:
`ffencounterworld` (99 entities, no usable collider near spawn, player
free-falls permanently), `freesidefortworld` (7113 entities, wrong worldspace
but grounded), and only `WastelandNV` when `--wrld` was explicitly forced
(11867 entities, correctly grounded).

## Impact

The documented CLI example does not reliably load the intended worldspace;
worst case the player spawns into permanent freefall with no usable terrain
collision. Silent and non-reproducible — the same command can pass or fail
across runs with zero code changes.

## Suggested Fix

When the containment rule matches multiple candidates, break the tie via the
preferred-list rule before falling back to arbitrary/HashMap order — collect
all grid-containing candidates, then if more than one matches, pick the first
that also appears in the preferred list (falling back to most-cells only if
none of the containing candidates are on the preferred list). Log all
grid-containing candidates when the match is ambiguous.

## Validation

CONFIRMED — verified directly against `build_exterior_world_context`
(selection order and `.find()` over `HashMap::iter()` both confirmed as
described), independently re-confirmed by a background validation pass. No
open-issue duplicate found (#444 is the prior, distinct single-DLC-worldspace
fix this finding builds on, not a duplicate).
