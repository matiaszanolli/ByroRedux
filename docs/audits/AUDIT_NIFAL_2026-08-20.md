# NIFAL Audit — 2026-08-20

Full sweep, **all 9 dimensions** (`/audit-nifal`, run as part of the
`comprehensive` audit-suite preset). No dimension was skipped.

Baseline: HEAD `bb0b92f2`. Delta since the last sweep: **335 commits**
(`/tmp/audit/commits_since_last.txt`), dominated by session-70 WATAL water
work. Per the dispatch brief, weighting went to Dimension 1 (the
`ImportedMaterial → Material` boundary — `byroredux/src/material_translate.rs`
changed 11 times, `byroredux/src/asset_provider/material.rs` 7 times) and
Dimension 8 (texture-role vocabulary — `crates/nif/src/import/material/slot_role.rs`
gained a full per-game slot layout in `86c41022`).

Dedup baseline: `/tmp/audit/issues.json` (400 entries, numbers 2671–3103,
124 OPEN) plus all 15 prior `AUDIT_NIFAL_*` reports in `docs/audits/`. Note the
pre-fetched issue file does not reach below #2671, so older issue numbers cited
by the 2026-08-16 report (#2490, #2532, #2533, #2549, #2571) could not be
re-queried and are carried forward on that report's word.

No `cargo` was run (suite rule). All conclusions are static: source reads, greps,
and cross-file parity checks.

---

## Executive Summary

The four findings that the 2026-08-16 sweep raised are all now **filed and OPEN**
(#3072–#3075) and are **not re-reported** here. Every regression pin from the
2026-08-12 / 2026-08-16 texture-role work is still closed in code, and
`crates/nif/src/import/material/slot_role.rs` has been *strengthened* in the
delta — it went from a single shader-type-keyed table to a proper
`(TextureSlotLayout, slot)` matrix with a `record_unrouted_texture_slot`
telemetry counter for gaps. That is the single best structural improvement in
this delta and it closes the Dim-8 "per-game slot index survives past the import
boundary" risk class at the table level.

**The new risk in this delta is not material translation — it is mesh-bound
water**, which crossed NIFAL for the first time (`8110f359` → `1a428278`) and did
so *without* getting the boundary discipline the material slice has. Concretely:

- The 39-line component-composition block that turns a water-shader mesh into
  `WaterPlane` + `WaterFlow` + `WaterVolume` is **copy-pasted verbatim** at both
  spawn sites — line-for-line identical except for three placement-variable
  names. This is textually the same defect `translate_material` was created to
  remove, re-created for a new category (D1-01).
- The canonical `WaterKind → foam_strength` mapping is written as literals at
  three ESM-path sites and at **zero** NIFAL-path sites, so the same canonical
  `WaterKind::River` renders with foam `0.20` from a WATR record and `0.65` from
  a NIF mesh (D1-02).
- The mesh-water classifiers introduce four uncited constants, the most
  consequential being a hardcoded **world +X** flow direction applied to every
  river/stream/canal mesh regardless of its placement rotation — which the same
  block already holds in hand (D1-03).

The one genuinely new Dim-8 finding is structural rather than tabular: the new
per-game `texture_slot_layout` discriminator is assigned on **one of four**
shader-property branches, so FO4+/FO76/Starfield meshes whose only property is a
BSEffect/BSSky/BSWater shader silently fall back to the `Skyrim` default and
resolve their REFR texture overrides through the wrong game's table (D8-01).

| Tier invariant | Violations found this sweep |
|---|---|
| `single-boundary` | 2 (mesh-water composition block ×2 sites; `WaterKind`→foam literals ×3 sites, absent on the NIFAL path) |
| `no-fabrication` | 1 (mesh-water geometry/volume/flow constants) |
| `no-leak` | 1 (`texture_slot_layout` unset on 3 of 4 shader-property branches) |
| `no-render-time-fallback` | 0 new (the one standing case is #3073, OPEN) |
| documentation / spec drift | 1 (`docs/engine/nifal.md`) |

**Zero per-game branches reach the renderer.** `grep -riE 'game *==|GameVariant::|GameKind::'`
over `byroredux/src/render/` and `crates/renderer/src/` returns nothing;
`crates/renderer/shaders/triangle.frag`, every GLSL header under `crates/renderer/shaders/include/`, and the new `crates/renderer/shaders/water.frag` contain game names only
inside comments. The cardinal NIFAL rule is intact.

### Per-dimension finding counts

| Dim | Area | Findings |
|---|---|---|
| 1 | Material (incl. the new mesh-water slice) | 3 (3 MEDIUM) |
| 2 | Geometry / Transform | 0 |
| 3 | Skinning & Lights | 0 |
| 4 | Nodes | 0 new (#3072, #3074 OPEN) |
| 5 | Particles | 0 |
| 6 | Collision | 0 |
| 7 | Animation / controllers | 0 |
| 8 | Shader flags / texture roles | 2 (1 MEDIUM, 1 LOW) |
| 9 | Completeness + cross-cutting | 1 (LOW) |

---

## Per-Category Tier Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Material — scalars/colours/flags/PBR/glass | `byroredux/src/material_translate.rs::translate_material` | PASS (3 production callers, 1 site; signature still `&ImportedMaterial` + `mesh_name`, **not** widened back to `&ImportedMesh`) | PASS (emissive still a measured no-op) | PASS | PASS |
| Material — parallax scalars | *none* — bypasses the boundary | **FAIL** — #3073, OPEN | PASS | PASS | **FAIL** — #3073, OPEN |
| Material — texture-only draws | `byroredux/src/material_translate.rs::translate_texture_only_material` | PASS (3 callers, pinned by `every_exterior_spawner_inserts_a_boundary_material`) | PASS | PASS | PASS |
| Material — external sidecar merge | `byroredux/src/asset_provider/material.rs::merge_external_material` | PASS (`&mut ImportedMaterial`, not widened) | PASS | PASS | PASS |
| **Mesh water — component composition** | *none* — 39 identical lines at both spawn sites | **FAIL** (D1-01) | — | — | PASS |
| **Mesh water — `WaterKind` → `foam_strength`** | *none* — literals at 3 ESM sites, 0 NIFAL sites | **FAIL** (D1-02) | PASS | **divergent** (D1-02) | PASS |
| **Mesh water — kind / volume / flow derivation** | `byroredux/src/material_translate.rs::water_kind_from_mesh_geometry` + `water_volume_from_mesh` | PASS (one helper each, both load paths call them) | **FAIL** (D1-03) | PASS | PASS |
| Mesh water — optical response | `byroredux/src/material_translate.rs::water_material_from_mesh` | PASS | PASS | PASS | PASS |
| Geometry / transform | `crates/nif/src/import/mesh/` + `crates/nif/src/import/coord.rs` + `crates/nif/src/rotation.rs` | PASS | PASS | PASS | PASS |
| Skinning | `crates/nif/src/import/mesh/skin.rs` | PASS | PASS | documented gap (#2440) | PASS |
| Lights | `crates/nif/src/import/walk/mod.rs` + `byroredux/src/systems/light_anim.rs` | PASS | PASS | PASS (no consumer downcasts `NiPointLight`/`NiSpotLight`/…; only comments name them) | PASS |
| Nodes — live data | spawn sites (no single boundary, by design) | N/A by design | PASS | **FAIL** on the streaming-partial path — #3072, OPEN | PASS |
| Nodes — parked passthroughs | n/a | N/A | PASS | PASS (all 7 re-verified zero-consumer) | N/A |
| Particles | `byroredux/src/systems/particle.rs::apply_emitter_overlays` | PASS (both load sites, unchanged in delta) | PASS | PASS | PASS |
| Collision | `crates/nif/src/import/collision/shape.rs::resolve_shape` | PASS | PASS | PASS (16 arms vs 16 dispatched shapes, automated by `dispatch_coverage_tests`) | PASS |
| Collision — authoring census | `crates/nif/src/import/collision/mod.rs::summarize_collision_authoring` | PASS | PASS | PASS (still three bare `u32`s) | PASS |
| Animation | `byroredux/src/anim_convert.rs::convert_nif_clip` | PASS (7 callers, one boundary) | PASS | PASS | PASS |
| Texture roles — slot→role | `crates/nif/src/import/material/slot_role.rs::slot_to_role` | PASS (one table, two callers) | PASS (every arm evidence-backed) | **FAIL** on layout population (D8-01) | PASS |
| Texture roles — `MaterialTextureSet<T>` mechanics | `crates/nif/src/import/types.rs` | PASS | PASS | PASS (`values()` = 18 roles + 4 decals, matches `map_ref` field-for-field) | N/A |
| Texture roles — REFR `XTXR` slot swap | `byroredux/src/cell_loader/refr.rs::apply_slot_swap` | **FAIL** (D8-02, game-agnostic slot table) | PASS | PASS | PASS |
| Shader flags / effect shaders | `crates/nif/src/shader_flags.rs` + `crates/nif/src/import/material/dedicated_shader.rs` | PASS | PASS | PASS | PASS |
| EXAL exterior | `byroredux/src/env_translate.rs::translate_*` | PASS | PASS | PASS | PASS |

---

## Findings

### MEDIUM

#### NIFAL-D1-2026-08-20-01: The mesh-water spawn block is 39 lines of verbatim copy-paste at both load paths — the exact duplication `translate_material` exists to prevent, re-created for a new category
- **Severity**: MEDIUM
- **Dimension**: Material (mesh-water slice)
- **Tier Violated**: `single-boundary`
- **Game Affected**: all (Oblivion/FO3/FNV `WaterShaderProperty` + Skyrim+/FO4 `BSWaterShaderProperty`; `is_water_shader` is set from both `crates/nif/src/import/material/legacy_properties.rs:639` and `crates/nif/src/import/material/dedicated_shader.rs:612`)
- **Location**: `byroredux/src/scene/nif_loader.rs:1027` and `byroredux/src/cell_loader/spawn/mesh_instance.rs:724`
- **Status**: NEW (introduced in this delta by `8110f359` / `0ea00e5f` / `91e01118` / `e195c511`)
- **Description**: The individual *derivations* were centralised correctly —
  `water_material_from_mesh`, `water_kind_from_mesh_geometry` and
  `water_volume_from_mesh` all live once in
  `byroredux/src/material_translate.rs`. What was **not** centralised is the
  *composition*: which components get inserted, in what order, with what
  literals, and under which guard. That block is duplicated in full at both
  spawn sites. Stripping comments and leading whitespace, the two blocks are
  39 lines each and differ **only** in the three placement-variable names
  (`translation`/`quat`/`mesh.scale` vs `final_pos`/`final_rot`/`final_scale`).
  This is structurally identical to the pre-NIFAL state that
  `material_translate.rs`'s own module doc describes: *"the `Material` struct
  literal was built verbatim at two sites … kept in sync by hand. That
  duplication was itself a translation leak: a field added to one site and not
  the other silently diverged the two load paths."*
- **Evidence**: normalised diff of `nif_loader.rs:1027-1065` against
  `mesh_instance.rs:724-772` (comments and indentation stripped):
  ```
  1d0
  < if mesh_water {
  33,35c32,34
  < translation,          > final_pos,
  < quat,                 > final_rot,
  < mesh.scale,           > final_scale,
  39a39
  > }
  ```
  Everything else — the `WaterPlane { kind, material, damage_per_second: 0.0 }`
  literal, the `if let Some(flow)` insert, the `bound_center` construction, and
  the `water_kind != WaterKind::Waterfall` volume guard — is byte-identical.
  The `damage_per_second: 0.0` literal in particular is now hardcoded twice
  (`nif_loader.rs:1044`, `mesh_instance.rs:747`) while the ESM path resolves the
  same field from the WATR record (`byroredux/src/cell_loader/water.rs:492`,
  `:825`) — so the field that `06f84f0d` ("apply authored legacy damage") made
  authored on one path has two hand-written zeroes on the other.
- **Impact**: No wrong value today — the two blocks agree. What is broken is the
  guarantee. Every future mesh-water change (a fifth component, a damage source,
  a different waterfall-volume rule) has to be made twice with nothing to catch
  a miss: no compile error, and no test compares the two paths' component sets.
  This is the same class as #2206 and #3072, both of which are live proof that
  the two load paths *do* drift when nothing forces them together.
- **Related**: #3072 (`furniture: None` — same "one path does it, the other
  doesn't" shape); #2490 (raw-material → marker-component block copy-pasted at
  both spawn sites — the identical defect for a different component group,
  reported OPEN by the 2026-08-16 sweep).
- **Suggested Fix**: Add one `pub(crate) fn spawn_mesh_water(world, entity,
  material: &Material, mesh_name, positions, normal_idx, flow_idx, position,
  rotation, scale, local_center, local_radius)` to
  `byroredux/src/material_translate.rs` beside the four helpers it already
  owns, and call it from both sites. That is the declared boundary for this
  category (Dim 9's "New categories must declare a boundary"), and it gives
  `damage_per_second` one home to grow a real source in.

#### NIFAL-D1-2026-08-20-02: `WaterKind` → `foam_strength` is written as literals at three ESM-path sites and at zero NIFAL-path sites, so the same canonical kind renders with 3.25× the foam depending on which boundary produced it
- **Severity**: MEDIUM
- **Dimension**: Material (mesh-water slice)
- **Tier Violated**: `single-boundary` (a kind-derived canonical value with no single derivation site) — manifesting as a divergent canonical output
- **Game Affected**: all games that ship mesh-bound river/stream/rapids water (Oblivion, FO3, FNV, Skyrim+)
- **Location**: derivation absent at `byroredux/src/material_translate.rs:90` (`water_material_from_mesh`); the ESM-path literals are at `byroredux/src/env_translate.rs:932`, `:947` and `byroredux/src/cell_loader/water.rs:379`, `:381`
- **Status**: NEW
- **Description**: `WaterKind` is the canonical enum, and the canonical type
  already demonstrates the right pattern for kind-derived values:
  `WaterFlow::speed_for_kind` (`crates/core/src/ecs/components/water.rs:453`)
  lives on the canonical type with its measurement rationale attached, and both
  boundaries call it. `foam_strength` — the sibling kind-derived value — has no
  such function. Instead the mapping `Rapids → 0.85` / `River → 0.20` is typed
  out as literals at three ESM-path sites, and the NIFAL mesh-water path
  supplies none of them: `water_material_from_mesh` starts from
  `WaterMaterial::default()` (`foam_strength: 0.65`,
  `crates/core/src/ecs/components/water.rs:341`) and never touches the field,
  even though `water_kind_from_mesh_geometry` has already produced the kind two
  statements later at the call site.
- **Evidence**:
  ```rust
  // ESM/EXAL path — byroredux/src/env_translate.rs:929-948
  if lowered.contains("rapid") || (…) { kind = WaterKind::Rapids; mat.foam_strength = 0.85; }
  else if lowered.contains("waterfall") || … { kind = WaterKind::River; mat.foam_strength = 0.20; }

  // ESM/EXAL path, second copy — byroredux/src/cell_loader/water.rs:378-382
  if matches!(kind, WaterKind::Rapids) { material.foam_strength = 0.85; }
  else if matches!(kind, WaterKind::River) { material.foam_strength = 0.20; }

  // NIFAL path — byroredux/src/material_translate.rs:90-150
  let mut water = WaterMaterial::default();   // foam_strength stays 0.65
  water.shader_flags = material.water_shader_flags;
  …                                            // no foam_strength assignment anywhere
  ```
  The value is live at the GPU: `byroredux/src/render/water.rs:227` uploads
  `mat.foam_strength` into `GpuWaterParams.timing.z`, which
  `crates/renderer/shaders/water.frag:602` reads as `foamStrength` and
  `:997` multiplies into the final foam mask.
- **Impact**: A river authored as a NIF mesh renders with **0.65** foam while an
  identical river authored as a WATR-backed cell plane renders with **0.20** —
  3.25× too much foam on exactly the seam where the two meet (a mesh river
  segment flowing into a cell water body is the common authoring pattern in
  Oblivion and Skyrim exteriors). Rapids diverge the other way (0.65 vs 0.85).
  Because the derivation exists only as literals, no test or type can observe
  the divergence.
- **Related**: NIFAL-D1-2026-08-20-01 (the same category, the same missing
  boundary); #2872 (the audit that established `WaterFlow::speed_for_kind` as
  the canonical kind-derived-value pattern this one should follow).
- **Suggested Fix**: Add `WaterMaterial::foam_for_kind(kind) -> f32` (or
  `WaterKind::canonical_foam_strength`) to
  `crates/core/src/ecs/components/water.rs` next to `speed_for_kind`, carrying
  the same rationale comment the literals currently carry nowhere; call it from
  all three ESM sites and from the mesh-water path. That collapses four
  hand-written numbers into one and makes the NIFAL/EXAL agreement structural.

#### NIFAL-D1-2026-08-20-03: The mesh-water classifiers introduce four uncited constants, including a fabricated world-space **+X** current direction applied to every river mesh regardless of its placement rotation
- **Severity**: MEDIUM
- **Dimension**: Material (mesh-water slice)
- **Tier Violated**: `no-fabrication`
- **Game Affected**: all (the name heuristic and geometry fallback are game-agnostic and run on every water-shader mesh)
- **Location**: `byroredux/src/material_translate.rs:173`, `:211`, `:236`
- **Status**: NEW
- **Description**: The NIFAL layer's `no-fabrication` invariant requires a new
  constant to cite a measurement or source (`feedback_no_guessing`). The
  mesh-water slice added four that do not:
  1. **`[1.0, 0.0, 0.0]` — the river/rapids flow direction** (`:173`). This is
     the most consequential. `WaterFlow.direction` is documented on the
     canonical type as *"Unit vector in **world Y-up space**"*
     (`crates/core/src/ecs/components/water.rs:396`), and the EXAL sibling
     derives it from real authored data — WATR `NAM0` linear velocity when
     present, otherwise the `wind_direction` angle after the Z→Y swizzle
     (`byroredux/src/env_translate.rs:962-980`). The NIFAL path hands it a
     constant world +X for *every* river/stream/canal mesh. A river mesh placed
     to run north–south gets a current pushing perpendicular to its own channel.
     Note the block already holds the placement rotation (`quat` / `final_rot`)
     — it passes it to `water_volume_from_mesh` on the very next statement — so
     even a local-axis convention would be strictly better-sourced than a world
     constant.
  2. **`spans[1] > 16.0`** (`:211`) — the vertical-extent floor for the
     waterfall geometry fallback. Units are un-stated (they are post-import
     Y-up game units, so 16 is roughly 0.23 m at Skyrim's ~70 units/m) and no
     corpus measurement is cited.
  3. **`spans[1] > horizontal * 1.5`** (`:211`) — the tall-and-narrow aspect
     ratio. Reasoned in prose ("horizontal rivers/lakes have their largest span
     in X/Z") but not measured.
  4. **`radius * 4.0`** (`:236`) — the synthesized underwater depth of a mesh
     water volume, which sets `WaterVolume.min.y` and therefore how deep
     `submersion_system` believes an actor can sink.
  This is precisely the shape the layer's own reference cases are contrasted
  against: the emissive scale is a *measured* no-op and the particle
  `initial_color` is a *deliberate* non-application, both with the evidence
  written down at the site.
- **Evidence**:
  ```rust
  // byroredux/src/material_translate.rs:169-175
  let flow = match kind {
      WaterKind::Calm => None,
      WaterKind::Waterfall => Some(WaterFlow::for_kind(kind, [0.0, -1.0, 0.0])),
      WaterKind::River | WaterKind::Rapids => {
          Some(WaterFlow::for_kind(kind, [1.0, 0.0, 0.0]))   // ← world +X, always
      }
  };

  // byroredux/src/material_translate.rs:209-215
  let horizontal = spans[0].max(spans[2]).max(1.0);
  if spans[1] > 16.0 && spans[1] > horizontal * 1.5 {   // ← both uncited

  // byroredux/src/material_translate.rs:233-239
  min: [center.x - radius, position.y - radius * 4.0, center.z - radius],  // ← uncited
  ```
  The `[0.0, -1.0, 0.0]` waterfall direction is **not** part of this finding —
  "falls are downward in Y-up" is stated as the canonical convention on the
  component doc itself and is self-evidently sourced.
- **Impact**: The flow direction reaches both the shader UV scroll bias and
  `crates/physics/src/water.rs`'s current drag on dynamic bodies and actor swim
  resistance, so a mis-oriented river pushes the player sideways out of the
  channel — a gameplay effect, not just a visual one. The geometry thresholds
  decide River-vs-Waterfall, which in turn decides whether the mesh gets a
  swimmable `WaterVolume` at all (`nif_loader.rs:1055` /
  `mesh_instance.rs:759` skip the volume for `Waterfall`), so a mis-fire
  silently removes swimmability from a body of water.
- **Related**: `feedback_no_guessing` (project memory); `#2872` (the WATR
  `wind_speed` constant-90.0 investigation — the precedent for "a value with no
  variance is not an authored source, say so at the site").
- **Suggested Fix**: For (1), derive the horizontal direction from the mesh's
  own longest horizontal principal axis (the positions array is already in
  hand) rotated by the placement quaternion, or emit no `WaterFlow` at all
  rather than a fabricated one — either is sourced. For (2)–(4), either cite a
  corpus measurement over the installed Oblivion/FNV/Skyrim water meshes at the
  constant, or name them (`WATERFALL_MIN_VERTICAL_SPAN`,
  `WATERFALL_ASPECT_RATIO`, `MESH_WATER_DEPTH_RADII`) with an explicit
  "unmeasured placeholder" note so the next sweep can tell a measurement from a
  guess.

#### NIFAL-D8-2026-08-20-01: `texture_slot_layout` — the new per-game slot discriminator — is assigned on only one of four shader-property branches, so FO4+ effect/sky/water meshes resolve their REFR texture overrides through the Skyrim table
- **Severity**: MEDIUM
- **Dimension**: Shader-flags / texture-role vocabulary
- **Tier Violated**: `no-leak` — the per-game vocabulary fails to collapse for three of four property kinds, and the wrong game's table silently drops or misroutes an authored override
- **Game Affected**: FO4, FO76, Starfield (Skyrim and earlier are unaffected — their correct layout *is* the default)
- **Location**: `crates/nif/src/import/material/dedicated_shader.rs:105` (the only assignment); the missing scene-level assignment belongs at `crates/nif/src/import/material/walker.rs:118`
- **Status**: NEW (introduced in this delta by `86c41022`)
- **Description**: `86c41022` correctly made slot→role resolution game-aware by
  adding `TextureSlotLayout` and threading it through `TextureSlotContext`. The
  layout itself is a pure function of the file's generation —
  `TextureSlotLayout::from_bsver(scene.bsver)`
  (`crates/nif/src/import/material/slot_role.rs:102`) — but it is written into
  `MaterialInfo` at exactly one place: inside the
  `if let Some(shader) = scene.get_as::<BSLightingShaderProperty>(idx)` body.
  `apply_dedicated_shader_property` dispatches to **four** property handlers
  (`apply_bs_lighting_shader`, `apply_bs_effect_shader`, `apply_bs_sky_shader`,
  `apply_bs_water_shader`); the other three never set it, and neither does the
  legacy `NiProperty` chain. A mesh with no `BSLightingShaderProperty` therefore
  keeps `TextureSlotLayout::default()`, which is `Skyrim`
  (`crates/nif/src/import/material/slot_role.rs:91-94`). `crates/nif/src/import/material/mod.rs:1465` then
  copies that wrong value onto `ImportedMaterial`, and
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:116` feeds it straight into
  the `TextureSlotContext` that gates every REFR override.
- **Evidence**:
  ```
  $ grep -n "texture_slot_layout" crates/nif/src/import/material/*.rs
  dedicated_shader.rs:105:        info.texture_slot_layout = slot_layout;   ← the ONLY write
  mod.rs:479:    pub texture_slot_layout: TextureSlotLayout,
  mod.rs:1099:            texture_slot_layout: TextureSlotLayout::default(),   ← = Skyrim
  mod.rs:1465:            texture_slot_layout: self.texture_slot_layout,
  ```
  Consequences for an FO4 mesh whose only property is a
  `BSEffectShaderProperty`, carrying a REFR `XATO`/`XTNM`/`XTXR` override:
  - slot 2 — correct arm is `(Fallout4, 2) => Some(Emissive)` unconditionally;
    the Skyrim arm requires `context.glow_map`, which is also only ever set at
    `dedicated_shader.rs:106`, so it is `false` → `None` → **the override is
    dropped**, not misrouted.
  - slot 3 — correct arm is `(Fallout4, 3) => Some(GreyscaleLut)`; the Skyrim
    arm yields `Height`, so an FO4 palette gradient is bound as a POM height
    field and `triangle.frag`'s POM branch (which gates only on
    `parallaxMapIndex != 0u`) ray-marches over it.
  - slot 5 — correct FO4 arm is `Wrinkle`/`EnvironmentMask` by shader family;
    the Skyrim arm gates on `tint_family`.
  - slot 7 — correct arm is `(Fallout4, 7) => Some(Specular)` unconditionally
    (that is the whole point of #2998); the Skyrim arm additionally requires
    `model_space_normals`, so a specular override on an FO4 effect mesh without
    the almost-never-set MSN flag is **dropped**.
  Note the *import* side is unaffected — `slot_to_role` is only called from
  inside `apply_bs_lighting_shader`, which has the correct local `slot_layout`
  in scope. The defect is confined to the value that leaves the crate.
- **Impact**: Bounded, and I did not measure the population: it needs an
  FO4+/FO76/Starfield mesh whose *only* shader property is
  BSEffect/BSSky/BSWater **and** a REFR texture override on the placement. That
  intersection is small but non-empty (FO4 ships many effect-shader decals,
  holograms and FX cards, and TXST-overridden REFRs are routine). The reason
  this is MEDIUM rather than LOW is structural, not statistical: a per-game
  discriminator that silently defaults to a *different real game* is the exact
  failure mode #2695 was filed for, and the `record_unrouted_texture_slot`
  counter added in the same commit cannot see it — the wrong-table lookups
  either succeed with the wrong role (invisible) or return `None` for a reason
  the counter attributes to the wrong layout bucket. Escalate to HIGH if a
  corpus probe shows FO4 effect-shader meshes with TXST overrides in shipped
  cells.
- **Related**: #2695 (the two-disagreeing-tables defect this table was created
  to fix); #2998 / #3085 / #2999 (the per-game arms that make the layout
  load-bearing); #2697 (OPEN — the third hand-written role walk, same
  "unprotected parallel structure" family).
- **Suggested Fix**: One line at
  `crates/nif/src/import/material/walker.rs:118`, immediately after
  `let mut info = MaterialInfo::default();`:
  `info.texture_slot_layout = TextureSlotLayout::from_bsver(scene.bsver);`
  The layout is a property of the *scene*, not of any one property block, and
  that function already takes `scene`. Leave the assignment in
  `apply_bs_lighting_shader` (harmless, recomputes the same value), or drop it.
  Pin with a test asserting an FO4-bsver mesh carrying only a
  `BSEffectShaderProperty` reports `TextureSlotLayout::Fallout4`.

### LOW

#### NIFAL-D8-2026-08-20-02: `RefrTextureOverlay::apply_slot_swap` is a third slot table, game-agnostic, and its FO4 slot-5 arm reads a lane the FO4 TXST parser never populates
- **Severity**: LOW
- **Dimension**: Shader-flags / texture-role vocabulary
- **Tier Violated**: `single-boundary` (a per-game slot vocabulary re-implemented outside `slot_to_role`)
- **Game Affected**: FO4, FO76, Starfield
- **Location**: `byroredux/src/cell_loader/refr.rs:158-183`
- **Status**: NEW
- **Description**: `apply_slot_swap` maps a raw `XTXR` NIF-slot index onto a
  named `esm::cell::TextureSet` field with a flat, shader-type- and
  game-agnostic match. Its doc justifies the flatness with *"The source TXST
  has already been translated from its different TXnn ordering into named
  roles, so this match is intentionally NIF-role order rather than raw ESM
  index order."* That premise is only half true: the TXST→named-role
  translation is itself **game-dependent**.
  `crates/plugin/src/esm/cell/support.rs:462-471` routes `TX02` to
  `set.wrinkle` for `Fallout4 | Fallout76 | Starfield` and to `set.env_mask`
  otherwise — so on those three games `set.env_mask` is *never* populated,
  while `apply_slot_swap(slot_index = 5)` reads exactly `ts.env_mask`.
  Meanwhile `slot_to_role((Fallout4, 5))` on the tint family resolves to
  `TextureRole::Wrinkle` (`crates/nif/src/import/material/slot_role.rs:301-308`),
  which is the role that lane should have reached.
- **Evidence**:
  ```rust
  // crates/plugin/src/esm/cell/support.rs:462-471
  b"TX02" => { if matches!(game, Fallout4 | Fallout76 | Starfield) { set.wrinkle = path; }
               else { set.env_mask = path; } }

  // byroredux/src/cell_loader/refr.rs:164 + :179   (no `game` in scope at all)
  5 => ts.env_mask.as_deref(),        // ← always None on FO4/FO76/Starfield
  5 => &mut self.env_mask,
  ```
  The non-`XTXR` path is unaffected: `merge_from_texture_set`
  (`byroredux/src/cell_loader/refr.rs:130`) fills `self.wrinkle` from
  `ts.wrinkle` directly, and `byroredux/src/cell_loader/spawn/mesh_instance.rs:172` forwards it unconditionally.
  Only the explicit slot-index swap form loses the binding.
- **Impact**: An FO4/FO76/Starfield REFR that overrides NIF slot 5 via `XTXR`
  is a silent no-op instead of a wrinkle-map swap. Narrow — `XTXR` slot-5 swaps
  on head meshes are the only population — and it fails closed (nothing wrong
  is bound), which is why this is LOW rather than MEDIUM. The maintenance cost
  is the real one: this is a fourth place the slot vocabulary is written down,
  after `slot_to_role`, the FO4 `TX02` branch, and the
  `mesh_instance.rs` `pick(...)` list.
- **Related**: #2695 (the two-table defect); NIFAL-D8-2026-08-20-01 (the same
  "per-game routing decided outside the shared table" root cause);
  #2999 (which introduced the FO4 slot-5 → Wrinkle arm without a matching
  overlay-side path).
- **Suggested Fix**: Give `apply_slot_swap` the game/layout it is missing and
  route slot 5 to `self.wrinkle` when the layout is FO4-family (or, better, add
  `pick(5, o.wrinkle, TextureRole::Wrinkle)` alongside the existing
  `EnvironmentMask` pick in `mesh_instance.rs` and have `apply_slot_swap` write
  slot 5 into both lanes, letting `slot_to_role` remain the sole arbiter).

#### NIFAL-D9-2026-08-20-01: `docs/engine/nifal.md` documents a `translate_material` signature that was narrowed a month ago, and has no record of the mesh-water category that crossed the layer in this delta
- **Severity**: LOW
- **Dimension**: Completeness / cross-cutting
- **Tier Violated**: documentation (the spec is what every NIFAL sweep reads before the code)
- **Game Affected**: n/a
- **Location**: `docs/engine/nifal.md:525` (the stale signature); the whole file (zero occurrences of "water")
- **Status**: NEW — distinct from #3075, which is OPEN and covers the collision
  shape count and the missing `translate_texture_only_material` producer. Neither
  of the two drifts below falls inside that issue's fix scope.
- **Description**: Two new drifts:
  1. **Stale boundary signature.** `nifal.md:525` states the boundary is
     `translate_material(mesh, paths, extra_material_flags) -> Material`. That
     `mesh` parameter was removed on 2026-07-27 by `05d68926`, which narrowed
     the signature to `(source: &ImportedMaterial, mesh_name: Option<&str>,
     paths: ResolvedPaths, extra_material_flags: u32)` — and that narrowing is
     the single most important checkable invariant in Dimension 1, because it
     is what makes "material translation cannot depend on geometry" provable
     rather than merely intended. `.claude/commands/audit-nifal/SKILL.md` records the narrowing
     and names a widening back to `&ImportedMesh` an explicit regression; the
     spec still prints the pre-narrowing form, i.e. it documents the regression
     state as current.
  2. **Mesh water is absent entirely.** a case-insensitive grep for "water" over `docs/engine/nifal.md`
     returns nothing. A whole category began crossing the layer in this delta —
     `ImportedMaterial.is_water_shader` → `WaterPlane` + `WaterMaterial` +
     `WaterFlow` + `WaterVolume`, with four new helpers in the layer's own
     module and two consuming spawn sites. It is documented only in
     `docs/engine/watal.md:343-368`, from the water subsystem's point of view;
     the layer spec's §2 leak inventory and §3 boundary list have no entry, so
     the three findings above (a duplicated composition, a divergent
     kind-derived value, four uncited constants) had no spec to be checked
     against when they landed.
- **Evidence**:
  ```
  $ sed -n '525p' docs/engine/nifal.md
  - **The boundary**: `byroredux/src/material_translate.rs::translate_material(mesh,

  $ grep -n "fn translate_material" -A 5 byroredux/src/material_translate.rs
  268:pub(crate) fn translate_material(
  269:    source: &ImportedMaterial,
  270:    mesh_name: Option<&str>,
  271:    paths: ResolvedPaths,
  272:    extra_material_flags: u32,
  273:) -> Material {

  $ grep -ci water docs/engine/nifal.md
  0
  ```
- **Impact**: Same failure mode as #3075 and its four predecessors: an auditor
  who trusts the spec (as `_audit-common.md` instructs) is told the boundary
  still takes an `ImportedMesh`, and is given no reason to look at mesh water at
  all. This file now has a standing drift problem across six consecutive
  sweeps.
- **Related**: #3075, #2488, #2306, #2301, #2299 (all prior `nifal.md`-drift
  findings).
- **Suggested Fix**: Fold into the #3075 fix: correct the signature at `:525`
  (and the `De-duplication` bullet below it, which still names
  `byroredux/src/cell_loader/spawn.rs` rather than the current
  `byroredux/src/cell_loader/spawn/mesh_instance.rs`), and add a "Mesh water" subsection to §2
  naming the four helpers in `byroredux/src/material_translate.rs`, the two
  consuming spawn sites, and the parked `damage_per_second`.

---

## Documented-limitation ledger (verified parked, NOT findings)

Re-verified at HEAD `bb0b92f2` so the next sweep does not re-report them:

| Item | State | Verification |
|---|---|---|
| 7 raw-tier parked node/mesh fields (`bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`) | parked, zero canonical consumers | Re-grepped each outside `crates/nif/src/import/types.rs`, the parser and `*_tests`: only literal `None` initialisers in `crates/spt/src/import/mod.rs` and the per-game mesh extractors. `parked-not-leak` confirmed. |
| **`MaterialInfo::legacy_shader_type` (NEW this delta, `5661e065`)** | parked, zero canonical consumers | Set at `crates/nif/src/import/material/legacy_properties.rs:306` and `:483`; never copied onto `ImportedMaterial`, never read. Deliberately kept separate from `shader_type` because the FO3 and Skyrim enums collide numerically. Correct `parked-not-leak`. |
| `BhkNPCollisionObject` FO4+/FO76/Starfield packed-Havok blob | documented limitation | `summarize_collision_authoring` census intact; still crosses as three bare `u32`s (`crates/nif/src/import/collision/mod.rs:88-92`). |
| `BhkPCollisionObject` phantoms | documented limitation | Phantom park arms intact in `crates/nif/src/import/collision/shape.rs`. |
| `BhkPlaneShape` → `None` | documented exception | Arm present; 16 resolve arms vs 16 dispatched shapes, cross-checked automatically by `dispatch_coverage_tests` in `crates/nif/src/import/collision/mod.rs:598`. |
| Particle size-over-life *curve*; `initial_color` deliberately unapplied | future work / deliberate | `byroredux/src/systems/particle.rs::apply_emitter_overlays` still the single overlay site, called from `byroredux/src/scene/nif_loader.rs:569` and `byroredux/src/cell_loader/spawn.rs:981` only. |
| Per-light ambient colour + morph-weight animation channels | parked | No canonical consumer. |
| Cell-loader path never builds `SkinnedMesh` (#2440) | recorded, not fixed | Unchanged. |
| `SkinnedMesh.bones: Vec<Option<EntityId>>` (#2441) | terminal sentinel, not a resolve-later leak | Unchanged. |
| Raw-tier `byroredux_nif::anim::AnimationClip` name collision (#2442) | permitted by the tier model | Unchanged. |
| `convert_hkx_clip`'s synthesized cart/furniture exit events (#2305) | declared no-fabrication exception | Unchanged. |
| SLSF1 `Refraction` without `Fire_Refraction` has no consumer (#2327) | documented, not a leak | `material_optical_scalar`'s rationale block intact at `byroredux/src/material_translate.rs:31-64`. |
| Starfield contributes zero `BSShaderTextureSet` bindings (#3057) | explicit format boundary | Documented at `crates/nif/src/import/material/slot_role.rs:17-23`; the `Starfield` arms in `slot_to_role` are therefore inert-by-content, not dead-by-mistake. |
| Starfield `.mat`/CDB arm returns `PresenceOnly`, forwards no authored field (#2709) | Phase-1 deferral | `byroredux/src/asset_provider/material.rs:1063-1116`; rationale block intact and now also covers the `.bgsm`/`.bgem`-named Starfield case (#3053). |
| `byroredux/src/cell_loader/refr.rs::fill_from_bgsm` is a second external-material resolver with no `.mat` arm (#2708) | documented divergence, harmless until CDB Phase 2 | Warning comment intact at `refr.rs:195-215`. |
| `lighting` / `flow` / `wrinkle` GPU role lanes declared but unsampled (#2712, CLOSED) | deliberate deferral | Present in both the Rust struct and `crates/renderer/shaders/include/bindings.glsl` for layout parity. |
| `material_kind: u32` as the `triangle.frag` dispatch contract | deliberate, not a leak | Per spec §1 and the SKILL; not flagged. |
| `MaterialTextureSet::values()` / `secondary_values()` ordering | load-bearing, currently correct | 18 roles + `decals` chain, field-for-field identical to `map_ref`'s literal (`crates/nif/src/import/types.rs:309-390`). No role was added this delta. |

## Open issues matched during dedup (reported previously, still OPEN — skipped, not re-reported)

- **#3075** — `docs/engine/nifal.md` understates the material boundary set and
  the collision shape count. Still true at HEAD: `nifal.md:317` and `:605` say
  "13 parsed `bhk*Shape` variants" against a live count of 16, and
  `translate_texture_only_material` still appears nowhere in the file.
- **#3074** — the stated blocker for dropping `flame_attach_offset` on the
  streaming path is false. Unchanged.
- **#3073** — `parallax_height_scale` / `parallax_max_passes` bypass the
  canonical `Material`. **Unchanged and re-verified**: the identical
  `unwrap_or(0.04)` / `unwrap_or(4.0)` pair is still at
  `byroredux/src/scene/nif_loader.rs:1023-1024` and
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:720-721`, with the
  per-draw third copy still in `byroredux/src/render/static_meshes.rs`.
- **#3072** — `finish_partial_import` hardcodes `furniture: None`. Unchanged.
- **#2697** — `supplemental_texture_indices` is a third hand-written role walk
  with no lockstep test.
- **#2532** — the canonical-tier completeness harness covers 1 of ~5 declared
  translate boundaries. Unchanged: `crates/nif/tests/translation_completeness.rs`
  was not touched in this delta and still measures `Material` fill-rate only.
- **#2490** — raw-material → marker-component block copy-pasted at both spawn
  sites. (Directly adjacent to D1-01, which is the same defect for the newer
  mesh-water component group; kept separate because the fix sites differ.)
- **#2571** — three raw-tier `ImportedMaterial` fields re-read at each spawn
  site.
- **#2533** — BGEM v21+/v22 glass-overlay texture paths have no
  `MaterialTextureSet` role.
- **#2440** — cell-loader path never builds `SkinnedMesh`.

## Closed issues re-verified as fixed (no regression)

- **#2549** — `bhkRigidBody.havok_filter` is no longer dropped at the boundary
  (`00fc0f3b`, the non-collidable Havok layer is honoured). This was on the
  2026-08-16 OPEN-skip list; it closed inside the delta.
- **#2555** — `classify_pbr_keyword`'s env-map-arm reachability claim corrected
  (`e26bafd3`); the corrected reachability is now cited from
  `translate_texture_only_material`'s own rationale block.
- **#2556** — `Material::default().emissive_mult` aligned with
  `EmissiveSource::None`'s doc (`5b8f2123`).
- **#2626 / #2627 / #2700 / #2601** — the BGEM glass-behaviour bit, BGSM
  `inner_layer_texture` wiring, unconditional `is_pbr` for BGSM resolves, and
  merge-site resolve-failure logging all landed in
  `byroredux/src/asset_provider/material.rs` and are present at HEAD.
- **#2693 / #2694 / #2695** — the three 2026-08-12 slot-table findings remain
  closed and are now *reinforced*: the shared table gained a per-game
  `TextureSlotLayout` dimension (`86c41022`) with per-arm occupancy evidence and
  a `record_unrouted_texture_slot` counter for future gaps.
- **#2999 / #2998 / #3085** — FO4 slots 4/5 (cubemap + wrinkle), FO4 slot 7
  specular without MSN, and FO76 slot 6 specular are all present as measured
  arms in `crates/nif/src/import/material/slot_role.rs`.
- **#2444** — every exterior draw population still gets a boundary-produced
  `Material`, pinned by `every_exterior_spawner_inserts_a_boundary_material`
  (`byroredux/src/material_translate.rs:1198`).

## Candidates raised and disproved (recorded so the next sweep does not re-chase them)

- **`translate_material` was widened back to `&ImportedMesh`.** False. The
  signature at `byroredux/src/material_translate.rs:268-273` is still
  `(&ImportedMaterial, Option<&str>, ResolvedPaths, u32)`. Only the *spec doc*
  says otherwise (D9-01).
- **`water_kind_from_mesh_geometry` re-widens material translation to
  geometry.** Not a Dimension-1 regression: it is a separate function producing
  a separate canonical type (`WaterKind`), and `translate_material` itself never
  sees the positions array. The geometry dependency is legitimate for a
  *spatial* classification; what is wrong there is the uncited thresholds
  (D1-03), not the input type.
- **`cornell.rs` is a second `Material` materialization site.** Still false —
  its single `translate_material` call is at line 1783, inside the
  `#[cfg(test)]` module that starts at line 1638.
- **`BhkSimpleShape` has no resolve arm.** Still false; the grep hit is
  `BhkSimpleShapePhantom`, a phantom. 16 arms vs 16 dispatched shapes.
- **The mesh-water `world.get::<Material>(entity).unwrap()` immediately after
  `world.insert(entity, material)` is a leak.** It is a needless round-trip
  through the world (and an `unwrap`), but it reads the *canonical* component
  after the boundary produced it, which is the correct tier. Code-quality only;
  belongs to `/audit-tech-debt`, not here.
- **The Starfield `.mat` arm's `has_starfield_cdb()` gate makes translation
  depend on provider runtime state.** True but deliberate and documented
  (`byroredux/src/asset_provider/material.rs:1056-1060`): the gate exists to
  stop modded sidecars PBR-routing against non-Starfield archives, and the
  engine loads one game's archives per process. Not a leak.
- **`damage_per_second: 0.0` on mesh water is a dropped authored field.** No
  authored source exists — mesh water has no WATR record, which is the whole
  premise of the mesh-water slice. The literal is honest; what is wrong is that
  it is written *twice* (folded into D1-01).
- **`WaterKind` has two divergent name-token vocabularies (EXAL demotes
  "waterfall" to River, NIFAL promotes it to Waterfall).** Deliberate and
  documented at `docs/engine/watal.md:355-358` — horizontal cell planes named
  "…Falling…" are pools, dedicated vertical meshes are sheets. Not a finding.
  (The `canal` token existing only on the NIFAL side is a trivial asymmetry with
  no observed content behind it; not reported.)

---

TALLY: CRITICAL=0 HIGH=0 MEDIUM=4 LOW=2
