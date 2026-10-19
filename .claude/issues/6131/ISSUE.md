# NIFAL-SD-04: NPC texture hooks override `base_color` but leave the provenance label at the NIF slot

Labels: low,nifal,game:oblivion,game:fo3,game:fnv,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6131

**Severity**: LOW · **Dimension**: Shader-flags / texture roles · **Tier violated**: `harness-gap` · **Games**: Oblivion, FO3, FNV

## Location
`byroredux/src/npc_spawn/resumable/runtime.rs` (body, eyes and head hooks that assign `mesh.material.textures.base_color`); compare `byroredux/src/scene/nif_loader.rs` (FaceGen path stamping `MaterialTextureSource::RuntimeOverride`)

## Description
The FaceGen tint path stamps `MaterialTextureSource::RuntimeOverride` when it replaces the base colour (`nif_loader.rs`). The three legacy hooks replace `mesh.material.textures.base_color` on the imported tier but do not touch `texture_sources`, which `spawn_nif_mesh` derives from the mesh's own `ImportedTextureSource`. `tex.*` / `mat.dump` therefore report `nif-texture-set` for a texture that came from the RACE record.

## Evidence
`ImportedTextureSource` has no runtime variant (`crates/nif/src/import/types.rs`), so a hook cannot stamp it; only `diffuse_override` can.

## Impact
Diagnostics only. It misleads the documented "chrome means missing textures" triage on NPC heads and bodies.

## Related
#4421.

## Suggested Fix
Route the hooks through the same override channel as `diffuse_override` (a per-mesh runtime override carried into `spawn_nif_mesh`), which also fixes the provenance label.

Source: docs/audits/AUDIT_NIFAL_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
