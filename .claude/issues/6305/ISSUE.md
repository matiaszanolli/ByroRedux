# #6305: NIFAL-D8-2026-10-21-01: ObScript `SwapTexture` re-resolves the base colour outside the per-role resolve: authored clamp mode dropped, provenance left stale

**Labels**: low,bug,nifal,renderer,scripting,legacy-compat,game:oblivion,game:fo3,game:fnv

**Source**: docs/audits/AUDIT_NIFAL_2026-10-21.md · **Severity**: LOW · **Dimension**: Shader flags / texture roles · **Location**: `byroredux/src/asset_provider/texture.rs` (`apply_texture_swap`)

- **Tier Violated**: `single-boundary`
- **Game Affected**: Oblivion, FO3, FNV (ObScript `SwapTexture`; e.g. the FO3 end-game slides `interface\endgame\endscrn_*`)
- **Location**: `byroredux/src/asset_provider/texture.rs:1388-1432` (`apply_texture_swap`), driven by `byroredux/src/app_step.rs:1115` (`step_texture_swaps`); compare `byroredux/src/cell_loader/spawn/mesh_instance.rs:1153-1158` (spawn) and `:1372` (`MaterialTextureDebugInfo` insert)
- **Status**: NEW. The sibling issues #6110 (ref counting, closed) and #6111 (upload flush, open) are about different defects in the same function.
- **Description**: The spawn path resolves the base-colour handle with `resolve_texture_with_clamp(.., material.texture_clamp_mode)`. `apply_texture_swap` (07bd22a6b) replaces the same role at runtime with `resolve_texture(ctx, tex_provider, Some(&path))`, which hard-codes clamp mode 3 (WRAP_S_WRAP_T). It never reads `Material.texture_clamp_mode`, which is on the entity it already holds. Flipbooks had the same class of defect and were routed through `resolve_flip_texture_for_role` (#4426). The swap also rewrites `Material.texture_path` but leaves the entity's `MaterialTextureDebugInfo` untouched, so `paths.base_color` keeps the old path and `sources.base_color` keeps `nif-texture-set`. Dim 8's rule is that an override which keeps the NIF slot's label is a finding. `MaterialTextureSource::RuntimeOverride` already exists for this case (#6131).
- **Evidence**: `texture.rs:630-637` (`resolve_texture` → `resolve_texture_with_clamp(.., 3)`). `apply_texture_swap` has no `texture_clamp_mode` read, and `MaterialTextureDebugInfo` is not mentioned anywhere in the function.
- **Impact**: A CLAMP-authored swap target, such as a full-screen slide quad or a decal, samples with REPEAT and can bleed its opposite edge along the border. `mat.dump` / `tex.*` show the pre-swap path and label for a swapped mesh, which undercuts the "chrome means missing textures" triage. No crash and no data loss. Reachable only through the ObScript command.
- **Related**: #4426 (flipbook per-role resolve), #6131 (`RuntimeOverride`), #6110, #6111.
- **Suggested Fix**: Resolve with `resolve_texture_with_clamp(ctx, tex_provider, Some(&path), material.texture_clamp_mode)`, or through a shared single-role helper like `resolve_flip_texture_for_role` for `base_color`. In the same loop, update `MaterialTextureDebugInfo.paths.base_color` and set `sources.base_color = MaterialTextureSource::RuntimeOverride`.

**Validation note**: a source-shape test in `texture.rs` (#6110) asserts `apply_texture_swap`'s body contains exactly one `resolve_texture(` inside the per-mesh loop. Switching to `resolve_texture_with_clamp(` changes that substring, so the guard must be updated in the same change (keep its one-acquire-per-mesh intent).

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
