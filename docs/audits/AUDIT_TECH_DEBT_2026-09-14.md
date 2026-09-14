# Tech-Debt Audit — 2026-09-14

**Scope**: all 9 dimensions, `--depth deep`. Eight dimension agents (Dim 5 triaged by the
orchestrator); every HIGH/MEDIUM and every headline claim spot-verified against the live
tree before merge.
**Commit at audit time**: `358999c40` (branch `main`), 112 commits after the 2026-09-11 audit.
**Dedup baseline**: 677 `tech-debt`-labelled issues (all states; 16 open) + 125 open issues
of every label + `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md` and same-week sibling reports.
**Un-owned subsystems reached**: gameplay slice (combat AI, `FactionRelations`), `crates/sdk`,
`crates/mod-runtime`, launcher crates + tools (Dim 6/8 sweeps), `crates/facegen`, `crates/hkx`,
`crates/debug-server` / `crates/debug-protocol`. FSR3 FFI was not specifically targeted.

## Executive Summary

**57 new findings: 0 CRITICAL, 1 HIGH, 3 MEDIUM, 53 LOW.** In addition, 13 open issues were
re-verified (not re-filed). One of them, #4215, has a partly stale premise and should be
narrowed.

| Dim | Topic | HIGH | MED | LOW | Kind |
|---|---|---:|---:|---:|---|
| 1 | Complexity | – | 1 | 4 | tech-debt |
| 2 | Duplication | – | – | 8 | tech-debt |
| 3 | Stale docs | – | 1 | 11 | doc-rot |
| 4 | Audit-finding rot | – | – | 10 | doc-rot |
| 5 | Stale markers | – | – | – | — |
| 6 | Stubs | – | – | 5 | tech-debt (TD6-008 doc-rot) |
| 7 | Magic numbers | 1 | 1 | 4 | tech-debt |
| 8 | Dead code | – | – | 9 | tech-debt |
| 9 | Test hygiene | – | – | 2 | test-gap |

By kind: 32 `tech-debt`, 23 `doc-rot`, 2 `test-gap`.

Three things stand out this cycle.

1. **The audit's own measuring stick was broken.** Dim 1's `prod_loc` awk helper
   (Phase 1, step 5) counts `{`/`}` inside string literals, so it misreads files whose
   test modules contain format strings or source-scanning `split("\n}\n")` calls
   (**TD1-001**, MEDIUM). This run's Phase-1 baseline reported 4 files over 2000
   production LOC, with `crates/scripting/src/translate/effects.rs` as a "new crossing".
   A string-aware counter gives **3**: effects.rs is really 1681. The same bug hides
   67 real lines in `crates/renderer/src/vulkan/context/draw.rs`, which is actually
   **43 lines** from threshold, not 110. The baseline snapshot below uses the corrected
   figures.
2. **The only HIGH is a GPU lockstep literal in new ground-cover code** (**TD7-001**).
   The blade SSBO is sized with a bare `16` that must equal the std430 stride of the
   GLSL-only `GroundCoverBlade` struct. The shader-contract table classifies that struct
   `ShaderLocal`, which checks only that no Rust mirror exists. It is latent: it fires
   the day someone adds a field, which the struct's own doc comment weighs as an option.
   The same commit window cut `GROUNDCOVER_MAX_CHUNKS` to 256 with a silent,
   origin-ordered truncation (**TD7-002**, MEDIUM).
3. **Doc and skill rot is where most of the volume is, and much of it is one week old.**
   It comes from the 2026-09-09..09-12 splits (`boot/`, `asset_provider/material/`,
   `texture_registry/`) and from features that landed in the last three days (Skyrim
   CHARAL ruleset wiring, NPC combat AI, Skyrim LE hkx, ambient AI re-selection). Two
   structural gaps keep producing it:
   - The path gate does not scan fenced code blocks (**TD4-005**). CLAUDE.md's Workspace
     Structure and `_audit-common.md`'s Project Layout both live in fences, and both are
     stale (**TD3-003**, **TD4-004**).
   - The `GpuMaterial` 432→428 B rot is **still unfixed**. #4211 was closed as a
     duplicate of #4114, which is open with zero sites corrected. A fifth struct-size
     rot of the same class appeared for `GpuTerrainTile` (**TD3-001**, MEDIUM).

**Engine hygiene still holds.** `unimplemented!`/`todo!()` is 0, and there are 0 live
markers: all 22 raw hits are documented exclusions. `cargo machete` is clean, every
`#[ignore]` carries a reason, 0 of 513 `include_str!` source scans are vacuous, and none of
last cycle's thirteen closed fixes has regressed (#4216, #4220–#4226 and #4030 verified
individually).

### Delta vs the 2026-09-11 baseline

| Metric | 2026-09-11 | 2026-09-14 | Note |
|---|---|---|---|
| Markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE) | 22 | 22 | 0 live; identical exclusion classes |
| `allow(dead_code)` (raw grep lines) | 29 | 27 | includes prose mentions; see Dim 8 |
| `unimplemented!` / `todo!()` | 0 | 0 | |
| `#[ignore]` (all forms, crates+byroredux+tools) | 189 | 194 | +5, all "needs <game> data" gates (Skyrim LE etc.); bare form 0 |
| Files >2000 **production** LOC | 2 | **3** (awk helper: 4) | `crates/nif/src/blocks/shader.rs` re-crossed 09-12; effects.rs is a false positive (TD1-001) |
| Total LOC >2000 (secondary bucket) | 39 | 43 | |
| Functions >200 production LOC | — | 148 | vs 135 on 2026-09-04 with the same string-aware scanner |
| Open `tech-debt`-labelled issues | 9 | 16 | includes cross-audit filings (REN/SF/SAFE/ECS) |

## Baseline Snapshot

```
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 22   (0 live)
allow(dead_code):                              27
unimplemented!/todo!():                         0
#[ignore] tests (crates+byroredux+tools):     194   (bare form: 0)
production LOC >2000 (string-aware counter):    3   (skill's awk prod_loc reports 4 — TD1-001)
total LOC >2000 (secondary bucket):            43
functions >200 production LOC:               148
open tech-debt-labelled issues:                16
```

