# TD5-003: GI bounce albedo uses material tint only — texture-average fold-in untracked

_Filed as #1628 from `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md`. Immutable snapshot as-filed; GitHub is authoritative for live state._

**Severity**: LOW · **Dimension**: Stale Marker · **Effort**: medium · **Age**: commit 6ac502ac8, 2026-06-05
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md` (TD5-003)
**Status**: Active marker, no tracking issue (this issue is that tracker)

## Description
`avg_albedo` (the GI colour-bleed source) in `byroredux/src/render/static_meshes.rs:656-659` is filled from `diffuse_color`; the TODO notes textured surfaces should bounce a texel-mean (a 1×1 average computed at asset load). Enhancement, not a bug — the current value is exact for untextured / vertex-coloured surfaces and the correct tint for textured content (an improvement over the prior hardcoded 0.5 grey).

## Evidence
`static_meshes.rs:656` `// TODO: fold in a 1×1 texture-average at asset load so`; `:659` `avg_albedo: mat.map(|m| m.diffuse_color).unwrap_or([0.5, 0.5, 0.5]),`.

## Impact
Textured surfaces bounce their flat material tint rather than their texel mean — a minor GI colour-bleed accuracy gap. No correctness defect.

## Suggested Fix
Compute a texel-mean (1×1 average at texture upload) and feed it into `avg_albedo`; this issue is the tracker. Or leave as a well-scoped, honestly-documented enhancement.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: A texel-mean computed at texture upload is read once into `avg_albedo`, not re-derived per frame in the renderer
- [ ] **TESTS**: A reference comparison pins the GI bleed colour for a textured surface if the texel-mean lands
