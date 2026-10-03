# #5210 — REN-D6-2026-10-03-05: Spec and doc rot from CDB Phase 2 and the guard's exemption list

**Labels**: low,nifal,documentation,doc-rot,game:starfield
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `docs/engine/nifal.md` (§ parked Starfield kinds; §3 "Drawn-surface exemptions"); `byroredux/src/asset_provider/material/merge.rs` (the `.mat` gate comment in `merge_external_material`); `byroredux/src/material_translate.rs` (`every_exterior_spawner_inserts_a_boundary_material` doc)
- **Status**: NEW
- **Description**:
  - nifal.md still says "zero Starfield texture roles are produced, so the gap is latent". It also names `mat_path_forwards_no_texture_roles_until_cdb_phase_2_lands` as the pin that "gets rewritten", but that test no longer exists. 18fce7e43 forwards colour/normal/emissive/height and replaced it with `mat_path_lookup_miss_keeps_presence_only_and_no_textures` and `mat_path_merges_cdb_authored_textures_when_indexed`.
  - nifal.md records none of the CDB scalar translations: flat colour, alpha, glass, SSS.
  - The `merge_external_material` comment still describes Phase 1: "forwards no authored field. Phase 2 should return `Merged`…".
  - nifal.md §3 says there are "exactly four deliberate exemptions": Cornell, `crates/save`, ground cover, and mesh→participating medium (#5102). The spawner-guard doc lists Cornell, save, ground cover and `scene.rs` demo primitives. The two lists differ.
  - The guard doc names `npc_spawn/resumable.rs`, which is now `npc_spawn/resumable/mod.rs`. It names `cornell.rs`, but the Cornell `MeshHandle` inserts are in `cornell/builders.rs`.
- **Suggested Fix**: Rewrite the nifal.md Starfield paragraph to describe the forwarded roles and point at the live pins. Reconcile the two exemption lists. Update the guard-doc paths.
- **Folded in**: REN-D12-2026-10-03-03 (Dim 12). The `mat.set` glass-optics comment in `byroredux/src/commands/scene.rs` still says "`cornell.rs`'s `glass()`"; since #5090 (8f38df6df) `glass()` lives in `byroredux/src/cornell/builders.rs`. Fix the pointer in the same change.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)
