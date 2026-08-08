# REN-D17-NEW-03: Stale line citation in the sun_angular_radius guard

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2510
**Finding ID**: REN-D17-NEW-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 17 — Soft Shadows
**Location**: `byroredux/src/render/sky.rs:104-107`
**Status**: NEW

## Description
The debug-assert's rationale cites `triangle.frag:2418-2425` for the tangent-plane-approximation derivation. That block now lives at `triangle.frag:3029-3060` (the legacy-WRS arm) with a second copy of the sampler at `triangle.frag:2916-2921` (the ReSTIR arm, which is the default-on path and carries **no** such derivation comment).

## Evidence
`sky.rs:105` — "Tangent-plane disk approximation valid only for α < ~0.05 rad (documented in triangle.frag:2418-2425)"; lines 2418-2425 of `triangle.frag` are now ReSTIR pHat/reservoir prose, unrelated to the sun disk.

## Impact
Doc rot only; a future reader tuning `sun_angular_radius` (or a per-cell / per-TOD override, which #1023 made a one-line host-side write) lands on unrelated code and may not find the α < 0.05 rad validity bound. Note the guard threshold (0.10) is already 2× the documented validity bound.

## Related
#1023 / REN-D20-002; the ReSTIR path at `triangle.frag:2916`.

## Suggested Fix
Repoint to the symbol rather than the line number (`triangle.frag`'s directional shadow-jitter block) and add a one-line back-reference in the ReSTIR arm at 2916 so the default-on path carries the same caveat.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
