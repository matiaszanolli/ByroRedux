# Legacy Compatibility Audit — 2026-09-06

**Base:** `a8233f2f` · **Type:** full `/audit-legacy-compat` sweep, all 7
dimensions, run solo in-process (no sub-agent fan-out).

## Scope

All seven dimensions: coordinate-system correctness (Z-up→Y-up), NIFAL
cross-layer mapping shape, the material translation boundary, PHYSAL's source
axis, EXAL/WATAL, per-game translation-survey patterns (A/B/C), and subsystem
coverage vs the legacy engines.

**Delta weighting.** 497 commits since the prior sweep
(`docs/audits/AUDIT_LEGACY_COMPAT_2026-08-30.md`, base `64f64480`) — 1,297
files, the largest window this audit has ever covered. The churn is *not*
concentrated on the three boundaries: `material_translate.rs` took 10 commits,
`env_translate.rs` 5, `ragdoll.rs` 3, and `crates/core/src/math/coord.rs`,
`crates/nif/src/import/coord.rs` and `byroredux/src/cell_loader/euler.rs` took
**zero**. The volume landed in new or heavily-reworked feeders:
`cell_loader/spawn/mesh_instance.rs` (+529), `cell_loader/load.rs` (+503),
`crates/plugin/src/esm/records/weather.rs` (+406), the new
`crates/nif/src/import/mesh/skeleton.rs` (+460) and `mesh/normal.rs` (+222),
`cell_loader/references/attach.rs` (+407), `render/mod.rs` (+384). Every
single-producer contract was re-traced against HEAD rather than carried
forward.

**Source-availability statement.**

| Reference | Status |
|---|---|
| Gamebryo 2.3 source (`/media/matias/Respaldo 2TB/…/Gamebryo_2.3/`) | **UNMOUNTED** — not consulted. |
| `/mnt/data/src/reference/` (gamebryo-v26, gamebryo-v32, nifxml, havok-2007/2013, openmw, nifly, Champollion, …) | **Present** — available; consulted only where a finding needed it. |
| Vanilla archives (Oblivion, FO3, FNV, Skyrim SE, FO4, Starfield) | **MOUNTED this run** (first time in three sweeps) — but **no corpus census was run**: only ~8 GB RAM was free against 29 GB total, and whole-ESM parsing in `byroredux-plugin` has killed this session three times before (`plugin_ignored_tests_oom`). Occupancy-dependent premises say so explicitly. |

**Method.** Static analysis. Each dimension re-traced from HEAD by symbol grep
+ targeted reads; no engine launch, no cargo test run, no sub-agent delegation.
Deduplicated against the **135 open GitHub issues** fetched live this run, and
against `docs/engine/{nifal,exal,physal,watal,per-game-translation-survey}.md`.
`.claude/commands/_audit-validate.sh` → **OK: all path references valid**. No
source file, game file, or GitHub issue was modified.

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 2 |
| **Total (new, owned by this report)** | **3** |

Plus, not counted in the totals:

| Class | Count | Detail |
|---|---:|---|
| Prior sweep findings **verified FIXED** | 3 | all of #3795, #3796, #3797 are CLOSED and re-verified at HEAD |
| **Existing** open issues re-verified, not re-filed | 3 | #3307, #3142, #3930 |
| Stale candidates investigated and **dropped** | 6 | enumerated below |

**The three layers are structurally intact under the largest delta this audit
has seen.** Re-traced at HEAD: `translate_material` is still the single
populated-`Material` producer (four callers, one function); `env_translate` is
still the single exterior-translate site; the PHYSAL source axis carries **zero**
`GameKind` / `game ==` in `crates/nif/src/import/collision/`,
`crates/nif/src/blocks/collision/`, `crates/physics/src/` or
`byroredux/src/ragdoll.rs`; the coordinate helpers remain the sole axis-swap
site with no duplicated `(x, z, -y)` anywhere; and the shaders still contain no
per-game branch (only per-game *comments* naming which title authored a field).

**This sweep's finding is a first for this audit: a real, code-level EXAL leak
rather than a documentation defect.** For three consecutive sweeps the highest
finding was about the audit's own reference material. This time the boundary
itself drops authored data: `parse_wthr` decodes FO4/FO76's ten-value
`WeatherHeightFog` block (form versions 119/120) and Skyrim/FO4/FO76's fog
falloff powers, and `translate_exterior_cell_lighting` / `translate_weather`
forward **neither** — while `composite.frag`'s height-fog branch runs on a
hardcoded `DEFAULT_SCALE_HEIGHT_METERS = 30.0`. The tell that this is a leak and
not a deferral: the *sibling* fields read by the same `SubReader` in the same
`FNAM` arm (`fog_day_max` / `fog_night_max`) **are** consumed, three lines away.

### Skill-vs-code discrepancies (reported per dispatch instruction)

