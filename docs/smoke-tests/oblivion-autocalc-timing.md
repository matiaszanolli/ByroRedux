# Oblivion auto-calc timing — observation protocol (#7022)

Owner-run, in the **original** `Oblivion.exe` (not the engine). It settles the one thing the executable decode
leaves open: what an NPC's stats read in the running game, for each combination of the ACBS flags
**Auto-calc stats** (bit 4, `0x10`) and **PC Level Offset** (bit 7, `0x80`), at load, after the player's level
changes and after a cell reload. The decode (which code path runs when) is in
[charal-oblivion-ruleset.md](../engine/charal-oblivion-ruleset.md#npc-auto-calc-stats--locked-from-the-executable-7008-oav-pop-003),
under *When the calculator runs*; this protocol checks its predictions.

## 1. The plugin

```bash
scripts/oblivion-autocalc-plugin.py --data "$BYROREDUX_OBLIVION_DATA" --out Byro7022Autocalc.esp
```

It writes four `NPC_` records cloned from one vanilla guard (`AnvilGuardCityPostDay02`, no packages), form ids
`01000800` to `01000803` with `Oblivion.esm` as the only master. Authored stats are plainly not calculator output:
every skill and attribute 90, Health 999, spell points 777, Fatigue 888. Levels are 5 (A, B) and an offset of +2
(C, D).

| NPC | EDID | Flags | Authored level field |
|---|---|---|---|
| A | `Byro7022A` | none | 5 |
| B | `Byro7022B` | auto-calc | 5 |
| C | `Byro7022C` | PC level offset | +2 |
| D | `Byro7022D` | both | +2 |

Copy the `.esp` into the game's `Data` folder and tick it in the launcher's Data Files. Remove it afterwards;
nothing else is changed. Record the executable (`sha256sum Oblivion.exe`, the local one is
`a8f313845c1545e9a60e1e995961eef4c033115da9443f6d756341df3c2b7dc6`), the plugin's hash and the load order.

## 2. What the decode predicts

Read from `Oblivion.exe` (the calculator `0x5222d0`; the one-time init `0x523f40`; the bit-7 walker
`0x447300`): *authored* means the 90 / 999 / 777 / 888 values, *calculated* means anything else.

| NPC | At first load | After the player's level-up | After save and load | After `coc` away and back | After `player.setlevel` |
|---|---|---|---|---|---|
| A | authored | authored | authored | authored | authored |
| B | calculated at level 5 | unchanged | unchanged | unchanged | unchanged |
| C | calculated at player level + 2 | recalculated at the new level | recalculated at the loaded level | unchanged | unchanged |
| D | calculated, then recalculated as C | recalculated as C | recalculated as C | unchanged | unchanged |

A new game loads with the `Player` record's level (1), then a loaded save recalculates C and D at the saved
character's level, so the first-load row differs between the two ways of starting.

## 3. The observation

For each of A to D, with the console (`~`): `player.placeatme <form id>`, click the new NPC, then read
`getlevel`, `getav Health`, `getav Magicka`, `getav Fatigue`, `getav Strength`, `getav Endurance`,
`getav Blade` and `getav Armorer`. Fill one row per event:

| Event | How | NPC | level | Health | Fatigue | Magicka | Str | End | Blade | Armorer |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 first load | place them right after the save loads or the new game starts | A, B, C, D | | | | | | | | |
| 2 level-up | `player.advlevel`, spend the three attribute picks, close the menu; read the same NPCs | A, B, C, D | | | | | | | | |
| 3 save and load | save, quit to the menu, load; read the same NPCs | A, B, C, D | | | | | | | | |
| 4 cell reload | `coc` to another interior and back; read the same NPCs | A, B, C, D | | | | | | | | |
| 5 `setlevel` | `player.setlevel 10`; read the same NPCs | A, B, C, D | | | | | | | | |

Notes: use a fresh save and the same one for every run; do the events in order, one save after event 1 so a
second run can start from it; the level-up needs the level-up screen, not `setlevel` (the executable's
`SetLevel` skips the recalculation for the player). Screenshots of the console are enough as evidence.

## 4. Resolving it

Paste the filled table into #7022. Each cell that differs from section 2 is a finding: record it in
`docs/engine/charal-oblivion-ruleset.md` (*When the calculator runs*), and where it changes what the engine
stamps (`oblivion_population.rs`, spawn-time derivation), change the code and add a test pinned to the observed
numbers. Where every cell agrees, the decode stands and the section's wording drops "predicted".
