# State of the Bink 2 clean-room effort (handoff)

Last updated 2026-11-13 (P33; speed; moved into ByroRedux). Provenance rules: `CLEANROOM.md`. Every observation, with the
probe that produced it: `docs/spec/probes.md`. Nothing here comes from any other decoder.

## Code
The decoder is ByroRedux's `crates/bink2` (`byroredux-bink2`); `ref/`, `tools/`, the oracle and
`bk2cmp` named below are in the research repository `bink2-clean` (`CLEANROOM.md`), whose `bk2cmp`
builds against this crate.
- **`src/` — the decoder (Rust, no dependencies).** A port of `ref/`; every rule keeps its probe tag.
  Validated against the Python reference line for line: all 181 probe files (28,009 frames, including
  the 14 stops on a negative quantiser in `fz`) and the 30-frame coverage below. The scalar code is the
  reference; AVX2 kernels (`src/simd.rs`, chosen at run time) and slice threads compute the same frames
  (Speed, below).
- **`ref/` — the Python reference**, the record of how each rule was found; the probing tools in
  `tools/*.py` import it. New rules go into `src/` first.

## What decodes bit-exactly
- **Every game file we have, every frame — 54 files, 74,680 frames** (P32):
  - KB2g (P29): the 14 Fallout 4 videos (45,539 frames, 1920x1080) and Fallout 76 `BGSLogo4k`.
  - KB2j (P30-P32): Starfield's 36 files (15,942 frames at 1080p, 1440p and 4K) and Fallout 76's
    `AtomShop`, `Intro` and 4K `MainMenuLoop` — keyframes, two and four slices, MV clamping, filters
    per direction, and the filter line maps of flag bit 16 (keyframes and inter frames).
- Every probe file of P20-P31 except the experiments below.
- Not exact, by design: the experiment files (bit flips `flip`/`fz`/`f2`/`rf`, appended random tails
  `l2`/`lb`/`as`, consumption probes `cons`, the invalid pre-P22 slice probes `sl`/`sl4`, the
  bit-16-with-filter-A probes `kb2j_flags4`, `kb2j_a16_*`, `kb2j_fa16_*`, `kb2j_fh16_*`) and the DC sweeps
  at q >= 37.

## Speed
Release build on a Ryzen 9 7950X, chained decode of each file's first 1,000 frames (`bk2cmp bench`),
ms per frame (worst frame in brackets):

| file | size | scalar, 1 thread | AVX2, 1 thread | AVX2, 4 threads |
|---|---|---|---|---|
| `fo4_Intro` (KB2g, 2 slices) | 1920x1080 | 7.5 (14.3) | 3.4 (9.1) | 2.2 (4.5) |
| `sf_EndingVision` (KB2j, 4 slices) | 2560x1440 | 12.9 (16.8) | 5.4 (8.9) | 2.1 (4.1) |
| `fo76_MainMenuLoop` (KB2j, 4 slices) | 3840x2160 | 17.7 (25.0) | 8.8 (18.2) | 2.9 (5.6) |
| `sf_MainMenuLoop` (KB2j, 4 slices) | 3840x2160 | 22.1 (27.3) | 12.6 (23.7) | 4.1 (6.7) |

Whole files, AVX2 on one thread: every 1080p file 2.4-4.7, every 1440p file 4.1-4.7, the 4K ones 9.8
and 12.0. Before this work (commit 30cda07) `sf_MainMenuLoop`'s first 120 frames took 73 ms each.
The colour conversion for display (`Frame::rgba_into`, P33) adds about 1.3, 2.3 and 5.2 ms a frame at
1080p, 1440p and 4K with AVX2 (6.5 ms at 1080p scalar; `bk2cmp bench --rgba`). Through ByroRedux's
`crates/video` (4 slice threads, conversion, scaling to the window) a whole movie decodes at 4.5 ms a
frame at 1080p and 1440p and 9.5 ms at 4K (its corpus test).

