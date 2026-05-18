**Source**: [`docs/audits/AUDIT_CONCURRENCY_2026-05-17.md`](docs/audits/AUDIT_CONCURRENCY_2026-05-17.md)
**Dimension**: Worker Threads
**Severity**: LOW

## Observation

`crates/core/src/ecs/resources.rs:97-115, 130-139`:

```rust
pub fn take_result(&self) -> Option<Vec<u8>> {
    self.result.lock().unwrap().take()  // panics on PoisonError
}

pub fn take_result_for(&self, owner: u8) -> Option<Vec<u8>> {
    if self.owner.load(...) != owner { return None; }
    let bytes = self.result.lock().unwrap().take()?;  // panics on PoisonError
    self.owner.store(SCREENSHOT_OWNER_NONE, ...);
    Some(bytes)
}

pub fn cancel(&self) -> bool {
    let had_request = self.requested.swap(false, ...);
    let had_result = self.result.lock().unwrap().take().is_some();  // panics on PoisonError
    self.owner.store(SCREENSHOT_OWNER_NONE, ...);
    had_request || had_result
}
```

`ScreenshotBridge::result: Mutex<Option<Vec<u8>>>`. Both `DebugDrainSystem` (main thread) and the renderer's screenshot path (also main thread, but inside `draw_frame` which itself may spawn a tokio task in future) touch it via `.lock().unwrap()`.

## Why it's a bug

If a panic propagates from inside the lock, the mutex is poisoned and every subsequent `take_result_for` / `cancel` panics — a process-killer.

Since the renderer's encode path runs on the main thread today the practical exposure is low. If `--screenshot` ever wires through a worker thread (encode on a tokio task, PNG compression on rayon), a panic poisons the bridge for the rest of the session.

## Trigger Conditions

A panic inside the renderer's screenshot-encode path (`png::encode` etc.) while `result` is locked.

## Fix

The state inside the mutex (`Option<Vec<u8>>`) is a simple value; poisoning carries no recovery-required invariant. Replace `.lock().unwrap()` with `.lock().unwrap_or_else(|e| e.into_inner())`:

```rust
pub fn take_result(&self) -> Option<Vec<u8>> {
    self.result.lock().unwrap_or_else(|e| e.into_inner()).take()
}
```

Apply uniformly across all three method sites + the test sites (lines 745, 750, 779, 781, 818, 823 use the same pattern but tests are fine).

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: any other `Mutex<Option<…>>` or `Mutex<Vec<u8>>` in the codebase that's safe to recover from poison? Sweep for `\.lock\(\)\.unwrap\(\)` across crates and treat each as a decision
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: regression test that explicitly poisons the bridge and asserts subsequent calls don't panic

## Related

- #1006 / #1007 / #1011 — screenshot bridge owner-tagging + cancel + queue cap
