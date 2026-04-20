# Issue #486

FNV-AN-L2: AnimationPlayer.reverse_direction not serialized for debug snapshots

---

## Severity: Low

**Location**: `crates/core/src/animation/player.rs`; `debug-protocol/src/registry.rs` (component serialization)

## Problem

`AnimationPlayer::reverse_direction: bool` (ping-pong direction tracker for `CycleType::Reverse`) is not included in the debug-protocol component serialization. A snapshot loaded back into the engine resets to `forward`, making the ping-pong step backward across the boundary.

Equivalent field on `AnimationLayer::reverse_direction` has the same issue — the debug-protocol registry inspection of the animation stack doesn't surface it.

## Impact

- Not user-visible in normal gameplay (play doesn't restart from debug snapshots).
- Debugging a specific frame of a ping-pong sequence requires knowing the direction at that frame.

## Fix

Add `reverse_direction` to the `ComponentDescriptor` for `AnimationPlayer` and each `AnimationLayer` in the stack. Serialize as bool.

## Completeness Checks

- [ ] **TESTS**: Debug CLI snapshot + reload round-trip preserves reverse_direction
- [ ] **SIBLING**: Check other animation-state fields (`blend_in_remaining`, `blend_out_remaining`) for the same serialization gap

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-AN-L2)
