# #3808 — Phase 2.1 geometry-tail dissection

Research spike. Instrument: `crates/spt/examples/spt_tail.rs` (new, `--features recon`).
Corpus: FNV + FO3 + Oblivion (incl. Shivering Isles) `Meshes` archives —
**159 `.spt` files, all 159 parsed**. Full measured output:
`spt_tail_report.md` in this directory.

## Framing choice that mattered

The issue asks to "confirm or refute the two candidate markers". Testing only
the stated hypothesis would have produced "not found" and stopped. The tool
instead ranks *every* high-band value by the number of distinct files it
appears in, so the named pair has to earn its place against all comers. That
is what surfaced the actual cause.

## Finding 1 — the markers never existed (arithmetic error)

`0x4E25` was written up as 19 989 and `0x4E21` as 19 985. Both are wrong by
exactly 16.

| hex | claimed | actual |
|---|---:|---:|
| `0x4E25` | 19 989 | **20 005** |
| `0x4E21` | 19 985 | **20 001** |

19 985/19 989 appear in **0 of 159** files. 20 001/20 005 appear in **100 %**,
once per file. The blocking question was unanswerable as written for four
months because the decimals named are not in the data.

Propagation found in **5** places: the 2026-05-09 origin note, that same
file's plan item 5, `exal-trees.md` §2/§3.2/§10, plus two source comments
(`parser.rs`, `spt_tagmap.rs`). Two regression pins now cover both classes.

## Finding 2 — the tail is not a geometry section

`parser::TAG_MAX = 13_999` caps the walker. Every family found past
`tail_offset` sits immediately above it (14 000 / 15 000 / 16 000 / 18 000 /
19 000 / 20 000 / 21 000 / 22 000 bands).

**159/159 files (100 %)** carry values past `tail_offset` that the *existing*
parameter dictionary already classifies — a coherent recurring set (10001,
10003, 10004, 13000, 13002–13007), each in ~151 files, not stragglers.

Byte-exact confirmation from `trees\treecottonwoodsu.spt`, at `tail_offset + 1`:
tag `14002`, `u32` length `16`, then `"DefaultFrond.tga"` — exactly 16 bytes.
That is the already-dictionaried `SptTagKind::String` shape, and the length
prefix predicting its own payload boundary is what makes it self-validating.

**Why this is not the 2026-07-04 debunked `14007` sighting.** That one keyed
off a single value eyeballed 3 bytes off the walker's cursor with nothing to
check it against, and was correctly rejected. A declared length that matches
its string's actual length cannot be produced by reading float data at the
wrong offset. Different class of evidence, deliberately checked against the
prior negative result rather than around it.

## Finding 3 — `tail_offset` is a desync point, not a boundary

Byte shift from `tail_offset` maximising known-tag hits:

| shift | files |
|---:|---:|
| 0 | 86 |
| 1 | 36 |
| 2 | 33 |
| 3 | 4 |

**46 % need a 1–3 byte shift**, so in nearly half the corpus the walker
stopped *inside* a payload it mis-sized. `SptScene::tail_offset`'s doc
("start of the binary geometry tail") described something unsupported; now
corrected in place.

## Finding 4 — no baked geometry can be present

| metric | bytes |
|---|---:|
| smallest | 5 131 |
| mean | 6 632 |
| **largest** | **8 793** |

A mesh of N vertices at position + normal + UV costs 32 N bytes before
indices. The largest file in the corpus is under the cost of **274 vertices**
— and that is the entire file, parameter section included.

## Conclusion and consequence

`.spt` is a **procedural tree definition** — parameters, BezierSpline curves,
texture names — not a geometry container. Whether the generator is the IDV
runtime is not observable from the files and does not need to be: ByroRedux
cannot decode geometry that is not there.

`exal-trees.md` §3 explicitly asked for this outcome to be recorded rather
than silently abandoned. Done, in §2/§3/§10 plus a re-scoping note listing
three directions (generate from parameters / keep billboards / source
geometry elsewhere) — **without picking one**, since that is a design call.

Phase 2.2–2.4 as written have no layout to confirm. What *is* unblocked is
bounded parser work: the bands past `TAG_MAX` are ordinary TLV in the format
this crate already walks, so raising the cap and dictionarying 14 000–22 000
by the existing measured-modal method is ordinary work — with the desync in
finding 3 fixed first, since a walker that stops mid-payload cannot be
extended past the stop.

## Scope note

7 files, above the pipeline's 5-file gate. All one defect; 2 of the 7 are
one-line comment corrections (`parser.rs`, `spt_tagmap.rs`) found by the
SIBLING sweep after the pin was already written. Not scope creep — leaving
them would have left known-wrong values behind a test built to catch exactly
that.
