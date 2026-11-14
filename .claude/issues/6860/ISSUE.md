=== #6860 Game music — tracking (PRD game-music)
Tracking issue for the APPROVED PRD [Game Music](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md) (approved by the owner 2026-11-13; decisions Q1–Q6 taken as recommended).

The background music of Oblivion, Fallout 3 and Fallout: New Vegas plays where and when the games' own data asks for it — location music from the cell, worldspace, region or (New Vegas) audio marker; battle music in combat; the death and level-up stings; `PlayMusic` / `StreamMusic`; the hold a movie places on music — and every music decision is reported by `music.status`. Today the three games are silent: the engine plays region music only from a `--sounds-bsa` archive, and none of them ships music in an archive (PRD §3.4, §3.5). Skyrim and Fallout 4 wait on a separate xWMA PRD (MUS-NG-001).

Each phase is a tracker; the work is in its small leaf issues, each carrying the requirement IDs it satisfies.

## Phases
- [ ] Phase 0: #6861 — census, verify items, design (no engine code)
- [ ] Phase 1: #6862 — the controller, on Fallout 3
- [ ] Phase 2: #6863 — New Vegas media sets
- [ ] Phase 3: #6864 — Oblivion

=== #6861 MUS Phase 0: music census, verify items and design (no engine code)
Part of #6860. PRD [§9 Phase 0](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). No engine code.

## Work items
- [x] #6865 — MUS-0a: check in the music census and publish it in ROADMAP
- [ ] #6866 — MUS-0b: verify New Vegas media-set and controller semantics
- [ ] #6867 — MUS-0c: verify Fallout 3 / New Vegas music-type, script and sting behaviour
- [ ] #6868 — MUS-0d: verify Oblivion music precedence, fades and StreamMusic
- [ ] #6869 — MUS-0e: write docs/engine/music.md (the music controller design)

## Acceptance Criteria
- [ ] Every work item above is closed
- [ ] Exit: the census is in ROADMAP for the three games, the design is merged, and every verify item is closed or filed as an issue.


=== #6862 MUS Phase 1: the music controller, on Fallout 3
Part of #6860. PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6861, M44's streaming playback, and the movie request's pause-music flag (carried at HEAD; #6858 does not block it).

## Work items
- [ ] #6870 — MUS-1a: decode DOBJ music slots and keep the cell, worldspace and region music fields
- [ ] #6871 — MUS-1b: resolve music files loose-first, expand folder forms, report Missing
- [ ] #6872 — MUS-1c: music controller core and the Location layer (Fallout 3 / New Vegas music types)
- [ ] #6873 — MUS-1d: track selection, looping and crossfades
- [ ] #6874 — MUS-1e: music.status and headless music resolution
- [ ] #6875 — MUS-1f: battle music layer
- [ ] #6876 — MUS-1g: death and level-up stings
- [ ] #6877 — MUS-1h: PlayMusic on the Script layer; movie sound off the music slot
- [ ] #6878 — MUS-1i: music volume setting and the movie Hold layer
- [ ] #6879 — MUS-1j: music across saves and loads
- [ ] #6880 — MUS-1k: music census test and coverage metrics (scenario 1)
- [ ] #6881 — MUS-1l: Fallout 3 music smoke gate (scenarios 2 to 5)

## Acceptance Criteria
- [ ] Every work item above is closed
- [ ] Exit: §7.2 scenario 1 passes on Fallout 3 and New Vegas and scenarios 2 to 5 on Fallout 3, and Fallout 3's source and playable coverage are published in ROADMAP.

=== #6863 MUS Phase 2: New Vegas media sets, audio markers and battle sets
Part of #6860. PRD [§9 Phase 2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6862.

## Work items
- [ ] #6882 — MUS-2a: decode ALOC and New Vegas audio-marker references
- [ ] #6883 — MUS-2b: audio-marker areas play their Location media sets, by day and night
- [ ] #6884 — MUS-2c: media-set Outer / Middle / Inner layers
- [ ] #6885 — MUS-2d: New Vegas battle sets, intro and outro
- [ ] #6886 — MUS-2e: New Vegas media-set gate (scenario 6) and coverage

