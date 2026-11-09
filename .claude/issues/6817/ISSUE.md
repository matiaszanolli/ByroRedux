# NIFAL-2026-11-08-02: `static_height` (51ce1c2f9) decides material-adjacent state after the boundary from raw `mesh.material`; constants unmeasured; not in the spec

GitHub: #6817 (snapshot as filed; GitHub is authoritative for state)
Labels: low,nifal,renderer,bug

- **Severity**: LOW (opt-in prototype, `render.static_parallax_depth_m` defaults to 0)
- **Dimension**: Material / Geometry / Textures (found by Dims 1, 2, 8, 9)
- **Tier Violated**: single-boundary (secondary: no-fabrication, doc gap)
- **Game Affected**: all, when the setting is on; MSWP-swapped FO4 / Skyrim most exposed
- **Location**: `byroredux/src/static_height.rs:151-166` (`mesh_qualifies`), `:52-72` (constants), `:236-266` (`bind_synthetic_height`); `byroredux/src/scene/mesh_entity.rs:182-194` (call)
- **Status**: NEW
- **Description**: `bind_synthetic_height` runs after `translate_material_with_provenance`. It writes `handles.textures.height`, `parallax_height_scale` and `parallax_max_passes` on `MaterialTextureHandles`, so `Material.parallax_*` keeps the translate default while the render path reads the handles. The mismatch is provenance-tagged (`MaterialTextureSource::SyntheticHeight`), which is the right mitigation. The gate, though, reads `mesh.material.{is_decal, has_alpha, alpha_test, model_space_normals}` from the raw `ImportedMesh`, not the `source_material` that `spawn_mesh_entity` is handed. On the cell path `source_material` is the REFR material-swap target (`mesh_instance.rs`: `swapped_material.as_ref().unwrap_or(&mesh.material)`), so the gate judges pre-swap flags. It also misses the canonical model-space-normals bit that `translate_material` ORs in from `extra_material_flags` and the `_msn` name rule (#4548). The #6690 guard `markers_and_layer_read_the_source_material_not_the_meshes_own` scans `attach_mesh_components`, not this call, so it stays green. Constants: `MARCH_PASSES = 16.0` and `MIN_TILING_UV_EXTENT = 1.02` cite nothing, and `MAX_SEAM_EXCEEDANCE = 0.05` is self-described as "a starting point to tune by eye" (it sits just inside the measured terrain-layer p90 of 0.062). The REFR placement scale is ignored. `SyntheticHeight` and the post-boundary write appear in no spec document.
- **Failure scenario**: With the setting on, a FO4 REFR whose MSWP swaps an opaque tiling wall for an alpha-blended one still qualifies, so a height march runs on a blended surface. A tiling static whose normal slot is model-space via the REFR overlay bit gets a garbage height field.
- **Suggested fix**: Pass `source_material`, or read the translated `Material`'s flag word, into `mesh_qualifies`. Extend the #6690 source-scan guard to the call. Cite or mark provisional the three constants before the setting leaves prototype. Add a `nifal.md` §2 line recording the synthetic-height slot as an opt-in non-authored role with its post-boundary exception.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test or guard pins this specific fix


_Source: `docs/audits/AUDIT_NIFAL_2026-11-08.md` (HEAD 51ce1c2f9)._
