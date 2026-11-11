# Bink video playback — Product Requirements

**Status**: PROPOSED (2026-11-11). Revision 1, the first draft, written at
the owner's request after the FNV new-game route reached its intro movie
and could only log it. Statements about how an original engine, format or
tool behaves cite external sources; a claim that could not be verified is
marked **Phase 0: verify** instead of being stated as fact.

**Scope**: the games' own Bink movies play: the intro and logo movies the
engine starts, and every movie a script asks for (`PlayBink`). Each plays
full-screen with its sound, holds the game while it runs, and ends early on
Escape when its request allows it. For every title whose movies the engine
can decode. Outside it: encoding or authoring Bink, linking RAD's Bink SDK,
Scaleform menu movies (the game UI PRD's), and Bink 2 video unless decision
Q2 takes it in.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Menus, pages and the pause they impose | [Game UI Integration — PRD](game-ui-integration.md) |
| Sound playback and its tracks | none: the `audio` crate (M44) has no design document; Phase 0's design covers the movie's audio half |
| The new-game route that starts the FNV intro | [Playable vertical slice](../engine/playable-vertical-slice.md#from-the-beginning--a-new-game-2026-11-11) |
| Decoding, presentation, A/V sync | none yet: Phase 0 writes `docs/engine/video-playback.md` |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `BIK-` prefix.

---

## 1. Product thesis

The owner's goal for New Vegas is "make FNV playable from the beginning".
The original begins a new game with `FNVIntro.bik`; this engine records the
request and logs `video FNVIntro.bik (no Bink decoder)`. Every title in the
lineage ships movies the same way: Oblivion's logo sequence and intro,
Fallout 3's thirty-five films, Skyrim's logo, Fallout 4's intro and S.P.E.C.I.A.L.
films (census in §3). They are part of how the games begin and end.

The scripting contract is documented for every era. `PlayBink` "opens a Bink
video file" from `Data\Video`, with flags for Escape to interrupt, muting the
world, pausing music and letterboxing, and "appears to stop game time"
([GECK: PlayBink](https://geck.uesp.net/wiki/PlayBink)); Oblivion's takes an
Escape flag ([CS: PlayBink](https://cs.uesp.net/wiki/PlayBink)); Papyrus's
`Game.PlayBink` freezes "the game (including Papyrus)" until the movie ends
or is cancelled ([CK: PlayBink](https://ck.uesp.net/wiki/PlayBink_-_Game)),
and Fallout 4 adds a flag for "the bink we show at the start of a new game"
([Fallout 4 CK: PlayBink](https://falloutck.uesp.net/wiki/PlayBink_-_Game)).

It is for players, who see the games open and close as they were made, and
for mod authors whose quests call `PlayBink`.

The corollary that shapes every requirement: **a movie is played or it is
reported, never silently dropped.** The engine plays what it can decode,
says per file what it cannot, and never stalls the game on a movie it cannot
play. The project is "FOSS-only by design. No proprietary dependencies, no
closed-source middleware" ([README](../../README.md#why-byroredux-exists)), so
what can be decoded is bounded by the open decoders that exist (Q1, Q2).

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Bink 1** | RAD's first Bink format; file magic `BIK` and a revision letter (`BIKi` in every title's corpus below) |
| **Bink 2** | RAD's second format; file magic `KB2` and a revision letter (`KB2g`, `KB2j`) |
| **Movie request** | A movie to play, with its flags: from a script (`PlayBink`) or from the engine's own start sequence |
| **Flags** | GECK `PlayBink`'s four: interruptable, mute audio, pause music, letterboxed |
| **Start sequence** | The movies the original plays outside any script, named by INI settings (Oblivion's `SIntroSequence`, `SMainMenuMovieIntro`) |
| **Corpus** | The Bink files under each installed game's `Data\Video` |

---

## 3. Core model

A movie request ends in one of three outcomes, each reported:

| Outcome | Meaning | Reported by |
|---|---|---|
| **Played** | Every frame decoded and shown, sound with it, or ended early by Escape on an interruptable request | `video.status` (BIK-RPT-001) |
| **Skipped** | The file's format has no decoder in this build (Bink 2 under Q2's recommendation; Bink 1 when Q1's decoder is absent) | one log line per file and `video.status` |
| **Failed** | The file is missing or does not decode | one log line and `video.status`; the game continues |

The corpus below was measured on 2026-11-11 from the installed games
(`ffprobe` and the file magic; game files are never checked in).

| Game | Files in `Data\Video` | Bink 1 | Bink 2 | Example |
|---|---|---|---|---|
| Oblivion | 9 | 9 | 0 | `OblivionIntro.bik` |
| Fallout 3 | 35 | 35 | 0 | `B01.bik`, `1 year later.bik` … |
| Fallout: New Vegas | 1 | 1 | 0 | `FNVIntro.bik`: 1280×720, 29.97 fps, Bink audio 48 kHz stereo, 289.7 s |
| Skyrim SE | 1 | 1 | 0 | `BGS_Logo.bik` |
| Fallout 4 | 15 | 1 | 14 | `Intro.bk2`: 1920×1080, 23.976 fps, four Bink audio tracks |
| Fallout 76 | 4 | 0 | 4 | — |
| Starfield | 36 | 0 | 36 | `PowerVision_*.bk2` |

FFmpeg decodes both streams of `FNVIntro.bik` (`binkvideo`,
`binkaudio_dct`) and the audio of `Intro.bk2`, but reports the Bink 2 video
codec as unknown: no open Bink 2 video decoder exists to build on.

The engine's state at HEAD on 2026-11-11:

| # | Piece | Today | Proof |
|---|---|---|---|
| 1 | ObScript `PlayBink` | Records the file name only; the four flags are dropped | `cmd_1114`, `crates/scripting/src/obscript_more/tail.rs` |
| 2 | Where a request goes | Logged as `video <file> (no Bink decoder)` and drained | `script_ui_request_system`, `byroredux/src/systems/script_requests.rs` |
| 3 | Papyrus `Game.PlayBink` | No native | no `PlayBink` symbol under `crates/scripting/src/fragment` |
| 4 | Start sequence (INI movies) | Not read | — |
| 5 | Decoder | None | — |

The census table is re-measured by Phase 0's checked-in corpus script; the
HEAD table is replaced by `video.status` once Phase 2 ships.

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
- **BIK-NG-002.** Encoding or authoring Bink files.
- **BIK-NG-003.** Scaleform and MenuXml menu movies: owned by the
  [Game UI Integration PRD](game-ui-integration.md).
- **BIK-NG-004.** Main-menu background movies (Oblivion's `Map loop.bik`,
  Fallout 4's `MainMenuLoop.bk2`): the engine has no main menu, which is the
  game UI PRD's to add.
- **BIK-NG-005.** Fallout 76: no playable flow starts its movies (§7.1).

---

## 5. Requirements by area

### 5.1 Requests (`BIK-REQ`)

- **BIK-REQ-001.** ObScript `PlayBink` MUST carry its four flags with the
  file name to the player, defaulting as the GECK states (interruptable 0,
  mute 1, pause 1, letterbox 1). Today the flags are dropped (§3 row 1).
- **BIK-REQ-002.** Papyrus `Game.PlayBink` MUST reach the same player with
  its arguments, including Fallout 4's new-game flag. Today there is no
  native (§3 row 3).
- **BIK-REQ-003.** A request MUST resolve its file relative to
  `Data\Video` as loose files, the GECK's "they can't be packaged into a BSA
  File"; a missing file is **Failed** (§3).
- **BIK-REQ-004.** The start sequence MUST play from the game's INI
  settings: for Oblivion `SIntroSequence`, then `SMainMenuMovieIntro`
  ([UESP: Oblivion.ini](https://en.uesp.net/wiki/Oblivion_Mod:Oblivion.ini)).
  **Phase 0: verify** the settings Fallout 3, New Vegas, Skyrim and
  Fallout 4 use, and whether `FNVIntro.bik` comes from a script or the
  start sequence; settled by each game's default INI and the scripts that
  name the file.

### 5.2 Decoding (`BIK-DEC`)

- **BIK-DEC-001.** Bink 1 video MUST decode to frames at the file's
  resolution and frame rate.
- **BIK-DEC-002.** Bink audio MUST decode to PCM at the file's sample rate
  and channel count; a file with several audio tracks MUST play the first
  unless the design shows the original chooses another.
- **BIK-DEC-003.** Bink 2 video MUST follow decision Q2. Today, under its
  recommendation, it is **Skipped** with one log line per file.
- **BIK-DEC-004.** A frame that fails to decode MUST end the movie as
  **Failed** with a log line naming the file and frame, and the game MUST
  continue.

### 5.3 Presentation (`BIK-PRES`)

- **BIK-PRES-001.** Frames MUST cover the window, letterboxed when the
  letterbox flag is set and the aspect ratios differ.
- **BIK-PRES-002.** Game time MUST stop while a movie plays: the GECK's
  "appears to stop game time" and the CK's "the game (including Papyrus) is
  completely frozen".
- **BIK-PRES-003.** Sound MUST stay within one video frame of the picture
  for the whole movie.
- **BIK-PRES-004.** Escape MUST end an interruptable movie and MUST NOT end
  one that is not.
- **BIK-PRES-005.** The mute and pause flags MUST silence world sound and
  pause music for the movie's length.
- **BIK-PRES-006.** A Papyrus call MUST return when its movie ends, as the
  CK states. **Phase 0: verify** whether an ObScript `PlayBink` ends its
  script's run as player `MoveTo` does
  ([CS: MoveTo](https://cs.uesp.net/wiki/MoveTo)); settled by the GECK
  talk pages or a test in the original.

### 5.4 Reporting (`BIK-RPT`)

- **BIK-RPT-001.** `video.status` MUST print the movie playing, its frame
  index and count, and the outcome (§3) of the last request.
- **BIK-RPT-002.** A headless run (`--bench-hold`) MUST play a movie to its
  last frame, so the gates can drive it without a display.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Plays Bink 2 (Fallout 4, Fallout 76, Starfield) through RAD's SDK | Bink 2 video **Skipped** under Q2's recommendation (BIK-NG-001) | — | Fallout 4's intro and S.P.E.C.I.A.L. films do not play. Reported per file (BIK-DEC-003); Q2 names the alternatives |
| 2 | Decoder built into the game | Decoder from Q1, possibly absent from a build | — | A build without it plays no movie. **Skipped** is reported, never silent (BIK-DEC-003, BIK-RPT-001) |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow how each title reaches a movie today: FNV's new-game route
reaches one now.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 2. Nine Bink 1 movies; the start sequence comes from documented INI settings (BIK-REQ-004) |
| Fallout 3 | yes | Wave 1. Thirty-five Bink 1 movies; shares FNV's `PlayBink` |
| Fallout: New Vegas | yes | Wave 1. Reference title; its new-game route reaches `FNVIntro.bik` today |
| Skyrim (LE / SE / AE) | yes | Wave 2. One Bink 1 logo; Papyrus `Game.PlayBink` (BIK-REQ-002) |
| Fallout 4 | partly | Wave 2. Its one Bink 1 file plays; its fourteen Bink 2 files follow Q2 |
| Fallout 76 | no | No playable flow starts its movies (BIK-NG-005) |
| Starfield | per Q2 | All thirty-six files are Bink 2 |

### 7.2 Acceptance

Game data comes from local installs only (`BYROREDUX_FNV_DATA` and the
other per-game variables the smoke scripts read); nothing is checked in.

A scenario passes when:

1. **Corpus (per game, headless).** An ignored `cargo test`, run with the
   game's data variable, decodes every Bink 1 file in `Data\Video` to its
   last frame (BIK-G-002).
2. **FNV intro (headless).** `fnv-new-game.sh` sees `video.status` report
   `FNVIntro.bik` playing at the new game, then **Played** (by Escape if the
   request is interruptable), before VCG00 moves the player (BIK-G-001).
3. **Flags (FNV, headless).** `PlayBink "FNVIntro.bik" 0` ignores Escape
   and `PlayBink "FNVIntro.bik" 1` ends on it (BIK-PRES-004).
4. **Reporting.** A Bink 2 request on Fallout 4 logs one **Skipped** line
   and the game continues (BIK-DEC-003).

---

## 8. Metrics

The numbers live in ROADMAP's per-game compatibility matrix, in a video
column added by Phase 1.

| Metric | Definition | Granularity |
|---|---|---|
| Bink 1 decode rate | Bink 1 files decoded to their last frame ÷ Bink 1 files in `Data\Video` | per game |
| Movie coverage | Files the build can play (**Played** possible) ÷ files in `Data\Video` | per game |
| A/V drift | Largest gap between the audio and video clocks over a movie | per file, worst per game |

---

## 9. Delivery sequence

**Phase 0 — Corpus, verify list, design (no engine code).** Check in the
corpus script that re-measures §3's census; close the two **Phase 0:
verify** items (BIK-REQ-004, BIK-PRES-006) or file each as an issue; write
`docs/engine/video-playback.md` and add it to Related; decide Q1–Q3. Exit:
the census is in ROADMAP for every in-scope game, the design is merged, and
every verify item is closed or filed.

**Phase 1 — The decoder.** BIK-DEC-001, BIK-DEC-002, BIK-DEC-004 under Q1.
Exit: acceptance scenario 1 passes for every Bink 1 file of every in-scope
game, and the decode rate is published.

**Phase 2 — Playing a request.** BIK-REQ-001 to BIK-REQ-003, BIK-PRES-001
to BIK-PRES-006, BIK-RPT-001, BIK-RPT-002, BIK-DEC-003. Depends on the
audio track API and the native page overlay. Exit: scenarios 2, 3 and 4
pass.

**Phase 3 — Start sequences.** BIK-REQ-004 for every in-scope game. Exit:
a headless boot of Oblivion and of Skyrim plays its start sequence to its
last frame.

---

## 10. Decisions required

1. **Q1 — Where the Bink 1 decoder comes from.** Recommended: load the
   system FFmpeg (`libavcodec`, LGPL) at run time when it is installed and
   report **Skipped** when it is not; the MIT code links nothing, and the
   decoder is the one the census measured. Alternative: write a Rust Bink 1
   decoder from published format descriptions; no native dependency and
   MIT throughout, but a large job, and **Phase 0: verify** that the
   descriptions are complete enough to decode the corpus. Alternative: port
   FFmpeg's decoder to Rust; the fastest self-contained route, but the
   crate is then LGPL-derived inside an MIT project, which is a licence
   decision.
2. **Q2 — Bink 2.** Recommended: **Skipped** with a log line until an open
   decoder exists; nothing to build on today (§3). Alternative: play the
   audio track alone over a black screen; the decoder exists, but the
   player hears a film they cannot see. Alternative: drop Fallout 4 and
   Starfield from scope altogether; simpler reporting, but their one Bink 1
   file and any future decoder are given up.
3. **Q3 — When an engine-started movie plays.** Recommended: at the point
   the original plays it (logo sequence at launch, the intro at a new
   game), with a launch flag that skips them for development and the gates.
   Alternative: only on request (`PlayBink`), never at launch; faster
   starts, but Oblivion's and Skyrim's start sequences never play.

---

## 11. Document map

| Need | Go to |
|---|---|
| The pause native pages already impose; menus | [Game UI Integration — PRD](game-ui-integration.md) |
| The new-game route the FNV intro opens | [Playable vertical slice](../engine/playable-vertical-slice.md#from-the-beginning--a-new-game-2026-11-11) |
| The project's dependency stance | [README](../../README.md#why-byroredux-exists) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-11 | First draft, at the owner's request (the FNV new-game route reaches its intro movie) | — |
