# Oblivion Compatibility Audit — 2026-04-17

**Scope**: Readiness to load and render content from The Elder Scrolls IV: Oblivion (NIF v20.0.0.5, BSA v103, TES4 ESM).
**Method**: Six parallel dimension audits (legacy-specialist, renderer-specialist, general-purpose) against the codebase + vanilla Oblivion install at `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`.
**Verdict**: Three material state updates vs. the previous baseline. One is positive (BSA v103 is green); two are negative (NIF parse rate is regressed, ESM LIGH color is swizzled cross-game).

---

## Executive Summary

| Layer              | State                                                                                                   |
|--------------------|---------------------------------------------------------------------------------------------------------|
| NIF v20.0.0.5 parser | Parser **correctness good** (Dim 1: no Critical/High). Real-data sweep shows **regression** (Dim 5: ~9% truncation + OOM abort). |
| BSA v103 archive    | **WORKING** — 83,217 / 83,217 files extracted across all 17 vanilla BSAs (Dim 2). ROADMAP claim "decompression NOT WORKING" is stale. |
| ESM (TES4)          | Walker survives full `Oblivion.esm` in 0.2 s (Dim 3). Missing CREA + ACRE dispatch, missing XCLW water height. LIGH DATA byte order is BGRA but read as RGB — **cross-game bug**, not Oblivion-only. |
| Cell → render       | Anvil Heinrich Oaken Halls already renders (per README). Next interior with creatures/animated doors requires CREA + KF importer work. |

### Top blockers (priority order)

