# GAME-D3-2026-09-21-01: A refused native inventory action is silent to the player

**Issue**: #4707
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: LOW
**Dimension**: 3 — Consumables
**Location**: `byroredux/src/main.rs` (`MutationResult::Unavailable` handling)

## Description
`Unavailable` reaches only `log::warn!`; success pushes a visible "Used {name}" notification. Refusals (branch conditions false, missing AV, health <= 0, item equipped) look like a dead button.

## Evidence
`if inventory::apply_action(world, action) == inventory::MutationResult::Unavailable { log::warn!(...) }`.

## Impact
Player can't tell why Use/Equip did nothing — the shape that hid closed #4458.

## Related
#4458 (closed; same silent-refusal shape, different root cause).

## Suggested Fix
Return a reason and push a `PlayerNotifications` line.
