# R-N3 / #787 — Tangent transformed via inverse-transpose under non-uniform scale

**Severity**: MEDIUM (dormant — surfaces only when R-N2 / #786 is resolved)
**Domain**: renderer (Shader Correctness)
**Status**: NEW

## Location
`crates/renderer/shaders/triangle.vert:181-193`

## One-line summary
Non-uniform-scale path uses `(M⁻¹)ᵀ * T` (cotangent transform — for normals). Tangents are contravariant, need `M * T`. Off-axis tangents on non-uniformly-scaled meshes get a wrong direction (rotated bump highlight).

## Fix shape
Three-line shader edit: collapse the non-uniform branch to use `m3 * inTangent.xyz` (same as the existing uniform-scale path). Bundle with R-N2 / #786 re-enablement.

## Audit source
`docs/audits/AUDIT_RENDERER_2026-05-03.md` finding R-N3.
