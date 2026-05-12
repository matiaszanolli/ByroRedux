---
description: "Per-game audit of Fallout New Vegas compatibility — reference title, ESM + cells + RT lighting"
argument-hint: "--focus <dimensions>"
---

# Fallout New Vegas Compatibility Audit

Deep audit of ByroRedux readiness for **Fallout: New Vegas** content. FNV is the **reference title** — the most validated end-to-end path in the engine. Audits here look for regressions and unshipped features more than missing foundations.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game data locations, methodology, deduplication rules, and finding format.

## Game Context

| Aspect              | State                                                                                   |
|---------------------|-----------------------------------------------------------------------------------------|
| NIF format          | v20.2.0.7 (BSVER 34)                                                                    |
| BSA format          | v104 ✓                                                                                  |
| ESM parser          | Long-tail dispatch closed — `unknown_records` bucket cleared by #808 (PROJ + EFSH + IMOD + ARMA + BPTD), #809 (REPU + EXPL + CSTY + IDLE + IPCT + IPDS + COBJ), #810 (31 long-tail records bulk-dispatched). Audit guard: any FNV record landing in `unknown_records` is a regression. |
| Parse rate          | 100.00% (14881 / 14881)                                                                 |
| Interior cells      | ✓ — Prospector Saloon: 809 entities, 48 FPS with full RT shadows + 25 point lights     |
| Exterior cells      | ✓ — WastelandNV 3×3 / 7×7 grid, M32 landscape, M33 sky/clouds, M34 sun                 |
| RT pipeline         | ✓ — M22 (shadows + reflections + 1-bounce GI) + M31.5 streaming RIS + M37.5 TAA        |
| Reference data      | `/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/`                       |

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 6.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/fnv`.
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Fallout New Vegas/Data/` exists (required — FNV is the baseline).

## Phase 2: Launch Dimension Agents (parallel)

