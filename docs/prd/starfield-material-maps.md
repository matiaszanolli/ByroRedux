# Starfield Material Maps — Product Requirements

**Status**: ACTIVE (2026-11-26). Revision 5 files the tracking issues and
clarifies what Phase 0 may change. The owner approved revision 3, recorded
as revision 4; §12 has the history.

**Tracking**: #7419 (parent). Phases: #7420 (0), #7421 (1), #7422 (2), #7423 (3). Phase 0's verify items are listed in #7420.
Statements about how an original engine, format or tool behaves cite
external sources; a claim that could not be verified is marked
**Phase 0: verify** instead of being stated as fact.

**Scope**: four properties that Starfield's material database authors and the
engine does not use: opacity, roughness, metalness and ambient occlusion. In
scope: reading each from the database and from loose `.mat` files, carrying it
through the translation boundary, and using it on directly visible surfaces
and on traced hits. Outside it: blending material layers, the transmissive,
height and emissive data, the blend modes still parked, matching the original
renderer's look, and the memory or frame time of a Starfield load beyond
reporting what these maps add.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Texture roles, the translation boundary, the parked-kinds table | [NIFAL](../engine/nifal.md); Phase 0 writes the four roles' channel, encoding and fallback contract there |
| Canonical material and its GPU record | [Material Abstraction](../engine/material-abstraction.md), [Shader Pipeline](../engine/shader-pipeline.md) |
| Texture floors and per-frame detail levels | [Texture Streaming](../engine/texture-streaming.md) |
| Texture memory accounting | [Memory Budget](../engine/memory-budget.md) |
| GPU material lanes | [Material and Decal Layering PRD](material-decal-layering.md), `MDL-GPU` (proposed) |
| Starfield load memory, unread texture roles | [Starfield Memory Efficiency PRD](starfield-memory.md) (proposed) |
| Transmissive and subsurface data | [Subsurface Scattering PRD](subsurface-scattering.md) (proposed) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `SMM-` prefix.

---

## 1. Product thesis

The owner's framing (2026-11-26): *"Starfield is still way too broken
visually, and it's probably leaking memory. We should tackle compatibility
before performance."* This PRD addresses one measured part of the visual
problem. It does not claim to explain all of it, and it does not address the
suspected leak.

The material database authors these four properties for much of what the
game's models reference (§3.3), and the engine reads none of them into the
canonical material. A surface with no roughness or metalness from another
source gets one fixed value of each. A cutout that tests alpha tests the
base colour's alpha instead of the opacity data.

It is for anyone who looks at Starfield content in the engine, and for later
Starfield visual work, which cannot be judged while surfaces lack their
authored response.

**Corollary:** a property is used only where its source and meaning are
known. It is never guessed from a file name, never routed into a role that
means something else, and never drawn one way on a visible surface and
another way in a shadow or reflection.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Material database** | Starfield's compiled `materialsbeta.cdb`, which the engine reads through `MaterialIndex` |
| **Loose material** | A JSON `.mat` file outside the database, which resolves through the same merge |
| **Slot** | The index of a texture within a database texture set (§3.2) |
| **Texture role** | A named member of `MaterialTextureSet` ([NIFAL](../engine/nifal.md)) |
| **Texture replacement** | A colour a texture set carries for a slot, with an enable flag (`BSMaterial::TextureReplacement`, F76U-comps) |
| **Property** | One of opacity, roughness, metalness, ambient occlusion |
| **Coverage test** | A shader's decision, from an alpha value, that a point on a surface exists or how much of it does |
| **Traced hit** | A surface point reached by a ray: shadow, reflection, refraction, bounce light, water or volumetrics |

