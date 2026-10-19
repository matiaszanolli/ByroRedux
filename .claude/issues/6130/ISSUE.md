# NIFAL-SD-03: FO3/FNV `HairTint` is resolved by a `GameKind` branch plus a vertex-colour rewrite at NPC spawn

Labels: low,nifal,gameplay,game:fo3,game:fnv,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6130

**Severity**: LOW · **Dimension**: Material / Shader-flags · **Tier violated**: `single-boundary` · **Games**: FO3, FNV (Oblivion takes a different branch of the same closure)

## Location
`byroredux/src/npc_spawn/resumable/runtime.rs` (`hair_uses_fallout_tint_mask: matches!(game, GameKind::Fallout3NV)`, the hook that reads it, `bake_fallout_hair_tint`, `fallout_hair_tint_factor`)

## Description
The tint formula is a faithful decode of the shipped `HairTint` pixel shader, so the output is right. But the choice to apply it is `GameKind`, not authored data, and the result overwrites the imported `mesh.colors` in place (the tint mask becomes a final modulation). Skyrim's hair tint travels as `ShaderTypeData::HairTint` into `Material.shader_type_fields.hair_tint_color`; Oblivion hair multiplies `material.diffuse_color`. Three carriers for one concept, chosen at spawn. FO3/FNV author the `Hair` bit (`F3SF1` bit 18, nif.xml `BSShaderFlags`), which `ImportedMaterial` does not carry (no consumer of that bit under `crates/nif/src/import`), so the only discriminator available at the boundary is absent.

## Evidence
`bake_fallout_hair_tint` runs on every mesh of the hair NIF, not on meshes that author the flag. The neutral tint `0.5` for an unauthored `HCLR` is documented as "an engine choice", acceptable under no-fabrication. Whether FO3/FNV hair NIFs author the `Hair` bit was not measured (nif.xml bit cited).

## Impact
No wrong pixels today. A hair NIF reached by another path (a cell-placed FO3 mannequin or hat, a modded hair mesh) renders its tint mask as teal vertex colour, and the mask has no canonical home. A cost for the next hair-tint consumer, not a live defect.

## Related
ECS-SD-2026-10-19-04 (`source_shader_type`, the canonical discriminator the boundary does not yet carry).

## Suggested Fix
Surface the authored `Hair` flag on `ImportedMaterial` and let the NPC hook key on it instead of `GameKind`; longer term give the vertex-colour tint-mask a canonical field.

Source: docs/audits/AUDIT_NIFAL_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
