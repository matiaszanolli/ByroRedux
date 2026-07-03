# #1867: CONC-D1-NEW-01: bind_inverses requeue/rollback skipped on fatal queue_submit/queue_present Err path

- **Severity**: LOW (informational, negligible impact — no fix recommended)
- **Labels**: `low`, `sync`, `bug`
- **Source**: `docs/audits/AUDIT_CONCURRENCY_2026-07-03.md` (CONC-D1-NEW-01)
- **Dimension**: Vulkan Queue & AS Sync / Compute → AS → Fragment Chains

## Location
- `byroredux/src/main.rs:~1863-1867`
- `crates/renderer/src/vulkan/context/draw.rs:~3735` (queue_submit failure) / queue_present failure path

## Description
The #1791/#1796 requeue-and-rollback logic only runs on `draw_frame`'s `Ok` arm. On a fatal `Err` arm (which triggers `event_loop.exit()` the same tick), the bind_inverses requeue and pose-hash rollback never fire.

## Impact
One-shot CPU-side bookkeeping loss on a path that's already fatal — the engine is tearing down. No use-after-free, no data race, no persistent corruption.

## Suggested Fix
None recommended by the audit — flagged for completeness only. If ever addressed: run rollback/requeue unconditionally on both arms.
