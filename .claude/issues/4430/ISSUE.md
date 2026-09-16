# #4430: NIFAL-D8-2026-09-16-03: FO4 is the only slot-2 arm that ignores the `Glow_Map` gate it is handed, and the BGSM `glowmap` flag is parsed but never read — the census shows the NIF flag is unreliable in both directions, so the rule needs a source

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4430
- **Labels**: low,nifal,import-pipeline,game:fo4,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW. The right answer is unknown without an FO4 reference. At most 20 vanilla properties are plausibly wrong today (see Evidence).
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: no-fabrication. It is the only `slot_to_role` arm with no cited measurement, and its companion comment is false.
- **Game Affected**: FO4
- **Location**:
  - The routing arm: `crates/nif/src/import/material/slot_role.rs:361-367`, `(Fallout4, 2)`, which returns `Emissive` for every non-tint type and never reads `context.glow_map`.
  - The false comment: `crates/nif/src/import/material/dedicated_shader.rs:324-326`, "The `Glow_Map` bit (F4SF2 bit 6) participates only in the texture-slot vocabulary above". The bit is computed at `:112-127` and placed in `TextureSlotContext.glow_map`, but the FO4 arm discards it.
  - The flag parsed and never read: `crates/bgsm/src/bgsm.rs:121` and `:292`. The only readers are the parser and its tests.
  - The unconditional BGSM fill: `byroredux/src/asset_provider/material/merge.rs:614-619`.
- **Status**: NEW. #3068 gated Skyrim in `86c410229` and gave no FO4 evidence. #1733 looked only at the flag-set-but-no-texture case and called it inert.
- **Description**: nif.xml defines slot 2 as `Glow(SLSF2_Glow_Map)/…` (`nif.xml:6313`), and `Fallout4ShaderPropertyFlags2` has `Glow_Map` at bit 6 (`:6487`). The other slot-2 arms follow that gate: Skyrim tests the flag (#3068), and FO76/Starfield test the CRC `GLOWMAP`. The FO4 arm binds any non-empty slot 2 as the emissive mask. On the external side, BGSM v≤2 carries its own `glowmap` bool next to `glow_texture`, and the merge fills `emissive` from `glow_texture` without looking at the bool. `triangle.frag` replaces `emissiveMask` with the glow sample whenever `glowMapIndex != 0`, so the role decides whether emission is masked or flat.
- **Evidence**: census of `Fallout4 - Meshes.ba2` + `Fallout4 - MeshesExtra.ba2`, non-tint `BSLightingShaderProperty` with slot 2 populated:

  | Glow_Map flag | emissive authored | count |
  |---|---|---|
  | clear | no | 2,493 (inert) |
  | clear | **yes** | **137** — 116 with a `_g.dds` glow map (e.g. `putridglowingonebodyb.nif` → `GlowingOneHead1_g.DDS`, `mirelurkqueen.nif` → `MeatTile01_g.DDS`); **20 with a `_d.dds` diffuse** (e.g. `terminaloninstitute.nif` → `PipBoyScreen_d.dds`, `floorlampnoshadeonoff.nif` → `lightfixtureglass01_d.dds`); 1 `ColorWhiteUtility` |
  | set | no / yes | 3,254 / 5,951 |

  `Fallout4 - Materials.ba2` + the three DLC mains, BGSM v≤2 with `glow_texture` non-empty: `glowmap=false` × 65 (5 of them `emit_enabled`, e.g. `bloodbugremap.bgsm`, `sublightinner.bgsm`), `glowmap=true` × 249.

  So neither reading is safe. Gating on the NIF flag, as Skyrim does, would strip the mask from 116 genuine FO4 glow maps, Glowing Ones among them. Routing without the gate, as today, masks emission with a diffuse texture on 20 properties.
- **Impact**: For up to 20 inline FO4 properties (Institute terminal screens, lit floor lamps), emission is shaped by a diffuse texture where the engine may emit flat colour. The 5 BGSM `glowmap=false` + `emit_enabled` materials are in the same situation. The bigger cost is that this is the one arm in the table with no recorded evidence, sitting next to a comment that claims the gate is used. That is the pattern that let #3068 go unquestioned.
- **Related**: #3068, #1733, #1592, #2997/#2998/#2999, #3458
- **Suggested Fix**:
  1. Do not flip the gate yet. First find a source for FO4's actual rule (does the lit shader sample slot 2 without `Glow_Map`, and does BGSM `glowmap` override the NIF bit?).
  2. Correct the `crates/nif/src/import/material/dedicated_shader.rs:324-326` comment, and record the census above in the FO4 arm, the way every other arm records its numbers.
  3. Once the source is in hand, decide whether `glowmap` gates the BGSM `emissive` fill.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