- **Same frames on every path.** `bink2::set_simd(false)` (`bk2cmp --scalar`) runs the scalar
  reference; each AVX2 kernel has a unit test against its scalar twin, and `coverage all` is exact on
  all 54 files for the scalar path and for AVX2 with 4 slice threads.
- **Threads** (`Options::threads`, `bk2cmp --threads`): slices decode at once, each from its packet
  offset word, checked against the 32-bit boundary after the slice before it (P22; on a mismatch the
  rest are decoded again in order) — every frame of every game file has offset words that match. The
  post-filters then run in bands of block rows (filter B and vertical edges), then of block lines
  (horizontal edges, each band starting half a block above its first line). Slices write disjoint rows
  through `frame::Rows`. Threads are spawned per frame (three times: slices, then the two filter
  passes); a persistent pool would save that where the caller has one.
- **What it took** (scalar first, then SIMD): a bit reader that peeks 57 bits with table-driven
  codewords; motion compensation from a clamped window (or the plane in place) with the 6-tap filter
  separable; no per-macroblock temporaries (reused `context::Scratch`, frame buffers reused by
  `Decoder`); the residual in i32 when every coefficient of a macroblock's plane is below 2^24 (i64
  otherwise, same results); filter decisions per block edge. AVX2: luma 6-tap and chroma bilinear MC
  (16-bit lanes), IDCT (i32, two 8x8 transposes), residual add and intra store (saturating packs),
  the P23 block DCs (`pavgb`/`psadbw`), filter A's horizontal edges and filter B's blocks.
- **Profiling:** `bk2cmp bench <name> --profile out.txt` — a SIGPROF sampler inside `bk2cmp` named
  through `addr2line` (`perf` is not permitted on the development machine).

## How to check
In the research repository: `cargo build --release`, then from its root:
  - `target/release/bk2cmp check <name>... [--frames 1,3] [--show N]` — every frame decoded with the
    *oracle's* previous frame as reference (a mismatch is local to that frame), per macroblock on the
    visible rows; same output as `tools/oraclecmp.py`. All frames and files run in parallel.
  - `target/release/bk2cmp chain <name>...` — decoded from our own output, whole planes.
  - `target/release/bk2cmp coverage <N|all> <name>... [--reuse]` — the first N frames (or all) of real
    files (game videos symlinked as `probes/files/fo4_*.bk2`, `fo76_*`), chained, against the oracle's
    plane hashes (`fnv` mode: no disk); files in parallel; at the first differing frame the oracle's
    raw planes of it are fetched and that frame is `check`ed. ~4 minutes for all 14 Fallout 4 files.
  - `target/release/bk2cmp dump <name> <frame> <out.yuv>` — our planes, padded, Y U V.
  - `target/release/bk2cmp mb <name> <frame> <mbx> <mby>` — one INTER macroblock: syntax, DC levels,
    and per luma block our residual next to the oracle's (oracle pixels minus our prediction).
  - `target/release/bk2cmp maps <name> <frame>` — a KB2j bit-16 frame's filter line maps read back from
    the oracle's output (their length and true lines), next to what the decoder parses (P32).
  - `target/release/bk2cmp bench <name>... [--frames N] [--threads T] [--profile out.txt]` — decode
    speed, a hash of every output plane (an optimisation must leave it unchanged) and the slice offset
    word check. `--scalar` (any command) turns the AVX2 kernels off.
- `tools/run_oracle.sh file.bk2 out-prefix [frames] [yuv|yuv:K|fnv|info|rgbN]` — the game's
  `bink2w64.dll` under Wine, exported API only, raw planes or plane hashes out (`docs/spec/oracle.md`);
  `ORACLE_DLL=bink2w64_sf.dll` (Starfield's, needed for KB2j; `bk2cmp` picks it by the file's magic).
