# Probe log: the KB2g bitstream (handcrafted streams → oracle)

Every entry is something observed by feeding a stream we wrote (`tools/mkbk2.py`) to the game's own
decoder through `oracle/` and reading the Y/U/V planes it leaves. Statements are tagged `[probe:Pn]`.
Where the MultimediaWiki page already says it, the tag is `[wiki:video2]` and the probe only
confirms. Nothing here comes from any other decoder.

## P2 — the container and frame wrapper `[probe:P2]`
- A container written by `tools/mkbk2.py` (44-byte header, **0 audio tracks**, index of frames+1
  offsets, frame bytes) opens in `BinkOpen` and decodes; 32x32 is accepted (Y 32x32, C 16x16, **one**
  buffer set when the file has one frame; two sets otherwise).
- A KB2g video packet is `u32 flags, u32 offset-of-slice-2, slice-1 bits…, slice-2 bits…`. The real
  keyframes of Fallout 4's `Intro.bk2` carry flags `0xe000` (`0xe018` once); `0xe000` works for ours.
  The offset is measured from the start of the video packet (real: 1032 for a 1920x1080 frame whose
  slice 1 covers 17 macroblock rows).
- **Slice data starts at once with macroblock data** — there is no extra slice header word for KB2g.
  Evidence: in a one-macroblock frame the first 38 bits of slice 1 decide the whole picture; flipping
  any later bit changes nothing (`tools/flipmap.py`).
- A stream of zero bytes decodes to Y=0, U=V=0 (an error fill?), not to black: **black is Y=0,
  U=V=128** `[probe:P2]`, which the real black keyframe also produces.

## P3 — first intra macroblock, luma DC `[probe:P3]`
Setup: keyframe, 32x32 = one macroblock, bits = `0` `1 0` `1` then DC elements, which is the wiki's
KB2g order (quantiser delta 0; luma CBP "zero" + VLC bit; chroma CBP "reuse"; DCs).
- **All-zero DC deltas give flat grey 128**, i.e. the DC predictor for the first block is 1023
  (`(1023*8+32)>>6 = 128`): consistent with the wiki's "1023 for the first block" and `dst[0]=dc*8+32`.
- One level of DC delta moves a flat block by about **2.4 grey levels** (levels 19→21 give 145, 147,
  149; 34→39 give 152…164).
- The first element behaves differently from the wiki's generic DC element: for unary prefix
  `1`, `11` the picture does not move, for `111` it moves +2, `1111`+0 → +5 with the **next bit a
  sign and no extra magnitude bits** (`11110 0…` = +5, `11110 1…` = -5), `11111 0` + sign = ±7,
  `111111 0` + sign = ±10 … Bits after that sign are read as the *next* element (confirmed by
  changing them: the following blocks move by +2.4 per level).
- Reading: either the first element of the slice carries the start value (the wiki mentions an
  extra start value for the first macroblock of a slice in KB2f) or the element coding differs from
  the wiki's pseudo-code. **Open.**

## P4 — which bits of a one-macroblock keyframe steer what `[probe:P4]`
Tool: `tools/layout.py <bitstring>...` (single-bit flips of a base stream, zero padded; a letter per
bit index says whether flipping it changes the Y, U or V plane, `.` = nothing changes).
First correction to P3: the DC element run is *not* preceded by a fixed 4-bit head. What precedes
the luma DC elements is data dependent.

Observed (stream bit 0 first; `Y` = luma, `U`, `V`; groups of four are four 1-bit zero elements):

    base      layout
    00000     ............YYYY.YYYY.YYYY.YYYY...UUUU...VVVV
    00010     Y.......YYYY.YYYY.YYYY.YYYY...UUUU...VVVV
    00100     ......YYYY.YYYY.YYYY.YYYY...UUUU...VVVV
    01000     .....YYYY.YYYY.YYYY.YYYY...UUUU...VVVV
    10000     ...YYYY.YYYY.YYYY.YYYY...UUUU...VVVV
    01010     .......UUUU...VVVV                      (no luma DC elements at all)
    11110     ....YYYYYYYYYYYYYYYY.UUUU...VVVV        (16 single-bit luma elements, then U, V)

What this says:
- Luma DC elements come as **four groups of four** with one extra bit between groups when the
  stream is all zeros; chroma is **4 elements for U, 3 bits, 4 elements for V**, each preceded by
  3 bits in the all-zero stream. In the `11110` stream the luma elements run unbroken: the
  separators are therefore not fixed bits but part of what each element or group means.
- The number of header bits before the luma DC elements runs from 3 to 12 depending on the
  first bits (`10000`: 3, `01000`: 5, `00100`: 6, `00010`: 8, `00000`: 12), so the header is a
  variable-length code. Bit 0 set starts a unary quantiser delta (`10`+sign = 3 bits fits
  `10000`), which is the wiki's `dq`; what follows is the CBP machinery, not yet understood.
- For some headers the luma DC elements are absent altogether (`01010`, `00101`, ...): the luma
  section is then CBP-gated.
- Flipping a luma DC element bit moves the block *and every block predicted from it*, in the
  wiki's Z order `0 1 4 5 / 2 3 6 7 / 8 9 12 13 / 10 11 14 15` (verified for blocks 1-15 with
  `tools/flipgrid.py`). One DC level is about +2.5 grey levels when the element is not the first.
- Chroma DC units are large: one level on one chroma block moves the plane by 40.

Next: pin the header (CBP and VLC-select bits) by parsing the 38 real bits of Fallout 4's first
keyframe macroblock under candidate grammars and checking that the all-black picture results.

## P5 — luma DC elements come in 2x2 groups, and groups can be absent `[probe:P5]`
Tools: `tools/dcmap.py`, `tools/cbpscan.py`, `tools/hdrscan.py` (single-bit flips; for each flip the
Z-order index of the first luma block that changes).
- In every stream tried, the luma DC elements are **one element per block in the wiki's Z order**,
  **grouped by four (a 16x16 quarter of the macroblock)**, with **one extra bit after each group**
  (inert when zero). Example, stream `000000` + zeros: elements for Z0..Z3 at bits 12..15, a bit at
  16, Z4..Z7 at 17..20, a bit at 21, Z8..Z11 at 22..25, a bit at 26, Z12..Z15 at 27..30.
- **Whole groups can be missing**: prefixes exist whose DC elements start at Z4 or Z8 (groups 0, or 0
  and 1, absent). The group-presence code is a short unary-like code after the first header bits
  (`0100` + `0` → all groups, + `10` → groups 1-3, + `11x` → groups 2-3). For some prefixes there
  are no luma DC elements at all (U then starts about 4-8 bits in).
- The header before the DC elements is **12 bits for all-zero streams and shrinks with set bits**
  (`0001…`: 8, `0010…`: ≤6, `01…`: 5). Flipping the zero bits inside the 12-bit header changes
  nothing visible, so they are values that matter only when DC/AC data are non-zero (quantiser,
  nibble data) — not yet separated.
- Chroma: U elements (4), then 3 bits, then V elements (4); same shape for every prefix.
- Hypothesis from the real stream (not probe-confirmed): Fallout 4's first keyframe macroblock reads
  as quantiser delta `1101` (= -2), luma header bits `11`, then luma Z0 with unary prefix 6 and
  3 extra bits and sign `1`, then 15 zero elements (bits 19-33 are all zero), then chroma. That
  reading would make the first luma DC a large negative step (black). To be tested by building the
  same stream from this grammar and comparing planes.
