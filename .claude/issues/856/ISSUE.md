## Source Audit
`docs/audits/AUDIT_CONCURRENCY_2026-05-05.md`

## Severity / Dimension
LOW / Worker Threads (Streaming, Debug)

## Location
`byroredux/src/streaming.rs:150-158` (handle held in `Option`), `byroredux/src/main.rs:785` (`self.streaming.take()` drops the handle)

## Description
**Trigger Conditions**: Engine shutdown on `WindowEvent::CloseRequested` while the worker is mid-parse on a large cell.

`WorldStreamingState.worker: Option<JoinHandle<()>>` carries the worker's join handle. The comment at line 157 says "nothing currently calls `.take().join()` — holding the handle is the point." On shutdown (`main.rs:785: self.streaming.take()`), the `WorldStreamingState` is dropped, which drops `request_tx` (closing the channel) AND drops the `JoinHandle` (detaching the thread). The worker may still be inside a 100-300 ms `pre_parse_cell` call when the main thread exits. The worker holds an `Arc<TextureProvider>` and `Arc<ExteriorWorldContext>` — both get dropped when the worker thread exits, but the timing relative to `event_loop.exit() → main return → process exit` is racy.

## Evidence
No `.join()` call on the handle anywhere. Comment is honest: "Kept inside `Option` so `WorldStreamingState` can be moved out of the App on shutdown without forcing a join." The streaming.rs:38 import brings `std::thread::JoinHandle` into scope but it's never `.join()`-ed.

## Impact
On shutdown, the OS may race-free the worker's `Arc` references against its own use. In practice the process exits cleanly within a few ms because the worker's `recv()` returns `Err` immediately after the sender drops, so the worker exits before the process tears down. Theoretical: a slow `BsaArchive::extract()` (network filesystem, spinning disk under contention) could leave the worker mid-extract for 100+ ms, blocking shutdown indirectly via the `Arc` count.

## Suggested Fix
Add a `WorldStreamingState::shutdown(self)` helper that drops `request_tx` first, then `.join()`-s the worker with a 1-second timeout (using a `(JoinHandle, Receiver<()>)` pattern). Call it from `WindowEvent::CloseRequested` instead of `self.streaming.take()`. Aligns with the existing comment intent.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
