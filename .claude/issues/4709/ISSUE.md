# GAME-D4-2026-09-21-06: BYRO_NO_AI_LOCOMOTION also unregisters combat feedback

**Issue**: #4709
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: LOW
**Dimension**: 4/5 — kill-switch scope
**Location**: `byroredux/src/boot/schedule/post_update.rs`; pin `boot/schedule/mod.rs` (`locomotion_registers_behind_the_single_kill_switch`)

## Description
`make_combat_feedback_system` is registered inside the `if locomotion_enabled` block, gated by `BYRO_NO_AI_LOCOMOTION` (documented as an AI-motion debug switch). The pin checks only env-var presence, not block membership.

## Evidence
`scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_combat_feedback_system())` sits inside the `locomotion_enabled` block.

## Impact
Setting the switch to debug AI motion silently also removes P2 combat feedback.

## Related
ECS-2026-09-21-D5 cross-audit pointer.

## Suggested Fix
Register combat feedback after the block; extend the pin to name the gated system set.
