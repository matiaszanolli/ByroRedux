# #4406 — NIFAL-D7-2026-09-14-03: #4166's `normalized_rotation_sample` substitutes an identity key for the overflow case instead of skipping it, contradicting its own doc and its three siblings

**Labels**: low,nifal,animation,bug
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (malformed content only; finite output)
- **Dimension**: Animation
- **Tier Violated**: no-fabrication
- **Game Affected**: FO3/FNV, Skyrim+ (`NiBSplineCompTransformInterpolator`)
- **Location**: `crates/nif/src/anim/bspline.rs:259-264` (doc), `:297-318` (fn), `:430-438` (push); pinned by `crates/nif/src/anim/tests/bspline.rs:191-197`
- **Status**: NEW (introduced by `1cedb6f8e`)
- **Description**: The doc says a bad sample is skipped "so the bone falls back to its bind pose". Only the non-finite input returns `None`; the `len_sq == inf` overflow case is routed into the identity arm and pushed as a real `RotationKey`. A local identity rotation is an invented pose, not the bind pose. The translation, scale and float siblings all skip the sample.
- **Evidence**: The test `bspline_rotation_sample_substitutes_identity_when_squaring_overflows` pins `[1,0,0,0]`.
- **Impact**: On malformed content the bone snaps to identity for the affected span.
- **Related**: #4166, NIFAL-D7-2026-09-14-01 (the shared sanitizer proposed there should return `None` here too).
- **Suggested Fix**: Return `None` when `!len_sq.is_finite()` and update the two pinning tests, or rewrite the doc if identity is deliberate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