## Acceptance Criteria
- [ ] Every work item above is closed
- [ ] Exit: §7.2 scenario 6 passes and New Vegas's source coverage is published in ROADMAP.

=== #6864 MUS Phase 3: Oblivion music categories and StreamMusic
Part of #6860. PRD [§9 Phase 3](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6862.

## Work items
- [ ] #6887 — MUS-3a: Oblivion music categories, battle folder and stings
- [ ] #6888 — MUS-3b: StreamMusic
- [ ] #6889 — MUS-3c: OBSE GetCellMusicType / SetCellMusicType (optional)
- [ ] #6890 — MUS-3d: Oblivion music gate (scenario 7) and coverage

## Acceptance Criteria
- [ ] Every work item above is closed
- [ ] Exit: §7.2 scenario 7 passes and Oblivion's source coverage is published in ROADMAP.

=== #6865 MUS-0a: check in the music census and publish it in ROADMAP
Part of #6861 (#6860). PRD [§9 Phase 0](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence).

The scratch record walker behind PRD §3.4 becomes a checked-in `crates/plugin` example, extended to the DLC masters. No engine code.

## Acceptance Criteria
- [ ] The example prints, per master, every music source: `CELL` `XCMT`/`XCMO`, `WRLD` `SNAM`/`ZNAM`, `REGN` `RDMD`/`RDMO`/`RDSB`/`RDSI`, `DOBJ` music slots, `MUSC`, `MSET`, `ALOC` and audio-marker `REFR`s, plus the `Data\Music` file count
- [ ] Its output for the base and DLC masters of Oblivion, Fallout 3 and New Vegas is in ROADMAP's per-game matrix (a music column); PRD §3.4 is replaced by a link to it
- [ ] §7.2 scenario 6's Goodsprings audio marker is named by FormID in the PRD

=== #6866 MUS-0b: verify New Vegas media-set and controller semantics
Part of #6861 (#6860). PRD [§9 Phase 0](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence).

Settles the New Vegas **Phase 0: verify** items, mostly from the GECK wiki's Media Set and Media Location Controller pages (geckwiki.com needs a browser past its bot check). Each item is closed with a cited source or filed as its own issue.

## Acceptance Criteria
- [ ] Verify, MUS-SRC-005: an audio marker's area shape and radius, and the rule when areas overlap
- [ ] Verify, MUS-SRC-006: the unit of a controller's day and night start times
- [ ] Verify, MUS-SRC-007: how a set's Outer / Middle / Inner layers combine, and what a marker's layer trigger percentages mean
- [ ] Verify, MUS-CMB-001 and MUS-CMB-003: which wins between a controller's and a region's battle set; when a Battle set's Intro and Outro play
- [ ] Verify, MUS-SEL-003 and Q3: what a positive `ANAM` magnitude means; how the faction relation picking Neutral / Ally / Friend / Enemy sets is computed

=== #6867 MUS-0c: verify Fallout 3 / New Vegas music-type, script and sting behaviour
Part of #6861 (#6860). PRD [§9 Phase 0](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence).

Settles the Fallout 3 / New Vegas **Phase 0: verify** items from the GECK wiki's Music Type and PlayMusic pages, or by observation in the original game. Each item is closed with a cited source or filed as its own issue.

## Acceptance Criteria
- [ ] Verify, MUS-SRC-002: the precedence `XCMO` → `ZNAM` → `RDMO` → Default Music, and whether Fallout 3 reads the `RDMD` two of its regions author
- [ ] Verify, MUS-SCR-005 and MUS-CMB-004: whether `PlayMusic` mixes over or replaces the cell's music, whether a cell change ends it, and whether it gives way to battle music; a vanilla quest that calls it is named
- [ ] Verify, MUS-STG-003 and MUS-STG-004: which event plays the Success Music object; whether Battle and Location resume or restart after a sting
- [ ] Verify, MUS-SEL-004 and MUS-CMB-002 (Fallout 3): the music fade, whether `fASFadeInTime` / `fRegionLoopFadeInTime` govern it, and the battle recovery delay
- [ ] Verify, PRD §3.4: whether the original New Vegas is silent where its `Default*` folders and death / success stings are absent

