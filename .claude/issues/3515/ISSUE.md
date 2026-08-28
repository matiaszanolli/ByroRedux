# Issue #3515: the same texture_clamp_mode field carries two different defaults across the three material tiers

**Filed**: 2026-08-27 · **Source**: `docs/audits/AUDIT_FO4_2026-08-27.md`

- **Severity**: LOW
- **Dimension**: 5 (FO4 shader flags) ∩ 7 (NIFAL canonical translation)
- **Location**: `crates/nif/src/import/material/mod.rs:1075` and `:1202` (`MaterialInfo`, default `3`) vs. `crates/nif/src/import/types.rs:643` (`ImportedMaterial`, default `0`) vs. `crates/core/src/ecs/components/material.rs:551` (`Material`, default `0`)
- **Source**: `docs/audits/AUDIT_FO4_2026-08-27.md` — finding `FO4-2026-08-27-D5-02`

## Description

`3` is `WRAP_S_WRAP_T`, the Gamebryo default that `#610` established and that `resolve_texture` hardcodes for its clamp-unaware variant (`byroredux/src/asset_provider/texture.rs:285-297`, "3 = WRAP_S_WRAP_T per nif.xml — the legacy REPEAT default"). `0` is `CLAMP_S_CLAMP_T`, the *opposite* end of the enum.

`MaterialInfo` — the tier the NIF walker actually fills — uses `3`; the two tiers below it default to `0`, and `Material::texture_clamp_mode`'s own doc rationalises this as mirroring "that struct's own `0` (CLAMP_S_CLAMP_T) parser-stub default" (`material.rs:388-390`).

Today the divergence is inert on every real path because `into_imported_material` overwrites the field verbatim (`mod.rs:1455`) and the only production `ImportedMesh::from_geometry` consumers that keep `ImportedMaterial::default()` are the fog volumes (`byroredux/src/fog.rs:1151`, `:1176`), which are untextured. The FO4 precombine path — named in `from_geometry`'s own doc as its other production consumer (`types.rs:844-846`) — is safe only because `into_imported_mesh` reassigns `mesh.material` immediately afterwards (`crates/nif/src/import/precombine.rs:84`).

## Evidence

The three literals above; `resolve_texture_with_clamp`'s registry contract, where `0` selects the CLAMP/CLAMP sampler and out-of-range values fall back to `3` (`texture.rs:298-305`, `crates/renderer/src/texture_registry.rs:171-183`).

## Impact

Latent. The next synthetic-geometry producer that builds through `ImportedMesh::from_geometry` and *does* bind a tiling texture — distant object LOD, terrain LOD, a future `_precomb.nif` collision-visual path — will silently get CLAMP/CLAMP on a tiling atlas and read as one stretched edge texel per axis. The wrong default is also load-bearing documentation: the `Material` doc currently teaches the reader that `0` is the field's default, which contradicts `#610`'s rule.

## Related

- `FO4-2026-08-27-D5-01` (filed as its own issue) — the live half of the same field
- `#610`
- `#2571` / OBL-D5-01 — which propagated the `0` down to `Material`

## Suggested Fix

Set `ImportedMaterial::default().texture_clamp_mode = 3` and `Material::default().texture_clamp_mode = 3`, matching `MaterialInfo` and `resolve_texture`'s own hardcoded fallback, and correct the two doc comments. Note this is a saved `Material` field (`FORMAT_MAJOR` 6 made it required, `crates/save/src/snapshot.rs:65-67`) but changing a *default* does not change the serialised shape, so no format bump is needed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — other fields whose default diverges across the `MaterialInfo` → `ImportedMaterial` → `Material` tiers
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
