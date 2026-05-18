**Source**: [`docs/audits/AUDIT_CONCURRENCY_2026-05-17.md`](docs/audits/AUDIT_CONCURRENCY_2026-05-17.md)
**Dimension**: Worker Threads
**Severity**: LOW (sub-case of CONC-D6-NEW-01 specific to the most-trafficked automation path)

## Observation

`byroredux/src/main.rs:1592-1594` — the `--bench-frames` natural-exit branch calls `event_loop.exit()` after a screenshot write. The streaming worker is still mid-flight (bench cadence drives cell crossings continuously). No `state.shutdown(...)` is called.

Worker is detached on `App::Drop` per CONC-D6-NEW-01.

## Why it's a bug

Every CI / nightly bench run is exactly this code path. Leaks a worker thread + holds BSA files open until OS reaps them on process exit. Doesn't corrupt anything but exactly matches the failure mode #856 closed for `CloseRequested`. Process exit may delay 100–300 ms on a slow disk while the worker's in-flight extract winds down.

## Trigger Conditions

`cargo run --release -- ... --bench-frames 300` without `--bench-hold`. Every nightly bench / regression run.

## Fix

If CONC-D6-NEW-01's option (a) is taken, this is automatically fixed (Drop on `App` field unwinds the streaming state).

Otherwise (option b), insert before `event_loop.exit()` at line 1594:

```rust
if let Some(state) = self.streaming.take() {
    state.shutdown(std::time::Duration::from_secs(1));
}
```

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: every other `event_loop.exit()` site (lines 938, 948, 1011, 1085, 1267, 1275) needs the same call — though streaming usually isn't initialised by that point, so check
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: bench-frames smoke test that asserts worker joined before process exit

## Related

- CONC-D6-NEW-01 — parent issue, structural fix (Drop impl) covers this automatically
- #856 — original streaming worker join fix
