---
description: "Deep audit of NIFAL — the NIF Abstraction Layer (canonical translation tier): Imported* → translate() → Canonical, single-boundary / no-fabrication / no-render-time-fallback"
argument-hint: "--focus <dimensions> --game <fnv|fo3|skyrim|oblivion|fo4|fo76|starfield>"
---

# NIFAL Audit — Canonical Translation Layer

Audit **NIFAL** (spec: `docs/engine/nifal.md`): the discipline that every per-game NIF data
category is folded into one game-agnostic representation through a single explicit
`translate()` boundary — no `Option` "resolve-later" leaks, no render-time heuristics.

```
  NIF bytes ──parse──▶ Imported* ──translate()──▶ Canonical ──consume──▶ ECS / GPU
           (per-game, raw, messy)  (one site per category)  (the ECS component that already
                                                             serves the role; no third type)
```

**Canonical-type rule** (spec §1): where an ECS component already serves the game-agnostic
role, that component IS the canonical type. Do NOT flag the absence of a third `Canonical*`
struct.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` (it has
dedicated NIFAL rows: wrong `translate_material` output = HIGH; translatable block silently
dropped = MEDIUM, HIGH if it removes visible content). Do not re-derive severities.

**Scope vs `/audit-nif`**: NIF owns the *parse* side (bytes read wrong). NIFAL owns the
*translate* side: bytes read fine but data dropped, duplicated, or resolved per-game
downstream. **Not here**: mesh-water and env translation (`water_material_from_mesh`,
`attach_mesh_water` in `material_translate.rs`, `env_translate.rs`) → `/audit-exterior`;
`GpuMaterial` layout/shader contract → `/audit-renderer`; `Material` save shape →
`/audit-save`; BGSM/BGEM/CDB *parse* → `/audit-parsers` (the merge boundary is Dim 8 here).

## The four tier invariants (every dimension is a lens on these)

- **single-boundary** — exactly one `translate()` site per category; a second construction
  site filling a canonical type field-by-field is a violation (caller count is the detector).
- **no-fabrication** — no invented value or guessed normalization; a new constant must cite a
  measurement or source (*feedback_no_guessing*). Canonical "measured, then deliberately NOT
  normalized": emissive scale (Dim 1), particle colour/size-curve deferrals (Dim 5).
- **no-leak** — no `Option` "resolve-later" field or raw enum/block-type discriminator on a
  *canonical* type reaches a consumer that re-resolves it (raw-tier `Imported*` may carry them).
- **no-render-time-fallback** — no classification deferred to a per-draw heuristic (the deleted
  `classify_pbr` / render-side glass heuristics; re-introduction is a regression).

Risk order: (1) wrong/divergent canonical `Material` from `translate_material` (all games, no
fallback to mask it); (2) a translatable block silently dropped; (3) a render-time fallback;
(4) a second construction site.

**Guard confidence is limited.** The text-scan / kitchen-sink completeness guards below all
have known holes (open as of 2026-09-19: #4404 particles, #4405 animation, #4411 material —
comment prose or substrings count as pins). A green guard is not proof a field is asserted:
open the test and confirm a real assertion exists for the field you are auditing.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: comma-separated numbers (e.g. `1,8`). Default: all 9.
- `--game <name>`: `fnv`, `fo3`, `skyrim`, `oblivion`, `fo4`, `fo76`, `starfield`. Default: all detected.

## Extra Per-Finding Fields

- **Dimension**: Material | Geometry/Transform | Skinning/Lights | Nodes | Particles | Collision | Animation | Shader-flags/Effects | Completeness
- **Tier Violated**: `single-boundary` | `no-fabrication` | `no-leak` | `no-render-time-fallback` | `parked-not-leak` (verify a "deferred" field is genuinely unconsumed) | `harness-gap`
- **Game Affected**: variant(s) where the divergence manifests

## Phase 1: Setup

1. Parse `$ARGUMENTS`; `mkdir -p /tmp/audit/nifal`.
2. `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`;
   scan `docs/audits/` for `AUDIT_NIFAL_*` / `AUDIT_NIF_*`. NIFAL has many open findings —
   dedup carefully (`gh issue list --search "NIFAL in:title"`).
3. Read `docs/engine/nifal.md` (§2 per-category inventory; its Skinning and Particles prose
   is known-stale, #4410/#4409) and `docs/engine/material-abstraction.md` (Leak A/B are
   recorded closed — do not re-report).
4. `cargo test -p byroredux-nif -p byroredux material_translate` for the default-lane guards.

## Phase 2: Dimensions

### Dimension 1: Material — the reference realisation
Paths: `byroredux/src/material_translate.rs`, `crates/core/src/ecs/components/material.rs`, `byroredux/src/helpers.rs`, `byroredux/src/cell_loader/{spawn/mesh_instance,placement_lod,object_lod}.rs`, `byroredux/src/scene/nif_loader.rs`
First step: `git diff <last-report>..HEAD -- crates/core/src/ecs/components/material.rs byroredux/src/material_translate.rs | grep '^[+-].*pub '`
**The boundary**: `translate_material(source: &ImportedMaterial, mesh_name: Option<&str>, paths: ResolvedPaths, extra_material_flags: u32) -> Material` — the only raw-material → `Material` producer (calls `Material::resolve_pbr()` then `helpers::classify_glass_into_material`). Its narrow input type is what makes "material translation cannot depend on geometry" checkable: **widening it back to `&ImportedMesh` is a regression** even if nothing misuses it yet. Callers must all route through it: loose path `nif_loader.rs`, cell path `spawn/mesh_instance.rs`, LOD paths `placement_lod.rs` + `object_lod.rs`; `translate_texture_only_material` serves draws with no NIF material (terrain, water, `.btr`) — each such spawner still has to get a `Material`.
**Guards** (`byroredux` crate tests): `every_exterior_spawner_inserts_a_boundary_material` (scans `SPAWNER_ROOTS` for `, MeshHandle(` inserts; file granularity only), `both_spawn_sites_derive_markers_through_this_boundary`, `translate_material_copies_every_canonical_field` + `every_source_derived_material_field_is_pinned_by_a_test` (kitchen sink; known hole #4411), `nif_importer_material_kind_literals_match_renderer_constants` — all in `material_translate.rs`; `workspace_hygiene_tests::no_source_file_frames_the_deleted_classify_pbr_as_live` (docs only — nothing forbids a render-time classifier in code).
**Checklist** (what the guards cannot see):
- Only `translate_material` builds a `Material` from import data (a second field-by-field literal is `single-boundary`). Phase-2 path resolution for texture-only spawners is documented incompletely (#4264/#4246 open — known, dated 2026-09-19).
- `Material.metalness` / `roughness` are plain `f32`, clamped `[0,1]` / `[0.04,1]` in `resolve_pbr`; no `Option` override field on the canonical type; no per-draw `classify_pbr` in the renderer. `resolve_pbr` fills only `NaN` sentinels (backstop for non-pre-classified sources, e.g. Starfield material-reference stubs) and reads the threaded `specular_authored`, never a hardcoded `false`; it must not overwrite an authored BGSM/BGEM override.
- Glass is classified **once**, alpha-aware, inside `translate_material` after `resolve_pbr` (forced glass roughness wins); engine-synthesized kinds (`material_kind >= 100`: 100 glass, 101 effect, 102 no-lighting, 103 fire-refraction) are never demoted; conductors, non-alpha and decals are gated out. `material_kind: u32` is the GPU shader-dispatch contract — its `u32`-ness is not a leak. Known-open: the shader-type discriminator never reaches the canonical `Material` (#4256), and glass split by `shader_type` (#4392).
- Carried, not re-derived: `texture_clamp_mode` / `src_blend_mode` / `dst_blend_mode` copy the import fields (defaults 0 / 6 / 7); Oblivion `parallax_height_in_alpha` is decided once from `NiTexturingProperty.apply_mode` (a downstream `if game == oblivion`, or an invented Oblivion height scale, is the violation); the detail-combine neutral is **declared by the producer** (`Material.detail_neutral`: 128/255 classic, 65/255 Skyrim FaceTint via `slot_role::detail_neutral_for`; detail view is raw UNORM, the shader divides in encoded space with no per-game branch — #4422). A shader-side per-game constant for any of these is `no-render-time-fallback`.
- **Every `f32` field added to `Material` must be added to `Material::sanitize_finite`'s macro list in the same commit** (silently missed once — the BGEM glass-optics fields, #3373; `detail_neutral` did it right). Neither the size pin nor the offset pin catches an omission; `Material` is a saved column (shape change → `/audit-save`).
- **Emissive scale = no-op** (spec §4): the three `EmissiveSource` variants were measured across four games and share ~1.0 scale; an "emissive normalization constant" is `no-fabrication`. Open question Q2 in `material-abstraction.md` is resolved — do not re-open. Tool: `crates/nif/examples/material_dump.rs`.
**Output**: `/tmp/audit/nifal/dim_1.md`

### Dimension 2: Geometry / Transform — the template the others match
Paths: `crates/nif/src/import/{coord,transform,mod}.rs`, `crates/nif/src/rotation.rs`, `crates/nif/src/import/mesh/{ni_tri_shape,bs_tri_shape,bs_geometry,tangent}.rs`
First step: `git log --since=<last report> --format='%h %s' -- crates/nif/src/import/mesh crates/nif/src/rotation.rs`
**Guards**: `tangent_convention_tests.rs`, `crates/nif/src/import/mesh/*_tests.rs` per-game extractors.
**Checklist**:
- Every per-game vertex decode (classic, packed-half `BSTriShape`, Starfield `BSGeometry` UDEC3) converges to renderer-space positions + `u32` indices; no `Option`-gated decode-later geometry reaches the consumer (`MeshRegistry::upload` in `crates/renderer/src/mesh.rs` is format-agnostic).
- Z-up→Y-up once at the import boundary (`zup_point_to_yup`, `zup_matrix_to_yup_quat`), never per consumer. One resolved tangent array (authored or Mikkelsen `synthesize_tangents`) reaches the vertex buffer.
- Degenerate-rotation SVD repair (`is_degenerate_rotation`, `repair_rotation_svd_or_identity`, `sanitize_rotation` in `rotation.rs`) fires once at parse; a consumer re-validating rotations is a raw-tier leak. `local_bound_radius` is derived at extraction (`extract_local_bound`), Y-up; no render-time recomputation.
**Output**: `/tmp/audit/nifal/dim_2.md`

### Dimension 3: Skinning & Lights
Paths: `crates/nif/src/import/mesh/{skin,sse_recon}.rs`, `crates/nif/src/import/walk/lights.rs`, `crates/nif/src/import/types.rs`
First step: `grep -rn 'skin_attach_bone_names\|widen_packed_bone_indices' crates/nif/src`
**Guards**: `walk::lights::light_dispatch_coverage_tests` (all four `NiLight` arms present; anti-vacuity assert); `walk::tests::the_scene_graph_walkers_never_call_a_satellite_walker`; `sse_skin_index_space_tests.rs` (`#[ignore]`, Skyrim data).
**Checklist**:
- `ImportedSkin` emits **global** bone indices. Skyrim SE packed indices already address the skin bone list and are only widened (`widen_packed_bone_indices`); a partition-palette remap on that channel is the regression (see `/audit-nif` Dim 4). Palette skinning is game-agnostic downstream — no consumer re-derives partition layout.
- Starfield bone names read **both** authored channels, `SkinAttach` first (`skin_attach_bone_names`), then node refs, then the geometric solver, then a `Bone{i}` placeholder; a count mismatch between channels declines the list rather than zipping (#3930). Reverting to `bone_refs`-only fabricates names.
- Lights: `ImportedLight` resolves to `LightKind` + effective `radius`; the renderer never inspects the source block type. `NiSpotLight`/`NiDirectionalLight` direction is world column 0 (Gamebryo model direction (1,0,0), #4395), with the directional kind negated once at the boundary ("toward the light"). `NiAmbientLight` scoping is parked by census (0 live vanilla population; `crates/nif/examples/ambient_light_census.rs`; the spawn gate's colour-sum predicate drops black ones).
**Output**: `/tmp/audit/nifal/dim_3.md`

### Dimension 4: Nodes — raw-tier-parked passthroughs (verify parked, not silently dropped)
Paths: `crates/nif/src/import/types.rs`, spawn sites `byroredux/src/scene/nif_loader.rs`, `byroredux/src/cell_loader/spawn.rs`
First step: `for f in bs_value_node bs_ordered_node tree_bones range_kind lod_group bs_lod_cutoffs bs_sub_index; do grep -rn --include='*.rs' "\.$f\b" byroredux/src crates/renderer/src crates/core/src | grep -v test; done` — **expected empty** (zero canonical consumers).
- Live node data (name, flags → `SceneFlags`, collision, `billboard_mode` → `Billboard`) IS consumed at the spawn sites. There is deliberately no single *translate_node* (loose-NIF spawns the full hierarchy; the cell loader a flattened placement root) — do not flag its absence.
- Parked with deferred translation (blocker in parens): `bs_value_node` (M35 LOD selector), `bs_ordered_node` (render-order hint), `tree_bones` (SpeedTree wind), `range_kind` (destructible/blast/debris systems), `lod_group` (per-frame distance switch; content-absent), `bs_lod_cutoffs` (in-cell LOD draw count), `bs_sub_index` (dismemberment). Any of them feeding a canonical component without a translate step is a leak. The deeper passthrough inventory (NiTextureEffect, NiSwitchNode identity, BSFurnitureMarker, BSBound) is `nifal.md` §2 "Passthroughs" — cross-check before reporting.
**Output**: `/tmp/audit/nifal/dim_4.md`

### Dimension 5: Particles — one overlay boundary folds every authored emitter override
Paths: `byroredux/src/systems/particle.rs`, `crates/nif/src/import/walk/emitter.rs`, `crates/nif/src/blocks/particle.rs`
First step: `grep -rn 'apply_emitter_overlays(' byroredux/src | grep -v 'systems/particle.rs'` (expect the two spawn sites: `cell_loader/spawn.rs`, `scene/nif_loader.rs`)
**The boundary**: `apply_emitter_overlays` folds colour curve, base params (`apply_emitter_params`), birth rate, force fields, texture/blend, max particles, effect shader and greyscale LUT onto a name-heuristic preset; both load paths route through it (#1513).
**Guards**: `every_overlay_parameter_reaches_the_preset` (value test, each overlay set distinct from the preset) + `every_declared_overlay_parameter_is_read_by_the_body` (source scan) — both have known holes (#4404).
**Checklist** (`no-fabrication` / `single-boundary`):
- A second inline overlay at a spawn site is a `single-boundary` violation. Authored kinematics + lifetime + size override the preset; `initial_color` is intentionally NOT applied (the colour curve owns colour — starting to apply it is a reverse `no-fabrication` regression). Size = `initial_radius × base_scale` (`base_scale` essential: FNV oasis smoke 50 × 0.15); the grow→steady→fade bell cannot map to linear start/end — only magnitude translates.
- Birth rate is authored (`extract_emitter_rate`; FLT_MAX sentinel rejected; four tiers in order — constant/keyed curve recovered as the **time-weighted mean**, not `keys.first()` or the peak (#3754), legacy `NiPSysEmitterCtlrData` first key, blend-interpolator sub-target (#2548), embedded-sequence scan for manager-driven blends (#3329, `Idle`-family preferred); sibling-controller resolution by `target_ref`, #4467 — see `/audit-nif` Dim 4). Legacy `NiParticleSystemController` keeps the preset rate.
- Planar spread is applied (#4240): NIF authors a **half-spread**, the canonical field is full width — the translate doubles it (`× 2.0`); fog-volume sizing must use the doubled value. Attribution: base params, colour, budget and the modern rate tiers are **per-instance** (#4261); only the legacy controller fallback and the #3329 sequence tier remain whole-scene.
- Force fields are Z-up→Y-up converted once at overlay time (`convert_force_fields_zup_to_yup`). Known-open: emitter orientation dropped so the spawn cone is world-axis aligned (#4398). Tool: `crates/nif/examples/emitter_dump.rs`.
**Output**: `/tmp/audit/nifal/dim_5.md`

### Dimension 6: Collision — every parsed bhk*Shape resolves (no silent drop)
Paths: `crates/nif/src/import/collision/{mod,shape,ragdoll}.rs`
First step: `cargo test -p byroredux-nif dispatch_coverage_tests`
**Guard**: `import::collision::dispatch_coverage_tests::every_dispatched_bhk_shape_has_resolve_arm` (every `Bhk*Shape` with a dispatch arm has a `downcast_ref` arm in `resolve_shape_inner`; source scan over whole files, known-weak per #4411). Count shape arms fresh; do not quote a number.
**Checklist** (`no-leak` — "parsed for byte-correctness then dropped at the unsupported-shape fallback" is the prime leak class):
- Every parsed `bhk*Shape` is resolved, delegated, folded into a `Compound`, or explicitly parked with a documented reason. `BhkPlaneShape → None` is the documented exception — no collider is produced for its placement: the one vanilla instance's alpha-tested render mesh is rejected by the trimesh fallback's `!alpha_test` gate (#4407 corrected the arm's earlier "fallback covers it" justification; the drop is counted in `plane_shapes` and surfaced by the spawn census). Havok→engine transform + per-game `havok_scale` are applied uniformly in `collision/`; recursion depth is bounded and non-finite floats guarded.
- The raw `hkMotionType` byte collapses to canonical `MotionType` at translate (`havok_motion_type`); a consumer reading the raw byte is `no-leak`.
- **`CollisionAuthoringSummary`** (`summarize_collision_authoring`) crosses the boundary carrying only four `u32` counts (`classic`, `new_physics`, `phantom`, `plane_shapes`) — no `bsver`/block-type string/per-game enum (a game-specific field reaching `CachedNifImport` is `no-leak`). Its consumer (`cell_loader/spawn.rs::spawn_packed_collision_proxy`) stays renderer-free: `CollisionShape` + `RigidBodyData` + transforms, and **no** `MeshHandle` (a blob-derived guess must not enter the BLAS/TLAS as authored geometry). Stale-comment lead: #4408.
- Documented limitations (NOT leaks; confirm they stay in the table at the top of `import/collision/mod.rs`): `BhkNPCollisionObject` blob does not resolve to a shape (container decoded, `hknpCompressedMeshShapeData` layout not — `docs/engine/physal.md`); `BhkPCollisionObject` phantoms need a trigger-volume path, not a rigid body; decoded-but-not-imported constraint kinds (ball-and-socket / spring / chain).
**Output**: `/tmp/audit/nifal/dim_6.md`

### Dimension 7: Animation / controllers — single NIF→AnimationClip boundary
Paths: `byroredux/src/anim_convert.rs`, `byroredux/src/asset_provider/animation.rs`, `crates/nif/src/anim/`
First step: `cargo test -p byroredux -- canonical_animation_completeness_harness every_hkx_sample_field`
**Boundary**: `convert_nif_clip` (many callers, one function — correct). **It is not the only producer** of the canonical `AnimationClip`: `convert_hkx_clip` builds it from Havok behaviour-graph idles; a field added to `AnimationClip` usually needs both producers.
**Guards**: `anim_convert::canonical_animation_completeness_harness::{every_clip_scalar_survives_convert_nif_clip, every_transform_channel_field_survives_convert_nif_clip, every_non_transform_channel_survives_convert_nif_clip}`; `asset_provider::animation::tests::every_hkx_sample_field_survives_convert_hkx_clip` — harness value choices let some field drops pass (#4405).
**Checklist**:
- Per-game variation resolved at import: B-spline interpolators (FO3/FNV *and* Skyrim+ — *feedback_bspline_not_skyrim_only*) sampled to linear keys; XYZ-Euler composed to quaternions; TBC/Hermite decoded; Z-up→Y-up once. Consumers see only game-agnostic quaternion keys.
- Text-key events wired end to end (`text_keys_ref` → `AnimationClip.text_keys` → `AnimationTextKeyEvents`); embedded controllers' empty `text_keys` is deliberate. Parked, not leaks: per-light ambient colour channels and morph-weight channels (confirm no canonical consumer).
- Known-open rotation-sanitizer holes: non-B-spline paths still square to inf (#4396), NaN static-pose fallbacks (#4397), identity substitution for overflow (#4406).
**Output**: `/tmp/audit/nifal/dim_7.md`

### Dimension 8: Shader flags / texture roles / effect shaders (highest report yield)
Paths: `crates/nif/src/shader_flags.rs`, `crates/nif/src/import/material/{slot_role,dedicated_shader,walker,mod}.rs`, `crates/nif/src/import/types.rs` (`MaterialTextureSet`), `byroredux/src/asset_provider/material/merge.rs`, `byroredux/src/cell_loader/refr.rs`, `byroredux/src/asset_provider/texture.rs`
First step: `git log --since=<last report> --format='%h %s' -- crates/nif/src/import/material byroredux/src/asset_provider/material byroredux/src/cell_loader/refr.rs`
**Guards**: `crates/nif/src/import/types.rs` tests `roles_covers_every_field_in_the_set` + `values_covers_every_field_in_the_set` (count-based: catch an omission, not *which* role); `material_translate::tests::documented_texture_role_list_matches_the_struct` (struct ↔ `nifal.md`); `asset_provider::texture::tests::common_material_texture_walk_covers_every_secondary_role_once` (all 25 secondary roles pinned to a colour space both directions; a data map flipping to sRGB fails); `slot_role.rs` confinement tests (non-Skyrim layouts never take the Skyrim co-location arms, #4431); `render/tint_alpha_gate_tests.rs`; `fo3nv_and_skyrim_decal_bits_agree`.
**Checklist** (`no-render-time-fallback` / `no-leak`):
- **Vocabulary**: `MaterialTextureSet<T>` — 22 named roles + `decals: [T; 4]`; `values()` yields 26, `secondary_values()` (= `values().skip(1)`, hard-codes `base_color` first — reordering is load-bearing) yields 25. Every source populates the same roles (`NiTexturingProperty`, `BSShaderTextureSet` FO3→Skyrim, BGSM/BGEM, `BSEffectShaderProperty`). **A per-game slot index surviving past the NIF import boundary is the cardinal violation.** `slot_to_role` is the one table used by both the importer and the REFR overlay; Starfield/FO76 do not enter it (roles come from BGSM/CDB). Starfield `.mat`/CDB is not yet a live texture-role source (the unproduced `Mat` provenance label was deleted, #3906) — re-adding it before a code path fills a role through it is the regression.
- **Role checklist** (restored 2026-09-20 after `a43b19603`'s rewrite dropped it; enforced by `documented_texture_role_list_matches_the_struct`, #3465): the 22 roles are (`base_color`, `normal`, `emissive`, `detail`, `smooth_spec`, `dark`, `height`, `environment`, `environment_mask`, `tint`, `inner_layer`, `specular`, `lighting_mask`, `back_lighting`, `lighting`, `flow`, `wrinkle`, `greyscale_lut`, `reflectance`, `emittance_gradient`, `glass_roughness_scratch`, `glass_dirt_overlay`) plus ordered `decals: [T; 4]` — 26 entries from `values()`. This list is the checklist an auditor diffs `values()` against.
- `T` converts with `map_ref` only (`Option<FixedString>` → `Option<String>` → bindless index); a consumer rebuilding a path from a role it was handed is `no-leak`.
- Likeliest mis-merges: `smooth_spec` (specular-**strength**/gloss) vs `specular` (specular-**colour**) — FO4 slot 7 is `smooth_spec` (its `_s.dds` is BC5; feeding it to the colour multiply tinted every FO4 highlight, #4424); `environment` vs `environment_mask`. Known-open: FO4 slot-2 ignores the `Glow_Map` gate (#4430).
- **Tint role**: the Skyrim skin `_sk.dds` is a subsurface input, not an albedo multiplier; the tint multiply fires only when the tint DDS carries a real alpha (`TINT_ALPHA_WEIGHT_BIT`, #4423). FaceGen per-NPC tint override keys on canonical **FaceTint** heads (kind 4), not SkinTint (`select_facegen_diffuse`, `scene/nif_loader.rs`, #4421).
- **BGSM/BGEM merge boundary** (`asset_provider/material/merge.rs`): the overlay carries **named roles** (`bgsm_emissive`, `bgsm_height`, `bgsm_greyscale_lut`, `bgsm_inner_layer` on `RefrTextureOverlay`), never re-resolved through NIF wire slots (#4434); the BGEM arm updates the NIF's effect payload in place and ORs enable bits, never assigns (#4425); greyscale-to-palette bits are captured on the lit path too, and BGSM's enable is one-way OR'd when the NIF supplied the LUT (#3897/#3898/#4286). Known-open: MSWP re-merge leaves BGEM glass roles on the source sidecar (#4400); paired clamp/parallax slot precedence (#4401); #4402 contract comment.
- Per-game flag vocabularies dispatch by **block type**, never a runtime `if game ==`: `grep -n 'if game' crates/renderer/shaders/triangle.frag crates/renderer/shaders/include/*.glsl` must be empty. All nine `BSLightingShaderProperty` shader types forward trailing data. `BSEffectShaderProperty` is captured and routed (`material_kind == 101`); its `base_color_scale` diffuse-tint-vs-emissive path is deferred via `EmissiveSource::Effect`. FO4 `Model_Space_Normals` / `Alpha_Test` bits reach `MaterialInfo` (`/audit-nif` Dim 4 owns the parse side); Skyrim effect-shader `env_map_scale` unauthored stays out of `MaterialInfo` (#4393); `BSShaderTextureSet` outranks `NiTexturingProperty` (#4235).
- **Flipbooks**: `NiFlipController` slots resolve to canonical roles (#3901) — known-open: frames bypass the per-role resolver, dropping authored CLAMP (#4426); frame textures are released on cell unload via `AnimatedTextureFlip::all_handles` (#4427) — check any new texture-owning component joins the unload walk.
- Starfield wetness/luminance parsed but no `ImportedMaterial` sink (#4282, known-open).
**Output**: `/tmp/audit/nifal/dim_8.md`

### Dimension 9: Translation-completeness signal + cross-cutting invariants
Paths: `crates/nif/tests/translation_completeness.rs`, `crates/nif/examples/`
First step: `cargo test -p byroredux-nif --test translation_completeness -- --ignored` (opt-in, needs game data; one game at a time)
- `cross_game_translation_completeness` reports per-game fill rate over the **raw pre-merge `ImportedMaterial`** tier (before BGSM/BGEM/CDB merge, #2214), stratified sample. A near-zero `tex`/`nrm` fill on FO76/Starfield is a documented structural zero, not a leak. Large per-game fill divergence is a lead, not proof — verify the extractor. A category that converges on FNV but drops to ~0 on Starfield is an unverified-game leak.
- Boundary inventory to keep true: Material ✓ `translate_material`; Particles ✓ `apply_emitter_overlays`; Animation ✓ `convert_nif_clip` (+ `convert_hkx_clip`); Nodes ✗ by design (Dim 4); exterior → `/audit-exterior`. A new category must declare a boundary. Ground cover has no canonical `Material` and neither spec records it (#4304, known-open).
**Output**: `/tmp/audit/nifal/dim_9.md`

## Phase 3: Merge

1. Read all `/tmp/audit/nifal/dim_*.md`; combine into `docs/audits/AUDIT_NIFAL_<TODAY>.md`:
   - **Executive Summary** — per-category status (converged / triaged / pending) vs the spec §2 inventory; violation counts per invariant.
   - **Per-Category Tier Matrix** — category × invariant (pass / fail / N-A) with the boundary fn cited.
   - **Findings** — by severity (NIFAL rows in `_audit-severity.md`), base format + Extra Fields above.
   - **Documented-limitation ledger** — restate parked-not-leak items (node/mesh passthroughs, FO4+ NP blob, phantoms, size-over-life curve, ambient/morph anim channels, NiAmbientLight scoping) so they are not re-reported.
2. Remove cross-dimension duplicates.

Run `.claude/commands/_audit-validate.sh` before finalizing. Suggest `/audit-publish docs/audits/AUDIT_NIFAL_<TODAY>.md` (domain label `nifal`, plus `nif-parser` for parse-side or `renderer`/`shaders` for the consuming end; add `game:*` when title-specific).
