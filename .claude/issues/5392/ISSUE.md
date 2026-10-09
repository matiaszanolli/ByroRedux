# #5392: GAME-D2-2026-10-08-01: `forcegreet_open` has no `player_can_act` gate and no open-conversation guard — a dead player is force-greeted, and a force-greet hijacks a live conversation with another NPC

**Labels**: medium,gameplay,dialogue,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5392

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D2-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: MEDIUM
- **Dimension**: Interaction & dialogue (Dim 2)
- **Location**: `byroredux/src/systems/npc_dialogue.rs:477-507`; `byroredux/src/systems/forcegreet.rs:49-125`
- **Status**: NEW
- **Trigger**: Any game. A Dialogue-package NPC (GAME-D5-2026-10-08-01) or a console-installed directive while (a) the player is dead, or (b) the player is mid-conversation with another NPC.
- **Description**: The activation selection and the topic click both gate on `player_can_act` (#5043/#4701). The skill names a third dialogue path without that gate as the regression class. `forcegreet_open` checks only `npc_refuses_dialogue`, and `forcegreet_system` does not check the player at all, so the NPC walks to the player's corpse and opens a conversation. It also never looks at `DialogueSurfaceState.npc`. `apply_selection` then strips the current partner's `NpcDialogueTopic`, runs that partner's OnEnd fragment and re-points the surface, cutting off a conversation the player is in.
- **Evidence**: `if npc_refuses_dialogue(world, npc).is_some() { return false; }` is the only actor gate before `apply_selection(world, npc, selected, record)`.
- **Impact**: The dialogue surface opens over a death screen and the spoken line's OnBegin fragment runs. A live conversation is pre-empted mid-line, and the outgoing line's OnEnd fragment fires early. With the ambient wiring, any of the 60 always-winning NPCs triggers this.
- **Related**: #5043, #5367, GAME-D5-2026-10-08-01.
- **Suggested Fix**: Gate `forcegreet_open` (or `forcegreet_system`) on `player_can_act` and on no open surface (`DialogueSurfaceState.npc.is_none()`). Keep the directive pending rather than consuming it.

## Completeness Checks
- [ ] **SIBLING**: every dialogue-open path (activation, topic click, force-greet, console door) gates on player_can_act and open surface
- [ ] **TESTS**: A regression test pins this specific fix