Four places where `audit-legacy-compat/SKILL.md` disagrees with HEAD. The code
was trusted, the check run anyway, the outcome recorded:

1. **Dimension 3** — *"both spawn paths delegate (`byroredux/src/cell_loader/spawn.rs`
   + `byroredux/src/scene/nif_loader.rs`, both calling
   `material_translate::translate_material`)"*. `spawn.rs` no longer calls it;
   the call moved to `byroredux/src/cell_loader/spawn/mesh_instance.rs:837`.
   There are now **four** production callers, not two — the two named ones plus
   `cell_loader/placement_lod.rs:542` (Oblivion `_far.nif`, #1726) and
   `cornell.rs:2073`. The *contract* is intact (one producer function), but the
   bullet's caller list is two names and one path out of date. The path gate
   cannot catch this: `spawn.rs` still exists as a file.
2. **Dimension 5, LOD** — the backticked symbol `LodBandSelection::coarsen_to_available`
   is described as a method (*"Fixed via …"*). It is a **struct field**
   (`lod_bands.rs:281`, `pub(crate) coarsen_to_available: bool`), set `true` at
   the three `object_lod.rs` sites and `false` at `terrain_lod.rs:273`. The
   behaviour the bullet describes is real and correct; the symbol kind is wrong,
   which is why it shows up as a miss on a `fn` search.
3. **Dimension 3, single-producer wording** — the bullet reads as though a
   second *caller* would be a violation. Four callers now exist and none is a
   violation. The invariant that actually matters, and that this sweep verified,
   is "no site constructs a populated `Material` outside the boundary": all 18
   `Material { … }` literals outside `material_translate.rs` are inside
   `#[cfg(test)]` modules (verified per file against each module's `#[cfg(test)]`
   offset — `render/mod.rs:120`, `render/static_meshes.rs:981`,
   `commands/assets.rs:181`, `save/validate.rs:808`, plus `cornell.rs`, the
   `--cornell` harness, which routes through `translate_material` for its
   authored materials).
4. **Dimension 2, NIFAL leak inventory** — the bullet says to cross-check
   against `nifal.md` §2 "so audits stop re-filing" closed leaks. §2's Skinning
   entry has no record of the largest skinning change since it was written
   (#3549). Filed as LC-2026-09-06-D2-01.

### Per-dimension finding counts (every dimension enumerated)

| Dimension | CRIT | HIGH | MED | LOW | Findings |
|---|---:|---:|---:|---:|---|
| 1. Coordinate-system correctness (Z-up→Y-up) | 0 | 0 | 0 | 0 | **none — clean** |
| 2. NIFAL — canonical NIF→ECS mapping shape | 0 | 0 | 0 | 1 | LC-2026-09-06-D2-01 |
| 3. Material translation boundary | 0 | 0 | 0 | 0 | **none — clean** (1 skill-drift item) |
| 4. PHYSAL — per-game Havok → solver (source axis) | 0 | 0 | 0 | 0 | **none — clean** |
| 5. EXAL / WATAL — exterior + water → renderer & solver | 0 | 0 | 1 | 1 | LC-2026-09-06-D5-01, LC-2026-09-06-D5-02 (2 Existing: #3307, #3142) |
| 6. Per-game translation-survey gaps (Pattern A/B/C) | 0 | 0 | 0 | 0 | **none — clean** (see the Pattern A note) |
| 7. Subsystem coverage vs legacy | 0 | 0 | 0 | 0 | **none — clean** (prior LOW #3797 verified fixed) |

---

## Dimension 1: Coordinate-system correctness (Z-up → Y-up)

**Clean.** Zero commits touched `crates/core/src/math/coord.rs`,
`crates/nif/src/import/coord.rs` or `byroredux/src/cell_loader/euler.rs` in the
497-commit window, and nothing re-derived their work elsewhere.

| Check | Result |
|---|---|
| Duplicated `(x, z, -y)` swap outside `coord.rs` | **zero** — regex over `crates/` + `byroredux/` for a `Vec3::new(_.x, _.z, -…)` shape returns nothing outside the helper |
| Single source of truth | `zup_to_yup_pos`, `zup_to_yup_quat_wxyz`, `euler_zup_to_quat_yup`, `euler_zup_to_quat_yup_mode`, `normalize_quat`, `cell_grid_to_world_yup` all present, one definition each; the NIF-typed wrappers `zup_point_to_yup` / `zup_matrix_to_yup_quat` likewise |
| REFR Euler convention | `euler_zup_to_quat_yup_refr` is the sole REFR entry; six call sites (`refr.rs:631`, `references/mod.rs:499`, `placement_lod.rs:199`, `transition.rs:358`, `light_anim.rs`, the `cell_loader.rs` re-export), no site re-derives the ZYX product or hardcodes a mode. `transition.rs:651` carries a source-scan guard asserting its wrapper actually calls the dispatcher. Both pins (`euler_multi_axis_matches_openmw_objectpaging`, `euler_zyx_order_pinned_by_rx_then_rz`) present |
| Winding | `to_triangles` no longer contains winding logic at all — #2298 collapsed it into `crate::blocks::strip::destrip`, shared by `NiSkinPartition` and `resolve_compressed_mesh`'s chunk walk. One implementation, three consumers |
| Projection Y-flip | `camera.rs::projection_matrix` — `proj.col_mut(1).y *= -1.0`, comment intact |
| Exterior grid | `EXTERIOR_CELL_UNITS` is the only `4096.0` doing cell math. Every other hit is a test, a doc comment, an unrelated ray/fog distance (`render/mod.rs:23`, `systems/locomotion.rs:39`, `systems/cinematic.rs:334` — the cart-route continuation distance already adjudicated by the prior sweep), or a derived alias (`scene_buffer/constants.rs:379` `RENDER_ORIGIN_SNAP = …::EXTERIOR_CELL_UNITS`, pinned equal at `:440`) |

New this window and checked: `crates/nif/src/import/mesh/normal.rs` synthesises
normals **after** the Z-up→Y-up conversion (`positions_yup`, doc at `:31`),
matching `synthesize_tangents_yup`'s convention — it does not add a second
conversion.

**Two matrix→quaternion sites** exist outside `coord.rs`
(`crates/nif/src/import/collision/mod.rs:592`, `crates/physics/src/ragdoll.rs:666`),
both `Quat::from_mat3(…).normalize()`. Neither is a Z-up→Y-up conversion —
the first converts an already-Y-up Havok transform, the second builds a
solver-space basis from orthonormal columns. Not the #1044 duplication class.

---

## Dimension 2: NIFAL — canonical NIF→ECS translation contract (mapping shape)

**One LOW.** The three-tier shape holds; the layer *spec* has fallen behind it.

### Clean axes re-verified

- **Single `translate()` per category** — no second producer appeared. The
  material slice is Dimension 3. `SkinnedMesh::new_with_global` still has
  exactly **one** production call site (`scene/nif_loader.rs:1264`); every other
  hit is a test or the renderer's own skin-compute fixture.
- **New slice, correctly placed.** `crates/nif/src/import/mesh/skeleton.rs`
  (#3549, +460) recovers Starfield bone names that are absent from the file
  entirely — 78,587 of 107,717 vanilla `BSSkin` bone refs are NULL — by solving
  the per-file bind offset against an external skeleton and **declining unless a
  unique offset matches every bone**. It sits inside the raw tier (`skin.rs`
  calls it while building `ImportedSkin`), fed by the same `MeshResolver` both
  load paths already pass. It is not a second canonical boundary, and its
  decline-or-be-right contract (measured: 8,708 exact, 349 position-coincident,
  **zero wrong** over 9,057 recovered names) is the right shape for the tier.
- **No `Option` resolve-later leak** introduced downstream.
- **No per-game branch downstream of a boundary.** `crates/renderer/src` and
  `byroredux/src/render/` contain no `GameKind` outside
  `volumetrics.rs:3300-3341`, which is a *guard test* asserting the strings
  `"GameKind"`, `"Fallout"`, `"Skyrim"` do **not** appear in the shader source.

### LC-2026-09-06-D2-01 (LOW) — `nifal.md`'s Skinning leak inventory has no entry for the #3549 external-skeleton slice, so the layer spec no longer describes its own largest skinning change

- **Severity**: LOW
- **Dimension**: 2 — NIFAL mapping shape
- **Location**: `docs/engine/nifal.md:140-183` (§2, "Skinning — **half-stale
  (2026-08-07): loose-NIF path only**"), vs `crates/nif/src/import/mesh/skeleton.rs`
  and `crates/nif/src/import/mesh/skin.rs:405-425`
- **Status**: NEW
- **Description**: §2 is the inventory this audit's own Dimension 2 instructs
  every auditor to cross-check against "so audits stop re-filing" closed leaks.
  Its Skinning entry describes exactly one thing: the #2440 cell-loader gap
  (cell-placed skinned REFRs get no palette binding). That entry is still
  **accurate** — re-verified this run, `SkinnedMesh::new_with_global` has one
  production caller and the cell loader has no `node_by_name` map. But the
  entry is no longer *complete*: #3549 added a whole sub-slice to the skinning
  category — external-skeleton bone-name recovery for a game where 73% of
  authored bone refs are NULL — and §2 does not mention it, name
  `skeleton.rs`, or record its decline contract. An auditor reading §2 to
  decide whether Starfield skinning is converged, gapped, or parked gets no
  answer, and the module's own header is the only place the census and the
  zero-wrong validation live.
- **Evidence**: `nifal.md:140` heading is dated `2026-08-07`; `git log` shows
  `crates/nif/src/import/mesh/skeleton.rs` created after it, and
  `git log --since=2026-08-30 -- docs/engine/nifal.md` shows the file was not
  touched in the window. Grep for `skeleton.rs`, `3549`, `resolve_external_bone_names`
  or `BSSkin` in `nifal.md` → no hits.
- **Impact**: Documentation only. The risk is the one §2 exists to prevent: a
  future sweep re-deriving Starfield skinning status from scratch, or filing
  "Starfield NPCs render in bind pose" against code that fixed it. Related
  open issue #3930 proposes a better input for the same solve (`SkinAttach`
  carries the authored names for 100% of the skins #3549 solves geometrically)
  — a reader of §2 alone would not connect the two.
- **Related**: #3549 (the slice), #3930 (`SkinAttach` alternative, OPEN),
  #2440 (the cell-loader gap §2 *does* record), #2441 (the `Option` residual
  note directly below it).
- **Suggested Fix**: Add a Starfield sub-entry under §2 Skinning naming
  `skeleton.rs`, its decline-or-be-right contract, the measured accuracy, and
  a pointer to #3930 as the open follow-up. Re-date the section heading.

---

## Dimension 3: Material translation boundary (NIFAL reference slice)

**Clean.** No finding. This is the boundary with the HIGH severity floor, and
it survived 10 commits plus a +529-line rewrite of its principal caller.

| Invariant | Result at HEAD |
|---|---|
| `translate_material` is the sole populated-`Material` producer | **Holds.** One definition (`material_translate.rs:471`). Four production callers: `cell_loader/spawn/mesh_instance.rs:837`, `scene/nif_loader.rs:983`, `cell_loader/placement_lod.rs:542`, `cornell.rs:2073`. `translate_texture_only_material` (terrain, water ×2, terrain LOD) is a sibling constructor **inside the same boundary module**, not an outside producer |
| No populated `Material` literal outside the boundary | **Holds.** All 18 outside literals are `#[cfg(test)]` — verified per file against each module's `#[cfg(test)]` line, not assumed from the filename |
| `metalness`/`roughness` are resolved `f32`, no per-draw fallback | **Holds.** `Material::resolve_pbr` (`material.rs:1286`) is the only filler; `classify_pbr_keyword` is the surviving free function. The deleted per-draw `Material::classify_pbr` survives **only** in explanatory doc comments (`material.rs:839,1246,1497`) — none is a call |
| Glass/cloth/metal classified once, alpha-aware | **Holds.** `classify_glass_into_material` defined once (`helpers.rs:69`), called from `material_translate` after `resolve_pbr` |
| Second `Material` producer: `save/driver.rs::restore_world` | **Known and gated.** `driver.rs:138-165` names itself the second renderer-bound producer (#2687 / SAFE-D9-01) and sweeps every restored `Material` through `sanitize_finite()` before returning. Deliberate, documented, guarded — not a leak |
| Emissive scale | Untouched. Per `nifal.md` §4 the three `EmissiveSource` variants share a ~1.0 scale; a "unify emissive scale" finding remains a false premise |
| `NiFogProperty` | Still parsed, still deliberately not dispatched (`legacy_properties.rs`). Not re-filed |

---

## Dimension 4: PHYSAL — per-game Havok articulation → solver (source axis)

**Clean.** No finding owned here.

- **The per-game seam is still only the constraint CInfo decode.**
  `crates/nif/src/blocks/collision/constraints.rs` carries the three typed
  decoders — `RagdollCInfo` (`:66`), `LimitedHingeCInfo` (`:152`),
  `PrismaticCInfo` (`:316`) — each with its `parse_fo3` / `parse_oblivion` pair,
  plus `parse_fo3_malleable_inner` (`:468`). #3792's prismatic arm and
  `BhkBreakableConstraint` wrapped-geometry retention landed inside this seam,
  not beside it.
- **Zero `GameKind` / `game ==` / `game_kind`** in
  `crates/nif/src/import/collision/`, `crates/nif/src/blocks/collision/`,
  `crates/physics/src/`, or `byroredux/src/ragdoll.rs`. `extract_ragdoll` is
  still game-agnostic by construction.
- **One translate, one build.** `template_from_imported` and `activate_ragdoll`
  are single-definition; `build_ragdoll` is the single solver boundary;
  `ragdoll_writeback_system` still writes bone `GlobalTransform`s only.
- **Coverage improved inside the window.** #3921 (`a8233f2f`, HEAD) added the
  missing FO3 arm to `crates/nif/tests/ragdoll_import.rs`, so all four
  classic-chain games (Oblivion / FO3 / FNV / Skyrim) now have a real-data
  threading gate. Measured there: FO3 `_male\skeleton.nif` → 18 bodies,
  17 joints, 1 connected component — identical to the FNV reference. #3655
  hoisted `LocalBound`/`WorldBound` above `PhysicsWorld` in writeback lock
  order; #3492 gave the buoyancy sink a `Ragdoll` pass.
- **Documented limitations not re-filed**: FO4/FO76/Starfield ragdolls blocked
  on `BhkSystemBinary` (open as #3809), the cone+2-plane → per-axis limit
  approximation, captured-but-unused motors.

---

## Dimension 5: EXAL / WATAL — per-game exterior environment → renderer & solver

**One MEDIUM, one LOW.**

### Clean axes re-verified

- **Single boundary holds.** `env_translate.rs` remains the sole exterior
  translate site: `default_water_for_worldspace`, `resolve_water_material`,
  `translate_sky`, `translate_weather`, `translate_exterior_cell_lighting`,
  `translate_lod_water`, `translate_terrain_lod_textures`, `climate_tod_hours`,
  and the three `procedural_fallback_*` constructors — one definition each.
- **No second producer.** Every `SkyParamsRes { … }` / `WeatherDataRes { … }` /
  `WaterMaterial { … }` literal outside `env_translate.rs` /
  `material_translate.rs` is inside a `#[cfg(test)]` module — checked per file
  against its `#[cfg(test)]` offset (`systems/weather.rs:547`,
  `systems/character.rs:1198`, `crates/physics/src/water.rs:1106`,
  `render/sky.rs`).
- **No render-time fallback.** The no-climate case is still the explicit
  canonical `procedural_fallback_*` path, with `FB_TOD_HOURS` the single
  declaration shared by all three fallback producers (#2812).
- **Per-game LOD predicates re-checked** and again not a leak: `lod_bands.rs`,
  `lod_support.rs` and `placement_lod.rs` hold named GameVariant tables, each
  answering a distinct question and each test-pinned. Same adjudication as the
  prior sweep's dropped candidate #4.

### LC-2026-09-06-D5-01 (MEDIUM) — FO4/FO76 authored height-fog is decoded into `WeatherHeightFog` and dropped at the EXAL boundary, while the shader's height-fog branch runs on a hardcoded 30 m scale height

- **Severity**: MEDIUM (translatable block silently dropped by the exterior
  translate boundary, with a live canonical sink)
- **Dimension**: 5 — EXAL exterior environment → renderer
- **Location**: `byroredux/src/env_translate.rs` (`translate_exterior_cell_lighting`
  and `translate_weather` — neither reads `wthr.fog_height`);
  `crates/plugin/src/esm/records/weather.rs:496-510` (the decode);
  `crates/renderer/src/vulkan/context/draw.rs:811-821` (the sink);
  `crates/renderer/src/vulkan/volumetrics.rs:328`
  (`DEFAULT_SCALE_HEIGHT_METERS = 30.0`)
- **Status**: NEW
- **Description**: `parse_wthr`'s `FNAM` arm decodes FO4/FO76's ten-value
  height-fog extension (day/night near+far height mid and range, day/night high
  density scale — the fields xEdit attributes to form versions 119 and 120)
  into `WeatherRecord::fog_height: Option<WeatherHeightFog>`. **Nothing outside
  the parser ever reads that field** — a repo-wide grep for `fog_height` returns
  only `crates/plugin`'s own decode plus `fog_height_reference`, an unrelated
  render constant (the #2225 ground-anchor Y). Meanwhile `composite.frag` has a
  live height-fog branch (`height_fog_params`, gated
  `is_exterior && extinction > 0`), and `draw.rs:813` fills its scale-height lane
  with the engine constant `DEFAULT_SCALE_HEIGHT_METERS * WORLD_UNITS_PER_METER`
  for **every** game. So the authored quantity and the consumed quantity are the
  same physical parameter, decoded on one side of the boundary and hardcoded on
  the other.
- **Evidence**: the decode, `weather.rs:496-510` —
  `record.fog_height = Some(WeatherHeightFog { day_near_height_mid: …, day_high_density_scale: …, … })`,
  gated `matches!(game, Fallout4 | Fallout76) && sub.data.len() >= FO4_FNAM_SIZE`,
  pinned by `parse_fo4_fnam_retains_all_eighteen_floats` and by
  `starfield_long_fnam_does_not_assume_the_fo4_tail_schema` (which asserts an
  unverified Starfield tail must *not* be decoded as FO4 height fog — evidence
  the schema was taken seriously). The sink, `draw.rs:811`:
  ```rust
  height_fog_params: [
      fog_extinction_per_meter.max(0.0) / volumetrics::WORLD_UNITS_PER_METER,
      volumetrics::DEFAULT_SCALE_HEIGHT_METERS * volumetrics::WORLD_UNITS_PER_METER,
      fog_single_scatter_albedo.clamp(0.0, 1.0),
      if sky_params.is_exterior && fog_extinction_per_meter > 0.0 { 1.0 } else { 0.0 },
  ],
  ```
  **The discriminating evidence** that this is a leak rather than a deferral:
  `fog_day_max` / `fog_night_max` are read by the *same* `SubReader` in the
  *same* `FNAM` arm, three lines earlier — and `translate_weather` /
  `translate_exterior_cell_lighting` both forward them into
  `FogMedium::from_legacy_ramp(near, far, Some(max))`. The boundary consumes
  half of one subrecord's tail and drops the other half without a note.
- **Impact**: On FO4 and FO76 exteriors the fog's altitude profile is the
  engine's 30 m default instead of the authored per-weather one, for every
  weather, in every worldspace. Fog still renders — nothing is missing or
  black — so this is a wrong-value divergence, not a dropout, which is what
  keeps it at MEDIUM rather than escalating. No other game is affected: the
  block is FO4/FO76-only by decode gate, and the Starfield tail is
  deliberately not assumed.
- **Confidence**: The code path is certain (decode present + zero consumers +
  hardcoded sink, all three grep-verified). **The occupancy is not measured** —
  no census was run this sweep (memory headroom; see the source-availability
  statement), so "how many vanilla FO4 weathers actually ship the 72-byte
  `FNAM`" is unknown, and the existing tests are synthetic. If vanilla FO4
  authors the extension rarely, real-world blast radius is smaller than the
  code path suggests. It is rated MEDIUM on the boundary defect, which is
  occupancy-independent, not on assumed content.
- **Related**: #1926 / #1927 (the composite fog branch that was removed as
  unreachable — different fields, same fog subsystem); LC-2026-09-06-D5-02
  below (the `fog_*_power` half of the same tail); `docs/engine/exal.md` §2
  "Weather / TOD — canonical; the cleanest of the dynamic categories", which
  this contradicts.
- **Suggested Fix**: Carry `WeatherHeightFog` onto `WeatherDataRes` /
  `CellLightingRes` as a canonical `Option`, TOD-lerped like
  `skyrim_dalc_per_tod` already is, and let `draw.rs` prefer it over
  `DEFAULT_SCALE_HEIGHT_METERS` when `Some`. Keep the constant as the
  no-authored-data fallback — that is the EXAL-correct shape (an explicit
  canonical default, not a render-time branch). Census FO4/FO76 `FNAM` sizes
  first to size the work.

### LC-2026-09-06-D5-02 (LOW) — Skyrim/FO4/FO76 fog falloff power is decoded and dropped at the same boundary, into a canonical field that exists and reaches the GPU

- **Severity**: LOW (capture-completeness; the canonical field is currently
  shader-unconsumed, so filling it changes nothing today)
- **Dimension**: 5 — EXAL exterior environment → renderer
- **Location**: `byroredux/src/env_translate.rs::translate_exterior_cell_lighting`
  (hardcodes `fog_power: None`); decode at
  `crates/plugin/src/esm/records/weather.rs:490-491` (FO4/FO76) and `:894-895`
  (Skyrim); canonical field `byroredux/src/components.rs:470`
- **Status**: NEW
- **Description**: `WeatherRecord::fog_day_power` / `fog_night_power` are
  decoded from `FNAM` on Skyrim, FO4 and FO76. `CellLightingRes::fog_power`
  exists as the canonical landing site, is copied into the frame
  (`components.rs:528`), and is uploaded to the GPU as `fog_params[3]`
  (`draw.rs:793`). `translate_exterior_cell_lighting` nonetheless writes
  `fog_power: None` unconditionally, under a comment that explains only the
  *XCLL* source ("the extended XCLL tail applies to interior cells") and does
  not mention that WTHR carries its own authored value for exteriors.
- **Evidence**: `env_translate.rs` — `fog_power: None,` with the `#861` comment;
  `weather.rs:894-895` — `record.fog_day_power = r.f32().unwrap_or(1.0);` in the
  Skyrim 32-byte `FNAM` arm, pinned at `weather.rs:1257-1258` (0.45 / 0.25).
- **Impact**: **None visible today.** `draw.rs:1508-1523` documents `fog_clip`
  and `fog_power` as "**Currently unconsumed** (#1926, #1927)" — the
  `composite.frag` branch that read the curve was removed once
  `VOLUMETRIC_OUTPUT_CONSUMED` made it unreachable, and the fields are
  "reserved for a future interior-scoped composite branch". So this is a gap in
  what the boundary *captures*, not in what renders. It is filed separately
  from D5-01 precisely so the two are not conflated: D5-01 has a live sink,
  this does not.
- **Related**: LC-2026-09-06-D5-01 (same subrecord tail), #1926, #1927.
- **Suggested Fix**: Fill `fog_power` from `wthr.fog_day_power` in
  `translate_exterior_cell_lighting` when the game decodes it, so the value is
  already canonical if and when the interior-scoped composite branch lands.
  One line; the alternative (leave it and note why) is also defensible, in
  which case the `#861` comment should say "WTHR's own power is intentionally
  not forwarded because the field is shader-unconsumed", which it currently
  does not.

### Existing (deduped, not re-filed)

- **#3307** — EX-10/11 item 8, active VWD full-model culling. Re-verified: the
  flag is parsed (`RecordHeader::is_visible_when_distant`), stamped
  (`stamp_visible_when_distant`), and read by the reconcile loop
  (`resident_vwd_refr_cells`, 6 sites). The *cull* is still unwired. Unchanged.
- **#3142** — `resident_vwd_refr_cells` takes a fresh storage read-lock per VWD
  entity. Unchanged at HEAD.
- **#3930** — `SkinAttach` carries authored bone names for the skins #3549
  solves geometrically. Adjacent to LC-2026-09-06-D2-01; that finding is about
  the spec, this issue is about the input. Not merged.

---

## Dimension 6: Per-game translation-survey gaps (Pattern A/B/C)

**Clean.** No finding.

- **Pattern A (hardcoded BSVER literals where a named constant should be):**
  **zero** in production. Every `bsver()` comparison outside `#[cfg(test)]`
  reads against a named `crate::version::bsver::*` constant (e.g.
  `particle.rs:370` `> bsver::NI_BS_LTE_16`, `:391` `>= bsver::FO3_FNV`). Of the
  59 raw `bsver()` occurrences, the ones with bare numeric literals are all in
  `version.rs`'s own round-trip test (`:1171-1180`) or in comments describing
  history.
- **The prior sweep's MEDIUM on this dimension (#3795) is CLOSED** — the
  `per-game-translation-survey.md` §5 Pattern A prescription that inverted the
  settled no-`NifVariant`-predicates doctrine has been corrected. Re-verified
  this run rather than assumed: the deleted helpers have not reappeared, and
  `version.rs`'s doctrine note still records the deletion as fully enforced.
- **Pattern B / C** unchanged: the wire format discriminates, and variant-enum
  struct shapes carry the divergent records.

---

## Dimension 7: Subsystem coverage vs legacy

**Clean.** No finding. The prior sweep's LOW is fixed.

- **LC-2026-08-30-D7-01 → #3797 CLOSED, verified fixed at HEAD.**
  `NiAlphaProperty` bit 13 ("No Sorter") is now decoded to a typed
  `no_sorter: bool` on `ImportedMaterial` (`crates/nif/src/import/types.rs:613`),
  documented against `MaterialInfo::no_sorter`, and threaded into the renderer's
  draw-sort input (`crates/renderer/src/vulkan/context/mod.rs:98-104`). The
  engine no longer back-to-front-sorts draws whose author opted out.
- **Property → pipeline mapping**: `NiAlpha`, `NiMaterial`, `NiTexturing`,
  `NiVertexColor` (full `SourceMode` decode incl. the Skyrim+ inheritance
  guard at `material/mod.rs:763`), `NiSpecular` (including the authored-disable
  `flags: 0` case), `NiStencil` (two-sided), `NiWireframe`, `NiShade` all reach
  a `Material` field or pipeline state. `NiFogProperty` remains the one
  deliberate skip.
- **Transform model**: `Transform` is `{ Vec3, Quat, f32 }`. Gamebryo's
  `NiTransform` carries `float m_fScale` — uniform on both sides, so there is no
  non-uniform scale being collapsed. (The skill's D7 bullet still says
  otherwise; the prior sweep already corrected this against
  `gamebryo-v32/Include/NiTransform.h:27` and the wording has not been updated.
  The real, already-filed loss is scale baked into a `NiMatrix3`, #3532.)
- **String interning**: `StringPool` / `FixedString` unchanged; bone-name →
  entity resolution still routes through it (`byroredux/src/name_lookup.rs`
  names `node_by_name` / `external_skeleton` as its consumers).

---

## Deduplication

### Against open GitHub issues (135 open, fetched live this run)

Searched by keyword for every candidate: `skin`, `LOD`, `coord`, `material`,
`ragdoll`, `VWD`, `distant`, `weather`, `water`, `Havok`, `bsver`, `translate`,
`fog`, `cloud`, `hdr`, `height`, `EXAL`, `exterior`, `EX-`. Three matches
recorded as Existing (#3307, #3142, #3930). **No open issue covers the WTHR fog
tail** — the closest, #3567, is about Oblivion parallax/normal alpha, and the
`EX-*` series covers ground cover, previs, SpeedTree, precombine collision,
reversed-Z and VWD culling, none of them weather.

### Against the layer specs

- `nifal.md` §2 — Materials/Geometry/Lights/Animation/Shader-flags converged
  entries all still hold; the Skinning entry's #2440 half re-verified true and
  its #3549 omission filed as D2-01.
- `exal.md` §2 "Weather / TOD" claims the category is "canonical; the cleanest
  of the dynamic categories". D5-01/02 contradict that for the fog tail; the
  finding text says so rather than silently disagreeing with the spec.
- `physal.md` §5 — the classic-chain convergence claim now has FO3 test cover
  (#3921) it lacked when the claim was written.

### Against prior reports

Three findings owned by `AUDIT_LEGACY_COMPAT_2026-08-30.md` — all CLOSED
(#3795, #3796, #3797) and each re-verified against code rather than trusted from
the issue state.

## Stale candidates investigated and dropped (6)

1. **`crates/save/src/driver.rs::restore_world` as a second `Material`
   producer.** It says so itself. Deliberate (#2687), and it gates every
   restored `Material` through `sanitize_finite()`. Not a leak. (D3)
2. **Four `translate_material` callers instead of the skill's two.** One
   producer function, four call sites — the contract is on the producer. The
   skill's caller list is stale, which is recorded as skill drift, not as a
   finding. (D3)
3. **`skyrim_cloud_textures[32]` decoded, only 4 forwarded.** `SkyParamsRes`
   has exactly four cloud slots (`cloud_texture_index` + `_1.._3`), and
   `weather.rs:947-950` forwards the first four by design. This is a renderer
   capability limit, not a boundary leak — the boundary cannot forward what the
   canonical type has no room for. Would become a finding only if the sky
   pipeline grew more layers. (D5)
4. **`oblivion_hdr` (14 HNAM params) decoded with zero consumers.** Eye-adapt
   speed, blur radius/passes, bright scale/clamp, luminance ramp,
   sunlight/grass/tree dimmers — these are fixed-function tone-mapping and
   bloom tuning for a renderer Redux deliberately replaced (ACES + RT). Per the
   standing rule, an unmapped legacy *shading* param is not automatically a
   gap. Distinguished from D5-01 on exactly this axis: height fog is scene
   geometry the engine already models with a constant; HDR tuning is a knob for
   a pipeline that no longer exists. (D5/D7)
5. **`sun_damage`, `transition_delta`, `precipitation_fade`, `thunder_fade`,
   `visual_effect_window`, `wind_direction_range`, `skyrim_sun_glare`,
   `skyrim_precipitation_effect`, `skyrim_visual_effect` — decoded, zero
   consumers.** Each needs a subsystem that does not exist yet (radiation
   damage, weather-transition blending beyond the current cross-fade, particle
   precipitation, referenced SPEL/EFSH effects). Capture-ahead-of-consumer, not
   a translate leak. Worth a single tracking note in `exal.md` §2 rather than
   nine findings; not filed. (D5)
6. **Two `Quat::from_mat3` sites outside `coord.rs`.** Neither is a Z-up→Y-up
   conversion; one converts an already-Y-up Havok transform, the other builds an
   orthonormal solver basis. Not the #1044 duplication class. (D1)

## Verification

- **Every finding's premise was re-checked against HEAD**, and the three prior
  findings were verified fixed in the *code* (the No Sorter field traced from
  `types.rs` through `MaterialInfo` to the renderer's sort input) rather than
  trusted from their CLOSED state.
- **The D5-01 premise was actively attacked before filing.** Checked whether
  `fog_height` was consumed under another name (it is not — `fog_height_reference`
  is the unrelated #2225 ground anchor); whether the height-fog shader branch is
  dead like `fog_power`'s (it is not — it is live and gated only on
  `is_exterior && extinction > 0`); whether the sibling `fog_max` is also
  dropped (it is not — which is what proves the drop is field-selective); and
  whether an open issue already covers it (none does).
- **D5-02 was de-rated on evidence, not filed at MEDIUM by association** with
  D5-01. `draw.rs:1508-1523` documents `fog_power` as currently unconsumed, so
  the finding says the fix changes nothing visible today.
- **No corpus census was run.** Game archives were mounted, but only ~8 GB of
  29 GB was free and whole-ESM parsing in this crate has killed the session
  three times. D5-01's occupancy premise is explicitly marked unmeasured.
- No source file, game file, shader, or GitHub issue was modified. `git status`
  is unchanged from the start of the run. `_audit-validate.sh` → all path
  references valid.

## Summary

Three findings under a 497-commit, 1,297-file delta — and for the first time in
four sweeps, the highest one is in the engine rather than in the audit's own
reference material. The coordinate helpers, the material boundary and the PHYSAL
source axis are untouched and clean. EXAL's boundary is where the layers gave
ground: it forwards half of one WTHR subrecord tail and drops the other half,
into a sink that is already hardcoded to an engine constant.
