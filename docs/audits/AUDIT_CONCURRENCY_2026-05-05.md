# Concurrency and Synchronization Audit — 2026-05-05

**Scope (focused)**: Dimension 4 (Thread Safety) and Dimension 6 (Worker Threads:
Streaming, Debug Server) ONLY. Dimensions 1 (ECS Locking), 2 (Vulkan Sync),
3 (Resource Lifecycle), and 5 (Compute → AS → Fragment Chains) were NOT re-audited
in this pass — see `AUDIT_CONCURRENCY_2026-04-10.md` and
`AUDIT_CONCURRENCY_2026-04-12.md` for the most recent coverage there.

**Trigger**: M44 audio subsystem landed (kira-backed `AudioWorld` Resource +
`audio_system` + `SoundCache`) and an audio-deep audit needs a thread-safety
sanity check on the new shared state. Cross-cuts with the existing M40 cell
streaming worker and the long-standing debug-server listener.

**Methodology**: Static read of every cited path; re-checked each finding's
premise against the current code before retaining it.

---

## Executive Summary

| Severity | Count | Dimension |
|----------|-------|-----------|
| CRITICAL | 0     | — |
| HIGH     | 0     | — |
| MEDIUM   | 2     | Worker Threads (1), Thread Safety (1) |
| LOW      | 4     | Worker Threads (3), Thread Safety (1) |
| INFO     | 1     | Audio cross-cut |

**6 findings total**. No CRITICAL or HIGH. The two MEDIUM findings are both
defensive-gap issues, not active bugs:

- C6-NEW-01 (MEDIUM, Worker Threads): the cell-stream worker has no panic
  catch — a panic in `parse_nif` / import code permanently kills the
  streaming subsystem with no respawn.
- C4-NEW-01 (MEDIUM, Thread Safety): `play_oneshot` keeps queueing entries
  on a no-device host even though `audio_system` early-returns and never
  drains — bounded at 256 by FIFO, but the queue never clears once filled.

The audio crate's M44 shared state is sound: `AudioWorld` and `SoundCache`
both `impl Resource`, which has `Send + Sync` as a supertrait, so the compiler
statically guarantees every kira handle inside them satisfies `Send + Sync`.
Audio is purely main-thread (Stage::Late, parallel batch with
`log_stats_system`) — neither the streaming worker nor the debug server
touch `AudioWorld`.

### Key invariants verified

| Invariant | Status | Evidence |
|-----------|--------|----------|
| `AudioWorld: Resource` requires `Send + Sync` | PASS | `crates/core/src/ecs/resource.rs:13` (`Resource: 'static + Send + Sync`); compiles with `impl Resource for AudioWorld` at `crates/audio/src/lib.rs:437` |
| kira `AudioManager`, `SpatialTrackHandle`, `StreamingSoundHandle`, `ListenerHandle`, `SendTrackHandle`, `Arc<StaticSoundData>` are all `Send + Sync` | PASS | Compilation of `impl Resource for AudioWorld` enforces it; kira 0.10.8 sources contain no `thread_local!` / `Rc` / `Cell` / `*const` markers |
| Streaming worker MUST NOT call into `AudioWorld` | PASS | `byroredux/src/streaming.rs:210-323` — worker only touches `TextureProvider` (BSA Mutex<File>) + `ExteriorWorldContext` + NIF parser; no `byroredux_audio::*` reference anywhere in the worker path |
| Debug server per-client threads do NOT touch `World` directly | PASS | `crates/debug-server/src/listener.rs:75-128` — client thread only pushes into `Arc<Mutex<Vec<PendingCommand>>>`; all `World` access happens in `DebugDrainSystem::run` (Stage::Late exclusive, `crates/debug-server/src/system.rs`) |
| `consume_streaming_payload` only modifies `World` from main thread | PASS | `byroredux/src/main.rs:576-650` runs after `scheduler.run()` returns |
| NIF import cache (`NifImportRegistry`) accessed only from main thread | PASS | Worker emits raw `PartialNifImport` payloads (`streaming.rs:91-104`); cache mutation happens in `cell_loader::finish_partial_import` from main-thread drain (`main.rs:606-617`) |
| Audio system's `query_mut` calls ordered correctly | PASS | `crates/audio/src/lib.rs:817-870` — `prune_stopped_sounds` drops the `AudioEmitter` read query (`drop(emitter_q)` at line 841) BEFORE acquiring the write `query_mut::<AudioEmitter>()` |
| Cell unload → audio cleanup path | PASS (one-frame lag) | `cell_loader::unload_cell` despawns entities synchronously (`byroredux/src/cell_loader.rs:106-262`); next `audio_system` tick observes missing `AudioEmitter` and stops looping handles (`crates/audio/src/lib.rs:817-846`) |
| Screenshot bridge race-free across debug-drain ↔ draw_frame | PASS | `screenshot_finish_readback` runs after fence wait (`crates/renderer/src/vulkan/context/screenshot.rs:14-66`); `Arc<Mutex<Option<Vec<u8>>>>` carries the result; debug drain reads next frame |
| Stage scheduling prevents `AudioWorld` parallel-write contention | PASS | `footstep_system` writes `AudioWorld` in `Stage::Update`; `audio_system` writes in `Stage::Late` — sequential stages, no overlap. No other parallel writer in either stage |

