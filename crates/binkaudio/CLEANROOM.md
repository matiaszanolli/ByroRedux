# Clean-room rules and provenance

This crate is meant to be a Bink Audio decoder whose authors can say, for every fact in it, where it
came from. It follows the rules of the Bink 2 video decoder (`crates/bink2/CLEANROOM.md`), applied to
audio.

The decoder, its specification (`docs/spec/`) and this file live in ByroRedux as `crates/binkaudio`.
The research harness — the oracle that runs the game's DLL, the packet builder and probe tools, the
comparison tool `bacmp` and the snapshots of the public pages below — lives in the research
repository `binkaudio-clean`, which builds against this crate. These rules bind both. ByroRedux's
`crates/video` links `mercs2_bink`, which also decodes Bink audio: whoever works on this crate does not
read that crate's source either.

## Allowed sources
1. **Public documentation**, snapshotted in the research repository's `docs/sources/` (retrieved
   2026-11-12 and 2026-11-13): the MultimediaWiki pages *Bink Audio*, *Bink Container* and *RAD Game
   Tools Bink API* (`https://wiki.multimedia.cx/index.php/<page>`); RAD's user-facing pages (the
   passages of `https://www.radgametools.com/bnkhist.htm` about audio tracks and Bink Audio 2).
2. **Black-box behaviour of the user's own copy of a game's Bink 2 DLL**, called through its
   exported function names, with inputs we construct and outputs we read (PCM, return values). The
   DLL's code is **not** read: no disassembly, no decompilation, no debugging of its internals, no
   memory dumps of its tables. Its export table (function names) is metadata and may be listed.
3. **The Bink files in the user's own game installs**, as inputs and as the thing to decode.
4. **Our own experiments and measurements**, recorded in `docs/spec/` with the experiment that
   produced them.

## Not allowed
- Reading, translating or "checking against" any other Bink audio decoder's source: FFmpeg's
  (`binkaudio.c`, including unmerged patches), NihAV, libavcodec forks, `mercs2_bink`, vgmstream, the
  Bink Audio code shipped with game engines (Unreal Engine's included), bonkdec, unbikit, or RAD's SDK
  headers and samples. Prose descriptions without code may be cited.
- Copying tables, constants or structure from any of the above.
- Running another decoder to obtain reference output: the reference is the game's own DLL.

## Provenance log
Every spec fact carries a tag: `[wiki:<page>]`, `[probe:<experiment id>]`, or `[ours]`.

### Disclosure: what the author may already know (2026-11-13)
This crate was written by an AI model working for the owner. Models of its kind are trained on large
amounts of public source code, which very probably included FFmpeg's Bink audio decoder and possibly
other Bink decoders. No such source was read or consulted during this work, and nothing was written
from recall: every rule comes from the sources above and carries its tag. But the model cannot rule
out that what it already knew shaped which hypotheses it tested first (for instance the coefficient
group syntax of A4, which the wiki does not describe and which was then established from the DLL's
behaviour). The same agent also wrote the Bink 2 video decoder, whose `CLEANROOM.md` logs one earlier
exposure to facts about NihAV's video decoder (none about audio).

This makes the process a **procedural** clean room, not an independent two-party one. Whether that
is sufficient is a question for counsel, not for this file.
