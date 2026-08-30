# #3588 — REN-2026-08-30-D5-06: `destroy_depth_capture_staging`'s SAFETY comment names a caller set that does not exist

**Labels**: `low,renderer,memory,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3588 --json state`.

---

- **Severity**: Low
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/depth_capture.rs:299-305`
- **Status**: Open — new in this delta.
- **Description**: The `unsafe { destroy_buffer }` inside
  `destroy_depth_capture_staging` is justified as: *"callers are the resize path in
  `ensure_depth_capture_staging` (between frames, before any copy is recorded
  against the new buffer) and shutdown teardown (after `device_wait_idle`)"*. Two
  of the three claims are wrong. `ensure_depth_capture_staging` is called only from
  `depth_capture_record_copy` (`depth_capture.rs:134`), which runs **during**
  command-buffer recording at `draw.rs:3684`, not between frames; and there is no
  resize call site at all — `recreate_swapchain` never touches depth-capture
  staging (`grep -n depth_capture crates/renderer/src/vulkan/context/resize.rs` is
  empty). The destroy *is* sound, but for a different reason: `draw_frame` waits
  **both** FIF fences at `draw.rs:1628-1640` before any recording, so no submitted
  copy can still target the buffer being freed.
- **Evidence**:
  - `depth_capture.rs:132-136`: `self.ensure_depth_capture_staging(buffer_size);`
    inside `unsafe fn depth_capture_record_copy`.
  - `depth_capture.rs:238-242`: `ensure_depth_capture_staging`'s only
    `destroy_depth_capture_staging()` call, on the grow branch.
  - `draw.rs:1628-1640`: `wait_for_fences(&[in_flight[frame], in_flight[prev]], true, u64::MAX)`.
- **Impact**: The recorded justification for an `unsafe` free points at a call site
  that does not exist and mis-states the timing of the one that does. The real
  invariant is the both-slot fence wait — which #3442 already flags as pinned by
  nothing that can see `draw.rs`'s `(f + 1) % MAX_FRAMES_IN_FLIGHT`. So the one
  correct reason is also the one currently unguarded, and this comment points away
  from it.
- **Suggested Fix**: Rewrite the SAFETY block to name the two real callers
  (`ensure_depth_capture_staging`'s grow branch, during recording, and
  `VulkanContext::drop`) and to cite `draw_frame`'s both-slot fence wait as the
  property that makes the recording-time free sound — the same invariant the
  screenshot sibling depends on.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D5-06

## Dedup cross-reference

The real invariant this SAFETY block should cite (`draw_frame`'s both-slot fence wait) is
the same one **#3442** flags as unpinned. Worth fixing in the same pass.


## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
