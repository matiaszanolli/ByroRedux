# #4409 — NIFAL-D5-2026-09-14-03: `docs/engine/nifal.md` §2 Particles no longer describes what the boundary applies or defers after #3754/#4240/#4261

**Labels**: low,nifal,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (doc; the spec is the authority auditors check "parked" against)
- **Dimension**: Particles
- **Tier Violated**: parked-not-leak
- **Game Affected**: all
- **Location**: `docs/engine/nifal.md:301-303`, `:311-317`, `:335-338`
- **Status**: NEW (precedent #2488)
- **Description**:
  - (a) The applied-field list omits `planar_angle` / `planar_angle_variation` (#4240) and the ×2 half-spread→full-width variation convention.
  - (b) The rate is still described as "`NiFloatData` first key"; the #3754 curve-mean, #2548 blend and #3329 sequence tiers are missing.
  - (c) Per-emitter attribution is still called wholly pending, although #4261 made params, colour, budget and the modern rate tier per-instance; only the legacy and #3329 sequence tiers remain whole-scene.
  - (d) The tooling line omits planar columns.
- **Evidence**: Line citations against `9e372f452` / `b3237e65a`; neither commit touched nifal.md.
- **Impact**: A later audit will misclassify the sequence-tier residual and miss the variation-convention change that affects fog-volume sizing (`byroredux/src/fog.rs:303`).
- **Related**: #2488, #3754, #4240, #4261, #3329, NIFAL-D3-2026-09-14-03.
- **Suggested Fix**: Update §2 Particles to match (a)–(d), and mirror the change in the skill's Dimension 5 text.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
