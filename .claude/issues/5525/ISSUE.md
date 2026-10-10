# #5525: NIFAL-D8-2026-10-09-03: `parse_loose_mat` and `MaterialIndex::lookup` are two producers of `CdbMaterial`, and 9bb6b7dce extended only the compiled one, so 16 of the 20 installed loose `.mat` files drop their authored `EmissiveSettingsComponent`

**Labels**: bug, game:starfield, import-pipeline, low, nifal

**Source**: `docs/audits/AUDIT_NIFAL_2026-10-09.md` — finding `NIFAL-D8-2026-10-09-03` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW. This is Creation (mod) content only, and the arm would be inert anyway until D8-01 and D8-02 are
  settled.
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: single-boundary (the same authored component lowers differently depending on its container)
- **Game Affected**: Starfield (Creation archives)
- **Location**: `byroredux/src/asset_provider/material/loose_mat.rs:84-143`. The match has arms for `MRTextureFile`,
  `TextureReplacement`, `MaterialParamFloat`, `AlphaSettingsComponent`, `EffectSettingsComponent` (`IsGlass` only) and
  `Translucency`. There is no `EmissiveSettingsComponent` or `LayeredEmissivityComponent` arm, and no `BlendingMode` read.
  Compare the compiled side at `crates/sfmaterial/src/index.rs:492-539`.
- **Status**: NEW. #5396, which closed the layout defect, is verified. This is the parity gap it left once 9bb6b7dce landed
  the same evening.
- **Description**: All 20 installed loose files were extracted (to `/tmp/audit/nifal/matsamples/`):
  - All 20 decode (each carries `MRTextureFile`), so each one wins over the CDB.
  - 16 author `BSMaterial::EmissiveSettingsComponent`. These are the 11 QOG pawnshop-terminal parts and the 5 requisition-kiosk
    splash screens. None of them author `Enabled`: they inherit it from the shader model they `Import`. `ColorEmissive.mat`
    looks up with `Enabled = true`.
  - 1 (`lasersight_white.mat`) authors `LayeredEmissivityComponent` with `Enabled "true"` and a luminance of 432.
  - All of it is ignored. Their emissive maps, for example `TerminalCase_emissive.dds` at slot 7, bind at zero weight.
- **Evidence**: A JSON census of the extracted files (component-type counts: `EmissiveSettingsComponent` 16,
  `EmittanceSettings` 16, `LayeredEmissivityComponent` 1). `loose_mat.rs` has no arm for either component.
- **Impact**: Glowing terminal screens from installed Creations render unlit. The two decoders will keep drifting each time
  one of them gains a capture.
- **Related**: #5396 (closed), D8-01, D8-02, #5466 (open, the loose `.mat` doc text).
- **Suggested Fix**: Factor the per-component capture (class name plus field map → `CdbMaterial` fields) into one function
  that both decoders call. `serde_json` string-typed values and CDB `Value`s can both feed one small accessor trait. Then
  apply D8-01's inheritance rule and D8-02's emissive decision once, in that function.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→`Material` boundary (`translate_material` / `Material::resolve_pbr`) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
