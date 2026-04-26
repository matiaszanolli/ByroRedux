# Skyrim SE Compatibility Audit — 2026-04-24

**Scope**: Skyrim SE (NIF v20.2.0.7, BSVER 100; BSA v105 LZ4) end-to-end — vertex format, archive reader, shader variants, specialty nodes, real-data validation, ESM readiness.
**Baseline**: Prior audit `AUDIT_SKYRIM_2026-04-22.md` (2 days ago). Per-dimension drafts in `/tmp/audit/skyrim/dim_{1..6}.md`.
**Method**: Dimensions ran as parallel agents; findings verified against current code (HEAD `a2a3fcd`) before inclusion.

---

## Executive Summary

**The Skyrim audit shows a content-rendering regression and a parser-coverage drift since 2026-04-22.**

- **Parse-rate regression: 100.00 % → ~99.7 %** on `Skyrim - Meshes0/1.bsa` (~60 files now flagged "truncated/recovered" — was 0). Root causes are SK-D5-03 (`BSBoneLODExtraData` missing parser, 52 actor skeletons) and SK-D5-04 (stream drift on 7 parser types).
- **Skinned actor bodies still import 0 meshes** (existing #559 SK-D5-02 — global vertex buffer in `NiSkinPartition` discarded). Confirmed empirically against `malebody_1.nif`, `dragon.nif`. **No NPC/creature renders.**
- **Tree LODs / SpeedTree content imports 0 meshes** (NEW SK-D5-02 here): `parse_nif`'s root selector at lib.rs:454-457 only matches the literal block-name `"NiNode"`, so scenes rooted at `BSTreeNode` / `BsValueNode` / `BsMultiBoundNode` / `NiSwitchNode` / `NiLODNode` / `NiSortAdjustNode` / `BsRangeNode` skip the real root and pick a leaf NiNode child.
- **FO76 SkinTint shader path silently miscolours** every NPC (NEW SK-D3-04): `material_kind == 4` reaches the GPU but `triangle.frag:734` only dispatches `== 5u`. SkinTint colours imported but never multiplied.

| Sev | Count | NEW IDs |
|--|--:|--|
| CRITICAL | 0 | — |
| HIGH | 3 | SK-D5-02 root selector · SK-D3-04 FO76 SkinTint · SK-D1-01 bone-index range |
| MEDIUM | 6 | SK-D2-03 embed-name flag · SK-D2-06 v105 unit tests · SK-D3-05 MultiLayer/Eye stubs · SK-D4-01 BSEffect falloff · SK-D5-03 BSBoneLODExtraData · SK-D5-04 stream drift bundle · SK-D1-02 vertex alpha · |
| LOW | 11 | SK-D1-04, SK-D1-05, SK-D2-02, SK-D2-04, SK-D2-05, SK-D2-07, SK-D3-06, SK-D3-07, SK-D4-02, SK-D4-03, SK-D5-05, SK-D6-NEW-01, SK-D6-NEW-02, SK-D6-NEW-03 |

Existing-issue confirmations: #559 (SK-D5-01 here = duplicate), #351 (SK-D1-03 here = duplicate, dropped). Dim 6 prompt premise was stale: the `legacy/tes5.rs` stub no longer exists; Skyrim ESM parses through the unified `esm/` walker today and Whiterun renders at 253 FPS / 1932 entities (#566 LGTM still open).

---

## Forward Blocker Chain — what must land for vanilla Skyrim SE actors to render

1. **HIGH #559 SK-D5-02 (open)** — `NiSkinPartition` SSE global vertex buffer discarded → every NPC/creature body invisible.
2. **HIGH SK-D5-02 (this audit, NEW)** — `parse_nif` root selector → tree LODs, `BSTreeNode`-rooted scenes empty.
3. **HIGH SK-D3-04 (this audit, NEW)** — FO76 SkinTint variant gate.
4. **MEDIUM SK-D5-03 / SK-D5-04 (this audit, NEW)** — parser drift; restores 100 % parse rate.
5. **MEDIUM #561 (open)** — multi-master CLI; DLC interiors today render empty.

Items 1+2 unblock visible content; 3 fixes silent miscolour; 4 fixes parse-rate regression; 5 unlocks DLC.

---

## Shader Variant Coverage Matrix (BSLightingShaderType)

| Type | Variant | Parse | Import | Render |
|----:|----|:--:|:--:|:--:|
| 0 | Default | ✓ | ✓ | ✓ |
| 1 | EnvironmentMap | ✓ | ✓ | partial (no branch gate) |
| 2 | Glow | ✓ | ✓ | ✓ |
| 3 | Parallax | ✓ | ✓ | ✓ POM |
| 5 | SkinTint | ✓ | ✓ | ✓ Skyrim/FO4; ✗ FO76 (SK-D3-04) |
| 6 | HairTint | ✓ | ✓ | ✓ |
| 7 | ParallaxOcc | ✓ | ✓ | ✓ POM |
| 11 | MultiLayerParallax | ✓ | ✓ | ✗ stub (SK-D3-05) |
| 14 | SparkleSnow | ✓ | ✓ | ✓ |
| 16 | EyeEnvmap | ✓ | ✓ | ✗ stub (SK-D3-05) |

---

## Findings by Dimension

### Dimension 1 — BSTriShape Vertex Format

#### SK-D1-01: Bone indices `[u8; 4]` allow silent aliasing on multi-partition skins (HIGH)
- **Location**: [crates/nif/src/blocks/tri_shape.rs:240, 689-692](crates/nif/src/blocks/tri_shape.rs#L240); consumer at [crates/nif/src/import/mesh.rs:496](crates/nif/src/import/mesh.rs#L496).
- **Description**: `read_vertex_skin_data` reads 4 × u8 bone indices with no awareness of which `NiSkinPartition` partition the vertex belongs to. Skyrim SE skins with > 256 distinct bones (Argonian/Khajiit body + worn armor) split into multiple partitions, each reissuing local 0..255 indices. The importer hands partition-local indices to a global bone array. Wrong bones bind silently when the partition splitter has been exercised.
- **Fix**: promote to `Vec<[u16; 4]>` and remap during partition unpacking, OR emit a parser warning when `inst.bone_refs.len() > 256` so the bug surfaces.

#### SK-D1-02: Vertex alpha channel dropped at import (MEDIUM)
- **Location**: [crates/nif/src/import/mesh.rs:270-278](crates/nif/src/import/mesh.rs#L270); `ImportedMesh::colors` at [crates/nif/src/import/mod.rs:161](crates/nif/src/import/mod.rs#L161).
- **Description**: `BsTriShape.vertex_colors` parses RGBA per nif.xml `ByteColor4`; importer keeps only `[c[0], c[1], c[2]]`. `ImportedMesh::colors: Vec<[f32; 3]>` has no alpha lane. Skyrim hair tip cards, eyelash strips, and several BSEffectShader meshes lose vertex-alpha modulation. Same shape on the NiTriShape path (`material.rs:468`).
- **Fix**: promote `ImportedMesh::colors` to `Vec<[f32; 4]>`; propagate alpha through the renderer's per-vertex color buffer.

#### SK-D1-04: `parse_dynamic` overwrites positions but never widens the precision claim (LOW)
- **Location**: [crates/nif/src/blocks/tri_shape.rs:628-648](crates/nif/src/blocks/tri_shape.rs#L628).
- **Description**: rewrites `shape.vertices` from the trailing Vector4 array but leaves `shape.vertex_desc` untouched. Latent today — no consumer cross-checks. Future GPU-skinning re-uploading from the packed buffer would read stale half-precision metadata.
- **Fix**: when `dynamic_count > 0`, OR `VF_FULL_PRECISION << 44` into `vertex_desc`.

#### SK-D1-05: `data_size` mismatch warns but parse continues from the wrong offset (LOW)
- **Location**: [crates/nif/src/blocks/tri_shape.rs:426-441](crates/nif/src/blocks/tri_shape.rs#L426).
- **Description**: WARN logs on data-size mismatch, then unconditionally enters the per-vertex loop using the suspect `vertex_size_quads`. Block-size realignment in the dispatcher hides the slip from the parse-rate metric, so a misparsed `vertex_desc` corrupts every vertex while stats still show 100 %.
- **Fix**: prefer `data_size`-derived stride: `(data_size - num_triangles*6) / num_vertices`, or hard-fail on mismatch and let dispatcher recover via `block_size` skip.

### Dimension 2 — BSA v105 (Skyrim SE / LZ4)

#### SK-D2-03: Per-file embed-name override bit `0x80000000` never consulted (MEDIUM)
- **Location**: [crates/bsa/src/archive.rs:188, 315-318, 432-440](crates/bsa/src/archive.rs#L188).
- **Description**: archive-level `embed_file_names` flag (bit 8 of archive flags) is XOR'd against per-file `compression_toggle` for compression, but no XOR for embed-name. The per-file `size` field's bit 31 (`0x80000000`) is masked off as part of `size & 0x3FFFFFFF` and never re-tested. Mixed-mode-name BSAs (mods that flip the flag per-file) extract with wrong path prefix consumption. Vanilla unaffected.
- **Fix**: extract bit 31 alongside bit 30 and XOR against archive-level `embed_file_names`, mirroring the compression-toggle pattern.

#### SK-D2-06: BSA v105 (LZ4 + 24-byte folder records + u64 offsets) has no unit-test coverage (MEDIUM)
- **Location**: [crates/bsa/src/archive.rs:533-722](crates/bsa/src/archive.rs#L533).
- **Description**: every `#[ignore]`'d on-disk test points at FNV (v104/zlib). End-to-end works empirically (~50 sample extracts succeed), but the audit's exact scope — v105 + LZ4 frame + 24-byte folder records + u64 offsets — has zero unit-level coverage. A regression in any v105-specific code path would only surface against on-disk archives.
- **Fix**: add a roundtrip test using a tiny synthetic v105 BSA (or a checked-in fixture) covering LZ4 frame decode + embed-name + compression toggle.

#### SK-D2-02 / SK-D2-04 / SK-D2-05 / SK-D2-07: BSA hardening LOWs
- **SK-D2-02**: `genhash_*` allocates `Vec<u8>` per name in debug, ~22k pointless allocations per Meshes0 open ([archive.rs:24-29, 62-66](crates/bsa/src/archive.rs#L24)).
- **SK-D2-04**: post-LZ4 decompressed length never asserted equal to prefix-declared size; truncated frames fail late ([archive.rs:464-509](crates/bsa/src/archive.rs#L464)).
- **SK-D2-05**: `total_folder_name_length` / `total_file_name_length` read into `_`-prefixed bindings; never validated against actual bytes consumed ([archive.rs:180-181](crates/bsa/src/archive.rs#L180)).
- **SK-D2-07**: `FolderRecord.hash` + `FolderRecord.offset` dead in release; persistent compiler warning. Sibling `RawFileRecord.hash` already gated on `#[cfg(debug_assertions)]` ([archive.rs:213-244](crates/bsa/src/archive.rs#L213)).

### Dimension 3 — BSLightingShaderProperty Variants

#### SK-D3-04: FO76 SkinTint material_kind=4 never reaches `triangle.frag` (HIGH)
- **Location**: [crates/nif/src/blocks/shader.rs:639](crates/nif/src/blocks/shader.rs#L639), [crates/nif/src/import/material.rs:606](crates/nif/src/import/material.rs#L606), [crates/renderer/shaders/triangle.frag:734](crates/renderer/shaders/triangle.frag#L734).
- **Description**: For `bsver == 155`, `BSShaderType155` enum has SkinTint as type **4**. The importer writes `info.material_kind = shader.shader_type as u8` raw. The fragment shader only dispatches `materialKind == 5u`. `Fo76SkinTint` payload reaches `skinTintRGBA` (render.rs:519-525) but the multiply branch is gated out.
- **Fix**: remap FO76 type 4 → 5 next to material.rs:606, or add `materialKind == 4u` sibling branch in triangle.frag.

#### SK-D3-05: MultiLayerParallax (11) and EyeEnvmap (16) packed per-draw with no shader consumer (MEDIUM)
- **Location**: [crates/renderer/shaders/triangle.frag:762-778](crates/renderer/shaders/triangle.frag#L762), [byroredux/src/render.rs:529-549](byroredux/src/render.rs#L529).
- **Description**: shader explicitly stubs both as "variant stubs" with deferred-to-followup comments. Meanwhile the renderer packs `multi_layer_inner_thickness/refraction_scale/inner_scale_*`, `eye_left_center`, `eye_right_center`, `eye_cubemap_scale` into every DrawCommand (56 bytes / instance at offsets 252-300). Default zeros keep the GPU output neutral, so the cost is dead per-draw CPU packing.
- **Fix**: file follow-up render-branch issue, or `Option`-gate the render.rs lookups behind `material_kind ∈ {11, 16}` until the shader path lands.

#### SK-D3-06: FO76 type 12 EyeEnvmap claimed no trailing payload — unverified vs nif.xml (LOW)
- **Location**: [crates/nif/src/blocks/shader.rs:1041](crates/nif/src/blocks/shader.rs#L1041).
- **Description**: catch-all returns `ShaderTypeData::None` for FO76 type 12 with comment `12 Eye Envmap … no trailing`. Legacy/FO4 parser at shader.rs:903-921 reads 28 bytes (cubemap scale + two reflection centers) for type **16** Eye Envmap. Per the No-Guessing policy, needs verification against `/mnt/data/src/reference/nifxml/nif.xml` `BSShaderType155`. If FO76 carries the same payload, we under-read 28 bytes per FO76 eye mesh and stream drifts.
- **Fix**: verify against nif.xml; if non-empty, add the trailing read.

#### SK-D3-07: Vec4 share between `multi_layer_envmap_strength` and `hair_tint_b` enum-tag-safe but unenforced (LOW)
- **Location**: [crates/renderer/src/vulkan/scene_buffer.rs:252-260](crates/renderer/src/vulkan/scene_buffer.rs#L252).
- **Description**: doc claims "the two variants never overlap" — holds because `ShaderTypeData` is a single-tag enum. No `debug_assert!` enforces it; a future bitflag refactor that lets a mesh carry both Type 6 and Type 11 would silently render one as the other.
- **Fix**: `debug_assert!(f.hair_tint_color.is_none() || f.multi_layer_envmap_strength.is_none())` in `apply_shader_type_data` (material.rs:1000).

### Dimension 4 — BSEffectShaderProperty + Specialty NiNode Subclasses

#### SK-D4-01: BSEffectShaderProperty falloff fields captured in MaterialInfo but never reach GPU (MEDIUM)
- **Location**: import side at [crates/nif/src/import/material.rs:438-441](crates/nif/src/import/material.rs#L438) (capture into `BsEffectShaderData`); no `falloff_*` / `soft_falloff_depth` reference in [byroredux/src/render.rs](byroredux/src/render.rs) or [crates/renderer/src/mesh.rs](crates/renderer/src/mesh.rs).
- **Description**: `falloff_start_angle`, `falloff_stop_angle`, `falloff_start_opacity`, `falloff_stop_opacity`, `soft_falloff_depth` populate `BsEffectShaderData` end-to-end through the importer. They are never copied into a `GpuInstance` field or routed to a shader. Effect-shader meshes (magic VFX, decals, smoke planes, water edges) render with hard alpha edges instead of view-angle falloff or soft-depth feathering.
- **Fix**: extend `GpuInstance` with the 5-field falloff struct under a feature flag; in `triangle.frag`, when `materialKind` indicates effect-shader, compute `dot(N, V)` falloff lerp + soft-depth scene-depth comparison (scene depth already available via gbuffer in composite pass).

#### SK-D4-02: BsValueNode `value` + `value_flags` discarded by importer (LOW)
- **Location**: parse at [crates/nif/src/blocks/node.rs:175-205](crates/nif/src/blocks/node.rs#L175); `as_ni_node` returns just `&n.base` at [crates/nif/src/import/walk.rs:45-46](crates/nif/src/import/walk.rs#L45).
- **Description**: `BsValueNode` carries `value: u32 + value_flags: u8` per nif.xml — used by FO3/FNV (and persisted in Skyrim NiNode chains) for tagging numeric metadata on subtree roots (LOD-distance overrides, billboard-mode hints). The unwrap at walk.rs:45 returns the embedded `NiNode` and forgets the trailing fields.
- **Fix**: surface `value` + `flags` as `ImportedNode::extras.bs_value_node` and consume in scene setup. Cross-check nif.xml `BSValueNode`.

#### SK-D4-03: BsOrderedNode parent-declared draw order ignored (LOW)
- **Location**: parse at [crates/nif/src/blocks/node.rs:128-167](crates/nif/src/blocks/node.rs#L128); `as_ni_node` discards `alpha_sort_bound` + `is_static_bound`; render sort path uses depth only.
- **Description**: `BsOrderedNode` exists specifically to declare a fixed draw order for its children (alpha-sorted UI/HUD overlays, certain Dragonborn banner meshes). Sort decision based on `Transform.translation.z` ignores the parent-supplied ordering; alpha bleed on banner stacks.
- **Fix**: tag children of a BsOrderedNode with a `RenderOrderHint(u16)` component derived from sibling index; back-to-front sorter checks that component first.

### Dimension 5 — Real-Data Validation

#### SK-D5-02: `parse_nif` root_index picks wrong block when scene root is a NiNode subclass (HIGH, NEW)
- **Location**: [crates/nif/src/lib.rs:454-457](crates/nif/src/lib.rs#L454).
- **Description**: Root selector uses `b.block_type_name() == "NiNode"` literal match. Scenes rooted at `BSTreeNode` / `BsValueNode` / `BsMultiBoundNode` / `BsOrderedNode` / `BsRangeNode` / `NiBillboardNode` / `NiSwitchNode` / `NiLODNode` / `NiSortAdjustNode` (every Bethesda NiNode subclass with its own Rust type) skip past the real root and pick a plain-NiNode child — typically a leaf bone container. Empirical: `treepineforest01.nif` → 0 meshes (BSTreeNode root with 4 geometry shapes unreachable). `BSFadeNode` and `BSLeafAnimNode` survive only because `blocks/mod.rs:134-140` aliases them at parse to `NiNode`.
- **Fix**: widen the predicate to call the existing `as_ni_node()` helper from `import::walk`, or (simpler in-place) extend the match arm to cover every subclass type-name.

#### SK-D5-01 (DROPPED — duplicate of #559)
Skinned BsTriShape import 0 meshes. Same root cause as the open #559 SK-D5-02. Verified empirical reproduction (`malebody_1.nif` → 0 meshes; `dragon.nif` → 0 meshes). Not refiled.

#### SK-D5-03: `BSBoneLODExtraData` has no parser entry — every actor skeleton truncates (MEDIUM)
- **Location**: dispatch table absent — `BSBoneLODExtraData` falls to `NiUnknown`. 52 actor skeletons (vampire, draugr, dragon, hare, all DLC02 races) hit the recovery path. Logged in [/tmp/audit/skyrim/meshes0.log](/tmp/audit/skyrim/meshes0.log).
- **Description**: every Skyrim SE `skeleton.nif` carries a `BSBoneLODExtraData` block. Block-size table snaps the stream forward, but the data — bone-LOD distance thresholds for skeleton mesh swapping — is lost. Drives the 100 % → 99.7 % parse-rate regression.
- **Fix**: add parser per nif.xml `BSBoneLODExtraData` (`u32 num_bone_lods; struct { u32 distance; Ref<NiNode> bone; }[num_bone_lods]`); wire dispatch in `blocks/extra_data.rs` mod.

#### SK-D5-04: Stream-alignment drift on 7 Skyrim parsers (MEDIUM, bundle)
- **Location**: log evidence in [/tmp/audit/skyrim/meshes0.err](/tmp/audit/skyrim/meshes0.err) + `meshes1.err`.
- **Description**: 7 parsers misalign vs declared `block_size`:
  - `BSLODTriShape` over-consumes 23 bytes
  - `NiStringsExtraData` under-consumes 26 bytes (entire strings array body unread)
  - `BSLagBoneController` under-consumes 12 bytes
  - `BSWaterShaderProperty` over-consumes 14 bytes
  - `BSSkyShaderProperty` over-consumes 10 bytes
  - `bhkBreakableConstraint` under-consumes 41 bytes
  - `BSProceduralLightningController` under-consumes 69 bytes

  Block-size adjustment hides the slip from the parse-rate metric, but per-block fields are wrong (over-consumes leak the next block's prefix; under-consumes drop tail fields). Highest-yield fix is `NiStringsExtraData` (>30 occurrences, used for SpeedTree LOD bone names + anim-event trigger lists).
- **Fix**: structural review per parser against nif.xml.

#### SK-D5-05: `magic\absorbspelleffect.nif` imports 1 of 2 BsTriShape (LOW)
- Likely manifestation of the #559 skin-partition gap. Filed for verification once #559 closes.

### Dimension 6 — ESM Readiness

**Premise correction**: the prompt's framing — *"cell loading is blocked on the TES5 ESM parser (stub)"* — is **stale**. `legacy/tes5.rs` was deleted in #390. Skyrim ESM parses today through the unified `crates/plugin/src/esm/` walker; ROADMAP confirms `Skyrim SE WhiterunBanneredMare 1932 entities @ 253.3 FPS / 3.95 ms` (commit 6a6950a, 2026-04-24). The CLAUDE.md text referencing the stub is outdated.

Only three additional CELL-meta gaps surface beyond the prior audit's open items (#561 multi-master, #566 LGTM):

#### SK-D6-NEW-01: `is_localized_plugin` thread-local leaks across overlapping ESM parses (LOW)
- **Location**: [crates/plugin/src/esm/records/common.rs:26](crates/plugin/src/esm/records/common.rs#L26), set/clear pair at [records/mod.rs:251 / 477](crates/plugin/src/esm/records/mod.rs#L251).
- **Description**: panic-during-parse leaves the thread-local in an undefined state; next ESM parse on the same thread reads FULL/DESC of a non-localized plugin through the lstring branch, returning `<lstring 0x…>` placeholders.
- **Fix**: replace the thread-local with a value passed down through `parse_esm` → `parse_*` closures, or wrap set/clear in a `Drop`-guard struct.

#### SK-D6-NEW-02: Cell-walker never consumes `b"FULL"` (LOW)
- **Location**: [crates/plugin/src/esm/cell.rs](crates/plugin/src/esm/cell.rs); `b"FULL"` not matched. `CellData` lacks a `display_name` field.
- **Description**: Skyrim cells ship FULL (e.g. WhiterunBanneredMare's FULL = "The Bannered Mare"). Doesn't block render; a future "Show: 'Bannered Mare'" UI command sees only `editor_id`.
- **Fix**: add `display_name: Option<String>` to `CellData`; consume `b"FULL"` via `read_lstring_or_zstring`.

#### SK-D6-NEW-03: IMGS / IMSP / IMAD imagespace records dropped at the catch-all skip (LOW)
- **Location**: [crates/plugin/src/esm/records/mod.rs:435](crates/plugin/src/esm/records/mod.rs#L435).
- **Description**: CELL.XCIM stores an imagespace FormID (per-cell tone-map / colour-grading LUT). Skyrim ships ~1k IMGS entries; almost every Solitude / Whiterun interior overrides the worldspace default. Without an IMGS index a future per-cell HDR-LUT consumer cannot resolve XCIM.
- **Fix**: add `b"IMGS" => extract_records(...)` arm following the LGTM (mod.rs:384) shape; parser stub mirroring `parse_lgtm` (EDID + scalar floats: brightness, saturation, tint, fade times). IMAD modifier graph deferred.

---

## Verified Working — No Gaps

- **NIF parser dispatch**: 80+ block types resolved cleanly across both Meshes archives.
- **Half-float decode** (tri_shape.rs:697-723): subnormals, Inf/NaN, sign all correct.
- **`byte_to_normal` mapping**: `(b/127.5) - 1.0` ≡ nif.xml's `2b/255 - 1`.
- **VF_FULL_PRECISION dispatch**: forces full-precision on BSVER < 130 (Skyrim LE/SE), bit-gated for FO4+.
- **BSA v105 LZ4 frame**: end-to-end against 9 sampled archives (~50 file extracts), magic bytes verified.
- **BSA compression-toggle XOR**: archive.rs:443 idiom is correct.
- **Folder/file hash**: regression-tested against vanilla FNV stored hash (#449).
- **BSLightingShaderProperty 320-byte GpuInstance layout**: offset asserts at scene_buffer.rs:1411-1463 cover every variant field.
- **Variant ladder dispatch** (#562): exhaustive on the 9-variant enum at material.rs:1062-1106.
- **BSVER shader dispatch**: Skyrim LE/SE (83/100) → legacy, FO4 (130-139) → FO4 parser, FO76 (155) → dedicated.
- **BSXFlags + BSBound extra-data**: round-trip via root-node extraction at [import/mod.rs:497-522](crates/nif/src/import/mod.rs#L497).
- **TES5 ESM end-to-end**: `EsmReader::detect()` → 24-byte headers → HEDR 1.71 → `GameKind::Skyrim` → compressed-record zlib → CELL/REFR walk → render at 253 FPS.
- **Compressed-record zlib**: `flate2::ZlibDecoder` at [reader.rs:401-417](crates/plugin/src/esm/reader.rs#L401).

---

## Priority Fix Order

1. **#559** (HIGH, existing) — `NiSkinPartition` global vertex buffer; unblocks every NPC + creature.
2. **SK-D5-02** (HIGH, NEW) — `parse_nif` root selector; one-line widen via `as_ni_node` helper, restores tree LODs.
3. **SK-D3-04** (HIGH, NEW) — FO76 SkinTint variant gate; two-line remap.
4. **SK-D1-01** (HIGH, NEW) — bone-index range; medium-scope refactor (partition-aware index).
5. **SK-D5-03** (MEDIUM) — `BSBoneLODExtraData` parser; 52 actor skeletons clean up.
6. **SK-D5-04** (MEDIUM bundle) — stream-drift bundle; restores 100 % parse rate. Start with `NiStringsExtraData`.
7. **SK-D4-01** (MEDIUM) — BSEffectShader falloff to GPU; soft edges on magic VFX.
8. **SK-D2-03** (MEDIUM) — embed-name flag XOR.
9. **SK-D2-06** (MEDIUM) — v105 unit-test fixture.
10. **SK-D3-05** (MEDIUM) — option-gate MultiLayer/Eye CPU packing until shader branches land.
11. **SK-D1-02** (MEDIUM) — vertex alpha lane.
12. Remaining LOWs (SK-D1-04/05, SK-D2-02/04/05/07, SK-D3-06/07, SK-D4-02/03, SK-D5-05, SK-D6-NEW-01/02/03).

---

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_SKYRIM_2026-04-24.md
```
