# ByroRedux Tech-Debt Audit — 2026-07-25

Comprehensive-suite leg. Depth: deep (per-instance triage with concrete fix
proposals across all 9 dimensions). Prior report:
`docs/audits/AUDIT_TECH-DEBT_2026-07-16.md`.

## Session Context

110 commits landed between the prior report and this one — the heaviest
inter-audit window this tech-debt series has covered. Highlights: **every
single finding from the 2026-07-16 report was filed and closed** (all of
TD1-001 through TD1-012, TD2-101 through TD2-114, TD3-101/102, TD4-001
through TD4-005, TD7-101, TD8-103 — verified via `gh issue view` on each;
TD8-101/TD8-102/TD2-108/109/110/111 remain open and tracked, re-verified
still accurate and **not** re-reported here). On top of that remediation,
the window shipped: AMD FSR 3.1 upscaler integration (7 execution phases,
now the engine's **default** upscaler as of Session 60 Phase 7), a new
`crates/fsr3-sys` workspace member + vendored `third_party/fidelityfx-sdk-v1.1.4/`
FFI boundary, the M47.3 Quest Alias System (ALST/ALLS decode + runtime
fill), AddItem/MoveTo object-targeting scripting effects, a universal
settings registry, Follow/Escort/Guard/Patrol AI-behavior runtimes, and a
new CI shader source/artifact parity job landed **the same day as this
audit** (commit `ca7a4e0e`).

Given that near-total remediation, this report focuses entirely on debt
introduced or re-grown since 07-16 — there is nothing left to re-litigate
from the prior baseline.

## Executive Summary

**10 findings**, all net-new. **0 CRITICAL, 0 HIGH, 5 MEDIUM, 5 LOW.**
Dimensions 5 (Stale Markers), 6 (Stubs), and 9 (Test Hygiene) are again
**entirely clean** — same 17 known-false-positive markers, zero
`unimplemented!`/`todo!()`, zero new `#[ignore]` tests introduced across the
110-commit window (confirmed by diffing every commit's added lines for
`#[ignore]`).

The five MEDIUM findings are all **Dimension 3/4 documentation-rot**, and
they cluster around one root cause: the FSR 3.1 work (a large, fast-moving,
multi-week feature arc) has outpaced the docs that are supposed to describe
it, and three unrelated file-split commits (`misc/ai.rs` → per-family files,
`records/actor.rs` → `actor/`, `blocks/shader_tests.rs` → `shader_tests/`)
re-broke the path-validation gate that the 07-16 report left GREEN.

- **TD3-NEW-01**: `CLAUDE.md`'s own Quick Reference still claims `Vertex` is
  100 bytes; it has been 104 bytes (pinned by a test) since the RGBA-color
  layout change three days ago.
- **TD3-NEW-02**: `docs/engine/shader-pipeline.md` (the canonical GPU-layout
  doc) and `docs/engine/renderer.md` (updated *in today's own commit*) both
  omit `presentation.frag`; the per-frame submission-order diagram still
  ends at the composite pass with no FSR/presentation step.
- **TD3-NEW-03**: `docs/feature-matrix.md`'s Rendering table still shows only
  TAA, with FSR 3.1 Quality now the shipped default — the same
  feature-matrix-lags-shipped-code failure mode as the already-fixed
  TD3-101, recurring for a different feature.
- **TD3-NEW-04**: `_audit-common.md`'s Project Layout table and "21 crates"
  count omit `crates/fsr3-sys/` and the whole FSR renderer file set
  entirely — should read 22, matching ROADMAP.md.
- **TD4-NEW-01**: the path-validation gate is RED again (9 stale refs across
  7 files) — three file-split commits since 07-16 left dead paths in
  `audit-ecs`, `audit-fnv` (×2), `audit-incremental`, `audit-oblivion`,
  `audit-skyrim`, `audit-starfield`, **and this very skill's own Dimension-1
  discovery-recipe text**, plus `_audit-common.md`.

The five LOW findings are ordinary complexity/duplication debt from the new
code: a function (`draw_frame`) that regrew past a threshold it was fixed
for four days ago, a file (`npc_spawn.rs`) that re-crossed 2000 LOC after its
tracked function-level fix, a barely-over-threshold test file, and one small
hand-rolled-barrier duplication in the new FSR upscaler module.

## Baseline Snapshot (for the next audit to diff)

```
TODO/FIXME/HACK/XXX:    17   (unchanged — all 17 re-verified as the same false positives)
allow(dead_code):       36   (was 20; +16, entirely explained by crates/plugin/src/esm/records/misc/quest.rs's
                              new AliasFlags bit catalog — each constant is test-exercised, explicitly
                              commented as forward-looking M47.3 scaffolding; verified NOT debt)
unimplemented!/todo!(): 0    (unchanged)
#[ignore] tests:        135  (unchanged; zero added in the 110-commit window)
files >2000 LOC:        4    (was 7 — net improvement. Dropped below threshold via prior-cycle splits:
                              particle.rs, misc/ai.rs, records/actor.rs, shader_tests.rs. New/regrown:
                              npc_spawn.rs, crates/nif/src/anim/tests.rs)
path gate:              RED — 9 stale refs / 7 files (was GREEN at 07-16)
```

Oversized set (live, today):
```
3723  crates/renderer/src/vulkan/context/mod.rs         (Existing: #1749, OPEN — grew 3533→3723; VulkanContext::new() 1046→1165 LOC)
3209  crates/renderer/src/vulkan/context/draw.rs          (Existing: #1857, CLOSED — file itself dropped 4732→3209 via the 07-21 decomposition;
                                                            draw_frame regrew 1927→2048 LOC in the 4 days since, see TD1-NEW-02)
2777  byroredux/src/npc_spawn.rs                          (TD1-NEW-03 — re-crossed after #2052's function-level fix; file grew 2400→2777)
2002  crates/nif/src/anim/tests.rs                        (TD1-NEW-04 — new crossing, test-only, 2 lines over threshold)
```

Dropped below threshold since 07-16 (all via already-closed issues, not
re-verified here beyond confirming the files no longer exist at their old
paths): `crates/nif/src/blocks/particle.rs` (#2053), `crates/plugin/src/esm/records/misc/ai.rs`
→ split into `misc/{pack,quest,dialogue,character,equipment,effects,water,world}.rs`
(#2054), `crates/plugin/src/esm/records/actor.rs` → `actor/{mod,tests}.rs`
(#2055), `crates/nif/src/blocks/shader_tests.rs` → `shader_tests/{mod,legacy,skyrim,fo4,fo76,starfield}.rs`
(#2056).

## Top 10 Quick Wins

1. **TD4-NEW-01** (trivial, ~20 min total) — fix all 9 stale backticked paths across 7 files in one pass; re-run `.claude/commands/_audit-validate.sh` to confirm GREEN.
2. **TD3-NEW-01** (trivial) — change `CLAUDE.md:135`'s Vertex line from "100 B (19 f32 + 4 u32 + 8 u8)" to "104 B (20 f32 + 4 u32 + 8 u8)".
3. **TD3-NEW-04** (trivial) — add a `FSR3-sys:` row to `_audit-common.md`'s Project Layout table and bump "Crate count: 21" to 22.
4. **TD3-NEW-02** (small) — add `presentation.frag` (and `skin_palette.comp` / `svgf_atrous.comp`, both already shipped but also absent from `renderer.md`'s "full set" list) to `docs/engine/shader-pipeline.md`'s Shader Files table and Per-Frame Submission Order diagram.
5. **TD3-NEW-03** (small) — add an "Upscaling" row to `docs/feature-matrix.md`'s Rendering table noting FSR 3.1 Quality is the shipped default, TAA the `--upscaler taa` fallback.
6. **TD1-NEW-04** (trivial) — split `crates/nif/src/anim/tests.rs`'s embedded modules along existing per-controller-family boundaries, mirroring the `shader_tests/` precedent from the same window.
7. **TD2-NEW-01** (small, ~15 min) — extract the repeated 4-image barrier shape in `frame_upscaler.rs::record_fsr_barriers_before` into a local closure/helper.
8. **TD1-NEW-02** (small) — extract the ~90-line FSR-frame-parameter-assembly block (jitter, camera-cut detection, `FsrFrameParameters` construction) out of `draw_frame` into a `fn build_fsr_frame_parameters(...)` free function, mirroring the file's existing `dof_effective_view_proj` extraction pattern.
9. **TD1-NEW-01** (tracking only) — update #1749 with the current `VulkanContext::new()` LOC (1165, +119 since 07-16).
10. **TD1-NEW-03** (tracking + small) — update or re-scope a tracking issue for `npc_spawn.rs`'s file-level re-crossing; no single function is newly oversized (the fixed `spawn_npc_entity` shrank 1045→828 LOC), the growth is legitimate new `apply_ai_package_behavior` logic (228 LOC) — no urgent action needed beyond noting the file crossed again.

## Top 5 Medium Investments

There are no medium/large code investments this cycle — all five MEDIUM
findings are documentation-only fixes (Top-10-Quick-Wins items 2–5 above
already cover them at small/trivial effort). The one genuine code-complexity
item worth a deeper look if `draw.rs` keeps absorbing FSR follow-up work is
extending #1857's original split axis (per-pass recording groups) to cover
frame-parameter assembly as its own recording-adjacent phase, not just
barrier/pass recording — see TD1-NEW-02.

## Findings

### MEDIUM

#### TD3-NEW-01: CLAUDE.md's Vertex byte-size (100 B) is stale — actual size is 104 B, pinned by a test since 3 days before this audit
- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `CLAUDE.md:135`
- **Status**: NEW
- **Description**: CLAUDE.md's Workspace Structure table reads: `Vertex (position + color + normal + uv + bone_idx + bone_wt + splat0/1 + tangent), 9 attribute descriptions, 100 B (19 f32 + 4 u32 + 8 u8)`. Commit `cd2b5fe4` ("refactor: update vertex structure to include RGBA color and adjust layout", 2026-07-22) widened `color` from `[f32; 3]` to `[f32; 4]`, adding one f32. The struct is now 104 bytes (20 f32 + 4 u32 + 8 u8), pinned by `crates/renderer/src/vertex.rs:320`'s `assert_eq!(size_of::<Vertex>(), 104)` and documented correctly in that file's own comment ("12 (pos) + 16 (color) + 12 (normal) + 8 (uv) + 16 (indices) + 16 (weights) + 4 (splat_0) + 4 (splat_1) + 16 (tangent) = 104"). CLAUDE.md — read at the start of every session, per this project's own tooling — was never updated.
- **Evidence**: `crates/renderer/src/vertex.rs:320: assert_eq!(size_of::<Vertex>(), 104);` vs. `CLAUDE.md:135: ... 100 B (19 f32 + 4 u32 + 8 u8)`.
- **Impact**: This is exactly the `feedback_shader_struct_sync.md` lockstep-drift-bait pattern the severity table calls out for `GpuCamera`/`GpuInstance`/`GpuMaterial`; `Vertex` is the same class of shader-contract struct and CLAUDE.md is a more heavily-read surface than any single doc file (loaded into every agent session in this repo).
- **Related**: commit `cd2b5fe4`; the fix is otherwise clean — `docs/engine/shader-pipeline.md` doesn't reference the Vertex byte count at all, so this is CLAUDE.md-only.
- **Suggested Fix**: Update the line to "104 B (20 f32 + 4 u32 + 8 u8)".
- **Age**: 3 days (struct changed 2026-07-22, audit run 2026-07-25).
- **Effort**: trivial

#### TD3-NEW-02: Canonical shader docs omit `presentation.frag` and the FSR/presentation submission step — including in today's own "shader documentation update" commit
- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `docs/engine/shader-pipeline.md:12-59` (Shader Files table + Per-Frame Submission Order), `docs/engine/renderer.md:738-742`
- **Status**: NEW
- **Description**: `presentation.frag` is a real, shipped shader (`crates/renderer/shaders/presentation.frag`, dispatched from `record_post_passes` via `self.presentation...`, gated by GPU timers `cmd_presentation_start`/`_end`) introduced during the FSR 3.1 integration arc. It appears in neither `shader-pipeline.md`'s "Shader Files" table (8 raster + 12 compute = 20 listed, but the live count is 21 first-party `.vert`/`.frag`/`.comp` files) nor its "Per-Frame Submission Order" diagram, which still ends at step 16 (`composite.vert/frag → swapchain (PRESENT_SRC_KHR)`) with no step for the `frame_upscaler`/`presentation` dispatch that `post_passes.rs` now records between composite and the final present. The omission survived even **today's** commit `ca7a4e0e` ("update shader documentation and add artifact check script"), whose `renderer.md` edit lists "the full set" of shaders by name but still misses `presentation.frag` **and** `svgf_atrous.comp` (also previously shipped, also absent from that same list).
- **Evidence**: `grep -c` over `crates/renderer/shaders/*.{vert,frag,comp}` → 21 files, including `presentation.frag` and `svgf_atrous.comp`; `docs/engine/shader-pipeline.md`'s tables list 20, missing `presentation.frag`; `post_passes.rs:569-606` shows `self.frame_upscaler...` and `self.presentation...` dispatches between the composite pass and present, with no corresponding numbered step in the doc's order diagram.
- **Impact**: `shader-pipeline.md` is explicitly named "authoritative" for shader-pipeline audits in `_audit-common.md`'s Key Reference Docs table; a reader (human or `/audit-renderer`) trusting it would not know the FSR/presentation pass exists in the pipeline at all, or that the composite pass no longer writes directly to the swapchain.
- **Related**: `_audit-common.md`'s "All 19 shaders" summary claim (see TD3-NEW-04) is the same undercount one file removed.
- **Suggested Fix**: Add a `presentation.frag` row and an `svgf_atrous.comp`/`skin_palette.comp` mention to `renderer.md`'s "full set" list (both already shipped, unrelated to today's omission but caught by the same sweep); add a `presentation.frag` row to `shader-pipeline.md`'s Shader Files table; renumber the Per-Frame Submission Order diagram to insert the upscale/presentation dispatch between the current steps 16 and 17.
- **Age**: `presentation.frag` shipped ~2026-07-23/24 (FSR phases 5-7); `svgf_atrous.comp` is older (Session-49) and was already missing before this window, newly caught here because the same table was touched today without being corrected.
- **Effort**: small

#### TD3-NEW-03: feature-matrix.md's Rendering table doesn't mention FSR 3.1 — still reads as if TAA is the only upscaling path, though FSR is now the shipped default
- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `docs/feature-matrix.md:39` (Rendering table, TAA row)
- **Status**: NEW
- **Description**: The Rendering table's only temporal-reconstruction row is `| **TAA** | ✓ All games | Halton(2,3) jitter, YCoCg variance clamp |`. Per ROADMAP.md's Session 60 closeout, "Phase 7 closed the same day: FSR Quality is now the engine default (+40% to +68% net frame recovery across every measured game scene), `--upscaler taa` retained as the fallback" (2026-07-24, one day before this audit). `feature-matrix.md` has no row, footnote, or edit reflecting any of this — a reader would conclude TAA is what's running by default today, which is now wrong. This is the identical failure mode as the already-fixed TD3-101 (feature-matrix lagging shipped AI-behavior work) recurring one file-section over, for the single largest renderer change of the audited window.
- **Evidence**: `docs/feature-matrix.md:39`; ROADMAP.md:15's Session 60 closeout paragraph; `crates/renderer/src/vulkan/upscaling.rs`'s `UpscalerMode` default resolves to FSR Quality per the phase-7 commits (`5c7acfe2` "make FSR 3.1 Quality the default upscaler").
- **Impact**: A reader using feature-matrix.md as the "what renders today" reference (its own stated purpose) would misjudge both the current upscaling behavior and the newly-introduced `--upscaler taa` fallback flag's existence.
- **Related**: TD3-101 (closed, 07-16 report) — same file, same recurring pattern class.
- **Suggested Fix**: Add an "Upscaling" row: `FSR 3.1 (default, Quality preset) / TAA native (--upscaler taa fallback)`, all games, with a one-line note on the four presets and the FP32-permutation caveat already tracked in ROADMAP.md.
- **Age**: gap opened 2026-07-24 (Phase 7 commit), 1 day old at audit time.
- **Effort**: small

#### TD3-NEW-04: _audit-common.md's Project Layout table and crate count are stale — crates/fsr3-sys (the 22nd workspace crate) is entirely absent
- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `.claude/commands/_audit-common.md:1-90` (Project Layout table, "Crate count: 21" line)
- **Status**: NEW
- **Description**: `_audit-common.md` is the shared file "referenced by all audit skills" and states "Crate count: 21 under `crates/`" with an explicit enumerated list — `crates/fsr3-sys` (added by commit `c4b070a7`, "vendor pinned Vulkan upscaler SDK", 2026-07-22) is not in that list, and neither are the three new renderer files that consume it (`frame_upscaler.rs`, `exposure.rs`, `upscaling.rs`) or the vendored `third_party/fidelityfx-sdk-v1.1.4/` tree. ROADMAP.md already correctly tracks this: "Workspace members | 24 (22 crates + `byroredux` binary + `tools/byro-dbg`)". `audit-tech-debt/SKILL.md` itself repeats the stale count in prose ("the 21-crate roster").
- **Evidence**: `find crates -maxdepth 1 -type d | wc -l` → 22 (including `fsr3-sys`); `_audit-common.md`'s crate list and "Crate count: 21" line predate the crate's addition; `ROADMAP.md:865` already says 22.
- **Impact**: `_audit-common.md` explicitly instructs "Use this as a coverage sanity check: an audit that never touches a relevant crate here is incomplete" — a future audit checking crate coverage against this list would never know `fsr3-sys` needs auditing at all (it has no dedicated owner audit skill either, unlike `pex`/`save`).
- **Related**: TD3-NEW-02 (same root cause — FSR work outpacing doc sync).
- **Suggested Fix**: Add an `FSR3:` row to the Project Layout table alongside the existing renderer rows, bump "Crate count: 21" to 22, and update `audit-tech-debt/SKILL.md`'s "21-crate roster" phrase.
- **Age**: crate added 2026-07-22, 3 days old at audit time.
- **Effort**: trivial

### LOW

#### TD4-NEW-01: Path-validation gate is RED again — 9 stale backticked-path refs across 7 files, from 3 unrelated file-split commits since 07-16
- **Severity**: LOW (batched trivial fixes; flagged distinctly because the gate itself is a regression from GREEN)
- **Dimension**: 4 (Audit-Finding Rot)
- **Location**: `.claude/commands/audit-ecs/SKILL.md:213`, `.claude/commands/audit-fnv/SKILL.md:149,152`, `.claude/commands/audit-incremental/SKILL.md:70`, `.claude/commands/audit-oblivion/SKILL.md:173`, `.claude/commands/audit-skyrim/SKILL.md:122`, `.claude/commands/audit-starfield/SKILL.md:228`, `.claude/commands/audit-tech-debt/SKILL.md:114`, `.claude/commands/_audit-common.md:76`
- **Status**: NEW
- **Description**: `.claude/commands/_audit-validate.sh` now fails with 9 stale refs, all traceable to 3 file-split commits landed in this window: `crates/plugin/src/esm/records/misc/ai.rs` was split into `misc/{pack,quest,dialogue,character,...}.rs` (#2054, closed 2026-07-18) — 5 refs (`audit-ecs`, `audit-fnv` ×2, `audit-incremental`, `_audit-common.md`) still cite the deleted path; `crates/plugin/src/esm/records/actor.rs` became `actor/{mod,tests}.rs` (#2055, same date) — 2 refs (`audit-oblivion`, and **this very skill's own Dim-1 discovery text**, `audit-tech-debt/SKILL.md:114`, which ironically suggested exactly this split in the 07-16 report as TD1-006 and never updated itself once the split shipped); `crates/nif/src/blocks/shader_tests.rs` became `shader_tests/{mod,legacy,skyrim,fo4,fo76,starfield}.rs` (#2056, same date) — 2 refs (`audit-skyrim`, `audit-starfield`).
- **Evidence**: `.claude/commands/_audit-validate.sh` output: `FAIL: 9 stale path reference(s)` across the 8 files above (checked 1074 refs / 26 skill files).
- **Impact**: Per the gate's own design intent (it exists specifically because backticked paths in audit skills "assert this path exists right now"), every one of the 6 per-game/domain skills above now has at least one dead entry-point reference an agent following it literally would fail to `Read`.
- **Related**: Same failure class as the already-fixed 07-16 TD4 batch (TD4-001/002/004/005), just triggered by file moves instead of stale prose claims.
- **Suggested Fix**: Update each ref to its current path (`misc/pack.rs`/`misc/quest.rs`/etc. as appropriate per context, `actor/mod.rs`, `shader_tests/mod.rs` or the specific per-era sibling file each ref's context implies); re-run the gate to confirm GREEN.
- **Age**: splits landed 2026-07-18, 7 days stale at audit time.
- **Effort**: trivial (batch, ~20 min for all 9)

#### TD1-NEW-02: draw_frame regrew past its just-closed complexity fix — 1927 LOC right after #1857 landed (07-21), 2048 LOC four days later
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:425-2473` (`draw_frame`)
- **Status**: NEW
- **Description**: Commit `9a9a4c5d` (2026-07-21) closed #1857 by extracting `record_geometry_pass`/`record_skinned_blas_refit`/`record_post_passes` into sibling files, leaving `draw_frame` at 1927 LOC (measured directly from that commit's tree). By this audit (2026-07-25), `draw_frame` is 2048 LOC — +121 lines / +6% in 4 days. The growth is a new ~90-line inline block (lines ~865-955) that assembles `FsrFrameParameters` (jitter offset, reset-pending, camera near/far/fov), performs FSR-vs-DOF interaction gating (`active_dof` override), and runs camera-cut detection tied to the FSR reset flag — all added directly into the function body rather than extracted, unlike the file's own established pattern of pulling pure-computation blocks into standalone functions (e.g. `dof_effective_view_proj`, `rebase_model_matrix`, `origin_corrected_prev_view_proj`, all already free functions in this same file).
- **Evidence**: `git show 9a9a4c5d:crates/renderer/src/vulkan/context/draw.rs` — `draw_frame` spans lines 421-2348 (1927 LOC) in that commit; current file has it at 425-2473 (2048 LOC). New FSR-parameter-assembly block confirmed inline at current lines ~865-955.
- **Impact**: Not a correctness issue — purely maintainability. The function this issue chain has already been fixed once is trending back toward its pre-fix size within days, driven by legitimate new feature work landing without a matching extraction.
- **Related**: Existing: #1857 (CLOSED — this is the file/function growing back around a fix that held for the file level but not, four days later, for this specific function).
- **Suggested Fix**: Extract the FSR-frame-parameter-assembly block into a `fn build_fsr_frame_parameters(active_dof: &DofView, fsr_jitter_pixel: Option<[f32;2]>, fsr_reset_pending: bool, frame_time_delta_ms: f32) -> Result<Option<FsrFrameParameters>>` free function alongside the file's existing `dof_effective_view_proj`.
- **Age**: 4 days (07-21 → 07-25).
- **Effort**: small

#### TD1-NEW-03: npc_spawn.rs re-crossed 2000 LOC after #2052's function-level fix — legitimate new AI-behavior code, not a regression of the fix
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `byroredux/src/npc_spawn.rs` (2777 LOC total), `apply_ai_package_behavior` (228 LOC, new)
- **Status**: NEW
- **Description**: #2052 (closed) extracted `spawn_npc_entity` down from 1045 to (currently) 828 LOC — that fix holds. The file re-crossed the 2000-LOC threshold anyway (2400→2777 since 07-16) because `apply_ai_package_behavior` (228 LOC) was added in the interim, consolidating what was previously a re-resolve-per-procedure pattern into a single-resolve dispatcher for the Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol behavior tags (itself the fix for the separately-closed PERF-D7-01 / #2031). No function in the file is newly oversized; this is pure file-level LOC growth from legitimate, already-reviewed feature work.
- **Evidence**: `wc -l byroredux/src/npc_spawn.rs` → 2777; `apply_ai_package_behavior` at line 1593, 228 LOC; `spawn_npc_entity` confirmed at 828 LOC (down from the 1045 LOC #2052 targeted).
- **Impact**: None beyond the file continuing to tax full-file reviews; no single function needs decomposition today.
- **Related**: Existing: #2052 (CLOSED, function-level, not regressed) — this is a fresh file-level crossing with no open tracking issue.
- **Suggested Fix**: No urgent action. If the file grows further with the remaining ~10 unbuilt AI procedures, consider extracting `apply_ai_package_behavior` and its seven `active_package_is_*`-driven arms into a sibling module (e.g. `npc_spawn/ai_package.rs`).
- **Age**: growth accumulated over the 07-16→07-25 window.
- **Effort**: n/a (tracking note) / small if acted on

#### TD1-NEW-04: crates/nif/src/anim/tests.rs crossed 2000 LOC — test-only file, 2 lines over threshold
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/nif/src/anim/tests.rs` (2002 LOC)
- **Status**: NEW
- **Description**: Marginal, mechanical crossing (2 lines over the 2000 threshold) via ordinary test accumulation on the KF-animation import path; no organizational problem, same pattern as the already-fixed `shader_tests.rs`/`particle_tests.rs` split precedent from this same window.
- **Evidence**: `wc -l crates/nif/src/anim/tests.rs` → 2002.
- **Suggested Fix**: If/when next touched, split along the existing per-phase boundaries the sibling `anim/` modules already use (`coord`, `controlled_block`, `transform`, `sequence`, `keys`, `channel`, `bspline`). Not urgent — 2 lines over.
- **Age**: crossed sometime in the 07-16→07-25 window (file was well under threshold at 07-16, not separately tracked then).
- **Effort**: trivial, deferrable

#### TD2-NEW-01: frame_upscaler.rs hand-rolls the same 4-image barrier shape instead of a local helper
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `crates/renderer/src/vulkan/frame_upscaler.rs:601-634` (`record_fsr_barriers_before`)
- **Status**: NEW
- **Description**: Four of the six barriers built in `record_fsr_barriers_before` are byte-identical in every field except `.image(...)`: `.src_access_mask(COLOR_ATTACHMENT_WRITE).dst_access_mask(SHADER_READ).old_layout(SHADER_READ_ONLY_OPTIMAL).new_layout(SHADER_READ_ONLY_OPTIMAL)`, applied to `inputs.scene_color`, `inputs.motion_vectors`, `inputs.reactive`, `inputs.transparency` in turn. This is a new occurrence of the same duplication class Dim 2 already fixed once this window (#2071/TD2-112, a different barrier shape) — the existing `descriptors.rs` helpers don't cover this specific same-layout/access-pair shape, so it wasn't reachable from the prior fix.
- **Evidence**: `crates/renderer/src/vulkan/frame_upscaler.rs:604-634` — 4 near-identical `vk::ImageMemoryBarrier::default()...` blocks differing only in the `.image(...)` argument.
- **Impact**: Cosmetic/maintainability only — all 4 are semantically correct today; a future barrier-shape change would need to be applied at 4 sites by hand.
- **Related**: #2071/TD2-112 (closed) fixed a different, GENERAL→GENERAL compute barrier shape in `descriptors.rs`; this is a distinct shape from a file outside that fix's scope.
- **Suggested Fix**: Add a small local closure or free function `fn shader_read_barrier(image: vk::Image, range: vk::ImageSubresourceRange) -> vk::ImageMemoryBarrier` in `frame_upscaler.rs` and call it 4×.
- **Age**: introduced with the FSR barrier-recording code, this window (~07-22 to 07-24).
- **Effort**: small

## Verified Clean

- **Dimension 5 (Stale Markers)**: same 17 markers, all re-confirmed false positives (protocol `XXXX` tag ×3, `read_vec4`-region `XXXX` protocol comments ×8, ref-impl FIXME doc, closed-issue TODO breadcrumb). Zero new markers across the entire 110-commit window, including the FSR/quest-alias/settings-registry/AI-behavior arcs.
- **Dimension 6 (Stub Implementations)**: `unimplemented!()`/`todo!()`/`panic!("not …)` still 0 repo-wide. The `quest.rs` `AliasFlags` bit catalog (16 new `#[allow(dead_code)]` constants, entire delta of the Dim-8 baseline bump 20→36) is explicit, test-exercised, forward-looking M47.3 Phase-0 scaffolding with an in-file comment justifying the allow — matches the "public API a future binary will consume" exemption, not debt.
- **Dimension 7 (Magic Numbers)**: `GpuCamera` (336 B) / `GpuInstance` (112 B) sizes unchanged and still correctly cross-referenced between the pinned layout test and `docs/engine/shader-pipeline.md`. The already-fixed `MaterialTable` VRAM comment (TD3-102/#2074) remains correct (decimal-MB consistently). New CI script `scripts/check-shader-artifacts.sh`'s `expected_version="11:16.2.0"` pin is intentional (exact-parity requirement, well-commented) — verified by actually running the script locally, which passed against all 21 shader artifacts.
- **Dimension 8 (Dead Code)**: no new `#[deprecated]`, no new `// removed:` breadcrumbs, no new `_`-prefixed function-parameter refactor leftovers. `_unused` local-binding reads (4 sites, 2 pre-existing + 2 in this window's `script_instance.rs`/`walkers.rs` touches) follow the established "deliberately skip a byte we don't decode" convention, not orphaned refactor artifacts.
- **Dimension 9 (Test Hygiene)**: `#[ignore]` count flat at 135 across the window; diffed every commit's added lines and confirmed zero new `#[ignore]` attributes introduced by any of the 110 commits.
- **Dimension 2 (spot checks beyond TD2-NEW-01)**: the `misc/ai.rs` → 8-file split (#2054) introduced no duplicate function names across the new siblings; `apply_ai_package_behavior`'s single-resolve consolidation in `npc_spawn.rs` is itself a duplication *fix* (closes PERF-D7-01/#2031's 14×-re-resolve pattern), not new debt; the AI-behavior procedure runtimes (Follow/Escort/Guard/Patrol) continue to share `locomotion.rs::step_toward` per CLAUDE.md's documented pattern, confirmed unchanged.

## Deferred

None. Every finding in this report is actionable now; no in-progress
milestone gates any of them. (The one open-but-not-reported item touching
this window, PERF-REGRESSION-6c56e311, is a performance regression, not
tech debt — owned by `/audit-performance`, already filed in ROADMAP.md.)
