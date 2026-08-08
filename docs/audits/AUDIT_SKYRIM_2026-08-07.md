# Skyrim SE Compatibility Audit — 2026-08-07

**Repo**: `/mnt/data/src/gamebyro-redux` · HEAD `c0f3cda3`
**Scope**: 7 dimensions per `.claude/commands/audit-skyrim/SKILL.md` — BSTriShape
packed geometry + SSE skinned reconstruction, `BSLightingShaderProperty` /
`BSEffectShaderProperty` shader-type dispatch, NPC equip + FaceGen (M41),
multi-master load order + TES5 cell-load, BSA v105 (LZ4), specialty NIF
blocks + real-data rendering, NIFAL canonical material translation.
**Prior audit**: `docs/audits/AUDIT_SKYRIM_2026-08-03.md` (4 days earlier —
that pass's headline HIGH finding, every `BSDynamicTriShape` importing to
zero meshes, is CLOSED and confirmed fixed by this pass's Dimension 1 corpus
run: 78,146 imported meshes, 21,140 `BSDynamicTriShape` blocks present and
importing non-empty geometry). This pass independently re-derived every
checklist item against current code and live data (four throwaway corpus
probes, five real-data BSA/ESM sweeps, one live Vulkan smoke run) rather than
citing the prior report.

## Executive Summary

Skyrim SE is the engine's renderer **control bench** — Whiterun BanneredMare
(the tavern interior with its 6 named, OTFT/LVLI-equipped NPCs) is the one
vanilla cell that exercises loose-mesh loading, cell/REFR streaming,
multi-master DLC load order, BSA v105 extraction, NPC equip, and FaceGen
head geometry simultaneously, and both loose-mesh and cell rendering work
end-to-end against it today. This audit is therefore **not readiness
scoping** — it is regression coverage over a bench that already passes,
plus the genuinely Skyrim-specific risk surface: packed `BSTriShape`
geometry with its SSE skinned-reconstruction side path, the 18-way numeric
`BSLightingShaderType` dispatch, the NPC equip/FaceGen chain, and
multi-master FormID remap.

**Headline result: the geometry side is functionally sound but the FaceGen
skin payload is not.** Dimension 1 found that every Skyrim SE
`BSDynamicTriShape` — i.e. **all** NPC head/eye/brow/mouth geometry —
imports its *positions* correctly (the prior audit's HIGH regression is
confirmed fixed), but the sibling **skin-weight decode** was never updated
to match: `decode_sse_skin_payload` still calls the position-dependent
`decode_sse_packed_buffer` wrapper instead of the
`_with_external_positions` variant, so it bails on every FaceGen partition
buffer (all of which clear `VF_VERTEX`). Net effect: **78% of all skinned
Skyrim SE geometry (21,139 of 26,940 shapes) — every NPC head — renders
rigid**, parented to the placement root instead of skinned to `NPC
Head`/`NPC Neck`. This is invisible to the standing M41 equip smoke test,
which asserts entity/draw/`tex.missing` counts, not skin-weight presence.

The second HIGH finding is unrelated and lands in distant-object LOD:
`object_lod.rs`/`terrain_lod.rs` assume every worldspace's `.bto`/`.btr`
quad grid is aligned to absolute multiples of the LOD level in cell
coordinates. That assumption holds only for Tamriel and Solstheim — the
other 9 of 12 vanilla worldspaces (Apocrypha, Soul Cairn, Blackreach,
Skuldafn, Deepwood Redoubt, Falmer Valley, Hunter HQ, Japhet's Folly,
Markarth) tile from their own non-zero grid origin, so **27.4% of level-4
`.bto`/`.btr` files are permanently, silently unresolvable** — including
Dragonborn's and Dawnguard's main questing spaces.