- A two-macroblock frame (`tools/consume.py`) did **not** let macroblock 1 start cleanly at
  macroblock 0's end: a macroblock that is not first in the slice evidently has a different
  syntax (the wiki's "additional start value for the first macroblock" fits). So consumption
  cannot be measured that way with a macroblock copied from the first position.

## P6 — Fallout 4's first keyframe macroblock, parsed `[probe:P6]`
Stream = bytes `fb cf 07 00 d4` (bits LSB first), the first 38 bits of the real `Intro.bk2` keyframe,
fed alone as a 32x32 frame; output Y=0, U=V=128 (black). Single-bit flips (`probes` scratch
script, summarised) give this parse, which matches every flip:
- bits 0-3 `1101`: quantiser delta (unary `110`, sign `1` → -2; quantiser 14).
- **No CBP bits between the quantiser delta and the luma DC elements** in this macroblock.
- bits 4-18: luma Z0 = unary prefix **8** (`11111111 0`), **5 extra bits** (`0 1 1 1 1`, LSB first
  = 30), sign `1`. Value = `(1 << (8-3)) + 30 + 2` = 64 → **-64 levels**, exactly the wiki's
  formula `(1 << (n-3)) + bits(n-3) + 2` with the sign read **after** the extra bits.
- One luma DC level at quantiser 14 is **2.0 grey levels**, so Z0 = 128 - 64*2.0 = 0 (black).
  Flipping extra bit k (weights 2, 4, 8, 16 → Y 4, 8, 16, 32) and the sign (→ 255) reproduces
  this to the exact value. (At quantiser 16 one level was about 2.5; the DC scale depends on q.)
- bits 19-33: Z1..Z15, each a single `0` bit, **contiguous, no separator bits** (bit 19 → Z1, …,
  bit 33 → Z15; flips verified block by block in Z order).
- Separator bits and a long header appear for other quantiser-delta values and zero bits; whatever
  decides that (a CBP-like code, probably quantiser dependent) is the next thing to pin.
- Chroma follows the luma at bit 34. Enumerating all 9-bit tails (`X`) after bit 33:
  `X0 = 0`: U elements at `X1..X4`, three bits (`X5..X7`), V elements at `X8..X11`;
  `X0 = 1`: two bits (`X1,X2`) then V elements from `X3`. One chroma level moves a block by
  -33 (`95`) or +38 (`166`), `+2`/`-2`-style small steps for the low values: chroma DC elements
  use the same unary/extra/sign grammar with a larger step. In the real stream `X = 1 0 1 0…`, so the
  macroblock ends within bits 34-37 and chroma carries no coded values (U=V=128).

Open from this: where the CBP bits sit (the all-zero stream has 12 header bits and per-group
separators, this real stream has none), what the 3-bit gaps in chroma are, and the "first macroblock
of a slice" extra values.

## P7 — a DC-only macroblock, and "same as the left neighbour" `[probe:P7]`
Using the real black frame as the model (`tools/flip2.py`: 64x32 two-macroblock bit-flip map).
- **Real slice consumption check.** Fallout 4 `Intro.bk2` frame 0, slice 1: macroblock 0 is 38 bits
  (`1101` + 16 luma DC elements + `1010`) and every later macroblock is the 8 bits `1100 1010`:
  38 + 1019 x 8 = 8190 bits = 1023.75 bytes, i.e. exactly the 1024-byte slice (17 rows x 60 macroblocks
  = 1020). The partial grammar below therefore parses the whole real keyframe to its end.
- **DC step is constant: 2 grey levels per DC level** (not quantiser dependent; earlier values of
  2.0/2.4/2.5 were my mis-evaluated element values). `Y = 128 + 2 * level` for a flat macroblock.
- **Macroblock head, 4 bits, for keyframes** (as far as probed): bits 0-1 `11`, bit 2 inert,
  **bit 3 = "luma DC elements follow"**. `1101`/`1111` → 16 luma DC elements; `1100`/`1110` → none.
  Flipping bit 2 changes nothing visible; bit 3 set/clear is the presence switch (flip experiment on
  macroblock 1 of the real frame, bit 41, adds luma data).
- **Luma DC elements (head bit 3 set):** 16, in Z order, each the wiki's element (unary prefix `n`;
  `n < 4` → value `n`; otherwise `(1 << (n-3)) + bits(n-3) + 2`; then a sign bit if the value is non-zero).
  No separator bits.
- **Chroma tail `1010`** (4 bits) = no coded chroma. The chroma section is `X0 X1 …`: `X0 = 1`:
  U absent, then two bits, then V elements; `X0 = 0`: four U elements, three bits, four V elements
  (each a DC element; meaning of the in-between bits open).
- **A macroblock without luma DCs copies its left neighbour's luma DC level** (tested by feeding
  macroblock 0 with level +10, +20, -20, +50 followed by two `1100 1010` macroblocks: Y = 148,
  168, 88, 228 in all three). DC prediction from the left is confirmed; vertical prediction and the
  median rule are untested.
- Other heads (`0…`, `10…`) start luma DCs after a CBP-like code; for all-zero tails the header is
  12 bits and luma elements come in four groups of four with one bit between groups. Not yet decoded.

## P8 — frame flags, a rich test keyframe, and the luma CBP nibble `[probe:P8]`
- **Frame flag bit 13 (`0x2000`) is required**: with flags 0, `0x4000`, `0x8000` or `0xc000` the DLL
  fills Y=0, U=V=0 (decode failure) whatever the payload; with `0x2000`, `0x6000`, `0xa000`,
  `0xe000` or `0x2018` the same payload decodes identically. Real keyframes use `0xe000`
  (black/simple), `0xe018` and `0x2018` (detailed). Meaning of the other bits: open.
- **Rich keyframe with ground truth:** Fallout 4 `GameIntro_V3_B.bk2` frame 0 (the "PLEASE STAND BY"
  test card, flags `0x2018`, video packet 283,776 bytes, slice 1 = 147,252 bytes). Its first
  macroblock spans about 1,500 bits (`tools/realflip.py`: flips up to bit 1,425 still change the
  top-left 32x32; the slice averages 144 bytes per macroblock). Use it to validate AC grammar once
  the head grammar is known; the oracle supplies the exact planes.
- **The wiki's luma CBP code is confirmed for the head:** `0` + `1111` + a VLC bit = "no nibble
  coded" (the stream `01111` has no luma DC elements and its chroma section begins at bit 6).
  For `0 0 b2 b3 b4 b5 1 1 1 0` (first get_bit `0`, nibble-0 flag `0`, nibble value `b2..b5`,
  nibbles 1-3 skipped `111`, VLC bit `0`) the luma DC element layout depends on the nibble value
  (`tools/lab.py`, label = first luma block moved by flipping that bit):

      nibble bits b2..b5   layout of the luma elements
      0000, 0010, 0001, 0011   four groups of four with a single bit between groups, 12-bit head
      0100, 0110, 0101, 0111   16 contiguous elements starting at bit 9, no separators
      1010, 1110, 1011, 1111   16 contiguous elements
      1001, 1101               no luma elements, chroma only
      1000, 1100               (desynchronised / not decoded)

  So the presence and shape of the luma DC section is controlled by the CBP value, in line with
  the wiki's "low bits are coded bits, top bits select the alternative Huffman code".
  **Not yet understood:** the mapping, and the AC block syntax that the separator bits belong to.

## P9 — first sight of AC data `[probe:P9]`
- **Pad every probe stream with >= 1,500 zero bits.** A stream that ends while the decoder is still
  reading makes it fill later blocks with noise-like garbage (uninitialised reads); my first fuzz run
  "found" AC blocks that were really that. (`tools/fuzz_ac.py` is therefore unpadded and unreliable;
  use `flipgrid.decode` with padded streams.)
- With padding, random sparse streams give smooth, structured 8x8 blocks that vary inside the block
  (3,000 of 6,000), i.e. real AC reconstruction. Greedy bit-clearing (`tools/minimize.py`) reduces
  one to `1000100…` (bits 0 and 4 set) plus a **single `1` far down the stream**.
