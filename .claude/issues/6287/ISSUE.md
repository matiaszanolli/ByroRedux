# #6287: GAME-D2-2026-10-21-01: The scalar and bulk dialogue-refusal checks disagree for knocked-down, fleeing and AI-disabled actors

**Labels**: low,gameplay,dialogue,bug,game:fo3,game:fnv,game:oblivion

**Source**: docs/audits/AUDIT_GAMEPLAY_2026-10-21.md · **Severity**: LOW · **Dimension**: Interaction & Dialogue (Dim 2, the #5043 / #5017 refusal contract) · **Location**: `byroredux/src/systems/npc_dialogue.rs:647-659` (`npc_refuses_dialogue`, uses `is_unconscious`), `:661-698` (`collect_dialogue_refusals`, uses `collect_unconscious`); `crates/scripting/src/player_control.rs:222-256`

**Audit status**: NEW (`8d32b3fe9` folded `AiSuspended` and `SetActorsAI 0` into `collect_unconscious` and not into `is_unconscious`)

- **Description**: `collect_dialogue_refusals`' doc says the bulk form and the scalar "must always check the same three conditions". The bulk form now also refuses every `AiSuspended` actor (knocked down, fleeing) and every `SetActorsAI 0` actor. The scalar refuses neither. The scalar form is the one the following paths use:
  - activation selection (`:754`);
  - the per-frame "partner can no longer hold a conversation" head (`:609`);
  - topic clicks (`:816`);
  - `forcegreet_open` (`forcegreet.rs:165`);
  - `begin_barter` (`byroredux/src/barter.rs`).
- **Impact**: The HUD offers no "Talk" on a knocked-down or fleeing NPC. A scripted activation or `StartConversation` still opens a conversation with it, and a conversation already open stays open when the partner is knocked into a ragdoll or starts to flee. No test pins the two forms against each other.
- **Suggested Fix**: Decide the rule once (most likely: `AiSuspended` refuses, and `SetActorsAI 0` follows the GECK's behavior) and express it in a shared predicate both forms call. Add a parity test over the same state matrix.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every other scalar/bulk predicate pair built on `is_unconscious` / `collect_unconscious`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
