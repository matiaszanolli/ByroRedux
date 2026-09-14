# #4303: REN-2026-09-14-D6-04: The #4287 fix pasted its six-line rationale comment twice in `fill_from_bgsm`'s BGEM arm

- **Labels**: low,nifal,tech-debt,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4303
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/cell_loader/refr.rs` (`RefrTextureOverlay::fill_from_bgsm`)
- **Status**: NEW
- **Description**: The `// #4287 / SF-2026-09-11-D9-02 — \`base_texture\` is BGEM's …` block appears twice back-to-back; the `a5a6407d` diff added it twice.
- **Evidence**: `rg -n "#4287 / SF-2026-09-11-D9-02" byroredux/src/cell_loader/refr.rs` → two hits, six lines apart.
- **Impact**: Hygiene only.
- **Related**: #4287.
- **Suggested Fix**: Delete one copy.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`) or `Material::resolve_pbr`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer. See `/audit-nifal`.
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
