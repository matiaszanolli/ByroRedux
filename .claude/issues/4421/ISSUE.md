# #4421: REN-2026-09-16-D6-01: `select_facegen_diffuse` applies the per-NPC FaceGen tint DDS to `material_kind == 5` shapes, but every vanilla Skyrim FaceGeom head is kind 4 — the face atlas lands on scars, Argonian hair and Orc tusks, and the head keeps t…

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4421
- **Labels**: high,renderer,nifal,game:skyrim,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

**Regression of #2095** (closed) — the gate introduced by `b3a53e567` keys on the wrong material kind.

- **Severity**: HIGH. The wrong base-colour texture reaches the canonical
  `Material` and `TextureHandle` on every vanilla Skyrim NPC that has pre-baked
  FaceGen.
- **Dimension**: NIFAL Material (base-colour role selection feeding `translate_material`)
- **Game Affected**: Skyrim (measured). FO4, FO76 and Starfield share the
  pre-baked path (`prebaked_facegen_tint_path`); their head `material_kind` was not
  measured in this sweep.
- **Location**:
  - The gate: `select_facegen_diffuse` and `MATERIAL_KIND_SKIN_TINT`
    (`byroredux/src/scene/nif_loader.rs`).
  - The call site in the same file:
    `owned_textures.base_color = select_facegen_diffuse(..., mesh.material.material_kind)`.
  - The test that encodes the premise: `facegen_diffuse_override_targets_only_skin_tint_head`.
  - The caller: `PrebakedPhase::Facegen` in `byroredux/src/npc_spawn/resumable.rs`,
    which passes the result of `prebaked_facegen_tint_path` as the diffuse override.
- **Status**: Regression of #2095 (closed). The gate was introduced in
  `b3a53e567` (2026-08-09) to stop the override replacing mouth, eye and hair
  textures. It keyed on SkinTint (5) on the stated premise that the head mesh is
  the SkinTint mesh. Skyrim's FaceGeom head uses FaceTint (4), and
  `canonical_shader_type` does not remap 4 on the Skyrim layout.
- **Description**:
  - **The doc comment's claim.** It says the generated FaceTint DDS "is the
    diffuse replacement for the SkinTint head mesh only".
  - **What vanilla FaceGeom NIFs contain.** The head mesh is `material_kind 4`.
    The only kind-5 shapes are overlays with their own authored textures: facial
    scars, Argonian hair and Orc tusks.
  - **Result.** The override is never applied to any vanilla head. Instead, the
    per-NPC face atlas is bound as the diffuse of those overlay meshes, which is
    exactly the layered "mush" `b3a53e567` set out to prevent.
  - **Why the test did not catch it.** The unit test hard-codes the premise
    `(MATERIAL_KIND_SKIN_TINT, "head.dds", FACE_TINT)`, so it passes.
- **Evidence** (census over `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa`,
  `ImportedMaterial.material_kind` from `import_nif`; "FaceGeom" means the path
  contains `facegendata`):
  - `kind=4, facegeom, detail+tint, base texture contains "head"`: **3,149**
    (`malehead.dds`, `femalehead.dds`, `maleheadvampire.dds`,
    `argonianfemalehead.dds`, …), plus one more kind-4 FaceGeom head without
    `detail`.
  - `kind=5, facegeom`: **750** in total, none with a head base texture. Their base textures include
    `actors\character\male\facedetails\faceleftsidegash*.dds`,
    `…\female\facedetails\facefemaleleftsidegash_*.dds`,
    `textures\actors\character\argonianmale\argonianhair.dds`,
    `…\argonianfemale\argonianfemalehair.dds` and
    `textures\actors\character\orcmale\orctusks.dds`.
  - Code: `if material_kind == MATERIAL_KIND_SKIN_TINT { diffuse_override… }`,
    with `MATERIAL_KIND_SKIN_TINT: u32 = 5`.
- **Impact**:
  - Every Skyrim NPC with pre-baked FaceGen shows the generic race head texture:
    no per-NPC skin tone, complexion or make-up from its FaceTint DDS.
  - Any NPC whose FaceGeom includes a scar, Argonian hair or Orc tusks shows
    pieces of the face atlas on those overlays.
  - This is independent of D7-01 and D7-02, which darken whatever diffuse the
    head ends up with.
- **Related**: #2095, `b3a53e567`, #2694 (which established that FaceTint belongs
  to the tint family), REN-2026-09-16-D7-01, REN-2026-09-16-D7-02
- **Suggested Fix**:
  1. Key the override on the FaceGen head shape, measured, rather than on
     SkinTint: `material_kind == FACE_TINT` on the Skyrim layout, or better, a
     canonical "FaceGen head" flag set at the NIFAL boundary, where the per-game
     shader-type numbering is already normalised.
  2. Replace the unit test's hand-written `(5, "head.dds")` case with the measured
     `(4, "malehead.dds")` head and the `(5, "faceleftsidegash04.dds")` overlay.
  3. Confirm the FO4, FO76 and Starfield head kinds before widening the gate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
