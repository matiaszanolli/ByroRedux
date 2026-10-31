# GAME-D5-2026-10-31-01: A random-conversation partner is orphaned in `ConversationHold` when its initiator is despawned or double-booked, and stays frozen

Labels: medium,gameplay,ai,bug,game:skyrim,game:fo4

Filed from `docs/audits/AUDIT_GAMEPLAY_2026-10-31.md` (finding `GAME-D5-2026-10-31-01`).

- **Severity**: MEDIUM
- **Dimension**: NPC Spawn -> AI Package (Dim 5, M43 random conversations)
- **Location**: `byroredux/src/systems/random_conversation.rs:446-496` (`random_conversation_system` / `begin_conversation` / `release_pair`), `:548-700` (`conversation_hold_inner`); consumer `byroredux/src/npc_spawn/ai_package.rs:1160-1162`
- **Status**: NEW (`b0591ba88`, 2026-10-30)
- **Trigger**: Skyrim and Fallout 4 (an `ADIA` root). (a) The initiator's cell is unloaded by streaming while the partner stays resident (exterior radius 2000 BU). (b) Two actors in one social radius come due in the same one-second pass, so one is both a target and an initiator.
- **Description**: Only the initiator's hold is driven. `conversation_hold_inner` collects `hold.initiator` entries and, per initiator, decides the step for both; `release_pair` removes the holds of the two actors it is given. The partner's `ConversationHold { initiator: false }` is never examined on its own. (a) When the initiator's entity is despawned, its hold goes with it and the pass has nothing to release; the partner's hold stays. (b) `collect_participants` filters held actors once, at the start of the pass, and `pick_target` is run over that fixed list for every due initiator with no "already claimed in this pass" set (only the `(initiator, target)` cooldown pair). If A picks B and B (also due) picks C, `begin_conversation(A, B)` then `begin_conversation(B, C)` overwrites B's hold. When A's hold later releases (`Waiting` with no scene, `MAX_APPROACH_SECS`), `release_pair(A, B)` removes B's initiator hold, which leaves C's partner hold without a driver.
- **Evidence**: `release_pair` is `holds.remove(a); holds.remove(b);` and nothing else clears a `initiator: false` hold. `ambient_ai_package_system` does `due.retain(|&actor| held.get(actor).is_none())`, and `collect_participants` drops held actors, so the orphan is never re-selected a package and never talks again. `grep` finds no other `ConversationHold` consumer outside `random_conversation.rs` and `ai_package.rs:1160`.
- **Impact**: The orphaned NPC stands with its package cleared (`suspend_ambient_behavior_for_combat` ran in `begin_conversation`) for the rest of the session, until a load drops the unsaved component. It still fights and dies normally, since combat does not read the hold. A town NPC standing idle forever is visible. Not measured: how often the double-booking case occurs. The despawn case needs only a stream boundary.
- **Related**: GAME-D4 family in the 10-27 report (a new runtime missing a rule the older paths enforce); `ConversationHold` allowlist reason says "a load drops the conversation".
- **Suggested Fix**: Add a partner-side sweep in `conversation_hold_inner` that removes a non-initiator hold whose `partner` lacks the reciprocal hold (or is not alive/placed). Claim actors during the due loop so one pass cannot book an actor twice. Add both cases to the tests.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
