# Skyrim SE Compatibility Audit — 2026-06-28

- **Command**: `/audit-skyrim` (all 7 dimensions, deep)
- **Branch**: main · **Engine HEAD** ~`b312951b`
- **Method**: Orchestrator + 7 dimension agents (legacy-specialist / renderer-specialist / general-purpose), adversarial per-finding disproof, symbol-anchored verification. Real Skyrim SE data present at `/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/` (Skyrim.esm + Dawnguard/Dragonborn/HearthFires/Update + ESL/CC + `Skyrim - Meshes0/1.bsa` + `Textures0..8.bsa`), so every real-data validation actually ran headlessly. On-device cell-render / FPS bench is out of scope (no Vulkan device in the agent env). Dedup baseline: `gh issue list` (31 open) + the prior all-clean sweep `AUDIT_SKYRIM_2026-06-23.md`.

---

## Executive Summary

Skyrim SE is the engine's **renderer control bench** (Whiterun BanneredMare, 6 equipped NPCs); both loose-mesh and cell rendering work. This audit is regression coverage of the Skyrim-specific geometry/shader/equip/load-order risk surface. **All seven dimensions verified clean of correctness regressions** — the 2026-06-23 baseline holds across the board, re-confirmed against real archive data (18,862 Meshes0 NIFs @ 100% clean / 0 unknown / 0 realignment WARNs; 75,471 shader blocks; full v105 LZ4 extraction sweep). The only NEW finding is one **LOW doc-rot** comment left behind by an interim fix. Three tracked issues are reconciled to their true GitHub state (two already closed; one stale-open that should be closed).

### Finding Tally

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 1 new (SKY-D4-NEW-01, doc-rot) |

**Positive deltas since 2026-06-23** (no longer findings): **#1660** deleted-REFR tombstones — FIXED (`2dc43106`, 2026-06-26); **#1560** equip 6-NPC count guard — landed + issue CLOSED; **#1661** numeric-sibling zero-start — issue CLOSED. The only remaining Skyrim-adjacent carry-over is **#1659** (BSDismember body-part flags, still OPEN, out of fix-scope this pass).

### Session housekeeping (not a code finding)
An interim sub-agent run left a broken **untracked** `crates/nif/examples/vf_survey.rs` (referencing a removed `BsaArchive`/`NifScene` API) that broke `cargo test -p byroredux-nif` (unfiltered). The orchestrator **removed it**; `cargo build -p byroredux-nif --examples` is green again.

---

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction — CLEAN (0 findings)
All four invariants HOLD as regression guards. `VF_*` bits map 1:1 to nif.xml `BSVertexDesc.VertexAttribute` (attribute nibbles `(vertex_desc >> 44) & 0xFFF`, stride low-nibble); `half_to_f32` is correct IEEE-754 binary16. `decode_bs_vertex_stream`'s consumed-vs-stride guard **hard-errors** on `consumed > vertex_size_bytes` (no `usize` underflow / runaway skip) — over-read cannot wrap. SSE skinned-recon (`decode_sse_packed_buffer`) Z-up→Y-up converts positions/normals via the canonical `zup_to_yup_pos` (`[x,z,-y]`, shared with the inline path), routes the on-disk "bitangent" triplet as the Y-up tangent (∂P/∂U), derives the sign from the on-disk tangent (∂P/∂V) via `crate::types::bitangent_sign` (operand order pinned, #1516), and gates on `VF_TANGENTS` **alone** (#1559) — no magenta/chrome handedness flip. Alpha-property cascade consulted at exactly the two `!info.alpha_property_consumed` sites in `material/walker.rs`, set once in `material/mod.rs`. Real-data: Meshes0 100% clean, 52,196 BSTriShape + 21,140 BSDynamicTriShape + 26,708 NiSkinPartition, 0 realignment recoveries.

