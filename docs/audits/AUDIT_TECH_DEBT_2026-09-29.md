# Tech-Debt Audit — 2026-09-29

**HEAD**: `9fcfdc3fc` · **Baseline**: `docs/audits/AUDIT_TECH_DEBT_2026-09-22.md` (HEAD `ee6d3fb39`, 272 commits /
1 090 files / +90 102 −12 639 ago) · **Audited**: all 9 dimensions, delta-first (deep) · **Unchanged since
baseline (skimmed)**: none. Every dimension's paths had commits in the window. Dims 5 and 6 were reviewed in
full and came out clean.

This report was written by a single agent during the `/audit-suite --preset comprehensive` run. No sub-agents
were used and nothing was fixed. The findings were deduplicated against open issues (`/tmp/audit/issues.json`,
163 open), against closed issues via `gh --search`, and against the 21 sibling `*_2026-09-29.md` reports. Doc
rot those reports already file is cross-referenced, not re-filed.

## Executive Summary

| Severity | NEW | Regression | Total |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 1 | 1 | 2 |
| LOW | 16 | 4 | 20 |

- **The fix→issue loop is broken, and so is the gate that watches it.**
  - Mis-titled commits fixed about 30 issues without naming them, and those issues are still OPEN
    (TD4-01). Examples: `2f8538334` "Enhance audio systems…", `63c0aee3b` "Refactor environment
    variable usage…", `88c23887b`, `0e0d35b96`, `546366364`.
  - Today, five or more sibling audits spent passes re-verifying them.
  - The CI job built to annotate exactly those commits (`issue-traceability`, #3504) has failed on
    **every push since it was enabled on 09-07**, with `rg: command not found`. Its annotate step has
    never run (TD9-01).
  - The two findings share one root cause. The CI fix is one line.
- **CI at HEAD: 5 of 10 jobs are red.**
  - clippy on rustc 1.98.1: SAFE-D4-2026-09-29-02.
  - ABBA lock order: ECS-2026-09-29-D1-01.
  - Vulkan validation: #4987 / CONC-D2.
  - traceability: TD9-01.
  - shader parity: TD9-02. The container has no `python3`, so #2835's FSR bench self-test has not run
    once in 6 weeks.
  - A permanently red board trains readers to ignore it. The local 1.96 `clippy --workspace` gate is
    green (#4765 holds).
- **Oversized files: 2 → 7.** Three are regressions of closed splits:
  - `context/draw.rs` 2211 (#4767 — the draw_frame line budget holds, but the file regrew around it)
  - `groundcover.rs` 2025 (#4568)
  - `volumetrics.rs` 2391 (#2256)

  Four crossed for the first time: `cornell.rs` 2494, `npc_spawn/resumable.rs` 2235,
  `esm/records/actor/mod.rs` 2120 and `streaming.rs` 2069. Each has a proposed split axis below.
  `storage_util.rs` left the list via the #4768 split.
- **Always-loaded agent docs have rotted.**
  - CLAUDE.md names two functions deleted in June/August and a type that never existed.
  - AGENTS.md is an 84-line-divergent fork of it, and still recommends the pre-#3895
    `cargo test -p byroredux-core` trap.
- **Duplication.**
  - Every shader constant is hand-typed three times (data, build.rs emitter, pin list).
  - The `production_text` source-scan cut is copied into 9+ places using 3 different needles. One copy
    falls back to scanning the whole file.
  - The 8-lane terrain splat budget is a bare `8` in about 17 places. The newest copy was added after
    #4496 closed.

## Baseline Snapshot (re-measured at `9fcfdc3fc`; `prod_loc` self-test ok)

```text
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 21   (09-22: 22)   15 are ESM XXXX false positives
allow(dead_code):                             27   (09-22: 28)
unimplemented!/todo!():                        0   (09-22: 0)
#[ignore] tests:                             250   (tools-inclusive; crates+byroredux 249 vs 221 on 09-22, +29 added in window, all data-gated)
files >2000 production LOC:                    7   (09-22: 2)
test files >2000 total LOC:                   61   (09-22: 53)
clippy --workspace (CI gate, rustc 1.96):      0 errors  (GREEN locally; RED on CI's 1.98.1 — SAFE-D4-2026-09-29-02)
clippy --workspace --all-targets --keep-going: 824 unique sites / 806 errors (09-16: 698)
_audit-validate.sh:                           OK — 65 basename + 267 docs/engine symbol advisories (267 flat)
```

## Top 10 Quick Wins (trivial / small)

1. **TD9-01**: install ripgrep in the `issue-traceability` job. One line revives the push annotations.
2. **TD9-02**: add `python3` to the shader-parity container, or split the FSR self-test into its own job.
3. **TD4-01**: close the ~30 stale-open issues that sibling reports verified fixed, citing the carrying
   commit. This is the sweep half.
4. **TD3-01**: fix CLAUDE.md's `build_blas_for_mesh`, `debug_assert_scratch_aligned` and `GameArchive`
   lines, plus the facegen comment.
5. **TD3-02**: reduce AGENTS.md to a pointer to CLAUDE.md.
6. **TD3-03**: correct the two feature-matrix rows (player mesh, dialogue).
7. **TD4-03**: change `triangle_early.frag` to `triangle_early.frag.spv` in two skills.
8. **TD8-01**: delete the 11 throwaway probes and widen the `_tmp_` guard to `tmp_`.
9. **TD2-02**: hoist `production_text` into one shared module and make anim_convert's cut `.expect()`.
10. **TD7-01**: name `TERRAIN_SPLAT_LAYERS`, emit it to GLSL, and replace the four loops and three arrays.

## Top 5 Medium Investments

1. **TD1-01 + TD1-08**:
   - Move the pure frame-parameter builders out of `context/draw.rs` into `context/frame_params.rs`.
   - Add a file-level prod_loc pin beside the `draw_frame` line budget.
   - Give `about_to_wait` / `render_one_frame` the same budget pin.
2. **TD2-01**: a single `SHADER_DEFINES` table drives both the build.rs emitter and the header pin. This
   replaces 315 hand-written `writeln!` calls.
3. **TD1-04**: split `npc_spawn/resumable.rs` along its two state machines (runtime FaceGen vs prebaked).
4. **TD1-03 / TD1-05 / TD1-06 / TD1-07**: four responsibility splits.
   - cornell harness scenes
   - streaming telemetry vs pre-parse pipeline
   - actor records per record type
   - volumetrics fog clustering + combustion
5. **TD4-01 (automation half)**: `/session-close` or `/audit-publish` harvests reports' "Fixed; close"
   lines, plus a `Fix #N` / `Refs #N` convention check.

---

## Findings

### MEDIUM

### TD4-2026-09-29-01: Mis-titled fix commits leave about 30 fixed issues OPEN, and today's audits spent passes re-verifying them
- **Severity**: MEDIUM. Promotion rule: a stale audit baseline that misled audits in the last 90 days.
- **Dimension**: 4 — Audit-Finding Rot (process debt)
- **Location**: git history `ee6d3fb39..HEAD`; the open-issue pool (`/tmp/audit/issues.json`)
- **Status**: NEW
- **Age**: 2026-09-23 → 2026-09-28
- **Effort**: small (sweep) / medium (automation)
- **Description**: several commits fixed issues whose numbers appear nowhere in the message, so GitHub
  never closed them. Under the dedup protocol (`_audit-common.md` step 3: "OPEN → Existing, skip") each
  one reads as live debt, so every audit re-verifies it.
- **Evidence**:
  - `2f8538334` "Enhance audio systems and combat animations" (09-26). I verified these in the diff:
    - #4750: `MAX_OBSCRIPT_VM_NESTING`, `exec_if_chain` / `run_arm_body` depth.
    - #4751: the `ObScriptDiagnostics` doc link.
    - #4749: the `material_translate.rs` SpeedTree placeholder PBR.
    - Plus the #4748 doc half.
  - `546366364` "feat(audio): integrate SoundCache…" (09-27) carries #4739, per the audio report.
  - `63c0aee3b` "Refactor environment variable usage for Skyrim SE data path" (09-27, 43 files) carries
    #4753–#4756 and #4758–#4760, per the tooling report. I verified #4760 myself: the only
    `BYROREDUX_SKYRIM_DATA` reader left is the deliberate `.github/workflows/playable-smoke.yml:45`
    fallback.
  - `88c23887b` "feat(renderer): Enhance volumetrics and groundcover models with GPU timers" carries
    #4866. I verified `model_timer_encloses_all_phases_and_stats_copy` at `groundcover_models.rs:958`.
    Per the renderer report it likely also carries #4783.
  - `0e0d35b96` "Refactor Vulkan Renderer Code and Update Documentation" carries #4868. I verified
    `gpu_timers.rs:461-468`.
  - `b7491072f` "Refactor code structure and remove redundant changes" carried #4620–#4625. Those were
    hand-closed at 12:17Z today.
  - `b9e961eeb`, titled as lighting docs, carried the Starfield units module and +392 LOC of Cornell
    oracle.
  - Still OPEN at HEAD while a sibling report says fixed:
    - audio: #4739, #4740, #4741, #4742, #4744, #4745, #4746
    - speedtree: #4749
    - scripting: #4750, #4751
    - tooling: #4753, #4754, #4755, #4756, #4758, #4759, #4760
    - UI: #3429
    - legacy-compat: #4127
    - renderer: #4778, #4783, #4785, #4862–#4865, #4867, #4868, #4872, #4880. The renderer report
      re-verified only #4868.
  - Signs that audits were misled:
    - The exterior report calls #4866 "the second report recommending closure".
    - The renderer report carried a 13-issue open-but-fixed list forward from 09-27.
    - The audio, scripting, speedtree, tooling, UI and legacy-compat reports each spent a disposition
      pass on these.
- **Impact**:
  - About 30 of the 163 open issues are phantom, and every audit pays to re-verify them.
  - A real regression can hide behind a stale "Existing: #N".
  - The inverse failure (#4768: closed while the debt remained) shows the pool is unreliable in both
    directions.
- **Related**: TD9-2026-09-29-01 (the gate that should have flagged these commits has been dead since
  09-07); #4768; memory notes *Multi-issue Commit Close* and *perf_fix_status_unclosed_issues*.
- **Suggested Fix**:
  - Close each issue listed above that has a sibling-report verification line, citing its commit.
  - Have `/session-close` (or `/audit-publish`) harvest the "Fixed; close" / "stale-open" lines that
    reports already write, and list them for closure.
  - Fix TD9-01 so pushes get annotated again.

### TD9-2026-09-29-01: The issue-traceability CI gate has failed on every push since it was enabled (09-07): `rg: command not found`
- **Severity**: MEDIUM. It is the root cause of TD4-01, and #3218 and #3504 were both closed as fixes for
  this class.
- **Dimension**: 9 — Test Hygiene (CI lanes)
- **Location**: `.github/workflows/ci.yml:14-52` (job `issue-traceability`, `runs-on: ubuntu-latest`,
  with no ripgrep install); `scripts/check-issue-traceability.sh:9-11` and every `rg` call after them
- **Status**: Regression of #3504
- **Age**: `798c31651` (2026-09-07, "Fix #3504: run the traceability gate on pushes to main")
- **Effort**: trivial
- **Description**: the job's first step, `scripts/check-issue-traceability.sh --self-test`, dies because
  the runner has no `rg`. The job never reaches "Annotate uncited fixes and uncited closures on the pushed
  range" (`ci.yml:39-43`), which is the step #3504 added for direct pushes to main.
- **Evidence**:
  - HEAD run `36609043348`, job `109545444545`, fails with:
    ```
    scripts/check-issue-traceability.sh: line 9: rg: command not found
    scripts/check-issue-traceability.sh: line 11: rg: command not found
    ##[error]Process completed with exit code 1.
    ```
  - Sampled main runs 09-08 23:17, 09-09, 09-11, 09-12, 09-14, 09-15, 09-16, 09-19, 09-22, 09-24, 09-25,
    09-26, 09-28 and 09-29 are all `failure`. Runs on 09-02 → 09-07 14:28 were `skipped` (the old PR-only
    condition).
  - The only job that installs ripgrep is shader parity (`ci.yml:113`).
  - Locally the self-test passes: `check-issue-traceability: self-test passed`.
- **Impact**:
  - The push-direction traceability signal has not existed for three weeks.
  - Every mis-titled commit in TD4-01 would have been annotated at push time, while its author still had
    the context.
  - The self-test prints nothing of its own on a missing tool, so the red job looked like noise.
- **Related**: #3504, #3218, #3425, #3538 (CLOSED); TD4-2026-09-29-01.
- **Suggested Fix**:
  - Install ripgrep in the job (`sudo apt-get install -y ripgrep`), or fall back to `grep -E` when `rg`
    is absent. Have the script check for its tools with `command -v rg` and a clear message.
  - Once green, run `--push ee6d3fb39 HEAD` once to annotate the window.

### LOW — Dimension 1 (File / Function Complexity)

### TD1-2026-09-29-01: `context/draw.rs` regrew to 2211 production LOC; the #4767 budget pins only `draw_frame`
- **Severity**: LOW · **Dimension**: 1 · **Status**: Regression of #4767 · **Effort**: small
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:29-2033` (helpers), `:2034` (`draw_frame`,
  714 lines), `draw_frame_stays_within_its_line_budget` (`:4098`, cap 720)
- **Age**:
  - `e17b4b653` (Fix #4767, 09-23) cut the file to 1982.
  - `0572bfd5a` took it back to 2051 the same day.
  - `88c23887b` (+106) and the 09-28 fix batches brought it to 2211.
- **Description**: the growth landed in the pure helper functions above `draw_frame`, which the
  function-scoped budget cannot see: `build_composite_params` (~260 lines), `build_sky_cube_params`,
  DoF/FSR parameter builders, camera deltas and jitter. `draw_frame` itself sits at 714 of 720.
- **Suggested Fix**:
  - Move lines 29–2033 (pure fns, with their test mods) to `context/frame_params.rs`.
  - Repoint the `include_str!("draw.rs")` scans in `bloom.rs`, `sync.rs`, `sky_dome.rs`,
    `post_passes.rs`, `context/{mod,resources,build_and_upload_instances}.rs`, `skin_compute.rs` and the
    draw.rs self-scans.
  - Add a file-level prod_loc pin beside the function budget.

### TD1-2026-09-29-02: `groundcover.rs` re-crossed 2000 (2025)
- **Severity**: LOW · **Dimension**: 1 · **Status**: Regression of #4568 · **Effort**: small
- **Location**: `crates/renderer/src/vulkan/groundcover.rs`
- **Age**: #4568 extracted `groundcover_stats.rs`, leaving 1982. Then `aabd99a05` (#4413 model tier,
  09-24), `21430f45e` (#4729) and `c14f5361a` (#4056) pushed it back over.
- **Suggested Fix**: split construct from record.
  - Construct: `new`, `create_buffers`, `create_layouts`, `set_layout_contracts`, `create_descriptors`,
    `build_*pipelines` (≈421–1200).
  - Per-frame: `prepare`, `harvest`, `write_descriptor_sets`, `record_scatter`, `record_interaction`,
    `record_draw` (≈1205–1940).
  - This is the axis that worked for `context/` and volumetrics.

### TD1-2026-09-29-03: `cornell.rs` crossed to 2494 (+580) and hosts five unrelated harness scenes
- **Severity**: LOW · **Dimension**: 1 · **Status**: NEW · **Effort**: small
- **Location**: `byroredux/src/cornell.rs`
- **Age**: `b9e961eeb` (+392, titled as lighting docs, 09-23); `ff1b48d7c` (+189 shared-mesh oracle, 09-23)
- **Description**: the file holds five scenes, each on its own lines:
  - classic Cornell (`setup_cornell_scene` :1620)
  - the RT oracle ladder (`setup_cornell_oracle_scene` :715, a 363-line function)
  - glass dragon (:1229–1442)
  - combustion lab (:1443–1619)
  - godray lab (:94–287)

  It also holds the shared material/spawn builders and `MeshBuilder`.
- **Suggested Fix**: `cornell/{mod.rs (mode-flag parsing), oracle.rs, glass_dragon.rs, combustion_lab.rs,
  godray_lab.rs, builders.rs}`. Grep `include_str!("cornell.rs")` first.

### TD1-2026-09-29-04: `npc_spawn/resumable.rs` crossed to 2235 (+565)
- **Severity**: LOW · **Dimension**: 1 · **Status**: NEW · **Effort**: small–medium
- **Location**: `byroredux/src/npc_spawn/resumable.rs`
- **Age**: `5570c221c` (+661 head-part textures, 09-23); `a070baaad` (+340 player body, 09-28).
  `advance_runtime_unit` is now 518 lines (+92).
- **Suggested Fix**: the two spawn arms are already separate state machines. Split them into
  `resumable/runtime.rs` and `resumable/prebaked.rs`, and keep `NpcSpawnJob` + part parenting in `mod.rs`.
  - Runtime FaceGen: `RuntimeNpcState`, `prepare_runtime_state`, `advance_runtime_unit`,
    `spawn_runtime_head`, head morphs, hair tint (≈443–1800).
  - Prebaked: `PrebakedNpcState`, `prepare/advance/finalize_prebaked` (≈1800–2154).

### TD1-2026-09-29-05: `streaming.rs` crossed to 2069 (+316)
- **Severity**: LOW · **Dimension**: 1 · **Status**: NEW · **Effort**: small
- **Location**: `byroredux/src/streaming.rs`
- **Age**: `a3632909a` (texture prefetch, 09-27) and the #3659/#4999/#5000 archive-read fixes
- **Suggested Fix**: split three ways.
  - Telemetry (`StreamingLatencySummary`, `StreamingTelemetry`, `phase_distribution`; 48–470) →
    `streaming/telemetry.rs`.
  - Worker + pre-parse pipeline (`join_with_timeout` … `pre_parse_cell`, `ParseInputBudget`; 1114–1937)
    → `streaming/pre_parse.rs`.
  - State and deltas stay.

### TD1-2026-09-29-06: `esm/records/actor/mod.rs` crossed to 2120 (+125)
- **Severity**: LOW · **Dimension**: 1 · **Status**: NEW · **Effort**: small
- **Location**: `crates/plugin/src/esm/records/actor/mod.rs`
- **Age**: `9789d8153` (#4414 faction hostility, +80); `5570c221c`; `cd4fc019a` (#4415 magic)
- **Suggested Fix**: split by record, and re-export from `mod.rs` so `records::actor::*` paths stay put.
  - `npc.rs`: `NpcRecord` + `parse_npc*`
  - `race.rs`: `RaceRecord`, `head_part`, and `parse_race` (a 358-line function)
  - `class.rs`
  - `faction.rs`

### TD1-2026-09-29-07: `volumetrics.rs` is back over the line at 2391 (+490)
- **Severity**: LOW · **Dimension**: 1 · **Status**: Regression of #2256 · **Effort**: small
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs`
- **Age**: `a5dfd472d` (#4781, +93), `7859a1b04` (#4775), `29e62cf2f` (#4776), `4ec1e48c5`
  (#4959–#4970). #2256 had moved construction into `volumetrics/init.rs`.
- **Suggested Fix**:
  - Move fog-volume clustering (`GpuFogVolume`, cluster build, portal sweep, grid filter; ≈202–1060,
    about 850 LOC) to `volumetrics/fog_clusters.rs`.
  - Move combustion light moments (≈1190–1370 plus `append_combustion_surface_lights`) to
    `volumetrics/combustion.rs`.
  - Repoint the `include_str!("volumetrics.rs")` scans in `caustic.rs`, `svgf.rs` and `context/draw.rs`.

### TD1-2026-09-29-08: Per-frame driver functions keep regrowing after #4342; only `draw_frame` has a budget
- **Severity**: LOW · **Dimension**: 1 · **Status**: Regression of #4342 (partial) · **Effort**: medium
- **Location**:
  - `byroredux/src/app_events.rs:611` `about_to_wait`: 969 lines. It was 822 when #4342 was filed, and
    +104 in this window.
  - `crates/renderer/src/vulkan/context/geometry_pass.rs:21` `record_geometry_pass`: 763 (+131).
  - `byroredux/src/app_frame.rs:117` `render_one_frame`: 656. #4342 extracted it down to 591.
  - `byroredux/src/asset_provider/material/merge.rs:469` `merge_bgsm_arm`: 669 (+111).
- **Evidence**:
  - `about_to_wait` growth came from `6d05c2bc0` (#4992 boot guard), `4790227ce` (#4208 pin),
    `a070baaad` (view toggle), `0182fc5e8` (gear import) and `766e1746e` (dialogue). Each added a hook
    inline.
  - Functions that newly crossed 200 lines: `prepare_mesh_upload_range` 259, `npc_combat_ai_system_inner`
    233, `refresh_scene_actor_bindings` 228, `trigger_detection_system` 227, `spawn_object_lod_quad` 221,
    `pre_parse_cell` 216, `fill_scratch_telemetry` 216.
- **Suggested Fix**:
  - Extract the per-feature hooks (dialogue, gear import, view toggle) as `App` methods *inside*
    `app_events.rs`, since its `include_str!` scans at :1746/:1860/:1901 must keep matching.
  - Add budget pins like `draw_frame`'s for `about_to_wait` and `render_one_frame`.

### LOW — Dimension 2 (Logic Duplication)

### TD2-2026-09-29-01: Every generated shader constant is hand-typed three times
- **Severity**: LOW · **Dimension**: 2 · **Status**: NEW · **Effort**: medium (mechanical)
- **Location**:
  - `crates/renderer/build.rs:39-1454`: `main()` is one 1415-line function made of 315 `writeln!` calls
    (+105 lines this window).
  - `crates/renderer/src/shader_constants_data.rs`: 369 `pub const`s.
  - `crates/renderer/src/shader_constants.rs:888` `generated_header_contains_all_defines`: a hand
    pin-list of `(name, format!("#define NAME {NAME}u"))` tuples.
- **Description**:
  - One new GLSL constant takes three edits: declare it, hand-write its emitter (choosing the `u` suffix
    or the `{:?}` float format by hand), and add a matching pin tuple.
  - The pin re-types the same format string, so a wrong suffix typed twice passes.
  - The provenance gate stops shaders from redeclaring constants, but nothing derives the emitter from
    the data.
- **Suggested Fix**:
  - Declare `SHADER_DEFINES: &[(&str, ShaderValue)]` in `shader_constants_data.rs`, with variants
    `Uint/Int/Float/Vec3/Raw` and a `Section` variant for the grouping comments.
  - build.rs iterates it to write the header.
  - The test asserts that each entry renders into the header, and that `shader_constant_data_names()`
    equals the table.

### TD2-2026-09-29-02: The source-scan `production_text` cut is copied into at least 9 places using 3 different needles
- **Severity**: LOW · **Dimension**: 2 · **Status**: NEW · **Effort**: small
- **Location**:
  - Helpers:
    - `crates/renderer/src/source_scan.rs:21`
    - `crates/physics/src/source_scan.rs:21`, a documented copy: "cfg(test) items do not cross crate
      boundaries"
    - `byroredux/src/extensions/mod.rs:522`, its own brace-stripping `-> String` version
  - Inline copies:
    - `byroredux/src/app_events.rs:1746/1860/1901` (`"\n#[cfg(test)]\nmod "`)
    - `byroredux/src/anim_convert.rs:485` (`"\n#[cfg(test)]\n"` + `.unwrap_or(src)`)
    - `crates/core/src/ecs/resources/skin_slot_pool.rs:1112`, `crates/renderer/src/vulkan/water.rs:1902`
      and `frame_upscaler.rs:1320` (`"\n#[cfg(test)]"`, which also cuts at a `#[cfg(test)] use`)
- **Evidence**:
  - `_audit-common.md` makes `production_text` the house rule for source scans (#4604/#4842). Only
    renderer and physics can call it.
  - The byroredux bin has 201 `include_str!` scans and 5 uses. core, nif, plugin, scripting and ui have
    12–31 scans each and no helper.
  - `anim_convert.rs:485` falls back to the whole file when the cut misses. Its positive needle
    `resolve_flip_texture_for_role` also appears in its own assertion, so a lost cut makes the guard
    vacuous. That is the #4842 defect class.
- **Suggested Fix**:
  - Put one non-`cfg(test)` `#[doc(hidden)] pub mod source_scan` (`production_text`, `rust_files`) in
    `byroredux-core`, or in a tiny dev-dependency crate.
  - Delete the copies.
  - Turn `.unwrap_or(src)` into `.expect(...)`.

### LOW — Dimension 3 (Stale Documentation)

### TD3-2026-09-29-01: CLAUDE.md's Workspace Structure names two functions deleted months ago and a type that never existed
- **Severity**: LOW · **Dimension**: 3 · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Location**:
  - `CLAUDE.md:125` (`resources.rs  build_blas_for_mesh, register_ui_quad, …`)
  - `CLAUDE.md:128` (`mod.rs  … new()/destroy()/debug_assert_scratch_aligned()`)
  - `CLAUDE.md:66` (`archive.rs  GameArchive — wraps BSA … or BA2`)
  - `crates/facegen/src/eval.rs:116`
- **Evidence**:
  - `build_blas_for_mesh` was deleted by `999478ef4` (2026-08-15, #2914, "delete the dead single-shot
    BLAS path"). `resources.rs:323` now says "never-called single-shot `build_blas_for_mesh`".
  - `debug_assert_scratch_aligned` was deleted by `d6d0516f9` (2026-06-01).
  - `GameArchive` appears in no `.rs` file in the whole history; the type is
    `asset_provider::archive::Archive`.
  - `facegen/src/eval.rs:116` still says `out` "reaches the vertex SSBO and `build_blas_for_mesh`".
  - `_audit-validate.sh` covers skills and docs/engine only, not CLAUDE.md.
- **Impact**: CLAUDE.md is loaded into every agent session, and `_audit-common.md` names it as the
  authoritative tree.
- **Suggested Fix**:
  - Replace the three names with `build_blas_batched`, the round-up-at-use note, and `Archive`.
  - Fix the facegen comment.
  - Point the validator's symbol advisory at CLAUDE.md and AGENTS.md too.

### TD3-2026-09-29-02: AGENTS.md is a divergent fork of CLAUDE.md (84 differing lines) and still recommends the #3895 test trap
- **Severity**: LOW · **Dimension**: 3 · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Location**: `AGENTS.md` (added `db625997b` 2026-07-27; last edited `76c4521cf` 2026-09-23)
- **Evidence**:
  - `AGENTS.md:10` reads `cargo test -p byroredux-core    # Run ECS/core tests (162 tests)`. That is the
    exact command CLAUDE.md:16-25 warns silently drops the #486 inspect-gated guards (#3895).
  - It carries a "rustc ≥ 1.94 / distro rustc 1.93.1" toolchain section that CLAUDE.md dropped.
  - It repeats the `GameArchive` line.
  - Neither file refers to the other.
- **Suggested Fix**:
  - Fold any still-true fact unique to AGENTS.md into CLAUDE.md.
  - Replace AGENTS.md with a pointer, or a symlink, to CLAUDE.md.

### TD3-2026-09-29-03: docs/feature-matrix.md contradicts the player body and dialogue features shipped 09-28/09-29
- **Severity**: LOW · **Dimension**: 3 · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Location**:
  - `docs/feature-matrix.md:246`: "container/corpse transfer, visible player-mesh attachment, general HUD
    bars, and quest-objective presentation remain open".
  - `docs/feature-matrix.md:207`: "Dialogue tree + dialogue UI integration | ✗ M43 remainder".
- **Evidence**:
  - The player mesh shipped in `a070baaad` (body + view toggle), `db8351587` (third-person walk/idle) and
    `0182fc5e8` (mid-life gear import). `ROADMAP.md:239` lists it with `p3-player-body.sh`.
  - Dialogue shipped in `ab31cfefe` / `766e1746e` (NPC activation → topic selection, native response
    surface). `ROADMAP.md:240` says "live-verified in MarkarthWarrens".
  - The matrix was last touched 09-27.
  - The container-transfer clause should also be re-checked against `container_loot_system` (#4712).
- **Impact**: `_audit-common.md` names the matrix as the status floor audits re-check. These two rows
  would lead a gameplay or UI audit to scope shipped features as absent.
- **Related**: sibling matrix findings AUD-2026-09-29-D5-04 and CHAR-2026-09-29-D5-01 cover other rows.
- **Suggested Fix**:
  - Drop "visible player-mesh attachment" from row 246.
  - Mark row 207 as "~ single-level topic selection + native response surface (P4); tree/UI open".

### TD3-2026-09-29-04: game-loop.md's live-schedule table predates M42.10 and the hostility, dialogue and player-body systems
- **Severity**: LOW · **Dimension**: 3 · **Status**: NEW · **Effort**: small · **Kind**: doc-rot
- **Location**: `docs/engine/game-loop.md:136-160` (last edited `d10c84338`, 2026-09-05)
- **Evidence**:
  - The PostUpdate row still reads "opt-in sandbox/wander/travel/follow/escort/guard/patrol systems |
    Environment-gated NPC locomotion experiments".
  - Since M42.10 (09-18) these systems run by default, with `BYRO_NO_AI_LOCOMOTION` as the single
    kill-switch (`boot/schedule/post_update.rs:70-90`, pinned at `schedule/mod.rs:256-292`).
  - The table has no row for `npc_walk_animation_system`, which is registered last by rule.
  - No row covers these window additions:
    - `make_faction_hostility_system` (update.rs:243)
    - `make_npc_combat_ai_system` (:272)
    - `player_body_facing_system` (:586)
    - `make_npc_dialogue_selection_system` (late.rs:428)
    - `fragment_activation_flush_system`
  - These older systems are also absent: `restoration_system`, `equipment_appearance_system`, and the
    `extension_*` dispatch family (late.rs:485).
- **Impact**: `_audit-common.md` lists game-loop.md as a code-verified runtime trace.
- **Suggested Fix**:
  - Rewrite the locomotion row as default-on, with the kill-switch.
  - Add group rows for Update combat/hostility, Late dialogue selection + equipment appearance +
    extension dispatch, and player-body facing.

### LOW — Dimension 4 (Audit-Finding Rot)

### TD4-2026-09-29-02: session-close SKILL's README (<120 lines) and ROADMAP (~500) budgets are both broken
- **Severity**: LOW · **Dimension**: 4 · **Status**: NEW · **Effort**: small
- **Location**: `.claude/commands/session-close/SKILL.md:258` and `:317` (last edited `da3a437bf` 09-01)
- **Evidence**:
  - README.md is 533 lines. It was 382 on 08-01, 492 on 09-01 and 527 on 09-15.
  - ROADMAP.md is 759 lines, trimmed from 1657 on 09-22, and still 1.5× its budget.
  - The ritual runs every session (`8b334c102` today) and never enforces either rule.
- **Suggested Fix**:
  - Either trim README to quick start + pointers, or restate the budgets as measured ceilings.
  - Add a `wc -l` check to Step 6.

### TD4-2026-09-29-03: Two audit skills backtick a nonexistent `triangle_early.frag`
- **Severity**: LOW · **Dimension**: 4 · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Location**: `.claude/commands/audit-performance/SKILL.md:55,128,129`;
  `.claude/commands/audit-renderer/SKILL.md:38,110` (introduced by `9fcfdc3fc`, today)
- **Evidence**:
  - The early-test variant is `triangle.frag` compiled to `triangle_early.frag.spv`
    (`scripts/check-shader-artifacts.sh:59`).
  - `_audit-validate.sh` lists all five as deleted-file basename advisories.
- **Suggested Fix**: write `triangle_early.frag.spv`, or "the early-test variant of `triangle.frag`".

### LOW — Dimension 7 (Magic Numbers)

### TD7-2026-09-29-01: The 8-lane terrain splat budget is a bare `8` in about 17 places; #4056 added a fourth shader loop after #4496 closed
- **Severity**: LOW · **Dimension**: 7 · **Status**: NEW · **Effort**: small
- **Location**:
  - GLSL:
    - `crates/renderer/shaders/include/bindings.glsl:469-471`
    - `triangle.frag:396/529/662` (`for (uint i = 0u; i < 8u; ++i)`)
    - `groundcover_blade.frag:155` (the same loop, `c14f5361a`, 2026-09-24)
  - Rust: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:13/16/19` (`[u32; 8]` ×3)
  - CPU packer and budget: `byroredux/src/cell_loader/terrain.rs:378-379` (`[Vec<u32>; 8]`), `:468`
    (`8 - base_transition_count.min(8)`), `:935` (`splat1[i - 4]`)
- **Description**:
  - The value comes from `Vertex.splat_weights_0/1` (2 × `[u8; 4]`).
  - #4496 (CLOSED 09-20) pinned the packer's premise but never named the constant. Each new consumer
    re-types `8` and the `i < 4u ? splat0 : splat1[i-4]` split.
  - The GLSL array length and the Rust `[u32; 8]` are tied only through the struct-size pin.
- **Related**: #4496, #4027
- **Suggested Fix**:
  - Add `TERRAIN_SPLAT_LAYERS = 8` and `TERRAIN_SPLAT_LANES_PER_WORD = 4` to `shader_constants_data.rs`,
    emitted to GLSL.
  - Size the `GpuTerrainTile` arrays from it and replace the loops and budget arithmetic.
  - Pin it against `2 * size_of_val(&Vertex.splat_weights_0)`.

### LOW — Dimension 8 (Dead Code)

### TD8-2026-09-29-01: Eleven self-described "Throwaway" probe examples stay committed; the `_tmp_` guard misses `tmp_`
- **Severity**: LOW · **Dimension**: 8 · **Status**: NEW · **Effort**: small
- **Location**:
  - `crates/nif/examples/tmp_fo4_d4_{psglod,lodoverlap,lodsize}.rs` (`f74f8f68a`, 2026-09-12). Header:
    "Throwaway (FO4 audit D4): census …".
  - `crates/bsa/examples/{obl_sweep,probe_substring,probe_extensions}.rs`,
    `crates/plugin/examples/{find_ext_cell,roof_probe,qust_alias_rawdump}.rs`,
    `crates/nif/examples/lod_probe.rs` and `crates/bgsm/examples/dump_bgsm.rs`. These date from
    2026-05-05 → 07-21, and each module doc says "Throwaway" or "One-off diagnostic".
  - 669 LOC in total.
  - Guard: `byroredux/src/workspace_hygiene_tests.rs:26-29`.
- **Evidence**:
  - The guard matches only `starts_with("_tmp_")`.
  - NIFAL reports 09-14, 09-16 and 09-21 each routed the three `tmp_fo4_d4_*` files here. No tech-debt
    report picked them up.
  - `clippy --all-targets` already fails on all three.
  - Each one is an example target that links on every workspace test run, which is the cost #3746
    measured.
- **Related**: #3746, #3150
- **Suggested Fix**:
  - Delete the probes, or give a keeper a real documented purpose.
  - Widen the guard to `tmp_`/`_tmp_` plus a `^//! *(Throwaway|One-off|TEMP scratch)` module-doc check.

### TD8-2026-09-29-02: `clippy --all-targets` debt grew from 698 to 824 sites; 2 new `too_many_arguments` allows lack a reason
- **Severity**: LOW (lower-priority bucket, outside the CI gate) · **Dimension**: 8 · **Status**: NEW
- **Effort**: small (plugin, mechanical) / medium (the rest)
- **Location**: workspace test and example targets; `byroredux/src/cell_loader/terrain.rs:282`,
  `byroredux/src/helpers.rs:94`
- **Evidence**:
  - `cargo clippy --workspace --all-targets --keep-going -- -D warnings` on rustc 1.96 reports 824 unique
    sites (806 errors in 39 targets). The memory note *Clippy --keep-going* measured 698 on 09-16.
  - By crate:
    - plugin 654: 522 `needless_borrows_for_generic_args`, 79 `unnecessary_to_owned`, 46
      `field_reassign_with_default`
    - byroredux/src 90
    - renderer 22
    - nif 18
    - scripting 11
    - ui 9
  - Real signal inside the bucket:
    - An unused `bridge` at `crates/ui/tests/fallout4_hudmenu_protocol.rs:36`.
    - `empty line after doc comment` at `context/geometry_pass.rs:787`, `papyrus/src/parser/script.rs:849`,
      `commands/view.rs:233` and `nif/tests/common/mod.rs:326`. This is the doc-splice class that
      UI-D5-2026-09-29-01 and PHYS-D2-2026-09-29-03 describe.
  - The two allows listed under Location have no reason comment; the other 11 added in the window do.
- **Suggested Fix**:
  - Run `cargo clippy --fix --all-targets -p byroredux-plugin`.
  - Fix the doc-splice lints by hand.
  - Add the two reason comments.
  - Consider a non-blocking `--all-targets` count lane.

### LOW — Dimension 9 (Test Hygiene)

### TD9-2026-09-29-02: #2835's FSR bench-report self-test has never run in CI because the shader job's container has no python3
- **Severity**: LOW · **Dimension**: 9 · **Status**: NEW · **Effort**: trivial · **Kind**: test-gap
- **Location**: `.github/workflows/ci.yml:98-120`. The job runs in `container: ubuntu:26.04` and its apt
  list is ca-certificates, git, glslang-tools and ripgrep. The failing step is "FSR bench report reads
  both TSV schemas".
- **Evidence**:
  - The HEAD job log shows `python3: not found` → exit 127.
  - The step also failed in the sampled runs from 09-02, 09-09, 09-15 and 09-25.
  - The step was added by `4de5e78ee` (08-14); the container has been in place since `ca7a4e0ea` (07-25).
  - Locally: `ok — fsr_bench_report self-test passed (7 schemas)`.
  - The shader recompile/compare step passes. The job is red only because of this step.
- **Impact**:
  - #2835's guard has had no CI coverage for 6 weeks.
  - Shader parity has been permanently red for a non-shader reason, so a real SPIR-V drift would land
    on an already-red check.
- **Suggested Fix**: add `python3` to the apt list, or move the step into its own job.

---

## Cross-Referenced (owned by sibling reports today; not re-filed here)

- **CI / toolchain**:
  - SAFE-D4-2026-09-29-02: clippy is red on rustc 1.98.1 in sdk and nif, the renderer is never checked,
    and the finding is a regression of #4595.
  - ECS-2026-09-29-D1-01: the ABBA lane is red.
  - #4987 and CONC-D2-2026-09-29-01/-02: the validation lane.
  - PERF-D7-2026-09-29-01: the dhat pin is flaky.
- **Duplication**:
  - LC-D3-02: FO3/FNV Child race flag duplicated in `player_body.rs`.
  - CHAR-2026-09-29-D4-02: TPLT fields read three ways.
  - PAR-D2-2026-09-29-02: the debug-load NIF resolver.
  - PERF-D7-2026-09-29-02: `GearImportLoader` opens a third archive set.
  - Open issues #4928, #4925, #4939.
- **Doc rot**:
  - NIF-D3-2026-09-29-02, NIF-D4-01, NIF-D5-01
  - AUD-2026-09-29-D5-04, CHAR-2026-09-29-D5-01
  - GAME-D1-2026-09-29-03
  - UI-D5-2026-09-29-01, UI-D7-02
  - PHYS-D2-2026-09-29-03
  - NIFAL-D1-01, NIFAL-D5-01
  - ESM-D1-01, ESM-D2-03
  - SPT-D1-01
  - PAR-D5-02
  - TOOL-D2-01
  - PERF-D7-03
  - SCR-D5-01
  - REN-D3/D4/D5-2026-09-29
- **Dead or unread data**:
  - GAME-D7-2026-09-29-02: `NpcEquipmentPart.inventory_index`.
  - `TerrainCoverInputs.authored_grass` (`components.rs:485`, written and never read). It is the input
    open #4906 needs.
  - #4137.
- **Test gaps**:
  - PAR-D4-2026-09-29-01
  - NIF-D3-2026-09-29-01
  - EXT-D7-2026-09-29-01, SPT-D3-01, TOOL-D1-2026-09-29-03: smoke gates broken by `63c0aee3b`.
  - AUD-2026-09-29-D5-02
  - PHYS-D2-2026-09-29-01

## Per-Dimension Notes (clean areas / what was checked)

- **Dim 5 (markers)**: clean.
  - 21 hits: 15 `XXXX`, 4 upstream-quote or allowlisted, and 1 historical ("Closes the #242 … TODO").
  - `loading_screen.rs:3` "remain TODO" is 13 days old, under the 30-day triage floor. Re-check it next
    run.
  - No shader markers. The triangle.frag MIT notice is intact.
- **Dim 6 (stubs)**: clean.
  - 0 `unimplemented!`/`todo!()`.
  - 45 "stub/placeholder/not yet" hits, all descriptive.
  - Of 259 new `pub`/`pub(crate)` fns, only 2 lack a non-test caller: a menuxml `tests/common` helper and
    an example function.
  - The 5 new console commands are wired.
- **Dim 7 (magic numbers)**: apart from TD7-01, the new shader `#define`s are local debug macros. New
  ESM `len() >= N` gates follow house style.
- **Dim 8**:
  - `cargo machete` CI job is green.
  - No `#[deprecated]`, `// removed:` or `_unused`.
  - allow(dead_code) is 27, flat. The build.rs `Vertex` mirror was added (justified); two #4413 allows
    were removed.
  - cxx-bridge (36 LOC) and platform (60 LOC) have no second consumer.
- **Dim 9**:
  - All 29 new `#[ignore]`s are data-gated.
  - The hygiene guards are live.
  - The feature lanes match `[features]`. Only the `not(parallel-scheduler)` serial branch has no lane:
    it predates the window, and it is noted only.
  - `golden_frames.rs` and its images are present.
- **Dim 4**:
  - Skills re-synced today. This skill's own baseline block matches the re-measurement exactly.
  - No skill callout cites a CLOSED issue as open.
  - The CRITICAL/HIGH findings of reports dated 2026-06-23 → 07-02, which newly crossed 90 days, all
    trace to CLOSED issues. ECS-2026-06-23-01 and SAVE-D6-01 were spot-verified in code.

## Deferred

- `TerrainCoverInputs.authored_grass`: its consumer is the EXAL ground-cover Phase C LTEX→GRAS work,
  open #4906. It is not dead code to delete.
- `byroredux/src/loading_screen.rs:3` TODO: M48 loading-menu remainder. Re-triage it once it is over 30
  days old.

## Next Step

`/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

Label mapping:
- TD3-01..04 and TD4-03 → `doc-rot`
- TD9-02 → `test-gap`
- TD4-01, TD9-01 and TD4-02 → `tech-debt`. They are audit infrastructure and CI.
- The rest → `tech-debt` + `bug`.

The issue-closure sweep in TD4-01 is a manual maintainer step, not part of publishing.
