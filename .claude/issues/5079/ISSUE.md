# #5079 — LC-D3-02: The FO3/FNV "Child" race-flag translation is duplicated in player_body.rs

**Labels**: low, bug, legacy-compat, tech-debt, character, game:fo3, game:fnv

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-09-29.md` — finding `LC-D3-02`

**Severity**: LOW

**Dimension**: 3 — Cross-game translation pattern (a per-game bitfield re-derived per consumer)

**Location**: `byroredux/src/player_body.rs:406-407`, which copies `byroredux/src/npc_spawn/resumable.rs:557-560`

**Status in report**: NEW. It landed in `db8351587`, and no report or issue mentions it.

## Description

- `RaceRecord::race_flags` is a raw per-game bitfield (`crates/plugin/src/esm/records/actor/mod.rs:655-662`). Bit 2 means Child on FO3/FNV and BeastRace on Oblivion.
- The NPC spawn path translates it as `matches!(game, GameKind::Fallout3NV) && race_flags & 0x04 != 0` and records why the game gate is part of the translation.
- The new player-body walk-clip resolution re-derives the same expression inline, with no shared helper. Its comment says it does this "exactly as `prepare_runtime_state` derives them".
- The mid-life gear import's `ActorBodyClass` (`npc_spawn.rs:1128`) retains gender and race but not the translated child flag, so each future consumer will need a third copy.

## Evidence

```rust
// player_body.rs:406-407
let is_child = matches!(game, GameKind::Fallout3NV)
    && race_flags.is_some_and(|flags| flags & 0x04 != 0);
// npc_spawn/resumable.rs:557-560
// FO3/FNV RACE DATA bit 2 is the authored Child flag. Oblivion reuses
// that bit for BeastRace, so the game gate is part of the translation.
let is_child = matches!(game, GameKind::Fallout3NV)
    && race.is_some_and(|race| race.race_flags & 0x04 != 0);
```

## Impact

- Both copies agree today.
- The risk is drift, which is the per-game-branch sprawl the translation survey warns about. Suppose one site is widened (for example, a Skyrim child race gaining a KF walk path) or tightened (for example, FO4's `HumanChildRace` semantics under `#2455`) and the other is not. The player and NPCs of the same race would then pick different body or walk variants.

## Related

`#2455` (FO4 RACE flag semantics unverified); `docs/engine/per-game-translation-survey.md` §5.

## Suggested Fix

- Add one helper, for example `npc_spawn::is_child_race(game, race_flags)` or an accessor on the RACE side, and call it from both sites.
- Optionally, also carry the translated flag on `ActorBodyClass` so runtime consumers read the canonical value.

Validated at HEAD 9fcfdc3fc: `byroredux/src/player_body.rs` derives `is_child = matches!(game, GameKind::Fallout3NV) && race_flags.is_some_and(|flags| flags & 0x04 != 0)`, an inline copy of `npc_spawn/resumable.rs`'s expression; no shared helper exists.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other consumer of `race_flags & 0x04`)
- [ ] **TESTS**: A regression test pins this specific fix (the helper keeps Oblivion BeastRace distinct from FO3/FNV Child)
