# Music

**Status**: SHIPPED (2026-11-14). The design the [Game Music PRD](../prd/game-music.md)
names as the owner of the music controller (PRD *Related*). It also covers the music
half of the `audio` crate, which has no design document of its own. Tracking: #6860;
phases #6861 (0, this document), #6862 (1), #6863 (2), #6864 (3). The PRD states the
promise and the `MUS-` requirements. This document states how the engine keeps them.

## 1. Pieces

| Piece | Where | Role |
|---|---|---|
| Records | `byroredux-plugin` | The sources, as authored: cell `XCMO` / `XCMT` (`CellData::music_type_form` / `music_type_enum`), worldspace `ZNAM` / Oblivion `SNAM` (`WorldspaceRecord::default_music` / `music_category`), region `RDMO` / `RDMD` / `RDSB` (`RegionDataPayload::Sound`), the five `DOBJ` music slots (`EsmIndex::default_music`), `MUSC`, `MSET`, `ALOC` (`EsmIndex::media_locations`) and the New Vegas audio-marker block on a placed reference (`PlacedRef::audio_marker`) |
| Files | `asset_provider::SoundArchiveProvider` | Music paths resolve loose first, then in the sound archives (MUS-SRC-011, -013); a folder form expands to the playable files directly in it (MUS-SRC-010) |
| Translate | `byroredux/src/music/translate.rs` | Each game's sources become a `MusicDirective`. The only per-game code (R5) |
| Controller | `byroredux/src/music/controller.rs` (`MusicController`, `music_system`) | One directive per layer; decides what is audible; picks tracks; fades; reports |
| Voices | `byroredux_audio::AudioWorld` music voices | Streamed, non-spatial tracks on the main track: no reverb send, no underwater filter (MUS-MIX-003). The controller owns every voice; nothing else plays music |
| Observable | `music.status` (`byroredux/src/commands/music.rs`) | MUS-RPT-001 |

## 2. Directives, layers and outcomes

A **directive** is what translation produces and the controller plays:

- its **source**: the record that produced it (EditorID and FormID), or the Oblivion
  category / keyword;
- its **content**: one of
  - **Playlist**: one file or a folder's files, played one after another (or one
    file looping);
  - **Stems**: a New Vegas Location set's tracks, played together (§6);
  - **Silent**: the data asks for silence (`1NoMusic`, or no source at all);
  - **Unresolved**: a source this build does not translate, with the reason;
  - **Missing**: a path the install lacks;