### Dedup notes

Existing concurrency-related issues confirmed open or closed and NOT
re-reported here:

- #46 (transform propagation lock count) — OPEN, ECS Locking dim, not in scope
- #92 (`update_rgba` descriptor write race) — OPEN, Vulkan Sync dim, not in scope
- #267 (single SSAO AO image across frames) — OPEN, Vulkan Sync dim, not in scope
- #653 (SVGF/TAA history slot read RAW) — CLOSED, fixed in M37.5 work
- #823 (lock_tracker hot-path allocation) — CLOSED, fixed
- #826 (`world_bound_propagation` root scan) — OPEN, ECS Locking dim, not in scope
- #829 (`billboard_system` lock recycle) — OPEN, ECS Locking dim, not in scope
- C4-01 (`AUDIT_CONCURRENCY_2026-04-12`): cross-thread lock tracker gap — still
  open as a long-term architectural item; not repeated here.

---

## Findings (Dim 4 — Thread Safety)

### C4-NEW-01: `AudioWorld::play_oneshot` queue never drains on a no-device host
- **Severity**: MEDIUM
- **Dimension**: Thread Safety
- **Location**: `crates/audio/src/lib.rs:320-342, 538-550, 603-624`
- **Status**: NEW
- **Trigger Conditions**: Engine launched on a host where `AudioManager::new()`
  fails (CI / headless server / broken sound driver), and a system runs that
  calls `AudioWorld::play_oneshot` — currently `byroredux/src/systems.rs::footstep_system`
  on a player entity that has a `FootstepEmitter` AND a non-`None`
  `FootstepConfig.default_sound`.
- **Description**: `play_oneshot` always pushes a `PendingOneShot` into
  `pending_oneshots` (line 336), regardless of whether the manager is active.
  `audio_system` runs the early-return `if !audio_world.is_active() { return; }`
  at line 542-544 BEFORE calling `drain_pending_oneshots`, so the queue
  never drains when audio is inactive. Bounded at 256 entries via FIFO
  drop-oldest (line 327-335), so this is a steady-state cap, not unbounded
  growth — but those 256 entries (each holds an `Arc<StaticSoundData>` clone)
  pin one strong refcount on each cached sound and ~2.5 KB of `Vec<PendingOneShot>`
  state for the lifetime of the engine. Long-term the queue can also block
  hypothetical audio-recovery flows (the docstring at line 313-316 hints at
  "future re-init" replay, but pinning 256 stale entries from earlier
  gameplay would replay them all the moment audio comes online).
- **Evidence**:
```rust
// crates/audio/src/lib.rs:538-550
pub fn audio_system(world: &World, _dt: f32) {
    let Some(mut audio_world) = world.try_resource_mut::<AudioWorld>() else {
        return;
    };
    if !audio_world.is_active() {
        return;  // ← early-return BEFORE drain_pending_oneshots
    }

    sync_listener_pose(world, &mut audio_world);
    drain_pending_oneshots(&mut audio_world);  // ← never reached when inactive
    ...
```
- **Impact**: ~12 KB pinned memory + an `Arc` strong-count per cached sound
  on no-audio hosts after ~8 seconds of footstep activity. No functional bug
  in the steady state. CI runs on a headless host might accumulate a queue
  of stale one-shots if a long-running test fires footsteps.