=== #6868 MUS-0d: verify Oblivion music precedence, fades and StreamMusic
Part of #6861 (#6860). PRD [§9 Phase 0](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence).

Settles the Oblivion **Phase 0: verify** items from the CS wiki or by observation in the original game. Each item is closed with a cited source or filed as its own issue.

## Acceptance Criteria
- [ ] Verify, MUS-SRC-008: the precedence `XCMT` → `SNAM` → region `RDMD` (a Default cell inside a region with a non-Default `RDMD`)
- [ ] Verify, MUS-SEL-004 and MUS-CMB-002 (Oblivion): the music fade and the battle recovery delay
- [ ] Verify, MUS-SCR-003: what a cell change does to a `StreamMusic` track

=== #6869 MUS-0e: write docs/engine/music.md (the music controller design)
Part of #6861 (#6860). PRD [§9 Phase 0](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence).

The design the PRD's Related table points at: layers, directives, outcomes, the per-game translate step, track selection, fades and `music.status`. It also covers the music half of the `audio` crate, which has no design document.

## Acceptance Criteria
- [ ] `docs/engine/music.md` describes the layer model, the directive type, the translate step per game and the controller's system and resource placement, citing the PRD's requirement IDs
- [ ] It folds in every Phase 0 verify result closed so far
- [ ] It is added to the PRD's Related table and document map and to `docs/engine/index.md`

=== #6870 MUS-1a: decode DOBJ music slots and keep the cell, worldspace and region music fields
Part of #6862 (#6860). PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6865.

The data half of the Location sources. Today `XCMT`/`XCMO` are parsed and discarded, `WRLD` music and `RDMD` have no reader, and `DOBJ` is an EditorID-only stub (PRD §3.5 rows 5 and 6).

## Acceptance Criteria
- [ ] MUS-SRC-003: the Fallout 3 / New Vegas `DOBJ` record decodes its five music slots (Default, Battle, Death, Success, Level Up)
- [ ] MUS-SRC-001 (data): a cell's `XCMO` music type and `XCMT` category reach the cell index instead of being discarded
- [ ] MUS-SRC-002 (data): `WRLD` `ZNAM`/`SNAM` and `REGN` `RDMO`/`RDMD` are readable from `byroredux`

=== #6871 MUS-1b: resolve music files loose-first, expand folder forms, report Missing
Part of #6862 (#6860). PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence).

None of the three games ships music in an archive (PRD §3.4); today only `PlayMusic` reads `Data\Music`, through its own lookup. One resolver replaces it.

## Acceptance Criteria
- [ ] MUS-SRC-011: the loose tier indexes a `music` root beside its four asset roots
- [ ] MUS-SRC-013: a music path resolves loose first, then in the sound archives, by the engine-wide precedence
- [ ] MUS-SRC-010: a folder-form source expands to the playable files directly in that folder
- [ ] MUS-SRC-012: an absent file or folder yields the outcome **Missing** naming the path

=== #6872 MUS-1c: music controller core and the Location layer (Fallout 3 / New Vegas music types)
Part of #6862 (#6860). PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6870, #6871.

The game-neutral controller (layers, directives, outcomes) and its first source. It replaces the region-music path in `dispatch_region_ambient_music` rather than sitting beside it.

## Acceptance Criteria
- [ ] The controller holds one directive per layer (Hold, Sting, Script, Battle, Location) with the outcomes Playing / Silent / Unresolved / Missing, and has no per-game branch past the translate step
- [ ] MUS-SRC-001, MUS-SRC-002: a cell's `XCMO`, else worldspace `ZNAM`, else region `RDMO`, else the Default Music object, becomes the Location directive (precedence per MUS-0c)
- [ ] MUS-SRC-004: a music type with no file (`1NoMusic`) gives Location the outcome **Silent**
- [ ] MUS-SEL-005: moving between places with the same Location directive does not restart the track

