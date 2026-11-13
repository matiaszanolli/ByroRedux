# Bink Audio — probes

Every fact the decoder uses, with the experiment that established it (`CLEANROOM.md`). Probe IDs are
`A<n>` to keep them apart from the Bink 2 video probes (`P<n>`, ByroRedux `crates/bink2`).

## A0 — the corpus (2026-11-13) `[probe:A0]`
Method: the container headers (`[wiki:Bink Container]`) of every `.bk2` in the installed games.
- 17 of 54 files carry audio: Fallout 4's 12 films with 5 tracks and `GameIntro_V3_B` with 6
  (`MainMenuLoop` none); Fallout 76's `BGSLogo4k` (1) and `Intro` (6); Starfield's
  `BGS_LOGO_1080p_BinkVersion` (1) and `EndingVision` (6). Starfield's 34 power and artifact clips have
  none.
- Every track: 48000 Hz, channels word 2, first word 14848; flags `0x7000` (bits 14, 13 stereo, 12
  DCT) except `EndingVision`'s six (`0x3000`: bit 14 clear). Track IDs 1-5 (plus 0 in
  `GameIntro_V3_B` and `BGSLogo4k`/`BGS_LOGO`, plus 8 in the two six-track `KB2j` files).
- RAD's history page: Bink Audio 2 ("mostly a bit reordering of Bink Audio 1.1") arrived in 2021.06
  (`docs/sources/rad-bink-history-excerpts.txt`); Fallout 4's DLL is older, so its files cannot use
  it.

## A1 — the DLL's track API (2026-11-13) `[probe:A1]`
Tool: `oracle/src/oracle_audio.c`. Fallout 4's `bink2w64.dll`, `GameIntro_V3_B.bk2`, `BinkOpen`
flags 0.
- `BinkGetTrackID(bink, index)` returns the header's track IDs (0-5 here): the signature holds.
- `BinkGetTrackType(bink, index)` returns `flags << 16 | sample rate` (`0x7000BB80`).
- `BinkGetTrackMaxSize(bink, index)` returns 146432.
- `BinkOpenTrack(bink, index)` gives a handle for every track without any open flag;
  `BinkGetTrackData(track, buffer)` after `BinkDoFrame` returns a byte count and fills the buffer with
  16-bit little-endian PCM, channels interleaved (it scribbles past the count: up to 154624 bytes for
  145920 returned). RAD documents it as the way to get a frame's audio (history page, 2026.03).
- The count equals the container's per-packet "number of samples" word, which is therefore the
  decoded size in bytes: 145920 for frame 0 (19 x 7680), 7680 for most frames, 15360 now and then.
  7680 bytes = 1920 samples per channel = 2048 - 128: one 2048-sample block per frame overlapped by
  a 128-sample window (frame size / 16, `[wiki:Bink Audio]`) — a hypothesis for A2.
- On the frames read (0-39) all six tracks decode to the same samples.

## A2 — handcrafted files; where things sit in a block (2026-11-13) `[probe:A2]`
Tools: `tools/mkbk2a.py` (a Bink 2 file with chosen audio packets around black 32x32 keyframes, every
frame a keyframe), `tools/flipmap.py` (flip one bit of a one-block packet at a time; each variant
follows an unflipped copy so all overlap the same tail; mean change per channel). Reference: frame 1 of
Fallout 4 `GameIntro_V3_B` track 0 (1420 bytes, 7680 decoded bytes).
- A real packet alone in a handcrafted file decodes; repeating it, the second copy's first samples
  nearly equal what followed the first copy in the original file: the frame's head is blended with
  the previous block's tail.
