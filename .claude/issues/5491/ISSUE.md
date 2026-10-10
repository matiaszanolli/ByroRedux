# #5491: CONC-D7-2026-10-09-01: Streaming `shutdown` does not stop the worker — the queued backlog keeps it parsing through the whole join timeout

**Labels**: bug, concurrency, medium, performance

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-09.md` — finding `CONC-D7-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM. The join timeout bounds the damage, so it cannot deadlock, and the worker touches neither the `World`
  nor Vulkan, so there is no use-after-free. The cost is a deterministic main-thread stall on a hot path (every
  transition), plus wasted CPU and memory overlap. The documented invariant it rests on is false.
- **Dimension**: Worker Threads
- **Location**:
  - `byroredux/src/streaming/mod.rs:680-704` (`shutdown`); the false claim is in the comment at `:687-693`.
  - `byroredux/src/streaming/mod.rs:425`: `payload_rx` is a plain field, alive until `self` drops.
  - `byroredux/src/streaming/pre_parse.rs:145-175` (worker loop) and `:183-202` (`recv_next_batch_request`).
  - Callers:
    - `streaming_helpers.rs:620` (`drain_streaming_state`, 1 s). That drain serves every exterior→interior door,
      exterior→exterior transition, save-load reload (`save_io.rs:1438`, `:1593`) and debug load (`debug_load.rs:401`,
      `:497`).
    - `app_events.rs:130` (CloseRequested, 1 s).
    - The `Drop` safety net (`mod.rs:717-721`, 1 s).
- **Status**: NEW. Searches matched the closed #856, #1167, #1168 and #1169. Those concern joining versus detaching and the
  watcher-thread leak; none covers the queued backlog.
- **Trigger Conditions**: A drain runs while the worker has more than about 1 s of queued work. That is the normal state for
  a while after exterior entry. Interactive startup is foreground-first: it waits for the centre cell, then leaves the
  rest of the 11×11 default radius (120 cells) queued on the worker (`docs/engine/exterior-grid-streaming.md` §1,
  `queue_loads` at `mod.rs:628-660`). Fast travel across boundaries also builds a backlog.
- **Verification Path**: `cargo test`-level. The shutdown path has no test with a queued backlog; the `join_with_timeout_*`
  tests pin the helper against synthetic threads only. Measured with the probe below.
- **Description**: `shutdown` takes the handle, drops `request_tx`, then polls `is_finished` until the deadline. The comment
  says the worker's `recv()` *"returns Err on its next loop iteration and the thread exits"*, and the skill's Dim 7
  checklist repeats it. That is not how `std::sync::mpsc` works. `recv` and `try_recv` keep returning every message sent
  before the disconnect, and report `Disconnected` only once the queue is empty.

  The worker therefore keeps draining the backlog. Its `payload_tx.send` keeps succeeding, because `payload_rx` is a field
  of the `WorldStreamingState` being shut down and lives until `shutdown` returns and `self` drops. The join hits its
  deadline and detaches the worker. Only after the state drops does the worker's next send fail; it exits one cell later.
  Every payload produced during the join is thrown away with the receiver.
- **Evidence**: The probe mirrors the handshake: 49 queued requests, 50 ms per request, drop the sender, a 10 ms
  `is_finished` poll and a 1 s deadline. Output:
  ```
  joined=false after 1.005532549s; payloads delivered while joining=20
  worker exit after 21 requests        # only after payload_rx was dropped
  ```
- **Impact**:
  - A 1 s main-thread stall on any door walk, save load or debug load made during the fill window, and the same on window
    close.
  - Up to 1 s of stream-pool CPU spent parsing cells the drain has already discarded.
  - After the detach, the old worker, its N/2-thread rayon pool, `Arc<ExteriorWorldContext>` and `Arc<TextureProvider>`
    outlive the state by one more cell. That overlaps with the next worldspace's freshly built worker and pool: transient
    oversubscription and duplicated record-index memory at the transition.
  - The p5 transition/save soak exercises this path.
- **Related**: #856, #1167, #1169 (shutdown design), #3670 (the batch dispatch that makes deep queues normal),
  CONC-D7-2026-10-09-02.
- **Suggested Fix**: Make shutdown cancel the queue, not only close it. Either:
  - share an `Arc<AtomicBool>` cancel flag, set it before dropping `request_tx`, and have the worker check it at the top of
    each loop iteration (optionally also between pipeline admissions); or
  - make `payload_rx` an `Option` and drop it before the join, so the in-flight cell's send fails and the loop breaks.

  Either bounds the join to the cell already in flight. Add a test that queues N slow requests and asserts that `shutdown`
  joins within one request's time. Then fix the comment and the skill text.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
