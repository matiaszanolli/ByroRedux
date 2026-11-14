# Clean-room decoders

How ByroRedux's own Bink 2 and Bink Audio decoders were written, and what that process does and
does not cover. The README carries a summary; the rules each crate enforces are in its own
`CLEANROOM.md`.

Bink is RAD Game Tools' proprietary video codec. Fallout 4, Fallout 76 and
Starfield ship their movies as Bink 2, and when this work started no decoder
for it existed under a permissive licence (FFmpeg has none; NihAV's is
AGPL). So ByroRedux wrote its own: [`crates/bink2`](../crates/bink2/README.md)
(Bink 2 video, `KB2g` / `KB2j`, and the container) and
[`crates/binkaudio`](../crates/binkaudio/README.md) (Bink Audio, the
soundtracks). Both are pure Rust, MIT, with no dependencies, and both were
written so that every fact in them can be traced to where it came from.

**Allowed sources**, and nothing else:

1. Public documentation: the MultimediaWiki pages *Bink Video 2*, *Bink
   Audio*, *Bink Container* and *RAD Game Tools Bink API*, and RAD's
   user-facing pages — snapshotted on the day they were read.
2. The black-box behaviour of the owner's own copy of a game's
   `bink2w64.dll`: called through its exported functions with inputs we
   construct (synthetic `.bk2` streams built one feature at a time), its
   outputs (pixels, PCM, return values) compared against ours. The DLL's
   code is never read — no disassembly, no decompilation, no debugging of
   its internals, no dumps of its tables.
3. The `.bk2` files in the owner's own game installs, as inputs.
4. Our own experiments and measurements.

**Not allowed:** reading, translating or "checking against" any other Bink
decoder's source — FFmpeg (including `binkaudio.c` and unmerged patches),
NihAV, libavcodec forks, bonkdec, unbikit, vgmstream, the Bink code shipped
in game engines, `mercs2_bink`, or RAD's SDK headers and samples; copying
their tables or constants; running another decoder to get reference output
(the reference is the game's own DLL).

**Provenance.** Every rule in the specifications carries a tag —
`[wiki:<page>]`, `[probe:<experiment>]` or `[ours]` — and every probe is
written down with the experiment that established it:
[`crates/bink2/docs/spec/probes.md`](../crates/bink2/docs/spec/probes.md) and
[`crates/binkaudio/docs/spec/probes.md`](../crates/binkaudio/docs/spec/probes.md).
The research harnesses (the DLL oracle, the probe-stream builders, the
comparison tools) live in separate research repositories that build against
these crates.

**Results**, measured against the DLL on every Bink 2 file the games ship:

- Video: bit-exact on all 54 files, 74,680 frames (scalar and AVX2 paths).
- Audio: all 80 tracks of the 17 files with sound (938 million samples)
  decode to the DLL's sample counts; 0.017% of samples differ by one, where
  the DLL's 32-bit float rounding lands on the other side of a half.

**Disclosures.** This is a *procedural* clean room, not an independent
two-party one, and the crates say why in their
[`CLEANROOM.md`](../crates/bink2/CLEANROOM.md) files
([audio](../crates/binkaudio/CLEANROOM.md)):

- The decoders were written by an AI model working for the project's owner.
  Models of its kind are trained on public source code, very probably
  including FFmpeg's Bink audio decoder. No such source was consulted and
  nothing was written from recall — every rule comes from a tagged source
  above — but the model cannot rule out that prior knowledge shaped which
  hypotheses it tested first.
- While options were being evaluated, before the video decoder existed, a
  summariser read NihAV's Bink 2 source in a separate session and reported
  a few facts about it. They are logged in `CLEANROOM.md`, were not used as
  a source, and anything the decoder needed from them was re-established by
  its own probe.

Whether that is sufficient is a question for counsel. The project owner accepted this risk
on 2026-11-14 ([#6907](https://github.com/matiaszanolli/ByroRedux/issues/6907); the
[Bink video playback PRD](prd/bink-video-playback.md), Q1 and Q2).

**What it does not cover.** Bink 1 (`BIKi`, the Oblivion → Skyrim movies)
plays through [`mercs2_bink`](https://crates.io/crates/mercs2_bink), an MIT
crate from another project whose own description says it was written "from
the specifications beside it, which were read out of binkw32.dll"; it is a
dependency, outside these rules, and the clean-room crates' authors do not
read its source. Separately, the engine's *behaviour* research — how the
original games choose music, for instance — is not clean-room work: it
reads the community references credited in the README's
[Acknowledgements](../README.md#acknowledgements) and, where none answers,
the games' executables (Fallout 3's and Oblivion's code was traced for the
music rules) as documentation. No code is copied from any of them.
