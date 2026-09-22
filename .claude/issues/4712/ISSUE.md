# GAME-D7-2026-09-21-03: Fragment and package Activate() never reaches container_loot_system — the flush runs after that consumer

**Issue**: #4712
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: LOW
**Dimension**: 7 — Stage order
**Location**: `byroredux/src/boot/schedule/update.rs` (`container_loot_system` vs `fragment_activation_flush_system`); pin `boot/schedule/mod.rs`

## Description
The #2654 flush is registered after `container_loot_system`; the pin's 3-consumer list omits it as a fourth real consumer. Same class of gap as open #4116 (`mg07_on_activate_dispatch`, a fifth omitted consumer).

## Evidence
`container_loot_system` at update.rs line 152 vs flush at line 255.

## Impact
A scripted container/pickup `Activate()` for the player is a no-op; console `script.activate` is currently the only road in.

## Related
#4116 (open, sibling gap); #2654; GAME-D2-2026-09-21-02 (#4697).

## Suggested Fix
Move the flush ahead of `container_loot_system`; add both missing consumers to the pin.
