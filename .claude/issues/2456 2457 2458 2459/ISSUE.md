# Issues 2456, 2457, 2458, 2459 — NIF/legacy-compat subsystem gaps (Dimension 7)

## #2456 — SUBSYS-01 (MEDIUM): Scale/shear baked into NiTransform.rotation silently discarded
- Files: crates/nif/src/rotation.rs (sanitize_rotation, 11-64), crates/nif/src/import/coord.rs (33-63), crates/nif/src/stream.rs (679,702)
- Non-orthonormal 3x3 (uniform/non-uniform scale or shear baked by exporter) is either SVD-orthogonalized (discarding singular values) or force-unit-quat'd, never folded into NiTransform.scale. No logging either way.
- Minimum viable fix per issue: rate-limited log::warn! when a scaled (not zeroed) degenerate matrix is detected, to measure corpus incidence; regression test pins composed translation for the 2*I parent-rotation case already in transform.rs:250-270.

## #2457 — SUBSYS-02 (MEDIUM): NiVertexColorProperty suppressed by NiMaterialProperty
- File: crates/nif/src/import/material/legacy_properties.rs (apply_vertex_color_property gate at 742, has_material_data set at 133-154)
- has_material_data is set both by BSLightingShaderProperty (Skyrim, intended gate per #1208) AND by ordinary NiMaterialProperty (pre-Skyrim). File-order property chain means NiMaterialProperty-before-NiVertexColorProperty (dominant Oblivion/FO3/FNV shape) silently drops the vertex-color property.
- Fix: mirror #435/N06 remedy - dedicated vertex_color_mode_consumed flag set only by genuine vertex-colour-intent sites (BSLightingShaderProperty), not has_material_data. Add NiMaterialProperty-before-NVCP precedence test.

## #2458 — SUBSYS-03 (MEDIUM): Bone-name binding case-sensitive on skin/ragdoll paths
- Files: byroredux/src/scene/nif_loader.rs (412,449,966-968,994-998,1089), byroredux/src/ragdoll.rs (83-104); contrast crates/core/src/string/mod.rs (40-88, StringPool lowercases)
- node_by_name: HashMap<Arc<str>, EntityId> keyed on raw case-preserved NIF node name; skin-bone + ragdoll template lookups are exact-match, unlike StringPool-backed Name comparisons elsewhere (case-insensitive, matches Gamebryo GlobalStringTable).
- Fix (interim, cheaper per issue): lowercased-key fallback lookup with log::warn! when it's what resolves. Full fix (deferred): key node_by_name through StringPool/FixedString.
- Test: case-mismatched bone name (Bip01 Spine vs bip01 spine) resolves.

## #2459 — SUBSYS-04 (LOW): DISABLE_SORTING captured but never reaches draw sort
- Files: crates/core/src/ecs/components/scene_flags.rs (41-44,81-84), byroredux/src/render/mod.rs (309-360, draw_sort_key)
- SceneFlags::DISABLE_SORTING (0x0040) parsed/stored but draw_sort_key has no sorting-disabled lane - always back-to-front alpha sorts.
- Suggested: verify against Gamebryo semantics (unmounted source per issue - do best-effort investigation); if reachable, wire a file-order lane into draw_sort_key gated on the flag; if legacy semantics can't be verified, document as intentional skip.