- Base `1000100` + zeros decodes flat. Adding one `1` at bit 30, 35, 40, … 115 (**every 5th bit**)
  makes **block 1 (Z1) and only block 1** non-flat, always with a large-amplitude textured pattern
  of similar shape. So (a) AC data sit after the DC and chroma sections of the macroblock, (b)
  with all-zero data the AC loop consumes **5 bits per step**, (c) a `1` anywhere in that run
  produces a dense block, i.e. each loop step turns zero bits into coefficients and the `1` selects
  which. Amplitudes (±130 at the centre of the block) say the dequantised levels are large for this
  quantiser.
- Not yet resolved: where exactly the AC section starts, how a block is marked coded (the head's
  `1000…` bits), the skip/level code (the wiki's KB2g skip codebook and unary level code), and the
  scan order.

## P10 — the keyframe macroblock header grammar `[probe:P10]`
Tool: `tools/lab2.py` (labels every bit: hex = flat DC change of that Z-order block, `t` = textured
luma change, `U`/`V` = textured chroma change, lower case = flat) and `tools/grammar.py` (a parser
of the grammar below that tags every bit it reads). The grammar reproduces **every** layout recorded
in P2-P9, including the all-zero 12-bit "header", the "separator" bits, the "absent groups" and the
real Fallout 4 slice. The earlier "luma DC groups with separators" were CBP nibbles and their flags.

    quantiser delta   count 0-bits until a 1 (at most 4)  → dq
                      dq == 3: dq += 1 bit;  dq == 4: dq += bits(5) + 1;  sign bit if dq != 0
    luma CBP          wiki KB2g: bit 1 → cbp 0; bit 0 → for each of 4 nibbles: flag bit,
                      flag 0 → 4 value bits (LSB first, bit i = block 4n+i in Z order)
    VLC select        1 bit (selects the AC codebook, P11)
    luma DC flag      1 bit; 1 → 16 DC elements follow (Z order), 0 → none (predicted)
    luma AC           one AC block per coded block (P11)
    U, then V         chroma CBP (wiki): bit 1 → reuse; bit 0 → 4 value bits + VLC-select bit;
                      DC flag; 1 → 4 DC elements; then the AC blocks of coded chroma blocks

- The quantiser-delta unary counts **zeros** (stop bit 1), unlike the DC element unary, which counts
  ones (stop bit 0). The wiki's KB2g quantiser rules (`== 3`, `== 4`) hold exactly.
- Real `Intro.bk2` black frame: MB0 = `1` (dq 0) `1` (cbp 0) `0` (VLC) `1` (DC flag) + 16 DCs +
  U `1 0` + V `1 0`; every other MB = `1 1 0 0 1 0 1 0`.
- Chroma DC elements: same element grammar as luma (marker U DC +3 → U = 134 at q = 16).

## P11 — intra AC block grammar `[probe:P11]`
Tool: `tools/acsearch.py` — every 1..12-bit pattern P after a head that codes luma block 0, followed by
a chroma marker (U DC +3); P is a complete AC block exactly when the marker decodes. 218 complete
patterns; all are explained by:

    repeat:  skip  = codeword (codebook chosen by the VLC-select bit)
             skip 0..10 → advance by skip; 11 → advance by bits(6); 12 → end of block
             level = count 1-bits until a 0 (prefix p), v = p + 1;
                     v >= 4 → v = (1 << (v-3)) + bits(v-3) + 2      (wiki KB2g AC formula)
             sign bit
             coefficient at scan index idx += skip, then idx += 1;  stop when idx reaches 64

- **VLC-select bit 0 uses the wiki's "AC zero run codebook 2" (KB2g), bit 1 uses "codebook 1"**, both
  read LSB first (codebook 2: `1`=0, `000`=1, `0010`=2, … `01`=12; codebook 1: `1`=0, `001`=1,
  `0000`=2, … `011`=12). End-of-block codes verified for both.
- Skip symbol 11 (+6 bits) verified for every scan index 12..63. Symbol 13 (run of 7) untested.

## P12 — scan order, IDCT and dequantisation `[probe:P12]`
- **Intra scan = the JPEG zig-zag** (0 1 8 16 9 2 3 10 17 24 32 25 18 11 4 5 …): one coefficient
  at each scan index 1-63 gives a single dominant DCT basis function (`tools/dct8.py`).
- **IDCT = the wiki's fixed-point IDCT**, rows first, then columns, final `>> 6`, with the DC term
  `dc * 8 + 32` and **grey = dc 1024** (term 8224): 63 of 63 single-coefficient blocks reproduce
  bit-exactly with a single integer coefficient value (`tools/idct.py`). Columns-first or 8216
  fails.
- AC dequantisation follows the wiki formula `((level * quant[q & 3][pos] << (q >> 2)) + 0x40) >> 7`
  with initial quantiser 16: 5,040 measurements (levels 1-8, q 8-19) are all consistent with one
  integer per (matrix, position). Ratios between the four matrices are 2^(1/4). Exact values:
  being pinned by DC-phase sweeps (`tools/qphase.py`, `tools/qsolve.py`); see P13.
- **DC dequantisation also depends on q** (correcting P6/P7, which only measured q = 16): one DC
  level moves the DC term X by 64 at q = 12 and ~76.2, ~90.75, ~107.9 at q = 13, 14, 15, i.e.
  `quant[q & 3][0] << (q >> 2) / 128` dc units with quant[0][0] ≈ 128.

## P13 — intra dequantisation, exact `[probe:P13]`
Tables: `docs/spec/quant-tables.txt` (generated from `oracle/out/quant_table_final.pkl`).
Validated: **33,332 single-coefficient blocks, 0 mismatches** (`tools/qvalidate.py` style check:
every pixel of every block reproduced with the IDCT of P12).

- **AC:** `v = ((|level| * quant[q & 3][pos] << (q >> 2)) + 0x40) >> 7`, then the sign of the level.
  `quant` is four 8x8 integer tables (positions in raster order). Sign-after-magnitude matters:
  flooring the signed product is off by one for negative values.
- **DC:** `v = ((|level| * DC5[q & 3] << (q >> 2)) + 0x800) >> 12`, sign after, with
  `DC5 = 4096, 4871, 5793, 6889` = round(128 * 2^(m/4) * 32). At the `>> 7` scale these are
  128, 152.2, 181.0, 215.3 — the fraction is needed (q = 27, DC level 6 gives 646, not 645). The DC
  term of the IDCT is `8 * (1024 + predicted/coded DC) + 32`.
- **Where the AC numbers come from (observation, used only to choose among measured candidates):**
  `quant[m][u][v] ≈ 204.8 * 2^(m/4) * J[u][v] / (G_u * G_v)` where `J` is the well-known JPEG
  example luminance table and `G_k` are the per-frequency gains of the wiki's integer IDCT
  (`G_0 = sqrt(2)`, `G_1 = G_7 = 1.21353`, `G_2 = G_6 = 1.34629`, `G_3 = G_5 = 1.71619`, `G_4 = sqrt(2)`).
  The formula reproduces 248 of 252 entries; the rest differ by one at a .5 boundary. **Every
  entry in the table file is measured**; the formula only pointed at which candidates to test.
- Measurement technique: one coefficient per block, the block's DC level swept (-6..6) to move the
  IDCT rounding phase; first pin the DC term X per (q, DC level) by intersecting over all 63
  positions, then require every pixel of every block to match with X fixed; finally choose (q,
  level) pairs whose dequantised values separate the remaining candidates.

## P14 — DC prediction inside a macroblock `[probe:P14]`
60 random 16-delta macroblocks (q = 16, no AC): every block's DC equals the wiki's rule — median
`min(max(A + B - C, min(A, B, C)), max(A, B, C))` of left A, top B, top-left C in the Z order; only
left or only top available → that one; first block of the slice → 1024 (grey). Cross-macroblock
neighbours and the wiki's two-predictor form: untested.

## P15 — frame flags and the two post-filters `[probe:P15]`
- Frame-flag bit 13 (`0x2000`) must be set (P8). Bit 14 (`0x4000`) set **disables filter A**, a short
  edge filter at the edges of blocks that carry AC data (about 2 pixels each side; a step of 11 across
  an edge moves the pixels by +1, +3 | -3, -1). Bits 3-4 (`0x18`) are a 2-bit **strength s of filter B**,
  a wide smoothing across block edges between DC-only blocks. Bit 15 (`0x8000`): no effect seen yet.
- Filter B on a single vertical edge between two flat blocks (step S, q = 12, `tools/` session data in
  `oracle/out/edge_resp.pkl`): applied iff `|S| < 8 * s`; identical response for all s below the
  threshold; antisymmetric (the response to -S is the response to S mirrored), so corrections are
  computed from |S| and applied towards each other. Each output pixel = `(sum w_j p_j + 16) >> 5` with
  weights summing to 32; the weight that reaches across the edge at distance 1..6 is
  13, 9, 6, 3.5, 2, 1 (/32). Two nearby edges do not superpose linearly: the kernel per position and
  the application order are still open. Horizontal edges behave the same as vertical ones.
- Flags in use (frame flags of sampled frames): Fallout 4 KB2g: `0x2018` (most), `0xe000`, `0xe018`,
  `0x20ff`; Starfield / Fallout 76 KB2j: `0x6e000`, `0x72018`, `0x6e018`, `0x76018`, `0x720ff`
  (bits 16-18 are new in KB2j).
- **KB2j container:** 4 extra bytes at offset 44 (e.g. `00 09 07 02`, `01 09 e4 87`) before the audio
  tables; the frame index follows them (AtomShop: first offset 44 + 4 + 4 * 2601 = 10452 exactly).

## P16 — DC quantiser floor, run mode, signed AC dequantisation `[probe:P16]`
Found by decoding the real keyframe of Fallout 4 `GameIntro_V3_B.bk2` with its frame flags rewritten
to `0xe000` (filters off) and comparing with the oracle macroblock by macroblock (`tools/realcmp.py`).
- **DC uses quantiser max(q, 8):** handcrafted DC steps are identical for q = 4..8 (4 per level).
- **Skip symbol 13 = 7 coded values in a row** (this one + 6 more without skip codes); 8 fails.
- **AC dequantisation is signed with floor** — `((level * quant << (q >> 2)) + 0x40) >> 7` exactly as
  the wiki writes it (negative levels are not rounded by magnitude). DC stays magnitude-then-sign.

## P17 — cross-macroblock prediction `[probe:P17]`
- **Quantiser prediction = plain median of left, top and top-left macroblock quantisers** (left or
  top alone at the edges, 16 for the first macroblock of a slice): 240 of 240 macroblocks of the real
  frame, recovered q vs coded dq (`tools/qsolve_real.py`). Not the DC-style gradient median.
- **CBP history (luma and chroma) = the left macroblock's CBP, 0 at the start of a row.**
- **DC prediction uses the frame-level 8x8 block grid** (left / top / top-left blocks, also across
  macroblock boundaries), the top row of a slice having no top neighbours.
- Slices: slice 1 = the first half of the macroblock rows (17 of 34 at 1080p), slice 2 the rest,
  each starting with fresh state.

## P18 — chroma AC matrices; first bit-exact real keyframe `[probe:P18]`
- Chroma has its own four AC matrices (`docs/spec/quant-tables.txt`), shaped like the well-known
  JPEG example chroma table divided by the same IDCT gains. Measured like luma (`tools/cqmeasure.py`,
  `cqsolve2.py`, `cqdecide.py`, `cqclass.py`); 43 high-frequency entries could not be separated by
  unclipped probes and were chosen among the measured candidates from the same cell in the other
  matrices (marked `*`).
- **Result: the whole 1920x1080 keyframe of GameIntro_V3_B decodes bit-exactly in `ref/bink2_ref.py`
  with filters off — Y, U and V, both slices, 2,040 macroblocks, 0 differing pixels.**

## P19 — post-filter A exact; filter B characterised but not yet exact `[probe:P19]`
Data: the real keyframe decoded by the oracle with flags `0x2000` (filter A only), `0xe018` (filter B
only), `0x2018` (both), against the bit-exact unfiltered decode of `ref/`.
- **Filter A (`ref/filters.py: filter_a`) — exact on Y, U and V of the whole frame.**
  For every 8x8 block edge, vertical edges first (only within the visible rows), then horizontal edges:
  `step = q0 - p0`; a side whose block has **fewer than 8 non-zero AC coefficients** moves its edge pixel
  by `(step + 2) >> 2` towards the other side (signed floor); a side with **at most 3** coefficients also
  moves the next pixel by `(step + 4) >> 3`; edges where **both** blocks have no AC are skipped (they
  are filter B's). Applies to luma and chroma alike (chroma: 540 visible rows at 1080p). P24: the
  vertical edges run on every row of the padded plane — the visible-rows limit only looked right
  because padding was not compared, and motion compensation reads the padding.
- **Filter B (strength s = frame-flag bits 3-4) — characterised, not exact** (exact form: P24). Acts on DC-only blocks
  using the IDCT DC terms `X = 8 * dc + 32` of the 3x3 neighbouring blocks (neighbours that are coded,
  outside the frame, or differ by `>= 512 * s` are replaced by the block's own X). The 1-D weights per
  distance from the neighbour's edge are 26, 18, 12, 7, 4, 2, 0, 0 (/64), and diagonal neighbours
  contribute as in a tensor product. Exact fixed-point form still open: the best separable variant
  (horizontal pass `>> 10` truncated, then vertical) mismatches 528 of 40,960 probe pixels; the per-
  position rounding bias measured from single-neighbour sweeps (`oracle/out/kernel2d.pkl`, q = 8,
  half-pixel DC steps) ranges 0.38-0.48 px. Next: fit the per-position integer weights directly (the
  weights may carry more precision than /64), or test a coefficient-domain synthesis (low-frequency AC
  derived from neighbour DC differences, then the normal IDCT).

## P20 — inter frames: block types, MOTION/INTER syntax, motion compensation `[probe:P20]`
Tools: `tools/inter.py` (two-frame probes: textured keyframe + handcrafted inter frame; `key_frame_textured2`
codes every luma and chroma block of every macroblock — later macroblocks of a row need CBP `1`, which the
`ones >= 8` mask turns into "all coded" after a 0xFFFF neighbour), `tools/lab_inter.py`, `tools/mvenc.py`.
- **Real black inter frame:** every macroblock row = `00001` + 58 x `1` (63 bits): explained below.
- **Block type:** list initially {MOTION, INTER, SKIP, INTRA} (the wiki's order); codes **`1` = index 0,
  `01` = 1, `000` = 2, `001` = 3** (the last two are not plain truncated unary); the chosen type moves **one
  place towards the front**; the list **resets at the start of every macroblock row**. Verified with all
  four types in every position.
- **SKIP:** copy of the reference macroblock (motion vector 0).
- **INTRA in an inter frame:** exactly the keyframe intra macroblock (8 bits for "nothing coded").
- **MOTION:** flag (`1` = one vector, `0` = four vectors: x0..x3 then y0..y3), then vector components from
  the wiki's MV codebook (LSB-first codes, values -7..7 + escape). **Units: half luma pixels.**
- **INTER:** MOTION data, then the residual: quantiser delta, luma CBP, VLC bit, luma DC flag, **one more
  flag that is 1 when chroma follows** (U then V sections as in intra); `11000` = no residual.
- **Luma sub-pixel interpolation (exact, 13 vectors):** 6 taps `(1, -4, 19, 19, -4, 1)`, value
  `clamp((sum + 16 - ((C + D) & 1)) >> 5)` with C, D the two centre taps — a halfway sum rounds up when
  C + D is even and down when odd (112 of 112 halfway cases). Horizontal pass first, then vertical on the
  8-bit results. (The wiki's 5-tap formula is a garbled version of this.)
- **Chroma motion:** the luma half-pixel vector is a quarter-pixel chroma vector; prediction is one
  bilinear step `((4-fx)(4-fy)A + fx(4-fy)B + (4-fx)fy C + fx fy D + 8) >> 4` (exact on U and V for all
  quarter/half combinations). The wiki's `(6A + 2B + 1) >> 3` has the right weights, wrong rounding.
- **Frame edges:** a 16x16 sub-block whose vector reaches outside the frame is not edge-clamped pixel by
  pixel (observed: right half left unshifted for +1 px at the right edge; left half fetched from 16 px to
  the right for -1 px at the left edge). Rule: P22.

## P21 — motion-vector escape and prediction, inter residual `[probe:P21]`
Tools: `tools/mvenc.py` (`code(v)` incl. escapes), `tools/mc.py`, `tools/interq.py`, `tools/interq_solve.py`.
- **MV escape:** after `0000`, count zeros up to a 1 (u), read u+4 bits (v): magnitude `(8 << u) + (v >> 1)`,
  positive when v is odd. Round-trips -40..40, 100, -257 through `read_mv_component`.
- **MV prediction (inside the frame):** per 16x16 quadrant, the plain component-wise median of the *final*
  MVs of the quadrants to the left, above and above-left — siblings in the same macroblock included (all
  deltas (2,1) give quadrant 3 = (4,2)). A one-MV macroblock predicts quadrant 0 and copies it to all four.
  Interior four-MV macroblocks of random fields: 15 of 15 fit (two of them once P22's edge rule moves
  their pixels), no other median of nearby quadrants does.
- **INTER residual:** quantiser delta, luma CBP (wiki), VLC bit, luma DC flag, chroma flag, [U, V]. Residual
  DCs are median-predicted inside the macroblock starting from 0 (no cross-macroblock prediction),
  dequantised like intra DCs (P13, floor max(q, 8)); AC in zigzag order with the inter luma matrices
  (`docs/spec/quant-tables.txt`); residual = IDCT(c0 = 8 dc + 32, AC) per pixel (P12 transform, >> 6),
  added to the prediction with clamping. Chroma residuals use the inter luma matrices — consistent with
  every chroma block of the real frame in P23, not separately measured.

## P22 — slices, MV prediction at edges, MVs that leave the frame `[probe:P22]`
Tools: `tools/sliceprobe.py`, `tools/mvprobe2.py`, `tools/mvedge.py`, `tools/mvclamp.py`, `tools/oraclecmp.py`.
- **Slice layout.** The DLL reads slice 2 from the **next 32-bit boundary after slice 1's last
  macroblock**; the packet's offset word is not consulted (0, 9999 and a wrong offset decode the same;
  padding between the slices breaks slice 2). Real files put slice 2 exactly there, and their offset
  words name it (and slices 3 and 4): every frame of the 54 game files, 74,680 frames (`bk2cmp bench`,
  2026-11-13). The threaded decoder starts slices there and checks each start afterwards.
- **Split row:** `mbh // 2` for 4 or more padded macroblock rows, a single slice for 1-3 rows (heights 96,
  97, 100, 120, 127, 128, 129, 150, 160, 190, 200, 230, 250 probed). The earlier "first half" reading
  ((mbh + 1) // 2) agreed only for even mbh — a 720p file splits at row 11, not 12. Consequence: probes
  that coded all rows of a 4-row frame in slice 1 were misread from row 2 on (the P21 4x4 MV fields).
- **MV prediction at the top of a slice:** the missing top pair is replaced by the quadrant two to the
  left and the one below-left: Q0 = median(L1, L0, L3), Q1 = median(Q0, L1, Q2) — so quadrant 2 is
  evaluated before quadrant 1 there (Q2, Q3 follow the inside rule). 70 of 70 four-MV macroblocks with
  every alternative of three nearby quadrants tried; unique.
- **Left frame column (transposed; confirmed by the random fields below):** Q0 = median(T2, T0, T3),
  Q2 = median(Q0, T2, Q1). **The slice's
  first macroblock:** Q0 = 0, Q1 = Q2 = Q0 (14 of 14). SKIP keeps MV 0.
- **MVs leaving the frame** (per 16x16 luma quadrant, per axis; s = source start = x0 + (mv >> 1)):
  whole-pixel: valid `0 <= s <= N-16`, else `s = N-16` (an unsigned compare: negative s lands at the far
  side); half-pixel: valid `2 <= s <= N-20`, else a **plain copy** (no filter on that axis) from 2 when
  s is 0 or 1, otherwise from N-21. The other axis keeps its filter. Chroma is predicted with the
  adjusted luma MV; MV prediction keeps the coded MV. **N:** width = the padded width (70- and 80-wide
  frames behave as 96), height = the visible height rounded up to 16 (70 -> 80, 80 -> 80, 96 -> 96).
  1080p therefore uses 1088 and zero MVs are never adjusted.
- Random fields with the rule: 384 of 384 and 720 of 720 quadrants (`mvprobe2.py`, the P21 guess 162 and
  463); every inter frame of the P22 probe files bit-exact on Y, U and V (`oraclecmp.py`).

## P23 — intra macroblocks in inter frames, residual contexts; first bit-exact real P-frame `[probe:P23]`
Tools: `tools/intrainter.py`, `tools/nbdc.py`, `tools/rqguide.py`, `tools/oraclecmp.py`.
- **The DC a non-intra block offers an intra neighbour:** for each of its 8 columns the 8 pixels are halved
  pairwise three times with round-half-up, `(a + b + 1) >> 1`, and the 8 column results are summed
  (8 x mean in DC units). Not the pixel mean: a vertical cosine (zigzag 2) adds +8 at every amplitude, a
  horizontal one nothing. Fitted on 192 cases (controlled single coefficients, all 63 positions, random
  textures; a search over every order/rounding of the six halving steps finds this one alone), then
  12 of 12 fresh textured keyframes and every intra macroblock of the real frame below.
- **Quantisers:** an inter frame keeps an intra-quantiser map and a residual-quantiser map. Every
  macroblock holds a value in both: the one it codes, and for the other kind **the prediction it would
  have used** (median of left / top / top-left in that map, P17 edges, 16 at the slice start). So an
  intra macroblock among INTER ones predicts 16, one below another intra macroblock predicts from it.
- **CBP histories:** intra and residual luma CBP histories are separate; a macroblock that codes none of a
  kind holds its left neighbour's (in effect the last coded one to the left in the row, 0 at the start).
  Residual chroma CBP: left macroblock only, nothing held (holding breaks the real frame at (24,0)).
  Intra chroma: left only (holding is indistinguishable on the data so far).
- **An intra macroblock leaves its predicted MVs** for later MV prediction (per quadrant, P22 order).
- **Real P-frame:** GameIntro frame 1 with flags rewritten to 0xe000 (filters off), decoded from the
  oracle's frame 0: **all 2,040 macroblocks bit-exact on Y, U and V** (2,008 INTER). Method that found
  the quantiser rule: decode each INTER residual for every candidate prediction from the same bit
  position and keep the ones that reproduce the oracle (`rqguide.py`): 23 misses under a "running left
  value" model, 0 under the held-prediction model.
- **Open — extreme values:** DC elements with unary prefix 11 (|level| 513), residual quantiser 36, and
  keyframe AC level 30 at high frequencies (zigzag 49, 61) give oracle output our arithmetic does not
  reproduce (looks like overflow or saturation of an intermediate): probe files `ip6`, `inb`, `iq3`
  (frames 71, 79), two `nb5` keyframes. Nothing of the kind in the real frames so far. *(P27: the DC
  cases and `iq3` are solved; the `nb5` keyframe AC case is still open.)*

## P24 — filter B exact; keyframes as shipped bit-exact `[probe:P24]`
Tools: `tools/fbdata.py` (DC-only keyframes from random or explicit DC grids, `check` mode),
`tools/oraclecmp.py`.
- **1-D step (in DC units, 8 per pixel):** `v = c + ((W[x] * (L - c)) >> 7) + ((W[7 - x] * (R - c)) >> 7)`
  with **W = 52, 36, 24, 14, 8, 4, 1, 0** (/128) — each neighbour's correction floored on its own. Found
  in stages: one-sided stripes fit `(a X_L + (64 - a) X_C) >> 12` exactly per column; two-sided ones only
  with separately floored terms in DC units and weights in 128ths (column 1's right weight is 1/128).
- **2-D:** the 1-D step on each of the three block rows (above, own, below), then the 1-D step vertically
  on the three results; pixel = `(v + 4) >> 3`, clamped. Vertical-first misses 4 % of pixels.
  Stripe frames give perfectly column-constant output, as this form predicts.
- **Neighbours:** usable when inside the plane's (padded) block grid, DC-only, and `|dc - c| < 64 s`.
  An unusable left/right neighbour becomes the block's own DC; an unusable entry in the row above or
  below takes the middle row's value in its column (so a row outside the frame copies the own row, an
  unusable diagonal its left/right neighbour). Every other substitution tried left errors at diagonals.
- Exact on 950 probe keyframes (s = 1, 2, 3; random DCs, stripes, one-sided pairs, thresholds, frame
  borders) and on the real 1080p keyframe with flags 0xe018 (Y, U, V).
- **Order:** filter B, then filter A. The real keyframe is bit-exact on full planes (padding included)
  with flags 0x2018, 0x2008, 0x2000, 0xe018 and 0xe000 — keyframes now decode as shipped.

## P25 — the post-filters in inter frames; real frames as shipped bit-exact `[probe:P25]`
Tools: `tools/filtinter.py`, `tools/oraclecmp.py` (`--oracle-out`, `--chain`); real data: GameIntro with
its frame flags rewritten to 0x2000 (A only) and 0xe018 (B only).
- **In-loop:** inter frames predict from the filtered previous frame (block interiors: 485 differing pixels
  from the filtered reference, 7,589 from the unfiltered one).
- **Filter B** acts only on intra DC-only blocks, with intra DC-only neighbours (B-only real frame: exact).
- **Filter A, residual stage:** while an INTER macroblock is reconstructed, its residual (integer,
  before the prediction is added) is filtered on the macroblock's internal edges with the filter A
  formula and coefficient-count limits; two blocks without AC only when their residual DCs differ by
  more than 32 (probe: |dDC| 32 never, 33 and up always; real frame: 880 of 880 and 108 of 108).
- **Filter A, frame stage** (vertical edges, then horizontal, after filter B): intra|intra edges as in a
  keyframe; intra|non-intra edges move only the intra side (by its own counts); non-intra|non-intra edges
  get the full formula on both sides when the two blocks' MVs differ by 2 or more half pixels in either
  component, else nothing (macroblock edges: 6,202 rows with |dMV| = 1 unchanged, every row with
  |dMV| >= 2 filtered). Luma blocks take their quadrant's MV, chroma blocks the co-located quadrant's;
  SKIP and MOTION blocks take their MVs (SKIP = 0); intra blocks never use the MV test.
- **Result:** GameIntro (1920x1080, flags 0x2018) frames 0-5 bit-exact on full planes, decoded from the
  oracle's previous frames and chained from our own; A-only and B-only variants of frame 1 exact
  (frames 6-59 needed P26).

## P26 — rules found by decoding real files at length `[probe:P26]`
Tools: `tools/coverage.py` (oracle + chained decode of the first N frames of game files symlinked into
`probes/files/`), `tools/oraclecmp.py`. Method: find the first macroblock whose block interiors (untouched
by the post-filters) differ, then vary one context at a time.
- **Residual chroma CBP history is held by INTRA macroblocks**: an INTER macroblock after an intra one
  reads the chroma CBP of the last INTER to its left (GameIntro frame 11). *(P28: MOTION and SKIP hold it
  too; only INTER without chroma resets it.)*
- **Intra chroma CBP history is held by non-intra macroblocks**, like the luma one (GameIntro frame 16).
- **Luma motion compensation, both components half-pel:** the horizontal pass is rounded but **not
  clamped** before the vertical pass (a 256 there; GameIntro frame 58, one pixel).
- **Intra DCs are clamped to 0..2047** after prediction + delta (a dark block came out at -2; Fallout 4
  `Intro.bk2` frame 19, quantiser 0).
- Also confirmed: the intra-quantiser map holds predictions (holding 16 instead breaks frames 2, 7, 8,
  10, 11, 13); SKIP blocks offer the column-cascade DC of their pixels, not a DC inherited from the
  reference frame (no effect on any file).
- GameIntro frames 0-59 bit-exact chained (full planes).

## P27 — value ranges, found on the S.P.E.C.I.A.L. videos `[probe:P27]`
Tools: `bk2cmp check` / `bk2cmp mb` (Rust: one macroblock's syntax, prediction, our residual and the
oracle's, i.e. oracle pixels minus our prediction), a search for the single dequantised coefficient
change that reproduces an oracle block exactly.
- **Residual DCs are clamped to -1023..1023** after prediction + delta, and the clamped value is what
  later blocks predict from. Fallout 4 `LUCK` frame 1, MB (19,10): DC 1040 -> 1023, the error running
  down the right block column through the prediction; `ENDURANCE` frame 3, MB (18,20): -1100 lands at
  -1023, not -1024 — in each of the four affected blocks the only single-coefficient change that
  reproduces the oracle is +8 on coefficient 0 (= +1 DC).
- **A DC element whose unary prefix reaches 11 reads one more bit after the 11th one, and ignores it.**
  `STRENGTH` frame 1, MB (43,4): the oracle's residual DCs are ours shifted by one element (exact on 15
  of 16 blocks), its 16th element is the first AC coefficient's bits (`1 11110 00 1` read as a DC
  element = -6, matching), and its block-0 AC starts one coefficient later — the same total length.
  The P23 extreme probes `ip6`, `inb`, `iq3` become bit-exact. Rejected: the bit as a 12th prefix bit
  (breaks `ip6`/`inb`, where it is often 1); 9 value bits at prefix 11 (breaks real files).
- AC level prefix 12 (the analogous cap): reading an extra bit there changes nothing in any file or
  probe — no data reaches it yet; left as it is.
- **Result:** the 14 Fallout 4 files bit-exact on their first 30 frames (chained, full planes).

## P28 — DC multipliers, residual chroma CBP history; whole Fallout 4 files `[probe:P28]`
Tools: `tools/dcsweep.py` (DC dequantisation sweep: keyframes of one macroblock row, each macroblock
its own quantiser and one DC level, block 0's DC pinned exactly through the IDCT rounding of two known
AC coefficients; positioning macroblocks at q = 8 move the DC to the low end first, so deltas up to
~1,900 fit inside the 0..2047 clamp), `bk2cmp coverage all` (oracle plane hashes, `fnv` mode),
`bk2cmp check --nofilter --tol N`, `bk2cmp mb` with chroma, `bk2cmp qstat`.
- **DC multiplier:** `dequant = (|level| * D[q] + 512) >> 10`, sign after (P13), with
  `D[q] = ((R[q & 3] << (q >> 2)) + 256) >> 9` and `R = round(2^(m/4) * 2^19) = 524288, 623487, 741455,
  881744` — i.e. 4 x round(2^(q/4) * 1024) computed from 19-bit fixed-point roots. 7,587 measured
  (q, level) pairs over q 0..36 (q < 8 = 8, P16), 0 misses; P13's `DC_QUANT[q & 3] << (q >> 2)` misses 40
  (q = 14, 15, 18, 19, 22, 23 at large levels; P13 swept levels -6..6 only). The rejected candidates and
  their misses: exact `round(2^(q/4) * 4096)` 63, `4 * round(2^(q/4) * 1024)` from exact roots 1 (q = 19,
  level 60: 1615, reproduced from a second chain position), half/eighth grids 55/223. Found from the
  Fallout 76 `BGSLogo4k` keyframe (q 2..15): one block at q = 14, level 43 one too high, shifting its
  neighbours through the DC prediction; keyframe exact with the rule.
- **Residual chroma CBP history is held by every non-INTER macroblock** (INTRA, MOTION, SKIP), like the
  residual luma one; only INTER without chroma resets it. `MainMenuLoop` frame 142: an INTER macroblock
  after two MOTION ones read its V CBP from the INTER before them (desync at the next macroblock
  otherwise). P26's "MOTION and SKIP reset it" was wrong; holding by MOTION only fails at frame 307.
- **Result:** whole files bit-exact (chained, every frame, full planes): `AGILITY`, `CHARISMA`,
  `ENDURANCE`, `INTELLIGENCE`, `Intro`, `LUCK`, `MainMenuLoop` (4,028 frames), `PERCEPTION`, `STRENGTH`.

## P29 — the filter-B threshold is the flags' low byte; every KB2g game file exact `[probe:P29]`
Tools: `tools/fbthr.py` (DC-only keyframes, two DC stripes a | a + delta, delta 4..1020, filter A off,
one file per low byte), `fbdata.collect_targets(..., flags=)`.
- **Filter B uses a neighbour whose DC differs by less than 8 x (flags & 0xFF)** (DC units; the low
  byte in pixels), and runs whenever that byte is non-zero. P24's "64 x strength (bits 3-4)" is the case
  of bits 0-2 and 5-7 clear. Measured thresholds: low byte 0x01 -> 8, 0x04 -> 32, 0x07 -> 56, 0x08 -> 64,
  0x10 -> 128, 0x18 -> 192, 0x19 -> 200, 0x1a -> 208, 0x1c -> 224, 0x1f -> 248, 0x38 -> 448, 0x58 -> 704
  (0x98, 0xf8, 0xff beyond the probed 1020); all 15 files (3,825 frames) bit-exact with the rule.
  Weights and everything else of P24 unchanged.
- Real frames with flags `0x20ff` (threshold 2040): `GameIntro_V3_B` 226-227, the `Endgame_*` files,
  Fallout 76 `BGSLogo4k` — all exact with the rule.
- **Result: every KB2g game file available is bit-exact on every frame**, chained, full planes: the 14
  Fallout 4 videos and Fallout 76 `BGSLogo4k` (45,854 frames; `bk2cmp coverage all`, ~3.5 minutes).
- Open (extreme range): DC sweep frames with q >= 37 fail (`dcsall` 159, `dcswide` 219-220). There the
  smallest AC level of the readout dequantises above 5,000, so the IDCT is in the unprobed range (P23);
  no real file comes near.

## P30 — KB2j `[probe:P30]`
Oracle: Starfield's `bink2w64.dll` (Fallout 4's answers "Not a Bink file" for KB2j); on KB2g it gives
hashes identical to Fallout 4's DLL (GameIntro 262 frames, Endgame_MALE_A 1,500). Its frame-buffer
record differs (P1c): 16-byte plane records `{u64 pointer, u32 present, u32 pitch}`, five per set
(stride 0x50), buffers already allocated, pitch padded to 64 bytes — `oracle.c` detects the layout by the
word at +0x1c and decodes into the DLL's own buffers. Tools: `tools/kb2j.py` (KB2j probe files: two
slices, extra header word 0x02070000), `bk2cmp mb` (MOTION/SKIP traced too), `examples/mvstat.rs`.
- **Slices:** 4 when the file header's video flags have bit 1 (0x12: all Starfield files and Fallout 76's
  4K `MainMenuLoop`; their extra header word is 0x87e40901 — which of the two decides is not separated),
  else 2. The packet starts with that many words (flags, then the offsets of slices 2..n), the bitstream
  after them; slices still start at the next 32-bit boundary. Four slices take the rows as evenly as
  possible, the first slices the extra rows (34 rows: 9, 9, 8, 8; 45: 12, 11, 11, 11; 68: 17 each).
- **Luma CBP:** no "duplicate into the upper half" bit when the CBP is 0 (chroma keeps it). Found on the
  Starfield logo's first macroblock (a 10-one DC element then parses as 256); the 4K keyframe (8,160
  macroblocks) exact with it, broken by the chroma variant.
