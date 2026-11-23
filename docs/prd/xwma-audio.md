# xWMA Audio Decoding — Product Requirements

<!--
Copy this file to docs/prd/xwma-audio.md (kebab-case, no "-prd" suffix), add a
row for it in docs/engine/index.md, replace every placeholder, and delete
these comments as you go. `python3 scripts/check-prd-conformance.py`
fails on any placeholder it finds (XWMA-, 2026-11-23, xwma-audio, xWMA Audio Decoding).
The rules behind every section are in README.md; the worked example is
legacy-mod-compatibility.md.
-->

**Status**: PROPOSED (2026-11-23). Revision 1 drafts the thesis (§1), the
core model with its inventory at HEAD (§3) and the decisions (§10); the other
sections are still the template. Statements about how an original engine,
format or tool behaves cite external sources; a claim that could not be
verified is marked **Phase 0: verify** instead of being stated as fact.

<!-- Add when the PRD becomes ACTIVE; the gate requires it from then on:
**Tracking**: #NNNN (parent), #NNNN (Phase 0), …
-->

**Scope**: <What the feature promises, to whom, and what is explicitly
outside it. One paragraph.>

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Music directives, layers and the `MUSC` / `MUST` stack this PRD inherits | [Music](../engine/music.md) |
| Voiced dialogue lines and their `.fuz` decode | [Dialogue Trees](../engine/dialogue-trees.md) |
| The clean-room decoder precedent (Bink 2 video and audio) | [Video Playback](../engine/video-playback.md) |

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
same codec (`xWMAEncode`, CK wiki, *How to generate voice files by batch*). A mod's `.xwm` is catalogued today as unsupported (legacy-mod
catalog A7.xwma, D5, target D1).

**Corollary: xWMA is one codec wherever it appears.** An effect, a music
track and the stream inside a `.fuz` voice line are the same WMA v2 data
(§3 measures every vanilla file), so the engine decodes them in one place,
and a file it cannot decode is reported by path, never played as silence
without a word. The engine does not encode xWMA, does not convert a mod's
files on disk, and does not take on other codecs: no shipped effect or
music file of the two games uses anything but WMA v2 (§3), and console-only
formats are out (UESP, *Skyrim:Music*: the PS4 release stores its music in
Sony's ATRAC9). Starfield is outside this PRD (**Phase 0: verify** what its
sound archives carry before a later revision says whether it joins).

---

## 2. Terminology

<!-- Every term the requirements lean on, defined once, in a table. Link
to a related design's terminology instead of redefining its terms. -->

| Term | Meaning |
|---|---|
| **<Term>** | <Meaning> |

---

## 3. Core model

An **xWMA file** is a RIFF file whose form type is `XWMA`, holding a `fmt `
chunk, a seek table and the encoded data (**Phase 0: verify** the chunk set
and the seek table's meaning against Microsoft's XAudio2 documentation of
the format; the shipped files read as RIFF `XWMA` with a `fmt ` chunk, as
measured below). Its codec is named by the `fmt ` chunk's format tag.

Every xWMA the engine meets takes one of three **routes**, by what it is
for, and each route reports a file it cannot play:

| Route | Meaning | Reported as |
|---|---|---|
| **Static** | A short effect, decoded whole into memory when it is first played and cached like any other effect | the path, once, in the audio log and the asset-missing report |
| **Streamed** | A music track, decoded while it plays, minutes long, never held whole | the directive's track named Unresolved, as music reports any file it cannot play today |
| **Voice** | The stream inside a `.fuz` line, decoded with the line's lip data beside it | the line's path, once, as voice reports a decode failure today |

The **music stack** is the part of the game-music model that waits on this
decoder: a Skyrim or Fallout 4 music type's priority and ducking (`MUSC`)
and its tracks' palette, single and silent kinds (`MUST`). The records are
read today (`MuscRecord`, `MustRecord`,
`crates/plugin/src/esm/records/misc/music.rs`); what the controller does
with them is this PRD's to specify.