=== #6873 MUS-1d: track selection, looping and crossfades
Part of #6862 (#6860). PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6872.

How a directive's playlist plays (Q1 decided: shuffle).

## Acceptance Criteria
- [ ] MUS-SEL-001: a playlist shuffles; no track repeats until the folder has played through, a new round never starts on the last round's final track, and a one-track folder loops it
- [ ] MUS-SEL-002: when a non-looping track ends, the next track of the same directive starts
- [ ] MUS-SEL-003: a New Vegas music type loops when its `ANAM` is positive, at the volume MUS-0b settles
- [ ] MUS-SEL-004: a Location change crossfades over the directive's fade time (per MUS-0c / MUS-0d), else `REGN_AMBIENT_CROSSFADE_SECS`

=== #6874 MUS-1e: music.status and headless music resolution
Part of #6862 (#6860). PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6872.

Every music decision is reported, including in headless runs where nothing can be heard.

## Acceptance Criteria
- [ ] MUS-RPT-001: `music.status` prints, per layer, its outcome, the source record's EditorID and FormID, the file playing and its position
- [ ] MUS-RPT-002: each Unresolved source and each Missing path logs one line per session, never per frame
- [ ] MUS-RPT-003: a headless run (`--bench-hold`, no audio device) still resolves directives and reports them in `music.status`

=== #6875 MUS-1f: battle music layer
Part of #6862 (#6860). PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6872.

Battle music while the player is in combat (Q2 decided: `is_in_combat` plus a recovery hold).

## Acceptance Criteria
- [ ] MUS-CMB-001 (Fallout 3 / New Vegas object): while `is_in_combat` holds for the player, the Battle layer plays the Battle Music object
- [ ] MUS-CMB-002: after combat ends, Battle holds for the recovery time (per MUS-0c) before Location resumes
- [ ] MUS-CMB-004 (controller half): a Script directive that gives way to Battle waits while Battle plays

=== #6876 MUS-1g: death and level-up stings
Part of #6862 (#6860). PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6872.

The Sting layer, fed from the same death and level-up paths the console reaches.

## Acceptance Criteria
- [ ] MUS-STG-001, MUS-STG-002: the player's death plays the Death Music object and a level-up the Level Up Music object
- [ ] MUS-STG-003: the Success Music object stays unbound until MUS-0c names its event
- [ ] MUS-STG-004: a sting pauses Battle and Location for its length (resume or restart per MUS-0c)
- [ ] MUS-RPT-004: a test pins that `levelup xp <n>` and `obscript.cmd 0x14 108b` each raise their sting

=== #6877 MUS-1h: PlayMusic on the Script layer; movie sound off the music slot
Part of #6862 (#6860). PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6872, #6871.

