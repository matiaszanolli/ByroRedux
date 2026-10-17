# CHAR-2026-10-16-L-02: Script `SetActorValue`/`SetAV`/`SetValue` writes the base of the player's derived pools; the per-frame refresh reverts it (third writer of the #5239 class)

Labels: low,character,bug,scripting,game:fo4
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: LOW (reach: FO4 Papyrus; Skyrim has no `PlayerOnly`+`Absolute` row)
- **Dimension**: Population Boundary (writers vs derived stamps)
- **Game**: fo4
- **Location**: `crates/scripting/src/fragment/effects/evaluate.rs` `set_actor_value_base` (called at `fragment/effects.rs:492` and `:513`)
- **Status**: NEW (Related: #5239, #5412, which routed only `setav` and the SDK `SetBase`)
- **Source**: FO4 CK `SetValue` "modifiers are left intact", quoted in `charal.md` §6 / `ruleset.rs` (the #5412 reroute rationale).
- **Description**: `set_actor_value_base` calls `ActorValues::set_base` unconditionally. For the FO4 player's Health/AP, `refresh_player_only_bases` overwrites `base` with the formula on the next `player_derived_stats_system` tick. The console (`commands/actor_value.rs:88`) and SDK (`extensions/commands.rs:467`) both ask `CharacterRuleset::is_player_derived_pool` first; this writer does not. `#5862` added `queue_health_death` after the write, so a lethal write still kills, but a non-lethal one is dropped within a frame.
- **Evidence**: `grep is_player_derived_pool` shows exactly two call sites; `set_actor_value_base` has no ruleset read.
- **Impact**: A Papyrus `SetValue(Health, x)` / `SetActorValue("ActionPoints", x)` on the FO4 player reports no error and is undone in one tick.
- **Suggested Fix**: Route the player-derived-pool case into `set_override` here too (read the ruleset before taking the `ActorValues` write guard, #3441 order); better, hoist the "route SetBase" decision into one helper that the three writers share.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (read the ruleset before taking the `ActorValues` write guard, #3441)
- [ ] **TESTS**: A regression test pins this specific fix
