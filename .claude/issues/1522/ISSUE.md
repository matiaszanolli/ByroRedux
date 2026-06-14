# MAT-DOC-01: Starfield .mat arm comment references deleted shader-side classify_pbr fallback

**Severity**: LOW (doc rot)
**Dimension**: ECS audit dim 9 — NIFAL canonical material producers
**Source**: docs/audits/AUDIT_ECS_2026-06-14.md
**Status**: NEW

## Description
`byroredux/src/asset_provider.rs:1020-1022` — the Starfield `.mat` arm returns `true` without setting `metalness_override`/`roughness_override` (unlike the BGSM arm), and its comment says "the shader's legacy `classify_pbr` fallback handles the missing override gracefully." There is no shader-side / render-side `classify_pbr` fallback anymore — it was removed when the canonical resolve-once contract landed.

## Evidence
- Render path reads `m.roughness`/`m.metalness` directly (`byroredux/src/render/static_meshes.rs:314-315`); every `classify_pbr` token there is a comment documenting the removed fallback.
- The actual gap-fill is parse-time: unset override → `None` → `f32::NAN` (`byroredux/src/material_translate.rs:157-158`) → `Material::resolve_pbr` NaN-sentinel classifier arm at the `translate_material` boundary.

## Impact
None to runtime — Starfield `.mat` content still routes correctly through `translate_material`/`resolve_pbr`. Only the comment misdescribes *where* the gap-fill happens.

## Suggested Fix
Reword the comment to point at the `resolve_pbr` NaN-sentinel backstop at the translation boundary (mirroring the accurate doc on `Material::resolve_pbr`). No code change.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: confirm the `.mat` arm still routes through `translate_material`/`resolve_pbr` (it does) — comment-only fix, no per-game logic moved
- [ ] **SIBLING**: scan for other comments referencing a shader-side `classify_pbr` fallback
