# The oracle

`oracle/src/oracle.c` is a Windows program (built with MinGW, run under Wine) that loads a game's
own `bink2w64.dll`, opens a `.bk2` through the exported `BinkOpen`, decodes frames with
`BinkDoFrame` / `BinkNextFrame`, and writes each frame through `BinkCopyToBuffer`. It uses
exported function names only. Build and run: see the header of the source file; the DLL is copied
into the git-ignored `oracle/bin/`.

## P0 — the oracle runs (2026-11-12)  `[probe:P0]`
- Input: Fallout 4 `Data/Video/Intro.bk2` (`KB2g`, 1920x1080, 3944 frames) and the DLL shipped
  with Fallout 4 (421-438 KB `bink2w64.dll`).
- `BinkOpen(path, 0)` succeeds under Wine; the first three 32-bit fields of the returned object
  (width, height, frame count) equal the container header's, as the public Bink API page says
  they should (`wiki-rad-bink-api`). The harness refuses to continue if they differ.
- Frames 0-16 decode to black, frame 17 on to a picture; frame 60 reads "BETHESDA GAME STUDIOS"
  in light grey on black — a real decode.
- `BinkCopyToBuffer` with surface types 3, 4, 5 and 6 writes identical bytes for this frame
  (grey on black cannot show channel order) and leaves the fourth byte of each pixel 0. Which
  type is which, and the exact colour converter, are **open**; a coloured frame is needed.
- Decode speed under Wine: 120 frames of 1080p in a few seconds.

## P1 — the decoder will write its planes into buffers we own  `[probe:P1]`
Method: look at memory the DLL returns or fills; call exported functions only.
- **P1a.** The object `BinkOpen` returns holds, among small integers, 64-bit pointers. Three of them
  (+0x2c8, +0x2d0, +0x2f0) point into one multi-megabyte region. Dumped as a picture it is
  high-entropy noise: the compressed input, not decoded pixels. (Not a plane; ignore.)
- **P1c.** `BinkGetFrameBuffersInfo(bink, buf)` on a sentinel-filled buffer writes exactly:
  `u32 2` (buffer sets), `u32 Y width` (1920), `u32 Y height` (1088, rounded up to a multiple of
  16 from 1080), `u32 C width` (960), `u32 C height` (544), then 2 sets x 4 plane records of
  24 bytes at `+0x18` and `+0x78`: `{u32 present, u32 pad, u64 pointer, u32 pitch, u32 pad}`.
  Plane order Y, Cr, Cb, A (the A record has `present = 0` on a stream without alpha). Pointers
  and pitches are zero: the decoder allocates nothing itself until asked.
- **P1d.** Filling both sets with our own buffers (`present = 1`, pointer, pitch = padded width) and
  passing the record to `BinkRegisterFrameBuffers` *before the first `BinkDoFrame`* makes the DLL
  decode into them. Frame 60 of `Intro.bk2` appears in the Y plane as the expected picture;
  every byte of all six buffers changed.
- **P1e.** The sets alternate: after `BinkDoFrame` on frame *i*, frame *i* is in set `(i+1) & 1`
  (frame 15 in set 0, 16 in set 1, ...). The other set still holds frame *i-1*. The harness
  detects the written set by content change and falls back to parity for a repeated frame.
- Black in the Y plane is 0 (mean 0.95 over a mostly black frame) - Y is not studio-range 16-235
  here, or the stream's black is 0. To be pinned when colour is probed.

`oracle.exe ... <out-prefix> [max-frames]` now defaults to this mode and writes one
`<prefix>.<frame>.yuv` per frame: Y, then Cr, then Cb, padded dimensions, tight pitch. Pass
`rgb<N>` as the fifth argument for the old `BinkCopyToBuffer` path with surface type N, `rgb<N>:<K>`
to write frames K and later only (P33).

## Open
- Colour: surface types, matrix, range and chroma upsampling measured in P33; the converter's exact
  rounding is still open there.
- Alpha plane (`present` for A) on streams that carry alpha.
- A way to feed the DLL handcrafted bitstreams: the container is public (`wiki-bink-container`),
  so a `.bk2` of our own can be written and opened by path.
