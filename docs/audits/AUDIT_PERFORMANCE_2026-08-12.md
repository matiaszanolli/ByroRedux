# Performance Audit — 2026-08-12

**Suite**: `/audit-suite renderer-deep` → `/audit-performance` (focused run)
**Date**: 2026-08-12 · **HEAD**: `efc089ba` · **Branch**: `main` · **Depth**: deep
**Hardware target**: RTX 4070 Ti (12 GB) + Ryzen 7950X (16c/32t) · RT VRAM minimum 6 GB · total budget < ~4 GB
**Ceilings source**: [memory-budget.md](docs/engine/memory-budget.md) (authoritative; cited, not re-derived)

---

## 1. Executive Summary

### SCOPE — read this before acting on anything below

This was the `renderer-deep` suite's **focused** run of `/audit-performance`:
**Dimensions 1, 2, 3 and 5 only.**

| Dimension | Title | Run? |
|---|---|---|
| 1 | CPU Per-Frame Allocations & Hot Paths | **YES** |
| 2 | Draw-Call & Instancing Efficiency | **YES** |
| 3 | GPU Memory Pressure & Eviction Thrash | **YES** |
| 4 | SSBO sizing / `#[repr(C)]` layout lockstep | **NOT RUN** |
| 5 | GPU Pipeline & Pass Efficiency | **YES** |
| 6 | Skinning + BLAS/TLAS build & refit | **NOT RUN** |
| 7 | Streaming & cell lifecycle | **NOT RUN** |
| 8 | NIF parse cost | **NOT RUN** |
| 9 | Telemetry & render-origin cost | **NOT RUN** |

Dimensions 4, 6, 7, 8 and 9 were **not executed and are not covered by this report**.
Absence of a finding in those areas is absence of evidence, not evidence of absence.
Do not read this document as a full-coverage performance pass.

