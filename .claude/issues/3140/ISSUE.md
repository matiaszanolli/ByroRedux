# SAFE-2026-08-20-05: VulkanContext::drop's hoisting comment still says the water teardown needs no allocator — it has owned an Arc clone of the shared allocator since the param UBOs landed

**Issue**: #3140 — https://github.com/matiaszanolli/ByroRedux/issues/3140
**Finding**: `SAFE-2026-08-20-05`
**Labels**: documentation, low, safety
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_SAFETY_2026-08-20.md` — Dimension 3 (memory & resource leaks — drop ordering)
**Severity**: LOW · **Status**: NEW

## Location
- `crates/renderer/src/vulkan/context/mod.rs:3837-3841` — the stale note
- `crates/renderer/src/vulkan/context/mod.rs:3920-3922` — the hoisted call
- Ground truth: `crates/renderer/src/vulkan/water.rs:255-261` and `:719-727`

## Description
#1483 moved `water.destroy()` into `Drop`'s allocator-*independent* block, with a comment explaining why that is safe: "its pipeline + caustic descriptor pool need no allocator." That was true then.

Commit `ed3570ad` subsequently gave `WaterPipeline` a `Vec<GpuBuffer>` of per-FIF host-visible parameter UBOs, and with it an `allocator: Option<SharedAllocator>` field, precisely so it could still free them from that block. The mechanism is correct and its own field doc says so — but the `Drop`-side comment a reader hits first now describes a subsystem that no longer exists, and it is exactly the comment someone consults before reordering teardown.

The ordering is currently sound and worth recording as such: `destroy()` frees the UBOs and then sets `self.allocator = None`, dropping its `Arc` clone; that happens at `:3921`, far above the `Arc::try_unwrap` at `:4040`, so the strong count is released in time and the #665 / LIFE-L1 leak-instead-of-use-after-free fallback is not engaged.

## Evidence
```rust
// context/mod.rs:3837-3841 — stale
// NOTE: `self.water` teardown hoisted to the
// allocator-independent block near the top of Drop
// (#1483) — its pipeline + caustic descriptor pool need no
// allocator. The per-FIF `water_caustic_accum` images
// below DO need the allocator and stay here.
```
```rust
// water.rs:255-261 — the field that contradicts it
/// Retained so the allocator-independent context teardown can still
/// release these buffers before the allocator is unwrapped.
allocator: Option<SharedAllocator>,
```
```rust
// water.rs:719-727 — allocator-dependent work inside the "allocator-independent" destroy
if let Some(allocator) = self.allocator.as_ref() {
    for buffer in &mut self.param_buffers { buffer.destroy(device, allocator); }
}
self.allocator = None;
```
Both sites verified verbatim at HEAD.

## Impact
Documentation only today. The hazard it creates is specific: a future reader trusting the note could conclude `WaterPipeline` holds no allocator reference and either

(a) skip `destroy()` on some new early-return path, stranding an `Arc` clone that makes `Arc::try_unwrap` fail and pushes teardown into the leak-the-device fallback; or
(b) reorder the hoisted block after the allocator is taken, which would strand the UBO allocations outright.

## Related
#1483 (the hoist), #665 / LIFE-L1 (the `try_unwrap` fallback the stale note could route teardown into), #732 / LIFE-N1 (the same "drop the `GpuBuffer` structs so their `Arc` clones release now" pattern, correctly applied and commented in `volumetrics.rs:2695-2701`). Adjacent: SAFE-2026-08-20-02 (the same param UBOs, sized wrong against the spec floor) and SAFE-2026-08-20-04 (the same constructor's degradation arm).

## Suggested fix
Rewrite the note to say the water teardown is hoisted because `WaterPipeline` carries its **own** `SharedAllocator` clone and can therefore free its param UBOs without the context's, and that `destroy()` must stay ahead of the `Arc::try_unwrap` at `:4040`. One sentence.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct — the rewritten note must state the real precondition (`destroy()` before `Arc::try_unwrap`), not just describe the move
- [ ] **SIBLING**: Every other subsystem hoisted into the allocator-independent block re-checked for the same "acquired an allocator clone since the hoist" drift
