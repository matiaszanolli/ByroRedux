## Source Audit
`docs/audits/AUDIT_CONCURRENCY_2026-05-05.md`

## Severity / Dimension
LOW / Worker Threads (Streaming, Debug)

## Location
`crates/debug-server/src/lib.rs:22-31` (start discards `_listener_handle`), `crates/debug-server/src/listener.rs:58-61` (per-client `.spawn(...).ok()` discards `JoinHandle`)

## Description
**Trigger Conditions**: Engine shutdown via `event_loop.exit()` while a debug client is connected, or when no client is connected (the listener thread itself loops on `accept` indefinitely with a 50 ms sleep).

`byroredux_debug_server::start` calls `let (mut drain_system, _listener_handle) = listener::spawn(port);` and immediately drops `_listener_handle`, detaching the listener thread. Inside the listener loop (`listener.rs:55-61`), each per-client thread is spawned with `.ok()` which also discards the `JoinHandle`. There is no shutdown signal — the listener never exits its `loop { listener.accept() }`, and per-client threads block on `wire::decode(&mut reader)` indefinitely (with a 300 s read timeout). On engine exit the OS kills both. Practically no bug; the threads hold an `Arc<Mutex<Vec<PendingCommand>>>` (the queue) whose ref-count survives because the threads survive — it just means the queue (now empty since `DebugDrainSystem` is dropped on `Scheduler` drop) isn't reclaimed until process exit.

## Evidence
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

## Impact
Untidy shutdown when `--features debug-server` is enabled. Process exit reaps the threads. The listener thread's 50ms `accept` poll burns ~0.001% CPU at idle, fine. The shutdown leak is a few bytes per detached thread, eclipsed by everything else dropping.

## Suggested Fix
Plumb a `Arc<AtomicBool>` shutdown flag through both the listener thread and per-client threads. Have `start` return the `JoinHandle` so the engine's shutdown path can flip the flag and join. Per-client threads can poll the flag when their TCP read times out. Low priority — acceptable as-is for a developer-only feature.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