### Dimension 1: NIF Parser — FNV Regression Guard
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/*.rs`, `crates/nif/tests/parse_real_nifs.rs`
**Checklist**: Every previously-fixed FNV bug stays fixed — reference N23.4 (Fallout 3/NV validation) and related issues. `NiTexturingProperty` decal-slot off-by-one. `BSMultiBound*` family. `BSDecalPlacementVectorExtraData`. Parse rate holds at 100%. Block histogram from `nif_stats` matches the expected distribution (if the histogram shifts meaningfully, a new block type is being mis-dispatched). `import_nif_scene` returns expected mesh counts on canonical cells (Prospector Saloon sub-meshes).
**Output**: `/tmp/audit/fnv/dim_1.md`

### Dimension 2: ESM Record Parser — Coverage & Accuracy
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/records/`, `crates/plugin/src/esm/cell.rs`
**Checklist**: All 23 record types still parse cleanly. Record counts on FalloutNV.esm match the M24 Phase 1 baseline (items 2643, containers 2478, LVLI 2738, LVLN 365, NPCs 3816, races 22, classes 74, factions 682, globals 218, settings 648). Spot-check specific records: Varmint Rifle stats, NCR faction relations, VATS AVIF entries. CELL XCLL fog_far_color optional field handling. FO4 additions (SCOL/MOVS/PKIN/TXST from session 10) don't inadvertently steal FNV dispatch (the `unreachable_patterns` warning in `cell.rs:211` is a code smell to investigate).
**Output**: `/tmp/audit/fnv/dim_2.md`

### Dimension 3: Cell Loading End-to-End
**Subagent**: `general-purpose`
**Entry points**: `byroredux/src/cell_loader/{load,unload,exterior,references,spawn,partial,refr,terrain,water}.rs` (cell_loader.rs is thin re-export), `byroredux/src/scene/{nif_loader,world_setup}.rs` (scene.rs is thin re-export)
**Checklist**: Interior cell load — Prospector Saloon entity count, XCLL lighting, NiAlphaProperty decal routing. Exterior 7×7 grid load from WastelandNV — LAND terrain mesh, LTEX/TXST splatting, WTHR→CLMT→WTHR resolution chain, M33 cloud texture resolution through `TextureProvider`. Reference count consistency across multiple cell loads. `CachedNifImport` Arc cache prevents duplicate parsing (session 6). `CellLoadResult` exposes WeatherRecord for `scene/world_setup.rs` consumption. Watch for memory leaks across cell unload/load cycles. M38 water-plane spawn from cell water references — verify `cell_loader/water.rs` spawns WaterPlane components and `submersion_system` writes camera state on entry.
**Output**: `/tmp/audit/fnv/dim_3.md`

### Dimension 4: RT Lighting Pipeline — FNV Scenes
**Subagent**: `renderer-specialist`
**Entry points**: `crates/renderer/src/vulkan/acceleration.rs`, `crates/renderer/shaders/triangle.frag`, `crates/renderer/shaders/composite.frag`
**Checklist**: TLAS frustum culling correctness — no lights dropped for in-view fragments. Streaming RIS (M31.5) — 8 reservoirs/fragment from full cluster, unbiased W estimator, 64× clamp engaged. Shadow ray budget caps. Distance-based shadow / GI ray fallback. BLAS compaction (M36) — occupancy query succeeds, compact copy replaces original. BLAS LRU eviction at the 1 GB budget. SVGF temporal accumulation uses motion vectors + mesh_id disocclusion. TAA Halton jitter + YCoCg variance clamp + luma blend α=0.1. M33 sky gradient + cloud layer blends correctly with tone-mapped geometry.
**Output**: `/tmp/audit/fnv/dim_4.md`

### Dimension 5: Real-Data Validation
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, demo CLI invocations
**Checklist**: Run `cargo run -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior --bsa Meshes.bsa --textures-bsa Textures.bsa --textures-bsa Textures2.bsa --debug` and capture `/cmd stats` at T+3s. Compare entity / mesh / texture / draw-call counts vs roadmap numbers (809 entities, 784 draws at 48 FPS target). Run exterior: `--grid <x>,<y> --radius 3` for a WastelandNV cell. Capture screenshots for visual regression baseline. Validate `tex.missing` + `tex.loaded` debug commands return sensible output (session 10).
**Output**: `/tmp/audit/fnv/dim_5.md`

### Dimension 6: Animation & Skinning (FNV) + M41 NPC Spawn Long-Tail
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/anim.rs` (+ `anim/types.rs`, `anim/tests.rs`), `crates/core/src/animation/`, `byroredux/src/anim_convert.rs`, `byroredux/src/npc_spawn.rs`
**Checklist**: `.kf` file loading from BSA (`--kf meshes/anim.kf`). AnimationClipRegistry populated correctly. NiTransformInterpolator + NiFloatInterpolator + NiBoolInterpolator channels sample correctly. Text key events collected from NiTextKeyExtraData. Cycle types Clamp / Loop / Reverse all honored. KFM state machine parser. FixedString interning at clip load time (#340) — no per-frame StringPool locks. Skinning data extraction from NiSkinData sparse weights — ready for M29 GPU skinning. #178 SkinnedMesh palette computed correctly.
**M41.0 long-tail regression guards (Session 29)**:
- B-spline pose-fallback (#772, 3c32a5e): gated on a `FLT_MAX` sentinel. Without the gate, NPCs vanish under FNV `BSPSysSimpleColorModifier` particle stacks that share keyframe time-zero with the actor's animation player. **Note**: B-splines (`NiBSplineCompTransformInterpolator`) ARE reachable on FNV/FO3 (`feedback_bspline_not_skyrim_only.md`) — do not rule them out by game era.
- AnimationClipRegistry dedup (#790, da99d15): registry deduplicates by lowercased path so cell streaming doesn't grow it unboundedly. Without dedup, one full keyframe set leaks per cell load (observable as steady RAM growth on exterior streaming).
- NPC hand-mesh load (#793 / M41-HANDS, da8d7e2): `lefthand.nif` + `righthand.nif` loaded alongside `upperbody.nif` on kf-era NPCs. Audit any NPC body assembly that loads only `upperbody` — every Doc Mitchell, Sunny Smiles, Megaton dweller would otherwise render with no hands.
**Output**: `/tmp/audit/fnv/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/fnv/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_FNV_<TODAY>.md` with structure:
   - **Executive Summary** — FNV is the baseline. Any regressions against the roadmap's validated numbers are CRITICAL.
   - **Dimension Findings** — Grouped by severity per dimension.
   - **Baseline Comparison Table** — Roadmap number vs observed number for entity count, draw count, FPS, parse rate, record count.
   - **Regression Guard List** — Previously-fixed issues this audit verified are still correct.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_FNV_<TODAY>.md`