- Bits are read LSB-first. Bits 0-1 change nothing. Bits 2-30 are a float: 5-bit exponent, 23-bit
  mantissa, sign (`[wiki:Bink Audio]` get_float; flipping exponent bits scales the contribution by 2,
  4, 16, 1/256, 1/65536; the mantissa's top bits double from bit to bit; the sign doubles it). Bits
  31-59 a second float, then 8-bit fields from bit 60 whose top two bits change the output enormously
  (quantizers past the `min(v, 95)` clamp). Everything up to channel 0's end moves only the left
  channel.

## A3 — the two floats: DC and the first cosine; output scale, window (2026-11-13) `[probe:A3]`
Handcrafted one-block packets: 2 zero bits, the two floats, 25 zero quantizers, zero bits after.
- A float is `m * 2^(e - 23)` for exponents 0-23 (mantissa 1000: exponents 17-23 give 1, 1, 3, 6, 11,
  22, 44; mantissa 2000 at 22 = 1000 at 23); exponents 24-31 give 0 (with mantissas 125-2000, alone
  and inside a real packet). The real packet's floats: exponent 8 and 10.
- The first float is a constant across the block (1000 -> 44, 2000 -> 88, negative mirrored), the
  second a half cosine with the same scale: antisymmetric about output sample 1023.5, i.e.
  `cos(pi (n + 0.5) / 2048)` over block samples n = 0..2047. Neither depends on the quantizers.
  Amplitude for 400000: 17675 (scale 0.04419, about 2 / sqrt(2048); to be pinned).
- A frame outputs block samples 0..1919; the last 128 are held and blended into the next block's first
  128 (`[wiki:Bink Audio]` window, frame size / 16). The blend works on clamped 16-bit values with
  truncation: a saturated block after silence fades in as 16383 at sample 64, 32511 at 127
  (32767 i / 128).

## A4 — coefficients (2026-11-13) `[probe:A4]`
Tools: `tools/parse.py` (hypothesis parser), flip maps of bits 160-420 and 4000-8000.
- After the floats, 25 quantizers of 8 bits (bits 60-259; the last two change nothing in the reference,
  whose top bands are empty).
- Then groups until 2048 coefficients per channel (the first two being the floats): a flag bit; if set,
  a 4-bit index into the RLE table (`[wiki:Bink Audio]`: 2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15,
  16, 32, 64) times 8 coefficients, else 8; a 4-bit width; then per coefficient `width` bits of
  magnitude, LSB-first, and a sign bit (1 = negative) only after a non-zero magnitude. Evidence: the
  flip map's desync bits are exactly the lone set bits and zero magnitudes (a sign bit appears or
  vanishes); each sign bit's effect is twice its coefficient's; and parsed this way channel 0 ends at
  bit 5520, where channel 1 begins with no gap and no 2-bit lead-in (its floats -169.4 / -545 and
  quantizers 30, 33, 29, 19 mirror channel 0's -169.5 / -544.4 and 32, 33, 29, 19), ending at bit 11345
  of 11360 — inside the packet's last 32-bit word. 1024 or 4096 coefficients, or aligned or offset
  channel starts, give implausible second channels.
- The flip-map boundary agrees: bits up to 5500 move the left channel (and, desynchronising, the
  right), 5525 nothing (channel 1's first float's lowest mantissa bit), from 5550 the right only.

## A5 — dequantization and the transform (2026-11-13) `[probe:A5]`
Tools: `tools/abuild.py` (one-block packets from chosen floats, quantizers and coefficient magnitudes,
in the A4 syntax). One coefficient at index 100, magnitude 1, every quantizer q.
- The output is `a cos(pi 100 (n + 0.5) / 2048)` (residual within 0.57: rounding), with
  `a = 0.044195 * 10^(min(q, 95) * 0.0664)` for q = 50-80 (`[wiki:Bink Audio]` quantizer); q >= 95
  (96, 100, 127, 128, 200, 255) all equal q = 95. 0.044195 is the floats' scale too (A3):
  2 / sqrt(2048) = 0.0441942. So a block is `x[n] = 2 / sqrt(2048) * sum_k c_k cos(pi k (n + 0.5) / 2048)`
  over all 2048 coefficients, the DC term not halved; c0, c1 are the floats, `c_k` = magnitude x
  quantizer of k's band.

## A6 — the bands (2026-11-13) `[probe:A6]`
Quantizer of band k set to 45 + k; one coefficient at each index 2-2047 in turn; the band read back
from its amplitude.
- 25 bands, starting at 2, 8, 16, 24, 34, 42, 52, 64, 78, 92, 108, 126, 146, 170, 196, 230, 268, 314,
  374, 452, 546, 656, 810, 1024, 1322 (the last running to 2048). Every edge after the first is
  `2 * floor(f * 1024 / 24000)` for the critical frequencies f = 100 ... 15500 (`[wiki:Bink Audio]`,
  frame length 2048 and half the sample rate 24000), the one at 24500 falling past 2048. Measured at
  48000 Hz only (the only rate in the corpus).

## A7 — conversion, the window, whole tracks (2026-11-13) `[probe:A7]`
Tools: `tools/proto.py` (a numpy decoder of A2-A7), the oracle's PCM of `GameIntro_V3_B` track 0 (40
frames, 230400 samples); handcrafted fades between constant blocks.
- Block samples are rounded to the nearest integer and clamped to 16 bits (truncation: 48% exact; with
  rounding, 5 differences in 215040 samples outside the overlaps, every one within 0.00003 of a .5 tie:
  float noise).
