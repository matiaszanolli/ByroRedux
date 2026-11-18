# Bink video playback — Product Requirements

**Status**: SHIPPED (2026-11-17). Revision 13 settles the verify items #6896–#6898 from the executables. Revision 12 records ACTIVE → SHIPPED. Revision 11 records the owner's acceptance of the decoders' provenance risk (#6907). Revision 10 folds in review round 2: BIK-DEC-004 is met (a frame that does not decode now ends the movie Failed), the stale HEAD claims are history, and the provenance questions go to the owner in #6907. Revision 9 refreshes the HEAD table. Revision 8 extends Q2 at the owner's direction: Bink 2 sound decodes in process too, through the workspace's clean-room Bink Audio decoder. Revision 7 re-decides Q2 at the owner's direction: Bink 2 video decodes in process through the workspace's clean-room decoder. Revision 6 re-decides Q1 at the owner's direction: Bink 1 decodes in process, no FFmpeg at run time. Revision 4 files the tracking issues. Revision 3 records the owner's approval
with every §10 decision taken as recommended and folded into the
requirements (BIK-DEC-003, BIK-DEC-005, BIK-REQ-005). Statements about how
an original engine, format or tool behaves cite external sources; a claim
that could not be verified is marked **Phase 0: verify** instead of being
stated as fact.

**Tracking**: #6855 (parent). Phases: #6856 (0), #6857 (1), #6858 (2), #6859 (3). Verify items: #6896–#6898. Decoder provenance: #6907 (risk accepted by the owner).

