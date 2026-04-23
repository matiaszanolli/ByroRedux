# AUDIT — Skyrim Special Edition Compatibility

**Date:** 2026-04-22
**Game:** The Elder Scrolls V: Skyrim Special Edition (BSVER 83 / 100, NIF 20.2.0.7, BSA v105)
**Scope:** 6-dimension audit (BSTriShape vertex, BSA v105 LZ4, BSLightingShaderProperty variants, BSEffectShader + specialty nodes, real-data validation, ESM readiness)
**Reference data:** `/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/`
**Methodology:** see `.claude/commands/_audit-common.md`

## Executive Summary

**Headline:** parse-side claims hold (22,047/22,047 NIFs parse, BSA v105 LZ4
extraction works, sweetroll demo runs at 3000–5000 FPS — well above the
roadmap's stale 1615 FPS figure). The **headline gap is import-side**: every
Skyrim skinned actor body NIF (dragons, draugr, all humanoid bodies) imports
**zero meshes** because the SSE NiSkinPartition global vertex buffer is
silently discarded at `crates/nif/src/blocks/skin.rs:190-197`. This single
defect almost entirely explains why Skyrim cell rendering has not advanced to
FNV/FO3 tier despite the parse-rate gate showing 100%.

**ESM readiness was reframed.** The audit prompt's premise that `tes5.rs` is
a "stub" is stale: the module was deleted in #390 and Skyrim.esm now parses
through the unified `crates/plugin/src/esm/` walker with `GameKind::Skyrim`
fan-out, including 92-byte XCLL, compressed records (zlib via `flate2`), and
all major record types. A `#[ignore]`-gated integration test
(`cell.rs:2955`) already parses Skyrim.esm and asserts SolitudeWinkingSkeever
populates correctly. The single remaining ESM-side blocker is **CLI
multi-master wiring** — `parse_esm_with_load_order` exists (#445) but the
binary still calls the no-remap variant, so DLC-only loads (Dawnguard,
Dragonborn, Update.esm) come back empty.

**Three real bugs hide behind the green parse gate:**

1. `bhkRigidBody` parser is off by ~32 bytes on the Skyrim path; **14,408
   blocks** across both Meshes BSAs demote to `NiUnknown` placeholders. The
   block-level recovery path (`crates/nif/src/lib.rs:302`) absorbs the
   warning so `nif_stats` counts these as clean — collision data is silently
   missing for every Skyrim mesh that uses bhkRigidBody.
2. `triangle.frag` only dispatches on the engine-synthesized
   `MATERIAL_KIND_GLASS=100`. All 21 NIF `BSLightingShaderType` values
   (0–20) are forwarded CPU→GPU but the fragment shader never branches on
   them. MultiLayerParallax, ParallaxOcc, EyeEnvmap, SkinTint, HairTint,
   GlowShader all fall through to the generic PBR path.
3. Every `BsTriShape` variant (`Plain`, `LOD`, `MeshLOD`, `SubIndex`,
   `Dynamic`) collapses into the same struct with hard-coded
   `block_type_name() = "BSTriShape"`. The importer cannot tell them apart;
   `BSSubIndexTriShape` segmentation (dismemberment) and
   `BSPackedCombinedGeomDataExtra` LOD batches are silently dropped on the
   floor.

## Premise corrections (audit prompt was stale on two items)

| Prompt claim | Reality (verified) |
|--------------|--------------------|
| "BSA v105 uses `lz4_flex::block::decompress`" | Bethesda v105 actually ships **LZ4 frame format** (magic `04 22 4D 18`); current `lz4_flex::frame::FrameDecoder` is correct. |
| "ESM parser: Stub — Skyrim.esm not yet parsed" | `tes5.rs` was removed in #390. Skyrim.esm parses today via unified walker; integration test exists. |

## Severity tally

| Sev | Count |
|-----|-------|
| HIGH | 5 |
| MEDIUM | 6 |
| LOW | 8 |
| NONE (PASS / baseline) | 4 |

---

## Dim 1 — BSTriShape Vertex Format

Parser is structurally solid: VF_* attribute bits 0/1/3/4/5/6/8/9/10 match
nif.xml `VertexAttribute` exactly, BSVertexDesc bits [44:55] are sliced
correctly, IEEE 754 binary16 is implemented properly (no `(h as f32) /
65535.0` placeholder), Triangle stride is correctly hardcoded `u16[3]`.

### [LOW] SK-D1-01: VF_UVS_2 (bit 2) and VF_LAND_DATA (bit 7) attributes are silently dropped
- **Location:** `crates/nif/src/blocks/tri_shape.rs:267-289` (no constants), parse loop `:412-444` (no branch).
- **Evidence:** nif.xml `VertexAttribute` (line 2080, 2086) defines bit 2 = `UVs_2` and bit 7 = `Land_Data`. The parser's flag set jumps from `VF_UVS = 0x002` straight to `VF_NORMALS = 0x008`, and from `VF_SKINNED = 0x040` to `VF_EYE_DATA = 0x100`. When set, the bytes are absorbed by the trailing `vertex_size_bytes - consumed` pad at `:464-465` so parsing doesn't desync, but the data is dropped. Source comment at `:283-285` references issue #358.
- **Fix:** Define `VF_UVS_2 = 0x004` and `VF_LAND_DATA = 0x080`. Add `uvs2: Vec<[f32; 2]>` to `BsTriShape` and a `read_u16/half_to_f32` pair gated on `VF_UVS_2`. Land_Data extraction can wait for the terrain importer.

### [LOW] SK-D1-02: BSDynamicTriShape with data_size==0 yields a renderable shape with zero triangles
- **Location:** `crates/nif/src/blocks/tri_shape.rs:484` (triangles loop gated on `data_size > 0`); `crates/nif/src/import/mesh.rs:248-250` (early return on `triangles.is_empty()`).
- **Evidence:** When `data_size == 0`, `parse()` skips both vertex and triangle reads; `parse_dynamic` overwrites `vertices` from the trailing Vector4 array but leaves `triangles` empty. Test `bs_dynamic_tri_shape_with_zero_data_size_imports_dynamic_vertices` (`:1108-1144`) confirms 2 dynamic verts populate but never asserts `triangles.len() > 0`. Dormant on shipped facegen heads (data_size always non-zero).
- **Fix:** Document the precondition in `parse_dynamic`'s doc comment; optionally `log::warn!` when data_size==0 with non-empty dynamic verts.

### [NONE] SK-D1-03 / 04 / 05: half-float decode, FULL_PRECISION dispatch, BSVertexDesc bit-slice — PASS

`half_to_f32` (`tri_shape.rs:606-632`) handles sign/exp/mantissa correctly
including subnormals and inf/NaN; tested at 0x3C00=1.0 / 0x3800=0.5 /
0x3400=0.25. SSE/FO4 split (`:395-409`) correctly uses `BSVertexDataSSE`
(always Vector3 positions) for BSVER<130 vs `BSVertexData` for FO4+.
BSVertexDesc bit-slice `(vertex_desc >> 44) & 0xFFF` matches nif.xml
`<member width="12" pos="44">`.

---

## Dim 2 — BSA v105 (LZ4)

End-to-end verified: `meshes\clutter\ingredients\sweetroll01.nif` extracted
from vanilla `Skyrim - Meshes0.bsa` (10,245 bytes, valid Gamebryo 20.2.0.7
header). v105 dispatch (`archive.rs:157-166`), 24-byte folder records
(`:204`), u64 offsets (`:218-223`), embedded-name handling (`:417-425`),
compressed-file XOR (`:428`) all correct.

### [LOW] SK-D2-01: No committed full-archive extraction sweep for Skyrim SE BSAs
- **Location:** `crates/bsa/src/archive.rs:501-842` — test module has FNV-only fixtures.
- **Evidence:** Only `skip_if_missing`-style fixture constant is `FNV_MESHES_BSA`; no equivalent for any Skyrim SE archive. The only v105 exercise is the ad-hoc `bsa_extract_one.rs` example.
- **Fix:** Add `#[ignore]`d sister tests for `Skyrim - Meshes0.bsa` (file_count + sweetroll roundtrip) and `Skyrim - Textures0.bsa` (DDS magic sanity). Follow the existing `skip_if_missing()` pattern.

### [LOW] SK-D2-02: `hash` field on `RawFileRecord` only retained under `cfg(debug_assertions)`
- **Location:** `crates/bsa/src/archive.rs:237-242, 310-314`. Documented for completeness — release-build hash-mismatch warnings won't fire on hand-crafted archives. Acceptable tradeoff.

---

## Dim 3 — BSLightingShaderProperty (8+ shader-type variants)

Parser coverage of `BSLightingShaderType` (0–20) is **complete and correct**
across Skyrim LE/SE, FO4 (BSVER 83–139), and FO76 (BSVER 155
`BSShaderType155`). Every trailing-field arm matches nif.xml.

### Variant coverage matrix (BSVER 83–139)

| #  | Variant                | Parse | MaterialInfo | triangle.frag |
|----|------------------------|-------|--------------|---------------|
| 0  | Default                | ✓     | ✓            | generic PBR   |
| 1  | EnvironmentMap         | ✓     | ✓            | ✗ (PBR)       |
| 2  | GlowShader             | ✓     | ✓            | ✗             |
| 3  | Parallax               | ✓     | ✓            | ✗             |
| 4  | FaceTint               | ✓     | ✓            | ✗ (slots 4,7 lost) |
| 5  | SkinTint               | ✓     | ✓            | ✗             |
| 6  | HairTint               | ✓     | ✓            | ✗             |
| 7  | ParallaxOcc            | ✓     | ✓            | ✗             |
| 8  | Multitexture Landscape | ✓     | ✓            | ✗             |
| 11 | MultiLayerParallax     | ✓     | ✓            | ✗ (slot 7 lost) |
| 14 | SparkleSnow            | ✓     | ✓            | ✗             |
| 16 | EyeEnvmap              | ✓     | ✓            | ✗             |
| (others 9, 10, 12, 13, 15, 17, 18, 19, 20) | ✓ | kind only | ✗ |

`MATERIAL_KIND_GLASS=100` (engine-synthesized) is the only shader-side
variant actually dispatched today.

### [HIGH] SK-D3-01: triangle.frag has zero NIF shader-type dispatch
- **Location:** `crates/renderer/shaders/triangle.frag:716-719` — only `MATERIAL_KIND_GLASS (100u)` is compared against `inst.materialKind`.
- **Evidence:** All 21 enum values 0–20 from `BSLightingShaderType` are forwarded NIF→MaterialInfo→DrawCommand→GpuInstance unchanged (`material.rs:586`: `info.material_kind = shader.shader_type as u8`) but the fragment shader never reads them. MultiLayerParallax, ParallaxOcc, EyeEnvmap, Glow, SkinTint, HairTint all render as generic PBR. Comment at `material.rs:234-236` flags this as the SK-D3-02 follow-up.
- **Fix:** Add an `else if` ladder in `triangle.frag` gated on `inst.materialKind` for the Skyrim-critical arms: 2 (Glow — emissive boost), 5/6 (SkinTint/HairTint — tint multiply), 7 (ParallaxOcc — use `parallax_max_passes` / `parallax_height_scale` already on the instance), 11 (MultiLayer — iridescent blend), 16 (EyeEnvmap — cubemap). All CPU-side data is in place; this is pure shader work. Per `feedback_shader_struct_sync.md`, lockstep across `triangle.vert/frag`, `ui.vert`, `caustic_splat.comp` if any GpuInstance fields change.

### [MEDIUM] SK-D3-02: BSShaderTextureSet slot 7 (FaceTint tint, MultiLayer inner) and FaceTint slot-4-as-detail never routed
- **Location:** `crates/nif/src/import/material.rs:547-564` — slot 3 (parallax), 4 (env cube), 5 (env mask) captured; `tex_set.textures.get(6)` and `get(7)` never read.
- **Evidence:** nif.xml `BSLightingShaderType` comments: MultiLayerParallax "Enables … Layer(TS7)"; FaceTint "Enables Detail(TS4), Tint(TS7)". `MaterialInfo` has no `inner_layer_texture` / `tint_texture` / `detail_texture` fields. **FaceTint's TS4 is the only case where slot-4 is NOT env-cube** — current code at `material.rs:556` misbinds FaceTint's detail map as an envmap.
- **Fix:** Branch slot routing on `shader.shader_type`: for FaceTint (4), read slot 4 as `detail_map` + slot 7 as `tint_map`; for MultiLayerParallax (11) + EyeEnvmap (16), keep slot 4 as env cube and add slot 7 as `inner_layer_map`. Add `Option<String>` fields to `MaterialInfo` + bindless indices to `GpuInstance`; lockstep across the 4 shader files.

### [LOW] SK-D3-03: material_kind truncated to u8 — safe for 0–20 + 100 today, fragile on extension
- **Location:** `crates/nif/src/import/material.rs:239` (`pub material_kind: u8`), line 586 (`shader.shader_type as u8`).
- **Evidence:** Parser keeps `shader_type` as `u32` (`shader.rs:414`); GPU struct is `u32` (`scene_buffer.rs:164`). The `as u8` cast in the importer silently masks values ≥256. No content hits this today.
- **Fix:** Widen `MaterialInfo::material_kind` to `u32`; remove the `as u8` cast. Two-line change, no data-layout impact (CPU-side only).

### [NONE] SK-D3-04: All trailing-field arms match nif.xml — PASS

Cross-checked every arm against nif.xml lines 1400–1423 across
`shader.rs:794-872` (Skyrim/FO4), `:878-960` (FO4 BSVER 130+), `:963-992`
(FO76 BSShaderType155). Tests at `:1507-1622` and `:2015-2090`.

---

## Dim 4 — BSEffectShaderProperty + Specialty Nodes

`BSEffectShaderProperty` parser layout matches nif.xml for SK v20.2.0.7
byte-for-byte (`shader.rs:1108-1210`). NiNode-subclass walker (`as_ni_node`)
correctly unwraps `BSFadeNode` (aliased to plain `NiNode`),
`BSMultiBoundNode`, `BSTreeNode`, `BSOrderedNode`, `BSValueNode`,
`NiBillboardNode`, `NiSortAdjustNode`, and `BsRangeNode` (which absorbs
`BSBlastNode` / `BSDamageStage` / `BSDebrisNode` via `kind` discriminator).
**No subclass falls through to `None`.**

### [HIGH] SK-D4-02: TriShape variant identity erased at parse time
- **Location:** `crates/nif/src/blocks/tri_shape.rs:189-216, 541-583`; dispatch `crates/nif/src/blocks/mod.rs:217-253`.
- **Evidence:** `parse_dynamic`, `parse_lod`, and `BSSubIndexTriShape` (line 239) all return the same `BsTriShape` struct, and `block_type_name(&self) -> "BSTriShape"` is hard-coded (line 215). `parse_lod` reads then discards the three u32 LOD sizes (`let _lod0 = …; let _lod1 = …; let _lod2 = …`); `parse_dynamic` parses morph vertices and overwrites `shape.vertices` in place. The importer cannot distinguish a facegen head, a distant LOD shell, a dismember-segmented body, or a static prop.
- **Fix:** Add `pub kind: BsTriShapeKind` to `BsTriShape` (enum `Plain | LOD | MeshLOD | SubIndex | Dynamic`) stamped by each parser arm, mirroring the `BsRangeKind` pattern at `node.rs:526`. Prerequisite for SK-D4-03 and SK-D4-04.

### [MEDIUM] SK-D4-03: BSSubIndexTriShape segmentation block silently skipped
- **Location:** `crates/nif/src/blocks/mod.rs:239-249`.
- **Evidence:** After the BSTriShape body, the dispatcher does `if consumed < size as u64 { stream.skip(size as u64 - consumed)?; }` with the comment "the segmentation structure is used only for gameplay damage subdivision — the renderer doesn't need it". For Skyrim SE this is the **dismemberment partition table** that maps every triangle to a body part (head / torso / arm / leg) — required for hit-location, decapitation, armor slot masking, and dismember-aware ragdoll.
- **Fix:** Parse the trailing segmentation per nif.xml (num primitives, segment table, optional SSF filename) into a new `segments: Option<BsSitsSegmentation>` field on the `BsTriShapeKind::SubIndex` variant from SK-D4-02.

### [MEDIUM] SK-D4-04: BSPackedCombinedGeomDataExtra body skipped, no LOD import path
- **Location:** `crates/nif/src/blocks/mod.rs:364-379`; `crates/nif/src/blocks/extra_data.rs` (parser stub).
- **Evidence:** Dispatcher parses the fixed header then skips the variable-size per-object data + vertex/triangle pools via `block_size`. Grep for the type name in `crates/nif/src/import/` returns zero hits. **Distant settlement / city silhouettes will be blank in Skyrim SE exterior cells once exterior streaming lands.**
- **Fix:** Either (a) parse the per-object array + vertex/index pools into the existing `BsPackedCombinedGeomDataExtra` struct, or (b) document as known M35 (terrain LOD) gap and skip the host `BSMultiBoundNode` subtree so the LOD NIF doesn't contribute zero-mesh nodes. SK-D4-02 is prerequisite either way.

### [LOW] SK-D4-01: BSEffectShaderProperty SK v20.2.0.7 layout matches nif.xml — baseline PASS

Logged so a future audit doesn't re-flag.

---

## Dim 5 — Real-Data Validation & Rendering

| Archive | NIFs | Clean | Truncated | Failures | Rate |
|---------|------|-------|-----------|----------|------|
| `Skyrim - Meshes0.bsa` | 18,862 | 18,862 | 0 | 0 | 100.00% |
| `Skyrim - Meshes1.bsa` | 3,185 | 3,185 | 0 | 0 | 100.00% |
| **Combined** | 22,047 | 22,047 | 0 | 0 | 100.00% |

Top block types (Meshes0): NiNode 109,145; BSTriShape 73,359;
BSLightingShaderProperty 67,105; BSShaderTextureSet 57,414; NiAlphaProperty
31,826; **NiUnknown 22,039** (12,866 are bhkRigidBody recovery placeholders);
BSDismemberSkinInstance 15,726; bhkCompressedMeshShape 7,577.

| Mesh | Imports cleanly? | Notes |
|------|------------------|-------|
| `clutter\ingredients\sweetroll01.nif` | YES — 1 mesh, 209/396, draws=1 | 3000-5000 FPS, BC1 + BC3 textures, BLAS compacted 55%. |
| `actors\dragon\character assets\dragon.nif` | **NO — 0 meshes** | BSTriShape body is metadata-only; geometry sits in NiSkinPartition global vertex buffer **discarded by parser** (S5-02). |
| `dlc01\landscape\trees\winteraspen04.nif` | YES — 1 node + 2 meshes | Uses `BSLeafAnimNode` (aliased to NiNode); no `BsTreeNode` / `BSPackedCombinedGeomDataExtra` here. |
| `actors\character\character assets\malehead.nif` | NO — 0 meshes | BSDynamicTriShape parses 898 verts; **triangles ship in sibling `.tri` morph file** (FaceGen runtime out of scope). |
| `magic\firestormhandeffects.nif` | YES — 9 nodes + 1 mesh, draws=78-94 | Effect-shader path lights up; particle emitters surface as `ImportedParticleEmitter`. |

### [HIGH] SK-D5-01: bhkRigidBody parser misaligned — 14,408 blocks demoted to NiUnknown across Skyrim BSAs
- **Location:** `crates/nif/src/blocks/collision.rs:174-310` (`BhkRigidBody::parse`).
- **Evidence:** Every Skyrim SE bhkRigidBody block is 250 bytes; parser stops at 215 bytes (35 bytes short). The next field read is `num_constraints = stream.read_u32_le()?` → reads `0xD0CDD0BE` = **3,503,082,814**, the magic in the warning spam. The Skyrim-path padding skip at line 288 (`stream.skip(4)?`) is 4 bytes; the actual layout requires 4 + 4 + 24 = 32 bytes between `quality_type` and `num_constraints` (autoRemoveLevel + responseModifierFlags + numShapeKeysInContactPoint + forceCollidedOntoPPU + the 24-byte `body_flags` extension that #127 partially addressed).
- **Impact:** **No collision data reaches renderer/physics for any Skyrim mesh that uses bhkRigidBody.** Headline 100% parse rate is misleading because the block-size recovery path (`crates/nif/src/lib.rs:302`) inserts NiUnknown placeholders that `nif_stats` counts as clean. Single sweetroll demo logs the warning even on one mesh.
- **Fix:** Re-derive the Skyrim-path layout against nif.xml `bhkRigidBody` cinfo for `BSVER >= 83`. Add a `consumed == size` invariant assertion in debug builds inside `parse()`. Route the warning into `Stats::record_truncated` so `nif_stats` exit gate flags the regression.

### [HIGH] SK-D5-02: Skyrim skinned actor bodies import 0 meshes (NiSkinPartition global vertex buffer discarded)
- **Location:** `crates/nif/src/blocks/skin.rs:190-197`.
- **Evidence:**
  ```rust
  if is_sse {
      let data_size = stream.read_u32_le()?;
      let _vertex_size = stream.read_u32_le()?;
      let _vertex_desc = stream.read_u64_le()?;
      if data_size > 0 {
          stream.skip(data_size as u64)?;   // ← silently drops the global vertex buffer
      }
  }
  ```
  `dragon.nif` BSTriShape blocks are 120 bytes each (header only, no inline vertices). Inline `extract_bs_tri_shape` returns `None` because `shape.vertices.is_empty()`. The 528 KB of vertex data is stored in the `NiSkinPartition` global block (641,172 bytes for partition[0]). Same on `parthurnax.nif`, `bossdragon.nif`, `decaydragon.nif`, `malebody_1.nif`.
- **Impact:** **All Skyrim SE NPCs and creatures are invisible** (dragons, draugr, all humanoid bodies, all DLC actors). This single defect almost certainly explains why Skyrim cell rendering has not advanced to FNV/FO3 tier despite the parse-rate gate showing 100%.
- **Fix:** Store the SSE global vertex buffer on `NiSkinPartition` (struct field + parser branch). In `extract_bs_tri_shape`, when `shape.vertices.is_empty()` and `shape.skin_ref` is set, resolve the skin ref to `NiSkinPartition` and reconstruct `vertices`/`normals`/`uvs`/`triangles` from `partitions[i].vertex_map` (partition-local → global remap) + the global buffer's packed data (decode via the `_vertex_desc` bitfield, same as the inline BSTriShape path).

### [MEDIUM] SK-D5-03: ROADMAP sweetroll FPS (1615) is stale — measured 3000-5000 FPS
- **Evidence:** Sweetroll demo on RTX 4070 Ti @ 1280x720: `fps=2804 avg=3999`, `fps=3761 avg=4494`, `fps=5040 avg=4261`, `fps=5005 avg=4001` (4 separate runs).
- **Fix:** Update ROADMAP and game-compat row to reflect ~3000-5000 FPS sweetroll baseline (date-stamped 2026-04-22).

### [MEDIUM] SK-D5-04: bhkRigidBody warning spam pollutes every cell load
- **Location:** `crates/nif/src/lib.rs:302-312`.
- **Evidence:** Single sweetroll run already logs `Block 4 'bhkRigidBody' (size 250, offset 1034, consumed 215): NIF claims 3503082814 elements …`. A full Meshes0 cell load will burst ~14,408 of these.
- **Fix:** Companion to SK-D5-01. Either fix SK-D5-01 (preferred) or downgrade the bhkRigidBody-specific recovery path to `log::debug!`.

### [LOW] SK-D5-05: FaceGen face triangles missing from per-NPC NIFs (expected limitation)
- **Evidence:** `malehead.nif` BSDynamicTriShape parses to `vertices=898, num_triangles=0`. Triangle list lives in a sibling `.tri` morph file consumed by the FaceGen runtime. Out of scope.
- **Fix:** Add `log::info!` when an imported NIF has BSDynamicTriShape with 0 triangles so the silent failure is at least audible.

### [LOW] SK-D5-06: nif_stats "clean" metric counts NiUnknown-recovered blocks as clean parses
- **Location:** `crates/nif/src/lib.rs:302-317` × `crates/nif/examples/nif_stats.rs:73-99`.
- **Evidence:** Headline rate is 100.00% clean / 0 truncated despite 12,866 bhkRigidBody fall-throughs. The truncated/dropped-blocks telemetry that #393 added is blind to per-block recovery. **This is why SK-D5-01 slipped through the gate.**
- **Fix:** Either bump `scene.truncated = true; scene.dropped_block_count += 1` in the recovery branch (so #393's metric catches it) OR introduce a `recovered_blocks` counter on `NifScene` that `record_success` checks and routes through `record_truncated`.

---

## Dim 6 — ESM Readiness & Forward Blockers

The TES5 path is **far closer to "interior cell renders" than the audit
prompt's wording suggested**. There is no `tes5.rs` parser stub — the module
was deleted (#390) — and Skyrim.esm is parsed by the unified
`crates/plugin/src/esm/` walker, the same code that drives FNV. Per-game
divergence is dispatched by `GameKind::from_header` (HEDR Version band:
Skyrim SE = 1.71). Compressed records (FLAG_COMPRESSED 0x00040000) are
decompressed with `flate2::ZlibDecoder` at every record body read. A
`#[ignore]`-gated integration test at `cell.rs:2955` already parses
Skyrim.esm and asserts `SolitudeWinkingSkeever` populates correctly.

### Current TES5 parser coverage

| Record | Status | Blocker for interior render? |
|--------|--------|------------------------------|
| TES4 / GRUP | full incl. HEDR + MAST + auto-detect | — |
| Compressed records (FLAG 0x00040000) | full (zlib via flate2) | — |
| CELL | full incl. Skyrim 92-byte XCLL, XCIM/XCWT/XCAS/XCMO/XLCN/XCLR (#356), XCLW (#397) | NO — done |
| REFR / ACHR / ACRE | full incl. XESP enable-parent gating (#349) | NO — done |
| STAT, MSTT, FURN, DOOR, ACTI, CONT, MISC, FLOR, TREE, IDLM, BNDS, ADDN, TERM, TACT, MOVS, PKIN, CREA | full | NO — done |
| LIGH | full (DATA → radius/RGB/flags) | NO — done |
| LGTM | full extraction | LOW — see SK-D6-03 |
| WRLD / LAND / LTEX | full | NO (interior path) |
| TXST | full — all 8 slots TX00..TX07 (#357) | NO — done |
| ADDN | full | NO — done |
| VMAD | presence flag only (#369), full Papyrus VM blob deferred | NO |
| WEAP / ARMO / AMMO | full with `GameKind::Skyrim` arm — BOD2, 8-byte DATA, packed DNAM | NO |
| MISC, KEYM, ALCH, INGR, BOOK, NOTE, CONT, LVLI, LVLN, LVLC, NPC_, RACE, CLAS, FACT, GLOB, GMST, WTHR, CLMT, SCPT | full | NO |
| PACK, QUST, DIAL, MESG, PERK, SPEL, MGEF | stub structs (EDID + form refs + scalars) | NO |
| WATR, NAVI, NAVM, REGN, ECZN, HDPT, EYES, HAIR | stub structs | NO |
| SCOL | full (FO4) — Skyrim doesn't ship SCOL | NO |
| **STRINGS / DLSTRINGS / ILSTRINGS** | **MISSING** | NO for cell render (EDID/MODL not localized); blocks UI-visible names |
| Multi-master FormID load order (`FormIdRemap`) | DONE in `parse_esm_with_load_order` (#445) but **CLI doesn't expose it** | YES for any DLC interior |

### [HIGH] SK-D6-01: CLI single-master only — DLC interior cells will not render even though the parser supports them
- **Location:** `byroredux/src/cell_loader.rs:348` (`load_cell` body) and `:438` (`load_exterior_cells`); CLI parser at `byroredux/src/scene.rs:60-380` exposes `--esm <single path>` with no `--master <name>` flag.
- **Evidence:**
  ```rust
  // byroredux/src/cell_loader.rs:348
  let index = esm::cell::parse_esm_cells(&esm_data)?;
  ```
  `grep parse_esm_with_load_order byroredux/src/` returns zero callers in the binary — only the parser's own tests. `parse_esm_with_load_order(data, Some(FormIdRemap{...}))` is unit-tested at `reader.rs:642-668` and `records/mod.rs:752-790`.
- **Impact:** Every Dawnguard / HearthFires / Dragonborn / Update.esm interior REFR whose `base_form_id` lives in Skyrim.esm fails the `index.statics.get(&base_form_id)` lookup. The cell renders empty. **The only ESM-side gap that visibly breaks a Skyrim interior render attempt today.**
- **Fix:** Thread a `--master <path>` repeatable arg through `scene.rs` → `cell_loader::load_cell{,_with_masters}`, parse each master into a single merged `EsmIndex` (cells + statics + records collapsed), build the `LoadOrder` from disk-order ESM list, call `parse_esm_with_load_order` with the proper `FormIdRemap` per file. Tracked as M46.0 in `ROADMAP.md:970` — promote to a hard prerequisite of M32.5.

### [MEDIUM] SK-D6-02: LGTM lighting template fallback not wired — Skyrim cells without explicit XCLL render with engine default ambient
- **Location:** `crates/plugin/src/esm/cell.rs:560-707` (XCLL); `crates/plugin/src/esm/records/mod.rs:111` (`lighting_templates: HashMap<u32, LgtmRecord>` extraction lands but is never read).
- **Evidence:** `Grep b"LTMP" crates/plugin/src/esm/cell.rs` returns zero hits. `CellData` struct has no `lighting_template_form: Option<u32>` field. LGTM extraction is implemented (`records/misc.rs::parse_lgtm`) but the cell→template link is missing.
- **Impact:** Vanilla Skyrim ships many interior cells that omit XCLL and rely on a referenced LGTM (Solitude inn cluster, Dragonsreach throne room, Markarth cells). These render with the cell-loader's default ambient — **not a blocker for "renders at all", but a quality blocker for "looks right".**
- **Fix:** (1) Parse `LTMP` (4-byte FormID) inside `parse_cell_group` near the XCIM/XCWT block. (2) Add `lighting_template_form: Option<u32>` to `CellData`. (3) In `cell_loader::load_cell`, when `cell.lighting.is_none()` and `lighting_template_form.is_some()`, look up `index.lighting_templates` and synthesise a `CellLighting` from the LGTM record. Tracked as #379 but not currently scheduled.

### [LOW] SK-D6-03: No localized strings loader — Skyrim cell EDIDs and MODLs work, but FULL/DESC/MESG names will be u32 garbage
- **Location:** `crates/plugin/src/esm/cell.rs:1312` (`read_zstring` consumes raw bytes); `crates/plugin/src/esm/reader.rs:481-516` (`read_file_header` ignores TES4 `flags` 0x80 Localized bit).
- **Evidence:** `grep -i "lstring\|STRINGS\|ilstrings\|dlstrings" crates/plugin` returns zero hits. Skyrim.esm header dump confirms TES4 flags = `0x00000081` = Master | Localized.
- **Impact:** **Does not block cell rendering** because EDID and MODL are not localized. The misread surfaces only on display-text fields (FULL, DESC, NAM1, RNAM, etc.). Classified HIGH for UI in the 2026-04-16 audit (S6-03), still LOW here.
- **Fix:** When this becomes blocking (M48 UI integration), add a `Strings` loader. Vanilla Skyrim strings live in BSA at `Strings/Skyrim_English.STRINGS` (z-string names), `.DLSTRINGS` (descriptions), `.ILSTRINGS` (item names). Plumb `pub localized: bool` from TES4 flags into `FileHeader`, route FULL/DESC through a `LocalizedString` enum, resolve at consumer time.

### [NONE] SK-D6-04: Compressed-record handling — PASS

`reader.rs:392-409` correctly inflates `FLAG_COMPRESSED` records via
`flate2::read::ZlibDecoder`, reconciles bytes at `:441-446`. No unit test
covers a real compressed Skyrim record but the logic matches the BSA v104
zlib path (100% over 18,862 NIFs).

---

## Forward Blocker Chain — "interior cell renders"

**Single-master Skyrim.esm-only interior** (e.g. SolitudeWinkingSkeever,
BleakFallsBarrowExt01, WhiterunDragonsreach01):

1. ✅ BSA v105 LZ4 mesh + texture extraction
2. ✅ `EsmReader::detect()` classifies Skyrim.esm; `GameKind::from_header(1.71) == Skyrim`
3. ✅ Compressed records inflated transparently
4. ✅ CELL group walked; REFRs collected; XCLL parsed; XESP gating applied
5. ✅ Base STAT / LIGH / DOOR / FURN / CONT records resolved
6. ⚠️ **Renderer-side blockers (this audit's headline finds):**
   - **SK-D5-02** (NiSkinPartition global vertex buffer discarded → no NPCs)
   - **SK-D3-01** (`triangle.frag` only dispatches GLASS → SkinTint/HairTint/EyeEnvmap fall to PBR)
   - **SK-D5-01** (bhkRigidBody parser off by ~32 bytes → no collision)
7. ⚠️ End-to-end smoke run never executed: `cargo run -- --esm Skyrim.esm --cell SolitudeWinkingSkeever --bsa "Skyrim - Meshes0.bsa" --textures-bsa "Skyrim - Textures3.bsa"`. Test infra exists (#[ignore] integration test) but renderer-side smoke run is unlogged.

**DLC interior** (Dawnguard / HearthFires / Dragonborn / Update.esm):

8. All of the above PLUS
9. **SK-D6-01** — CLI multi-master wiring. The blocker that prevents any DLC-only interior from rendering at all.

---

## Recommended action order

| Priority | Item | Why |
|----------|------|-----|
| P0 | SK-D5-02 (NiSkinPartition reconstruction) | Single highest-impact bug; unblocks every Skyrim NPC and creature. |
| P0 | SK-D5-06 (route NiUnknown recovery into `record_truncated`) | Without this, every future SK-D5-01-class regression slips past the gate. |
| P1 | SK-D5-01 (bhkRigidBody alignment) | Once SK-D5-06 lands, this stops being silent. Re-derive layout against nif.xml. |
| P1 | SK-D3-01 (triangle.frag material_kind dispatch) | Skin/hair/eye render-quality. |
| P1 | SK-D6-01 (CLI multi-master) | Required for any DLC interior. |
| P2 | SK-D4-02 (`BsTriShapeKind` enum) | Prerequisite for SK-D4-03 (dismemberment) and SK-D4-04 (LOD batches). |
| P2 | SK-D6-02 (LGTM fallback) | Quality-of-render for vanilla cells without XCLL. |
| P2 | SK-D3-02 (slot-7 + FaceTint detail routing) | Wrong textures bound on FaceTint materials. |
| P3 | SK-D1-01, SK-D2-01, SK-D5-03 (FPS update), SK-D5-04 (warn spam), SK-D6-03 | Long-tail polish. |

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_SKYRIM_2026-04-22.md
```
