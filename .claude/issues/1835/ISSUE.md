# ECS-2026-07-02-02: Four new CHARAL/faction components stamped at NPC spawn share ActorValues' save-registry gap

**Labels**: low, ecs, bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1835
**Source**: docs/audits/AUDIT_ECS_2026-07-02.md

**Severity**: LOW
**Dimension**: 7 — Component Lifecycles (save/load)
**Location**: `crates/core/src/character/components.rs:16` (`CharacterLevel`), `:44` (`Perks`), `:91` (`Background`), `crates/core/src/ecs/components/faction_ranks.rs:26` (`FactionRanks`); write sites `byroredux/src/npc_spawn.rs:570,572` (`stamp_faction_ranks`, `stamp_character_components`); registry gap `byroredux/src/save_io.rs:155-186`

## Description
This week's CHARAL wiring (`b4ee7bfe` and follow-ups) made `spawn_npc_entity` stamp four more live-shaped components on every NPC placement root: `FactionRanks`, `CharacterLevel`, `Background`, and `Perks` (the last only when the NPC has FO4+ `PRKR` data). None derive `serde`; none are registered in `build_save_registry()`. `crates/scripting/src/condition.rs` now reads three of them (`GetLevel`/`GetXPForNextLevel` → `CharacterLevel`, `GetIsClass`/`GetIsRace` → `Background`, `GetFactionRank` → `FactionRanks`).

**Verified against source**: unlike `ActorValues`, none of these four types has a runtime write path anywhere in production code today — no `world.get_mut::<CharacterLevel/Perks/Background/FactionRanks>` call exists outside test modules, and no console command (`setlevel`/`addperk`/`setfactionrank`, etc.) exists. They are written exactly once, at spawn, from static ESM `NPC_` data, so a save→load cycle re-derives byte-identical values from the same ESM on NPC respawn — the round trip is a no-op, not a data-loss bug, today.

## Evidence
```rust
// components.rs:16 — no serde, same shape as ActorValues
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CharacterLevel { pub level: u16, pub xp: u32 }
```
```rust
// npc_spawn.rs:110-141 — stamp_character_components: three world.insert
// calls (CharacterLevel, Background, conditional Perks), zero world.get_mut
```
`grep` for `get_mut::<CharacterLevel>|get_mut::<Perks>|get_mut::<Background>|get_mut::<FactionRanks>` across `byroredux/src/` and `crates/` returns no production matches — confirms write-once-at-spawn (re-confirmed at publish time, 2026-07-02).

## Impact
None today (verified no active mutator). But `CharacterLevel.xp` documents itself as "resets on level-up" and `Perks` as "iterated by the perk entry-point modifier pipeline" — both signal near-term runtime mutation (leveling, perk-granting) is the intended next step for CHARAL, per its own module docs and the roadmap direction. The moment either lands, this becomes the same class of bug as the `ActorValues` save-registry gap (silent save→load revert of player-earned progress), and unlike `ActorValues` there would be four types to fix at once instead of one.

## Related
`ECS-2026-07-02-01` (same pattern, active for `ActorValues`); CHARAL character abstraction layer (`docs/engine/charal.md`); the M45 save registry's tripwire test (`save_io.rs:716`, `delta_columns_carry_only_session_stable_fields`) only guards `MUTABLE_DELTA_COLUMNS` entries already in the map — it has no coverage check that would have flagged an unregistered-but-spawn-stamped component type, so this class of gap is structurally invisible to CI until a save round-trip test exercises it. Genuinely new finding — did not exist before this week's CHARAL spawn-wiring commits.

## Suggested Fix
No urgent action required (no data loss today). When wiring the first runtime mutator for any of these four types (leveling XP gain, `AddPerk`/perk-rank commands, faction-rank changes), register that type in `build_save_registry()` + `MUTABLE_DELTA_COLUMNS` in the same commit that adds the mutator — do not let the pattern repeat a third time. Consider a lint/test that enumerates every component `npc_spawn.rs` stamps and cross-checks it against the save registry or an explicit "intentionally derived, not saved" allowlist.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (see `ECS-2026-07-02-01`, the same class of issue)
- [ ] **TESTS**: A regression test pins this specific fix (a spawn-stamp vs. save-registry cross-check, or extend the tripwire test)
