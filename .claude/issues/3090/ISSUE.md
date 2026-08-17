# CONC-2026-08-16-02: a cancelled screenshot skips that frame's entire command drain

**Issue**: #3090
**Severity**: LOW
**Labels**: `low,sync,bug`
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_CONCURRENCY_2026-08-16.md` (Dimension — Worker Threads).

**Location**: `crates/debug-server/src/system.rs`:72-78

**Trigger conditions**: A `byro-dbg` screenshot request whose client-side 5 s `recv_timeout` fires before the engine's 10-frame ceiling — i.e. a paused or GPU-stalled engine, precisely the state #1007 was written for — with at least one other command already queued behind it.

## Description

The #1007 abandonment handler cancels the in-flight GPU capture, clears `pending_screenshot`, and then `return`s from `System::run`. **That `return` exits the whole system, not just the screenshot block**, so the command drain never runs on that frame.

## Evidence

```rust
// crates/debug-server/src/system.rs:72-78 (re-verified 2026-08-17)
if pending.cancel.load(Ordering::Acquire) {
    if let Some(bridge) = world.try_resource::<ScreenshotBridge>() {
        bridge.cancel();
    }
    self.pending_screenshot = None;
    return;          // <- exits System::run, skipping the drain at :136-142
}
```

The three sibling arms in the same block (`:110`, `:124`, `:131`) all fall through rather than returning — this arm is the odd one out.

## Impact

Every other pending command is deferred a frame. Bounded and self-correcting (the drain runs next frame), which is why it is LOW.

It matters because the trigger state is a **paused or stalled engine** — exactly when a developer is issuing several `byro-dbg` commands and least able to tell a deferred command from an ignored one.

## Suggested Fix

Replace the `return` with the fall-through the sibling arms use, so cancellation clears the screenshot without skipping the drain.

## Related

- #1007 (the abandonment handler this arm implements)
- #3007 (RT-08 — the other debug-server finding this sweep)

## Completeness Checks
- [ ] **SIBLING**: The arm matches the fall-through behaviour of the three siblings at :110/:124/:131
- [ ] **NO-SKIP**: The drain at :136-142 runs on every frame regardless of screenshot state
- [ ] **CANCEL-STILL-WORKS**: #1007's abandonment behaviour is preserved
- [ ] **TESTS**: A regression test cancels a screenshot with a queued command and asserts same-frame drain

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3090 --json state` when live state is needed.*
