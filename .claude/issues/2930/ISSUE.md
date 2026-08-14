# CON-D1-02 (HYPOTHESIS): static BLAS build paths don't self-emit the leading scratch-serialize barrier

- **Issue**: [#2930](https://github.com/matiaszanolli/ByroRedux/issues/2930)
- **Finding ID**: `CON-D1-02`
- **Labels**: `medium,sync,vulkan,bug`
- **Source report**: [`docs/audits/AUDIT_CONCURRENCY_2026-08-14.md`](../../../docs/audits/AUDIT_CONCURRENCY_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2930 --json state`.

---

> ### ⚠ HYPOTHESIS — do not ship a barrier change on this alone
>
> No Vulkan device, no captured validation run, and no RenderDoc capture backed this finding; it is source-read only. Per the project's standing rule against speculative Vulkan fixes, **confirm before changing any barrier, stage mask, or layout.**
>
> Confirm with a **release** build (debug is too slow to stream the dense cells that fault):
>
> ```
> BYRO_VALIDATION=1 cargo run --release -- ...
> BYRO_VALIDATION=gpuav cargo run --release -- ...
> ```
>
> The `--cornell` harness plus one dense cell adjudicates this. That is the same channel that confirmed #507945d8 at ~40 RAW hazards/frame pre-fix.


- **Severity**: MEDIUM — **HYPOTHESIS row, not a fix.** Do not ship a barrier change on this
  reasoning alone.
- **Dimension**: Vulkan Queue & AS Sync
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs`
  (`AccelerationManager::build_blas`, `AccelerationManager::build_blas_batched` — the
  `submit_one_time` closures, whose build loops emit `record_scratch_serialize_barrier` only for
  `i > 0`); counterpart `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`
  (`build_skinned_blas_batched_on_cmd`, `refit_skinned_blas`, `record_scratch_serialize_barrier`);
  the trailing barrier in `crates/renderer/src/vulkan/context/skinned_blas_refit.rs`
  (`record_skinned_blas_refit`); rule model in
  `crates/renderer/src/vulkan/acceleration/predicates.rs` (`ScratchUser`,
  `requires_scratch_serialize_barrier_before`)
- **Status**: NEW (mirror direction of #1300 CLOSED and #983 / #644; no OPEN match)
- **Description**: `blas_scratch_buffer` is a **single shared allocation** used by four writers —
  `build_blas`, `build_blas_batched` (both on `submit_one_time` one-off command buffers), and
  `build_skinned_blas_batched_on_cmd` / `refit_skinned_blas` (both on the per-frame `cmd`). That
  sharing is established fact in this codebase (#2460 was filed precisely because
  `shrink_blas_scratch_to_fit` walked only the static half of it).

  The house rule, codified by `requires_scratch_serialize_barrier_before` and its `ScratchUser`
  enum, is that *any* prior writer to the shared scratch — including across a submission boundary —
  requires an `AS_WRITE → AS_WRITE` dependency before the next build reuses it, and that a host
  fence-wait does **not** substitute for it. Both skinned paths self-emit that leading barrier
  (`refit_skinned_blas` under #983, `build_skinned_blas_batched_on_cmd`'s `i == 0` under #1300).
  **Neither static path does.** `build_blas_batched`'s loop is `if i > 0 { record_scratch_serialize_barrier(..) }`
  with no pre-loop emit, and `build_blas` records a single build with none at all.

  The direction that leaves unguarded is the mirror of the one the enum models. `ScratchUser`
  enumerates only `CrossSubmissionBuildWithFenceWait` — "a one-time BUILD ran earlier this frame and
  the host has since fence-waited it". The reverse is: the **previously-submitted per-frame command
  buffer's** skinned builds/refits are still executing on the GPU, writing this scratch, when
  `step_streaming` (in `about_to_wait`, before the next `draw_frame`'s fence wait — the exact window
  #1782's own comment names) submits a static `build_blas_batched`. Nothing has fence-waited
  anything in that direction.

  **The reason this is a HYPOTHESIS and not a defect claim**: the hazard is not entirely
  unguarded. `record_skinned_blas_refit` closes its skinned block with an
  `AS_BUILD/AS_WRITE → AS_BUILD/AS_READ` `memory_barrier`, and a `vkCmdPipelineBarrier`'s second
  synchronization scope includes commands *later in submission order on the same queue* — i.e. the
  subsequent one-time submission. I checked the gating: that trailing barrier and the scratch writes
  are **co-gated** on the same `if !dispatches.is_empty()` block, so whenever the frame command
  buffer writes `blas_scratch_buffer` the trailing barrier is emitted too. So an execution
  dependency does exist. The open question is narrowly whether an `AS_READ`-only **dst access mask**
  is sufficient for the write-after-write on the scratch region, or whether `AS_WRITE` must appear
  there — the exact symmetric question #1790 answered in the other direction when it added `AS_READ`
  to a `WRITE`-only dst mask on `record_scratch_serialize_barrier`.
- **Evidence**:
  ```rust
  // blas_static.rs — build_blas_batched Phase 4: no pre-loop barrier
  let build_result = submit_one_time(device, queue, command_pool, transfer_fence, |cmd| {
      for (i, p) in prepared.iter().enumerate() {
          if i > 0 { self.record_scratch_serialize_barrier(device, cmd); }
          ...
  ```
  ```rust
  // blas_skinned.rs — the symmetric path DOES pre-emit (#1300 / D12B-1)
  if !prepared.is_empty() { self.record_scratch_serialize_barrier(device, cmd); }
  for (i, p) in prepared.iter().enumerate() {
      if i > 0 { self.record_scratch_serialize_barrier(device, cmd); }
  ```
  ```rust
  // skinned_blas_refit.rs — the only thing standing between the two directions
  memory_barrier(&self.device, cmd,
      vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
      vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
      vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
      vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR);  // dst access: READ only
  ```
  The window is named by the codebase itself, in `build_blas_batched`'s own #1782 comment: *"This is
  the M40 streaming hot path (called from `step_streaming` in `about_to_wait`), the exact window
  where the previously-submitted frame's skinned-BLAS refit/first-sight command buffer may still be
  executing on the GPU."* #1782 fixed the *destroy* of the old buffer in that window; the *reuse* of
  the same buffer when no grow is needed was not part of that fix. I confirmed no `device_wait_idle`
  or fence wait exists anywhere on the `byroredux` streaming → cell-load → `build_blas_batched` path.
- **Impact**: If the `AS_READ`-only dst mask is insufficient, a cell-load BLAS build overlapping an
  in-flight skinned-BLAS refit corrupts the shared scratch region for one or both builds — a
  malformed BVH, which surfaces as wrong or missing shadows / reflections / GI on the affected
  meshes, or a GPU fault in the worst case. That matches the "BLAS/TLAS build with wrong geometry"
  severity row. It is intermittent and driver-scheduling-dependent, which is exactly why this must
  be confirmed before being fixed rather than fixed on reasoning.
- **Trigger Conditions**: A frame that (a) records at least one skinned-BLAS build or refit into the
  per-frame `cmd` (any visible NPC with a live skin slot), followed by (b) `step_streaming` in the
  same `about_to_wait` reaching `build_blas_batched` or `build_blas` with `need_new_scratch == false`
  (the existing scratch is already big enough — the steady-state case), while (c) the GPU has not yet
  retired the frame submission. Highest-probability repro: walking across an exterior cell boundary
  with NPCs on screen, at a frame rate where the GPU is ~1 frame behind.
- **Verification Path**: Not observable in `cargo test` — no headless device, and the existing
  `scratch_barrier_*` tests only pin the *predicate*, never the emitted masks on the static path.
  **Validation layer is the check**: a `BYRO_VALIDATION=1` **release** run streaming an exterior grid
  with NPCs, watching for Synchronization Validation `WRITE_AFTER_WRITE` on the
  `blas_scratch_buffer` allocation at `vkCmdBuildAccelerationStructuresKHR`. Caveat worth stating:
  `predicates.rs`'s own comment asserts *"validation layers do NOT catch it because they reason
  per-submission"* — that was written in the #983 era; current Synchronization Validation does
  maintain per-queue submission history and does report cross-submission hazards, so the run is
  worth doing, but a clean run is weaker evidence here than usual. If the layer stays silent,
  RenderDoc's resource-usage view on `blas_scratch_buffer` across the two submissions is the
  fallback. **Absent one of those signals, do not change the barrier** — the cheap, spec-safe move
  (a pre-loop `record_scratch_serialize_barrier` in both static paths, exactly mirroring #1300) is
  still a barrier change and falls under the guardrail.
- **Related**: #1300 (CLOSED — the identical gap on the skinned batched builder's `i == 0`, fixed by
  a pre-loop self-emit), #983 / #644 (CLOSED — the `refit_skinned_blas` self-emit and the original
  cross-submission scratch bug), #1782 (CLOSED — the *destroy* half of this same window),
  #1790 (CLOSED — the symmetric dst-access-mask question, answered in the other direction),
  #1797 (CLOSED — the throughput cost of the shared scratch, and the standing decision not to
  sub-allocate it).
- **Suggested Fix** (only after the signal above): mirror #1300 — emit
  `record_scratch_serialize_barrier` once before the first build in both `build_blas` and
  `build_blas_batched`'s `submit_one_time` closures, and extend `ScratchUser` with the reverse
  direction (e.g. a `CrossSubmissionRefitStillInFlight` variant) so
  `requires_scratch_serialize_barrier_before` and its unit tests pin both directions rather than one.
  The barrier is idempotent and same-stage, so the cost on a queue with no in-flight AS work is
  negligible.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CONCURRENCY_2026-08-14.md`](docs/audits/AUDIT_CONCURRENCY_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
