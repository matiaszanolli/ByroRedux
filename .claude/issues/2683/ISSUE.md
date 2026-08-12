# SAFE-D4-01: GpuBuffer flush SAFETY comments assert facts that are false

**Issue**: #2683
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`

- **Severity**: MEDIUM
- **Dimension**: 4 (Unsafe-Block Discipline) — commented block, stated invariant false
- **Location**: [buffer.rs](crates/renderer/src/vulkan/buffer.rs) — `GpuBuffer::flush_if_needed` (SAFETY at :768-771, block at :772), `GpuBuffer::write_mapped`'s flush (SAFETY at :826-832, block at :833), `GpuBuffer::flush_range` (SAFETY at :870-872, block at :873); helper `aligned_flush_range` at :506-516
- **Status**: NEW
- **Description**: Three `vkFlushMappedMemoryRanges` call sites justify their
  `unsafe` with claims about `aligned_flush_range`'s output that the function
  demonstrably does not produce.
  1. `flush_if_needed`: *"The range is contained within this allocation's slice
     of the parent VkDeviceMemory: offset is rounded down and size rounded up to
     nonCoherentAtomSize, which gpu-allocator already pads sub-allocations to."*
     Rounding the offset **down** by up to 255 B moves the range's start *before*
     the allocation, and rounding the size **up** moves its end *after* it. By
     construction the flushed range is a strict superset of the allocation, not a
     subset — the sentence contradicts itself.
  2. The `gpu-allocator already pads sub-allocations to [nonCoherentAtomSize]`
     claim is false about the dependency. The workspace pins
     `gpu-allocator = "0.28"` (`Cargo.toml:74`); the vendored source at
     `~/.cargo/registry/src/index.crates.io-*/gpu-allocator-0.28.0/src` contains
     **zero** occurrences of `non_coherent` / `atom_size` — it sub-allocates on
     `VkMemoryRequirements.alignment` only and has no `nonCoherentAtomSize`
     awareness at all (unlike VMA, which does clamp `vmaFlushAllocation`).
  3. `write_mapped`: *"aligned_size is rounded up to atom size but capped at the
     written length"*. `aligned_flush_range` never caps — `aligned_size >= extra + size`
     unconditionally. There is no clamp anywhere on the path.
- **Evidence**:
  ```rust
  // buffer.rs:506
  fn aligned_flush_range(offset, size) -> (DeviceSize, DeviceSize) {
      let aligned_offset = offset & !(NON_COHERENT_ATOM_SIZE - 1);   // rounds DOWN, out of the alloc
      let extra = offset - aligned_offset;
      let aligned_size = (extra + size + N - 1) & !(N - 1);          // rounds UP, no clamp
      (aligned_offset, aligned_size)
  }
  // buffer.rs:767 — the whole allocation, then rounded outward past both ends
  let (aligned_offset, aligned_size) = aligned_flush_range(alloc.offset(), alloc.size());
  ```
  Existing tests confirm the widening is intentional: `aligned_flush_range_unaligned_offset`
  asserts `off == 0` for input offset 100, and `aligned_flush_range_small_allocation`
  asserts a 48 B allocation flushes 256 B.
- **Impact**: Not unsound today: every call site uses
  `AllocationScheme::GpuAllocatorManaged` (verified — 20/20 `AllocationScheme`
  occurrences in `crates/renderer/src` are `GpuAllocatorManaged`), so each
  allocation is a sub-range of a multi-MB gpu-allocator block whose size is a
  multiple of 256, and the widened range stays inside the parent
  `VkDeviceMemory`. Flushing a neighbouring sub-allocation's bytes only
  publishes host writes that were already made; it does not corrupt.
  The risk is the *comment*: it tells a future reader the range is bounded and
  that the dependency guarantees the padding, so a refactor that (a) switches a
  host-visible buffer to `AllocationScheme::DedicatedBuffer`, or (b) grows one
  past gpu-allocator's 64 MB host-visible block size — which forces a dedicated
  block sized exactly `VkMemoryRequirements.size` — would put `offset + size`
  past the end of the memory object and violate
  VUID-VkMappedMemoryRange-size-01389 with no guard and no test to catch it.
  Per the No-Speculative-Vulkan-Fixes rule, that escalation is stated as a
  hazard, **not** as a confirmed bug: **needs validation-layer verification**
  before any claim that a VUID is currently being violated.
- **Related**: #1759 / TD7-002 (`NON_COHERENT_ATOM_SIZE` power-of-two pin +
  the `device.rs` `debug_assert` on the reported atom size — both intact and
  verified); the closed doc-accuracy siblings #2545 / #2546.
- **Suggested Fix**: Rewrite the three SAFETY comments to state what is
  actually true — "the range is widened outward past the sub-allocation and is
  bounded only by the parent gpu-allocator block, which is a multiple of
  `NON_COHERENT_ATOM_SIZE`" — and add a `debug_assert!` that
  `aligned_offset + aligned_size <= <parent block size>` (or clamp the tail to
  the allocation end when the allocation is the block, i.e. the dedicated case)
  so the dedicated-allocation refactor fails loudly instead of silently.

---


---
*Filed from [`docs/audits/AUDIT_SAFETY_2026-08-12.md`](docs/audits/AUDIT_SAFETY_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `SAFE-D4-01`.*

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
