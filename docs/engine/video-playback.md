# Video playback

**Status**: ACTIVE (2026-11-13). The design the
[Bink video playback PRD](../prd/bink-video-playback.md) names as the owner of
decoding, presentation and A/V sync (PRD *Related*). Tracking: #6855; phases
#6856 (0, this document), #6857 (1), #6858 (2), #6859 (3). The PRD states the
promise and the `BIK-` requirements. This document states how the engine keeps
them.

## 1. Pieces

| Piece | Where | Role |
|---|---|---|
| Decoders | `crates/video` (`byroredux-video`) | `Movie::open` sorts a file by its magic: Bink 1 `BIKi` through `mercs2_bink` (Q1), Bink 2 `KB2g` / `KB2j` through the clean-room `byroredux-bink2` and `byroredux-binkaudio` (Q2). Any other revision is `VideoError::Unsupported` (**Skipped**); an unreadable file or a frame that fails is **Failed** |
| Requests | `obscript_more::PendingVideos` (`crates/scripting`) | `VideoRequest { file, interruptable, mute_audio, pause_music, letterbox, new_game }`, queued by ObScript `PlayBink` (opcode `0x1114`), Papyrus `Game.PlayBink` (`Effect::PlayBink`), and the engine's start sequence |
| Player | `byroredux/src/video.rs` (`VideoPlayer`) | One movie at a time, in queue order: open, show frames, end, report |
| Audio | `crates/audio` (`AudioWorld`) | The movie's own sound slot, its playback position, and the hold its flags put on the world's sound and music |
| Observables | `video.status`, `video.skip` (`byroredux/src/commands/video.rs`) | BIK-RPT-001 and BIK-RPT-003 |

## 2. Decoding

