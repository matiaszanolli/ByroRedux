# #4290: REN-2026-09-14-D6-01: An MSWP material swap swaps the textures but keeps every non-texture material scalar from the *source* BGSM

- **Labels**: high,nifal,renderer,game:fo4,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4290
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: HIGH
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/cell_loader/refr.rs` (`RefrTextureOverlay::fill_from_bgsm`, `build_refr_texture_overlay`); `byroredux/src/cell_loader/spawn/mesh_instance.rs` (`resolve_mesh_paths`, `spawn_mesh_instance`); `byroredux/src/material_translate.rs` (`translate_material`, `bgsm_authored_scalars_are_never_reclassified_by_an_overlay_swap`)
- **Status**: NEW
- **Description**:
  - An XMSP swap substitutes the BGSM path at REFR level (`build_refr_texture_overlay`) and per shape (`resolve_mesh_paths`, #973).
  - Both then call `fill_from_bgsm`, which fills **texture paths only** from the swap target.
  - `spawn_mesh_instance` then calls `translate_material(&mesh.material, …)`. `mesh.material` was merged at import time (`byroredux/src/cell_loader/references/import.rs` → `merge_external_material`) with the mesh's **original** BGSM.
  - The swap target's alpha test/threshold, two-sided, emissive, smoothness and PBR overrides, translucency, glass flags, greyscale bits and UV transform never reach `Material`. The source's values ride onto the target's textures.
  - #4229's `recomputed_pbr` skips `bgsm_pbr_scalars_authored` sources on any overlay swap, and its test pins that. That is right for a TXST texture swap and wrong for an MSWP material swap.
- **Evidence**:
  - `fill_from_bgsm`'s `.bgsm` and `.bgem` arms contain only `Self::fill(&mut self.<texture_slot>, …)`.
  - There is no `merge_external_material` call in `byroredux/src/cell_loader/spawn/mesh_instance.rs`, `spawn.rs` or `refr.rs`.
  - `extra_material_flags` carries only the TXST model-space-normals bit.
- **Impact**: FO4 placements whose MSWP target authors different alpha/two-sided/emissive/glass/PBR data render the target's textures with the source's material response. That is a wrong `Material` out of the translate boundary, the NIFAL HIGH floor. Vanilla prevalence is **unmeasured**.
- **Related**: #973, #4229, #2708, #4287/#4288.
- **Suggested Fix**: When the effective `material_path` comes from an MSWP swap, run `merge_external_material` on a clone of `mesh.material` against the swapped path before `translate_material`, replacing texture-only `fill_from_bgsm` for that half. Scope the #4229 "never reclassify BGSM scalars" test to TXST provenance.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`) or `Material::resolve_pbr`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer. See `/audit-nifal`.
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
