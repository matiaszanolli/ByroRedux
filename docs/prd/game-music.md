# Game Music — Product Requirements

**Status**: SHIPPED (2026-11-14). Revision 8 records every phase closed
(#6861–#6864, parent #6860) with the §8 metrics in ROADMAP; the PRD is kept
as history. Revision 7 moves scenario 6 to Sloan's
audio marker (#6904). Revision 6 closes Phase 0: the design is
[music.md](../engine/music.md) and the verify items left open are #6899–#6903.
Revision 5 moves the census to ROADMAP
(#6865). Revision 4 files the tracking issues, one small issue per work
item under each phase. Revision 3 records the owner's approval with every
§10 decision taken as recommended; revision 2 folded in review round 1 on
the first draft, which was written from a census of the three masters and
the installed `Data\Music` trees (§3.4) and an inventory of the engine at
HEAD (§3.5). Statements about how an original engine, format or tool
behaves cite external sources; a claim that could not be verified is
marked **Phase 0: verify** instead of being stated as fact.

**Tracking**: #6860 (parent). Phases: #6861 (0), #6862 (1), #6863 (2), #6864 (3); work items #6865–#6890.

**Scope**: the background music of Oblivion, Fallout 3 and Fallout: New
Vegas plays where and when the games' own data asks for it: location music
chosen from the cell, worldspace, region or (New Vegas) audio marker the
player is in; battle music while the player is in combat; the death and
level-up stings; the script requests `PlayMusic` and `StreamMusic`; and the
hold a movie places on music. Every music decision is reported. Outside it:
Skyrim and Fallout 4 music, which is xWMA and waits for a separate codec
PRD together with their `MUSC` / `MUST` priority stack (MUS-NG-001); radio
stations (MUS-NG-002); main-menu music (MUS-NG-003); region incidental
sounds (MUS-NG-004); new audio codecs (MUS-NG-006).

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| The music controller: layers, directives, track selection, fades, `music.status` | [Music](../engine/music.md) (Phase 0, #6869), which also covers the `audio` crate's music half |
| Loose files under `Data\` and their precedence over archives | [Archives § Asset provider precedence and the loose tier](../engine/archives.md#asset-provider-precedence-and-the-loose-tier) |
| Region resolution (`REGN` sound data, override and priority) | [Exterior readiness § EX-16](../engine/exterior-readiness-plan.md#ex-16-2372--regn-navm-ambient-audio-ai-integration) |
| A movie's pause-music flag (its mute flag silences world sound, not music, and stays with the Bink PRD) | [Bink video playback — PRD § 5.3](bink-video-playback.md#53-presentation-bik-pres) (BIK-PRES-005) |
| Detection as a possible battle-music trigger (Q2) | [Sneak and Detection — PRD](sneak-and-detection.md) |
| Extender script functions (OBSE `SetCellMusicType`) | [Legacy Mod Compatibility — PRD § 5.4](legacy-mod-compatibility.md#54-scripts-lmc-scr) (LMC-SCR-004) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `MUS-` prefix.

---

## 1. Product thesis

The owner's framing (2026-11-13): with the in-process Bink Audio decoder
landed, "a good moment to integrate music in the games as well". The codec
premise turned out not to apply: the three games' music is loose MP3 that
the engine already decodes (§3.4), and Bink Audio only carries movie
soundtracks. What is missing is the part that decides *what* plays. At HEAD
the engine plays a region's music only when the region names a single
sound file inside a `--sounds-bsa` archive. None of the three games ships
music in an archive, and none authors region music that way, so the games
are silent.

Music in these games is authored data, not engine code. Oblivion tags
cells, worldspaces and regions with a music category and plays a random
track from the matching folder ([UESP: Oblivion Music](https://en.uesp.net/wiki/Oblivion:Music);
[CS: Regions](https://cs.uesp.net/wiki/Regions)). Fallout 3 tags them with a
music type record that names a file or a folder, and keeps defaults for
battle, death and level-up in its default objects
([GECK: Default Form](https://geck.uesp.net/wiki/Default_Form)). New Vegas
layers music by audio-marker areas, day and night, and faction relation
through media sets and media location controllers (xEdit
`wbDefinitionsFNV.pas:8094-8173`).

It is for players, who hear the games scored as they were made, and for mod
authors whose cells, regions and scripts carry music.

The corollary that shapes every requirement: **music is translated from the
data and played, or it is reported, never silently dropped.** Each game's
sources are translated into one canonical directive; one game-neutral
controller plays it. The engine never invents music the data does not ask
for, and when the data asks for something this build cannot play, a status
line says why.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Music type** | A `MUSC` record (`MuscRecord`): Fallout 3 / New Vegas name a file or a folder under `Data\Music` (`FNAM`); New Vegas adds a dB value whose sign marks looping (`ANAM`, xEdit `wbDefinitionsFNV.pas:6306-6310`) |
| **Music category** | Oblivion's music enum on `CELL.XCMT`, `WRLD.SNAM` and `REGN.RDMD`: Default, Public, Dungeon (xEdit `wbDefinitionsCommon.pas:7976-7983`) |
| **Folder form** | A music type or media-set path ending in a separator: it names every file in that folder, not one file |
| **Media set** | A New Vegas `MSET` record (`MediaSetRecord`): a typed bank (Battle, Location, Dungeon, Incidental) of up to six track paths with per-slot dB, boundaries, enable flags and timings (xEdit `wbDefinitionsFNV.pas:8126-8173`) |
| **Media location controller** | A New Vegas `ALOC` record: lists of media sets per role (Neutral, Ally, Friend, Enemy, Location, Battle), a conditional faction, day and night start times, location and retrigger delays (xEdit `wbDefinitionsFNV.pas:8094-8124`) |
| **Audio marker** | A New Vegas `REFR` carrying an audio-data block (`MMRK`) that points at a media location controller (`CNAM`) and sets layer trigger percentages (xEdit `wbDefinitionsFNV.pas:7616-7623`) |
| **Default music objects** | The five Fallout 3 / New Vegas default-object slots Default Music, Battle Music, Death Music, Success Music and Level Up Music (xEdit `wbDefinitionsFO3.pas:5672-5676`) |
| **Sting** | A single non-looping track played for an event (death, level-up) while the Battle and Location layers wait (§3.1) |
| **Directive** | The canonical result of translation: a playlist (files and a pick rule), a loop flag, a volume, fade times, and the record that produced it |
| **Layer** | One of the controller's five slots (§3.1), each holding at most one directive |
| **Outcome** | What `music.status` reports for a layer (§3.2) |

---

## 3. Core model

### 3.1 Layers

The controller holds one directive per layer, ordered top to bottom as in
the table. Each active layer acts on the layers below it as its row says;
what is audible is the highest active layer plus any layer it mixes with.

| Layer | Fed by | What it does to the layers below | Ends when |
|---|---|---|---|
| **Hold** | A movie whose pause-music flag is set | Pauses all of them: "the game's music will continue to play while the movie is running, otherwise it will be paused" ([GECK: PlayBink](https://geck.uesp.net/wiki/PlayBink), Notes) | The movie ends; every layer below resumes where it was |
| **Sting** | Player death; player level-up (MUS-STG) | Pauses Battle and Location (MUS-STG-004) | The track ends |
| **Script** | `PlayMusic` (Fallout 3 / New Vegas), `StreamMusic` (Oblivion) | Mixes over Battle and Location, as at HEAD (§3.5 row 1), until Phase 0 settles MUS-SCR-005; a directive that gives way to Battle waits while Battle plays (MUS-CMB-004) | The track ends or a load (MUS-SCR, MUS-SAV); a cell change per MUS-SCR-003 and MUS-SCR-005 |
| **Battle** | The player in combat (Q2) | Replaces Location, crossfading (MUS-SEL-004) | Combat ends and the recovery time passes (MUS-CMB) |
| **Location** | The player's cell, worldspace, region or audio marker (MUS-SRC) | — | Never; it changes as the player moves |

### 3.2 Outcomes

| Outcome | Meaning | Reported by |
|---|---|---|
| **Playing** | A track of the layer's directive is playing | `music.status` (MUS-RPT-001) |
| **Silent** | The data asks for silence: a music type with no file (`1NoMusic`), or no source at all | `music.status` |
| **Unresolved** | The data names a source this build does not translate (an Incidental media set, a Skyrim music type) | `music.status` and one log line per source |
| **Missing** | The directive's file or folder is not in the install | `music.status` and one log line per path |

### 3.3 Translation per game

Each game's sources become a directive in the translate step; the
controller past it has no per-game branch (R5).

| Game | Location sources | Battle source | Sting sources |
|---|---|---|---|
| Oblivion | `CELL.XCMT`, `WRLD.SNAM`, `REGN.RDMD` category → a `Data\Music` folder (MUS-SRC-008, MUS-SRC-009) | `Data\Music\Battle` ([UESP: Oblivion Music](https://en.uesp.net/wiki/Oblivion:Music)) | `Special\death.mp3`, `Special\success.mp3` (same page) |
| Fallout 3 | `CELL.XCMO`, `WRLD.ZNAM`, `REGN.RDMO`, then the Default Music object → a music type (MUS-SRC-001 to MUS-SRC-004) | Battle Music object | Death Music and Level Up Music objects |
| Fallout: New Vegas | Audio marker → media location controller → media set (MUS-SRC-005 to MUS-SRC-007); music types as Fallout 3 | The controller's Battle sets, the region's `RDSB` battle sets (MUS-CMB-001) | As Fallout 3 (MUS-STG) |

### 3.4 Census

The census of every music source in the three games, base masters and
official DLC together, lives in ROADMAP's
[compatibility matrix](../../ROADMAP.md#compatibility-matrix) (music
column), measured by the `music_census` example (#6865). In short: none of
the three ships music in an archive, all of it is loose under
`Data\Music`; Oblivion authors music categories (cells, worldspaces,
regions); Fallout 3 and New Vegas author music types (`MUSC`), with the
default music objects in `DOBJ` slots 9–13; New Vegas adds media sets,
media location controllers and audio markers. Game files are never checked
in.

Two consequences shape the phases. First, New Vegas does not ship the five
`Default*` folders its music types and cells name, nor the death and success
stings its default objects name: on that install those directives are
**Missing**, and its location and battle music comes from media sets alone.
**Phase 0: verify** that the original is silent there too; settled by the
GECK wiki's Music Type page (geckwiki.com, unreachable from this
environment) or by listening in a `DefaultDungeon` cell such as `Vault19a`.
Second, Fallout 3's default-object and worldspace sources and all but one
of its cell sources resolve to shipped files and folders (the exception is
`TenpennyLobbyMusic`, a `.wav` the install does not ship), so it is the
first title the controller can play end to end.

### 3.5 State at HEAD

The engine's music support at HEAD on 2026-11-13 (`72010bd15`), before
Phase 1: kept as the baseline the requirements were written against. The
shipped behaviour is [music.md](../engine/music.md) and `music.status`.

| # | Piece | Today | Proof |
|---|---|---|---|
| 1 | Playback slots | Two non-spatial streaming slots on the main track, `music` and `script_music`, each a linear-tween crossfade; they play together with no ducking; whole-track looping only; music bypasses the reverb send and the underwater filter | `AudioWorld::play_music` / `play_script_music`, `crates/audio/src/world.rs:523-563`; `play_main_track`, `crates/audio/src/cache.rs:59-104` |
| 2 | Codecs | WAV, OGG, MP3, FLAC through kira's features; xWMA refused | `Cargo.toml:220-232`; `is_xwma_path`, `byroredux/src/asset_provider/audio.rs:78` |
| 3 | Region music | Plays a `REGN` music FormID only when it is a `SOUN` file found in a `--sounds-bsa` archive, or a Skyrim `MUSC`'s first track (then refused as xWMA); dispatched only when the FormID changes | `dispatch_region_ambient_music`, `byroredux/src/asset_provider/audio.rs:272`; `byroredux/src/cell_loader/load.rs:780-791` |
| 4 | Loose music | Only `PlayMusic` reads `Data\Music` from disk (a movie's sound is its own Bink audio track, decoded from the movie under `Data\Video`); the loose tier indexes `textures`, `meshes`, `materials`, `geometries` only, and the sound provider has no loose tier | `byroredux/src/systems/script_requests.rs:358-362`; `crates/video/src/lib.rs:366`, `byroredux/src/video.rs:229`; `LOOSE_ASSET_ROOTS`, `byroredux/src/asset_provider/archive.rs:50` |
| 5 | Cell / worldspace / region music fields | `XCMT` and `XCMO` parsed and discarded; `WRLD` music parsed, no reader; `RDMD` parsed, no reader | `byroredux-plugin` `cell/index.rs:345-346`; `cell/worldspace.rs:180`; `misc/world.rs:771` |
| 6 | Records | `MUSC` decoded for Fallout 3 / New Vegas and Skyrim+; `MSET` fully decoded, no reader in `byroredux`; `ALOC` and `DOBJ` are EditorID-only stubs | `misc/music.rs:52-102`; `misc/media_set.rs:47-65`; `dispatch_misc_stub.rs:37-41`, `:80-88` |
| 7 | `PlayMusic` | Fallout 3 / New Vegas: resolves the music type, plays it on the script slot over the cell's music, refuses folder form (#6438), stops on cell change; skipped entirely when headless | `music_system`, `byroredux/src/systems/script_requests.rs:329-381`; `cmd_11ff`, `crates/scripting/src/obscript_more/extras.rs:244-255` |
| 8 | `StreamMusic`, OBSE `Get/SetCellMusicType` | Not dispatched | `rg -i 'streammusic\|setcellmusictype'` finds nothing outside the reference dumps |
| 9 | Combat | `is_in_combat(world, player)` is true while any actor's `AiCombatState` targets the player | `crates/scripting/src/combat.rs:50-62` |
| 10 | Death and level-up | Death (a lethal hit or a script `Kill`) inserts `Dead` and raises the death events; the player is never spared into bleedout; level-up logs, notifies and raises the level-up story event; no music. Both are drivable from the console: `levelup xp <n>`, and `obscript.cmd 0x14 108b` (`KillActor` on the player) | `byroredux/src/combat.rs:512-522`, `:1471-1493` (death), `:838-847` (level-up); `crates/scripting/src/bleedout.rs:198-199`; `byroredux/src/commands/gameplay.rs:795-800`; `crates/scripting/src/obscript_effects/combat.rs:28-33` |
| 11 | Movies | A movie's sound plays on the script slot, replacing any `PlayMusic` track; the pause-music and mute flags are parsed and logged only | `byroredux/src/video.rs:136-146`, `:218` |
| 12 | Volume | No music volume setting, no master bus | `crates/core/src/settings/builtin.rs:18-80` registers no audio key |
| 13 | Save | No music state saved by design; region music re-dispatches on load only if its FormID differs | `NOT_SAVED_BY_DESIGN`, `byroredux/src/save_io/registry_completeness_tests.rs:451`; `load.rs:782-785` |
| 14 | Reporting | `cell.owners` prints the region music FormID and whether the music slot is active; no music command | `byroredux/src/commands/world_info.rs:897-959` |

---

## 4. Goals and non-goals

### 4.1 Goals

- **MUS-G-001 — Fallout 3 is scored.** Location, battle and sting music
  play from Fallout 3's own records in every cell the census lists.
- **MUS-G-002 — New Vegas is scored.** Audio-marker areas play their media
  sets, and combat plays its battle sets.
- **MUS-G-003 — Oblivion is scored.** Cells, worldspaces and regions play
  their category's folder, combat plays Battle, and death and level-up
  play their stings.
- **MUS-G-004 — Every decision is reported.** `music.status` gives each
  layer's outcome (§3.2) and source record in every in-scope game.
- **MUS-G-005 — Movies hold music.** A movie with the pause-music flag
  pauses music and resumes it afterwards (the music half of BIK-PRES-005).

### 4.2 Non-goals

- **MUS-NG-001.** Skyrim and Fallout 4 music. Their tracks are xWMA (every
  music entry of `Skyrim - Sounds.bsa` and all but three of
  `Fallout4 - Sounds.ba2`'s, counted 2026-11-13), and their `MUSC` priority, ducking and `MUST`
  palette stack only matter once those decode. The owner decided
  (2026-11-13) that the xWMA decoder gets its own PRD; that PRD owns the
  stack. The controller's layer model (§3.1) is designed so it can host it.
- **MUS-NG-002.** Radio stations (Fallout 3 / New Vegas). They are talking
  activators flagged as stations, playing dialogue and songs through the
  quest Radio tab, with their own volume setting
  ([GECK: TalkingActivator](https://geck.uesp.net/wiki/TalkingActivator);
  [GECK: Radio Stations](https://geck.uesp.net/wiki/Radio_Stations)); none
  of their records references a music type, media set or controller.
- **MUS-NG-003.** Main-menu music (`SMainMenuMusicTrack` in each game's
  default INI): the engine has no main menu and no PRD owns one
  ([Bink PRD §6 row 3](bink-video-playback.md#6-deliberate-deviations-and-risks)).
- **MUS-NG-004.** Incidental media sets (`MSET` type 3, region `RDSI`) and
  region ambient sound loops. They play sound clips, not music (an
  Incidental set's day and night entries are `SOUN` records, xEdit
  `wbDefinitionsFNV.pas:8126-8173`), and #3301 owns the region ambient
  emitter. They are reported **Unresolved** until that lands.
- **MUS-NG-005.** Fallout 76 and Starfield: their sound archives are Wwise
  banks (`Starfield - WwiseSounds*.ba2`), and no playable flow reaches
  Fallout 76.
- **MUS-NG-006.** New codecs. The three games' music is MP3 and WAV, which
  decode at HEAD (§3.5 row 2).

---

## 5. Requirements by area

### 5.1 Location sources (`MUS-SRC`)

- **MUS-SRC-001.** On Fallout 3 and New Vegas, a cell's `XCMO` music type
  MUST become the Location directive while the player is in that cell.
  Today it is parsed and discarded (§3.5 row 5).
- **MUS-SRC-002.** On Fallout 3 and New Vegas, a cell with no `XCMO` MUST
  take its worldspace's `ZNAM`, then its region's `RDMO`, then the Default
  Music object. **Phase 0: verify** this precedence, and whether Fallout 3
  reads the music category `RDMD` that two of its regions author (§3.4);
  settled by the GECK wiki's Music Type page or by observing a cell whose
  cell and worldspace music differ.
- **MUS-SRC-003.** The Fallout 3 / New Vegas `DOBJ` record MUST decode its
  five music slots (§2). Today `DOBJ` is an EditorID-only stub (§3.5
  row 6).
- **MUS-SRC-004.** A music type with no file (`1NoMusic`) MUST give the
  Location layer the outcome **Silent** and play nothing.
- **MUS-SRC-005.** On New Vegas, while the player is inside an audio
  marker's area, the Location directive MUST come from that marker's
  controller's Location media sets. **Phase 0: verify** the area's shape
  and radius and the rule when areas overlap; settled by the GECK wiki's
  Media Location Controller page, or by comparing xEdit across the vanilla
  markers. The only source read so far is a modder's tutorial
  ([Nexus wiki: Fallout New Vegas Music, 2014](https://web.archive.org/web/20170614070055/http://wiki.nexusmods.com/index.php/Fallout_New_Vegas_Music)).
- **MUS-SRC-006.** A New Vegas Location media set MUST play its day layers
  between the controller's day start and night start and its night layers
  otherwise, against `GameTimeRes`. **Phase 0: verify** the start times'
  unit (xEdit renders them as `value / 256` hours,
  `wbDefinitionsFNV.pas:1213-1221`).
- **MUS-SRC-007.** A New Vegas Location media set MUST enable its Outer,
  Middle and Inner layers by the player's distance into the marker's area
  against the set's boundary percentages and enable flags. **Phase 0:
  verify** how layers combine (crossfade or mix) and what the marker's
  layer trigger percentages mean; settled as MUS-SRC-005.
- **MUS-SRC-008.** On Oblivion, the Location category MUST come from the
  cell's `XCMT`, else the worldspace's `SNAM`, else the winning region's
  `RDMD` by the region override and priority rule
  ([CS: Regions](https://cs.uesp.net/wiki/Regions)). **Phase 0: verify**
  this precedence; settled by observing a Default cell inside a region with
  a non-Default `RDMD`.
- **MUS-SRC-009.** On Oblivion, Public MUST play `Data\Music\Public`,
  Dungeon `Data\Music\Dungeon`, and Default the folder or folders Q6
  decides. A Default cell does play music: "When used in a 'Default' cell,
  StreamMusic only restarts the current track"
  ([CS: StreamMusic](https://cs.uesp.net/wiki/StreamMusic), Notes);
  [UESP: Oblivion Music](https://en.uesp.net/wiki/Oblivion:Music) lists
  Explore as the countryside music.
- **MUS-SRC-010.** A folder-form source MUST expand to the playable files
  directly in that folder. Today `PlayMusic` refuses folder form (#6438)
  and region music refuses a folder-form `SOUN` (§3.5 rows 3 and 7).
- **MUS-SRC-011.** The loose tier MUST index a `music` root beside its
  four asset roots. Today it does not (§3.5 row 4).
- **MUS-SRC-013.** A music path MUST resolve loose first, then in the sound
  archives, by the engine-wide precedence
  ([Archives](../engine/archives.md#asset-provider-precedence-and-the-loose-tier)).
  Today the region path reads archives only, where none of the three games
  ships music (§3.4, §3.5 row 3).
- **MUS-SRC-012.** A source whose file or folder is absent MUST give the
  outcome **Missing** naming the path. On New Vegas this covers the
  `Default*` music types and the death and success stings (§3.4).

### 5.2 Playback and track selection (`MUS-SEL`)

- **MUS-SEL-001.** A playlist of several files MUST pick its next track by
  the rule Q1 decides. The sources say only "randomly played one after
  another" ([UESP: Oblivion Music](https://en.uesp.net/wiki/Oblivion:Music)).
- **MUS-SEL-002.** When a non-looping track ends, the next track of the
  same directive MUST start: Oblivion chooses "a new random music …
  according to the current location"
  ([CS: StreamMusic](https://cs.uesp.net/wiki/StreamMusic), Notes).
- **MUS-SEL-003.** A New Vegas music type MUST loop when its `ANAM` value
  is positive (xEdit's "dB (positive = Loop)"), at the magnitude's volume.
  Today `MuscRecord::looping` reads the sign; the volume is applied only
  when negative (`byroredux/src/systems/script_requests.rs:376`).
  **Phase 0: verify** the magnitude's meaning for a positive value.
- **MUS-SEL-004.** A change of Location directive MUST crossfade over the
  directive's fade time: a New Vegas media set's crossfade fields (xEdit
  `ENAM` / `FNAM`), otherwise the interim `REGN_AMBIENT_CROSSFADE_SECS` at
  HEAD. **Phase 0: verify** Oblivion's and Fallout 3's fade and whether
  `fASFadeInTime` / `fRegionLoopFadeInTime` in the default INI govern
  music.
- **MUS-SEL-005.** Moving between two places with the same Location
  directive MUST NOT restart the track. Today region music meets this by
  FormID (`load.rs:782-785`).

### 5.3 Combat (`MUS-CMB`)

- **MUS-CMB-001.** While the player is in combat (Q2), the Battle layer MUST
  play: Oblivion's `Battle` folder, the Fallout 3 / New Vegas Battle Music
  object, and on New Vegas the active controller's Battle sets, else the
  region's `RDSB` battle set. **Phase 0: verify** which wins between a
  controller's and a region's battle set.
- **MUS-CMB-002.** After combat ends, the Battle layer MUST hold for a
  recovery time before the Location layer resumes: a New Vegas Battle
  set's recovery time (xEdit `FNAM`). **Phase 0: verify** Oblivion's and
  Fallout 3's delay; the default INIs carry no combat-music key.
- **MUS-CMB-003.** A New Vegas Battle set's Intro MUST play on entering
  combat and its Outro on leaving it (xEdit `HNAM` / `INAM`). **Phase 0:
  verify** against the GECK wiki's Media Set page.
- **MUS-CMB-004.** A Script directive MUST carry whether it gives way to
  the Battle layer, set in the translate step, and the controller MUST
  hold a giving-way Script directive while Battle plays. Oblivion's
  translation sets it for `StreamMusic`, which "will also not override
  combat music" ([CS: StreamMusic](https://cs.uesp.net/wiki/StreamMusic),
  Notes). **Phase 0: verify** Fallout 3 and New Vegas `PlayMusic`; until
  then their translation leaves it unset.

### 5.4 Stings (`MUS-STG`)

- **MUS-STG-001.** The player's death MUST play the death sting: Oblivion's
  `Special\death.mp3`, the Fallout 3 / New Vegas Death Music object. Today
  death plays no music (§3.5 row 10).
- **MUS-STG-002.** A player level-up MUST play the level-up sting:
  Oblivion's "Success" track
  ([UESP: Oblivion Music](https://en.uesp.net/wiki/Oblivion:Music)), the
  Fallout 3 / New Vegas Level Up Music object.
- **MUS-STG-003.** The Fallout 3 / New Vegas Success Music object MUST NOT
  be bound to an event until Phase 0 verifies which event plays it; settled
  by the GECK wiki's Default Form page in full or by observation.
- **MUS-STG-004.** A sting MUST suspend the Battle and Location layers for
  its length. **Phase 0: verify** whether they resume or restart.

### 5.5 Script requests (`MUS-SCR`)

- **MUS-SCR-001.** `PlayMusic` MUST play folder-form music types
  (MUS-SRC-010). Today it refuses them (§3.5 row 7).
- **MUS-SCR-002.** A movie's sound MUST NOT replace or stop the Script
  layer. Today it plays on the script slot (§3.5 row 11).
- **MUS-SCR-003.** Oblivion `StreamMusic` MUST play a quoted file relative
  to the game folder, or a random track for the keywords `Public`,
  `Explore`, `Dungeon` and `Random`, on the Script layer, and MUST hand
  back to the Location layer when the track ends
  ([CS: StreamMusic](https://cs.uesp.net/wiki/StreamMusic)). Today it is
  not dispatched (§3.5 row 8). **Phase 0: verify** what a cell change does
  to a streamed track.
- **MUS-SCR-004.** OBSE `GetCellMusicType` and `SetCellMusicType` MAY get
  catalog entries under LMC-SCR-004, reading and writing the cell's
  category ([CS: SetCellMusicType](https://cs.uesp.net/wiki/SetCellMusicType)).
- **MUS-SCR-005.** The Script layer MUST mix over the Battle and Location
  layers, as `PlayMusic` does at HEAD (§3.5 row 7), until Phase 0 settles
  the original's rule. The only source is one ambiguous note, quoted
  verbatim: "-Will non stop the currently playing music of the cell."
  [sic] ([GECK: PlayMusic](https://geck.uesp.net/wiki/PlayMusic), Notes).
  **Phase 0: verify** whether `PlayMusic` mixes over or replaces the cell's
  music and whether a cell change ends it; settled by the GECK wiki's
  current PlayMusic page or by observing a vanilla quest that calls it
  (Phase 0 names one from the census).

### 5.6 Mix (`MUS-MIX`)

- **MUS-MIX-001.** Music MUST play at the volume Q4 decides, on a music
  level separate from every other sound. Today no such setting exists
  (§3.5 row 12).
- **MUS-MIX-002.** A movie with the pause-music flag set MUST raise the
  Hold layer: every music layer paused for the movie's length and resumed
  at the same position afterwards. Today the flag is logged only (§3.5
  row 11).
- **MUS-MIX-003.** Music MUST keep bypassing the reverb send and the
  underwater filter (§3.5 row 1).

### 5.7 Reporting (`MUS-RPT`)

- **MUS-RPT-001.** `music.status` MUST print, per layer, its outcome
  (§3.2), the source record's EditorID and FormID, the file playing and
  its position.
- **MUS-RPT-002.** Each **Unresolved** source and each **Missing** path
  MUST log one line per distinct source or path per session, never per
  frame.
- **MUS-RPT-003.** A headless run (`--bench-hold`, no audio device) MUST
  still resolve directives and report them in `music.status`, so the gates
  can drive music without sound. Today `music_system` returns early when
  headless (`byroredux/src/systems/script_requests.rs:337`).
- **MUS-RPT-004.** The stings MUST hang off the same death and level-up
  paths the console reaches, so a headless gate drives them with
  `obscript.cmd 0x14 108b` (`KillActor` on the player) and `levelup xp <n>`
  (§3.5 row 10); a test pins that each driver raises its sting.

### 5.8 Saves (`MUS-SAV`)

- **MUS-SAV-001.** After a load, the Location layer MUST re-resolve from
  the loaded place before the first rendered frame of it.
- **MUS-SAV-002.** What a save keeps of the Script layer MUST follow Q5.
  Today nothing is saved and a load ends the scripted track (§3.5 row 13).

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Tracks "randomly played one after another" (UESP); repeat avoidance unknown | The pick rule of Q1 | Shuffle-fix playlists in music mods | Small: an order unlike the original's (MUS-SEL-001) |
| 2 | Battle music when "an enemy sees the player character" on Oblivion ([UESP: Oblivion Music](https://en.uesp.net/wiki/Oblivion:Music)) | Battle while the player is in combat (Q2) | — | Battle music starts later than the original's when an enemy detects without attacking (MUS-CMB-001); revisited when the Sneak and Detection PRD lands |
| 3 | Music read from loose `Data\Music` | Loose first, then archives (MUS-SRC-011, MUS-SRC-013) | Mods' "extract music from the BSA" instructions | None: the archives hold no music in vanilla (§3.4) |
| 4 | Unknown what a save keeps of playing music | Q5's choice | — | A quest's scripted track does not resume after a load (MUS-SAV-002) |
| 5 | Oblivion `StreamMusic` with a file that does not exist "causes the game to restart the song that is currently playing", and in a Default cell it "only restarts the current track" ([CS: StreamMusic](https://cs.uesp.net/wiki/StreamMusic), Notes) | A missing file is reported **Missing** and the current track keeps playing (MUS-SRC-012); a named file plays in any cell | The documented workaround of calling `StreamMusic random` first in a Default cell; it still works, as two requests | A mod that relied on a bad path to restart its track no longer restarts it (MUS-SCR-003) |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow the engine tiers in
[game-compatibility.md](../engine/game-compatibility.md): Fallout 3 and New
Vegas are Tier 1, Oblivion Tier 3.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 2. Category music, `StreamMusic`, stings (MUS-SRC-008, MUS-SRC-009, MUS-SCR-003) |
| Fallout 3 | yes | Wave 1. All but one of its location and default-object sources resolve to shipped files and folders (§3.4), so the controller is proven here first |
| Fallout: New Vegas | yes | Wave 1 for the controller, its cell music types and `PlayMusic`; wave 2 for media sets (MUS-SRC-005 to MUS-SRC-007), which is where its music actually lives |
| Skyrim (LE / SE / AE) | no | xWMA and the `MUSC` / `MUST` stack: the separate xWMA PRD (MUS-NG-001) |
| Fallout 4 | no | As Skyrim (MUS-NG-001) |
| Fallout 76 | n/a | Wwise banks; no playable flow (MUS-NG-005) |
| Starfield | no | Wwise banks (MUS-NG-005) |

### 7.2 Acceptance

Game data comes from local installs only (`BYROREDUX_FNV_DATA` and the
other per-game variables the smoke scripts read); nothing is checked in.

A scenario passes when:

1. **Census (per game, headless).** An ignored `cargo test`, run with the
   game's data variable, translates every music source in the base master
   and its DLC masters and reports each as Playing-capable (its files exist),
   **Silent**, **Unresolved** or **Missing**; the counts match the Phase 0
   census (MUS-SRC, MUS-RPT-002).
2. **Fallout 3 cell music (headless).** Loading `MegatonCratersideSupply`
   (`DefaultPublic`), `RedRacer03` (`DefaultDungeon`) and
   `MegatonWaterProcessingPlant` (`1NoMusic`) gives `music.status` Location
   **Playing** from `public\`, **Playing** from `dungeon\`, and **Silent**
   (MUS-SRC-001, MUS-SRC-004, MUS-SRC-010, MUS-RPT-003).
3. **Fallout 3 exterior (headless).** At `--grid 0,0` in `Wasteland`, whose
   cell has no `XCMO` and whose worldspace has no `ZNAM` (census
   2026-11-13), `music.status` reports the Default Music object
   `DefaultExplore` **Playing** from `explore\` (MUS-SRC-002,
   MUS-SRC-003).
4. **Combat and stings (Fallout 3, headless).** Placing the player in melee
   range of a hostile actor (`combat.approach`) switches `music.status` to
   Battle once the actor engages; `obscript.cmd <actor> 108b` kills it and
   `music.status` returns to Location after the recovery time;
   `levelup xp <n>` past the next level and then `obscript.cmd 0x14 108b`
   report the level-up and death stings (MUS-CMB-001, MUS-CMB-002,
   MUS-STG-001, MUS-STG-002, MUS-RPT-004).
5. **Movie hold (Fallout 3, headless).** In `MegatonCratersideSupply`, with
   Location **Playing** from `public\`,
   `obscript.cmd none 1114 t:B01.bik 1 1 1 1` (a shipped Bink 1 movie,
   540 frames at 29.97 fps) reports the Hold layer, and after `video.skip`
   the Location track resumes at the position it had. In
   `fnv-new-game.sh`, the intro's own `PlayBink "FNVIntro.bik" 1 1 0 1`
   raises no Hold layer (MUS-MIX-002, MUS-SCR-002).
6. **New Vegas media sets (headless).** Standing at audio marker
   `0x0015D028` (controller `musCtrlSloan`, `WastelandNV` grid (-8, 1)) by
   day and by night reports its controller's Location set with the day and
   night layers in turn (MUS-SRC-005, MUS-SRC-006). The Goodsprings marker
   `0x00152433` first named here sits under `musCtrlGoodsprings`, which
   carries faction-relation sets and no Location set, so that area reports
   **Unresolved** until #6904.
7. **Oblivion categories (headless).** A Public cell (`BrumaJGhastasHouse`)
   and a Dungeon cell (`SageGlenHollow`) report `public\` and `dungeon\`;
   `StreamMusic Random` reports the Script layer (MUS-SRC-008,
   MUS-SCR-003).

---

## 8. Metrics

The numbers live in ROADMAP's per-game compatibility matrix, in a music
column added by Phase 1.

| Metric | Definition | Granularity |
|---|---|---|
| Source coverage | Music sources translated to a directive ÷ music sources in the masters (cells, worldspaces, regions, audio markers, default objects) | per game, base and DLC apart |
| Playable coverage | Directives whose files exist in the install ÷ translated directives | per game |
| Unresolved count | Distinct sources reported **Unresolved** | per game, by record type |

---

## 9. Delivery sequence

**Phase 0 — Census, verify list, design (no engine code).** Check in the
census script behind §3.4 and extend it to the DLC masters; close
each **Phase 0: verify** item (MUS-SRC-002, MUS-SRC-005 to MUS-SRC-008,
MUS-SEL-003, MUS-SEL-004, MUS-CMB-001 to MUS-CMB-004, MUS-STG-003,
MUS-STG-004, MUS-SCR-003, MUS-SCR-005, §3.4's New Vegas silence) or file it as an issue;
the GECK wiki's Music Type, Media Set and Media Location Controller pages
settle most of them and need a browser past geckwiki.com's bot check; write
`docs/engine/music.md` and add it to Related; decide Q1–Q6. Exit: the
census is in ROADMAP for the three games, the design is merged, and every
verify item is closed or filed.

**Phase 1 — The controller, on Fallout 3.** The layer model, the loose
`music` root, music types and default objects, combat, stings, the movie
hold, `PlayMusic` folder form, reporting and saves: MUS-SRC-001 to
MUS-SRC-004, MUS-SRC-010 to MUS-SRC-013, MUS-SEL-001 to MUS-SEL-005,
MUS-CMB-001, MUS-CMB-002, MUS-CMB-004's controller half, MUS-STG-001 to
MUS-STG-004, MUS-SCR-001, MUS-SCR-002, MUS-SCR-005, MUS-MIX-001 to
MUS-MIX-003, MUS-RPT-001 to MUS-RPT-004, MUS-SAV-001, MUS-SAV-002.
Depends on M44's streaming playback and on the movie request's pause-music
flag, which reaches `VideoRequest` at HEAD (§3.5 row 11); the Bink PRD's
open Phase 2 issue (#6858) does not block it. Exit: scenario 1 passes on
Fallout 3 and New Vegas and scenarios 2 to 5 on Fallout 3, and Fallout 3's
source and playable coverage are published.

**Phase 2 — New Vegas media sets.** Audio markers, controllers, Location and
Battle sets, day and night, layers, intro and outro: MUS-SRC-005 to
MUS-SRC-007, MUS-CMB-001's New Vegas half, MUS-CMB-003. Exit: scenario 6
passes and New Vegas's source coverage is published.

**Phase 3 — Oblivion.** Categories and `StreamMusic`: MUS-SRC-008,
MUS-SRC-009, MUS-CMB-004's Oblivion half, MUS-SCR-003, MUS-SCR-004. Exit: scenario 7 passes
and Oblivion's source coverage is published.

---

## 10. Decisions required

1. **Q1 — How a playlist picks its next track.** Recommended: shuffle, so
   no track repeats until every track of the folder has played and the
   first of a new round differs from the last of the old; a folder of one
   track loops it. Consequence: no back-to-back repeats, at the cost of an
   order the original may not produce (§6 row 1). Alternative: uniform
   random each time, the literal reading of "randomly played"; back-to-back
   repeats happen, often in the five-track folders.
   **Decided** (2026-11-13): as recommended: shuffle; no track repeats until the folder has played through, and a round never starts on the track the last one ended on (MUS-SEL-001).
2. **Q2 — What "in combat" means for battle music.** Recommended:
   `is_in_combat` for the player, which exists at HEAD (§3.5 row 9), plus
   the recovery hold of MUS-CMB-002. Consequence: battle music starts when
   an actor engages, not when one first detects the player (§6 row 2).
   Alternative: start on detection; this waits for the
   [Sneak and Detection PRD](sneak-and-detection.md), which is PROPOSED, and
   blocks Phase 1's combat half on it.
   **Decided** (2026-11-13): as recommended: `is_in_combat` for the player plus the recovery hold; detection is revisited when the Sneak and Detection PRD lands (MUS-CMB-001, MUS-CMB-002).
3. **Q3 — New Vegas faction-relation sets.** Recommended: Phase 2 plays the
   Location and Battle sets and treats the Neutral, Ally, Friend and Enemy
   lists as a follow-up once Phase 0 verifies how the relation is computed;
   the census finds those lists on 8 to 10 of 89 controllers. Consequence:
   a handful of faction-held areas play their Location set regardless of
   standing. Alternative: build the relation sets in Phase 2 from faction
   reaction to the player; risks encoding an unverified rule.
   **Decided** (2026-11-13): as recommended: Phase 2 plays the Location and Battle sets; the faction-relation lists are a follow-up once Phase 0 verifies the relation rule (MUS-SRC-005, MUS-CMB-001).
4. **Q4 — Music volume.** Recommended: a canonical `audio.music_volume`
   setting whose default is the game's `fDefaultMusicVolume` from its
   shipped default INI (0.3 on Oblivion and Fallout 3, 0.6 on New Vegas,
   read in the translate step), with Oblivion's `bMusicEnabled` mapping to
   zero. Consequence: the first launch sounds like the original's default
   mix. Alternative: play at authored level (1.0) with no setting until a
   settings UI exists; music then sits louder against effects than in the
   original.
   **Decided** (2026-11-13): as recommended: `audio.music_volume`, defaulting to the game's `fDefaultMusicVolume`, with Oblivion's `bMusicEnabled` off mapping to zero (MUS-MIX-001).
5. **Q5 — What a save keeps.** Recommended: nothing, as at HEAD; the
   Location and Battle layers re-resolve on load and a scripted track ends.
   Consequence: a scripted quest track is lost on load (§6 row 4).
   Alternative: save the Script layer's music type and position and resume
   it; one more saved resource and a save-format change under M45.
   **Decided** (2026-11-13): as recommended: a save keeps no music state; Location and Battle re-resolve on load and a scripted track ends (MUS-SAV-001, MUS-SAV-002).
6. **Q6 — What Oblivion's Default category plays.** A Default cell has a
   current track (CS StreamMusic's notes, MUS-SRC-009), so the question is
   which folder it comes from. Recommended: `Explore`, the reading UESP's
   countryside description supports and the one Fallout 3 and New Vegas
   encode explicitly (their Default Music object is `DefaultExplore`,
   §3.4). Consequence: Default exteriors play only the countryside tracks;
   if Phase 0 finds otherwise, Default cells change music in Phase 3.
   Alternative: Default draws from Public, Explore and Dungeon the way
   `StreamMusic Random` does ("a random track out of one of the three/four
   folders", [CS: StreamMusic](https://cs.uesp.net/wiki/StreamMusic));
   town and dungeon tracks then play in the countryside.
   **Decided** (2026-11-13): as recommended: Default plays `Data\Music\Explore`; Phase 0 may still revise it on evidence (MUS-SRC-009).

---

## 11. Document map

| Need | Go to |
|---|---|
| How the controller works | [music.md](../engine/music.md) |
| Loose files and archive precedence | [archives.md](../engine/archives.md#asset-provider-precedence-and-the-loose-tier) |
| Region resolution and the ambient-audio epic | [exterior-readiness-plan.md § EX-16](../engine/exterior-readiness-plan.md#ex-16-2372--regn-navm-ambient-audio-ai-integration) |
| Movie flags and the Hold layer's trigger | [bink-video-playback.md](bink-video-playback.md) |
| Detection, for Q2's alternative | [sneak-and-detection.md](sneak-and-detection.md) |
| Extender function catalog | [legacy-mod-compatibility.md § 5.4](legacy-mod-compatibility.md#54-scripts-lmc-scr) |
| Game tiers and waves | [game-compatibility.md](../engine/game-compatibility.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-13 | First draft: census of the three masters and `Data\Music`, HEAD inventory, layer model, requirements, six decisions | — |
| 2 | 2026-11-13 | Review round 1 folded in: HEAD corrections (level-up/death drivers, movie audio, §3.5 row 10 cites), PlayMusic quote and layer-combination model, MUS-CMB-004 moved to translation, Q6 sourced from CS StreamMusic, scenarios 3–5 data and Phase 1 exit, mute flag out of scope, StreamMusic deviations, MUS-SRC-011 split, census home; Fallout 3 `RDMD` added to MUS-SRC-002's verify | claude: revise |
| 3 | 2026-11-13 | Owner approval: Q1–Q6 decided as recommended (MUS-SEL-001, MUS-CMB-001, MUS-CMB-002, MUS-SRC-005, MUS-MIX-001, MUS-SAV-001, MUS-SAV-002, MUS-SRC-009) | owner: approved |
| 4 | 2026-11-13 | APPROVED → ACTIVE: tracking issues filed (#6860 parent; phases #6861–#6864; 26 work items #6865–#6890, each citing the requirement IDs it satisfies) | — |
| 5 | 2026-11-13 | §3.4's census table replaced by a link to ROADMAP's music column, now measured over base masters and DLC by the checked-in `music_census` example; scenario 6's Goodsprings marker named; Fallout 3's `TenpennyLobbyMusic` exception noted (#6865) | — |
| 6 | 2026-11-13 | Phase 0 closed: `docs/engine/music.md` written and linked from Related; verify items settled from the executables' defaults and New Vegas's data, or filed as #6899–#6903 | — |
| 7 | 2026-11-14 | Scenario 6 moved from the Goodsprings marker `0x00152433` to Sloan's `0x0015D028`: `musCtrlGoodsprings` carries faction-relation sets and no Location set, which Q3 defers (#6904) | — |
| 8 | 2026-11-14 | ACTIVE → SHIPPED: Phases 0–3 closed (#6861–#6864, #6860); source and playable coverage published in ROADMAP; follow-ups #6899–#6904 stay open | — |
