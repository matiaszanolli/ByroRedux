# #4405 — NIFAL-D7-2026-09-14-04: the #4167 animation completeness harnesses have value choices that let specific field drops pass

**Labels**: low,nifal,animation,test-gap,bug
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (test coverage gap; both boundaries currently correct)
- **Dimension**: Animation
- **Tier Violated**: harness-coverage gap
- **Game Affected**: all
- **Location**: `byroredux/src/asset_provider/animation.rs:425-430` and `:498-499`; `byroredux/src/anim_convert.rs:1070-1095`, `:1079`, `:1113`, `:1272-1282`
- **Status**: NEW
- **Description**: Every field of core `AnimationClip` and its channel/key types is set, so the harnesses are not vacuous overall, but four value choices are not distinctive:
  1. The HKX scale fixture `[1,2,3]` averages to `2 == scale[1]`, so dropping the average passes.
  2. `translation_type: Linear` is the value a hard-coded converter would produce.
  3. `FloatTarget::Alpha` is the first variant, so a hard-coded target passes.
  4. Every channel has one key and only collection counts are asserted, so a first-key-only copy passes.
- **Evidence**: Arithmetic and assertions at the cited lines.
- **Impact**: A regression in exactly the transforms the harnesses were written to guard would ship green.
- **Related**: #4167, #3462, NIFAL-D5-2026-09-14-02.
- **Suggested Fix**: HKX scale `[1,2,6]`; non-Linear key types; a non-first `FloatTarget`; two keys per channel with `keys.len()` asserted.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