External source. **F76U** is `libfo76utils`, an independent, MIT-licensed
reader of the database, as vendored in
[fo76utils/nifskope](https://github.com/fo76utils/nifskope/tree/3797c8939058f34b9b3a2fea6e2259f0e4fbfa48/lib/libfo76utils)
at commit `3797c893`, the head of `develop` on
2026-11-26. The cited lines were checked at that commit. It is not Bethesda
documentation.

| Citation | File |
|---|---|
| **F76U-hpp** | [`src/material.hpp`](https://github.com/fo76utils/nifskope/blob/3797c8939058f34b9b3a2fea6e2259f0e4fbfa48/lib/libfo76utils/src/material.hpp) |
| **F76U-cpp** | [`src/material.cpp`](https://github.com/fo76utils/nifskope/blob/3797c8939058f34b9b3a2fea6e2259f0e4fbfa48/lib/libfo76utils/src/material.cpp) |
| **F76U-comps** | [`src/matcomps.cpp`](https://github.com/fo76utils/nifskope/blob/3797c8939058f34b9b3a2fea6e2259f0e4fbfa48/lib/libfo76utils/src/matcomps.cpp) |

---

## 3. Core model

### 3.1 Property source

For each material, each property has exactly one source:

| Source | Meaning | Reported by `mat.dump` as |
|---|---|---|
| **Map** | A texture role bound from a slot | the role and its path |
| **Constant** | A value from an enabled texture replacement | the value |
| **Override** | A value another source already authored on the material (NIF import or inline shader) | the value and its origin |
| **Absent** | None of the above, or a recorded rule holds the property unused; the engine's fallback applies | `absent`, the fallback, and the rule if one applies |

### 3.2 Slots

F76U-hpp:69-77 lists the texture-set slots. The four in scope are:

| Slot | F76U name | Property |
|---:|---|---|
| 2 | alpha (`_opacity`) | opacity |
| 3 | roughness (`_rough`) | roughness |
| 4 | metalness (`_metal`) | metalness |
| 5 | ambient occlusion (`_ao`) | ambient occlusion |

F76U does not state which texture channel each slot uses or whether it is
linear (**Phase 0: verify**, SMM-SRC-003).

### 3.3 Inventory

The inventory below is the state at HEAD on 2026-11-26 (`3e5ba755b`). The
counts came from a temporary probe that was not checked in, so no file in the
tree reproduces them. The probe counted a texture or replacement in any layer,
not by the rule of SMM-SRC-010. Treat the counts as estimates until the
census of SMM-OBS-004 replaces them in Phase 0.

Population: the distinct material paths named by the lighting and effect
shader blocks of the models in `Starfield - Meshes01.ba2`,
`Starfield - Meshes02.ba2` and `Starfield - MeshesPatch.ba2`
(`sf_matpath_dump`), looked up in the base database.

| Measure | Materials |
|---|---:|
| Paths named | 10,380 |
| Resolved in the base database | 10,158 |
| `HasOpacity` true | 2,273 |
| `HasOpacity` true with an opacity texture | 1,961 |
| `HasOpacity` true with a threshold above zero | 865 |
| An opacity texture without `HasOpacity` true (2,803 − 1,961) | 842 |

The counts are materials, not screen area or shapes.

| # | Property | Materials with a texture | Replacement only | Today |
|---|---|---:|---:|---|
| 1 | Opacity | 2,803 | 155 | Dropped by `apply_cdb_material` (`asset_provider/material/merge.rs`). Cutouts test base-colour alpha (`triangle.frag`, `include/ray_hit.glsl`, `volumetrics_inject.comp`) |
| 2 | Roughness | 8,766 | 786 | Dropped. A database hit with no override takes `PbrMaterial::NO_SIGNAL_NEUTRAL` |
| 3 | Metalness | 2,417 | 1,978 | As roughness |
| 4 | Ambient occlusion | 3,287 | 89 | Dropped; no shader reads a material occlusion value |

Four facts at HEAD shape the requirements:

| Fact | Where |
|---|---|
| `MaterialTextureSet` has no role for any of the four. `smooth_spec` is consumed as gloss, which is inverted from roughness | `crates/nif/src/import/types/material.rs`; `triangle.frag` |
| Traced-hit shading reads one roughness and one metalness per material | `pathHitEmitterBsdf`, `include/lighting.glsl` |
| The index fills a slot's texture and its replacement colour independently, each from the first texture set that has one (the root-named set, then layers in authored order), so the two can come from different layers | `MaterialIndex::lookup`, `collect_slots` (`crates/sfmaterial/src/index.rs`) |
| The index does not capture `OpacitySourceLayer` | `CdbMaterial` (same file) |
| Besides terrain layers, the wanted-mip pass walks eight named `DrawCommand` texture handles and not `supplemental_texture_indices` | `compute_wanted`, `byroredux/src/texture_streamer.rs` |

---

## 4. Goals and non-goals

### 4.1 Goals

- **SMM-G-001 — Authored data arrives.** For each delivered property, every
  census material that authors a texture or an enabled replacement MUST reach
  the canonical material as Map, Constant or Override, unless a recorded rule
  holds it Absent. SMM-OBS-006 lists each such material under its rule.
- **SMM-G-002 — Cutouts follow opacity.** Every coverage test on a material
  whose opacity is not Absent MUST use that source.
- **SMM-G-003 — One surface everywhere.** A traced hit MUST see the same
  opacity, roughness and metalness as the directly visible surface.
- **SMM-G-004 — Other games unchanged.** Content with no database material
  MUST take the same translation and shading path as before.

### 4.2 Non-goals

- **SMM-NG-001.** Blending material layers, which gets its own PRD once the
  mixing arithmetic has a source (decision Q2). F76U-cpp and F76U-comps name
  blend modes and channel switches but do not state it.
- **SMM-NG-002.** The transmissive slot and `UseSSS`, which the
  [Subsurface Scattering PRD](subsurface-scattering.md) covers. The height
  slot stays out of the parallax role (#5381).
- **SMM-NG-003.** Emissive brightness. It waits on a measured mapping from
  the authored luminance (#5503, [NIFAL](../engine/nifal.md)).
- **SMM-NG-004.** The soft additive and other parked blend modes (#5509),
  and dithered transparency.
- **SMM-NG-005.** The untranslated `MaterialParamFloat` values. These
  properties come from slots and replacements.
- **SMM-NG-006.** A memory or frame-time budget for Starfield. The
  [Starfield Memory Efficiency PRD](starfield-memory.md) owns budgets; this
  PRD reports what its maps add.
- **SMM-NG-007.** Pixel matching against the original renderer.
- **SMM-NG-008.** Which signal carries Starfield glass coverage.
  [NIFAL](../engine/nifal.md) records, from a census of `Meshes01`, 217
  inherited-glass materials that render opaque because none authors a
  coverage signal (an alpha blend mode, `HasOpacity` or a threshold). That
  question stays open there, and this PRD does not make those materials
  transparent.

---

## 5. Requirements by area

### 5.1 Resolution and translation (`SMM-SRC`)

- **SMM-SRC-001.** Each property's §3.1 source MUST be decided in
  `merge_external_material`. No later stage may change which source a
  property has, and there is no render-time fallback to another source; a
  Map that fails to load follows SMM-SRC-011. A test (new,
  `cdb_property_sources_are_decided_at_the_merge`) covers all four sources on
  a synthetic database. At HEAD on 2026-11-26 only the colour, normal and
  emissive slots and the colour replacement are forwarded.
- **SMM-SRC-002.** A loose material MUST resolve the same source and value
  as a database material authoring the same data. A test (new,
  `loose_and_database_materials_resolve_the_same_property_sources`) covers
  each property.
- **SMM-SRC-003.** [NIFAL](../engine/nifal.md) MUST record each role's
  channel and encoding before the role ships. **Phase 0: verify** both,
  from a format census of the slot's textures and an external reader or
  shader that samples them.
- **SMM-SRC-004.** A replacement's value for a single-channel property MUST
  follow a recorded channel and encoding rule. F76U-hpp:88 says replacements
  are R8G8B8A8 colours; it does not say how a single-channel slot reads one.
  **Phase 0: verify** from an external reader or shader that samples a
  replacement for such a slot; the bytes authored across the corpus can
  support that source but not replace it. Until the rule is recorded, a
  replacement MUST NOT become a Constant: the property stays Absent for that
  material.
- **SMM-SRC-005.** When one layer authors both a texture and an enabled
  replacement for a slot, the merge MUST apply one recorded precedence.
  **Phase 0: verify** from an external reader or shader that chooses between
  them; the census counts the materials affected. Until it is recorded, the
  property stays Absent for those materials.
- **SMM-SRC-006.** An Override MUST keep precedence over a database Constant
  or Map. For a texture role, the merge's existing `fill` rule MUST apply: a
  NIF-authored texture wins, except where its path resolves in no loaded
  archive and the database's path does (#4636).
- **SMM-SRC-007.** An Absent property MUST keep today's behaviour: the
  neutral roughness and metalness, base-colour coverage, and no material
  occlusion. F76U-cpp:9-14 holds a default replacement colour per slot, and
  F76U-cpp:118-125 copies those colours with the enable mask clear; neither
  says what applies to an empty slot. **Phase 0: verify** from Creation Kit
  material documentation or an external shader's empty-slot path before any
  other fallback is adopted. If neither settles it, today's behaviour stays.
- **SMM-SRC-008.** A change that adds a role MUST remove that suffix from
  NIFAL's parked-kinds list, with
  `starfield_single_channel_kinds_are_parked_by_name_or_canonical` passing.
  The transmissive kind stays listed.
- **SMM-SRC-009.** Starfield `TXST` texture slots for a delivered property
  SHOULD fill the same role. **Phase 0: verify** the slot numbers against
  xEdit's Starfield definitions; NIFAL's present numbers rest only on this
  repository's own measurement.
- **SMM-SRC-010.** For each of roughness, metalness and occlusion, the
  texture and the replacement MUST come from one layer: the first, checking
  the root-named texture set and then the numbered layers in authored order,
  that supplies either a texture or an enabled replacement for the slot
  (decision Q2). A later layer's entry MUST NOT be combined with it. One
  layer includes its texture set's `Parent` chain, which is inheritance and
  not blending. The colour, normal and emissive slots keep today's
  selection. The index does not do this at HEAD on 2026-11-26 (§3.3). A test
  (new, `a_slots_texture_and_replacement_come_from_one_layer`, in
  `crates/sfmaterial`) covers two layers that each supply half. Opacity
  follows SMM-OPA-005.
- **SMM-SRC-011.** A Map of any of the four properties whose texture cannot
  be loaded MUST shade with that property's Absent fallback, not with a
  Constant. This is the existing rule for secondary textures: an
  authored-but-missing one collapses to handle 0
  (`resolve_material_texture_handles_with_clamp`,
  `byroredux/src/asset_provider/texture.rs`).

### 5.2 Opacity (`SMM-OPA`)

- **SMM-OPA-001.** Where opacity is Map or Constant, the directly visible
  surface's coverage test MUST take its value from that source, not from
  base-colour alpha. At HEAD on 2026-11-26 it reads the base texture's alpha.
- **SMM-OPA-002.** Every traced coverage test MUST obtain its alpha from the
  same source through the same shared function as the visible surface. A
  source-shape test (new, `every_coverage_reader_uses_the_opacity_source`)
  fails for a coverage reader that bypasses the function. The behaviour is
  checked by A4.
- **SMM-OPA-003.** A blended material with an opacity source MUST blend by
  that source.
- **SMM-OPA-004.** A material with `HasOpacity` true and no authored
  threshold MUST apply a recorded threshold rule. F76U-cpp:31 initializes the
  threshold to one third; at HEAD on 2026-11-26 the merge tests alpha only
  when the database authors a value above zero. **Phase 0: verify** which
  materials are cutouts and which threshold they use, from an external
  reader's or shader's alpha-test path or Creation Kit material
  documentation. Until it is recorded, today's threshold rule stays.
- **SMM-OPA-005.** Opacity MUST come from the layer that
  `OpacitySourceLayer` names, with its texture and replacement both taken
  from that layer. F76U-comps:337-354 reads the string `MATERIAL_LAYER_n` as
  layer number `n`, and F76U-cpp:33 initializes it to zero. **Phase 0:
  verify** how `n` maps to the index's layer references, by comparing the
  reader's layer table with the database on materials that name a layer
  other than zero. Until a material's named layer can be resolved, its
  opacity MUST stay Absent.
- **SMM-OPA-006.** Where `HasOpacity` is not true, opacity MUST be Absent,
  whatever slot 2 holds. **Phase 0: verify** what a slot-2 texture or
  replacement means without the flag, from an external reader or shader that
  consumes the flag; SMM-OBS-004 counts these materials.

### 5.3 Roughness and metalness (`SMM-PBR`)

- **SMM-PBR-001.** A roughness Map MUST give the directly visible surface its
  base roughness per texel, in place of the material's scalar. The gloss-map
  branch (`glossMapIndex`) MUST NOT change that roughness, weather wet-film
  and snow changes apply after it, and the Map MUST NOT be bound to
  `smooth_spec`.
- **SMM-PBR-002.** A metalness Map MUST give the directly visible surface its
  base metalness per texel, in place of the material's scalar. Weather
  changes apply after it, and the Map MUST NOT be bound to `specular`.
- **SMM-PBR-003.** A roughness or metalness Constant MUST set the canonical
  material's scalar, which traced hits already read.
- **SMM-PBR-004.** Traced-hit shading MUST sample a roughness or metalness
  Map at the hit's texture coordinate.
- **SMM-PBR-005.** Neither the keyword classifier nor the passes that run
  after textures resolve (NIFAL's second stage) MUST overwrite a Map or
  Constant. A test (new, `authored_database_pbr_survives_every_resolver`)
  covers each pass.
- **SMM-PBR-006.** (withdrawn, rev 3) Replaced by SMM-SRC-011.

### 5.4 Ambient occlusion (`SMM-AO`)

- **SMM-AO-001.** An occlusion value MUST darken only ambient and indirect
  lighting terms that no ray has already occluded, and MUST NOT change direct
  light or traced bounce light (decision Q3). At HEAD on 2026-11-26,
  `triangle.frag` already multiplies screen-space occlusion (SSAO) into three
  such terms: the untraced share of sky diffuse on the fully traced exterior
  path, the Skyrim directional ambient cube, and the flat ambient term. Phase
  0 completes the list by shader and name. Where SSAO and material occlusion
  both apply, the term MUST use the lower of the two values (the one that
  darkens more), never their product; a term's floor (`AMBIENT_AO_FLOOR` on
  flat ambient) applies to the result. A pixel with no untraced share shows
  no change. A source-shape test (new,
  `material_occlusion_combines_with_ssao_in_one_place`) fails for a listed
  term that combines them elsewhere.
- **SMM-AO-002.** A material with no occlusion source MUST be lit as before.

### 5.5 GPU, streaming and upload (`SMM-GPU`)

- **SMM-GPU-001.** Each per-material value this PRD adds to the GPU record
  MUST be a `GpuMaterial` field, MUST NOT add a `GpuInstance` field or a
  `DrawCommand` field, and MUST update the size and layout tests in the same
  commit. These are the lane rules MDL-GPU-001 to MDL-GPU-004 of the
  [Material and Decal Layering PRD](material-decal-layering.md); that PRD is
  proposed, so they are stated here. A new texture role travels in the
  existing `DrawCommand::supplemental_texture_indices` array. Phase 0 records
  in NIFAL how a Constant for opacity or occlusion travels under the same
  rule.
- **SMM-GPU-002.** Each new shader branch MUST be gated on a bound,
  non-default value (the pattern of MDL-GPU-006), with a source-shape test
  per role.
- **SMM-GPU-003.** A role's textures MUST first be uploaded in the change
  that adds its shader reader. This matches decision Q5 of the
  [Starfield Memory Efficiency PRD](starfield-memory.md).
- **SMM-GPU-004.** With texture streaming on, each new role MUST receive a
  wanted level from `compute_wanted`, which therefore has to visit the
  supplemental array; it does not at HEAD on 2026-11-26. A test (new,
  `material_map_roles_receive_a_wanted_level`) fails for a role the pass
  does not visit.

### 5.6 Observability (`SMM-OBS`)

- **SMM-OBS-001.** `mat.dump` MUST print each delivered property's §3.1
  source and value or path.
- **SMM-OBS-002.** `tex.missing` MUST report a missing property texture
  under its role's name.
- **SMM-OBS-003.** A render debug view MUST show, per pixel, each delivered
  property's value on the directly visible surface; for occlusion it shows
  the value applied to ambient terms after the SMM-AO-001 combine and floor.
  The value is written without lighting or tone mapping.
- **SMM-OBS-004.** A checked-in census command (new, an example in
  `crates/sfmaterial`) MUST print the database side of the §3.3 population:
  for each property, the materials whose chosen layer (SMM-SRC-010,
  SMM-OPA-005) has a texture only, a replacement only, or both; for opacity,
  also the splits by `HasOpacity`, threshold and unresolved named layer. It
  does not report Override or the canonical material, which it cannot see.
- **SMM-OBS-005.** The census command MUST print, for each property, the
  materials in which more than one layer supplies it (decision Q2).
- **SMM-OBS-006.** A real-data test (new, `starfield_material_property_reach`,
  `#[ignore]`d, in the `byroredux` binary) MUST parse the §3.3 models, run
  `merge_external_material` on them, and print for each property the
  materials by §3.1 source. A material counts as reaching only if every shape
  that names it ends as Map, Constant or Override. Each other material that
  authors the property is listed under the recorded rule that holds it
  Absent; the test fails for one with no rule.
- **SMM-OBS-007.** In the SMM-OBS-003 view for roughness or metalness, a
  mirror surface MUST show the value the traced-hit path sampled at its
  reflection hit.

---

## 6. Deliberate deviations and risks

| # | Source behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | A material has several layers and blenders (F76U-hpp, F76U-comps) | One layer supplies each property (SMM-SRC-010, SMM-OPA-005) | — | A layered surface can show one layer's response everywhere → SMM-OBS-005 counts the materials affected |
| 2 | **Phase 0: verify** how the original renderer applies authored occlusion, from Creation Kit material documentation or an external shader; if neither settles it, this row stays open | Occlusion is limited to the terms of SMM-AO-001 | — | Taking the lower of SSAO and material occlusion avoids stacking them, and may soften contact shading where both are partial → SMM-AO-001 |
| 3 | The database authors a value another source may also author | The other source wins (SMM-SRC-006) | — | A NIF placeholder could mask authored data → SMM-OBS-001 shows the winner |
| 4 | **Phase 0: verify** the original fallback for an empty slot, by the sources of SMM-SRC-007 | Today's engine fallbacks (SMM-SRC-007) | — | Absent surfaces may differ from the original → reported as `absent` by SMM-OBS-001 |

Adding up to four textures per material raises texture memory on the title
that already strains the development machine. Phase 0 measures the bytes
before any role ships, SMM-GPU-003 keeps unread textures out, and
SMM-GPU-004 lets streaming manage the new ones.

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | n/a | No material database path; its translation is unchanged (SMM-G-004) |
| Fallout 3 | n/a | As Oblivion |
| Fallout: New Vegas | n/a | As Oblivion |
| Skyrim (LE / SE / AE) | n/a | As Oblivion |
| Fallout 4 | n/a | BGSM / BGEM materials take a separate merge arm, which this PRD does not change |
| Fallout 76 | no | Its materials are `.bgsm` / `.bgem` files ([game compatibility](../engine/game-compatibility.md)), merged by the arm this PRD does not change |
| Starfield | yes | The only wave |

### 7.2 Acceptance

A4 to A6 load one mesh at a time in the Studio viewer, with
`cargo run -- --game starfield --studio --mesh <path>`, so that no scenario
depends on loading a city cell (decision Q4). The viewer stands the mesh in a
lit room, which receives its shadow; its walls are matte, so a reflection
check first makes one a mirror with `mat.set`. Phase 0 names the vanilla
meshes, and confirms that this launch runs the database merge, loads a loose
material override and allows that edit.

Each of A4 to A6 has two parts. Before-and-after captures of the vanilla mesh
are the visual evidence (decision Q1). The pass rule is read on a fixture: a
checked-in generator (new, no game data) writes textures with uniform blocks
of known values and a loose `.mat` naming them, applied to the same mesh. The
capture script lists the sample points. A pixel "reads" a value when it is
within 2/255 of it after the debug view's output transfer, which Phase 0
records. "Unchanged" means within the difference measured between two
identical runs. A `citycydoniamainlevel` capture is taken when the machine
can hold the cell, and blocks no phase. Game data is never checked in.

A phase exits on these captures and the data tests below. An in-game
screenshot that the owner supplies is compared with the engine's capture and
does not block a phase. Passing shows that authored data is used as recorded,
not that it matches the original.

| ID | Scenario | Data | Passes when |
|---|---|---|---|
| A1 | `cargo test -p byroredux-sfmaterial` and `cargo test -p byroredux --bin byroredux starfield_mat` | synthetic database | every delivered property resolves each §3.1 source as authored |
| A2 | The SMM-OBS-004 census, and `cargo test -p byroredux --bin byroredux starfield_material_property_reach -- --ignored --nocapture` | installed Starfield archives | for each delivered property, every authoring material reaches, or is listed under a recorded rule |
| A3 | The source-shape tests of SMM-OPA-002, SMM-AO-001 and SMM-GPU-002 | none | each test for a delivered property passes |
| A4 | A fixture with open and closed opacity blocks | vanilla mesh, fixture textures | `mat.dump` names the fixture as Map; at every sample point the visible surface is open or closed as the block says, and the matching floor point is lit behind an open block and shadowed behind a closed one |
| A5 | A fixture with blocks of known roughness and metalness | vanilla mesh, fixture textures | every sample reads its block's value on the visible surface (SMM-OBS-003) and in the mirror (SMM-OBS-007) |
| A6 | A fixture with a known occlusion value, a second with 1.0, and a run with the role unbound | vanilla mesh, fixture textures | each sample of the occlusion view reads the lower of the SSAO view's value and the fixture's, then floored; the direct-light and traced-bounce views are unchanged from the unbound run; with 1.0 the final frame is unchanged too |
| A7 | `cargo test` (new, `non_database_materials_leave_the_new_lanes_default`) | fixtures for the other material paths | every new GPU value stays at its default |
| A8 | The streaming test of SMM-GPU-004 | none | each delivered role gets a wanted level |

---

## 8. Metrics

| Metric | Definition | Granularity |
|---|---|---|
| Property reach | From SMM-OBS-006: materials reaching the canonical material as Map, Constant or Override ÷ materials authoring a texture or enabled replacement, with the rest by recorded rule | per property |
| Unresolved opacity layer | Materials held Absent by SMM-OPA-005 | census total |
| Opacity without the flag | Materials held Absent by SMM-OPA-006 that author slot 2 | census total |
| Added texture bytes | Bytes the property's textures add, from archive headers (all levels and floors) and measured resident on each acceptance mesh | per property |
| GPU time delta | Main-render GPU time with the role bound minus unbound, same acceptance mesh and pose | per property |

[ROADMAP.md](../../ROADMAP.md) holds the published numbers, under a
Starfield material maps entry for this PRD. It replaces the #3398 entry when
#3398 is closed (decision Q5).

---

## 9. Delivery sequence

**Phase 0 — Verify and measure (no change to translation or rendering).**
Check in the census command as a tool (SMM-OBS-004, -005) and publish its
counts. The census needs each layer's data and `OpacitySourceLayer`, so this
phase may add read-only access to them in `crates/sfmaterial`;
`MaterialIndex::lookup` keeps today's selection until Phase 2. Close the
verify items of SMM-SRC-003 to -005, -007 and -009, SMM-OPA-004 to -006, and
§6 rows 2 and 4. Measure added texture bytes from archive headers. Name the
acceptance meshes, record the peak memory of loading each, make the viewer
checks of §7.2, build the fixture generator and record the debug view's
output transfer. List the lighting terms for SMM-AO-001. Write the four
roles' contract in NIFAL. Exit: every verify item above is closed or filed as
an issue, and the census output and added-byte table are published.

**Phase 1 — Opacity.** SMM-SRC-001, -002, -006 to -008 and -011, and the
rules of SMM-SRC-003 to -005, for opacity; SMM-OPA; SMM-GPU; SMM-OBS-001 to
-003 and -006. Exit: A1 to A4, A7 and A8 pass, and opacity reach is
published.

**Phase 2 — Roughness and metalness.** SMM-PBR and SMM-SRC-010 (the index
change), with the same source, GPU and report requirements for both
properties, and SMM-OBS-007. Exit: A1 to A3, A5, A7 and A8 pass, and both
reach figures and the GPU time delta are published.

**Phase 3 — Ambient occlusion.** SMM-AO, with SMM-SRC-010 and the same
source, GPU and report requirements. Exit: A1 to A3 and A6 to A8 pass, and
occlusion reach is published.

SMM-SRC-009 lands with whichever phase follows its verify item.

On approval the work gets a new parent issue, and #3398 is closed with a note
that its definition of done is met and the four properties moved here
(decision Q5).

---

## 10. Decisions required

1. **Q1 — What evidence says a property looks right?** Recommended: the
   engine's own before-and-after captures on named meshes, plus the data
   tests of §7.2. In-game screenshots are compared where the owner supplies
   them, and do not block a phase. This shows that authored data is used as
   recorded; it does not show that it matches the original. Alternative:
   require an in-game reference for each property. It is stronger evidence,
   but needs the owner to run the game and match camera poses before any
   phase can exit.
   **Decided** (2026-11-26): the recommendation. Folded into §7.2.
2. **Q2 — Layered materials.** Recommended: one layer per property in this
   PRD (the named layer for opacity, today's first-layer rule otherwise),
   with the affected materials counted; layer blending gets its own PRD
   once its arithmetic has a source. Alternative: include blending here. It
   would cover layered surfaces, but all three phases would wait on that
   source and on a much larger shader change.
   **Decided** (2026-11-26): the recommendation. Folded into
   SMM-NG-001, SMM-SRC-010 and SMM-OBS-005.
   **Amended** (2026-11-26, owner agreed after review round 1): there is no
   "today's first-layer rule" as the recommendation described it; the index
   picks a slot's texture and replacement separately (§3.3). One layer per
   property stands, and the index changes to enforce it (SMM-SRC-010).
3. **Q3 — Which lighting does authored occlusion darken?** Recommended:
   only ambient and indirect terms that no ray has already occluded; never
   direct light or traced bounce light. Alternative (a): leave occlusion
   unused. There is no double darkening, but contact detail finer than the
   geometry is lost, and Phase 3 is withdrawn. Alternative (b): multiply
   base colour by it. This is simple, but it also darkens direct light and
   every traced result.
   **Decided** (2026-11-26): the recommendation. Folded into
   SMM-AO-001.
   **Amended** (2026-11-26, owner agreed after review round 1): the terms this
   selects are the ones SSAO already darkens. Where both apply, the term uses
   whichever darkens more, not their product, and the term's floor applies to
   the result (SMM-AO-001).
4. **Q4 — Does any phase require a city interior?** Recommended: no.
   Acceptance uses single meshes; a `citycydoniamainlevel` capture is taken
   when the machine can hold the cell, and does not block. This follows the
   owner's "compatibility before performance". Alternative: require a
   Cydonia capture at each exit. It is representative, but ties these
   phases to the Starfield memory work.
   **Decided** (2026-11-26): the recommendation. Folded into §7.2.
5. **Q5 — Where is the work tracked?** Recommended: file a new parent issue
   for this PRD, and close #3398 with a note that its definition of done is
   met and the four properties moved here. Alternative: keep #3398 as the
   parent. Its history stays in one place, but its text describes a state
   that no longer exists.
   **Decided** (2026-11-26): the recommendation. Folded into §9.

---

## 11. Document map

| Need | Go to |
|---|---|
| Why each of the four has no role today; the parked-kinds guard | [NIFAL](../engine/nifal.md) |
| Where the four roles' contract is written | [NIFAL](../engine/nifal.md) |
| The canonical material | [Material Abstraction](../engine/material-abstraction.md) |
| The GPU record and shader files | [Shader Pipeline](../engine/shader-pipeline.md), [Material Abstraction](../engine/material-abstraction.md) |
| GPU lane rules | [Material and Decal Layering PRD](material-decal-layering.md), [Shader Pipeline](../engine/shader-pipeline.md) |
| Detail levels for new roles | [Texture Streaming](../engine/texture-streaming.md) |
| What the maps cost in memory | [Memory Budget](../engine/memory-budget.md), [Texture Streaming](../engine/texture-streaming.md) |
| Starfield load budgets | [Starfield Memory Efficiency PRD](starfield-memory.md), [Memory Budget](../engine/memory-budget.md) |
| Transmissive and subsurface data | [Subsurface Scattering PRD](subsurface-scattering.md) |
| Database lookup research | [CDB Phase 2 spike](../audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-26 | First draft | — |
| 2 | 2026-11-26 | Owner decisions Q1 to Q5 recorded, each as recommended, and folded in: acceptance evidence, one layer per property (SMM-SRC-010, SMM-OBS-005), the occlusion rule, no city-cell exit, tracking | — |
| 3 | 2026-11-26 | Review round 1 folded in: layer rule, count provenance, opacity sources, census and reach split, acceptance viewer and pass rules, occlusion combine rule, map and missing-map rules, metric home, phase assignments, verify wording and pinned source, GPU lane route and glass non-goal, Fallout 76 reason, document map; SMM-PBR-006 withdrawn; owner amendments to Q2 and Q3 | claude: revise |
| 4 | 2026-11-26 | Approved by the owner: PROPOSED → APPROVED; every decision (Q1 to Q5) decided; no requirement changed from revision 3 | owner: approved |
| 5 | 2026-11-26 | APPROVED → ACTIVE: tracking issues #7419 to #7423 filed, with Phase 0's verify items listed in #7420; the §9 Phase 0 wording now allows read-only access for the census in `crates/sfmaterial`; no requirement changed | — |
