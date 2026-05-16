# Concurrency Audit — 2026-05-16

**Focus**: Renderer-deep preset — Dimensions 2 (Vulkan Sync), 3 (Resource Lifecycle), 5 (Compute→AS→Fragment Chains).

**Trigger**: Post-`1775a7e6` review of the skinned-BLAS flag split
(`UPDATABLE_AS_FLAGS` / `SKINNED_BLAS_FLAGS`) in
`crates/renderer/src/vulkan/acceleration/constants.rs`. Verify no
other call site escaped the lift, then walk the surrounding
synchronization scaffolding around the same scratch buffer the
skinned path shares with static + TLAS builds.

**Method**: Manual audit following `.claude/commands/audit-concurrency.md`
(Skill tool denied). Read every `*_AS_FLAGS` use site, every
`record_scratch_serialize_barrier` emit / call, every `queue_submit`
site, and the `AccelerationManager::destroy` chain. Cross-checked
against the prior concurrency audit (`AUDIT_CONCURRENCY_2026-05-13.md`).

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 1 |
| MEDIUM   | 2 |
| LOW      | 1 |
| INFO     | 2 |

The `1775a7e6` flag split itself is **clean** — all 5 call sites
in `blas_skinned.rs` (3 build paths × 1-2 use sites each, plus the
refit) point at `SKINNED_BLAS_FLAGS`; the size-query / record pairs
within each function use the same flag set so VUID-…-03667 is
preserved by construction. No additional skinned-flag sites
escaped the lift.

The audit DID surface one HIGH that the prior 2026-05-13 audit
(F1 — INFO) wrongly cleared: the queue-mutex guard pattern
`let q = *queue.lock().expect(...)` drops the guard at end of
statement, so `queue_submit` runs **outside** the lock. Today
benign (all submits are main-thread), but the discipline the
`Arc<Mutex<vk::Queue>>` was built to enforce (VUID-vkQueueSubmit-
queue-00893) is silently absent.

## Dedup Baseline

- `gh issue list --limit 200` snapshot: `/tmp/audit/concurrency/issues.json` (no matching open issues for queue-lock, scratch-barrier, or skinned-BLAS-destroy keywords).
- Prior reports walked: `AUDIT_CONCURRENCY_2026-05-13.md`, `AUDIT_RENDERER_2026-05-15.md`, `AUDIT_RENDERER_2026-05-11_DIM8_v2.md`.
- One finding (`CONC-D4-NEW-01`) is a **REGRESSION of audit verdict** — the prior audit's F1 (INFO) cleared it incorrectly. Upgraded here to HIGH.

---

## Dimension 2 — Vulkan Synchronization

### CONC-D2-NEW-01: queue MutexGuard dropped before `queue_submit` (`*queue.lock()` deref pattern)
- **Severity**: HIGH
- **Dimension**: Vulkan Sync / Thread Safety
- **Location**: `crates/renderer/src/vulkan/texture.rs:658-664`, `crates/renderer/src/vulkan/context/draw.rs:2379-2399`
- **Status**: Regression-of-verdict of `AUDIT_CONCURRENCY_2026-05-13.md` F1 (INFO — incorrectly cleared)
- **Description**: Both queue-submit sites read the queue handle out of the Mutex with `let q = *self.graphics_queue.lock().expect("...");`. Because `vk::Queue` is `Copy` (a u64 dispatchable handle), `*` deref returns the value and the `MutexGuard` becomes a temporary that is **dropped at the end of the let-statement** — before `queue_submit` even executes. The named binding `q` is a plain `vk::Queue`, not a guard. By contrast the present-queue site (`draw.rs:2427-2440`) binds the guard (`let pq = self.present_queue.lock()...`) and derefs `*pq` inside the call — guard is held across `queue_present` as intended.
- **Evidence**:
  ```rust
  // texture.rs:658 — guard dropped before submit
  let q = *queue.lock().expect("graphics queue lock poisoned");
  device
      .queue_submit(q, &[submit_info], fence)
      .context("submit one-time commands")?;
  device
      .wait_for_fences(&[fence], true, u64::MAX)
      .context("wait for one-time commands")?;

  // draw.rs:2379 — same pattern in main per-frame submit
  let queue = *self.graphics_queue.lock().expect("graphics queue lock poisoned");
  if let Err(e) = self.device.queue_submit(queue, &[submit_info], ...) { ... }

  // draw.rs:2427 — CORRECT pattern (present queue): guard binds
  let pq = self.present_queue.lock().expect("present queue lock poisoned");
  match self.swapchain_state.swapchain_loader.queue_present(*pq, &present_info) { ... }
  ```
  Doc on `graphics_queue` (`context/mod.rs:1032-1035`) explicitly states:
  *"Graphics queue, wrapped in a Mutex for Vulkan-required external synchronization (VUID-vkQueueSubmit-queue-00893). All queue submissions (draw_frame, texture/buffer uploads) must lock this."*
