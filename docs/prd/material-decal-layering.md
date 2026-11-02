# Material and Decal Layering — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review
round 1 (GPU lane-allocation rule for M56 / M57 / M59 / M61, the seventh
`GpuInstance` mirror, lobe location corrected at HEAD, Starfield `PDCL`,
impact-decal data precedence, Skyrim surface-material mapping, split
multi-observable requirements, sibling links). Nothing below is built as a
result of this document yet, and the decisions it needs from the project
owner are collected in §10. Statements about how the original
engines, their file formats, or the cited rendering techniques behave cite
external sources (UESP, the Creation Kit / GECK wikis, nif.xml, published
papers); a claim that could not be verified is marked **Phase 0: verify**
instead of being stated as fact.

**Scope**: ROADMAP milestone M59 ([Tier 8](../../ROADMAP.md#milestone-tiers)).
What ByroRedux promises for the layers that sit on top of a base material:
authored decals (flagged decal geometry, `NiTexturingProperty` decal slots,
placed texture-set decals), impact decals driven by the games' impact data
(bullet holes, blood, scorch, footprints), micro-detail normal maps, and
anisotropic specular, each visible to the ray-traced passes and not only to
the camera. The parallax-occlusion slice (POM) shipped on 2026-07-29 and is
carried as inventory only. This PRD also owns the GPU lane-allocation rule
(§5.1) that the sibling PRDs for M56, M57 and M61 cite. Out of scope: wet /
snow / grime accumulation (M61,
[wet-surface-accumulation.md](wet-surface-accumulation.md)), hair BRDFs
(M57, [hair-shading.md](hair-shading.md)), subsurface scattering (M56,
[subsurface-scattering.md](subsurface-scattering.md)), displaced
silhouettes, and decals on skinned actors (§4.2).

**Related**: this document states the *promise*; the designs below own the
*how*. Projected decals have no owning design yet; Phase 0 writes it.

| Mechanism | Owning design |
|---|---|
| Canonical material, the translate step, texture roles, decal / two-sided flags | [NIFAL §3](../engine/nifal.md#3-materials--the-reference-realisation), [NIFAL shader-flag survey](../engine/nifal.md#shader-flags--texture-sets--effect-shaders--converged-surveyed-2026-06-02), [Material Abstraction](../engine/material-abstraction.md) |
| `GpuMaterial`, `GpuInstance`, the GLSL mirror contract | [Shader Pipeline § GPU Data Types](../engine/shader-pipeline.md#gpu-data-types), [Renderer § Material table (R1)](../engine/renderer.md#material-table-r1) |
| Anisotropic GGX lobe, Disney BSDF | [Renderer § Disney BSDF](../engine/renderer.md#disney-bsdf) |
| TLAS membership, visibility layers, material-aware ray hits | [Renderer § RT acceleration structures](../engine/renderer.md#rt-acceleration-structures), [RT Lighting & Material Recovery R5](../engine/rt-lighting-material-recovery.md#r5---close-material-and-texture-role-correctness) |
| Per-game shader-flag collisions (bit 21) | [Per-game Translation Survey](../engine/per-game-translation-survey.md#3-existing-abstraction-primitives--where-they-fail) |
| TXST, IPCT, IPDS record decode | [ESM Records](../engine/esm-records.md) |
| Projected / impact decals (raster and RT) | none yet — Phase 0 writes the decal design |
| Starfield `PDCL` placed-decal share of unresolved references | [Starfield ESM Phase 0 baseline](../engine/starfield-esm-phase0-baseline.md) |
| Sibling promises that cite the §5.1 lane-allocation rule | [wet-surface-accumulation.md](wet-surface-accumulation.md) (M61), [hair-shading.md](hair-shading.md) (M57), [subsurface-scattering.md](subsurface-scattering.md) (M56) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `MDL-` prefix.

---

## 1. Product thesis

M59 is the Tier 8 entry that, with M55, "change[s] every existing cell"
(ROADMAP Tier 8 preamble): a wall with a bullet hole, a floor with blood on
it, a rusted panel whose metal grain catches the light. The games author
much of this data and the engine already parses a good part of it, but
none of it reaches the screen beyond the flagged decal meshes. And the
flagged decal meshes the engine does draw are absent from every reflection,
GI and refraction ray, because they are kept out of the acceleration
structure (§3).

The ROADMAP row frames the promise in two sentences: "RT decals
participate in GI", and "Material slot extensions to `GpuMaterial` only;
the R1 promise holds (no DrawCommand or shader-lockstep growth)". The first
is the visual goal; the second is the engineering budget, and this PRD
treats it as a requirement, not a wish (§5.1).

The corollary that shapes every requirement below: **a layer is real only
when the camera and the rays agree on it.** A decal, a detail normal or an
anisotropic lobe that the raster pass shows but a reflection, a GI bounce
or a refraction cannot see is not delivered. Each layer class is therefore
promised as a raster/RT pair with one observable for each half, and a layer
that cannot be given its RT half is not shipped as a raster-only effect.

What the engine will not do: it will not invent values the games do not
author without saying so (each such value is a §6 deviation with an owner
decision), it will not add per-game branches past the translate step, and
it will not grow `GpuInstance`, repurpose its reserved lanes, or touch any
of its GLSL mirrors (§5.1).

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Layer class** | One of the closed set of material layers in §3 (L1–L8). The unit of the promise. |
| **Material decal slot** | One of the four ordered `NiTexturingProperty` decal textures (nif.xml `DECAL_0_MAP`..`DECAL_3_MAP`, "for placing images on the object like stickers"), carried as `GpuMaterial.decal_map_0_index`..`_3_index` (`MATERIAL_DECAL_SLOTS = 4`). |
| **Authored decal surface** | Geometry whose shader flags mark it as a decal (`MaterialInfo.is_decal`), carrying the ECS marker `IsDecalMesh`. Distinct from `RenderLayer::Decal`, the depth-bias class, which also contains alpha cutouts. |
| **Placed decal** | A reference whose base object is a texture set with decal data. CK *TextureSet*: "drag one from the Object Window to the Render View. The direction of the arrow indicates where the texture will project." |
| **Impact decal** | A decal created at runtime where a projectile, weapon swing, footstep or spell strikes a surface, chosen by the struck surface's material through an impact data set (CK *ImpactDataSet*, *ImpactData*). |
| **Projected decal** | The runtime object behind placed and impact decals: a texture set projected along a direction over a bounded depth onto the surfaces it covers. Its representation is decision Q2. |
| **Decal data** | The `DODT` payload of a TXST or IPCT record (UESP *Skyrim Mod:Mod File Format/TXST*, */IPCT*): width and height ranges, depth, shininess, parallax scale and passes, flags, colour. Decoded today as `DecalData` (`crates/plugin/src/esm/cell/base_objects.rs`). |
| **Detail normal layer** | A tiling normal map blended over the base normal for close-range surface grain. Not the existing *detail map*, which modulates albedo (nif.xml `DETAIL_MAP`: "combined with base map for added detail"). |
| **Anisotropy enable bit** | Skyrim `SLSF2` / FO4 `F4SF2` bit 21 `Anisotropic_Lighting` (nif.xml), and BGSM `bAnisoLighting` (Material-Editor `MaterialLib/BGSM.cs`). On FO3/FNV the same bit position is `Alpha_Decal` (nif.xml `BSShaderFlags2`). |
| **Lockstep surface** | The GLSL declarations that hand-mirror the Rust `GpuInstance` ([Shader Pipeline](../engine/shader-pipeline.md#gpuinstance--160-bytes-ssbo-set-1-binding-4)): the six literal `struct GpuInstance` copies listed in `GPU_INSTANCE_GLSL_MIRRORS` (`scene_buffer/shader_contract_tests.rs`) plus the stride-only prefix mirror `GpuBoundaryInstance` in `include/volumetrics_bindings.glsl`. Growing it, or renaming a lane in it, is the cost the ROADMAP row forbids. |
| **Per-material lane** | A `GpuMaterial` field, or a bit of its `materialFlags` word, written once per material by the translate step. One GLSL declaration (`include/bindings.glsl`). |
| **Per-instance side buffer** | A GPU struct array, parallel to the instance SSBO and indexed by the same instance index, declared once in GLSL; the route for per-entity state that changes at runtime (MDL-GPU-005). |
| **Lane-allocation rule** | The §5.1 table that says which of the above a new piece of GPU-visible state takes; owned by this PRD for M56, M57, M59 and M61. |
| **Translate step** | The parser → canonical `Material` boundary in `byroredux/src/material_translate.rs` and the NIF import material walker; the only place a per-game difference may be expressed (CLAUDE.md, NIFAL). |
| **RT half** | A layer's contribution to reflection, GI, refraction and water ray hits, resolved through `include/ray_hit.glsl`. |
| **Surface-response decal** | A projected decal that changes roughness / specular / albedo darkening without a colour texture of its own; the hook M61's puddles use (MDL-DEC-008). |

---

## 3. Core model

Every material layer the engine draws belongs to exactly one class. Each
class has a raster half (in `triangle.frag`) and an RT half (in
`include/ray_hit.glsl`); a class is reported through `mat.dump`,
`render.debug material_role`, or, for projected decals, the telemetry line
MDL-DEC-007 adds.

The inventory is the state at HEAD on 2026-11-01, from the files named. It
is hand-written and will go stale: Phase 1 replaces the "Today" column with
the census test MDL-GPU-007 prints, and the column MUST NOT be maintained by
hand after that.

| # | Layer class | Authored source | Today |
|---|---|---|---|
| L1 | Material decal slot | `NiTexturingProperty` decal 0–3 | Raster: alpha-over composite in `triangle.frag` (the `materialDecals` loop). RT: albedo and coverage composite in `include/ray_hit.glsl` (#3986). The census in that file's comment records zero vanilla meshes binding a slot across Oblivion, FO3, FNV, Skyrim SE and FO4; a mod-facing path |
| L2 | Authored decal surface | SLSF1 / F4SF1 bits 26–27, FO3/FNV `F3SF1` bits 26–27 (`Decal_Single_Pass`, `Dynamic_Decal_Single_Pass`) and `F3SF2` bit 21 (`Alpha_Decal`), BSShaderCRC32 `DECAL` / `DYNAMIC_DECAL` (nif.xml); BGSM `bDecal` (Material-Editor `MaterialLib/BaseMaterialFile.cs`) | Raster: `RenderLayer::Decal` depth bias `(-64, 0, -2)` (`crates/core/src/ecs/components/render_layer.rs`), z-write off when blended (`static_meshes.rs`). RT: **absent** — `tlas_exclusion` returns `TlasExclusion::DecalSurface` (`byroredux/src/render/static_meshes.rs`, test `decal_surfaces_are_excluded`) |
| L3 | Placed decal | TXST + `DODT`, placed as a reference; Starfield `PDCL` (`BGSProjectedDecal`) | **Not drawn.** `DODT` decodes into `TextureSet.decal_data` (`parse_txst_group`, `crates/plugin/src/esm/cell/support.rs`) with no consumer (#3638). TXST is indexed only into `texture_sets`, so a reference to one finds no `index.statics` row and counts as a statics miss (`references/synth_child.rs`). Starfield `PDCL` groups are skipped on purpose by a dedicated arm (`crates/plugin/src/esm/records/parse.rs`, #1568) and recorded in `skipped_unconsumed_groups`; their share of Starfield's unresolved references is measured in the [Starfield ESM baseline](../engine/starfield-esm-phase0-baseline.md) |
| L4 | Impact decal | IPCT, IPDS, MATT; consumers WEAP, PROJ, MGEF, FSTP, RACE, BPTD (UESP Skyrim record pages) | **Not drawn.** `parse_ipct` keeps EDID + model path; `parse_ipds` counts `DATA` entries only (`crates/plugin/src/esm/records/misc/effects.rs`). MATT, FSTP, FSTS are not in the parsed record list (`records/parse.rs`). `projectile_flight_system` (`byroredux/src/systems/projectiles.rs`) ends a projectile on a solid and emits nothing for it |
| L5 | Detail normal layer | none known (§10 Q3) | **Absent.** The detail *albedo* map exists (`triangle.frag`, `detailNeutral`, ×2 UV, mirrored in `ray_hit.glsl`). Ground cover has an uncited `GROUNDCOVER_DETAIL_NORMAL_STRENGTH` of its own |
| L6 | Anisotropic lobe | anisotropy enable bit; no strength scalar in any format | Lobe live: `deriveAxAy` and `distributionGGXAniso` in `include/pbr.glsl` (#1250/#1254), read in `include/lighting.glsl` behind `mat.anisotropic > 0.0`. Content never drives it: `translate_material` writes `anisotropic: 0.0` (#3613); `ANISOTROPIC_LIGHTING` and `aniso_lighting` have no production reader. Reachable only through `mat.set` |
| L7 | Directional material | Skyrim STAT `DNAM` → MATO (UESP *STAT*, *MATO*) | **Not decoded.** `RecordType::MATO` exists (`crates/plugin/src/record.rs`); MATO is not in the parsed list. Scope is decision Q6 |
| L8 | Parallax occlusion | height role, TXST / DODT parallax | Shipped 2026-07-29: primary POM and height-displaced UVs at every material ray hit ([Renderer changelog](../engine/renderer.md#historical-changelog-date-stamped)). Carried, not reopened |

Reporting per class:

| Class | Raster report | RT report |
|---|---|---|
| L1, L5, L6, L7 | `mat.dump <entity>` role / scalar line; `render.debug material_role` | same material record, read by the probe in `render.debug probe <x> <y>` |
| L2, L3, L4 | `render.debug` decal view and the decal telemetry line (MDL-DEC-007) | the same telemetry line's "RT-visible" count |
| L8 | unchanged | unchanged |

---

## 4. Goals and non-goals

### 4.1 Goals

- **MDL-G-001 — RT parity.** Every layer class drawn by the camera is
  also resolved at reflection, GI, refraction and water ray hits; counted
  per class by the census of MDL-GPU-007.
- **MDL-G-002 — Authored decal data consumed.** Placed and impact decals
  from vanilla TXST / IPCT / IPDS data appear in Wave 1 cells; counted by
  the placed-decal spawn rate and impact-to-decal rate (§8).
- **MDL-G-003 — Anisotropy from content.** Materials that author the
  anisotropy enable bit outside the hair class drive the existing lobe;
  counted by the anisotropy content coverage (§8).
- **MDL-G-004 — Micro-detail normals.** Eligible surfaces (Q3) carry a
  detail normal layer in both halves; counted by detail-normal coverage.
- **MDL-G-005 — Zero lockstep growth.** `GpuInstance` stays 160 bytes
  with every mirror untouched, `GpuMaterial` keeps one GLSL declaration,
  and `DrawCommand` gains no field; pinned by tests (§5.1). The same
  lane-allocation rule binds the sibling milestones that cite it.
- **MDL-G-006 — No cost where unused.** A cell with no M59 layer pays
  nothing measurable; counted by the decal cost metric under the bench
  methodology.
- **MDL-G-007 — The M61 hook.** The surface-response decal class M61's
  puddles need exists and is documented, so the wet-surface PRD depends
  on a shipped interface, not on a plan.

### 4.2 Non-goals

- **MDL-NG-001.** No new deferred decal pass that rewrites the G-buffer
  before lighting. Shading runs in the forward `triangle.frag` pass, which
  writes lit colour directly (`outColor` "HDR color (direct light only)");
  the ROADMAP row bounds M59 to material-slot extensions.
- **MDL-NG-002.** No decals on skinned or moving actors in this milestone.
  Oblivion exposes `bDecalsOnSkinnedGeometry` per quality preset (UESP
  *Oblivion Mod:Oblivion.ini*), so this is a deliberate deviation (§6),
  revisited after M41's actors and M56's skin shading.
- **MDL-NG-003.** No displaced silhouettes: POM changes material detail,
  not the acceleration structure (ROADMAP M59 row; Renderer changelog).
- **MDL-NG-004.** No hair BRDF. Hair belongs to M57; the anisotropy bit on
  hair-class materials is handled by decision Q5.
- **MDL-NG-005.** No wetness, snow or grime accumulation; that is M61,
  which consumes MDL-DEC-008.
- **MDL-NG-006.** No per-pixel parity with the original look of decals
  (ROADMAP [anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
- **MDL-NG-007.** No `FORMAT_MAJOR` change: impact decals are transient
  unless Q7 decides otherwise.

---

## 5. Requirements by area

### 5.1 GPU contract and the R1 budget (`MDL-GPU`)

This area owns the GPU material contract for every milestone that adds
shading state under the ROADMAP's "Material slot extensions to
`GpuMaterial` only" sizing: M59 itself, and the sibling PRDs for M56
([subsurface-scattering.md](subsurface-scattering.md)), M57
([hair-shading.md](hair-shading.md)) and M61
([wet-surface-accumulation.md](wet-surface-accumulation.md)), which cite
the IDs below instead of restating them. New GPU-visible state is placed
by what it varies with, at HEAD on 2026-11-01:

| State varies per | Lane it takes | Rule | Never |
|---|---|---|---|
| Material: a scalar, a texture role, or a class with three or more values | a new `GpuMaterial` field | MDL-GPU-002, MDL-GPU-003 | a `DrawCommand` field (MDL-GPU-004) |
| Material: one boolean the translate step derives | one free `materialFlags` bit, 24–31 (`material_flag::EFFECT_LI_SHIFT` doc, `crates/renderer/src/vulkan/material.rs`); when none is free, a second flags field under MDL-GPU-002 | MDL-GPU-008, MDL-GPU-009 | a bit inside the `EFFECT_LI` byte (16–23) |
| Instance: one boolean the collect step derives | one free `GpuInstance.flags` bit, 11–15 (`INSTANCE_FLAG_*`, `crates/renderer/src/vulkan/scene_buffer/constants.rs`; bits 4–5 are the render layer, 16–31 the terrain tile slot) | MDL-GPU-010 | a new `GpuInstance` field |
| Entity or instance: a value that changes at runtime (wetness, snow or grime coverage) | a per-instance side buffer | MDL-GPU-005 | `GpuInstance` reserved lanes (MDL-GPU-001); `GpuMaterial` variants per state |
| Space: a projected decal, including a surface-response decal's per-frame strength | the decal buffer of Q2 | MDL-GPU-005, MDL-DEC-010 | `GpuInstance`; `GpuMaterial` |

The siblings' claims under this rule, as their rev 2 texts state them:

| Milestone | Claims | Source |
|---|---|---|
| M56 | per-material fields only | SSS-PERF-003, SSS-NG-004 |
| M57 | one `materialFlags` bit for the hair class | HAIR Q2 (recommended) |
| M59 | `GpuMaterial` fields (MDL-DET-002, MDL-LAY-002), the decal buffer (Q2); no flag bit | this PRD |
| M61 | `GpuMaterial` fields for porosity and the authored-snow response; a per-instance side buffer for per-entity channels | WET-MAT-004, WET Q1 (recommended) |

- **MDL-GPU-001.** `GpuInstance` MUST stay at 160 bytes with no field
  added and no reserved lane (`_reserved`, `_reserved2a..c` in
  `scene_buffer/gpu_types.rs`) renamed or repurposed, at every declaration
  of the lockstep surface: the six literal copies (`include/bindings.glsl`,
  `triangle.vert`, `water.vert`, `ui.vert`, `caustic_splat.comp`,
  `groundcover_models.comp`) and the `GpuBoundaryInstance` prefix mirror
  (`include/volumetrics_bindings.glsl`). Every commit under M56, M57, M59
  or M61 MUST pass, unmodified, `gpu_instance_is_160_bytes_std430_compatible`,
  `gpu_instance_field_offsets_match_shader_contract`,
  `gpu_instance_does_not_re_expand_with_per_material_fields`,
  `gpu_instance_glsl_copies_stay_in_lockstep` (which also fails on a
  seventh literal declaration, through `assert_mirror_list_is_complete`)
  and `gpu_boundary_instance_stride_matches_gpu_instance`. Repurposing a
  reserved lane is an owner decision that amends the ROADMAP row, not a
  route this rule offers.
- **MDL-GPU-002.** Every per-material lane M56, M57, M59 or M61 adds MUST
  be a `GpuMaterial` field or a `materialFlags` bit, and the commit that
  adds a field MUST update, in the same change, the
  size pin (today `gpu_material_size_is_432_bytes`),
  `gpu_material_field_offsets_match_shader_contract` and
  `gpu_material_glsl_field_order_matches_rust_struct`.
- **MDL-GPU-003.** `GpuMaterial` MUST remain declared at exactly one GLSL
  site, `include/bindings.glsl`. Today that holds by inspection, but the
  only guard covers `ui.vert`
  (`ui_vert_reads_texture_index_from_instance_not_material_table`). The
  existing `assert_mirror_list_is_complete` helper
  (`scene_buffer/shader_contract_tests.rs`), called from
  `gpu_material_glsl_field_order_matches_rust_struct` with the one-entry
  list `include/bindings.glsl`, MUST make that test fail when any other
  file under `crates/renderer/shaders/` declares `struct GpuMaterial`.
  Adding a `GpuMaterial` field is therefore one GLSL edit plus the three
  tests of MDL-GPU-002; it is not lockstep growth in the ROADMAP row's
  sense.
- **MDL-GPU-004.** `DrawCommand` (`crates/renderer/src/vulkan/context/types.rs`)
  MUST NOT gain a field for any M59 lane, and a test MUST pin its field
  count as non-increasing. Today `DrawCommand` still carries the
  per-material scalars that `DrawCommand::to_gpu_material` copies into
  `GpuMaterial`, the deferral R1 recorded at its close
  ([roadmap-history §6](../archive/roadmap-history.md#6-risk-reducer-index-growth-tripwire-and-pacing-discipline));
  how a new lane reaches `GpuMaterial` without one is decision Q1.
- **MDL-GPU-005.** Any new GPU-visible struct that M56, M57, M59 or M61
  introduces (the M59 decal buffer of Q2, the M61 per-instance side buffer
  of WET Q1) MUST be declared in exactly one GLSL file, guarded by an
  `assert_mirror_list_is_complete` call with that one-entry list, and
  pinned by a size test, an offset test and a GLSL field-order test landing
  in the same commit, following the `GpuMaterial` pattern. It MUST NOT be
  referenced from `GpuInstance` and MUST NOT be fed through a `DrawCommand`
  field (MDL-GPU-004); a per-instance side buffer is indexed by the same
  instance index the raster and ray-hit paths already use for
  `GpuInstance`, and the citing design shows that indexing for both.
- **MDL-GPU-006.** Every new shader branch MUST be gated on its lane
  holding a non-default value (the pattern of `mat.anisotropic > 0.0` and
  `mat.detailMapIndex != 0u`), so content that authors none of the new
  layers takes the pre-M59 path; a source-shape test MUST assert the gate
  for each lane.
- **MDL-GPU-007.** A census test MUST report, per layer class L1–L7, the
  raster consumer and the RT consumer it finds in the shaders, and MUST
  fail when a class has a raster consumer but no RT consumer (the §1
  corollary), unless the class is listed as primary-only with a reason in
  the decal design (the only such case foreseen is MDL-ANI-004). Its
  output replaces the §3 "Today" column.
- **MDL-GPU-008.** A `materialFlags` bit MUST carry only a boolean the
  translate step derives; a property with three or more values MUST take
  a `GpuMaterial` field instead, and each bit claimed from 24–31 MUST name,
  in its `material_flag` constant's doc comment, the requirement ID that
  claims it.
- **MDL-GPU-009.** A new test, `material_flag_bits_are_disjoint`, MUST
  fail when two `material_flag` single-bit constants share a bit or when
  one falls inside the `EFFECT_LI` byte (bits 16–23).
- **MDL-GPU-010.** A new test, `instance_flag_bits_are_disjoint`, MUST
  fail when two `INSTANCE_FLAG_*` constants share a bit or when one falls
  inside the render-layer field (bits 4–5) or the terrain tile window
  (bits 16–31).

### 5.2 Authored decals (`MDL-AUTH`)

- **MDL-AUTH-001.** A reflection, GI, refraction or water ray that hits a
  host surface under an authored decal surface MUST return the
  decal-composited albedo, not the bare host. Today the decal is excluded
  from the TLAS (`TlasExclusion::DecalSurface`), so rays see the host
  only. Observable: a `render.debug probe` on a mirror-facing decal and a
  new `cargo test` over the TLAS policy.
- **MDL-AUTH-002.** An authored decal surface MUST NOT occlude a shadow
  ray and MUST NOT shadow its own host. Today this holds because the decal
  is not in the TLAS; after MDL-AUTH-001 it MUST hold through the
  visibility-layer mask (`VisibilityMask` in `byroredux_core::lighting`,
  the precedent being `VISIBILITY_LAYER_EFFECT` kept out of
  `VISIBILITY_MASK_SOLID`).
- **MDL-AUTH-003.** Material decal slots (L1) MUST keep their raster/RT
  agreement. Today `triangle.frag` and `include/ray_hit.glsl` composite the
  same four slots alpha-over; `ni_texturing_property_decals_reach_semantic_overlay_slots`
  MUST stay green.
- **MDL-AUTH-004.** A reference whose base is a TXST carrying `DODT` MUST
  spawn as a projected decal oriented by the reference rotation (CK
  *TextureSet*: the arrow "indicates where the texture will project").
  Today it is a statics miss (§3, L3). **Phase 0: verify** the number of
  such references per game and one cell per Wave 1 game that contains
  them, by a census over the vanilla masters (it settles MDL-AUTH-004's
  acceptance cell).
- **MDL-AUTH-005.** A placed or impact decal MUST honour its decal data:
  a size drawn between min and max width / height, the projection depth,
  the colour multiply, the alpha-blend and alpha-test flags, parallax
  scale and passes, and the four-subtexture choice (CK *TextureSet*
  "Decal Data"; UESP *TXST* `DODT` flags `0x01` parallax, `0x02` blend,
  `0x04` test, `0x08` "not 4 subtextures"). Each field MUST be
  observable in the decal telemetry dump for one decal. For an impact
  decal, the IPCT's own `DODT` MUST take precedence over its texture set's
  (GECK *ImpactData*: "Has Own Decal Data: Overides the Texture Set decal
  data"). UESP *IPCT* lists that `DODT` with `uint8[3]` unknown where TXST
  has its flags byte; **Phase 0: verify** against xEdit's IPCT definition
  whether impact decals take their blend / test flags from the IPCT or the
  texture set.
- **MDL-AUTH-006.** `DODT` *shininess* ("sets the specular exponent", CK
  *TextureSet*) MUST be converted to the canonical roughness at the
  translate step, never in the shader. **Phase 0: verify** the conversion
  the original renderer applies to a specular exponent for decals; until
  settled, MDL-AUTH-006 uses the canonical material's own exponent
  convention and records that as a §6 deviation.
- **MDL-AUTH-007.** The FO3/FNV `DODT` layout MUST be confirmed before the
  FO3/FNV arm consumes it. Today `parse_txst_group` reads every game's
  `DODT` with the 36-byte layout documented for Skyrim / FO4. **Phase 0:
  verify** against xEdit `wbDefinitionsFNV.pas` / `wbDefinitionsFO3.pas`.
- **MDL-AUTH-008.** `BSDecalPlacementVectorExtraData` (nif.xml: "lists
  locations and normals on a mesh that are appropriate for decal
  placement") MAY seed decal placement on the mesh that carries it. Today
  it parses with no consumer (#987). **Phase 0: verify** which vanilla
  meshes carry it and what event the original engine places decals on
  from it; until then it stays parsed-only.
- **MDL-AUTH-009.** A Starfield reference whose base is a `PDCL` SHOULD
  spawn through the same projected-decal path as MDL-AUTH-004 in Wave 3.
  Today the `PDCL` group is skipped by its dedicated arm in
  `crates/plugin/src/esm/records/parse.rs`. **Phase 0: verify** the `PDCL`
  record layout against xEdit's Starfield definitions
  (`wbDefinitionsSF1.pas`) before any decode is scheduled.

### 5.3 Impact decals (`MDL-IMP`)

- **MDL-IMP-001.** `parse_ipct` MUST decode `DATA` (effect duration,
  angle threshold, placement radius, sound level), `DODT`, `DNAM` (texture
  set), `ENAM` (secondary texture set), `SNAM` / `NAM1` (sounds) and
  `NAM2` (hazard) per UESP *Skyrim Mod:Mod File Format/IPCT*. Today it
  keeps EDID and the model path. The sound fields also feed the empty
  `ImpactConfig` / `FootstepConfig` maps (`byroredux/src/components/audio.rs`),
  which wait on this decode.
- **MDL-IMP-002.** `parse_ipds` MUST return the (material, IPCT) pairs:
  Skyrim `PNAM` pairs of MATT and IPCT FormIDs (UESP *IPDS*). Today it
  stores only `material_pair_count`. **Phase 0: verify** the FO3/FNV `DATA`
  shape (the code comment's 12 × 8-byte reading is unsourced) and the FO4
  and Starfield shapes, against xEdit's definitions.
- **MDL-IMP-003.** MATT MUST be decoded for the games that author it,
  keeping `HNAM` (impact data set) and `PNAM` (parent material) per UESP
  *MATT*. Today MATT is not in the parsed record list.
- **MDL-IMP-004.** The mapping from a struck collider's surface material
  to an IPCT MUST be resolved at the translate step, once per game: the
  FO3/FNV fixed material kinds of the IPDS table versus the Skyrim / FO4
  MATT chain. The decal system MUST see one canonical (surface material →
  IPCT) table and no game check. The struck collider carries the Havok
  surface material as `RigidBodyData::surface_material`
  (`crates/core/src/ecs/components/collision.rs`), which on Skyrim is a
  `SkyrimHavokMaterial` value (nif.xml: `SKY_HAV_MAT_STONE` =
  `3741512247`; #5542), while IPDS
  pairs name MATT FormIDs (UESP *IPDS*). **Phase 0: verify** the link from
  the collider's Havok material to a MATT record (MATT `MNAM` material name
  hashed, or another field) from xEdit or the CK *Material Type* page.
- **MDL-IMP-005.** A projectile that ends on solid geometry MUST publish
  an impact carrying position, surface normal, struck surface material
  and the source's impact data set. Today `projectile_flight_system`
  records only actor hits. Observable: a `cargo test` beside the existing
  projectile tests asserting one impact for one wall hit.
- **MDL-IMP-006.** An impact MUST create a decal only when the IPCT names
  a texture set, and MUST respect the IPCT angle threshold and placement
  radius (CK *ImpactData*: "Angle Threshold: affects how far around in
  degrees a decal renders"; "Placement Radius: affects the distance
  between decals").
- **MDL-IMP-007.** Live impact decals MUST be bounded by an engine budget
  with oldest-first eviction, and the budget and current count MUST
  appear in the decal telemetry line. Where a game's INI carries a decal
  lifetime (Oblivion `fDecalLifetime`, UESP *Oblivion.ini*), the engine
  SHOULD honour it through the honoured-INI table
  ([LMC-SET](legacy-mod-compatibility.md#57-settings-lmc-set)).
  **Phase 0: verify** the decal lifetime and count settings of FO3, FNV,
  Skyrim and FO4 from their INI documentation.
- **MDL-IMP-008.** Footstep impacts SHOULD produce decals where the
  footstep record names an impact data set (Skyrim FSTP `DATA`, UESP
  *FSTP*), through the same path. Today FSTP / FSTS are not parsed and
  `select_footstep_sound` plays sound only. **Phase 0: verify** how many
  vanilla FSTP-referenced IPCTs name a texture set (footprint decals may
  not exist in vanilla data at all).
- **MDL-IMP-009.** Weapon-swing, spell and blood impacts (Skyrim WEAP
  `INAM`, MGEF impact data, BPTD severable / explodable decal counts, RACE
  "Don't Show Blood Decal"; Oblivion CREA `NAM1` "Blood Decal") MAY
  produce decals in a later phase. **Phase 0: verify** which of these
  fields each game authors, from the UESP / GECK pages, before any is
  scheduled.

### 5.4 Projected decal rendering (`MDL-DEC`)

- **MDL-DEC-001.** A projected decal MUST change the receiving surface's
  albedo, normal and roughness from its texture set's roles before
  lighting, so the decal is lit by the same direct, ReSTIR and GI terms as
  its host. The technique is decision Q2; the clustered-forward precedent
  is idTech 6 (Sousa & Geffroy, "The devil is in the details: idTech 666",
  SIGGRAPH 2016 Advances in Real-Time Rendering,
  <https://advances.realtimerendering.com/s2016/index.html>); the
  box-projection precedent is Persson, "Volume Decals", *GPU Pro 2*,
  A K Peters, 2011 (page range to be confirmed from the volume's table of
  contents when the design cites it). **Phase 0: verify** from the idTech 6
  slides that decals share the light cluster structure and what per-cluster
  cap they use (secondary write-ups report it; the abstract does not).
- **MDL-DEC-002.** A reflection, GI, refraction or water ray hitting a
  surface covered by a projected decal MUST see the decal, including
  decals outside the camera frustum; the RT half is the milestone's
  headline promise. Observable: `render.debug probe` on a mirror that sees
  a decal behind the camera.
- **MDL-DEC-003.** A projected decal MUST NOT write its own stable surface
  ID: the receiving pixel keeps its host's `outMeshID`, so TAA and SVGF
  history survive under it. A source-shape test MUST assert it.
- **MDL-DEC-004.** A projected decal MUST NOT be a shadow occluder: a
  shadow probe on a decal-covered host reports the same visibility with
  the decal present and absent.
- **MDL-DEC-005.** A projected decal MUST NOT apply to skinned geometry,
  water, glass, or effect-shader surfaces (MDL-NG-002); the exclusion MUST
  be one predicate with a unit test.
- **MDL-DEC-006.** A projected decal's texture set MUST be resolved
  through the same texture-role walk as any material (`MaterialTextureSet`,
  NIFAL §3), so a missing texture reports through `tex.missing` like any
  other.
- **MDL-DEC-007.** The engine MUST print one decal telemetry line on
  request (a new debug-console command named in the Phase 0 design) with:
  live projected decals by class (placed / impact), RT-visible count,
  budget, evictions, and the GPU time of the decal work from the per-pass
  timers.
- **MDL-DEC-008.** The decal system MUST offer the surface-response decal
  class, the interface M61 cites for puddles (WET-MAT-011). Its contract,
  which a citing PRD MAY rely on without restating: a decal with a
  footprint box and no colour texture that, inside the footprint, (a)
  multiplies the host albedo by one factor, (b) overrides roughness, (c)
  overrides specular, each scaled by one strength in [0, 1]; outside the
  footprint the host is unchanged; the decal is RT-visible like any
  projected decal (MDL-DEC-002) and inherits MDL-DEC-003, -004, -005 and
  -009. Observable: scenario A10.
- **MDL-DEC-009.** A projected decal MUST NOT apply to geometry farther
  from its projection plane than its decal-data depth; a unit test over
  the footprint predicate pins it.
- **MDL-DEC-010.** A surface-response decal's strength MUST be writable
  every frame as a field of its decal-buffer entry (Q2, MDL-GPU-005)
  without despawning, re-creating or re-uploading the decal set; a test
  changes the strength over two frames and asserts the decal count and
  identity are unchanged.
- **MDL-DEC-011.** The decal design's surface-response section MUST be
  checked in, and linked from the M61 PRD, before any M61 implementation
  issue that consumes MDL-DEC-008 is filed.

### 5.5 Detail normal layer (`MDL-DET`)

- **MDL-DET-001.** A detail normal layer MUST combine with the base normal
  by Reoriented Normal Mapping (Barré-Brisebois & Hill, "Blending in
  Detail", 2012, <https://blog.selfshadow.com/publications/blending-in-detail/>),
  in both `triangle.frag` and `include/ray_hit.glsl`; a test MUST assert
  the two call the same helper, as the detail albedo combine is kept in
  lockstep today.
- **MDL-DET-002.** The layer MUST be carried as `GpuMaterial` lanes: a
  texture role, a tiling scale and a strength (MDL-GPU-002). `mat.dump`
  MUST print all three and their provenance.
- **MDL-DET-003.** Detail normal textures MUST come only from the source,
  and be assigned only by the signal, that decision Q3 selects, and
  `mat.dump` MUST name that source. Today no format the
  engine reads authors one: BGSM has no such field (Material-Editor
  `MaterialLib/BGSM.cs`), and the NIF detail slot is an albedo map
  (nif.xml `DETAIL_MAP`). **Phase 0: verify** whether Starfield's CDB
  layered materials author a detail normal, against the `crates/sfmaterial`
  schema and a Starfield material census.
- **MDL-DET-004.** The layer's contribution MUST fade to zero with distance
  or screen-space footprint, by a rule the decal design states; a
  source-shape test asserts the fade term multiplies the layer's strength.
- **MDL-DET-006.** The layer's added normal variance MUST feed the existing
  specular anti-aliasing roughness widening in `include/pbr.glsl`; a
  source-shape test asserts the detail normal reaches that widening.
- **MDL-DET-005.** A debug bypass bit for the layer MUST exist beside
  `DBG_BYPASS_DETAIL`, and `render.debug material_role` MUST show where it
  is bound.

### 5.6 Anisotropic specular (`MDL-ANI`)

- **MDL-ANI-001.** The anisotropy enable bit MUST be captured into
  `ImportedMaterial` at the NIF import and BGSM merge boundaries, read only
  for the shader-flag families where bit 21 means `Anisotropic_Lighting`
  (Skyrim `SLSF2`, FO4 `F4SF2`, nif.xml) and from BGSM `bAnisoLighting`.
  On FO3/FNV the bit MUST NOT be read as anisotropy (it is `Alpha_Decal`,
  nif.xml `BSShaderFlags2`); the `TextureSlotLayout` gate already used for
  sibling SLSF2 bits is the route `material_translate.rs` names (#3613). A
  test MUST pin that no FO3/FNV material comes out anisotropic.
- **MDL-ANI-002.** No format supplies an anisotropy *strength*
  (`material_translate.rs` #3613 rationale; Material-Editor `BGSM.cs`
  stores a bool). The strength an enabled material receives is decision
  Q4 and MUST be one documented value set at the translate step, never a
  shader constant.
- **MDL-ANI-003.** The lobe MUST be oriented by the authored or
  synthesized vertex tangent and MUST fall back to isotropic when the
  tangent projects to zero, as `include/lighting.glsl` does today (#3984).
  The tests `anisotropic_ggx_reduces_to_the_isotropic_ndf_at_zero_anisotropy`
  and `derive_ax_ay_clamps_anisotropic_against_nan_and_floor_escape` MUST
  stay green.
- **MDL-ANI-004.** Wherever secondary-hit shading evaluates a specular
  lobe, it MUST use the same anisotropic lobe as the raster pass for that
  material. Today the only shader read of `mat.anisotropic` is in
  `shadowableLightRadiance` (`include/lighting.glsl`), called only from
  `triangle.frag`; the hit-shading functions `pathHitRadiance` and
  `reflectionHitIrradiance` do not read it. Where hit shading has no
  specular lobe, the MDL-GPU-007 census MUST record L6 as primary-only
  with that reason.
- **MDL-ANI-005.** The lobe reference is Burley, "Physically-Based Shading
  at Disney", SIGGRAPH 2012 course *Practical Physically Based Shading in
  Film and Game Production*; the shipped `aspect = sqrt(1 − 0.9 ·
  anisotropic)` split MUST keep citing it and the GLSL-PathTracer port it
  came from ([Renderer § Disney BSDF](../engine/renderer.md#disney-bsdf)).

### 5.7 Directional material (`MDL-LAY`)

Applies only if decision Q6 places Skyrim directional materials in M59.

- **MDL-LAY-001.** MATO `DATA` (falloff scale and bias, noise and material
  UV scale, projection vector, normal dampener, single-pass colour; UESP
  *MATO*) and the STAT `DNAM` max angle and MATO FormID (UESP *STAT*) MUST
  be decoded. Today neither is.
- **MDL-LAY-002.** A static with a directional material MUST show the
  layer on faces within the authored max angle of the projection vector,
  in both halves, as `GpuMaterial` lanes only (MDL-GPU-002). **Phase 0:
  verify** how the original blends the layer from the CK *Material Object*
  page ("the data for a material shader that can be applied to an object
  (such as the snow shader)") or a further source, because that page
  states no blend rule.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Decals render where they are projected for the camera (CK *TextureSet*, *ImpactData*); whether they appear in the FO4 screen-space reflections BGSM switches on (`bScreenSpaceReflections`, Material-Editor `MaterialLib/BaseMaterialFile.cs`) is **Phase 0: verify** | Decals take part in every ray-traced term (MDL-AUTH-001, MDL-DEC-002) | — | A decal coplanar with its host can win or lose the ray hit at random → MDL-AUTH-002, MDL-DEC-004 and the acceptance probe |
| 2 | Anisotropy is an enable bit with no strength (#3613; `BGSM.cs`) | One engine strength per Q4 | — | Content tuned for the original's look reads stronger or weaker → MDL-ANI-002 records the value; `mat.set anisotropic` stays the per-material override |
| 3 | No authored micro-detail normals | Engine-assigned detail normals (Q3) | Texture mods whose only purpose is close-range grain | Double detail on mod textures that bake grain in; misassigned grain → Q3's per-profile switch and assignment signal; MDL-DET-005 bypass |
| 4 | Oblivion can put decals on skinned geometry (`bDecalsOnSkinnedGeometry`, UESP *Oblivion.ini*) | Not in M59 (MDL-NG-002) | — | Missing blood on actors → MDL-IMP-009 and a later milestone |
| 5 | Decal lifetime and count are INI policy (Oblivion `fDecalLifetime`); the other games' settings are **Phase 0: verify** | Engine budget, INI honoured where present (MDL-IMP-007) | INI tweaks raising decal counts | Visible decal churn under heavy fire → telemetry in MDL-DEC-007 |
| 6 | Whether impact decals persist in the original saves is **Phase 0: verify** | Transient by default (MDL-NG-007, Q7) | — | Bullet holes vanish across a save/load → Q7 |
| 7 | `DODT` shininess is a specular exponent (CK *TextureSet*) | Converted to canonical roughness at translate (MDL-AUTH-006) | — | A wrong exponent mapping makes decals glossier than their host → MDL-AUTH-006's verify item |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow the engine tiers in
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail).
Every per-game difference below is expressed in the parser or the translate
step; nothing in the renderer branches on the game.

| Game | In scope | Why / wave |
|---|---|---|
| Fallout: New Vegas | yes, Wave 1 | Engine reference title. TXST with decal data and IPDS / IPCT exist (GECK *TextureSet*, *ImpactData*); L1–L4 and L5. No anisotropy (bit 21 is `Alpha_Decal`) |
| Skyrim (LE / SE / AE) | yes, Wave 1 (SE; LE shares the data) | Fullest documented data: TXST / IPCT / IPDS / MATT / FSTP (UESP), anisotropy bit, MATO (Q6). All classes |
| Fallout 3 | yes, Wave 2 | Same record families as FNV through the shared parser; `DODT` layout per MDL-AUTH-007 |
| Fallout 4 | yes, Wave 2 | TXST `DODT` on every decal-bearing vanilla TXST (`DecalData` doc, #813), BGSM `bDecal` / `bAnisoLighting`, F4SF2 bit 21. IPDS / IPCT shapes per MDL-IMP-002's verify item |
| Oblivion | partial, Wave 2 | No TXST, IPDS or IPCT records (UESP Oblivion record list); L1 decal slots, L2, L5 only. CREA blood decals deferred (MDL-IMP-009) |
| Starfield | partial, Wave 3 | Tier 2. L2 via the BSShaderCRC32 decal identifiers; L3 through `PDCL` once its layout is verified (MDL-AUTH-009), and its decal texture sets need the parked `_opacity` role (NIFAL, #4429); IPDS / IPCT shapes unverified |
| Fallout 76 | no | Tier 3, parse only: no cell loader to place or receive decals |

### 7.2 Acceptance

A scenario passes when every listed observable holds. Game data is the
user's installed copy; nothing is checked in. Bench numbers follow the
ROADMAP [bench methodology](../../ROADMAP.md#how-to-run-a-bench): a
same-machine control and a pinned `--camera-pos` / `--camera-forward`.

| # | Scenario | Driven by | Data | Passes when |
|---|---|---|---|---|
| A1 | Contract guards | `cargo test -p byroredux-renderer` with the existing guards of MDL-GPU-001/002/003 plus the new single-declaration, `DrawCommand` field-count, gate, census and flag-disjointness tests (MDL-GPU-009, -010) | none | All green; `GpuInstance` 160 B; one `struct GpuMaterial` |
| A2 | Anisotropy gating | new `cargo test` in `byroredux` over translated FO3/FNV, Skyrim and FO4 fixtures | synthetic shader-flag fixtures | Skyrim / FO4 flagged non-hair → strength per Q4; FO3/FNV `Alpha_Decal` → 0 |
| A3 | Anisotropy census | opt-in corpus test (the `parse_real_*` lane) | Skyrim SE, FO4 masters + meshes | anisotropy content coverage (§8) equals 100% of the Phase 0 eligible set |
| A4 | Authored decal in rays | `--bench-hold` on a Phase 0-chosen FNV cell with a blood-splat decal mesh; `byro-dbg`: `render.debug probe <x> <y>` on a reflective surface seeing it, `screenshot` | FNV | probe reports the decal-composited albedo; shadow probe on the host reports no decal occlusion |
| A5 | Placed decals | `--bench-hold` on the MDL-AUTH-004 cell per Wave 1 game; decal telemetry line; `screenshot` | FNV, Skyrim SE | placed-decal spawn rate = 100% of the cell's TXST references with decal data |
| A6 | Impact decal | new script `m59-impact-decals.sh` under `docs/smoke-tests/` (Phase 5), firing a projectile at a wall through the Phase 0-named command | FNV, Skyrim SE | one impact, one decal of the IPCT's texture set, visible in a `render.debug probe` reflection; telemetry count 1 |
| A7 | Decal-free cost | bench-of-record cells, control vs candidate | FNV Prospector, Skyrim Bannered Mare, FO4 MedTek | main-pass GPU time delta within the measured noise floor |
| A8 | Decal-heavy cost | A6's script repeated to the budget | FNV | decal GPU time and evictions published; frame stays within the budget Phase 0 sets |
| A9 | Detail normal | `--bench-hold` close-up on an eligible surface; `mat.dump .`, `render.debug material_role`, bypass bit on/off screenshots | per Q3 | role listed with provenance; bypass toggles only that layer |
| A10 | M61 hook | new `cargo test` creating a surface-response decal and reading its effect on a fixture material | none | roughness / albedo inside the footprint changed, outside unchanged |

---

## 8. Metrics

One home for the numbers: the M59 row of ROADMAP's
[Tier 8 table](../../ROADMAP.md#milestone-tiers) for coverage, and the
bench-of-record for cost. This section defines what is counted.

| Metric | Definition | Granularity |
|---|---|---|
| RT parity | layer classes with an RT consumer ÷ layer classes with a raster consumer (MDL-GPU-007) | engine-wide |
| Placed-decal spawn rate | TXST-with-`DODT` references spawned as decals ÷ such references in the loaded cell | per game, per acceptance cell |
| IPCT decode coverage | IPCT records with texture set and decal data decoded ÷ IPCT records | per game |
| Impact-to-decal rate | solid impacts producing a decal ÷ solid impacts whose impact data set resolves to an IPCT naming a texture set | per game, per scenario |
| Anisotropy content coverage | non-hair materials with `anisotropic > 0` ÷ non-hair materials carrying the enable bit | per game |
| Detail-normal coverage | materials with a detail normal bound ÷ materials in the Q3 eligible set | per game |
| Decal cost | GPU ms of the decal work, and main-pass delta, at budget and at zero decals | per bench cell |

---

## 9. Delivery sequence

Phases are ordered by what they unblock: the contract first because every
later lane depends on it, anisotropy next because the lobe already exists,
decals after their design, and the M61 hook before Wave 2 so the
wet-surface PRD can be approved against a shipped interface. Milestone
dependencies: R1 (closed), M28 physics and its surface materials, M47.2
projectiles (present), M41 actors for anything on skin (deferred).

**Phase 0 — Census, design and decisions (no engine code).** Write the
missing decal design under `docs/engine/` (representation per Q2, RT
lookup outside the frustum, budget, the telemetry command, the
surface-response contract for M61). Resolve Q1–Q7. Close every **Phase 0:
verify** item: the placed-decal census and acceptance cells
(MDL-AUTH-004), the decal shininess conversion (MDL-AUTH-006), the FO3/FNV
`DODT` layout (MDL-AUTH-007), `BSDecalPlacementVectorExtraData` usage
(MDL-AUTH-008), the impact-decal flags source (MDL-AUTH-005), the `PDCL`
layout (MDL-AUTH-009), the Havok-material → MATT link (MDL-IMP-004), the
IPDS shapes of FO3/FNV/FO4/Starfield (MDL-IMP-002), the
INI decal settings (MDL-IMP-007), footstep decal data (MDL-IMP-008), the
blood and swing fields (MDL-IMP-009), the idTech 6 cluster detail
(MDL-DEC-001), Starfield detail normals (MDL-DET-003), the MATO blend rule (MDL-LAY-002), decals in original
reflections and in saves (§6 rows 1 and 6), and the hair / non-hair split
of anisotropy-flagged materials (Q5). Exit: the design is checked in, every
§10 item carries **Decided**, and a census table per game is checked in.

**Phase 1 — Contract groundwork.** MDL-GPU-001…010 and the Q1 route for new
lanes; this phase also unblocks the GPU work of M56, M57 and M61. Exit: scenario A1 green, and A7 shows no main-pass delta beyond the
noise floor.

**Phase 2 — Anisotropy from content.** MDL-ANI-001…005, Wave 1 and Wave 2
together (the bit is shared). Exit: A2 green and A3 at 100% of the eligible
set for Skyrim SE and FO4.

**Phase 3 — Authored decals in the rays.** MDL-AUTH-001…003. Exit: A4
passes on FNV and on one Skyrim SE cell.

**Phase 4 — Projected decals and placed decals.** MDL-DEC-001…011,
MDL-AUTH-004…007, Wave 1. Exit: A5 passes for FNV and Skyrim SE, A10 is
green, and the decal design's surface-response section is linked from the
M61 PRD.

**Phase 5 — Impact decals.** MDL-IMP-001…007 for projectile hits, Wave 1;
MDL-IMP-008 where Phase 0 found data. Exit: A6 and A8 pass for FNV and
Skyrim SE and the impact-to-decal rate is published.

**Phase 6 — Detail normals.** MDL-DET-001…006 per Q3. Exit: A9 passes and
detail-normal coverage is published for each Wave 1 game.

**Phase 7 — Wave 2 and 3.** FO3, FO4 and Oblivion for every class their
data supports (§7.1); Starfield once its opacity role exists, with MDL-AUTH-009;
MDL-LAY per Q6. Exit: placed-decal spawn rate and IPCT decode coverage published for
every in-scope game.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — How a new lane reaches `GpuMaterial` without growing
   `DrawCommand`.** Recommended: replace, don't grow — before the first
   new lane, `DrawCommand` carries one `GpuMaterial` (built at collect
   time from the canonical `Material`) in place of its per-material
   scalars, finishing the R1 deferral; the field count drops. Consequence:
   every `DrawCommand` producer (static meshes, particles, terrain, water,
   UI) changes once, and the dedup hash path is re-benched. Alternative:
   add the few M59 lanes to `DrawCommand` and amend the ROADMAP row's
   constraint; cheaper now, and the R1 promise stops holding. Either
   answer binds the per-material lanes of M56, M57 and M61 too (WET-MAT-004,
   SSS-PERF-003); per-instance state never takes this route (§5.1 table,
   MDL-GPU-005).
2. **Q2 — Projected decal representation.** Recommended: projected
   (clustered) decals — a decal buffer declared once in GLSL, assigned to
   the existing light clusters for the raster half (the idTech 6
   precedent), plus a world-space lookup the ray-hit path can query for
   decals outside the frustum. Consequence: one new GPU struct under
   MDL-GPU-005 and an RT lookup the design must size. Alternative A: mesh
   decals, i.e. host triangles clipped to the decal box on the CPU and
   drawn on the authored-decal-surface path, so RT comes from MDL-AUTH-001;
   no new GPU struct, but a BLAS per decal and CPU clipping cost per
   impact. Alternative B: drop projected decals from M59 and ship only the
   material layers; MDL-G-002 and the M61 hook are lost.
3. **Q3 — Detail normal source and assignment.** Recommended: a small
   engine-shipped library of original or CC0-licensed detail normals,
   assigned at the translate step from the struck-surface material the
   physics layer already carries (Havok surface material on the owning
   collider), none where no signal exists, on by default with a
   per-profile switch. Consequence: surfaces without collision get no
   detail; library licences are recorded per texture. Alternative A:
   assign by the texture-path keyword classifier `resolve_pbr` already
   uses as a last-resort tiebreaker; wider coverage, heuristic
   misassignment. Alternative B: content-authored only (mods or Starfield
   if MDL-DET-003 finds a source); vanilla surfaces gain nothing.
4. **Q4 — Anisotropy strength for an enabled material.** Recommended: one
   engine value, chosen in Phase 0 by matching a named modern reference
   renderer on a brushed-metal test surface in the Cornell harness, and
   recorded as §6 row 2. Consequence: one number to defend, no per-asset
   tuning. Alternative: keep strength 0 and treat the bit as unused;
   MDL-G-003 is withdrawn and the lobe stays harness-only.
5. **Q5 — Anisotropy on hair-class materials.** Recommended: M59 drives
   only non-hair materials; a material is hair-class exactly when the M57
   classifier says so ([hair-shading.md §3.1](hair-shading.md#31-hair-signals-per-game),
   HAIR-CLS-010), and hair-class surfaces never take this lobe
   (HAIR-BSDF-009), so the lobe is not tuned twice. Consequence: nif.xml
   annotates `SLSF2` bit 21 `Anisotropic_Lighting` "Hair only?", so
   Phase 0 may find the bit set almost only on hair, leaving MDL-G-003 with
   little content; and the ROADMAP M59 row's "anisotropic specular
   (brushed metal, hair, satin, vinyl)" loses "hair" to M57
   (HAIR-NG-005), a T1 wording change the owner makes when approving.
   Alternative: drive hair through anisotropic GGX now as an interim (the
   same option hair-shading's Q1 lists as its alternative B, so the two
   answers must match); visible sooner, replaced by M57, and it has no
   TT / TRT lobes.
6. **Q6 — Skyrim directional materials (MATO).** Recommended: in M59, as
   an authored static layer translated like any material (MDL-LAY), which
   M61's runtime snow later reuses. Consequence: M59 grows by one class
   and a ROADMAP scope line (T1). Alternative: M61 owns all snow,
   authored and runtime; M59 stays as the ROADMAP row lists it.
7. **Q7 — Impact decal persistence.** Recommended: transient, never saved,
   so no save-format change (MDL-NG-007). Consequence: decals vanish on
   load. Alternative: save live decals per cell, which bumps
   `FORMAT_MAJOR` and is a T2 host-contract change needing its own review.

---

## 11. Document map

| Need | Go to |
|---|---|
| Canonical material, texture roles, the translate step | [nifal.md §3](../engine/nifal.md#3-materials--the-reference-realisation) |
| Shader-flag vocabularies and the bit-21 collision | [nifal.md](../engine/nifal.md#shader-flags--texture-sets--effect-shaders--converged-surveyed-2026-06-02), [per-game-translation-survey.md](../engine/per-game-translation-survey.md#3-existing-abstraction-primitives--where-they-fail) |
| `GpuMaterial` / `GpuInstance` layouts and mirrors | [shader-pipeline.md § GPU Data Types](../engine/shader-pipeline.md#gpu-data-types) |
| Material table, Disney lobes, TLAS policy | [renderer.md](../engine/renderer.md#material-table-r1) |
| Material-role debug views and `mat.dump` | [rt-lighting-material-recovery.md R5](../engine/rt-lighting-material-recovery.md#r5---close-material-and-texture-role-correctness) |
| History of the canonical material | [material-abstraction.md](../engine/material-abstraction.md) |
| TXST / IPCT / IPDS records | [esm-records.md](../engine/esm-records.md) |
| Honoured INI settings | [legacy-mod-compatibility.md § LMC-SET](legacy-mod-compatibility.md#57-settings-lmc-set) |
| Per-game engine tiers | [game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail) |
| Smoke-test index | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| GPU lane-allocation rule cited by sibling milestones | §5.1 of this PRD; citing PRDs: [wet-surface-accumulation.md](wet-surface-accumulation.md), [hair-shading.md](hair-shading.md), [subsurface-scattering.md](subsurface-scattering.md) |
| Hair-class definition used by Q5 | [hair-shading.md §3.1](hair-shading.md#31-hair-signals-per-game) |
| Starfield `PDCL` measurements | [starfield-esm-phase0-baseline.md](../engine/starfield-esm-phase0-baseline.md) |
| Live milestone state, bench methodology, published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: GPU lane-allocation rule for M56 / M57 / M59 / M61 (§5.1 table, MDL-GPU-001/002/003/005 generalized, MDL-GPU-008..010 new), `GpuBoundaryInstance` mirror and offset / stride tests in MDL-GPU-001, anisotropic lobe location corrected to `pbr.glsl`, Starfield `PDCL` (L3, MDL-AUTH-009, §7.1), IPCT `DODT` precedence (MDL-AUTH-005), Skyrim Havok material → MATT verify (MDL-IMP-004), MDL-DEC-008 written as a citable contract (MDL-DEC-010/011), MDL-DEC-004 and MDL-DET-004 split (MDL-DEC-009, MDL-DET-006), Q5 aligned with hair-shading, sibling links | claude: revise |