**Scope**: the games' own Bink movies play: the intro and logo movies the
engine starts, and every movie a script asks for (`PlayBink`). Each plays
full-screen with its sound, holds the game while it runs, and ends early on
Escape when its request allows it. For every title whose movies the engine
can decode. Outside it: encoding or authoring Bink, linking RAD's Bink SDK,
Scaleform menu movies and main-menu background movies. Bink 2 video and
sound are in scope since decision Q2's re-decision (BIK-DEC-003).

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Menus, native pages and the pause they impose | [Game UI Integration — PRD](game-ui-integration.md#31-state-at-head) (§3.1 row 16) |
| Sound playback and its tracks | The `audio` crate (M44) has no design document; [Video playback](../engine/video-playback.md) §3 covers the movie's audio half (its own slot, the hold) |
| The new-game route that starts the FNV intro | [Playable vertical slice](../engine/playable-vertical-slice.md#from-the-beginning--a-new-game-2026-11-11) |
| Decoding, presentation, A/V sync, start sequences | [Video playback](../engine/video-playback.md) (Phase 0, #6856) |
| The Bink 2 decoders' format rules and their provenance | [`crates/bink2`](../../crates/bink2/README.md) (video) and [`crates/binkaudio`](../../crates/binkaudio/README.md) (sound): their specs (`docs/spec/`) and `CLEANROOM.md` |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `BIK-` prefix.

---

## 1. Product thesis

The owner's goal for New Vegas is "make FNV playable from the beginning".
The original begins a new game with the intro quest VCG00 running
`PlayBink "FNVIntro.bik" 1 1 0 1`; before this PRD the engine recorded the
request and logged `video FNVIntro.bik (no Bink decoder)`. Every title in the lineage ships
movies the same way (census in §3): logo sequences, intros, ending films.
They are part of how the games begin and end.

The scripting contract is documented for every era. `PlayBink` "opens a Bink
video file" from `Data\Video`, with flags for Escape to interrupt, muting the
world, pausing music and letterboxing, and "appears to stop game time"
([GECK: PlayBink](https://geck.uesp.net/wiki/PlayBink)). Oblivion's takes one
Escape flag and "Stops any script that it is called in from running while
the .bik file is played" ([CS: PlayBink](https://cs.uesp.net/wiki/PlayBink)).
Papyrus's `Game.PlayBink` freezes "the game (including Papyrus)" until the
movie ends or is cancelled
([CK: PlayBink](https://ck.uesp.net/wiki/PlayBink_-_Game)), and Fallout 4
adds a flag for "the bink we show at the start of a new game"
([Fallout 4 CK: PlayBink](https://falloutck.uesp.net/wiki/PlayBink_-_Game)).

It is for players, who see the games open and close as they were made, and
for mod authors whose quests call `PlayBink`.

The corollary that shapes every requirement: **a movie is played or it is
reported, never silently dropped.** The engine plays what it can decode,
says per file what it cannot, and never stalls the game on a movie it cannot
play. The project is "FOSS-only by design. No proprietary dependencies, no
closed-source middleware" ([README](../../README.md#why-byroredux-exists)), so
what can be decoded is bounded by the in-process open decoders: `mercs2_bink`
for Bink 1, the workspace's `byroredux-bink2` and `byroredux-binkaudio` for
Bink 2 (Q1, Q2; the owner accepted their provenance as a documented risk, #6907). FFmpeg is
a development-time measurement tool only (`ffprobe` in
`scripts/bink-corpus.sh`), never a run-time dependency.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Bink 1** | RAD's first Bink format; file magic `BIK` and a revision letter ([MultimediaWiki: Bink Container](https://wiki.multimedia.cx/index.php/Bink_Container)); every vanilla file in the corpus below is `BIKi` |
| **Bink 2** | RAD's second format; file magic `KB2` and a revision letter (same page); the corpus holds `KB2g` and `KB2j` |
| **Movie request** | A movie to play, with its flags: from a script (`PlayBink`) or from the engine's own start sequence |
| **Flags** | The request's options: GECK `PlayBink`'s four (interruptable, mute audio, pause music, letterboxed), CS `PlayBink`'s one (allow Escape), Papyrus's arguments |
| **Start sequence** | The movies the original plays outside any script, named by INI settings (`SIntroSequence`, `SMainMenuMovieIntro`, `sIntroMovie`) |
| **Corpus** | The Bink files under each installed game's `Data\Video` |

---

## 3. Core model

A movie request ends in one of three outcomes, each reported:

| Outcome | Meaning | Reported by |
|---|---|---|
| **Played** | Every frame decoded and shown, sound with it; *ended early* when Escape or `video.skip` ends an interruptable request | `video.status` (BIK-RPT-001) |
| **Skipped** | The file's format has no decoder in this build (a Bink 1 revision other than `BIKi`, a Bink 2 revision other than `KB2g` / `KB2j`), or there is no window to show it in | one log line per file and `video.status` |
| **Failed** | The file is missing, does not open, or a frame does not decode (BIK-DEC-004) | one log line and `video.status`; the game continues |

The corpus below is Phase 0's census, measured on 2026-11-11 from the
installed games (`ffprobe` and the file magic; game files are never checked
in). The live counts and decode rate are ROADMAP's compatibility matrix,
Video column (§8).

| Game | Files in `Data\Video` | Bink 1 | Bink 2 | Audio tracks per file | Example |
|---|---|---|---|---|---|
| Oblivion | 9 | 9 | 0 | 0–1 | `OblivionIntro.bik` |
| Fallout 3 | 35 | 35 | 0 | 0–1 | `Fallout INTRO Vsk.bik`, `B01.bik` … |
| Fallout: New Vegas | 1 | 1 | 0 | 1 | `FNVIntro.bik`: 1280×720, 30 fps, Bink audio 48 kHz stereo, 289.7 s |
| Skyrim SE | 1 | 1 | 0 | 2 | `BGS_Logo.bik` |
| Fallout 4 | 15 | 1 (a mod's) | 14 | 0–6 | `Intro.bk2`: 1920×1080, 23.976 fps, five Bink audio tracks |
| Fallout 76 | 4 | 0 | 4 | 0–6 | — |
| Starfield | 36 | 0 | 36 | 0–6 | `PowerVision_*.bk2` |

Fallout 4's one Bink 1 file, `threeweekslater_withSound.bik`, was added by
the Sim Settlements 2 mod in the measured install: vanilla Fallout 4 ships
Bink 2 only. FFmpeg decodes both streams of `FNVIntro.bik` (`binkvideo`,
`binkaudio_dct`) and the audio of `Intro.bk2`, but `ffprobe` reports the
Bink 2 video codec as unknown: the patch adding a decoder was never merged
([ffmpeg-devel, 2020-03](https://ffmpeg.org/pipermail/ffmpeg-devel/2020-March/258774.html)).
The workspace's own decoders (Q2, re-decided 2026-11-13) decode the video and
the sound of every Bink 2 file above.

What starts each game's movies: the default INIs, and each executable's
compiled-in defaults where the INI does not set the key (read out of the
executables on 2026-11-13; method and values in
[video-playback.md](../engine/video-playback.md) §5):

| Game | Start sequence | New-game movie |
|---|---|---|
| Oblivion | `SIntroSequence` (four logos), then `SMainMenuMovieIntro=Oblivion iv logo.bik`; `Oblivion_default.ini` and `Oblivion.exe` agree | `OblivionIntro.bik`: the compiled-in default of the `sOblivionIntro` setting. Plays at a new game, once, from the cell-change routine (§5.1 note; #6897, settled 2026-11-17) |
| Fallout 3 | `SIntroSequence` and `SMainMenuMovieIntro` empty in `Fallout_default.ini` and `Fallout3.exe`; the INI also sets `sIntroMovie=Fallout INTRO Vsk.bik`, empty compiled-in | `sIntroMovie` plays at a new game, from the routine after the new-game starter, interruptable, muted, music-paused and letterboxed (#6897, settled 2026-11-17) |
| Fallout: New Vegas | Same settings as Fallout 3; `Fallout INTRO Vsk.bik` is not installed | VCG00 runs `PlayBink "FNVIntro.bik" 1 1 0 1` (interruptable, muted, music not paused, letterboxed), then `FNVLogo1.bik` and `FNVLogo2.bik`, which are not installed |
| Skyrim SE | `Skyrim_Default.ini` names no movie; `SkyrimSE.exe`'s `sIntroSequence` is `BGS_Logo.bik` | — |
| Fallout 4 | `Fallout4_Default.ini` names no movie; `Fallout4.exe`'s `sIntroSequence` is `GameIntro_V3_B.bk2` | Papyrus `PlayBink` with the new-game flag (Fallout 4 CK) |
| Starfield | `Starfield.ini` names no movie; `Starfield.exe`'s `sIntroSequence` is `BGS_LOGO_1080p_BinkVersion.bk2` | — |

The engine's state at HEAD on 2026-11-14 (all phases built; design: [video-playback.md](../engine/video-playback.md)):

| # | Piece | Today | Proof |
|---|---|---|---|
| 1 | ObScript `PlayBink` | FO3 / FNV / Oblivion: a `VideoRequest` carries the file and the four flags (Oblivion's one flag; the rest default). The string's own value leads the VM's argument list, so the flags are read from index 1 | `cmd_1114`, `crates/scripting/src/obscript_more/tail.rs`, registered `Games::Both`; `a_string_form_commands_text_value_leads_its_other_arguments`, `crates/scripting/src/obscript_vm.rs` |
| 2 | Where a request goes | `step_video` hands it to the movie player, which opens the file under `Video` (any case, `..\` out of it), draws frames full screen, holds game time like the loading cover, ends on the last frame or on Escape / `video.skip` for an interruptable request | `VideoPlayer`, `byroredux/src/video.rs`; `step_video`, `byroredux/src/app_step.rs`; `docs/smoke-tests/fnv-new-game.sh` leg 0 |
| 3 | Papyrus `Game.PlayBink` | Lowers to `Effect::PlayBink` (the VM evaluates a run-time name) with the CK's defaults and Fallout 4's new-game flag; the script freezes until the movie ends (`FragmentResumeCondition::MovieEnded`) | `prim_play_bink`, `crates/scripting/src/translate/effects/prims/world.rs`; `play_bink_freezes_the_run_until_the_movie_ends` |
| 4 | Start sequence (INI movies) | `sIntroSequence` then `sMainMenuMovieIntro` from the install's default INI, else the executable's compiled-in default, queued once the boot load ends; `--skip-movies` skips them, a `--bench-frames` run unless `--start-movies` | `start_sequence`, `byroredux/src/video.rs`; `docs/smoke-tests/bik-start-sequence.sh` |
| 5 | Decoder | In process: Bink 1 (`BIKi`) video and audio through `mercs2_bink` (Q1, re-decided 2026-11-12); Bink 2 (`KB2g`, `KB2j`) video and sound through `byroredux-bink2` and `byroredux-binkaudio` (Q2, re-decided 2026-11-13), the first audio track; other revisions are reported Skipped, and a frame that does not decode ends the movie Failed (BIK-DEC-004). The mute and pause-music flags hold the world's sound and the music (`AudioWorld::hold_for_movie`), and the picture follows the soundtrack's playback position | `crates/video/src/lib.rs`, `crates/bink2`, `crates/binkaudio`; the ignored corpus test in `crates/video` |

`scripts/bink-corpus.sh` re-measures the census; `video.status` reports a
run's live state.

---

## 4. Goals and non-goals

### 4.1 Goals

- **BIK-G-001 — The FNV intro plays.** A new game plays `FNVIntro.bik`
  before Doc Mitchell's opening, then the route continues.
- **BIK-G-002 — The Bink 1 corpus decodes.** Every Bink 1 file of every
  in-scope game decodes to its last frame.
- **BIK-G-003 — Script movies keep their flags.** A `PlayBink` request
  plays with the behaviour its flags ask for.

### 4.2 Non-goals

- **BIK-NG-001.** Linking RAD's Bink SDK or any closed decoder: the
  project is FOSS-only ([README](../../README.md#why-byroredux-exists)).
  An open decoder whose format description was read out of RAD's library
  with a disassembler (`mercs2_bink`) is not closed: the owner accepted it,
  with the procedural clean rooms of `byroredux-bink2` and
  `byroredux-binkaudio`, as a documented risk (Q1, Q2, #6907).
- **BIK-NG-002.** Encoding or authoring Bink files.
- **BIK-NG-003.** Scaleform and MenuXml menu movies: the
  [Game UI Integration PRD](game-ui-integration.md)'s surfaces.
- **BIK-NG-004.** Main-menu background movies (Oblivion's
  `smainmenumovie=Map Loop.bik`,
  [UESP: Oblivion.ini](https://en.uesp.net/wiki/Oblivion_Mod:Oblivion.ini)):
  the engine has no main menu and no PRD owns one today (§6 row 3).
- **BIK-NG-005.** Fallout 76: no playable flow starts its movies (§7.1).

---

## 5. Requirements by area

### 5.1 Requests (`BIK-REQ`)

- **BIK-REQ-001.** ObScript `PlayBink` MUST carry its flags with the file
  name to the player: the GECK's four on FO3 / FNV, defaulting to
  interruptable 0, mute 1, pause 1, letterbox 1, and the CS's one Escape
  flag on Oblivion, through one handler: Oblivion's script supplies one flag
  and the rest take their defaults. Before Phase 2 (2026-11-11) FO3 / FNV
  dropped the flags and Oblivion's command was not dispatched (§3 HEAD
  row 1).
- **BIK-REQ-002.** Papyrus `Game.PlayBink` MUST reach the same player with
  its arguments, including Fallout 4's new-game flag. Before Phase 2 there
  was no native (§3 HEAD row 3).
- **BIK-REQ-003.** A request MUST resolve its file relative to
  `Data\Video` as a loose file, as the GECK states for FO3 / FNV ("they
  can't be packaged into a BSA File"); a missing file is **Failed**. For
  Skyrim, Fallout 4 and Starfield, Phase 0 found no vanilla archive that
  holds a Bink file ([video-playback.md](../engine/video-playback.md) §6):
  their movies ship loose. A mod that packs its movies in an archive
  does not play here, and the original does not play it either: its movie-open
  routines read files from disk (deviation 8, #6898).
- **BIK-REQ-004.** The start sequence MUST play `sIntroSequence`, then
  `sMainMenuMovieIntro`, each from the install's default INI when it sets
  the key (even to nothing) and from the executable's compiled-in default
  otherwise (Oblivion's settings:
  [UESP: Oblivion.ini](https://en.uesp.net/wiki/Oblivion_Mod:Oblivion.ini);
  every game's compiled-in defaults: §3). A user INI is not read
  (deviation 6). Fallout 3's `sIntroMovie` and Oblivion's `OblivionIntro.bik`
  (the `sOblivionIntro` setting) play at a new game, read out of the executables
  (#6897, [video-playback.md](../engine/video-playback.md) §5). The engine queues
  them after the start sequence on a new-game session (`NewGameStart`), when the
  file is installed.

- **BIK-REQ-005.** Engine-started movies MUST play where the original
  plays them, and a launch flag MUST skip every one of them for development
  and the gates (Q3). A `--bench-frames` run skips them too unless
  `--start-movies` asks for them (deviation 4).

### 5.2 Decoding (`BIK-DEC`)

- **BIK-DEC-001.** Bink 1 video MUST decode to frames at the file's
  resolution and frame rate.
- **BIK-DEC-002.** Bink audio MUST decode to PCM at the file's sample rate
  and channel count. Which of several tracks plays (Skyrim's logo has
  two, Fallout 4's intro five) is settled (#6896, 2026-11-17): the tracks are
  language versions of one mix (`crates/binkaudio`, probe A9), and Fallout 4 maps
  `sLanguage` to a track ID (`en` 1, `fr` 2, `it` 3, `de` 4, `es` 5, `ja` 8, any other
  1) before `BinkSetSoundTrack`. The engine plays the track with ID 1 (the first track
  of a file without one). Skyrim's two tracks and the tracks of Fallout 4's
  `GameIntro_V3_B` and Starfield's `EndingVision` are sample-identical.
- **BIK-DEC-003.** Bink 2 video (`KB2g`, `KB2j`) and its sound MUST decode in
  process through the workspace's clean-room decoders `byroredux-bink2` and
  `byroredux-binkaudio` (MIT, pure Rust, no dependencies) to frames at the
  file's resolution and frame rate and to PCM at the track's rate and
  channels (Q2). Another Bink 2 revision MUST be **Skipped** with one log
  line; a track the audio decoder does not take (anything but 48 kHz stereo
  DCT, the only kind in the corpus) MUST leave the picture playing without
  sound, with one log line.
- **BIK-DEC-005.** Bink 1 MUST decode in process through a pure-Rust,
  MIT-licensed library, so a player installs no external tool; the workspace
  MUST NOT link FFmpeg or any native decoder, and a format the library does not
  decode MUST be **Skipped** with one log line (Q1).
- **BIK-DEC-004.** A frame that fails to decode MUST end the movie as
  **Failed**, once the frames before it have been shown, with a log line
  naming the file and frame and the same reason in `video.status`; the game
  MUST continue (`a_frame_that_does_not_decode_ends_the_movie_as_failed`,
  `crates/video`; `a_frame_that_does_not_decode_is_reported_failed`,
  `byroredux/src/video.rs`).

### 5.3 Presentation (`BIK-PRES`)

- **BIK-PRES-001.** With the letterbox flag set, frames MUST keep their
  aspect ratio inside the window with bars on the unfilled sides. With the
  flag clear, the movie MUST fill the whole window. Settled (#6898, 2026-11-17) from
  Fallout 3's draw routine, which resets the viewport to the whole render target
  for a movie that is not letterboxed (the GECK says "the bink will not be
  letterboxed if it is widescreen and is showing on a 4:3 screen").
- **BIK-PRES-002.** Game time MUST stop while a movie plays: the GECK's
  "appears to stop game time" and the CK's "the game (including Papyrus) is
  completely frozen".
- **BIK-PRES-003.** The picture MUST follow the soundtrack's playback
  position (`movie_clock`, `byroredux/src/video.rs`, reads
  `AudioWorld::movie_sound_position`), so sound and picture cannot drift
  apart; a movie with no sound, or no audio device, MUST follow the wall
  clock. No test pins the clock's source yet.
- **BIK-PRES-004.** Escape MUST end an interruptable movie and MUST NOT end
  one that is not.
- **BIK-PRES-005.** The mute and pause flags MUST silence world sound and
  pause music for the movie's length, and `video.status` MUST report both.
- **BIK-PRES-006.** The script that requested a movie MUST resume only when
  it ends. Papyrus: the run suspends until the movie ends, the CK's freeze.
  ObScript: the world is held from the next frame, but the rest of the
  calling run executes the frame the request is made (deviation 7; the CS's
  "Stops any script that it is called in from running" is met for later
  frames only).

### 5.4 Reporting and drivers (`BIK-RPT`)

- **BIK-RPT-001.** `video.status` MUST print the movie playing, its frame
  index and count, its flags, and the outcome (§3) of the last request.
- **BIK-RPT-002.** A headless run (`--bench-hold`) MUST play a movie to its
  last frame, so the gates can drive it without a display.
- **BIK-RPT-003.** `video.skip` MUST act as Escape on the playing movie,
  honouring its interruptable flag: the window-level Escape key is out of
  `input.press`'s reach (the `KeyCode::Escape` arm in
  `byroredux/src/app_events.rs` that calls `VideoPlayer::request_skip`).

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Plays Bink 2 (Fallout 4, Fallout 76, Starfield) through RAD's library, shipped beside the executable (`bink2w64.dll` in each install) | Bink 2 video and sound decoded by the workspace's clean-room decoders (Q2); the English track (ID 1, else the first) plays | none: the original picks the language's track and the engine has no language setting (#6896) | A non-English install plays English (BIK-DEC-002, BIK-DEC-003) |
| 2 | Plays Bink 1 through RAD's library (`binkw32.dll` in each install) | Q1's library decodes Bink 1, Q2's decoders Bink 2 `KB2g` / `KB2j` | — | A format they cannot decode plays no movie. **Skipped** is reported, never silent (BIK-DEC-003, BIK-RPT-001) |
| 3 | Main menu with a background movie (`smainmenumovie`, UESP *Oblivion.ini*) | No main menu, so no menu movie (BIK-NG-004) | — | Unowned: no PRD promises a main menu yet. Recorded here so the gap has a home |
| 4 | The start sequence plays at every launch | A `--bench-frames` run skips it unless `--start-movies` is given | — | A benchmark or gate never sees it by accident; `bik-start-sequence.sh` passes `--start-movies` (BIK-REQ-005) |
| 5 | Fallout 3's `sIntroMovie` is interruptable (#6897); whether the other start movies can be skipped is not recorded | Start movies are interruptable | — | A player may skip a logo the original would not let them; low (BIK-REQ-004) |
| 6 | The user INI (`My Games\<game>\<game>.ini`) holds the settings the player changes (UESP *Oblivion.ini*) | Only the default INI and the executable's compiled-in defaults are read; `--skip-movies` stands in for the common "skip the intro" edit | — | A player who blanked `sIntroSequence` still sees the logos (BIK-REQ-004, BIK-REQ-005) |
| 7 | ObScript `PlayBink` "Stops any script that it is called in from running" (CS *PlayBink*) | The rest of the calling run executes that frame; the world is held from the next | — | A statement after `PlayBink` in the same run takes effect before the movie (BIK-PRES-006) |
| 8 | The original plays only files on disk: Oblivion, Fallout 3 and Fallout 4 `stat` the path and `BinkOpen` it without `BINKFROMMEMORY` (#6898, settled 2026-11-17) | Only loose files under `Data\Video` play | — | A mod that packs its movies (Sim Settlements 2's `ss2_xpac_chapter3 - main.ba2`) gets **Failed** (BIK-REQ-003) |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow how each title reaches a movie today: FNV's new-game route
reaches one now.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 2. Bink 1 only; the start sequence comes from documented INI settings (BIK-REQ-004); `PlayBink` gets dispatched with its one flag (BIK-REQ-001) |
| Fallout 3 | yes | Wave 1. Bink 1 only; shares FNV's `PlayBink` |
| Fallout: New Vegas | yes | Wave 1. Reference title; its new-game route reaches `FNVIntro.bik` today |
| Skyrim SE | yes | Wave 2. One Bink 1 logo, its start sequence; Papyrus `Game.PlayBink` (BIK-REQ-002). LE and AE are not in the census |
| Fallout 4 | yes | Wave 2. Its Bink 2 films play with their first audio track (BIK-DEC-002, BIK-DEC-003); a mod's Bink 1 file plays too |
| Fallout 76 | no | No playable flow starts its movies (BIK-NG-005) |
| Starfield | yes | Wave 2. Every file is Bink 2 (Q2); Papyrus `Game.PlayBink` reaches them (BIK-REQ-002). Its start sequence is wired (BIK-REQ-004) but no smoke fixture gates it |

### 7.2 Acceptance

Game data comes from local installs only: the corpus test reads
`BYROREDUX_STEAM_COMMON` (the Steam library's `steamapps/common`), the smoke
gates the per-game variables (`BYROREDUX_FNV_DATA`, …). Nothing is checked
in.

A scenario passes when:

1. **Corpus (every installed game, headless).** The ignored
   `every_installed_bink_movie_decodes_to_its_last_frame` (`crates/video`)
   decodes every Bink file in each game's `Data\Video` to its last frame
   (BIK-G-002, BIK-DEC-003) and checks that each Bink 2 file with tracks
   brings its first soundtrack. It does not assert the audio's rate or
   channel count (BIK-DEC-002).
2. **FNV intro (headless).** In `fnv-new-game.sh`, `video.status` reports
   `FNVIntro.bik` playing with flags `1 1 0 1` at the new game; `video.skip`
   ends it as **Played**; VCG00 then moves the player (BIK-G-001).
3. **Flags (FNV, headless; `docs/smoke-tests/bik-flags.sh`).** VCG00's
   flags reach the player and the audio hold (world muted, music playing);
   `video.skip` ends the interruptable movie as **Played** (ended early) and
   leaves a non-interruptable one playing (BIK-PRES-004, BIK-PRES-005,
   BIK-RPT-003).
4. **Bink 2 (Fallout 4, headless; `docs/smoke-tests/bik-start-sequence.sh
   fo4`).** Fallout 4's start sequence, `GameIntro_V3_B.bk2` (Bink 2 with
   six audio tracks), plays to its last frame with its first track
   (BIK-DEC-003).

---

## 8. Metrics

The numbers live in [ROADMAP](../../ROADMAP.md)'s per-game compatibility
matrix, Video column.

| Metric | Definition | Granularity |
|---|---|---|
| Bink decode rate | Bink files decoded to their last frame ÷ Bink files in `Data\Video`, Bink 1 and Bink 2 apart | per game |

Two metrics were withdrawn in revision 10: movie coverage equals the decode
rate while every file in the corpus has a decoder, and A/V drift is zero by
construction, because the picture follows the soundtrack's clock
(BIK-PRES-003).

---

## 9. Delivery sequence

**Phase 0 — Corpus, verify list, design (no engine code).** Check in the
corpus script that re-measures §3's census; close the **Phase 0: verify**
items (BIK-REQ-003, BIK-REQ-004, BIK-DEC-002, BIK-PRES-001, the §3 start
rows, the open Bink 2 decoders) or file each as an issue (filed
#6896–#6898); write
`docs/engine/video-playback.md` and add it to Related; decide Q1–Q3. Exit:
the census is in ROADMAP for every in-scope game, the design is merged, and
every verify item is closed or filed.

**Phase 1 — The decoder.** BIK-DEC-001 to BIK-DEC-005.
Exit: acceptance scenario 1 passes for every Bink file of every in-scope
game, and the decode rate is published.

**Phase 2 — Playing a request.** BIK-REQ-001 to BIK-REQ-003, BIK-PRES-001
to BIK-PRES-006, BIK-RPT-001 to BIK-RPT-003. Depends on the
native-page pause ([Game UI Integration §3.1](game-ui-integration.md#31-state-at-head),
row 16) and the `audio` crate's streaming playback (M44). Exit: scenarios
2, 3 and 4 pass.

**Phase 3 — Start sequences.** BIK-REQ-004 and BIK-REQ-005 for every
in-scope game. Exit: a headless boot of Oblivion and of Skyrim plays its
start sequence to its last frame (`bik-start-sequence.sh`, which gates
Fallout 4's too; Starfield's is wired but has no smoke fixture).

---

## 10. Decisions required

1. **Q1 — Where the Bink 1 decoder comes from.** Recommended: load the
   system FFmpeg (`libavcodec`, LGPL) at run time when it is installed and
   report **Skipped** when it is not; the MIT code links nothing, and the
   decoder is the one the census measured. Alternative: write a Rust Bink 1
   decoder from published format descriptions; no native dependency and
   MIT throughout, but a large job, and **Phase 0: verify** that the
   descriptions are complete enough to decode the corpus. Alternative: port
   an existing open decoder to Rust (FFmpeg's; NihAV's, whose licence is a
   **Phase 0: verify**); the fastest self-contained route, but the crate
   then carries that decoder's licence inside an MIT project.
   **Decided** (2026-11-11): as recommended: the system FFmpeg, reached at run time; the MIT code links nothing, and without it a movie is Skipped (BIK-DEC-005).
   **Re-decided** (2026-11-12, owner): ship a basic player inside the engine so
   players install no tool, using a Rust library where one exists. The choice
   is `mercs2_bink` 0.1.0 (MIT, pure Rust, no dependencies), pinned: it decodes
   `BIKi` video and DCT audio, and on every Bink 1 file of Oblivion, Fallout 3,
   Fallout: New Vegas, Skyrim SE and Fallout 4 every video frame is bit-identical
   to FFmpeg's, and its audio within one 16-bit step (measured with a probe
   against the FFmpeg tools, 2026-11-12). It does not
   decode other Bink 1 revisions, alpha or grayscale planes, or RDFT audio; none
   occurs in the corpus. **Provenance, for the owner:** its author reports the
   format description was read out of RAD's `binkw32.dll` with a disassembler and
   states that no third-party decoder source was read
   ([SPEC-video.md](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game),
   shipped inside the crate); this repository has not verified that claim.
   **Provenance accepted** (2026-11-14, owner): the risk is accepted as
   documented; `mercs2_bink` keeps shipping (#6907).
2. **Q2 — Bink 2.** Recommended: **Skipped** with a log line until Phase 0
   has measured the open Bink 2 decoders, then decide on that measurement.
   **Phase 0: verify** NihAV's reported Rust decoder
   ([NihAV: some words about Bink2](https://codecs.multimedia.cx/2020/08/some-words-about-bink2/))
   and the unmerged FFmpeg patch
   ([ffmpeg-devel, 2020-03](https://ffmpeg.org/pipermail/ffmpeg-devel/2020-March/258774.html)):
   their decode rate on the `KB2g` / `KB2j` corpus and their licences.
   Alternative: adopt or port the better of them now; Fallout 4's and
   Starfield's films play, at the cost of an unmeasured decoder and its
   licence. Alternative: play the audio alone over a black screen; the
   player hears a film they cannot see.
   **Decided** (2026-11-11): as recommended: Bink 2 video is Skipped until Phase 0 has measured the open Bink 2 decoders; the choice is then revisited on that measurement (BIK-DEC-003).
   **Re-decided** (2026-11-13, owner): decode Bink 2 video in process with the
   workspace's own decoder, `crates/bink2` (`byroredux-bink2`, MIT, pure Rust, no
   dependencies; BIK-DEC-003). It was written for this project in a separate
   clean-room repository from the MultimediaWiki format pages and black-box probes
   of the games' own `bink2w64.dll` (exported functions only, never its code), and
   reads no other decoder's source; the rules, the provenance log and a disclosure
   of one earlier exposure to facts about NihAV's decoder (which makes the process
   procedural rather than two-party clean room, a question for counsel) are in
   [`crates/bink2/CLEANROOM.md`](../../crates/bink2/CLEANROOM.md). Measured
   2026-11-13 against that DLL: every frame of every Bink 2 file of Fallout 4,
   Fallout 76 and Starfield has planes identical to the DLL's (the counts and
   decode times: [`crates/bink2/README.md`](../../crates/bink2/README.md)).
   **Extended** (2026-11-13, owner): the sound too, through `crates/binkaudio`
   (`byroredux-binkaudio`, MIT, pure Rust, no dependencies), written the same way
   from the MultimediaWiki *Bink Audio* page and black-box probes of the DLL's
   exported track API (`BinkOpenTrack`, `BinkGetTrackData`). Measured 2026-11-13:
   every track of every Bink 2 file with sound has the DLL's sample count, and a
   small fraction of samples differ by one, where the DLL's 32-bit float
   arithmetic rounds a .5 tie the other way (the counts:
   [`crates/binkaudio/README.md`](../../crates/binkaudio/README.md)). Its
   [`CLEANROOM.md`](../../crates/binkaudio/CLEANROOM.md) discloses that the
   authoring model was very probably trained on public code including FFmpeg's
   Bink audio decoder: a procedural clean room, a question for counsel.
   **Provenance accepted** (2026-11-14, owner): both procedural clean rooms are
   accepted as documented in their `CLEANROOM.md`, and the decoders keep
   shipping. Their specs (`docs/spec/`) leave a two-party re-implementation
   possible should review ever be wanted (#6907).
3. **Q3 — When an engine-started movie plays.** Recommended: at the point
   the original plays it (logo sequence at launch, the intro at a new
   game), with a launch flag that skips them for development and the gates.
   Alternative: only on request (`PlayBink`), never at launch; faster
   starts, but Oblivion's and Skyrim's start sequences never play.
   **Decided** (2026-11-11): as recommended: engine-started movies play where the original plays them, with a launch flag that skips them (BIK-REQ-005).

---

## 11. Document map

| Need | Go to |
|---|---|
| The pause native pages already impose; menus | [Game UI Integration — PRD](game-ui-integration.md#31-state-at-head) |
| The new-game route the FNV intro opens | [Playable vertical slice](../engine/playable-vertical-slice.md#from-the-beginning--a-new-game-2026-11-11) |
| Decoding, presentation, A/V sync, start sequences, the Phase 0 verify list | [video-playback.md](../engine/video-playback.md) |
| The Bink 2 video and audio decoders, their measurements and clean-room rules | [crates/bink2/README.md](../../crates/bink2/README.md), [crates/binkaudio/README.md](../../crates/binkaudio/README.md) |
| The decoders' provenance and the owner's acceptance | #6907; Q1, Q2 |
| The project's dependency stance | [README](../../README.md#why-byroredux-exists) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-11 | First draft, at the owner's request (the FNV new-game route reaches its intro movie) | — |
| 2 | 2026-11-11 | Review round 1 folded in: Oblivion PlayBink not dispatched + one flag, census fps/track corrections, open Bink 2 decoders (NihAV, FFmpeg patch) and Q1/Q2 alternatives, verify items closed from ESM/INI/CS, headless drivers for scenarios 2–3 (`t:` strings already supported; `video.skip` added), audio-track and BSA verifies, main-menu movie owner, letterbox/mute observables, Phase 2 dependencies, index row | claude: revise |
| 3 | 2026-11-11 | Owner approval: Q1–Q3 decided as recommended and folded in (BIK-DEC-003 reworded; BIK-DEC-005, BIK-REQ-005 added) | owner: approved |
| 4 | 2026-11-11 | APPROVED → ACTIVE: tracking issues filed (#6855 (parent). Phases: #6856 (0), #6857 (1), #6858 (2), #6859 (3)) | — |
| 5 | 2026-11-11 | §3 HEAD table refreshed for the shipped decoder, request flags and movie player (BIK-DEC-001/002/004/005, BIK-REQ-001/003, BIK-PRES-001/002/004/006, BIK-RPT-001..003); mute/pause flags, A/V sync, Papyrus native, start sequences open | — |
| 6 | 2026-11-12 | Owner-directed: Q1 re-decided: Bink 1 decodes in process through `mercs2_bink`, no FFmpeg at run time (BIK-DEC-005 reworded; §3 HEAD rows 2 and 5; §5 row 2; provenance note) | — |
| 7 | 2026-11-13 | Owner-directed: Q2 re-decided: Bink 2 video decodes in process through the clean-room `byroredux-bink2` (BIK-DEC-003 reworded, moved to Phase 1, Bink 2 audio verify item; scope line; Related row; §3 Skipped row, census note, HEAD row 5; §6 rows 1–2; §7.1 Fallout 4 and Starfield; acceptance 1 and 4; decode-rate metric; provenance note) | — |
| 8 | 2026-11-13 | Owner-directed: Q2 extended to Bink 2 sound through the clean-room `byroredux-binkaudio` (BIK-DEC-003 reworded: sound decodes, the no-sound rule and its verify item dropped; BIK-DEC-002 notes that tracks are language versions and the first plays; scope line; Related row; §3 census note and HEAD row 5; §6 row 1; Fallout 4 and Starfield in §7.1; acceptance 1 and 4; Q2 provenance note with the training-data disclosure) | — |
| 9 | 2026-11-13 | §3 HEAD rows 3–5 refreshed for the built Papyrus native and freeze, the mute / pause hold and A/V lock, and the start sequences (Phases 0–3, #6856–#6859); Related row points to `video-playback.md` | — |
| 10 | 2026-11-14 | Review round 2 folded in: BIK-DEC-004 met by a code fix (a frame that does not decode ends the movie Failed, two tests), stale "Today" claims (BIK-REQ-001/-002, thesis), BIK-PRES-006 narrowed and BIK-PRES-003 restated as the sound clock, start-sequence table and BIK-REQ-004 (executable defaults, Starfield), Fallout 4's mod Bink 1 file, verify items linked to #6896–#6898, provenance to #6907, format and original-engine citations, coverage and A/V-drift metrics withdrawn, §6 rows 4–8, scenarios 1, 3, 4 named to their gates, numbers moved to the crate READMEs, document map | claude: revise |
| 11 | 2026-11-14 | Owner-directed: the decoders' provenance risk is accepted (#6907): Q1 and Q2 record it, BIK-NG-001 says a disassembly-derived open decoder is not closed, thesis and document map updated | — |
| 12 | 2026-11-14 | ACTIVE → SHIPPED: Phases 0–3 closed (#6856–#6859, #6855); the decode rate is published in ROADMAP's compatibility matrix, Video column; the provenance risk is accepted (#6907); verify items #6896–#6898 stay open | — |
| 13 | 2026-11-17 | Verify items #6896–#6898 settled from the executables (track = the language's ID, `en` 1; `sIntroMovie` and `sOblivionIntro` play at a new game; the letterbox flag clear fills the window; the original opens only files on disk); BIK-DEC-002, BIK-REQ-004, BIK-PRES-001, §6 rows 1, 5, 8 and the start-sequence table updated; the engine plays the English track by ID and queues the new-game movie of Oblivion and Fallout 3 after the start sequence on a new-game session | — |
