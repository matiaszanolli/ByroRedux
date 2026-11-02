# glTF and USD Asset Export — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review
round 1 (skin bind-matrix composition, the two mesh-resolution seams the
re-import must cover, collision body data, the serialisation of the carried
material record, the DDS sidecar's legal footprint, the Blender and glTF
citations, the ROADMAP "NifSkope replacement" narrowing, override
precedence against loose files, Q2 withdrawn). It states the M80 promise:
export any mesh the engine can parse, with its materials, textures, skin,
morphs, animation and collision, to glTF 2.0 and OpenUSD, and bring an
edited glTF back into a Redux profile as a plugin. Nothing below is built
as a result of this document yet. Statements about how an original engine,
a file format, an external specification or a tool behaves cite an
external source; a claim that could not be verified is marked **Phase 0:
verify** instead of being stated as fact.

**Scope**: M80 in the [ROADMAP Tier 10 table](../../ROADMAP.md#tier-10--ecosystem-unlock-post-tier-4-horizon).
In scope: a local, offline exporter that turns one source NIF (with an
optional skeleton NIF and KF files) into a glTF 2.0 bundle or a USD layer,
carrying geometry, the node hierarchy, canonical materials, textures,
skinning, morph targets, animation clips and collision; a round-trip
re-import that decodes an exported (and possibly edited) glTF bundle into
the same `Imported*` tier every other mesh source produces; and a
Redux-native plugin that binds a re-imported bundle in place of a vanilla
mesh path. Out of scope: writing NIF files (unless decision Q1 chooses
it), whole-cell or worldspace export, terrain and LOD generation, FaceGen
head generation, particle systems, editing inside the engine (M50),
hosting or sharing exports (M72), and any claim that exported game assets
may be redistributed.

**Related**: this document states the *promise*; the designs below own the
*how*. The exporter and the glTF mesh source have no design yet; Phase 0
writes it (`docs/engine/asset-export.md`, to be registered in
[docs/engine/index.md](../engine/index.md)).

| Mechanism | Owning design |
|---|---|
| `Imported*` → canonical translation boundary, the canonical `Material` | [NIFAL](../engine/nifal.md), [Material Abstraction](../engine/material-abstraction.md) |
| NIF parse, import cache, BGSM/BGEM merge, texture lookup | [Asset Pipeline](../engine/asset-pipeline.md), [NIF Parser](../engine/nif-parser.md) |
| Z-up → Y-up conversion, winding, UV origin | [Coordinate System](../engine/coordinate-system.md) |
| Skinning, KF clips, the core `AnimationClip` | [Animation](../engine/animation.md) |
| Canonical collision, Havok decode | [PHYSAL](../engine/physal.md), [Physics](../engine/physics.md) |
| Archives, the loose tier, asset precedence | [Archives](../engine/archives.md#asset-provider-precedence-and-the-loose-tier), [LMC § 5.3](legacy-mod-compatibility.md#53-assets-lmc-ast) |
| Redux-native plugin manifests and `PluginId` | [Plugin Loading](../engine/plugin-loading.md#plugin-manifests-redux-native), [world-editor § 3.4](world-editor.md#34-which-identity-the-output-plugin-follows) |
| Serialised imported data (shared need with M82) | [asset-preprocessing PRD](asset-preprocessing.md) |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md) |
| Export format mapping, glTF mesh source, override binding | `docs/engine/asset-export.md` (Phase 0 writes it) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `AEX-` prefix.

---

## 1. Product thesis

The ROADMAP row frames M80 as an "Open NIF / NifSkope replacement" for
modders "who don't want to fight 3ds Max 2010 and a 15-year-old NIF
plugin": load any vanilla mesh, export it with its materials, edit it in
Blender or Houdini, and "re-import as a content-addressed plugin". The
community route today runs through NIF-specific tooling. NifSkope edits NIF
block trees ([niftools/nifskope](https://github.com/niftools/nifskope)); the
Blender NifTools add-on reads and writes NIF but states that "there is no
stable release" ([niftools/blender_niftools_addon](https://github.com/niftools/blender_niftools_addon));
PyNifly handles Skyrim LE, Skyrim SE and Fallout 4 fully, Fallout 3, New
Vegas and Fallout 76 in limited form, and Starfield experimentally
([BadDogSkyrim/PyNifly](https://github.com/BadDogSkyrim/PyNifly)). Each tool
speaks NIF, so each must track every game's block layout.

ByroRedux already decodes every title's NIFs into one game-agnostic
representation ([NIFAL § 1](../engine/nifal.md#1-the-three-tier-model)) at
the rates the [compatibility matrix](../../ROADMAP.md#compatibility-matrix)
records, which meets the row's "NIF parser stable" dependency. Export is
therefore one writer per target format, not one per game.

Who it is for: modders who author meshes in general-purpose DCC tools, and
engine developers who want to inspect what the importer produced in a
standard viewer.

The corollary that shapes every requirement below: **the exporter writes
what the engine imported, and says exactly how faithful each part is.**
Every exported field belongs to one fidelity class (§3); nothing is
silently approximated, and the round trip (export, re-import with no edit)
is the test of the whole chain. What the engine will not do: write NIF
files unless Q1 decides it will (so, under the recommended Q1, M80 is a
NifSkope *alternative* for Redux mods, not a replacement for mods aimed at
the original games); host or share exports; or claim that an export of
game data may be redistributed. "Content-addressed plugin" is read as the
world-editor PRD reads it (§2).

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Source asset** | One NIF at an archive or loose-tier key, plus what it pulls in: the textures and external materials (`.bgsm` / `.bgem`, Starfield `.mat`) it references, an optional skeleton NIF and optional KF files. |
| **Export bundle** | What one export writes: a `.gltf` with its `.bin` and image files, a single `.glb`, or a `.usda` layer with its images. |
| **Canonical import** | The `Imported*` tier that `import_nif_scene` produces (after the BGSM/BGEM merge) plus the canonical `Material` that `translate_material` lowers for each mesh ([NIFAL § 3](../engine/nifal.md#3-materials--the-reference-realisation)). The exporter's input. |
| **Fidelity class** | One of the five classes in §3.1 that every exported field belongs to. |
| **Redux extras** | A JSON object under a `byroredux` key in a glTF `extras` field (or `customData` in USD) carrying what the target format has no slot for. Blender's manual notes that extras "have no defined namespace" ([Blender manual, glTF 2.0](https://docs.blender.org/manual/en/latest/addons/scene_gltf2.html)), so the key is the namespace. |
| **Provenance stamp** | Redux extras on the bundle root: source game, archive name, asset key, a content hash of the source bytes, exporter version. |
| **Source reference** | An archive key plus content digest that names a source file in the user's install instead of copying its bytes (AEX-TEX-002). |
| **Round-trip identity** | Export, then re-import the bundle with no edit: the re-imported `Imported*` data equals the original within the tolerances of AEX-RT-002. |
| **glTF mesh source** | The re-import path: a glTF bundle decoded into the `Imported*` tier at the seam where NIF bytes become `Imported*` data, so both consumers of that tier (§3.2 row 16) receive it unchanged. |
| **Mesh override** | A Redux-native plugin entry that serves an export bundle wherever the engine resolves a given mesh key (decision Q5). |
| **Content-addressed plugin** | The ROADMAP row's wording, read as [world-editor § 3.4](world-editor.md#34-which-identity-the-output-plugin-follows) reads it: a Redux-native plugin with a stable manifest `PluginId`, not a plugin named by a hash of its bytes. |

---

## 3. Core model

### 3.1 Fidelity classes

| Class | Name | Meaning | Reported as |
|---|---|---|---|
| **F1** | Exact | Written into a target-format slot with the same meaning; round-trips bit-for-bit or within float tolerance. | (no line) |
| **F2** | Converted | Written into a target-format slot after a defined, invertible change (axes, units, winding, key format). The inverse runs on re-import. | `converted: <field> (<rule>)` |
| **F3** | Carried | No target-format slot; written to Redux extras, ignored by other tools, restored on re-import. | `carried: <field>` |
| **F4** | Approximated | Written into a target-format slot whose meaning differs from the source (legacy shading shown as PBR, rotation keys resampled). The source value is also carried (F3) so re-import does not use the approximation. | `approximated: <field> (<why>)` |
| **F5** | Dropped | Not written. Every drop is a report line naming the field and the count. | `dropped: <field> ×N` |

- **AEX-CLS-001.** Every field the canonical import carries MUST map to
  exactly one fidelity class per target format, recorded in the design's
  mapping table (Phase 0), and the export report MUST print one line per
  F2–F5 field present in the source asset.
- **AEX-CLS-002.** An F4 field MUST also be carried as F3, so that
  round-trip identity never depends on an approximation.

The material record carried is settled by NIFAL rather than by the owner:
core PBR from the canonical `Material` is written for viewing, and the
`ImportedMaterial` is carried for re-import (AEX-MAT-004, AEX-RT-003),
because re-import may only reach `Material` through `translate_material`,
"the sole lowering step" ([NIFAL § 3](../engine/nifal.md#3-materials--the-reference-realisation)).

### 3.2 Inventory

The inventory below is the state at HEAD on 2026-11-01 (re-checked by
reading the files named). It becomes the design's mapping table in Phase 0,
which replaces this column.

| # | Data | glTF 2.0 target | USD target | Class | Today |
|---|---|---|---|---|---|
| 1 | Positions, normals, tangents, UVs, vertex colours, indices | mesh primitive attributes | `UsdGeomMesh` + primvars | F1 / F2 | `ImportedMesh` (`crates/nif/src/import/types/mesh.rs:16`) carries all of them, already in the engine's Y-up frame and in Bethesda units for every game: Starfield's meters are scaled to 70/m by `length_scale` for `bsver >= 170` (`crates/nif/src/import/units.rs:13`), using `BETHESDA_UNITS_PER_METER` = 70 (`crates/core/src/lighting.rs:16`) |
| 2 | Node hierarchy and transforms | nodes | `Xform` prims | F1 / F2 | `ImportedScene.nodes` (`crates/nif/src/import/types/scene.rs:63`) |
| 3 | Canonical material scalars and texture roles | `pbrMetallicRoughness` + `KHR_materials_*` | `UsdPreviewSurface` | F1 / F4 | `Material` (`crates/core/src/ecs/components/material.rs:90`), lowered by `translate_material` (`byroredux/src/material_translate.rs:627`, `pub(crate)` in the binary crate) |
| 4 | Source material record | — | — | F3 | `ImportedMaterial` (`crates/nif/src/import/types/material.rs:361`), after `merge_external_material`; it derives `Debug, Clone` only and `crates/nif` has no serde dependency, so no serialised form exists ([asset-preprocessing § 3.3](asset-preprocessing.md) row 12 records the same gap for M82) |
| 5 | Textures | images (PNG/JPEG) | `UsdUVTexture` file | F2 / F3 | The one engine-side CPU DDS decoder is `crates/menuxml/src/tex.rs` (BC1–BC5, DX10 header, uncompressed; "BC6H/BC7 are not decoded", line 13); `tex.dump` writes one PNG per call (`byroredux/src/commands/assets.rs:228`) |
| 6 | Skin: bones, bind inverses, 4 weights per vertex | `skin` + `JOINTS_0`/`WEIGHTS_0` | UsdSkel | F1 / F2 | `ImportedSkin` (`mesh.rs:404`); `densify_sparse_weights` "Keeps the 4 highest contributions per vertex and re-normalizes" (`crates/nif/src/import/mesh/skin.rs:817`); `global_skin_transform` is "Informational / diagnostic only — the runtime palette does NOT compose it" (`mesh.rs:433`, #4555) |
| 7 | Morph targets | morph targets | UsdSkel blend shapes | F1 | `ImportedMorphTarget` (`mesh.rs:163`) |
| 8 | Animation clips | animations | `UsdSkelAnimation` | F2 / F4 | `import_kf` and `import_embedded_animations` (`crates/nif/src/anim/entry.rs:26`, `:672`); core `AnimationClip` with `KeyType` Linear / Quadratic / Tbc / Const (`crates/core/src/animation/types.rs:18`) |
| 9 | Non-transform channels (alpha, UV, colour, morph weight, flip) | `KHR_animation_pointer` where it applies | — | F2 / F3 | `float_channels`, `color_channels`, `bool_channels`, `texture_flip_channels` on `AnimationClip` (`types.rs:271`–`280`) |
| 10 | Collision | — (no ratified extension) | UsdPhysics | F3 / F2 | `ImportedCollision` = transform + `CollisionShape` + `RigidBodyData` (`crates/nif/src/import/types/node.rs:11`); shapes Ball, Cuboid, Capsule, Cylinder, ConvexHull, TriMesh, Compound (`crates/core/src/ecs/components/collision.rs:24`); `CollisionAuthoringSummary` beside them in `CachedNifImport` (`byroredux/src/cell_loader/nif_import_registry.rs:195`) |
| 11 | Lights | `KHR_lights_punctual` | `UsdLux` | F2 | `ImportedScene.lights` (`scene.rs:130`) |
| 12 | BSX flags, bounds, attach points, furniture markers, LOD cutoffs, body-part flags | — | — | F3 | fields on `ImportedScene` (`scene.rs:78`–`118`) and `ImportedMesh` (`mesh.rs:106`, `:135`) |
| 13 | Particle emitters | — | — | F5 | `ImportedScene.particle_emitters` (`scene.rs:74`) |
| 14 | glTF or USD writer | — | — | — | none: `rg -i gltf` finds two comments only (`mesh.rs:30`, `crates/renderer/src/shader_constants_data.rs:940`) and `Cargo.lock` has no glTF or USD crate |
| 15 | NIF writer | — | — | — | none: `crates/nif` is a reader (`NifStream`, `crates/nif/src/stream.rs:69`); byte writers exist only in test fixtures (e.g. `crates/nif/src/import/mesh/bs_geometry_sentinel_slot_tests.rs:198`) |
| 16 | Consumers of the `Imported*` tier | — | — | — | two seams: the cell loader's `CachedNifImport` (`nif_import_registry.rs:195`), which `parse_and_import_spt` already feeds from `.spt` "to the same `CachedNifImport` shape every other model goes through" (`byroredux/src/cell_loader/references/import.rs:329`); and `load_nif_bytes` / `load_nif_bytes_with_skeleton` (`byroredux/src/scene/nif_loader.rs:182`, `:466`), which NPC body and equipment spawn calls (`byroredux/src/npc_spawn/loot_appearance.rs`, `npc_spawn/resumable/prebaked.rs`) |
| 17 | Redux-native plugin loading | — | — | — | designed but unwired: [Plugin Loading](../engine/plugin-loading.md) states the manifest → `DataStore` → resolver pipeline "has no production caller" (#5609) |

---

## 4. Goals and non-goals

### 4.1 Goals

- **AEX-G-001 — Any parsed mesh exports.** Every NIF that parses clean in
  the compatibility matrix's corpus exports to a glTF bundle that the
  Khronos glTF-Validator ([KhronosGroup/glTF-Validator](https://github.com/KhronosGroup/glTF-Validator))
  reports with zero errors.
- **AEX-G-002 — The round trip is the identity.** Export followed by
  re-import with no edit reproduces the canonical import (AEX-RT-002) for
  every exported asset.
- **AEX-G-003 — A Blender round trip survives.** A bundle imported into
  Blender and exported again with custom properties on re-imports with the
  same identity, Redux extras included.
- **AEX-G-004 — An edited mesh replaces a vanilla one.** A plugin binding
  an edited bundle to a vanilla mesh key renders the edited mesh wherever
  the vanilla one was placed or equipped.
- **AEX-G-005 — USD export.** The same assets export to USD layers that
  `usdchecker` ([OpenUSD toolset](https://openusd.org/release/toolset.html))
  accepts.
- **AEX-G-006 — Every loss is reported.** No F4 or F5 field is written or
  dropped without a report line (AEX-CLS-001).

### 4.2 Non-goals

- **AEX-NG-001.** Writing NIF files, unless decision Q1 picks the NIF
  writer. Under the recommended option a re-imported asset does not load
  in the original games; §6 row 1 records the consequence.
- **AEX-NG-002.** Whole-cell, worldspace, terrain (`LAND`) and distant-LOD
  export. Cell editing belongs to M50, whose PRD in turn excludes mesh
  authoring ([WED-NG-008](world-editor.md#42-non-goals)); decision Q8.
- **AEX-NG-003.** Particle-system export. No ratified or in-progress entry
  in the [Khronos extension registry](https://github.com/KhronosGroup/glTF/tree/main/extensions)
  covers particles; emitters are F5 and reported.
- **AEX-NG-004.** FaceGen head morph and tint generation (`crates/facegen`);
  a head NIF exports as the mesh it is.
- **AEX-NG-005.** Hosting, sharing or uploading exports. Hosted services
  are anti-scope ([ROADMAP](../../ROADMAP.md#what-we-are-not-doing-anti-scope));
  distribution is M72's.
- **AEX-NG-006.** Linking or copying code from GPL-licensed NIF libraries.
  nifly reads and writes NIF for every target title but is GPL-3.0
  (`/mnt/data/src/reference/nifly/LICENSE`) and this repository is MIT
  (`Cargo.toml` `license = "MIT"`); it is a reference only, under the keep
  list's "Parse data, don't emulate engine … clean room" line
  ([keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)).
- **AEX-NG-007.** Texture authoring, recompression or upscaling; the
  offline upscaler (`tools/texture-upscale`) stays separate.

---

## 5. Requirements by area

### 5.1 Exporter surface (`AEX-SURF`)

- **AEX-SURF-001.** The engine MUST offer a headless export mode that
  resolves a source asset by key through the same archive and loose-tier
  provider the cell loader uses, writes one export bundle, prints the
  export report and exits without creating a window. Precedent: the
  headless `--list-cells` mode returns before any window exists
  (`byroredux/src/list_cells.rs`, dispatched from
  `byroredux/src/boot/mod.rs:450`). The surface is decision Q4.
- **AEX-SURF-002.** A `mesh.export` console command SHOULD export the
  source asset of a live entity (default: the `prid` selection, as
  `mesh.info` reads it, `byroredux/src/commands/assets.rs:646`), so the
  export can be driven from `byro-dbg`.
- **AEX-SURF-003.** Console-driven exports MUST be confined to one output
  directory under the engine's working directory and MUST NOT overwrite an
  existing file, the contract `tex.dump` enforces with `safe_tex_dump_path`
  (`assets.rs:372`) and `create_new` (`assets.rs:316`).
- **AEX-SURF-004.** The export MUST accept an optional skeleton NIF and any
  number of KF files for one source asset, and MUST report each KF clip it
  exported or skipped by name.
- **AEX-SURF-005.** A batch form SHOULD export every NIF under an archive
  path prefix and write one summary line per asset (exported, or failed
  with reason); it is the corpus driver for §7.2.
- **AEX-SURF-006.** The export report MUST be written beside the bundle as
  a machine-readable file as well as printed, one record per F2–F5 field.

### 5.2 Geometry and hierarchy (`AEX-GEO`)

glTF anchors below are the [glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html).

- **AEX-GEO-001.** Linear distances MUST be written in meters ("The units
  for all linear distances are meters", [§ 3.4](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#coordinate-system-and-units))
  by dividing engine positions by `BETHESDA_UNITS_PER_METER`; the inverse
  runs on re-import (F2). The 70/m figure is the editors' own: "~70 units
  per meter" ([GECK: Units](https://geck.uesp.net/wiki/Units), local
  `geck-uesp-wiki/(main)/Units.wiki:2`) and 1 unit = 1.428 cm
  ([CK: Unit](https://ck.uesp.net/wiki/Unit), local
  `ck-uesp-wiki/(main)/Unit.wiki:10`).
- **AEX-GEO-002.** The bundle MUST be in glTF's frame: "+Y as up, +Z as
  forward" ([§ 3.4](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#coordinate-system-and-units)).
  `ImportedMesh` is already Y-up right-handed
  ([Coordinate System § Conversion](../engine/coordinate-system.md#conversion));
  any remaining forward-axis difference MUST be one root-node rotation, not
  a per-vertex rewrite.
- **AEX-GEO-003.** Every exported triangle MUST be counter-clockwise seen
  from its front face: under a positive-determinant transform "the winding
  order triangle faces is counterclockwise" ([§ 3.7.4](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#instantiation)).
  The engine keeps NIF triangle order and relies on the projection flip
  ([Coordinate System § Triangle Winding](../engine/coordinate-system.md#triangle-winding)),
  while `to_triangles` emits strips counter-clockwise
  ([§ NiTriStrips Winding](../engine/coordinate-system.md#nitristrips-winding)).
  **Phase 0: verify** which `ImportedMesh.indices` producers (NiTriShape,
  NiTriStrips, BSTriShape, BSGeometry) need a swap: settled by a census
  example measuring the sign of dot(face normal, mean vertex normal) per
  producer per game.
- **AEX-GEO-004.** UVs MUST be written unchanged to glTF: NIF UVs have a
  top-left origin ([Coordinate System § UV Coordinates](../engine/coordinate-system.md#uv-coordinates))
  and in glTF "(0, 0) corresponds to the upper left corner"
  ([§ 3.8.3](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#images)).
  USD MUST flip V, because `UsdUVTexture` samples (0, 0) at "the
  lower-left-hand corner of the texture image"
  ([UsdPreviewSurface](https://openusd.org/release/spec_usdpreviewsurface.html)).
- **AEX-GEO-005.** Vertex colours MUST be written as `COLOR_0` with all
  four channels from `ImportedMesh.colors` (`mesh.rs:25`).
- **AEX-GEO-006.** Tangents MUST be written as `TANGENT` with the
  bitangent sign in `w`, the convention `ImportedMesh.tangents` already
  uses (`mesh.rs:30`, `:53`).
- **AEX-GEO-007.** Every `ImportedNode` MUST become one glTF node with its
  name and local transform, and every mesh MUST hang under its
  `parent_node`, so node names survive for skin and animation binding.
- **AEX-GEO-008.** LOD slots and cutoffs (`bs_lod_cutoffs`,
  `bs_geometry_lod_slot`) MUST be carried (F3); the bundle SHOULD export
  the highest-detail slot as the visible mesh.

### 5.3 Materials (`AEX-MAT`)

- **AEX-MAT-001.** Each mesh's canonical `Material` MUST be written to glTF
  `pbrMetallicRoughness`: base colour, metallic and roughness factors and
  the base-colour and normal texture roles, as the
  [metallic-roughness model (§ 3.9.2)](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#metallic-roughness-material)
  defines them.
- **AEX-MAT-002.** For legacy inline-shader content (Oblivion, FO3, FNV)
  the metalness and roughness written are the keyword classifier's
  inference (`Material::resolve_pbr`, `material.rs:1490`), not authored
  values ([NIFAL § 3](../engine/nifal.md#3-materials--the-reference-realisation));
  those fields MUST be reported F4 and the source record carried
  (AEX-MAT-004).
- **AEX-MAT-003.** Canonical fields with a ratified extension slot SHOULD
  use it: emissive strength (`KHR_materials_emissive_strength`), IOR
  (`KHR_materials_ior`), specular colour (`KHR_materials_specular`), UV
  offset and scale (`KHR_texture_transform`), all in the ratified list of
  the [Khronos extension registry](https://github.com/KhronosGroup/glTF/tree/main/extensions).
- **AEX-MAT-004.** The mesh's `ImportedMaterial` (after the BGSM/BGEM
  merge) MUST be carried in the material's Redux extras under a schema the
  design defines field by field (interned `FixedString` paths written as
  strings), together with a digest of the core-PBR values the exporter
  wrote. Phase 0 SHOULD agree that schema with M82's serialised imported
  data ([asset-preprocessing](asset-preprocessing.md)) so the two
  milestones do not define two encodings of one type.
- **AEX-MAT-005.** Alpha test and blend MUST map to glTF `alphaMode` `MASK`
  (with `alphaCutoff` from `alpha_threshold`) and `BLEND`; two-sided
  materials MUST set `doubleSided`.
- **AEX-MAT-006.** Colour factors MUST be written as the canonical floats
  with no colour-space transform: the engine treats authored colours as raw
  monitor-space values
  ([Material Abstraction § 3](../engine/material-abstraction.md#3-the-canonical-contract-target-state)),
  whereas `baseColorFactor` "defines linear multipliers for the sampled
  texels of the base color texture", and that texture "MUST contain 8-bit
  values encoded with the sRGB opto-electronic transfer function"
  ([§ 5.22.1](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#_material_pbrmetallicroughness_basecolorfactor),
  [§ 3.9.2](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#metallic-roughness-material)).
  **Phase 0: verify** how large the resulting appearance difference is in
  Blender against an engine capture of the same asset; settled by a
  side-by-side on one FNV and one Skyrim SE asset.

### 5.4 Textures (`AEX-TEX`)

- **AEX-TEX-001.** Each texture role MUST be written as an image glTF core
  accepts: the core media types are `image/png` and `image/jpeg`
  ([§ 3.8.3](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#images)),
  and Blender's manual states that "glTF requires that images be in PNG or
  JPEG format" ([Blender manual, glTF 2.0](https://docs.blender.org/manual/en/latest/addons/scene_gltf2.html)).
- **AEX-TEX-002.** Each image MUST carry, in its Redux extras, a source
  reference to the DDS it was decoded from (archive key and digest), and
  the bundle MUST NOT contain the original DDS bytes; re-import resolves an
  unedited image from the profile's archives by that reference (AEX-RT-006,
  §6 row 2). This keeps an override plugin built from the bundle free of
  verbatim game textures it did not change. The DDS policy is decision Q3.
- **AEX-TEX-003.** A texture whose DXGI format has no decoder at HEAD
  (BC6H, BC7: `crates/menuxml/src/tex.rs:13`) MUST NOT abort the export: it
  MUST be reported F5 for the core image slot with its source reference
  carried. **Phase 0: verify** the share of BC6H/BC7 textures per game,
  settled by a census over each game's texture archives (extending
  `crates/bsa/examples/dds_mip_scan.rs`).
- **AEX-TEX-004.** Two-channel normal maps (BC5, decoded as `(r, g, 0, 255)`
  by `decode_bc5`, `tex.rs:335`) MUST be written with a reconstructed blue
  channel, since glTF's normal texture "encodes XYZ components of a normal
  vector in tangent space as RGB values"
  ([§ 3.9.3](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#additional-textures)).
- **AEX-TEX-005.** A texture referenced by several meshes MUST be written
  once per bundle.

### 5.5 Skinning and morphs (`AEX-SKIN`)

- **AEX-SKIN-001.** Each `ImportedSkin` MUST become a glTF skin whose
  joints are the bone nodes by name and whose `inverseBindMatrices` are the
  bones' `bind_inverse` matrices alone, unit-scaled per AEX-GEO-001 (the
  translation column scaled, as `units.rs` does). `global_skin_transform`
  MUST NOT be composed in, because the runtime palette is
  `bone_world × bind_inverse` and composing it "double-applies the offset
  on every legacy body NIF" (`mesh.rs:433`–`444`, #4555); it MUST be
  carried (F3).
- **AEX-SKIN-002.** Weights MUST be written as one `JOINTS_0`/`WEIGHTS_0`
  set: one glTF set holds 4 influences, with more "stored in subsequent
  sets" ([§ 3.7.3.3](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#skinned-mesh-attributes)),
  and `ImportedSkin` holds 4 per vertex. **Phase 0: verify** how many
  vertices per game lose influences in `densify_sparse_weights` (the
  NiSkinData path), settled by extending
  `crates/nif/examples/r6_bone_count_survey.rs`; if any, the exporter MAY
  read the raw NiSkinData and write `JOINTS_1`/`WEIGHTS_1`.
- **AEX-SKIN-003.** A skin bone that is not a node of the source NIF MUST
  be taken from the skeleton NIF given with AEX-SURF-004, and the export
  MUST fail with the missing bone names when neither file has it.
  **Phase 0: verify** the share of skinned NIFs per game whose every bone
  is present in the same file, settled by a census example.
- **AEX-SKIN-004.** Morph targets MUST be written as glTF morph targets
  with their names in `mesh.extras.targetNames`, the convention the spec
  says "most tools use" ([§ 3.7.2.2](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#morph-targets)).
- **AEX-SKIN-005.** Body-part flags and per-triangle body-part indices
  (`body_part_flags`, `triangle_body_parts`) MUST be carried (F3), since
  dismemberment and equip-time skin hiding depend on them.

### 5.6 Animation (`AEX-ANIM`)

- **AEX-ANIM-001.** Each exported clip MUST become one glTF animation named
  after the clip, with samplers interpolated `LINEAR`, `STEP` or
  `CUBICSPLINE` ([§ 3.11](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#animations)).
- **AEX-ANIM-002.** `Const` keys MUST export as `STEP` and `Linear` keys as
  `LINEAR` (F2). nif.xml defines the source key types (`LINEAR_KEY`,
  `QUADRATIC_KEY`, `TBC_KEY`, `XYZ_ROTATION_KEY`, `CONST_KEY`,
  `/mnt/data/src/reference/nifxml/nif.xml:399`).
- **AEX-ANIM-003.** Quadratic and TBC translation and scale keys SHOULD
  export as `CUBICSPLINE` with tangents derived from the stored forward and
  backward values. **Phase 0: verify** that the core sampler and the glTF
  cubic formula agree at every sample point, settled by a unit test that
  evaluates both on the core's own key fixtures; where they disagree the
  channel MUST be resampled and reported F4.
- **AEX-ANIM-004.** Rotation channels with Quadratic or TBC keys MUST be
  resampled to `LINEAR` at the clip's key times plus a fixed rate set in
  the design, and reported F4: Gamebryo interpolates them by squad
  (`NiRotKey.h:49`, with the intermediate quaternion at `NiBezRotKey.h:30`,
  in `/mnt/data/src/reference/gamebryo-v32/Include/`), which glTF's
  per-component cubic does not reproduce.
- **AEX-ANIM-005.** Cycle type, frequency, phase, weight, accumulation root
  and text keys MUST be carried (F3) on the animation.
- **AEX-ANIM-006.** Float channels targeting morph weights SHOULD export as
  glTF `weights` channels (F2); the other non-transform channels SHOULD use
  `KHR_animation_pointer` (ratified, [extension registry](https://github.com/KhronosGroup/glTF/tree/main/extensions))
  where a material property is the target and MUST otherwise be carried
  (F3).

### 5.7 Collision (`AEX-COL`)

- **AEX-COL-001.** Each `ImportedCollision` MUST be carried whole in glTF
  Redux extras (F3): transform, `CollisionShape` tree and `RigidBodyData`,
  so AEX-RT-002 can compare body data as well as shapes. glTF has no
  ratified collision or physics extension: `KHR_collision_shapes` and
  `KHR_physics_rigid_bodies` are both Review Drafts in the
  [Khronos extension registry](https://github.com/KhronosGroup/glTF/tree/main/extensions).
- **AEX-COL-002.** TriMesh and ConvexHull shapes SHOULD also be written as
  hidden meshes named by a fixed suffix, so a DCC user can see and edit
  them; re-import MUST prefer an edited hidden mesh over the carried copy.
- **AEX-COL-003.** USD export MUST write collision through UsdPhysics:
  primitive shapes as its sphere, cube, capsule and cylinder shapes, hulls
  and meshes through `UsdPhysicsMeshCollisionAPI` with
  `physics:approximation` `convexHull` or none
  ([UsdPhysics](https://openusd.org/release/api/usd_physics_page_front.html)).
- **AEX-COL-004.** A source asset whose `CollisionAuthoringSummary` records
  authored Havok that produced no `ImportedCollision` (the FO4+ packed case
  PHYSAL lists as having one remaining payload class,
  [§ 5](../engine/physal.md#5-per-concern-inventory)) MUST report its
  collision as F5 and carry the summary (F3), not omit it silently.

### 5.8 Provenance and extras (`AEX-META`)

- **AEX-META-001.** Every bundle MUST carry the provenance stamp in the
  root's Redux extras: game, archive file name, asset key, source content
  digest, exporter version.
- **AEX-META-002.** Every F3 field MUST live under the `byroredux` key of
  the nearest glTF object's `extras` (node, mesh, material, image, skin,
  animation), never under another key.
- **AEX-META-003.** Redux extras MUST carry a schema version, and re-import
  MUST refuse an unknown major version with a message naming it.
- **AEX-META-004.** The glTF `asset.copyright` field MUST be filled or left
  empty according to decision Q7.

### 5.9 Re-import and round trip (`AEX-RT`)

- **AEX-RT-001.** The engine MUST decode a glTF bundle into the `Imported*`
  tier (`ImportedScene`, `ImportedMesh`, `ImportedMaterial`,
  `ImportedCollision`, clips) at the seam where NIF bytes become that tier,
  so both consumers in §3.2 row 16 (the cell loader's `CachedNifImport` and
  the `load_nif_bytes` family) receive it with no glTF-specific branch past
  that seam (R5, [NIFAL § 1](../engine/nifal.md#1-the-three-tier-model)).
  **Phase 0: verify** that row 16 lists every mesh-resolution site, settled
  by tracing each `import_nif_scene` / `import_nif*` caller in
  `byroredux/src` and recording the list in the design.
- **AEX-RT-002.** Round-trip identity MUST hold for every exported asset:
  equal index buffers after the winding inverse; positions, normals,
  tangents and UVs within 1e-5 relative; equal joint indices and weights
  within 1e-6; equal `ImportedMaterial`; equal `ImportedCollision`
  including `RigidBodyData`; equal carried fields; equal clip key counts for
  F1/F2 channels. Phase 0 confirms the tolerances against float32 round-off
  of the unit conversion.
- **AEX-RT-003.** Re-import MUST rebuild each mesh's `ImportedMaterial`
  from Redux extras when present and leave lowering to `translate_material`
  ([NIFAL § 3](../engine/nifal.md#3-materials--the-reference-realisation)):
  the glTF source module MUST construct no canonical type (`Material`,
  `LightSource`, a physics component), which a source-scan test in the
  module asserts.
- **AEX-RT-004.** When a material's core-PBR values no longer match the
  digest of AEX-MAT-004, the edited values MUST win for the fields core PBR
  covers, and the report MUST say which fields were taken from the edit.
- **AEX-RT-005.** A material with no Redux extras (authored new in a DCC
  tool) MUST be decoded into an `ImportedMaterial` whose PBR fields
  (`is_pbr`, `metalness_override`, `roughness_override`, `material.rs:400`
  onward) hold the authored glTF factors and whose other fields keep
  `ImportedMaterial`'s defaults, so nothing is fabricated before
  `translate_material` runs.
- **AEX-RT-006.** An image whose bytes still match the digest recorded at
  export MUST re-import as the DDS its source reference names, read from
  the profile's archives; an edited image MUST be uploaded with a full mip
  chain generated at load. A source reference that does not resolve in the
  current profile MUST be reported, and the PNG used.
- **AEX-RT-007.** Re-import MUST accept a bundle written by Blender's glTF
  exporter with custom properties enabled, which it stores "in the extras
  field on the corresponding object" for "most objects"
  ([Blender manual, glTF 2.0](https://docs.blender.org/manual/en/latest/addons/scene_gltf2.html)).
  **Phase 0: verify** that Blender keeps each Redux extras object on the
  same node, mesh, material, image, skin and animation it was read from,
  settled by a scripted `blender --background` import/export of one
  skinned, one static and one animated bundle.
- **AEX-RT-008.** Re-import MUST convert skin joints back to bone names and
  resolve them the way the engine resolves NIF bones, by name against the
  placement's node map ([NIFAL § Skinning](../engine/nifal.md#skinning--half-stale-re-verified-2026-09-10-loose-nif-path-only-plus-a-starfield-bone-name-sub-slice)).

### 5.10 Plugin binding (`AEX-PLG`)

- **AEX-PLG-001.** A Redux-native plugin MUST be able to declare that a
  bundle serves a mesh key, so every mesh-resolution site of AEX-RT-001
  (placed references and equipped or worn models alike) spawns the bundle
  for that key (the mechanism is decision Q5). Redux-native plugins have no
  production loader at HEAD (#5609, [Plugin Loading](../engine/plugin-loading.md));
  this requirement depends on it.
- **AEX-PLG-002.** The plugin MUST be identified by its manifest
  `PluginId` ([world-editor § 3.4](world-editor.md#34-which-identity-the-output-plugin-follows)),
  and the override MUST be keyed by the lowercase, backslash-separated
  asset key the loose tier and archives use
  ([Archives § loose tier](../engine/archives.md#asset-provider-precedence-and-the-loose-tier)).
- **AEX-PLG-003.** When two enabled plugins override the same key, the
  winner MUST follow profile order and the loser MUST be reported, as for
  any other conflict the resolver handles
  ([Plugin Loading § DependencyResolver](../engine/plugin-loading.md#dependencyresolver--conflict-resolution));
  where an override shadows a loose or archived file at the same key, the
  rank Q5 decides MUST apply and the shadowed file MUST appear in the
  preflight report LMC defines for every asset path claimed by more than
  one root or archive ([LMC-DX-001](legacy-mod-compatibility.md#511-tooling-and-explainability-lmc-dx)).
- **AEX-PLG-004.** `mesh.info` MUST report, for an entity spawned from a
  bundle, the bundle path and the plugin that bound it.
- **AEX-PLG-005.** The engine SHOULD scaffold an override plugin (manifest
  plus one override entry) from an export bundle and a target key, written
  to the local disk only, so the modder writes no TOML by hand.

### 5.11 USD (`AEX-USD`)

- **AEX-USD-001.** USD export MUST write the same canonical import as glTF:
  `UsdGeomMesh` with primvars, `UsdPreviewSurface` materials with the
  inputs the spec lists (diffuse colour, emissive colour, metallic,
  roughness, opacity, opacity threshold, IOR, normal, occlusion,
  [UsdPreviewSurface](https://openusd.org/release/spec_usdpreviewsurface.html)).
- **AEX-USD-002.** Normal maps MUST be read through `UsdUVTexture` with
  `sourceColorSpace` "raw" and the 8-bit scale (2, 2, 2, 1) and bias
  (−1, −1, −1, 0) the spec gives
  ([UsdPreviewSurface](https://openusd.org/release/spec_usdpreviewsurface.html)).
- **AEX-USD-003.** Skins MUST be written through UsdSkel, where "No
  restrictions are placed on the elementSize when defining joint
  influences" ([UsdSkel schemas](https://openusd.org/release/api/_usd_skel__schemas.html));
  weights MUST be normalized before writing, since "UsdSkel does not
  automatically normalize weights when reading data". **Phase 0: verify**
  how `bind_inverse` maps onto the Skeleton's bind transforms and whether
  the mesh's `geomBindTransform` stays identity (AEX-SKIN-001 keeps
  `global_skin_transform` out of the palette), settled by reading the
  UsdSkel binding schema in full and recording the mapping in the design.
- **AEX-USD-004.** Clips MUST be written as `SkelAnimation` with authored
  translation, rotation and scale components: "An animation source is only
  valid if its translation, rotation, and scale components are all
  authored" ([UsdSkel schemas](https://openusd.org/release/api/_usd_skel__schemas.html)).
- **AEX-USD-005.** Redux extras MUST be written as `customData` on the
  corresponding prim. USD re-import is not promised (decision Q6).

### 5.12 Legal (`AEX-LEGAL`)

- **AEX-LEGAL-001.** The engine MUST NOT ship, download or upload any
  exported asset; the exporter reads only the user's installed game data
  and writes only to the user's local disk.
- **AEX-LEGAL-002.** No acceptance corpus, fixture or golden file derived
  from game data MAY be checked in; tests that need data read it from the
  user's install, as the NIF parse-rate corpus does, and as LMC keeps its
  corpus "local and opt-in"
  ([LMC-LEGAL-001](legacy-mod-compatibility.md#513-legal-lmc-legal)).
- **AEX-LEGAL-003.** The exporter's documentation and every export report
  MUST state that an export of game data, and an edit derived from it,
  remains the publisher's material, and that the engine makes no claim
  about its redistribution. For context, the editors' own licences limit
  editor-made mods to non-commercial use "solely in connection with the
  Product" ([Skyrim Mod: EULA](https://en.uesp.net/wiki/Skyrim_Mod:EULA),
  the Skyrim SE Creation Kit EULA, local
  `uesp-wiki/Skyrim Mod/EULA.wiki:20`) or "solely in connection with the
  applicable Product(s)" ([Oblivion Mod: EULA](https://en.uesp.net/wiki/Oblivion_Mod:EULA),
  the Construction Set EULA, local `uesp-wiki/Oblivion Mod/EULA.wiki:9`).
  **Phase 0: verify** which licence governs content produced with another
  tool for each of the seven titles (only the two TES editor EULAs are
  transcribed locally); settled by the owner recording a reading of each
  game's licence text in the design, not by the engine.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Edited meshes reach the game as NIF, written by NIF tools; Gamebryo writes NIF through `NiStream::Save` (`NiStream.h:50`, gamebryo-v32) | Under the recommended Q1, an edited mesh reaches the engine as glTF (AEX-RT-001) | Re-export to NIF for Redux-only mods | A Redux override does not load in the original game, so a mod built this way is Redux-only → AEX-NG-001, Q1 |
| 2 | Games load pre-compressed DDS with authored mips | Unedited images resolve back to the original DDS by source reference; edited images load as PNG with mips generated at load (AEX-RT-006) | Texture conversion tools for Redux-only mods | Higher VRAM and load time for edited textures than for BCn → AEX-RT-006; unedited ones keep the DDS (AEX-TEX-002) |
| 3 | Legacy games shade with their own non-PBR shader model | Exported PBR values for legacy content are the classifier's inference | — | A modder treats inferred PBR as authored → F4 report (AEX-MAT-002), source record carried (AEX-MAT-004) |
| 4 | NiSkinData can store more than four weights per vertex (the importer keeps the four highest, `skin.rs:817`) | One four-influence set | — | Weight loss on dense rigs → AEX-SKIN-002 census |
| 5 | Gamebryo rotation keys interpolate by squad | Resampled linear rotation keys | — | Larger files, slight motion difference → AEX-ANIM-004 report |
| 6 | NIF space is Z-up in game units | glTF/USD in meters, Y-up | NIF-space hand conversions in DCC scripts | None beyond AEX-GEO-001/002; both are inverted on re-import |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Export depends only on parse, import and material translation, so every
title whose NIFs parse is in scope. Re-import produces game-agnostic
`Imported*` data, so it is not per-game: a bundle loads in whatever profile
the engine can render. Waves follow the geometry and material families.

| Game | In scope | Why / wave |
|---|---|---|
| Fallout: New Vegas | yes, wave 1 | Engine reference title; classic `NiTriShape` + `NiSkinInstance`, inline shader properties, KF animation |
| Skyrim (LE / SE / AE) | yes, SE wave 1, LE wave 2 | SE covers `BSTriShape` packed geometry and `BSLightingShaderProperty`; LE shares the classic geometry path and is parse-only in the [compatibility matrix](../../ROADMAP.md#compatibility-matrix), so its re-import is exercised through other profiles |
| Fallout 3 | yes, wave 2 | Shares FNV's NIF version and import paths |
| Oblivion | yes, wave 2 | v20.0.0.x and v10.x NetImmerse files; KF and inline materials like FNV |
| Fallout 4 | yes, wave 2 | Half-float vertices, BGSM/BGEM merged before translation; collision partly F5 per AEX-COL-004 |
| Fallout 76 | yes, export only, wave 3 | Parse-only in the compatibility matrix; no engine profile renders its content, so no re-import acceptance |
| Starfield | yes, wave 3 | External `.mesh` geometry and `.mat`/CDB materials; the matrix notes a `BSWeakReferenceNode` remainder, which the export report surfaces as F5 where it drops data |

No requirement adds a per-game branch to the exporter: per-game differences
are resolved by `import_nif_scene` and `translate_material` before the
exporter runs.

### 7.2 Acceptance

Scenarios use the user's installed game data; nothing is checked in
(AEX-LEGAL-002).

| # | Scenario | Driven by | Data | Pass rule |
|---|---|---|---|---|
| A1 | Synthetic box, skinned quad and two-key clip export with meters, Y-up and counter-clockwise faces | new unit tests in the exporter module (named in the design, e.g. `export::tests::box_exports_ccw_in_meters`) | none | all pass |
| A2 | Corpus export | new ignored test `export_real_nifs -- --ignored export_rate`, modelled on `crates/nif/tests/parse_real_nifs.rs` over `Game::mesh_archives()` | every in-wave game's mesh archives | each clean-parsing NIF exports; glTF-Validator reports zero errors |
| A3 | Round-trip identity | same corpus test, `-- --ignored roundtrip_rate` | as A2 | AEX-RT-002 holds per asset |
| A4 | Blender round trip | new script `docs/smoke-tests/aex-blender-roundtrip.sh` running `blender --background --python` import then export with custom properties | a local Blender install; 20 assets per in-wave game chosen in Phase 0 | AEX-RT-002 holds after the Blender pass |
| A5 | Live export | `--bench-hold` on FNV Prospector Saloon, then `byro-dbg`: `prid <entity>`, `mesh.export`, then the headless re-import check on the written bundle | FNV data | bundle written under the export directory; A3 rule holds |
| A6 | Override renders | new script `docs/smoke-tests/aex-override.sh`: profile with a scaffolded override plugin binding a bundle whose geometry was offset 1 m, once for a placed static and once for an equipped armour model, `--bench-hold`, `byro-dbg` `mesh.info` on each | FNV and Skyrim SE data | `mesh.info` reports the bundle and plugin (AEX-PLG-004) on both; each entity's world bound is offset by 1 m |
| A7 | USD validity | corpus test `-- --ignored usd_rate` writing `.usda`, each checked by `usdchecker` | as A2 | zero `usdchecker` errors |

---

## 8. Metrics

The numbers live in ROADMAP: Phase 1 adds an export column group beside
the [compatibility matrix](../../ROADMAP.md#compatibility-matrix); this
document defines what is counted.

| Metric | Definition | Granularity |
|---|---|---|
| Export rate | NIFs exported without error ÷ NIFs that parse clean | per game, per format |
| Validator-clean rate | glTF bundles with zero glTF-Validator errors ÷ bundles written | per game |
| Round-trip identity rate | bundles passing AEX-RT-002 ÷ bundles written | per game |
| Blender round-trip rate | A4 assets passing AEX-RT-002 after Blender ÷ A4 assets | per game |
| Loss census | F4 and F5 report lines by field | per game, per field |
| USD check rate | `.usda` layers with zero `usdchecker` errors ÷ layers written | per game |

---

## 9. Delivery sequence

M80 depends on the NIF parser (met: [compatibility matrix](../../ROADMAP.md#compatibility-matrix)).
Phase 4 also depends on a production loader for Redux-native plugins
(#5609, M46).

**Phase 0 — Design, census and decisions (no engine code).** Write
`docs/engine/asset-export.md` with the field-by-field mapping table that
replaces §3.2's Today column, the Redux-extras schema (agreed with M82 per
AEX-MAT-004) and the mesh-resolution site list (AEX-RT-001), and register
it. Run the census examples for winding (AEX-GEO-003), skin-bone presence
(AEX-SKIN-003), dropped influences (AEX-SKIN-002) and texture formats
(AEX-TEX-003). Run the scripted Blender extras check (AEX-RT-007) on
hand-built bundles. Pick the glTF writer crate and confirm its licence is
MIT-compatible. Record the licence reading of AEX-LEGAL-003. Resolve the §10
decisions and close every **Phase 0: verify** item. Exit: the design and
the four census tables are checked in, and every open §10 item carries
**Decided**.

**Phase 1 — Static meshes, wave 1.** AEX-SURF-001, -003, -005, -006;
AEX-GEO-001…008; AEX-MAT-001…006; AEX-TEX-001…005; AEX-META-001…004;
AEX-CLS-001, -002. Exit: A1 passes and A2 reports its export and
validator-clean rates for FNV and Skyrim SE static meshes, published in
ROADMAP.

**Phase 2 — Skin, morphs, animation, collision.** AEX-SURF-002, -004;
AEX-SKIN-001…005; AEX-ANIM-001…006; AEX-COL-001, -002, -004. Exit: A2
covers every FNV and Skyrim SE NIF, and A5 passes.

**Phase 3 — Re-import and round trip, then waves 2 and 3.** AEX-RT-001…008.
Exit: A3 and A4 publish their rates for every in-scope game, and A2 covers
every wave-2 and wave-3 game.

**Phase 4 — Plugin binding.** AEX-PLG-001…005, after the Redux-native
plugin loader lands. Exit: A6 passes on FNV and Skyrim SE.

**Phase 5 — USD export.** AEX-USD-001…005, AEX-COL-003. Exit: A7 publishes
its rate for every in-scope game.

AEX-LEGAL-001…003 hold from Phase 1 on and are checked at every exit.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — What does "re-import as a plugin" produce?** Recommended: glTF
   itself is the Redux-native mesh source (AEX-RT-001), decoded into the
   `Imported*` tier upstream of every `translate()` site, so NIFAL's single
   boundary is unchanged and every downstream path is reused with no NIF
   serializer. Consequences: mods built this way load in Redux only, and
   the ROADMAP M80 row's "Open NIF / NifSkope replacement" wording narrows
   to an alternative for Redux mods, which is a ROADMAP row edit (README
   §1 T1). Alternative A: a NIF writer in `crates/nif`. It makes the output
   usable by the original games and by NifSkope, the literal reading of the
   row; it needs a serializer per block family (Gamebryo's own writer is
   per-class `SaveBinary` behind `NiStream::Save`, `NiStream.h:50`). A
   splice writer that re-encodes only edited geometry and copies other
   blocks verbatim is possible from v20.2.0.5, where the header records
   every block's size (`nif.xml:1977`), which excludes Oblivion.
   Alternative B: an engine-native mesh format shared with M82
   ([asset-preprocessing](asset-preprocessing.md)); M82 keeps `translate()`
   on load too (BAKE-DER-003), so the boundary would hold, but it couples
   M80 to a bundle format M82 has not decided and DCC tools cannot open.
2. **Q2 — Which material record does the export carry?** (withdrawn, rev 2:
   not an owner decision. Rev 1's recommendation, core PBR for viewing plus
   the carried `ImportedMaterial` for re-import, is forced by NIFAL's
   one-lowering-site rule and AEX-MAT-001, and now stands as the model
   statement in §3.1.)
3. **Q3 — DDS handling.** Recommended: decoded PNG in the core slot plus a
   source reference (archive key and digest) instead of a DDS copy
   (AEX-TEX-001, -002): Blender opens the bundle, unedited textures
   round-trip exactly from the user's own install, and an override plugin
   built from the bundle carries no unchanged game textures. Consequence:
   re-import of an unedited texture needs the source game's archives in the
   profile (AEX-RT-006 falls back to the PNG). Alternative: copy the DDS
   beside the bundle; self-contained, but every bundle and any mod built
   from it contains verbatim publisher textures. Alternative:
   `MSFT_texture_dds`, a vendor extension
   ([extension registry](https://github.com/KhronosGroup/glTF/tree/main/extensions))
   that Blender's manual does not mention; files stay small but open
   untextured in Blender. Alternative: transcode to KTX2 with
   `KHR_texture_basisu`; ratified, but a lossy re-encode of already-lossy
   data.
4. **Q4 — Exporter surface.** Recommended: a headless mode of the engine
   binary plus `mesh.export` (AEX-SURF-001, -002), because
   `translate_material` is `pub(crate)` inside the `byroredux` binary crate
   (`material_translate.rs:627`) and a headless mode already exists
   (`--list-cells`). Alternative: a standalone `tools/` binary like
   `byro-texture-upscale`; cleaner to ship, but it first moves material
   translation and the asset provider into a library crate.
5. **Q5 — How a bundle replaces a vanilla mesh, and at what rank.**
   Recommended: an explicit override table in the Redux-native plugin
   keyed by asset key (AEX-PLG-001, -002), conflict-resolved like any
   record and waiting for #5609, ranked above every loose root and archive
   for its key because the binding is explicit; each shadowed loose or
   archived file is reported (AEX-PLG-003). Alternative A: the override
   ranks with the plugin's own mod root in LMC's loose-root order
   ([LMC-AST-010](legacy-mod-compatibility.md#53-assets-lmc-ast)), so a
   later mod's loose `.nif` beats it; consistent with mod-manager habits,
   but an explicit binding can silently lose. Alternative B: the loose tier
   serves `x.glb` for `x.nif` when both stems match; works without the
   plugin loader, but is an implicit rule with no owner to report in a
   conflict. Alternative C: bundles only back new base records; nothing
   vanilla is replaced, which drops AEX-G-004.
6. **Q6 — USD scope.** Recommended: export only, written as `.usda` text by
   the engine and validated by an external `usdchecker` (AEX-USD-001…005).
   Alternative: drop USD from M80 and deliver glTF only; Houdini users
   convert through their own tools. Alternative: link OpenUSD's C++
   libraries through `crates/cxx-bridge` for binary `.usdc` and USD
   re-import; a heavy native dependency and a licence review.
7. **Q7 — Copyright field and notice.** Recommended: leave
   `asset.copyright` empty, write provenance in Redux extras
   (AEX-META-001), and state the caveat in documentation and the report
   (AEX-LEGAL-003); the engine names no rights holder it cannot verify.
   Alternative: fill `asset.copyright` with the source game's title from
   the provenance stamp, which every viewer shows, at the risk of reading
   as an attribution or licence claim.
8. **Q8 — Export unit.** Recommended: one source asset (NIF plus optional
   skeleton and KFs). Alternative: a cell's placed references with their
   transforms as one scene; useful for kitbashing, but it overlaps M50's
   cell editing and pulls in REFR resolution, precombines and LOD.

---

## 11. Document map

| Need | Go to |
|---|---|
| The canonical tier the exporter reads, the one-site translation rule | [nifal.md](../engine/nifal.md) |
| The canonical `Material` contract and colour convention | [material-abstraction.md](../engine/material-abstraction.md), [nifal.md § 3](../engine/nifal.md#3-materials--the-reference-realisation) |
| Import cache, BGSM/BGEM merge, texture lookup | [asset-pipeline.md](../engine/asset-pipeline.md) |
| Axes, winding, UV origin | [coordinate-system.md](../engine/coordinate-system.md) |
| Clips, key types, skinning runtime | [animation.md](../engine/animation.md) |
| Canonical collision and Havok decode status | [physal.md](../engine/physal.md) |
| Archives and the loose tier | [archives.md](../engine/archives.md#asset-provider-precedence-and-the-loose-tier) |
| Redux-native plugins, `PluginId`, conflict resolution | [plugin-loading.md](../engine/plugin-loading.md) |
| Per-game engine tiers | [game-compatibility.md](../engine/game-compatibility.md) |
| Asset precedence, loose-root order, legal stance shared with mod compatibility | [legacy-mod-compatibility.md § 5.3](legacy-mod-compatibility.md#53-assets-lmc-ast), [§ 5.13](legacy-mod-compatibility.md#513-legal-lmc-legal) |
| Plugin identity reading, M50's mesh-authoring non-goal | [world-editor.md](world-editor.md) |
| Serialised imported data, the M82 native-format alternative | [asset-preprocessing.md](asset-preprocessing.md) |
| Smoke-test index | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Live milestone state, parse rates and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: skin bind matrices exclude `global_skin_transform` (#4555); re-import decodes into the `Imported*` tier covering both mesh-resolution seams (`CachedNifImport`, `load_nif_bytes`); collision carries `ImportedCollision` with body data; `ImportedMaterial` serialisation schema shared with M82; DDS carried by source reference, not copied; Blender and glTF citations corrected; ROADMAP "NifSkope replacement" narrowing in Q1; override rank against LMC loose roots in Q5; Q2 withdrawn as settled by NIFAL | claude: revise |
