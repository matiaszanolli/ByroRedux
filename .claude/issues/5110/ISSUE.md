# TD3-2026-09-29-04: game-loop.md's live-schedule table predates M42.10 and the hostility, dialogue and player-body systems

**Labels**: low,ecs,tech-debt,documentation,doc-rot

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 3 · **Status**: NEW · **Effort**: small · **Kind**: doc-rot
- **Location**: `docs/engine/game-loop.md:136-160` (last edited `d10c84338`, 2026-09-05)
- **Evidence**:
  - The PostUpdate row still reads "opt-in sandbox/wander/travel/follow/escort/guard/patrol systems |
    Environment-gated NPC locomotion experiments".
  - Since M42.10 (09-18) these systems run by default, with `BYRO_NO_AI_LOCOMOTION` as the single
    kill-switch (`boot/schedule/post_update.rs:70-90`, pinned at `schedule/mod.rs:256-292`).
  - The table has no row for `npc_walk_animation_system`, which is registered last by rule.
  - No row covers these window additions:
    - `make_faction_hostility_system` (update.rs:243)
    - `make_npc_combat_ai_system` (:272)
    - `player_body_facing_system` (:586)
    - `make_npc_dialogue_selection_system` (late.rs:428)
    - `fragment_activation_flush_system`
  - These older systems are also absent: `restoration_system`, `equipment_appearance_system`, and the
    `extension_*` dispatch family (late.rs:485).
- **Impact**: `_audit-common.md` lists game-loop.md as a code-verified runtime trace.
- **Suggested Fix**:
  - Rewrite the locomotion row as default-on, with the kill-switch.
  - Add group rows for Update combat/hostility, Late dialogue selection + equipment appearance +
    extension dispatch, and player-body facing.

**Validated at HEAD 9fcfdc3fc**: `docs/engine/game-loop.md` PostUpdate row still reads "opt-in sandbox/wander/…" / "Environment-gated NPC locomotion experiments" while `boot/schedule/post_update.rs:90` enables locomotion unless `BYRO_NO_AI_LOCOMOTION` is set; `grep` finds none of `npc_walk_animation_system`, `faction_hostility`, `npc_combat_ai`, `player_body_facing`, `dialogue_selection`, `equipment_appearance`, `restoration_system` in game-loop.md.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
