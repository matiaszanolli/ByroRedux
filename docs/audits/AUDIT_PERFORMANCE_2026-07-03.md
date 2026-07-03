# ByroRedux Performance Audit — 2026-07-03

**HEAD**: `8498e559`. **Depth**: deep, all 9 dimensions.
**Hardware target**: RTX 4070 Ti (12 GB) + Ryzen 7950X (16c/32t); RT VRAM
minimum 6 GB; RT budget total under ~4 GB. A CPU bottleneck on this machine
is a bug, not a tuning gap.

## Scope note — a fix sprint landed between the last audit and this one

`docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` ran against `1b4e8e84` and
found 1 HIGH + 9 MEDIUM + 13 LOW (23 total). Between that HEAD and this
audit's `8498e559`, a dedicated fix sprint (commits `af6e4c9b`…`9f48a16e`,
all dated 2026-07-02/03) closed out **9 of those 10 non-LOW findings** plus
2 of the LOW findings, each with a real code change, regression tests, and
(where a live Vulkan device isn't available in `cargo test`) a static
source-position assertion pinning the ordering invariant the fix depends on.

This audit is therefore primarily a **fix-verification pass**: every closed
commit was read in full (not just its message) and cross-checked against
the failure mode the original finding described, per the mandatory
skepticism rule. Two items were closed with a **documentation-only**
resolution (the maintainer explicitly decided the fix was too risky to ship
without a real repro/measurement) — these are carried forward as still-open
gaps below, tagged accordingly. One item was closed with a **partial**
fix (added the measurement the issue asked for, not the full budget
rewrite) — also carried forward. No new dimension-level regressions were
found in the changed code, and no findings beyond the 2026-07-02 set were
newly discovered in the unchanged code.

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 4 |
| LOW      | 12 |
| **Total**| **16** |

Zero CRITICAL, zero HIGH — the sprint's headline result is that **D6-01**,
the one HIGH finding from 2026-07-02 (a real, if narrow, correctness hole
that could corrupt a skinned entity's geometry in both raster and RT
permanently), is fixed and verified correct. The remaining 16 findings are
all MEDIUM/LOW efficiency or hardening gaps carried forward unchanged from
2026-07-02 (their code sites were not touched by the sprint), 4 of which
were explicitly re-confirmed present in this pass via direct grep against
current line numbers.

### Fixes verified this pass (read in full, not just the commit message)

| Finding | Fixed by | Verification method | Verdict |
|---|---|---|---|
| **D6-01** (HIGH) — first-sight `bind_inverses` lost on `draw_frame` early return | #1791 | Read `SkinSlotPool::requeue_pending` + call-site diff in `main.rs`; traced that no early return exists between the `bind_inverses` upload (`draw.rs:2665`) and the `skin_dispatch_ran` flip inside `record_skinned_blas_refit` (`draw.rs:2762`) — confirmed via `grep -n "return Ok\|return Err"` across the `draw_frame` body | **Correct.** 3 new unit tests pin ordering + no-op-on-empty + FIFO-priority-for-requeued behavior. |
| **D6-02** (MEDIUM) — pose-hash committed before `draw_frame` runs, no rollback on early return | #1796 | Read the `skin_dispatch_ran` flag + `rollback_pending_pose_commits` implementation; confirmed `rollback_pose_hash` is a persistent `HashMap` cleared via `.clear()` (capacity-preserving), not reallocated per frame | **Correct.** Static source-position test (`skin_dispatch_ran_is_reset_before_both_early_return_guards`) pins reset-before-guards and flag-flip-after-guards in one assertion, immune to future line-shuffling silently breaking the invariant. |
| **PERF-D3-NEW-01** (MEDIUM) — mid-batch BLAS eviction gate blind to `pending_bytes` | #1792 | Read `blas_over_budget` predicate + all 4 call sites in `blas_static.rs`; confirmed the 3 no-batch-context callers correctly pass `0` (behavior-preserving) and only the mid-batch loop passes the running total | **Correct.** Unit tests pin the exact failure mode (`static=0, pending=large` must read over-budget). |
| **D2-NEW-02** (MEDIUM) — unquantized particle color fade defeats `MaterialTable` dedup | #1795 | Read `quantize_fade` + the 4 new unit tests; confirmed size LERP is deliberately left on the continuous `t` (only color is quantized) | **Correct.** `output_has_at_most_step_count_plus_one_distinct_values` test directly proves the dedup-collapse guarantee. |
| **PERF-D5-NEW-01** (MEDIUM) — legacy 16-slot WRS arrays live on every frame regardless of ReSTIR default | #1799 | Read `shader_constants.rs`/`shader_constants_data.rs` (new generated-constants channel, mirrors #1758's `SKIN_WORKGROUP_SIZE` pattern) + `triangle.frag`'s `#if ENABLE_LEGACY_WRS` gating; confirmed `triangle.frag.spv` shrank 185008→177840 bytes, a genuine preprocessor elimination (not relying on the compiler to fold a runtime branch) | **Correct.** 2 regression tests pin the shipped default (`0`) and that the array declaration falls strictly inside the `#if` block. |
| **PERF-D4-NEW-01** (MEDIUM) — `bone_world` re-identity-filled from empty every frame | #1794 | Read the diff to `render/mod.rs` (dropped `.clear()`) + `render/skinned.rs` (unconditional `resize`, not grow-only) + the 3 new tests, including one that pokes a sentinel into the padding tail and confirms it survives a second frame | **Correct, but partial by the fix's own admission** — eliminates the CPU identity-fill in steady state; the staging memcpy + GPU copy + `skin_palette.comp` dispatch still transfer/process the full `MAX_BONES_PER_MESH`-stride per slot every frame regardless of dirtiness. That residual is exactly **D6-04** below (still open). |
| **D2-NEW-03** (LOW) — two-sided blend split runs on non-depth-writing (particle) batches | #1804 | Read `needs_two_sided_blend_split(&DrawBatch)` extraction + 4 new unit tests covering all 4 boolean combinations | **Correct.** Glass (`z_write: true`) unaffected; particles (`z_write: false`) no longer split. |
| **PERF-D1-NEW-03** (LOW) — dead `GlobalTransform` probe in `emit_particles` | #1803 | Read the diff — clean removal of the query acquisition, the discarded `get()`, and the now-unused import | **Correct.** No behavior change (the probe was provably dead). |

Two related **safety-adjacent** fixes landed in the same window and are
noted for completeness (not perf findings, but touch code these dimensions
watch): **#1790** widened the skinned-BLAS scratch-serialize barrier's dst
mask to include `AS_READ` (closing a same-command-buffer RAW hazard on
first-sight frames — a correctness fix, zero added per-frame cost, pinned
by a new static assertion on the access-mask constant); **#1782** routed
the shared `blas_scratch_buffer`'s destroy-on-grow/shrink path through the
existing `pending_destroy_scratch` deferred-destroy queue at the same
`MAX_FRAMES_IN_FLIGHT` countdown the guard table already covers (closes a
GPU use-after-free window, not a budget or throughput change).

### Closed as documented-not-fixed or partially-fixed (carried forward below)

| Finding | Issue | What actually shipped | Residual gap |
|---|---|---|---|
| **PERF-D3-NEW-02** | #1793 (CLOSED) | A doc comment at `tlas.rs`/`blas_static.rs` explaining why neither the missing-rigid-BLAS recovery path nor the burst-aging fix is safe to ship speculatively | Both gaps are unchanged in code. See finding below. |
| **D6-03** | #1797 (CLOSED) | A doc comment at `record_scratch_serialize_barrier` pointing future work at the `skin.coverage` / `gpu_skin_blas_refit_ms` measurement gate | The shared-scratch serialization is unchanged. See finding below. |
| **D7-NEW-01** | #1798 (CLOSED) | `npc_spawn_wall` timing added around both NPC-dispatch call sites in `load_references`, surfaced in the end-of-cell summary log | No per-frame/per-NPC spawn budget exists yet — the cost is now *visible*, not *bounded*. See finding below. |

These three are **not** re-filed as new issues (the maintainer's own closed
issues already carry the full reasoning); they're carried forward here so
the audit's severity accounting stays honest about what's actually fixed
versus measured-or-explained.

### Bench-of-Record delta (observed vs ROADMAP — not absolute FPS)

Unchanged from the 2026-07-01/07-02 audits: ROADMAP's **Bench-of-record**
block (`R6a-stale-14`, HEAD `1c26bc25`, 2026-06-03) is self-flagged
"437 commits stale" as of Session 53 and predates the Session 47
camera-origin work and the Session 49 RT-denoiser overhaul. This sprint adds
more commits on top without a fresh bench. **R6a-stale-15 remains overdue**
— this is a process recommendation, not a code finding, repeated from the
last two audits because it still hasn't been actioned.

---

## Findings (carried forward, unchanged code sites)

### MEDIUM

#### PERF-D3-NEW-02: Budget eviction has no rebuild path — evicted static BLAS drop out of RT permanently, and multi-cell load bursts age not-yet-drawn BLAS into candidacy
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:150-158` (missing rigid BLAS → count + skip only); `crates/renderer/src/vulkan/acceleration/blas_static.rs:425-431` (per-batch `frame_counter` bump) + `:1129,1156-1175` (idle candidacy + slot clear)
- **Status**: Existing: #1793 (CLOSED — documented, not fixed; both gaps confirmed unchanged in code this pass)
- **Description**: Unchanged from 2026-07-02. (1) No recovery: a static BLAS evicted under budget pressure vanishes from shadows/reflections/GI forever until its cell unloads and reloads — `build_blas_batched` is only invoked from cell/scene-load sites, never per-frame, so `build_tlas`'s `missing_rigid_blas` arm can only count and skip. (2) Burst aging: a synchronous multi-cell load (e.g. `--grid` radius 3 = 49 batched calls before the first frame) ages cell #1's just-built, never-yet-drawn entries into LRU-victim candidacy alongside later cells in the same burst.
- **Impact**: Gated on `static_blas_bytes > budget` — unreachable on the 12 GB dev card with vanilla content; plausible on 6-8 GB devices with heavy exteriors or mod load orders. Crash-safe (deferred destroy); recovery requires a cell round-trip.
- **Related**: #920, #740, #1228, #1449, PERF-D3-NEW-01 (fixed, #1792).
- **Suggested Fix**: Unchanged — a lazy re-acquire path on `missing_rigid_blas` hit (mirroring the skinned first-sight build), and/or a burst-boundary tick so an in-burst load can't age its own cells into eviction candidacy. The maintainer's own #1793 closeout explains why this needs a real low-VRAM `--grid` repro before shipping either speculatively — that repro still doesn't exist.

#### D6-03: All skinned BLAS builds/refits in a frame are serialized on one shared scratch buffer — zero build overlap on multi-NPC frames
- **Severity**: MEDIUM
- **Dimension**: Skinning & BLAS
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:417` (per-refit barrier), `:278-283` (per-build barrier in first-sight batch); consumed at `crates/renderer/src/vulkan/context/draw.rs:1835-1899`
- **Status**: Existing: #1797 (CLOSED — documented, not fixed)
- **Description**: Unchanged. `blas_scratch_buffer` is one allocation sized to the max single-build demand; every skinned BLAS build/refit reuses the same address, so the required AS_WRITE→AS_WRITE(|READ, post-#1790) barrier fully serializes N dirty skinned entities per frame with no overlap.
- **Impact**: Moving-crowd-only ceiling (idle crowds already gated by #1195/#1196). No quantitative measurement has been taken yet — the #1194 GPU timer brackets (`skin.coverage` → `gpu_skin_blas_refit_ms` vs `refits_attempted`) exist specifically to gather it before investing in the round-robin scratch-slot rewrite the original finding suggested.
- **Related**: #642, #983, #1300 (correctness chain, intact), #1194 (measurement hook), #1790 (barrier correctness fix landed on top of this same call site this sprint).
- **Suggested Fix**: Unchanged — measure via `skin.coverage` on a moving-crowd bench scene before spending the sync-redesign risk budget.

#### D7-NEW-01: Interior NPC/REFR spawn loop has no per-frame or per-NPC budget, unlike the exterior streaming path
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/references.rs:224` (`load_references` ref loop, now instrumented) → `byroredux/src/npc_spawn.rs` (`spawn_npc_entity` @319, `spawn_prebaked_npc_entity` @354)
- **Status**: Existing: #1798 (CLOSED — partial fix: measurement only)
- **Description**: `load_references` still iterates every `PlacedRef` in one synchronous pass with no batching, yield, or cap — unlike the exterior path's `MAX_CELLS_SPAWNED_PER_FRAME`. What changed: both NPC-dispatch call sites are now wrapped in `Instant::now()` timing accumulated into `npc_spawn_wall`, surfaced in the end-of-cell summary log (`"{} NPCs spawned via {} path … {:.1}ms wall in spawn calls"`) and pinned by a static source-assertion regression test so a future refactor can't silently drop the timing.
- **Impact**: Unchanged — an interior transition into an NPC-dense cell (door walk-in, save-load-apply reload, fast travel) still pays its full spawn cost in one synchronous frame. The difference from 2026-07-02 is that this cost is now **visible** in the log rather than invisible; it is still **unbounded**.
- **Related**: Existing: #1698 (adjacent — post-load ECS/Rapier settle-storm, not a duplicate); #881 (batched texture flush, unaffected).
- **Suggested Fix**: Unchanged — the timing this sprint added is exactly the prerequisite the original suggested fix called for ("add `Instant::now()` timing … before deciding whether to invest in chunking"). The next step (a resumable-cursor spawn budget across frames) is still unbuilt and needs real `npc_spawn_wall` numbers from a dense interior (e.g. a populated Skyrim inn) to size correctly.

#### PERF-D5-NEW-02: One-bounce-GI hit irradiance samples the first 8 lights in upload order, not the 8 relevant to the hit point
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/include/lighting.glsl:173-178` (`giHitIrradiance` fixed-prefix loop), `crates/renderer/shaders/include/shader_constants.glsl` (`GI_HIT_LIGHT_CAP = 8u`); light upload order `byroredux/src/render/lights.rs:73-160`
- **Status**: Existing: #1800 (OPEN)
- **Description**: Unchanged — re-confirmed present via direct grep this pass (`lighting.glsl:178: uint count = min(lightCount, GI_HIT_LIGHT_CAP);`). `collect_lights` still pushes lights in plain ECS sparse-set iteration order (proximity-blind); any cell with >8 lights permanently ignores lights past index 8 in the GI bounce.
- **Impact**: Unchanged — quality (wrong/dim/flickering bounce lighting in >8-light interiors) and efficiency (up to 8 ray-query traces per lit fragment spent on an unprioritized set) both apply.
- **Confidence**: HIGH on the premise; magnitude scene-dependent.
- **Related**: `GI_HIT_LIGHT_CAP` comment.
- **Suggested Fix**: Unchanged — sort or distance-select the light prefix before the fixed-cap loop.

---

### LOW

All 12 re-confirmed present via direct grep against current line numbers this pass (none of their code sites were touched by the sprint's diff).

#### PERF-D1-NEW-01: `about_to_wait` runs a full MeshHandle+TextureHandle dedup walk every frame for on-demand-only telemetry
- **Severity**: LOW · **Dimension**: CPU Hot Paths · **Location**: `byroredux/src/main.rs:2294-2330`
- **Status**: Existing: #1801 (OPEN)
- **Description/Impact/Suggested Fix**: Unchanged from 2026-07-02 — `meshes_in_use`/`textures_in_use` dedup counts are computed unconditionally every frame for consumers (`stats` command, debug-server entity evaluator) that are both on-demand, not per-frame. Throttle to the 1 Hz `log_stats_system` cadence or compute lazily.
- **Related**: #1584, #637.

#### PERF-D1-NEW-02: Per-frame process-environment lookups in two hot-path sites (PARTIAL)
- **Severity**: LOW · **Dimension**: CPU Hot Paths · **Location**: `byroredux/src/render/mod.rs:354` (`BYRO_PROFILE`), `byroredux/src/render/static_meshes.rs:138` (`BYRO_NO_CULL`)
- **Status**: Existing: #1802 (OPEN)
- **Description/Impact/Suggested Fix**: Unchanged — both confirmed still live `var_os` calls (no heap allocation; `apply_fog_overrides`'s sibling `OnceLock` pattern remains the model to follow).

#### D2-NEW-04: Static-mesh hot loop pays a redundant GlobalTransform re-probe and a late IsFxMesh gate
- **Severity**: LOW · **Dimension**: Draw & Instancing · **Location**: `byroredux/src/render/static_meshes.rs:147,175,280`
- **Status**: Existing: #1805 (OPEN)
- **Description/Impact/Suggested Fix**: Unchanged — confirmed `tq.get(entity).is_none()` hoist at `:147` still followed by a second fetch, and `fx_q` gate at `:280` still fires after ~12 optional-component gets + frustum test.

#### D2-NEW-05: `draw_sort_key` omits the `wireframe` pipeline axis — sort key and `PipelineKey` no longer in lockstep
- **Severity**: LOW · **Dimension**: Draw & Instancing · **Location**: `byroredux/src/render/mod.rs:192-240`
- **Status**: Existing: #1806 (OPEN)
- **Description/Impact/Suggested Fix**: Unchanged — confirmed `draw_sort_key`'s signature is still the 10-tuple with no `wireframe` slot. Near-zero real-content impact (`NiWireframeProperty` essentially absent from shipped assets).

#### PERF-D3-NEW-03: `memory-budget.md` links the BGSM cache section to a path deleted by the `asset_provider` module split
- **Severity**: LOW · **Dimension**: GPU Memory Pressure · **Location**: `docs/engine/memory-budget.md:149`
- **Status**: Existing: #1807 (OPEN)
- **Description/Impact/Suggested Fix**: Unchanged — pure doc-rot, values still correct.

#### PERF-D4-NEW-02: `upload_lights` silently truncates past `MAX_LIGHTS` (512) — no overflow warn, no proximity prioritization
- **Severity**: LOW · **Dimension**: SSBO Sizing & Upload · **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:25`
- **Status**: Existing: #1808 (OPEN)
- **Description/Impact/Suggested Fix**: Unchanged — confirmed `let count = lights.len().min(MAX_LIGHTS);` with no warn path, unlike every sibling overflow (instances, indirect draws, terrain tiles, material intern).

#### PERF-D4-NEW-03: `upload_indirect_draws` lacks the dirty gate both its siblings have
- **Severity**: LOW · **Dimension**: SSBO Sizing & Upload · **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:606-626`
- **Status**: Existing: #1809 (OPEN)
- **Description/Impact/Suggested Fix**: Unchanged — confirmed `upload_indirect_draws` still has no `last_uploaded_hash` field, unlike `upload_instances`/`upload_materials`.

#### PERF-D4-NEW-04: Stale byte-math in scene_buffer comments — `GpuLight` quoted at 48 B (is 64 B), `GpuInstance` quoted at 72 B (is 112 B)
- **Severity**: LOW · **Dimension**: SSBO Sizing & Upload · **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:506,520`; `descriptors.rs:292`
- **Status**: Existing: #1810 (OPEN)
- **Description/Impact/Suggested Fix**: Unchanged — confirmed `descriptors.rs:292` and `upload.rs:506` both still quote "72 B" for `GpuInstance` (actual: 112 B, pinned by `gpu_instance_layout_tests.rs`). Doc-only.

#### D6-04: Fixed per-frame skinning costs run even on fully-clean frames
- **Severity**: LOW · **Dimension**: Skinning & BLAS · **Location**: `byroredux/src/render/mod.rs:308,328` (bone_world seeding, no longer clearing — see #1794 above), `byroredux/src/render/skinned.rs:153-186`, `crates/renderer/src/vulkan/context/draw.rs:2630-2732`
- **Status**: Existing: #1811 (OPEN) — **narrowed by #1794**, not closed
- **Description**: The `pose_dirty` gate still only guards the per-entity GPU compute dispatch. #1794 (this sprint) eliminated the CPU-side identity re-fill of `bone_world` on clean frames — a real subtraction from this finding's original three-fold cost — but the other two-thirds remain exactly as described: the full-range staging memcpy, the full-range `cmd_copy_buffer`, and the full-range `skin_palette.comp` dispatch all still run every frame regardless of `pose_dirty` emptiness, and `pool.sweep` still runs unconditionally every frame.
- **Impact**: Unchanged magnitude assessment (LOW — sub-millisecond at realistic slot counts, relevant only on dense crowd cells); the *scope* of what's left to fix is smaller than it was on 2026-07-02.
- **Related**: #1794 (this sprint, narrows this finding), #1379, #1195, #1284.
- **Suggested Fix**: Unchanged — track frames-since-last-dirty and skip the upload+copy+dispatch trio (not just the CPU fill, which #1794 already handled) when clean for ≥`MAX_FRAMES_IN_FLIGHT`.

#### D6-05: First-sight entities pay a redundant BLAS UPDATE immediately after their fresh BUILD in the same command buffer
- **Severity**: LOW · **Dimension**: Skinning & BLAS · **Location**: `crates/renderer/src/vulkan/context/draw.rs:1878-1902`
- **Status**: Existing: #1812 (OPEN)
- **Description/Impact/Suggested Fix**: Unchanged — confirmed the fall-through comment at `:1870-1883` still states first-sight entities "fall through to the refit unconditionally," with no "built this frame" short-circuit. Note: #1790 (this sprint) fixed the *correctness* of the barrier between this BUILD and the redundant UPDATE (added the missing `AS_READ` bit) but did not remove the redundant UPDATE itself — the two findings share a call site but are orthogonal (one is a sync-correctness bug, now fixed; this one is a throughput/telemetry-skew waste, still open).

#### PERF-D5-NEW-03: SVGF à-trous recomputes the 5×5 spatial-variance estimate in all 5 iterations
- **Severity**: LOW · **Dimension**: GPU Pipeline · **Location**: `crates/renderer/shaders/svgf_atrous.comp:134-150`; `crates/renderer/src/vulkan/svgf.rs:88` (`ATROUS_ITERATIONS = 5`)
- **Status**: Existing: #1813 (OPEN)
- **Description/Impact/Suggested Fix**: Unchanged — confirmed `ATROUS_ITERATIONS: usize = 5` still asserted odd with no per-iteration gate on the spatial-variance recompute.

#### PERF-D5-NEW-04: ReSTIR reservoir SSBOs (~130-530 MB screen-dependent) are absent from `memory-budget.md` and all VRAM telemetry
- **Severity**: LOW · **Dimension**: GPU Pipeline · **Location**: `crates/renderer/src/vulkan/restir.rs:34,69` (`RESERVOIR_STRIDE = 32`, `width * height * RESERVOIR_STRIDE`)
- **Status**: Existing: #1814 (OPEN)
- **Description/Impact/Suggested Fix**: Unchanged — confirmed the buffer sizing formula is still present with no corresponding `memory-budget.md` row or telemetry line.

---

## Existing Issues Re-confirmed (adjacent, not duplicates)

| Issue | Note |
|---|---|
| #1698 (OPEN, HIGH, performance) | Skyrim Dragonsreach ECS scheduler stalls ~140 ms/frame for ~28 s — a post-load Rapier/ECS-scheduler settle-storm (`docs/audits/AUDIT_RUNTIME_2026-06-26.md`). D7-NEW-01 is the load-time freeze that precedes and compounds with it. Unaffected by this sprint. |
| #1849 (OPEN, LC0702-05) | WRLD NAM3/NAM4 LOD-water + OFST cell-offset table skipped — a legacy-compat gap, not a performance finding; noted here only because it surfaced in an issue-title search during dedup and is out of this audit's scope. |

---

## Prioritized Fix Order

1. **PERF-D5-NEW-02 (MEDIUM)** — sort or distance-select the GI-hit light prefix; fixes a quality bug and wasted ray budget together. Highest-value remaining item since the other 3 MEDIUMs are explicitly gated on a measurement or repro that doesn't exist yet.
2. **D7-NEW-01 (MEDIUM, now measurable)** — `npc_spawn_wall` is live; run an NPC-dense interior transition (a populated Skyrim inn, or FNV's own dense interiors) and read the number before scoping the chunked-budget rewrite.
3. **D6-03 (MEDIUM, now measurable)** — same pattern: run `skin.coverage` on a moving-crowd bench scene to get `gpu_skin_blas_refit_ms` before deciding whether the scratch-slot round-robin rewrite is worth its sync-redesign risk.
4. **PERF-D3-NEW-02 (MEDIUM)** — needs a low-VRAM-budget `--grid` repro before either sub-fix (lazy re-acquire, burst-boundary tick) is safe to build; unreachable on the 12 GB dev card today, so this can wait for a repro opportunity rather than blocking on effort.
5. **D6-04 (LOW, narrowed)** — the remaining upload+copy+dispatch skip-on-clean-frames work; smaller scope than before #1794.
6. **All other LOW findings** — unchanged from 2026-07-02's own prioritization; batch as a hygiene pass (env-var caching, redundant probes, stale comments, missing warns, doc-link fix, ReSTIR/memory-budget doc rows).
7. **Process recommendation** — run the overdue R6a-stale-15 three-scene GPU bench. It is now more stale than at either of the last two audits and gates any current FPS claim.

No finding requires reverting a Vulkan render-pass, barrier, or
pipeline-state change speculatively.

---

## Methodology Note

This audit read the full diff (not just the commit message) of every fix
commit it credits as resolving a prior finding: `af6e4c9b` (#1791),
`e040231a` (#1796), `e682f78c` (#1792), `e3e9df0d` (#1795), `15e63cee`
(#1799), `c7c1fe1d` (#1794), `9f48a16e` (#1804), `d68c86c9` (#1803), plus
the two safety-adjacent `d688fe06` (#1790) and `6245106c` (#1782). For
`D6-01`/`D6-02` specifically, the ordering claim ("no early return exists
between the upload/commit and the flag flip") was independently re-derived
by grepping `return Ok`/`return Err` across the entire `draw_frame` body
and checking each site's line position against the upload/flip sites,
rather than trusting the commit message's own account. For the 12 carried-
forward LOW findings and the 4 MEDIUM findings whose code sites the sprint
didn't touch, each location was re-grepped against current line numbers to
confirm the finding hasn't gone stale (all confirmed present). No genuinely
new finding was discovered in code the sprint didn't touch — expected,
since that code hasn't moved since the thorough 2026-07-01/07-02 sweeps.
