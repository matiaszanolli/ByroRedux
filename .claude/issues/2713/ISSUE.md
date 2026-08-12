# #2713: RefrTextureOverlay.inner (TXST TX06 / XTXR slot 6) is populated by both TXST paths and has zero consumers - every inner-layer override is silently dropped

- **Severity**: MEDIUM
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: `no-leak` — authored override captured, then dropped at the spawn→translate boundary
- **Game Affected**: Skyrim SE, FO4, FO76 (every TXST-bearing REFR)
- **Location**: written at `byroredux/src/cell_loader/refr.rs:65`, `:120`, `:157`, `:172`; never read — `byroredux/src/cell_loader/spawn.rs:1149-1219`
- **Status**: NEW
- **Description**: `RefrTextureOverlay` carries a resolved `inner` role, filled by
  both the whole-TXST merge and the `XTXR` slot-6 swap. `resolve_mesh_paths`, the
  overlay's only consumer, applies `diffuse`, `normal`, `glow`,
  `specular`/`smooth_spec`, `height`, `env`, `env_mask`, `wrinkle` and
  `material_path` — and never assigns `textures.inner_layer`.
- **Evidence**: `grep -rn "o\.inner" byroredux/` returns nothing, while every
  sibling role appears at `byroredux/src/cell_loader/spawn.rs:1158-1213`.
- **Impact**: ESM-level retextures of the multilayer inner layer silently fall
  back to the base NIF texture. Bounded to one role on the override path, hence
  MEDIUM rather than HIGH.
- **Related**: NIFAL-D8-2026-08-12-01 (base-path half of the same role).
- **Suggested Fix**: Assign `textures.inner_layer` alongside the other eight and
  add a test asserting every `RefrTextureOverlay` field has a consumer.

---
## Independently found by three audits in the same suite

### FO4 view (`FO4-D6-01`)

- **Severity**: LOW
- **Dimension**: 6 — ESM TXST → REFR overlay → spawn
- **Location**: `byroredux/src/cell_loader/refr.rs:62-65`, `:120`, `:157`, `:172`; absent from `byroredux/src/cell_loader/spawn.rs:1158-1218`
- **Status**: NEW (sibling of the OPEN #2627, which covers the BGSM half of the same role)
- **Description**: `RefrTextureOverlay` carries an `inner` slot populated from `TextureSet.inner` (TXST `TX06`) and swappable via `XTXR` `slot_index == 6`. `resolve_mesh_paths` applies eight overlay slots to the mesh's `MaterialTextureSet` and never reads it; `grep -rn "\.inner\b" byroredux/src/cell_loader/*.rs` returns only the four declaration/fill/match sites in `byroredux/src/cell_loader/refr.rs` itself.
- **Impact**: A REFR overriding the MultiLayerParallax inner-layer texture renders with the base mesh's inner layer or none. Narrow — with #2627 also open, the canonical `inner_layer` role currently has no live producer at all on the FO4 path.
- **Related**: #2627, #2533.
- **Suggested Fix**: Add the one-line `textures.inner_layer` resolve alongside its sibling slots and close it together with #2627, so the role gains a producer and a consumer in one change.

### Starfield view (`SF-2026-08-12-D9-01`)

- **Severity**: MEDIUM
- **Dimension**: 9 — external material flow / texture roles
- **Location**: `byroredux/src/cell_loader/refr.rs:62-65` (field + its own contradicting doc), `:120` (`merge_from_texture_set` write), `:157,172` (`apply_slot_swap` write), `byroredux/src/cell_loader/spawn.rs:1139-1234` (`resolve_mesh_paths` — the only overlay consumer, no `inner` read)
- **Status**: NEW
- **Description**: `RefrTextureOverlay` carries an `inner` slot, filled from
  `TextureSet.inner` by the full-TXST merge and from `slot_index == 6` by the XTXR
  per-slot swap. `resolve_mesh_paths` is the sole place an overlay is folded into the
  canonical `MaterialTextureSet`, and it reads `diffuse`, `normal`, `glow`,
  `specular`, `height`, `env`, `env_mask`, `wrinkle`, `material_path` and
  `model_space_normals` — but never `inner`. `grep -rn 'o\.inner\|ov\.inner\|overlay\.inner' byroredux/src/`
  returns zero hits, and `.inner` appears nowhere in `cell_loader/` outside `refr.rs`.
- **Evidence**: The field's own doc comment asserts the opposite — *"Preserved for
  parity with `TextureSet.inner` so the slot_index=6 XTXR swap round-trips"* — and the
  round-trip does not happen. The sink exists and is fully live: `MaterialTextureSet`
  has an `inner_layer` role (`crates/nif/src/import/types.rs:322`), the NIF
  multi-layer-parallax path populates it, `map_secondary_texture_handles` resolves it
  to a bindless handle (`byroredux/src/asset_provider/texture.rs:444`), and it reaches
  `GpuMaterial.inner_layer_map_index` (`crates/renderer/src/vulkan/material.rs:304`). The compiler cannot
  flag the dead write because `#[derive(Debug, Default, Clone)]` on the struct
  (`refr.rs:51`) suppresses the `dead_code` field lint.
- **Impact**: A REFR that overrides its base mesh's inner/multi-layer-parallax
  texture — ice/glass panes, layered display cases, Skyrim SE and FO4 multi-layer
  content, and any Starfield REFR once a `.mat` overlay path exists — renders with the
  base mesh's inner layer, or none. Silent: no warn, no telemetry, and the
  regression-test file `refr_texture_overlay_tests.rs` never asserts on `inner`.
- **Related**: #2627 (the BGSM merge's sibling `inner_layer` gap — same role, the other
  producer; fixing one without the other still leaves the role unreachable from REFR
  overrides), #2594 (`fill_from_bgsm` role coverage).
- **Suggested Fix**: In `resolve_mesh_paths`, add
  `textures.inner_layer = resolve_to_owned(&pool, ov.and_then(|o| o.inner).or(mesh.material.textures.inner_layer));`
  next to the existing `wrinkle` fill, and extend
  `refr_texture_overlay_tests.rs` with a slot-6 XTXR round-trip assertion so the
  derive-suppressed dead write can't come back.

---
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-12.md` (`NIFAL-D8-2026-08-12-03`), `AUDIT_FO4_2026-08-12.md` (`FO4-D6-01`), `AUDIT_STARFIELD_2026-08-12.md` (`SF-2026-08-12-D9-01`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

