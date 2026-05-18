# #1158 — REN-D9-NEW-07: Stale doc-comment line numbers in traceReflection

**Severity**: LOW
**Domain**: renderer
**Status**: OPEN
**Source Audit**: `docs/audits/AUDIT_RENDERER_2026-05-17_DIM9_DIM10.md` — Dimension 9

## Location

`crates/renderer/shaders/triangle.frag:449-455` (`traceReflection`'s `tMin = 0.05` rationale comment).

## Description

The `tMin = 0.05` rationale comment cites caller bias values `0.05 and 0.1` at lines `1633 and 2049`. Post-Session-34 split the actual sites are at `1692` (glass IOR reflection ray) and `2108` (metal/glossy reflection ray). The bias values are still correct (0.05 and 0.1); only the line-number anchors are stale.

Same drift on the "other ray-query sites" anchors at the end of the same comment: cites `1486, 1702, 2408, 2484`; actual is `1543` (window portal), `1774` (refraction loop), `2470` (cluster shadow), `2549` (GI bounce).

## Evidence

```glsl
// tMin = 0.05 matches the N_bias offset every caller already applies
// to `origin` (callers at lines 1633 and 2049 use bias 0.05 and 0.1
// respectively) and the convention every other ray-query site in
// this shader uses (1486, 1702, 2408, 2484). Pre-#1017 this was 0.01
```

## Suggested Fix

Update the line-number anchors to the current sites: callers `1692` / `2108`; sibling ray-query origins `1543` (window portal), `1774` (refraction loop), `2470` (cluster shadow), `2549` (GI bounce).

Or — since these will drift again on the next refactor — replace with grep-friendly anchor comments like `// see windowRQ at the isWindow/glassIORAllowed split` that don't depend on line numbers at all.

## Related

- Session 34 large-module split sweep (HISTORY.md)
- #1114 — audit-skill path-validate gate (catches backticked paths but not free-text line numbers)
