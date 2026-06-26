# Tech-Debt Audit — 2026-06-26

9-dimension orchestrated sweep (one Task agent per dimension, 3 concurrent).
Prior report: [2026-06-23](AUDIT_TECH_DEBT_2026-06-23.md). Depth: **deep**
(per-instance triage + concrete fix). Scope reached the three young crates
(`crates/pex/`, `crates/save/`, the expanded `crates/scripting/`) and the
asset_provider directory split that landed this cycle.

---

## 1. Executive Summary

**26 findings** (+ 4 NOTE/non-finding records) — **0 CRITICAL, 0 HIGH,
6 MEDIUM, 20 LOW**.

Three themes dominate this cycle:

1. **The path-validation gate is RED.** `byroredux/src/asset_provider.rs`
   (3405 LOC last cycle) was split into the directory
   `byroredux/src/asset_provider/` { mod, archive, material, script, texture,
   tests }, leaving **12 stale backticked refs** across audit skills — including
   one in *this audit's own skill* (`audit-tech-debt/SKILL.md:108`). The gate
   `_audit-validate.sh` exits 1 on every run until these are repointed
   (TD3-005 + TD4-001 + TD4-002). This is the single highest-payoff cleanup —
   it un-masks future path drift.

2. **Three OPEN tech-debt issues are already fixed in-tree.** Commit `eb71bcb9`
   (2026-06-26 14:53, *"Fix #1729 #1704 #1735 #1709 #1627"*) resolved the
   debt for **#1627** (glass()/car_paint() dangling-issue comments — TD5-001),
   **#1704** (`mswp::peek_path_filter` dead fn — TD8-001), and **#1709**
   (vol→dB triplication, now `linear_volume_to_db()` — Dim 2 non-finding). All
   three GitHub issues are still **OPEN**. Verify-and-close, no code change.

3. **`draw_frame()` regressed past its own closed fix.** #1052 (CLOSED) extracted
   it to "2322 LOC". It is now **3325 LOC** (`draw.rs:410-3735`) — grew +1000 LOC
   as M55/M58 volumetrics+bloom passes were appended inline (TD1-001, MEDIUM as a
   stale-fix regression). It is the worst single complexity site in the repo.

The six MEDIUMs are all amplification-promoted, not intrinsic severity:
two duplication findings with **proven divergent-fix history** (TD2-001/002),
two stale GPU-struct-size docs (lockstep-drift bait — TD3-001/002), one
scripting doc that misrepresents implemented functions as stubs (TD6-001), and
the draw_frame regression (TD1-001).