**No benchmark was run.** No Vulkan device and no game data were in scope. **No absolute
FPS or frame-time figure is asserted anywhere in this report.** Every quantity that appears
below is either a scalar read out of a checked-in artifact
(`.claude/audit-baselines/runtime/*.tsv`, [memory-budget.md](docs/engine/memory-budget.md))
or an explicitly labelled derivation from source constants. ROADMAP.md's Bench-of-record
block is self-flagged `R6a-stale-*` (116 commits stale, "re-run before any further perf
claim") and was treated as **non-gating**; nothing from it is asserted.

---

### THE HEADLINE — four CLOSED issues have no fix on `main`

**PERF-D3-01 and PERF-D5-01 both trace to a single orphaned fix commit.**

Commit `f3babea3` — *"Fix #2460 #2461 #2462 #2463: AS scratch sizing, RT shading, GPU
struct pin"* (2026-08-08) — **is not an ancestor of `main`**. Independently verified:

- `git merge-base --is-ancestor f3babea3 main` → **false**
- `git branch -a --contains f3babea3` → only `fix/2460-2461-2462-2463-as-rt-correctness`
- `blas_scratch_peak` (the helper that commit introduced) appears **nowhere in the working tree**

Consequently **four issues — #2460, #2461, #2462, #2463 — are CLOSED on GitHub with no fix
on `main`.**

This is a **process failure, not a coding failure.** The tracker asserts these are fixed,
so no future audit, triage sweep, or regression run will re-surface them on its own; they
are invisible to every mechanism the project uses to remember open work. The two findings
below are the audit re-discovering them by accident, from the code side.

**Recommended action: MERGE (or cherry-pick) the branch
`fix/2460-2461-2462-2463-as-rt-correctness` onto `main`, then re-verify all four issues.
Do not re-fix the code — the fix already exists and is already reviewed.** Separately, add a
merged-ness check to whatever closes issues; four CLOSED issues with no fix on `main` is a
defect in the close path.

This is also **why PERF-D3-01 presents as a regression-of-closed rather than as a new bug**:
the defect was found, triaged, fixed and closed, and then the fix failed to land. See
§4 *Eroded Guards vs New Issues* — the skill requires these be separated from new work, and
they are.

Note also that **#2463** (missing `GpuTerrainTile` size/offset lockstep test) is a fourth
casualty of the same orphaned branch. It is confirmed absent at HEAD
(`grep GpuTerrainTile` in `scene_buffer/gpu_instance_layout_tests.rs` → no hits) but belongs
to **Dimension 4, which was not run**, so it is *not* counted among this report's findings.
It is flagged here so it is not lost.

---

### Findings by severity

| Severity | Count | IDs |
|---|---:|---|
| CRITICAL | 0 | — |
| **HIGH** | **1** | PERF-D3-01 |
| **MEDIUM** | **4** | PERF-D1-01, PERF-D3-02, PERF-D3-03, PERF-D5-01 |
| **LOW** | **6** | PERF-D1-02, PERF-D2-01, PERF-D2-02, PERF-D2-03, PERF-D2-04, PERF-D2-05 |
| **Total** | **11** | |

**HIGH — PERF-D3-01**: `shrink_blas_scratch_to_fit` sizes its shrink peak from static BLAS
only, ignoring `skinned_blas`, which shares the same scratch buffer and re-validates nothing
on the refit path. The shrink is **ratio-gated, not budget-gated**, so it is reachable on
the 12 GB dev card, at every cell unload. The consequence is a write past an allocation —
GPU memory corruption. HIGH is retained only to match #2460's original triage; the severity
table's "writing outside an allocation" row would argue CRITICAL.

**MEDIUM**: an ungated per-frame skinned-bounds refold (PERF-D1-01); a permanently-latched
compaction gate that pays a full multi-hundred-MB CPU copy of both geometry pools at every
global-SSBO rebuild (PERF-D3-02); a VRAM ledger understated by 32 B/px across two
independent omissions (PERF-D3-03); and the shader half of the orphaned commit, burning ray
budget on guaranteed self-hits (PERF-D5-01).

**LOW**: three documentation/rationale drifts (PERF-D2-01/02/03), one measured-cost
hypothesis filed explicitly as a hypothesis (PERF-D2-04), one bounded worst-case memcpy
(PERF-D2-05), and three residual per-frame heap allocations (PERF-D1-02).

---

### Key NEGATIVE and verification results — what came back CLEAN

These matter as much as the findings. Several are disproofs of things an auditor would
reasonably suspect.

- **Shipped SPIR-V matches source: 21/21 identical.** All 21 GLSL sources were recompiled
  with `glslangValidator -V -I.` into a scratch dir and **byte-compared** against the
  checked-in `.spv`, including `triangle.frag.spv` (315 416 B). **No stale shader artifact
  exists anywhere in `crates/renderer/shaders/`.**
- **`ENABLE_LEGACY_WRS = 0` and the dead-code elimination is real in the shipped artifact.**
  `strings triangle.frag.spv | grep -i "resLight\|resWSel\|NUM_RESERVOIRS"` → **no hits**.
  The legacy-WRS gate (#1799) is not merely set — the code is genuinely gone from the
  binary.
- **`RT_COMPILE_ABLATION_MASK = 0`** — the shipped build has every ray path enabled; the
  ablation harness does not leak into the default build.
- **CPU-side `inv_vp` confirmed.** `draw.rs` computes `vp_mat.inverse()` once per frame into
  `inv_vp_arr`; `cluster_cull.comp` and `ssao.comp` both take `mat4 invViewProj` as a
  precomputed input. `grep "inverse("` across every shader finds **no** `inverse(viewProj)`
  in any shader body.
- **BGSM/BGEM half-eviction confirmed, and the suspected bypass was disproved.** Both
  `bgem_cache` and `failed_paths` drop the oldest `N/2` via insertion-order `VecDeque`; no
  full-flush path exists. The second `bgem_cache.insert` at `insert_bgem_for_test` — which
  looks like a cap bypass — is `#[cfg(test)]`. **Disproved as a bypass.**
- **Deferred-destroy depth is exactly `MAX_FRAMES_IN_FLIGHT` on all three queues.**
  `DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT`; `DeferredDestroyQueue::tick` checks `== 0`
  *before* decrement, so an item survives exactly `DEFAULT_COUNTDOWN` ticks (pinned by
  `default_countdown_survives_max_frames_in_flight_ticks`). The texture side uses the
  frame-id variant at the same depth. **No path frees earlier.**

### Calibration evidence — two findings where the auditor argued against itself

Recorded here rather than buried, because they are the report's own evidence that its
severities are not inflated:

- **PERF-D2-05 was DOWNGRADED from MEDIUM to LOW** after the auditor disproved its own
  claim. The self-swap in the partition loop occurs only while `raster_len == index` — i.e.
  only across the *initial run* of consecutive `in_raster` commands. Once one RT-only draw
  has been seen, every subsequent swap is real. The O(N) waste materialises only in the
  fully-visible case (`BYRO_NO_CULL=1`, or a cell where culling flags nothing). The one-line
  guard is still worth landing for the bounded worst case, but the expected case is small.
- **PERF-D2-03 concluded `DRAW_SORT_PARALLEL_THRESHOLD = 3000` is WELL-PLACED.** The auditor
  set out to show 3000 was misplaced and **could not**. The in-comment crossover table
  (re-measured 2026-07-25 on a 7950X) has serial ~19 % ahead at N=2000, still ahead at
  N=2750, tied at N=3000, parallel pulling away from N=5000. 3000 is the first size where
  the two are interchangeable — the right place for the gate. **Only the justification prose
  around the constant is stale**, and that is all the finding reports.

### Corrected belief

**PERF-D2-02 corrects a previously-held belief.** The two-sided blend split's dormancy has
been recorded as an *empirical* observation ("`blended && two_sided == 0` on every measured
cell"). It is in fact **structural**: `collect_static_mesh_draws` unconditionally force-clears
`two_sided` for `MATERIAL_KIND_GLASS` *before* the `DrawCommand` is constructed, so
`b.two_sided` is false for every glass batch **by construction**. Only kind-11
MultiLayerParallax with a non-zero refraction scale can ever reach the predicate. The
dormancy is a guarantee, not a sample.

### Related — cross-audit overlap (cross-reference only)

Other reports in this suite are being written in parallel; their content is **not** copied here.

- **PERF-D2-01** (stale `z_write` skill text for `needs_two_sided_blend_split`) was
  independently found by **REN-D12-01** in the renderer audit.
- **PERF-D3-03** (memory-budget.md screen-sized ledger understated) overlaps **REN-D5-03 /
  REN-D14-01**. PERF-D3-03 **additionally contributes** that SVGF's 4 à-trous ping-pong
  images were **never ledgered at all** — a gap not covered by the overlapping findings.

---

## 2. Hot Path Analysis

All figures below are read out of checked-in artifacts. Nothing here was measured during
this audit.

### Per-frame CPU

**Skinned-leaf world-bound refold** — [bounds.rs](byroredux/src/systems/bounds.rs),
`make_world_bound_propagation_system`. The largest un-gated per-frame loop found in
Dimension 1. Cost scales as `skin_pool_live × bones_per_skin`, each bone costing a
`Mat4 × Mat4` plus `transform_sphere`'s three `Vec3::length()` square roots. Scale inputs
from `.claude/audit-baselines/runtime/`:

| baseline cell | `skin_pool_live` |
|---|---:|
| `fnv-FreesideAtomicWrangler` | **677** (vs `skin_pool_max` 1364) |
| `fo4-InstituteBioScience` | 124 |
| `skyrim_se-WhiterunDragonsreach` | 83 |

Every one of those 677 entities' full bone list is re-walked on every frame in which
*anything* moved — and because camera motion propagates a `GlobalTransform` write, that is
essentially every frame the player moves. `bones_per_skin` and the resulting millisecond
cost are **not measured anywhere in-repo and were not estimated to a number**.

Secondarily, `render::skinned::build_skinned_palettes` performs the **identical**
`gt.to_matrix()` conversion for every bone of every skinned entity in the same frame — the
bone→matrix conversion is done **twice per bone per frame** by two subsystems that never
share the result.

**Residual per-frame allocations** — order of one to two heap allocations per frame plus a
light-count-sized sort temp: two stable `sort_by` calls (`collect_lights`,
`collect_fog_volumes`) that allocate above the insertion-sort cutoff, and one `format!` per
frame in `InteractionState::prompt`.

**Draw-command sort** — `draw_sort_key` is evaluated on *each side of every comparison*
(≈ `2·N·log₂N` extractions), each touching ~10 fields scattered across a `DrawCommand`
whose field tally puts it near **480 bytes** (~8 cache lines per key build) to materialise a
44-byte tuple. The in-repo comment already attributes measurable cost to key width:
`883f57cd` widening the key 10→11 tuples "raised per-comparison cost and moved the crossover
UP" — direct in-repo evidence that key extraction, not element movement, dominates this sort.

**Draw-count scale** — `bench_draws_*` from the five checked-in runtime baselines
(regenerated 2026-06-14 → 2026-08-06):

| baseline cell | `entities_total` | `bench_draws_cmds` | `bench_draws_batches` | `bench_draws_gpu_calls` |
|---|---:|---:|---:|---:|
| `oblivion-ICMarketDistrictTheGildedCarafe` | 701 | **324** | 47 | 4 |
| `fo3-MegatonPlayerHouse` | 3311 | **1839** | 96 | 9 |
| `skyrim_se-WhiterunDragonsreach` | 8126 | **2342** | 9 | 2 |
| `fnv-FreesideAtomicWrangler` | 9271 | **2553** | 89 | 25 |
| `fo4-InstituteBioScience` | 12448 | **3440** | 753 | 42 |

Exactly one of five sits inside the 400–1500 band the sort-threshold rationale quotes; one
(`fo4-InstituteBioScience`, 3440) is **above** `DRAW_SORT_PARALLEL_THRESHOLD` and takes the
parallel path. Whether its *raster prefix* stays above the gate after the `in_raster`
partition is **not determinable** — `bench_draws_cmds` is the total and the raster/RT-only
split is not a baseline scalar.

**Global-SSBO rebuild** — `compact_pending_geometry` runs a full compaction pass on every
rebuild after the first mesh drop. [memory-budget.md](docs/engine/memory-budget.md) puts
typical resident vertex/index pools at **~208 MB** (soft cap 4 M vertices ≈ **416 MB** at
the 104 B stride), so each pass is a transient ~2× host-RAM spike plus a multi-hundred-MB
scattered single-threaded copy, at each cell boundary.

### Per-pass GPU

**All post passes are resolution-derived, none draw-count-derived.** `record_post_passes`
records **eight fixed passes per frame**. SSAO, TAA, SVGF temporal, SVGF à-trous
(`ATROUS_ITERATIONS = 3`, odd-pinned by a `const_assert`), caustic decay + splat, and water
caustic each derive their `cmd_dispatch` group count from `width`/`height` — **none** from
batch or draw-command counts. Volumetrics is froxel-grid-derived from `render_extent`; bloom
walks `BLOOM_MIP_COUNT` down + up with group counts from the mip extent and takes no
draw/mesh input. The G-buffer is **7 attachments × 2 FIF** (`normal, motion, mesh_id,
raw_indirect, albedo, reactive, transparency`); memory-budget.md's "8 attachments" row
counts depth and is not a discrepancy.

**Screen-sized VRAM per pixel** (from source constants, per PERF-D3-03):

| Resource | Ledgered | Actual |
|---|---:|---:|
| Glass caustic accumulator (3 layers × 4 B × 2 FIF) | — | 24 B/px |
| Water caustic accumulator (4 B × 2 FIF) | — | 8 B/px |
| **Caustics combined** | **16 B/px** | **32 B/px** |
| SVGF `indirect_history` + `moments_history` | 24 B/px | 24 B/px |
| SVGF à-trous ping-pong (4 × 4 B/px) | **0 (never ledgered)** | 16 B/px |
| **SVGF combined** | **24 B/px** | **40 B/px** |

At the doc's own reference resolutions that is **+66 MB at 1080p** and **+265 MB at 4K**
(~6.6 % of the < 4 GB engine budget). All of these are allocated at `render_extent`, which
under the shipped FSR 3.1 Quality default is below output resolution — so the *labels* are
wrong in the other direction, not the ratios.

**Ray-query waste** — every fragment hitting the glass IOR passthru loop or the one-bounce
GI path burns `GLASS_RAY_BUDGET`/`GLASS_RAY_COST` allowance (and the GI sample) on a hit
against the surface it just left. Divergent, so the cost concentrates on glass-heavy
interiors. Not measured.

**Not observable without a device**: pipeline-bind counts, occupancy, real BLAS eviction
behaviour under streaming, scratch high-water across a real cell-unload sequence, and the
per-frame GPU cost of the tripled caustic atomic traffic after `610cb170`.

---

## 3. Findings

### HIGH

---

#### PERF-D3-01: The #2460 BLAS-scratch fix was never merged to `main` — `shrink_blas_scratch_to_fit` still sizes its shrink peak from static BLAS only, and three sibling fixes are orphaned with it

- **Severity**: HIGH
- **Dimension**: 3 — GPU Memory Pressure
- **Location**: [memory.rs](crates/renderer/src/vulkan/acceleration/memory.rs) — `AccelerationManager::shrink_blas_scratch_to_fit` (the `peak` binding); [blas_skinned.rs](crates/renderer/src/vulkan/acceleration/blas_skinned.rs) — `refit_skinned_blas`
- **Status**: **Regression of #2460** (issue CLOSED on GitHub; fix absent from `main`)
- **Description**: `shrink_blas_scratch_to_fit` computes its shrink target by walking
  `self.blas_entries` (static, mesh-keyed BLAS) only, and never consults
  `self.skinned_blas` — even though both families build/refit out of the *same*
  `blas_scratch_buffer`. This is precisely the defect reported and triaged HIGH as
  #2460 ("AS-D1-NEW-01"), and #2460 is marked CLOSED in the dedup baseline. The fix
  commit `f3babea3` exists, but only on the branch
  `fix/2460-2461-2462-2463-as-rt-correctness`, which `git branch --no-merged main`
  reports as unmerged. `git merge-base --is-ancestor f3babea3 HEAD` returns false.
  The helper that commit introduced, *blas_scratch_peak*, exists nowhere in the
  repository (`grep -rn "blas_scratch_peak" crates/` → no hits), and
  `git log -S "blas_scratch_peak" --all` shows exactly one commit — the orphaned one.
  The same orphaned commit carries three siblings, all likewise absent from `main`
  and all likewise CLOSED upstream: **#2461** (GI hemisphere axis never viewer-flipped),
  **#2462** (glass IOR passthru resets `rayTMin` to 0.0 — see PERF-D5-01), and
  **#2463** (no `GpuTerrainTile` size/offset lockstep test — Dimension 4's territory).
- **Evidence**:
  ```rust
  // crates/renderer/src/vulkan/acceleration/memory.rs, shrink_blas_scratch_to_fit
  let peak: vk::DeviceSize = self
      .blas_entries          // <-- static BLAS only; self.skinned_blas never consulted
      .iter()
      .flatten()
      .map(|e| e.build_scratch_size)
      .max()
      .unwrap_or(0);
  ```
  `refit_skinned_blas` (`blas_skinned.rs`) then takes that shared buffer with **no**
  size re-validation — it calls `scratch_needs_growth` only on the *batched build*
  path (`build_skinned_blas_batched_on_cmd`), never on refit:
  ```rust
  let scratch_buffer = self.blas_scratch_buffer.as_ref().context(
      "blas_scratch_buffer absent — must be allocated by build_skinned_blas_batched_on_cmd first",
  )?;
  ```
  The in-code comment justifying this ("This UPDATE reuses the BUILD's padded
  `blas_scratch_buffer` (UPDATE scratch ≤ BUILD scratch), so the round-up headroom is
  already present") is exactly the premise the shrink invalidates.
- **Impact**: Reachable on the 12 GB dev card, unlike the #1793 pair — the shrink is
  **ratio-gated**, not budget-gated (`scratch_should_shrink`: capacity > 2 × peak AND
  excess > `BLAS_REBUILD_SLACK_BYTES` = 16 MB per `memory-budget.md`), and it runs at
  every cell-unload ([unload.rs](byroredux/src/cell_loader/unload.rs)) plus swapchain recreate.
  A transition from a heavy-scratch cell to a light-static one while skinned actors
  (followers, NPCs) stay resident shrinks the shared buffer to the static survivors'
  peak; the next `refit_skinned_blas` writes AS build scratch past the allocation.
  That is GPU memory corruption, not a suboptimal shrink — the severity table's
  "memory corruption / writing outside an allocation" row argues CRITICAL; HIGH is
  retained here only to match #2460's original triage. The `peak == 0` early-drop arm
  has the same hole: with all static BLAS evicted but skinned entities live, the whole
  scratch buffer is deferred-destroyed while refits still target it.
- **Related**: #2460, #2461, #2462, #2463 (all CLOSED, all orphaned on the same
  branch); PERF-D5-01 below is the shader-side half of the same orphan.
  Not related to #1793 (that pair is genuinely budget-gated and unreachable here).
- **Suggested Fix**: Merge or cherry-pick `f3babea3` onto `main`, then re-verify all
  four issues; separately, add a merged-ness check to whatever closes issues, because
  four CLOSED issues with no fix on `main` is a process defect, not a code one. If the
  branch is rejected for unrelated reasons, the minimum standalone fix is to union
  `skinned_blas` into the `peak` walk and re-open #2460.

---

### MEDIUM

---

#### PERF-D1-01: Skinned-leaf world-bound refold is the one ungated pass in an otherwise fully dirty-gated system

- **Severity**: MEDIUM
- **Dimension**: 1 — CPU Hot Paths
- **Location**: [bounds.rs](byroredux/src/systems/bounds.rs) — `make_world_bound_propagation_system` (skinned block, ~lines 189-203) + `skinned_world_bound` (~lines 31-48)
- **Status**: NEW
- **Description**: `make_world_bound_propagation_system` is otherwise a model of
  incremental design: it drains `GlobalTransform`'s dirty set into a persistent
  `g_dirty` (#1371), early-returns when `g_dirty.is_empty() && !structural_changed`,
  drives Pass 1 off `g_dirty`, and drives Pass 2 off a `dirty_roots` set walked up
  from `g_dirty`. The skinned-leaf block between them has **no per-entity dirty
  check at all** — it iterates *every* `SkinnedMesh` entity and recomputes the full
  bone-palette-enclosing sphere whenever *anything* in the world moved. Because the
  camera's own `Transform` mutation propagates a `GlobalTransform` write (and
  `make_billboard_system` writes GT for every billboard on camera motion), `g_dirty`
  is non-empty on essentially every frame the player moves — so this block runs
  every frame regardless of whether any *bone* moved.
- **Evidence**: The block is bracketed by neither `g_dirty` nor `structural_changed`:
  ```rust
  if let Some(ref sq) = skin_q {
      for (entity, skin) in sq.iter() {
          let Some(local) = lb_q.get(entity) else { continue };
          let bound = skinned_world_bound(local, skin, |bone| {
              g_q.get(bone).map(GlobalTransform::to_matrix)
          });
          ...
      }
  }
  ```
  and `skinned_world_bound` is per-bone `Mat4 × Mat4` plus `transform_sphere`'s
  three `Vec3::length()` square roots:
  ```rust
  for (bone, bind_inverse) in skin.bones.iter().zip(&skin.bind_inverses) {
      let palette = bone.and_then(&mut bone_world)
          .map(|world| world * *bind_inverse).unwrap_or(Mat4::IDENTITY);
      merged = merged.merge(&transform_sphere(local, palette));
  }
  ```
  Scale is not hypothetical: the repo's own checked-in runtime baseline
  `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv` records
  `skin_pool_live 677` (vs `skin_pool_max 1364`) — 677 live `SkinnedMesh` entities
  in one FNV interior. `skyrim_se-WhiterunDragonsreach.tsv` records 83 and
  `fo4-InstituteBioScience.tsv` 124. Every one of those entities' full bone list is
  re-walked, per frame, with a matrix multiply and three square roots per bone.
  Secondly, the same frame's `render::skinned::build_skinned_palettes` performs the
  *identical* `gt.to_matrix()` conversion for every bone of every skinned entity
  into `bone_world` — the bone→matrix conversion is done twice per bone per frame by
  two subsystems that never share the result.
- **Impact**: Unbounded-by-dirty-state CPU cost proportional to
  `skin_pool_live × bones_per_skin`, paid on the PostUpdate stage of every frame in
  which anything moved. On the 677-skin FNV baseline this is the largest single
  un-gated per-frame loop found in Dimension 1. The work is *not* redundant for
  actively animating actors (their bones genuinely move), so the win is confined to
  idle/asleep skinned entities and camera-only-motion frames — but that is exactly
  the steady state the rest of this system was rewritten to exploit. On a 7950X
  this is the class of cost the audit charter calls a bug rather than a tuning gap.
  **No quantitative guard exists for this site** — there is no dhat bound and no
  runtime-baseline scalar that would catch it growing.
- **Related**: #1371 (`drain_dirty_into`, intact), #1195 / PERF-DIM7-01
  (`SkinSlotPool::try_mark_pose_dirty` — an already-computed per-entity
  "bones changed?" signal, but produced later in the same frame by
  `build_skinned_palettes`, so not usable as-is without a one-frame lag).
  Adjacent to but distinct from #1794.
- **Suggested Fix**: Gate the skinned block per entity: `g_dirty` is already
  `sort_unstable`'d + `dedup`'d in the incremental branch, so a `binary_search` of
  each `skin.bones` entry against it (plus the mesh entity itself) skips the whole
  matrix/sqrt path for clean skins at a fraction of the cost. Do the same sort in
  the `structural_changed` branch so the gate is uniform. Longer term, consider
  publishing `build_skinned_palettes`' per-bone world matrices as a frame resource
  so bounds and the palette pass stop computing `to_matrix()` twice per bone.
  Quantify with a targeted micro-bench before and after — no existing harness covers
  this loop.

---

#### PERF-D3-02: `compact_pending_geometry`'s dirty gate is permanently true after the first mesh drop, so every global-SSBO rebuild pays a full CPU copy of both geometry pools

- **Severity**: MEDIUM
- **Dimension**: 3 — GPU Memory Pressure
- **Location**: [mesh.rs](crates/renderer/src/mesh.rs) — `MeshRegistry::compact_pending_geometry`, called unconditionally from `MeshRegistry::rebuild_geometry_ssbo`
- **Status**: NEW (no issue in the 400-entry baseline mentions `compact_pending_geometry`; the symbol appears in no other `.rs` or `.md` file in the tree)
- **Description**: The compaction fast-path tests `self.meshes.iter().any(|slot| slot.is_none())`.
  But `meshes` is a **grow-only** `Vec<Option<GpuMesh>>` whose dropped slots hold `None`
  *forever* by deliberate design — `release_mesh_ref` does `slot.take()` and the upload
  paths always `push` a fresh slot (handle stability, #372). So the moment the first
  mesh of any kind is dropped — the first cell unload, the first LOD swap — `any_dead`
  latches true for the rest of the process, and the "fast path: no holes → nothing to
  compact" branch is dead code from then on. Every subsequent `rebuild_geometry_ssbo`
  runs a full compaction pass even when nothing has been dropped since the previous one,
  in which case the pools already have no holes and the pass reproduces a byte-identical
  layout. The function's own doc comment states the opposite contract: *"Pure appends
  (no drops) skip this pass."*
- **Evidence**:
  ```rust
  fn compact_pending_geometry(&mut self) {
      // Fast path: no holes → nothing to compact.
      let any_dead = self.meshes.iter().any(|slot| slot.is_none());
      if !any_dead { return; }
      let mut new_vertices: Vec<Vertex> = Vec::with_capacity(self.pending_vertices.len());
      let mut new_indices:  Vec<u32>    = Vec::with_capacity(self.pending_indices.len());
      … per-live-mesh extend_from_slice over the whole pool …
      self.pending_vertices = new_vertices;
      self.pending_indices  = new_indices;
  }
  ```
  and the slot-never-reused invariant it collides with:
  ```rust
  // MeshRegistry::drop_mesh doc
  /// Handles stay stable: the dropped slot holds `None` forever.
  ```
- **Impact**: CPU-side, per global-SSBO rebuild — i.e. per cell load once streaming is
  running. Two fresh allocations sized to the whole live pool plus a scattered
  per-mesh copy of it, then a free of the old pair. `memory-budget.md` puts the typical
  resident vertex/index pools at **~208 MB** (soft cap 4 M vertices ≈ 416 MB at the
  104 B stride); that is a transient ~2× host-RAM spike and a multi-hundred-MB
  scattered copy on a single thread at each boundary crossing, on a machine where a
  CPU bottleneck is a bug by policy. No GPU correctness impact — the compaction result
  is correct, just redundant. No quantitative guard exists for this site (dhat covers
  the NIF parse path only, per the skill's alloc-bound note), so the cost is reasoned
  from the pool ceilings rather than measured.
- **Related**: `check_pool_growth` soft/hard caps (verified intact, below); the
  batched-teardown work in `app_step.rs::step_streaming` that already de-duplicated
  the *global* `shrink_storages`/`shrink_blas_scratch_to_fit` passes per boundary —
  this is the same class of "global pass repeated when its precondition no longer
  holds" that batching closed elsewhere.
- **Suggested Fix**: Replace the `any_dead` scan with a `geometry_has_holes: bool` flag
  set in `release_mesh_ref` when `was_scene_mesh` is true and cleared at the end of
  `compact_pending_geometry`. `geometry_dirty` cannot be reused — appends set it too.

---

#### PERF-D3-03: `memory-budget.md`'s screen-sized-resource ledger understates actual VRAM by 32 B/px across two independent omissions (glass caustic RGB array; SVGF à-trous ping-pong)

- **Severity**: MEDIUM
- **Dimension**: 3 — GPU Memory Pressure
- **Location**: [memory-budget.md](docs/engine/memory-budget.md) — "Glass + Water Caustics" and "SVGF (indirect-lighting denoiser)" tables plus the "VRAM Rough Budget" rows; ground truth in [caustic.rs](crates/renderer/src/vulkan/caustic.rs) (`CAUSTIC_COLOR_LAYERS`, `CausticPipeline::create_slot`) and [svgf.rs](crates/renderer/src/vulkan/svgf.rs) (`SvgfPipeline::new_inner`, `atrous_color`)
- **Status**: NEW (no baseline issue covers either site; the closest precedent, #1814/#1872, created these very sections)
- **Description**: Two separate ledger gaps, same remedy.
  1. **Glass caustics tripled yesterday and the ledger did not follow.** Commit
     `610cb170` (2026-08-11) turned the glass accumulator into a three-layer R32_UINT
     array for RGB radiance: `const CAUSTIC_COLOR_LAYERS: u32 = 3;` and
     `.array_layers(CAUSTIC_COLOR_LAYERS)` in `create_slot`, pinned by
     `caustic_accumulator_spans_rgb_array_layers`. `water_caustic.rs` stayed at
     `.array_layers(1)`. The doc still says "each own a full-resolution R32_UINT
     (4 B/px) atomic accumulator image, double-buffered per FIF — two independent
     accumulators, 16 B/px combined". Actual: glass 3 × 4 B × 2 FIF = 24 B/px, water
     4 B × 2 FIF = 8 B/px → **32 B/px**.
  2. **SVGF's à-trous ping-pong pair was never ledgered.** The doc enumerates
     "two double-buffered history images per slot: `indirect_history` (4 B/px) and
     `moments_history` (8 B/px) … 24 B/px total". `SvgfPipeline::new_inner` also
     allocates, inside the same per-FIF loop, **two** full-resolution
     `INDIRECT_HIST_FORMAT` à-trous colour images per frame-in-flight
     (`for pp in 0..2 { … partial.atrous_color.push(at); }`), consumed by the
     `ATROUS_ITERATIONS` (currently 3) spatial pass. That is 4 × 4 B/px = 16 B/px
     never counted → actual **40 B/px**, not 24.
- **Evidence**: `CAUSTIC_COLOR_LAYERS = 3` + `caustic_subresource_range().layer_count == 3`
  (test-pinned); `svgf.rs` `atrous_color` is `Vec<HistorySlot>` indexed
  `frame * 2 + pp` and destroyed alongside the two histories in `destroy()`.
- **Impact**: Doc-only defect, but the doc is the *authoritative* input to every VRAM
  decision (this audit is required to cite it rather than re-derive). At the doc's own
  reference resolutions the understatement is **+66 MB at 1080p** (SVGF 49.8 → 82.9;
  caustics 33.2 → 66.4) and **+265 MB at 4K** (SVGF 199.1 → 331.8; caustics
  132.7 → 265.4) — ~6.6 % of the < 4 GB engine budget at 4K, on top of a peak table
  that is already the tightest part of that budget. Real-world numbers are lower than
  the doc's headings imply because all of these are allocated at `render_extent`
  (verified: `context/mod.rs` passes `render_extent.width/height` to
  `CausticPipeline::new` and `SvgfPipeline::new`), which under the shipped FSR 3.1
  Quality default is below output resolution — but that makes the *labels* wrong in the
  other direction, not the ratios.
- **Related**: #1814 (ReSTIR ledger entry + attributing telemetry), #1872 (the sweep
  that added this whole section and grepped for exactly these omissions). Both are the
  precedent for treating ledger drift as a finding. **Cross-audit**: overlaps REN-D5-03 /
  REN-D14-01 in the renderer report; the SVGF à-trous half is this report's unique
  contribution.
- **Suggested Fix**: Recompute both tables and the "VRAM Rough Budget" rows against
  `CAUSTIC_COLOR_LAYERS` and the à-trous pair; add the one-line `log::info!`
  size report at `CausticPipeline::new`/`create_slot` and `SvgfPipeline::new_inner`
  that #1814 established as the attribution mechanism, so the next resolution or
  layer-count change is self-reporting.

---

#### PERF-D5-01: The two orphaned shader fixes (#2461, #2462) leave the GI and glass-passthru ray paths spending ray-query budget on guaranteed self-hits

- **Severity**: MEDIUM
- **Dimension**: 5 — GPU Pipeline
- **Location**: [triangle.frag](crates/renderer/shaders/triangle.frag) — the glass IOR passthru loop (`rayTMin = 0.0;` inside the refraction-passthru arm) and the one-bounce GI hemisphere axis derived from `fragNormalEffective`
- **Status**: **Regression of #2462 and #2461** (both CLOSED; fix absent from `main`)
- **Description**: Same root cause as PERF-D3-01 — commit `f3babea3` never merged. Both
  defects are still present verbatim at HEAD:
  - **#2462**: the glass IOR refraction passthru loop enters with the shader-wide
    `rayTMin = 0.05` convention and then resets `rayTMin = 0.0;` on every subsequent
    passthru iteration, leaning entirely on a `0.05` origin nudge along the newly
    refracted direction. At a grazing exit that nudge projects to well under 0.05 of
    perpendicular clearance from the interface just crossed, so the fragment's *own*
    already-crossed surface is a committable hit at `t ≈ 0`.
  - **#2461**: the GI hemisphere axis is built from `fragNormalEffective` and is never
    viewer-flipped, while its ray-origin bias `N_bias` always is
    (`vec3 N_bias = dot(N, V) < 0.0 ? -N : N;`). On a two-sided draw's back face the
    hemisphere points into the surface while the origin points toward the viewer.
- **Evidence**: `grep -n "rayTMin = 0.0;" crates/renderer/shaders/triangle.frag` →
  one hit, still live. The viewer-flip pattern the fix mirrors is present for the two
  sibling consumers of the same input (`macroN`, `glassViewNormal`) but not for the GI
  axis. `git merge-base --is-ancestor f3babea3 HEAD` → false.
- **Impact**: Pure pipeline waste on top of the visual defect, which is why it lands in
  this dimension rather than only in a correctness one. Every affected fragment burns
  `GLASS_RAY_BUDGET`/`GLASS_RAY_COST` allowance (and, for GI, the one-bounce sample)
  on a hit against the surface it just left; the GI case additionally clamps `rtAO` to
  its 0.3 floor, so the shading work downstream of it is spent producing a
  known-wrong result. Divergent, so the cost concentrates on exactly the
  glass-heavy interiors the Prospector control bench targets.
- **Confidence / Speculative-Vulkan caveat**: This is **shader source, not pipeline or
  barrier state** — the claim is verifiable by reading the compiled artifact, and the
  fix is a revert-to-an-existing-commit, so the caveat's "needs RenderDoc or a revert
  plan" bar is met by the revert plan (`f3babea3`). No render-pass, barrier, or
  pipeline-state change is proposed here.
- **Related**: PERF-D3-01 (same orphaned commit); #779 (`early_fragment_tests`, still
  unaddressed and still a correctness trade against the alpha-cutout `discard` sites —
  re-confirmed, not re-reported).
- **Suggested Fix**: Merge `f3babea3` (see PERF-D3-01) and recompile
  `triangle.frag.spv`; the orphaned commit already did the recompile.

---

### LOW

---

#### PERF-D1-02: Residual per-frame heap allocations on the render tick — `sort_by` scratch in the light/fog decorate sorts, and the interaction-prompt `format!`

- **Severity**: LOW
- **Dimension**: 1 — CPU Hot Paths
- **Location**: [lights.rs](byroredux/src/render/lights.rs) — `collect_lights` (the `sort_scratch.sort_by` call, ~line 293); [fog_volumes.rs](byroredux/src/render/fog_volumes.rs) — `collect_fog_volumes` (`out.sort_by`, ~line 49); [interaction.rs](byroredux/src/interaction.rs) — `InteractionState::prompt`, reached from `build_interaction_prompt` in [main.rs](byroredux/src/main.rs)
- **Status**: NEW
- **Description**: Three small per-frame heap allocations survive in code whose
  surrounding lines were explicitly rewritten to eliminate exactly this pattern.
  (a) `collect_lights` builds its decorate buffer allocation-free (`sort_scratch` is
  caller-owned and `clear`+`extend`ed per #2172 — guard intact), then immediately
  sorts it with `sort_by`, the **stable** sort, which allocates a temporary above the
  insertion-sort cutoff. The decorate tuple is `(f32, GpuLight)`, so the temp scales
  with the light count. Stability buys nothing here: the array is a freshly built
  decoration, and `sort_unstable_by` is still deterministic for a given input, so
  no frame-to-frame GI-prefix flicker is introduced. (b) `collect_fog_volumes` has
  the same `sort_by`-then-`truncate` shape. (c) `build_interaction_prompt` runs on
  **every** frame — including the overlay-hidden path, which is the only field
  #1376 deliberately left populated when hidden — and `InteractionState::prompt` is
  `format!("[E] {}", target.kind.verb())`, one `String` per frame whenever the
  player is looking at an activatable.
- **Evidence**:
  ```rust
  // render/lights.rs
  sort_scratch.clear();
  sort_scratch.extend(suffix.iter().map(|l| (gi_priority_score(l), *l)));
  sort_scratch.sort_by(|a, b| b.0.total_cmp(&a.0));   // stable → allocates
  ```
  ```rust
  // interaction.rs
  pub(crate) fn prompt(&self) -> Option<String> {
      self.target.map(|target| format!("[E] {}", target.kind.verb()))
  }
  ```
- **Impact**: Small — order of one to two allocations per frame plus a
  light-count-sized temp. Reported at LOW precisely because the magnitude is small;
  it is included because these are the last three sites in the per-frame render tick
  still doing what #1372 / #1725 / #2034 / #2172 removed everywhere else, and
  because **no quantitative guard exists for any per-frame render/ECS site** — there
  is nothing that would flag them growing.
- **Related**: #2034 / #2172 (decorate-sort-undecorate + caller-owned scratch for
  `collect_lights`); #1376 (debug-UI snapshot visibility gate, intact).
- **Suggested Fix**: Swap both `sort_by` calls to `sort_unstable_by`. For the
  prompt, return a `&'static str` verb plus a formatting decision at the UI layer,
  or cache the composed string on `InteractionState` and rebuild it only when
  `target` changes.

---

#### PERF-D2-01: `/audit-performance` Dimension 2 checklist still describes the pre-#2165 `z_write` form of `needs_two_sided_blend_split`

- **Severity**: LOW
- **Dimension**: 2 — Draw & Instancing
- **Location**: `.claude/commands/audit-performance/SKILL.md:91` (Dimension 2 checklist, "Two-sided blend split gate (#1804)")
- **Status**: NEW
- **Description**: The skill text instructs auditors that
  `needs_two_sided_blend_split(&DrawBatch)` "requires `z_write` in addition to
  `is_blend && two_sided`", and frames a split on a non-depth-writing batch as the
  regression to look for. The live predicate has not had a `z_write` limb since
  #2165: it is `is_blend && b.two_sided && b.order_dependent_glass`. The `z_write`
  proxy was removed *deliberately* — FO4 BGEM glass is commonly authored
  `z_write == false`, so the old spelling excluded the population the split exists
  for. An auditor following the skill literally would report the correct current
  code as a regression.
- **Evidence**: [draw.rs](crates/renderer/src/vulkan/context/draw.rs):
  ```rust
  pub(super) fn needs_two_sided_blend_split(b: &DrawBatch) -> bool {
      let is_blend = matches!(b.pipeline_key, PipelineKey::Blended { .. });
      is_blend && b.two_sided && b.order_dependent_glass
  }
  ```
  The doc comment above it states the history explicitly ("Both earlier spellings
  were wrong in opposite directions"), and `DrawBatch::order_dependent_glass`'s own
  doc says "The material kind is the real signal; depth state never was."
- **Impact**: Documentation only — but it is the kind of drift that manufactures a
  false-positive finding in every subsequent Dimension-2 run, which is precisely the
  noise class the audit-hygiene rules exist to suppress.
- **Related**: #1804, #2165, `8e55a714`, #2215. **Cross-audit**: independently found
  as REN-D12-01 in the renderer audit.
- **Suggested Fix**: Update the Dimension 2 checklist bullet to the live predicate
  and re-point the "regression to watch for" at a split reappearing on
  non-`order_dependent_glass` batches (the #2165 particle case), not on
  non-`z_write` ones.

---

#### PERF-D2-02: The two-sided blend split's dormancy is structural, not just empirical — glass can never satisfy `two_sided`

- **Severity**: LOW
- **Dimension**: 2 — Draw & Instancing
- **Location**: [static_meshes.rs](byroredux/src/render/static_meshes.rs) — `collect_static_mesh_draws`, the glass single-sided override (~lines 448-452); consumed by `needs_two_sided_blend_split` / `is_refractive_glass` in [draw.rs](crates/renderer/src/vulkan/context/draw.rs)
- **Status**: NEW (mechanism); the dormancy itself is already recorded empirically in `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`'s header
- **Description**: The FNV baseline header and prior audit notes record that
  `blended && two_sided == 0` on every measured cell, and correctly warn that
  changes to `needs_two_sided_blend_split` are runtime no-ops. That is presented as
  an observation. It is in fact a **structural guarantee** for the predicate's
  primary target population, and the guarantee is not documented at either site.
  `is_refractive_glass` accepts two signals: `material_kind == MATERIAL_KIND_GLASS`,
  and `material_kind == 11` (MultiLayerParallax) with a non-zero refraction scale.
  But `collect_static_mesh_draws` — the only producer of glass `DrawCommand`s —
  unconditionally clears `two_sided` for `MATERIAL_KIND_GLASS` *before* the
  `DrawCommand` is constructed. So `b.two_sided` is false for every glass batch by
  construction, and `is_blend && two_sided && order_dependent_glass` can only ever be
  satisfied by an alpha-blended, two-sided, kind-11 MultiLayerParallax draw with
  `multi_layer_refraction_scale > 0` — a vanishingly rare Skyrim+ authoring case.
- **Evidence**:
  ```rust
  // render/static_meshes.rs — the only site that sets two_sided on a glass draw
  let two_sided = if material_kind == byroredux_renderer::MATERIAL_KIND_GLASS {
      false
  } else {
      two_sided
  };
  ```
  The other two `DrawCommand` producers cannot reach the predicate either:
  `render::particles::emit_particles` hardcodes
  `material_kind: MATERIAL_KIND_EFFECT_SHADER` (101, rejected by
  `is_refractive_glass` — this is #2165 working as intended), and
  `render::water::reemit_water_planes` only flips `is_water` on an
  already-emitted command, which `draw.rs` excludes from batch formation via
  `skip_batch`.
- **Impact**: No runtime cost — the dead path costs nothing. The impact is
  interpretive: the split is carried as a live mitigation for the #1804/#2237 glass
  compositing artifact, when for engine-classified glass that artifact is actually
  handled by the single-sided override (which solves it by removing back faces
  entirely, at the documented cost of glass interiors not rendering). Two
  independent mitigations for one artifact, one of them unreachable, with neither
  site cross-referencing the other. This also means Dimension 2's split-related
  checklist items are unfalsifiable on real content and should not be used to
  attribute batch-count movement — consistent with the RT-1 / #2215 conclusion that
  the depth-primary alpha-over sort, not this predicate, drove the
  `bench_draws_batches` rise.
- **Related**: #1804, #2165, #2215, #2237; the `two_sided_blend_split_dormant` note.
- **Suggested Fix**: No code change. Add a cross-reference from
  `needs_two_sided_blend_split`'s doc comment to the `MATERIAL_KIND_GLASS`
  single-sided override in `static_meshes.rs`, stating that the glass arm of
  `is_refractive_glass` is unreachable through `b.two_sided` and that kind-11 is the
  only live population. That converts a repeatedly-rediscovered empirical surprise
  into a stated invariant.

---

#### PERF-D2-03: `sort_draw_commands`' threshold rationale cites a "typical Bethesda cell" draw-count band the repo's own runtime baselines contradict

- **Severity**: LOW
- **Dimension**: 2 — Draw & Instancing
- **Location**: [mod.rs](byroredux/src/render/mod.rs) — `sort_draw_commands` (`DRAW_SORT_PARALLEL_THRESHOLD`) and the rationale comment in `build_render_data` immediately above the `sort_draw_commands` call
- **Status**: NEW
- **Description**: **The constant itself checks out** — I set out to show 3000 was
  misplaced and could not. The in-comment crossover table (re-measured 2026-07-25 on
  a 7950X after `883f57cd` widened the key to 11 tuples) shows serial ~19% ahead at
  N=2000, still ahead at N=2750, tied at N=3000, and parallel pulling away from
  N=5000. 3000 is the first size where the two are interchangeable, which is the
  right place for the gate. What is stale is the *justification prose* wrapped
  around it: "Typical Bethesda cell counts sit in 400–1500 (Prospector ~811,
  GSDocMitchell ~263, exterior radius-3 grid ~1200), so serial remains the common
  path either way; this only moves the 2000–3000 band."
- **Evidence**: `bench_draws_cmds` from the five checked-in runtime baselines in
  `.claude/audit-baselines/runtime/` (regenerated 2026-06-14 → 2026-08-06):

  | baseline cell | `entities_total` | `bench_draws_cmds` | `bench_draws_batches` | `bench_draws_gpu_calls` |
  |---|---:|---:|---:|---:|
  | `oblivion-ICMarketDistrictTheGildedCarafe` | 701 | 324 | 47 | 4 |
  | `fo3-MegatonPlayerHouse` | 3311 | 1839 | 96 | 9 |
  | `skyrim_se-WhiterunDragonsreach` | 8126 | 2342 | 9 | 2 |
  | `fnv-FreesideAtomicWrangler` | 9271 | 2553 | 89 | 25 |
  | `fo4-InstituteBioScience` | 12448 | 3440 | 753 | 42 |

  Exactly one of five sits inside the quoted 400–1500 band. Three sit in the
  1800–2600 range the comment dismisses as merely "the band this moves", and one is
  *above* the gate — `fo4-InstituteBioScience` at 3440 commands takes the parallel
  path (modulo the in-raster prefix split), which the prose says is uncommon.
- **Impact**: No runtime defect. The risk is that the next person tuning this
  constant reasons from the stale band and lowers the gate to "cover typical cells",
  landing back in the 2000–2750 range where the same comment's measured table shows
  serial winning by ~8-24%. Reported so the rationale and the constant stop
  disagreeing.
- **Related**: #934 / PERF-DC-01, #2173, `883f57cd`; reproduction harness
  `manual_bench_draw_sort_serial_vs_parallel` in
  `byroredux/src/render/draw_sort_key_tests.rs` (`--ignored`).
- **Suggested Fix**: Replace the cited cell counts with the current
  `.claude/audit-baselines/runtime/*.tsv` `bench_draws_cmds` column (or reference the
  directory rather than transcribing numbers, per the audit's own cite-don't-copy
  rule), and restate the conclusion as "one of five baseline cells currently crosses
  the gate" rather than "serial remains the common path either way".

---

#### PERF-D2-04: `sort_draw_commands` re-extracts the 11-tuple key from a ~480-byte `DrawCommand` on every comparison, unlike the decorate-sort the same module family uses for lights

- **Severity**: LOW
- **Dimension**: 2 — Draw & Instancing
- **Location**: [mod.rs](byroredux/src/render/mod.rs) — `sort_draw_commands` / `draw_sort_key`
- **Status**: NEW
- **Description**: Both the serial and parallel arms pass `draw_sort_key` to
  `sort_unstable_by_key`, which evaluates the key function on *each side of every
  comparison* — roughly `2·N·log₂N` extractions. Each extraction touches ~10 fields
  scattered across a `DrawCommand` whose field tally
  (21×`u32`, 19×`f32`, 11×`bool`, 9×`[f32;3]`, `[f32;16]`, `[u32;12]`, `[f32;5]`,
  2×`[f32;4]`, 3×`[f32;2]`, 3×`u8`, `RenderLayer`, `Option<u32>`) puts it near 480
  bytes — i.e. ~8 cache lines per key build — and materialises a 44-byte tuple.
  Meanwhile `collect_lights` in the sibling module was explicitly converted to
  decorate-sort-undecorate for exactly this reason (#2034: "precompute
  `gi_priority_score` once per light … instead of recomputing it on both sides of
  every comparator call"), on an array two orders of magnitude smaller. The larger,
  hotter sort never got the same treatment.
- **Evidence**:
  ```rust
  if raster_draws.len() >= DRAW_SORT_PARALLEL_THRESHOLD {
      raster_draws.par_sort_unstable_by_key(draw_sort_key);
  } else {
      raster_draws.sort_unstable_by_key(draw_sort_key);
  }
  ```
  `draw_sort_key` returns `(u8, u8, u8, u32, u32, u32, u32, u32, u32, u32, u32)` and
  its alpha-blend arm additionally branches on `material_kind` and `dst_blend`
  before assembling the tuple. The comment block above the call site already
  attributes a measurable cost to key width: "`883f57cd` widened the sort key from
  10 to 11 tuples (the stable surface ID), which raised per-comparison cost and
  moved the crossover UP" — direct in-repo evidence that per-comparison key
  extraction, not element movement, dominates this sort.
- **Impact**: A hypothesis, not a measured regression — stated as such. Sorting an
  array of `(key, u32 index)` pairs (≈48 B/element vs ≈480 B) and then applying the
  permutation would cut key extraction from `~2·N·log₂N` to `N` and shrink the bytes
  the sort itself shuffles by ~10×, at the cost of one permutation pass and either a
  scratch buffer or an in-place cycle walk. It would also very likely move
  `DRAW_SORT_PARALLEL_THRESHOLD` again, so the two must be re-tuned together. **No
  quantitative guard exists for this site**; do not land it on reasoning alone.
- **Related**: #2034 / PERF-D1-2026-07-16-02 (the same transform applied to
  `collect_lights`), #2172, #934, #2173, `883f57cd`.
- **Suggested Fix**: Prototype the index-decorate variant behind the existing
  `manual_bench_draw_sort_serial_vs_parallel` harness (which already sweeps
  N=400…10K) and extend that bench with a third arm. Ship only if the measured win
  survives at the N=1800–3400 range the current runtime baselines actually occupy
  (see PERF-D2-03); re-derive the parallel threshold in the same run.

---

#### PERF-D2-05: `sort_draw_commands`' in-place partition self-swaps ~480-byte `DrawCommand`s

- **Severity**: LOW — **DOWNGRADED from MEDIUM** after the auditor disproved its own claim (see Impact)
- **Dimension**: 2 — Draw & Instancing
- **Location**: [mod.rs](byroredux/src/render/mod.rs) — `sort_draw_commands` (the raster/RT-only partition loop)
- **Status**: NEW
- **Description**: The partition calls `draw_commands.swap(raster_len, index)`
  without guarding `raster_len == index`. `<[T]>::swap` lowers to `ptr::swap`, which
  performs the full three-way copy regardless of index equality — so every raster
  draw encountered before the first RT-only draw pays a round-trip memcpy of a
  ~480-byte struct against itself.
- **Evidence**:
  ```rust
  let mut raster_len = 0;
  for index in 0..draw_commands.len() {
      if draw_commands[index].in_raster {
          draw_commands.swap(raster_len, index);
          raster_len += 1;
      }
  }
  ```
- **Impact**: I tried to disprove this and it is **much smaller than it first
  looks**, which is why it is LOW rather than MEDIUM. A self-swap occurs only while
  `raster_len == index`, i.e. only across the *initial run* of consecutive
  `in_raster` commands; once one RT-only draw has been seen, every subsequent swap is
  a real one. For a mixed set the expected wasted-swap count is small. The waste
  becomes O(N) only in the fully-visible case — a cell where frustum culling flags
  nothing, or any run under `BYRO_NO_CULL=1` — where it reaches roughly
  `N × 2 × 480 B` of pointless traffic (~2.4 MB/frame at the
  `fo4-InstituteBioScience` baseline's 3440 commands). **No quantitative guard
  exists for this site.**
- **Related**: #516 (the `in_raster` / TLAS predicate split that introduced the
  partition), #2173.
- **Suggested Fix**: One line — `if raster_len != index { draw_commands.swap(raster_len, index); }`.
  Cheap enough that the bounded worst case justifies it even though the expected
  case is small.

---

## 4. Eroded Guards vs New Issues

The skill requires these two classes be called out separately. They demand different
remedies: an eroded guard means a fix that once existed no longer holds and the remedy is
**restoration**; a new issue means the remedy is **new work**.

### Eroded guards — regressions of CLOSED issues (2 findings, 4 upstream issues)

| Finding | Severity | Regresses | Why it eroded |
|---|---|---|---|
| **PERF-D3-01** | HIGH | **#2460** (and orphans #2463) | Fix commit `f3babea3` never merged to `main` |
| **PERF-D5-01** | MEDIUM | **#2461**, **#2462** | Same commit, same cause |

**Both trace to one cause: `f3babea3` is not an ancestor of `main`.** Verified via
`git merge-base --is-ancestor f3babea3 main` → false, `git branch -a --contains f3babea3` →
only `fix/2460-2461-2462-2463-as-rt-correctness`, and `blas_scratch_peak` absent from the
working tree.

This is **not code erosion** in the usual sense — nobody edited a guard away. The guard was
written, reviewed, and committed, and then the commit did not reach `main` while the issues
were closed anyway. The correct remedy is therefore **merging the branch, not re-fixing the
code**. Re-implementing would duplicate reviewed work and risk diverging from the version
the four issues were closed against.

**#2463** is a fourth casualty of the same orphan (missing `GpuTerrainTile` size/offset
lockstep test, confirmed absent at HEAD) but falls in **Dimension 4, which was not run**, so
it is not counted as a finding here. It is listed so the merge covers it.

### New issues (9 findings)

PERF-D1-01, PERF-D1-02, PERF-D2-01, PERF-D2-02, PERF-D2-03, PERF-D2-04, PERF-D2-05,
PERF-D3-02, PERF-D3-03.

**Everything except PERF-D3-01 and PERF-D5-01 is NEW.** No existing guard covers any of
them; none is a regression of previously-fixed behaviour. Notably PERF-D3-02's symbol
(`compact_pending_geometry`) appears in **no** issue in the 400-entry dedup baseline and in
no other `.rs` or `.md` file in the tree.

**Dedup performed**: `/tmp/audit/issues.json` (open + closed) grepped for `PERF-*` and for
`skinned`/`bound`/`sort_draw`/`partition`/`swap`/`sort_by`/`light`/`fog volume`/
`blend split`/`threshold`/`instanc`/`batch`/`compact_pending_geometry`. Nearest existing
items — #2367 (OPEN, FO4 ~33-34 % perf regression at flat entity count), #2351 (CLOSED, RT-1
`bench_draws_batches` skyrim_se 3→8), #2278/#2279 (CLOSED, telemetry + ROADMAP staleness) —
overlap none of the nine NEW findings. Prior reports reviewed:
`docs/audits/AUDIT_PERFORMANCE_2026-08-07.md`, `AUDIT_PERFORMANCE_2026-08-03.md`.

**Confirmed still in their documented state, deliberately NOT re-reported**: #1793 (both
documented gaps), #1797 (shared scratch serialize ceiling), #2030 (texture slot leak), #779
(`early_fragment_tests`), #2367 (unbisected bench regression), #2520 (`UpscalerMode::Taa`
redundant blit).

---

## 5. Guards Verified Intact

All confirmed by reading the cited symbol at HEAD `efc089ba`. None is re-proposed.
Consolidated and deduplicated across both dimension pairs — the blend-pipeline
pre-population guard (#1259) was verified independently by both and is listed once, folded
into the Dimension 5 row. **47 unique guards, all PASS** (21 across Dimensions 1 & 2,
13 in Dimension 3, 13 in Dimension 5).

### Dimensions 1 & 2 — CPU hot paths and draw/instancing

| Guard | Symbol / site | Status |
|---|---|---|
| #1371 — dirty drain preserves storage capacity | `PackedStorage::drain_dirty_into` ([packed.rs](crates/core/src/ecs/packed.rs)); used by `make_transform_propagation_system` ([systems.rs](crates/core/src/ecs/systems.rs), via `transform_dirty`) and by bounds propagation ([bounds.rs](byroredux/src/systems/bounds.rs), via `g_dirty`). `take_dirty` still exists but has **no** production caller — only its own unit tests. Guard test `drain_dirty_into_preserves_storage_capacity` present. | PASS |
| #1372 / #1725 — animation scratch reuse | `make_animation_system` → `AnimScratch { entities, playback, player_events, stack_events, seen_labels, channel_names, updates }`, all `clear()`+`extend()`; `player_events.clone()` (not `mem::take`) preserves capacity. | PASS (extended beyond the original two Vecs by #1725) |
| #1374 — billboard camera-motion gate | `make_billboard_system` captures `last_cam: Option<(Vec3, Vec3)>` and returns before acquiring the `Billboard` query when the pose is unchanged, so `get_mut` never re-arms `GlobalTransform`'s dirty set. | PASS |
| #1376 — debug-UI snapshot visibility gate | `App::render_one_frame` ([main.rs](byroredux/src/main.rs)): `build_debug_ui_snapshot` runs only under `self.debug_ui.as_ref().is_some_and(\|ui\| ui.visible)`; the hidden path builds only `interaction_prompt`. | PASS (see PERF-D1-02(c) for the one field that still allocates on the hidden path) |
| #1377 / #1805 — GT-presence hoist in the static-mesh loop | `collect_static_mesh_draws`: `let Some(transform) = tq.get(entity) else { continue; };` is the **first** statement in the loop body, ahead of the `vis_q` / `wb_q` sibling probes. #1805 improved it further — the hoist now binds `transform` so the later re-fetch is gone. | PASS (strengthened) |
| #1379 — `SkinSlotPool.next_slot` contracts after the idle sweep | `SkinSlotPool::sweep` sorts `free_list` and tail-pops while `top == next_slot - 1`, so `max_used_slot()` shrinks. Guard tests `sweep_contracts_next_slot_when_tail_is_freed` + `sweep_does_not_contract_when_tail_is_live` present. | PASS |
| #1794 — `bone_world` steady-state reuse | `build_render_data` does **not** `.clear()` `bone_world` (only `bone_world[0] = IDENTITY_4X4`); `build_skinned_palettes` Pass 2 is an unconditional `resize(required_slots, IDENTITY_4X4)` that identity-fills only new tail slots and truncates on shrink; the per-mesh padding tail is explicitly left untouched. Guard tests `steady_state_overwrites_the_used_slot_without_a_prior_clear`, `padding_tail_beyond_bone_count_is_left_untouched_across_frames`, `resize_grows_then_shrinks_to_the_exact_required_length` present. | PASS |
| #1803 — dead `GlobalTransform` probe removed from `emit_particles` | `emit_particles` acquires only `ParticleEmitter` and `TextureHandle`; positions come from `em.particles.positions[i]`. | PASS |
| #1802 — env vars cached, not `getenv` per frame | `OnceLock` caches in `apply_fog_overrides`, the `BYRO_PROFILE` read in `build_render_data`, and `BYRO_NO_CULL` in `collect_static_mesh_draws`. Covered by `render/env_var_cache_tests.rs`. | PASS |
| #1806 / D2-NEW-05 — wireframe folded into the depth-state sort slot | `pack_depth_state` sets bit 2 from `cmd.wireframe`; consumed by all three `draw_sort_key` arms. | PASS |
| #781 / PERF-N4 — material dedup fast path | `material_table.intern_by_hash(cmd.material_hash(), \|\| cmd.to_gpu_material())` at both `DrawCommand` producers (`static_meshes.rs`, `particles.rs`) — the 348-byte construction is skipped on the dedup hit. | PASS |
| #1795 / D2-NEW-02 — particle colour-fade quantization | `quantize_fade` (32 steps) applied to the colour LERP parameter only; four unit tests pin the ≤33-distinct-values guarantee. | PASS |
| #1136 / PERF-D3-NEW-02 — FX classification precomputed at spawn | `IsFxMesh` marker probed once per draw in `collect_static_mesh_draws`, hoisted to immediately after the visibility gate; no per-draw substring scan. | PASS |
| #1195 / PERF-DIM7-01 — pose-hash idle gate | `pose_hash` + `SkinSlotPool::try_mark_pose_dirty` / `clear_pose_dirty` in `build_skinned_palettes`. | PASS |
| #1258 — batch telemetry | `last_draw_call_stats.batch_count` set from `batches.len()`. | PASS |
| #664 — VB/IB rebind elision in the direct-draw fallback | `dispatch_direct`'s `last_bound` threading in [geometry_pass.rs](crates/renderer/src/vulkan/context/geometry_pass.rs). | PASS |
| #398 / #930 / #renderlayer — change-tracked dynamic state in the draw loop | `geometry_pass.rs` emits `cmd_set_depth_test_enable` / `_write_enable` / `_compare_op` / `cmd_set_depth_bias` / `cmd_set_cull_mode` only on change (`last_z_*`, `last_render_layer`, `last_cull_mode`, `set_cull` helper). No unconditional per-draw depth-bias or cull set. | PASS |
| #1581 / F1 + #2165 — indirect-merge key completeness | `group_state` returns `(pipeline_key, render_layer, two_sided, z_test, z_write, z_function, order_dependent_glass)`; the `order_dependent_glass` limb (#2165) is present, so a non-glass leader cannot absorb a glass batch. Unit-tested in `group_state_tests` / `needs_two_sided_blend_split_tests`. | PASS |
| #294 — single global VB/IB bind per frame | `cmd_bind_vertex_buffers` / `cmd_bind_index_buffer` issued once before the batch loop when `global_bound`. | PASS |
| #243 — renderer scratch amortization | `gpu_instances_scratch`, `previous_models_scratch`, `current_rigid_models_scratch`, `batches_scratch`, `indirect_draws_scratch`, `terrain_tile_scratch`, `blend_seen_scratch` all `mem::take`+`clear`+`reserve`d and moved back. `previous_rigid_models` / `current_rigid_models_scratch` are `FxHashMap`s swapped (not reallocated) at frame end. | PASS |
| #2504 — indirect upload failure forces direct draws | `self.indirect_upload_ok` set from `upload_indirect_draws`, read by `should_use_indirect_draws`. | PASS |

### Dimension 3 — GPU memory pressure & eviction

| Guard | Verification | Status |
|---|---|---|
| Mid-batch eviction routes through `blas_over_budget` (#1792) | `blas_over_budget` present in `predicates.rs`; **both** call sites inside `evict_unused_blas` use it (early-return gate + per-candidate loop break); `build_blas_batched` passes the live `pending_bytes` accumulator, the three no-batch callers (`build_blas`, pre-batch, per-frame `draw.rs`) pass `0`. | PASS |
| BLAS budget is dynamic, not a static 1 GB | `compute_blas_budget` = `total_device_local_bytes / 3` floored at `MIN_BLAS_BUDGET_BYTES` (256 MB, `constants.rs`). No static figure cited anywhere in this report. | PASS |
| `BATCH_EVICTION_CHECK_INTERVAL` = 64, 90 % trigger line | `should_evict_mid_batch` (`projected × 10 >= budget × 9`) gated on `idx % BATCH_EVICTION_CHECK_INTERVAL == 0`; reclaim target stays the real 100 % line. | PASS |
| LRU victim = smallest `last_used_frame` | `candidates.sort_unstable_by_key(\|&(_, frame, _)\| frame)`, idle gate `MAX_FRAMES_IN_FLIGHT + 1`. | PASS |
| #1793 pair (missing-rigid-BLAS recovery, `--grid` false-evict) | Confirmed still documented-not-fixed and still gated behind `static_blas_bytes > budget`; **not** re-investigated, per instruction. | PASS (as documented) |
| Eviction frees through deferred destroy, not inline | `evict_unused_blas` pushes to `pending_destroy_blas` with `DEFAULT_COUNTDOWN` (#1449). | PASS |
| Deferred-destroy countdown = `MAX_FRAMES_IN_FLIGHT` | `DEFAULT_COUNTDOWN = crate::vulkan::sync::MAX_FRAMES_IN_FLIGHT as u32`; `DeferredDestroyQueue::tick` checks `== 0` *before* decrement, so an item survives exactly `DEFAULT_COUNTDOWN` ticks (pinned by `default_countdown_survives_max_frames_in_flight_ticks`). Texture side uses the frame-id variant with the same depth: `should_destroy_pending` = `current - queued >= MAX_FRAMES_IN_FLIGHT`. **No path frees earlier.** | PASS |
| `pending_destroy_scratch` (#1782) | Present and ticked/drained alongside `pending_destroy_blas`; `shrink_blas_scratch_to_fit` defers rather than destroys. | PASS |
| TLAS shrink reserve floors | `shrink_tlas_to_fit` clamps with `working_set.max(WORKING_SET_FLOOR)` (= `MIN_TLAS_INSTANCE_RESERVE` = 8192) before testing `tlas_instance_should_shrink`; TLAS scratch uses the separate `TLAS_SCRATCH_SLACK_BYTES` = 256 KB path (#1226), not the 16 MB BLAS slack. | PASS |
| `MeshRegistry` soft/hard caps fire, not silent | `check_pool_growth` called for both pools on every `accumulate_global_geometry`; soft cap → one-shot `warn!` via `Once`, hard cap → `Err`. | PASS |
| BGSM/BGEM **half**-eviction, not full flush (#1430) | `bgem_cache` and `failed_paths` both drop the oldest `N/2` via their insertion-order `VecDeque` at `MAX_BGEM_CACHE_ENTRIES`/`MAX_FAILED_PATHS` = 1024. No full-flush path. `bgsm_cache` is `TemplateCache::new(256)` (LRU). The second `bgem_cache.insert` at `insert_bgem_for_test` is `#[cfg(test)]` — **disproved as a cap-bypass**. | PASS |
| `NifImportRegistry` 2048-entry LRU | Default 2048, `BYRO_NIF_CACHE_MAX=N` override, `=0` disables; eviction is `#[must_use]` so freed clip handles cannot be dropped (#863 / #2524). | PASS |
| Texture-registry slot leak (#2030) | Still the documented-not-fixed grow-only design; `live_slot_count()`/`dead_slot_count()`/`check_slot_available` (90 % one-shot warn) all present. Not re-reported. | PASS (as documented) |

### Dimension 5 — GPU pipeline & pass efficiency

| Guard | Verification | Status |
|---|---|---|
| Legacy-WRS compile-time gate (#1799) | `ENABLE_LEGACY_WRS = 0` in `shader_constants_data.rs`, emitted to the generated `shader_constants.glsl` as `0`, pinned by a value test **and** a structural test asserting the `resLight`/`resWSel` declarations sit inside the first `#if ENABLE_LEGACY_WRS` block. `strings crates/renderer/shaders/triangle.frag.spv \| grep -i "resLight\|resWSel\|NUM_RESERVOIRS"` → **no hits**: the dead-code elimination is real in the shipped artifact. | PASS |
| Shipped SPIR-V matches source at default constants | Recompiled **all 21** GLSL sources with `glslangValidator -V -I.` into a scratch dir and byte-compared against the checked-in `.spv`: **21/21 identical**, including `triangle.frag.spv` (315 416 B). No stale artifact anywhere in `crates/renderer/shaders/`. | PASS |
| RT ablation mask ships disabled | `RT_COMPILE_ABLATION_MASK = 0` (new, `8131699c`); `triangle.frag` derives `compileDisable*` as `(MASK & BIT) != 0u`, so 0 = every ray path enabled. The shipped `.spv` is the default build (previous row). | PASS |
| `inv_vp` computed once on CPU, passed via UBO | `draw.rs` computes `vp_mat.inverse()` once per frame into `inv_vp_arr`; `cluster_cull.comp` and `ssao.comp` both declare `mat4 invViewProj` as a precomputed input. `grep "inverse("` across every shader finds no `inverse(viewProj)` in any shader body. | PASS |
| No shader-side per-invocation `inverse()` regression | The three surviving `transpose(inverse(m3))` sites (`triangle.vert`, `triangle.frag` model-space-normal arm, `ray_hit.glsl` hit-normal) are each gated on `INSTANCE_FLAG_NON_UNIFORM_SCALE` (the frag site additionally on `MAT_FLAG_MODEL_SPACE_NORMALS`), with a `determinant` fallback — not on the common path. Not proposed for change: the alternative is a per-instance normal matrix, which collides with the `gpu_instance_does_not_re_expand_with_per_material_fields` guard. | PASS |
| G-buffer is 7 attachments × 2 FIF | `GBuffer` holds exactly `normal, motion, mesh_id, raw_indirect, albedo, reactive, transparency`. (`memory-budget.md`'s "8 attachments" row counts depth; not a discrepancy.) Blend pipelines declare all 8 color-blend states in lockstep, incl. the new `preserve_opaque_gbuffer` variant. | PASS |
| Volumetrics is resolution-derived, never O(meshes) | `VolumetricsPipeline::new` receives `render_extent`; both dispatches are `extent.width/height/depth ÷ WORKGROUP_*`. Deliberately downstream of the FSR preset query, as `memory-budget.md` requires. | PASS |
| Bloom is pure O(pixels) | `dispatch` walks `BLOOM_MIP_COUNT` down + up levels with `groups_x/y` from the mip extent; no draw/mesh input. | PASS |
| Per-pass dispatches are O(pixels)/O(froxels) | SSAO, TAA, SVGF temporal, SVGF à-trous (`ATROUS_ITERATIONS` = 3, odd-pinned by a `const_assert`), caustic decay + splat, water caustic — every `cmd_dispatch` derives its group count from `width/height`, none from batch or draw-command counts. `record_post_passes` records eight fixed passes per frame. | PASS |
| TLAS build → ray-query read barrier | `draw.rs` emits `ACCELERATION_STRUCTURE_BUILD_KHR / AS_WRITE → FRAGMENT\|COMPUTE / AS_READ` immediately after a successful `build_tlas`, before `write_tlas`. BUILD-vs-UPDATE short-circuit (`decide_use_update`) intact incl. the empty-frame guard. | PASS |
| Disney BSDF lobes not evaluated for fragments that skip them | Diffuse+sheen split gated on `MAT_FLAG_PBR_BSDF`; anisotropic GGX gated on `mat.anisotropic > 0.0`; env-reflection ray gated on `roughness < 0.6 && !isGlass && needsEnvironmentReflection`; `evaluatePathBsdf` early-returns on `NdotV`/`NdotL`/`NdotH`/`VdotH` ≤ 1e-5. | PASS |
| Blend pipelines pre-populated, never created mid-draw-loop (#1258 / #1259) | `draw.rs` fills `blend_seen_scratch` (persistent `HashSet`) with a two-stage swap and an `all_cached` steady-state short-circuit, and only then creates missing variants — the geometry-pass loop does a pure `get(...)`. The new `preserve_opaque_gbuffer` axis is threaded through the key on both sides. | PASS |
| Caustic splat zero-skip | The 5×5 × 3-channel deposit loop guards each `imageAtomicAdd` with `if (fv != 0u)`. | PASS |

---

## 6. Documentation & Skill-Text Drift

Four of the eleven findings are documentation defects rather than code defects. They are
collected here because they share a remedy class (edit prose to match code) and because
three of them actively manufacture false positives in *future* audit runs.

| ID | Artifact | Drift | Consequence |
|---|---|---|---|
| **PERF-D2-01** | `.claude/commands/audit-performance/SKILL.md:91` | Dimension 2 checklist still describes the pre-#2165 `z_write` form of `needs_two_sided_blend_split`. Live predicate is `is_blend && b.two_sided && b.order_dependent_glass`. | **An auditor following the skill literally reports the correct current code as a regression** — in every subsequent Dimension-2 run. Independently found as REN-D12-01. |
| **PERF-D2-02** | `needs_two_sided_blend_split` doc comment + `static_meshes.rs` glass override | Neither site cross-references the other, so the split's structural unreachability for glass is rediscovered empirically each time. | Two independent mitigations for one artifact, one unreachable; split-related checklist items are unfalsifiable on real content and must not be used to attribute batch-count movement. |
| **PERF-D2-03** | `build_render_data` rationale comment above `sort_draw_commands` | Cites a "typical Bethesda cell" band of 400–1500 that 4 of 5 checked-in baselines contradict. **The constant `= 3000` is correct**; only the prose is stale. | The next person tuning the threshold reasons from the stale band and lowers the gate into the 2000–2750 range, where the same comment's own measured table shows serial winning by ~8–24 %. |
| **PERF-D3-03** | [memory-budget.md](docs/engine/memory-budget.md) — caustics + SVGF tables, "VRAM Rough Budget" rows | Caustics ledgered at 16 B/px, actual 32 B/px (glass tripled to a 3-layer RGB array by `610cb170`, 2026-08-11). SVGF ledgered at 24 B/px, actual 40 B/px — **the 4 à-trous ping-pong images were never ledgered at all**. | The doc is the *authoritative* input to every VRAM decision and this audit is required to cite rather than re-derive it. Understates by +66 MB @1080p / +265 MB @4K (~6.6 % of the < 4 GB budget). Overlaps REN-D5-03 / REN-D14-01; the à-trous gap is unique to this report. |

Two adjacent doc-contract violations found inside code findings:

- **PERF-D3-02**: `compact_pending_geometry`'s doc comment asserts *"Pure appends (no drops)
  skip this pass"* — the exact opposite of the observed behaviour after the first drop.
- **PERF-D3-01**: `refit_skinned_blas`'s in-code justification comment ("UPDATE scratch ≤
  BUILD scratch, so the round-up headroom is already present") states a premise that
  `shrink_blas_scratch_to_fit` invalidates.

A fifth, process-level drift: **four GitHub issues (#2460–#2463) are CLOSED while their fix
lives only on an unmerged branch.** The tracker is documentation too, and it is currently
wrong about the state of `main`.

---

## 7. Prioritized Fix Order

Quick wins first — scratch reuse, preallocation and gate restoration — before anything
architectural.

### Tier 0 — Do this first (restoration, not new work)

1. **Merge or cherry-pick `f3babea3` (`fix/2460-2461-2462-2463-as-rt-correctness`) onto
   `main`**, then re-verify #2460, #2461, #2462, #2463 and recompile `triangle.frag.spv`
   (the orphaned commit already did the recompile). Closes **PERF-D3-01 (HIGH)** and
   **PERF-D5-01 (MEDIUM)** and restores the #2463 terrain-tile lockstep test in one action.
   *No new code is to be written for these.* If and only if the branch is rejected for
   unrelated reasons, the minimum standalone fix is to union `skinned_blas` into the `peak`
   walk and **re-open #2460**.
2. **Add a merged-ness check to whatever closes issues.** Four CLOSED issues with no fix on
   `main` is a process defect that will recur. This is the only item here that prevents a
   repeat rather than repairing an instance.

### Tier 1 — Quick wins (one-line to one-flag; land independently)

3. **PERF-D2-05** — one line: `if raster_len != index { draw_commands.swap(raster_len, index); }`.
   Bounded worst case, zero risk. LOW severity but the cheapest item in the report.
4. **PERF-D1-02** — swap both `sort_by` calls to `sort_unstable_by` in `collect_lights` and
   `collect_fog_volumes` (stability buys nothing on a freshly built decorate array; the
   result stays deterministic, so no GI-prefix flicker). Cache or `&'static str` the
   interaction prompt so the per-frame `format!` goes away.
5. **PERF-D3-02** — replace the `any_dead` scan with a `geometry_has_holes: bool` flag set
   in `release_mesh_ref` (when `was_scene_mesh`) and cleared at the end of
   `compact_pending_geometry`. **Do not reuse `geometry_dirty`** — appends set it too. This
   is a gate restoration: it makes the documented contract true again. MEDIUM severity for a
   ~10-line change.

### Tier 2 — Documentation, cheap and false-positive-preventing

6. **PERF-D3-03** — recompute the caustics and SVGF tables and the "VRAM Rough Budget" rows
   against `CAUSTIC_COLOR_LAYERS` and the à-trous pair; add the `log::info!` size report at
   `CausticPipeline::new`/`create_slot` and `SvgfPipeline::new_inner` that #1814 established,
   so the next resolution or layer-count change is self-reporting. **Coordinate with
   REN-D5-03 / REN-D14-01 so the doc is edited once.**
7. **PERF-D2-01** — update the Dimension 2 checklist to the live predicate and re-point the
   "regression to watch for" at non-`order_dependent_glass` batches. **Coordinate with
   REN-D12-01.** Prevents a manufactured false positive in every future run.
8. **PERF-D2-02** — no code change. Cross-reference `needs_two_sided_blend_split`'s doc
   comment to the `MATERIAL_KIND_GLASS` single-sided override, stating the glass arm is
   unreachable through `b.two_sided` and kind-11 is the only live population.
9. **PERF-D2-03** — replace the stale cell-count band with a reference to
   `.claude/audit-baselines/runtime/` (per the cite-don't-copy rule) and restate the
   conclusion as "one of five baseline cells currently crosses the gate". **Leave
   `DRAW_SORT_PARALLEL_THRESHOLD = 3000` alone — it is correctly placed.**

### Tier 3 — Measure before building (architectural; do not land on reasoning alone)

10. **PERF-D1-01** — gate the skinned-bounds block per entity via `binary_search` against the
    already-sorted-and-deduped `g_dirty` (and apply the same sort in the
    `structural_changed` branch so the gate is uniform). Highest-value CPU item in
    Dimension 1 at 677 live skins on the FNV baseline, but **no existing harness covers this
    loop** — write a targeted micro-bench before and after. Longer term, publish
    `build_skinned_palettes`' per-bone world matrices as a frame resource so bounds and the
    palette pass stop computing `to_matrix()` twice per bone.
11. **PERF-D2-04** — prototype the index-decorate sort variant behind the existing
    `manual_bench_draw_sort_serial_vs_parallel` harness (already sweeps N=400…10K) as a
    third arm. **Filed as a hypothesis, not a measured regression.** Ship only if the win
    survives at the N=1800–3400 range the runtime baselines actually occupy, and re-derive
    `DRAW_SORT_PARALLEL_THRESHOLD` in the same run — the two must move together.

### Explicitly not scheduled

Dimensions 4, 6, 7, 8 and 9 were not run. Before this report is used to plan a performance
push, run the remaining five dimensions — SSBO sizing / `#[repr(C)]` lockstep, skinning +
BLAS/TLAS build and refit, streaming and cell lifecycle, NIF parse cost, and telemetry /
render-origin cost. #2463 in particular sits in unrun Dimension 4 territory and is only
covered here incidentally, by the Tier 0 merge.

---

## Appendix — Coverage & Limits

**Read at HEAD, symbol-anchored (Dimensions 1 & 2):**
`byroredux/src/render/{mod,static_meshes,particles,skinned,lights,fog_volumes,water,camera}.rs`
(`build_render_data`, `draw_sort_key`, `pack_depth_state`, `sort_draw_commands`,
`collect_static_mesh_draws`, `emit_particles`, `quantize_fade`, `build_skinned_palettes`,
`pose_hash`, `collect_lights`, `collect_fog_volumes`, `reemit_water_planes`);
`byroredux/src/systems/{animation,bounds,billboard,particle,metrics}.rs`;
`crates/core/src/ecs/systems.rs`; `crates/core/src/ecs/packed.rs`;
`crates/core/src/ecs/resources/skin_slot_pool.rs`;
`crates/renderer/src/vulkan/context/draw.rs`;
`crates/renderer/src/vulkan/context/geometry_pass.rs`;
`byroredux/src/main.rs`; `byroredux/src/boot.rs` (scheduler wiring, to confirm every audited
system actually runs per frame); `byroredux/src/interaction.rs`.

**Read at HEAD (Dimensions 3 & 5):** the acceleration module
(`memory.rs`, `blas_skinned.rs`, `blas_static.rs`, `predicates.rs`, `constants.rs`, `tlas.rs`),
`crates/renderer/src/mesh.rs`, `caustic.rs`, `water_caustic.rs`, `svgf.rs`, `gbuffer.rs`,
`volumetrics`, `bloom`, `ssao.comp`, `taa.comp`, all 21 shader sources plus their `.spv`,
`shader_constants_data.rs`, the texture registry, `NifImportRegistry`, and the BGSM/BGEM caches.

**Scale inputs used (read, not measured):** all five
`.claude/audit-baselines/runtime/*.tsv` — `entities_total`, `skin_pool_live`,
`bench_draws_cmds`, `bench_draws_batches`, `bench_draws_gpu_calls`; and
`docs/engine/memory-budget.md` for every SSBO size, LRU threshold, reserve floor,
deferred-destroy depth and VRAM figure. PERF-D3-03 is the one place this report contradicts
that doc, and it does so with the source constants quoted. ROADMAP.md's Bench-of-record block
was read only to confirm its `R6a-stale-*` non-gating status; no number from it is asserted.

**Could NOT be verified without a running engine (Vulkan device + game data):**
- Any absolute or delta frame-time / FPS / draw-call number. No bench was run.
- The actual per-frame cost of PERF-D1-01. `skin_pool_live 677` is checked in, but
  bones-per-skin and the resulting millisecond cost are not measured anywhere in-repo and
  were **not** estimated to a number.
- Whether `fo4-InstituteBioScience`'s 3440 commands leave a *raster prefix* above
  `DRAW_SORT_PARALLEL_THRESHOLD` after the `in_raster` partition. `bench_draws_cmds` is the
  total; the raster/RT-only split is not a baseline scalar. PERF-D2-03 is worded to survive
  either answer.
- Whether the PERF-D2-04 decorate-sort variant actually wins — filed as a hypothesis with a
  named existing harness.
- Live confirmation that `blended && two_sided == 0` beyond the five baselines. PERF-D2-02
  argues the dormancy from code structure precisely because measurement was unavailable.
- GPU-side effects of any Dimension-2 finding (pipeline-bind counts, occupancy).
- Real BLAS eviction behaviour under streaming. The dev card's dynamic budget
  (`device_local / 3` ≈ 4 GB on 12 GB) makes eviction essentially unreachable in practice, so
  the eviction *policy* was audited by reading the predicates and their unit tests rather
  than observed. Likewise scratch high-water across a real cell-unload sequence, BGSM/BGEM
  cache hit-rate on real archives, `MeshRegistry` pool growth against real cell content, the
  per-frame GPU cost of the tripled caustic atomic traffic after `610cb170`, and PERF-D3-02's
  actual per-boundary copy cost (no dhat coverage exists for render/ECS hot paths — the
  profiler is a process singleton, so this remains smoke-test territory).
- **PERF-D3-01's exploit window** (skinned `build_scratch_size` > every surviving static
  BLAS's, with the 2× + 16 MB shrink condition met) is argued from the code and the
  constants, not observed on hardware. The *defect* is not conditional on that window,
  though: the fix is simply absent from `main`, and #2460 already triaged it.

**dhat coverage note**: quantitative allocation bounds exist for the NIF parse path only
(`crates/nif/tests/heap_allocation_bounds.rs`,
`crates/nif/tests/heap_allocation_bounds_geometry.rs`). Every allocation / hot-path finding
in this report is on a per-frame render or ECS path, where **no quantitative guard exists for
the site** — stated explicitly per finding.