- **Suggested Fix**: In `play_oneshot`, short-circuit the push when
  `self.manager.is_none()` (cheap check). Alternatively, in `audio_system`,
  drain (and discard) the pending queue on every tick when inactive so a
  later activation doesn't replay stale events. The first option is simpler.
- **Related**: The 256-cap path in `play_oneshot` was added precisely to
  bound this scenario; this finding tightens it from "bounded leak" to "no leak".

### TS-INFO-01: kira types' `Send + Sync` is enforced by `Resource` supertrait, not local asserts
- **Severity**: INFO (defense-in-depth note)
- **Dimension**: Thread Safety
- **Location**: `crates/core/src/ecs/resource.rs:13`, `crates/audio/src/lib.rs:437, 1041`
- **Status**: NEW (informational)
- **Description**: `pub trait Resource: 'static + Send + Sync {}` makes
  `Send + Sync` a supertrait — `impl Resource for AudioWorld` at
  `crates/audio/src/lib.rs:437` therefore requires `AudioWorld: Send + Sync`
  and the compiler enforces it transitively across every field
  (`AudioManager<DefaultBackend>`, `StreamingSoundHandle<FromFileError>`,
  `SendTrackHandle`, `ListenerHandle`, `Vec<ActiveSound>` where
  `ActiveSound` holds `StaticSoundHandle` + `SpatialTrackHandle`,
  `Vec<PendingOneShot>` holding `Arc<StaticSoundData>`). The same for
  `SoundCache: Resource` at line 1041. There's no explicit
  `static_assertions::assert_impl_all!(AudioWorld: Send, Sync)` in the
  audio crate, so a future kira version that quietly removes `Send + Sync`
  on one of these handle types would surface as a compile error at the
  `impl Resource` line — which is fine, just not as obvious as a named
  assertion. **No action required**, but adding a one-line
  `assert_impl_all!` would future-proof against kira API churn.
- **Evidence**: kira 0.10.8 source check confirms no `thread_local!`,
  `Rc<T>`, `Cell<T>`, or `PhantomData<*const T>` in `AudioManager`,
  `SpatialTrackHandle`, `StaticSoundHandle`, `StreamingSoundHandle`,
  `ListenerHandle`, `SendTrackHandle`, or `StaticSoundData`. The
  `CpalBackend` field `cpu_usage_consumer: Option<Mutex<Consumer<f32>>>`
  uses `Mutex` to make the cpal-side ringbuffer `Sync`, and the actual
  `cpal::Stream` (which is `!Send`) lives behind a `StreamManagerController`
  on a dedicated kira-spawned thread (`kira/src/backend/cpal/desktop/stream_manager.rs:89`).
- **Impact**: None today.
- **Suggested Fix**: (Optional) Add to `crates/audio/src/lib.rs`:
  ```rust
  #[cfg(test)]
  static_assertions::assert_impl_all!(AudioWorld: Send, Sync);
  ```

---

## Findings (Dim 6 — Worker Threads)

### C6-NEW-01: Cell-streaming worker has no panic recovery — one panic permanently disables streaming
- **Severity**: MEDIUM
- **Dimension**: Worker Threads (Streaming, Debug)
- **Location**: `byroredux/src/streaming.rs:210-230` (worker loop), `byroredux/src/streaming.rs:285-314` (rayon parallel parse), `byroredux/src/main.rs:545-559` (request dispatch)
- **Status**: NEW
- **Trigger Conditions**: A NIF in a freshly-loaded cell hits a code path
  in `byroredux_nif::parse_nif`, `extract_bsx_flags`, `import_nif_lights`,
  `import_nif_particle_emitters`, or `import_embedded_animations` that
  panics. The panic propagates up through the rayon `into_par_iter().map(...)
  .collect()` (rayon-style: panics in worker tasks resurface in `collect()`),
  then up through `pre_parse_cell` and `cell_pre_parse_worker`. The
  `JoinHandle<()>` is held in `WorldStreamingState.worker` but is never
  observed — it's `#[allow(dead_code)]` at line 157 with a comment "nothing
  currently calls `.take().join()`."
