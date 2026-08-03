# FNV-D7-04: extract_ragdoll has silent drop sites alongside loud #1539/#1850 warnings for the same edge-loss class

Source: `docs/audits/AUDIT_FNV_2026-08-03.md`, Dimension 7 (PHYSAL Ragdoll — FNV Reference Slice), finding FNV-D7-04.
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2339
Labels: low, legacy-compat, bug

**Severity**: LOW
**Dimension**: Dimension 7 — PHYSAL Ragdoll (FNV reference slice; PHYSAL-wide, not FNV-specific)
**Location**: `crates/nif/src/import/collision/ragdoll.rs` (`extract_ragdoll`) — silent sites at lines 47 (unhosted body), 51 (failed `resolve_shape`), 62/71/75 (non-finite mass/translation/rotation guards), 134 (unresolved constraint endpoint), 168 (non-finite joint-limit guard); logged sites at 106-120 (#1850 `BhkBreakableConstraint`) and 155-166 (#1539 `BhkConstraintData::Other`)

## Description

Four classes of drop sites in `extract_ragdoll` stay silent (`continue` with
no log), while two adjacent guards in the same function — the #1850
`bhkBreakableConstraint` drop and the #1539 `BhkConstraintData::Other` drop —
log loudly via `log::warn!` for the same class of lost articulation edge.

Silent sites (all bare `continue`, no logging): unhosted body, failed shape
resolve, non-finite mass/translation/rotation on the body CInfo (#1534),
unresolved constraint endpoint, non-finite joint-limit guard.

## Impact

Telemetry-only — the downstream forest/emptiness gates in `build_ragdoll`
still correctly fire regardless — but it undercuts the diagnostic story the
#1539/#1850 guards exist for: an auditor or developer investigating a
malformed ragdoll sees warnings for some dropped edges but not others, making
the silent classes harder to diagnose from logs alone.

## Suggested Fix

Route all silent sites through the same "dropping … linking bones 'a' <-> 'b'"
phrasing used by the #1539/#1850 guards. Add a test driving each previously-
silent drop condition through `extract_ragdoll` and asserting the warning is
emitted.

## Validation

CONFIRMED — verified directly by reading `extract_ragdoll` in full (silent
vs. logged sites counted and line-matched), independently re-confirmed by a
background validation pass. No open-issue duplicate found.
