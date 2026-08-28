# #3449 — SAFE-2026-08-27b-05: debug-server is a default cargo feature and its accept loop spawns an uncapped OS thread per connection

- **Source**: `docs/audits/AUDIT_SAFETY_2026-08-27b.md`
- **Severity**: LOW
- **Labels**: `low,safety,tech-debt,bug`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3449

---

From `docs/audits/AUDIT_SAFETY_2026-08-27b.md` (un-owned subsystem — `crates/debug-server`, per `_audit-common`'s coverage table: *"a TCP listener that evaluates queries against the live `World`; nothing audits its command surface"*).

- **Severity**: LOW (hardening; loopback binding is the mitigation that keeps it here)
- **Location**: `byroredux/Cargo.toml:8` (`default = ["debug-server"]`); `crates/debug-server/src/listener.rs:158`, `:185-232`
- **Status**: NEW — no issue and no prior audit finding covers it; `crates/debug-server` appears in the 2026-08-16 / 2026-08-20 tech-debt reports only as a named scope gap.

## Description

`spawn` binds `TcpListener::bind(("127.0.0.1", port))` — the loopback binding is correct and is the reason this is not higher. What is unbounded is what happens after `accept()`: every connection gets its own named OS thread (`thread::Builder::new().name(format!("byro-debug-client-{addr}")).spawn(...)`) with no concurrent-connection cap, no accept rate limit, and no authentication. `active_streams` is pruned opportunistically, but it only tracks `Weak` handles for shutdown teardown — it never refuses a connection.

The reason this is worth a line rather than nothing is `byroredux/Cargo.toml:8`: `debug-server` is in `default`, so an ordinary `cargo build --release` produces a binary that listens. The command surface behind it mutates the live `World` (`setav`/`modav`, `script.activate`, `door.teleport`, debug cell loads) and writes screenshots to disk, so a local process — not a remote one — can drive the engine and can also exhaust its thread budget.

## Evidence

```rust
// crates/debug-server/src/listener.rs:158
let listener = TcpListener::bind(("127.0.0.1", port))?;
// :228-232 — no cap between accept and spawn
thread::Builder::new()
    .name(format!("byro-debug-client-{}", addr))
    .spawn(move || handle_client(stream_arc, q, s))
    .ok();
```
```toml
# byroredux/Cargo.toml:7-9
[features]
default = ["debug-server"]
debug-server = ["dep:byroredux-debug-server"]
```

## Impact

A local process opening connections in a loop exhausts the thread budget and can wedge the engine. Not remotely reachable — the loopback bind is what bounds this. Filed as hardening, and as the first finding of any kind against this crate's command surface.

## Related

#3007 (the last debug-listener bind defect), #1009 / #1172 (the `active_streams` shutdown side channel this sits next to).

## Suggested Fix

Cap concurrent clients (an `AtomicUsize` incremented before spawn, decremented in `handle_client`'s exit; refuse and close past ~8), which is a few lines inside the critical section that already exists. Separately, decide deliberately whether `debug-server` should remain in `default` once there is a shipping profile — and record that decision next to the feature.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (any other accept/spawn loop — `tools/byro-dbg`'s client side, the screenshot channel)
- [ ] **LOCK_ORDER**: the counter must not widen the existing `active_streams` mutex critical section or introduce a second lock inside it
- [ ] **TESTS**: A regression test pins this specific fix
