# #2693: NIFAL-D8-2026-08-12-01: MultiLayerParallax inner layer is read from `BSShaderTextureSet` slot 7; shipped content authors it in slot 6

- **Severity**: HIGH
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: `no-leak` (authored role dropped) — and a wrong canonical texture-role output, which the severity table floors at HIGH
- **Game Affected**: Skyrim SE/LE (measured); FO4/FO76 share the `shader_type == 11` arm (unmeasured)
- **Location**: `crates/nif/src/import/material/dedicated_shader.rs:168-189`
- **Status**: NEW
- **Description**: The `11 =>` arm routes texture-set slot 7 into
  `MaterialInfo::inner_layer_map` → canonical `MaterialTextureSet::inner_layer` →
  `GpuMaterial.inner_layer_map_index` → `crates/renderer/shaders/triangle.frag`
  (`materialKind == 11u` branch). Vanilla Skyrim authors the inner layer in slot
  **6**, which the importer never reads at all — `textures.get(6)` appears nowhere
  under `crates/nif/src/`.
- **Evidence** — `crates/nif/examples/_tmp_nifal_d8_mlp.rs` over
  `Skyrim - Meshes0.bsa` (607 MLP properties) and `Meshes1.bsa` (55, i.e. 100 % of
  that archive's MLP shapes):
  ```
  slot 6: 607 + 55 non-empty
      textures\architecture\windhelm\WHwindowinner02.dds
      textures\architecture\solitude\Sinside.dds
      textures\dungeons\caves\IceCaveWall02.dds
  slot 7: 370 + 10 non-empty
      textures\dungeons\caves\IceCaveSubsurfacetint01.dds
  ```
  Three independent corroborations that slot 6 is the inner layer: the filenames
  themselves (`WHwindowinner02`, `Sinside` = Solitude interior); nif.xml's
  `BSShaderTextureSet` field table, which documents slot 6 as *"Subsurface for
  Multilayer Parallax"* and slot 7 as *"Back Lighting Map
  (SLSF2_Back_Lighting)"* (niftools nif.xml, lines 6307-6319 at
  /mnt/data/src/reference/nifxml/nif.xml); and this engine's own REFR overlay
  table, which already maps NIF slot 6 → `inner`
  (`byroredux/src/cell_loader/refr.rs:157`). The arm's comment cites nif.xml's
  *enum* prose ("Layer(TS7)", same file line 1413) — the one statement the data
  contradicts.
- **Impact**: Every Skyrim multilayer-parallax surface (ice caves and glaciers,
  Windhelm/Solitude/ship windows) samples its parallax inner layer from the
  subsurface/backlight tint map, and the authored inner layer is never uploaded.
  No downstream fallback masks it.
- **Related**: NIFAL-D8-2026-08-12-04; #2627 (the BGSM half of the same canonical role).
- **Suggested Fix**: Read slot 6 into `inner_layer_map` in the `11 =>` arm and
  decide slot 7's canonical home separately (back-lighting role, or an explicit
  park). Pin with a fixture test asserting slot 6 → `inner_layer` for shader type 11.

---
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-12.md` (finding `NIFAL-D8-01`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

