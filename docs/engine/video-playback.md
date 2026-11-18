# Video playback

**Status**: SHIPPED (2026-11-14). The design the
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
ahead is held back by the ring. A frame that does not decode ends the ring with
its reason (file and frame): `advance` shows the frames decoded before it, then
returns `Playhead::Failed`, and the player reports the movie **Failed** with that
reason in `video.status` (BIK-DEC-004). The movie's audio track is decoded once,
up front, into a temporary WAV that the audio crate streams (BIK-DEC-002, settled
by #6896). A Bink 2 file's tracks are language versions of one mix
(`crates/binkaudio` probe A9) and carry the language's ID. `Fallout4.exe` maps its
`sLanguage` setting to a track ID before `BinkSetSoundTrack`: `en` 1, `fr` 2,
`it` 3, `de` 4, `es` 5, `ja` 8, any other language 1. The engine has no language
setting, so it plays the track with ID 1 and, in a file without that ID, the
first. Oblivion and Fallout 3 never ask Bink for a track (no import of
`BinkSetSoundTrack`, no `BINKSNDTRACK` open flag) and none of their movies has
more than one; Skyrim SE imports no track call either, and the two tracks of its
`BGS_Logo.bik` decode to identical samples (Starfield's six-track `EndingVision`
and Fallout 4's `GameIntro_V3_B` are identical across tracks too).

## 3. Presentation

- **Letterbox flag clear (BIK-PRES-001, #6898).** Fallout 3's draw routine
  (`0xb5efb0`) skips a step when the letterbox argument is non-zero and otherwise,
  if the game's viewport flag (`0x131a446`) is set, resets the viewport to the
  whole render target before drawing the movie and again after. So a movie that is
  not letterboxed fills the entire window (no bars, no crop, not its native size),
  and a letterboxed one is drawn inside the viewport the game already has. That is
  what the engine does: `fit` stretches to the window when the flag is clear. What
  the flag-clear picture does with a widescreen movie on a 4:3 screen is the GECK's
  "will not be letterboxed", i.e. it is stretched. The quad's scale and offset
  fields (`1, 1` and `0, 0` when reset) were not traced to every writer.
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
  early)**, and is logged and consumed for any other movie. It is the only
  input a movie lets through: like the loading cover, it discards every other
  key, mouse, wheel, touch and IME event and releases mouse look
  (`App::cover_holds_game`, #7000).

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

`OblivionIntro.bik` is the compiled-in default of Oblivion's `sOblivionIntro:General`
setting (its name string sits right after the file name, as `sCreditsMenuMovie`'s
does), and `Oblivion_default.ini` does not set it. Fallout 3's `sIntroMovie` is a
setting whose compiled-in default is empty and whose INI value is
`Fallout INTRO Vsk.bik`. Both play **at a new game** (#6897, read out of
`Oblivion.exe` and `Fallout3.exe` on 2026-11-17):

- Both executables start a new game the same way. The new-game starter looks up the
  character-generation quest (Oblivion: the routine that logs "Unable to find
  character generation quest '%s'." at `0x5b5df0`, FO3: the handler at `0x9671b0`
  that reads `sCharGenQuest`) and sets a flag (Oblivion: byte `+0x7f8` of the player
  object; FO3: global `0x1239beb`).
- A later routine plays the movie while that flag is set (Oblivion: the cell-change
  routine `0x66eaf0`, at `0x66f2be`, passing the file name from `sOblivionIntro`
  and clearing the flag, so it plays once; FO3: `0x8bfac0`, at `0x8c071f`, passing
  `sIntroMovie`). FO3 keeps its global set until the player leaves for the main
  menu, and what limits its play to one is not decoded (`0x8bfac0` has seven callers,
  three of them in the save-game load region, where the flag is clear). Nothing else
  in either executable names the movie.
- FO3's call passes interruptable, mute and pause-music `1` and letterbox `1`
  (the arguments the script command feeds from its flags). Oblivion's call passes the
  script command's own argument list with its one flag set to `1`; what Oblivion's
  other arguments mean was not decoded. FO3's `sMainMenuMovieIntro` (empty by
  default) is played interruptable, muted, music-paused and **not letterboxed**.
- The engine queues the movie when the session is a new game (`NewGameStart`: no
  location named, or `--new-game`, and not restoring a save; in practice `--new-game`,
  because `--esm` alone is refused without `--cell` or `--grid`), right after the start
  sequence, as an interruptable, muted, music-paused, letterboxed request with
  `new_game: true` (`new_game_movie`, `VideoPlayer::queue_start_sequence`). Fallout
  3 and New Vegas share a game kind and an INI, and New Vegas ships no
  `Fallout INTRO Vsk.bik` (its VCG00 plays `FNVIntro.bik`), so a movie that is not
  installed is not queued. `--skip-movies` skips it with the start sequence. The
  engine has no Oblivion or Fallout 3 chargen or birth flow. Their profiles name the
  new-game cell (`assets/debug_profiles.toml`: Fallout 3 `Vault101d`, where the
  start markers of the chargen quest CG00 named by `Fallout_default.ini` sit;
  Oblivion `ImperialDungeon01`, where the actors of the exe's `Charactergen` quest
  sit), so `--game fo3 --new-game` and `--game oblivion --new-game` reach it; the
  movie plays when the world is up, not between the new-game menus and a birth
  sequence.
- **Live run (2026-11-17, release build, headless Xvfb, `--new-game --start-movies
  --bench-frames 30 --bench-hold`, one engine at a time).** Oblivion, with
  `--cell AbandonedMine`: the log line `video: start sequence [five logos],
  new-game movie Some("OblivionIntro.bik")`, then the five start movies each
  `Played { ended_early: false }` (300, 180, 299, 122 and 418 frames), then
  `OblivionIntro.bik` (1280x720, 115.5 s, one audio track, soundtrack=yes,
  `new_game=true`) `Played { ended_early: false }` after 3461 frames. Fallout 3,
  with `--cell MegatonMoriartysSaloon`: `video: start sequence [], new-game movie
  Some("Fallout INTRO Vsk.bik")`, then that movie (1280x720, 243.5 s, one audio
  track, soundtrack=yes, `new_game=true`) `Played { ended_early: false }` after
  7299 frames. Not exercised: the skip flags, a New Vegas launch (its missing file
  is filtered by `movie_path`), and a window with a hardware display (the run's
  frames were not looked at; the played-to-the-last-frame outcome comes from the
  player's log, not a screenshot).

Neither is played at launch.

Start movies are interruptable, muted, music-paused and letterboxed, as FO3's
`sIntroMovie` is. A user INI
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
census. A mod that packs movies in an archive does not play here, and it does
not in the original either (#6898): Oblivion, Fallout 3 and Fallout 4 each check
the file with `stat` (`_stat64i32` in Fallout 4, which tries an explicit path and
then `Data\Video\<name>`) and hand the path to `BinkOpen` with the flags
preload (`0x2000`), no-MMX (`0x8000000`) and, in Fallout 4, sound-track
(`0x4000`); none sets `BINKFROMMEMORY`, so Bink reads the file from disk. The GECK
page for `PlayBink` says the same ("they can't be packaged into a BSA File"). Skyrim
SE and Starfield were not read.

| Item | Settled by | State |
|---|---|---|
| BIK-REQ-003: loose files only on Skyrim, FO4, Starfield | whether any archive holds a Bink file | closed: no vanilla archive does |
| BIK-REQ-004: what FO3's `sIntroMovie` plays and when; where Oblivion's new-game intro and Skyrim's logo are named | the executables' settings, strings and code | closed (§5): both play at a new game; the engine has no new-game flow for them yet (#6897) |
| BIK-DEC-002: which audio track the original plays | `Fallout4.exe`'s `BinkSetSoundTrack` caller and the tracks' samples | closed (§2): the language's track, `en` is ID 1, and the engine plays it (#6896) |
| BIK-PRES-001: what the original does with the letterbox flag clear | `Fallout3.exe`'s movie draw routine | closed (§3): the movie fills the whole render target (#6898) |
| BIK-REQ-003 (archive lead): a movie packed in an archive | the movie-open routines of Oblivion, FO3 and FO4 | closed (§6): the original opens only files on disk (#6898) |
| Q2: the open Bink 2 decoders | superseded | closed: Q2 re-decided 2026-11-13. The workspace's clean-room decoders are measured against the games' own DLL (PRD §10) |
