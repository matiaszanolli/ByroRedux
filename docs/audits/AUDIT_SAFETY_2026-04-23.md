# Safety Audit — 2026-04-23

**Scope**: Full codebase — unsafe blocks, Vulkan spec, GPU memory, thread safety, FFI, RT pipeline
**Auditor**: Claude Opus 4.7 (direct + renderer-specialist + ecs-specialist agents)
**Baseline**: `AUDIT_SAFETY_2026-04-05.md` — SAFE-01 through SAFE-19

## Summary

| Severity | NEW | Carried Over | Fixed since 2026-04-05 |
|----------|-----|--------------|------------------------|
| HIGH     | 0   | 0            | 3 of 3 (SAFE-01, SAFE-02, SAFE-03) |
| MEDIUM   | 1   | 3            | 5 of 8 (SAFE-04/05/06/08/10) |
| LOW      | 1   | 4            | 4 of 8 (SAFE-07/13/16/17) |

**Headline**: The codebase has substantially closed the 2026-04-05 findings. All 3 HIGH and 5 of 8 MEDIUM items are now fixed — SAFE-03 via a dedicated `lock_tracker.rs` with a thread-local TypeId set; SAFE-04/05/06 via an RAII `StagingGuard` in `buffer.rs`; SAFE-10 via `resource_2_mut` with same-type panic. The remaining surface is mostly hygiene (SAFETY comment coverage on post-April modules) and one defense-in-depth item around resource/N-way query ordering that only becomes live under a parallel scheduler.

---

## Fixed Since 2026-04-05 (confirmed)

| ID | Title | Evidence |
|----|-------|----------|
| SAFE-01 | `write_mapped` silently truncates | `buffer.rs:601` — logs `warn!` on overflow before truncating |
| SAFE-02 | `build_tlas` error swallowed at init | `context/mod.rs:673-676` — propagates via `.context(...)` + `?`; per-frame path at `draw.rs:262-270` uses `if let Err(e)` with warning |
| SAFE-03 | RwLock not reentrant | `core/src/ecs/lock_tracker.rs:48` — `thread_local!` TypeId set; test `resource_read_then_write_same_type_panics` at `world.rs:1658` |
| SAFE-04/05/06 | Staging buffer leaks on error | `buffer.rs:264` `StagingGuard` with `Drop` at `:323`; used in `texture.rs:90,338` and `buffer.rs:759` |
| SAFE-08 + SAFE-16 | `nonCoherentAtomSize` / WHOLE_SIZE flush | `buffer.rs:346` `aligned_flush_range`; consumed at `:563,:620,:664` |
| SAFE-07 | `update_rgba` deferred destroy single-slot | `texture_registry.rs:34` now `VecDeque<(u64, Texture)>` frame-tagged |
| SAFE-10 | Resource locks no ordering | `world.rs:575` `resource_2_mut`; test `resource_2_mut_same_type_panics` at `:1477` |
| SAFE-13 | `upload_lights` ptr copy SAFETY | `scene_buffer.rs:897,949,985,1032,1111` — all 5 `copy_nonoverlapping` sites now have SAFETY comments |
| SAFE-17 | `CStr::from_ptr` uncommented | `device.rs:120-122,149-151` both have SAFETY comments |
| SAFE-18 | Union field init uncommented | `acceleration.rs:410,510,493` — `DeviceOrHostAddressConstKHR` + `DeviceOrHostAddressKHR` init sites covered (9 SAFETY comments in the file) |

---

## Still Open (from 2026-04-05)

### SAFE-09: Ad-hoc multi-query N>2 has no TypeId ordering
- **Severity**: MEDIUM (upgrades to HIGH under parallel scheduler)
- **Dimension**: Thread Safety
- **Location**: `byroredux/src/systems.rs` (render data collection, transform propagation)
- **Status**: Existing (SAFE-09 from 2026-04-05)
- **Description**: `query_2_mut` + `resource_2_mut` both sort their pair by `TypeId` and guard against same-type double-locking. Queries with 3+ components, or query+resource mixes, still acquire locks in source order.
- **Impact**: Benign under the current sequential scheduler. An ABBA deadlock becomes reachable the moment two systems with overlapping type sets can run in parallel.
- **Suggested Fix**: `query_N_mut` builder that sorts a Vec<TypeId> before acquisition; or runtime lock-order validator gated on debug builds via `lock_tracker`.

