# NIFAL Audit — 2026-08-16

Full sweep, **all 9 dimensions** (`/audit-nifal`, run as part of the
`comprehensive` audit-suite preset). No dimension was skipped; every one is
enumerated below with its finding count, including the six that came back
clean.

Dedup baseline: `/tmp/audit/issues.json` (269 OPEN issues, pre-fetched
2026-08-16) plus a fresh 400-entry CLOSED pull, plus all 14 prior
`AUDIT_NIFAL_*` reports in `docs/audits/`.

Games: reasoned from shared code paths across Oblivion → Starfield. No new
archive measurement was taken this sweep — the two measurement-driven
questions from 2026-08-12 (`BSShaderTextureSet` slot occupancy) were both
closed in code and are re-verified here as regression pins, not re-measured.

Scope note per `_audit-common.md`: this audit touches `crates/nif`,
`crates/core` (Material/collision/animation components), `byroredux/src`
(material_translate, cell_loader, scene, systems/particle, render) and
`crates/renderer` (GPU material contract). It does **not** cover the un-owned
gameplay slice, FaceGen, mod-runtime, FSR3 or the debug server.

---

## Executive Summary

The layer is in the best shape any NIFAL sweep has recorded. Every regression
pin from the 2026-08-12 texture-role sweep is closed **in code**, not just in
prose: `crates/nif/src/import/material/slot_role.rs` now holds one
`slot_to_role` table that both the NIF importer and the REFR overlay call, and
the four slot disagreements (`2`, `3`, `4/5`, `7`) plus the slot-6 inner-layer
bug resolve the evidence-backed way with unit pins for each.

The four tier invariants hold across every category with three exceptions,
none of which produces a wrong canonical value today:

| Tier invariant | Violations found this sweep |
|---|---|
| `single-boundary` | 1 (parallax scalars resolved outside `translate_material` at 6 sites) |
| `no-fabrication` | 0 |
| `no-leak` (translatable data silently dropped) | 1 (`BSFurnitureMarker` on the streaming-partial import path) |
| `no-render-time-fallback` | 1 (same parallax scalars — a per-draw `unwrap_or` default in `render/static_meshes.rs`) |
| documentation / deferral-rationale only | 2 |

**Zero per-game branches** reach the renderer. `grep -riE 'game *==|GameVariant::'`
over `byroredux/src/render/` and `crates/renderer/src/` returns nothing, and
`crates/renderer/shaders/triangle.frag` + every `include/*.glsl` header have no
game-name token at all. The cardinal NIFAL rule is intact.

The one *new* substantive finding is a path asymmetry rather than a
translation error: the exterior streaming pre-parse path
(`finish_partial_import`) hardcodes `furniture: None` for data it is already
holding the right input type to extract, and the process-lifetime NIF cache
then propagates that loss to every later placement of the same model. It is
structurally the same defect class as #2206 (`billboard_mode` on the flat walk)
— a field consumed on one import path and hardcoded absent on the other.

### Per-dimension finding counts

| Dim | Area | Findings |
|---|---|---|
| 1 | Material | 1 (MEDIUM) |
| 2 | Geometry / Transform | 0 |
| 3 | Skinning & Lights | 0 |
| 4 | Nodes | 2 (1 MEDIUM, 1 LOW) |
| 5 | Particles | 0 |
| 6 | Collision | 0 |
| 7 | Animation / controllers | 0 |
| 8 | Shader flags / texture roles | 0 |
| 9 | Completeness + cross-cutting | 1 (LOW) |

---

