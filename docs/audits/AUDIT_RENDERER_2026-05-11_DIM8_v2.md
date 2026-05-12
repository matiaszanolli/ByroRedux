# Renderer Audit — 2026-05-11 (Dimension 8 re-sweep)

**Scope**: Dimension 8 — Acceleration Structures (BLAS / TLAS / scratch / refit / compaction / LRU eviction).
**Depth**: deep.
**Method**: orchestrator + single dimension agent + prior-audit verification.
**Prior audit**: `AUDIT_RENDERER_2026-05-11_DIM8.md` (earlier today, 0 new findings).

## Executive Summary

- **Findings**: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 4 new LOW (+ 3 carry-over LOWs from prior audit still open).
- **Pipeline areas affected**: AS hardening — future-regression-bait only, no live validation hazards.
- **Net verdict**: **CLEAN.** The May 7-10 audit cycle hardened every spec-strict surface (scratch alignment, refit-counts validation, inter-build serialize barriers, BUILD-vs-UPDATE address invariants, TLAS bookkeeping length-equality assert). All new findings are pure spec-strict code review observations — none would surface in validation layers today.

## Prior Audit Re-verification

| Prior Finding | Status |
|---|---|
| #914 / REN-D8-NEW-04 (TLAS address-length invariant) | RESOLVED in-tree — `debug_assert_eq!` at `acceleration.rs:2324-2329` |
| #915 / REN-D8-NEW-05 (`evict_unused_blas` placement) | RESOLVED — first stmt of `build_blas` (663-665) + pre-batch (1443-1445) |
| #907 / REN-D12-NEW-01 (refit count validation) | RESOLVED — `validate_refit_counts` at 1139-1160 + counts populated at all 3 BUILD sites |
| #642 / #644 (scratch-serialize barrier between batched builds) | STILL-OPEN as wired (`record_scratch_serialize_barrier` at 1613-1616) — PASS |
| REN-D8-NEW-10 (TLAS `padded_count` over-allocation) | STILL-OPEN low — by-design pre-sizing |
| REN-D8-NEW-11 (transform conversion unit test) | STILL-OPEN low — conversion verified correct, no test pin added |
| REN-D8-NEW-12 (`frame_counter` shared across slots) | STILL-OPEN cosmetic |

## RT Acceleration Structure Assessment (positive checks)

All 14 checklist items verified clean:

- **BLAS inputs**: `R32G32B32_SFLOAT` at offset 0 of 100 B Vertex stride at all 4 BUILD sites (686, 1423, 923, 1181); `UINT32` index type; `OPAQUE` geometry flag uniformly applied; `PREFER_FAST_TRACE` on static, `PREFER_FAST_BUILD` on skinned (correct per per-frame refit semantics).
- **Scratch buffer**: `SHADER_DEVICE_ADDRESS` at every alloc (780, 1584, 1005, 2657); alignment validated via `is_scratch_aligned` + `debug_assert_scratch_aligned` at every build site; grow-only via `scratch_needs_growth` (356-364); never shrinks mid-frame.
- **Result buffer**: `ACCELERATION_STRUCTURE_STORAGE_KHR | SHADER_DEVICE_ADDRESS` at all 4 sites (743-744, 971-972, 1535-1536, 1750-1751).
- **TLAS instance buffer**: host-visible staging (2107) + device-local copy (2116-2123) with `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR | SHADER_DEVICE_ADDRESS | TRANSFER_DST`; sized `max(2× count, 8192)` (2102).
- **Host→AS barrier**: HOST_WRITE → TRANSFER_READ at 2378-2393, then TRANSFER_WRITE → AS_READ at 2409-2424. Empty-count case skips both per `VUID-VkBufferCopy-size-01988`.
- **BUILD vs UPDATE decision**: `decide_use_update` (284-316) does byte-exact slice comparison on `last_blas_addresses`; gated by `needs_full_rebuild` and `blas_map_generation` dirty flag; empty list forces BUILD (test pin at 3334).
- **UPDATE primitive count**: submits `primitive_count = instance_count` (line 2479), matching source BUILD's count transitively via the address-length-equality invariant (#914).
- **Transform conversion**: column-major glam `to_cols_array` → 3×4 row-major `VkTransformMatrixKHR` at 2017-2021. Spec-correct (`VkTransformMatrixKHR` is row-major `float[3][4]`).
- **`TRIANGLE_FACING_CULL_DISABLE`**: per-instance from `draw_cmd.two_sided` at 2036-2040.
- **Empty TLAS at frame 0**: BUILD with `primitiveCount = 0` legal per `VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03801`; descriptor binding via `tlas_handle(frame).is_some()` after `build_tlas` runs; shader-side `rayQueryInitializeEXT` against 0-instance TLAS returns no-hit per spec.
- **Device addresses**: every `get_buffer_device_address` site preceded by `SHADER_DEVICE_ADDRESS` usage flag.
- **Compaction + LRU**: compaction at `build_blas_batched` Phase 4-6 (1611-1822) outside any render pass; `drop_blas` (551-570) routes via `pending_destroy_blas`; `last_used_frame` bumped during `build_tlas` BEFORE eviction call at `draw.rs:887`; skinned BLAS never eviction candidates (live in `skinned_blas`, not `blas_entries`).
- **Destroy order**: `AccelerationManager::destroy` (2938-2980) drains pending_destroys → `blas_entries` → tlas slots → per-frame scratch → blas_scratch_buffer. Parent `VulkanContext::Drop` calls `device_wait_idle` first.

