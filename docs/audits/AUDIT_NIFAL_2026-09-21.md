# NIFAL Audit — 2026-09-21

Full 9-dimension run (orchestrator + 9 dimension subagents, one per dimension, all
concurrent). **HEAD**: `4dca737e2` · **Baseline**: `docs/audits/AUDIT_NIFAL_2026-09-16.md`
(HEAD `7996edf61`, Dims 1+8 only; last full-9 run 2026-09-14) · **Audited**: all 9 ·
**Unchanged since baseline (skimmed)**: none — every dimension had at least one commit
since `7996edf61` (152 commits total), so all were audited in full.

Dedup caveat for this run: `gh` was **offline** (no network). Issue states were derived
from local sources — the fix-commit map (`git log --grep '#NNNN'`), prior reports in
`docs/audits/`, code-comment issue references, and ROADMAP/action-plan docs. The window
since baseline contains the whole NIFAL fix batch (#4398–#4407, #4421–#4436, #4467,
#4514/#4523/#4529), so most of this audit is fix-holding verification; **every
known-FIXED item was re-verified against the live tree, not just its commit message.**

## Executive Summary

**18 new findings: 0 CRITICAL, 1 HIGH, 2 MEDIUM, 15 LOW.** Nine of the fifteen LOWs are
doc-rot (stale comments/spec prose), continuing the pattern that the code side of the
translation layer is in materially better shape than its documentation.

**Headline (HIGH): the static `NiTransform` translate boundary is non-finite-blind.**
Every gate on the static path is an unordered float comparison — false on NaN — so a NaN
rotation matrix passes `sanitize_rotation` unchanged and becomes an all-NaN quaternion,
an **infinite** entry makes `repair_rotation_svd_or_identity` itself *return* a NaN
matrix (the `max_sv < 0.01` identity escape is false on NaN singular values), and
translation/scale have no finite gate at all. The NaN reaches the canonical `Transform`
→ `GlobalTransform` → TLAS instance / BLAS refit with a non-finite AABB — Vulkan
AS-build UB, the exact chain `d53be91be` just closed for the animation side (#4396/
#4397). The codebase is now internally inconsistent about the same poison class; the
collision and emitter paths already gate their transforms, making geometry the outlier.
(NIFAL-D2-2026-09-21-01)

**Two MEDIUMs, both NEW:**
- A particle system whose own data block authors `BS Max Vertices = 0` ("keep the
  preset") falls through `extract_emitter_max_particles`' own-instance path to the
  whole-scene first-match scan and silently inherits a **sibling's** budget in
  multi-emitter NIFs (67.3% of Oblivion+DLC content). Residual hole in #4261's
  per-instance attribution. (NIFAL-D5-2026-09-21-01)
- `populate_draugr_combat_clips` is wired only on the `--game` world-setup route; the
  `--esm --cell` route installs the walk clip but never the combat family while still
  attaching the `DraugrCombatAnim` marker — so the P2 combat takes silently no-op on
  exactly the route the P2 gate's smoke script drives (`BleakFallsBarrow01`).
  (NIFAL-D7-2026-09-21-01)

**Clean windows**: Dimension 8 produced **zero new findings** — all 13 commits since
baseline are fixes answering the 09-16 report and its issue batch, and all 16
known-fixed issues verified holding in code and by test. Dimension 6 has zero tier
violations (16 dispatched bhk shapes / 16 resolve arms / 0 missing, counted fresh).
Dimension 9's boundary inventory fully holds — every since-baseline addition (morph
slots, combat takes, walk clips, the `build_material_texture_handles` consolidation)
routes through a declared boundary — and the completeness harness re-run on SkyrimSE,
FO4 and Starfield shows zero drift (FO4's metO/rghO 99.4% investigated and legitimate:
union-of-signals classification, #4393 gate intact).

Per-category status against `docs/engine/nifal.md` §2:

| Category | Spec status | This sweep |
|---|---|---|
| Material (Dim 1) | converged | **converged**: 0 tier violations, 2 LOW consumer-side gaps (a missed #4444 literal site; LOD base-texture resolve ignores the clamp it just produced) |
| Geometry / Transform (Dim 2) | converged (reference template) | **1 HIGH** (non-finite-blind boundary), 1 LOW doc |
| Skinning (Dim 3) | half-stale prose (#4410) | **clean**; #3930 verified FIXED (`a4cbe36f8`); 1 LOW doc (the #4410 code-side sibling) |
| Lights (Dim 3) | converged | **clean**; 2 LOW doc |
| Nodes (Dim 4) | triaged | **clean** — the parked-field grep is empty; 2 LOW doc |
| Particles (Dim 5) | emitter base converged | **1 MEDIUM** (budget sibling-inheritance), 2 LOW |
| Collision (Dim 6) | audited | **clean** — 0 tier violations; 1 LOW NEW (rename comment miss) + 2 Existing re-checks |
| Animation (Dim 7) | converged | **1 MEDIUM** (combat clips not installed on `--cell` route), 3 LOW; both declared producers intact, zero third producers |
| Shader flags / texture sets (Dim 8) | converged | **clean** — 0 new; residuals are Existing #4426/#4430 |
| Completeness (Dim 9) | signal live | fills stable, boundary inventory holds; 1 LOW (ceiling guard covers only Skyrim lanes) |

Tier-invariant violations among the **new** findings:

| Invariant | Count | Findings |
|---|---|---|
| single-boundary | 2 | D1-01 (restated parallax literal), D7-02 (softened — post-boundary overrides, not a second construction site) |
| no-fabrication | 3 | D5-01 (MEDIUM, sibling budget), D5-02, D5-03 |
| no-leak | 2 | D2-01 (HIGH, non-finite crossing), D1-02 (carried `texture_clamp_mode` ignored by the LOD samplers) |
| no-render-time-fallback | 0 | — |
| none / doc-rot / harness-gap | 11 | D2-02, D3-01..03, D4-01..02, D6-01, D7-01 (install-site coverage, none of the four strictly), D7-03, D7-04, D9-01 |

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | PASS — `translate_material` sole producer, 4 spawn/LOD callers + Cornell; signature un-widened; `build_material_texture_handles` is a new *consolidation* (#4529), not a second site | PASS — #4422's neutrals measured; no emissive normalization (Q2 stays no-op) | PASS — plain clamped f32s; `sanitize_finite` list matches every `Material` f32 field | PASS — glass classified once post-`resolve_pbr`; zero `classify_pbr` in `render/` | `byroredux/src/material_translate.rs::translate_material` |
| Geometry / Transform | PASS — one coord SoT, matrix sanitizer at exactly the two `stream.rs` readers, destrip unified | PASS — BSGeometry bound ×`HAVOK_SCALE` carries its 40k-shape measurement | **FAIL** — D2-01: non-finite rotation/translation/scale crosses unresolved | PASS — authorship + tangents resolved at extraction | `crates/nif/src/import/{coord,transform}.rs` + `rotation.rs::sanitize_rotation` |
| Skinning | PASS — one production `SkinnedMesh::new_with_global` site (`attach_nif_skin_binding`); cell-loader structural gap #2440 unchanged (documented) | PASS — packed indices only widened (cited to nifly + on-disk measurement, real-data test passed this session) | PASS — global bone indices; no consumer re-derives partition layout | PASS — palette from canonical `SkinnedMesh` | `import/mesh/skin.rs::extract_skin_*` → `nif_loader.rs::attach_nif_skin_binding` |
| Lights | PASS — NIF: one `imported_light_from_base`; ESM: 3 LIGH producers all through `translate_light`/`canonical_light_*` (#4514 consolidation) | PASS — every constant cited (NiSpotLight.h column-0, xEdit defaults, DAT2+56 enum) | PASS — renderer reads only canonical `Emitter` | PASS — `gpu_light_from_emitter` matches canonical kind only | `walk::lights::imported_light_from_base` + `systems/light_anim.rs::translate_light` |
| Nodes | none by design (spec-triaged two-path split) | PASS | PASS — all seven parked fields have **zero** canonical consumers (core grep empty) | PASS | (no single `translate_node`, by design) |
| Particles | PASS — exactly 2 `apply_emitter_overlays` callers; no inline overlays remain | **PARTIAL** — D5-01 (sibling budget via whole-scene fallback) | PASS — `original_type` log-only; renderer forwards verbatim with pins | PASS — billboard-vs-volumetric at spawn | `byroredux/src/systems/particle.rs::apply_emitter_overlays` |
| Collision | PASS — `resolve_shape_inner` sole bhk→`CollisionShape` site; no bhk downcast outside `import/collision/` | PASS — zero-mass reclass + proxy motion-type documented, not guessed | PASS — `hkMotionType` collapses once; `CollisionAuthoringSummary` = four `u32`s; proxy renderer-free | PASS — fallback branches on canonical `RenderLayer` | `import/collision/shape.rs::resolve_shape_inner` |
| Animation | PASS — exactly two producers (`convert_nif_clip` ×4–5 callers, `convert_hkx_clip` ×3 in-module); combat takes + walk clips both through `convert_hkx_clip`; zero third producers | PASS — one documented exception (hkx completion events); walk-speed clamp measurement-anchored | PASS — consumers handle-driven | PASS — take decisions are state machines over canonical handles | `anim_convert.rs::convert_nif_clip` + `asset_provider/animation.rs::convert_hkx_clip` |
| Shader flags / texture sets | **FAIL (Existing #4426)** — flipbook frames bypass the per-role resolver; everything else holds (`slot_to_role` one table; #4529 single handles producer) | **PARTIAL (Existing #4430)** — FO4 slot-2 arm uncited, ignores `glow_map` | PASS — no per-game slot index crosses the boundary; #4434 named-role overlay verified both sides | PASS — zero `if game` in `triangle.frag` + includes (re-run this sweep) | `slot_role.rs::slot_to_role` → `resolve_material_texture_handles_with_clamp` |
| Completeness signal | n/a (meta) | n/a | n/a | n/a | inventory verified: all 5 declared boundaries hold, caller counts re-censused |

## Findings

### HIGH

#### NIFAL-D2-2026-09-21-01: Static NiTransform translate boundary is non-finite-blind — NaN/inf rotations, translations and scales reach the canonical Transform and thence BLAS/TLAS builds
- **Severity**: HIGH (Vulkan AS-build UB on non-finite AABB — the #4166 chain; vanilla incidence 0, mod/corrupt content live scope per #2383; rated per the 2026-09-14 orchestrator ruling "impact, not likelihood")
- **Dimension**: Geometry/Transform
- **Tier Violated**: no-leak (raw-tier non-finite data crosses the translate boundary unresolved; `rotation.rs`'s own contract — "downstream code can assume the matrix is a valid rotation" — is silently void)
- **Game Affected**: all (corrupt or hand-edited NIFs, modded content)
- **Location**: `crates/nif/src/rotation.rs:14-20` (`is_degenerate_rotation`), `:69-81` (`is_non_orthonormal`), `:165-197` (`repair_rotation_svd_or_identity`); `crates/nif/src/import/coord.rs:58-70` (det gate + SVD arm); `crates/nif/src/stream.rs:745-755` / `:769-779` (both read sites — translation/scale ungated); consumer `byroredux/src/scene/nif_loader.rs:1245-1253`
- **Status**: NEW
- **Description**: Every gate on the static-transform path is an unordered float comparison, and all such comparisons are false for NaN. A **NaN** rotation matrix passes `sanitize_rotation` unchanged (det and orthonormality checks NaN-blind), then `zup_matrix_to_yup_quat`'s `(det - 1.0).abs() < 0.1` fast path is also false, routing to SVD whose arithmetic on NaN yields an all-NaN quaternion (`normalize_quat`'s zero-length guard does not catch NaN). An **infinite** entry trips the degenerate branch, but `repair_rotation_svd_or_identity` then *returns a NaN matrix*: `max_sv < 0.01` is false for NaN singular values, so the identity escape never fires — the repair function violates its own doc for non-finite input. `translation` and `scale` have no finite gate at all. The NaN flows `Quat::from_xyzw` → `Transform` → `GlobalTransform` → TLAS instance / skinned BLAS refit with a non-finite AABB. Repo-wide grep confirms no `is_finite` gate on static transforms downstream; the single downstream check (`byroredux/src/cell_loader/spawn.rs:200-206`) only skips the mesh from the collision-proxy bounds union — the entity still spawns poisoned, and that check is itself the leak pattern in miniature (a consumer re-validating what the boundary should resolve). Static-path sibling of the just-fixed #4396/#4397; the collision and emitter paths already gate their own transforms (`collision/shape.rs:278`, `collision/mod.rs:387`, `walk/emitter.rs`), so geometry is the outlier, not the convention.
- **Evidence**: call path `read_ni_transform` (`read_ni_point3` + `sanitize_rotation(read_ni_matrix3())` + raw scale) → `compose_transforms` → `zup_matrix_to_yup_quat` (`coord.rs:62`) → `ImportedMesh.rotation` → `Quat::from_xyzw` (`nif_loader.rs:1245`). Rust semantics: every NaN comparison is false; inf-containing matrices produce NaN singular values in SVD.
- **Impact**: One corrupt float in any placed NIF poisons the entity's `GlobalTransform`; for a skinned or TLAS-instanced mesh that is a non-finite AABB in an acceleration-structure build — GPU UB / potential `VkDevice` loss, not a visual glitch. All games; corrupt/mod content only (vanilla incidence measured 0 for the adjacent classes: #3532's 642,589-matrix census, #4397's 16.06M-key census).
- **Related**: #4396, #4397 (fixed anim-side siblings, `d53be91be`); #4166 (blast-radius precedent); #2383
- **Suggested Fix**: Gate the nine rotation cells, translation and scale on `is_finite()` at the two `stream.rs` read sites (or at the head of `sanitize_rotation`: non-finite → identity, matching the established `max_sv < 0.01` precedent, behind the existing rate-limited warn). Have `repair_rotation_svd_or_identity` verify its output determinant is finite before returning, closing the inf-entry case where the repair itself manufactures NaN.

### MEDIUM

#### NIFAL-D5-2026-09-21-01: A zero-own-budget particle system inherits a sibling's authored budget via the whole-scene fallback
- **Severity**: MEDIUM (silently wrong authored-value attribution; density/memory wrong in the dense direction on multi-emitter NIFs)
- **Dimension**: Particles
- **Tier Violated**: no-fabrication (a value the emitter's own data block did not author is applied to it)
- **Game Affected**: Oblivion / FO3 / FNV / Skyrim / FO4 (any multi-emitter NIF where one system authors `BS Max Vertices = 0` and another authors a budget; Starfield N/A per #2354)
- **Location**: `crates/nif/src/import/walk/emitter.rs:355-380` (`extract_emitter_max_particles`)
- **Status**: NEW
- **Description**: `extract_emitter_max_particles` only returns from the per-instance `data_ref` path when the own block yields a budget `> 0`. When the system's own `data_ref` resolves to its own data block and that block authored `0` (parsed to `None`, documented semantic "no authored budget → keep the preset", `blocks/particle.rs:167-177`), execution falls through to the whole-scene `find_map` scan — written for *unresolvable* refs and tested only as `null_data_ref_falls_back_to_the_whole_scene_scan`. In a multi-emitter NIF the scan returns the first budget-bearing block in block order, i.e. a sibling system's budget, which `apply_emitter_overlays` then clamps and writes into this system's `preset.max_particles`.
- **Evidence**: own-ref hit requires `.and_then(|d| d.max_particles).filter(|m| *m > 0)`; both `None` (authored 0) and a non-`NiPSysBlock` target drop to the scene-wide scan. The existing zero-budget test uses a single-block scene, so the zero-own-budget + sibling-with-budget shape is untested.
- **Impact**: The emitter's pool silently jumps from its heuristic preset (~64–96) to `min(sibling_budget, 256)` — unlogged, game-agnostic, exactly on the multi-emitter NIFs #4261 measured at 67.3% of Oblivion+DLC content. Authored-0 budgets are rare ("a handful of vanilla emitters"), so frequency is low.
- **Related**: #4261 (per-instance attribution — this is a residual hole in that fix), #3344
- **Suggested Fix**: When `data_ref.index()` is `Some` **and** the target downcasts to `NiPSysBlock`, return its budget as-is (`None` stays `None`); reserve the whole-scene scan for NULL/non-resolving/non-downcasting refs. Add the two-system fixture (A budget 0, B budget 5000 → A keeps preset, B gets its own).

#### NIFAL-D7-2026-09-21-01: Draugr combat clips are never installed on the cell-loader route — including the P2 gate's own smoke route
- **Severity**: MEDIUM (the translated content is silently dropped on one engine route; kept at MEDIUM rather than the "removes visible content → HIGH" NIFAL row because it is a 3-day-old dev fixture on a not-yet-gated P2 feature, not shipped-content regression)
- **Dimension**: Animation / controllers (P2 combat tail install path)
- **Tier Violated**: none of the four strictly (the boundary itself is correct — `convert_hkx_clip` is used); an install-site coverage gap
- **Game Affected**: Skyrim (the only game with the fixture)
- **Location**: `byroredux/src/cell_loader/load.rs:635-637` and `:1014-1015` (missing call), against `byroredux/src/scene/world_setup.rs:1012-1017` (present), `byroredux/src/asset_provider/animation.rs:251` (`populate_draugr_combat_clips`)
- **Status**: NEW
- **Description**: `populate_draugr_combat_clips` is invoked only from the `--game` world-setup route. The `--esm … --cell` cell-loader route installs `populate_idle_clip_runtime` + `populate_skyrim_walk_clip` at both of its sites but never the combat family, while the shared spawn finalize still inserts the `DraugrCombatAnim` marker (`npc_spawn/resumable.rs:1075-1080`, gated only on race + skeleton) — so `combat_feedback_system` runs, finds no `DraugrCombatClips` resource, and silently no-ops. The P2 gate smoke script drives exactly this route (`docs/smoke-tests/p2-melee-core.sh` → `--esm $FIXTURE_ESM --cell BleakFallsBarrow01`, target `encdraugr01ambushmelee2hheadm06` / REFR `0x0383F7` — the same actor the fixture doc pins), so the fixture doc's step-4 gate ("assert the death take actually started") cannot pass where it is designed to run.
- **Evidence**: `grep -rn populate_draugr_combat_clips byroredux/src` → only `scene/world_setup.rs:1017` (+ a doc comment); `cell_loader/load.rs:636`/`:1015` install the walk clip with no combat sibling.
- **Impact**: The headline P2 capability ("play one attack/hit/death animation family and spatial sound family") is inert on the `--cell` route — the route every current smoke test and the AGENTS.md usage examples use — while working on `--game`.
- **Related**: NIFAL-D7-2026-09-21-03; `docs/engine/p2-combat-anim-sound-fixture.md` §Wiring step 4
- **Suggested Fix**: Add `populate_draugr_combat_clips` beside the two `populate_skyrim_walk_clip` call sites in `cell_loader/load.rs` (already idempotent + game-gated), or fold the three clip installers into one helper so a future route cannot pick up two of three.

### LOW

#### NIFAL-D1-2026-09-21-01: #4444's named-default sweep missed `terrain_lod_btr.rs` — the `.btr` distant-terrain sibling still restates the parallax defaults as literals
- **Severity**: LOW (latent per-game divergence on retune)
- **Dimension**: Material (canonical-default single source)
- **Tier Violated**: single-boundary (a canonical default constant re-invented at a second production site)
- **Game Affected**: Skyrim / FO4 (`.btr` distant terrain) vs FNV (`terrain_lod.rs` path)
- **Location**: `byroredux/src/cell_loader/terrain_lod_btr.rs:394-395`
- **Status**: NEW (incomplete sweep in the #4444 fix; the site was never converted)
- **Description**: f5cddc7c5 (#4444) replaced bare `0.04`/`4.0` parallax literals with `DEFAULT_PARALLAX_HEIGHT_SCALE`/`DEFAULT_PARALLAX_MAX_PASSES` at `terrain.rs`, `terrain_lod.rs`, `render/particles.rs` and the `GpuMaterial` neutral — but left the `.btr` spawner on the literals. A future retune now splits distant terrain by game. Repo-wide grep: this is the **only** remaining production site restating either literal.
- **Evidence**: `terrain_lod_btr.rs:394-395` literals vs `terrain_lod.rs:864-868` / `terrain.rs:1094-1098` on the named constants.
- **Impact**: None today (values equal); on retune, `.btr` distant terrain keeps old POM parameters while every other synthetic path moves — silent, untestable (both files build `MaterialTextureHandles` inline; no guard compares them).
- **Related**: #4444, #3073, NIFAL-D1-2026-09-21-02
- **Suggested Fix**: Swap the two literals for the named constants (2-line change); optionally extend #4444's regression test to grep for bare parallax literals in production code.

#### NIFAL-D1-2026-09-21-02: The two LOD `translate_material` callers resolve their base textures clamp-unaware — the canonical `texture_clamp_mode` they just produced is not consumed
- **Severity**: LOW (edge-texel bleed on distant geometry; population unmeasured)
- **Dimension**: Material (canonical carried field dropped by a consumer)
- **Tier Violated**: no-leak (carried-not-re-derived: the canonical field is correct, but the only sampler on these paths ignores it)
- **Game Affected**: placement-LOD: Oblivion / FO3 / FNV (`_far.nif`); object-LOD: Skyrim / FO4 (`.bto`)
- **Location**: `byroredux/src/cell_loader/placement_lod.rs:564`, `byroredux/src/cell_loader/object_lod.rs:492` (contrast `nif_loader.rs:1241`, `mesh_instance.rs:992`)
- **Status**: NEW
- **Description**: Both full-detail spawn paths resolve the base texture with `resolve_texture_with_clamp(.., material.texture_clamp_mode)` (#2571/#610 contract). The two LOD callers run `translate_material` (clamp mode in scope and riding to the entity on the `Material`), then resolve with plain `resolve_texture`, which hardcodes clamp 3 (WRAP). These paths attach no `MaterialTextureHandles` (the documented #4246 exemption), so the base texture is the *only* sampler the authored clamp could reach — and it doesn't. Same shape as the 09-16 flipbook finding (D8-01/#4426), on the LOD lane.
- **Evidence**: `placement_lod.rs:546` builds the material; `:564` resolves via `resolve_texture`. `object_lod.rs:492` same inside the loop that feeds `insert_object_lod_submesh_material`.
- **Impact**: Authored CLAMP on a `_far.nif`/`.bto` material is sampled WRAP — #610's edge-bleed class on distant geometry. Structural divergence: four callers of one boundary, two honoring a canonical field and two not.
- **Related**: #610, #2571, #4246, #4264, #4426
- **Suggested Fix**: Resolve with `resolve_texture_with_clamp` threading `material.texture_clamp_mode` at both sites; census the authored-clamp population of `_far.nif`/`.bto` materials first, the way D8-01 did for flipbooks.

#### NIFAL-D2-2026-09-21-02: Stale field doc — `ImportedMesh.tangents` still describes the Starfield UDEC3 unpack as unbuilt
- **Severity**: LOW (doc)
- **Dimension**: Geometry/Transform
- **Tier Violated**: — (doc rot)
- **Game Affected**: Starfield (doc only)
- **Location**: `crates/nif/src/import/types.rs:950-953` vs `crates/nif/src/import/mesh/bs_geometry.rs:296-334`
- **Status**: NEW
- **Description**: The `tangents` doc ends "UDEC3 unpack into `[f32; 4]` is a follow-up to this issue." The unpack has been implemented since #1086/#1232 and is pinned by `bs_geometry_tangent_tests.rs` (counts, values, exact ±1 signs). Only the prose is stale.
- **Evidence**: `bs_geometry.rs:296-334` produces `Vec<[f32;4]>` from `tangents_raw` via `unpack_udec3_xyzw` + `clamp_sign` (#2246) or `synthesize_tangents_yup` (#1232).
- **Impact**: A reader concludes Starfield tangents are untranslated and may "fix" a non-gap.
- **Related**: #1086, #1232, #2246
- **Suggested Fix**: Replace the stale sentence with the current reality.

#### NIFAL-D3-2026-09-21-01: `ImportedSkin.global_skin_transform` doc implies the runtime palette composes the transform — the code (and its own regression test) deliberately does not
- **Severity**: LOW (doc-rot with a live regression trap)
- **Dimension**: Skinning/Lights
- **Tier Violated**: — (doc contract contradicts the canonical consumer)
- **Game Affected**: all (trap fires on legacy body NIFs)
- **Location**: `crates/nif/src/import/types.rs:1288-1299` vs `crates/core/src/ecs/components/skinned_mesh.rs:72-90`/`:167-182` and `byroredux/src/scene/nif_loader.rs:1795-1807`
- **Status**: NEW (code-side sibling of open #4410; fold into it on publish if #4410's scope is read broadly)
- **Description**: The raw-tier field doc's narrative ("OpenMW composes it into the runtime palette as the OUTERMOST factor … which is why our pre-Phase-1b.x palette produced the ribbon artifact") implies the current palette composes it. The post-#771 semantics — pinned by `palette_matches_nifly_skin_to_bone_semantics_with_non_identity_global` — is the opposite: `bind_inverses[i]` is nifly's compose-ready `transformSkinToBone` and `compute_palette_into` does **not** multiply `global_skin_transform` (doing so double-applies).
- **Evidence**: `compute_palette_into` builds `palette[i] = bone_world × bind_inverses[i]` with no reference to the field; the field is documented "Informational / diagnostic only".
- **Impact**: A future change that "restores the documented invariant" by composing the transform would double-apply the offset on every legacy body NIF (Doc Mitchell class) and regress skinning while the doc keeps pointing at the wrong authority.
- **Related**: #4410, #771, M41.0 Phase 1b.x
- **Suggested Fix**: Rewrite the doc to state the resolution (captured, informational only, NOT composed at runtime; `compute_palette_into`'s doc + its regression test are the authority).

#### NIFAL-D3-2026-09-21-02: Stale light-doc contracts — "zero for ambient/point" direction (twice) and a phantom numeric kind tag
- **Severity**: LOW (doc)
- **Dimension**: Skinning/Lights
- **Tier Violated**: — (docs misstate the canonical contract)
- **Game Affected**: all
- **Location**: `crates/nif/src/import/types.rs:34-36` and `:42-44`; `crates/core/src/lighting.rs:252-254`
- **Status**: NEW
- **Description**: Since #4395, `imported_light_from_base` computes world-rotation column 0 for **every** kind and `Emitter::from_legacy_world_units` sanitizes it, so a NIF point/ambient light's canonical `direction` is a normalized non-zero unit vector. The documented "zero for ambient/point" contract is false in two places (renderer point branch ignores it, so behaviour is correct). The `kind` doc's "0 = ambient, 1 = directional…" numeric tag matches neither the `EmitterKind` enum order nor the GPU packing (Ambient|Point→0, Spot→1, Directional→2) — pre-enum prose.
- **Evidence**: `walk/lights.rs:138-151` (column 0 for all kinds, negated only for Directional); `render/lights.rs:77-81` (GPU packing).
- **Impact**: An auditor checking the "point direction is zero" contract finds it violated and may "fix" the boundary, or write a consumer relying on zero.
- **Related**: #4395, #2205
- **Suggested Fix**: Update the three doc sites: direction is "column 0 of the world rotation for all kinds; consumers ignore it for ambient/point"; `kind` is the enum, GPU packing lives in `gpu_light_from_emitter`.

#### NIFAL-D3-2026-09-21-03: #3987's Starfield-shadow narrative and its test message still say `for_legacy_projection(false)` = "ARCHITECTURE only", stale since 3ce970a5a
- **Severity**: LOW (doc)
- **Dimension**: Skinning/Lights (ESM boundary documentation)
- **Tier Violated**: —
- **Game Affected**: Starfield (narrative), all legacy-fill lights (the constant)
- **Location**: `byroredux/src/systems/light_anim.rs:151-156` and `:1022-1030`; constant at `crates/core/src/lighting.rs::VisibilityMask::for_legacy_projection`
- **Status**: NEW
- **Description**: `3ce970a5a` (2026-09-17) broadened `for_legacy_projection(false)` from `ARCHITECTURE` to `ARCHITECTURE | DYNAMIC_ACTOR` so legacy fill lights cast dynamic-actor contact shadows. The #3987 rationale block still derives its punchline from the old value, and the test's assertion message states the old mapping in the present tense.
- **Evidence**: `git show 3ce970a5a -- crates/core/src/lighting.rs` (mask broadened with updated doc); `light_anim.rs:1027-1029`.
- **Impact**: The next person re-verifying #3987 finds doc and constant disagreeing and must re-derive which is real.
- **Related**: #3987, 3ce970a5a
- **Suggested Fix**: Date the historical narrative and update the test message to name the current mask.

#### NIFAL-D4-2026-09-21-01: nifal.md §2 Nodes still says the `.spt` path "keeps using `placement_root_billboard`" — dead since #3076; the #3533 fix sweep missed the spec
- **Severity**: LOW (doc-rot in this dimension's ground-truth spec)
- **Dimension**: Nodes
- **Tier Violated**: — (documentation; code side correct and pinned)
- **Game Affected**: all games with TREE/spt exterior content
- **Location**: `docs/engine/nifal.md:287-288`; code truth `crates/spt/src/import/mod.rs:214`/`:410`, `byroredux/src/cell_loader/references/import.rs:550-556`, `byroredux/src/cell_loader/spawn.rs:926-934` (consumer branch "currently unreachable")
- **Status**: NEW (residual of closed #3533 — its fix commit `57fdcc577` swept code comments but not this spec paragraph)
- **Description**: #3076 (`aee8783f2`) moved the SpeedTree billboard from the placement root onto the renderable mesh. Since then the spt root is a plain anchor (`billboard_mode: None`, pinned by `placeholder_uses_default_size_without_bounds`), the quad carries the mode on `ImportedMesh.billboard_mode`, and `CachedNifImport::placement_root_billboard` is structurally always `None`. The spec paragraph still asserts the opposite.
- **Evidence**: spt test asserts `imported.nodes[0].billboard_mode == None, "root is a plain anchor"`; `AUDIT_SPEEDTREE_2026-08-30.md` §#3533 row ("`spawn.rs` is dead for `.spt`").
- **Impact**: The next Dim 4 audit starts from a false premise about the spt billboard contract.
- **Related**: #3533, #3076, #2206, #994
- **Suggested Fix**: Rewrite the sentence: since #3076 the spt billboard rides on the placeholder mesh via the #2206 per-mesh consumer; `placement_root_billboard` is a documented dead seam for a future `NiBillboardNode`-rooted producer (none exists).

#### NIFAL-D4-2026-09-21-02: `spawn_nif_nodes`' SceneFlags comment claims unconditional emission; the code has gated it on `flags != 0` since the comment's own introducing commit #222
- **Severity**: LOW (comment/code mismatch at a node spawn site; no functional reader today)
- **Dimension**: Nodes
- **Tier Violated**: — (documentation)
- **Game Affected**: all (any all-zero-flags node, i.e. most nodes)
- **Location**: `byroredux/src/scene/nif_loader.rs:1560-1570` (comment "unconditionally (not gated on `flags != 0`)" immediately above `if node.flags != 0 { … }`)
- **Status**: NEW (contradiction present since introducing commit `f0a0ec1f6`, 2026-04-20; never filed)
- **Description**: A node with zero flags gets no `SceneFlags` row at all, so the "future toggle-visible system can just flip the bit on the existing component" contract the comment offers does not hold for those entities. Production readers today: only the debug console listing (re-verified).
- **Evidence**: `git show f0a0ec1f6` shows the comment and the gated `if` added together.
- **Impact**: None today; latent for a future visibility-toggle system written to the comment's contract (would find no row on zero-flag nodes and silently no-op).
- **Related**: #222, #1235
- **Suggested Fix**: Prefer making the code match the comment (drop the gate; `SceneFlags::from_nif(0)` is `Default`, i.e. visible; sparse-stored), else fix the comment.

#### NIFAL-D5-2026-09-21-02: Legacy `NiPSysEmitterCtlrData` rate tier is still a whole-scene first-match
- **Severity**: LOW (measured 0 files on FNV; dead on the target corpus)
- **Dimension**: Particles
- **Tier Violated**: no-fabrication (misattributed authored rate, legacy tier)
- **Game Affected**: pre-10.2 Gamebryo content only
- **Location**: `crates/nif/src/import/walk/emitter.rs:452-458` (documented residual), `:702-708` (the scan)
- **Status**: NEW (documented in-code as a deliberate #4261 residual, never issue-tracked)
- **Description**: The final tier of `extract_emitter_rate` scans the whole scene for the first `NiPSysEmitterCtlrData` and applies its first birth-rate key to *this* system — the same whole-scene first-match class #4261 eliminated and #4467 repaired, left on the deprecated legacy tier.
- **Evidence**: `scene.blocks.iter().find_map(…)` with no per-instance key, in contrast to the own-chain + `target_ref` fallback directly above it.
- **Impact**: Minimal today (block deprecated pre-10.2, census-measured absent on FNV); live only for old non-Bethesda Gamebryo FX with multiple emitters.
- **Related**: #4261, #4467, NIFAL-D5-2026-09-21-01
- **Suggested Fix**: If a corpus measurement ever shows target-game incidence, link the legacy block through its owning `NiParticleSystemController`; otherwise leave with the existing comment + this report as the tracking record.

#### NIFAL-D5-2026-09-21-03: APP_CULLED / editor-marker gating missing on the `NiParticleSystem` leaf in both walkers
- **Severity**: LOW (incidence unmeasured; analogous #3640 census found 581 APP_CULLED shapes in 13 FNV files)
- **Dimension**: Particles (block selection feeding the boundary)
- **Tier Violated**: no-fabrication (an emitter the file authored hidden is translated and spawned as visible)
- **Game Affected**: all NIF-particle games
- **Location**: `crates/nif/src/import/walk/mod.rs:642-668` (`walk_node_flat` particle arm), `crates/nif/src/import/walk/emitter.rs:791-820` (`walk_node_particle_emitters_flat` particle branch)
- **Status**: NEW
- **Description**: Every node arm and geometry arm in both walkers gates on `av.flags & 0x01` (APP_CULLED) — shapes via the #3640 nuance (`!has_live_visibility_controller`, since a live `NiVisController` may un-cull later). Neither particle arm checks the block's own flags or `is_editor_marker`: a `NiParticleSystem` authored APP_CULLED under a visible parent still spawns particles from frame zero.
- **Evidence**: flag reads at `mod.rs:280/370/720/767` and shape arms `:461/:514/:556/:595`; the `downcast_ref::<NiParticleSystem>` arm at `:642` has none.
- **Impact**: Hidden-at-author FX (state-machine-driven emitters that start culled) render from frame zero. A naive fix that drops culled emitters outright would delete emitters a vis controller later unhides — hence the #3640-mirroring condition.
- **Related**: #3640, #222
- **Suggested Fix**: Mirror the shape arms: skip the emitter push only when `ps.av.flags & 0x01 != 0 && !has_live_visibility_controller(scene, ps.av.net.controller_ref)` (plus `is_editor_marker`), in both walkers, with a fixture each.

#### NIFAL-D6-2026-09-21-01: Rename commit missed one `synthesize_packed_havok_proxy` reference
- **Severity**: LOW (doc-rot; introduced today by `4dca737e2`)
- **Dimension**: Collision
- **Tier Violated**: — (doc-rot)
- **Game Affected**: FO4 / FO76 / Starfield (packed-collision proxy path)
- **Location**: `crates/physics/src/convert.rs:239`
- **Status**: NEW
- **Description**: The Havok-brand rename renamed `synthesize_packed_havok_proxy` → `synthesize_packed_collision_proxy` in `cell_loader/spawn.rs` but left the #2543 comment in the Cuboid arm of the canonical→Rapier converter citing the old name (repo's only remaining `packed_havok` hit).
- **Evidence**: `grep -rn "packed_havok" crates byroredux` → exactly one hit.
- **Impact**: Documentation/search only.
- **Related**: NIFAL-D6-2026-09-21-02 (Existing #4408)
- **Suggested Fix**: One-word comment edit.

#### NIFAL-D7-2026-09-21-02: Canonical clip fields are overridden after the `convert_hkx_clip` boundary at two install sites
- **Severity**: LOW (once-at-install mutations, not per-frame re-resolution; drift risk only)
- **Dimension**: Animation / controllers (HKX boundary hygiene)
- **Tier Violated**: single-boundary (softened — policy decided at call sites, not a second construction site; the caller-count detector correctly does not fire)
- **Game Affected**: Skyrim
- **Location**: `byroredux/src/asset_provider/animation.rs:206-212` (walk installer sets `clip.accum_root_name` post-boundary, duplicating the boundary's own COM lookup at `:482-484`) and `:304` (combat installer sets `clip.cycle_type = Clamp` after the boundary chose `Loop`)
- **Status**: NEW
- **Description**: `convert_hkx_clip` owns cycle type and accum-root policy; the walk installer re-implements the COM bone lookup outside the boundary, and the combat installer overwrites `cycle_type` after the boundary's decision. Three places now decide `cycle_type`.
- **Evidence**: `let mut clip = convert_hkx_clip(…); … clip.accum_root_name = Some(pool.intern(&com.name));` (walk); `clip.cycle_type = CycleType::Clamp;` (combat).
- **Impact**: A future field/policy change must be checked against the boundary AND every call-site override; the COM-lookup duplication can drift.
- **Related**: #2305 / NIFAL-D7-NEW-01 (the declared second boundary)
- **Suggested Fix**: Parameterize `convert_hkx_clip` (e.g. `one_shot: bool`, `accum_root: Option<&str>`) so the overrides happen inside the boundary.

#### NIFAL-D7-2026-09-21-03: Code pins the fixture doc's *alternate* attack clip while its comment claims the paths are pinned by that doc
- **Severity**: LOW (fixture/doc drift)
- **Dimension**: Animation / controllers (P2 fixture alignment)
- **Tier Violated**: —
- **Game Affected**: Skyrim
- **Location**: `byroredux/src/asset_provider/animation.rs:236` (`DRAUGR_ATTACK_PATH = …2hmattackforwardb.hkx`) vs `docs/engine/p2-combat-anim-sound-fixture.md` freeze rule (power chop `2gsattackforwardpowerchop2.hkx` is "the primary attack")
- **Status**: NEW
- **Description**: The fixture doc designates the 2GS power chop as the gate's primary attack; the implementation pins the 2HM non-power sibling while its comment says "Asset paths pinned by `docs/engine/p2-combat-anim-sound-fixture.md`". Both decode (84-track hk_2014 clips), so mechanics work; doc and installed family disagree on which take the gate demonstrates.
- **Evidence**: `:227-233` comment + `:236` const, against the fixture doc's freeze rule.
- **Impact**: The P2 gate, when wired per the doc, demonstrates a different attack than installed; code-vs-doc authority is ambiguous.
- **Related**: NIFAL-D7-2026-09-21-01
- **Suggested Fix**: Pin the power chop per the doc, or edit the doc's freeze rule to name `2hmattackforwardb` as the installed primary.

#### NIFAL-D7-2026-09-21-04: Harness doc cites a nonexistent test module `unsanitized_clip_scalar_tests`
- **Severity**: LOW (doc rot)
- **Dimension**: Animation / controllers (guard doc rot)
- **Tier Violated**: —
- **Game Affected**: all
- **Location**: `byroredux/src/anim_convert.rs:1124`
- **Status**: NEW
- **Description**: The `canonical_animation_completeness_harness` module doc says the scalar sanitizers "have their own tests in `unsanitized_clip_scalar_tests` above". No such module exists — the tests live in `clip_frequency_tests`, `clip_phase_tests`, `clip_duration_weight_tests`.
- **Evidence**: `grep -rn unsanitized_clip_scalar_tests byroredux/src` → single hit at `:1124`.
- **Impact**: A reader following the pointer concludes the sanitizers are untested.
- **Related**: #4405 (the harness rework that added the sentence)
- **Suggested Fix**: Repoint to the three real module names.

#### NIFAL-D9-2026-09-21-01: The #4393 PBR-override ceiling guard covers only the two Skyrim variants — the other six games cannot see upward drift
- **Severity**: LOW (harness-coverage / test gap; no live mistranslation found)
- **Dimension**: Completeness
- **Tier Violated**: harness-gap
- **Game Affected**: Oblivion, FO3, FNV, FO4, FO76, Starfield
- **Location**: `crates/nif/tests/translation_completeness.rs:578` and `:617` (the only two `assert_pbr_override_ceiling` call sites)
- **Status**: NEW
- **Description**: #4393's lesson (recorded in `assert_pbr_override_ceiling`'s own doc) is that a floor alone cannot see upward drift — a parser placeholder counted as a classifier signal. The ceiling was added only where #4393 fired (SkyrimLE/SkyrimSE, 97.0). Every other game asserts floors only. The low-fill BGSM-era rows are the most exposed: FO76's floor is 8.0%, Starfield's 1.0% (measured 5.1% today) — a placeholder drifting Starfield's metO to 50% would pass. FO4 measured 99.4% today with no ceiling (legitimate per the union-of-signals analysis, but nothing pins that).
- **Evidence**: ceiling call sites at `:578`/`:617` only; the harness's own doc states the upward-drift rationale (`:236-240`).
- **Impact**: A repeat of #4393 on any non-Skyrim game is invisible to the harness until re-measured by hand.
- **Related**: #4393, #4250, #2707
- **Suggested Fix**: Add a ceiling per game ~10pp above each current measured value (Oblivion/FO3/FNV ≈ 99–100 or documented skip, FO4 ≈ 100, FO76 ≈ 20, Starfield ≈ 10).

## Existing open issues re-checked (unchanged, not re-filed)

- **#4246**: two-phase doc still names `placement_lod.rs` only for the handle-less exemption; `object_lod.rs` is the fourth handle-less caller (doc gap).
- **#4256**: `ImportedMaterial.shader_type` still raw-tier; no typed discriminator on canonical `Material` (documented design).
- **#4264**: texture-only spawner path-resolution doc still incomplete (dated 2026-09-19).
- **#4282 / #4283**: Starfield wetness/luminance sink and the `from_bgsm` provenance overload — untouched this window.
- **#4304**: ground cover carries no canonical `Material` (`groundcover_translate.rs` has zero `Material` references, re-verified).
- **#4408**: stale collision comment located and confirmed — `crates/nif/src/import/collision/mod.rs:17-18` ("the trimesh fallback … produces the actual collider today", superseded by #2355's layer-split proxies).
- **#4409**: nifal.md Particles prose still stale — the "Still pending" bullet lists per-emitter attribution (fixed by #4261 on 2026-09-12), predates the overlay fields, and §2:321/351 still names `apply_emitter_params` as the site-facing boundary where both load paths now call `apply_emitter_overlays` (D9's boundary-name detail folded here).
- **#4410**: nifal.md Skinning prose still stale (spec side); the code-side sibling is D3-01 above.
- **#4411**: source-scan guard class still foolable — concretely re-verified on the collision guard (comment prose counts as an arm; a local-variable constructor pattern escapes the dispatched extractor entirely; the `bhkRigidBodyT` arm at `blocks/mod.rs:1219-1223` already uses that pattern).
- **#4426**: flipbook frames still bypass the per-role resolver (`anim_convert.rs:267` calls plain `resolve_texture`) — authored CLAMP dropped, all frames sRGB, missing frames bind the checker. The unload half is fixed (#4427).
- **#4430**: FO4 slot-2 arm (`slot_role.rs:418-424`) still returns `Emissive` for every non-tint type without consulting `context.glow_map`; BGSM `glowmap` bool still parsed-and-never-read; the `dedicated_shader.rs:337-339` comment still falsely says the bit participates.
- **#4468 / #4469**: open FO3 LOWs — per `audit-fo3/SKILL.md` these are the FO3 `.high.` LOD quads and the INFO `DATA` undecode; `docs/engine/near-term-action-plan.md:109` mislabels them as "Emitter-rate siblings from the FO3 audit" (the FO3 emitter-rate HIGH was #4467, fixed). Minor action-plan doc-rot, not re-filed here.
- **#4546**: walk_anim lock-order — code shape unchanged (Pass 1 still acquires the seven read guards together).
- **#4547**: `p0[oblivion]` contract-SKIP gap — fixture-dispatch scope (playable-smoke game choices omit oblivion), not animation translation; the Oblivion KF walk/idle paths themselves load.
- **Verified FIXED since baseline** (fix holds at HEAD, listed for the next run's dedup): #4391 #4392 (`d776d37e1`) #4393 #4395 #4396 #4397 (`d53be91be`) #4398 #4399 #4400 #4401 #4402 #4403 #4404 #4405 #4406 #4407 #4421 #4422 #4423 #4424 #4425 #4427 #4431 #4432 #4433 #4434 #4467 #4514 #4523 #4529 — plus **#3930**, which this sweep found fixed by `a4cbe36f8` (SkinAttach names primary, count-mismatch declines, solver behind the data) and which the audit skill's "known-open" list still carried.

## Regression pins verified (highlights; full lists in the dimension reports)

- **Boundary signatures/caller sets**: `translate_material` un-widened, 4 production callers + Cornell; `merge_external_material` sole export; `apply_emitter_overlays` exactly 2 callers; `convert_nif_clip` + `convert_hkx_clip` the only two canonical `AnimationClip` producers (zero non-boundary production constructions); `translate_light` 3 ESM producers (centralized in `references/synth_child.rs` by #4514); `resolve_shape_inner` sole bhk→`CollisionShape` site; #4529's `build_material_texture_handles` single-producer pin green.
- **NaN/rotation sanitizers**: #4396/#4397/#4406 hold on the animation side (`normalized_rotation_sample` fires once; skip-not-identity); the static-side gap they left is D2-01.
- **Glass/PBR**: #4392 (`2..=20` authored-carrier dispatch), #4391 (window env-mapping coverage gate), `sanitize_finite` list matches every `Material` f32 field (incl. `detail_neutral`).
- **Texture roles**: all 16 fixed issues verified in code and by green test (incl. #4432 full 25-role bidirectional colour table, #4434 named-role overlay, #4423 tint alpha gate, #4427 flipbook unload walk, #4431 Starfield confinement, #4523 dark role). Zero `if game` in `triangle.frag` + includes.
- **Particles**: #4398 orientation composed once per path (homomorphism argument: convert-then-compose == compose-then-convert), #4404 guards non-vacuous (mutation-replay defeated), #4467 own-chain + `target_ref`, #3754 trapezoid mean-not-peak.
- **Collision**: #4407 plane-shape plain wording + `plane_shapes` census counter; 16/16 dispatch↔resolve arms counted fresh; Havok rename introduced no raw-enum passthrough; ragdoll refactor translates once.
- **Lights**: #4395 column-0 + single directional negation; #4399 morph slots gated on canonical attach; real-data SSE index-space guard **run and passed** on installed Skyrim SE data.
- **Completeness**: fills byte-identical (SkyrimSE) / structurally identical (FO4, Starfield) to recorded baselines; 17 always-on harness guards green.

## Documented-limitation ledger (parked, not leaks — re-confirmed)

- Node/mesh passthroughs: all seven parked node fields parsed and populated on the raw tier, zero canonical consumers (core grep empty); Passthroughs-table rows accurate (furniture markers and BGEM glass roles consumed; `NiTextureEffect` test-only/content-absent; `BSInvMarker` unwalked; `bs_bound` loose-path-only).
- FO4+ NP collision blob (container decoded, `hknpCompressedMeshShapeData` layout not; census-driven Architecture-trimesh / Clutter-Actor-AABB proxies); `BhkPCollisionObject` phantoms (trigger-volume path); decoded-but-not-imported constraint kinds (documented at the `ragdoll.rs` drop sites rather than the `mod.rs` top table — location drift only); ragdoll-template parse-only stubs (#980).
- Particle size-over-life bell (constant `radius × base_scale`); colour-curve interior keys; `initial_color` non-application (both directions pinned); Starfield particle slice N/A (`starfield_corpus_has_no_particle_blocks` green, run this session); FO76 unconfirmed as recorded.
- Animation: per-light `LightAmbient` channels (#983 no-op); embedded controllers' empty `text_keys` (merge path preserves sequence keys); hkx completion-event fabrication (scoped + pinned); `EmissiveMultiple`/`RefractionStrength` captured, no consumer (#3327); morph channels NOT parked (live `AnimatedMorphWeights` + GPU slots since #4399; only the mesh-vertex blend remains, #2221).
- Material: `material_kind: u32` GPU contract; `shader_type_fields`/`effect_falloff` Options are variant conventions with `sanitize_finite` descents; emissive normalization deliberately no-op (Q2 resolved; FNV 10.0 mode / FO4 0.05 mode remain recorded unmeasured consequences); non-fire `Refraction` has no `ior` consumer (#2327); Disney subsurface/sheen/anisotropic zeroed (no source authors them).
- Starfield wetness/luminance (#4282), `.mat`/CDB not a live role source (#3906), tint parked pending M56 (FO4/FO76 tint alpha unmeasured), FaceTint slot 6 unrouted (#2095), tint-family slots 4/5 (#1350/#2999), BGSM distance-field role deferred (#2642), ground cover no canonical `Material` (#4304).
- Starfield tex/nrm 0.0% fill = documented structural zero (#2214 pre-merge tier; harness split coherent).

## Method notes

- **Mode**: orchestrator + 9 concurrent dimension subagents (general-purpose), each read-only, each given the dimension instructions from the audit skill, the local dedup map (gh offline), and the 1.96.0-toolchain invocation per AGENTS.md §Binary-crate toolchain (`-j 4` throughout). Dimension reports: `/tmp/audit/nifal/dim_{1..9}.md`.
- **Tests executed (all green, 1.96.0 toolchain)**: default-lane `material_translate` 66 passed (pre-fan-out); Dim-1 guard lane 37; `byroredux-nif` full lib 1450 (D2) and exit-0 suite (D6); skin 106 / light 106 / bin-light 102 (D3); particle 47 + `import::walk` 57 + starfield-corpus 1 (D5); anim harness 4 + hkx 1 + `anim::` 72 + walk_anim 6 + combat_anim 9 (D7); slot_role/texture lanes 40 + 87 + 131 (D8); billboard/lod/walk/placement lanes 110 (D4); completeness harness re-run on SkyrimSE+FO4+Starfield (0.42 s / 1.30 s, all floors + ceilings green) + 17 always-on guards (D9). The `#[ignore]` real-data SSE index-space guard was run and passed on installed Skyrim SE data.
- **Censuses**: none run fresh this sweep beyond the harness re-runs; D8's #4426/#4430 checks intentionally re-use the 09-16 census tables (per that report's "do not redo the census" record). D5's budget finding cites #4261's existing 67.3% multi-emitter measurement.
- **Dedup**: gh offline; `git log --grep '#NNNN'` fix map + `grep` over `docs/audits/` (~200 reports) + code-comment issue references. Reports before 2026-06-07 carry the pre-`/audit-publish` caveat; none of this sweep's findings rest on one.
- **Not run**: `dark_texture_role_population_matches_the_documented_census` (walks every mesh archive; minutes) — source-level pins green in the normal suite. Oblivion/FO3/FNV/SkyrimLE/FO76 completeness rows not re-measured (2026-08-06/09-14 numbers stand; metadata guards green).
- **Out-of-scope routing**: the committed throwaway census examples `crates/nif/examples/tmp_fo4_d4_{psglod,lodoverlap,lodsize}.rs` are noted a third consecutive time → `/audit-tech-debt`. The action-plan's #4468/#4469 mislabel (D5 re-check) is minor doc-rot in `docs/engine/near-term-action-plan.md`.

**Suggested publish labels** (domain `nifal` on all):
- D2-01: + `safety` + `vulkan` (non-finite AABB → AS-build UB)
- D5-01: + `import-pipeline`; D5-03: + `game:oblivion` et al. only if the census confirms incidence
- D7-01: + `animation` + `game:skyrim` + `combat`
- D1-01: + `tech-debt`; D1-02: + `renderer`; D9-01: + `test-gap`; the doc-rot LOWs: + `doc-rot`

## Next Step

```
/audit-publish docs/audits/AUDIT_NIFAL_2026-09-21.md
```