The inventory below is the state at HEAD on 2026-11-23. Counts were read
from the vanilla archives' own entries; a coverage test that decodes every
xWMA of the installed archives replaces this table when Phase 1 lands.

| # | Where xWMA appears | Route | Vanilla files | Today |
|---|---|---|---|---|
| 1 | Fallout 4 effects (`sound\**\*.xwm`) | Static | 6,463 of the game's 8,410 sounds (the rest `.wav`) | silent: the effect decoder is symphonia built for wav, ogg, mp3 and flac (`crates/audio/src/cache.rs`, `load_sound_from_bytes`) |
| 2 | Fallout 4 music | Streamed | 459 | refused as Unresolved (`byroredux/src/music/translate.rs`, MUS-NG-001) |
| 3 | Skyrim music | Streamed | 269 | refused as Unresolved, as row 2 |
| 4 | Skyrim voices (`.fuz`) | Voice | 75,408 | plays through the host's `ffmpeg` (`crates/audio/src/fuz.rs`, #5912); silent with a warning where no `ffmpeg` is installed |
| 5 | Fallout 4 voices (`.fuz`) | Voice | 116,101 | as row 4 |
| 6 | A mod's `.xwm` on Skyrim or Fallout 4 | Static or Streamed | — | silent, as rows 1 to 3 (catalog A7.xwma) |
| 7 | Skyrim effects | — | none (5,929 `.wav`) | plays |

The codec, read from every xWMA's `fmt ` chunk in `Fallout4 - Sounds.ba2`
and `Skyrim - Sounds.bsa` on 2026-11-23:

| Game | Files | Format tag | Channels | Sample rates |
|---|---|---|---|---|
| Fallout 4 effects | 6,463 | `0x0161` (WMA v2) | 5,497 mono, 966 stereo | 44,100 (5,625), 48,000 (828), 32,000 (10) |
| Fallout 4 music | 459 | `0x0161` | stereo | 48,000 (458), 44,100 (1) |
| Skyrim music | 269 | `0x0161` | stereo | 44,100 (135), 32,000 (133), 48,000 (1) |

No shipped file carries another format tag, so one WMA v2 decoder covers
every vanilla file. The voice archives' `.fuz` streams were not read here
(**Phase 0: verify** their format tags the same way).

---

## 4. Goals and non-goals

### 4.1 Goals

<!-- Each goal is one measurable outcome, phrased so §8 can count it. -->

- **XWMA-G-001 — <Name>.** <Outcome.>

### 4.2 Non-goals

<!-- What is deliberately out, with the reason or the design that already
decided it. A non-goal that is merely "later" belongs in §9 as a phase. -->

- **XWMA-NG-001.** <Exclusion and why.>

---

## 5. Requirements by area

