# #4304: REN-2026-09-14-D6-05: Ground cover is a drawn surface with no canonical `Material`, and neither the NIFAL spec nor the ground-cover spec records it as an exemption

- **Labels**: low,nifal,terrain-exterior,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4304
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `docs/engine/nifal.md` (§3), `docs/engine/exal-groundcover.md`, `byroredux/src/material_translate.rs` (`translate_texture_only_material` doc)
- **Status**: NEW
- **Description**: The #2444 doc states the invariant "every drawn surface's canonical material is produced at one boundary". EXAL ground cover draws blades with no `Material`, no `GpuMaterial` and no `MaterialTable`, shaded from `GroundCoverPalette` (`groundcover_translate.rs`). That is the correct shape: no `Imported*` tier, no renderer per-game branch. But unlike the Cornell and `crates/save` exemptions, it is not written down.
- **Evidence**: zero `Material` / `GpuMaterial` / `intern(` hits in the three ground-cover files; `exal-groundcover.md` mentions only `KHR_materials_sheen`; `nifal.md` has no ground-cover mention.
- **Impact**: Documentation only; the next audit must re-derive it.
- **Related**: #2444, #4054–#4058.
- **Suggested Fix**: Add a one-paragraph exemption to `nifal.md` §3, the `translate_texture_only_material` doc, and the Dimension 6 exemption list.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`) or `Material::resolve_pbr`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer. See `/audit-nifal`.
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
