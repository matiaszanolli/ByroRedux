# 3251: ECS-2026-08-24-03: AnimatedTextureFlip::handle_for_slot silently aliases an out-of-range index to bindless handle 0

**Severity**: LOW (informational) · **Report**: `docs/audits/AUDIT_ECS_2026-08-24.md` (ECS-2026-08-24-03)

## Description

`handle_for_slot`'s `Option` return is meant to distinguish "no flipbook on this slot" from "flipbook present", but a present-yet-out-of-range `current_index` collapses to `Some(0)` (bindless slot 0 — *some other entity's texture*) instead of `None`.

## Location

`crates/core/src/ecs/components/animated.rs:206-213`

## Evidence

```rust
pub fn handle_for_slot(&self, slot: u32) -> Option<u32> {
    self.0.iter().find(|e| e.texture_slot == slot)
        .map(|e| e.handles.get(e.current_index).copied().unwrap_or(0))
}
```

## Impact

Not reachable today — attach path and per-frame writer keep `handles.len()` and `current_index` in lockstep. A future clip-swap path rebinding a different `NiFlipController` onto an already-attached entry could silently show the wrong texture.

## Suggested Fix

`.and_then(|e| e.handles.get(e.current_index).copied())` — out-of-range then correctly reads as "no handle".

## Completeness Checks
- [ ] **TESTS**: A regression test asserting out-of-range `current_index` returns `None`, not `Some(0)`