1. **[CRITICAL / Dim 5 C-1]** OOM abort on `NiTextKeyExtraData::parse` — unchecked `Vec::with_capacity(u32_from_file)` at [interpolator.rs:432](crates/nif/src/blocks/interpolator.rs#L432) allocates 135 GB on `clutter/upperclass/upperclassdisplaycaseblue01.nif`. Kills any content sweep. 15-min fix (`check_alloc`), then audit the ~40 sibling sites.
2. **[CRITICAL / Dim 3 C2]** LIGH DATA color read as RGB on disk layout that is BGRA. Every torch/brazier currently emits wrong-hue light cross-game. 3-line fix at [cell.rs:854-856](crates/plugin/src/esm/cell.rs#L854-L856).
3. **[HIGH / Dim 5 H-1,H-2]** 9 missing Oblivion-only block parsers (`NiBSBoneLODController`, `NiColorData`, `NiFogProperty`, `NiFlipController`, `NiPathInterpolator`, `NiBoolTimelineInterpolator`, `bhkMultiSphereShape`, `bhkBlendController`) cascade into 678 truncated scenes (138 of which lose their root NiNode = empty render).
4. **[HIGH / Dim 4 C4-01]** `BlendType::from_nif_blend` collapses 7+ Gamebryo AlphaFunction pairs into generic `SRC_ALPHA/INV_SRC_ALPHA`. Magic FX, glass, additive decals all wrong-blend. Pipeline has only 3 static variants.
5. **[HIGH / Dim 4 H4-01,H4-03]** `NiZBufferProperty` z_test/z_write and 3 of 7 texture slots (glow/detail/gloss) extracted to `MaterialInfo`, then dropped on the floor before reaching GpuInstance.
6. **[HIGH / Dim 6 #4]** KF animation importer does not handle Oblivion's `NiSequenceStreamHelper` + `NiKeyframeController` chain. All door idles, creature anims dead-on-arrival.
7. **[HIGH / Dim 6 #2]** Legacy particle stack parses but has zero renderer path. All dungeon torches/spell FX/ghosts invisible.
8. **[HIGH / Dim 3 H2,H4]** ESM: `CREA` absent from MODL match arm (dungeon creatures don't spawn); `XCLW` water height not parsed (flooded Ayleid ruins render dry).

### Stale doc surface to fix

- `.claude/commands/audit-oblivion.md:19-22` — "BSA v103 — archive opens, decompression NOT WORKING (open blocker)". False.
- `ROADMAP.md:778` + game matrix row ~L817 — same blocker misstatement.
- `crates/nif/tests/parse_real_nifs.rs:74-78` — comment claims "BSA v103 decompression is not yet implemented". False and the test now live-aborts via C-1.
- `CLAUDE.md` — "Oblivion → 100%" / "ALL 7 games at 100% (177,286 NIFs)" is regressed. Real clean-parse rate on Oblivion is ≈ 90.96%.

---

## Dimension Findings

### Dim 1 — NIF v20.0.0.5 Parser Correctness

No Critical or High findings. All 27 checklist items verified correct. Three Medium + four Low follow-ups:

- **M-01** — `as_ni_node` at [walk.rs:34-66](crates/nif/src/import/walk.rs#L34-L66) does NOT unwrap `NiCamera`; cinematic NIFs with an NiCamera anywhere in the parent chain silently drop their subtrees. No impact on cell loading.
- **M-02** — `NiLODNode` walker hard-picks `children[0]` without consulting `lod_level_data` ([walk.rs:80-89](crates/nif/src/import/walk.rs#L80-L89)). Low content impact.
- **M-03** — `NiLODNode` pre-10.1.0.0 legacy path writes `BlockRef::NULL` instead of consuming the inline LOD body ([node.rs:452-459](crates/nif/src/blocks/node.rs#L452-L459)). Morrowind-expansion risk only.
- **L-01..L-04** — BSStreamHeader gate comment drift, `NifVariant::Oblivion.bsver() == 0` misleading hardcode, zero-default `NiTexturingProperty.flags` in the 10.0.1.3–20.1.0.1 gap window, median-based `parsed_size_cache` recovery strategy.

**Regression guards verified holding**:
- `NiTexturingProperty` trailing count is raw u32 (no bool gate) — [properties.rs:311-312](crates/nif/src/blocks/properties.rs#L311-L312). Regression test at `:606` pins this.
- `user_version` threshold gated at `>= 10.0.1.8` — [header.rs:81](crates/nif/src/header.rs#L81).
- BSStreamHeader dual condition `version == 10.0.1.2 || user_version >= 3` — [header.rs:104-110](crates/nif/src/header.rs#L104-L110).
- Full block-type coverage for Oblivion: 13-entry legacy particle stack, 4-entry NiLight hierarchy, 11 BSShader*Property aliases, 12 NiNode subclasses — [blocks/mod.rs:120-630](crates/nif/src/blocks/mod.rs).

### Dim 2 — BSA v103 Archive

**No Critical or High findings. The reported blocker is empirically refuted.**

Measured extraction rates on vanilla Oblivion BSAs (all 17 archives):

| Archive                                | files   | pass    | %    | path exercised         |
|----------------------------------------|---------|---------|------|------------------------|
| Oblivion - Meshes.bsa                  | 20,182  | 20,182  | 100% | zlib default           |
| Oblivion - Textures - Compressed.bsa   | 18,040  | 18,040  | 100% | zlib via per-file toggle |
| Oblivion - Voices1.bsa                 | 33,198  | 33,198  | 100% | uncompressed           |
| Knights.bsa                            | 4,810   | 4,810   | 100% | mixed (4.5% toggled)   |
| ...                                    |         |         |      |                        |
| **Total**                              | **83,217** | **83,217** | **100%** | zlib + XOR toggle + uncompressed all verified |

All 12 spec-correctness checks (header version byte, folder record size, folder offset field position, compression XOR toggle, `u32 original_size` + zlib stream layout, folder + file hash algorithms, v103-vs-v105 zlib/LZ4 dispatch) pass.

**Medium findings** (stale-doc / infrastructure, not behavior):
- **M-1** — Zero v103 test coverage. Every `#[ignore]`-gated integration test targets FNV v104. Add coverage to lock in behavior.
- **M-2** — Misleading comment at [archive.rs:162-164](crates/bsa/src/archive.rs#L162-L164) asserts "Oblivion v103 uses different flag semantics for bits 7-10" — speculation; real UESP semantics for 0x100 in v103 are "Xbox archive". Behavior correct, comment wrong.
- **M-3** — `extract()` reopens the file handle on every call ([archive.rs:378-387](crates/bsa/src/archive.rs#L378-L387)). Perf, not correctness.

### Dim 3 — ESM Record Coverage

**Parser is surprisingly complete for rendering.** Walker survives full `Oblivion.esm` (1,252,095 records) in 0.19s release: 1855 interior cells, 17,846 statics, 1768 cells-with-refs, 95.4% XCLL coverage. Verified by `parse_real_oblivion_esm_walker_survives`.

**Critical**:
- **C1** — `crates/plugin/src/legacy/tes4.rs` is still `todo!()` and has no callers. Dead-code stub. Either delete or wire to `esm::parse_esm` via a `Record`/`PluginManifest` adapter (~40 lines). Same pattern in tes3/tes5/fo4 stubs.
- **C2** — LIGH DATA color bytes are BGRA on disk but read as RGB at [cell.rs:854-856](crates/plugin/src/esm/cell.rs#L854-L856). Sample dump: `DATA[32] = ...36 74 66 00...` = B=0x36, G=0x74, R=0x66. Fix swaps 8↔10. **Cross-game bug — also affects FNV.**
- **C3** — `parse_esm` walker hard-wires `group.total_size - 24` at ~13 sites; Oblivion's group header is 20 bytes. Works today only because `read_group_header()` advances `pos` variant-correctly. Latent corruption risk. Thread `reader.variant().group_header_size()` through.

**High**:
- **H2** — CREA records ignored; MODL match arm at [cell.rs:230-233](crates/plugin/src/esm/cell.rs#L230-L233) lists `NPC_` but not `CREA`. 1-line fix. Unblocks all dungeon creature placements.
- **H4** — CELL subrecords `XCLW` (water height), `XCMT`, `XCCM`, `XCWT`, `XOWN`, `XRNK` dropped by `parse_cell_group`. Rendering-critical among these: **XCLW** (flooded interiors render dry without it).
- **H1/H3** — LIGH radius is u32 on Oblivion/FO3/FNV but f32 on Skyrim (our code assumes u32 unconditionally — Skyrim lights wildly wrong-sized). BOOK skill_bonus field is mis-named (it's the skill AVIF index, signed).

**Medium**: SPEL/ENCH/MGEF/DIAL/INFO/REGN/RCLR unparsed (gameplay-ward; no rendering impact). `records/items.rs` 100% FNV-layout — Oblivion WEAP/ARMO DATA offsets differ; needs a `GameVariant` dispatch per user-memory `format_abstraction.md`.

### Dim 4 — Rendering Path for Oblivion Shaders

**Critical**:
- **C4-01** — `BlendType::from_nif_blend` at [pipeline.rs:46-53](crates/renderer/src/vulkan/pipeline.rs#L46-L53) collapses 11×11 possible Gamebryo AlphaFunction pairs into 3 static pipelines (`opaque`/`alpha`/`additive`). `GpuInstance` has no `src_blend_mode`/`dst_blend_mode` fields ([scene_buffer.rs:44-93](crates/renderer/src/vulkan/scene_buffer.rs#L44-L93)). Oblivion glass (DEST_COLOR/ONE modulate), flame decals (ONE/ONE), lens flares (ONE/INV_SRC_ALPHA) all render wrong.
- **C4-02** — **(regression-guard)** `NiMaterialProperty.ambient/diffuse/specular/emissive` and BSShader emissive/specular are copied raw (no `srgb_to_linear`). Status: **correct as-is** per [feedback_color_space.md] and commit 0e8efc6. Flagged at Critical only to prevent future "fix" regressing it.

**High**:
- **H4-01** — `NiZBufferProperty` `z_test`/`z_write` extracted to `ImportedMesh`, never consumed by the renderer. No pipeline variant for `depth_test_enabled=false` (except hardcoded UI/composite). `z_function` not even pulled from the block.
- **H4-02** — `NiStencilProperty`: only `is_two_sided` consumed at [material.rs:470-477](crates/nif/src/import/material.rs#L470-L477). Function/ref/mask/fail/zfail/pass all discarded. Blocks portal/mirror masks.
- **H4-03** — Three of seven `NiTexturingProperty` slots (glow/detail/gloss) populate the ECS `Material` but have no corresponding `GpuInstance` field. Every enchanted weapon renders without glow.
- **H4-04** — Decal slots (7+) never extracted into the `NiTexturingProperty` struct at all. Blood splatters, wall paintings, Imperial City signs silently vanish.

**Medium**:
- **M4-01** — `NiVertexColorProperty.lighting_mode` parsed but never read. `vertex_mode=1` (Emissive) has no routing — vertex colors unconditionally treated as ambient+diffuse by the shader.
- **M4-02** — `NiWireframeProperty`, `NiDitherProperty`, `NiShadeProperty` parsed but never consumed.

### Dim 5 — Real-Data Validation

**The "Oblivion → 100% / 7963+ NIFs" claim in CLAUDE.md and ROADMAP is regressed.**

**Critical**:
- **C-1** — `parse_nif` aborts the process on `meshes\clutter\upperclass\upperclassdisplaycaseblue01.nif` with `memory allocation of 135,822,034,912 bytes failed` at [interpolator.rs:432](crates/nif/src/blocks/interpolator.rs#L432). Root cause: `Vec::with_capacity(num_text_keys as usize)` is never gated by `check_alloc`; `MAX_SINGLE_ALLOC_BYTES` at [stream.rs:184](crates/nif/src/stream.rs#L184) is bypassed because `with_capacity` allocates `T`-sized slots. The u32 value ≈ 0xFD12EEFF is harvested from misaligned stream position. **CVE-adjacent: a crafted NIF in a mod would DoS the engine.** Same pattern in ~40 sibling sites across `blocks/{collision,skin,palette,texture,controller}.rs`, `anim.rs`, `kfm.rs`, `header.rs`, `import/mesh.rs`.

**High**:
- **H-1** — 678 of ~7,500 files (9.04%) parse truncated (return `Ok(NifScene{truncated: true})` with blocks silently dropped). 67,987 total blocks lost; median 37 per failing file, max 3,945. 138 files truncate at the root NiNode → empty render. `nif_stats` and `parse_real_nifs.rs` count truncation as success; the `MIN_SUCCESS_RATE = 0.95` gate only passes because of this semantic fudge. Real clean-parse rate ≈ 90.96%.
- **H-2** — 9 Oblivion-used block types have no parser: `NiBSBoneLODController` (34 files — cascades into creature animation loss), `NiColorData` (17), `NiFogProperty` (3), `NiFlipController`, `NiPathInterpolator`, `NiBoolTimelineInterpolator`, `bhkMultiSphereShape`, `bhkBlendController`. Terminal truncation (no `block_sizes` table in Oblivion). Stub with size-walk parsers + `oblivion_skip_sizes` hints would recover most.
- **H-3** — 309 truncations cite bogus `NiTransformData.KeyType` (ASCII chunks like `"cers"` / `"oamo"`) and `NiStringPalette` requesting 4.29 GB reads (0xFFFFFFFF). These are downstream symptoms of upstream stream-position drift; the named blocks are victims, not perpetrators.
- **H-4** — [parse_real_nifs.rs:74-78](crates/nif/tests/parse_real_nifs.rs#L74-L78) comment says "BSA v103 decompression is not yet implemented". Stale; v103 works, and the test would now live-abort via C-1.

**Low** — 3 representative interior meshes trace end-to-end cleanly:

| File                                      | Blocks | Meshes | Notes                                       |
|-------------------------------------------|--------|--------|---------------------------------------------|
| `meshes\lights\chandelier01.nif`          | 39     | 5      | Metal + candles + collision                 |
| `meshes\clutter\books\octavo01.nif`       | 18     | 2      | Pages + cover + bhkBoxShape                 |
| `meshes\creatures\imp\imp.nif`            | 97     | 1 skinned | 86-bone skeleton + alpha_test via NiAlphaProperty |

Failures concentrate on animation-heavy meshes (creatures, animated doors).

### Dim 6 — Blockers & Game-Specific Quirks

**Critical (documentation)**:
- **#1** — BSA v103 "NOT WORKING" claim is stale in `ROADMAP.md:778`, `.claude/commands/audit-oblivion.md:20`, game matrix row at ROADMAP.md:~817, and `parse_real_nifs.rs:74-78`.

**High (rendering-critical)**:
- **#2** — Legacy particle stack parses in `blocks/particle.rs` + `legacy_particle.rs` but **zero** `particle`/`Particle` references under `crates/renderer/src/` or `byroredux/src/`. Every Oblivion torch/fire/brazier/dust/ghost/spell FX renders invisible.
- **#3** — `CREA` + `ACRE` record types absent from ESM walker ([cell.rs:230-232](crates/plugin/src/esm/cell.rs#L230-L232) and [cell.rs:468](crates/plugin/src/esm/cell.rs#L468)). Anvil Heinrich Oaken Halls (statics-only) unaffected; dungeons and Arena matches lose creatures.
- **#4** — KF animation importer has explicit TODO at [controller.rs:922-924](crates/nif/src/blocks/controller.rs#L922-L924); `anim.rs::import_kf` only walks `NiControllerManager` / `NiControllerSequence` (Skyrim+ path). Oblivion/FO3/FNV's `NiSequenceStreamHelper` + per-bone `NiKeyframeController` chain **parses** but produces no clips. Scene-graph name resolution is already wired — only the KF reader path is missing.
- **#5** — LIGH DATA byte order (handoff from Dim 3 C2) — verified BGRA by Dim 3 byte-level sampling. Not just Oblivion.

**Low**: Pre-Gamebryo v3.3.0.13 inline-type-name fallback is already `log::debug!` at [lib.rs:148](crates/nif/src/lib.rs#L148) — no archive-sweep spam. Already resolved.

---

## Blocker Chain — "any new Oblivion interior cell renders end-to-end"

Execution order (each step unblocks or validates the next):

### Tier 0 — Process survival + trust restoration (day 1)
1. **Dim 5 C-1** — Add `stream.check_alloc(num_text_keys * size_of_text_key)` at [interpolator.rs:432](crates/nif/src/blocks/interpolator.rs#L432). Then audit the ~40 `Vec::with_capacity(file_u32)` sites. A `stream.allocate_vec::<T>(count)` helper fixes the class. **15 min – 2 h.**
2. **Dim 3 C2** — Swap `data[8]` ↔ `data[10]` for LIGH DATA color at [cell.rs:854-856](crates/plugin/src/esm/cell.rs#L854-L856). Cross-game fix. **3 lines.**
3. **Doc refresh** — Strike BSA v103 "NOT WORKING" from ROADMAP.md:778, .claude/commands/audit-oblivion.md:19-22, game matrix, `parse_real_nifs.rs:74-78`. **15 min.**

### Tier 1 — Rate restoration (week 1)
4. **Dim 5 H-2** — Stub parsers for `NiColorData`, `NiBSBoneLODController`, `NiFogProperty`; add remaining 6 to `oblivion_skip_sizes`. **~2-4 h.**
5. **Dim 5 H-3** — Debug-mode per-parser consumed-byte cross-check against `parsed_size_cache` to diagnose upstream drift; fix root-cause parsers. **~1 day.**
6. **Dim 5 H-1** — Change `truncated: true` from success to failure in `nif_stats` / `parse_real_nifs::record_success`. Lower `MIN_SUCCESS_RATE` or re-measure after #4/#5. **~30 min.**
7. **Re-measure**: target 98%+ clean-parse on Oblivion - Meshes.bsa.

### Tier 2 — Second-cell rendering (week 2)
8. **Dim 3 H2** — Add `b"CREA"` to MODL match arm ([cell.rs:230-232](crates/plugin/src/esm/cell.rs#L230-L232)) and `b"ACRE"` to REFR matcher ([cell.rs:468](crates/plugin/src/esm/cell.rs#L468)). **~2 h + test.**
9. **Dim 3 H4** — Parse XCLW into `CellData.water_height`; optional XCWT/XCMT/XCCM. **~10 lines + test.**
10. **Dim 3 C3** — Replace hardcoded `- 24` with `reader.variant().group_header_size()` across ~13 sites. Hardening, mechanical. **~1 h.**

### Tier 3 — Animated Oblivion content (weeks 3-4)
11. **Dim 6 #4** — Implement `import_kf` Path 3 for `NiSequenceStreamHelper` + `NiKeyframeController` chain. Scene-graph name resolution already wired in `anim_convert::build_subtree_name_map`. **1-2 days.**

### Tier 4 — Parity polish (parallel, weeks 3-6)
12. **Dim 4 C4-01** — Extend `GpuInstance` with `src_blend_factor`/`dst_blend_factor` u8 pair; map Gamebryo AlphaFunction → Vulkan blend factor in the pipeline selection or via dynamic state. **~1 day.**
13. **Dim 4 H4-01** — Add depth-state fields to `GpuInstance` (or pipeline cache key) and select pipeline accordingly. **~1-2 days.**
14. **Dim 4 H4-03,H4-04** — Extend `GpuInstance` with `glow_map_index`, `detail_map_index`, `gloss_map_index`, `decal_*_index`; surface them in the fragment shader. **~2-3 days** (3 shaders to keep in sync — see [feedback_shader_struct_sync.md]).
15. **Dim 6 #2** — M36-shaped particle subsystem: ECS component + instanced billboard renderer. **1-2 weeks.**

---

## Regression Guard List

These load-bearing patterns were verified still correct by this audit. Re-introducing any of them would be a regression:

1. **`NiTexturingProperty` trailing `num_shader_textures` = raw u32** (no `Has Shader Textures: bool` gate).
   Source: [properties.rs:311-312](crates/nif/src/blocks/properties.rs#L311-L312). Pinned by regression test at `:606`. Reverted #149 per commit `afab3e7`.

2. **`user_version` threshold `>= 10.0.1.8`** (not 10.0.1.0).
   Source: [header.rs:81](crates/nif/src/header.rs#L81). Test coverage: `parse_minimal_skyrim_header`, `accept_netimmerse_header` at [header.rs:314,407](crates/nif/src/header.rs).

3. **`BSStreamHeader` dual condition `version == 10.0.1.2 || user_version >= 3`** (not `user_version >= 10`).
   Source: [header.rs:104-110](crates/nif/src/header.rs#L104-L110). Regression test at `:431`.

4. **Strings table since `>= 20.1.0.1` (0x14010001)** — Oblivion 20.0.0.5 correctly inlines strings.
   Source: [header.rs:162](crates/nif/src/header.rs#L162).

5. **`stream.bsver()` is the in-file `user_version_2`, not the variant's hardcoded value.**
   All NiAVObject flag-width / properties-list / compact-material decisions ride on this.

6. **No `srgb_to_linear` on Gamebryo colors.**
   `NiMaterialProperty.ambient/diffuse/specular/emissive` and BSShader emissive/specular are raw monitor-space floats per [feedback_color_space.md] and commit `0e8efc6`. Verified by Dim 4 C4-02; `rg -l srgb_to_linear crates/` finds zero call sites on these fields.

7. **Pre-Gamebryo v3.3.0.13 inline-type fallback logs at `debug!`**, not `warn!`.
   Source: [lib.rs:148](crates/nif/src/lib.rs#L148).

8. **NIF `as_ni_node` walker unwraps 10 NiNode subclasses** (NiNode, BsOrderedNode, BsValueNode, BsMultiBoundNode, BsTreeNode, NiBillboardNode, NiSortAdjustNode, BsRangeNode, plus NiSwitchNode/NiLODNode in `switch_active_children`).
   Source: [walk.rs:34-66](crates/nif/src/import/walk.rs#L34-L66), `:77-102`.

---

## Suggested next action

`/audit-publish docs/audits/AUDIT_OBLIVION_2026-04-17.md`

Top candidates for issue filing (ordered by impact-per-fix-cost):

1. Dim 5 C-1 — OOM on `NiTextKeyExtraData` (Critical; 15-min fix).
2. Dim 3 C2 — LIGH BGRA color swizzle (Critical cross-game; 3-line fix).
3. Dim 6 #1 / Dim 2 L-3 — Doc refresh (BSA v103 blocker is stale).
4. Dim 3 H2 — Add CREA to MODL match arm.
5. Dim 5 H-2 — Missing Oblivion block-type parsers.
6. Dim 4 C4-01 — Generalize `BlendType::from_nif_blend` beyond 3 static pipelines.
7. Dim 6 #4 — KF importer Path 3 for `NiSequenceStreamHelper`.
