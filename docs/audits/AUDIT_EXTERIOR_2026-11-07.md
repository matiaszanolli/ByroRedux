**HEAD**: cb9faca7d · **Baseline**: [AUDIT_EXTERIOR_2026-10-31.md](AUDIT_EXTERIOR_2026-10-31.md) (HEAD `670f81917`) · **Audited**: Dim 1 (EXAL boundary), Dim 2 (terrain), Dim 3 (ground cover), Dim 4 (sky / weather / sun), Dim 5 (water), Dim 6 (distant LOD / trees), Dim 7 (gates, static only) · **Unchanged since baseline (skimmed)**: Dim 5's water-specific files (none of `env_translate/water.rs`, `cell_loader/water.rs`, `systems/water.rs`, `commands/water.rs`, `crates/core/.../water*` changed; only the shared mesh-spawn step and the Z-up→Y-up move touched it); Dim 6's ring partition/eviction/admission code (only `terrain_block_bound` and comment/census changes)

# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-11-07

**Method.** Delta-first against `670f81917` (`git log 670f81917..HEAD -- <Paths>` per dimension, hunks read). I did Dims 1, 4 and 7 myself; Dims 2+3 and Dims 5+6 were each audited by one read-only sub-agent, and I spot-checked the claims below that I carry into findings. No windowed engine was launched. Tests run: `cargo test -p byroredux terrain` (111 passed, 3 ignored), `-p byroredux-terrain-height` (11), `-p byroredux-renderer groundcover` (72), `-p byroredux groundcover` (69), `-p byroredux-core --features inspect groundcover` (22), `-p byroredux water` (173, 4 ignored), `-p byroredux lod` (220, 9 ignored), plus `scripts/check-playable-smoke-contracts.sh`, `m-exteriors.sh --self-test`, `scripts/check-shader-artifacts.sh` (40 shaders match glslang byte for byte) and `bash -n` on the gates: all green. Dedup against `/tmp/audit/issues.json` (open issues), the 2026-10-31 report and closed-issue lookups.

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 3 new (1 more is existing #6248) |

No tier-invariant violation in this window. All four new/related items are in Dim 3 (ground cover), added by the 2026-11-04 grass-colour/gust/blade retune commits (`da6d65dc1`, `69a3b0b30`, `4c48e5754`): untracked shader literals, two untested Rust↔GLSL couplings, one stale doc sentence. The 10-31 report's four findings are all closed (see the register).

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| single-boundary | **holds.** No second `translate_sky` / `translate_weather` / `translate_exterior_cell_lighting` caller outside `scene/world_setup.rs`; no production `SkyParamsRes {` / `WeatherDataRes {` literal outside `env_translate/` and `components/environment.rs`; every non-test `WaterMaterial {` literal is `water_material_from_mesh`, and `resolve_water_material` builds from `WaterMaterial::default()`. Both mesh spawn paths now reach `attach_mesh_water` only through the shared `spawn_mesh_entity` step (`MeshWaterPlacement` arguments diffed identical to the two old call sites). |
| no-fabrication | **holds, with one soft spot.** The new exterior haze floor (`ExteriorHazeFloor`, `fog.rs`) is a documented engine choice (owner decision VLM-HF-008), `SUN_ANGULAR_RADIUS` is sourced (NASA, 959.5″), the Skyrim MATT→Havok material is sourced (nif.xml `SkyrimHavokMaterial` = CRC32). The soft spot is EXT-D3-02 (inline art-direction literals in the blade shaders). |
| no-leak | **holds.** `grep -rn GameKind crates/physics/src` is empty; no game token in `sky*`, `clouds`, `groundcover_*` or water/LOD GLSL code (only comments). |
| no-render-time-fallback | **holds.** The haze floor is applied at the frame builder with no game branch (interior and diagnostic-fog-override excluded, one application point, pinned by `the_floor_is_applied_to_exteriors_only_at_the_frame_boundary`). |

## Per-Category Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Terrain | `spawn_terrain_mesh` / `build_cell_splat_layers`; MATT material in the parse tier (`land_surface_material`) | ok | ok | ok | ok |
| Sky | `translate_sky`, `procedural_fallback_sky` (both read `SUN_ANGULAR_RADIUS`) | ok | ok | ok | ok |
| Weather / sun | `translate_weather`, `build_tod_keys` (now `env_translate/tod.rs`, a clean move of 181 lines) | ok | ok | ok | ok |
| Water | `resolve_water_material`, `water_material_from_mesh` | ok | ok | ok | ok |
| Ground cover | `groundcover_translate.rs`, `AuthoredCover::union_colour` | ok | **EXT-D3-02** | ok | ok |
| Distant LOD | `terrain_lod_layout`, `object_lod_scheme`, `terrain_lod_albedo_gain` | ok | ok | ok | ok |

## Findings

### EXT-D3-02: new bare literals in `groundcover_*` GLSL
- **Severity**: LOW
- **Dimension**: Ground-cover pipeline
- **Location**: `crates/renderer/shaders/groundcover_blade.frag` (`groundLab.x > 0.1` near-black ground cutoff; the 6-iteration gamut bisection); `crates/renderer/shaders/groundcover_blade.vert` (`vec2(17.3, 41.9)` height-field noise offset; the `.a > 0.5` known-colour tests are a flag convention, not a tunable)
- **Tier Violated**: no-fabrication
- **Game Affected**: all
- **Status**: NEW
- **Description**: `da6d65dc1` named `GROUNDCOVER_NEUTRAL_LUMA`, `GROUNDCOVER_GROUND_COLOUR_FOOTPRINT` and `GROUNDCOVER_HEIGHT_FIELD_*` in `shader_constants_data.rs`, but left these threshold and offset values inline. The skill states that a new bare literal in `groundcover_*` GLSL is a finding (the `tau` literals were fixed by #6414). The OKLab matrices are sourced (Ottosson) and fine.
- **Evidence** (spot-checked): `groundcover_blade.vert:439` `... + vec2(17.3, 41.9)`; `groundcover_blade.frag:260` `groundLab.x > 0.1`.
- **Impact**: untracked art-direction values; the near-black cutoff silently decides when a texture's colour is replaced by the palette chroma.
- **Related**: #4378 (blade-shaping constant set registered uncited in exal-groundcover.md §12.12), #6414.
- **Suggested Fix**: name the cutoff and the noise offset in `shader_constants_data.rs` (for example `GROUNDCOVER_CHROMA_MIN_LIGHTNESS`) and register them as art direction in §12.12.

### EXT-D3-03: two new Rust↔GLSL couplings have no pin
- **Severity**: LOW (test gap)
- **Dimension**: Ground-cover pipeline
- **Location**: `byroGcLocalGust` (`include/groundcover_wind.glsl`) vs `groundcover::local_gust` (`crates/core/src/ecs/components/groundcover.rs`); `byroGcGrassColour` (`groundcover_blade.vert`) vs `cover_density_inputs` (`byroredux/src/cell_loader/terrain.rs`)
- **Tier Violated**: n/a
- **Game Affected**: all
- **Status**: NEW (the gust half is already noted in the skill as unpinned; nothing tracks it)
- **Description**: (a) The gust lattice walk exists twice. The sub-agent read both line by line and they match today (`lowbias32` constants, `salt * 0x9E3779B9`, centre range `[0.3, 0.7]`, the sin² envelope, salt `0xB16B00B5`), and the constants flow from core through the generated header, but nothing stops a one-sided edit. (b) Nothing tests that `layer_grass_colour` follows the splat lane order or that the ordered-mix algebra drops unknown lanes; the coverage is the `union_colour` unit test plus a substring check in `blade_ground_colour_uses_the_shared_terrain_splat_chain`.
- **Impact**: a future edit to one side changes wind or grass hue on screen with all tests green.
- **Suggested Fix**: a CPU mirror test, or a normalised-body shader-text check as `terrain_height_blend_tests` does for the blend; a host test that `cover_density_inputs` fills the colour lanes in the same order as `layer_affinity`.

### EXT-D3-04: `exal-groundcover.md` says the blade widths are unchanged after `4c48e5754` widened them
- **Severity**: LOW (doc rot)
- **Dimension**: Ground-cover pipeline
- **Location**: `docs/engine/exal-groundcover.md` §12.16 ("…12-27 units; widths unchanged"); the `GroundCoverSpecies` doc comment in `crates/core/src/ecs/components/groundcover.rs`
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Status**: NEW
- **Description**: `4c48e5754` widened the temperate default from 1.2–2.35 to 1.7–3.3 BU and raised `GROUNDCOVER_CHROMA_VIBRANCE` to 1.6; the doc still says widths are unchanged and records no vibrance value, and the core doc comment still describes earlier retunes. (This audit skill's own Dim 3 text already carries the new widths and the 1.6 value.)
- **Impact**: a reader tuning the species trusts the doc over the code.
- **Suggested Fix**: update §12.16 and the §8 note; refresh the species doc comment.

## Existing (not re-filed)

- **#6248** (every grass-blade fragment recomputes the root ground colour with up to ~18 bindless fetches) is open and now worse in one specific way: since `da6d65dc1` the blend reads `vGrassColour.rgb` whenever `vGrassColour.a > 0.5` (the common Skyrim case, `AuthoredCover::localised`), so `rootGroundAlbedo` is never read, yet the base fetch, the `byroTerrainHeightBlend` height fetches and `byroTerrainSplatAlbedo` still run for every fragment. Suggested addition to #6248: wrap the ground-sample block in `if (vGrassColour.a <= 0.5)` (`vGrassColour` is `flat` and the gradients are computed outside the branch, so `textureGrad` inside it is legal).
- **#6048** (terrain collider LTEX Havok data), **#6106** (full-detail refusal never retried), **#3307** (VWD full-model culling): open, code unchanged except `4ce5ddf2a`, which adds the Skyrim MATT material half of #6048 (friction / restitution are still decoded and unused).
- **#6633–#6640** (M35 distant-LOD completion) are tracked work, not defects.

## Verification notes

- **Skyrim "Use Land Data" gate (#6594, `d548def7d`)**: `needs_exterior_processing` now includes Skyrim; the real `Skyrim.esm` census reproduces (308 cells, per-world counts). A throwaway probe over Update / Dawnguard / HearthFires / Dragonborn shows they only re-author the same Whiterun / Windhelm / Riften / Solitude worlds' cells, so the inheritance stays consistent for the DLC. Not visually confirmed on a Whiterun gate approach.
- **One parse for both session kinds (#6591)**: `exterior_ready` and the `exterior` parameter are gone; `one_parse_serves_an_interior_session_and_then_an_exterior_one` pins it. The skill text and `exal.md` were updated in the same changes.
- **`terrain_block_bound` (`5425bd4d8`)** adds the previously missing guard `terrain_block_bound_covers_skirts_beyond_the_hole_mask`.
- **Z-up→Y-up move (`13647f400`)**: math byte-identical, every caller repointed to `crate::zup_yup::`, the `--rotation-mode` dispatcher is still the single path (`rotation_zup_to_yup_quat` → `euler_zup_to_quat_yup_refr`), the #2435 source-scan guard was repointed.
- **Guards**: about 100 `Guard:` names across Dims 1–6 were grepped; none vanished, none of the load-bearing ones is `#[ignore]`d. The only `#[ignore]` ones are the real-data tests the skill already documents.
- **Layout lockstep**: `GpuGroundCoverCell` grew 64→208 B (13 × 16), pinned by `gpu_records_match_their_std430_layout` and `shader_contract_tests.rs`; `GpuTerrainTile` is untouched (224 B).

## Notes outside this audit's ownership

- `systems/billboard.rs`: `wind_active = true` now rewrites every `SpeedTreeWind` tree's rotation each frame in all weathers (`69a3b0b30`, an intended trade-off per the commit). If the physics or TLAS layers react to `GlobalTransform` changes it adds per-frame refit cost → `/audit-performance`.
- `authored_cover_colour.rs` cannot decode BC7 (`Rgba8::decode_dds` in menuxml), so FO4 grass cards in BC7 silently fall back to the ground's colour; the "N/M records carry a texture colour" log is the only signal. Graceful, but worth a line in exal-groundcover.md.
- Skill text drift (fix on the next `/audit-sync`): Dim 6 calls the tree-LOD cross shape "an interpretation", but `tree_lod.rs` now sources the two-crossed-planes shape to DynDOLOD help (only the 0.5 alpha cutoff stays unsourced).

## Known-Open Register (what this pass changed)

| Item | Owner doc / issue | This pass |
|---|---|---|
| 10-27 CRITICAL `holes[i]` out-of-bounds, fix unguarded (EXT-D6-2026-10-31-01) | `5af77a128` | **guarded** by `terrain_block_bound_covers_skirts_beyond_the_hole_mask` (`5425bd4d8`), verified |
| `exal.md` "Oblivion and FO3/FNV only … not measured on Skyrim+" (EXT-D1-2026-10-31-01) | exal.md | **fixed** for FO4 earlier and for Skyrim by #6594 (`d548def7d`); now reads "not measured on the Starfield or Fallout 76 children" |
| #6464 test spliced from its doc (EXT-D1-2026-10-31-02) | #6552 | **closed**, verified (`daa68ef37`) |
| `m-exteriors.sh --self-test` run by no lane (EXT-D7-2026-10-31-01) | #6553 | **closed**, verified (the contract checker runs it; it passes) |
| `w1` README row | #6421 | **closed** |
| Blade-length sourcing | #6201 / #6413 | #6201 closed; the draw remains an unsourced placeholder |
| Terrain collider LTEX Havok material | #6048 | **partly done**: Skyrim MATT material landed (`4ce5ddf2a`); friction / restitution unapplied; still open |
| Full-detail refusal never retried | #6106 | unchanged |
| VWD full-model culling | #3307 | unchanged |
| Skyrim "Use Land Data" children | #6594 | **closed**, census reproduced; visual confirmation owed |
| Interior→exterior re-parse for gated games | #6407 / #6591 | **gone**: one parse serves both session kinds |
| FO4 `Goodneighbor` co-location | #6464 | not visually confirmed; the Nuka-World pair carries the justification |
| Exterior haze floor (new engine choice) | #6726, VLM-HF-008 | documented, owner-decided, game-invariant; defaults chosen from captures (VLM-SH-011) |

Next step: `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-11-07.md` (labels `terrain-exterior`, plus `shaders`, `test-gap`, `doc-rot`; `renderer` for the shader literals).
