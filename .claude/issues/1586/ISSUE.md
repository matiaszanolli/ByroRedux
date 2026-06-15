**Severity**: MEDIUM · **Dimension**: Streaming & Cells · **Status**: NEW
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-06-14.md` (F7)

## Description
The #877 two-phase pre-parse correctly moves NIF *parse* off-thread, but the drain that consumes those payloads is an uncapped `loop { try_recv() }` (`byroredux/src/main.rs:1071-1084`) and each iteration runs `consume_streaming_payload` (`streaming_helpers.rs:117-126`) → `load_one_exterior_cell` (`cell_loader/exterior.rs:319-342`), which synchronously spawns the terrain mesh, submits a batched BLAS build (`ctx.build_blas_batched`, `exterior.rs:320`), spawns water, decodes + spawns precombines (F6), and uploads vertex/index buffers. No per-frame cell budget: if N worker payloads are ready at frame start, all N cells spawn (with all GPU work) before the frame proceeds. The *pre-parse* split is intact; the *spawn* phase has no throttle.

## Evidence
Verified live (`main.rs:1071-1084`): `loop { let payload_opt = ...try_recv().ok(); let Some(payload) = payload_opt else { break }; consume_streaming_payload(...); }` — no cap, no budget; loop only exits when the channel is drained empty.

## Impact
Frame-time spike when >1 payload completes in one frame — realistic on fast-travel/teleport (full new batch dispatched), post-stall catch-up (worker ran ahead), or larger `radius_load`. Steady-state at `radius_load=1` is mild (MEDIUM, not HIGH). On-disk-data + Vulkan-device → smoke-only, out of `cargo test`.

## Related
F6 (CSG open runs inside this loop); BLAS batching (Dim 3).

## Suggested Fix
Cap the steady-state drain at 1–2 cells/frame and leave the rest queued (`break` after the cap — `try_recv` makes this trivial). Spreads spawn/upload/BLAS cost across frames at the price of slightly later pop-in, which the hysteresis radius already tolerates. Keep `stream_initial_radius`'s blocking boot path unchanged.

## Completeness Checks
- [ ] **SIBLING**: Confirm the initial-radius blocking boot path is left uncapped intentionally and any other drain site honors the same budget
- [ ] **TESTS**: Pin that ≤ N cells spawn per `step_streaming` call when many payloads are queued