### Dimension 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch — CLEAN (0 findings)
`parse_shader_type_data` dispatches Skyrim types {1,5,6,7,11,14,16} to the correct `ShaderTypeData` arm with EOF-asserted trailing-field counts (wire pins in `shader_tests.rs`); all other types hit the single `_ => None` arm reading **zero** bytes — no silent over-read. FO76 (`parse_shader_type_data_fo76`, {0,2,3,4,5,12,17}) stays a separate enum (type4=`Fo76SkinTint` Color4 vs Skyrim type4→None; type5=HairTint Color3 vs Skyrim type5=SkinTint Color3) — no cross-contamination. Skyrim flag bits live in separate `skyrim_slsf1`/`skyrim_slsf2` modules with the bit-21 decal/cloud-LOD divergence explicitly pinned. `BSEffectShaderProperty` field order matches nif.xml (the #166 emissive→base_color rename is byte-identical). #1241 PBR scalars flow into `MaterialInfo`/`ImportedMesh`. **Disney/Burley lobe vanilla-unreachable guard HELD**: `MAT_FLAG_PBR_BSDF` is OR'd only by `pack_bgsm_material_flags` under `if mesh.is_pbr`, and all three NIF extractors hard-code `is_pbr: false` (set only by BGSM/`.mat`/CDB merge, FO4+) → vanilla Skyrim authors **0** instances. Real-data: 75,471 shader blocks, 0 unknown.

### Dimension 3 — NPC Equip + FaceGen (M41) — CLEAN (0 new; 2 carry-overs reconciled)
`build_npc_equip_state` is hoisted ABOVE skeleton load in `spawn_prebaked_npc_entity` (equip components land even on a mesh-less early-return). `resolve_armor_mesh` walks ARMO→ARMA→worn-mesh with the upperbody.nif pre-scan skip. LVLI `expand_leveled_form_id` is level-gated; 8 resolver tests green (passthrough/level-gate/multi-pick/nested/circular-cap/unknown-id). FaceGen heads parse via the `BSDynamicTriShape` dispatch arm + `facegen` crate (24 tests green). Tests: `byroredux-plugin equip` 33 pass, `byroredux --bins` 515 pass (incl. both prebaked-equip hoist guards), `byroredux-facegen` 24 pass. **#1560 (count guard) is verified CLOSED** — `m41-equip.sh` now passes `equip_floor 6` as a HARD-FAIL on both Inventory + EquipmentSlots. **#1659** (BSDismember per-partition body-part flags parsed but routed nowhere — bone/vertex half DOES route) confirmed still OPEN; not re-filed.

### Dimension 4 — Multi-Master Load Order + TES5 Cell-Load Regression — 1 LOW (doc-rot)
All five checklist items pass as regression guards: repeatable `--master` FormID remap (`build_remap_for_plugin` / `FormIdRemap::remap` / last-write-wins via `merge_from`, named missing-master diagnostics); per-plugin `.STRINGS` (`install_strings_guard` inside the load loop — not just the active plugin); ESL 12-bit light-space decode (`0xFE00_0000 | ((sub&0xFFF)<<12) | (raw&0xFFF)`, symmetric own-forms + refs-to-ESL); `parse_real_skyrim_esm` finds `SolitudeWinkingSkeever` on real `Skyrim.esm`; compressed + minimum-render record set parse. **#1660 deleted-REFR tombstones is FIXED** (`2dc43106`, `walkers.rs:803` `RECORD_FLAG_DELETED` skip + 2 passing tests in `cell/tests/refr.rs`). The fix left one stale doc comment → the single new finding below. Tests: `byroredux-plugin` 514 pass + `parse_real_skyrim_esm` 1 pass.

### Dimension 5 — BSA v105 (LZ4) — CLEAN (0 findings)
v105 header/version dispatch ({103,104,105} accepted, else clean `InvalidData`), 24-byte folder records with u64 offsets, LZ4 codec via `lz4_flex::frame::FrameDecoder`, embed-name + compression XOR-toggle priority (per-file bit *toggles* the archive default — libbsarch/UESP-correct), full-archive extraction, and zero-based sibling auto-load (`numeric_sibling_paths` / `siblings_skyrim_zero_start_offers_1_through_9`) all correct and regression-pinned. Real-data: `skyrimse_meshes_bsa_v105_brute_force_extract_zero_errors` passes; Sweetroll extracts to exactly 10,245 B with Gamebryo magic; `byroredux-bsa` 52 unit + 16 real-data tests green. **#1661 verified CLOSED.** *(Checklist phrasing note, not a finding — see Skill-checklist notes.)*

### Dimension 6 — Specialty Blocks + Real-Data Rendering — CLEAN (0 findings)
All four guards HELD against adversarial disproof: **#838** `"BSLODTriShape" → NiLodTriShape::parse` (97 B NiTriShape body + 3 trailing LOD u32 = 109 B, NiTriBasedGeom lineage) stays distinct from `"BSMeshLODTriShape" → BsTriShape::parse_lod` and `BSSubIndexTriShape` (#404) — pinned by a dispatch test; corpus 23 BSLODTriShape, 0 unknown, 0 over-read. **#837** `BsLagBoneController` (12-B trailer) + `BsProceduralLightningController` (73-B trailer) dedicated parsers present — corpus 163 + 3, 0 realignment WARNs (the exact symptom a missing arm reintroduces is absent). `BSTreeNode` wind-bones / `BSPackedCombined[Shared]GeomDataExtra` / `BSFadeNode`→NiNode / `BSBlastNode`→BsRangeNode walker unwraps all present. **M35 `.btr`** distant-terrain LOD wired from `terrain_lod.rs` with an `.or_else(spawn_lod_block)` fall-through (a `.btr` miss degrades to synth, never to silent no-LOD); `btr_local_to_world` unit-tested. Real-data: Meshes0 100% clean / 0 WARN; 843 nif tests green. Three render-trace paths (tree-LOD static / skinned creature / BSEffectShaderProperty magic) traced clean at import→translate→draw-assembly; final lit pixels flagged **needs RenderDoc** (out of headless scope, no speculative changes).

### Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice) — CLEAN (0 findings)
`translate_material` is the sole boundary (two production callers: `scene/nif_loader.rs:796`, `cell_loader/spawn.rs:880`); `Material.metalness`/`roughness` are plain resolved `f32` seeded from override-or-NaN then filled by `resolve_pbr`→`classify_pbr_keyword`; the per-draw `Material::classify_pbr` is DELETED with no render-time re-classification (`render/static_meshes.rs` reads resolved scalars directly); `resolve_pbr()` runs BEFORE `classify_glass_into_material` (forced-glass roughness wins). **Skyrim-specific primary check VERIFIED**: `BSLightingShaderProperty.emissive_multiple` routes to `EmissiveSource::Lighting` (`walker.rs:347-348`), and the `Effect`/`Material` arms are gated on `!info.has_material_data` so a Skyrim mesh cannot be clobbered to `Effect`; the discriminator is copied verbatim through the mesh extractor + boundary. Pinned by `bslighting_tags_emissive_source_as_lighting` + `bseffect_tags_emissive_source_as_effect`. Tests: `byroredux-core material` 18, `byroredux-nif emissive` 12, `byroredux material_translate` 7 — all green.

---

## Findings

### SKY-D4-NEW-01: stale deleted-REFR tombstone doc comment (the #1660 fix left it behind)
- **Severity**: LOW (doc-rot; no runtime impact)
- **Dimension**: Multi-Master Load Order
- **Location**: `crates/plugin/src/esm/cell/mod.rs` :: doc comment on `merge_cell_references` (the "tombstones (the 0x20 Deleted flag) aren't captured by the parser yet" line)
- **Status**: NEW (drift introduced by the #1660 fix landing 2026-06-26)
- **Description**: The comment asserts deleted-REFR tombstones "aren't captured by the parser yet." True at the 2026-06-23 baseline; **false since `2dc43106` (2026-06-26)** added the `RECORD_FLAG_DELETED` (`0x0020`) skip in `crates/plugin/src/esm/cell/walkers.rs` (the REFR walk). The fix landed in `walkers.rs`; the neighbouring `mod.rs` comment was not updated, so the codebase now documents a gap it no longer has.
- **Evidence**: orchestrator-confirmed `cell/mod.rs:947` still reads "…aren't captured by the parser yet"; `walkers.rs:62` `const RECORD_FLAG_DELETED: u32 = 0x0000_0020` + skip at `:803`; tests `deleted_refr_tombstone_is_skipped` + `non_deleted_refr_still_places` (`cell/tests/refr.rs:122,151`) both pass; `git log 2dc43106` = "Fix #1730 #1660: …skip deleted-REFR tombstones".
- **Impact**: None at runtime. Risk is a future reader / stale-premise audit re-filing #1660 (this audit's own dedup brief tripped on the stale "still present" label).
- **Suggested Fix**: One-line doc edit at `cell/mod.rs` — state tombstones are now skipped at the walker level and cite #1660 as resolved.

---

## Shader-Type Coverage Matrix (`ShaderTypeData`, Skyrim `BSLightingShaderType`)

| Numeric type | ShaderTypeData arm | Trailing (bytes) | Parse | Import |
|---|---|---|---|---|
| 0 Default | `None` | 0 | ✓ | pass-through |
| 1 EnvironmentMap | `EnvironmentMap` | env_map_scale (4) | ✓ | `env_map_scale` |
| 2 Glow | `None` | 0 | ✓ | — |
| 3 Parallax | `None` | 0 | ✓ | — |
| 4 Face Tint | `None` | 0 | ✓ | — |
| 5 SkinTint | `SkinTint` | Color3 (12) | ✓ | `skin_tint_color` |
| 6 HairTint | `HairTint` | Color3 (12) | ✓ | `hair_tint_color` |
| 7 ParallaxOcc | `ParallaxOcc` | max_passes+scale (8) | ✓ | parallax fields |
| 8–10 Landscape | `None` | 0 | ✓ | — |
| 11 MultiLayerParallax | `MultiLayerParallax` | 20 | ✓ | 4 multi_layer fields |
| 12–13 Tree/LOD | `None` | 0 | ✓ | — |
| 14 SparkleSnow | `SparkleSnow` | 4×f32 (16) | ✓ | `sparkle_parameters` |
| 15 LOD HD | `None` | 0 | ✓ | — |
| 16 EyeEnvmap | `EyeEnvmap` | scale + 2×Color3 (28) | ✓ | eye cubemap + L/R centers |
| 17–20 Cloud/Noise | `None` | 0 | ✓ | — |

All `None`-mapped types read **zero** trailing bytes (single `_ =>` arm) — no silent over-read. FO76 (`BSShaderType155`) admits only {0,2,3,4,5,12,17} through the distinct `parse_shader_type_data_fo76` — no enum bleed. Render-complete status for the lit arms is GPU-side (needs-RenderDoc), out of this headless pass.

---

## Cell-Load Regression Status

TES5 cells parse through the unified `esm/cell/` walker; compressed records decompress; `parse_real_skyrim_esm` finds `SolitudeWinkingSkeever` on real `Skyrim.esm`. ESL light-master forms decode into the `0xFE` 12-bit sub-index space; per-plugin `.STRINGS` load is wired into the multi-plugin path; `--master` cross-plugin FormID remap is last-write-wins with named-missing-master diagnostics. **Control-bench note**: Whiterun BanneredMare entity-count + FPS vs the ROADMAP Bench-of-record (R6a-stale-14, `1c26bc25`, 2026-06-03: 3216 ent / 362.8 FPS / fence=0.98) requires a Vulkan device + live cell render — **not exercised** this session (no GPU). The bench-of-record is ~375 commits stale; R6a-stale-15 gates any current FPS claim. Headless plugin + NIF suites all green, so no parse-level cell-load regression.

---

## Issue-State Reconciliation (verified against live GitHub)

| Issue | Brief label | True state | Action |
|---|---|---|---|
| #1660 (SKY-D4-01) | MEDIUM, OPEN carry-over | **Fixed `2dc43106`, but still OPEN** (the `Fix #1730 #1660` commit auto-closed only #1730 — the multi-issue-keyword gotcha) | **Recommend closing #1660** (fix + 2 tests live) |
| #1560 (SK-D3-02) | OPEN carry-over | **CLOSED** (count guard ships in `m41-equip.sh`) | None |
| #1661 (SKY-D5-01) | LOW, OPEN | **CLOSED** (sibling zero-start fix `821a425b`) | None |
| #1659 (SKY-D3-03) | MEDIUM, OPEN | **OPEN** (BSDismember body-part flags parsed, not routed) | Carry-over; out of fix-scope this pass |

---

## Skill-checklist notes (audit-tooling doc-rot, not engine findings)

- **Dim 5 checklist** says "LZ4 **block** decompression via `lz4_flex::block`." The live v105 BSA path correctly uses `lz4_flex::frame::FrameDecoder` (the **frame** codec); `lz4_flex::block` belongs to Starfield's BA2, and a negative test (`synthetic_v105_block_codec_payload_is_rejected_by_frame_reader`, #1558) deliberately pins that a block-encoded body must NOT round-trip through the v105 frame reader. Code is correct; the checklist wording is loose. Suggest rewording the Dim-5 bullet to "LZ4 **frame** decompression via `lz4_flex::frame`."

---

## Verification Commands Run

```
gh issue list / gh issue view 1560 1661 1660            # state reconciliation
nif_stats "Skyrim - Meshes0.bsa"                        # 18862 / 100% clean / 0 unknown / 0 realignment WARN
cargo test -p byroredux-nif                             # 843 green
cargo test -p byroredux-plugin                          # 514 green
cargo test -p byroredux-plugin parse_real_skyrim_esm -- --ignored  # 1 passed (SolitudeWinkingSkeever)
cargo test -p byroredux-bsa (+ --test bsa_real --ignored)          # 52 unit + real-data extraction green
cargo test -p byroredux-plugin equip / byroredux --bins / byroredux-facegen  # 33 / 515 / 24 green
cargo test -p byroredux-core material / byroredux-nif emissive / byroredux material_translate  # 18 / 12 / 7 green
rm crates/nif/examples/vf_survey.rs ; cargo build -p byroredux-nif --examples  # stray removed, examples build green
```

## Prioritized Fix Order

1. **SKY-D4-NEW-01** (LOW) — one-line doc edit at `crates/plugin/src/esm/cell/mod.rs`; fold into the next touch of that file.
2. **Close #1660** on GitHub (fix already shipped `2dc43106`; the multi-issue-keyword commit left it stale-open).

No correctness work required — Skyrim SE compatibility holds clean across all 7 dimensions.