## New Findings

### [LOW] `instance_custom_index` 24-bit overflow has no guard
**Dimension**: Acceleration Structures
**Location**: `crates/renderer/src/vulkan/acceleration.rs:2047`
**Severity**: LOW
**Observation**:
```rust
instance_custom_index_and_mask: vk::Packed24_8::new(ssbo_idx, 0xFF),
```
`ssbo_idx: u32` is fed from `build_instance_map` (monotonic `0, 1, 2, …` per surviving draw). `vk::Packed24_8::new` silently truncates to 24 bits (max 16 777 215). `padded_count = max(2× count, 8192)` is not a hard cap upstream.
**Why bug**: `VkAccelerationStructureInstanceKHR.instanceCustomIndex` is 24 bits per spec; the SSBO indexing in `triangle.frag` reads it as the GpuInstance array index. A future exterior with > 2^24 surviving draws writes the wrong `ssbo_idx` and silently corrupts every RT hit's material / transform lookup. The hard 32 767 ceiling from the R16_UINT mesh_id (Dim 4 / Dim 5) makes this unreachable today, but it's an invariant tied to two separate locations.
**Fix**: `debug_assert!(ssbo_idx < (1 << 24), …)` at the push site, plus an `info!` once-per-second warning if `instance_count` is within 10% of 2^24.
**Confidence**: HIGH (spec-textual, no live failure today)
**Dedup**: New. Adjacent to #647 / RP-1 (mesh_id ceiling) and the Dim 5 LOW #956 (debug_assert in active recording).

### [LOW] Skinned BLAS BUILD + UPDATE flag set lacks a shared constant
**Dimension**: Acceleration Structures
**Location**: `crates/renderer/src/vulkan/acceleration.rs:948-949, 1213-1214`
**Severity**: LOW
**Observation**: Four distinct flag-set call sites. Static path uses `STATIC_BLAS_FLAGS = PREFER_FAST_TRACE | ALLOW_COMPACTION` (716-720, 1512-1513). Skinned path uses `ALLOW_UPDATE | PREFER_FAST_BUILD` inline at two sites (`build_skinned_blas` 948-949, `refit_skinned_blas` 1213-1214) — no shared constant.
**Why bug**: Vulkan spec requires `mode = UPDATE` to use the same flag set as the source BUILD. The two literals drift in lockstep on every fix; a future change to one without the other would violate the spec invariant silently.
**Fix**: Lift `SKINNED_BLAS_FLAGS = ALLOW_UPDATE | PREFER_FAST_BUILD` to a module constant and reuse at both sites. Mirrors the existing `STATIC_BLAS_FLAGS` pattern.
**Confidence**: HIGH (cosmetic / future-regression-only)
**Dedup**: New.

