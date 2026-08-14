# PERF-D3-01: Swapchain recreate zeroes the frame counter the skinned-BLAS LRU measures idleness against

- **Issue**: [#2925](https://github.com/matiaszanolli/ByroRedux/issues/2925)
- **Finding ID**: `PERF-D3-01`
- **Labels**: `medium,performance,memory,renderer,bug`
- **Source report**: [`docs/audits/AUDIT_PERFORMANCE_2026-08-14.md`](../../../docs/audits/AUDIT_PERFORMANCE_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2925 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` (`recreate_screen_passes`, the `self.frame_counter = 0;` reset), `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (`record_skinned_blas_refit` — the `let now = self.frame_counter as u64;` sweep and the `slot.last_used_frame = self.frame_counter as u64;` stamp), `crates/renderer/src/vulkan/skin_compute.rs` (`should_evict_skin_slot`)
- **Status**: NEW
- **Description**: `VulkanContext::frame_counter` is a single `u32` serving two unrelated roles. It drives the Halton TAA jitter index, and #913 therefore resets it to `0` on every swapchain recreate so the first post-resize frame's jitter aligns with the freshly-allocated TAA history. It is *also* the clock the M29 `SkinSlot` / skinned-BLAS LRU sweep uses: `SkinSlot.last_used_frame` is stamped from it, and the sweep computes idleness as `current_frame.saturating_sub(last_used_frame)`. After a reset, every resident slot carries a `last_used_frame` from the pre-reset epoch, so the subtraction saturates to `0` and `should_evict_skin_slot` returns `false` for all of them. Nothing in `recreate_swapchain_core` / `recreate_screen_passes` re-bases `SkinSlot.last_used_frame`. Slots whose entity is still drawn re-stamp themselves on the next frame and self-heal; slots whose entity stops being drawn around the resize are never re-stamped and become **un-evictable until `frame_counter` climbs back past their stale stamp** — i.e. for as many frames as had already elapsed in the session.
- **Evidence**:
  - `resize.rs`, inside `recreate_screen_passes`: `self.frame_counter = 0;` with a comment scoped entirely to Halton jitter + TAA history (#913 / REN-D7-NEW-07). No other consumer is mentioned.
  - `skinned_blas_refit.rs`: `let min_idle = MAX_FRAMES_IN_FLIGHT as u64 + 1; let now = self.frame_counter as u64;` then `should_evict_skin_slot(slot.last_used_frame, now, min_idle)`.
  - `skin_compute.rs`: `let idle = current_frame.saturating_sub(last_used_frame); idle >= min_idle` — saturating, so a stamp in the future yields `0`, never a large value.
  - `draw.rs` is the sole bump site (`self.frame_counter = self.frame_counter.wrapping_add(1);`, once per `draw_frame`), so the counter is genuinely a frame clock everywhere else.
  - Contrast: `AccelerationManager::frame_counter` (the static-BLAS LRU clock) is a *separate* field and is **not** reset by resize — static BLAS eviction is unaffected, which is why this is a skinned-path-only defect.
  - Secondary, self-healing: a slot dispatched on the very first post-reset frame gets `last_used_frame == 0`, which `should_evict_skin_slot` treats as the "never dispatched" sentinel and skips.
- **Impact**: GPU-resource retention proportional to "skinned actors alive but off-screen at resize time" × session length. Each stranded entry is a `SkinSlot` (output buffer at `SKIN_OUTPUT_STRIDE_BYTES` × vertex count, plus `MAX_FRAMES_IN_FLIGHT` descriptor sets from the `FREE_DESCRIPTOR_SET` pool) plus its per-entity skinned BLAS. This does not compound per frame and is bounded by the actor population, so it is not a HIGH-severity leak — but the pressure it applies lands on the two ceilings that already have observed failure modes: `SKIN_MAX_SLOTS` (#1284 — exhaustion drops actors to bind-pose with no RT shadows) and the skin descriptor pool (#900). Note also that `pending_skin_unload_victims` (#1003) still drains correctly, so *despawned* entities are unaffected; only the idle-policy arm stalls. Resizes are not rare on this path — window resize, and `set_upscaler_mode` (FSR preset change) both reach the reset.
- **Related**: #913 (introduced the reset, for TAA only), #643 / MEM-2-1 (introduced the sweep), #2494 (hoisted the sweep out of the vertex-buffer guard), #1003 (`pending_skin_unload_victims`), #1284 / #900 (the ceilings this pressures). No existing OPEN or CLOSED issue covers the interaction.
- **Suggested Fix**: Give the LRU sweep its own monotonic counter that `recreate_*` never touches (mirroring `AccelerationManager::frame_counter`), or — cheaper — re-base every `SkinSlot.last_used_frame` to `0`… `min_idle` at the reset site so the sweep sees them as immediately-agable rather than future-stamped. Whichever is chosen, add a note at the `self.frame_counter = 0;` line naming the second consumer, since the current comment actively implies TAA is the only one.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_PERFORMANCE_2026-08-14.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