A reader thread decodes frames ahead of playback into a small bounded ring
(`FRAME_RING`), converts them to RGBA at the window's size (letterboxed when
the flag is set), and hands them to `Movie::advance(elapsed)`. That call is the
whole playback contract: the caller owns the clock and asks which frame belongs
at a given time. A decoder that falls behind holds the last frame; one that runs
ahead is held back by the ring. The movie's first audio track is decoded once,
up front, into a temporary WAV that the audio crate streams (BIK-DEC-002: the
first track plays until #6896 settles which one the original picks; a Bink 2
file's tracks are language versions of one mix, `crates/binkaudio` probe A9).

## 3. Presentation

- **Overlay.** Frames are uploaded into one overlay texture, kept between
  movies, and drawn full-screen through the quad the loading cover and the HUD
  use. Letterboxing happens at conversion, so the overlay is always
  window-sized (BIK-PRES-001).
- **Holding the game (BIK-PRES-002).** While a movie is on screen the app pins
  the scheduler's `dt` to 0, exactly as the loading cover does
  (`cover_holds_game`). World input is released.
- **The clock (BIK-PRES-003).** The movie clock starts with its first frame,
  together with its sound. While the soundtrack plays, the clock is the
  soundtrack's playback position (`AudioWorld::movie_sound_position`), so the
  frame shown is always the one the sound is at, and the two cannot drift. With
  no sound (none in the file, no audio device, or a soundtrack that ends early)
  the clock is the wall clock.
- **Flags (BIK-PRES-005).** `AudioWorld::hold_for_movie(mute, pause)` pauses
  every playing world sound (and drops one-shots raised under the movie) when
  `mute_audio`, and the cell and scripted music layers when `pause_music`.
  `release_movie_hold` resumes them where they stopped when the movie ends.
  `video.status` prints the request's flags and what the audio world actually
  holds (`holding world_muted=… music_paused=…`).
- **Escape (BIK-PRES-004, BIK-RPT-003).** The Escape key and `video.skip` raise
  `VideoControl::skip`. It ends an interruptable movie as **Played (ended
  early)**, and is logged and consumed for any other movie.

## 4. Scripts and the freeze (BIK-REQ-001, -002, BIK-PRES-006)

- **ObScript** `PlayBink "<file>" [interruptable mute pause letterbox]` reads
  the GECK's four flags on FO3/FNV (defaults `0 1 1 1`) and the CS's one on
  Oblivion, and queues the request. The ObScript VM has no mid-block
  suspension, so the statements after `PlayBink` in the same run execute that
  frame, before the movie starts. While the movie plays, the world is held at
  `dt = 0`. **Deviation:** the CS's "Stops any script that it is called in from
  running" is met for later frames, not for the rest of the calling run.
- **Papyrus** `Game.PlayBink(file, interruptible = false, muteAudio = true,
  muteMusic = true, letterbox = true[, isNewGameBink = false])` (CK and Fallout
  4 CK *PlayBink - Game*). It lowers to `Effect::PlayBink` when its arguments are
  literals. Otherwise the VM evaluates them. Either way the request is queued
  and the script **suspends** with the `FragmentResumeCondition::MovieEnded`
  continuation, which resumes once no movie is queued or playing
  (`obscript_more::movies_idle`, fed by the `MoviePlaying` flag the app writes
  each frame). That is the CK's "It will not return until the movie is over, or
  the user has canceled the movie".
- Files resolve under `Data\Video`, case-insensitively, `..\` climbing out of it
  (the GECK's `PlayBink "..\sound\videos\x.bik"`). A missing file is **Failed**
  (BIK-REQ-003).

## 5. Start sequences (BIK-REQ-004, -005)

Once a run, when the boot load has finished, the player queues the game's start
sequence ahead of any request: `sIntroSequence` (comma- or semicolon-separated),
then `sMainMenuMovieIntro`. Each setting is read from the install's shipped
default INI when it sets the key (even to nothing), and from the executable's
compiled-in default otherwise.

The compiled-in defaults were read out of each executable on 2026-11-13. A
Bethesda setting object stores its default immediately before the pointer to its
`name:Section` string, so the setting's string is found, then the pointer to it,
then the value before it:

| Game | `sIntroSequence` | `sMainMenuMovieIntro` | Default INI |
|---|---|---|---|
| Oblivion | `bethesda softworks HD720p.bik,2k games.bik,game studios.bik,Oblivion Legal.bik` | `Oblivion iv logo.bik` | `Oblivion_default.ini`, same values |
| Fallout 3 / New Vegas | empty | empty | `Fallout_default.ini`, both empty; it sets only `sIntroMovie=Fallout INTRO Vsk.bik` |
| Skyrim SE | `BGS_Logo.bik` | empty | `Skyrim_Default.ini`, neither key |
| Fallout 4 | `GameIntro_V3_B.bk2` | no such setting | `Fallout4_Default.ini`, neither key |
| Starfield | `BGS_LOGO_1080p_BinkVersion.bk2` | no such setting | `Starfield.ini`, neither key |

`OblivionIntro.bik` is a bare string in `Oblivion.exe`, not a setting.
Fallout 3's `sIntroMovie` is a setting whose compiled-in default is empty. When
the original plays either (presumably at a new game) is verify item #6897.
Neither is played at launch.

Start movies are interruptable, muted, music-paused and letterboxed. The
interruptable choice is the engine's: the original's is not recorded. A user INI
override (`My Games\<game>\<game>.ini`) is not read; `--skip-movies` stands in
for the common "skip the intro" tweak.

`--skip-movies` skips them (BIK-REQ-005). A `--bench-frames` run also skips them
unless `--start-movies` asks for them. Benchmarks and every headless gate measure
the world, and a 30-second logo sequence at the start would hold it (the
deviation from "a launch flag skips them" is that the benchmark flag implies it).

## 6. Phase 0 census and verify items

`scripts/bink-corpus.sh` re-measures the corpus (§3 of the PRD; `ffprobe` for the
track counts). The decode rate is published in ROADMAP's per-game compatibility
matrix (video column) from the ignored corpus test
`every_installed_bink_movie_decodes_to_its_last_frame` (`crates/video`).

No vanilla archive of any in-scope game holds a Bink file (Oblivion 17 archives,
FO3 16, FNV 21, Skyrim SE 23, FO4 187, Starfield 129, listed with
`archive_list_all`): the games ship their movies loose under `Data\Video`. The
one archive hit is a mod's (`ss2_xpac_chapter3 - main.ba2`,
`video\threeweekslater_withsound.bk2`, Sim Settlements 2). The same mod's loose
`threeweekslater_withSound.bik` is Fallout 4's one "Bink 1" file in the PRD's
census. A mod that packs movies in an archive does not play (lead in #6898).

| Item | Settled by | State |
|---|---|---|
| BIK-REQ-003: loose files only on Skyrim, FO4, Starfield | whether any archive holds a Bink file | closed: no vanilla archive does |
| BIK-REQ-004: what FO3's `sIntroMovie` plays and when; where Oblivion's new-game intro and Skyrim's logo are named | the executables' settings and strings | Skyrim's logo and Oblivion's start sequence: closed (§5). FO3's `sIntroMovie` and `OblivionIntro.bik` *timing*: filed #6897 |
| BIK-DEC-002: which audio track the original plays | listening against the original | filed #6896 |
| BIK-PRES-001: what the original does with the letterbox flag clear | observation in the original | filed #6898 (with the archive lead) |
| Q2: the open Bink 2 decoders | superseded | closed: Q2 re-decided 2026-11-13. The workspace's clean-room decoders are measured against the games' own DLL (PRD §10) |
