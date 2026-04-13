# #284: C2-03..C2-04, C3-05, C4-01 — Concurrency LOW findings bundle

**Severity**: LOW | **Domain**: renderer, ecs | **Type**: enhancement

## Findings
- C2-03: graphics_queue and present_queue alias same VkQueue under separate Mutex (`context/mod.rs:130-134`)
- C2-04: BLAS build blocks main thread via one-time commands (`acceleration.rs:222-232`)
- C3-05: StagingPool Drop warns but doesn't destroy GPU resources (`buffer.rs:29-213`)
- C4-01: Thread-local lock tracker can't detect cross-thread ABBA deadlocks (`lock_tracker.rs:20`)