## Per-Category Tier Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Material — scalars/colours/flags/PBR/glass | `byroredux/src/material_translate.rs::translate_material` | PASS (3 production callers, 1 site) | PASS (emissive still a measured no-op) | PASS | PASS |
| Material — parallax scalars | *none* — bypasses the boundary | **FAIL** (D1-01) | PASS (`0.04`/`4.0` cited to #453) | PASS (value not wrong) | **FAIL** (D1-01) |
| Material — external sidecar merge | `byroredux/src/asset_provider/material.rs::merge_external_material` | PASS (`&mut ImportedMaterial`, not widened) | PASS | pre-existing (#2533) | PASS |
| Geometry / transform | `crates/nif/src/import/mesh/` + `coord.rs` + `rotation.rs` | PASS | PASS | PASS | PASS |
| Skinning | `crates/nif/src/import/mesh/skin.rs` | PASS | PASS | documented gap (#2440) | PASS |
| Lights | `crates/nif/src/import/walk/mod.rs` + `byroredux/src/systems/light_anim.rs::translate_light` | PASS | PASS | PASS | PASS |
| Nodes — live data | spawn sites (no single boundary, by design) | N/A by design | PASS | **FAIL** on the streaming-partial path (D4-01) | PASS |
| Nodes — parked passthroughs | n/a | N/A | PASS | PASS (all 7 verified zero-consumer) | N/A |
| Particles | `byroredux/src/systems/particle.rs::apply_emitter_overlays` | PASS (both load sites) | PASS | PASS | PASS |
| Collision | `crates/nif/src/import/collision/shape.rs::resolve_shape` | PASS | PASS | PASS (16 arms, 16 dispatched shapes) | PASS |
| Animation | `byroredux/src/anim_convert.rs::convert_nif_clip` (+ declared `convert_hkx_clip`) | PASS | PASS (the two synthesized cart events are documented) | PASS | PASS |
| Texture roles — slot→role | `crates/nif/src/import/material/slot_role.rs::slot_to_role` | PASS (one table, two callers) | PASS (every arm evidence-backed) | PASS | PASS |
| Texture roles — `MaterialTextureSet<T>` mechanics | `crates/nif/src/import/types.rs` | PASS | PASS | PASS (`values()` matches the struct field-for-field) | N/A |
| Shader flags / effect shaders | `crates/nif/src/shader_flags.rs` + `import/material/dedicated_shader.rs` | PASS | PASS | PASS | PASS |
| GPU material contract | `crates/renderer/src/vulkan/material.rs` ↔ `crates/renderer/shaders/include/bindings.glsl` | PASS | PASS | PASS (348 B, field-for-field, double-pinned) | PASS |
| EXAL exterior | `byroredux/src/env_translate.rs::translate_*` | PASS | PASS | PASS | PASS |

---

## Findings

### MEDIUM

#### NIFAL-D4-2026-08-16-01: `finish_partial_import` hardcodes `furniture: None`, and the process-lifetime NIF cache propagates the loss into interiors
- **Severity**: MEDIUM
- **Dimension**: Nodes
- **Tier Violated**: `no-leak` — a translatable, already-extractable block is silently dropped on one of the two import paths
- **Game Affected**: all (every game whose exteriors place furniture; measured content exists in Oblivion/FO3/FNV/Skyrim)
- **Location**: `byroredux/src/cell_loader/partial.rs:170`
- **Status**: NEW
- **Description**: `CachedNifImport::furniture` is the canonical sink for
  `BSFurnitureMarker` sit/sleep/lean entry markers (M41.5 Phase B). The
  synchronous import path builds it —
  `byroredux/src/cell_loader/references/import.rs:187-194` calls
  `byroredux_nif::import::extract_furniture_markers(&scene)` and lifts the
  result through `furniture_component`. The exterior streaming pre-parse path
  (`finish_partial_import`) instead writes a bare `furniture: None`. Unlike its
  two neighbours in the same struct literal (`flame_attach_offset`,
  `attach_points`/`child_attach_connections`), this one carries **no comment and
  no stated rationale**, and there is no cost argument available: the function
  already holds a `&NifScene` (it calls
  `byroredux_nif::import::collision::summarize_collision_authoring(&scene)` at
  `partial.rs:100`), which is exactly the argument type
  `extract_furniture_markers` takes.
- **Evidence**:
  ```
  crates/nif/src/import/mod.rs:330
      pub fn extract_furniture_markers(scene: &NifScene) -> Vec<ImportedFurnitureMarker>

  byroredux/src/cell_loader/partial.rs:100
      let collision_authoring = ...summarize_collision_authoring(&scene);   // &NifScene in hand
  byroredux/src/cell_loader/partial.rs:162-170
      flame_attach_offset: None,   // ← justified by a comment (see D4-02)
      attach_points: None,         // ← justified: "streamed REFRs are architecture/clutter,
      child_attach_connections: None,//   not modular weapons … near-zero-loss follow-up"
      furniture: None,             // ← no comment, no rationale
  ```
  The blast radius is wider than the streaming path itself because
  `NifImportRegistry` is a **process-lifetime, path-keyed, first-writer-wins**
  cache (`byroredux/src/cell_loader/references/mod.rs:280-288`: "subsequent
  placements of the same model in this cell *and* later cells reuse the shared
  `Arc`"), and `finish_partial_import` early-outs only when an entry already
  exists (`partial.rs:45-50`). Once a chair/bench model is first cached by the
  streaming worker, every later placement of that same path — including
  interior cells reached through a door transition, which never run the
  streaming path — reads the furniture-less entry.
- **Impact**: Exterior-streamed furniture spawns with no `Furniture` component,
  so `byroredux/src/systems/sandbox.rs`'s seat search
  (`world.query::<Furniture>()`, line 171) cannot see it, and the marker
  positions/headings are lost. Cache poisoning extends the loss to interiors
  for any model that appears in both. Currently bounded by the
  `BYRO_SANDBOX_SIT` env gate on the sit runtime, which is why this is MEDIUM
  rather than HIGH — the drop is real and unconditional, the *visible*
  consequence is gated.
- **Related**: #2206 (`billboard_mode` — the same "consumed on one import path,
  hardcoded `None` on the other" shape, which four prior sweeps restated as
  PASS from doc prose); #2010 / M41.5 Phase B (the feature this drops);
  NIFAL-D4-2026-08-16-02 (the sibling comment defect in the same literal).
- **Suggested Fix**: Call `extract_furniture_markers(&scene)` in
  `finish_partial_import` and route it through the same `furniture_component`
  helper the sync path uses (widen its visibility from `pub(crate)` in
  `references/attach.rs` if needed). Pin with a test asserting the two paths
  produce the same `CachedNifImport::furniture` for one fixture NIF — that
  parity assertion is what stops the next field from diverging.

#### NIFAL-D1-2026-08-16-01: `parallax_height_scale` / `parallax_max_passes` bypass the canonical `Material`, with the same magic defaults duplicated at six sites plus a render-time fallback
- **Severity**: MEDIUM
- **Dimension**: Material
- **Tier Violated**: `single-boundary` + `no-render-time-fallback`
- **Game Affected**: all (FO3/FNV legacy parallax and Skyrim+/FO4 `BSLightingShaderProperty` parallax alike)
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:613-614`,
  `byroredux/src/scene/nif_loader.rs:1016-1017`,
  `byroredux/src/render/static_meshes.rs:271-276`
- **Status**: NEW
- **Description**: These two authored scalars are the only material values that
  never pass through `translate_material`. They stay raw `Option<f32>` on
  `ImportedMaterial` (`crates/nif/src/import/types.rs:490-491`) and are read
  **directly off the raw tier at both spawn sites**, each applying its own
  hardcoded `unwrap_or(0.04)` / `unwrap_or(4.0)`, into
  `MaterialTextureHandles` — a render-facing component, not the canonical
  `Material`. `render/static_meshes.rs` then applies the *same two literals a
  third time* as a per-draw fallback for entities lacking the component. This
  is precisely the shape #2444 (MAT-D3-02) removed for the PBR scalars: a
  materialization decision living in the render path outside the single source
  of truth.
- **Evidence**:
  ```rust
  // byroredux/src/cell_loader/spawn/mesh_instance.rs:613
  parallax_height_scale: mesh.material.parallax_height_scale.unwrap_or(0.04),
  parallax_max_passes:   mesh.material.parallax_max_passes.unwrap_or(4.0),

  // byroredux/src/scene/nif_loader.rs:1016   — byte-identical duplicate
  parallax_height_scale: mesh.material.parallax_height_scale.unwrap_or(0.04),
  parallax_max_passes:   mesh.material.parallax_max_passes.unwrap_or(4.0),

  // byroredux/src/render/static_meshes.rs:271-276   — a THIRD copy, per draw
  let parallax_height_scale = material_texture_handles
      .map(|handles| handles.parallax_height_scale)
      .unwrap_or(0.04);
  let parallax_max_passes = material_texture_handles
      .map(|handles| handles.parallax_max_passes)
      .unwrap_or(4.0);
  ```
  Two more literal copies exist in `byroredux/src/cell_loader/terrain.rs:624-625`
  and `byroredux/src/cell_loader/terrain_lod_btr.rs:297-298`, and a sixth in
  `GpuMaterial::default()` (`crates/renderer/src/vulkan/material.rs:369-370`) —
  six occurrences of the pair with no shared constant. Note the canonical
  `Material` *does* already carry the same authored value, as
  `ShaderTypeFields::parallax_height_scale` /
  `parallax_max_passes` (`crates/core/src/ecs/components/material.rs:402-403`),
  copied by the boundary via `shader_type_fields` — the renderer simply never
  reads it from there.
- **Impact**: No wrong value today; all six literals agree. What is broken is
  the guarantee: a change to the default, or a new load path that forgets one
  of the copies, silently diverges POM depth between the loose-NIF and
  cell-loaded renderings of the same mesh, with no test able to catch it (the
  boundary's own `translate_material_copies_every_canonical_field` harness
  cannot see fields that never enter the boundary). It also means the raw-tier
  `Option` — a "not authored" sentinel — is resolved by consumers rather than
  at translate, which is the `no-leak` pattern the layer exists to prevent.
- **Related**: #2444 / MAT-D3-02 (the identical fix applied to the PBR scalars
  and the exterior draw populations); #2317 (FO3 parallax gating, closed —
  different angle: *when* POM fires, not *where* its scalars resolve).
- **Suggested Fix**: Resolve both scalars inside `translate_material` into
  plain `f32` fields on the canonical `Material` (mirroring
  `metalness`/`roughness`), have both spawn sites and
  `render/static_meshes.rs` read `Material` instead of hand-defaulting, and
  hoist `0.04`/`4.0` to one named constant next to the `#453` citation in
  `byroredux/src/components.rs:225-228`.

### LOW

#### NIFAL-D4-2026-08-16-02: The stated blocker for dropping `flame_attach_offset` on the streaming path is false — the helper takes `&NifScene`, not `&ImportedScene`
- **Severity**: LOW
- **Dimension**: Nodes
- **Tier Violated**: `parked-not-leak` — the deferral is recorded, but its recorded justification does not match the code
- **Game Affected**: all (streamed exterior candles / torches / campfires)
- **Location**: `byroredux/src/cell_loader/partial.rs:131-139` (the comment) and `:162` (the drop)
- **Status**: NEW
- **Description**: `partial.rs` justifies `flame_attach_offset: None` with:
  "The helper takes `&ImportedScene` (post-import node array); partial.rs works
  on the raw `NifScene`. Running the full `import_nif_scene` again here just to
  get the node names would double the per-NIF parse cost." Both claims are
  false against current code. `find_flame_attach_offset` is declared
  `fn find_flame_attach_offset(scene: &byroredux_nif::scene::NifScene)`
  (`byroredux/src/cell_loader/references/import.rs:245`) and its body walks
  `scene.blocks` directly, downcasting to `NiNode` — it needs no
  `ImportedScene` and no second import pass. `partial.rs` already holds exactly
  that `&NifScene`.
- **Evidence**:
  ```rust
  // byroredux/src/cell_loader/references/import.rs:245
  pub(super) fn find_flame_attach_offset(scene: &byroredux_nif::scene::NifScene) -> Option<[f32; 3]> {
      for idx in 0..scene.blocks.len() {
          let Some(node) = scene.get_as::<byroredux_nif::blocks::node::NiNode>(idx) else { continue };
  ```
  The only real obstacle is visibility (`pub(super)` scopes it to the
  `references` module), which is a one-word change, not a parse-cost argument.
- **Impact**: The behavioural cost is small and has a documented fallback
  (streamed flame lights sit at the placement root rather than the authored
  flame node). The real cost is auditability: an incorrect deferral rationale
  is exactly what let #2206 be restated as PASS by four consecutive sweeps.
  A future reader checking "is this still blocked?" is told a blocker that has
  not existed since the helper was written.
- **Related**: NIFAL-D4-2026-08-16-01 (the neighbouring field in the same
  struct literal); #2206.
- **Suggested Fix**: Either widen `find_flame_attach_offset` to `pub(crate)`
  and call it from `finish_partial_import` (a two-line change that closes the
  gap outright), or correct the comment to state the actual reason. Do not
  leave the false one.

#### NIFAL-D9-2026-08-16-01: `nifal.md` — the layer spec — understates both the material boundary set and the collision shape count
- **Severity**: LOW
- **Dimension**: Completeness / cross-cutting
- **Tier Violated**: documentation (the spec is the authority every NIFAL sweep reads before the code)
- **Game Affected**: n/a
- **Location**: `docs/engine/nifal.md:317`, `:605`, and the "Materials" section at `:66-99`
- **Status**: NEW
- **Description**: Two independent drifts in the spec doc:
  1. **Collision count.** `nifal.md:317` states "All 13 parsed `bhk*Shape`
     variants now translate" and `:605` repeats "all 13 parsed shape variants
     now translate." The live count is **16**: sixteen
     `downcast_ref::<Bhk*Shape>` arms in
     `crates/nif/src/import/collision/shape.rs` against sixteen dispatched
     `*Shape` structs in `crates/nif/src/blocks/mod.rs`. Every NIFAL report
     since 2026-07-03 has recorded 16 in its own matrix; none propagated the
     correction back to the spec. `/audit-nifal/SKILL.md` also says 16, so the
     spec is now the only source stating 13.
  2. **Missing second Material producer.** The Materials section names
     `translate_material` as the boundary and says "the two previously-duplicated
     construction sites are collapsed into the one boundary." Since #2444 there
     is a second *declared* production function in the same module —
     `translate_texture_only_material` (`byroredux/src/material_translate.rs:280`),
     used by `cell_loader/terrain.rs`, `terrain_lod.rs` and `object_lod.rs` for
     draw populations that have no source material record — and
     `translate_material` itself now has three production callers, not two
     (`scene/nif_loader.rs:979`, `cell_loader/spawn/mesh_instance.rs:575`,
     `cell_loader/placement_lod.rs:512`). Neither the helper nor the third
     caller appears anywhere in `nifal.md`.
- **Evidence**: `grep -c 'downcast_ref::<Bhk' crates/nif/src/import/collision/shape.rs`
  → `16`; `grep -o 'Bhk[A-Za-z]*Shape' crates/nif/src/blocks/mod.rs | sort -u`
  → 16 distinct shapes (`BhkSimpleShapePhantom` is a phantom, explicitly parked
  at `shape.rs:303`, not a shape). `grep -n 'translate_texture_only_material'
  docs/engine/nifal.md` → no match.
- **Impact**: An auditor who trusts the spec (as the audit protocol instructs)
  under-counts the collision surface by three arms and misses one of the two
  canonical-`Material` producers entirely — the exact failure mode that made
  the #2206 billboard gap invisible for four sweeps.
- **Related**: #2299, #2301, #2306, #2488 (all prior `nifal.md`-drift findings —
  this doc has a standing drift problem, not a one-off).
- **Suggested Fix**: Correct both counts, and add a short "Material producers"
  subsection to §3 naming `translate_material` (3 callers) and
  `translate_texture_only_material` (3 callers) with the rule that separates
  them — has a source `ImportedMaterial` vs has only a bound texture path.

---

## Documented-limitation ledger (verified parked, NOT findings)

Re-verified this sweep so the next one does not re-report them:

| Item | State | Verification |
|---|---|---|
| 7 raw-tier parked node/mesh fields (`bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`) | parked, zero canonical consumers | Grepped each outside `types.rs`/parser/tests: the only hits are literal `None` initialisers in `crates/spt/src/import/mod.rs`. Confirmed `parked-not-leak`. |
| `BhkNPCollisionObject` FO4+/FO76/Starfield packed-Havok blob | documented limitation | `summarize_collision_authoring` census + `missing_collision_fallback` proxy intact; census still crosses as three plain `u32`s. |
| `BhkPCollisionObject` phantoms | documented limitation | `is::<BhkSimpleShapePhantom>` / `is::<BhkAabbPhantom>` park at `shape.rs:303`. |
| `BhkPlaneShape` → `None` | documented exception | Arm present, comment intact. |
| Particle size-over-life *curve* | future work | Only authored magnitude translated (`initial_radius × base_scale`); `initial_color` still deliberately unapplied. |
| Per-light ambient colour animation channels | parked | No canonical consumer. |
| Cell-loader path never builds `SkinnedMesh` | recorded, not fixed (#2440) | Requires a per-placement node-entity map the cell loader does not have. |
| `SkinnedMesh.bones: Vec<Option<EntityId>>` | terminal sentinel, not a resolve-later leak (#2441) | Unchanged. |
| Raw-tier `byroredux_nif::anim::AnimationClip` sharing a name with the canonical one | permitted by the tier model (#2442) | Unchanged. |
| `convert_hkx_clip`'s two synthesized cart/furniture exit events | declared no-fabrication exception (#2305) | Unchanged. |
| SLSF1 `Refraction` without `Fire_Refraction` has no consumer | documented, not a leak (#2327) | `material_optical_scalar` comment intact. |
| Starfield particle slice structurally N/A | pinned by `starfield_corpus_has_no_particle_blocks` (#2354) | Unchanged. |
| `lighting` / `flow` / `wrinkle` GPU role lanes declared but unsampled | deliberate deferral (#2712) | Present in both the Rust struct and `bindings.glsl` for layout parity. |

## Open issues matched during dedup (reported previously, still OPEN — skipped)

- **#2697** — `supplemental_texture_indices` is a third hand-written role walk with no lockstep test (Dim 8).
- **#2533** — BGEM v21+/v22 glass-overlay texture paths have no `MaterialTextureSet` role (Dim 8).
- **#2532** — the canonical-tier completeness harness covers 1 of ~5 declared translate boundaries (Dim 9).
- **#2490** — raw-material → marker-component block copy-pasted at both spawn sites (Dim 1/4).
- **#2571** — three raw-tier `ImportedMaterial` fields re-read at each spawn site (Dim 1).
- **#2549** — `bhkRigidBody.havok_filter` parsed then dropped at the boundary (Dim 6).

## Closed issues re-verified as fixed (no regression)

- **#2693** — MultiLayerParallax inner layer now reads slot **6**
  (`slot_role.rs:143-146`, pinned by `multi_layer_parallax_inner_layer_is_slot_six`).
- **#2694** — FaceTint slot 2 → `Tint`, slot 3 → `Detail`, slots 4/5 → `None`
  (pinned by `resolves_the_four_table_disagreements_the_importer_way`).
- **#2695** — one shared `slot_to_role` table, called by the importer
  (`dedicated_shader.rs:145`) *and* the REFR overlay
  (`cell_loader/spawn/mesh_instance.rs:118`).
- **#2530** — the loose-NIF path now extracts and spawns authored lights
  (`scene/nif_loader.rs:266` + `:1230`).
- **#2443** — `Material::grayscale_to_palette_scale` exists and
  `translate_material` copies it (`material_translate.rs:209`).
- **#2444** — every exterior draw population gets a boundary-produced
  `Material`, pinned by `every_exterior_spawner_inserts_a_boundary_material`.

## Candidates raised and disproved (recorded so the next sweep does not re-chase them)

- **`BhkSimpleShape` has no resolve arm.** False — the grep hit is
  `BhkSimpleShapePhantom`, a phantom explicitly parked at
  `crates/nif/src/import/collision/shape.rs:303`. Shape arms and dispatched
  shapes both count 16.
- **FO4 precombines fabricate a zeroed `CollisionAuthoringSummary`.**
  `byroredux/src/cell_loader/precombined.rs:753-771` does set
  `collision_authoring: Default::default()`, but precombines spawn as
  `RenderLayer::Architecture` (`precombined.rs:399`) and
  `missing_collision_fallback` (`byroredux/src/cell_loader/spawn.rs:82-84`)
  returns `ArchitectureTriMesh` **before** consulting the census. No
  behavioural loss.
- **`cornell.rs` is a second `Material` materialization site.** It builds
  literals (`matte`/`pbr`/`glass`/`emissive`/`fire_refraction`), but the
  `--cornell` harness has no NIF source to translate, and its one
  `translate_material` call is inside `#[cfg(test)]`. Not a NIFAL boundary
  bypass.
- **The loose-NIF and cell paths diverge on particle overlays.** Both call
  `apply_emitter_overlays` with identical argument lists, and both apply the
  `fog::medium_from_particle` replacement. The only asymmetry is placement
  (`Quat::IDENTITY`/scale `1.0` on the cell billboard path vs `ref_rot`/
  `ref_scale` on its fog sibling) — a placement question, not a translation
  one, and out of NIFAL scope.
- **`placement_lod.rs` drops secondary texture roles.** It inserts no
  `MaterialTextureHandles`, so Oblivion `_far.nif` imposters get diffuse only —
  but they do route through the full `translate_material` boundary and are
  distant imposters by construction. Not a translation leak.

---

Suggested next step:

```
/audit-publish docs/audits/AUDIT_NIFAL_2026-08-16.md
```
