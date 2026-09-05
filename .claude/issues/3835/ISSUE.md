# #3835: PERF-D1-2026-09-05-02: `append_combustion_surface_lights` decodes and zeroes the whole 8 KB moment buffer before its own `had_grid` early-out, so every frame in every fog-free scene pays for it

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D1-2026-09-05-02) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3835 --json state`.

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D1-2026-09-05-02), published from `/audit-suite volumetrics-deep`. Premise re-verified against HEAD at publish time.

> Note: `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:2484-2505`; call site `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs:89-98`
- **Status**: NEW
- **Description**: The call site is unconditional — invoked whenever
  `self.volumetrics.is_some()`, i.e. every frame of every scene once the
  pipeline constructs, independent of whether `record_volumetrics_pass`
  actually dispatched. Inside, the function runs `invalidate_if_needed` →
  256-bin decode (`decode_combustion_light_moment`, 2,048 word reads) →
  8,192-byte `fill(0)` → `flush_if_needed`, and only *then* checks
  `if !had_grid { return Ok(0); }`. When no volumetrics dispatch ran for this
  slot, the decoded `moments` array is discarded unread and the buffer it
  just zeroed was already zero.
- **Evidence**:
```rust
// volumetrics.rs:2485-2505 — early-out at :2503, after the work
let had_grid = std::mem::take(&mut self.combustion_light_grid_valid[frame]);
let buffer = &mut self.combustion_light_moment_buffers[frame];
buffer.invalidate_if_needed(device)?;
let mut moments = [GpuCombustionLightMoment::default(); COMBUSTION_LIGHT_GRID_COUNT];
{
    let bytes = buffer.mapped_slice_mut()?;
    for (index, moment) in moments.iter_mut().enumerate() {   // 256 iterations
        *moment = decode_combustion_light_moment(&bytes[start..start + stride]);
    }
    bytes[..stride * COMBUSTION_LIGHT_GRID_COUNT].fill(0);    // 8,192 B
}
buffer.flush_if_needed(device)?;
if !had_grid {
    return Ok(0);
}
```
- **Impact**: A few microseconds per frame of decode + memset (the buffer is
  `HOST_CACHED` `GpuToCpu`, so reads are cheap — not a WC-read stall) plus up
  to two `vkInvalidateMappedMemoryRanges`/`vkFlushMappedMemoryRanges` calls,
  all for a discarded result. Small in absolute terms; flagged because it is
  unconditional across every game and every cell, and the fix is a two-line
  reorder.
- **Related**: PERF-D1-2026-09-05-01 (same file, same "unconditional
  whole-array traversal for a mostly-empty payload" shape).
- **Suggested Fix**: Hoist `if !had_grid { return Ok(0); }` above the
  invalidate/decode/fill block — the buffer's zero state is already
  maintained by construction and by every drain that did run.
- **Confidence**: High.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