- its **loop** flag, **level** (linear), **fade** time, and where they differ the fade
  for **replacing** another directive (Oblivion's cell change is instant) and Oblivion's
  **category**; for the Script layer, whether **Battle ends it** (MUS-CMB-004), whether it
  **plays on** past a track, and what **follows** it (Oblivion `StreamMusic`, §4).

The controller holds at most one directive per layer (PRD §3.1):

| Layer | Fed by | While active |
|---|---|---|
| Hold | `AudioWorld::movie_hold().music_paused`, raised by a movie with the pause-music flag (BIK-PRES-005) | Every voice paused; resumed at the same position when the movie ends (MUS-MIX-002) |
| Sting | The player's death (the player gains `Dead`), a level-up (`PendingMusicEvents`, pushed where the level is awarded) | In the music's place: Script ended, Battle and Location stopped and silent until the place changes, combat starts or a script plays; Fallout 3 / New Vegas's level-up sting plays over the music instead (MUS-STG-004, #6901) |
| Script | `PlayMusic` (`PendingMusic`), `StreamMusic` | Replaces Battle and Location: a scripted track plays in their place, a scripted silence (`1NoMusic`) silences them (MUS-SCR-005, #6899); Battle ends an Oblivion stream |
| Battle | `is_in_combat` for the player (Q2), held for the recovery time after it ends (MUS-CMB-002) | Location paused, crossfading (MUS-SEL-004) |
| Location | The player's place (§3) | — |

A paused voice keeps its position: a layer that a higher one suspended resumes where it
stopped, it is not restarted. A higher layer crossfades over the one it suspends; the Hold
layer pauses and releases at once, so the music stops where the movie begins. A layer's **outcome** (PRD §3.2) is **Playing**, **Silent**,
**Unresolved** or **Missing**; a Playing layer under a higher one reports
`Playing (paused under <layer>)`.

### Track selection (MUS-SEL)

A playlist plays one track at a time. Its order is a shuffle (Q1): every track of the
folder plays once before any repeats, and a new round never starts on the track the last
one ended on. A one-track playlist and a looping directive loop the track. When a
non-looping track ends, the next track of the same directive starts (MUS-SEL-002). A
new Location directive equal to the playing one (same source, same files) does not
restart it (MUS-SEL-005); a different one crossfades over the directive's fade time
(MUS-SEL-004).

## 3. The Location layer per game

The controller re-translates the Location layer whenever the player's **place** changes:
the interior cell (`CurrentCellContext`) or the exterior worldspace and grid cell
(`CurrentExteriorContext`), or the loaded content (`LoadedCellIndex`). On New Vegas it
also follows the audio-marker area the player stands in (§6).

| Game | Order (first authored wins) | Default |
|---|---|---|
| Fallout 3, New Vegas | cell `XCMO` → worldspace `ZNAM` (an exterior's) → `DOBJ` Default Music (MUS-SRC-001, -002; #6900) | — |
| Oblivion | cell `XCMT` (or an OBSE override) → an exterior's winning region's `RDMD` → worldspace `SNAM`, a Default value falling through (MUS-SRC-008, #6903); the category picks `Data\Music\Public`, `Dungeon` or, for Default, `Explore` (MUS-SRC-009, Q6) | Default |
| Skyrim and later | **Unresolved** ("xWMA music waits on its own PRD", MUS-NG-001) | — |

On Oblivion the winning region is the one the engine's region selection already picks
(`select_active_region_sound`: override flag, then priority). A music type with no file
is **Silent** (MUS-SRC-004).

The Oblivion order is `Oblivion.exe`'s (#6903). `TESObjectCELL::GetMusicType` (`0x4cad00`)
returns the cell's `ExtraCellMusicType` (extra `0xb`, which `XCMT` and OBSE's
`SetCellMusicType` set); if that is 0 and the cell is an exterior, the music type of the
sound region data (type 7) at the player's position; if that is 0, the worldspace's
(`SNAM`, `TESWorldSpace+0x94`). Each step reads a value, so a Default one falls through,
and an interior reads no region. The folder switch (`0x6a8e80`) maps 0 and 3 to
`Explore\`, 1 to `Public\`, 2 to `Dungeon\` and 4 to `Battle\`. On a cell change
(`0x66eaf0` → `0x6ab160`) the music keeps playing if the new type is the playing one;
otherwise the old track stops and one of the new type's starts **at once**, with no fade:
an Oblivion Location directive replaces another instantly.

The Fallout 3 / New Vegas order is `Fallout3.exe`'s (#6900): `TESObjectCELL::GetMusicType`
(`0x745460`) returns the cell's `ExtraCellMusicType` (`XCMO`), else, for an exterior cell,
its worldspace's music (`ZNAM`, `TESWorldSpace+0x9c`, not inherited from a parent
worldspace); the cell-change handler (`0x8bfbf1`) plays that, and a null music type plays
`GetDefaultObject(DefaultMusic)` (`0x8e10b0`). No music path reads a region: the region
loader keeps `RDMO` but nothing plays it (no vanilla region authors one), and it reads
`RDMD` into a local and drops it, as the cell loader drops `XCMT`, so Fallout 3's 17 legacy
`XCMT` cells and 2 `RDMD` regions have no effect. New Vegas's code is SteamStub-encrypted;
it keeps the shared `BGSMusicType.cpp` and the same music strings, and authors no
`RDMO`, `RDMD` or `XCMT`, so the same order is taken.

A music type whose folder is absent has an empty file list; its `GetFileName` falls back to
the bare `FNAM` folder path, the stream open fails, and the music routine (`0x8e2680`) has
already stopped the old track: silence. That is **Missing** here, which also stops the
old track. So a New Vegas `DefaultDungeon` cell outside any audio marker (`Vault19a`) is
silent, as in the original: New Vegas ships none of the `Default*` folders.

A music type's level: Fallout 3 authors no `ANAM`. New Vegas's sign marks looping
("dB (positive = Loop)"); its magnitude is played as an attenuation, `-|ANAM|` dB, for
either sign (MUS-SEL-003; #6902).

## 4. The Script layer

- **`PlayMusic <music type>`** (Fallout 3 / New Vegas) translates the type like a
  Location source, folder form included (MUS-SCR-001). The directive replaces the
  previous scripted one; `PlayMusic 1NoMusic` is a Silent directive, which silences the
  music until the layer ends (New Vegas's `VVault11ExterminationChairSCRIPT` does exactly
  that).
- **`StreamMusic "<file>" | Public | Explore | Dungeon | Random | Battle`** (Oblivion,
  opcode `0x104e`, a string-form command; `Oblivion.exe` runs it at `0x509610`, #6903):
  - a keyword makes its category the music's (`0x6acd10`, the 1.7 s transition of §7):
    `Random` rolls one of four (Explore, Public, Dungeon, Explore, so Explore is twice as
    likely). The music plays on from that folder after a track, as the update replays the
    current type;
  - a quoted path relative to the game folder plays at once (`0x6ab160`) as Default
    music, so when it ends the Default music (`Explore\`) plays on;
  - `Battle` is ignored: the update ends battle music whenever the player is not in
    combat (`0x6ad9fc`), and in combat it is already playing.

  A stream outlives a cell change into its own category (a path's is Default) and ends at
  any other: the cell change compares types, not cells. Battle ends it: combat switches
  the music to Battle (`0x6136b0` → `0x6acd10`), and when combat ends the place's music
  returns, not the stream's; a path streamed in combat is dropped ("StreamMusic will also
  not override combat music", CS *StreamMusic*). A missing file is **Missing** and the
  playing track keeps playing (PRD §6 row 5).
- **OBSE `GetCellMusicType` / `SetCellMusicType`** (Oblivion, opcodes `0x1617` / `0x19e5`
  by xOBSE's command-table order; MUS-SCR-004) read the player's cell's own category and
  give a cell another one. The override wins over the cell's `XCMT` in the translate step
  and re-resolves Location at once; it lasts the session, like OBSE's change to a cell.
- The layer ends when its track ends (a non-looping directive does not advance to a
  next track), on a cell change and on a load (MUS-SAV-002); an Oblivion stream ends by
  the rules above instead, a load counting as a cell change.
- The movie's soundtrack is not on the Script layer: it has its own voice in the audio
  world (MUS-SCR-002, BIK-PRES-003).

A scripted track **replaces** the music (#6899): the vanilla scripts are written for it.
Fallout 3's `MQ05Script` and `MQ11Script` call `playMusic defaultBattle` "every frame to
ensure it remains queued", then "cut the music" with `playMusic defaultExplore`. New
Vegas's `VVault11ExterminationChairSCRIPT` drops a commented-out `PlayMusic
MUSVault10106` for `PlayMusic 1NoMusic` before enabling the sound reference that carries
the slideshow music. `VCG00` silences the game start the same way. While the Script
layer holds a track or a silence, Battle and Location are paused, and they resume when
it ends. That the layer ends on a cell change is the engine's rule (the scripts are
silent on it). `PlayMusic` does not give way to battle (the Fallout 3 scripts queue
battle music themselves); Battle ends a `StreamMusic` stream.

## 5. Battle and stings

- **Battle (MUS-CMB-001, -002).** While the player is in combat, the Battle layer plays
  the game's battle directive: Fallout 3 / New Vegas the `DOBJ` Battle Music object,
  New Vegas the active controller's Battle sets first and the region's `RDSB` set next
  (§6), Oblivion `Data\Music\Battle`. When combat ends, the layer holds for the recovery
  time (§7), then Location resumes. Oblivion has none: it counts the actors whose combat
  targets include the player (`0x6136b0` / `0x6136e0`) and switches back when the count
  reaches 0.
- **Stings (MUS-STG, #6901).** Oblivion plays `Special\death.mp3` on the player's death
  (`0x600b7d`) and `Special\success.mp3` on a level-up (`0x5ad005`, the "Success" track
  UESP lists), both on its one music graph (`0x6ab160`), which stops the track that was
  playing. Fallout 3 and New Vegas play sounds, not music types: `MUSDeath`
  (`fx\mus\mus_death.wav`) when the player dies, after stopping the music (`0x7e5d7d`,
  `0x8e52b0`), and `MUSSuccess` (`fx\mus\mus_success.wav`) when the level-up menu opens
  (`LevelUpMenu::Update`, `0x93cdd0`), over the music. `Fallout3.exe` never reads the
  Death, Success or Level Up Music objects: no `GetDefaultObject` call names slot 11, 12 or
  13 (MUS-STG-003). A sting plays once. The level-up sound leaves the music playing;
  every other sting ends the Script layer and stops Battle and Location, which stay
  silent (`music.status`: "paused under a sting's silence") until the place changes,
  combat starts or a script plays, and then start a fresh track (MUS-STG-004): Oblivion's
  update replays the sting's type, a no-op, until one of those changes the type. Both drivers are the ones the console
  reaches (MUS-RPT-004): `levelup xp <n>` awards levels through the same loop that
  pushes the level-up event, and `obscript.cmd 0x14 108b` (`KillActor` on the player)
  inserts `Dead`, which the controller watches on the player. Oblivion's level-up sting
  is translated but has no driver yet: Oblivion levels by major-skill increases, which the
  engine does not model (`levelup xp` awards no level under classic-TES rules).

## 6. New Vegas media sets

An **audio marker** is a placed reference with an `MMRK` block: its controller (`CNAM`
→ `ALOC`), its "Use Controller Values" flag and two layer triggers (`MNAM` / `NNAM`),
and the reference's `XRDS` radius. Every one of the 572 vanilla markers carries a
radius (402 at 5000 units); that sphere is the marker's **area** (MUS-SRC-005). Of
several areas holding the player, the one the player is deepest into (smallest
distance ÷ radius) wins (#6902).

A controller's Location sets become the Location directive while the player is in its
area. A Location set is **Stems**: up to six tracks, day Outer / Middle / Inner and
night Outer / Middle / Inner, each with a level (dB), a boundary percentage and an
enable bit (`PNAM`). The controller plays:

- the **day** stems between the controller's day start and night start, the **night**
  stems otherwise, against `GameTimeRes` (MUS-SRC-006). The starts are `value / 256` of
  a day (xEdit's `wbAlocTime`); every vanilla controller authors 64 and 255, 06:00 and
  23:54;
- of those, every enabled stem whose boundary holds the player: the player's distance
  from the marker is at most `boundary %` of the radius. Outer is 100 % on every vanilla
  set, Middle and Inner smaller (66.6 / 33.3 on most), so the stems stack as the player
  walks in (MUS-SRC-007);
- the stems **mix**, each looping, each faded in or out over the set's layer crossfade
  time (`FNAM`, 6 or 9 s on the vanilla sets) as it turns on or off. The tutorial reads
  the layers as stacking ("each part is a trigger that controls when each layer plays
  while within the radius"), and the vanilla stems are written to stack
  (`mus_loc_desexpl_day_1low` / `_2mid` / `_3high`).
- a marker's own layer triggers (`MNAM` / `NNAM`) replace the set's boundaries when its
  "Use Controller Values" flag is clear (146 of the 572 base-game markers); the tutorial
  says only that triggers of zero leave the Outer layer alone. Their exact reading is
  #6902, so the engine uses the set's boundaries for every marker.

A controller's Neutral, Ally, Friend and Enemy lists play by how its **conditional
faction** (`RNAM`) treats the player: "music that plays ... when you're in an area
controlled by an Enemy Faction (relates to Conditional Faction)" (Nexus wiki *Fallout New
Vegas Music*, 2014). The relation is the engine's one faction rule
(`faction_reaction_toward`: authored `XNAM` relations from the faction to the player's
factions, scripted `SetEnemy` / `SetFactionReaction` overrides first), re-read every
frame, so a faction turning hostile switches the area's music. The relation's list
plays, else the Location list, else the Neutral list; a controller with no conditional
faction is Neutral. Nine of the 689 markers sit under controllers with relation lists and
no Location set, `musCtrlGoodsprings` among them (#6904). `NAM1`'s option to force one
list whatever the relation is not decoded (xEdit: "Flags and Enums, messily combined").
A controller with several sets in a list plays the first; one with Battle sets only
(169 markers) leaves the place's own music playing.

Twenty-three controllers list a **Dungeon** set among their Location sets. A Dungeon
set's slots are Battle, Explore and Suspense (xEdit's `(D)` labels), not layers: its
Explore track (`NAM3`) plays, looping; when Suspense and Battle play is #6902.

A controller's Battle sets, else the region's `RDSB` set (the precedence is #6902), feed
the Battle layer (MUS-CMB-001): the set's loop track (`NAM2`), its Intro (`HNAM`, a
`SOUN`) on entering combat and its Outro (`INAM`) on leaving it, and its recovery time
(`FNAM`, 10 s on every vanilla Battle set) as the hold (MUS-CMB-002). The tutorial
describes Intro and Outro as the sounds that open and close the battle piece, "compared
to the Finale folder" Fallout 3 played as combat ended (MUS-CMB-003).

The controller's `NAM1` also holds its looping mode (the tutorial's Loop, Random,
Re-trigger, None) beside the forced-relation option. In the base game, bit `0x10` is set
on 38 of the 89 controllers, including 26 of the 29 with several Location sets, so it
may be Random. Nothing confirms that, so the engine plays a list's first set.

## 7. Timing and level

| Value | Fallout 3 | New Vegas | Oblivion | Source |
|---|---|---|---|---|
| Crossfade between directives | 1 s | 1 s; a media set's layer crossfade time | 0.85 s out, then 0.85 s in; a cell change at once (#6903) | `iMusicTransitionInMS:Audio`, compiled-in default 1000 in `Fallout3.exe` (read 2026-11-13 by the pointer method, [video-playback.md §5](video-playback.md#5-start-sequences-bik-req-004--005)); neither default INI sets it. `fASFadeInTime` and `fRegionLoopFadeInTime` are the ambient-sound and region-loop fades, not music. `Oblivion.exe` plays music through one DirectShow graph: a transition (`0x6acd10`) fades the old track out linearly over the first half of 1700 ms (`0x6a4`, the divisor `1700.0` at `0xa771c0`), starts the new track once the level is under 0.05, and fades it in over the second half; here the two halves overlap |
| Battle recovery | 5 s | 3 s; a Battle set's recovery time | none (#6903) | GMST `fCombatMusicStopTime`: compiled-in 5.0 in `Fallout3.exe`, overridden to 3.0 by `FalloutNV.esm`; `Oblivion.exe` has no such setting and ends battle music when combat ends (§5) |
| Default music level | 0.3 | 0.6 | 0.3 | `fDefaultMusicVolume` in the shipped default INI (Q4); Oblivion's `bMusicEnabled=0` maps to 0 |

The level is the `audio.music_volume` setting (MUS-MIX-001). Its default is seeded from
the game's default INI before stored settings load, so a launch with no stored value
starts at the game's own mix, and a value the user sets is kept. It scales every music
voice and nothing else.

## 8. Reporting and headless runs

`music.status` prints the place and the audio marker whose area holds the player, the
Hold flag, and one line per layer: its outcome, its source (EditorID, FormID, the field
that named it), the file playing and its position (MUS-RPT-001). Each Unresolved
source and each Missing path logs once per session (MUS-RPT-002). With no audio device
(a headless `--bench-hold` run) the controller runs the same way: directives resolve,
tracks are chosen, and a voice's position is the controller's own clock, which a paused
layer stops (MUS-RPT-003). Tracks never end on that clock, so a headless run reports the
first track of each directive.

### Census and gates

The ignored `music::census_tests` (`byroredux`) translate every music source of each
game's master and official DLC (PRD §7.2 scenario 1, §8), 2026-11-14:

| Game | Sources | Source coverage | Playable coverage |
|---|---|---|---|
| Fallout 3 | 591 cells, 16 worldspaces, 2 default objects (Default, Battle; #6901), 31 music types | 100 % | cells 99.8 % (one `.wav` the install lacks), music types 93.3 % |
| New Vegas | 375 cells, 3 worldspaces, 14 region battle sets, 2 default objects, 520 audio-marker areas | 100 % | markers and battle sets 100 %; cells, worldspaces and default objects 0 %: the `Default*` folders are not shipped, and the original is silent there too (§3) |
| Oblivion | 1919 cells, 39 worldspaces, 128 regions | 100 % | 100 % |

`docs/smoke-tests/music-fo3.sh` drives scenarios 2 to 5, `music-fnv.sh` scenario 6 (at
Sloan, and at Goodsprings its Neutral and, after `SetFactionReaction`, Enemy set) and scenario 5's New
Vegas leg, `music-oblivion.sh` scenario 7.

## 9. Saves

A save keeps no music state (Q5, MUS-SAV-002). After a load, the place changes, so the
Location layer re-resolves on the first frame of the loaded place (MUS-SAV-001); Battle
follows combat; a scripted track ends. Every music resource is in the save registry's
not-saved list with its reason.

## 10. Phase 0 verify items

| Item | Settled by | State |
|---|---|---|
| MUS-SRC-005 area shape and radius | `XRDS` on every marker | closed: a sphere of the reference's radius. Overlap: #6902 |
| MUS-SRC-006 day / night unit | xEdit `wbAlocTime`, vanilla data | closed: `value / 256` of a day |
| MUS-SRC-007 boundaries, layer triggers, mixing | vanilla data, Nexus wiki (2014) | boundaries are percentages of the radius; layers mix; the marker's own triggers: #6902 |
| MUS-SEL-003 positive `ANAM` | — | #6902 |
| MUS-SEL-004, MUS-CMB-002 (Fallout 3 / New Vegas) | executable defaults, `FalloutNV.esm` GMSTs | closed (§7) |
| MUS-SEL-004, MUS-CMB-002 (Oblivion) | `Oblivion.exe` (the music manager's transition and update, the combat-target count) | closed (#6903): 0.85 s out then in, a cell change at once; no recovery (§7) |
| MUS-SRC-002 precedence, Fallout 3 `RDMD`, New Vegas silence (§3.4) | `Fallout3.exe` (`TESObjectCELL::GetMusicType`, the region and cell loaders, the music routine) | closed (#6900): cell → exterior's worldspace → Default Music, no region; `RDMD` / `XCMT` dropped; an absent folder is silence (§3) |
| MUS-SCR-005, MUS-CMB-004 (`PlayMusic`) | vanilla scripts (`MQ05Script`, `MQ11Script`, `VVault11ExterminationChairSCRIPT`, `VCG00`) | closed (#6899): replaces; does not give way to battle; ends on a cell change (engine rule) |
| MUS-STG-003, MUS-STG-004 | `Fallout3.exe` (`GetDefaultObject` callers, the death handler, `LevelUpMenu`), `Oblivion.exe` (the sting calls, the update) | closed (#6901): Fallout 3 / New Vegas stings are sounds, the level-up one over the music; the music objects are unread; other stings stop the music until the place, combat or a script changes it (§5) |
| MUS-CMB-003 (New Vegas) intro / outro | xEdit labels, Nexus wiki (2014) | closed: Intro on entering combat, Outro on leaving it |
| MUS-CMB-001 (New Vegas) controller vs region battle set; `NAM1` looping mode and forced relation | — | #6902 |
| Q3: how the relation picks a list | Nexus wiki *Fallout New Vegas Music* (2014): by the conditional faction's relation to the player | closed (#6904); `NAM1`'s forced list undecoded |
| MUS-SRC-008, MUS-SCR-003 (Oblivion) | `Oblivion.exe` (`GetMusicType`, the cell-change hook, `StreamMusic`) | closed (#6903): cell → region → worldspace, Default falling through; a stream outlives a cell change into its own category (§3, §4) |
| MUS-SCR-004 (OBSE opcodes) | xOBSE `CommandTable.cpp` order, checked against five known OBSE opcodes | closed: `0x1617`, `0x19e5` |
