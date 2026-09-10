# #4089 — INC-2026-09-09-04

`GpuImage` handles a poisoned allocator mutex two different ways inside the one type whose stated purpose is to state the allocator-lock rules once

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_INCREMENTAL_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4089 --json state`).

---

- **Severity**: LOW
- **Dimension**: renderer resource lifecycle (`/audit-renderer`), `/audit-safety`
- **Location**: [`crates/renderer/src/vulkan/image.rs:188-190`](../../crates/renderer/src/vulkan/image.rs) vs [`crates/renderer/src/vulkan/image.rs:289-302`](../../crates/renderer/src/vulkan/image.rs)
- **Changed in**: `crates/renderer/src/vulkan/image.rs` (commit `bb843dad`)
- **Status**: NEW. Dedup: no issue matches; #3860 is the parent refactor issue and
  describes the consolidation, not this. Verified against code at HEAD.
- **Description**: `GpuImage`'s module doc says the allocator-lock rules "now live here
  once". They are stated twice, differently. The allocation path panics on a poisoned
  mutex (`.expect("allocator lock")`, inherited verbatim from the `taa.rs` copy it
  replaced); the free path — added new in this commit — deliberately recovers from
  poisoning via `into_inner()`. So the same `Mutex<Allocator>`, in the same type, aborts
  the process when locked to allocate and is recovered when locked to free.
- **Evidence**:
  ```rust
  // create — panics on poison
  let allocation = match allocator
      .lock()
      .expect("allocator lock")
      .allocate(&vk_alloc::AllocationCreateDesc { … })
  ```
  ```rust
  // free_allocation — recovers from poison
  match allocator.lock() {
      Ok(mut guard) => { if let Err(e) = guard.free(allocation) { log::error!(…); } }
      Err(poisoned) => { if let Err(e) = poisoned.into_inner().free(allocation) { log::error!(…); } }
  }
  ```
- **Impact**: no behaviour change versus the copies it replaced (this is preserved, not
  introduced). It matters because this is the consolidation site: whichever rule stands
  here is the one all fourteen migrations inherit, and the migrations are landing now
  (nine had already landed by the end of this audit). A poisoned allocator means another
  thread panicked mid-allocation; the free path treats that as recoverable and the
  create path turns it into a second panic during teardown.
- **Related**: #3860, #1163, #1165, #927.
- **Suggested Fix**: pick one and say why in the doc. Given `free_allocation` already
  models poison as recoverable, `create` should use the same `Ok/Err(poisoned)` shape and
  return an `anyhow::Error` on the poisoned path rather than panicking — that keeps a
  failed allocation on the same recoverable path as every other `create` error arm.

---
