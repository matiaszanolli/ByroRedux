# #4323 SCR-D5-2026-09-14-03: `StartCombat` accepts the player as the combatant; `npc_combat_ai_system` would then drive the player's Transform and auto-strike

**Labels**: medium,scripting,combat,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs` `prim_start_combat` (`actor: receiver_actor(object, scope)?`); `crates/scripting/src/fragment/effects.rs` `Effect::StartCombat` arm; `byroredux/src/systems/combat_ai.rs` `npc_combat_ai_system`
- **Status**: NEW
- **Description**: `receiver_actor` maps `Game.GetPlayer()` or a player local to `ActorRef::Player`. `prim_start_combat` accepts that as the receiver, and dispatch inserts `AiCombatState` on the player entity. `npc_combat_ai_system` has no player exclusion: it overwrites that entity's `Transform` with a straight-line chase and emits `HitEvent`s on cooldown. The runtime models only an NPC chase-and-strike, so a player receiver is unmodeled and should be declined. The real engine behavior of `Player.StartCombat` is **UNVERIFIED** (CK wiki unreachable), but it is not "AI moves the player".
- **Evidence**: `Actor.StartCombat(Actor akTarget)` (probe). `start_combat_declines_on_wrong_arg_count` uses a player receiver but tests only arity.
- **Impact**: A fragment of the form `Game.GetPlayer().StartCombat(X)` fights the character controller for the player's Transform. Reachability in vanilla or mods was not measured; MQ101's pinned shapes use alias NPC receivers.
- **Related**: SCR-D5-2026-09-14-01, SCR-D6-2026-09-14-01
- **Suggested Fix**: Decline when the receiver is `ActorRef::Player` (optionally also when receiver == target), with a correct-arity decline test.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