- **Description**: When the worker panics, `request_rx` (the receiver) is
  dropped. The next `step_streaming` tick on the main thread tries
  `state.request_tx.send(req)` (`main.rs:552`). The send returns `Err` and
  the main thread logs `"Streaming worker channel closed; cell ({},{})
  cannot be loaded"`, removes the pending entry, and continues — but no new
  worker is spawned, no fatal error is propagated, and every subsequent
  cell-crossing produces the same warning while the world streaming
  silently stops working. Looking at the code there is no catch_unwind
  around `pre_parse_cell`, no panic-hook installed for the worker, and no
  health check on the `JoinHandle::is_finished()` state.
- **Evidence**:
```rust
// streaming.rs:210-230 — no catch_unwind, no panic recovery
fn cell_pre_parse_worker(
    request_rx: mpsc::Receiver<LoadCellRequest>,
    payload_tx: mpsc::Sender<LoadCellPayload>,
) {
    while let Ok(req) = request_rx.recv() {
        let payload = pre_parse_cell(...);  // ← panic here = thread death
        if payload_tx.send(payload).is_err() { break; }
    }
}

// main.rs:552-559 — main thread observes Err and logs, but doesn't re-spawn
if state.request_tx.send(req).is_err() {
    log::error!(
        "Streaming worker channel closed; cell ({},{}) cannot be loaded",
        gx, gy
    );
    state.pending.remove(&(gx, gy));  // ← world keeps running, streaming dead
}
```
- **Impact**: One bad NIF (or one bug in NIF parser code that triggers on
  vanilla content for a specific game) silently kills exterior streaming
  for the whole session. The player can keep playing — but no new cells
  load, and existing cells don't unload (because unload happens off the same
  diff loop that's now no-oping). Particularly painful in CI where a single
  panic in a parser regression test could be misattributed to "streaming
  worked, just nothing rendered" instead of "the worker died on cell 3".
  In production a transient `unwrap()` regression in any of ~30 NIF block
  parsers immediately bricks all subsequent cell loads.
- **Related**: The NIF parser already does graceful error recovery for
  malformed blocks (the `parsed: HashMap<String, Option<...>>` shape carries
  per-NIF None failure markers). The panic case is the unhandled path.
- **Suggested Fix**: Wrap `pre_parse_cell` in
  `std::panic::catch_unwind(AssertUnwindSafe(|| pre_parse_cell(...)))` and
  emit an empty `LoadCellPayload` with a logged warning when the closure
  panics. Alternatively, use `JoinHandle::is_finished()` in `step_streaming`
  to detect worker death and re-spawn (more invasive but catches resource-
  exhaustion panics too). The catch_unwind path is the lightweight option
  and matches the per-NIF error contract already in place.

### C6-NEW-02: TCP listener thread + per-client threads are detached on debug-server `start`
- **Severity**: LOW
- **Dimension**: Worker Threads (Streaming, Debug)
- **Location**: `crates/debug-server/src/lib.rs:22-31` (start discards `_listener_handle`), `crates/debug-server/src/listener.rs:58-61` (per-client `.spawn(...).ok()` discards `JoinHandle`)
- **Status**: NEW
- **Trigger Conditions**: Engine shutdown via `event_loop.exit()` while a
  debug client is connected, or when no client is connected (the listener
  thread itself loops on `accept` indefinitely with a 50 ms sleep).
- **Description**: `byroredux_debug_server::start` calls
  `let (mut drain_system, _listener_handle) = listener::spawn(port);` and
  immediately drops `_listener_handle`, detaching the listener thread.
  Inside the listener loop (`listener.rs:55-61`), each per-client thread is
  spawned with `.ok()` which also discards the `JoinHandle`. There is no
  shutdown signal — the listener never exits its `loop { listener.accept() }`,
  and per-client threads block on `wire::decode(&mut reader)` indefinitely
  (with a 300 s read timeout). On engine exit the OS kills both. Practically
  no bug; the threads hold an `Arc<Mutex<Vec<PendingCommand>>>` (the queue)
  whose ref-count survives because the threads survive — it just means the
  queue (now empty since `DebugDrainSystem` is dropped on `Scheduler` drop)
  isn't reclaimed until process exit.