- **Impact**: Latent — every Vulkan submit today runs from the main thread (streaming worker only touches the World; SwfPlayer wgpu device is independent). The lock is functionally a no-op single-threaded, so VUID-00893 is satisfied **by call-site discipline, not by the lock**. The bug becomes a real concurrent-submit data race the day a future change spawns a parallel BLAS-build worker, async texture-streaming thread, or moves the screenshot path off main — exactly the parallelism the Mutex was built to enable. Validation layers do NOT catch this (single-threaded test runs never trip 00893). NVIDIA / AMD drivers will execute the racing submits but the ordering of in-flight semaphores / fences becomes implementation-defined.
- **Trigger Conditions**: Two threads concurrently entering any `with_one_time_commands` / draw-frame submit site. Today: zero in-engine triggers. Tomorrow (parallel BLAS workers, async upload, screenshot off-main): immediate.
- **Related**: Prior audit F1 (INFO) at `AUDIT_CONCURRENCY_2026-05-13.md:135-138` cleared this with "Recording is outside both locks" — correct as stated (recording IS outside the lock by design) but the test the auditor meant to run was whether the **submit** is inside the lock. It is not.
- **Suggested Fix**: Bind the guard, deref inside the call: `let q = self.graphics_queue.lock().expect("..."); self.device.queue_submit(*q, &[submit_info], fence)?;` and let the guard live to end of unsafe block. Mirror the existing present-site pattern. The fix is mechanical (drop the leading `*`, rename `q` → keep the guard) at every `*queue.lock()` site. Single grep covers both: `grep -rn 'let.*=\s*\*.*queue.lock'`.

### CONC-D2-NEW-02: `STATIC_BLAS_FLAGS` constant duplicated as inline literal in `build_blas_batched`
- **Severity**: LOW
- **Dimension**: Vulkan Sync (flag-set drift defense — sibling of the `1775a7e6` lift)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs:210-213` (const), `:549-556` (size-query literal), `:671-682` (record literal)
- **Status**: NEW
- **Description**: `build_blas` (single-shot path) hoists the static-BLAS flags to a function-local constant `STATIC_BLAS_FLAGS = PREFER_FAST_TRACE | ALLOW_COMPACTION`. `build_blas_batched` writes the **same** flag set as two separate inline literal expressions — one at the per-mesh size query (line 552) and one at the per-mesh record (line 674). VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03801 requires size-query flags and record flags to match, which the `1775a7e6` skinned-BLAS lift made structural via a shared constant. Static path has not yet had the same lift.
- **Evidence**: Three separate textual sites for the same flag set in `blas_static.rs`. A patch adding `ALLOW_UPDATE` (or, more realistically, `LOW_MEMORY` for the upcoming compaction work) to one site silently drifts from the other two; the size-query mismatch lights up at runtime, the build-record mismatch tripping pInfos-03667 only after a separate UPDATE call site is added.
- **Impact**: Today: zero — every site reads the same flag pair. Future-regression risk: same shape as the `6059e2ab` skinned-flag drift that motivated `1775a7e6`. Maintainability + defense-in-depth, not correctness.
- **Trigger Conditions**: A patch touches one of the three sites without touching the others. Compiler/test cannot catch — they all type-check the same `vk::BuildAccelerationStructureFlagsKHR` value.
- **Related**: `1775a7e6` (skinned-BLAS lift), `#958` (TLAS lift), `STATIC_BLAS_FLAGS` const at line 210.
- **Suggested Fix**: Promote `STATIC_BLAS_FLAGS` from a function-local const to `pub(super) const` in `acceleration/constants.rs`, alongside `SKINNED_BLAS_FLAGS` / `UPDATABLE_AS_FLAGS`. Replace the two inline literals in `build_blas_batched` with the new constant. Three constants now cover the three BUILD-target families (TLAS / static BLAS / skinned BLAS), each with the matching docstring referencing VUID-03667 / 03801.

---

## Dimension 3 — Resource Lifecycle