- Probe streams: `tools/mkbk2.py` (container), `tools/inter.py` (textured keyframes, inter-frame grids),
  `tools/sliceprobe.py: two_slice_packet` (frames of 4+ macroblock rows always have two slices; slice 2
  starts at the next 32-bit boundary — never pad between slices), `tools/mvenc.py`, `tools/acenc.py`.

## Decoder layout (`src/`; the same split in `ref/bink2_ref.py`)
`lib.rs: decode_frame` (slices, P22) → per macroblock: `intra.rs: decode_mb` (keyframes and INTRA in
inter frames; element syntax P10-P18, DC clamp P26) or `inter.rs: decode_mb` (block-type list P20,
`predict_mv` + `mv_order` P21/P22, `edge_mv` P22, `mc_luma`/`mc_chroma` P20/P26, residual P21 with the
in-macroblock residual filter P25, intra-in-inter P23, `block_avg_dc` = the P23 column cascade).
`context.rs: SliceCtx` holds the cross-macroblock rules (quantiser maps with held predictions, CBP
histories; `hold` = P23/P26), `Scratch` the reused per-macroblock buffers. `filters.rs`: `filter_b`,
filter A (`PlaneInfo::rule`; keyframes are the all-intra case), `filter_a_residual` (P19, P24, P25), the
banded `apply`. `idct.rs`: IDCT P12 and dequantisation P13/P16, exact below 2^40 per dequantised value
(beyond = an error: desync or the unprobed extreme range); `Coef` = the i32/i64 residual. `frame.rs:
Rows` = one slice's rows of a frame. `simd.rs`: the AVX2 kernels. `rgb.rs`: `Frame::to_rgba`, the
colour converter measured in P33. `container.rs`: `.bk2`.

## Open, in the order they block real playback
1. **KB2j details no game file has shown wrong yet:** which header field decides 4 slices (video flags
   bit 1 or the extra word), the KB2j residual DC bound, Wolfenstein II's KB2i/KB2j files (1920x816:
   the padding bound for filter A / MC), alpha planes. Audio is separate.
2. **Extreme values:** the DC cases are solved (P27, P28); left: keyframe AC level 30 at high
   frequencies (`nb5`), quantisers >= 37 (P29), AC level prefix 12, the intra DC upper clamp (2047
   assumed; only 0 is confirmed).
3. **Colour:** the DLL's converter is measured (P33: full-range BT.601, bilinear centred chroma) but
   not its exact rounding; `Frame::to_rgba` is within 2 of it.
4. **Sound:** Bink 2's audio tracks are not decoded (ByroRedux PRD `bink-video-playback`,
   BIK-DEC-003: whether they are the Bink 1 audio format is a verify item there).

## Hypotheses that failed (don't repeat)
- Slice 2 at the packet's offset word / split at (mbh + 1) // 2 (P22).
- Filter line map (P32): bit 16 = "no chroma residual"; n = the visible size / 8; the run length width
  fixed per map (from n - 1) or from the lines left minus the shortest run; a length read when exactly
  the shortest run's lines are left; an implied opposite line after every run; maps in inter frames only.
- Filter B as a tensor product with one rounding, separable with any intermediate shift, box/bilinear
  cascades, low-order coefficient synthesis (P24). Filter A in inter frames on the pixel step with the
  keyframe rules everywhere (P25).
- MV prediction at edges: "A only / B only", "missing = 0", "missing = adjacent macroblock's quadrant 3".
- Non-intra DC = pixel mean in any rounding, any 8x8/16x16 window, edge rows/columns, 2x2-box pyramids,
  the stored keyframe DC (P23).
- Residual quantiser: separate "running left value" plus "intra offers its q" (fits 2008 - 23 MBs).
- Earlier: KB2f-style DC coding; wiki "quantiser, CBP, DCs" order from bit 0; measuring consumption by
  appending a second macroblock; single-bit flips of unary/terminator bits (they desynchronise).
