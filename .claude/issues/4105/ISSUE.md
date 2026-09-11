# CHAR-2026-09-11-D5-03: `6b73c84d`'s `CreatureAttack` reaches the right component on the right entity and has no reachable reader — `attack_damage`'s only production caller is the player

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4105
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4105 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: 5 — Population boundary
- **Game**: FO3 / FNV
- **Location**: `byroredux/src/combat.rs:129-132`, `:250`, `:374-405`; `byroredux/src/npc_spawn.rs:114-145`; `crates/core/src/ecs/components/creature_attack.rs:1-44`; `docs/feature-matrix.md:271-274`
- **Source**: n/a (non-numeric). The *value* is sourced and correct — `CREA.DATA` `Damage` is `i16 @ 8` of the 17-byte block per xEdit `wbDefinitionsFNV.pas`, verified byte-for-byte against `VCrTier3GiantRadscorpionMedPers` `00167EA7` (`actor/mod.rs:241-258`); `stamp_creature_attack` reads that field, drops `<= 0`, and writes `CreatureAttack { damage: f32::from(stats.damage) }` on the placement root — all correct.

## Description

the fix's *reader* is unreachable. `attack_damage` has exactly one production call site (`combat.rs:250`), inside `combat_input_system`, and its `aggressor` is `world.try_resource::<PlayerEntity>()` (`combat.rs:129-132`). There is exactly one production `HitEvent` producer in the workspace (`combat.rs:251-257`; the file says so itself at `:277-280`, "this slice has exactly one HitEvent producer and it is always player-initiated"), and `grep -rn aggressor` over `byroredux/ crates/` outside `combat.rs` returns only the SDK dispatch *consumer*. `CreatureAttack` is stamped only in `spawn_placement_root`, i.e. never on the player. So no code path can reach `world.get::<CreatureAttack>(aggressor)` with a `Some`. ROADMAP corroborates: NPC combat is named as a subsystem that "doesn't exist in the engine yet" (`ROADMAP.md:1215`).

## Impact

no gameplay change from the fix, and — the part that matters for a future sweep — **no gameplay bug before it either**. Three places now assert a symptom that cannot occur: the commit message and `npc_spawn.rs:118-127` ("a Deathclaw hitting for 8 instead of 125"), `creature_attack.rs:17-22`, and `docs/feature-matrix.md:273-274` ("before that fix every creature in both games attacked for 8"). Creatures do not attack at all. A stale sibling: `CreatureStats::damage`'s own docstring (`actor/mod.rs:279-282`) still says the field is "parsed and left for a future combat consumer", which `6b73c84d` was supposed to close.

## Related

#3762 (CLOSED), #3390; `CHAR-2026-09-11-D1-01` (the same fix's TPLT bypass).

## Suggested Fix

keep the component and the stamp — both are correct and cheap — but re-word the three claims to say the value is now *available* to a combat consumer rather than that it fixed a live shortfall, and re-point `CreatureStats::damage`'s docstring at `CreatureAttack`. Record the real gap (no NPC/creature aggressor path) where a reader will meet it.

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix