# NIFAL Audit — 2026-08-07

**Scope**: NIFAL (NIF Abstraction Layer) — the canonical translation tier.
Spec: [`docs/engine/nifal.md`](../engine/nifal.md). Predecessor spec:
[`docs/engine/material-abstraction.md`](../engine/material-abstraction.md).

**Repo HEAD**: `79bfc76e`. **Prior sweep baseline**: `1ae86f62`
([`AUDIT_NIFAL_2026-08-03.md`](AUDIT_NIFAL_2026-08-03.md)), 65 commits of
delta. All 9 dimensions were run as parallel sub-agents, each re-tracing the
in-scope delta against current source (not commit titles), re-verifying every
carried-forward finding against live code, and cross-checking
`gh issue list` (94 open issues fetched to `/tmp/audit/issues.json`) before
filing anything as NEW.

Two commits landed in this window with outsized NIFAL relevance and received
extra scrutiny across Dimensions 4, 6, and 9: `8ee151e0` ("add collision
authoring summary and integrate into NIF import process") and `716b7ee9`
("improve packed collision compatibility").

## Executive Summary

| Category | Status | Boundary |
|---|---|---|
| Material | **converged** | `translate_material` (`byroredux/src/material_translate.rs`) |
| Geometry / Transform | **converged** (cleanest category) | import-time Z-up→Y-up + single destrip (`crates/nif/src/blocks/strip.rs`) |
| Skinning | **converged** | `ImportedSkin` global-bone-index remap (`crates/nif/src/import/mesh/skin.rs`) |
| Lights | **regressed on one load path** (new finding, see NIFAL-D3-NEW-01) | `LightKind` resolution (`crates/nif/src/import/walk/mod.rs`) — reachable only via `import_nif_lights`, which the loose-NIF loader never calls |
| Nodes | **triaged by design** (no single boundary; 7 fields parked, 0 consumers, re-verified) | N/A — two structurally different consumption shapes, documented |
| Particles | **converged** | `apply_emitter_overlays` (`byroredux/src/systems/particle.rs`) — now *more* single than last cycle (NIFAL-D5-01 closed) |
| Collision | **converged, with one fresh gap in the new compatibility-proxy path** (NIFAL-D6-NEW-01) | `resolve_shape_inner` (`crates/nif/src/import/collision/shape.rs`) + new `CollisionAuthoringSummary` fallback (`crates/nif/src/import/collision/mod.rs`) |
| Animation / controllers | **converged** | `convert_nif_clip` (NIF) + declared 2nd boundary `convert_hkx_clip` (Havok packfile) |
| Shader flags / texture sets | **converged** | per-block-type dispatch at parse; `MaterialTextureSet<T>` 18-role vocabulary, `values()`/struct-field parity re-verified programmatically, zero drift |
| Completeness signal | **partial** — Material now has a real canonical-tier harness; the other ~4 declared boundaries still rely on manual audit tracing (NIFAL-D9-04) | `translation_completeness.rs` (raw tier) + `canonical_completeness_harness` (`material_translate.rs`, canonical tier, Material-only) |

**New findings this cycle**: 1 HIGH, 2 MEDIUM, 1 LOW (4 total).
**Carried-forward findings verified FIXED and closed out**: 9
(MAT-D1-NEW-04, NIFAL-D2-01, NIFAL-D3-02, NIFAL-D4-03, NIFAL-D5-01,
NIFAL-D7-03, NIFAL-D7-NEW-01, NIFAL-D8-01, NIFAL-D8-02, NIFAL-D9-03 — plus 4
Collision issues fixed as a side effect of the two new commits: #2355,
#2332, #2333, #2339).
**Carried-forward findings confirmed still open, unchanged**: MAT-D1-NEW-01
(#2296), MAT-D1-NEW-02 (#2297), NIFAL-D7-02 (#2303), #2330 (SKY-D7-03).

No tier-invariant violation was found in Material, Geometry/Transform,
Skinning, Particles (post-fix), Animation, or Shader-flags this cycle — the
delta in those categories was uniformly fix-quality or neutral refactors.
The two genuine new gaps (Lights, Collision) both trace to the same shape of
mistake: a boundary function exists and is correct, but a *caller* on one of
the two production load paths either never invokes it (Lights) or feeds it
data from the wrong pose/space (Collision's new proxy).

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|
| Material | PASS — `translate_material` sole site (2 callers only) | PASS — emissive scale no-op re-confirmed measured, not invented | PASS — no `metalness_override`/`roughness_override`, clamps verified | PASS — no per-draw `classify_pbr`/glass fallback |
| Geometry/Transform | PASS — destrip unified to one fn (`blocks/strip.rs`), closing NIFAL-D2-01 | PASS — no new magic numbers | PASS — no `Option`-gated geometry field reaches `MeshRegistry::upload` | PASS — bound derived once at extraction, diagnostic-only cross-check added, not a recompute |
| Skinning | PASS — `ImportedSkin` global remap, one path | N/A | PASS — no consumer re-derives partition layout | N/A |
| Lights | PASS (boundary itself fine) | N/A | **FAIL on loose-NIF path** — `import_nif_lights` never called → not a leak of a bad value but a leak of *absence* (NIFAL-D3-NEW-01) | PASS — renderer still matches only on `LightKind`, zero source-block-type hits |
| Nodes | N/A by design (documented) | N/A | PASS — 7 parked fields re-verified at 0 canonical consumers, 7/7 | N/A |
| Particles | PASS — `apply_emitter_overlays` now folds `texture_path`/`src_blend`/`dst_blend` too (NIFAL-D5-01 closed) | PASS — `initial_color` still unapplied, size-over-life still undocumented-as-future-work only | PASS | PASS — force fields converted once at overlay time |
| Collision | PASS — 16 shape arms unchanged; `CollisionAuthoringSummary` crosses the boundary as 3 plain `u32`s only | PASS — new compatibility proxy's `Keyframed`/conservative-cuboid choices are documented, not invented | **PARTIAL** — packed-Havok proxy consumes bind-pose vertex positions for skinned meshes with no skin-aware guard (NIFAL-D6-NEW-01) | PASS — proxy decision made once at spawn time from the summary, not per-draw; renderer-free (no `MeshHandle`) confirmed |
| Animation | PASS — 2 declared boundaries (NIF + Havok packfile), both target the one canonical `AnimationClip` | PASS — the one `convert_hkx_clip` text-key fabrication is documented and now has a real consumer | PASS | N/A |
| Shader-flags/textures | PASS — per-block-type dispatch, `triangle.frag` + `include/*.glsl` re-grepped, zero `if game ==` | PASS | PASS — `MaterialTextureSet` role vocabulary diffed programmatically against `values()`, zero drift; `smooth_spec`/`specular` and `environment`/`environment_mask` confirmed distinct | PASS |
| Completeness | — | — | — | Harness gap: canonical-tier signal covers Material only (NIFAL-D9-04); raw-tier harness floors re-tightened and verified live against all 7 game corpora |

## Findings

### HIGH

#### NIFAL-D3-NEW-01: Loose-NIF load path never extracts or spawns any of a mesh's authored lights
- **Severity**: HIGH
- **Dimension**: Skinning/Lights (Lights) · **Tier Violated**: single-boundary / no-fabrication (the extraction call is *absent* on one of the two production load paths, not a bad translation of present data)
- **Game Affected**: All (Oblivion → Starfield) — every loose-loaded NIF carrying an embedded `NiPointLight` / `NiSpotLight` / `NiAmbientLight` / `NiDirectionalLight`
- **Location**: `byroredux/src/scene/nif_loader.rs` (entire file, 1165 lines — `parse_import_and_merge` / `load_nif_bytes_with_skeleton`)
- **Status**: NEW — `gh issue list` search for "light"/"nif_loader" found only closed `#156`, which added the extraction+spawn path used by the **cell loader** only, not this one. Not a duplicate.
- **Description**: `byroredux_nif::import::import_nif_lights` — the sole function that walks a parsed `NifScene` and produces `Vec<ImportedLight>` — has exactly three call sites in the whole tree: `crates/nif/examples/import_probe.rs:47` (debug example), `byroredux/src/streaming.rs:895` (exterior grid pre-parse), and `byroredux/src/cell_loader/references/import.rs:116` (cell-loader ref import). `byroredux/src/scene/nif_loader.rs` — the module backing `cargo run -- path/to/mesh.nif` (documented in `CLAUDE.md`'s Quick Reference/Usage as a primary invocation, and the cache path behind *all* skeleton/body/hand NPC-part loading) — calls neither `import_nif_lights` nor a light-populating path. `grep -in light byroredux/src/scene/nif_loader.rs` returns zero matches across the full file, and `world.insert(entity, LightSource ...)` never appears in it — the only `LightSource` insertion site in the whole repo is `byroredux/src/cell_loader/spawn.rs:779`, unreachable from the loose loader.
- **Evidence**:
  ```
  $ grep -rn "import_nif_lights\b" --include='*.rs' crates/nif byroredux
  crates/nif/src/import/mod.rs:483:pub fn import_nif_lights(scene: &NifScene) -> Vec<ImportedLight> {
  crates/nif/examples/import_probe.rs:47:    let lights = byroredux_nif::import::import_nif_lights(&scene);
  byroredux/src/streaming.rs:895:        let lights = byroredux_nif::import::import_nif_lights(&scene);
  byroredux/src/cell_loader/references/import.rs:116:    let lights = byroredux_nif::import::import_nif_lights(&scene);

  $ grep -in "light" byroredux/src/scene/nif_loader.rs
  (no output)
  ```
- **Impact**: A torch, candle, lantern, or streetlamp NIF loaded standalone (`cargo run -- <mesh>.nif`) renders its flame/bulb geometry but contributes zero light to the scene — visible content loss, not cosmetic. Since `load_nif_bytes_with_skeleton`'s cache path backs *every* skeleton/body/hand NPC-part load (not just the standalone entry point, per that function's own doc comment), the blast radius extends to normal cell-loaded NPC rendering wherever NPC-part NIFs carry lights, though the most directly observable case is the documented loose-load workflow.
- **Related**: Sibling gap to closed `#156` (which fixed the cell-loader path only). Not a duplicate of any open issue.
- **Suggested Fix**: Call `byroredux_nif::import::import_nif_lights(&scene)` in `parse_import_and_merge`, store the result on the loader's cache-entry struct, and add a light-spawn loop in `load_nif_bytes_with_skeleton` mirroring `cell_loader/spawn.rs::spawn_nif_lights` — widen `is_spawnable_nif_light`/`light_radius_or_default` to `pub(crate)`-shared (they already are `pub(crate)` in `spawn.rs`) or lift them to a shared helper rather than re-deriving the sanitization logic a third time.

### MEDIUM

#### NIFAL-D6-NEW-01: `synthesize_packed_havok_proxy` unions skinned-mesh bind-pose geometry into the compatibility AABB, unlike its Architecture-trimesh sibling
- **Severity**: MEDIUM
- **Dimension**: Collision · **Tier Violated**: NIFAL translate boundary (canonical-fallback tier — the compatibility-proxy consumer introduced this cycle by `716b7ee9`/`8ee151e0`, not the raw/translate tiers proper)
- **Game Affected**: FO4 / FO76 / Starfield — any `RenderLayer::Actor` (CREA — creature) or `RenderLayer::Clutter` placement with packed (`BhkNPCollisionObject`) collision authoring and a skinned render mesh
- **Location**: `byroredux/src/cell_loader/spawn.rs:118-135` (`synthesize_packed_havok_proxy`'s mesh filter), contrasted with `byroredux/src/cell_loader/spawn.rs:1680-1687` (the sibling `ArchitectureTriMesh` gate, which requires `mesh.skin.is_none()`)
- **Status**: NEW — brand-new code path (landed `8ee151e0`, this delta window). Not a duplicate; sibling of the same-cycle-fixed `#2355` (that issue was "no proxy at all" for Clutter/Actor; this is "proxy built from the wrong pose data" once the sibling fix landed).
- **Description**: The Architecture trimesh fallback (`synthesize_static_trimesh`) explicitly excludes skinned meshes (`mesh.skin.is_none()`, "never synthesize for animated bodies"). `synthesize_packed_havok_proxy` has no equivalent check:
  ```rust
  let geometry = meshes
      .iter()
      .filter(|mesh| {
          !mesh.material.is_decal
              && !mesh.material.alpha_test
              && mesh.material.material_kind
                  != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION
              && !mesh.positions.is_empty()
      })
      .map(|mesh| ProxyMeshGeometry { positions: &mesh.positions, ... });
  ```
  `mesh.positions` on a skinned `ImportedMesh` is bind-pose (T-pose/rest-pose) local geometry — the same array GPU skinning deforms at render time, not a runtime-posed shape. Creature (CREA) REFRs on FO4+/FO76/Starfield reach `spawn_placed_instances` through the generic REFR path (`npcs: &HashMap<u32, NpcRecord>` is keyed by NPC_ only — CREA is absent, so it falls through to `spawn_synth_child` → `spawn_placed_instances` with `base_layer = RenderLayer::Actor`), so a creature whose model is a skinned mesh and whose NIF authors only packed Havok gets its collision cuboid built from bind-pose vertex positions.
- **Evidence**: No test in either commit constructs an `ImportedMesh` with `skin: Some(...)` through this path — both new tests (`packed_proxy_bakes_outer_scale_into_cuboid_extent`, `packed_proxy_is_keyframed_and_parented_to_visual_placement`) use `ImportedMesh::from_geometry(...)`, which defaults `skin: None`. The gap is untested as well as unguarded.
- **Impact**: A bind-pose T-pose skeleton for many creature/character rigs has limbs splayed far wider than the resting silhouette, so the resulting `Cuboid` half-extents can be substantially oversized relative to the creature's visible footprint — an invisible collision block extending well beyond the rendered model, obstructing movement in open space around the creature. The proxy is `Keyframed` and parented to `placement_root` (not any bone), so it never reflects animated posture — the mis-sizing is permanent for the creature's lifetime, not a spawn-frame transient. Scoped to skinned creature/actor content on FO4+/FO76/Starfield, exactly the population most likely to lack decoded classic collision.
- **Related**: Sibling of fixed `#2355`.
- **Suggested Fix**: Either (a) add a `mesh.skin.is_none()` filter to the closure, matching the Architecture precedent, and fall back to each mesh's already-computed `local_bound_center`/`local_bound_radius` (pose-independent, mesh-local) for skinned submeshes instead of dropping the creature to "unresolved"; or (b) use the authored `local_bound_center`/`local_bound_radius` directly for skinned submeshes rather than raw bind-pose vertex positions — preserves the "conservative coarse box" intent without trusting bind-pose extremities as representative.

#### NIFAL-D9-04: The new canonical-tier completeness harness covers 1 of ~5 declared translate boundaries — the six bugs that motivated it were all outside Material
- **Severity**: MEDIUM
- **Dimension**: Completeness · **Tier Violated**: (harness gap — no production tier violated; same classification `#2213`/`#2214` used pre-fix)
- **Game Affected**: all seven (harness-coverage gap, not a per-game data bug)
- **Location**: `byroredux/src/material_translate.rs:571-574` (the scoping comment); no equivalent kitchen-sink module exists for `crates/nif/src/import/collision/shape.rs::resolve_shape_inner`, `byroredux/src/anim_convert.rs::convert_nif_clip` (+ `byroredux/src/asset_provider/animation.rs::convert_hkx_clip`), `byroredux/src/systems/particle.rs::apply_emitter_overlays`, or `crates/nif/src/import/walk/mod.rs`'s `LightKind` resolution
- **Status**: NEW (successor to `#2214`'s residual scope; `#2214` itself is now closed as Material-scoped, verified genuinely functional by an independent revert-and-fail test)
- **Description**: The six translate-boundary bugs the 2026-07-27 sweep found and cited as evidence the harness was needed were NIFAL-D6-01, D6-02, D3-01, D4-02, D6-03, D6-04 — four in Collision, one in Lights, one in Nodes. **Zero were in Material.** The new kitchen-sink harness added by `#2214` (`byroredux/src/material_translate.rs:547-798`, `mod canonical_completeness_harness`) is scoped to Material only; its own doc comment says "collision/animation have no `translate_*` boundary yet to extend it to" — but per `docs/engine/nifal.md` itself, both categories *do* have declared, named, "converged"/"audited" boundaries. What's missing for those categories is not the boundary but a kitchen-sink canonical-output completeness test of the kind `#2214` just wrote for Material; the harness's scoping comment understates what already exists.
- **Evidence**: `grep -rln "kitchen_sink" crates/nif/src byroredux/src` returns only `byroredux/src/material_translate.rs`. The four fixed Collision bugs and the Lights bug were all caught by manual code tracing in the 2026-07-27/08-03 sweeps — no automated harness existed for those categories then, and none exists now. Collision's own dimension independently caught and fixed two further boundary bugs this delta (`#2285`/NIFAL-D6-07, `#2298` triple-duplicated destrip logic) — again by manual trace, supporting evidence for this finding.
- **Impact**: The completeness *signal* is real for one of ~9 NIFAL categories. "Dimension 9 passes" cannot be read as "the translation layer's output is regression-tested" beyond Material — the other categories still depend entirely on manual audit sweeps catching drift.
- **Related**: Successor/residual scope of closed `#2214` (NIFAL-D9-02).
- **Suggested Fix**: Extend the `canonical_completeness_harness` pattern in priority order: Collision (`resolve_shape_inner` — highest historical bug count, 4/6), Lights (`LightKind` resolution — 1/6), Animation (`convert_nif_clip`/`convert_hkx_clip`). Also correct the scoping comment at `material_translate.rs:571-574` regardless of extension timing — it currently reads as though Collision/Animation have no boundary at all, contradicting `nifal.md`'s own "converged"/"audited" verdicts.

### LOW

#### NIFAL-D8-NEW-01: BGEM v21+/v22 glass-overlay texture paths have no `MaterialTextureSet` role — undocumented in nifal.md's texture-roles section
- **Severity**: LOW
- **Dimension**: Shader-flags/Effects · **Tier Violated**: no-leak (doc-completeness only — the code-level gap is already deliberately deferred, not a live bug)
- **Game Affected**: FO76/Starfield-era BGEM content (mod-added; `bgem_uses_glass_behavior` gate)
- **Location**: `byroredux/src/asset_provider/material.rs:1271-1282`; `crates/bgsm/src/bgem.rs:32-43`; missing from `docs/engine/nifal.md`'s "Shader flags / texture sets / effect shaders" section
- **Status**: Existing: **#2109** (CLOSED, code-comment-documented) — this finding is narrower: the code-site comment is accurate and complete, but `nifal.md`'s dedicated deferred/parked-passthrough tables have no entry for it, even though two of the six BGEM fields (`glass_roughness_scratch`, `glass_dirt_overlay`) are texture paths, not scalars, and belong conceptually next to the `MaterialTextureSet<T>` role inventory this dimension audits.
- **Description**: `BGEM` (v21+/v22) decodes `glass_fresnel_color`, `glass_refraction_scale_base`, `glass_blur_scale_base`, `glass_blur_scale_factor`, `glass_roughness_scratch` (String texture path), `glass_dirt_overlay` (String texture path), and `environment_mapping_mask_scale`. All six decode correctly but none reach `ImportedMesh`/`ImportedMaterial`/`MaterialTextureSet<T>` — no 19th/20th named role exists for them the way `tint`/`inner_layer`/`reflectance` were added in the 2026-07-27 unification. The asset-provider comment is honest about this being deferred, but the spec doc's own texture-role inventory doesn't mention the gap.
- **Impact**: None beyond documentation completeness — intentionally deferred per `#2109`'s own resolution, reachability on real content already flagged as low/unmeasured there.
- **Suggested Fix**: Add a one-line entry to `nifal.md`'s texture-roles/Passthroughs table naming `glass_roughness_scratch`/`glass_dirt_overlay` as parsed-but-unrouted BGEM texture paths, blocked on a renderer glass-overlay consumer — mirroring the existing `bs_lod_cutoffs`/`BSInvMarker` table-row format. Doc-only.

## Carried-forward findings confirmed still OPEN (re-verified this cycle, no regression, not re-filed)

| ID | Dimension | Issue | Summary |
|---|---|---|---|
| MAT-D1-NEW-01 | Material | #2296 | No cross-crate assert pins the NIF importer's `material_kind` 101/102/103 literals to `byroredux_renderer::MATERIAL_KIND_*`; a future renumber would keep `cargo test -p byroredux-nif` green while silently misrouting effect/no-lighting/fire-refraction surfaces. |
| MAT-D1-NEW-02 | Material | #2297 | `draw_command_eligible_for_tlas` excludes `MATERIAL_KIND_EFFECT_SHADER` from the TLAS but not `MATERIAL_KIND_FIRE_REFRACTION`, despite its own constant doc and the sibling shadow-mask gate requiring the same exclusion. No live defect — defense-in-depth gap only. |
| NIFAL-D7-02 | Animation | #2303 | `nifal.md:253-254` still describes per-light ambient colour channels and morph-weight channels as symmetrically "parked... no renderer consumer yet." Ambient genuinely has zero ECS presence; morph-weight has a live, per-frame-updated `AnimatedMorphWeights` ECS sink (since `a8b0cf64`) and lacks only a GPU/mesh-vertex-blend consumer (tracked separately by `#2221`). The doc conflates two different states. |
| #2330 (SKY-D7-03) | Material | #2330 | Both spawn paths call `resolve_normal_alpha_spec_roughness` after texture-handle attachment — a two-phase-boundary documentation-precision gap, not a defect (idempotent, NaN-guarded). |

## Carried-forward findings verified FIXED this cycle (closed out, do not re-report)

| ID | Fixed by | Dimension | What changed |
|---|---|---|---|
| MAT-D1-NEW-04 | #2284 (`95e77897`) | Material | 6 `BSLightingShaderProperty` shading scalars (`lighting_effect_1/2`, `subsurface_rolloff`, `rimlight_power`, `backlight_power`, `fresnel_power`) now land on canonical `Material`. |
| MAT-D1-NEW-03 | #2232/#2239 (`4279c195`) | Material | `Material::ior`'s triple-discriminated meaning documented at both `bindings.glsl` and `SurfaceBehavior::ior`. |
| NIFAL-D2-01 | #2298 (`342ef84e`) | Geometry/Transform | Destrip winding logic unified into `crates/nif/src/blocks/strip.rs::destrip`; all three prior hand-copies (classic tri-strips, `NiSkinPartition`, `resolve_compressed_mesh`) now delegate to it. |
| NIFAL-D3-02 | #2210 (`7dacef90`) | Lights | Uncited `2048.0` no-attenuation light-radius fallback replaced with cited `EXTERIOR_CELL_UNITS` (4096.0), matching the cell-loader's sibling fallback. |
| NIFAL-D4-03 | #2299 (`342ef84e`) | Nodes | `nifal.md`'s passthrough table row split: `BSFurnitureMarker` correctly marked consumed (since #2010/M41.5 Phase B); `BSInvMarker` correctly remains "parsed, not walked." |
| NIFAL-D5-01 | #2300 (`342ef84e`) | Particles | `texture_path`/`src_blend`/`dst_blend` overrides folded into `apply_emitter_overlays` itself, removing the last duplicated inline overlay block at both spawn sites. |
| NIFAL-D7-03 | #2304 (`66f0775e`) | Animation | `operation`→`FloatTarget` / `target_color`→`ColorTarget` discriminator tables unified into `color_target_from_target_color`/`float_target_from_operation`, called from both the KF and embedded arms. |
| NIFAL-D7-NEW-01 | #2305 (`66f0775e`) | Animation | `nifal.md` now names `convert_hkx_clip` as a second declared boundary, with its one documented text-key fabrication exception; that fabrication now has a live consumer (`cinematic_animation_event_system`). |
| NIFAL-D8-01 | #2212 (in-window) | Shader-flags | Authored BGSM `alpha_test_ref` now always wins over the NIF-flag-seeded 128/255 default via a chain-local `set_alpha_test` sentinel. |
| NIFAL-D8-02 | #2306 (`66f0775e`) | Shader-flags | `nifal.md` no longer cites the deleted `ShaderFlags<'a>` typed view (removed by #1897) and correctly calls the bit-collision guards `#[test]`-gated runtime asserts. |
| NIFAL-D9-03 | #2307 (`66f0775e`) | Completeness | Fill-rate floors tightened to ~10-15pp below re-measured values; `metO`/`rghO ≥ 99.9%` floors added for all 7 games; `normal_map` floors added where measured non-zero, with cited reasons for the two deliberate omissions (Oblivion, Starfield). Re-run live against all 7 game corpora, all floors pass. |
| #2355 (SF-D8-04) | `8ee151e0` | Collision | "NIFAL collision slice never fires on Starfield" — fixed by `CollisionAuthoringSummary` + `missing_collision_fallback` + `spawn_packed_havok_proxy`. Clutter/Actor now get a conservative proxy; Architecture keeps the precise trimesh path; mutually exclusive by branch order. |
| #2332 (FO3-D5-02) | `716b7ee9` | Collision | `bhkSPCollisionObject` now dispatches through `BhkPCollisionObject::parse` instead of the shared classic-rigid-body arm; pinned by a dedicated dispatch test. |
| #2333 (FO3-D5-03) | `8ee151e0` | Collision | `CollisionAuthoring`/`examine_collision_kind` now have real callers via `summarize_collision_authoring`, read at all five `CachedNifImport` construction sites. |
| #2339 (FNV-D7-04) | `716b7ee9` | Collision | All four silent-drop sites in `extract_ragdoll` (`ragdoll.rs`) now log with block index/bone name/offending value; guarded by a correctly-scoped `has_constraint_authoring` early-out. |

## Documented-limitation ledger (parked-not-leak / no-action — do not re-report next sweep)

- **Node/mesh parked fields** — all 7 (`bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`) re-measured at zero canonical consumers this cycle, 7/7. `bs_lod_cutoffs`'s *source* coverage widened this delta (`#2283` threads Skyrim `NiLodTriShape` in addition to the pre-existing FO4 `BSMeshLODTriShape` path) but its consumer count is unchanged — still a raw-tier-only field.
- **`NiTextureEffect`**: dead extractor, content-absent, unchanged. **`NiSwitchNode` identity**: walked via active-index only, no discriminator surfaced. **`bs_bound`**: loose-path-only, unchanged.
- **Collision documented limitations**: `BhkPlaneShape` → `None` (`#1334`, documented at its arm — the one deliberate shape-resolution exception). `BhkNPCollisionObject` (FO4/FO76/Starfield `BhkSystemBinary` blob): fallback is now layer-aware rather than blind — Architecture keeps `synthesize_static_trimesh`; Clutter/Actor get the conservative `Cuboid` proxy when `CollisionAuthoringSummary.needs_packed_havok_fallback()` — this is the intended effect of this cycle's two commits, consistently documented across `docs/engine/physal.md`, `docs/engine/physics.md`, and `ROADMAP.md`. `BhkPCollisionObject`/`BhkSimpleShapePhantom`/`BhkAabbPhantom` phantoms still need a dedicated `TriggerVolume` ECS path (now additionally correctly covering the FO3 DLC `bhkSPCollisionObject` variant post-`#2332`). `hkMotionType` byte→canonical `MotionType` collapse re-verified correct and untouched. NIFAL-D6-08 (`NiTriStripsData.normals` not cross-checked by `resolve_tri_strips_data_refs`, #2302) unaffected by either commit, still parked-not-leak.
- **Particles**: `initial_color` intentionally unapplied; size-over-life curve documented future work; multi-emitter scene-first attribution is `#1402`, closed as a documented deferral — re-confirmed still present (`extract_first_color_curve`/`extract_emitter_params`/`extract_emitter_rate` do whole-scene-first matching; `extract_particle_material`, the source #2300 folded in, is correctly per-emitter-scoped).
- **Animation**: per-light ambient colour channels genuinely parked (zero ECS presence — not just missing a render consumer, unlike morph-weight); `AnimationTextKeyEvents` now has a live consumer (`cinematic_animation_event_system`, post-dates the 2026-08-03 baseline) — the historical ledger note about "no system reads the labels" is stale but that staleness lives only in a dated report file, not in `nifal.md` itself, so no doc fix needed.
- **Material / shader flags**: emissive scale is a measured no-op (`nifal.md` §4), untouched this cycle; glass classified once, alpha-aware, correctly ordered after PBR resolve; `material_kind: u32` deliberately kept as the GPU dispatch contract; `smooth_spec` vs `specular` and `environment` vs `environment_mask` re-confirmed distinct, never conflated; `MaterialTextureSet::values()` vs struct-field-list diffed programmatically this cycle — zero drift.
- **Skinning**: `body_part_flags` parked, zero consumers, unchanged.
- **Shader-flags texture-role unification** (2026-07-27 `1d94eb24`/`05d68926`): re-verified clean this cycle with a mechanical diff, not spot-check — 18/18 roles present in both `values()` and the struct field list.
- **Pre-existing open issues confirmed still accurate, not regressed, not duplicated**: `#2320` (FO3-D1-04, `BSShaderPPLightingProperty.shader_type` parsed and never read), `#2331` (FO3-D2-04, falloff-absent default disagreement, unreachable on retail bsver-34 content), `#2334` (FO3-D5-04, FO3 DLC collision baselines, still open, untouched by this cycle's commits).

## Method note

All 9 dimensions ran as parallel sub-agents against live source at HEAD
`79bfc76e`, each independently re-tracing its assigned delta
(`1ae86f62..HEAD`) via `git log`/`git show`, re-verifying every
carried-forward finding by direct code read rather than trusting prior
commit-message titles, and running targeted `cargo test` (960/960 passing on
`byroredux-nif --lib`; Dimension 9 additionally ran both the raw-tier and new
canonical-tier completeness harnesses live against all 7 present game
corpora, and independently confirmed the canonical harness's drop-detection
claim by reverting one field-copy line, observing the expected test failure,
and restoring it — `git status --porcelain` clean afterward). Dimension 6
independently re-verified all four Collision issues the same-day
`AUDIT_LEGACY_COMPAT_2026-08-07.md` claimed fixed, rather than trusting that
report. This orchestrator cross-checked every dimension's written scratch
file (`/tmp/audit/nifal/dim_*.md`) against the coordinator's relayed summary
before compiling this report — all claims matched the underlying evidence.
