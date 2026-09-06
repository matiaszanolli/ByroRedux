# #3924: SK-2026-09-05-D5-01: the runtime archive chain has no present-only optional tier, so AE / Creation Club archives are unreachable

Filed from `docs/audits/AUDIT_SKYRIM_2026-09-05.md` (SK-2026-09-05-D5-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `low,game:skyrim,legacy-compat,import-pipeline,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3924 --json state`.

---

**Source**: `docs/audits/AUDIT_SKYRIM_2026-09-05.md` (SK-2026-09-05-D5-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: 5
- **Location**: `assets/debug_profiles.toml` (`[profiles.skyrim_se]`),
  `byroredux/src/game_profiles.rs`, `byroredux/src/asset_provider/archive.rs`
  (`open_with_numeric_siblings`)
- **Status**: NEW
- **Description**: the installed Data directory is a stock Anniversary
  Edition install and carries six archives the `skyrim_se` profile lists
  nowhere and no auto-load rule can reach: `_ResourcePack.bsa` (916 MB),
  `ccBGSSSE001-Fish.bsa`, `ccBGSSSE025-AdvDSGS.bsa`,
  `ccBGSSSE037-Curios.bsa`, `ccQDRSSE001-SurvivalMode.bsa`, and
  `MarketplaceTextures.bsa`. None is a numeric sibling of a listed archive,
  so `numeric_sibling_paths` cannot find them, and the profile has no
  optional/present-only archive field.

  The NIF corpus gate already models this correctly and separately:
  `Game::optional_mesh_archives` in `crates/nif/tests/common/mod.rs` sweeps
  exactly this present-only tier (that is what took the #3369 headline from
  32 709 to 33 424 files). The concept exists **only** in the test harness;
  the runtime has no counterpart.
- **Impact**: bounded today — the profile also loads only `Skyrim.esm`, so
  nothing in a default launch references CC assets. It becomes a real 404
  surface the moment a user passes `--esm ccBGSSSE001-Fish.esm` or
  `--master _ResourcePack.esl`, which is the natural way to reach AE content
  and the exact case the corpus gate was extended to cover. The asymmetry
  also means the corpus gate is measuring content the engine cannot render.
- **Related**: #3369 (the corpus split that introduced `optional_mesh_archives`),
  #3896 / #3637 (archive precedence), #2584 (sibling dedup).
- **Suggested Fix**: give `GameProfile` a present-only optional archive list
  (skip silently when absent, appended last to match #3637's precedence) and
  seed `skyrim_se` from the same names the test harness already enumerates,
  so the two tiers cannot drift.

---

# Dimension 6 — Specialty Blocks + Real-Data Rendering

## Block-dispatch guards — all intact

| Guard | Site |
|---|---|
| `"BSLODTriShape" => NiLodTriShape::parse` (#838 — must NOT route through BSTriShape) | `crates/nif/src/blocks/mod.rs` |
| `"BSMeshLODTriShape" => BsTriShape::parse_lod` | same |
| `"BSSubIndexTriShape"` its own arm | same |
| `"BSLagBoneController" => BsLagBoneController::parse` (#837) | same |
| `BsProceduralLightningController::parse` (#837) | same |

No realignment or `block_size` recovery WARN was observed on any of the
32 709 entries swept.

## Distant LOD

`.btr` terrain LOD (`byroredux/src/cell_loader/terrain_lod_btr.rs`) and `.bto`
object LOD (`byroredux/src/cell_loader/object_lod.rs`) both parse and import
cleanly — 13 847 entries in `Skyrim - Meshes1.bsa`, 0 failures, 9 619 of them
MSN-flagged terrain/architecture quads. The band ladder
(`LodBandLadder::for_object_game`) is unchanged since the 2026-08-30
verification against the game's own `Ultra.ini`.

Terrain LOD is the one MSN class SK-2026-09-05-D2-01 does **not** visibly
damage, for the reason given in that finding — recorded here so a fix is not
validated on the LOD view and declared green.

---

# Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice)

## Verified clean

* **Single boundary**: `translate_material` in
  `byroredux/src/material_translate.rs` remains the only
  `ImportedMesh → Material` path. `Material::classify_pbr` is still deleted;
  `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`) is
  the resolve-once site and delegates to `classify_pbr_keyword`.
* **Ordering**: `material.resolve_pbr()` runs immediately before
  `crate::helpers::classify_glass_into_material`, so forced-glass roughness
  still wins over the keyword default.
* **`EmissiveSource` discriminator (#1280)**: production writes exist in
  `crates/nif/src/import/material/dedicated_shader.rs` (`Lighting` for
  `BSLightingShaderProperty`, `Effect` for `BSEffectShaderProperty`) and
  `crates/nif/src/import/material/legacy_properties.rs` (`Material` for
  `NiMaterialProperty`), each pinned by
  `crates/nif/src/import/material/emissive_source_tests.rs`. Skyrim's
  `emissive_multiple` routes through `Lighting`, not `Effect` — confirmed.
* **`specular_authored` (#2573)** now flows `MaterialInfo` → `ImportedMaterial`
  → `Material` and is read by `resolve_pbr`'s backstop instead of being
  hardcoded `false`, preserving the #1873 chrome-flyer reasoning.

No Dimension 7 findings.

---

## Cell-Load Regression Status

* TES5 cells parse through the unified `crates/plugin/src/esm/cell/` walker;
  compressed records still decompress (the `BleakFallsBarrow01` probe walks a
  full interior and enumerates every ACHR/NPC_/WEAP leaf).
* `Skyrim.esm` continues to resolve the frozen `p2` triple
  (CELL / reference-base pair / weapon family) byte-for-byte.
* Whiterun BanneredMare control-bench entity count and FPS were **not**
  re-measured — that requires an engine launch, which this run was instructed
  not to perform. Cite ROADMAP's Bench-of-record; nothing in this audit's
  static findings would move the entity count.

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_SKYRIM_2026-09-05.md
```

Label every finding `game:skyrim` + `legacy-compat`, plus:
SK-2026-09-05-D2-01 → `high` `bug` `shaders` `renderer` `nifal`;
SK-2026-09-05-D4-01 → `medium` `bug` `character` `test-gap`;
SK-2026-09-05-D5-01 → `low` `enhancement` `import-pipeline`.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