Primary bucket (string-aware production LOC): `crates/renderer/src/vulkan/context/mod.rs`
**2882** (+51, #4217/#3736) · `crates/sdk/src/compatibility/storage_util.rs` **2160**
(unchanged, #4218) · `crates/nif/src/blocks/shader.rs` **2058** (re-crossed, TD1-002).
Near threshold: `crates/plugin/src/esm/records/actor/mod.rs` 1979,
`crates/renderer/src/vulkan/context/draw.rs` 1957, `byroredux/src/cornell.rs` 1903.

## Top 10 Quick Wins (trivial/small effort)

1. **TD7-003** — define `GROUNDCOVER_MAX_BLADES_PER_CHUNK` as
   `GROUNDCOVER_SCATTER_WORKGROUP * GROUNDCOVER_CANDIDATES_PER_THREAD`; a one-line
   const expression retires a prose invariant.
2. **TD9-001** — set the Skyrim `RosterCase` to `derived_rows: Some(2)` so the CHARAL
   real-data gate stops panicking and reaches the Oblivion case.
3. **TD3-001** — correct `GpuTerrainTile` 96→144 B in `docs/engine/exal-groundcover.md`
   and `docs/engine/memory-budget.md`.
4. **TD9-002** — add a `cargo test -p byroredux-spt --features recon --lib` step; five
   value-asserting tests have never run in CI.
5. **TD2-001** — replace volumetrics' five hand-rolled GENERAL write→read barriers with
   the `image_barrier_general_write_to_read` helper the file already imports.
6. **TD2-008** — make scripting's `drain<T>` / `snapshot<T>` / `append_scene_completions`
   `pub(crate)` in `scene/playback.rs` and delete the dialogue/package copies.
7. **TD3-003** — fix CLAUDE.md's three split-file entries (`material/`, `tests/`,
   `texture_registry/`).
8. **TD4-001** — strike the skill's false "`shader.rs` dropped back under / do not
   re-propose" clause and the "2 files / every monolith is split" sentences.
9. **TD6-005** — add `SetInChargen` + `SetHudCartMode` to `Effect::is_placeholder`, so
   MQ101 coverage stops counting write-only effects as real.
10. **TD4-009 / TD4-011** — two one-line skill fixes that currently point auditors at
    already-closed gaps (#3814) or a disproved fog premise.

## Top 5 Medium Investments

1. **TD7-001 + TD7-002** — give `GroundCoverBlade` a `#[repr(C)]` Rust mirror sized with
   `size_of`, reclassify it `Guarded` in the contract table, and tie
   `GROUNDCOVER_MAX_CHUNKS` to draw distance / chunk size with a test, a
   `chunks_truncated` stat and nearest-first truncation.
2. **TD1-001 + TD4-005** — repair the audit tooling that misled this run: a string- and
   comment-aware production-LOC counter checked in under `tools/` with fixtures, and a
   fence-aware path gate. Together they close the two blind spots behind most of this
   report's LOW volume.
3. **#4114 (TD3-002) + a workspace size-claim scanner** — do the ~26-site 432→428
   sweep, then extend the `bindings_glsl_contract_pin` idea into a
   `workspace_hygiene_tests.rs` scan that covers every pinned `Gpu*` struct's size
   claims, including `GpuTerrainTile` (TD3-001).
4. **TD1-004** — land the per-family helper extraction #3854 prescribed for
   `apply_effect` (519→680 LOC in four days, cognitive complexity 71), keeping helpers in
   flat files listed in `SOURCES`.
5. **TD2-003 + TD2-004 + TD2-005** — three shared GLSL includes/constants:
   `phase_functions.glsl` (restores #1021's g-clamp on the cloud copy),
   `froxel_slices.glsl` (four copies of the slice curve, with a drifted Rust mirror), and
   a generated `LUMA_REC709` (~20 hand copies).

## Findings

### HIGH

#### TD7-001: The ground-cover blade buffer is sized by a bare Rust `16` that must equal the std430 stride of the GLSL-only `GroundCoverBlade` struct
- **Severity**: HIGH (magic number that silently overflows under a documented design change) · **Dimension**: 7 · **Kind**: tech-debt · **Effort**: small
- **Location**: `crates/renderer/src/vulkan/groundcover.rs:534-537` (allocation), `:1741-1756` (`gpu_records_match_their_std430_layout`, arithmetic only), `crates/renderer/shaders/include/groundcover_scene.glsl:65-80` (struct), `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:1703` (`("GroundCoverBlade", ShaderLocal)`)
- **Status**: NEW · **Age**: literal `637b652647` (2026-09-06); `ShaderLocal` class `3c16c42e70` (2026-09-08)
- **Finding**: `blade_bytes = GROUNDCOVER_MAX_CHUNKS × GROUNDCOVER_MAX_BLADES_PER_CHUNK × 16`. The `16` is the stride of `GroundCoverBlade { uint packedXZ; float baseY; uint seedSpecies; float dGround; }`, written by `groundcover_scatter.comp` (`gcBlades[chunkIdx * pc.bladesPerChunk + slot]`) and read by `groundcover_blade.vert`. It has no Rust mirror. `MirrorClass::ShaderLocal` only asserts that no Rust struct exists, so nothing compares the size. The sibling Cell/Chunk/Species records are all `size_of`-pinned; the blade record is the one exception. The struct's own doc (`groundcover_scene.glsl:57-64`) weighs extra terms that "would roughly double this record".
- **Impact**: If the record grows, the runtime array holds `16 MiB / stride` entries while the host dispatches `256 × 4096` slots. The high chunk slices read and write past the SSBO end: UB without `robustBufferAccess`, silently dropped with it, and not flagged by default validation. It shows up as missing or corrupt grass with an empty log. This is the lockstep-drift class in `feedback_shader_struct_sync.md`. (Verified by the orchestrator: the struct has exactly four 4-byte fields, and `grep GroundCoverBlade crates/renderer/src` finds only the contract-table row.)
- **Suggested Fix**: Add `#[repr(C)] GpuGroundCoverBlade { packed_xz: u32, base_y: f32, seed_species: u32, d_ground: f32 }`, size `blade_bytes` with `size_of`, pin it in `gpu_records_match_their_std430_layout`, and reclassify the contract entry `Guarded` so the std430 field check compares both sides.

### MEDIUM

#### TD1-001: Dimension 1's `prod_loc` helper counts braces inside string literals, so its production-LOC numbers are wrong in both directions
- **Severity**: MEDIUM (stale audit baseline that misled an audit — this one) · **Dimension**: 1 · **Kind**: tech-debt (audit infrastructure) · **Effort**: small
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:92-116` (the awk body, Phase 1 step 5)
- **Status**: NEW · **Age**: helper added under #3081; first false positive 2026-09-14
- **Finding**: Inside a `#[cfg(test)]` block the awk tracks depth by counting every `{`/`}` on the line without stripping string/char literals or comments. Test format strings (`"{call}"`) and source-scan splits (`split("\n}\n")`) close the block early or never. Measured against a string-aware counter: `crates/scripting/src/translate/effects.rs` 2093 vs **1681** (+412; its `#[cfg(test)]` is at `:1682`, confirmed); `crates/renderer/src/vulkan/groundcover.rs` 1966 vs 1680 (+286); `crates/nif/src/blocks/shader.rs` 2071 vs 2058; `crates/renderer/src/vulkan/context/draw.rs` 1890 vs **1957** (−67). It also only matches `#[cfg(test)]` at column 0.
- **Impact**: It produced a false "NEW crossing" that would have filed a split for a file 319 lines under threshold, and it under-reports the repo's most-regrown file.
- **Suggested Fix**: Strip `//`, `/* */`, `"…"`, `r#"…"#` and char literals before counting. Better: move the helper to a checked-in script under `tools/` with fixtures for the effects.rs and draw.rs shapes, then re-derive the Phase-1 orientation figures (see TD4-001).

#### TD3-001: `GpuTerrainTile` grew 96 → 144 B (#4057), but two engine docs still say 96 B and name a test that no longer exists
- **Severity**: MEDIUM (GPU-struct size drift) · **Dimension**: 3 · **Kind**: doc-rot · **Effort**: trivial
- **Location**: `docs/engine/exal-groundcover.md:769-771`, `docs/engine/memory-budget.md:96`
- **Status**: NEW · **Age**: `b01ef9260` (2026-09-06)
- **Finding**: `exal-groundcover.md` says "96 bytes of `uint[8] × 3`, pinned by *gpu_terrain_tile_is_96_bytes* and by `ArrayStride 96`", and `memory-budget.md` gives 96 B / ~96 KB. The live pin is `gpu_terrain_tile_is_144_bytes` (`crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:327`). `exal-groundcover.md:1129` contradicts its own §11.1.
- **Suggested Fix**: Say "was 96 B; now 144 B, pinned by `gpu_terrain_tile_is_144_bytes`", update the budget row to ~144 KB, and fold `GpuTerrainTile` into the size-claim scanner proposed under Medium Investment 3.

#### TD7-002: `GROUNDCOVER_MAX_CHUNKS` cut to 256 with only a prose headroom argument; the host truncates at the cap silently and in origin order
- **Severity**: MEDIUM · **Dimension**: 7 · **Kind**: tech-debt · **Effort**: small
- **Location**: `crates/renderer/src/shader_constants_data.rs:290-296`, `byroredux/src/render/groundcover.rs:83` (origin sort), `:94` (silent `return`), `crates/renderer/src/vulkan/groundcover.rs:1006` (second clamp), `:23-31` (module doc)
- **Status**: NEW · **Age**: `673b21458` (2026-09-13, 1024 → 256)
- **Finding**: The cap is justified only in prose ("~67 in reach … ~4× headroom"), and the module doc itself says it "is no longer enough for §11.2's chunk-size sweep". Nothing ties it to `GROUNDCOVER_DRAW_DISTANCE` / `GROUNDCOVER_CHUNK_UNITS`. The gather sorts by `origin_xz` and returns at the cap, with no stat (`GroundCoverStats` has no truncated counter). The disturber list in the same file *is* sorted nearest-first "because the renderer truncates".
- **Impact**: Halving chunk size (the documented sweep) or raising draw distance past ~4 000 units drops every chunk past the 256th in west-to-east order. Grass vanishes on one side of the camera with nothing logged. Memory stays in bounds, which is why this is not HIGH.
- **Suggested Fix**: Add a test that the ceiling of `π(draw + bound)² / chunk²` is ≤ `GROUNDCOVER_MAX_CHUNKS`, add a `chunks_truncated` stat, and truncate nearest-first.

### LOW

#### Dimension 1 — Complexity

##### TD1-002: `crates/nif/src/blocks/shader.rs` re-crossed 2000 production LOC; no split axis recorded
- **Location**: `crates/nif/src/blocks/shader.rs` (2058 prod / 2073 total; tests already external in `crates/nif/src/blocks/shader_tests/`) · **Status**: NEW · **Age**: 1975 on 09-11 → crossed via `ae088b2ab`, `797e82124`, `37b2178b6` (all 2026-09-12) · **Effort**: medium · **Kind**: tech-debt
- **Finding**: The file holds four block families behind one shared head: legacy FO3/FNV `BSShader*Property` (`:47-411`), Skyrim+ sky/water + `BSShaderTextureSet` (`:502-636`), `BSLightingShaderProperty` + per-game parsers + `ShaderTypeData` (`:637-1706`, ~1070), and `BSEffectShaderProperty` (`:1707-2046`). No single function exceeds 200 LOC, so this is file cohesion, not function size. Every growth commit this week touched a single family.
- **Suggested Fix**: Split into a *shader/* directory **by block family**: mod (shared head `parse_skyrim_shader_base`, `read_starfield_tail`, `is_material_reference`, the `NiObject` impls, re-exports), legacy, sky_water, lighting, effect. Do not split per game, which would scatter one struct's impl across five files. No `include_str!` scans target the file today.

##### TD1-004: `apply_effect` grew 519 → 680 LOC in four days; the per-family helper extraction #3854 prescribed never landed
- **Location**: `crates/scripting/src/fragment/effects.rs:626-1305` · **Status**: Regression of #3854 (only the file split landed, `f5127c1cc`) · **Age**: `11bed5111`, `d0dac91b1`, `f61ea0447`, `5162829a3`, `37ed10277` (09-12..09-14) · **Effort**: medium · **Kind**: tech-debt
- **Finding**: A 29-arm `match effect`; cognitive complexity 71/25, nesting 6. New MQ101 arms `SetLocked` (72 lines) and `SetLockLevel` (39) arrived after the split. It carries the nested-lock contract doc (#3493/#3949).
- **Suggested Fix**: Make each arm a one-line delegate to `apply_<family>_effect`: globals, inventory, placement/enable, lock ledger, scene, player control & chargen, vehicle-cinematic, AI/combat. Helpers must stay in `crates/scripting/src/fragment/effects.rs` or in flat sibling files added to `SOURCES`, because `every_production_file_is_in_the_sources_concat` does not recurse. Keep every acquired lock named in `apply_effect`'s doc block (#3949 scan).

##### TD1-005: `setup_scene` is 1056 lines with nesting depth 10, and has regrown past its 2026-08 size
- **Location**: `byroredux/src/scene.rs:720-1775` · **Status**: NEW · **Age**: 760 on 08-20 → 996 on 09-04 → 1056; latest jump `7019eb84b` (09-08) · **Effort**: medium · **Kind**: tech-debt
- **Finding**: One body does harness decode → content-source chain (Cornell / ESM / loose NIF, `:774-1099`) → `--kf` → demo primitives → camera spawn → player-mode selection + ground probe (`:1367-1593`) → SSBO/descriptor finalize → `--menu` launch. The phases share six mutable locals.
- **Suggested Fix**: Extract in place, in order: `load_scene_content` → `SceneContent`, `start_cli_animation`, `spawn_initial_camera`, `select_and_spawn_player` (keep #2375's probe-before-mode ordering inside it), `finalize_scene_gpu_buffers`, `launch_archive_menu`.

##### TD1-006: #3858's census functions kept growing after closure — `render_one_frame` +148 LOC (+22%) in nine days
- **Location**: `byroredux/src/app_frame.rs:49-861` (813 LOC, nesting 7); also `about_to_wait` (822), `record_skinned_blas_refit` (912), `collect_static_mesh_draws` (947) · **Status**: NEW (follow-up to closed #3858) · **Age**: `637b65264`, `40b5c5b6a`, `0025d8221`, `1a7a22cf0` · **Effort**: medium · **Kind**: tech-debt
- **Finding**: Each EXAL/SKYAL feature adds its per-frame hook inline. The ground-cover collection (`:274-391`) and Ruffle UI tick (`:392-535`) alone are ~260 lines.
- **Suggested Fix**: Extract `collect_groundcover_frame`, `tick_ui_overlay`, `drain_skin_slot_uploads`, `apply_pending_debug_requests` and `reconcile_post_draw_skin_state` as `App` methods **inside `byroredux/src/app_frame.rs`**: five `include_str!("app_frame.rs")` scans would go vacuous if the code moved files. File `record_skinned_blas_refit` / `collect_static_mesh_draws` separately with the RenderDoc caveat.

#### Dimension 2 — Duplication

##### TD2-001: `volumetrics.rs` hand-rolls five copies of `image_barrier_general_write_to_read`, which it imports
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:1201-1263` (×5); helper `crates/renderer/src/vulkan/descriptors.rs:270` · **Status**: NEW · **Age**: `5d3625416` (07-28) → `c98436b72`/`2325c1de4`/`2155cc917`/`e56d7654d` (08-18); all later than the helper · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: Each transported combustion field copied the previous barrier literal; the file calls the helper at `:1306`/`:1357`. The subresource is `color_subresource_single_mip()`, so the fields match exactly.
- **Suggested Fix**: Call the helper five times. Optionally add `image_barrier_general_read_to_write` for the paired pre-write barriers.

##### TD2-002: SKYAL's `cloud_noise.rs` re-implements volumetrics' R8 density-noise upload; both hand-roll the TRANSFER_DST→SHADER_READ publish barrier
- **Location**: `crates/renderer/src/vulkan/cloud_noise.rs:82-244` (barrier `:209-218`), `crates/renderer/src/vulkan/volumetrics/init.rs:880-903`, `:1003-1070` (barrier `:1047-1055`); helper `crates/renderer/src/vulkan/descriptors.rs:449-466` · **Status**: NEW · **Age**: `564d0d2fe` (09-13) · **Effort**: small · **Kind**: tech-debt
- **Finding**: Same memoized payloads, extents, `R8_UNORM` format and staging/copy sequence. The VRAM images are also allocated twice.
- **Suggested Fix**: Add `volumetrics::noise::record_density_noise_upload` + `create_density_noise_staging` using `image_barrier_transfer_dst_to_shader_read`. Follow-up: have volumetrics sample `CloudNoiseVolumes` instead of owning a second pair.

##### TD2-003: Henyey-Greenstein duplicated in `clouds.glsl` and `volumetrics_inject.comp`; the new copy lacks #1021's g-clamp
- **Location**: `crates/renderer/shaders/include/clouds.glsl:61-65`, `crates/renderer/shaders/volumetrics_inject.comp:1329-1343` · **Status**: NEW · **Age**: cloud copy `c379898fd` (09-13); clamp `8ac5b91cf` (#1021, 05-14) · **Effort**: small · **Kind**: tech-debt
- **Finding**: Divergent fix history (the inject copy clamps `g` to ±0.999 and uses `PI`; the cloud copy has neither, verified). Not promoted to MEDIUM because the cloud `g` is currently a compile-time constant ≤ `CLOUD_PHASE_G0/G1`.
- **Suggested Fix**: A guarded *include/phase_functions.glsl* (new) with the clamped HG, called from both; add it to `crates/renderer/build.rs`'s dependency list.

##### TD2-004: The hybrid froxel Z-slice mapping is copied four times across three shaders, and its Rust mirror has drifted clamps
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp:1345-1367`, `crates/renderer/shaders/volumetrics_integrate.comp:49-59`, `crates/renderer/shaders/composite.frag:130-140`; Rust `crates/renderer/src/vulkan/volumetrics.rs:422-455` · **Status**: NEW · **Age**: `5d3625416` (07-28) · **Effort**: small · **Kind**: tech-debt
- **Finding**: The copies are body-identical (only UBO member names differ). The Rust mirror floors at `1.0e-4` where every shader floors at `1.0`, so it can't catch shader drift.
- **Suggested Fix**: A guarded *include/froxel_slices.glsl* (new) exposing `froxelSliceDistance`/`froxelSliceCoordinate`; align the Rust floors or document why they differ.

##### TD2-005: Rec. 709 luminance is written out ~20 times
- **Location**: `crates/renderer/shaders/svgf_atrous.comp:72-74`, `crates/renderer/shaders/svgf_temporal.comp:60-62`, `crates/renderer/shaders/include/pbr.glsl:417-419`, `crates/renderer/shaders/volumetrics_inject.comp:281`, plus inline in `taa.comp`, `composite.frag`, `water.frag`, `presentation.frag`, `include/lighting.glsl` and `triangle.frag` (×6) · **Status**: NEW · **Effort**: small · **Kind**: tech-debt
- **Finding**: The inject shader claims agreement with `byroredux_core::radiometry::linear_srgb_luminance`, but every other consumer retypes the weights; the two svgf `luminance()` functions are byte-identical. All consumers already include the generated header.
- **Suggested Fix**: Emit `LUMA_REC709` from `crates/renderer/src/shader_constants_data.rs`, sourced from `crates/core/src/radiometry.rs`, and replace every copy.

##### TD2-006: Ground-cover scatter and its bench carry byte-identical candidate generators
- **Location**: `crates/renderer/shaders/groundcover_scatter.comp:155-169` (`gcCandidate`), `crates/renderer/shaders/include/groundcover_bench.glsl:148-164` (`benchCandidate`) · **Status**: NEW · **Age**: `637b65264` / `40b5c5b6a` (09-06) · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: The bench exists to time the production distribution, but it re-types it. #4057 already moved the Laplacian into a shared include for the same reason.
- **Suggested Fix**: A guarded *include/groundcover_candidate.glsl* (new) with `byroGcCandidate`, included by both.

##### TD2-007: `descriptors.rs` has no layered GENERAL→SHADER_READ helper, so the sky cube bake hand-rolls it and the ground-cover bench grew a private builder
- **Location**: `crates/renderer/src/vulkan/sky_cube.rs:463-505`, `crates/renderer/src/vulkan/groundcover_bench.rs:1181-1206`; family `crates/renderer/src/vulkan/descriptors.rs:404-466` · **Status**: NEW (same gap class as closed #4221, for layered images) · **Age**: `b54b86b7b`/`6db9eac2e` (09-13), `40b5c5b6a` (09-06) · **Effort**: small · **Kind**: tech-debt
- **Suggested Fix**: Add `image_barrier_general_to_shader_read_layers(image, layer_count)` with the single-layer helper delegating to it; migrate both sites and delete the bench builder.

##### TD2-008: scripting's `snapshot<T>` / `drain<T>` / `append_scene_completions` are verbatim copies in `dialogue.rs` and `package.rs` (a third `drain` in `scene/playback.rs`)
- **Location**: `crates/scripting/src/dialogue.rs:170-189`, `:225-243`; `crates/scripting/src/package.rs:299-339`; `crates/scripting/src/scene/playback.rs:199-207` · **Status**: NEW · **Age**: `022cf421d` (08-01) · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: The completion-batch merge rule decides when a scene action counts as done; a fix to one copy would leave the other runtime on the old rule.
- **Suggested Fix**: Make all three `pub(crate)` in `scene/playback.rs` beside `SceneActionCompletionBatch` and delete the copies.

#### Dimension 3 — Stale Documentation

##### TD3-003: CLAUDE.md's Workspace Structure names three files that were split into directories
- **Location**: `CLAUDE.md:68` (*asset_provider/material.rs*), `:70` (*asset_provider/tests.rs*), `:152` (*texture_registry.rs*) · **Status**: NEW · **Age**: `42f0ead40` (09-09), `196a2faa3` (09-11) · **Effort**: trivial · **Kind**: doc-rot
- **Suggested Fix**: Point them at `byroredux/src/asset_provider/material/`, `byroredux/src/asset_provider/tests/` and `crates/renderer/src/texture_registry/`. The tree is fenced, so the gate can't see it (TD4-005).

##### TD3-004: docs/engine still points at *byroredux/src/boot.rs* after the `boot/` split (3 broken links, 8 present-tense sites)
- **Location**: broken links `docs/engine/launcher.md:6,195`, `docs/engine/physics.md:94`; present-tense `docs/engine/m47-2-design.md:248`, `docs/engine/npc-spawn-ai-packages.md:188,507,513`, `docs/engine/packal.md:166`, `docs/engine/save-load-roundtrip.md:111`, `docs/engine/pipeline-overview.md:124,142` · **Status**: NEW (skill-file sites are #4174) · **Age**: `8c5e02aab` (09-09) · **Effort**: small · **Kind**: doc-rot
- **Suggested Fix**: Re-point to `byroredux/src/boot/world.rs`, `byroredux/src/boot/cli.rs`, `byroredux/src/boot/schedule/post_update.rs`, `byroredux/src/boot/schedule/update.rs` and `byroredux/src/boot/schedule/mod.rs` as mapped in the Dim 3 evidence. Cite functions, not line numbers.

##### TD3-005: Other pre-split module paths in docs/engine
- **Location**: `docs/engine/asset-pipeline.md:62,275` (broken links to *asset_provider/material.rs*), `docs/engine/coordinate-system.md:246` and `docs/engine/lighting-from-cells.md:57` (*references.rs*), `docs/engine/esm-records.md:531` (*actor.rs*) · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Suggested Fix**: Re-point to `material/provider.rs`, `material/merge.rs`, `cell_loader/references/` and `records/actor/`.

##### TD3-006: Every file:line anchor in `pipeline-overview.md` predates the #2731 and boot splits
- **Location**: `docs/engine/pipeline-overview.md:25-34,124,137-154`; `docs/engine/launcher.md:123` · **Status**: NEW · **Age**: since `7a4cb781f` (08-13) · **Effort**: small · **Kind**: doc-rot
- **Finding**: For example, `main.rs:1035` about_to_wait is now `byroredux/src/app_events.rs:518`, and `main.rs:378` render_one_frame is now `byroredux/src/app_frame.rs:49`; see Dim 3's full anchor table.
- **Suggested Fix**: Replace line anchors with `file::function` anchors and refresh the currency note.

##### TD3-007: The Skyrim CHARAL ruleset was wired by #3848, but feature-matrix, charal.md and the corpus-test comments still say "unwired"
- **Location**: `docs/feature-matrix.md:251,259-262,330`, `docs/engine/charal.md:352`, `crates/plugin/tests/parse_real_esm.rs:382-383,490-491` · **Status**: NEW · **Age**: `e13985dfc` (09-12) · **Effort**: small · **Kind**: doc-rot · **Related**: TD9-001 (the assertion half)
- **Suggested Fix**: Mark the Skyrim SE cell ✓, rewrite the gap prose to cover Oblivion only, and drop the #3170 aside.

##### TD3-008: NPC combat AI made two "the player is the only HitEvent producer" claims false
- **Location**: `crates/core/src/ecs/components/creature_attack.rs:14-19`, `docs/feature-matrix.md:222,272-277` · **Status**: NEW (supersedes #4105's wording) · **Age**: `f61ea0447` (09-13) · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: `byroredux/src/systems/combat_ai.rs:122,170` computes `attack_damage` for NPC aggressors and inserts `HitEvent` (verified).
- **Suggested Fix**: Name `npc_combat_ai_system` as a second producer and consumer of `CreatureAttack`; add an NPC-combat row to the feature matrix.

##### TD3-009: feature-matrix and ROADMAP still say AI package selection is spawn-time only
- **Location**: `docs/feature-matrix.md:93-96,324`, `ROADMAP.md:1217` · **Status**: NEW · **Age**: since M42.9 / #2652 · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: `ambient_ai_package_system` is registered unconditionally (`byroredux/src/boot/schedule/update.rs:246`) and re-evaluates once per in-game minute. ROADMAP also lists combat as unbuilt.
- **Suggested Fix**: Update both status docs and drop the gap row.

##### TD3-010: ROADMAP Project Stats and `game-compatibility.md` still count Oblivion at 8 032 NIFs (re-measured 9 612, #3925)
- **Location**: `ROADMAP.md:517,1527`, `docs/engine/game-compatibility.md:34` · **Status**: NEW · **Age**: `232fdc458` (09-07) · **Effort**: trivial · **Kind**: doc-rot
- **Suggested Fix**: Oblivion 9 612; seven-game total 604 787 (not 603 207). ROADMAP's own compat matrix (`:669`) already has the right figure.

##### TD3-011: hkx status docs still say "Skyrim SE 64-bit only" after today's Skyrim LE 32-bit support
- **Location**: `docs/feature-matrix.md:122,124-125,307`, `ROADMAP.md:596` · **Status**: NEW · **Age**: `fb8173fe0` (09-14) · **Effort**: trivial · **Kind**: doc-rot

##### TD3-012: docs/engine describes deleted or renamed code symbols as live
- **Location**: `docs/engine/scripting.md:116,600` (*quest_advance_on_activate_system* → `quest_advance_system`), `docs/engine/animation.md:352` (deleted AnimationController's *resolve_blend_time*), `docs/engine/exal.md:632` (*bto_archive_path* → `object_lod_archive_path`), `docs/engine/renderer.md:312` (per-draw push constants that no longer exist; data comes via `gl_InstanceIndex`), `docs/engine/stream-boundary-state-continuity.md:187` (deleted *persistent_ref_index.rs*), `docs/engine/watal.md:134` (removed *MAX_CELLS_SPAWNED_PER_FRAME*) · **Status**: NEW · **Effort**: small · **Kind**: doc-rot

##### TD3-013: `clouds.glsl` header and march comment still describe the fixed-step march the adaptive march replaced
- **Location**: `crates/renderer/shaders/include/clouds.glsl:54` (lists *_VIEW_STEPS*), `:202-203` ("48 steps do not band") · **Status**: NEW (Dim 7 hand-off, orchestrator-verified) · **Age**: `9ac8a9299` (09-13) · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: `shader_constants_data.rs` now defines `CLOUD_CHEAP_SAMPLES_ZENITH = 64` / `CLOUD_CHEAP_SAMPLES_HORIZON = 128`, and no `CLOUD_VIEW_STEPS` exists.
- **Suggested Fix**: Name the cheap-sample constants and drop the step count.

#### Dimension 4 — Audit-Finding Rot

##### TD4-001: The tech-debt skill's Phase-1 orientation asserts false facts: "`shader.rs` dropped back under — do not re-propose", "2 files", "every monolith is split"
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:137-155,180,184,192,215` · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot · **Related**: TD1-001
- **Finding**: `crates/nif/src/blocks/shader.rs` was never split and re-crossed on 2026-09-12 (2058 production, string-aware), so a do-not-re-propose instruction covers a live candidate. The primary bucket is 3 files, not 2 (the "4" the skill's own awk reports includes the TD1-001 false positive). `:180` "11 files" contradicts `:138`. `:192` says `context/` has 19 files; it has 18. The secondary bucket is 43, not 40.
- **Suggested Fix**: Reword the shader.rs clause as dated history, drop the count and "every monolith" sentences in favour of "re-run the recipe", and fix 19→18. Do this after TD1-001's counter fix so the new figures are real.

##### TD4-002: The skill's GPU-struct size recipes grep a file that has no `GpuMaterial` pin
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:321-327` (Dim 3), `:452-458` (Dim 8) · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: Both recipes grep only `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`; `gpu_material_size_is_428_bytes` lives in `crates/renderer/src/vulkan/material_tests.rs`.
- **Suggested Fix**: `grep -rn "fn gpu_.*_is_[0-9]\+_bytes\|size_of::<Gpu" crates/renderer/src/vulkan/`.

##### TD4-003: The mod-runtime "no engine consumer" premise survives in two audit files after #3828, and the layout still names *runtime.rs*
- **Location**: `.claude/commands/_audit-common.md:186` ("audit it as a contract, not as a live path"), `:28`; `.claude/commands/audit-tech-debt/SKILL.md:26` ("still consumer-less") · **Status**: NEW (unfixed siblings of closed #3828) · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: `SandboxRuntime` is consumed by `byroredux/src/extensions/mod.rs` and `byroredux/src/extensions/install.rs`; `crates/mod-runtime/src/runtime/` is a directory (#3853).
- **Suggested Fix**: Match `audit-safety` Dim 11's live-consumer framing; list the `runtime/` modules.

##### TD4-004: Stale rows in `_audit-common.md`'s fenced Project Layout
- **Location**: `.claude/commands/_audit-common.md:29,63,74,80,83,85,89,99,105` · **Status**: NEW (`:82` is #4121; `:101` is #4114) · **Effort**: small · **Kind**: doc-rot
- **Finding**: *texture_registry.rs* (`:63`), *boot.rs* (`:74,80,85`) and *asset_provider/material.rs* (`:99,105`) no longer exist. The Scripting row omits `compatibility.rs`, `obscript.rs`, `obscript_runtime.rs`, `papyrus_provider/` and `combat.rs` (~3.4k LOC). The Systems row omits `combat_ai.rs` and `navmesh_path.rs`. The binary misc list omits `workspace_hygiene_tests.rs`.
- **Suggested Fix**: Rewrite the rows to their directory shapes and add the missing modules.

##### TD4-005: `_audit-validate.sh` checks only backticked tokens, so fenced layout maps are outside the gate
- **Location**: `.claude/commands/_audit-validate.sh` (token extraction, ~`:115-120`) · **Status**: NEW · **Effort**: small · **Kind**: doc-rot (audit infrastructure)
- **Finding**: A prototype fence scan over all skill files (`os.path.exists` on path-prefixed tokens in fenced lines) returned 4 misses: 2 real (`_audit-common.md:63,99`) and 2 filename-template false positives. It is low-noise, and it would have caught TD3-003/TD4-003/TD4-004 and past #4121/#4244/#4020.
- **Suggested Fix**: Scan fenced lines for path-prefixed tokens as STALE (skipping `<…>` placeholders and trailing-`_` templates); route in-row basenames through the existing bare-basename advisory. Consider running the same scan over CLAUDE.md.

##### TD4-006: `_audit-common.md` still says the feature-matrix M45/M47 rows lag; they were fixed 2026-06-21
- **Location**: `.claude/commands/_audit-common.md:132` · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: This contradicts the tech-debt skill's own Dim 3 note. Two audits already cited it as a premise (`docs/audits/AUDIT_FO3_2026-08-27.md:184`, `docs/audits/AUDIT_CHARACTER_2026-08-30.md:675`), though neither re-filed.
- **Suggested Fix**: Replace with "status floor; re-check each row against its crate".

##### TD4-007: Backticked names of deleted or out-of-repo files in three skills
- **Location**: `.claude/commands/audit-concurrency/SKILL.md:104` (*extensions.rs*), `.claude/commands/audit-tech-debt/SKILL.md:181` (*papyrus_provider.rs*, *extensions.rs*, and `compatibility.rs`/`runtime.rs`, which resolve only to unrelated same-named files), `.claude/commands/audit-renderer/SKILL.md:287` (memory file *reference_glsl_pathtracer.md*) · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Suggested Fix**: Italicise them; optionally widen the gate's memory-file skip rule beyond `feedback_*`.

##### TD4-009: `audit-nifal` calls #3814 "the still-open sibling"; it closed 2026-09-03
- **Location**: `.claude/commands/audit-nifal/SKILL.md:252` · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Suggested Fix**: "#3814 (closed, `d63b8ce09`) pins the 16 generic supplemental lanes — verify the guard at `byroredux/src/render/static_meshes.rs`."

##### TD4-010: `audit-scripting` anchors `fragment.rs::resolve_object` / `::populate_quest_fragments_from_script` at the 144-line front file
- **Location**: `.claude/commands/audit-scripting/SKILL.md:282,874,912` · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Suggested Fix**: Re-anchor to `crates/scripting/src/fragment/effects.rs` and `crates/scripting/src/fragment/populate.rs`.

##### TD4-011: `audit-renderer` Dim 18 still says "fog applied to direct only (Dim 8)", the premise #4047 removed from Dim 8
- **Location**: `.claude/commands/audit-renderer/SKILL.md:299` · **Status**: NEW (unfixed sibling of closed #4047) · **Effort**: trivial · **Kind**: doc-rot
- **Suggested Fix**: "fog/volumetric transmittance applied to the combined direct+indirect term, pre-tonemap".

#### Dimension 6 — Stubs & Placeholders

##### TD6-005: `SetInChargen` and `SetHudCartMode` write state nothing reads, but `Effect::is_placeholder` doesn't flag them
- **Location**: `crates/scripting/src/translate/effects.rs:338-340` (+ the `:2799` assertion pinning `!SetHudCartMode.is_placeholder()`), `crates/scripting/src/cinematic.rs:185-202`, `crates/scripting/src/player_control.rs:59` · **Status**: NEW (sibling of closed #4328) · **Age**: `d0dac91b1` (09-13), `5162829a3` (09-14) · **Effort**: small · **Kind**: tech-debt
- **Finding**: `disable_saving` / `disable_waiting` / `show_controls_disabled_message` and `hud_cart_mode` have no production reader; `cinematic.rs`'s own doc says saving is not actually blocked. The code path is reachable through the MQ101 `--new-game` route (no smoke test). Coverage harnesses report these fragments as fully claimed, and a player can quicksave inside Bethesda's save-disabled chargen block.
- **Suggested Fix**: Either tag both as placeholders (trivial) or gate quicksave/wait on the flags (small).

##### TD6-006: `FactionRelations` is write-only, and its doc compares it to itself
- **Location**: `crates/scripting/src/combat.rs:39-83` (self-reference at `:54`) · **Status**: NEW · **Age**: `f61ea0447` (09-13) · **Effort**: trivial (doc) / medium (consumer) · **Kind**: tech-debt
- **Finding**: `SetEnemy` inserts pairs; `is_enemy` has no production caller. The scope is documented in prose but untracked.
- **Suggested Fix**: Fix the `:54` sentence and open a tracker for ambient hostility.

##### TD6-007: `DebugRequest::ListLoadedAssets` always errors "not yet implemented", no client sends it, and the debug-CLI doc presents it as working
- **Location**: `crates/debug-server/src/evaluator.rs:133-140`, `crates/debug-protocol/src/lib.rs:118-134`, `docs/engine/debug-cli.md:126,731-734` · **Status**: NEW · **Effort**: trivial (delete) / small (implement) · **Kind**: tech-debt
- **Suggested Fix**: Delete the variant and `AssetKind` (`tex.loaded` / `mesh.*` cover the use case) unless the TUI plans a consumer.

##### TD6-008: `AnimationClip.phase` doc says "NOT yet consumed … no field exists"; #3345 wired it
- **Location**: `crates/nif/src/anim/types.rs:165-174` · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Finding**: `AnimationPlayer::with_phase` is called at `byroredux/src/scene/nif_loader.rs:647`, `byroredux/src/cell_loader/spawn.rs:840` and `byroredux/src/scene.rs:1183`.

##### TD6-009: SPEL / ENCH / MGEF / LVSP maps (plus the Oblivion effect-code index) are parsed on every ESM load with zero production readers
- **Location**: `crates/plugin/src/esm/records/index.rs:137,211,217,219,231`; inserts in `dispatch_misc_gameplay_b.rs` / `dispatch_container.rs` · **Status**: NEW (no open issue mentions magic/SPEL/MGEF/SPLO) · **Effort**: trivial (tracker) / large (magic runtime) · **Kind**: tech-debt
- **Finding**: MQ101's `AddRaceSpells` already declines for want of a SPLO decoder. Dim 6's report also tabulates ~45 other zero-reader `EsmIndex` fields (ECZN, NAVI, FLST, CSTY, PROJ/EXPL/IPCT, MESG, REPU, BPTD, COBJ, plus the deliberate EDID+FULL long tail); all 77 `ImportedMaterial` fields have readers.
- **Suggested Fix**: Open a "magic runtime: SPLO → spell lists → MGEF application" tracker and reference it from the `leveled_spells` / `magic_effects_by_code` docs. Keep the parsers.

#### Dimension 7 — Magic Numbers

##### TD7-003: "blades per chunk = workgroup × candidates per thread" is a prose invariant over three hand-edited constants
- **Location**: `crates/renderer/src/shader_constants_data.rs:259,275,286-289` · **Status**: NEW · **Age**: `673b21458` (09-13, edited in lockstep by hand) · **Effort**: trivial · **Kind**: tech-debt
- **Suggested Fix**: `pub const GROUNDCOVER_MAX_BLADES_PER_CHUNK: u32 = GROUNDCOVER_SCATTER_WORKGROUP * GROUNDCOVER_CANDIDATES_PER_THREAD;`

##### TD7-004: SKYAL cloud density-shaping literals are bare inline, under a header claiming every value is sourced
- **Location**: `crates/renderer/shaders/include/clouds.glsl:125-126,143,169,171-172,251,288`; claim at `crates/renderer/src/shader_constants_data.rs:881-884` · **Status**: NEW · **Age**: `c379898fd` (09-13) · **Effort**: small · **Kind**: tech-debt
- **Finding**: The named `CLOUD_*` constants are properly cited; the height gradient, noise frequencies, erosion strength/height factor, wind scale and 0.01 cutoff are not, and the provenance gate can't see inline literals. `docs/engine/skyal.md:263-264` tracks only the two frequencies.
- **Suggested Fix**: Promote them to named `CLOUD_*` constants with a source or an explicit "uncited, calibration pending" note; extend the skyal.md open list.

##### TD7-005: Blade shaping literals are bare inline in the ground-cover blade shaders and absent from the uncited-values list
- **Location**: `crates/renderer/shaders/groundcover_blade.vert:215,243,245,254,340`, `crates/renderer/shaders/groundcover_blade.frag:75` · **Status**: NEW · **Age**: `637b652647` (09-06) · **Effort**: small · **Kind**: tech-debt
- **Finding**: The stiffness factor `0.7` coincides with `GROUNDCOVER_WIND_MAX_BEND = 0.7` two lines later, which invites a wrong "unification".
- **Suggested Fix**: Name them in `shader_constants_data.rs` and list them as uncited in `docs/engine/exal-groundcover.md` §12.12.

##### TD7-006: The 8×8 blue-noise table's dimensions (`& 7`, `* 8`, `/ 64.0`) are re-typed at both consumers
- **Location**: `crates/renderer/shaders/include/blue_noise.glsl:16`, `crates/renderer/shaders/composite.frag:286-288`, `crates/renderer/shaders/volumetrics_inject.comp:1375-1379` · **Status**: NEW · **Age**: composite consumer now drives the cloud step jitter (`564d0d2fe`, 09-13) · **Effort**: trivial · **Kind**: tech-debt
- **Suggested Fix**: Add `blueNoiseRankAt(ivec2)` derived from `BLUE_NOISE_RANKS.length()`.

#### Dimension 8 — Dead Code

##### TD8-001: Debug-server whole-component `set_json` accessor is dead — stub closure, field and type alias have no invoker
- **Location**: `crates/debug-server/src/registration.rs:29`, `crates/debug-protocol/src/registry.rs:14,37` · **Status**: NEW (Dim 6 hand-off, confirmed) · **Age**: `cc6aea877` (2026-04-13) · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: Every registered component gets a closure that always returns "whole-component replacement not yet supported". `DebugRequest` has no variant that calls it; only `SetField` → `set_field` is live. The field's doc describes behaviour that never existed.
- **Suggested Fix**: Delete the field, the `SetJsonFn` alias and the closure. Add them back with a request variant if the feature is ever wanted.

##### TD8-002: Five `populate_*_fragments` entry points have zero callers workspace-wide, yet stay re-exported from `lib.rs`
- **Location**: `crates/scripting/src/fragment/populate.rs:145,254,397,532,552`; re-exports `crates/scripting/src/lib.rs:69-76` · **Status**: NEW (follow-on to #3854) · **Age**: `27875a021` (08-23), `ab2a193a9` (09-01) · **Effort**: small · **Kind**: tech-debt
- **Finding**: The engine calls only the `populate_owned_*_with_providers` family. `populate_quest_fragments_from_pex_detailed_with_providers`, `populate_quest_fragments_from_script_with_providers`, `populate_scene_fragments_from_pex_detailed_with_providers`, `populate_scene_fragments_from_script` ("exposed for focused conformance tests", yet no test calls it) and `populate_scene_fragments_from_script_with_providers` have no caller, test or example. Because they are `pub` and re-exported, the dead-code lint cannot see them.
- **Suggested Fix**: Delete the five functions and their re-exports; keep the `_internal` helpers the owned variants use.

##### TD8-003: Six dead `pub fn`s on engine-consumed types
- **Location**: `crates/renderer/src/vertex.rs:110` (`Vertex::new_skinned`), `crates/renderer/src/vulkan/texture.rs:500` (`Texture::from_dds`), `crates/renderer/src/vulkan/frame_upscaler.rs:330` (`FrameUpscaler::output_image`), `crates/renderer/src/vulkan/context/mod.rs:2214` (`VulkanContext::with_transfer_commands`), `crates/scripting/src/scene.rs:85` (`SceneActorBindings::unbind`), `crates/core/src/animation/stack.rs:358` (`collect_stack_text_events`) · **Status**: NEW · **Age**: 1.5–5.5 months · **Effort**: small · **Kind**: tech-debt
- **Finding**: Each name occurs only at its definition. `with_transfer_commands`'s doc says "prefer this over the free function", but the free functions have 51 call sites and the method has none. `collect_stack_text_events` is "retained for test ergonomics" but no test uses it. Eleven more zero-reference items in library crates are noted in Dim 8's scratch report, not filed; `read_lstring_sub` (#1045) has never had an adopter and deserves a look.
- **Suggested Fix**: Delete all six. Removing the six-line method also shrinks `context/mod.rs` (#4217).

##### TD8-004: Ground-cover dead code — `layer_affinities`' consumer claim is disproven; the Phase C helpers wait on an untracked design-doc phase
- **Location**: `byroredux/src/groundcover_translate.rs:190` (`layer_affinities`), `:279` (`classify_species_name`), `:315` (`climate_weights_for`); stale doc `:59-69` · **Status**: NEW (related to closed #4226; not a regression) · **Age**: `637b65264` (09-06), `f8a900b3c` (09-13) · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: Phase 1 shipped through the singular `layer_affinity` (`byroredux/src/cell_loader/terrain.rs:178,734`) and the shader's `byroGcAffinity`; the plural batch form has only a test caller. `f8a900b3c` removed the sole production caller of the two species helpers and added allows citing §12.12 Phase C, which has no issue (#3807 is the umbrella, #4056 is Phase 3). The DEFAULT_AFFINITY doc still calls the GPU affinity dispatch "pending".
- **Suggested Fix**: Delete `layer_affinities` and its test. File a Phase C tracker and cite it in the allows, or delete the helpers (recoverable from `f8a900b3c^`). Fix the `:59-69` doc.

##### TD8-005: `crates/plugin/src/legacy/mod.rs` hides a 5.5-month-old consumer-less module behind `#![allow(dead_code)]`, while the live ESM path models slots separately
- **Location**: `crates/plugin/src/legacy/mod.rs:35`, `crates/plugin/src/lib.rs:35`; parallel model `crates/plugin/src/esm/reader.rs:408-435` (`GlobalSlot::compose`, `FormIdRemap`) · **Status**: NEW (#1322 kept it as scaffolding; the evidence below is new) · **Age**: `bed67b87b` (2026-03-28) · **Effort**: small · **Kind**: tech-debt
- **Finding**: `LegacyFormId` / `LegacyLoadOrder` have no production caller, and neither does the `DataStore::add_plugin` layer they would feed. There are two independent ESL `0xFE` models, and only the dead one knows ESH `0xFD`. #4081 (2026-09-11) spent a fix on `LegacyFormId::is_null` in code nothing executes. The module-wide allow also hides any future dead additions.
- **Suggested Fix**: Delete the module and fold ESH `0xFD` into `GlobalSlot` when needed; or keep it, add a tracker for the esm→`Record` bridge, and plan to reuse `GlobalSlot::compose`.

##### TD8-006: `NiNode` "backward compat" inherent accessors duplicate its trait impls one-for-one, with an April removal promise
- **Location**: `crates/nif/src/blocks/node.rs:23-51` (dup of `:70-98`) · **Status**: NEW · **Age**: `7e9274bd1` (2026-04-03) · **Effort**: small · **Kind**: tech-debt
- **Finding**: Seven inherent methods mirror `HasObjectNET` / `HasAVObject`. Inherent methods shadow trait methods, so the traits never become the access path. `NiTriShape` and the light blocks already implement only the traits. A dangling comment has no field after it.
- **Suggested Fix**: Delete the inherent block and the comments; add the trait imports at whatever call sites the compiler flags.

##### TD8-007: `_`-prefixed parameters that survived refactors in production functions
- **Location**: `byroredux/src/asset_provider/material/provider.rs:292` (`register_starfield_cdb_probe(_info)`), `crates/nif/src/blocks/shader.rs:957` (`parse_skyrim(_bsver)`, private, 1 caller), `crates/renderer/src/vulkan/frame_upscaler.rs:1052` (`destroy_device_objects(_device)`) · **Status**: NEW · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: Because the CDB probe discards its `_info`, `sf_cdb_cache` stores a whole `Option<CdbHeaderInfo>` where a bool would do. The table-bound `prim_*` signatures, the `_game` seam in `light_anim.rs` and the fixed system/trait signatures were checked and are justified.
- **Suggested Fix**: Drop the parameters (and shrink the CDB cache value to `bool`).

##### TD8-008: Three `byroredux` binary feature gates are never compiled in their non-default state in CI
- **Location**: `byroredux/Cargo.toml:7-22`; cfg sites `byroredux/src/main.rs:8,897`, `byroredux/src/boot/mod.rs:95,121`; `.github/workflows/ci.yml` · **Status**: NEW (same class as closed #1763 / #3894, which each got a lane) · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: No lane builds `byroredux` with `--no-default-features` (the `not(feature = "debug-server")` arms), `--features tracing-tracy` or `--features dhat-heap`, so any of the three can rot until a developer reaches for the profiler.
- **Suggested Fix**: Add three separate `cargo check -p byroredux` lines to the existing check job.

##### TD8-009: `MergeOutcome`'s test-only `allow(dead_code)` justification is obsolete
- **Location**: `byroredux/src/asset_provider/material/merge.rs:45-66` · **Status**: NEW (follow-on to closed #4289 / #2709) · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: `resolved()` / `merged()` are kept "so the deferred telemetry sink #2709 asks for has something to call". #4289's `trace_merge_outcome` (`:88`) landed comparing `== MergeOutcome::PresenceOnly` directly, so every use of the two methods is in `byroredux/src/asset_provider/tests/`.
- **Suggested Fix**: Put `#[cfg(test)]` on the impl block and drop the comment.

Minor, unfiled notes from the Dim 8 allow-site table:
- `crates/bsa/src/ba2.rs:165` gates `end_mip` on "M40 streaming", but M40 (cell streaming) is closed and mip streaming is M39.
- `ActionBindings::bind_key` and the `VF_*` schema bits would read more honestly as `#[cfg(test)]` / `cfg_attr(not(test), …)`.
- `crates/plugin/examples/sf_smoke.rs:112`'s comment is slightly off.
- All other `allow(dead_code)` sites are justified: RAII guards, debug-gated items, std430 header fields, and hkx's `global_target`.


#### Dimension 9 — Test Hygiene

##### TD9-001: #3848 wired the Skyrim ruleset, so the ignored CHARAL real-data gate now panics on Skyrim and never reaches the Oblivion case
- **Location**: `crates/plugin/tests/parse_real_esm.rs:339` (`derived_rows: None`), `:509-518` (`None =>` arm asserting `ruleset.is_none()`), `:524` · **Status**: NEW · **Age**: `e13985dfc` (09-12) · **Effort**: trivial · **Kind**: test-gap · **Related**: TD3-007
- **Finding**: `crates/core/src/character/profile.rs:140,203` now always builds `Some(skyrim_ruleset(..))`; the non-ignored `skyrim_profile_builds_a_ruleset_and_actually_calls_gmst` already `.expect`s it. The expected value is measured at `crates/plugin/src/esm/records/tests.rs:153` (`derived_row_len() == 2`). Not promoted: #3848 was `medium`. Not run (plugin `--ignored` OOM rule); concluded from the code path.
- **Suggested Fix**: `derived_rows: Some(2)`, re-measured once with the GMST overlay active.

##### TD9-002: The five `recon`-feature unit tests in `crates/spt` run in no CI lane
- **Location**: `crates/spt/src/recon/mod.rs:199-272`, `crates/spt/src/lib.rs:53`, `.github/workflows/ci.yml:151-152` · **Status**: NEW (sibling of closed #3894) · **Age**: `8b77cb7c2` (05-09) · **Effort**: trivial · **Kind**: test-gap
- **Finding**: #3894 added `cargo check --features recon --examples`, which never builds `cfg(test)`; `cargo test --workspace` leaves `recon` off. These are value-asserting tests.
- **Suggested Fix**: Add `cargo test -p byroredux-spt --features recon --lib`.

## Existing Issues — Re-verified This Cycle (not re-filed)

| Issue | State | This cycle's verification |
|---|---|---|
| #4114 GpuMaterial 432→428 doc rot | OPEN | **Zero sites fixed.** ~26 sites remain; add `.claude/commands/audit-safety/SKILL.md:261` and `ROADMAP.md:110` to its list. #4211 was closed as its duplicate. No workspace-wide scanner landed; only the `bindings.glsl` pin exists. |
| #4217 / #3736 context/mod.rs | OPEN | +51 production LOC (2831→2882), mostly `VulkanContext` fields for sky cube/clouds; the proposal stands and #3736's thesis is reinforced (TD1-003). A `SkyResources` sub-struct is a natural first cut. |
| #4218 storage_util.rs | OPEN | Unchanged (0 commits). |
| #4219 records/mod.rs | OPEN | Unchanged. |
| #4212 Havok constraint stubs | OPEN | Accurate. Addendum: `crates/nif/src/scene.rs:94,108` docs name hinge/ragdoll constraints (which now have real decoders) as the stubs. |
| #4213 NiStencilProperty | OPEN | Accurate. |
| #4214 IMGS no consumer | OPEN | Accurate. The stale "deferred to M48" comment is **still** at `dispatch_misc_gameplay_a.rs:115` / `misc/world.rs:1346`, and its IMAD half is now doubly wrong (IMAD is typed and consumed). |
| #4215 mesh water flags | OPEN | **Premise partly stale** — reflection/refraction bits 6/7 *are* read in `water_material_from_mesh` (`byroredux/src/material_translate.rs:248-257`, since `45f65380e`, 2026-08-19, verified). Recommend re-titling to the Displacement/Depth/Cubemap bits and downgrading to LOW. |
| #4174 audit-ecs boot.rs | OPEN | `:305` fixed; `:154-155` `rg … boot.rs` still broken; widen to `audit-save:86`, `audit-tech-debt:221,224` (TD4-008). |
| #4121 extensions.rs in _audit-common | OPEN | Still present at `_audit-common.md:82`. |
| #4175 CommonNamedFields::from_subs | OPEN | Unchanged; no new hand-rolled loops since 09-11. |
| #4147 water-audio literals | OPEN | Unchanged (`byroredux/src/systems/audio.rs:304-319`). |
| #4227 golden-frame regen | OPEN | `byroredux/tests/golden/` untouched since `f5127c1cc` (TD9-003). |

## Verified Fixes (prior cycle)

#4216 (bloom VRAM constant now derived and pinned), #4220 / #4221 (barrier helpers), #4222
(EDID/MODL/VMAD funnel), #4223 / #4224 (skill italics), #4225 (PerkRecord doc), #4226
(groundcover allows), #4030 (mesh-ID mask), and prior TD8-001 (`was_released` allow removed).
None regressed.

## Checked Clean

- **Dim 5**: 22 raw marker hits, 0 shader hits, all documented exclusions (ESM `XXXX`
  protocol ×12, `*b"XXXX"` sentinels ×3, #3859 prose, upstream OpenMW / nif.xml / reference
  FIXME citations ×3, a retrospective "closes #242 TODO", the accepted `items.rs` TBD). The
  MIT/Burley attribution block atop `crates/renderer/shaders/triangle.frag` is intact
  despite today's edit (`5278e1639`).
- **Dim 1**: >50-arm matches besides the excluded NIF dispatch are one-line key/`Display`
  mappings; barrels with >20 `pub use` are pure barrels; `crates/renderer/build.rs::main`
  (1148, flat codegen) and the MQ101 conformance example remain census-accepted. Watch:
  `crates/nif/src/lib.rs::dispatch_blocks` 503→625, `draw_frame` 612→696.
- **Dim 2**: every other hand-rolled `vk::ImageMemoryBarrier` in `crates/renderer/src/vulkan/`
  checked; no residual analytic-sky copies after `b7abdaa55`; combat AI, Disable()
  persistence, release profiles, `.bto`/MSWP material paths, hkx splines and the pex string
  budget introduce no duplication.
- **Dim 3**: `GpuCamera` 368 / `GpuInstance` 160 / `GpuLight` 64 / `Vertex` 104 are clean;
  every `classify_pbr` mention is historical; no "not yet dispatched" wording survives in
  SKYAL; no reverts since 08-01; every README `--flag` resolves; feature-matrix rows for
  M45, M47.2, M48 and MQ101 SCEN are correct.
- **Dim 4**: every "all N dimensions" claim matches; audit-suite presets resolve; 64/66
  `path::symbol` anchors resolve; 8 >90-day CRITICAL/HIGH findings spot-checked against
  code (SAFE-01/02/03, E-1, REN2-02 fixed; NIF-04-11 all filed).
- **Dim 6**: zero `unimplemented!`/`todo!()`; no no-op console commands; SDK / mod-runtime /
  extensions / launcher crates have no silent stubs; SKYAL pipelines are dispatched and
  consumed.
- **Dim 7**: every new `CLOUD_*` / `BLOOM_*` / `GROUNDCOVER_*` define is sourced from
  `shader_constants_data.rs`; the provenance gate walks `include/`; workgroup sizes, sky-cube
  dispatch and other ground-cover record sizes are pinned; combat AI uses named constants;
  hkx offsets come from a tested pointer-size-aware layout.
- **Dim 9**: 194/194 ignores carry reasons (192 data/GPU gates, 2 manual benches); 9 ignored
  tests citing issues closed since 09-01 all assert post-fix values; 18 new smoke-shaped
  tests are legitimate; 0 commented-out asserts; `inspect` feature tests covered by
  workspace unification; 513 `include_str!` scans, 0 vacuous; 256 skill-named test guards
  resolve.

## Deferred

- **TD6-009** (magic runtime) and **#4214** (IMGS/IMAD) — implementation is large and gated
  on unbuilt subsystems; the actionable part now is a tracker plus comment corrections.
- **TD1-006**'s `record_skinned_blas_refit` / `collect_static_mesh_draws` decomposition — gated
  on the RenderDoc verification rule for Vulkan-recording changes
  (`feedback_speculative_vulkan_fixes.md`).
- **#3736** (VulkanContext God Object) — large; TD1-003 suggests sky/cloud resources as the
  first sub-struct.

---

**Next audit**: fix TD1-001 first, then re-run the Phase-1 baseline and diff against this
snapshot. Watch `crates/renderer/src/vulkan/context/draw.rs` (43 lines from threshold),
`apply_effect`'s growth, and whether #4114's scanner lands before a sixth struct-size rot.