- **Evidence**:
```rust
// debug-server/src/lib.rs:23
let (mut drain_system, _listener_handle) = listener::spawn(port);
// _listener_handle dropped here = listener thread detached

// debug-server/src/listener.rs:58-61
thread::Builder::new()
    .name(format!("byro-debug-client-{}", addr))
    .spawn(move || handle_client(stream, q))
    .ok();  // JoinHandle discarded = client thread detached
```
- **Impact**: Untidy shutdown when `--features debug-server` is enabled.
  Process exit reaps the threads. The listener thread's 50ms `accept`
  poll burns ~0.001% CPU at idle, fine. The shutdown leak is a few
  bytes per detached thread, eclipsed by everything else dropping.
- **Suggested Fix**: Plumb a `Arc<AtomicBool>` shutdown flag through both
  the listener thread and per-client threads. Have `start` return the
  `JoinHandle` so the engine's shutdown path can flip the flag and join.
  Per-client threads can poll the flag when their TCP read times out.
  Low priority — acceptable as-is for a developer-only feature.

### C6-NEW-03: Streaming worker `JoinHandle` held but never joined — relies on `Arc` drop semantics for clean shutdown
- **Severity**: LOW
- **Dimension**: Worker Threads (Streaming, Debug)
- **Location**: `byroredux/src/streaming.rs:150-158` (handle held in `Option`), `byroredux/src/main.rs:785` (`self.streaming.take()` drops the handle)
- **Status**: NEW
- **Trigger Conditions**: Engine shutdown on `WindowEvent::CloseRequested`
  while the worker is mid-parse on a large cell.
- **Description**: `WorldStreamingState.worker: Option<JoinHandle<()>>`
  carries the worker's join handle. The comment at line 157 says "nothing
  currently calls `.take().join()` — holding the handle is the point." On
  shutdown (`main.rs:785: self.streaming.take()`), the `WorldStreamingState`
  is dropped, which drops `request_tx` (closing the channel) AND drops the
  `JoinHandle` (detaching the thread). The worker may still be inside a
  100-300 ms `pre_parse_cell` call when the main thread exits. The worker
  holds an `Arc<TextureProvider>` and `Arc<ExteriorWorldContext>` — both get
  dropped when the worker thread exits, but the timing relative to
  `event_loop.exit() → main return → process exit` is racy.
- **Evidence**: No `.join()` call on the handle anywhere. Comment is honest:
  "Kept inside `Option` so `WorldStreamingState` can be moved out of the
  App on shutdown without forcing a join." The streaming.rs:38 import
  brings `std::thread::JoinHandle` into scope but it's never `.join()`-ed.
- **Impact**: On shutdown, the OS may race-free the worker's `Arc`
  references against its own use. In practice the process exits cleanly
  within a few ms because the worker's `recv()` returns `Err` immediately
  after the sender drops, so the worker exits before the process tears
  down. Theoretical: a slow `BsaArchive::extract()` (network filesystem,
  spinning disk under contention) could leave the worker mid-extract for
  100+ ms, blocking shutdown indirectly via the `Arc` count.
- **Suggested Fix**: Add a `WorldStreamingState::shutdown(self)` helper that
  drops `request_tx` first, then `.join()`-s the worker with a 1-second
  timeout (using a `(JoinHandle, Receiver<()>)` pattern). Call it from
  `WindowEvent::CloseRequested` instead of `self.streaming.take()`.
  Aligns with the existing comment intent.

### C6-NEW-04: Listener thread missing-port log says "127.0.0.1:port" even when bind hostname differs
- **Severity**: LOW
- **Dimension**: Worker Threads (Streaming, Debug) — minor cosmetic
- **Location**: `crates/debug-server/src/listener.rs:39-46`, `crates/debug-server/src/lib.rs:30`
- **Status**: NEW (cosmetic)
- **Description**: `listener_loop` binds `format!("127.0.0.1:{}", port)`
  unconditionally. The `start` log message also says
  `"Debug server listening on 127.0.0.1:{}"`. Both are correct today —
  no host argument exists — but if a future feature adds a host arg,
  these two log strings will need to be updated together. Mark as a
  documentation/coupling smell.
