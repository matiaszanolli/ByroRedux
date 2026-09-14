# #4302: REN-2026-09-14-D6-03: `every_exterior_spawner_inserts_a_boundary_material` skips `cell_loader/spawn/` and `cell_loader/references/`, so the guard the skill tells auditors to rely on misses the main spawner directory

- **Labels**: low,nifal,test-gap,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4302
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs` (`every_exterior_spawner_inserts_a_boundary_material`)
- **Status**: NEW
- **Description**: The scan is a non-recursive `read_dir(cell_loader/)` that `continue`s on subdirectories. `byroredux/src/cell_loader/spawn/mesh_instance.rs`, `references/`, `byroredux/src/scene/nif_loader.rs` and `npc_spawn/` are never scanned. They route through the boundary today by inspection only. The Dimension 6 checklist says to trust this test instead of hand-verifying callers.
- **Evidence**: `continue; // subdirectories (spawn/, references/) and non-.rs files`; the sanity floor lists only six top-level files.
- **Impact**: Test coverage gap; no live divergence.
- **Related**: #2444, #3733, #4041.
- **Suggested Fix**: Recurse, and also scan `scene/` and `npc_spawn/`, skipping `*_tests.rs`. Add `byroredux/src/cell_loader/spawn/mesh_instance.rs` to the sanity list.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`) or `Material::resolve_pbr`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer. See `/audit-nifal`.
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
