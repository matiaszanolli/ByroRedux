# #1303 -- OBL-D4-NEW-01: Oblivion renders without normal maps

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: HIGH | **Dim 4** — Rendering Path for Oblivion Shaders
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D4-NEW-01)

**Location**: `crates/nif/src/import/material/walker.rs:596-601` (normal_map = normal_texture.or_else(bump_texture)); `byroredux/src/asset_provider.rs` (no implicit derivation); `byroredux/src/render/static_meshes.rs:179-183` (normal_map_index)

**Issue**: ~All Oblivion `NiTexturingProperty` meshes leave both the normal and bump texture slots empty — Oblivion ships normal maps via the `<base_stem>_n.dds` load-time filename convention, not as an explicit NIF texture slot. With `normal_map_index == 0` everywhere, the fragment shader's TBN normal perturbation is bypassed and every Oblivion surface is lit by its flat interpolated vertex normal. This is the single largest visual regression vs the original engine in an RT-PBR pipeline (flat plaster/stone/metal, lost surface detail on every mesh).

**Suggested fix**: implement the Bethesda load-time convention — when a NIF arrives with a base texture path but no `normal_map`, derive a candidate `<base_stem>_n.dds` (strip extension, append `_n.dds`) and bind it if present in the archive/texture provider. This matches how every Bethesda game loader works for Oblivion content.

## Completeness Checks
- [ ] **SIBLING**: check FO3 path — same `_n.dds` convention applies
- [ ] **TESTS**: integration test verifying a known Oblivion mesh gets its `_n.dds` bound
- [ ] **CANONICAL-BOUNDARY**: if the resolution sets `normal_map` on `ImportedMesh`, it flows through `translate_material::ResolvedPaths.normal_map` — correct path, stays at the NIFAL boundary
- [ ] **UNSAFE**: no unsafe involved
