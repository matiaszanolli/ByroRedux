# REN-D1-03: Two latent defects in shrink_tlas_scratch_to_fit's live-slot arm (one a draw_frame panic)

- **Issue**: [#2915](https://github.com/matiaszanolli/ByroRedux/issues/2915)
- **Finding ID**: `REN-D1-03`
- **Labels**: `low,renderer,vulkan,memory,bug`
- **Source report**: [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](../../../docs/audits/AUDIT_RENDERER_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2915 --json state`.

---

- **Severity**: LOW (currently unreachable; would be HIGH if the arm were reached)
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs` (`shrink_tlas_scratch_to_fit`, case-2 live-slot arm), `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas` scratch-address query, `ensure_tlas_state`)
- **Status**: NEW — distinct defects, blocked behind OPEN **#2774** ("`shrink_tlas_scratch_to_fit` case-2 live-slot realloc arm appears unreachable"), which covers only the reachability question
- **Description**: Independent static analysis confirms #2774's premise — the live-slot arm cannot
  fire, because `tlas_scratch_peak_bytes[slot]` and `scratch_buffers[slot]` are written in the same
  `ensure_tlas_state` block, leaving `current - peak == scratch_alignment_padding(scratch_align)`
  (≤ 255 B) while `tlas_scratch_should_shrink` needs `current > 2 × peak` **and**
  `current - peak > TLAS_SCRATCH_SLACK_BYTES` (256 KB). `ensure_tlas_state` only ever grows
  (`max_instances < instance_count`), so the recorded peak never regresses below the allocated
  capacity. The arm nonetheless contains two defects that a "make it reachable" resolution of
  #2774 would ship:
  1. **Missing alignment headroom.** The realloc target is the bare `peak`
     (`create_device_local_uninit(device, allocator, peak, …)`), unlike the BLAS sibling
     `shrink_blas_scratch_to_fit`, which uses `peak.saturating_add(scratch_alignment_padding(self.scratch_align))`
     and documents exactly why. `build_tlas` then rounds the buffer's device address up via
     `align_scratch_address` before submitting — on a driver whose `GpuOnly` addresses are not
     already `minAccelerationStructureScratchOffsetAlignment`-aligned, the build's scratch range
     runs past the allocation by up to `align - 1` bytes. It is **not** self-correcting on this
     path: `scratch_needs_growth` is consulted only inside `ensure_tlas_state`'s `need_new_tlas`
     block, which may not run for many frames.
  2. **Destroy-then-allocate.** The arm takes and destroys the old buffer *before* attempting the
     replacement; on `Err` it logs a warn and leaves `scratch_buffers[slot] = None` **with
     `tlas[slot]` still `Some`**. The next `build_tlas` for that slot finds
     `max_instances >= instance_count`, so `ensure_tlas_state` returns early without allocating
     scratch, and `build_tlas` reaches
     `self.scratch_buffers[frame_index].as_ref().unwrap()` → panic inside an open command-buffer
     recording. The arm's own comment ("the next build's `scratch_needs_growth(None, …)` arm will
     re-allocate. Degraded but correct") states the case-1 behaviour, which is correct only
     because case 1 also leaves `tlas[slot] == None`. This is the exact hazard #2673 called out
     and fixed in `ensure_tlas_state` — the sibling site was not converted.
- **Evidence**:
  - `memory.rs`, case 2: `if let Some(mut old) = self.scratch_buffers[slot_index].take() { old.destroy(…); } match GpuBuffer::create_device_local_uninit(device, allocator, peak, …) { Ok(new_buf) => …, Err(e) => { log::warn!(…); true } }`.
  - `memory.rs`, BLAS sibling for contrast: `let target = peak.saturating_add(scratch_alignment_padding(self.scratch_align));`.
  - `tlas.rs`, `ensure_tlas_state` scratch allocation: `let scratch_size = sizes.build_scratch_size + scratch_alignment_padding(self.scratch_align);` versus the peak record `self.tlas_scratch_peak_bytes[frame_index] = sizes.build_scratch_size;` (unpadded).
  - `tlas.rs`, `build_tlas`: the unconditional `self.scratch_buffers[frame_index].as_ref().unwrap()`, with `ensure_tlas_state`'s scratch allocation nested inside `if need_new_tlas`.
  - `tlas.rs`, #2673's own comment already names this failure mode for its own site: *"had a later frame found the (now smaller) instance count fitting an existing TLAS, `build_tlas`'s `scratch_buffers[..].unwrap()` would have panicked on the missing scratch."*
  - `AUDIT_RENDERER_2026-08-12b.md` asserts the padding question is "self-correcting"; that holds for the BLAS path (which has a growth check on every build) but not for this one.
- **Impact**: None today. If #2774 is resolved by recalibrating the predicate rather than deleting
  the arm, defect 2 becomes a hard process abort mid-`draw_frame` under the exact VRAM-pressure
  regime the shrink exists to relieve, and defect 1 becomes a latent AS build-scratch overrun on
  a misaligning driver.
- **Related**: #2774 (OPEN, reachability), #2673 (CLOSED — allocate-then-swap, applied to
  `ensure_tlas_state` only), #1386 / #659 (scratch alignment padding), #1226 (TLAS-calibrated
  slack), #2460.
- **Suggested Fix**: Whoever closes #2774 should decide first: if the arm is deleted, both
  defects go with it. If it is kept, mirror `shrink_blas_scratch_to_fit` — add
  `scratch_alignment_padding` to the target and allocate into a local before retiring the old
  buffer — and make `build_tlas` tolerate a missing scratch (allocate on demand or bail with
  `Err`) rather than `unwrap`.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](docs/audits/AUDIT_RENDERER_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