### [LOW] `refit_skinned_blas` scratch-serialize barrier is caller-emitted
**Dimension**: Acceleration Structures
**Location**: `crates/renderer/src/vulkan/acceleration.rs:1108-1247` (docstring at 1098-1107), call site at `crates/renderer/src/vulkan/context/draw.rs:730-760`
**Severity**: LOW
**Observation**: The function doc says the caller must emit `record_scratch_serialize_barrier` BEFORE invoking it. The function itself does not emit it. Caller at `draw.rs:735` honors this only for entries where a prior synchronous build ran this frame, relying on the broader post-TLAS-build RT memory barrier to cover the per-entity scratch reuse. The contract is preserved today but extremely subtle.
**Why bug**: Not a bug as wired. The risk is "next refactor that adds a 2nd refit call site forgets the precondition".
**Fix**: Make `record_scratch_serialize_barrier` the first statement of `refit_skinned_blas` itself (idempotent — an extra barrier is harmless compared to a missing one).
**Confidence**: MED (no validation-detectable bug at present)
**Dedup**: Adjacent to #642 / #644 closure.

### [LOW] `evict_unused_blas` immediate-destroy safety tied to `MAX_FRAMES_IN_FLIGHT == 2` without a `static_assert`
**Dimension**: Acceleration Structures
**Location**: `crates/renderer/src/vulkan/context/draw.rs:887` + `crates/renderer/src/vulkan/acceleration.rs:2520-2583`
**Severity**: LOW
**Observation**: `evict_unused_blas` calls `destroy_acceleration_structure` immediately (2554-2557), NOT via `pending_destroy_blas`. Safety relies on the LRU gate `idle >= MAX_FRAMES_IN_FLIGHT + 1 = 3` — referenced BLAS have `idle = 0` (bumped at lines 1995, 2003 in `build_tlas` BEFORE the eviction call). Verified safe under current `MAX_FRAMES_IN_FLIGHT == 2`. If a future bump to 3-frames-in-flight ever lands without re-auditing the `min_idle` constant, the safety window closes.
**Why bug**: Not a bug under current pin (`MAX_FRAMES_IN_FLIGHT == 2`, asserted at `sync.rs:33-35`). Fragile to a future invariant change.
**Fix**: Add a `const_assert!(min_idle > MAX_FRAMES_IN_FLIGHT)` near the `min_idle` constant, or document the dependency next to `MAX_FRAMES_IN_FLIGHT`.
**Confidence**: HIGH (correct today, fragile)
**Dedup**: New.

## Prioritized Fix Order

All four are forward-looking hygiene. Recommended order if filing:

1. **LOW** — `instance_custom_index` 24-bit guard. Cheapest, prevents silent corruption at scale.
2. **LOW** — Lift `SKINNED_BLAS_FLAGS` to a shared constant. Forces BUILD/UPDATE flag parity in code.
3. **LOW** — `refit_skinned_blas` self-emits scratch serialize barrier. Idempotent hardening.
4. **LOW** — `MAX_FRAMES_IN_FLIGHT` `const_assert` near `evict_unused_blas`.

## Notes

- Dimensions 1, 3, 4, 5, 8, 9 are all CLEAN as of 2026-05-11. Broad 2026-05-09 sweep covered Dims 2, 6, 7, 10–16.
- The May 7-10 cycle (#907, #914, #915, #642, #644) closed the spec-strict invariants; remaining items are sub-spec hardening (asserts + comments) and refactors for future-safety.
- This audit supersedes the empty-finding prior `AUDIT_RENDERER_2026-05-11_DIM8.md` (which was a baseline pass against the morning's state).