- The window works on the interleaved 16-bit values: with `w` = 128 x channels values of overlap, value
  j (0..w-1) of the new block's head becomes `sat16(t_j + floor(wrap16(h_j - t_j) * j / w))`, `t` the
  previous block's last w values. Fades between constant levels give exactly this (1000 -> 0: 992,
  984, 976 at j = 2, 4, 6; 20000 -> -20000: the 16-bit difference wraps to 25536 and the fade
  saturates at 32767 from the middle on — the A3 surprise). In stereo the right channel's weights are
  half a step later than the left's: per channel, i/128 (left) and (2i + 1)/256 (right); on the real
  track that is 0 and 1 differences in 7552 overlap values each, against 684 for i/128 on the right.
- The first block of a stream is not blended with anything: it comes out as is.
- A packet holds `decoded bytes / (2 x channels x 1920)` blocks, each starting with 2 unused bits and
  padded to 32 bits (A4: frame 0's 19 blocks parse to the packet's last bit; Starfield's `EndingVision`,
  flags 0x3000, and Fallout 76's `Intro` parse the same way to their last bit).
- Altogether: 6 differences (all +-1, float ties) in the 230400 samples.

## A8 — the last packet; whole files (2026-11-13) `[probe:A8]`
Tool: `src/bin/bacmp.rs` (every track of a file through the oracle and the Rust decoder, sample by
sample).
- Every file's last audio packet states a size short of whole blocks (2560, 6400, 1456 ... bytes): its
  blocks are decoded as usual, the last one too, and only the stated samples are output. With that, the
  decoder's sample count equals the DLL's for every track (`GameIntro_V3_B`: 1049048 per track,
  `BGSLogo4k`: 1008000), every difference +-1 (34 and 106: float ties, as in A7).
- The whole corpus: 17 files, 80 tracks, 938211020 samples; every track's count and every frame's size
  equal the DLL's; 163868 samples (0.017%) differ, every one by 1 — the float ties of A7, more of them
  in louder tracks (the DLL's own float error grows with the signal).

## A9 — what the tracks are (2026-11-13) `[probe:A9]`
Tool: `examples/tracks.rs` (every track decoded; against track 0, the share of samples that differ and
the correlation).
- Fallout 4 `Intro`: 5 tracks of the same loudness, each differing from track 0 in 99% of samples with
  correlation 0.87-0.89 (the same music and effects, different voices); the narrated `CHARISMA`: 0.15-0.18;
  Fallout 76 `Intro` (6 tracks, IDs 1-5 and 8): 0.78-0.83; Starfield `EndingVision` (6 tracks): all six
  identical. So the tracks of a file are language versions of one mix. Which ID is which language is
  not measured here (ByroRedux PRD `bink-video-playback`, BIK-DEC-002's verify item).
- Speed (`examples/speed.rs`): about 490 times real time per stereo track on one core.
