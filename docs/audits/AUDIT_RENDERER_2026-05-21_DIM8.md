# Renderer Audit — Dimension 8: Acceleration Structures (RT BLAS/TLAS)

**Date:** 2026-05-21 · **HEAD:** 7eb137b5 · **Auditor:** renderer-specialist agent

## Executive Summary

Single-dimension audit of `crates/renderer/src/vulkan/acceleration/*` plus the
TLAS build/refit consumer sites in `context/draw.rs`. The acceleration module
is in good shape post-Session-35 split — 20 of 25 checklist items confirm cleanly
against the current code with explicit `debug_assert!` pins and well-documented
invariants. Four new findings, none critical, one medium-severity (FINDING-D8-22)
worth filing.

**Headline numbers**
- 20 invariants confirmed correct (one citation each)
- **4 NEW findings**: 0 critical, 0 high, 1 medium, 3 low
- 0 stale findings (no audit-text claims contradicted by current code)
- 0 duplicates of existing OPEN issues

The RT pipeline's load-bearing contracts — 24-bit `instance_custom_index`
truncation guard (#957), shared `instance_map` between TLAS + SSBO (#419),
BUILD↔UPDATE flag lockstep (#958), empty-instance copy skip (#317), scratch
alignment debug assert (#659), cross-submission scratch barrier (#983/#1140),
TLAS `last_blas_addresses` length pin + ping-pong (#660/#914),
`built_primitive_count` UPDATE guard (#1083), `MAX_INSTANCES` cap, and
`SKINNED_BLAS_REFIT_THRESHOLD` rebuild — all hold.

Suggested next step: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-21_DIM8.md`
will surface FINDING-D8-22 as a new issue and skip the three optional-polish items.

---

# Dimension 8 — Acceleration Structures (BLAS / TLAS)

Scope: `crates/renderer/src/vulkan/acceleration/{tlas,blas_static,blas_skinned,predicates,memory,constants,mod,types}.rs`
plus the consumer call sites in `crates/renderer/src/vulkan/context/draw.rs` and `byroredux/src/cell_loader/unload.rs`.

## Confirmed correct

### CORRECT-D8-01 — `instance_custom_index` 24-bit truncation guard (#957)
`tlas.rs:196-204` debug-asserts `ssbo_idx < (1u32 << 24)` at the truncation site itself, even though the
upstream `MAX_INSTANCES = 0x40000` (262 144) caps it at ~64× below the 2^24 ceiling. The pin survives a
future `MAX_INSTANCES` bump without re-routing through the original audit.

### CORRECT-D8-02 — Shared `instance_map` between TLAS + SSBO (#419)
`predicates.rs::build_instance_map` is the single source of truth; `tlas.rs:147` consumes it via
`instance_map.get(i).copied().flatten()`, so the TLAS `instance_custom_index` cannot drift from the
SSBO compacted position even when filters reject. Pre-#419 they were independent.

### CORRECT-D8-03 — BUILD↔UPDATE flag lockstep (#958)
`UPDATABLE_AS_FLAGS` / `STATIC_BLAS_FLAGS` / `SKINNED_BLAS_FLAGS` constants in `constants.rs` are shared
between every BUILD-mode and UPDATE-mode call site for each AS family, so
VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667 cannot drift by accident. Additional runtime pin
via `validate_refit_flags` in `predicates.rs` for skinned BLAS.

### CORRECT-D8-04 — Empty-frame BUILD short-circuit (#657)
`decide_use_update` in `predicates.rs:171` explicitly returns `(false, false)` when
`current_addresses.is_empty()`. Avoids the trap where empty frame N → empty frame N+1 would otherwise
pick UPDATE-with-primitiveCount=0 against a TLAS whose source BUILD had a non-empty primitive list.

### CORRECT-D8-05 — `built_primitive_count` UPDATE guard (#1083)
`tlas.rs:552-554` forces a BUILD when `instance_count > tlas.built_primitive_count`, even when the
zip-compare otherwise allows UPDATE. Pinned by `debug_assert_eq!` at line 758 on the UPDATE branch.

### CORRECT-D8-06 — TLAS resize destroys old slot under fence-wait invariant
`tlas.rs:270-301` invariant comment (line 276-289) explicitly documents that `draw_frame`'s double-fence
wait covers this site. Resources destroyed in correct order: accel → buffer → instance_buffer →
instance_buffer_device.

### CORRECT-D8-07 — Empty-instance copy skip (#317)
`tlas.rs:644-692` gates the host→transfer + transfer→AS barriers and the copy itself on
`copy_size > 0`, sidestepping VUID-VkBufferCopy-size-01988 / VUID-VkBufferMemoryBarrier-size-01188 on
empty-TLAS frames. Empty TLAS build with `primitiveCount = 0` is legal per spec.

### CORRECT-D8-08 — Scratch alignment debug assert (#659)
`is_scratch_aligned` checks the device address against
`minAccelerationStructureScratchOffsetAlignment` at each cmd-build site (`tlas.rs:715-719` plus the
sibling BLAS sites). `align <= 1` no-op path covers RT-disabled GPUs cleanly.

### CORRECT-D8-09 — Cross-submission scratch barrier (#983, #1140)
The `ScratchUser::CrossSubmissionBuildWithFenceWait` enum + `requires_scratch_serialize_barrier_before`
test pin make explicit that fence-wait does NOT excuse a device-side `AS_WRITE → AS_WRITE` barrier on
shared scratch reuse. Production sites unconditionally emit the barrier.

### CORRECT-D8-10 — TLAS `last_blas_addresses` length pin (#914)
`tlas.rs:579-584` debug-asserts `last_blas_addresses.len() == instance_count` right after the swap, so
next-frame UPDATE's `primitiveCount` cannot desync.

### CORRECT-D8-11 — TLAS `last_blas_addresses` ping-pong (#660)
`tlas.rs:563-585` swaps the Vec via `mem::swap` and recovers it into `self.tlas_addresses_scratch` for
the next frame. No per-frame heap allocation for what used to be 64 KB at the 8K-instance ceiling.

### CORRECT-D8-12 — TLAS post-build memory barrier
`draw.rs:1176-1183` issues `ACCELERATION_STRUCTURE_BUILD_KHR (AS_WRITE) → FRAGMENT_SHADER|COMPUTE_SHADER
(AS_READ)` immediately after `build_tlas`. Covers both the main render pass ray queries and the
caustic / volumetric compute consumers (#415).

### CORRECT-D8-13 — `MAX_INSTANCES` cap honoured by `build_instance_map`
`predicates.rs::build_instance_map` enforces `next < max_kept` and emits `None` past the cap, so
over-cap TLAS instances cannot produce out-of-bounds SSBO reads.

### CORRECT-D8-14 — Skinned BLAS refit threshold rebuild (#679)
`SKINNED_BLAS_REFIT_THRESHOLD = 600` (≈10 s @ 60 FPS) drops + rebuilds the per-entity BLAS to reset BVH
bounds. Pure predicate `should_rebuild_skinned_blas_after` unit-testable.

### CORRECT-D8-15 — TLAS instance-buffer cleanup in `Drop` / `destroy`
`mod.rs:280-288` destroys all three TLAS sub-buffers per slot in reverse-order: accel → buffer →
instance_buffer → instance_buffer_device. Scratch buffers and skinned BLAS HashMap also drained.

### CORRECT-D8-16 — Skinned BLAS HashMap drained on shutdown (#1138)
`mod.rs:311-315` drains `skinned_blas` unconditionally in `destroy`, independent of whether the App
pre-drained via `pending_destroy_blas`. Defends against future shutdown-path refactors.

### CORRECT-D8-17 — `rt_flag` gated on `tlas_written[frame]`
`draw.rs:409-414` writes `rt_flag = 0.0` into the camera UBO until `write_tlas` flips
`tlas_written[frame] = true`. Shaders' ray-query branches stay dormant on frame 0 (see FINDING-D8-21
below for the warmup-frame race).

### CORRECT-D8-18 — TLAS instance buffer device-address flag
`tlas.rs:350-353` — device-local buffer correctly carries
`ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR | SHADER_DEVICE_ADDRESS | TRANSFER_DST`. Host-visible
staging is `TRANSFER_SRC` only, which is correct (never device-addressed).

### CORRECT-D8-19 — TLAS scratch buffer device-address flag
`tlas.rs:459-465` — `STORAGE_BUFFER | SHADER_DEVICE_ADDRESS`. Required for the
`scratch_data.device_address` queried at line 711 before `cmd_build_acceleration_structures`.

### CORRECT-D8-20 — Host→AS barrier chain on TLAS instance write
`tlas.rs:646-691` correctly chains:
  - `HOST_WRITE @ HOST` → `TRANSFER_READ @ TRANSFER` (visibility of `write_mapped`'s flushed range)
  - `cmd_copy_buffer` from staging → device-local
  - `TRANSFER_WRITE @ TRANSFER` → `ACCELERATION_STRUCTURE_READ_KHR @ ACCELERATION_STRUCTURE_BUILD_KHR`
    on the device-local buffer

The `write_mapped` flush comment (line 617-624) is correct: `vkFlushMappedMemoryRanges` makes host
writes visible to the device; the barriers cover the visibility hops between stages.

## Findings

### FINDING-D8-21 — Frame-0 `rt_flag` upload races TLAS write **[low]**

**Site:** `crates/renderer/src/vulkan/context/draw.rs:409-414` (rt_flag computation),
`draw.rs:1158-1187` (build_tlas + write_tlas), `scene_buffer/buffers.rs:766`
(`tlas_written: vec![false; MAX_FRAMES_IN_FLIGHT]`).

**Issue:** The camera UBO is uploaded with `rt_flag` *near the top of the frame* (line 506, packed into
`flags[0]`), but `tlas_written[frame]` only flips to `true` at the end of `build_tlas` → `write_tlas`
much later in the same frame (line 1184-1187). So on the FIRST frame ever, `rt_flag = 0.0` is uploaded
even when TLAS gets built that frame. The first frame whose camera UBO sees `rt_flag = 1.0` is the
*next* time the SAME frame-in-flight slot recurs — i.e. frame 2 (with `MAX_FRAMES_IN_FLIGHT=2`).

**Impact:** Frame 0 (and frame 1, since `tlas_written` is per-slot and frame 1 is the other slot's
first hit) renders without any RT lighting contributions — flat shadows, no GI, no reflections, no
caustics. From frame 2 onward correct. Cosmetic only — the "RT off" state is the documented degraded
mode for hardware without ray-query support, and TAA accumulates across the transition.

**Why it's not a bug per se:** the gate is intentional — uploading `rt_flag = 1.0` before
`write_tlas` would have the shaders try to ray-query a stale or null TLAS descriptor on frame 0. The
existing order is correct; the cost is a 1-2 frame visual "warmup".

**Suggested follow-up:** Move the `tlas_written[frame] = true` flip to the END of `build_tlas` and the
`rt_flag` read to AFTER `build_tlas` (or hoist the UBO upload past TLAS build). Today the UBO is part
of `upload_lights` which runs before the render pass begins and before TLAS build by design. A clean
fix would split the camera UBO upload from the lights upload so `flags[0]` is patched in-place after
`write_tlas` lands. **Low priority** — file as enhancement, not blocker.

---

### FINDING-D8-22 — TLAS-scratch shrink uses BLAS-scale slack margin **[medium]**

**Site:** `crates/renderer/src/vulkan/acceleration/memory.rs:248-251` —
`shrink_tlas_scratch_to_fit` calls `scratch_should_shrink(current, peak)`.

**Issue:** `scratch_should_shrink` in `predicates.rs:262-265` hard-codes
`BLAS_REBUILD_SLACK_BYTES = 16 MB` (constants.rs:16). The constant's doc-comment explicitly says
"BLAS scratch lives at 80-200 MB scale, slack is ~10% of that" — but it's reused unmodified on the
TLAS path.

TLAS scratch sizes are typically <1 MB even for thousands of instances (TLAS scratch grows linearly
with instance count at ~tens of bytes per instance, not per-vertex). So on the TLAS path:
- Excess condition: `current - peak > 16 MB` is almost NEVER true at TLAS scale.
- Result: `shrink_tlas_scratch_to_fit` effectively never fires — the entire mechanism (#682 /
  MEM-2-7) is dead code for typical workloads.

**Verification:** the parallel `tlas_instance_should_shrink` in `predicates.rs:280-286` already gets
this right — it uses `TLAS_REBUILD_SLACK_BYTES = 1 MB` for the TLAS instance buffer. The scratch path
was simply not migrated to a TLAS-scale slack when MEM-2-7 landed.

**Suggested fix:** Introduce `TLAS_SCRATCH_SLACK_BYTES` (likely 256 KB given typical TLAS scratch
scale) and add a `tlas_scratch_should_shrink` predicate that mirrors `tlas_instance_should_shrink`'s
shape. Use it in `memory.rs:249` instead of `scratch_should_shrink`. Update the constants.rs comment
on `BLAS_REBUILD_SLACK_BYTES` to clarify "BLAS-only scale".

**Severity:** medium — wastes a few MB of DEVICE_LOCAL VRAM per FIF slot persistently after an
exterior peak, but does not affect correctness or stability.

---

### FINDING-D8-23 — `shrink_tlas_to_fit` thrash risk on cell boundaries **[low]**

**Site:** `crates/renderer/src/vulkan/acceleration/memory.rs:136-180`
(`shrink_tlas_to_fit`) + `crates/renderer/src/vulkan/context/draw.rs:2746-2753`
(per-frame caller).

**Issue:** `shrink_tlas_to_fit` is called every frame on the just-freed slot, gated by
`tlas_instance_should_shrink`'s `2× + 1 MB` hysteresis. Consider an oscillation:
  - Cell A: 30 000 instances → TLAS slot capacity = 60 000 (padded), 3.8 MB
  - Player crosses a boundary into Cell B: 200 instances. `working_set = 200`,
    `working_floor = max(200, 8192) = 8192`, `working_set_bytes = 512 KB`.
    `current_capacity_bytes = 3.8 MB`. Ratio = 7.5× > 2 ✓; excess = 3.3 MB > 1 MB ✓ → SHRINK.
  - TLAS slot is destroyed.
  - Player crosses back into Cell A: next `build_tlas` allocates fresh TLAS at padded size 60 000.
    This is a ~60 µs hit per FIF slot (full TLAS create + size-query + scratch realloc).

Two FIF slots, so each oscillation costs ~120 µs across two frames. The hysteresis correctly bounds
this — 1 MB slack means small variance doesn't churn — but it does fire on real cell-load boundaries.

**Why this is acceptable:** the alternative is pinning multi-MB BAR/DEVICE_LOCAL allocations
persistently after a single big exterior cell, which is exactly the failure mode MEM-2-3 fixed.
The ~120 µs amortized over a multi-second cell transition is well below the frame budget. The
`WORKING_SET_FLOOR = 8192 = MIN_TLAS_INSTANCE_RESERVE` matching pair (constants.rs:33-40) is the
load-bearing piece that prevents tiny-working-set churn.

**Confirmed correct as written**, but worth noting that adding a frame-count debounce on the shrink
trigger (e.g. "only fire after N consecutive frames below threshold") would cleanly fix the
"player oscillates at cell boundary" pathological case if it ever shows up in bench. Not actioned
today; record as an enhancement candidate.

---

### FINDING-D8-24 — Missing-BLAS warn counter is non-uniform between skinned + rigid **[low]**

**Site:** `crates/renderer/src/vulkan/acceleration/tlas.rs:113-127`.

**Issue:** When a skinned draw command references an entity whose BLAS hasn't been built yet, the
code increments `missing_blas`, samples the first 5 into `missing_samples`, and the rate-limited
`log::warn!` at line 259 reports it. **However**, the comment at line 117-120 says "raster's
inline-skinning path still renders it correctly" — i.e. the entity is visible but invisible to RT.
The warn message says "no RT shadows for those meshes" which is correct, but doesn't distinguish
"skinned entity not yet built" (transient — resolves once `build_skinned_blas_batched_on_cmd`
runs) from "rigid mesh evicted from BLAS cache" (potentially persistent).

**Why this is low-severity:** the sample text DOES include "skinned entity `EntityId(...)` (no
BLAS)" so a human triaging the log can tell them apart. The diagnostic is good enough; just
non-uniform.

**Suggested polish:** split the missing_blas counter into `missing_skinned_blas` and
`missing_rigid_blas` (already differentiated in the sample format string, just not in the count).
Trivial. Not actioned today.

---

### FINDING-D8-25 — Slack margin sanity consolidation **[duplicate of FINDING-D8-22]**

The constants themselves are correctly sized for their intended scales (16 MB on BLAS, 1 MB on TLAS
instance buffer); the bug is the *reuse* of the BLAS constant on the TLAS scratch path.

For reference:
- `BLAS_REBUILD_SLACK_BYTES = 16 MB` ↔ BLAS scratch scale 80-200 MB ✓ correct ratio (~10%)
- `TLAS_REBUILD_SLACK_BYTES = 1 MB` ↔ TLAS instance buffer scale 0-2 MB ✓ correct ratio (~50%, but
  the 2× ratio gate already absorbs most variance, so the slack mainly stops sub-MB churn — fine)
- TLAS scratch buffer scale: ~tens of KB to <1 MB at 8K instances → the 16 MB slack effectively
  disables the shrink permanently. **Bug.** See FINDING-D8-22.

## Recommendations

| ID | Severity | Action |
|----|----------|--------|
| FINDING-D8-22 | medium | File issue: introduce `TLAS_SCRATCH_SLACK_BYTES` + `tlas_scratch_should_shrink`; swap call in `memory.rs:249`. ~30 LOC + unit test. |
| FINDING-D8-21 | low | Optional polish: re-order camera UBO upload to after `write_tlas`, or split `flags[0]` into a post-TLAS patch. |
| FINDING-D8-23 | low | Optional polish: add frame-count debounce on `shrink_tlas_to_fit` trigger to bound cell-boundary thrash. |
| FINDING-D8-24 | low | Optional polish: split `missing_blas` counter into skinned vs rigid for cleaner telemetry. |

No high-severity findings.
