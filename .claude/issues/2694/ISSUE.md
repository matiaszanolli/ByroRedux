# #2694: NIFAL-D8-2026-08-12-02: FaceTint reads two slots vanilla content never authors, and misroutes the three it does

- **Severity**: HIGH
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: `no-leak` (three authored roles dropped or bound to the wrong canonical role)
- **Game Affected**: Skyrim SE/LE (measured); FO4 shares the `shader_type == 4` arm
- **Location**: `crates/nif/src/import/material/dedicated_shader.rs:106-125` (slot-2 gate), `:132-136` (slot 3), `:148-167` (the `4 =>` arm)
- **Status**: NEW
- **Description**: The FaceTint arm reads slot 4 → `detail_map` and slot 7 →
  `tint_map` (nif.xml enum prose again). Both slots are empty on **100 %** of
  vanilla Skyrim FaceTint properties, so the arm is inert, while the three
  populated slots each land wrong:
  - slot 2 (`*_sk.dds` skin-tint mask, 3158/3158) → `glow_map` → canonical
    `emissive`, because the `skin_tint_slot` gate only fires for `shader_type == 5`
    / `ShaderTypeData::SkinTint`, and Skyrim FaceTint parses as
    `ShaderTypeData::None` (`crates/nif/src/blocks/shader.rs:594-597`).
  - slot 3 (`FemaleHeadDetail_Age40.dds`, `BlankDetailmap.dds`, 3149/3158) →
    `parallax_map`, and `crates/renderer/shaders/triangle.frag` runs
    parallax-occlusion displacement whenever that index is non-zero — there is no
    `materialKind` gate on the POM branch.
  - slot 6 (`…\FaceGenData\FaceTint\Skyrim.esm\<formid>.dds`, 3150/3158) → nothing.
- **Evidence** — `crates/nif/examples/_tmp_nifal_d8_mlp.rs … 4` over
  `Skyrim - Meshes0.bsa`: 3158 FaceTint properties across 3158 NIFs, non-empty
  counts `0:3158, 1:3158 (_msn 3113 / _n 45), 2:3158 (_sk 3158), 3:3149, 6:3150`;
  slots 4, 5 and 7 never appear.
- **Impact**: Every vanilla Skyrim head binds its skin-tint mask as the glow map
  (latent while `emissive_color` is black — one authored non-black value away from
  glowing faces), ray-marches POM from a face detail map used as a height field,
  and drops the per-NPC FaceGen tint the NIF points at (the canonical `tint` role
  is live and sampled).
- **Related**: NIFAL-D8-2026-08-12-01 (same root cause); #2095 (the FaceGen
  diffuse override path, which is how tint reaches faces today).
- **Suggested Fix**: In the FaceTint arm route slot 2 → `tint`, slot 3 → `detail`,
  slot 6 → `tint`/FaceGen (deciding precedence against `select_facegen_diffuse`),
  and stop feeding slot 3 into `parallax_map` for this shader type.

---
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-12.md` (finding `NIFAL-D8-02`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

