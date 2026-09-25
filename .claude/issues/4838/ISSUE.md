# #4838 — REN-D9-2026-09-24-02: `MorphSlot::destroy` can never release the shared `MorphDelta` — the `Weak` in `morph_delta_cache` defeats `Arc::get_mut`, so the final release falls through to the `GpuBuffer::Drop` safety net

**Labels**: bug,renderer,medium,vulkan,memory

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D9-2026-09-24-02**._

- **Severity**: MEDIUM. Cleanup contract broken on a recoverable path. Release builds self-heal through the #656 safety net; debug builds panic. No per-frame growth.
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/morph_compute.rs` — `MorphSlot::destroy` (`Arc::get_mut(&mut self.delta)`); insertion in `context/resources.rs` `create_morph_slot_for_mesh` (`Arc::downgrade(&delta)`); callers `context/skinned_blas_refit.rs` (morph eviction loop) and `context/teardown.rs`.
- **Status**: NEW. Introduced by #3661 (`9c4814f8a`, 2026-09-03); first reachable when #4399 (`d689e0c04`, 2026-09-20) made slot creation live on the loose-NIF/NPC route, which the 2026-09-21 report says "has not been validated live".
- **Description**: `Arc::get_mut` returns `None` unless there are no other `Arc` **or `Weak`** pointers. `create_morph_slot_for_mesh` always stores `Arc::downgrade(&delta)` on a cache miss, and the entry is only pruned later (`retain(strong_count != 0)`, after the eviction loop). So when `destroy` runs a `Weak` is always alive, `get_mut` is always `None`, and the delta's `destroy()` is never called. `MorphSlot` then drops and `GpuBuffer::drop` runs with the allocation still `Some`.
  - Debug builds: `log::warn!` then `debug_assert!(false, "GpuBuffer leaked into Drop: call destroy() first")`, which panics.
  - Release builds: the same WARN, then `destroy_buffer` + `allocator.free` from Drop, correct only because eviction runs after the both-slots fence wait and teardown after `device_wait_idle`.
- **Evidence**: A standalone reproduction of the create / downgrade / destroy sequence printed `strong=1 weak=1` / `Arc::get_mut -> None: explicit destroy() SKIPPED` / `Delta dropped`. `create_morph_slot_for_mesh`'s own error branch uses `Arc::try_unwrap`, which ignores `Weak`. The comments in `teardown.rs` and on `morph_delta_cache` state the false premise. Every morph mesh gets a dedicated handle on both spawn paths, so "last slot of this mesh" is the *only* case. `morph_slot_shares_deltas_but_keeps_entity_weights` only greps the `Arc::get_mut` line into existence, so it proves the bug line.
- **Impact**: Debug-build panic on cell unload of any NPC carrying morph targets, and on engine exit. In release, one WARN per evicted morph mesh plus reliance on the Drop-time free. No leak in release, no wrong rendering. Live behaviour not observed.
- **Related**: #3661, #4399, #4294, #3374, #656, #4599 (open, `.expect` on the allocator lock in Drop).
- **Suggested Fix**: In `MorphSlot::destroy`, take the `Arc` out and `Arc::try_unwrap` it (succeeds at strong == 1 regardless of `Weak`), then `delta.destroy(..)`, mirroring the error branch in `create_morph_slot_for_mesh`. Correct the two comments. Add a test around a small generic wrapper that holds a `Weak` while destroying.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
- [ ] **DROP**: If Vulkan objects change, the Drop impl / teardown ordering is still correct (destroy before `allocator.take()`)