**The young crates remain clean.** `crates/pex/`, `crates/save/` carry zero
markers, zero `#[allow(dead_code)]`, zero panic-stubs, no file >2000 LOC. The
only new-crate debt is doc-rot in `crates/scripting/src/condition.rs` (TD6-001)
and two correctly-tracked stubs (#1663, #1739).

| Severity | NEW | Existing/Regression | Total | Dimensions |
|----------|-----|---------------------|-------|------------|
| CRITICAL | 0   | 0                   | 0     | — |
| HIGH     | 0   | 0                   | 0     | — |
| MEDIUM   | 5   | 1 (regression)      | 6     | D1, D2(×2), D3(×2), D6 |
| LOW      | 14  | 6                   | 20    | D1, D2, D3, D4, D5, D6, D7, D8, D9 |

**Delta vs 2026-06-23:** asset_provider split (3405→removed) *reduced* the
>2000 set 6→5 but the split also broke the path gate (12 new stale refs). The
two BLOCKED Vulkan monoliths grew again (draw.rs 4176→same range; draw_frame is
now a single 3325-LOC fn). Two new >2000 crossings (collision.rs 2155,
particle.rs 2131, both ~half tests). #1627/#1704/#1709 moved from OPEN-debt to
fixed-in-tree-but-issue-open. No new correctness rot.

---

## 2. Baseline Snapshot

Source: `/tmp/audit/tech-debt/baseline.txt` (captured 2026-06-26 pre-sweep).

| Metric | 2026-06-26 (today) | 2026-06-23 | Δ |
|---|---:|---:|---:|
| `TODO`/`FIXME`/`HACK`/`XXX` (raw grep) | 17 | 18 | −1 |
| ↳ active production markers | **0** | 1 (#1627, now fixed) | −1 |
| `#[allow(dead_code)]` | 20 | 21 | −1 |
| ↳ actionable (not RAII/wire/cfg/reserve) | **2** (end_mip attr, RawDependency.name) | — | — |
| `unimplemented!()` / `todo!()` | **0** | 0 | 0 |
| `panic!("not …")` | 0 | 0 | 0 |
| `#[ignore]` attr lines (excl target/) | **112** | 261* | — |
| ↳ genuine debt (not GPU/data gate) | **0** | 0 | 0 |
| Files > 2000 LOC | **5** | 6 | −1 |

\* The "263" raw baseline count includes doc-comment mentions + a HISTORY.md
sentence; the real `#[ignore]` *attribute* population is **112**, all
GPU/on-disk-data gated (Dim 9).

Files >2000 LOC (live): `draw.rs` 4176 · `context/mod.rs` 3275 ·
`main.rs` 2800 · `import/collision.rs` 2155 · `blocks/particle.rs` 2131.
(`asset_provider.rs` 3405 → split below threshold this cycle.)

GPU struct pins (authoritative, from tests): `GpuInstance` 112 B ·
`GpuCamera` 336 B · `GpuMaterial` 300 B · `Vertex` 100 B.

---

## 3. Top 10 Quick Wins (trivial/small — readability or gate payoff)

1. **Repoint 12 stale `asset_provider.rs` refs → restore path gate to GREEN**
   (TD3-005 self + TD4-001 ×11 + TD4-002 ×2 basename). Mechanical; map in TD4-001.
2. **Fix `renderer.md` GpuCamera 304 B → 336 B** and the dead `gpu_camera_is_288_bytes`
   test-name cite (TD3-001, MEDIUM lockstep bait).
3. **Fix `bindings.glsl` GpuMaterial test-name cite** `_260_bytes` → `_300_bytes`,
   drop the false "kept for grep continuity" clause (TD3-002, MEDIUM).
4. **Close #1627, #1704, #1709** — all fixed in-tree by `eb71bcb9`, issues still
   OPEN (TD5-001, TD8-001, Dim 2 non-finding). Bookkeeping only.
5. **Delete two dead `pub fn`** in `mesh.rs`: `oriented_quad` (+ lib.rs re-export)
   and `fullscreen_quad_vertices` (TD8-002/003, ~70 LOC).
6. **Fix `condition.rs` stub-claiming docstrings** (GetFactionRank/HasPerk are
   implemented; header says "6 functions", catalog maps 7) (TD6-001, MEDIUM).
7. **Fix two feature-matrix rows** that read ✗ for shipped work: M35 Terrain LOD
   (`.btr`/`.bto`/`_far.nif`) and Ragdoll (M41.x classic-chain) (TD3-003/004).
8. **Drop redundant `#[allow(dead_code)]` on `Dx10Chunk::start_mip`** (now read at
   ba2.rs:621); delete `RawDependency.name` dead field (TD8-004/005).
9. **Fix `audit-performance` symbol name** `blas_budget_bytes` → `compute_blas_budget`
   (TD4-003).
10. **Clean the `ZZZ_probe_…` physics test** — rename + delete 4 `PROBE:` eprintln
    (TD9-002); add the `dhat-heap` CI step so the heap-budget regression actually
    runs (TD9-001).

## 4. Top 5 Medium Investments (splits / consolidations)

1. **TD1-001 — split `draw_frame()` (3325 LOC)** into 3 mechanical phase helpers
   (skinning+skinned-BLAS / geometry-pass / denoise-post), preserving recording
   order and every barrier verbatim. Regression of CLOSED #1052; reopen or refile.
   *(large; no barrier/order changes — see feedback_speculative_vulkan_fixes.)*
2. **TD1-003 + TD1-004 — split `context/mod.rs` (3275 LOC)** into
   `draw_command.rs` / `screenshot.rs` / `init.rs` (the 1025-LOC `new()`) /
   `teardown.rs`; struct + accessors stay. *(medium)*
3. **TD1-005 + TD1-006 — split `main.rs` (2800 LOC) / `App::new` (626 LOC)**
   per the OPEN #1670 axis (boot / app+steps / event-loop / debug-ui). Restate
   with worse LOC (2720→2800, 581→626). *(medium)*
4. **TD2-001 — consolidate `NiDynamicEffect` base** into
   `base.rs::NiDynamicEffectData::parse`; light.rs + texture.rs call it. Has
   *proven* divergent-fix history (#721 fixed light, #1240 fixed the missed
   texture copy ~500 commits later). *(small)*
5. **TD2-002 — promote `bloom::create_compute_pipeline` to `pipeline.rs`** as
   `pub(crate)`; route the ~5 re-inlined compute-pipeline-create sites through it
   (shotgun-edit history confirms the hazard). *(medium)*

---

## 5. Findings

### MEDIUM

#### TD1-001: `draw_frame()` is a 3325-LOC single function (regression of CLOSED #1052)
- **Severity**: MEDIUM (promotion: stale-fix regression — #1052 closed claiming extraction at "2322 LOC"; now 3325, +1000)
- **Dimension**: 1 — Function Complexity
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:410-3735`
- **Status**: Regression of #1052 (CLOSED)
- **Age**: last touched `ae285062` 2026-06-20; grew through M55/M58 (volumetrics+bloom appended inline)
- **Effort**: large (>1d)
- **Description / Evidence**: One `pub fn draw_frame` body spanning 410→3735, with the codebase's worst nesting (466 lines indented ≥6 levels). Separable phases (verified against begin/end markers): acquire+fence ~410-670; GPU skinning + skinned-BLAS lifecycle ~671-1700 (~1000 LOC, the largest sub-block); camera/jitter/instance+material upload ~782-2300; geometry render pass ~2516-3114; denoise+post ~3136-3463; submit+present ~3509-3735.
- **Impact**: Unreviewable in one pass; the skinned-BLAS state machine is hidden in deep nesting; every new post-pass appends inline so the function monotonically grows. Highest-leverage complexity site in the repo.
- **Suggested Fix**: Extract `&mut self` private helpers each taking the open `cmd: vk::CommandBuffer` and recording one phase, called in the same order: `record_skinning_and_skinned_blas` ←671-1700, `record_geometry_pass` ←2516-3114, `record_denoise_and_post` ←3136-3463. Target host fn <600 LOC. **Do not move/merge any `cmd_pipeline_barrier` across a boundary** (feedback_speculative_vulkan_fixes). Reopen #1052 or file fresh.

#### TD2-001: `NiDynamicEffect` base (switch_state + affected_nodes) duplicated across light.rs and texture.rs
- **Severity**: MEDIUM (promotion: duplicated logic with *proven* divergent bug-fix history)
- **Dimension**: 2 — Logic Duplication
- **Location**: `crates/nif/src/blocks/light.rs:72-85` ↔ `crates/nif/src/blocks/texture.rs:899-911`
- **Status**: NEW
- **Age**: light.rs copy fixed under **#721**; texture.rs copy MISSED that sweep, got the identical fix ~500 commits later under **#1240**
- **Effort**: small
- **Description / Evidence**: The `pre_fo4` BSVER gate + `switch_state` u8 (V10_1_0_106) + `affected_nodes` u32-count array (V10_1_0_0) read is byte-identical across the only two `NiDynamicEffect` subclasses. texture.rs:888-898 even says "same version gates as NiLight. See light.rs."
- **Impact**: This *already bit the codebase* — between #721 and #1240, FO4 `NiTextureEffect` over-read 5+ bytes per block. The surface is closed today only because these are the only two subclasses; the next base change must again be applied by hand in two places.
- **Suggested Fix**: Add `NiDynamicEffectData::parse(stream) -> io::Result<(bool, Vec<u32>)>` to `crates/nif/src/blocks/base.rs` (alongside `NiObjectNETData` / `NiAVObjectData`); both parsers call it after `NiAVObjectData::parse`.

#### TD2-002: Compute-pipeline create dance duplicated ~5× while a ready-made helper exists next door
- **Severity**: MEDIUM (duplicated logic with shotgun-edit history — same line edited across copies in single commits)
- **Dimension**: 2 — Logic Duplication
- **Location**: helper `crates/renderer/src/vulkan/bloom.rs:984` (private); re-inlined at `ssao.rs:343-369`, `volumetrics.rs:371-394`+`:537-560`, `skin_compute.rs:319-346`+`:773-797`; "module-on-partial" variant at `taa.rs:365`, `caustic.rs:367`, `svgf.rs:578`+`:923`
- **Status**: NEW
- **Age**: bloom helper + first inline copy born SAME commit `33f48b56e`; later `e2a4a8259`/`dde22c37e` shotgun-edited the same line/SAFETY comment across copies
- **Effort**: medium
- **Description / Evidence**: `bloom::create_compute_pipeline` does the full load_shader_module → stage → create_compute_pipelines → destroy (Ok+Err) sequence; the inner `create_compute_pipelines(...).map_err(|(_,e)|e)` is byte-equivalent across ~9 sites.
- **Impact**: Module-destroy ordering / pipeline-cache wiring hand-replicated; a single semantic change already had to be applied to 2+ copies by hand. Divergence risk on shader-module leak handling.
- **Suggested Fix**: Promote `create_compute_pipeline` to `crates/renderer/src/vulkan/pipeline.rs` as `pub(crate)`; route ssao + volumetrics(×2) + skin_compute(×2) + bloom through it. The taa/caustic/svgf "module stored on partial" variant should switch to the self-managed-module form.

#### TD3-001: `renderer.md` says `GpuCamera` is 304 B and cites a non-existent test; live size is 336 B
- **Severity**: MEDIUM (stale GPU-size in a doc → lockstep-drift bait, per severity floor)
- **Dimension**: 3 — Stale Documentation
- **Location**: `docs/engine/renderer.md:255`, `:512`, `:514`
- **Status**: NEW
- **Age**: `3c66f6d7` 2026-05-28; the `1c5b30b2` doc-correction sweep fixed the *code comments* but missed this file
- **Effort**: trivial
- **Description / Evidence**: `:255` "GpuCamera, 304 bytes"; `:512` references test `gpu_camera_is_288_bytes`; `:514` "the live 304-byte GpuCamera layout". The pinned test is `gpu_camera_is_336_bytes` (gpu_instance_layout_tests.rs:57); there is **no** `_288_bytes` test. `shader-pipeline.md:105,248` already correctly says 336 B, so renderer.md self-contradicts its sibling.
- **Impact**: The canonical renderer narrative — the place a contributor checks before touching the camera UBO / shader struct-sync. A reader pinning 304 B (or grepping the dead test) gets the wrong byte budget.
- **Suggested Fix**: `:255` 304→336; `:512` test name → `gpu_camera_is_336_bytes`; `:514` "304-byte"→"336-byte" + drop the dead "288 for grep continuity" aside.

#### TD3-002: `bindings.glsl` GpuMaterial comment cites stale test `gpu_material_size_is_260_bytes`, mislabels as "kept for grep continuity"
- **Severity**: MEDIUM (lockstep-drift bait on the shader contract)
- **Dimension**: 3 — Stale Documentation
- **Location**: `crates/renderer/shaders/include/bindings.glsl:59-60`
- **Status**: NEW
- **Age**: `218b425b` 2026-06-16 (the bindings.glsl split carried the old comment forward)
- **Effort**: trivial
- **Description / Evidence**: Comment claims the 300 B struct "is pinned by `gpu_material_size_is_260_bytes` … (the test name is historical / kept for grep continuity; it asserts 300)." There is **no** `_260_bytes` test; the real test is `gpu_material_size_is_300_bytes` (material.rs:1202), and all 5 material.rs cross-refs already name it correctly. The stated reason is factually wrong — the test was renamed to track the size.
- **Impact**: This is the single-source-of-truth shader-side `GpuMaterial` declaration; the struct-sync invariant tells contributors to update the test in lockstep, then points them at a dead grep.
- **Suggested Fix**: `_260_bytes` → `_300_bytes`; delete the "historical / kept for grep continuity" clause (line 56-57 already records "300 B — was 260 B" correctly).

#### TD6-001: `condition.rs` docstrings + header claim stubbed/6-function status the impl has outgrown
- **Severity**: MEDIUM (stale doc that misleads a stub audit, per severity floor — this dimension's own premise was nearly tripped by it)
- **Dimension**: 6 — Stub & Placeholder (doc-rot subspecies)
- **Location**: `crates/scripting/src/condition.rs:24-39` (header table) · `:71-76` (GetFactionRank) · `:82-86` (HasPerk)
- **Status**: NEW
- **Age**: docstrings `ea9d0cfa` 2026-05-23; contradicting impls `f73c6fd7` 2026-06-24 (one-month divergence)
- **Effort**: trivial
- **Description / Evidence**: GetFactionRank doc says *"Stubbed today: always returns -1 … until a faction-membership component lands"* but the impl (309-321) reads `FactionRanks` with a passing test `get_faction_rank_reads_membership`. HasPerk doc says *"Stubbed … until a perk-list component lands"* but the impl (345-366) reads `PerkList` via `FormIdPool`, tested. The header table advertises *"6 representative functions"* but `from_index` (96-107) maps **7** (`GetStageDone`/idx 59 added, absent from the table).
- **Impact**: An auditor reading these docstrings concludes GetFactionRank/HasPerk are stubs and re-files or mis-scopes #1316. The 6-vs-7 undercount compounds it.
- **Suggested Fix**: Rewrite the two docstrings to describe the real lookups (−1/0.0 are now sentinel/not-held results, not stub returns); change "6"→"7" and add the `| 59 | GetStageDone | QuestStageState |` row.

### LOW

#### TD1-002: `draw.rs` is the largest file in the tree (4176 LOC)
- **Severity**: LOW · **Dimension**: 1 · **Location**: `crates/renderer/src/vulkan/context/draw.rs` (3762 prod + 414 test) · **Status**: NEW · **Effort**: large (gated on TD1-001)
- **Description**: 3325 of 4176 lines are `draw_frame` + its 4 pure helpers. After TD1-001, relocate the phase helpers + their tests into a `context/draw/{skinning,geometry_pass,post}.rs` submodule to bring the file under threshold.

#### TD1-003: `context/mod.rs` is 3275 LOC mixing 4 responsibilities
- **Severity**: LOW · **Dimension**: 1 · **Location**: `crates/renderer/src/vulkan/context/mod.rs` (3075 prod + 200 test) · **Status**: NEW · **Effort**: medium
- **Description**: Co-locates GPU-facing structs (`DrawCommand`+impl, sky/dof/stats, 81-857), `ScreenshotHandle` (858-928), the `VulkanContext` struct (929-1413), the 1025-LOC `new()` (1427-2451, see TD1-004), and `impl Drop` (2770-3075).
- **Suggested Fix**: Mechanical extraction into `context/{draw_command,screenshot,init,teardown}.rs`; struct decl + accessor impl stay in mod.rs. No init/teardown ordering changes — move bodies verbatim.

#### TD1-004: `VulkanContext::new()` is a 1025-LOC constructor
- **Severity**: LOW · **Dimension**: 1 · **Location**: `crates/renderer/src/vulkan/context/mod.rs:1427-2451` · **Status**: NEW (#1670 tracks a *different* constructor, App::new) · **Effort**: medium
- **Description**: Builds the whole Vulkan chain inline (entry→…→sync + all optional passes). Extract `build_core_device` / `build_swapchain_and_passes` / `build_pipelines` / `build_optional_passes` helpers along the documented init phases (CLAUDE.md invariant #6). Do with TD1-003's `context/init.rs`.

#### TD1-005: `main.rs` at 2800 LOC — restate of #1670 with worse LOC
- **Severity**: LOW · **Dimension**: 1 · **Location**: `byroredux/src/main.rs` · **Status**: Existing: #1670 (states 2720; now 2800, +80) · **Effort**: medium
- **Description**: #1670's boot/event-loop/system-wiring axis still holds. Note `about_to_wait` (371 LOC) and `App::new` (626 LOC, TD1-006) are independently extraction-worthy.

#### TD1-006: `App::new()` is 626 LOC (worse than #1670's stated 581)
- **Severity**: LOW · **Dimension**: 1 · **Location**: `byroredux/src/main.rs:469-1094` · **Status**: Existing: #1670 (now 626, +45) · **Effort**: small-medium
- **Suggested Fix**: Extract `register_systems(&mut World)` + `register_script_dispatchers(&mut World)` (the 7 nested dispatch fns at 711-731) out of `App::new`.

#### TD1-007: `import/collision.rs` 2155 LOC (3 clusters + 1113-LOC test tail)
- **Severity**: LOW · **Dimension**: 1 · **Location**: `crates/nif/src/import/collision.rs` (1041 prod + 1114 test) · **Status**: NEW · **Effort**: medium
- **Suggested Fix**: Split production into `import/collision/{classify,ragdoll,shape}.rs` + shared `coord.rs` (havok→engine helpers); move each cluster's tests beside it. The resolve_shape↔dispatch parity test (1783) moves with `shape.rs`.

#### TD1-008: `blocks/particle.rs` 2131 LOC (emitter/modifier/field/system tenants)
- **Severity**: LOW · **Dimension**: 1 · **Location**: `crates/nif/src/blocks/particle.rs` (1365 prod + 766 test) · **Status**: NEW · **Effort**: medium
- **Suggested Fix**: `blocks/particle/{base,modifiers,fields,emitters,systems}.rs` with re-exporting mod.rs; opaque `NiPSysBlock` fallback stays in `base.rs`.

#### TD2-003: ACCELERATION_STRUCTURE_KHR TLAS descriptor write duplicated verbatim (caustic ↔ volumetrics)
- **Severity**: LOW · **Dimension**: 2 · **Location**: `caustic.rs:638-643` ↔ `volumetrics.rs:1028-1033` · **Status**: NEW · **Effort**: trivial
- **Description**: Byte-equivalent `WriteDescriptorSetAccelerationStructureKHR` + push_next (binding 6 vs 2 the only diff); volumetrics.rs:1013 has a "Mirrors caustic.rs:627" comment = hand-kept-in-sync liability. Add `write_acceleration_structure(...)` to `descriptors.rs` next to the existing `write_*` helpers.

#### TD2-004: water.rs builds a STORAGE_IMAGE descriptor write inline instead of the existing helper
- **Severity**: LOW · **Dimension**: 2 · **Location**: `crates/renderer/src/vulkan/water.rs:372-376` · **Status**: NEW (policy regression — `write_storage_image` shipped #1046, this copy added 10 days later) · **Effort**: trivial
- **Suggested Fix**: One-line swap to `descriptors::write_storage_image(set, 0, &img_info)`.

#### TD2-005: Inline Z-up→Y-up `[x, z, -y]` axis-swap leaks outside the canonical coord module
- **Severity**: LOW · **Dimension**: 2 · **Location**: `import/mesh/sse_recon.rs:268,291,380` · `import/mesh/tangent.rs:88-92,254` · `byroredux/src/cell_loader/terrain.rs:351` · **Status**: NEW · **Effort**: small
- **Description**: The exact `(x,y,z)→(x,z,-y)` swap (= `zup_to_yup_pos`) re-typed inline on direction vectors; comments even acknowledge it. The #1044 consolidation sweep missed these array-form (vs `NiPoint3`) sites. 40 other call sites route through the helper.
- **Suggested Fix**: Route each through `byroredux_core::math::coord::zup_to_yup_pos` (+ the tangent variant flipping only the xyz triplet).

#### TD3-003: feature-matrix M35 Terrain LOD row reads "Not started" — `.btr`/`.bto`/`_far.nif` all shipped
- **Severity**: LOW · **Dimension**: 3 · **Location**: `docs/feature-matrix.md:50`, `:178` · **Status**: NEW · **Effort**: trivial
- **Description**: Row says "✗ Not started; .btr/.bto parsers unwritten" but `terrain_lod_btr.rs`/`object_lod.rs`/`terrain_lod.rs` ship (commits `9384d4c2`, `6ddcda30` #1726, PR #1685; ROADMAP:321 documents live-verified Skyrim 544 .btr). The only remaining piece is distance-based multi-band selection. Change to `~ Partial`.

#### TD3-004: feature-matrix Physics Ragdoll row marks ✗ — the M41.x classic-chain slice shipped
- **Severity**: LOW · **Dimension**: 3 · **Location**: `docs/feature-matrix.md:126` · **Status**: NEW · **Effort**: trivial
- **Description**: `ragdoll.rs` + `ragdoll <id>` console command run a Bethesda ragdoll on Rapier (18-body Doc Mitchell verified, ROADMAP:135-139; PR #1529). FO4+ blocked on `BhkSystemBinary` only. Change to `~ Classic constraint chain (Oblivion/FO3/FNV/Skyrim); FO4+ blocked on BhkSystemBinary`. (Leave the :124-125 general-dynamic-body ✗ rows — not ragdoll work.)

#### TD3-005: this skill's own backticked `asset_provider.rs` ref is stale (file → directory)
- **Severity**: LOW · **Dimension**: 3 · **Location**: `.claude/commands/audit-tech-debt/SKILL.md:108` · **Status**: NEW (path gate) · **Effort**: trivial
- **Description**: Line 108 cites `byroredux/src/asset_provider.rs`; the file was split into `byroredux/src/asset_provider/{mod,archive,material,script,texture,tests}.rs`. Keeps the gate RED even after Dim 4 fixes the other 11. The three concerns map onto `archive.rs` (BSA/BA2 + mesh) / `texture.rs` (TextureProvider).

#### TD4-001: 11 stale `byroredux/src/asset_provider.rs` path refs across 8 skill files
- **Severity**: LOW · **Dimension**: 4 · **Location**: audit-audio:32,344,384 · audit-fo4:81 · audit-incremental:78 · audit-performance:93 · audit-runtime:257 · audit-scripting:62,634 · audit-starfield:121,264 · **Status**: NEW · **Effort**: trivial
- **Description**: File → directory split; gate flags all 11 (+ the Dim-3 one) and exits 1, masking future path drift. Repoint map (by owning symbol):

  | Line | Cited symbol/topic | Target |
  |---|---|---|
  | audit-audio:32,344,384 | `try_load_default_footstep` | `asset_provider/texture.rs` |
  | audit-fo4:81 | `merge_bgsm_into_mesh` | `asset_provider/material.rs` |
  | audit-incremental:78 | sibling-BSA auto-load, AE strip | `asset_provider/archive.rs` |
  | audit-performance:93 | BGSM/BGEM cache | `asset_provider/material.rs` |
  | audit-runtime:257 | `resolve_texture` chain | `asset_provider/texture.rs` |
  | audit-scripting:62,634 | `build_script_provider`, `extract_pex` | `asset_provider/script.rs` |
  | audit-starfield:121 | `--materials-ba2` | `asset_provider/material.rs` |
  | audit-starfield:264 | `merge_bgsm_into_mesh` | `asset_provider/material.rs` |

#### TD4-002: 2 stale `asset_provider.rs::<symbol>` bare-basename refs the gate can't see
- **Severity**: LOW · **Dimension**: 4 · **Location**: `audit-skyrim/SKILL.md:163`, `audit-starfield/SKILL.md:126` · **Status**: NEW · **Effort**: trivial
- **Description**: Gate's `should_skip()` returns early on tokens without `/`. Symbols are live: skyrim:163 `open_with_numeric_siblings` → `archive.rs:306`; starfield:126 `discover_starfield_cdbs` → `material.rs:23`. Repoint the file half in the same sweep as TD4-001.

#### TD4-003: `audit-performance` names the BLAS-budget fn `blas_budget_bytes`; actual is `compute_blas_budget`
- **Severity**: LOW · **Dimension**: 4 · **Location**: `audit-performance/SKILL.md:93` (`blas_budget_bytes @551`) · **Status**: NEW · **Effort**: trivial
- **Description**: Live fn is `compute_blas_budget` at `acceleration/predicates.rs:547`; no `blas_budget_bytes` exists. audit-fnv:75 has it right — the two skills disagree (rename left audit-performance behind). A perf subagent grepping the wrong name concludes the budget logic was removed.

#### TD5-001: #1627 marker resolved in-tree but issue still OPEN — close it
- **Severity**: LOW · **Dimension**: 5 · **Location**: `crates/renderer/src/vulkan/material.rs:604-610` (`glass()`), `:623-627` (`car_paint()`) · **Status**: Existing: #1627 (OPEN) — recommend CLOSE · **Effort**: trivial
- **Description**: Commit `eb71bcb9` rewrote both presets' doc-comments to describe the transmission/clearcoat deferral without any issue number and clarified `glass()` is a tested reference preset. `grep 'followup|#1248-' material.rs` → 0 hits. No code debt remains. (Note: #1627's body cited the wrong path `core/.../material.rs`; the fix landed in `renderer/src/vulkan/material.rs` regardless.)

#### TD6-002: GetActorValue (condition fn index 9) returns hardcoded 0.0
- **Severity**: LOW · **Dimension**: 6 · **Location**: `crates/scripting/src/condition.rs:246-256` · **Status**: Existing: #1663 (lone survivor of #1316's "6 stub branches") · **Effort**: small
- **Description**: Logs "AVIF→ActorStats key resolver deferred" and returns 0.0 unconditionally — `param_1` is an AVIF FormID but `ActorStats` is string-keyed; resolver unwired. The other 5 #1316 branches are now implemented + tested, so **#1316 has narrowed to #1663 and can be closed in its favor**. Reachable via `script.activate` console path but no CLI flag/smoke test feeds a GetActorValue CTDA → not promoted. Do not re-file.

#### TD6-003: fragment lowerer implemented but its population path is unwired
- **Severity**: LOW · **Dimension**: 6 · **Location**: `crates/scripting/src/translate/effects.rs::lower_fragment` · `crates/scripting/src/fragment.rs:177-249` · **Status**: Existing: #1739 · **Effort**: medium
- **Description**: `lower_fragment` is complete + tested but only called from tests/examples. Consumer `quest_fragment_dispatch_system` reads `QuestStageFragments`, which is `default()`-empty at runtime (the QUST VMAD decoder that would fill it doesn't exist — fragment.rs:18 "Population (pending)"), so it early-returns every real frame. Not reachable with effect → not promoted. Do not re-file.

#### TD7-001: Skin-shader workgroup size `64` is a hand-written literal triplicated outside the generated-constants pipeline
- **Severity**: LOW · **Dimension**: 7 · **Location**: `skin_vertices.comp:40`, `skin_palette.comp:36`, `skin_compute.rs:37` · **Status**: NEW (lockstep risk partially closed by #1319's string-scan test) · **Effort**: small
- **Description**: Unlike every other compute shader (sources `local_size_x` from generated `WORKGROUP_X`/`THREADS_PER_CLUSTER`), the two skin shaders hard-write `64` and the Rust dispatch carries its own `WORKGROUP_SIZE=64` — 3 places, none the canonical `shader_constants_data.rs`. **LOW not HIGH** because `skin_compute.rs:1242` string-scans both GLSL sources against the Rust const and fails the build on drift. Residual debt is architectural (bypasses the mandated header pipeline).
- **Suggested Fix**: Add `SKIN_WORKGROUP_SIZE` to `shader_constants_data.rs`, emit the `#define` in `build.rs`, replace both shader literals + re-export the Rust const.

#### TD7-002: `NON_COHERENT_ATOM_SIZE = 256` hardcodes a device limit instead of querying `PhysicalDeviceLimits`
- **Severity**: LOW · **Dimension**: 7 · **Location**: `crates/renderer/src/vulkan/buffer.rs:389` · **Status**: Existing (prior REN-D2-NEW-03 INFO, not ticketed) · **Effort**: medium
- **Description**: `aligned_flush_range` rounds to 256 instead of `nonCoherentAtomSize`. **Not HIGH** — 256 is the spec's largest realistic atom size; every device reports ≤256, so it only ever over-aligns (wastes a few KB/frame), never under-aligns → no spec violation. Doc comment already explains the deliberate fallback. Suggest a debug-assert `limits.non_coherent_atom_size <= 256` at device create until limits are plumbed.

#### TD8-001: `mswp::peek_path_filter` dead, reserved for CLOSED #584 — already deleted, issue still OPEN
- **Severity**: LOW · **Dimension**: 8 · **Location**: `crates/plugin/src/esm/records/mswp.rs` (gone) · **Status**: Existing: #1704 (OPEN) — recommend CLOSE · **Effort**: trivial
- **Description**: Removed in `eb71bcb9`; `grep peek_path_filter` → 0 hits. mswp.rs now exposes only `parse_mswp`. Pure rot already excised; the OPEN issue is stale bookkeeping. Verify-and-close.

#### TD8-002: `oriented_quad` is a dead `pub fn` (0 callers, never wired)
- **Severity**: LOW · **Dimension**: 8 · **Location**: `crates/renderer/src/mesh.rs:1170-1209` + re-export `lib.rs:9` · **Status**: NEW · **Effort**: trivial
- **Description**: Introduced `7b8c0752` (Cornell harness) but `cornell.rs` used `uv_sphere`+`box_vertices_colored` instead. `grep oriented_quad` → 2 hits (def + re-export only). Delete both; ByroRedux has no external consumers so the re-export is pure surface rot.

#### TD8-003: `fullscreen_quad_vertices` is a dead `pub fn` superseded by the UI variant
- **Severity**: LOW · **Dimension**: 8 · **Location**: `crates/renderer/src/mesh.rs:1211-1241` · **Status**: NEW · **Effort**: trivial
- **Description**: Introduced `340f1fbc` (M20 UI); `grep` → 1 hit (def only), not in the re-export list. Live sibling `fullscreen_quad_ui_vertices` (emits `UiVertex`) is the used one (resources.rs). Plain-`Vertex` version is refactor residue. Delete.

#### TD8-004: `Dx10Chunk::end_mip` set-but-never-read; sibling `start_mip` graduated to live
- **Severity**: LOW · **Dimension**: 8 · **Location**: `crates/bsa/src/ba2.rs:150-151` · **Status**: NEW · **Effort**: trivial
- **Description**: The `#[allow(dead_code)]` at 144-151 reserves both for M40 (#1049), but `start_mip` is now read (monotonic-order check at 621/626), so **its attribute is now redundant** — that's the actionable bit. `end_mip` is written (600) and never read. Lowest-risk: drop the stale `start_mip` attribute; keep `end_mip` as the documented #1049 reserve or delete it.

#### TD8-005: `RawDependency.name` parsed from TOML then dropped
- **Severity**: LOW · **Dimension**: 8 · **Location**: `crates/plugin/src/manifest.rs:70-75` · **Status**: NEW · **Effort**: trivial
- **Description**: `RawDependency { uuid, #[allow(dead_code)] name }`; `from_toml` maps only `uuid`. No `#[serde(deny_unknown_fields)]`, so the `name` field isn't needed to parse a `[[dependencies]]` block. Classic "silence the warning instead of deleting." Delete the field (serde ignores the key), or propagate into a public `PluginDependency { id, name }` if display-names are a near-term feature (default: delete).

#### TD9-001: NIF heap-allocation regression test never runs in CI (feature dormant)
- **Severity**: LOW · **Dimension**: 9 · **Location**: `crates/nif/tests/heap_allocation_bounds.rs:30` (`#![cfg(feature = "dhat-heap")]`); `crates/nif/Cargo.toml:28`; `.github/workflows/ci.yml:31,59` · **Status**: NEW · **Effort**: trivial
- **Description**: The file is module-gated on opt-in `dhat-heap`; its header says "CI should run this alongside the default test job," but `ci.yml` runs only default features, so neither heap-budget test executes in CI. The two tests pin 4 allocation-hygiene fixes (#832/#833/#831/#408); a regression won't fail CI.
- **Suggested Fix**: Add a dedicated CI step `cargo test -p byroredux-nif --features dhat-heap --test heap_allocation_bounds` (own job — dhat installs a `#[global_allocator]`), or fix the file comment if CI execution is intentionally out of scope.

#### TD9-002: `ZZZ_probe_…` physics test ships dev-probe scaffolding
- **Severity**: LOW · **Dimension**: 9 · **Location**: `crates/physics/src/water.rs:649` + `PROBE:` eprintln at `:683,687,691,694` · **Status**: NEW · **Effort**: trivial
- **Description**: An otherwise-valid passing test (real `assert_eq!`s present) carries a `ZZZ_probe_` name prefix (sorts last, marks a temporary investigation) and four `eprintln!("PROBE: …")` diagnostics — leftover debug instrumentation in committed code (commit `1645112ca`, 2026-06-20). Rename to a descriptive name and delete the eprintln lines.

---

## 6. Notes / Non-findings (recorded so the next cycle doesn't re-investigate)

- **#1709 (vol→dB) — ALREADY FIXED** by `eb71bcb9`: `linear_volume_to_db()` (lib.rs:144) is the single source, all 3 sites route through it. No other `20.0*log10` in the tree. **Mark closeable** (Dim 2).
- **TD1-009** — the ~260-arm NIF block dispatcher `match type_name` (`blocks/mod.rs:302`) is idiomatic static dispatch; a `HashMap<&str, fn>` would lose inlining and gain nothing. **Not actionable** — documented so a future audit doesn't re-propose the lookup-table conversion.
- **TD1-010** — `ecs/components/mod.rs` 27 `pub use` is a pure facade (namespace flattening), no logic. The ">20 = two jobs" heuristic doesn't apply. Next-largest hubs (`esm/records/mod.rs`=17, `ecs/mod.rs`=16) are under threshold.
- **Verified-clean (no rot):** `shader-pipeline.md` (336/300 correct), `memory-budget.md`, CLAUDE.md vertex line (100 B field-by-field), README examples, all 8 `Material::classify_pbr` doc-comments (correctly frame the deleted symbol as historical — recurring trap NOT present), feature-matrix M45/M47.2 rows (#1699/#1703 fix held). `docs/audits/*` 260/272/280-B GpuMaterial quotes are correctly-dated historical snapshots.
- **Dim 2 consolidated-clean:** ESM SubReader, NIF NiObjectNET/NiAVObject/particle bases, texture-upload chain (#730), barrier/descriptor-pool helpers (93 call sites), cell_loader/euler coord flips.
- **Dim 4 swept-clean:** all 47 `path::symbol` anchors resolve (incl. the two audio fns); no closed-issue "Existing: #NNN" callouts; nifal=9/renderer=21 dimension counts accurate; oldest `docs/audits/` report is 2026-04-02 (within 90 days).
- **Dim 8 EXCLUDE (legit, keep):** query.rs RAII guard fields, `LightHeader.count` wire-contract, `main.rs:409` debug_server RAII, bs_tri_shape #336 reserve, components.rs:783 #1199 hook, legacy/mod.rs #390 bridge, cfg(test)/cfg(debug) sites. No `// removed:` breadcrumbs, no `#[deprecated]`, no single-branch feature flags, no dead `_`-params. `ErasedComponent*` re-export is load-bearing (false alarm).
- **Dim 9 regression guards all intact** and not `#[ignore]`d: golden_frames, `fnv_ignores_16byte_acbs`, refr_texture_overlay (#584), #170 header guard, Starfield glass-flag guards, concurrency KPI asserts. 112/112 `#[ignore]`s are GPU/data-gated.

## 7. Deferred (gated on an in-progress milestone)

- **TD6-002** (GetActorValue stub) — gated on the AVIF-FormID→name resolver (M47.1, #1663).
- **TD6-003** (fragment lowerer unwired) — gated on the QUST VMAD fragment decoder (#1739).
- **TD7-002** (NON_COHERENT_ATOM_SIZE) — gated on plumbing `PhysicalDeviceLimits` onto `VulkanContext`.

---

## 8. Recommended GitHub Actions

- **Close** #1627, #1704, #1709 (verify-and-close — fixed in-tree by `eb71bcb9`).
- **Narrow/close** #1316 in favor of #1663 (5 of 6 stub branches now implemented).
- **Reopen #1052 or file fresh** for the draw_frame regression (TD1-001).
- **Refresh umbrella #1323** — drop `asset_provider` (split/closed), add `draw.rs` / `context/mod.rs` / `collision.rs` / `particle.rs`.