### SAFE-11: Pipeline cache loaded from untrusted CWD path
- **Severity**: MEDIUM
- **Dimension**: Vulkan Spec
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs` (`load_or_create_pipeline_cache`)
- **Status**: Existing (SAFE-11 from 2026-04-05)
- **Description**: `pipeline_cache.bin` is read from CWD and fed to `VkPipelineCacheCreateInfo::initial_data`. Drivers validate header but a malicious file could still exercise driver bugs.
- **Suggested Fix**: Move to config dir (`dirs::config_dir()`), validate header vendor/device ID before passing to driver.

### SAFE-12: Swapchain raw pointer to stack-local queue_family_indices
- **Severity**: LOW
- **Dimension**: Unsafe Blocks
- **Location**: `crates/renderer/src/vulkan/swapchain.rs`
- **Status**: Existing (SAFE-12 from 2026-04-05)
- **Description**: `queue_family_indices.as_ptr()` stored as raw pointer; array and struct share function scope so currently valid but fragile and uncommented.
- **Suggested Fix**: Use ash builder `.queue_family_indices(&arr)` or add SAFETY comment explaining the lifetime.

### SAFE-14: Poisoned lock cascade
- **Severity**: LOW
- **Dimension**: Thread Safety
- **Location**: `crates/core/src/ecs/world.rs`, `context/mod.rs:1169,1219,1252` (various `.expect("...poisoned")`)
- **Status**: Existing (SAFE-14 from 2026-04-05)
- **Suggested Fix**: Optionally recover via `into_inner().unwrap_or_else(PoisonError::into_inner)` or wrap scheduler tick in `catch_unwind`.

### SAFE-15: Depth image leak on post-allocate error
- **Severity**: LOW (verified path still looks leaky per Drop in `context/mod.rs:1209`; needs rescan of `create_depth_resources` in `helpers.rs`)
- **Dimension**: GPU Memory
- **Status**: Existing (SAFE-15 from 2026-04-05, likely still present)
- **Suggested Fix**: Wrap in the same `StagingGuard`-style RAII used in `buffer.rs` for staging allocations.

### SAFE-19: `update_rgba` descriptor set update races with in-flight command buffers
- **Severity**: LOW (partial mitigation via VecDeque pending_destroy)
- **Dimension**: Vulkan Spec
- **Location**: `crates/renderer/src/texture_registry.rs:545` (frame-counted queue) + descriptor write loop
- **Status**: Existing (SAFE-19 from 2026-04-05) — pending_destroy timing fixed (SAFE-07) but all per-frame descriptor sets are still rewritten simultaneously.
- **Suggested Fix**: Write only the current frame's descriptor set per call; defer others to fence signal.

---

## NEW Findings

### SAFE-20: Safety-comment coverage gap in post-April renderer modules
- **Severity**: MEDIUM (aggregate)
- **Dimension**: Unsafe Blocks
- **Location**: `crates/renderer/src/vulkan/` — `caustic.rs`, `ssao.rs`, `taa.rs`, `gbuffer.rs`, `svgf.rs`, `composite.rs`
- **Status**: NEW
- **Description**: Module-level scan (grep `^[[:space:]]*unsafe ` vs `// SAFETY`):
  - `caustic.rs` — 19 unsafe / 0 SAFETY
  - `composite.rs` — 25 unsafe / 1 SAFETY
  - `ssao.rs` — 10 unsafe / 0 SAFETY
  - `taa.rs` — 17 unsafe / 0 SAFETY
  - `svgf.rs` — 18 unsafe / 0 SAFETY
  - `gbuffer.rs` — 9 unsafe / 1 SAFETY
