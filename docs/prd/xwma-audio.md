# xWMA Audio Decoding — Product Requirements

**Status**: ACTIVE (2026-11-25). Revision 13 records finite loop regions and combat finales.
Revision 12 records timed silence and layered palettes.
Revision 11 records ducking and authored playlist selection.
Revision 10 records live exterior and combat music on Skyrim SE and Fallout 4.
Revision 9 records the owner's decision on the Fallout 4 voice lines over
the static PCM budget (the 15 `npcfmagnolia` songs): the static limit stays, they belong to the Streamed
route, and scenario 5 is amended to say so (XWMA-VOX-003). Revision 8 records the build-time FFmpeg fetch and the 627
short entries measured in Phase 1 slices A to C. Revision 7 files the tracking issues.
Revision 6 records the owner's approval. Revision 5 records the owner's Q2 to Q4 decisions
(§10): voices move to the same decoder, effects ship before voices before music, and files
decode in the engine at play time. Revision 4 records the owner's Q1 decision (§10):
Alternative B, FFmpeg's `wmav2` decoder fetched at build time and linked statically. It
carries that through the requirements, deviations, acceptance and phases. Revision 3 records
the Phase 0 evidence: V1 and V3 to V6 are sourced in the
[findings](../engine/xwma-phase0-findings.md) and the decoder's
[design](../engine/xwma-decoding.md) is drafted. V2 is a negative result, which is a Q1
trigger (§10). Revision 2 folded in review round 1: it wrote the
terminology, goals, requirements, deviations, per-game scope, acceptance, metrics and
phases (§2, §4 to §9), measured the voice archives' format tags (§3), cited the
format-tag and music-type claims, moved Q1's two research conditions into Phase 0 and
recorded what the shipped decisions already settle (§10). Statements about how an original
engine, format or tool behaves cite external sources; a claim that could not be verified
is marked **Phase 0: verify** instead of being stated as fact.

**Tracking**: #7411 (parent). Phases: #7413 (1), #7414 (2), #7415 (3), #7416 (4). Phase 0 residuals: #7412.

**Scope**: the engine decodes every xWMA (`.xwm`) stream that Skyrim and Fallout 4 ship,
and a mod's, in process: the Fallout 4 effects (Static route), the Skyrim and Fallout 4
music (Streamed route) with the `MUSC` / `MUST` priority and ducking stack that only
matters once those tracks decode, and the `.fuz` voice
streams that decode through the host's `ffmpeg` today (Voice route, Q2). A file the engine
cannot decode is reported by path. Outside the promise: encoding xWMA, converting a mod's
files on disk, other codecs and console-only formats, Oblivion, Fallout 3 and New Vegas
(none of their installed archives holds an xWMA, §7.1), and Fallout 76 and Starfield
(game-music MUS-NG-005).

**Related**: this document states the *promise*; the designs below own the *how*. Every
mechanism promised here has an owner in this table, or Phase 0 writes the missing design.

| Mechanism | Owning design |
|---|---|
| Music directives, layers, outcomes and the streaming voices the controller drives | [Music](../engine/music.md) |
| The Skyrim and Fallout 4 music stack this PRD inherits from the music PRD's non-goal | [Game Music PRD](game-music.md) (MUS-NG-001) |
| Voiced dialogue lines and their `.fuz` container | [Dialogue Trees](../engine/dialogue-trees.md) |
| The in-process codec precedents (Bink 1, Bink 2 video and Bink Audio) | [Video Playback](../engine/video-playback.md) |
| The xWMA decoder boundary, its provenance rules, its oracle and the streaming bound | [xWMA Decoding](../engine/xwma-decoding.md) (PROPOSED; evidence in the [Phase 0 findings](../engine/xwma-phase0-findings.md)) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `XWMA-` prefix.

---

## 1. Product thesis

Skyrim and Fallout 4 store their music, and most of Fallout 4's sound
effects, as xWMA: "Microsoft's xWMA, which is an extension of normal WMA v2
audio" (Fallout CK wiki, *Sound File*). The engine has no xWMA decoder for
either, so on those two games the music never plays and three of every four
Fallout 4 effects are silent (§3). The game-music PRD stopped at that line:
"The owner decided (2026-11-13) that the xWMA decoder gets its own PRD; that
PRD owns the stack" (game-music MUS-NG-001). This is that PRD. It owns the
decoder, and with it the Skyrim and Fallout 4 music stack, because the
`MUSC` priority and ducking and the `MUST` palettes only matter once their
tracks decode.

It is for players of the two games, whose worlds are silent where the
originals have music and sound, and for mod authors, whom the Creation Kit
steers to `.xwm` for music ("These issues can be avoided by using the .xwm
file format", CK wiki, *Music Track*) and who encode voice lines with the
same codec (`xWMAEncode`, CK wiki, *How to generate voice files by batch*).
A mod's `.xwm` is catalogued today as unsupported (legacy-mod
catalog A7.xwma, D5, target D1).

