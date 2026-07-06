# Performance Audit — 2026-07-05 (RT-focused, Dimensions 1 + 3 only)

**Scope**: `/audit-performance` restricted to **Dimension 1 (CPU Per-Frame
Allocations & Hot Paths)** and **Dimension 3 (GPU Memory Pressure & Eviction
Thrash)**. RT-focused sweep: per-frame allocations on the RT/denoiser hot path,
acceleration-structure budget/eviction, and GPU memory pressure from AS +
G-buffer + SVGF history targets.

**Hardware target**: RTX 4070 Ti (12 GB) + Ryzen 7950X (16c/32t). RT VRAM
minimum 6 GB.

**Depth**: deep (hot paths traced, guards re-verified against live code).

**Bench-of-record**: ROADMAP R6a-stale-14 (`1c26bc25`, 2026-06-03) is **501
commits stale** and explicitly non-gating. No absolute FPS claim is made here;
this sweep is a structural verification against the Session-46/47/49 landed
invariants, per the Regression-Guard Posture in the skill.

---

## Executive Summary

**0 NEW findings.** Both restricted dimensions are in excellent shape and are
being actively maintained — the two recurring LOW findings that appeared in the
2026-07-01 / 07-02 / 07-03 reports are **now fixed in current code**, and every
Session-46/47/49 regression guard for these two dimensions is intact.

| Severity | New | Notes |
|----------|-----|-------|
| CRITICAL | 0 | — |
| HIGH     | 0 | — |
| MEDIUM   | 0 | — |
| LOW      | 0 | Both prior recurring LOWs (PERF-D1-NEW-01, PERF-D1-NEW-02) now RESOLVED |

**Resolved since last sweep (verified in live code, no longer reportable):**

- **PERF-D1-NEW-01** (per-frame handle-count telemetry walk) — the
  `meshes_in_use` / `textures_in_use` dedup walk in `about_to_wait` is now
  **throttled to the 1 Hz `crosses_one_second_boundary` cadence** (`#1801`,
  `byroredux/src/main.rs:2358-2389`). Was reported unconditional-per-frame in
  07-01 and 07-02.
- **PERF-D1-NEW-02** (per-frame `env::var` in the render hot path) — both cited
  sites are now `OnceLock`-cached: `BYRO_PROFILE` at `render/mod.rs:377`
  (`get_or_init`) and `BYRO_NO_CULL` at `static_meshes.rs:144` (`get_or_init`),
  matching the `apply_fog_overrides` convention. Was reported as live per-frame
  `var_os` in 07-01/07-02/07-03.

**Carryover (documented-not-fixed, unreachable on the 12 GB dev card — cited,
not re-discovered):** `#1793` (evicted static BLAS has no rebuild path + burst
aging false-eviction), `#1797` (shared skinned-BLAS `blas_scratch_buffer`
serializes N dirty entities per frame).

**Existing OPEN doc gap (Dim 3, already filed):** `#1872` —
`memory-budget.md` tracks no screen-sized RT-denoiser image resources.

---

## Dimension 1 — CPU Per-Frame Allocations & Hot Paths

### Guard Verification Matrix