- **Residual DCs** reach at least +-1280 (no P27 clamp at +-1023; 1440p files, a level 80 at q 16); the
  decoder uses +-2047, the bound is open.
- **Frame flag bit 17: MVs that leave the frame are clamped**, the half pel kept: the source start s goes
  to 0..n-16 (whole pel) or 2..n-19 (half pel — exactly where the 6-tap filter stays inside), not P22's
  rule. Measured per quadrant on random edge MVs (the effective MV that reproduces each oracle quadrant:
  s 1 -> 2 at half pel, 113 -> 112 whole, 80/81 -> 77 half for n = 96).
- **Frame flag bit 18: no in-loop residual filter** (P25's filter A on INTER residuals). Bits 17 and 18
  separated with flags 0x02000 / 0x22000 / 0x42000 / 0x62000 on the same streams; real KB2j frames set
  both. Fallout 4's DLL ignores both in KB2g files, so they apply to KB2j only.
- Unchanged in KB2j (probe-confirmed): element syntax otherwise, MV prediction, chroma, intra in inter.
- **Result:** Fallout 76 `AtomShop` (2,600 frames) bit-exact; every other KB2j file (Starfield 36, Fallout
  76 `Intro` and 4K `MainMenuLoop`) bit-exact up to its first frame with **flag bit 16**, still open: with
  filter A on in either direction it changes the macroblock syntax from the start (single macroblocks of
  every type and random frames with filter A off decode as without it; with filter A on even a lone SKIP
  differs and random frames make the DLL abort with 0xc0000409). Real bit-16 frames: 0x76018, 0x72018,
  0x7a018.

## P31 — filter A per direction `[probe:P31]`
- **Frame flag bit 14 turns filter A off on vertical edges, bit 15 on horizontal edges** — both stages
  (the in-loop residual filter and the frame-level one). Real files set both or neither, which looked
  like one switch; random frames with only one of them set differ exactly on the other direction's edge
  rows (two pixels either side). Fallout 4's DLL, KB2g: 0x2000 / 0x6000 / 0xa000 / 0xe000 all bit-exact.

## P32 — the filter line map of KB2j bit-16 frames `[probe:P32]`
Tools: `tools/kb2j.py linemap` (all-INTRA inter frames: every edge visibly filtered or not),
`tools/kb2j_maprd.py` (a candidate map + k zero bits; the k for which every edge row is either the
filtered or the unfiltered version gives the map's length and its decoded lines),
`tools/kb2j_mapfit.py` (run length field width and shortest run at the start of a map),
`tools/kb2j_mapbits.py` (what each bit position of a map does), `tools/kb2j_mapsize.py` (lines per map
when the frame size is not a multiple of 16), and `bk2cmp maps` (a real frame's maps recovered from the
oracle's output, see the method notes).
- **A KB2j frame with flag bit 16 — keyframe or inter frame — starts (slice 1 only) with a map of the
  block lines filter A runs on:** first the horizontal-edge part (one entry per luma block row) if
  horizontal filtering is on (bit 15 clear), then the vertical part (per block column) if vertical
  filtering is on (bit 14 clear). The zero prefix that makes an all-SKIP frame an exact copy is
  4 x (columns + rows) bits with both on, 4 x rows (horizontal only) or 4 x columns (vertical only), for
  1..6 macroblocks per side; a two-slice frame needs it in slice 1 only. (Without the map the DLL reads
  macroblocks as map data; random frames make it abort.) Real keyframes carry it too: the Starfield
  PowerVision files' bit-16 keyframes (frames 261, 283..286) decode only with it.
- **Lines per part (n):** P22's motion bounds in 8-pixel lines — the height rounded up to 16 (horizontal
  part), the width padded to whole macroblocks (vertical part). An all-INTRA frame with a mode-0 map of
  n - 1 zero flags keeps its macroblocks only for the right n: width 40 -> 8 (not 5 or 6), width
  72 -> 12 (not 9, 10 or 16), height 40 -> 6 (not 5 or 8), height 72 -> 10 (not 9, 12 or 16). 1080-line
  files have 136 horizontal lines: with 135, the last run of `fo76_Intro` frame 355 and
  `sf_BGS_LOGO_1080p_BinkVersion` frame 45 overruns by one line.
- **Part, mode 0** (first bit 0): one flag per inner line (1..n-1), **set = that line is not
  filtered**. Chroma line i follows luma line 2i. Exact on all-INTRA frames with no line, every line,
  line 2 and line 4 skipped.
- **Part, mode 1** (first bit 1): items until lines 1..n-1 are all coded. Item bit 0: an explicit group
  of the next 5 lines (fewer at the end), one flag each. Item bit 1: a run of equal lines — a value bit,
  then a w-bit length c (first bit least significant); the run covers b + c lines, where **w is the bit
  length of the lines remaining (this one included) clamped to 4..5, and b = w + 2**.
  - The line after a run has the opposite value and is not coded: it opens the next item, a group
    coding only its other 4 lines, a run having no value bit and covering b - 1 + c lines counting it.
  - **Unless c is all ones** (a longest run): then the next item is coded in full, and a run may go on
    with the same value — `sf_EndingVision` frame 18's first 54 unfiltered columns are a run of 38
    (c = 31) and a run of 16 (c = 9), the 55th filtered.
  - **A run with no more lines left than its shortest length** (b, or b - 1 after a run) takes them all
    and has no length: 4 real maps end with a run over 6 lines with b = 6 and no length bits; one ends
    with a run over 7 lines with b = 6 and reads a 4-bit c = 1.
  - Width: probes at the start of maps of 12..32 lines (`kb2j_mapfit`, `kb2j_mapbits`) give w = 4 for
    11 and 15 lines left, 5 for 19..31 and for 16 after a run; a run after 4 explicit groups of a
    32-line map (11 lines left) has a 4-bit length, after 2 (21 left) a 5-bit one. A whole-map run of
    39+ lines cannot be coded at all (no (w, b) fits for 40..480-line maps): long runs chain via c = 31.
- Every real bit-16 map decodes to the lines `bk2cmp maps` recovers, and ends where the frame's
  macroblocks begin; all 40 KB2j game files are exact on every frame with these rules.

## P33 — the colour converter (`BinkCopyToBuffer`) `[probe:P33]`
Tools: `oracle.exe … rgb<N>:<K>` (RGB of frame K on, added for this probe), `tools/p33_colour.py`,
`examples/chroma.rs` (finds colourful frames). Oracle: Starfield's DLL, `PowerVision_SolarFlare`
frame 20 (2560x1440, `KB2j`), its planes (`yuv:20`) against its RGB for surface types 3-6.
- **Plane order** Y, Cr, Cb (as P1c): the plane after Y moves red, the last one blue.
- **Byte order:** surface types 3 and 5 write B, G, R, 0; types 4 and 6 R, G, B, 0. The fourth byte
  is 0 (no alpha plane).
- **Range and matrix:** full range, BT.601 as in JPEG: R = Y + 1.402 (Cr - 128), G = Y - 0.344136
  (Cb - 128) - 0.714136 (Cr - 128), B = Y + 1.772 (Cb - 128), clamped. A least-squares fit over the
  frame's unclamped pixels gives 1.40203, -0.71417 / -0.34419, 1.77185, Y weight 1.0000 (rms 0.41).
  A grey frame (Fallout 4 `CHARISMA` frame 600, chroma 128 everywhere) gives R = G = B = Y exactly:
  no studio-range expansion.
- **Chroma upsampling:** bilinear with the chroma samples centred between luma pixels (weights 3/4,
  1/4 per axis, the edge sample repeated). Against nearest-neighbour, co-sited and one-axis variants
  it has the lowest error (mean 0.77 / 0.33 / 1.02 on R / G / B, largest 8 against 30 for nearest).
- **Open:** the exact integer form. With the float formula the DLL's values match 27% (R), 67% (G)
  and 12% (B) of the pixels exactly and are never more than 2 apart (`examples/rgbcmp.rs` on
  `Frame::to_rgba`: mean differences 0.77 / 0.33 / 1.03); the fitted intercepts (-0.79, +0.34, -1.03)
  fit no single rounding of the float model, so the interpolation is presumably rounded before the
  matrix. On flat chroma, R - Y still splits into two values one apart that depend on Y and the
  row's parity. A sweep of handcrafted flat-colour keyframes would pin it. Fallout 4's DLL was not
  compared: none of its films carries colour (chroma 128 on every frame checked).
- `Frame::to_rgba` implements the measured converter in fixed point (`[probe:P33]`): the float
  model, within 2 of the DLL on every pixel of the frame measured.

## Method notes
- `tools/flipmap.py <hex>` flips every bit of a base payload and prints a summary per flip: shows the
  extent of a macroblock (bits beyond the last consumed one change nothing) and which bits steer
  chroma vs luma.
- `tools/exp_b.py <prefix> <k>` appends every k-bit tail to a fixed prefix; best way to learn what a
  given prefix consumes.
- Clamping hides information: Y below 0 and above 255 saturate. Use flat 128 (zero deltas) as the
  base, not black.
- Multi-slice probes: build packets with `sliceprobe.two_slice_packet(s1, s2)` (32-bit aligned, no padding
  between the slices); a frame of 4+ macroblock rows always has two slices.
- `tools/oraclecmp.py <name>` decodes every frame of a probe file with the oracle's previous frame as the
  reference and lists mismatching macroblocks per plane — the regression check for every rule.
- Side information can be read back from real frames when the rest of the frame decodes: `bk2cmp maps
  <name> <frame>` tries every map length L (the frame decoded from bit L must match the oracle on all
  pixels no block edge filter touches), then decodes with every line filtered and with none, and per
  line reports which one the oracle's pixels match. That turns each failing real frame into a
  bits-and-answer pair for the grammar.
- When a context rule is unknown but the bits after it do not depend on it (quantisers), decode once per
  candidate from the same bit position and let the oracle pick (`rqguide.py`).
