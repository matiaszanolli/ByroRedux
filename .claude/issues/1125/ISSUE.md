# REN-D9-NEW-01: traceReflection miss fallback blends skyTint into sealed interior cells inappropriately

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1125
## Source Audit
`docs/audits/AUDIT_RENDERER_2026-05-16.md` — Dimension 9 (RT Ray Queries)

## Severity
**LOW** — subtle interior glass tint that should not carry outdoor TOD signal.

## Location
- `crates/renderer/shaders/triangle.frag:1873` (refraction miss)
- `traceReflection` body (reflection miss, same pattern noted in comment)

## Status
**NEW** at HEAD `1608e6a2`

## Description
The escape-ray fallback at `triangle.frag:1873` for refraction is `skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5` — half-sky half-ambient. The comment notes the reflection-miss path uses the same pattern. With #925 plumbing `skyTint.xyz` from the TOD/weather palette, interior cells receive a SKY tint into the half-sky term even when no sky portal is on screen — possibly inappropriate for sealed interiors. Markarth probe validated this works for canyon-fed interiors, but Megaton / Vault 21 (fully sealed) may bleed daylight tone into glass refractions.

## Impact
Subtle — interior glass refractions absorb half-sky color when the ray escapes; for fully-sealed cells this introduces an outdoor TOD signal where none should exist. Most visible in sunset / dawn interior glass.

## Suggested Fix
Gate the `skyTint * 0.5` term on `sky_params.is_exterior` (already in the UBO via `depth_params.x` / `radius < 0`). For interior cells, drop to `sceneFlags.yzw` alone (pure cell ambient). Marker for future Markarth-probe-like validation on interior glass-heavy cells.

## Completeness Checks
- [ ] **UNSAFE**: N/A — shader change
- [ ] **SIBLING**: Apply matching change in `traceReflection` body (same fallback pattern noted in comments)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Visual validation on a sealed interior glass-heavy cell (Vault 21, Megaton common house glass dome)

## Related
- #925 (closed, skyTint plumbing — landed the data flow this finding builds on)