| Guard (issue) | Status | Evidence |
|---|---|---|
| `PackedStorage::drain_dirty_into` preserves dirty-set capacity (#1371) | **INTACT** | `crates/core/src/ecs/packed.rs:73`; consumed in `bounds.rs:62` (`gq.storage_mut().drain_dirty_into(&mut g_dirty)`) |
| `make_animation_system` persistent scratch (clear+extend, #1372) | **INTACT** | `systems/animation.rs:435-441` (`entities_scratch.clear(); .extend(...)`, `playback_scratch.clear()`) |
| `make_billboard_system` `last_cam` early-skip (#1374) | **INTACT** | `systems/billboard.rs:23,56-59` (skips the `get_mut` loop when camera unmoved) |
| World-bound propagation persistent scratch (`make_*_system` closure) | **INTACT** | `systems/bounds.rs:33-50` — all 5 Vecs captured before the `move` closure; `drain_dirty_into` drainer |
| `bone_world` steady-state reuse, no per-frame `.clear()` (#1794) | **INTACT** | `render/mod.rs:344-365` (deliberately not cleared; `resize` truncates/tail-fills only); `render/skinned.rs:137-148` |
| `emit_particles` dead-probe removed (#1803) | **INTACT** | `render/particles.rs` acquires only `ParticleEmitter`, no stray `GlobalTransform` query |
| Handle-count telemetry walk throttled to 1 Hz (#1801) | **INTACT (was the finding)** | `main.rs:2358-2389`, gated on `should_refresh_handle_counts` |
| Render-path env lookups `OnceLock`-cached (was PERF-D1-NEW-02) | **INTACT (was the finding)** | `render/mod.rs:377`, `static_meshes.rs:144` |

### RT / denoiser hot-path allocation trace (all clean)

- **TLAS build** (`acceleration/tlas.rs`): the per-frame instance array and the
  address-diff array are persistent scratch, taken via `std::mem::take` and
  swapped back — `tlas_instances_scratch` (`:74`, restored `:853`),
  `tlas_missing_samples_scratch` (`:110`, restored `:863`),
  `tlas_addresses_scratch` (`:611`, ping-pong swap `:614+`, #660). No per-frame
  heap allocation. `last_blas_addresses: Vec::with_capacity(padded_count)`
  (`:539`) is allocated **only at TLAS creation**, not per frame.
- **`draw_frame`** (`context/draw.rs`): the only two `collect()` sites are both
  gated off steady state — `pending_slots` (`:2772`) runs only when
  `pending_capped > 0` (first-sight `bind_inverses` upload, cell-load-time), and
  the missing-blend-pipeline `collect()` (`:2413`) runs only when `!all_cached`
  (pipeline warmup). Steady-state `draw_frame` does zero heap allocation on
  these paths.
- **SVGF** (`vulkan/svgf.rs`): every `Vec::new()` / `vec!` / `create_image`
  lives in `new` / `new_inner` (`:365-381`) or `recreate_on_resize` (`:1357`) or
  the one-time `initialize_layouts` setup path (`:1043`). The per-frame
  `dispatch` (`:1153`) allocates nothing.
- **`build_skinned_blas_batched_on_cmd`** (`acceleration/blas_skinned.rs:60`)
  allocates two `Vec::with_capacity(entities.len())` (`:87-88`), but
  early-returns empty (`:68`) when there are no entities; `entities` is the
  **first-sight (not-yet-built) skinned set**, empty after warmup — this is a
  build/cell-load path, not the per-frame refit path. Not a steady-state churn
  site.

**Conclusion**: no new Dim-1 findings. Every antipattern the prior sweeps
tracked has either an intact guard or has been fixed since 07-03.

> **Coverage caveat (unchanged, per skill Regression-Guard Posture):** these
> render/ECS hot-path sites have **no quantitative dhat guard** — `dhat` is a
> process singleton and the live loop is smoke-test territory. The clean result
> above is by code inspection, not an alloc-bound test. Related tracked gap:
> `#1763` / TD9-001 (NIF dhat regression tests dormant in CI).

---

## Dimension 3 — GPU Memory Pressure & Eviction Thrash

### Guard Verification Matrix

| Guard (issue) | Status | Evidence |
|---|---|---|
| BLAS budget is **dynamic** `device_local/3` floored at `MIN_BLAS_BUDGET_BYTES` | **INTACT** | `acceleration/predicates.rs:573-579` (`compute_blas_budget`) — no static "1 GB" figure in code |
| Mid-batch eviction routes through `blas_over_budget` folding `pending_bytes` (#1792) | **INTACT (prior finding, fixed)** | Trigger `should_evict_mid_batch(static, pending, budget)` at `blas_static.rs:605-621`; callee invoked with the real running `pending_bytes` at `:639`; pre-batch/single-shot paths correctly pass `0` (`:194`, `:597`) |
| Mid-batch check interval `BATCH_EVICTION_CHECK_INTERVAL` = 64 builds | **INTACT** | `blas_static.rs:604` (`idx % BATCH_EVICTION_CHECK_INTERVAL == 0`) |
| BGSM/BGEM cache **half-eviction** (drop oldest N/2 via `VecDeque`, #1430) | **INTACT** | `asset_provider/material.rs:148-157` (`bgem_cache_order: VecDeque`, "oldest N/2 evicted") |
| Deferred-destroy countdown = `MAX_FRAMES_IN_FLIGHT` (2) | **INTACT** | `renderer/src/deferred_destroy.rs:34` (`DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT as u32`) |
| TLAS address-diff scratch reuse, no per-frame 64 KB churn (#660) | **INTACT** | `acceleration/tlas.rs:611-614` (`mem::take` + `mem::swap` ping-pong) |

### Carryover items (documented-not-fixed — verified still as-documented, NOT re-reported as new)

- **`#1793` (was PERF-D3-NEW-02)** — evicted static BLAS have **no rebuild
  path** (`build_blas_batched` is invoked only from cell/scene-load sites, never
  per-frame), so an evicted-but-still-drawn mesh permanently vanishes from
  shadows/reflections/GI until its cell reloads (`tlas.rs` `missing_rigid_blas`
  counter, #1228). A synchronous multi-cell `--grid` burst can also age a
  just-built, never-yet-drawn entry into LRU candidacy. Both are **unreachable
  on the 12 GB dev card** (budget is `device_local/3` ≈ 4 GB, floored 256 MB;
  no test cell approaches it). Needs a real low-VRAM `--grid` repro before any
  speculative fix — that repro still does not exist.
- **`#1797`** — every skinned BLAS build/refit in a frame shares one
  `blas_scratch_buffer` (`acceleration/blas_skinned.rs:38-49`), so the required
  `AS_WRITE → AS_WRITE` serialize barrier fully serializes N dirty skinned
  entities per frame with no build overlap. A real moving-crowd ceiling,
  deliberately left unfixed pending measurement via `skin.coverage` /
  `gpu_skin_blas_refit_ms`. This is primarily a **Dimension-6** concern (out of
  this sweep's 1+3 scope); noted here only because it touches AS scratch memory.

### Existing OPEN issue in this dimension

- **`#1872`** (OPEN, `documentation`) — `docs/engine/memory-budget.md` tracks no
  screen-sized RT-denoiser image resources (SVGF indirect/moments history +
  à-trous ping-pong, SSAO, TAA history, bloom pyramid, volumetrics froxel grid,
  water/caustic accumulators). These are the largest single class of resident RT
  VRAM after the AS + scene SSBOs, and all scale with swapchain extent × MAX
  frames-in-flight. **Confirmed still open and accurate.** The resources
  themselves are correctly bounded — each is allocated once per swapchain
  resize (SVGF `recreate_on_resize` at `svgf.rs:1357`, G-buffer
  `recreate_on_resize`) and not per-frame — so this is a documentation/tracking
  gap, not a leak. No new finding beyond the existing issue.

**Conclusion**: no new Dim-3 findings. The one previously-"eroded" guard
(mid-batch eviction effect, PERF-D3-NEW-01) is now structurally fixed by #1792
and unit-pinned. All other guards intact; remaining items are known carryover.

---

## Hot Path Analysis — Memory Posture Snapshot

RT-relevant resident GPU memory this sweep touched (all correctly bounded, no
per-frame growth):

| Resource | Sizing | Allocated | Per-frame growth? |
|---|---|---|---|
| Static BLAS pool | dynamic budget `device_local/3` ≥ 256 MB, LRU-evicted at 90% | cell/scene load | no (eviction pre- + mid-batch) |
| TLAS + instance buffer | `padded_count`, refit-vs-rebuild gated (#247/#1083) | on grow/creation | no (scratch ping-pong #660) |
| SVGF history + à-trous ping-pong | swapchain extent × `MAX_FRAMES_IN_FLIGHT` | per swapchain resize | no |
| G-buffer (6 attachments × 2 FIF) | swapchain extent | per swapchain resize | no |
| BGSM/BGEM material cache | half-eviction on overflow (#1430) | on material load | no |
| `bone_world` scratch (host) | `(max_used_slot+1) × MAX_BONES_PER_MESH` | grows to high-water, truncates on shrink (#1794/#1379) | no (no per-frame clear) |

Numbers not transcribed — see `docs/engine/memory-budget.md` (authoritative;
its untracked-denoiser-image gap is `#1872`).

---

## Prioritized Fix Order

Nothing actionable emerged from this restricted sweep. Both dimensions are
clean. The only forward work items are pre-existing and out of the "quick win"
category:

1. **`#1872`** (LOW, doc) — add the screen-sized denoiser image resources to
   `memory-budget.md` so Dim-3 VRAM accounting is complete. Pure documentation.
2. **`#1793` / `#1797`** — leave as documented-not-fixed until a real low-VRAM
   `--grid` repro (for #1793) or a `skin.coverage` moving-crowd measurement
   (for #1797) exists. Do **not** ship a speculative fix; both are unreachable /
   unmeasured on the current dev hardware.

---

## Deduplication Notes

- Dedup baseline: `gh issue list … > /tmp/audit/performance/issues.json`
  (retrieved; 200-issue window, all currently-open issues are LOW).
- Prior performance reports scanned: `AUDIT_PERFORMANCE_2026-07-01.md`,
  `…07-02.md`, `…07-03.md` (the three most recent, each covering Dims 1 + 3).
- No finding in this report is new; the two prior recurring LOWs are confirmed
  fixed in live code, and every remaining item maps to an existing issue
  (`#1793`, `#1797`, `#1872`, `#1763`).
