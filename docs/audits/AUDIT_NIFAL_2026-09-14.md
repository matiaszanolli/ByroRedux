# NIFAL Audit — 2026-09-14

Full nine-dimension `/audit-nifal` sweep, no preset narrowing, all seven game
variants (plus Skyrim LE, newly harnessed). Each dimension ran as its own agent
against the live tree (HEAD `7374634f5`). Every agent re-verified the checklist
and regression pins from code rather than from doc prose, and reviewed every
commit touching its entry points since the last sweep. Baseline:
`docs/audits/AUDIT_NIFAL_2026-09-11.md`. All four of that report's open findings
are now closed (#4166, #4167, #3901, #4044). This sweep re-checked each of those
fixes and found new defects in several of them.

The orchestrator independently re-read the code behind every HIGH finding before
merging (glass classifier gates, the `BSEffectShaderProperty` placeholder, the
Starfield bound/vertex scale split, the NIF-light direction column plus the
Gamebryo 3.2 headers, and the absence of any finiteness guard between animation
sampling and AS build).

## Executive Summary

**23 open findings: 0 CRITICAL, 7 HIGH, 2 MEDIUM, 14 LOW.** 22 are NEW; 1 (NIFAL-D1-2026-09-14-04) extends Existing #4246. One cross-dimension duplicate was merged: Dim 9's harness-drift finding is folded into NIFAL-D8-2026-09-14-01, which it corroborates independently.

**The headline: this window's fixes introduced most of the high-severity defects.** Five of the seven HIGH findings are regressions or side effects of issues closed since 2026-09-11. Each landed with synthetic tests only and no census against shipped content:
- **#4237** now promotes ~640 non-glass FNV/FO3 surfaces to refractive glass (D1-01).
- **#4255** splits identical Skyrim alchemy-lab glass by `shader_type` (D1-02).
- **#4250** fabricates `env_map_scale` on every Skyrim effect shader (D8-01).
- **#4166** closed the zero-quaternion hole only on the B-spline path (D7-01), and the static-pose NaN gate it relied on was never real (D7-02).

The remaining two HIGH findings are long-standing:
- **Starfield bounds**: every Starfield mesh's bound is ~70× too small (D2-01; #2098 was closed log-only without the real-data check it asked for).
- **NIF light direction**: embedded NIF spot/directional lights use an uncited −Z axis that contradicts the Gamebryo headers and the sibling ESM boundary (D3-01).

Four of the seven HIGH findings were measured against vanilla archives (D1-01, D1-02, D8-01 partially, D2-01). D7-01/D7-02 are malformed-content-only (0 of 16.06M vanilla keys trigger them) but share #4166's NaN-into-AS blast radius.

Per-category convergence against `docs/engine/nifal.md` §2:

| Category | Spec status | This sweep |
|---|---|---|
| Material | converged | **regressed**: 2 HIGH (glass classifier), 1 LOW latent swap seed, 1 LOW doc |
| Mesh water (NIFAL/WATAL seam) | converged | converged (value-tested; no structural guard) |
| Geometry / Transform | converged (reference template) | **1 HIGH** — Starfield bound units (never actually converged) |
| Skinning | half-stale (documented) | 1 MEDIUM (MorphSlot unreachable), 1 LOW doc; #2440/#2441 still match code |
| Lights | converged | **1 HIGH** — NIF-import direction axis/sign disagrees with the ESM boundary |
| Nodes | triaged by design | clean — 0 findings; 7 parked fields have zero consumers |
| Particles | emitter base converged | 1 MEDIUM (orientation dropped), 2 LOW (guard holes, spec drift); #4044 fixed |
| Collision | audited / converged | code clean; 2 LOW false comments |
| Animation / controllers | converged | **2 HIGH** (finiteness), 2 LOW (identity fabrication, harness values); #4166/#4167/#3901 fixed |
| Shader flags / texture sets | converged | **1 HIGH** (#4250 placeholder), 3 LOW; #3901 leak closed; zero per-game shader branches |
| Cross-cutting completeness | — | harness 8/8 games green (Skyrim LE newly real); 2 LOW (guard self-pin, doc bundle) |

Tier-invariant violations (primary classification per finding):

| Invariant | Count | Findings |
|---|---|---|
| single-boundary | 4 | D2-01, D1-03, D8-02, D5-02 (D3-01 is also a boundary divergence) |
| no-fabrication | 9 | D1-01, D1-02, D8-01, D3-01, D7-01, D7-02, D7-03, D8-03, D6-02 |
| no-leak | 1 | D3-02 |
| parked-not-leak | 4 | D5-01, D6-01, D5-03, D3-03 |
| no-render-time-fallback | 0 | — |
| harness-coverage / doc | 5 | D7-04, D9-02, D9-03, D1-04, D8-04 |

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | PASS (D1-03 LOW latent swap-seed split) | **FAIL**: D1-01 HIGH, D1-02 HIGH | PASS | PASS | `byroredux/src/material_translate.rs::translate_material` (4 production callers + Cornell harness; signature still `&ImportedMaterial`), glass via `byroredux/src/helpers.rs::classify_glass_into_material`, Phase 2 via the three post-texture resolvers |
| Geometry / Transform | **FAIL**: D2-01 HIGH (Starfield sphere skips the Havok unit conversion its vertices get) | PASS | PASS | PASS | per-game extractors → shared `Vec<[f32;3]>` / `Vec<u32>`; `crates/nif/src/import/coord.rs`, `crates/nif/src/rotation.rs`, `crates/nif/src/import/transform.rs` |
| Skinning | N/A by design (#2440 documented) | PASS | **FAIL**: D3-02 MEDIUM (raw `mesh.skin` stands in for canonical `SkinnedMesh` at MorphSlot creation) | PASS | `crates/nif/src/import/mesh/skin.rs` extractors → `SkinnedMesh::new_with_global` in `byroredux/src/scene/nif_loader.rs` |
| Lights | **FAIL**: D3-01 HIGH (NIF and ESM light boundaries disagree on the direction axis) | **FAIL**: D3-01 (uncited −Z) | PASS (renderer reads `emitter.kind` only) | PASS | `crates/nif/src/import/walk/lights.rs::imported_light_from_base` + `byroredux/src/systems/light_anim.rs::translate_light` → `LightSource::from_legacy_world_units` |
| Nodes | N/A by design | PASS | PASS (7 parked fields, zero consumers) | PASS | no single `translate_node` (documented) |
| Particles | PASS (guard holes: D5-02 LOW) | PASS | **PARTIAL**: D5-01 MEDIUM (emitter orientation dropped) | PASS | `byroredux/src/systems/particle.rs::apply_emitter_overlays` (sole overlay site; LUT now inside) |
| Collision | PASS | PASS (code); D6-01/-02 LOW doc | PASS | N/A | `crates/nif/src/import/collision/shape.rs::resolve_shape_inner` (16/16 arms, `dispatch_coverage_tests` green) |
| Animation / controllers | PASS (2 declared boundaries) | **FAIL**: D7-01 HIGH, D7-02 HIGH, D7-03 LOW | PASS | PASS | `byroredux/src/anim_convert.rs::convert_nif_clip` + `byroredux/src/asset_provider/animation.rs::convert_hkx_clip` |
| Shader flags / texture sets | PARTIAL: D8-02 LOW (clamp/parallax precedence split) | **FAIL**: D8-01 HIGH, D8-03 LOW | PASS (#3901 closed; no per-game slot past import) | PASS (zero per-game branches in `crates/renderer/shaders/triangle.frag` + all 25 includes) | block-type dispatch in `crates/nif/src/import/material/` → `MaterialTextureSet<T>` (26 entries, all walks guarded) → `translate_material` |
| Cross-cutting completeness | PASS (every category declares its boundary; this window's new material sites — `.bto` object LOD and the MSWP re-merge — route through `translate_material`; ground cover has none, Existing #4304) | FAIL (D8-01, surfaced independently by the harness drift) | PASS (the only canonical-component discriminator change this window replaces a raw slot with `FlipTextureRole`) | PASS (zero per-game/keyword classification in `byroredux/src/render/` and non-triangle shaders, except Existing #4285 Starfield water unit in `crates/renderer/shaders/water.frag`) | `crates/nif/tests/translation_completeness.rs` 8/8 games green; 96 guard tests green; guard holes D5-02, D7-04, D9-02 |

## Findings

Ordered by NIFAL blast radius within each severity: wrong canonical `Material`
first, then other wrong canonical data, then dropped content, then boundary and
guard hygiene.

### HIGH

#### NIFAL-D1-2026-09-14-01: #4237's `window_env_mapping` glass signal promotes ~640 non-glass FNV/FO3 surfaces to refractive GLASS
- **Severity**: HIGH (NIFAL row: wrong `Material` out of `translate_material`)
- **Dimension**: Material
- **Tier Violated**: no-fabrication (a new positive glass signal landed with synthetic tests only, no census)
- **Game Affected**: FNV, FO3
- **Location**: `byroredux/src/helpers.rs:164` (gate), `byroredux/src/material_translate.rs:736` (argument), `crates/nif/src/import/material/legacy_properties.rs:41` (`legacy_window_env_mapping`)
- **Status**: NEW (introduced by `c9b02ba4a`, the fix for closed #4237)
- **Description**: Before `c9b02ba4a`, an alpha-covered dielectric became `MATERIAL_KIND_GLASS` only through a glass keyword in its texture path or mesh name, or a BGEM glass flag. The fix added `window_env_mapping` as a third, independent trigger "with the same standing as `bgem_glass`". That trigger fires on either the FO3/FNV `Window_Environment_Mapping` bit or the `Eye_Environment_Mapping` bit. The coverage gate accepts `has_alpha || alpha_test`, so any alpha-tested surface carrying either bit is promoted. Vanilla content authors these bits on env-mapped atlases and clutter, not just glass panes.
- **Evidence**: The orchestrator re-read `byroredux/src/helpers.rs:157-168`: `if !keyword_match && !bgem_glass && !window_env_mapping { return; }` is the only positive gate. The agent ran a census that mirrors the classifier's gates:
  - **FNV**: 776 meshes author a bit. 66 were already keyword glass. **458 meshes in 338 NIFs are promoted by the new signal alone** (448 via `alpha_test`). 401 of those use `textures\dlc05\dungeons\mz\mz03.dds`, the Old World Blues Big MT room and corridor shells. The rest include trash piles, `rowhousetrim01.dds` railings, office-building exteriors, gun cabinets, cameras, blood packs and cazador wings.
  - **FO3**: **182 meshes in 61 NIFs are newly promoted**, including 133 `rowhousetrim01.dds` (Georgetown trim) and 40 `storefrontquad01.dds`.
  - #4237's impact note assumed "most vanilla window meshes happen to match a glass keyword". The census shows the reverse: the new signal promotes about 7× more meshes than keywords already did.
- **Impact**: Hundreds of Big MT interior walls, Georgetown facade trim and clutter meshes now shade as smooth refractive dielectric glass through the physical-transmission path (`crates/renderer/shaders/include/ray_hit.glsl:436-439`). No per-draw fallback masks it.
- **Related**: #4237 (closed), #1280, #2315, NIFAL-D1-2026-09-14-02. Test gap: `every_source_derived_material_field_is_pinned_by_a_test` scans only the `Material` literal. None of the arguments `translate_material` passes to `classify_glass_into_material` has a translate-level test.
- **Suggested Fix**: Revert the third trigger, or narrow it to a measured population before landing it again. Options: Window bit only (not Eye); require blend coverage rather than `alpha_test`; or require a co-occurring keyword/name signal. Cite the census in the code comment. Add a translate-level test that drives each glass-classifier argument through `translate_material`.

#### NIFAL-D1-2026-09-14-02: #4255's authored-shader-type guard splits identical Skyrim glass by `shader_type` — alchemy-lab glass loses GLASS, Tolfdir's alembic keeps it
- **Severity**: HIGH (NIFAL row: divergent `Material` for the same authored surface)
- **Dimension**: Material
- **Tier Violated**: no-fabrication ("authored `shader_type` 1..=20 ⇒ not glass" is an unmeasured proxy rule)
- **Game Affected**: Skyrim LE/SE (inline `BSLightingShaderProperty`, `from_bgsm == false`)
- **Location**: `byroredux/src/helpers.rs:118` (`lit_carrier_authored_dispatch`), `byroredux/src/helpers.rs:123-129` (early return)
- **Status**: NEW (introduced by `4a8875b39`, the fix for closed #4255; structural root is Existing: #4256)
- **Description**: `classify_glass_into_material` now returns before any glass gate whenever `material_kind` (the verbatim Skyrim `shader_type`) is in `1..=20` and no external BGSM resolved. That protects MultiLayerParallax (11) ice as intended. It also blocks EnvironmentMap (1), which is how Skyrim authors ordinary glass apparatus. The canonical classification of one physical surface therefore depends on which shader variant the artist picked.
- **Evidence**: The orchestrator re-read `byroredux/src/helpers.rs:104-129` and confirmed the unconditional `1..=20` range. The agent's census of Skyrim SE `Meshes0/1.bsa` (22,047 NIFs) counted glass-eligible meshes by authored kind:
  - Totals: kind 0: 29, **kind 1: 80**, kind 2: 1, kind 11: 7.
  - Kind 1 covers `InnerGlass02` / `OuterGlass02` / `Liquid02:*` on `textures\clutter\plainglasstile01.dds` in `alchemyworkstation.nif`, `alchemyworkbench.nif`, `workbenches\alchemyworkbench01.nif`, `workbenches\alchemyworkstation01.nif`, the load-screen and Hearthfire variants, and `winterholdbookcase01.nif` (26 glass rows).
  - The same sub-mesh names on the same texture in `alchemytolfdirsalembic01.nif` are authored as kind 0 and still classify as GLASS.
  - The guard does remove real false positives on kind 1 (ice wraith, `dragon_snow`, ice floes). Identical false positives remain on kind 0 (`dragon_icelake`, `icevine01*`, Solitude `swindow02` shutters). The guard is therefore neither a glass signal nor a false-positive filter; it is a shader-type partition.
- **Impact**: Every player-usable alchemy lab renders its glass apparatus as an alpha-blended env-mapped shell (kind 1 gets only an env-cubemap response, `crates/renderer/shaders/triangle.frag:2778`). Tolfdir's alembic, the same asset family, renders as refractive glass.
- **Related**: #4255 (closed), #4256 (OPEN), #2710 / `322f33a8` (the `InnerHaze` effect-layer precedent — kind 101, a different population), NIFAL-D1-2026-09-14-01.
- **Suggested Fix**: Scope the guard to kinds whose dispatch really conflicts with glass. MultiLayerParallax (11) is measured; census the others first. Do not guard EnvironmentMap (1). Handle keyword false positives (`ice` on dragons and wraiths) in the keyword list or name gate, not by shader type. Pin the fix with a test built from the two alchemy meshes.

#### NIFAL-D8-2026-09-14-01: #4250 fabricates `env_map_scale = 1.0` for a wire field Skyrim does not have, flipping every inline Skyrim effect shader into the "authored environment mapping" PBR arm
- **Severity**: HIGH (NIFAL row: wrong `Material` out of `translate_material`). The dimension agent proposed MEDIUM because every current GPU consumer short-circuits for `material_kind == 101`. The orchestrator re-floored it to HIGH: `.claude/commands/_audit-severity.md` makes the NIFAL row a minimum, and this canonical-tier error surfaces the moment the deferred `base_color_scale` effect render path lands.
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: no-fabrication
- **Game Affected**: Skyrim LE + SE (every `BSEffectShaderProperty` with BSVER < 130); FO4+ reads the real field
- **Location**: `crates/nif/src/blocks/shader.rs:1936-1956` (placeholder now `1.0`), `crates/nif/src/import/material/dedicated_shader.rs:535` and `:583-584` (copy + latch), `crates/nif/src/import/material/mod.rs:1407-1416` (`classify_legacy_pbr`), `crates/core/src/ecs/components/material.rs:1209` (`env_map_scale > 0.3` arm)
- **Status**: NEW (introduced by `797e82124`, the fix for closed #4250)
- **Description**: nif.xml defines `Env Map Scale` on `BSEffectShaderProperty` for FO4+ only. #4250 changed the parser's not-present placeholder from `0.0` to `1.0`, calling `1.0` "neutral". The engine's convention for *unauthored* environment mapping is `0.0`:
  - `legacy_env_map_scale` returns `0.0` when no env flag is authored.
  - `ImportedMaterial::default()` is `0.0`.
  - `translate_texture_only_material` deliberately overrides `Material::default()`'s `1.0` to `0.0`.
  - The water translator treats `0` as the absence sentinel.

  The fabricated `1.0` latches (#4251) and crosses the classifier's `> 0.3` gate, whose own comment reserves it for surfaces that "DO author real environment mapping". The canonical `Material` gets `env_map_scale 1.0` and `roughness 0.80` instead of `0.0 / 0.85`.
- **Evidence**: The orchestrator confirmed `crates/nif/src/blocks/shader.rs:1955` returns `1.0` for the non-FO4 arm. The agent ran `material_dump` on live Skyrim SE content:
  - `fxambbeamdust00.nif`: `BeamMeshDust05:0` and `BeamMeshStatic04` both import as `kind 101, env 1.00, rghO 0.80`.
  - `fxglowfillroundmid.nif`: `GlowMesh01:0` imports as `kind 101, env 1.00, rghO 0.80`.
  - None of the three paths matches a keyword.
- **Impact**: The same semantic state ("no env mapping authored") is canonical `env 0.0 / rough 0.85` on FO3/FNV and `env 1.0 / rough 0.8` on Skyrim. Nothing changes on screen today: the kind-101 raster branch returns at `crates/renderer/shaders/triangle.frag:1108`, RT hits add emission and break, and shadow rays skip effect cards. The wrong values are visible in `mat.dump` / `material_dump` tooling and in the completeness-harness fill rates. Any future consumer of `Material.env_map_scale` / `roughness` on effect surfaces would get them too.
- **Corroboration (merged from Dim 9, formerly NIFAL-D9-2026-09-14-01)**:
  - The cross-game completeness harness moved Skyrim SE `metO`/`rghO` from **93.8% to 99.0%** this window. 93.8% was also the 2026-08-23 and 2026-09-11 figure. Every other game stayed identical on every column.
  - A per-mesh probe over 198 identical LE/SE paths counted **36 meshes per install (72 total)** with no texture, normal, gloss or authored specular, yet `metalness_override = Some(0.0)` / `roughness_override = Some(0.8)`. Every one is `material_kind = 101` with `env = 1`. Under #2707's `has_no_pbr_classifier_signal` (`crates/nif/src/import/material/mod.rs:1455-1461`), each of these had no signal before #4250. This is the #2707 / #2352 fabrication class re-entered through a different input.
  - The harness asserts only lower bounds (`crates/nif/tests/translation_completeness.rs:220-231`), so the fabricated fill passed silently and reads as improved coverage.
- **Related**: #4250, #4251, #2707, #2352, #2315/#2555 (established 0.0 = unauthored), #4043.
- **Suggested Fix**: Give the harness a drift *band* for `metO`/`rghO` rather than a floor, so an upward jump fails as loudly as a drop. Separately, revert the placeholder to `0.0`. Alternatively, make the parsed field `Option<f32>` and write `MaterialInfo.env_map_scale` only when BSVER ≥ 130. Keep the #4251 latch on the authored path only. Replace the #4250 test with one asserting that a Skyrim effect shader stays on the default-matte arm. If `BsEffectShaderData::default()`'s `1.0` was the source of confusion, document it as the FO4 on-disk default, not an absence value.

#### NIFAL-D2-2026-09-14-01: Starfield BSGeometry local bound is in unscaled Havok units while its vertices are Havok-scaled — every Starfield mesh's `LocalBound` is ~70× too small
- **Severity**: HIGH (rendering correctness: frustum culling and render-layer depth bias on every Starfield mesh)
- **Dimension**: Geometry/Transform
- **Tier Violated**: single-boundary (the Havok-unit conversion for the category is applied to `positions` but not to the sibling authored sphere; `local_bound_*` leaves the boundary in a different unit system)
- **Game Affected**: Starfield (all BSGeometry)
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:374-413` (sphere used verbatim, `([cx, cy, cz], r)`), `crates/nif/src/blocks/bs_geometry.rs:421-431` (vertices multiplied by `HAVOK_SCALE`)
- **Status**: NEW (the same hypothesis as closed #2098, which was closed 2026-08-06 with only a `log::debug!` diagnostic; the real-data check it asked for was never run, and behaviour never changed, so this is not a regression)
- **Description**: `BSGeometryMeshData::parse` decodes positions as `unpack_norm_i16(v, scale, HAVOK_SCALE)`. The NIF-level `BSGeometry.bounding_sphere` is read raw and never scaled. `extract_bs_geometry` uses it verbatim whenever `r > 0`, under a comment that argues only its *basis* ("already in Y-up"), never its *units*. The #2098 guard `bs_geometry_bounding_sphere_mismatch` detects exactly this condition but only logs at debug level.
- **Evidence**: The orchestrator re-read both sites and confirmed that no scale is applied to the sphere. The agent's corpus census over 40,000 vanilla Starfield shapes found:
  - The verbatim sphere encloses the vertices on **0 / 40,000** shapes.
  - Sphere × 69.969 (centre and radius) encloses them on **39,939 / 40,000**.
  - Samples: `stsoccintsegsmwallmidbot_scktd01.nif` has r = 4.3397 and vertex extent 303.643 = r × K; `shpgenintpersmwallforemid02.nif` has r = 2.4225 and extent 169.499.
- **Impact**:
  - **Frustum culling**: `byroredux/src/render/static_meshes.rs:394` culls Starfield architecture while it is still largely on screen, so walls pop at frustum edges. They still shadow through the TLAS while invisible.
  - **Depth bias**: `escalate_small_static_to_clutter` (50-unit threshold) demotes nearly all Starfield architecture to Clutter depth bias. This is very likely the real mechanism behind the "#1294 trap" note.
  - **Cell bounds**: the cell foreground AABB (`byroredux/src/cell_loader/spawn.rs:205-222`) under-covers Starfield placements.
  - **Log noise**: the #2098 debug log fires on effectively every Starfield mesh.
- **Related**: #2098 (closed, log-only), #1294, `crates/nif/src/import/mesh/bs_geometry_bounding_sphere_tests.rs` (pins only the log helper).
- **Suggested Fix**: Scale the authored sphere's centre and radius by the Havok factor at the single extraction site; expose the constant or add a game-units accessor next to the parser. Fix the comment to state the unit contract. Promote the mismatch helper into a unit test. Add a data-gated corpus test mirroring `switchboard_precombine_transforms_match_authored_bounds`.

#### NIFAL-D3-2026-09-14-01: NIF-embedded spot and directional lights get an uncited "-Z" direction that contradicts Gamebryo's (1,0,0) model direction, the sibling ESM light boundary, and the canonical per-kind sign convention
- **Severity**: HIGH (rendering correctness — cone/sun direction wrong on every embedded spot/directional light)
- **Dimension**: Skinning/Lights
- **Tier Violated**: no-fabrication, plus a single-boundary divergence (the NIF and ESM light boundaries disagree on one Gamebryo convention)
- **Game Affected**: every game whose NIFs embed `NiSpotLight` / `NiDirectionalLight` (Oblivion exporter `NiDirectionalLight`s, #3557, are a known population; total population not measured)
- **Location**: `crates/nif/src/import/walk/lights.rs:131-137` (`imported_light_from_base`); consumed unchanged by `byroredux/src/cell_loader/spawn.rs:1108-1125` and `byroredux/src/render/lights.rs:77-88`
- **Status**: NEW
- **Description**: `imported_light_from_base` takes every NIF light's direction as the negated *third* column of the world rotation. Its only justification is the uncited comment "Gamebryo lights point down the local -Z axis". This has two problems:
  1. **Wrong axis.** The ESM boundary `translate_light` (`byroredux/src/systems/light_anim.rs:228-239`) and `docs/engine/nifal.md` §2 Lights both explicitly reject local −Z in favour of the first column. The NIF-import boundary was never brought into line.
  2. **One vector for two opposite conventions.** The canonical `Emitter.direction` means "toward the light" for directional emitters and "outward from the source" for spots (`crates/core/src/lighting.rs:217-219`), and the shader implements both. The importer emits the same vector for both kinds, and nothing downstream negates per kind, so at least one kind has the wrong sign whichever axis is right.
- **Evidence**: The orchestrator re-read both the NIF site and the ESM site and read the headers directly:
  - `/mnt/data/src/reference/gamebryo-v32/Include/NiDirectionalLight.h:30` and `/mnt/data/src/reference/gamebryo-v32/Include/NiSpotLight.h:31-32` both say "The model direction of the light is (1,0,0). The world direction is the first column of the world rotation matrix."
  - The parser's `rows` are row-major (`crates/nif/src/stream.rs`), so the first column is `[rows[0][0], rows[1][0], rows[2][0]]`, not `-[rows[i][2]]`.
  - The −Z comment dates to the original light parse (`14e9a06b0`, #156). No test pins NIF light direction.
- **Impact**: Embedded spot cones aim along the wrong local axis. Embedded directional lights light and RT-shadow surfaces from the wrong direction. The direction is resolved once and trusted by the shader, exactly as the tier model requires, so nothing masks it.
- **Related**: #3232 (closed — added `ref_rot` rotation, kept the imported axis), #2205, #2439, #3557.
- **Suggested Fix**: Take column 0 as the emission direction, then apply the Z-up→Y-up conversion. Negate it for `LightKind::Directional` so it points toward the light per the `Emitter` contract. Decide the sign once, at this boundary. Add a fixture test for each kind with a non-identity rotation. Optionally census the embedded population (`crates/nif/examples/import_probe.rs`).

#### NIFAL-D7-2026-09-14-01: #4166's "components square to inf → zero quaternion" hole is still open on every non-B-spline rotation path (mainline KF keys, static poses, HKX)
- **Severity**: HIGH. The dimension agent proposed MEDIUM on the grounds that vanilla never triggers it: 0 of 16.06M vanilla rotation keys across FNV, Oblivion and Shivering Isles. The orchestrator raised it to HIGH for consistency with #4166, the same arithmetic on a sibling path, filed HIGH. The TBC arm produces a NaN bone transform, which reaches BLAS refit and TLAS build. A grep of `crates/renderer/src/vulkan/acceleration/`, `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`, `byroredux/src/render/skinned.rs`, `byroredux/src/render/static_meshes.rs` and `byroredux/src/systems/animation.rs` finds no `is_finite` guard on transforms. Severity is impact, not likelihood.
- **Dimension**: Animation
- **Tier Violated**: no-fabrication
- **Game Affected**: all NIF titles (KF and embedded `NiTransformData`); Skyrim LE/SE via `convert_hkx_clip`
- **Location**:
  - `crates/nif/src/anim/keys.rs:57-78` (`convert_quat_keys`)
  - `crates/core/src/math/coord.rs:200-207` (`normalize_quat`)
  - `crates/nif/src/anim/transform.rs:145-157` (`constant_transform_channel`)
  - `crates/nif/src/anim/bspline.rs:439-448` (B-spline static pose)
  - `crates/hkx/src/animation.rs:1045-1058` (`normalize_quaternion`)
  - Consumers: `crates/core/src/animation/interpolation.rs:314`, `byroredux/src/systems/animation.rs:721-722`
- **Status**: NEW (sibling of closed #4166; #1443 guarded only non-finite components)
- **Description**: #4166's fix guards `len_sq.is_finite()` only inside `normalized_rotation_sample` on the B-spline path. Everywhere else a rotation key is normalized, a component that is individually sane (e.g. `2e19`) squares to `inf`, the inverse becomes `0`, and the result is the zero quaternion. An authored all-zero key also passes, because `normalize_quat` returns zero-length input unchanged. HKX `normalize_quaternion` returns `Ok([0,0,0,0])` for the same input. Neither canonical boundary re-checks unit length.
- **Evidence**: Measured with glam 0.29.3 in scratch programs:
  - **Const, single-key, or `i0 == i1`**: `Mat4::from_quat(zero)` is identity, so the bone gets a silent identity pose.
  - **Linear slerp toward zero**: length 0.707, which decomposes to a sheared scale (0.939, 1, 0.939).
  - **TBC with a zero start key**: `(q0 * identity).normalize()` at `crates/core/src/animation/interpolation.rs:314` is **NaN**, written straight into `Transform.rotation`.
- **Impact**: Malformed or crafted KF/NIF/HKX content only. On such content the result is a silent identity pose or visible shear, and on TBC channels a NaN bone transform that becomes undefined-behaviour AS input — the #3765/#4166 failure chain.
- **Related**: #4166, #3765, #1443, #3316, NIFAL-D7-2026-09-14-02, NIFAL-D7-2026-09-14-03.
- **Suggested Fix**: Promote `normalized_rotation_sample` into `crates/nif/src/anim/keys.rs` as the single rotation-key sanitizer. It should return `None` for non-finite *or* ≤ EPSILON `len_sq`. Use it at `convert_quat_keys`, `constant_transform_channel` and both B-spline static branches. In hkx, add `!length_squared.is_finite()` to `normalize_quaternion`'s error arm. Add `[2e19,0,0,0]` and `[0,0,0,0]` sanitize tests.

#### NIFAL-D7-2026-09-14-02: static-pose fallbacks gate only on `is_flt_max`, which is false for NaN — a NaN pose reaches the canonical clip despite `crates/nif/src/anim/keys.rs` documenting those paths as guarded
- **Severity**: HIGH. The dimension agent proposed MEDIUM because the vanilla scan found 0 non-finite T/R/S keys. The orchestrator raised it to HIGH on the same basis as D7-01: NaN reaches a `Transform` and the AS build with no downstream finiteness guard.
- **Dimension**: Animation
- **Tier Violated**: no-fabrication
- **Game Affected**: all NIF titles (static poses on `NiTransformInterpolator` / `NiLookAtInterpolator` / `NiBSplineComp*Interpolator`)
- **Location**:
  - `crates/nif/src/anim/transform.rs:134`, `:145-148`, `:158`
  - `crates/nif/src/anim/bspline.rs:181`, `:417`, `:441`, `:464`, `:496`, `:509`, `:518`
  - `crates/nif/src/anim/channel.rs:267`
  - Stale guarantee: `crates/nif/src/anim/keys.rs:14-23`
- **Status**: NEW (#1443 fixed the keyframe converters and explicitly relied on these paths being gated)
- **Description**: `is_flt_max(v)` is `v.abs() >= 3.0e38`, which is false for NaN. The eleven static-pose gates use only it, so a NaN in an authored `NiQuatTransform` or `interp.value` passes every one:
  - Translation goes through the pure swizzle and stays NaN.
  - Rotation normalizes to all-NaN.
  - Scale is copied verbatim.

  `read_ni_quat_transform` does no finiteness check. `constant_transform_channel` is the fallback for 39.7% of FNV transform controlled blocks (#3316), so this is the broadest static-pose path.
- **Evidence**: Measured: `is_flt_max(NaN) == false`, and `zup_to_yup_pos([NaN,…])` gives `[NaN,2,-1]`. `convert_nif_clip` copies keys with no sanitization (`byroredux/src/anim_convert.rs:431-463`).
- **Impact**: On corrupt content, a NaN `Transform` on the bone or node propagates to `GlobalTransform`, skinning and the TLAS.
- **Related**: #1443, #3765, #4166, #3316, NIFAL-D7-2026-09-14-01.
- **Suggested Fix**: Replace `is_flt_max(x)` with `!is_key_value_sane(x)` at the eleven gates; it already includes the FLT_MAX sentinel, so the "axis inactive" semantics are kept. Correct the `crates/nif/src/anim/keys.rs:15-16` doc. Add NaN-pose sanitize tests.

### MEDIUM

#### NIFAL-D5-2026-09-14-01: Emitter orientation is dropped, so the authored spawn cone is world-axis aligned and #4240's azimuth wedge points the wrong way on any rotated placement
- **Severity**: MEDIUM (NIFAL row: translatable particle emitter data silently dropped; no content removed)
- **Dimension**: Particles
- **Tier Violated**: parked-not-leak (authored rotation is parsed, then silently dropped; not recorded as a deferral)
- **Game Affected**: Oblivion, FO3, FNV, Skyrim (Starfield N/A). #4240 measured wedge-authoring emitters at FO3 250/422 and FNV 405/1262.
- **Location**:
  - `crates/nif/src/import/walk/emitter.rs:754-757` (flat walker keeps only `.translation`)
  - `crates/nif/src/import/types.rs:1857-1859` (`ImportedParticleEmitterFlat` has no rotation field)
  - `byroredux/src/cell_loader/spawn.rs:1274-1275` (`GlobalTransform::new(world_pos, Quat::IDENTITY, 1.0)`; the sibling fog branch at `:1225` does receive `ref_rot`)
  - `byroredux/src/systems/particle.rs:410-413` and `:488-511` (reads only `g.translation`; the cone is built around world +Y and world +X)
  - `crates/nif/src/blocks/particle.rs:139` (`NiPSysVolumeEmitter.Emitter Object` ref discarded)
- **Status**: NEW (the rotation half of #1333, which fixed translation only; the #4240 commit message acknowledges the gap, and no issue tracks it — confirmed by `gh issue list --search` on "emitter rotation" / "emitter orientation")
- **Description**: Gamebryo's emitter direction is expressed in the emitter's own frame. No orientation reaches the canonical `ParticleEmitter`:
  - The flat import drops the composed NIF rotation.
  - The cell spawn drops the REFR rotation.
  - `particle_system` ignores any rotation on the entity's `GlobalTransform`, so even the loose-NIF path's carefully set `local_rotation` is unused.

  Before #4240 the azimuth was a uniform random draw, so yaw was invisible. With the authored wedge now forwarded, every authored fan aims relative to world +X regardless of how the placement is yawed.
- **Evidence**: See Location. `authored_planar_angle_aims_the_spawn_azimuth` uses an identity host rotation, so nothing pins rotated hosts. The number of placed wedge emitters on non-identity REFR yaw was not measured.
- **Impact**: Directional FX (sparks, steam vents, spray/impact fans, directional dust) spawn toward a fixed world direction instead of the placed object's facing. The error varies per placement, so in-world it looks random.
- **Related**: #1333, #4240, #984 (force-field directions share the gap).
- **Suggested Fix**: Carry the composed NIF rotation on `ImportedParticleEmitterFlat`. Insert `ref_rot × nif_rot` on the billboard emitter transform at `byroredux/src/cell_loader/spawn.rs:1274-1275`. In `particle_system`, rotate the sampled offset and `dir` by `g.rotation`. Pin with a rotated-host variant of the azimuth test. Separately verify whether force-field directions are emitter-local before rotating them.

#### NIFAL-D3-2026-09-14-02: GPU morph-target deformation (#3231) creates its MorphSlot only on entities that can never have `bone_offset != 0`, so it is unreachable end-to-end
- **Severity**: MEDIUM (feature inert; per-entity GPU weight/delta buffers allocated and refreshed for nothing; no crash)
- **Dimension**: Skinning/Lights
- **Tier Violated**: no-leak (the cell loader uses raw-tier `ImportedMesh.skin.is_some()` as a stand-in for "has a canonical `SkinnedMesh`", which nifal.md documents as never true on that path)
- **Game Affected**: all games with morph-target content on skinned shapes
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:1001-1024` (creation), `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:382-384` (read gate)
- **Status**: NEW
- **Description**:
  - **Where slots are created**: the only production `create_morph_slot_for_mesh` call is in `spawn_mesh_instance` (cell loader only), gated on `mesh.skin.is_some()`.
  - **Where slots are read**: only when `bone_offset != 0`, which requires a `SkinnedMesh`. Per #2440, the only production `SkinnedMesh::new_with_global` is `byroredux/src/scene/nif_loader.rs`.
  - **The other path**: the loose-NIF / NPC path produces `bone_offset != 0` but never creates a MorphSlot.

  The two conditions therefore never meet on any entity.
- **Evidence**: Grep results cited above. #3231's verification was a no-regression live boot, not proof of deformation. The only guard, `morph_spawn_uses_mesh_handle_shared_delta_cache`, pins that the call exists, not that it is reachable.
- **Impact**: `AnimatedMorphWeights` are staged into slots no draw consumes, so authored morph animation never deforms anything. The #4294 LRU-recreation and #3661 residency work maintain buffers that are dead on arrival.
- **Related**: #3231, #2440, #4294, #3661, #2221.
- **Suggested Fix**: Gate slot creation on the canonical signal (the entity will carry a `SkinnedMesh`) and add the equivalent creation on the loose-NIF / NPC path where `SkinnedMesh` is built. Add a reachability test. If loose-NIF wiring is out of scope, stop creating cell-path slots and record the gap beside #2440 in nifal.md.
### LOW

#### NIFAL-D1-2026-09-14-03: MSWP swap re-merge (#4290) leaves the BGEM glass-overlay texture roles on the source sidecar while provenance labels follow the target
- **Severity**: LOW (latent: only BGEM v21+ authors these roles; FO4 ships none)
- **Dimension**: Material
- **Tier Violated**: single-boundary
- **Game Affected**: FO76/Starfield-era BGEM through a REFR material swap; latent on FO4
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:189-192` (`textures` seeded from `mesh.material`), `:193-202` (`sources` seeded from the swapped `material`)
- **Status**: NEW (introduced by `82c4450d6`)
- **Description**: #4290 moved every re-resolved role and scalar read onto the swapped material, but the `textures` seed still reads the pre-swap cached material. `glass_roughness_scratch` and `glass_dirt_overlay` never pass through `resolve_effective`, so they keep the source sidecar's maps while `MaterialTextureDebugInfo.sources` reports the target's provenance.
- **Evidence**: The orchestrator re-read `byroredux/src/cell_loader/spawn/mesh_instance.rs:187-202` and confirmed `mesh.material.textures.map_ref` next to `material.textures.zip_map_ref`. The #4290 test asserts only scalars and flags.
- **Impact**: Wrong glass scratch/dirt overlay and mislabelled `mat.dump` provenance on swapped BGEM glass. No current FO4 population.
- **Related**: #4290, #973, #3906.
- **Suggested Fix**: Seed `textures` from `material.textures` at `byroredux/src/cell_loader/spawn/mesh_instance.rs:189`. Extend the #4290 test with a BGEM pair that differ in `glass_roughness_scratch`.

#### NIFAL-D1-2026-09-14-04: `byroredux/src/cell_loader/object_lod.rs` is now a fourth handle-less `translate_material` caller; module doc, nifal.md §3 and the skill still name `byroredux/src/cell_loader/placement_lod.rs` as the only one
- **Severity**: LOW (doc rot on the two-phase boundary record)
- **Dimension**: Material
- **Tier Violated**: N/A (doc)
- **Game Affected**: all (`.bto` object LOD: Skyrim/FO4)
- **Location**: `byroredux/src/material_translate.rs:44-54`, `docs/engine/nifal.md:615-626`, `.claude/commands/audit-nifal/SKILL.md`; caller at `byroredux/src/cell_loader/object_lod.rs:318-350`
- **Status**: Existing: #4246 (scope extension — fold into that fix)
- **Description**: `bdc5ca6cc` routes `.bto` sub-meshes through `translate_material` without attaching `MaterialTextureHandles`, so like `byroredux/src/cell_loader/placement_lod.rs` it runs none of the three Phase-2 resolvers. The docs still describe a single exempt caller. The skill's "3 production callers" line is stale (there are now 4 plus the Cornell harness).
- **Evidence**: `grep translate_material(` finds `byroredux/src/scene/nif_loader.rs`, `byroredux/src/cell_loader/spawn/mesh_instance.rs`, `byroredux/src/cell_loader/placement_lod.rs`, `byroredux/src/cell_loader/object_lod.rs` and `byroredux/src/cornell.rs` (test).
- **Impact**: None at runtime; an author wiring handles would miss the object-LOD site.
- **Related**: #4246, #3465, #4228.
- **Suggested Fix**: In the #4246 fix, restate the exemption as "sites that attach no `MaterialTextureHandles` (`byroredux/src/cell_loader/placement_lod.rs`, `byroredux/src/cell_loader/object_lod.rs`)" and update the skill's caller list.

#### NIFAL-D8-2026-09-14-02: #4235 moves base-texture precedence to the shader but leaves the paired clamp mode and the parallax slot first-writer-wins
- **Severity**: LOW (vanilla renders unchanged; mod content exposed)
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: single-boundary (per-role precedence split across two independent latches in one walker)
- **Game Affected**: FO3 / FNV
- **Location**: `crates/nif/src/import/material/legacy_properties.rs:58-70` (`claim_shader_texture`), `:432-437` (clamp latch), `:518` (parallax slot `is_none()`-gated); consumer `byroredux/src/cell_loader/spawn/mesh_instance.rs:933-938`
- **Status**: NEW (introduced by `8dfa78eee`, the fix for closed #4235)
- **Description**: After #4235 the base path always comes from `BSShaderTextureSet`, but `texture_clamp_mode` is still latched by whichever property ran first. When `NiTexturingProperty` is listed first, the shader's texture is sampled with the legacy property's address mode. The parallax/height role has the same split, and there is no in-code deferral comment for either.
- **Evidence**: `claim_shader_texture` clears only the `texturing_property_roles` bits. The clamp write at `:584` stays behind `!texture_clamp_mode_consumed`. The new precedence test asserts paths only.
- **Impact**: On mod content with differing paths: wrong edge wrap/clamp on the shader's texture, and normal/height pairs from different sources. The 5+5 co-bound vanilla shapes name identical paths.
- **Related**: #4235, #3517, #2328, #208.
- **Suggested Fix**: When `claim_shader_texture` displaces a base path, let that block re-latch `texture_clamp_mode`. Apply the same rule to parallax, or add a `#4235` deferral comment at `:518`. Extend the precedence test to assert clamp mode.

#### NIFAL-D8-2026-09-14-03: #4286 inverted #2108's "a BGSM that wins the greyscale slot is authoritative, including OFF" rule, but the contract comment and test doc still state it
- **Severity**: LOW (tiny FO4 population: 11 of 30,166 lit properties have slot 3 empty)
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: no-fabrication (precedence policy changed with no source for which side the engine honours)
- **Game Affected**: FO4 (FO76/Starfield via the CRC-array half)
- **Location**: `byroredux/src/asset_provider/material/merge.rs:656-657` (contract bullet), `:668-684` (code now ORs), `byroredux/src/asset_provider/tests/bgsm_merge.rs:2187-2219` (doc)
- **Status**: NEW (introduced by `d28722fbf`, the fix for closed #4286)
- **Description**: All three merge branches now leave a NIF-set palette bit on, so a BGSM that wins the slot and authors the remap OFF no longer turns it off. The bullet directly above the code still says "(assignment, unchanged)". #4286's justification cited Skyrim-layout BGSM, which barely applies (BGSM is FO4+). The reachable FO4 case was not analysed. Both halves of the #3897/#3898 two-gate invariant still hold; nothing is dropped.
- **Evidence**: `byroredux/src/asset_provider/material/merge.rs:683` `bgsm_greyscale_lut_enabled |= …` sits under the bullet that says "assignment". The old `bgsm_winning_the_slot_still_authors_the_enable_bit_off` test still passes only because its NIF bit is false.
- **Impact**: FO4 content whose BGSM deliberately disables the remap while the NIF enables it renders the palette branch anyway. The larger cost is a self-contradicting precedence contract in the file that owns it.
- **Related**: #2108, #3897, #3898, #4286.
- **Suggested Fix**: Decide the rule from a source (does an FO4 named material file replace the NIF's SLSF1 bits?). Then either restore assignment in the `is_none()` branch or keep OR, and update the `:656-657` bullet and the `byroredux/src/asset_provider/tests/bgsm_merge.rs:2187` doc to match.

#### NIFAL-D8-2026-09-14-04: Engine docs still describe the pre-#3901 flipbook contract (`texture_slot: u32`, renderer bind "deferred")
- **Severity**: LOW (doc)
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: — (doc describes the removed raw-slot leak as the live shape)
- **Game Affected**: all (Oblivion/FO3/FNV flipbook content)
- **Location**: `docs/engine/animation.md:121-125`, `docs/engine/nif-parser.md:904-906`
- **Status**: NEW (same class as OPEN #4360, which does not list these lines — fold in)
- **Description**: The core `TextureFlipChannel` now carries `role: FlipTextureRole` (`crates/core/src/animation/types.rs:218`). `docs/engine/animation.md` still shows `texture_slot: u32 // raw TexType enum`. `docs/engine/nif-parser.md` still calls the renderer bind deferred, which has been false since #2221 (base role) and #3901 (all roles).
- **Evidence**: grep hits at the cited lines; `docs/engine/animation.md` was last touched before #3901.
- **Impact**: A reader following the doc would reintroduce a raw slot on the canonical channel — exactly the leak #3901 closed.
- **Related**: #3901, #4360, #2221.
- **Suggested Fix**: Update both docs to the shipped `FlipTextureRole` contract; fold into #4360.

#### NIFAL-D5-2026-09-14-02: The #4167 Particles completeness guards have holes — the structural scan matches comments and substrings, and the value test's `src_blend` equals the preset's
- **Severity**: LOW (test harness; no runtime impact today)
- **Dimension**: Particles
- **Tier Violated**: single-boundary (the boundary's completeness guard can pass while an overlay is dropped)
- **Game Affected**: all
- **Location**: `byroredux/src/systems/particle.rs:806-847` (structural guard, `body.contains(name)`), `:739` (`Some(6)` passed as `src_blend`), `:773` (assert); `crates/core/src/ecs/components/particle.rs:435` (`torch_flame().src_blend: 6`)
- **Status**: NEW (related #4167, closed by `f680df2e0`)
- **Description**: The structural guard checks raw body text with `contains`, comments included and with no identifier boundary. Most parameter names also appear as substrings of preset fields (`effect_shader` ⊂ `effect_shader_flags`, `greyscale_lut` ⊂ `greyscale_lut_index`), of helper names, or in comments. Separately, the value test passes `src_blend = Some(6)`, identical to the preset, so deleting only the `src_blend` overlay passes both guards.
- **Evidence**: The orchestrator confirmed `byroredux/src/systems/particle.rs:739` passes `Some(6)` inside `every_overlay_parameter_reaches_the_preset` and that `torch_flame()` has `src_blend: 6`. The agent replayed the guard's scan on four mutated copies of the file; the guard passes on all four, including (D) a new parameter mentioned only in a body comment — the exact case the guard exists to catch.
- **Impact**: False confidence; `src_blend` has no working pin.
- **Related**: #4167, #2300, #1513, NIFAL-D7-2026-09-14-04 (same vacuity class).
- **Suggested Fix**: Strip `//` comments before scanning and match whole identifiers not preceded by `.`. Change the fixture's `src_blend` to a value `torch_flame()` doesn't use, and `assert_ne!` every pinned field against `before`.

#### NIFAL-D7-2026-09-14-04: the #4167 animation completeness harnesses have value choices that let specific field drops pass
- **Severity**: LOW (test coverage gap; both boundaries currently correct)
- **Dimension**: Animation
- **Tier Violated**: harness-coverage gap
- **Game Affected**: all
- **Location**: `byroredux/src/asset_provider/animation.rs:425-430` and `:498-499`; `byroredux/src/anim_convert.rs:1070-1095`, `:1079`, `:1113`, `:1272-1282`
- **Status**: NEW
- **Description**: Every field of core `AnimationClip` and its channel/key types is set, so the harnesses are not vacuous overall, but four value choices are not distinctive:
  1. The HKX scale fixture `[1,2,3]` averages to `2 == scale[1]`, so dropping the average passes.
  2. `translation_type: Linear` is the value a hard-coded converter would produce.
  3. `FloatTarget::Alpha` is the first variant, so a hard-coded target passes.
  4. Every channel has one key and only collection counts are asserted, so a first-key-only copy passes.
- **Evidence**: Arithmetic and assertions at the cited lines.
- **Impact**: A regression in exactly the transforms the harnesses were written to guard would ship green.
- **Related**: #4167, #3462, NIFAL-D5-2026-09-14-02.
- **Suggested Fix**: HKX scale `[1,2,6]`; non-Linear key types; a non-first `FloatTarget`; two keys per channel with `keys.len()` asserted.

#### NIFAL-D7-2026-09-14-03: #4166's `normalized_rotation_sample` substitutes an identity key for the overflow case instead of skipping it, contradicting its own doc and its three siblings
- **Severity**: LOW (malformed content only; finite output)
- **Dimension**: Animation
- **Tier Violated**: no-fabrication
- **Game Affected**: FO3/FNV, Skyrim+ (`NiBSplineCompTransformInterpolator`)
- **Location**: `crates/nif/src/anim/bspline.rs:259-264` (doc), `:297-318` (fn), `:430-438` (push); pinned by `crates/nif/src/anim/tests/bspline.rs:191-197`
- **Status**: NEW (introduced by `1cedb6f8e`)
- **Description**: The doc says a bad sample is skipped "so the bone falls back to its bind pose". Only the non-finite input returns `None`; the `len_sq == inf` overflow case is routed into the identity arm and pushed as a real `RotationKey`. A local identity rotation is an invented pose, not the bind pose. The translation, scale and float siblings all skip the sample.
- **Evidence**: The test `bspline_rotation_sample_substitutes_identity_when_squaring_overflows` pins `[1,0,0,0]`.
- **Impact**: On malformed content the bone snaps to identity for the affected span.
- **Related**: #4166, NIFAL-D7-2026-09-14-01 (the shared sanitizer proposed there should return `None` here too).
- **Suggested Fix**: Return `None` when `!len_sq.is_finite()` and update the two pinning tests, or rewrite the doc if identity is deliberate.

#### NIFAL-D6-2026-09-14-01: `BhkPlaneShape → None` is justified by a trimesh fallback that never fires for its only vanilla instance
- **Severity**: LOW (one small underwater egg-cluster file; the deliberate `None` stays sound — only the documented safety net is false). The orchestrator considered the MEDIUM "silently dropped collision shape" row and rejected it: the drop is deliberate and documented at its arm, not silent.
- **Dimension**: Collision
- **Tier Violated**: parked-not-leak (the parked `None` is described as covered downstream, but nothing covers it)
- **Game Affected**: Skyrim SE
- **Location**: `crates/nif/src/import/collision/shape.rs:93-104` (claim), `byroredux/src/cell_loader/spawn/mesh_instance.rs:1318-1325` (gate that rejects the fallback)
- **Status**: NEW (related closed #1334, #4163)
- **Description**: The arm's comment says the dropped plane falls back to "the synthesized-trimesh fallback (spawn.rs) — its render-mesh surface". The one vanilla file, `slaughterfisheggcluster01_1.nif` (`Skyrim - Meshes1.bsa`), has the plane as its only collision and one `BSTriShape` with `NiAlphaProperty` flags `0x12EC`, so `alpha_test = true`. The trimesh fallback requires `!source_material.alpha_test`, so the placement gets no collider. #4163's `plane_shapes` counter has no production reader: it is not in `collision_authoring_totals` or `SpawnCensusAuthoring`, so the drop is invisible at runtime.
- **Evidence**: The orchestrator re-read `crates/nif/src/import/collision/shape.rs:93-104` and `byroredux/src/cell_loader/spawn/mesh_instance.rs:1318-1325`. The agent's `trace_block` probe of the file shows the block list above.
- **Impact**: One cosmetic physics-only collider missing; the documented-limitation rule rests on an unmeasured claim.
- **Related**: #1334, #4163, #2355.
- **Suggested Fix**: Correct the comment, `.claude/commands/audit-nifal/SKILL.md` and nifal.md wording to "no collider is produced for this instance". Optionally fold `plane_shapes` into `collision_authoring_totals` / `SpawnCensusAuthoring`.

#### NIFAL-D6-2026-09-14-02: Stale comment says physics ignores `GlobalTransform::scale` and the fallback bakes `final_scale` into trimesh verts — both false since #3064/#2860
- **Severity**: LOW (doc; code correct)
- **Dimension**: Collision
- **Tier Violated**: no-fabrication (comment states the opposite of the live "scale applied once" contract)
- **Game Affected**: all games reaching the Architecture trimesh fallback (mainly FO4/FO76/Starfield)
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:1285-1289`
- **Status**: NEW (a sibling copy that #3064's and #3961's comment sweeps missed; carried in by refactor `a0a52bc3f`)
- **Description**: The comment says the physics sync ignores scale and that this site bakes `final_scale` into verts. In fact `spawn_trimesh_collider_ghost` puts scale on `Transform`/`GlobalTransform`, and `crates/physics/src/convert.rs` applies it exactly once.
- **Evidence**: The orchestrator confirmed the comment text at `:1285-1289`. `git log -S "ignores scale — bhk shapes bake"` traces it to `15016ee02`, which predates #3064.
- **Impact**: This is the third stale copy of the wrong contract that already caused two scale² bugs (#3064, #3959). Following it would reintroduce `XSCL²` Architecture colliders.
- **Related**: #3064, #3959, #3961, #2860.
- **Suggested Fix**: Replace the lines with a pointer to the live contract in `docs/engine/physal.md`.

#### NIFAL-D5-2026-09-14-03: `docs/engine/nifal.md` §2 Particles no longer describes what the boundary applies or defers after #3754/#4240/#4261
- **Severity**: LOW (doc; the spec is the authority auditors check "parked" against)
- **Dimension**: Particles
- **Tier Violated**: parked-not-leak
- **Game Affected**: all
- **Location**: `docs/engine/nifal.md:301-303`, `:311-317`, `:335-338`
- **Status**: NEW (precedent #2488)
- **Description**:
  - (a) The applied-field list omits `planar_angle` / `planar_angle_variation` (#4240) and the ×2 half-spread→full-width variation convention.
  - (b) The rate is still described as "`NiFloatData` first key"; the #3754 curve-mean, #2548 blend and #3329 sequence tiers are missing.
  - (c) Per-emitter attribution is still called wholly pending, although #4261 made params, colour, budget and the modern rate tier per-instance; only the legacy and #3329 sequence tiers remain whole-scene.
  - (d) The tooling line omits planar columns.
- **Evidence**: Line citations against `9e372f452` / `b3237e65a`; neither commit touched nifal.md.
- **Impact**: A later audit will misclassify the sequence-tier residual and miss the variation-convention change that affects fog-volume sizing (`byroredux/src/fog.rs:303`).
- **Related**: #2488, #3754, #4240, #4261, #3329, NIFAL-D3-2026-09-14-03.
- **Suggested Fix**: Update §2 Particles to match (a)–(d), and mirror the change in the skill's Dimension 5 text.

#### NIFAL-D3-2026-09-14-03: nifal.md "Skinning" prose is stale — #3930 still described as an open proposal, and the cell loader said to read `mesh.skin` "exactly once"
- **Severity**: LOW (doc)
- **Dimension**: Skinning/Lights
- **Tier Violated**: parked-not-leak
- **Game Affected**: Starfield (point 1); all (point 2)
- **Location**: `docs/engine/nifal.md:190-194`, `docs/engine/nifal.md:153-157`
- **Status**: NEW (drift since #3958)
- **Description**:
  1. #3930 is closed and implemented (`SkinAttach` primary), and #4270 now skips the #3549 geometric solve when `SkinAttach` covers every bone. The spec still calls #3930 an open proposal.
  2. The cell loader reads `mesh.skin` four times, two of them positive consumers (proxy bounding sphere at `byroredux/src/cell_loader/spawn.rs:194`; MorphSlot creation at `byroredux/src/cell_loader/spawn/mesh_instance.rs:1010`). The spec says "exactly once, as a negative filter".
- **Evidence**: See Location; `gh issue view 3930` shows CLOSED.
- **Impact**: A future audit would re-propose #3930, and would miss that the cell path already acts on `mesh.skin` positively — which is how NIFAL-D3-2026-09-14-02 went unnoticed.
- **Related**: #3930, #4270, #3958, #2440, NIFAL-D3-2026-09-14-02.
- **Suggested Fix**: Rewrite both passages to the live state.

#### NIFAL-D9-2026-09-14-02: `every_source_derived_material_field_is_pinned_by_a_test` counts comment prose as a pin — its own rationale comment self-pins `alpha` and `alpha_threshold` (shared text-scan weakness in the Lights/Collision resolve scans)
- **Severity**: LOW (test guard; no live masked regression today)
- **Dimension**: Completeness
- **Tier Violated**: harness-coverage gap
- **Game Affected**: all
- **Location**: `byroredux/src/material_translate.rs:2773-2783` (the `pinned` closure; its own comment at `:2776-2777`). Latent siblings: `crates/nif/src/import/walk/lights.rs:276-288` and `crates/nif/src/import/collision/mod.rs:669-685`, whose resolve-side scans cover whole source files, test modules and comments included.
- **Status**: NEW
- **Description**: The guard treats a field as pinned when any `;`-delimited chunk of the test half contains `material.<field>` at a word boundary plus the substring `"assert"` or `".expect("`. Chunks are not comment-stripped, and `"assert"` also matches English prose. The guard's own rationale comment — "Word boundary, so `material.alpha` is not satisfied by an assertion on `material.alpha_threshold`" — contains both needles, so those two fields are pinned by prose alone. The Lights and Collision resolve-side scans match `downcast_ref::<X>` anywhere in the file, so a future comment or test line naming an arm would mark it resolved even if the production arm were deleted.
- **Evidence**: The agent ran a verbatim port of the scanner on a scratch copy with the only two real assertions on `material.alpha` / `material.alpha_threshold` deleted; it still reported both as pinned. With `//` comments stripped, each field drops from 2 matching chunks to 1. The exterior-spawner guard, replayed comment-stripped, still holds on code text for all 6 files. The Lights and Collision scans have no false match today.
- **Impact**: The #3462 contract ("the next added copy cannot slip through") is weaker than stated. Together with NIFAL-D5-2026-09-14-02 and NIFAL-D7-2026-09-14-04, every text-scan or kitchen-sink completeness guard added since #3462 has at least one hole of this shape.
- **Related**: #3462, #4302, NIFAL-D5-2026-09-14-02, NIFAL-D7-2026-09-14-04. These three can reasonably be published as one guard-hardening issue.
- **Suggested Fix**: Strip `//`/`///` comments and string literals before matching, or require the needle inside an `assert…!(` / `.expect(` expression on the same statement. Reword the rationale comment so it cannot self-match. Scan only the production prefix (`split_once("#[cfg(test)]").0`) in `resolved_light_structs` / `resolved_shape_structs`.

#### NIFAL-D9-2026-09-14-03: Skill / audit-protocol / harness doc-rot that misdirects future audits of this layer (bundle)
- **Severity**: LOW (doc)
- **Dimension**: Completeness
- **Tier Violated**: harness-coverage gap (documentation audits rely on)
- **Game Affected**: all
- **Location / items**: each verified against the live tree. The two `byroredux/src/cell_loader/object_lod.rs` caller/exemption items are carried by NIFAL-D1-2026-09-14-04 / #4246 and are not repeated here.
  1. `.claude/commands/audit-nifal/SKILL.md:186`: particle call-site hints "~line 513" / "~line 642". The live sites are `byroredux/src/scene/nif_loader.rs:1539` and `byroredux/src/cell_loader/spawn.rs:1191`.
  2. `.claude/commands/audit-nifal/SKILL.md:210`: "carrying only the three `u32` counts". `CollisionAuthoringSummary` (`crates/nif/src/import/collision/mod.rs:89-102`) now has four (`plane_shapes`, #4163). An auditor applying the invariant literally would flag the fourth count as a leak.
  3. `.claude/commands/_audit-common.md:208`: Skyrim LE "No BYROREDUX_* env var reads this path yet". Since `fb8173fe0`, `BYROREDUX_SKYRIMLE_DATA` is read (`crates/nif/tests/common/mod.rs:89`).
  4. `crates/nif/tests/translation_completeness.rs:341`: the `#[ignore]` reason omits SkyrimLE, and the module doc at `:40` still says "default Steam install paths" (LE's fallback is a Wine prefix).
  5. `.claude/commands/audit-nifal/SKILL.md:258`: describes the harness as a fill rate "over the canonical `Material` slots". It measures the raw pre-merge `ImportedMaterial` tier (#2214), and that misreading is what makes FO76/Starfield near-zero `tex`/`nrm` look like leaks.
- **Status**: NEW (the `.claude/commands/audit-nifal/SKILL.md:252` "#3814 still-open" item is Existing: #4369 and excluded)
- **Description / Impact**: Each item points a future Dim 5/6/9 auditor at the wrong line, count or tier. Item 2 would manufacture a false no-leak finding; item 5 invites re-filing documented structural zeros.
- **Related**: #4369, #4246, #4360, NIFAL-D1-2026-09-14-04, NIFAL-D6-2026-09-14-01 (which also needs a skill-wording fix).
- **Suggested Fix**: Update the five locations. For the caller list, consider a doc-scan test in the style of `documented_texture_role_list_matches_the_struct` that derives the `translate_material(` caller set from `byroredux/src/`.

## Documented-limitation ledger

Restated so the next sweep does not re-derive them.

**Parked, not leaks (re-verified this sweep)**
- **7 raw-tier-parked Node fields** — `bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`. Zero canonical ECS consumers (Dim 4 grep). The only non-producer hits are a `#[cfg(test)]` fixture in `byroredux/src/cell_loader/terrain_lod_btr.rs` and the dev census tool `crates/nif/examples/bto_segment_census.rs`, which reads `bs_sub_index` but writes no component.
- **Distant-LOD node attributes** — `byroredux/src/cell_loader/object_lod.rs` and `byroredux/src/cell_loader/placement_lod.rs` attach neither `Billboard` nor `SceneFlags`. Measured content-absent: Skyrim SE `.bto` has 0 of 1,078 with a billboard node. Oblivion `_far.nif` has 1 of 157 (`fxoblivionlightbeamlong01_far.nif`), placed by 0 of 9,889 `distantlod` `.lod` files. `SceneFlags`' only reader is a debug console listing. FO4 `.bto` has not been measured yet; census it next sweep.
- **Passthroughs** — `NiTextureEffect` is content-absent; `BSInvMarker` is parsed but not walked; `NiSwitchNode` identity is not surfaced; `bs_bound` is consumed on the loose-NIF path only. `BSFurnitureMarker` is **consumed** (#2010) — do not flag it as parked.
- **Collision limitations** — `BhkNPCollisionObject` (the FO4+ `BhkSystemBinary` blob; container and object table decode, but `hknpCompressedMeshShapeData` does not) and `BhkPCollisionObject` phantoms (need a `TriggerVolume` path). `BhkPlaneShape → None` also stays deliberate, but its documented trimesh safety net is false for the one vanilla instance (NIFAL-D6-2026-09-14-01).
- **`CollisionAuthoringSummary`** now carries **four** game-agnostic `u32` counts (`plane_shapes` added by #4163). Not a leak; the skill text still says three.
- **Particles** — `initial_color` intentionally not applied; size-over-life curve deferred (constant `initial_radius × base_scale` only); multi-emitter attribution now residual only for the legacy `NiPSysEmitterCtlrData` and #3329 sequence rate tiers (doc drift: NIFAL-D5-2026-09-14-03). Starfield particle slice N/A (`starfield_corpus_has_no_particle_blocks`).
- **Animation** — per-light ambient colour channels parked. `EmissiveMultiple` / `RefractionStrength` float channels are explicitly parked (#3327). Morph weights reach `AnimatedMorphWeights`, but the GPU morph path they feed is unreachable end-to-end (NIFAL-D3-2026-09-14-02). `behavior_completion_events` is the documented fabrication exception in `convert_hkx_clip`.
- **Skinning** — #2440 (cell-loader path never builds `SkinnedMesh`) and #2441 (`SkinnedMesh.bones` `Option` terminal sentinel) were closed as *documentation* fixes (`dc4d738e`). They remain live, documented limitations and still match the code.
- **Emissive scale is a deliberate no-op** (`docs/engine/nifal.md` §4); no normalization constant exists or is wanted.
- **`material_kind: u32`** is the GPU dispatch contract, not a leak.

**Prior findings confirmed fixed this sweep (do not re-file)**
- #4166 (B-spline rotation finiteness) — fixed by `1cedb6f8e`; all four B-spline sampled sub-channels are guarded. Residuals on *other* paths are NIFAL-D7-2026-09-14-01/-03.
- #4167 (Animation + Particles completeness guards) — fixed by `f680df2e0`; the guards exist and are not wholesale-vacuous. Value/scan holes are NIFAL-D5-2026-09-14-02 and NIFAL-D7-2026-09-14-04.
- #3901 (`TextureFlipEntry` raw `TexType`) — fixed by `4520f8d53`; no raw slot reaches any canonical component, and the mapping follows `NiTexturingProperty`, not `slot_to_role`. Doc drift is NIFAL-D8-2026-09-14-04.
- #4044 (particle greyscale LUT outside the boundary) — fixed by `5ee1c150f`; neither spawn site writes preset fields after the overlay call.

**Existing open issues touched but not re-filed**: #4256 (`shader_type` discriminator), #4246 (two-phase doc; D1-04 extends it), #4301 (flipbook vs spawn-time `normal_has_alpha`; the analogous `SmoothSpec` flip vs spawn-time Phase-2 roughness should widen its scope), #4302, #4303, #4304, #4282, #4268 (Starfield `.mesh` bone indices unbounded), #4375, #4360 (D8-04 belongs with it), #4212, #4369.

## Method notes

- **Agents**: nine dimension agents ran independently, at most three at a time, each with its own `gh issue` dedup pass, its own read of the prior NIFAL baselines, and a full `git show` review of every commit touching its entry points since 2026-09-11. All were read-only: no repo edits, no engine launch. Censuses and mutation replays ran in scratch programs outside the repo.
- **Orchestrator verification**: before merging, the orchestrator re-read the code behind every HIGH and MEDIUM finding and confirmed the evidence:
  - `byroredux/src/helpers.rs:104-168`, `crates/nif/src/blocks/shader.rs:1947-1956`, `crates/nif/src/import/mesh/bs_geometry.rs:370-413` and `crates/nif/src/blocks/bs_geometry.rs:418-431`, `crates/nif/src/import/walk/lights.rs:121-137` against `byroredux/src/systems/light_anim.rs:228-239`.
  - The Gamebryo 3.2 `/mnt/data/src/reference/gamebryo-v32/Include/NiSpotLight.h` / `/mnt/data/src/reference/gamebryo-v32/Include/NiDirectionalLight.h` headers.
  - The absence of any transform `is_finite` guard in the AS/instance-upload path.
  - The particle `src_blend` fixture collision.

  It raised three agent-proposed severities to match `.claude/commands/_audit-severity.md`: D8-01 (NIFAL row) and D7-01/D7-02 (consistency with #4166), and considered-and-rejected raising D6-01.
- **Tests executed** (all green):
  - `byroredux-nif` lib filters: `material` 275, `mesh` 169, `skin` 106, `anim` 78, `import::collision` 74, `tangent` 41, `emitter` 39, `particle` 43, `bspline` 28, `precombine` 8, `skeleton` 6, `dispatch_coverage` + `light_dispatch`.
  - `byroredux-hkx` 21.
  - `byroredux-core --features inspect` `animation` 72 and `sanitize_finite` 10.
  - `byroredux` bin filters: `material_translate` 63, `light` 91, `particle` 45, `glass_classification` 24, `anim_convert` 20, the full boundary-guard set 94, and FO4 precombine tests including ignored ones, 14.
- **Completeness harness** (`cargo test -p byroredux-nif --test translation_completeness -- --ignored`): 8/8 games, 100% structural consistency, all floors passed.

  | game | tex | mat_path | kind | metO | nrm | tan |
  |---|---|---|---|---|---|---|
  | Oblivion | 91.4 | 0.0 | 0.0 | 100.0 | 0.0 | 84.0 |
  | FO3 | 92.6 | 0.0 | 10.9 | 94.3 | 79.2 | 99.1 |
  | FNV | 95.3 | 0.0 | 17.6 | 96.7 | 78.9 | 99.3 |
  | SkyrimLE | 92.4 | 0.0 | 42.3 | 99.2 | 67.7 | 94.0 |
  | SkyrimSE | 93.8 | 0.0 | 35.5 | **99.0** (was 93.8 — D8-01) | 76.1 | 94.8 |
  | FO4 | 92.7 | 57.9 | 57.0 | 99.4 | 82.9 | 96.3 |
  | FO76 | 12.6 | 81.3 | 25.7 | 18.2 | 9.9 | 96.6 |
  | Starfield | 0.0 | 94.9 | 3.7 | 5.1 | 0.0 | 100.0 |

  The Skyrim LE vs SE row gap is sample composition, not translation divergence: SE samples `Meshes0` only. On 198 identical files the importer output converges (nrm 67.6/65.5, kind 42.5/42.7, metO 99.2/99.2). FO76/Starfield near-zero `tex`/`nrm` are the documented structural zeros of the raw pre-merge tier.
- **Ground-truth censuses**:
  - Material: Skyrim SE glass-eligible meshes by `material_kind`; FNV/FO3 `window_env_mapping`-only promotions; FO4/Skyrim `.bto` `material_path` rate (0/980, 0/2,392).
  - Geometry: 40,000 Starfield BSGeometry spheres; 1,744,036 FO4 precombine objects (tangents, band overlap).
  - Animation: 16.06M rotation keys across FNV, Oblivion and Shivering Isles.
  - Nodes: 1,078 Skyrim SE `.bto` files and 157 Oblivion `_far.nif` + 9,889 `.lod` placements.
- **Out-of-scope observations** (not NIFAL findings; route elsewhere):
  - #4234 is still OPEN though `66fff31bf` implements its fix (the commit carries no `Fix #4234` keyword).
  - Three throwaway census examples, `crates/nif/examples/tmp_fo4_d4_{psglod,lodoverlap,lodsize}.rs`, are committed (`/audit-tech-debt`).
  - The Dim 5 agent could not read the Gamebryo reference mount, so it left unverified whether life-span and radius variation carry the same ± half-width error #4240 fixed for angles. The orchestrator later read the same mount successfully, so that check is cheap next sweep.
  - FO4 `.bto` billboard census not yet run.
- **Coverage note**: this audit touches NIFAL-adjacent code only. The un-owned subsystems listed in `.claude/commands/_audit-common.md` were not examined: the gameplay slice, SDK, launcher, FaceGen, mod runtime, FSR3 and the debug server/protocol. The hkx parser was examined only where it feeds `convert_hkx_clip`.
- `.claude/commands/_audit-validate.sh` passes (all skill path references valid; 235 pre-existing advisory symbols, tracked by #4360). Every backticked path in this report was checked against the live tree.

**Suggested publish labels**: domain `nifal` on all findings.
- **D1-01/-02/-03/-04**: `renderer` (+ `game:fnv`/`game:fo3` for D1-01, `game:skyrim` for D1-02).
- **D8-01**: `nif-parser` + `game:skyrim`.
- **D8-02/-03**: `import-pipeline` + `game:fo3`/`game:fnv` and `game:fo4` respectively.
- **D8-04, D5-03, D3-03, D6-02, D9-03**: `doc-rot`.
- **D2-01**: `import-pipeline` + `renderer` + `game:starfield`.
- **D3-01**: `import-pipeline` + `renderer`.
- **D3-02**: `renderer` + `animation`.
- **D5-01**: `import-pipeline`.
- **D6-01**: `physics` + `game:skyrim`.
- **D7-01/-02/-03**: `animation` (+ `safety` for -01/-02).
- **D5-02, D7-04, D9-02**: `test-gap` (candidate for one combined guard-hardening issue).

## Next Step

```
/audit-publish docs/audits/AUDIT_NIFAL_2026-09-14.md
```
