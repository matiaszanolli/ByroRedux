# R-N4 / #788 — Vertex shader computes tangent transform unconditionally

**Severity**: LOW
**Domain**: renderer (Performance)
**Status**: NEW

## Location
`crates/renderer/shaders/triangle.vert:173-194`

## One-line summary
With perturbNormal off by default (R-N2 / #786), the `fragTangent` varying produced at vertex stage is never consumed. ~800k ops/frame waste. Trivially gateable on a `sceneFlags` bit.

## Fix shape
Defer until R-N2 / #786 resolves. If perturbation stays off long-term, add a sceneFlags gate around the tangent transform.

## Audit source
`docs/audits/AUDIT_RENDERER_2026-05-03.md` finding R-N4.