- **Impact**: None. Cosmetic.
- **Suggested Fix**: Centralise the bind address as a `const` or pass
  the `String` from `start` to `listener_loop` and reuse it for both
  the bind call and the log message.

---

## Audio-Deep Cross-Cuts

### Verified: streaming worker does NOT touch `AudioWorld`

A grep of `byroredux/src/streaming.rs` for `audio|AudioWorld|byroredux_audio|kira`
returns zero hits. The worker's input is `(gx, gy, generation, Arc<ExteriorWorldContext>,
Arc<TextureProvider>)`; its output is `LoadCellPayload` containing parsed NIF
scenes. No audio dispatch happens in the worker, and no audio sound decode
happens in the worker either — `byroredux_audio::load_sound_from_bytes` is
only called from `byroredux/src/asset_provider.rs::try_load_default_footstep`
on the main thread at engine init. Confirmed safe per the dim-6 checklist.

### Verified: cell-unload audio cleanup runs on main thread synchronously

`cell_loader::unload_cell` (`byroredux/src/cell_loader.rs:106-262`) runs on
the main thread (called from `App::step_streaming` at `main.rs:519` and
`main.rs:762`, both main-thread paths). The despawn loop at
`cell_loader.rs:260-262` drops `AudioEmitter` components synchronously.
The next `audio_system` tick (Stage::Late, parallel batch) detects
the missing component in `prune_stopped_sounds` and calls `handle.stop()`
on any looping kira handle (`crates/audio/src/lib.rs:824-846`). The
one-frame lag (~16 ms at 60 FPS) between despawn and stop is acceptable —
non-looping sounds finish naturally because the kira handle's
`SpatialTrackHandle` is owned by `ActiveSound` in `AudioWorld`, not by
the despawned entity. Confirmed safe per the dim-6 checklist.

### Verified: audio is purely main-thread

`audio_system` runs in `Stage::Late` parallel batch (`byroredux/src/main.rs:340`).
`footstep_system` runs in `Stage::Update` parallel batch (`byroredux/src/main.rs:315`)
and writes `AudioWorld` via `play_oneshot`. Stages run sequentially, so
there's no inter-stage `AudioWorld` write contention. Within `Stage::Late`,
`audio_system` runs alongside `log_stats_system` (which only reads
`DebugStats`, `TotalTime`, `DeltaTime`); no other parallel system in
`Stage::Late` writes `AudioWorld`. The exclusive `event_cleanup_system`
and `DebugDrainSystem` run after the parallel batch and don't touch
`AudioWorld`. Confirmed: kira manager + handles are only ever invoked
from the rayon worker thread that happens to schedule `audio_system`
on a given frame, but kira has no thread-affinity requirements
(no `thread_local!` / non-`Send` types anywhere in its Send/Sync chain).

---

## Priority Fix Order

1. **C6-NEW-01** (MEDIUM, Worker Threads): wrap `pre_parse_cell` in
   `catch_unwind` so a NIF parser regression doesn't permanently brick
   exterior streaming. Cheap fix, high defensive value.
2. **C4-NEW-01** (MEDIUM, Thread Safety): add `if self.manager.is_none() { return; }`
   to `AudioWorld::play_oneshot` to stop pinning queue entries on
   no-device hosts.
3. **C6-NEW-03** (LOW, Worker Threads): add a graceful streaming shutdown
   path that joins the worker.
4. **C6-NEW-02** (LOW, Worker Threads): plumb a shutdown flag through the
   debug-server listener / per-client threads.
5. **C6-NEW-04** (LOW): centralise the bind hostname constant.
6. **TS-INFO-01** (INFO): optional `assert_impl_all!(AudioWorld: Send, Sync)`
   for explicit future-proofing against kira API churn.

---

*Suggested next step: `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-05-05.md`*
