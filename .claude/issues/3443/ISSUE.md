# Issue #3443: REN-2026-08-27-D5-01: #3298 chunked geometry rebuild allocates a second full-size generation at any size, routing around the GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES guard #2374 added for exactly that case

**Filed**: 2026-08-27 via /audit-publish from `docs/audits/AUDIT_RENDERER_2026-08-27.md`

**Severity**: HIGH
**Dimension**: Memory/Lifecycle
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-27.md` (REN-2026-08-27-D5-01)

## Location
- `crates/renderer/src/mesh.rs` — `rebuild_geometry_ssbo` (~:1208-1288)
- `crates/renderer/src/mesh.rs` — `try_allocate_empty_geometry_buffers` (~:1309)
- `crates/renderer/src/mesh.rs` — `geometry_rebuild_needs_idle` (~:222-227), `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES` (:42)
- `crates/renderer/src/mesh.rs` — `rebuild_geometry_ssbo_atomic_fallback` (~:1507-1521), its only live caller

## Description
`GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES` exists because, per its own doc, *"Large global-geometry rebuilds cannot safely keep two prior SSBO generations alive while allocating the replacement on mid-range GPUs. Above 256 MiB, prefer a one-time device-idle reclamation over a recoverable allocation failure escalating into `VK_ERROR_DEVICE_LOST` (FO4 boundary traversal, #2374)."*

After #3298, `rebuild_geometry_ssbo` no longer consults that predicate at all on its primary path: whenever a prior generation exists it calls `try_allocate_empty_geometry_buffers` for the **full projected size**, unconditionally, and holds both generations for the whole multi-frame copy. `geometry_rebuild_needs_idle` is now reachable only from `rebuild_geometry_ssbo_atomic_fallback`, which itself is only reached *after* that allocation has already returned `Err`.

The threshold's premise is that on a constrained device the double allocation may **succeed** (driver-managed residency / system-memory spill) and escalate to device loss under later pressure — a path a post-hoc `Err` check cannot catch.

## Evidence
```rust
// mesh.rs — no size gate anywhere on this arm
if has_existing_buffers {
    let rt_usage = …;
    match Self::try_allocate_empty_geometry_buffers(
        device, allocator, vertex_size, index_size, rt_usage,
    ) {
        Ok((new_vertex_buffer, new_index_buffer)) => {
            self.geometry_rebuild = Some(GeometryRebuildInProgress { … });
```
versus the only site that still asks the question, inside the fallback:
```rust
let reclaim_before_rebuild =
    geometry_rebuild_needs_idle(projected_bytes, has_existing_buffers);
```
`GeometryRebuildInProgress`'s own doc states the trade-off plainly — *"two full geometry SSBO generations are resident in device-local memory at once for the rebuild's duration"* — and names the #2374 path as *"a fallback, not the common case"*, i.e. the inversion is intentional; what is missing is any size condition on it.

## Impact
On the 12 GB dev card this is invisible (the FO4 boundary case is ~800–900 MiB duplicated against ~9 GB of headroom). On a 6 GB card — the documented RT minimum in `feedback_vram_baseline.md` — an FO4/Skyrim boundary crossing now transiently doubles the single largest non-texture allocation class on top of a ~1.7 GB steady state, in exactly the scenario #2374 was filed for. The blast radius if it lands is `VK_ERROR_DEVICE_LOST`, which is unrecoverable. A *second* rebuild starting while the previous generation is still in the `DEFAULT_COUNTDOWN` deferred-destroy queue can put three generations in flight.

## Related
#2374 (CLOSED — the guard this bypasses), #3298 (CLOSED — the landing), #3372 (CLOSED — the sibling correctness fix on the same feature), `feedback_vram_baseline.md`. Distinct from `SAFE-2026-08-27-01`, which covered the compacted-offset publish, not headroom.

## Suggested Fix
Gate the chunked path on `!geometry_rebuild_needs_idle(projected_bytes, has_existing_buffers)` so rebuilds at or above 256 MiB take the atomic idle-reclaim route #2374 specified, and only sub-threshold rebuilds duplicate. If the intent is that the chunked path should supersede the threshold outright, that reversal needs its own evidence on a memory-constrained device, and `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES`' doc comment should be rewritten rather than left asserting a rule the primary path no longer follows.

## Verification note
Source-read confidence only — no Vulkan device, RenderDoc capture, or `BYRO_VALIDATION` run was available to the audit. Whether a mid-range driver satisfies the second full-size `GpuOnly` allocation (rather than returning `Err`) is a driver-residency question; #2374's own text asserts it does.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other buffer-rebuild paths, deferred-destroy queues)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
