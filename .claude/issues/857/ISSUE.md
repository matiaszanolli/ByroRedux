## Source Audit
`docs/audits/AUDIT_CONCURRENCY_2026-05-05.md`

## Severity / Dimension
LOW / Worker Threads (Streaming, Debug) — minor cosmetic

## Location
`crates/debug-server/src/listener.rs:39-46`, `crates/debug-server/src/lib.rs:30`

## Description
`listener_loop` binds `format!("127.0.0.1:{}", port)` unconditionally. The `start` log message also says `"Debug server listening on 127.0.0.1:{}"`. Both are correct today — no host argument exists — but if a future feature adds a host arg, these two log strings will need to be updated together. Mark as a documentation/coupling smell.

## Evidence
```rust
// crates/debug-server/src/listener.rs:39-46
fn listener_loop(port: u16, queue: CommandQueue) {
    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        ...
```
```rust
// crates/debug-server/src/lib.rs:30
log::info!("Debug server listening on 127.0.0.1:{}", port);
```

## Impact
None. Cosmetic.

## Suggested Fix
Centralise the bind address as a `const` or pass the `String` from `start` to `listener_loop` and reuse it for both the bind call and the log message.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