Everything else the audit checked — shader-type wire dispatch, NPC
equip/leveled-list resolution, multi-master FormID remap + `.STRINGS` +
ESL decode, BSA v105 LZ4 extraction, and the NIFAL canonical-material
boundary — holds up under live re-verification: full corpus sweeps against
`Skyrim - Meshes0/1.bsa` and all nine `Textures*.bsa` (0 parse errors across
65,637 BSA entries and 22,047 NIFs), a real `Skyrim.esm` + `Dawnguard.esm`
multi-master load (10,045 entities, 0 errors once the documented repro
command's two factual errors are corrected), and a live Vulkan smoke run
against the WhiterunBanneredMare control cell (5,183 entities, 6/6 named
NPCs equipped, 0 missing textures). The remaining findings are MEDIUM/LOW
hardening and documentation-drift items — a downstream FO76/Skyrim
`material_kind` enum leak (blast radius on FO76, not Skyrim), an unsafe
single-partition bone-remap shortcut affecting 59 vanilla shapes, a
`fresnel_power` canonical-field divergence latent until a shading consumer
lands, and several doc/test-coverage gaps.

## Total Findings: 17 (0 CRITICAL / 2 HIGH / 4 MEDIUM / 11 LOW)

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction

**Method**: wire layout cross-checked against `/mnt/data/src/reference/nifxml/nif.xml`;
`cargo test -p byroredux-nif` (1,043 passed, 0 failed); four throwaway corpus
probes over `Skyrim - Meshes0.bsa` + `Meshes1.bsa` (22,047 NIFs, 81,226
`BSTriShape` blocks, 78,146 imported meshes, ~35M vertices).

#### SK-D1-01: SSE skin payload is silently dropped for every `BSDynamicTriShape` — all 21,139 Skyrim SE FaceGen head meshes spawn rigid
- **Severity**: HIGH
- **Location**: `crates/nif/src/import/mesh/skin.rs:397-415` (`decode_sse_skin_payload`), reaching `crates/nif/src/import/mesh/sse_recon.rs:211-213` / `:232-234`
- **Status**: NEW (residual of #2318, CLOSED — #2318 fixed the *geometry* half; this is the unfixed *skin* sibling)
- **Description**: `#2318` taught `try_reconstruct_sse_geometry` to feed a `BSDynamicTriShape`'s trailing `Vector4` array in as external positions. `decode_sse_skin_payload` was never updated to match — it still calls the plain `decode_sse_packed_buffer(buffer)` wrapper (`_with_external_positions(buffer, None, None)`), which bails at `sse_recon.rs:232` whenever `VF_VERTEX` is clear and no external positions are supplied. Every vanilla Skyrim SE FaceGen partition buffer clears `VF_VERTEX` (positions live in the dynamic array), so the whole weights+indices decode is thrown away — even though the skin lanes sit further down the same vertex layout and don't depend on positions at all.
- **Evidence** (measured, `Skyrim - Meshes0.bsa` + `Meshes1.bsa`): 21,140 `BSDynamicTriShape` blocks; 21,139 carry a `skin_ref` resolving to a populated `SseSkinGlobalBuffer`. All observed partition-buffer attribute masks (`0x442`, `0x45a`, `0x462`, `0x47a`, `0x55a`) clear bit 0 (`VF_VERTEX`). Import outcome: `skin weights missing = 21139`, `skin indices missing = 21139`. Consumer `byroredux/src/scene/nif_loader.rs:639-642` filters on non-empty bone arrays, so every head vertex is built with zero bone weights → `triangle.vert`'s `wsum < 0.001` rigid fallback. Live path: `npc_spawn/resumable.rs:992-1024` (`PrebakedPhase::Facegen`) uses exactly this builder.
- **Impact**: Every Skyrim SE/AE NPC's head, eyes, brows, mouth and hair-cap geometry uploads as rigid geometry parented to the placement root instead of skinned to `NPC Head`/`NPC Neck` — heads stay in bind pose through every animation while the body deforms; the skinned-BLAS refit path sees a static blob. Same defect class as #638 (which fixed bodies), on the head. 21,139 of 26,940 skinned SSE shapes in vanilla — **78% of all skinned Skyrim SE geometry**.
- **Related**: #2318 (geometry half, CLOSED), #638 (body half, CLOSED), #2322, #341, #559
- **Suggested Fix**: Make `decode_sse_skin_payload` mirror `try_reconstruct_sse_geometry` — resolve the shape's external positions and `BsTriShapeKind::Dynamic { bitangent_x }`, call the `_with_external_positions` variant (widen visibility to `pub(super)`). Cleaner: split the position-presence guard out of the decoder so a skin-only decode never depends on positions. Pin with a regression test asserting non-empty `vertex_bone_weights` for a synthetic `VF_VERTEX`-clear/`VF_SKINNED`-set global buffer.

#### SK-D1-02: `remap_bs_tri_shape_bone_indices`' single-partition identity shortcut binds the wrong bone on 59 vanilla Skyrim SE shapes
- **Severity**: MEDIUM
- **Location**: `crates/nif/src/import/mesh/skin.rs:338-343`
- **Status**: NEW
- **Description**: The remapper short-circuits to an identity widen whenever `NiSkinPartition` has one partition, on the premise that a single partition's `bones` palette is always identity. Measurably false: 16,737 single-partition SSE skins, 14,195 with a non-identity palette (mostly benign trailing pad). Restricting to in-range, non-zero-weight vertices whose palette entry differs from the slot index: **7,740 vertices across 59 shapes** resolve to a different bone under the shortcut than under the palette lookup (e.g. `facegeom\skyrim.esm\00067667.nif`, palette `[0,1,3,4,5,6]`, local slot 2 → global bone 3, shortcut yields 2). A separate malformed-input class also surfaced (`armor\hide\m\1stpersoncuirassmedium_0.nif`, out-of-range local slot on both paths).
- **Impact**: Localised tearing/stretching on ~0.15% of single-partition skinned vertices. Currently **masked** for the FaceGen subset by SK-D1-01 (no weights reach the GPU at all) — fixing SK-D1-01 without fixing this makes the artifact newly visible on head meshes.
- **Related**: #613 (SK-D1-01 of a prior audit pass — introduced the shortcut; not the same ID as this pass's SK-D1-01)
- **Suggested Fix**: Delete the `<= 1` short-circuit and always resolve through `remap_one` (already degrades to identity when the palette is identity); or gate the fast path on `part.bones.iter().enumerate().all(|(i,&b)| b as usize == i)` instead of partition count.

#### SK-D1-03: packed-vertex parser ignores all ten `BSVertexDesc` offset nibbles; the `VF_UVS_2` "trailing skip absorbs it" rationale is asserted without evidence
- **Severity**: LOW
- **Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:205-237,951-1097`; same shape in `sse_recon.rs:276-436`
- **Status**: NEW (adjacent to #336, CLOSED)
- **Description**: `decode_bs_vertex_stream` walks a fixed field order and never consults the ten 4-bit offset nibbles `BSVertexDesc` publishes. The `VF_UVS_2` doc comment claims the reserved bytes are absorbed by "the trailing skip" — per `nif.xml:2092-2105` the `UV2 Offset` nibble sits *between* `UV1 Offset` and `Normal Offset`, implying UV2 is mid-vertex, in which case every attribute after UV1 would misalign. No sample exists to confirm either reading.
- **Evidence**: Corpus scan of 81,226 SSE `BSTriShape` blocks: `UVS_2 = 0`, `LAND_DATA = 0`, `INSTANCE = 0` occurrences — no vanilla Skyrim SE content exercises this path, and the fixed-order walk is provably correct for all 22 descriptors that do occur.
- **Impact**: Zero on vanilla Skyrim SE. On a mod-authored mesh setting bit 2, the whole post-UV1 attribute set would silently misalign with no diagnostic.
- **Related**: #336, #358, #359
- **Suggested Fix**: Soften the comment to state the assumption, not the conclusion; add a cheap post-walk offset comparison + `log::warn!` on mismatch (silent on 100% of vanilla content today, so free insurance).

---

### Dimension 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch

**Method**: every dispatch arm checked against nif.xml; empirical corpus run over `Skyrim - Meshes0/1.bsa` (22,047 NIFs / 78,146 imported meshes / 73,128 `BSLightingShaderProperty` / 8,116 `BSEffectShaderProperty`). `cargo test -p byroredux-nif shader` (171/171) and `material` (193/193).

#### SKY-D2-01: FO76 `BSShaderType155` numbering leaks into the Skyrim-numbered `material_kind` consumer — only type 4 is remapped, so FO76 hair tint is silently dropped
- **Severity**: MEDIUM
- **Location**: `crates/nif/src/import/material/shader_data.rs:103-114`; `dedicated_shader.rs:125-206,327`; `byroredux/src/render/static_meshes.rs:480-492`; `crates/renderer/shaders/triangle.frag:1133-1146`
- **Status**: NEW
- **Description**: The parser correctly keeps `parse_shader_type_data` (Skyrim/FO4) and `parse_shader_type_data_fo76` separate, but the importer then writes the raw `BSShaderType155` integer straight into `MaterialInfo.material_kind`, which downstream code consumes as if it were a `BSLightingShaderType`. `apply_shader_type_data` patches exactly one divergent value (type 4 → `material_kind = 5`), leaving three more mismatched: BSShaderType155 3 (Face Tint) vs BSLightingShaderType 3 (Parallax); 5 (Hair Tint) vs 5 (Skin Tint, **unremapped**); 12 (Eye Envmap) vs 12 (Tree Anim); 17 (Terrain) vs 17 (Cloud). The demonstrable loss is type 5: `parse_shader_type_data_fo76` correctly produces `ShaderTypeData::HairTint` and captures `hair_tint_color`, but `material_kind` stays `5`, so the render-data packer's `material_kind == 6` gate never fires — the authored FO76 hair tint is discarded and the mesh renders untinted. The same un-gated `match shader.shader_type` also misapplies Skyrim texture-slot semantics to FO76-numbered types.
- **Evidence**: `shader_data.rs:112-114` (single-value remap); `dedicated_shader.rs:327`; existing test `fo76_skin_tint_remaps_material_kind_to_skyrim_constant` pins only the type-4 case, with no HairTint sibling.
- **Impact**: FO76 hair meshes lose authored tint uniformly; FO76 FaceTint/EyeEnvmap/Terrain land on the wrong `material_kind` branch or none. **Skyrim SE itself is unaffected** — vanilla Skyrim never produces a `BSShaderType155` value (0 of 81,244 corpus blocks). This is a Skyrim-checklist item ("guard the two enums don't cross-contaminate") whose blast radius lands entirely on FO76.
- **Related**: #612 (established the incomplete type-4 remap); #2296 (`material_kind` literals not cross-crate pinned)
- **Suggested Fix**: Translate `BSShaderType155` → canonical `BSLightingShaderType` once at the import boundary, keyed on `scene.bsver >= FO76` (a small `canonical_material_kind(bsver, shader_type)` covering {3→4, 4→5, 5→6, 12→16, 17→17-or-None}), and gate the texture-slot `match` on the same canonical value.

#### SKY-D2-02: `shader_flags.rs` module doc asserts Skyrim has an `Alpha_Test` SLSF1 bit — nif.xml has none, and the file contradicts itself 37 lines later
- **Severity**: LOW
- **Location**: `crates/nif/src/shader_flags.rs:203` (vs `:240-241`)
- **Status**: NEW
- **Description**: The `fo4_slsf2` module doc's parenthetical ("Skyrim has Alpha_Test on SLSF1!") is unsupported by nif.xml — no `Alpha_Test` option exists anywhere in `SkyrimShaderPropertyFlags1`/`2` (bit 25 is `Remappable_Textures`). Skyrim routes alpha-test exclusively via `NiAlphaProperty`, which the same file's own doc states correctly 37 lines below.
- **Evidence**: nif.xml:6396 `<option bit="25" name="Remappable_Textures">`.
- **Impact**: No runtime effect (no code reads Skyrim SLSF1 bit 25), but this file's stated purpose is documenting per-game bit semantics for future contributors — exactly the error class behind #414/#1879.
- **Related**: #414, #1879
- **Suggested Fix**: Fix the parenthetical to match `fo4_slsf2::ALPHA_TEST`'s own correct doc.

#### SKY-D2-03: No wire-level Skyrim parse test for shader types 6 / 7 / 14 — HairTint is the second-most-common non-default type in vanilla Skyrim (10,817 instances) with zero byte-layout coverage
- **Severity**: LOW
- **Location**: `crates/nif/src/blocks/shader_tests/skyrim.rs:6-119`
- **Status**: NEW
- **Description**: Skyrim-era wire-parse tests cover shader types 0, 1, 5, 11, 16 only. Types 6 (HairTint, 10,817 vanilla instances — more than SkinTint/EyeEnvmap/MultiLayerParallax combined), 7 (ParallaxOcc, 0 vanilla but mod-reachable) and 14 (SparkleSnow, 19 instances) have no Skyrim wire-level test; the corresponding `apply_shader_type_data` tests construct the enum directly and never exercise the byte reader.
- **Impact**: Test-coverage gap only — code is currently correct (verified against nif.xml and the zero-drift corpus run). A future field-count regression in arm 6/7/14 would ship silently.
- **Related**: SKY-D2-01 (shares the same under-tested HairTint surface)
- **Suggested Fix**: Add three `build_bs_lighting_common(N)` + trailing-bytes tests mirroring the existing `skin_tint` one, keeping the over-read-detecting `stream.position() == data.len()` assertion.

#### SKY-D2-04: `BSEffectShaderProperty.env_map_min_lod` is parsed and captured but has no consumer past `MaterialInfo` — an undocumented dead-end field
- **Severity**: LOW
- **Location**: `crates/nif/src/blocks/shader.rs:1560,1736`; `import/material/shader_data.rs:37`; `import/material/mod.rs:894`
- **Status**: NEW
- **Description**: Unlike its packed-field siblings (`texture_clamp_mode` reaches sampler selection, `lighting_influence` reaches `material_flags`), `env_map_min_lod` stops at `BsEffectShaderData` — no packer, no `Material` field, no GLSL uniform — and carries no "parked for a future consumer" comment.
- **Evidence**: All 8,116 vanilla Skyrim `BSEffectShaderProperty` blocks author `env_map_min_lod = 0`, so nothing is lost today.
- **Impact**: None on vanilla Skyrim. On FO4+ effect materials that clamp the env-map mip chain, the authored floor is silently ignored.
- **Related**: #345/S4-01
- **Suggested Fix**: Either document it as explicitly parked, or plumb it into `GpuMaterial` alongside `soft_falloff_depth`.

---

### Dimension 3 — NPC Equip + FaceGen (M41)

**Method**: static read of the equip resolver, WNAM race-skin path, and NIF-side skin extraction; `cargo test -p byroredux npc_spawn` (28/28), `byroredux-facegen` (24/24 + 3 gated `#[ignore]`), `byroredux-nif --lib dismember` (3/3); **live smoke run** of `docs/smoke-tests/m41-equip.sh skyrim` against real Skyrim SE data.

**No findings.** All 5 checklist items PASS, live-verified against real data: WhiterunBanneredMare loaded 5,183 entities (≥1,200 floor), 1,500 draws (≥700 floor), Inventory=46 (≥6 floor), EquipmentSlots=6 (≥6 floor), `tex.missing=0`, and `byro-dbg entities EquipmentSlots` enumerated exactly the 6 named residents (saadia, brenuin, mikael, sinmir, amaundmotierreend, hulda). The #2093/#2094 fix chain (race-skin lowest-priority layer + post-loop occupancy filter) is intact. One pre-existing, already-documented deferral noted but not filed: `ImportedSkin::body_part_flags` has no downstream consumer outside its own tests (out of this dimension's game scope — the name-match workaround it stands in for is kf-era-only and doesn't touch any of the 6 named NPCs' meshes).

---

### Dimension 4 — Multi-Master Load Order + TES5 Cell-Load Regression

**Method**: read of the FormID remap pipeline, deleted-REFR handling, `.STRINGS` wiring; `cargo test -p byroredux-plugin esm::` (565/565) + `-- --ignored` real-data integration (13/13, incl. `parse_real_skyrim_esm` — 590 cells, 18,113 statics, 37 worldspaces); a live end-to-end `--master`/`--esm` run against real `Skyrim.esm` + `Update.esm` + `Dawnguard.esm`.

#### Finding 1 — CLAUDE.md's documented `--master` repro command is wrong (fails verbatim)
- **Severity**: LOW (documentation defect — the engine's own error handling did its job correctly)
- **Location**: `CLAUDE.md` Usage section; this audit skill's own Dimension-4 brief cites the identical broken repro
- **Status**: NEW
- **Description**: `cargo run -- --master Skyrim.esm --esm Dawnguard.esm --cell ForebearsHoldoutInt01` fails outright against real data: `Dawnguard.esm`'s actual `MAST` list is `["Skyrim.esm", "Update.esm"]` (the doc omits the second master), and `ForebearsHoldoutInt01` is not a real cell EditorID (the real interior is `Forelhost01`). With both corrections (`--master Skyrim.esm --master Update.esm --esm Dawnguard.esm --cell Forelhost01 --bsa …`), the cell loads cleanly: 10,045 entities, 928 meshes, 343 textures, 78.5 FPS, zero errors — proving the underlying M46.0 repeatable-`--master` FormID remap works correctly. The failure mode is soft: the engine logs one clear `ERROR` line naming the missing master but then falls back to the default 6-entity demo scene and keeps running rather than exiting non-zero, so a `--bench-hold` run not watching stderr closely could believe the repro "worked."
- **Impact**: Anyone verifying multi-master support via the documented command gets a false failure signal.
- **Suggested Fix**: Update the `--master` line in `CLAUDE.md`'s Usage section to `--master Skyrim.esm --master Update.esm --esm Dawnguard.esm --cell Forelhost01 --bsa …`. No code change needed — engine behavior is correct per #561's design intent.

**Verified correct, no findings**: FormID remap (`FormIdRemap`/`GlobalSlot`, 11 targeted tests + live end-to-end verification), `.STRINGS` load-order wiring (`db5bb149`, per-plugin RAII scoping, 2 regression tests), ESL/light-master FormID decode (#1554, 5 targeted tests incl. end-to-end), deleted-REFR tombstone skip (0x20, #1660, doc comment correctly current since #1781), `parse_real_skyrim_esm` (clean real-data walk), TES5 compressed-record decompression, minimum interior-render record set (CELL/REFR/STAT/LIGH/WEAP/ARMO/LAND/LTEX/TXST/ADDN all present and populated), out-of-scope record types (NAVM/HDPT/`BSBehaviorGraphExtraData`) all parse without error.

**Already-tracked, not re-filed**: the control-bench guard is currently violated — Whiterun BanneredMare's entity count grew 3,406→5,150 (+51%) between the R6a-stale-17 and R6a-stale-18 bench refreshes for reasons not yet root-caused, tracked as open issue **#2367**. Nothing in this dimension's own code path (multi-master load order / TES5 cell walk) plausibly explains the growth; ROADMAP attributes the surrounding regression window to Session 60–62 renderer feature work, not plugin/ESM changes.

---

### Dimension 5 — BSA v105 (LZ4)

**Method**: read of header/directory/codec dispatch against the UESP BSA spec and prior `#569`/`#617`/`#622`/`#1558` fix history; brute-force sweep of all 11 vanilla Skyrim SE BSAs (65,637 files) in both release and debug (hash-validating) builds.

#### SK-D5-LZ4-LOW-01: `open_with_numeric_siblings` has no de-dup guard against explicitly re-listing an auto-loaded sibling
- **Severity**: LOW
- **Location**: `byroredux/src/asset_provider/archive.rs:306-331`, called from `texture.rs:151-192`
- **Status**: NEW
- **Description**: `build_texture_provider` opens each `--bsa`/`--textures-bsa` occurrence independently with no tracking of already-opened paths. A user who still explicitly lists every Skyrim archive (e.g. both `Meshes0.bsa` and `Meshes1.bsa`) gets `Meshes1.bsa` opened twice — once explicitly, once as the auto-loaded sibling of `Meshes0.bsa`. Caps at one duplicate per redundantly-listed archive (mid-series digits don't re-expand).
- **Impact**: Wasted memory (duplicated directory `HashMap` + file handle) and non-deterministic archive lookup order between the two copies. Not a correctness bug — both copies are identical content. No evidence this fires in shipped smoke-test scripts or README examples.
- **Suggested Fix**: Track already-opened canonical paths in a `HashSet<String>` inside `build_texture_provider`, checked before both the primary open and each sibling open.

#### SK-D5-LZ4-LOW-02: Post-decompression size-mismatch check is `warn`, not surfaced to any caller-visible metric
- **Severity**: LOW
- **Location**: `crates/bsa/src/archive/extract.rs:154-164`
- **Status**: NEW (observation — not exercised on real data; the full sweep produced zero such warnings across all 65,637 files)
- **Description**: A declared/actual size mismatch after LZ4 frame decode logs `log::warn!` but returns `Ok` regardless — a deliberate, documented design choice (mirrors the BA2 zlib path). Recorded only because no `nif_stats`-style counter exists for the BSA layer either, so a future audit doesn't have to re-derive that this is intentional.
- **Impact**: None currently observed. Would only matter on a malformed/modded archive, surfacing downstream as a confusing NIF/DDS parse error rather than a clear BSA-layer diagnostic.
- **Suggested Fix**: None required now; pipe into a future parse-rate-style gate if one is added for the BSA extraction layer.

**Verified correct, no findings**: v105 header/directory layout (24-byte folder records, `embed_file_names` v104+ semantics) — 0 hash/length-bookkeeping mismatches across 65,637 files in a debug run; LZ4 **frame** decompression (not `lz4_flex::block` as the checklist wording assumes — checklist is stale, implementation is correct) byte-exact against a pinned sweetroll fixture and a full 18.9 GB brute-force sweep, 0 errors; compressed-file flag semantics are a per-file **XOR toggle** against the archive default, not a priority order (checklist wording implies an override model that doesn't exist); zero-based sibling auto-load (`821a425b`) intact and load-bearing — confirmed live that `Meshes1.bsa`'s 9,584 `.btr`/1,078 `.bto` and `Textures7.bsa`'s object-LOD atlases both resolve through it.

---

### Dimension 6 — Specialty Blocks + Real-Data Rendering

**Method**: dispatch-table read against nif.xml; `nif_stats` sweep of `Skyrim - Meshes0.bsa` (18,862 NIFs) + `Meshes1.bsa` (3,185 NIFs + 1,078 `.bto` + 9,584 `.btr`).

#### SK-D6-01: LOD quad origin assumes worldspace-independent alignment — 9 of 12 vanilla Skyrim worldspaces resolve zero `.bto`/`.btr`
- **Severity**: HIGH
- **Location**: `byroredux/src/cell_loader/object_lod.rs:385-400` (`quad_origin`/`bto_archive_path`); `terrain_lod.rs:273-277,367-380`; `terrain_lod_btr.rs:72-75`
- **Status**: NEW (adjacent to open epic #2371, which covers *missing coarse bands* — a different defect)
- **Description**: Both LOD path builders derive the quad's SW-corner cell as `cell.div_euclid(level) * level`, assuming every worldspace's LOD quad grid is aligned to absolute multiples of `level`. The vanilla Skyrim SE filename corpus disproves this: each worldspace tiles from its **own** grid origin, generally non-zero. The module's own doc comment asserts the wrong rule, citing only Tamriel filenames as evidence — the one worldspace where the assumption happens to hold.
- **Evidence**: All 10,662 `.bto`/`.btr` names in `Skyrim - Meshes1.bsa` parse against the expected path pattern (0 unmatched). Per-worldspace `(x mod level, y mod level)` at level 4 is a single non-zero constant for 10 of 12 worldspaces (tracks each worldspace's own minimum LOD cell). Reachability at level 4 (the only band either loader requests): **5,735 of 7,897 files resolvable (72.6%)** — Blackreach, Deepwood Redoubt, Falmer Valley, Soul Cairn, Hunter HQ, Apocrypha, Japhet's Folly, Skuldafn all resolve **zero**; Markarth resolves 169/194 (worst diagnostic case — looks like scattered content bugs, not a systematic defect). Across all levels: 3,074 of 10,662 files (28.8%) unreachable.
- **Impact**: Nine of twelve vanilla worldspaces — including Apocrypha (Dragonborn's main questing space, 1,063 LOD files) and the Soul Cairn (Dawnguard, 944) — get **zero distant object LOD**, permanently and silently: `spawn_object_lod_quad` misses caches an `ObjectLodBlock::empty()` sentinel with no log line and, unlike terrain, no synth fallback. Distant terrain in those worlds degrades to the flat-texture synth block (visible quality loss, not a blackout). Tamriel and Solstheim work, which is exactly why this survived the EXAL step-6 verification.
- **Related**: #2371 (distant LOD bands epic), #1866 (ring/hysteresis gating), #2086 (same "verified on one title, generalised to all" failure class)
- **Suggested Fix**: Derive the quad grid origin per worldspace instead of assuming `(0,0)` — `LODSettings\<World>.lod` and the `WrldRecord` min-cell both carry it. Replace `quad_origin(gx, gy, level)` with an origin-relative version and thread the same origin into `terrain_lod.rs`'s block index. Add a regression test using real non-Tamriel filenames (`dlc2apocryphaworld.4.-50.-50.btr`, `dlc01soulcairn.4.-52.-51.btr`).

#### SK-D6-02: `.bto`/`.btr` distant-LOD NIFs are outside every corpus regression gate — `nif_stats` filters on `.nif` only
- **Severity**: MEDIUM
- **Location**: `crates/nif/examples/nif_stats.rs:577,605`
- **Status**: NEW
- **Description**: The tool backing the Meshes0/Meshes1 clean-parse baselines and the per-block/block-coverage baseline tests only considers `.nif`-suffixed archive entries. `.bto`/`.btr` are renamed NIFs through the identical `parse_nif` → `import_nif_scene` pipeline and are the entire substrate of the M35/EXAL-step-6 distant-LOD milestones — 10,662 files in `Meshes1.bsa` alone (3.3× the `.nif` count in that archive), contributing 0 to any baseline.
- **Evidence**: Hand-parsed this run: 10,662/10,662 clean, 0 zero-mesh — no live regression today, but nothing keeps it that way.
- **Impact**: A parser change breaking Skyrim distant-LOD geometry would pass the full corpus gate silently. Given SK-D6-01 already hides the *runtime consumption* of these files in 9/12 worldspaces, a parse regression on top would be invisible twice over.
- **Related**: SK-D6-01; NIF corpus baseline tests
- **Suggested Fix**: Widen the archive-entry filter to `.nif`/`.bto`/`.btr`, re-baseline.

#### SK-D6-03: `BSTreeNode` wind-bone lists are imported but have no consumer outside the NIF crate
- **Severity**: LOW
- **Location**: `crates/nif/src/import/walk/mod.rs:1589-1600`; `import/types.rs:161`
- **Status**: NEW (informational — forward scope, same class as the VWD note this dimension was told not to re-file)
- **Description**: `BSTreeNode`'s two trailing `NiNode` ref lists (SpeedTree wind rig) are parsed correctly and surfaced onto `ImportedNode.tree_bones` by both walkers, but nothing outside `crates/nif`/`crates/spt` reads the field.
- **Impact**: None today (Skyrim trees render static). Recorded so the parse-vs-consume gap is on record rather than rediscovered as "the parser drops it."
- **Suggested Fix**: None required now — ready hook for when SpeedTree wind lands.

**Verified correct, no findings**: Meshes0/Meshes1 sweep baseline holds (100% clean, 0 truncated, 0 recovered, 0 realignment WARNs — the #837/#838 regression guards are intact); `BSLODTriShape`/`BSMeshLODTriShape`/`BSSubIndexTriShape` route through three genuinely distinct bodies with no confusion; `BsLagBoneController`/`BsProceduralLightningController` field-for-field match nif.xml; node unwrapping (`BSFadeNode`/`BSBlastNode`/`BSMultiBoundNode`) has a single correct unwrap point; `.bto`/`.btr` parse + import yield 100% on real data with resolved `base_color`; `ObjectLodBlock` lifecycle (ring eviction, mesh/BLAS/atlas cleanup) is correct as written.

**Not fully verified** (recorded rather than implied): the real-data render trace (creature/NPC-head/effect-shader through `import_nif_scene` → `translate_material` → renderer) was not run this pass — only the LOD half of that trace completed; the single-mesh sweetroll FPS smoke needs a live Vulkan device and wasn't attempted (standing no-parallel-launch rule); on-screen `.btr` visual confirmation wasn't attempted (would have been Tamriel-only and reported a false pass, given SK-D6-01); VWD full-model culling (#1731) reconfirmed still unwired, not filed per standing instruction.

---

### Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice)

**Method**: independent re-derivation (not citation) of all four checklist invariants against live code; cross-checked against `docs/audits/AUDIT_SKYRIM_2026-08-03.md` and `docs/audits/AUDIT_NIFAL_2026-08-07.md`.

**Checklist invariants — all four CLEAN**: `translate_material` remains the single canonical boundary (exactly two production callers, no third `Material{}` literal on any content path); per-draw `Material::classify_pbr` remains deleted (sole classifier is parse/translate-time `classify_pbr_keyword`); `resolve_pbr()` still runs before `classify_glass_into_material` so forced-glass roughness provably cannot be clobbered by the second-phase normal-alpha-as-spec pass; `EmissiveSource` routing is mechanically clean (Skyrim BSLSP → `Lighting`, cannot be downgraded by a co-present legacy `NiMaterialProperty`). Three prior-pass findings re-verified: #2284 and #2327 CLOSED with fixes confirmed live; #2330 remains OPEN, not re-reported (dedup).

#### SKY-D7-01: Skyrim's parser arm zeroes two FO4-only BSLSP scalars, and the importer copies them un-gated — canonical `Material.fresnel_power` is `0.0` on all Skyrim content instead of the documented `5.0` neutral
- **Severity**: MEDIUM
- **Location**: producer `crates/nif/src/blocks/shader.rs:938-939` (`parse_skyrim`); un-gated copy `dedicated_shader.rs:321-322`; neutral defaults `import/material/mod.rs:1033-1034`, `import/types.rs:562,565`, `crates/core/src/ecs/components/material.rs:408`; boundary `byroredux/src/material_translate.rs:200`
- **Status**: NEW
- **Description**: `grayscale_to_palette_scale`/`fresnel_power` are FO4+ wire fields (BSVER ≥ 130); every default site in the pipeline agrees on the neutral fallback (`1.0`/`5.0`) **except** `parse_skyrim`, which constructs the block with literal `0.0`/`0.0` for fields Skyrim never serializes. `apply_bs_lighting_shader` copies both unconditionally with no BSVER gate, so the Skyrim-arm `0.0` survives `into_imported_material` and lands in canonical `Material.fresnel_power = 0.0` for essentially all lit Skyrim geometry — while Oblivion/FO3/FNV (no BSLSP) keep `5.0` and FO4+ get their authored value. The canonical, game-agnostic `Material` diverges by source game on a field no game authors on Skyrim.
- **Evidence**: The very test meant to guard this (`material_info_default_matches_bslsp_parser_stub_defaults`) asserts only `MaterialInfo::default()`'s own literals against the FO76+ stopcond stub — it never compares against `parse_skyrim`, so it's structurally incapable of catching this exact drift.
- **Impact**: **Latent today** — `Material.fresnel_power` has no GPU consumer yet (the only `fresnel_power` hits in the renderer belong to an unrelated cell-ambient-cube term). The moment a `triangle.frag` consumer lands (the explicitly stated #2284 follow-up), Skyrim gets a Schlick exponent of `0.0` — `pow(1-cosθ,0)==1.0`, full Fresnel at every view angle, uniformly edge-bright/washed shading across all Skyrim content while FO4 renders correctly. A whole-game shading regression seeded now, detonating later at a site nobody will suspect. Rated MEDIUM (not the HIGH floor `_audit-severity.md` sets for wrong NIFAL output) because present-day live impact is nil; becomes HIGH the day the shading consumer lands.
- **Related**: #2284 (landed the six BSLSP scalars, promoting this latent parser quirk into canonical-tier state); #1241; SKY-D7-02
- **Suggested Fix**: Make `parse_skyrim` construct both fields with the same neutral literals every other default site uses (`1.0`/`5.0`) — a one-line change per field, no downstream BSVER gate needed. Extend the guard test to assert the invariant against all three parser arms.

#### SKY-D7-02: `MaterialInfo` default docs cite a "BSLSP parser stub default" that the Skyrim parser arm contradicts, at line numbers stale since the `#1279` parser split
- **Severity**: LOW
- **Location**: `import/material/mod.rs:588-598,1029-1031`; `lighting_shader_pbr_tests.rs:205-209`
- **Status**: NEW
- **Description**: Three sites anchor the neutral-default doc to specific `shader.rs` line numbers that, since the `#1279` three-arm parser split, land in unrelated code (the `starfield_tail` doc, not the stub). The docs also assert a single "parser stub default" exists when there are two disagreeing ones (`material_reference_stub` = `1.0/5.0`, `parse_skyrim` = `0.0/0.0`).
- **Impact**: A reader following these anchors lands in unrelated code and concludes the default contract is upheld — the documentation half of why SKY-D7-01 went unnoticed through #1241 → #2284.
- **Related**: SKY-D7-01
- **Suggested Fix**: Anchor to the function name, not a line number; state plainly which parser arms honour the neutral default.

#### SKY-D7-03: `EmissiveSource::None`'s documented contract ("no non-zero emissive authored") is contradicted by the unconditional `Lighting` tag — on Skyrim the discriminator degenerates to "has a BSLightingShaderProperty"
- **Severity**: LOW
- **Location**: contract `crates/core/src/ecs/components/material.rs:452-457`; Skyrim set-site `dedicated_shader.rs:298-300`
- **Status**: NEW
- **Description**: `apply_bs_lighting_shader` sets `EmissiveSource::Lighting` unconditionally regardless of whether `emissive_color`/`emissive_multiple` are actually non-zero. Vanilla Skyrim ships the overwhelming majority of BSLSP blocks with an unauthored `[0,0,0]`/`1.0` emissive, all tagged `Lighting` anyway — the discriminator carries no emissive-authoring information on Skyrim, contrary to its own doc's parenthetical.
- **Impact**: None at runtime today (no `GpuMaterial` field, no shader branch reads it yet). Cost is that the #1280 discriminator doesn't yet answer the question its doc promises.
- **Related**: #1280, #166
- **Suggested Fix**: Either amend the doc to describe actual behavior, or gate the three set-sites on a non-zero emissive contribution.

#### SKY-D7-04: `Material`'s #2284 doc cites a `grayscale_to_palette_scale` "precedent field" that does not exist on `Material` — the field is silently dropped at the NIFAL boundary and nothing records it
- **Severity**: LOW
- **Location**: `crates/core/src/ecs/components/material.rs:256-260`; dropped at `material_translate.rs:120-215`; carried on `import/types.rs:489`
- **Status**: NEW
- **Description**: The #2284 doc justifies landing six BSLSP scalars by appealing to a `grayscale_to_palette_scale` "precedent" on `Material` — but `Material` has no such field. The value is captured at import, reaches `ImportedMaterial`, and is then dropped entirely by `translate_material`; the only surviving trace is a `triangle.frag` comment describing a GPU-side gap without disclosing the value never leaves the raw tier.
- **Impact**: Low and FO4-facing (Skyrim never authors the field — see SKY-D7-01), so no Skyrim content is mis-shaded. Makes the NIFAL boundary look more complete than it is.
- **Related**: #2284; SKY-D7-01; `docs/engine/nifal.md`'s "Materials — converged" verdict, slightly overstated by this omission
- **Suggested Fix**: Correct the cross-reference to name `ImportedMaterial::grayscale_to_palette_scale` (raw tier) and add the field to `docs/engine/nifal.md`'s known-gap list.

---

## Cross-Dimension Duplicate Check

Per the merge instructions, every finding was checked against every other
dimension's findings for overlap. Confirmed **no duplicates**:

- SK-D6-01 (LOD quad-grid origin) is unrelated to any other finding — it
  touches only `object_lod.rs`/`terrain_lod.rs`'s worldspace-relative
  addressing, not geometry parsing, shading, or the material boundary.
- SK-D6-02 (`.bto`/`.btr` outside the corpus gate) and Dimension 5's BSA
  findings both touch archive-adjacent content but at different layers
  (NIF-parser corpus coverage vs. BSA byte-extraction correctness) — not
  the same defect.
- SK-D1-01 (FaceGen skin payload dropped) and Dimension 3's clean bill of
  health are not contradictory: Dimension 3's live smoke test asserts
  entity/draw/texture counts and equip-slot occupancy, never per-vertex
  skin-weight presence, so it could not have caught SK-D1-01. Both stand.
- SKY-D2-01 (FO76/Skyrim `material_kind` leak) and SKY-D7-01/02/03/04
  (canonical material-field divergences) both concern material data but at
  disjoint fields (`material_kind`/tint routing vs. `fresnel_power`/
  `EmissiveSource`/`grayscale_to_palette_scale`) with disjoint blast radii
  (FO76 vs. latent Skyrim) — not the same defect.

## Shader-Type Coverage Matrix

`ShaderTypeData` has **9 Rust variants** dispatched from `parse_shader_type_data`
(Skyrim/FO4 `BSLightingShaderType`, 18 numeric values) and the separate
`parse_shader_type_data_fo76` (`BSShaderType155`, 7 numeric values). Derived
from Dimension 2's corpus run (81,244 shader blocks, 22,047 NIFs) and
Dimension 7's producer-arm coverage notes.

| Variant | Numeric type(s) | Vanilla Skyrim instances | Parse-complete | Import-complete | Render-complete |
|---|---|---:|---|---|---|
| `None` | 0, 2, 3, 4, 8–10, 12–13, 15, 17–19 (Skyrim/FO4) | 45,458 + 1,395 + 11 + 3,158 = 50,022 | ✅ PASS — zero over/under-read across all types, confirmed by empty block-drift histogram | N/A (no trailing fields) | N/A |
| `EnvironmentMap` | 1 | 6,726 | ✅ PASS (1×f32 scale, matches nif.xml) | ✅ `env_map_scale` copied to `MaterialInfo`, feeds `classify_pbr_keyword`'s `>0.3` gate | ✅ reaches env-cube texture-slot routing + PBR classification |
| `SkinTint` | 5 (Skyrim); FO76 type 4 → remapped to 5 | 1,631 | ✅ PASS (Color3) | ✅ `skin_tint_color` captured | ✅ `material_kind==5` → `skin_tint_rgba` (Skyrim-source and the one remapped FO76 case both correct) |
| `HairTint` | 6 (Skyrim); FO76 type 5 → **not** remapped | 10,817 | ✅ PASS (Color3) | ✅ `hair_tint_color` captured | ✅ Skyrim-source only. **❌ FAIL for FO76-source** — SKY-D2-01: `material_kind` stays 5, the `material_kind==6` render gate never fires, authored tint is dropped |
| `ParallaxOcc` | 7 | 0 (mod-reachable only) | ✅ PASS structurally, but **0 wire-level test coverage** (SKY-D2-03) | not traced this audit | not traced this audit |
| `MultiLayerParallax` | 11 | 662 | ✅ PASS (5×f32) | ✅ inner-layer fields captured, texture-slot routing confirmed correct (env/mask/inner) | ✅ via generic texture-slot pipeline |
| `SparkleSnow` | 14 | 19 | ✅ PASS structurally, but **0 wire-level test coverage** (SKY-D2-03) | not traced this audit | not traced this audit |
| `EyeEnvmap` | 16 | 3,251 | ✅ PASS (7×f32: eye cubemap scale + 2× Vector3) | ✅ captured | ✅ via generic env-cube + mask texture-slot routing |
| `Fo76SkinTint` | BSShaderType155 type 4 (FO76 only) | 0 on Skyrim (BSVER < 155) | ✅ PASS (Color4, separate `parse_shader_type_data_fo76`) | ✅ correctly remapped to `material_kind=5` (the one value SKY-D2-01 confirms *is* handled) | ✅ reaches `skin_tint_rgba` via the type-4 remap |

**Numeric types mapping to `None`**: 0 (Default), 2, 3, 4, 8, 9, 10, 12, 13,
15, 17, 18, 19 under the Skyrim/FO4 `BSLightingShaderType` numbering — 13 of
the 18 possible values, exercised in vanilla Skyrim by 0/2/3/4 only (types 8–10/
12–13/15/17–19 never appear in vanilla Skyrim SE content).

## Cell-Load Regression Status

TES5 cells parse through the unified `esm/cell/` walker — the per-game
legacy stub was removed under #390, and there is no separate Skyrim-specific
cell parser. Dimension 4's real-data run confirms the walker healthy:
`parse_real_skyrim_esm` against real `Skyrim.esm` finds 590 cells, 18,113
statics, 37 worldspaces, and `SolitudeWinkingSkeever` with 981 refs and
populated Skyrim-extended XCLL lighting; compressed-record decompression,
deleted-REFR (0x20) skip, ESL/light-master FormID decode, and `.STRINGS`
load-order wiring are all live-verified against real `Skyrim.esm` +
`Update.esm` + `Dawnguard.esm` (10,045 entities, 0 errors, once the
documented repro command's two factual errors — see Dimension 4 Finding 1 —
are corrected).

**Whiterun BanneredMare control-bench status**: two independent
measurements exist and should not be conflated.

- **Live smoke run (this audit, Dimension 3)**: `docs/smoke-tests/m41-equip.sh
  skyrim` against real data — **5,183 entities**, 1,500 draws, all hard
  assertion floors cleared, 0 missing textures. This is a targeted M41 equip
  check (30-frame bench-hold), not the full FSR bench matrix.
- **ROADMAP Bench-of-record (R6a-stale-18, 2026-08-04, HEAD `28155b79`)**:
  Whiterun BanneredMare at **5,150 entities, 65.1 FPS / 15.37 ms** (TAA-native)
  / 136.6 FPS / 7.32 ms (FSR Quality). Per ROADMAP's own freshness note,
  this record is **37 commits stale** as of Session 63 close (flagged
  `R6a-stale-19`, not re-run) — none of the intervening commits are believed
  to touch a renderer hot path.

Per the Dimension 4 checklist's own control-bench guard ("Skyrim ships real
`bhk` collision, so entity count is flat across collider-gate changes — any
drop in entity count or FPS regression at flat entity count is a
control-bench regression"), the guard is **currently violated**: entity
count grew **3,406 → 5,150 (+51%)** between the R6a-stale-17 and R6a-stale-18
refreshes for reasons not yet root-caused, alongside an apparent −33% FPS
drop that is *confounded by* (not independent of) that entity growth. This
is **already tracked as open issue `#2367`** and is not attributable to
anything in this audit's own scope (multi-master load order / TES5 cell
walk / shader dispatch / NIFAL) — ROADMAP attributes the regression window
to Session 60–62 renderer feature work (volumetric fog, clustered local fog
volumes, GI extensions). Not re-filed here; noted for completeness since
it's this audit's named control-bench metric.

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 2 |
| MEDIUM | 4 |
| LOW | 11 |
| **Total** | **17** |

**HIGH** (2): SK-D1-01 (FaceGen skin payload dropped — 78% of skinned
Skyrim SE geometry renders rigid), SK-D6-01 (per-worldspace LOD quad-grid
origin ignored — 9 of 12 worldspaces get zero distant object LOD).

**MEDIUM** (4): SK-D1-02 (single-partition bone-remap shortcut, 59 shapes
mis-skinned), SKY-D2-01 (FO76/Skyrim `material_kind` enum leak, FO76-only
blast radius), SK-D6-02 (`.bto`/`.btr` outside every corpus regression
gate), SKY-D7-01 (`fresnel_power` canonical divergence, latent until a
shading consumer lands).

**LOW** (11): SK-D1-03, SKY-D2-02, SKY-D2-03, SKY-D2-04, Dimension-4
Finding 1 (CLAUDE.md repro command), SK-D5-LZ4-LOW-01, SK-D5-LZ4-LOW-02,
SK-D6-03, SKY-D7-02, SKY-D7-03, SKY-D7-04.

**Already-tracked, not re-filed**: Whiterun control-bench entity-count
growth (`#2367`), VWD full-model culling forward-scope note (`#1731`).

Suggest: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-08-07.md`
