# Legacy Compatibility Audit — 2026-06-23

**Scope**: Cross-layer mapping shape (NIFAL / EXAL / PHYSAL) + subsystem coverage
vs Gamebryo 2.3 / Creation Engine. Framing per `.claude/commands/audit-legacy-compat/SKILL.md`.

**Method**: Each dimension traced inline against the live tree (no nested sub-agents).
Boundaries verified single-producer by grepping the symbol and reading every construction
site. Each finding adversarially disproved before keeping. Dedup against the cached
OPEN-issue list (`/tmp/audit/issues.json`, 28 entries) + closed-issue spot checks via `gh`.

## Summary

3 findings: 0 CRITICAL, 0 HIGH, 2 MEDIUM, 1 LOW.

The three canonical-translation boundaries are **structurally sound**. NIFAL
(`translate_material`), EXAL (`env_translate`), and PHYSAL (`extract_ragdoll`) are each
single-producer with no surviving render-time fallback and no downstream `if game ==`
branch. The findings are (a) a second exterior-default producer outside the EXAL boundary
with divergent constants, and (b)(c) the documented-but-untracked older-game distant-object
LOD coverage gap. No regressions of closed issues were found.

### Dimension pass/clean notes

- **D1 Coordinate-system** — CLEAN. `crates/core/src/math/coord.rs` remains the sole
  source for the `(x,z,-y)` swap, the WXYZ→XYZW quat path (with the #333 normalize), the
  ZYX-product REFR Euler (`euler_zup_to_quat_yup`), and `EXTERIOR_CELL_UNITS = 4096.0`.
  The apparent duplicate swaps in `mesh/skin.rs` (C·R·Cᵀ rotation-similarity, translation
  routed through the SoT) and `mesh/sse_recon.rs` (per-vertex decoder, bit-identical to the
  inline parser) are the documented decoder/similarity pattern, not the pre-#1044
  five-site duplication. No new `4096.0` cell-math literal; `RENDER_ORIGIN_SNAP` and the
  `fog_volume`/`facegen`/`dds` literals are unrelated constants. `--rotation-mode` routes
  through `cell_loader/euler.rs::euler_zup_to_quat_yup_refr`; no caller hardcodes a mode.
- **D2 NIFAL shape** — CLEAN. Per-category leaks match `nifal.md` §2 (converged:
  materials/geometry/skinning/lights/animation/shader-flags; parked: 4 `ImportedNode`
  fields + passthrough table). No second `translate()` site.
- **D3 Material boundary** — CLEAN. `byroredux/src/material_translate.rs::translate_material`
  is the sole populated-`Material` producer (the only other `Material {}` constructions are
  `cornell.rs`, the self-contained RT reference scene, and a `helpers.rs` test fixture).
  `Material::resolve_pbr` fills metalness/roughness once from the NaN sentinel; the deleted
  render-time `classify_pbr` survives only in explanatory comments. NiFogProperty
  intentionally-not-dispatched and the EmissiveSource ~1.0 scale are both intact regression
  guards — not re-filed.
- **D4 PHYSAL** — CLEAN. `extract_ragdoll` switches on `BhkConstraintData`, never on game;
  the per-game seam stays confined to the typed CInfo decoders. The
  `BhkConstraintData::Other => continue` drop is already tracked (D7-02 / #1539, OPEN).
- **D5 EXAL** — one finding (LC-D5-01 below). `env_translate.rs` `translate_*` /
  `procedural_fallback_*` are `pub(crate)` and single-producer; `world_setup.rs` is the
  only caller. The `WeatherDataRes {}` / `CellLightingRes {}` constructions in
  `systems/weather.rs` and `systems/audio.rs` are inside `#[cfg(test)]` fixtures.
- **D6 Per-game survey** — CLEAN. Version gates in the parser tier (`morph.rs`,
  `header.rs`, `sequence.rs`, `material/walker.rs`) use named `bsver::*` constants or are
  wire-format Pattern-B gates — no raw-constant Pattern-A bypass in import/cell_loader.
- **D7 Subsystem coverage** — two findings (LC-D7-01, LC-D7-02 below). All 12 `NiProperty`
  types accounted for (alpha/zbuffer/material/texturing/stencil/specular/wireframe/shade/
  vertexcolor dispatched; dither no-op'd with rationale; fog documented-skip). Uniform-`f32`
  `Transform.scale` mirrors Gamebryo's scalar `NiTransform` scale exactly — **not** a
  fidelity gap (the spec's "non-uniform scale collapsed" premise is false; the legacy engine
  has no node-level non-uniform scale either).

---

## Findings

### LC-D5-01: Second exterior no-weather default producer outside the EXAL boundary with divergent constants
- **Severity**: MEDIUM
- **Dimension**: 5 — EXAL (no render-time fallback)
- **Location**: `byroredux/src/systems/weather.rs:208-235` (`apply_neutral_exterior_fallback` + `NEUTRAL_*` consts) vs `byroredux/src/env_translate.rs:331-429` (`procedural_fallback_*` + `FB_*` consts)
- **Status**: NEW
- **Description**: The EXAL contract requires the "no climate/weather" case to be one
  explicit canonical default at the translate boundary (`procedural_fallback_*`), never a
  branch in the runtime/render loop. There are **two** producers of the exterior-default
  lighting, with **divergent values**:
  - Boundary (`env_translate.rs`): `FB_AMBIENT [0.15,0.14,0.12]`, `FB_FOG_COLOR [0.65,0.7,0.8]`, `FB_FOG_NEAR 15000`, `FB_FOG_FAR 80000`, `FB_SUNLIGHT [1.0,0.95,0.8]`.
  - Runtime system (`weather.rs::apply_neutral_exterior_fallback`): `NEUTRAL_AMBIENT [0.4,0.4,0.4]`, `NEUTRAL_FOG_COLOR [0.5,0.55,0.6]`, `NEUTRAL_FOG_NEAR 1000`, `NEUTRAL_FOG_FAR 50000`, `NEUTRAL_SUNLIGHT [1.0,1.0,1.0]`.
- **Evidence**: `weather_system` (`weather.rs:366-379`) takes the neutral path when
  `WeatherDataRes` is absent and writes the `NEUTRAL_*` set directly into `CellLightingRes`.
  `world_setup.rs:361` already inserts `procedural_fallback_weather()` (the `FB_*` set) when
  no climate record is present. The same logical state ("exterior, no authored weather")
  thus resolves to two different lighting looks depending on which path executed.
- **Impact**: Inconsistent exterior fallback lighting (ambient differs by ~2.6×, fog
  distances differ by 10–15×). Visible only on the narrow window where `WeatherDataRes` is
  missing at `weather_system` time (e.g. pre-cell-load / demo states); after a normal
  exterior cell load the boundary fallback wins, so blast radius is small. The contract
  violation is the duplicated default living in a system rather than at the one boundary.
- **Related**: EXAL "no render-time fallback" rule (`exal.md` §3); #463 / #1034 (fallback history).
- **Suggested Fix**: Have `apply_neutral_exterior_fallback` consume the canonical
  `procedural_fallback_cell_lighting` (or its `FB_*` constants) instead of a private
  `NEUTRAL_*` set, collapsing to one source of truth for the exterior no-weather default.

### LC-D7-01: Oblivion/FO3/FNV distant-object LOD (`_far.nif` placement scheme) unimplemented
- **Severity**: MEDIUM
- **Dimension**: 7 (subsystem coverage) / 5 (EXAL LOD)
- **Location**: `byroredux/src/cell_loader/object_lod.rs` (Skyrim/FO4 `.bto`-only); no `PlacementLodProvider` for the `DistantLOD\<W>_<x>_<y>.lod` → `_far.nif` scheme
- **Status**: NEW
- **Description**: Distant **object** LOD is implemented only for Skyrim/FO4 (baked `.bto`
  quad atlases). The older-game distant-object scheme — `DistantLOD\<W>_<x>_<y>.lod`
  placement files instancing per-object `_far.nif` low-poly meshes — is unimplemented
  (`exal.md` §5, line 149: "`_far.nif` placement scheme remains unimplemented
  (`PlacementLodProvider`)"). Distant *terrain* for these games is covered by the heightmap
  synthesis fallback (`terrain_lod.rs:187-188`), so this is objects-only.
- **Evidence**: `terrain_lod.rs:258` gates `.btr` prebaked distant terrain on
  `GameKind::Skyrim | GameKind::Fallout4`; `object_lod.rs` `stream_object_lod_blocks` only
  walks `.bto` blocks. No grep hit for `_far.nif` / `DistantLOD` reading in any non-comment
  source. `exal.md` line 238 lists the Oblivion/FO3/FNV distant-object source as
  `DistantLOD\<W>_<x>_<y>.lod` → `_far.nif`, unimplemented.
- **Impact**: Oblivion / FO3 / FNV exteriors render no distant *object* LOD — the horizon
  beyond the loaded cell radius is missing buildings/rocks/landmarks the source games show.
  Terrain horizon is present (synth fallback); object silhouettes are not. Visible content
  gap on the reference title (FNV) and three of the six target games. No parse failure.
- **Related**: LC-D7-02 (the VWD flag this scheme would also consume); `exal.md` §5.4.
- **Suggested Fix**: Add a `PlacementLodProvider` that parses `DistantLOD\<W>_<x>_<y>.lod`
  placement entries and instances the corresponding `_far.nif` (resolved via the existing
  NIF parser + asset provider), spawned as `IsLodTerrain`-style LOD entities like the `.bto`
  path. Track as the older-game counterpart to the existing Skyrim/FO4 object-LOD slice.

### LC-D7-02: VWD / "Has Distant LOD" record-header flag (0x00010000) not parsed
- **Severity**: LOW
- **Dimension**: 7 (subsystem coverage) / 5 (EXAL LOD)
- **Location**: `crates/plugin/src/esm/reader.rs:18-19,486-499` (only `FLAG_COMPRESSED = 0x00040000` decoded)
- **Status**: NEW
- **Description**: The base-record header *Visible-When-Distant* / "Has Distant LOD" flag
  (`0x00010000`) is never read. The record-header flag decoder captures only
  `FLAG_COMPRESSED` (0x00040000), plus the TES4-file-header Localized (0x80) and
  Light-Master (0x0200) bits. `exal.md` §5.4 (line 350, 455) names this flag as the small
  parser gap blocking proper full-model VWD culling.
- **Evidence**: `reader.rs:19` defines exactly one record `FLAG_*` constant. `header.flags`
  is stored (`reader.rs:486-499`) but never masked against `0x00010000`. `object_lod.rs:97`
  comment confirms "The base record's VWD / 'Has Distant LOD' flag is … the full-model VWD
  cull is deferred". Distinct from the deleted-REFR 0x20 tombstone flag (SKY-D4-01 / #1660).
- **Impact**: Without the flag the engine cannot decide *which* full-resolution base models
  to cull once their LOD stand-in is shown, nor which records are LOD-eligible. Today both
  full models and (Skyrim/FO4) LOD quads can co-exist at distance. Low severity: the LOD
  pipeline currently distance-gates by other means and the flag is a refinement, not a
  load-bearing parse step. It is the prerequisite for LC-D7-01's correctness.
- **Related**: LC-D7-01; SKY-D4-01 / #1660 (different flag, 0x20 deleted-REFR — not a dup).
- **Suggested Fix**: Add `FLAG_VISIBLE_WHEN_DISTANT: u32 = 0x00010000`, expose it on the
  parsed record header, and consume it in the LOD spawn path to gate full-model culling and
  LOD eligibility.

---

## Dedup ledger

- Verified against `/tmp/audit/issues.json` (28 OPEN). Pre-existing legacy-compat issues
  re-confirmed still-OPEN and **not** re-filed: D7-02/#1539 (constraint drop), D7-03/#1540,
  SKY-D4-01/#1660 (0x20 deleted-REFR — distinct flag from LC-D7-02), SKY-D5-01/#1661,
  SKY-D3-03/#1659, D4-1/#1655, FO4-D6-LOW-01/#1598, SF-D4-01/#1567, SF-D4-03/#1576,
  SF-D9-02/#1580, D6-06a/#1359, FO3-6-01/#1542, NIF-2026-05-29-05/#1333, the M47.1
  condition-stub family (#1663–#1668, #1316), TD9/TD-D9 LOC-split items.
- Closed-issue spot check (`gh`, state=closed): no `_far.nif` / `DistantLOD` / `VWD` /
  neutral-fallback prior fix exists (#1244 is an unrelated NIF shader-property subclass).
  No regressions detected.

## Next step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-06-23.md
```