<!--
One H3 per area, each with its own ID segment (`XWMA-AREA-NNN`). One
observable per ID: a command, a value, a report line, a number. State at
HEAD goes in the sentence ("Today the resolver tie-breaks by UUID; it MUST
…"). Unverified behaviour of an original engine is marked **Phase 0:
verify**. Never reuse a number once the PRD has been reviewed; retire an ID
with "(withdrawn, rev N)" and keep the slot.
-->

### 5.1 <Area> (`XWMA-AREA`)

- **XWMA-AREA-001.** <Subject> MUST <observable>. Today: <state at HEAD>.
- **XWMA-AREA-002.** <Subject> SHOULD <observable>.

---

## 6. Deliberate deviations and risks

<!--
Every place the engine knowingly departs from original-engine behaviour,
with what that makes redundant and the risk the departure creates for
users or content, pointing at the requirement that handles the risk. This
is the register a reviewer reads to find silent breakage.
-->

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | <legacy limit, cited> | <ours, requirement id> | <tool / mod class> | <risk → XWMA-AREA-NNN, or "None" with why> |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

<!-- All seven titles, each in / out / not applicable with a reason. Follow
the engine tiers in docs/engine/game-compatibility.md. If delivery is in
waves, say which titles form each wave and why. -->

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | <yes / no / n/a> | |
| Fallout 3 | | |
| Fallout: New Vegas | | |
| Skyrim (LE / SE / AE) | | |
| Fallout 4 | | |
| Fallout 76 | | |
| Starfield | | |

### 7.2 Acceptance

<!-- Scenarios a tool can run: `cargo test` names, `--bench-hold` +
`byro-dbg` scripts under docs/smoke-tests/, or a corpus with a pass
definition. Name the data each scenario needs; mod or game files are never
checked in. -->

A scenario passes when:

1. <Observable condition.>
2. <Observable condition.>

---

## 8. Metrics

<!-- Definitions and granularity only. The numbers live in ROADMAP.md (one
home); say which table there. -->

| Metric | Definition | Granularity |
|---|---|---|
| <name> | <numerator ÷ denominator> | <per game / per phase / …> |

---

## 9. Delivery sequence

<!--
Phases ordered by what they unblock. Phase 0 is survey, catalog and
decisions, with no engine code. Every phase ends with "Exit:" followed by a
measurement, not a feature list; the gate requires the word. Name the
milestones each phase depends on.
-->

**Phase 0 — <Name> (no engine code).** <What is surveyed, cataloged,
verified; which §10 decisions it needs resolved; which **Phase 0: verify**
items it closes.> Exit: <measurable condition>.

**Phase 1 — <Name>.** <What ships; the requirement IDs it satisfies.>
Exit: <measurable condition>.

---

## 10. Decisions required

1. **Q1 — How xWMA is decoded.** Recommended: a clean-room WMA v2 decoder
   in a crate of its own, built the way the Bink 2 video and Bink Audio
   decoders were (public descriptions of the format plus black-box probing
   of a reference decoder, with the provenance kept in a `CLEANROOM.md`), so
   the games play in process with no host tool and no linked codec library.
   Its cost is a research effort before anything plays, and whether it is
   possible depends on two things to settle first: **Phase 0: verify** that
   a public description of the WMA v2 bitstream exists that is sufficient
   to write a decoder from (the format is proprietary), and which reference
   decoder can serve as the black-box oracle. Alternative A: the host's
   `ffmpeg`, as voices use it today (#5912): effects could play soon, but
   every player needs `ffmpeg` installed, each decode starts a process and
   writes a temporary file, and streaming music through it is a design of
   its own. Alternative B: link an existing WMA decoder library: in-process
   and fast to build, at the cost of that library's licence and a native
   build dependency on every platform.
2. **Q2 — Whether voices move to the same decoder.** Recommended: yes,
   once it decodes every vanilla voice stream, so `.fuz` lines stop
   depending on `ffmpeg` and the three routes share one codec. Alternative:
   keep `ffmpeg` for voices, which leaves two decoders of one codec and
   keeps the dependency for every Skyrim and Fallout 4 player.
3. **Q3 — Which route comes first.** Recommended: effects (Static), since
   they are the larger silence (6,463 Fallout 4 files) and need no
   streaming, then music with its stack. Alternative: music first, the more
   audible gap on Skyrim, where effects already play; streaming and the
   stack then come before any effect plays on Fallout 4.
4. **Q4 — When a file is decoded.** Recommended: in the engine, when a
   sound first plays (effects) or while it plays (music), so a mod's file
   works as shipped with no extra step. Alternative: convert every `.xwm`
   once to a decoded format on disk, as an asset-preprocessing step: the
   runtime then needs no xWMA decoder at all, at the cost of an install-time
   pass, the disk space of decoded audio, and a step a mod's new files must
   go through.

---

## 11. Document map

| Need | Go to |
|---|---|
| How music is chosen and layered | [Music](../engine/music.md) |
| What the game-music PRD left to this one (MUS-NG-001) | [Game Music PRD](game-music.md) |
| The mod-facing disposition of `.xwm` (catalog A7.xwma) | [Legacy Mod Compatibility PRD](legacy-mod-compatibility.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

<!-- One row per revision and per state change. The review cell is `—` or
`<who>: revise | approved | withdrawn`; only the owner's `approved` moves
the status. The last row's date and the status line's date are the same
date. -->

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-23 | First draft | — |
