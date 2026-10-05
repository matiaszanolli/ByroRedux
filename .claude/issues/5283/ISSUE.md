# #5283: NIFAL-D8-2026-10-05-02: The CDB `SLOT_EMISSIVE` → `emissive` role is zero-weighted, because `EmissiveSettingsComponent` is not captured and the stub's emissive colour and multiplier stay 0, so the texture is uploaded and bound for no contribution

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5283
- **Labels**: low,nifal,bug,import-pipeline,game:starfield
- **Source**: `docs/audits/AUDIT_NIFAL_2026-10-05.md` (NIFAL-D8-2026-10-05-02)

_From `docs/audits/AUDIT_NIFAL_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW · **Dimension**: Shader-flags/Effects (CDB merge) · **Tier Violated**: parked-not-leak · **Game Affected**: Starfield
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:258-264` (emissive `fill`);
  - `crates/nif/src/import/types.rs:911-912` (`emissive_color: [0.0; 3]`, `emissive_mult: 0.0` defaults);
  - `crates/renderer/shaders/triangle.frag:1512-1517` and `:1689-1693` (the glow sample becomes `emissiveMask`, applied only when
    `emissiveMult > 0.01 && emissiveLum > 0.01`, as `emissiveColor * emissiveMult * emissiveMask`).
- **Status**: NEW (introduced by `18fce7e43`)
- **Description**: `apply_cdb_material` presents slot 7 as a landed canonical role ("Pure role translation … 7=emissive land in
  their canonical `MaterialTextureSet` roles"). The CDB's `EmissiveSettingsComponent` (`Enabled`, `EmissiveTint`,
  `LuminousEmittance`, per the spike table) is not captured. A material-reference stub carries no inline emissive data, so the
  bound texture is multiplied by zero. Unlike the five parked single-channel kinds, this role is neither parked nor effective.
- **Impact**: No Starfield surface glows from its CDB emissive map, and every such map still costs a texture upload and bindless
  slot. Diagnostics (`tex.loaded`, `mat.dump`) show an emissive texture bound, which reads as "emission translated".
- **Related**: #3398, #4429 (the parked-kinds precedent), #5210.
- **Suggested Fix**: Either capture `EmissiveSettingsComponent` (enable plus tint/emittance → `emissive_color`/`emissive_mult`
  with an `EmissiveSource`) or skip `SLOT_EMISSIVE` and list it in `nifal.md`'s parked table until the scalars land.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
