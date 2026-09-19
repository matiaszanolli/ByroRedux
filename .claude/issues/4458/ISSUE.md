# D5-08: D5-08: Player carries no ActorValues/ActorVitals in production — P3 consumables and drowning silently return Unavailable

- **Labels**: medium,character,gameplay,bug
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4458

---

**Source**: Not numeric — structural reachability: `byroredux/src/inventory.rs:918-923` (`consume_item` requires `world.get::<ActorVitals>(player)` and `world.get::<ActorValues>(player)`, else `MutationResult::Unavailable`) vs `byroredux/src/scene.rs:1715-1722` (`spawn_player_body` deliberately does NOT stamp ActorValues/CharacterLevel/Background on the player, deferring to #3004/#2986 per #3158's sibling note); exhaustive insert survey — the only production ActorVitals/ActorValues inserts are `npc_spawn.rs:121-130` (NPC placement roots) and `reference_state.rs:207-209` (whose `capture` explicitly skips the player at :99-101).

**Description**

The consumable pipeline built by `4c0053322` / `2e2f40b23` / `479163836` (catalog, `consume_item`, `TimedRestorations`, `restoration_system`) and the player drowning path (`apply_player_drowning_damage`, systems/character.rs:1371-1389) all gate on the player carrying `ActorValues`+`ActorVitals` — but no production path stamps either on the player. In a fresh session, "Use" on a Stimpak in the P3 native inventory UI returns `Unavailable` before the item is consumed — no effect, no notification. `TimedRestorations` can never attach, `restoration_system` has no eligible target, and the player cannot drown.

**Evidence**

Production-insert survey (grep of every insert of ActorVitals/ActorValues outside `#[cfg(test)]` → only the two NPC/persistence sites). The writers themselves are layer-correct (`restore()` touches only the damage layer, floored at 0), so this is a silent no-op class, not a revert class. All 12+ tests pass because they hand-insert the components (inventory.rs:1196-1199, character.rs:2055-2056, water.rs:1002, combat.rs:652, interaction.rs:1694 — all #[cfg(test)]).

**Impact**

The P3 consumable feature — the headline of commit `479163836` — is unreachable for its intended user in production until the player actor gets a populated `ActorValues` (+ `ActorVitals`). The disclosure exists at the stamp site (written before these consumers existed) but nothing at the consumer sites records the gate.

**Related**

#3158, #3004, #2986; charal.md §7 "No player chargen yet".

**Suggested Fix**

(a) Land the minimal player population slice the gate needs — stamp `ActorValues` (via `derive_npc_actor_values` on the player NPC_ record `build_player_template_for` already reads) and `ActorVitals` in `attach_to_player`, updating #3158's sibling note; or (b) until then, surface the Unavailable result in the P3 UI and document the gate at the consumers. (a) is the real fix and is already scoped by #3004/#2986.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D5-08, /audit-character 2026-09-19, HEAD `479163836`).*