**Corollary: xWMA is one codec wherever it appears.** An effect, a music
track and the stream inside a `.fuz` voice line are the same WMA v2 data
(§3 measures every vanilla file), so the engine decodes them in one place,
and a file it cannot decode is reported by path, never played as silence
without a word. The engine does not encode xWMA, does not convert a mod's
files on disk, and does not take on other codecs: no shipped effect, music or
voice file of the two games uses anything but WMA v2 (§3), and console-only
formats are out (UESP, *Skyrim:Music*: "PS4 uses Sony Atrac format (.at9) at
144kbps to store music"). Starfield and Fallout 76 are outside this PRD
(game-music MUS-NG-005).

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **xWMA file** | A RIFF file whose form type is `XWMA`, holding a `fmt ` chunk, a seek table and the encoded data ([Microsoft XAudio2 file loading](https://learn.microsoft.com/en-us/windows/win32/xaudio2/how-to--load-audio-data-files-in-xaudio2), [V1](../engine/xwma-phase0-findings.md#v1--riff-xwma-format-tag-and-packet-table)). |
| **Format tag** | The first field of the `fmt ` chunk, naming the codec. Every vanilla file carries `0x0161`, which Windows' `mmreg.h` names `WAVE_FORMAT_WMAUDIO2` ([Microsoft Audio Subtype GUIDs](https://learn.microsoft.com/en-us/windows/win32/medfound/audio-subtype-guids), [V1](../engine/xwma-phase0-findings.md#v1--riff-xwma-format-tag-and-packet-table)). |
| **Seek table** | The `dpds` chunk of a shipped file: the decoded-packet table `crates/audio/src/fuz.rs` reads. Its meaning in the format is cumulative decoded bytes after each packet ([Microsoft XAUDIO2_BUFFER_WMA](https://learn.microsoft.com/en-us/windows/win32/api/xaudio2/ns-xaudio2-xaudio2_buffer_wma), [V1](../engine/xwma-phase0-findings.md#v1--riff-xwma-format-tag-and-packet-table)). |
| **Route** | How an xWMA is used: Static, Streamed or Voice, defined in §3. |
| **Music stack** | The part of the Skyrim and Fallout 4 music model that waits on the decoder: a music type's priority and ducking (`MUSC`) and its tracks' palette, single and silent kinds (`MUST`). |
| **Oracle** | The `ffmpeg` command-line tool of the same pinned FFmpeg release the engine links, run outside the engine on the same file. It is a test tool, never a dependency. Because it is the same codec, agreement pins the integration (packetising, sample conversion), not the codec; the codec itself is adjudicated against Microsoft's decoder where V3 finds a disagreement. |
| **Pinned FFmpeg** | One FFmpeg release, with its version, tarball URL and SHA-256 in [`crates/xwma/build.rs`](../../crates/xwma/build.rs) and [`third_party/ffmpeg/PATCHES.md`](../../third_party/ffmpeg/PATCHES.md). The build fetches and verifies the tarball, builds the `wmav2` decoder and its support code with every other component disabled, and links the resulting libraries statically. It deletes the extracted source and object files after the build (Q1). |
| **Unresolved** | The music outcome for a source the build does not play, with its reason ([Music](../engine/music.md) §2). |

---

## 3. Core model

Every xWMA the engine meets takes one of three **routes**, by what it is
for, and each route reports a file it cannot play:

| Route | Meaning | Reported as |
|---|---|---|
| **Static** | A short effect, decoded whole into memory when it is first played and cached like any other effect | the path and the reason, once, in the audio log and the sound failure listing (XWMA-REP-001, XWMA-REP-002) |
| **Streamed** | A music track, decoded while it plays, minutes long, never held whole | the directive's track named Unresolved, as music reports any file it cannot play today (XWMA-REP-003) |
| **Voice** | The stream inside a `.fuz` line, decoded with the line's lip data beside it | the line's path, once, as voice reports a decode failure today |

The **music stack** is read today: `MuscRecord` carries `flags`, `priority`,
`ducking_db`, `fade_secs` and `tracks`, and `MustRecord` carries `kind`,
`duration`, `fade_out`, `file`, `finale_file`, `loop_data` and `sub_tracks`
(`crates/plugin/src/esm/records/misc/music.rs`, parsed for Skyrim and Fallout 4
alike, #6790). What the controller does with them is §5.5.

The inventory below is the state at HEAD on 2026-11-24. Every count was
re-read from the vanilla archives' own entries on that date; a coverage test
that decodes every xWMA of the installed archives replaces this table when
Phase 1 lands (XWMA-DEC-001).

| # | Where xWMA appears | Route | Vanilla files | Today |
|---|---|---|---|---|
| 1 | Fallout 4 effects (`sound\fx\**\*.xwm`) | Static | 6,463 of the game's 8,410 sounds (the rest `.wav`) | silent: the effect decoder is kira's, built for wav, ogg, mp3 and flac (`crates/audio/src/cache.rs`, `load_sound_from_bytes`) |
| 2 | Fallout 4 music (`music\**\*.xwm`) | Streamed | 459 | refused as Unresolved (`byroredux/src/music/translate.rs:333`, MUS-NG-001) |
| 3 | Skyrim music | Streamed | 269 | refused as Unresolved, as row 2 |
| 4 | Skyrim voices (`.fuz`, `Skyrim - Voices_en0.bsa`) | Voice | 75,408 | decodes in process through the shared static loader (#7415, committed `cd7988507`); the host-`ffmpeg` route is deleted |
| 5 | Fallout 4 voices (`.fuz`, `Fallout4 - Voices.ba2`) | Voice | 116,101 | as row 4, except 15 `npcfmagnolia` lines (the Magnolia songs and their `_muffledreverb` and `_normalreverb` variants) whose decoded PCM exceeds the static budget and are refused with "exceeds the static PCM limit" (measured 2026-11-24, Phase 3 slice C) |
| 6 | A mod's `.xwm` on Skyrim or Fallout 4 | Static or Streamed | — | silent, as rows 1 to 3 (catalog A7.xwma) |
| 7 | Skyrim effects | — | none (5,929 `.wav`) | plays |

The codec, read from every xWMA's `fmt ` chunk (the `.fuz` rows from the embedded
stream after the lip data) on 2026-11-24:

| Archive | Files | Format tag | Channels and sample rates |
|---|---|---|---|
| `Fallout4 - Sounds.ba2`, effects | 6,463 | `0x0161` | 5,497 mono at 44,100; 966 stereo: 128 at 44,100, 828 at 48,000, 10 at 32,000 |
| `Fallout4 - Sounds.ba2`, music | 459 | `0x0161` | stereo: 458 at 48,000, 1 at 44,100 |
| `Skyrim - Sounds.bsa`, music | 269 | `0x0161` | stereo: 135 at 44,100, 133 at 32,000, 1 at 48,000 |
| `Skyrim - Voices_en0.bsa` | 75,408 | `0x0161` | 74,869 mono and 539 stereo, all at 44,100 |
| `Fallout4 - Voices.ba2` | 116,101 | `0x0161` | 115,709 mono at 44,100; 392 stereo at 48,000 |

No shipped file carries another format tag, so one WMA v2 decoder covers
every vanilla file; the Skyrim voices' other language archives were not read. The
installed Oblivion, Fallout 3 and New Vegas BSAs (54 archives) hold no `.xwm` and no
`.fuz` (counted 2026-11-24).

---

## 4. Goals and non-goals

### 4.1 Goals

- **XWMA-G-001 — Fallout 4 effects play.** All 6,463 vanilla Fallout 4 xWMA effects
  decode, and a weapon, footstep or impact that names one plays it (Static route).
- **XWMA-G-002 — Skyrim and Fallout 4 music plays.** The 269 and 459 vanilla tracks
  decode and stream, and the `MUSC` / `MUST` stack chooses, layers and ducks them
  (Streamed route).
- **XWMA-G-003 — Voices need no host tool.** A Skyrim or Fallout 4 `.fuz` line decodes
  with no `ffmpeg` on the host (Voice route, Q2).
- **XWMA-G-004 — A mod's `.xwm` plays as the original's does.** A loose or archived
  `.xwm` takes the route its use names, with no conversion step (catalog A7.xwma, D5 to D1).
- **XWMA-G-005 — A failure is named.** Every file the engine cannot decode appears in the
  log and in a listing by path and reason.

### 4.2 Non-goals

- **XWMA-NG-001.** Encoding xWMA. No game or tool of the project writes one.
- **XWMA-NG-002.** Converting a mod's `.xwm` to another format on disk. Q4 records the
  alternative that would: [Asset Preprocessing PRD](asset-preprocessing.md), not this one.
- **XWMA-NG-003.** Other codecs. No shipped xWMA is anything but WMA v2 (§3), and the PS4
  release stores its music as Sony Atrac (`.at9`; UESP, *Skyrim:Music*), a console format.
- **XWMA-NG-004.** Starfield and Fallout 76: game-music MUS-NG-005 decided that their
  sound archives are Wwise banks and that no playable flow reaches Fallout 76.
- **XWMA-NG-005.** Oblivion, Fallout 3 and New Vegas music and sound: no xWMA exists in
  their archives (§3), and game-music MUS-NG-006 keeps their MP3 and WAV decoders.

---

## 5. Requirements by area

### 5.1 The decoder (`XWMA-DEC`)

- **XWMA-DEC-001.** The decoder MUST decode every xWMA entry of `Skyrim - Sounds.bsa`,
  `Fallout4 - Sounds.ba2`, `Skyrim - Voices_en0.bsa` and `Fallout4 - Voices.ba2` to its last
  frame with no error. Today: 198,073 of 198,700 entries reach the declared end. The 627
  short entries remain open until adjudicated against Microsoft's decoder
  ([Phase 1 results](../engine/xwma-phase1-results.md)).
- **XWMA-DEC-002.** The decoder's samples MUST equal the oracle's decode of the same file,
  for every vanilla file, to within one signed 16-bit PCM count per sample, with equal channel
  count, sample rate and frame count (the difference V3 measured between two execution paths
  of one FFmpeg build). The 627 of 198,700 entries whose FFmpeg frame count falls short of
  `dpds` (82 in `Fallout4 - Sounds.ba2`, 389 in `Skyrim - Voices_en0.bsa`, 156 in
  `Fallout4 - Voices.ba2`) MUST be adjudicated against Microsoft's decoder before Phase 1
  exits. Today the wrapper and the V3 CLI invocation agree within tolerance across the
  corpus. The [Phase 1 results](../engine/xwma-phase1-results.md) record the CLI version
  and the pending Windows measurement.
- **XWMA-DEC-003.** For a file it decodes, the decoder MUST report channel count, sample
  rate and frame count, and the frames it produces MUST equal the frame count it reported.
- **XWMA-DEC-004.** A malformed or truncated file, or a `fmt ` format tag other than
  `0x0161`, MUST return an error carrying the tag where it is the cause, without a panic,
  an unbounded allocation or an unbounded wait. Today `crates/audio/src/fuz.rs` refuses a
  voice stream whose tag is not `0x0161` ("refusing to guess").
- **XWMA-DEC-005.** The decoder MUST be FFmpeg's `wmav2` decoder from one pinned release,
  fetched as a pinned tarball at build time and linked statically with every other FFmpeg
  component disabled. The decoder MUST build and run without an FFmpeg installation (Q1).
  The build needs network access for the initial fetch on an empty cache or a local copy
  supplied through `XWMA_FFMPEG_TARBALL`. It uses `curl` for the download, a `tar` that
  reads `.xz`, a C compiler, GNU Make and a POSIX shell. Today the voice route still starts
  the host's `ffmpeg` binary through `crates/audio/src/fuz.rs` until Phase 3 (#7415).
- **XWMA-DEC-006.** The repository MUST carry the pinned release's identifier, its source
  digest, the compiled files' licence notice and the configure line that built it.
  [`third_party/ffmpeg/PATCHES.md`](../../third_party/ffmpeg/PATCHES.md) records the release,
  digest and licence notice with the [`licence-files.tsv`](../../third_party/ffmpeg/licence-files.tsv)
  inventory. [`crates/xwma/build.rs`](../../crates/xwma/build.rs) defines the configure line.
  The build MUST disable assembly so the decoded samples do not vary with the CPU.
  Phase 1 verified CPU-flag invariance. V3 measured one count of difference between
  CPU-optimised and scalar runs of one build.

### 5.2 Static route: effects (`XWMA-STAT`)

- **XWMA-STAT-001.** A Fallout 4 `.xwm` effect named by a sound record or a default-sound
  lookup MUST load into a `StaticSoundData` and play, as a `.wav` one does. Today the
  load fails: kira, built for wav, ogg, mp3 and flac, has no WMA codec
  (`crates/audio/src/cache.rs`).
- **XWMA-STAT-002.** An effect that decodes MUST be cached by its path, so a second play
  of the same effect decodes nothing, as an effect in another format is today.

### 5.3 Streamed route: music (`XWMA-STRM`)

- **XWMA-STRM-001.** A music `.xwm` MUST play through the streaming decoder every music
  stream plays through (`crates/audio/src/stream_decoder.rs`, #6945), not through a second
  playback path.
- **XWMA-STRM-002.** A streamed track MUST NOT hold more decoded audio than the bound the
  design Phase 0 writes records; a test MUST assert it for the longest vanilla track.
- **XWMA-STRM-003.** A stream whose data ends before the frame count its header declares
  MUST end the voice, not spin: kira's own decoder spins at 100% on that case
  (`stream_decoder.rs`, #6945).

### 5.4 Voice route: `.fuz` (`XWMA-VOX`)

- **XWMA-VOX-001.** A `.fuz` line MUST decode with the
  in-process decoder (Q2) and MUST NOT start `ffmpeg`. Today it decodes in process
  (`crates/audio/src/fuz.rs`, #7415) and the host-`ffmpeg` route is deleted.
- **XWMA-VOX-002.** A `.fuz` line that fails to decode MUST fall back to the subtitle
  estimate, as it does today without `ffmpeg`, and MUST report its path once.
- **XWMA-VOX-003.** A `.fuz` line whose decoded PCM exceeds the static budget (64 MiB) MUST be refused
  with the "exceeds the static PCM limit" reason, fall back to the subtitle estimate and be listed by
  `snd.failed`; such a line belongs to the Streamed route (Phase 4). Owner decision, 2026-11-24: the
  limit is not raised for voices. Today 15 vanilla Fallout 4 lines are refused: the `npcfmagnolia`
  songs and their `_muffledreverb` and `_normalreverb` variants.

### 5.5 Music stack: `MUSC` and `MUST` (`XWMA-MUSC`)

The behaviours below are the Creation Kit's own (CK wiki, *Music Type* and *Music
Track*); the Fallout 4 pages list the same fields (Fallout CK wiki, *Music Type*, *Music
Track*), and whether the behaviour is the same is **Phase 0: verify** in the
original runtime ([V4 evidence on the shared fields](../engine/xwma-phase0-findings.md#v4--palette-layout-loop-data-and-fallout-4-behavior)).

- **XWMA-MUSC-001.** On Skyrim and Fallout 4 the Location layer MUST translate the
  sources Phase 0 settles (V5) to a directive that plays. The translate step now resolves
  the authored sources for single-file tracks. The provisional source order is documented
  in [Music](../engine/music.md#3-the-location-layer-per-game) (§1, R5).
- **XWMA-MUSC-002.** When two music types are requested, the one with the higher `PNAM`
  priority MUST be the audible one (lower numeric value, [Phase 0 V4](../engine/xwma-phase0-findings.md#v4--palette-layout-loop-data-and-fallout-4-behavior)).
  The controller compares active authored types, and the live gate exercises the combat transition.
- **XWMA-MUSC-003.** A type flagged Ducks Current Track and playing over a lower-priority
  one MUST attenuate that one by its `ducking_db` over its `fade_secs` (CK wiki, *Music
  Type*, Ducking and Fade Duration). The [live gate](../smoke-tests/xwma-music.sh) checks that the
  exterior playhead advances during combat and that gain returns on the same track.
- **XWMA-MUSC-004.** A type MUST honour Plays One Selection, Cycle Tracks and Maintain
  Track Order as the CK describes them (CK wiki, *Music Type*), through the controller's
  playlist model. The [controller tests](../../byroredux/src/music/controller.rs) exercise ordered,
  shuffled, single-selection and cycling playlists.
- **XWMA-MUSC-005.** A type flagged Abrupt Transition MUST start at once, without waiting
  to fade out the current music (CK wiki, *Music Type*).
- **XWMA-MUSC-006.** A `MUST` of kind single MUST play its `file`; of kind silent, play
  nothing for its `duration`; of kind palette, play its `sub_tracks` together for its
  `duration`, then fade out over its `fade_out` (CK wiki, *Music Track*). How a palette's
  layers are encoded is documented by [UESP MUST](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/MUST)
  and the [V4 vanilla record checks](../engine/xwma-phase0-findings.md#v4--palette-layout-loop-data-and-fallout-4-behavior)
  as zero FormID separators within `SNAM`. The controller tests check the silence timer, parallel
  layers, and the palette fade. The real-data test resolves three layers from each game's master.
- **XWMA-MUSC-007.** A `MUST` carrying loop data MUST loop between its begin and end for
  its count (CK wiki, *Music Track*, Contains Loop). The unit of begin and end and the
  meaning of a zero count are covered in the
  [V4 loop evidence](../engine/xwma-phase0-findings.md#v4--palette-layout-loop-data-and-fallout-4-behavior).
  The editor fields specify seconds. A zero count with a present loop range is
  **Phase 0: verify** in the original runtime. ByroRedux currently treats zero as no extra
  passes. The [stream tests](../../crates/audio/tests/support/xwma_stream.rs) compare real source
  samples at the wrap and tail.
- **XWMA-MUSC-008.** A combat track that ends while its finale (`finale_file`) exists
  SHOULD crossfade into the finale when combat ends first ("crossfading from one of the
  Cue Points directly into the Finale track", CK wiki, *Music Track*). The controller
  waits for the next cue, and the [live gate](../smoke-tests/xwma-music.sh) checks the finale
  when the selected authored track names one.
- **XWMA-MUSC-009.** A `MUST` whose conditions are false SHOULD be skipped (CK wiki,
  *Music Track*, Conditions). Today `music.rs` reads no conditions, so every track plays.

### 5.6 A mod's file (`XWMA-MOD`)

- **XWMA-MOD-001.** A loose or archived `.xwm` that a mod adds or overrides MUST take the
  same route and the same decoder as a vanilla one. Today the loose-override lookup finds
  it (`byroredux/src/asset_provider/tests/loose_override.rs`) and nothing decodes it.

### 5.7 Reporting (`XWMA-REP`)

- **XWMA-REP-001.** A file that fails to decode MUST log once per session with its path
  and the reason.
- **XWMA-REP-002.** A console command in the family of `tex.missing`
  (`byroredux/src/commands/assets.rs`) MUST list every sound that failed to decode, with
  its path and reason. Today no sound listing exists; `tex.missing` covers textures only.
- **XWMA-REP-003.** A music directive whose track fails to decode MUST report Unresolved
  with the reason in `music.status` (MUS-RPT-001) and MUST NOT report Playing.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | The games play xWMA through the platform's codec ([V1](../engine/xwma-phase0-findings.md#v1--riff-xwma-format-tag-and-packet-table) verifies platform support. The games' use of the API is **Phase 0: verify**). | FFmpeg's `wmav2` decoder, fetched at build time and linked statically (XWMA-DEC-005). | A host codec install; the `ffmpeg` binary for voices (Q2). | The decoder's samples may differ from the original's. XWMA-DEC-002 requires Microsoft decoder adjudication for the 627 of 198,700 entries short against `dpds` (82 in `Fallout4 - Sounds.ba2`, 389 in `Skyrim - Voices_en0.bsa`, 156 in `Fallout4 - Voices.ba2`). The decoder is C code reading untrusted mod files: XWMA-DEC-004 and scenario 6. The build needs a C toolchain and the engine carries an LGPL notice: XWMA-DEC-006, V7. |
| 2 | The CK describes music stack behaviour; the original's exact timing is not documented ([V4 comparison of the CK pages and record definitions](../engine/xwma-phase0-findings.md#v4--palette-layout-loop-data-and-fallout-4-behavior)). | The controller's playlist and crossfade model plays it (XWMA-MUSC-002 to -008). | None. | Music that chooses differently from the original: V4 and V5 settle the source order and the flags before Phase 4. |
| 3 | A `.fuz` line decodes through whatever the host provides. | In-process (XWMA-VOX-001), `ffmpeg` no longer started. | The `ffmpeg` dependency for every Skyrim and Fallout 4 player. | A voice that played through `ffmpeg` now fails: scenario 5 compares every vanilla line, XWMA-DEC-001, XWMA-VOX-002. |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | n/a | no `.xwm` or `.fuz` in its installed archives (54 BSAs across it, Fallout 3 and New Vegas, counted 2026-11-24); its music is MP3 and WAV (game-music MUS-NG-006). |
| Fallout 3 | n/a | as Oblivion. |
| Fallout: New Vegas | n/a | as Oblivion. |
| Skyrim (LE / SE / AE) | yes | Streamed music and the stack (row 3 of §3), Voice (row 4). Its effects are `.wav` and play. |
| Fallout 4 | yes | Static effects, Streamed music and the stack, Voice (rows 1, 2 and 5). |
| Fallout 76 | no | game-music MUS-NG-005: no playable flow reaches it. |
| Starfield | no | game-music MUS-NG-005: its sound archives are Wwise banks. Whether its other archives carry any `.xwm` is answered by the [V6 inventory](../engine/xwma-phase0-findings.md#v6--starfield-archive-inventory), which found zero `.xwm` entries across all 129 installed BA2s. |

Delivery is by route, not by game (§9): Skyrim and Fallout 4 share the decoder, and the
per-game difference stays in the translate step.

### 7.2 Acceptance

Data: `BYROREDUX_SKYRIMSE_DATA` and `BYROREDUX_FO4_DATA`; nothing is checked in but
digests and counts. A scenario passes when:

1. The ignored coverage test decodes every xWMA entry of the four archives of XWMA-DEC-001
   to its last frame, with the frames produced equal to the frames reported
   (XWMA-DEC-003): 0 failures of 6,463 + 459 + 269 + 75,408 + 116,101.
2. A checked-in table of per-file maximum differences against the oracle shows every
   vanilla file within the tolerance of XWMA-DEC-002. No decoded audio is checked in.
3. A `--bench-hold` run in a Fallout 4 cell, driven by `byro-dbg`, plays a `.xwm`
   effect and the sound failure listing (XWMA-REP-002) is empty for it.
4. On a Skyrim exterior cell and a Fallout 4 exterior cell, `music.status` reports Playing
   with a `.xwm` file and an advancing position, and a combat request reports the
   higher-priority type as audible (XWMA-MUSC-002). Run the
   [music smoke](../smoke-tests/xwma-music.sh) for `skyrim_se` and `fo4` with an active audio device.
5. With `ffmpeg` absent from `PATH`, every vanilla `.fuz` line of both voice archives decodes
   (XWMA-VOX-001) except the 15 `npcfmagnolia` lines over the static budget, which are refused with
   the static-limit reason and listed by `snd.failed` (XWMA-VOX-003): 191,494 of 191,509 lines
   (75,408 Skyrim, 116,086 Fallout 4). The 545 lines that decode fewer frames than their `dpds`
   declaration play under the provisional short-decode policy (XWMA-DEC-001, #7413).
6. A truncated copy of a vanilla music file ends its voice and logs its path (XWMA-STRM-003,
   XWMA-REP-001), and 1,000 bit-flipped copies of vanilla files each return an error or
   output with no crash, hang or unbounded allocation (XWMA-DEC-004).
7. A loose `.xwm` placed in the data directory plays (XWMA-MOD-001).

---

## 8. Metrics

| Metric | Definition | Granularity |
|---|---|---|
| Decode coverage | xWMA entries that decode ÷ xWMA entries in the installed archives | per game and route |
| Oracle agreement | decoded files within the XWMA-DEC-002 tolerance ÷ decoded files | per game |
| Music source coverage | `MUSC` sources translated ÷ authored; playable = tracks found and decoding ÷ translated, as the game-music PRD defines them | per game |
| Voices without `ffmpeg` | `.fuz` lines that decode with no `ffmpeg` on `PATH` ÷ `.fuz` lines (a line refused over the static budget counts as not decoded) | per game |

The numbers live in ROADMAP's compatibility matrix: music source coverage in its Music
sources column, and the other three metrics in the paragraph beside the matrix that already
publishes Bink's decode rate.

---

## 9. Delivery sequence

Phase 0 closes the **Phase 0: verify** items below. Phases 1 to 4 follow Q3's decided
order: effects, then voices, then music. The audio milestone M44 and the music controller of the game-music PRD are
the only dependencies.

| # | Item | Settled by |
|---|---|---|
| V1 | The xWMA chunk set, the seek table's meaning and `0x0161` as `WAVE_FORMAT_WMAUDIO2` | Microsoft's XAudio2 and `mmreg.h` documentation |
| V2 | That a public description of the WMA v2 bitstream sufficient to write a decoder from exists | closed: negative ([V2](../engine/xwma-phase0-findings.md#v2--public-wma-v2-descriptions)); Q1 decided B |
| V3 | Which decoder serves as the oracle, and the per-sample tolerance | an installed decoder the project may run; the Bink Audio harness (`binkaudio-clean`) as the model |
| V4 | `MUST` palette layers, loop units and the Fallout 4 pages' behaviour | xEdit `wbDefinitionsTES5.pas` and `wbDefinitionsFO4.pas`, the CK and Fallout CK wikis, vanilla records |
| V5 | Which sources feed Skyrim and Fallout 4 music, in what order | UESP, the CK wikis, xEdit's record definitions |
| V6 | Whether any Starfield archive carries `.xwm` | `ba2_extensions` over its archives |
| V7 | That a pinned FFmpeg release built with every component but the `wmav2` decoder, with assembly disabled, decodes the vanilla corpus fed packets by the engine's own xWMA parser to the oracle's samples. Also covers the licence of the compiled files and the source provenance. | [V7 findings](../engine/xwma-phase0-findings.md#v7--static-wmav2-build-and-corpus-comparison) and the [build-time fetch](../../third_party/ffmpeg/PATCHES.md) |

**Phase 0 — Sources, oracle and design (no engine code).** Closes V1 to V7, writes the
decoder's design in `docs/engine/` and links it from Related, and takes Q1 to Q4 to the
owner. Exit: the count of **Phase 0: verify** items in this file is zero or each remaining
one is filed as an issue, the design exists and passes the PRD gate's link check, and every
§10 question carries **Decided**.

**Phase 1 — The decoder.** XWMA-DEC-001 to XWMA-DEC-006, the build that fetches the pinned
release, and the amended note in `crates/audio/src/fuz.rs` that said linking FFmpeg was out.
Exit: scenarios 1 and 2 pass, with decode coverage published for both games.

**Phase 2 — Effects.** XWMA-STAT-001, XWMA-STAT-002, XWMA-MOD-001, XWMA-REP-001 and
XWMA-REP-002. Exit: scenario 3 and scenario 7 pass.

**Phase 3 — Voices.** XWMA-VOX-001 to XWMA-VOX-003.
Exit: scenario 5 passes and the voice rate is published.

**Phase 4 — Music and the stack.** XWMA-STRM-001 to XWMA-STRM-003, XWMA-MUSC-001 to
XWMA-MUSC-009 and XWMA-REP-003. Exit: scenarios 4 and 6 pass and music source coverage
is published for Skyrim and Fallout 4.

---

## 10. Decisions required

1. **Q1 — How xWMA is decoded.** Recommended: a clean-room WMA v2 decoder in a crate of its
   own, built the way the Bink 2 video and Bink Audio decoders were, so the games play in
   process with no host tool and no linked codec library. It holds only if Phase 0 finds a
   public description sufficient to write a decoder from (V2) and an oracle (V3); with
   neither, the owner chooses between the two alternatives. Phase 0 result
   ([V2](../engine/xwma-phase0-findings.md#v2--public-wma-v2-descriptions)): no public
   description sufficient to write a decoder from was found, so that condition is not met
   today; V3 found a candidate oracle. The choice is the owner's. Its cost is a research
   effort before anything plays. Alternative A: the host's `ffmpeg`, as voices use it today
   (#5912): effects could play soon, but every player needs `ffmpeg` installed, each decode
   starts a process and writes a temporary file, and streaming music through it is a design
   of its own. Alternative B: link an existing WMA decoder library: in-process and fast to
   build, at the cost of that library's licence, a native build dependency on every
   platform, and reopening the repository's parse-don't-link rule (`crates/audio/src/fuz.rs`;
   ROADMAP's keep list, "Legacy compat").
   **Decided** (2026-11-24): Alternative B, narrowed to FFmpeg's `wmav2` decoder, vendored at
   a pinned release and linked statically. Two other libraries were examined and set aside
   ([V2 addendum](../engine/xwma-phase0-findings.md#v2-addendum--existing-decoders-for-alternative-b)):
   `oxideav-wma`, whose code tables and frame layout were extracted from Microsoft's decoder
   binary, and Rockbox's WMA decoder, which is GPL-2.0-or-later and cannot be linked into an
   MIT binary. This reopens the parse-don't-link stance of `crates/audio/src/fuz.rs`, which
   Phase 1 amends. If V7 finds the compiled files' licence unusable, the clean-room route
   (the recommendation above) is the fallback and Q1 returns to the owner.
   Amended (2026-11-24, owner): the source is fetched at the pinned digest during the build, compiled and deleted, not committed.
2. **Q2 — Whether voices move to the same decoder.** Recommended: yes, once it decodes
   every vanilla voice stream (XWMA-DEC-001), so `.fuz` lines stop depending on `ffmpeg` and
   the three routes share one codec. Alternative: keep `ffmpeg` for voices, which leaves two
   decoders of one codec and keeps the dependency for every Skyrim and Fallout 4 player.
   **Decided** (2026-11-24): yes. Voices move to the in-process decoder once it decodes every
   vanilla voice stream (XWMA-DEC-001).
3. **Q3 — Which route comes first.** Recommended: effects (Static), since they are the
   larger silence (6,463 Fallout 4 files) and need no streaming, then voices, then music with
   its stack. Alternative: music first, the more audible gap on Skyrim, where effects
   already play; streaming and the stack then come before any effect plays on Fallout 4.
   **Decided** (2026-11-24): effects, then voices, then music with its stack, as the
   recommendation (§9 phases 2 to 4).
4. **Q4 — When a file is decoded.** Recommended: in the engine, when a sound first plays
   (effects) or while it plays (music), so a mod's file works as shipped with no extra step.
   Alternative: convert every `.xwm` once to a decoded format on disk, as the
   [Asset Preprocessing PRD](asset-preprocessing.md) (PROPOSED, no decision taken there) would
   do: the runtime then needs no xWMA decoder at all, at the cost of an install-time pass,
   the disk space of decoded audio, and a step a mod's new files must go through.
   **Decided** (2026-11-24): in the engine, at play time.
   The conversion alternative would still need pinned FFmpeg for the conversion tool.

---

## 11. Document map

| Need | Go to |
|---|---|
| How music is chosen, layered and reported | [Music](../engine/music.md) |
| What the game-music PRD left to this one (MUS-NG-001, MUS-NG-005, MUS-NG-006) | [Game Music PRD](game-music.md) |
| The `.fuz` container and what plays a voice line | [Dialogue Trees](../engine/dialogue-trees.md) |
| The in-process codec precedents | [Video Playback](../engine/video-playback.md) |
| The decoder boundary, provenance rules, oracle and streaming bound | [xWMA Decoding](../engine/xwma-decoding.md) |
| The evidence behind V1 to V6 | [Phase 0 findings](../engine/xwma-phase0-findings.md) |
| The mod-facing disposition of `.xwm` (catalog A7.xwma) | [Legacy Mod Compatibility PRD](legacy-mod-compatibility.md) |
| The alternative that converts files at install time (Q4) | [Asset Preprocessing PRD](asset-preprocessing.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-23 | First draft | — |
| 2 | 2026-11-24 | Review round 1 folded in: §2, §4 to §9 written; voice archives' format tags measured and every count re-read; format-tag and music-type claims cited or marked verify; ATRAC wording corrected; Q1 reframed (research conditions to Phase 0, the parse-don't-link rule named) and Q4 linked to the preprocessing PRD; the sound failure listing made a requirement; template remnants removed | claude: revise |
| 3 | 2026-11-24 | Phase 0 evidence recorded: V1 and V3 to V6 sourced in the findings, V2 a negative result (Q1 trigger), decoder design drafted, Related and document map link both | — |
| 4 | 2026-11-24 | Q1 decided (owner): Alternative B, vendored static FFmpeg `wmav2`; XWMA-DEC-002 and XWMA-DEC-005 rewritten, XWMA-DEC-006 added, V7 added, deviation row 1, scenario 6, terminology and Phase 1 follow; clean-room precedent wording replaced | — |
| 5 | 2026-11-24 | Q2 to Q4 decided (owner): voices move to the in-process decoder, effects then voices then music, decode in the engine at play time; the Q2 conditionals in the scope, XWMA-G-003, XWMA-VOX-001, §9 and the design's Voice row are removed | — |
| 6 | 2026-11-24 | Owner approval: every §10 question decided, the gate passes; PROPOSED to APPROVED | owner: approved |
| 7 | 2026-11-24 | APPROVED to ACTIVE: parent #7411 and phase issues #7412 to #7416 filed; the three open Phase 0 verify markers are tracked in #7412; `**Tracking**` line added | — |
| 8 | 2026-11-24 | Fetch-at-build wording and the 627-entry count folded in after Phase 1 slices A to C; no requirement observable changed | — |
| 9 | 2026-11-24 | Voice lines over the static budget (owner, Option 1): XWMA-VOX-003 added, scenario 5 amended to 191,494 of 191,509 lines, §3 voice rows and XWMA-VOX-001's Today brought to HEAD (#7415 slices A to C), Phase 3 and the voice metric reworded; no existing requirement's observable changed | — |
| 10 | 2026-11-25 | Translated authored location and battle types to single-file playlists, applied PNAM priority, and passed the live Skyrim SE and Fallout 4 exterior and combat gates (#7416). | — |
| 11 | 2026-11-25 | Applied combat ducking and gain recovery to the running exterior track and wired the three authored playlist flags (#7416). | — |
| 12 | 2026-11-25 | Preserved palette layer separators and played timed silence and layered palettes through the authored playlist (#7416). | — |
| 13 | 2026-11-25 | Streamed finite authored loop regions and scheduled combat finales on the active track's cue timeline (#7416). | — |
