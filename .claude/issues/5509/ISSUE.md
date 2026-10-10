# #5509: SF-2026-10-09-D6-01: #5277 forwards only `BlendingMode == "AlphaBlend"` (0 vanilla shapes); the 491 shapes whose CDB material authors an additive-family mode render as opaque lit surfaces, and nifal.md has no parked row for them

**Labels**: bug, game:starfield, legacy-compat, medium, nifal, renderer

**Source**: `docs/audits/AUDIT_STARFIELD_2026-10-09.md` — finding `SF-2026-10-09-D6-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM (the "translatable data silently dropped" row; visible artifacts on effect cards)
- **Dimension**: Material Flow (NIFAL boundary), CDB arm
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:331-351`: the comment reads "any other authored string stays untouched until its mapping is sourced", and the gate is `:348`;
  - `docs/engine/nifal.md:710` (records only the AlphaBlend arm);
  - capture `crates/sfmaterial/src/index.rs` (`effect_blend`).
- **Status**: NEW. #5277 (closed by 9bb6b7dce) scoped AlphaBlend only. NIFAL-D8-01 (this suite) covers the inheritance gap that leaves AlphaBlend and IsGlass inert. It does not cover these modes, and they are live without inheritance. No issue mentions Starfield additive blending.
- **Description**:
  - Starfield shapes almost never carry `NiAlphaProperty` (884 blocks), so the CDB `EffectSettingsComponent.BlendingMode` string is their only blend signal.
  - 9bb6b7dce captures it for every material, but forwards it only for `"AlphaBlend"`.
  - Every other authored mode leaves `has_alpha == false`. The surface goes down the opaque, depth-writing, lit path, and its glow sprite becomes a solid card.
- **Evidence**: The probe (`/mnt/data/tmp/starfield-audit-probe`, release, MemoryMax=4G; output `/tmp/audit/starfield/probe/blend_alpha.out`) visited every `BSGeometry` in `Meshes01/02/Patch` (344,355 shapes), looked up its shader-property material in the vanilla base CDB, and crossed the result with `NiAlphaProperty`:

  | `BlendingMode` | shapes | NiAlphaProperty | samples |
  |---|---|---|---|
  | Additive | 182 | none | `shipwep_*_projectile_*` (`ShipBallisticBoltCore`, `ShipEM_SwirlGlow_01`, `ShipKineticCannonGlow`), `starborntempleintpedestal_b02` (`SBStoneTrimGlowFXOff`) |
  | SourceSoftAdditive | 277 | none | `smod_fx_enginemain_*` (`EngineGlowFlames.mat`), `spacemine.nif` (`RadialGlow_01_Red`) |
  | DestinationInvertedSoftAdditive | 29 | none | `nathemast_topwaterfall.nif` (`Water\Waterfall01.mat`) |
  | DestinationSoftAdditive / Multiply / TakeSmaller | 1 / 1 / 1 | none | `effects\debugfiles\testblendingmodes_*` |
  | **AlphaBlend** | **0** | — | — |

  All 491 shapes sit on `BSLightingShaderProperty` stubs. Raw CDB (this suite's NIFAL probe, `cdb_raw.txt`): `BlendingMode` is authored on 526 `EffectSettingsComponent`s (Additive 234, SourceSoftAdditive 283, AlphaBlend 2).
- **Impact**:
  - Ship engine flames, weapon projectile glows, space-mine glows and the New Atlantis MAST waterfall render as opaque, depth-writing cards instead of additive effects.
  - This is visible wherever ships and those set pieces appear.
- **Related**: #5277 (closed), NIFAL-D8-01 (inheritance; would add more hits), NIFAL-D8-02 (emissive on the same effect materials), #1651 / #1823 (the FO4 BGSM additive history).
- **Suggested Fix**:
  - Record the dropped modes now as a named parked row in nifal.md, with these counts.
  - Then source the mapping:
    - **Additive / Multiply.** The same studio's FO4 BGSM preset vocabulary exists locally (`/mnt/data/src/reference/Material-Editor/MaterialLib/BaseMaterialFile.cs:363-427`: Additive = (SRC_ALPHA, ONE), Multiplicative = (DEST_COLOR, ZERO)). The renderer already keys those pipelines (`crates/renderer/src/vulkan/pipeline.rs:752-755`). Confirm the cross-game carry-over against a capture before landing it.
    - **The soft variants and `TakeSmaller`.** These have no local source and stay parked until one is found.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→`Material` boundary (`translate_material` / `Material::resolve_pbr`) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