### CONC-D3-NEW-01: `AccelerationManager::destroy` does not drain `skinned_blas` — leak if called without pre-drain
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/mod.rs:244-287` (destroy impl), `crates/renderer/src/vulkan/context/mod.rs:2067-2085` (only correct caller)
- **Status**: NEW
- **Description**: `destroy()` explicitly loops `blas_entries`, `tlas` slots, `scratch_buffers` array, and `blas_scratch_buffer` — but does NOT iterate `skinned_blas: HashMap<EntityId, BlasEntry>`. The only correct shutdown path is `context/mod.rs:2073-2075`, which manually walks `accel.skinned_blas_entities()` and routes each through `accel.drop_skinned_blas(eid)` BEFORE invoking `destroy()`. `drop_skinned_blas` moves the entry into `pending_destroy_blas`, which `destroy()` then drains via the `drain_pending_destroys()` first-line call. So in the production path: works. **Outside** the production path: any caller that invokes `destroy()` without the pre-drain leaks the inner `VkAccelerationStructureKHR` + GpuBuffer of every still-resident skinned BLAS — the HashMap drops as Rust frees memory, but `accel: vk::AccelerationStructureKHR` is just a u64 handle, destruction requires explicit `accel_loader.destroy_acceleration_structure(...)`.
- **Evidence**:
  ```rust
  // acceleration/mod.rs:262-287 — destroy() body
  self.drain_pending_destroys(device, allocator);  // drains pending_destroy_blas only
  for entry in self.blas_entries.drain(..) { ... } // static BLAS map
  for slot in &mut self.tlas { ... }               // per-FIF TLAS
  for scratch in &mut self.scratch_buffers { ... } // TLAS scratch
  if let Some(mut scratch) = self.blas_scratch_buffer.take() { ... }
  // <-- skinned_blas: HashMap<EntityId, BlasEntry> never iterated
  ```
- **Impact**: Production shutdown OK today (one correct caller; the discipline is documented at `context/mod.rs:2068-2075`). Risk surface:
  1. A future test that constructs an `AccelerationManager` directly, registers skinned entries (or just inserts via test harness), calls `destroy()` without the App-level dance → leaks AS handles + GPU buffers (driver warns on `device_destroy`, validation layer flags `VkAccelerationStructureKHR not destroyed`).
  2. A future error-path refactor in `App::shutdown` that skips the pre-drain (e.g. early-return on allocator-lookup failure) silently regresses.
  3. The asymmetry (static BLAS drained in `destroy()`, skinned BLAS drained by external orchestration) is invisible at the function signature — easy to miss in refactors.
- **Trigger Conditions**: `destroy()` called with `!skinned_blas.is_empty()`.
- **Related**: Prior audit REN-D3-NEW-02 (#643 / MEM-2-1) covers the per-frame eviction + cell-unload path for skin slots, but not the shutdown drain symmetry.
- **Suggested Fix**: Add a `skinned_blas` drain inside `destroy()`, mirroring the `blas_entries` loop, BEFORE the `blas_scratch_buffer` teardown:
  ```rust
  for (_eid, mut entry) in self.skinned_blas.drain() {
      self.accel_loader.destroy_acceleration_structure(entry.accel, None);
      entry.buffer.destroy(device, allocator);
  }
  ```
  The App-level pre-drain via `drop_skinned_blas` → `pending_destroy_blas` can then become an optimization (defers destruction by `MAX_FRAMES_IN_FLIGHT` if a draw is mid-flight) rather than a correctness requirement. `device_wait_idle` in the parent Drop chain (`context/mod.rs:1300`) covers in-flight references either way.

### CONC-D3-NEW-02: `record_scratch_serialize_barrier` self-emit in `refit_skinned_blas` is correct, but the docstring contradicts the runtime check at L555
- **Severity**: INFO
- **Dimension**: Resource Lifecycle / Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:523-555` (function safety docstring + self-emit), `:705-730` (helper)
- **Status**: NEW (cosmetic)
- **Description**: Verified the structural fix landed in `#983 / REN-D8-NEW-15` is intact — `refit_skinned_blas` self-emits `record_scratch_serialize_barrier` as its first statement. Cross-checked the three caller-side legacy emit sites; only `draw.rs:879-880` retains the documenting comment ("Scratch-serialize barrier is now self-emitted at the top of refit_skinned_blas") with the caller-side emit removed (#1095). `build_skinned_blas_batched_on_cmd` correctly emits the barrier only between iterations (`for (i, p) in prepared.iter().enumerate() { if i > 0 { ... } }` at line 445-448) — first iteration's BUILD is covered by the COMPUTE→AS_BUILD barrier at `draw.rs:794-800`. Subsequent refit-loop's first iteration is covered by the self-emit. The full chain:
  1. COMPUTE_SHADER_WRITE → AS_BUILD_INPUT_READ (`draw.rs:794-800`) — covers vertex-buffer write-to-read.
  2. AS_WRITE → AS_WRITE between BUILD-batch entries (`blas_skinned.rs:447`) — scratch reuse.
  3. AS_WRITE → AS_WRITE on first refit (`blas_skinned.rs:555`, self-emit) — covers BUILD-batch → first-refit transition.
  4. AS_WRITE → AS_WRITE between refits (`blas_skinned.rs:555` on each call) — scratch reuse.
  5. AS_WRITE → AS_READ at end (`draw.rs:903-909`) — BLAS → TLAS handoff.
- **Impact**: None — correct.
- **Suggested Fix**: None. The barrier chain audits clean. Filed as INFO so the next concurrency audit can skip re-verifying. The minor docstring contradiction (line 523-534 still calls the barrier "a caller-side precondition documented but unenforced" past tense, the next paragraph reverses it) is a comment-style nit not worth a code change.

---

## Dimension 5 — Compute → AS → Fragment Chains

### CONC-D5-NEW-01: Skinned-BLAS BUILD-batch and refit reuse `blas_scratch_buffer` across cell-load + per-frame submissions — host-fence-wait does NOT establish device-side memory ordering
- **Severity**: MEDIUM
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:140-155, 396-429` (scratch grow), `:545-555` (self-emit barrier), `crates/renderer/src/vulkan/acceleration/blas_static.rs:622-647` (cell-load batched scratch grow)
- **Status**: Coverage already in place via `blas_skinned.rs:555` self-emit barrier — flagged as MEDIUM because the **invariant** has no test that pins it.
- **Description**: The shared `blas_scratch_buffer` is written by three submission contexts: (a) cell-load `build_blas_batched` via `submit_one_time` (separate cmd buffer, fenced); (b) per-frame `build_skinned_blas_batched_on_cmd` on the main draw cmd; (c) per-frame `refit_skinned_blas` loop on the main draw cmd. All three write the same VkBuffer's memory. The Vulkan spec on `scratchData.deviceAddress` requires an `AS_WRITE → AS_WRITE` memory dependency between every pair of build/refit ops that share scratch — **regardless of submission boundary**. The host fence-wait that `submit_one_time` performs after cell-load BLAS builds establishes a *host-side* dependency (the CPU has observed the GPU finished) but does NOT establish a device-side memory barrier for the *next* submission's commands. The mitigation is in place: `refit_skinned_blas` self-emits the barrier at line 555 (#983 / REN-D8-NEW-15), so the steady-state per-frame chain is safe even if a cell-load BUILD ran in a separate submission earlier in the same frame.
- **Evidence**: Comment at `blas_skinned.rs:545-554` documents exactly this concern as the motivation for the self-emit. The barrier-coverage chain (CONC-D3-NEW-02) confirms the runtime is correct. **No unit test pins the invariant** — if a future refactor moves the barrier from the callee back to a caller-side optimization (e.g. an optimizer notices same-submission emit-count), the cross-submission case silently regresses with no test coverage. Validation layers do NOT catch cross-submission scratch races because they reason per-submission.
- **Impact**: Today: zero — barrier is correctly emitted on every refit entry. Tomorrow: refactor risk + a similar pattern landing for caustic / volumetrics scratch buffers without the same barrier could silently rate as "works on a fast machine, corrupts BVH on a slower one" (the host fence-wait latency is what hides the bug — on a hypothetical fast device the GPU could be still draining the cell-load build when the per-frame submit begins).
- **Trigger Conditions**: Two AS-build submissions sharing `blas_scratch_buffer` without the AS_WRITE→AS_WRITE serialize barrier between them. Sub-condition: a refactor removes the `blas_skinned.rs:555` self-emit, or a new BLAS / scratch-sharing path is added without porting the self-emit pattern.
- **Related**: #983 / REN-D8-NEW-15 (the structural fix), #644 / MEM-2-2 (original landing), `AUDIT_CONCURRENCY_2026-05-13.md` Dim 5 F4.
- **Suggested Fix**: Add a regression test under `acceleration/tests.rs` that builds a synthetic two-submission scenario (one mock BUILD command-buffer fingerprint, one mock REFIT) and asserts the barrier emit count via a recording-mock or a feature-gated `cmd_pipeline_barrier` counter. A pure unit-test predicate (e.g. `requires_pre_refit_serialize_barrier(prior_submission_kind) -> bool`) co-located with the existing `predicates.rs` family is sufficient if a live-Vulkan test is too heavy.

### CONC-D5-NEW-02: `build_skinned_blas` (single-shot, sync) shares `blas_scratch_buffer` with the same-frame batched on-cmd path — no barrier between if both fire in one frame
- **Severity**: INFO
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:46-231` (`build_skinned_blas` sync path), `:265-508` (`build_skinned_blas_batched_on_cmd` per-frame path)
- **Status**: NEW — verified unreachable in current call graph
- **Description**: The sync `build_skinned_blas` path uses `submit_one_time` and runs in its own fenced command buffer. The per-frame `build_skinned_blas_batched_on_cmd` records onto the main draw cmd. Both write the same `blas_scratch_buffer`. If both fire in the same frame:
  - Sync `build_skinned_blas` submits + waits for its fence (CPU blocked until GPU drained).
  - Then per-frame `build_skinned_blas_batched_on_cmd` records on the main cmd.
  - Submit happens at `draw.rs:2385`.
  The host fence-wait at the sync site does NOT establish a device-side memory dependency for the subsequent per-frame submission. The first BUILD inside `build_skinned_blas_batched_on_cmd` doesn't get a scratch-serialize barrier (the loop emits the barrier only `if i > 0` at `blas_skinned.rs:446`). The COMPUTE→AS_BUILD barrier at `draw.rs:794-800` covers the vertex-buffer dependency but NOT the scratch buffer.
  **Walked the call graph**: `build_skinned_blas` (sync) has zero in-engine callers as of `1775a7e6` (`grep -rn 'build_skinned_blas\b' --include='*.rs'` returns only the test harness and the trait-definition site; the production path at `draw.rs:822` uses `build_skinned_blas_batched_on_cmd`). Per #911 / REN-D5-NEW-02 the sync path was deprecated in favour of the batched on-cmd builder precisely to eliminate the per-NPC fence stall.
- **Impact**: None today (path unreachable). If the sync path is ever revived (test scaffold, one-off debug entry, fallback for batched failure), the latent gap turns into a real same-frame scratch race.
- **Suggested Fix**: Either (a) delete `build_skinned_blas` (sync) once a release cycle confirms no caller, downgrading the API surface to the batched-on-cmd builder, OR (b) add a comment + `debug_assert!(self.skinned_blas.is_empty() || !cfg!(debug_assertions), "sync build_skinned_blas is deprecated; use build_skinned_blas_batched_on_cmd")` at function entry to catch a future revival in tests. Option (a) is cleaner — the sync path is dead weight per the `1775a7e6` commit message.

---

## Out-of-Scope (Not Audited This Pass)

- Dimension 1 (ECS Lock Ordering) — out of focus per `--focus 2,3,5`.
- Dimension 4 (Thread Safety) — out of focus, though `CONC-D2-NEW-01` is **also** a Dim-4 finding by topic.
- Dimension 6 (Worker Threads) — out of focus. Streaming worker confirmed in passing to do ECS-only work (no Vulkan submits) so the queue-mutex finding's blast radius stays bounded today.

## Suggested Next Steps

1. Fix `CONC-D2-NEW-01` first — mechanical fix, defends the Mutex's stated invariant, removes the regression of the prior audit's verdict. Two call sites (`texture.rs:658`, `draw.rs:2379-2399`).
2. Wire `CONC-D3-NEW-01` skinned-BLAS drain into `destroy()` — symmetric with the existing `blas_entries` loop, defends against test / refactor regressions.
3. Promote `STATIC_BLAS_FLAGS` to module constant (`CONC-D2-NEW-02`) alongside the existing two — completes the lift pattern started by `1775a7e6` / #958 across all three BUILD-target families.
4. Add a unit test for the cross-submission scratch barrier invariant (`CONC-D5-NEW-01`) — pins the structural fix from `#983` so a future refactor can't silently regress it.
5. Delete sync `build_skinned_blas` per `CONC-D5-NEW-02` (or document deprecation).

Do **not** publish to GitHub — caller will route through `/audit-publish` after review.