Moves `music_system` onto the controller. Today `PlayMusic` refuses folder form (#6438) and a movie's sound replaces the scripted track (PRD §3.5 rows 7 and 11).

## Acceptance Criteria
- [ ] MUS-SCR-001: `PlayMusic` plays folder-form music types
- [ ] MUS-SCR-005: the Script layer mixes over Battle and Location (until MUS-0c settles the original's rule)
- [ ] MUS-SCR-002: a movie's sound no longer replaces or stops the Script layer

=== #6878 MUS-1i: music volume setting and the movie Hold layer
Part of #6862 (#6860). PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6872.

Q4 decided: a canonical music volume defaulting to each game's own default.

## Acceptance Criteria
- [ ] MUS-MIX-001: `audio.music_volume` exists, defaults to the game's `fDefaultMusicVolume` from its shipped default INI (Oblivion's `bMusicEnabled` off maps to 0), and scales only music
- [ ] MUS-MIX-002: a movie with the pause-music flag raises the Hold layer; every music layer pauses and resumes at the same position when the movie ends
- [ ] MUS-MIX-003: music still bypasses the reverb send and the underwater filter

=== #6879 MUS-1j: music across saves and loads
Part of #6862 (#6860). PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6872.

Q5 decided: a save keeps no music state.

## Acceptance Criteria
- [ ] MUS-SAV-001: after a load, the Location layer re-resolves from the loaded place before its first rendered frame
- [ ] MUS-SAV-002: no music state is saved; a scripted track ends on load, and the save registry's not-saved list says why for any new music resource

=== #6880 MUS-1k: music census test and coverage metrics (scenario 1)
Part of #6862 (#6860). PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6872, #6874.

PRD [§7.2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#72-acceptance) scenario 1 and the §8 metrics.

## Acceptance Criteria
- [ ] Scenario 1: an ignored `cargo test`, run with the game's data variable, translates every music source of the base and DLC masters and reports each as Playing-capable, Silent, Unresolved or Missing; counts match the MUS-0a census; passes on Fallout 3 and New Vegas
- [ ] Fallout 3's source coverage and playable coverage are published in ROADMAP's music column

=== #6881 MUS-1l: Fallout 3 music smoke gate (scenarios 2 to 5)
Part of #6862 (#6860). PRD [§9 Phase 1](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6875, #6876, #6878, #6874.

A `docs/smoke-tests/` script for PRD [§7.2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#72-acceptance) scenarios 2 to 5, indexed in `docs/smoke-tests/README.md`.

## Acceptance Criteria
- [ ] Scenario 2: `MegatonCratersideSupply`, `RedRacer03` and `MegatonWaterProcessingPlant` report Location Playing from `public\`, Playing from `dungeon\`, and Silent
- [ ] Scenario 3: `--grid 0,0` in `Wasteland` reports `DefaultExplore` Playing from `explore\`
- [ ] Scenario 4: `combat.approach` switches to Battle; `obscript.cmd <actor> 108b` returns to Location after the recovery time; `levelup xp <n>` and `obscript.cmd 0x14 108b` report the stings
- [ ] Scenario 5: `obscript.cmd none 1114 t:B01.bik 1 1 1 1` in `MegatonCratersideSupply` reports the Hold layer and the Location position unchanged after `video.skip`; New Vegas's intro `1 1 0 1` raises no Hold layer

=== #6882 MUS-2a: decode ALOC and New Vegas audio-marker references
Part of #6863 (#6860). PRD [§9 Phase 2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6865.

Today `ALOC` is an EditorID-only stub and audio markers are not read (PRD §3.5 row 6). Layout per xEdit `wbDefinitionsFNV.pas:8094-8124` and `:7616-7623`.

## Acceptance Criteria
- [ ] `ALOC` decodes its media-set lists per role (Neutral, Ally, Friend, Enemy, Location, Battle), conditional faction, day and night start times, location and retrigger delays
- [ ] An audio-marker `REFR` (`MMRK`) keeps its controller (`CNAM`) and layer trigger percentages on the placed reference
- [ ] A test against the MUS-0a census: 89 controllers and 572 markers on the New Vegas master

=== #6883 MUS-2b: audio-marker areas play their Location media sets, by day and night
Part of #6863 (#6860). PRD [§9 Phase 2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6882, #6866.

Where New Vegas's music actually lives (PRD §3.4). Q3 decided: faction-relation sets are a follow-up.

## Acceptance Criteria
- [ ] MUS-SRC-005: inside an audio marker's area (rule per MUS-0b), Location comes from the marker's controller's Location media sets
- [ ] MUS-SRC-006: day layers play between the controller's day and night start times, night layers otherwise, against `GameTimeRes`
- [ ] Q3: a controller with Neutral / Ally / Friend / Enemy lists plays its Location set, and `music.status` reports the relation sets as Unresolved

=== #6884 MUS-2c: media-set Outer / Middle / Inner layers
Part of #6863 (#6860). PRD [§9 Phase 2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6883.

Layer enabling by depth into a marker's area.

## Acceptance Criteria
- [ ] MUS-SRC-007: a Location media set enables its Outer, Middle and Inner layers by the player's distance into the marker's area, against the set's boundary percentages and enable flags, combined as MUS-0b settles

=== #6885 MUS-2d: New Vegas battle sets, intro and outro
Part of #6863 (#6860). PRD [§9 Phase 2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6882, #6875, #6866.

Battle music from media sets instead of the (unshipped) Default Battle folder.

## Acceptance Criteria
- [ ] MUS-CMB-001 (New Vegas): Battle plays the active controller's Battle sets, else the region's `RDSB` battle set (precedence per MUS-0b)
- [ ] MUS-CMB-002 (New Vegas): the Battle set's recovery time (`FNAM`) holds Battle after combat
- [ ] MUS-CMB-003: a Battle set's Intro plays on entering combat and its Outro on leaving it

=== #6886 MUS-2e: New Vegas media-set gate (scenario 6) and coverage
Part of #6863 (#6860). PRD [§9 Phase 2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6883, #6884, #6885.

PRD [§7.2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#72-acceptance) scenario 6.

## Acceptance Criteria
- [ ] Scenario 6: standing at the Goodsprings audio marker named by MUS-0a, by day and by night, `music.status` reports its controller's Location set with the day and night layers in turn; the gate is indexed in `docs/smoke-tests/README.md`
- [ ] New Vegas's source coverage is published in ROADMAP's music column

=== #6887 MUS-3a: Oblivion music categories, battle folder and stings
Part of #6864 (#6860). PRD [§9 Phase 3](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6872, #6875, #6876, #6868.

Oblivion's translate step: a category enum, not a record, picks a `Data\Music` folder (Q6 decided: Default plays Explore).

## Acceptance Criteria
- [ ] MUS-SRC-008: the Location category comes from the cell's `XCMT`, else the worldspace's `SNAM`, else the winning region's `RDMD` (precedence per MUS-0d)
- [ ] MUS-SRC-009: Public plays `Data\Music\Public`, Dungeon `Data\Music\Dungeon`, Default `Data\Music\Explore`
- [ ] MUS-CMB-001 (Oblivion): Battle plays `Data\Music\Battle`
- [ ] MUS-STG-001, MUS-STG-002 (Oblivion): death plays `Special\death.mp3` and level-up `Special\success.mp3`

=== #6888 MUS-3b: StreamMusic
Part of #6864 (#6860). PRD [§9 Phase 3](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6877, #6887.

Today `StreamMusic` is not dispatched (PRD §3.5 row 8). Behaviour per [CS: StreamMusic](https://cs.uesp.net/wiki/StreamMusic); the restart quirks are deliberately not reproduced (PRD §6 row 5).

## Acceptance Criteria
- [ ] MUS-SCR-003: `StreamMusic` plays a quoted file relative to the game folder, or a random track for `Public`, `Explore`, `Dungeon` and `Random`, on the Script layer, and hands back to Location when the track ends
- [ ] MUS-CMB-004 (Oblivion half): a `StreamMusic` directive gives way to battle music
- [ ] A missing file reports Missing and leaves the current track playing (MUS-SRC-012)

=== #6889 MUS-3c: OBSE GetCellMusicType / SetCellMusicType (optional)
Part of #6864 (#6860). PRD [§9 Phase 3](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6887.

A MAY requirement, catalogued under LMC-SCR-004.

## Acceptance Criteria
- [ ] MUS-SCR-004: OBSE `GetCellMusicType` and `SetCellMusicType` have catalog entries that read and write the cell's category, and a change re-resolves the Location layer

=== #6890 MUS-3d: Oblivion music gate (scenario 7) and coverage
Part of #6864 (#6860). PRD [§9 Phase 3](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#9-delivery-sequence). Depends on #6887, #6888.

PRD [§7.2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/game-music.md#72-acceptance) scenario 7.

## Acceptance Criteria
- [ ] Scenario 7: `BrumaJGhastasHouse` reports Location from `public\` and `SageGlenHollow` from `dungeon\`; `StreamMusic Random` reports the Script layer; the gate is indexed in `docs/smoke-tests/README.md`
- [ ] Oblivion's source coverage is published in ROADMAP's music column