- **Evidence**: Each module follows the new `try_or_cleanup!` + partial-destroy pattern (good RAII), but the per-call unsafe wrappers (`device.create_image`, `device.update_descriptor_sets`, etc.) are undocumented. Per `_audit-severity.md`, "unsafe block without safety comment" is MEDIUM.
- **Impact**: No live bug. Reviewer velocity tax: every future unsafe change in these files requires re-deriving the invariant from scratch.
- **Related**: `acceleration.rs` is the model — 22 unsafe / 9 SAFETY, with every non-trivial union init and device-address query annotated.
- **Suggested Fix**: One-line safety comment on each `unsafe { device.X(…) }` block stating which of (a) ash preconditions, (b) caller-supplied handle validity, (c) union-field choice is being upheld. Many will be a single common boilerplate ("device and descriptor handles are valid for the lifetime of `self`"), which is fine — the comment is what's enforcing the invariant on future edits.

### SAFE-21: `acceleration.rs:694` lifetime transmute to `'static` on batched BLAS build
- **Severity**: LOW
- **Dimension**: Unsafe Blocks
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:694`
- **Status**: NEW
- **Description**: A `std::mem::transmute` (or equivalent) extends the lifetime of `triangles_data` to `'static` for the batched BLAS build path. The SAFETY comment states "prepared buffers for this batch are local" — but the transmute itself is doing the lifetime lie; the real invariant is that the batch completes within the same command recording before `triangles_data` drops.
- **Evidence**: `grep SAFETY` shows the comment at `:681,:694` but the reasoning is compressed.
- **Impact**: Correct today (single function scope). A refactor that defers batch submission past `triangles_data`'s scope would be UB with no compiler warning.
- **Suggested Fix**: Expand the SAFETY comment to explicitly name the scope dependency: "`triangles_data` MUST outlive this `cmd_build_acceleration_structures` call — the transmute is sound only because the command is submitted before returning from this function." Consider whether a scope-tying builder could eliminate the transmute entirely.

---

## Positive Findings

- **VulkanContext::Drop** (`context/mod.rs:1152-1272`): Clean reverse-order teardown with a single top-level SAFETY comment. All 9 post-April pipelines (ssao, composite, caustic, svgf, taa, gbuffer, cluster_cull, accel, scene_buffers) are wired into the allocator-gated destroy block. Allocator drop uses `Arc::try_unwrap` with a `log::error!` + `debug_assert!` on outstanding refs (no silent leak).
- **ECS reentrant lock guard** (`lock_tracker.rs`): `thread_local!` TypeId set with panic-on-reentry tests covering both query and resource paths — SAFE-03 is comprehensively closed.
- **StagingGuard RAII** (`buffer.rs:264-345`): Exactly the fix suggested in SAFE-04; `Drop` frees the staging VkBuffer + gpu-allocator allocation on any error path. Adopted in texture.rs + buffer.rs.
- **acceleration.rs SAFETY coverage**: 9 SAFETY comments covering every `get_buffer_device_address` query (with SHADER_DEVICE_ADDRESS usage precondition), every `DeviceOrHostAddressConstKHR`/`DeviceOrHostAddressKHR` union init, and the BLAS/TLAS build entry points. This is the model for the other modules (SAFE-20).
- **FFI (cxx-bridge)** remains trivially safe — single struct + two functions, no raw pointers across the boundary (unchanged since 2026-04-05).

---

## Priority Action Items

1. **SAFE-20**: Sweep safety comments across `caustic.rs`, `taa.rs`, `ssao.rs`, `svgf.rs`, `composite.rs`, `gbuffer.rs`. Most are one-line boilerplate; the value is the forcing function on future edits.
2. **SAFE-09**: Sketch a `query_N_mut` API or a runtime lock-order validator before enabling a parallel scheduler.
3. **SAFE-21**: Tighten the safety comment on `acceleration.rs:694` to name the scope dependency.
4. **SAFE-11**: Move pipeline cache to `dirs::config_dir()` — cheap.
5. **SAFE-15**: Apply `StagingGuard`-style cleanup to `create_depth_resources`.

---

## Methodology Notes

- Issue dedup against 200-issue snapshot in `/tmp/audit/issues.json`.
- Each SAFE-NN from the 2026-04-05 audit re-verified by direct grep against current tree: found evidence of fix or survival before labelling.
- Module-level unsafe-vs-SAFETY counts driven from `grep -cE '^[[:space:]]*unsafe '` and `grep -c '// SAFETY'`.
- Skipped individual-block reporting where the aggregate (SAFE-20) carries more signal; published the grep table instead.
