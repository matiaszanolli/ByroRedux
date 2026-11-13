# Clean-room rules and provenance

This crate is meant to be a decoder whose authors can say, for every fact in it, where it came from.

The decoder, its specification (`docs/spec/`) and this file live in ByroRedux as `crates/bink2`. The
research harness — the oracle that runs the game's DLL, the probe streams and tools, the Python
reference the rules were first written in, the comparison tool `bk2cmp` and the snapshots of the
public pages below — lives in the research repository `bink2-clean`, which builds against this crate.
These rules bind both. ByroRedux's `crates/video` links `mercs2_bink`, a Bink 1 decoder: whoever works
on this crate does not read that crate's source either.

## Allowed sources
1. **Public documentation**, snapshotted in the research repository's `docs/sources/` (retrieved
   2026-11-12): the MultimediaWiki pages *Bink Video 2*, *Bink Container*, *RAD Game Tools Bink API*
   (`https://wiki.multimedia.cx/index.php/<page>`); RAD's user-facing help pages.
2. **Black-box behaviour of the user's own copy of a game's Bink 2 DLL**, called through its
   exported function names, with inputs we construct and outputs we read (pixels, return values).
   The DLL's code is **not** read: no disassembly, no decompilation, no debugging of its internals,
   no memory dumps of its tables. Its export table (function names) is metadata and may be listed.
3. **The `.bk2` files in the user's own game installs**, as inputs and as the thing to decode.
4. **Our own experiments and measurements**, recorded in `docs/spec/` with the experiment that
   produced them.

## Not allowed
- Reading, translating or "checking against" any other Bink decoder's source: FFmpeg (including
  unmerged patches), NihAV, libavcodec forks, bonkdec, unbikit, `mercs2_bink`, or RAD's SDK headers
  and samples. Prose descriptions in blog posts that contain no code may be cited.
- Copying tables, constants or structure from any of the above.

## Provenance log
Every spec fact carries a tag: `[wiki:<page>]`, `[probe:<experiment id>]`, or `[ours]`.

### Disclosure of prior exposure (2026-11-12)
The agent that bootstrapped this repository, while evaluating options in a separate session for
the engine this decoder is for, had a web-fetch summariser read NihAV's `bink2.rs` and report
facts about it. The report named the revision tags it checks (`KB2f`, `KB2g`; every other tag
falls to a generic branch), that it handles an alpha flag (`0x100000`), that it has an older
float-based path selected by a frame flag (`0x2000`), its entry point name, and that revisions
after `KB2g` select a slice count by indexing a table with the low two frame-flag bits. No code,
tables or constants were reproduced into this repository. Those facts are **not** to be used as
a source: anything the decoder needs from that list must be re-established by a probe and logged
as such. The same session read the MultimediaWiki pages above (allowed) and Kostya Shishkov's
blog posts about Bink 2 in summary (prose, no code).

This exposure means the process is **procedural** clean room, not an independent two-party one.
Whether that is sufficient is a question for counsel, not for this file.
