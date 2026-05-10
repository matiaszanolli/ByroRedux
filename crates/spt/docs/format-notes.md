# SpeedTree `.spt` Format — Observation Log

Black-box reverse-engineering notes for the SpeedTree binary container
shipped by Oblivion / Fallout 3 / Fallout New Vegas. **No SDK code, no
SDK header copying, no SDK documentation paraphrasing.** Findings here
are derived from the recon harness (`crates/spt/src/recon/`) running
over vanilla BSAs, cross-referenced against the TREE record's
sub-record layout in `crates/plugin/src/esm/records/tree.rs`.

Every claim below is dated and tied to the recon pass that produced
it. Stale claims get struck through (or removed) when a later pass
contradicts them — see project memory `feedback_audit_findings.md`
for the audit-hygiene policy this file follows.

---

## 2026-05-09 — Phase 1.2 baseline corpus sweep

Run command:
```
cargo run -p byroredux-spt --features recon --example spt_recon -- \
    "Fallout New Vegas/Data/Fallout - Meshes.bsa" \
    "Fallout 3 goty/Data/Fallout - Meshes.bsa" \
    "Oblivion/Data/Oblivion - Meshes.bsa"
```

### Per-archive corpus stats

| Game | `.spt` files | Total bytes | Min | Median | Max |
|---|---:|---:|---:|---:|---:|
| FNV | 10 | 67 730 | 5 197 | 6 810 | 8 045 |
| FO3 | 10 | (same as FNV — partial overlap; see note below) | 5 197 | 6 810 | 8 045 |
| Oblivion | 113 | 750 741 | 5 131 | 6 865 | 8 793 |

Note on the FNV/FO3 figure overlap: both games' `Fallout - Meshes.bsa`
ship 10 `.spt` each, in the same byte-size range, but the **embedded
authoring paths differ** (FO3's strings reference `C:\Hope\Fallout3\…`
and `C:\Projects\Fallout3\…`, FNV's reference `C:\Noah\Fallout\…`).
So the file *count* is coincidental, not a duplication artifact.

### File size profile

All vanilla `.spt` files cluster in the **5 KB to 9 KB** range. That's
small enough that the parser can comfortably load the whole file into
memory before parsing — same approach as the NIF parser (`NifScene`
materialises every block up-front).

### Cross-reference: TREE record count vs `.spt` file count

| Game | TREE base records | `.spt` files in BSA | Notes |
|---|---:|---:|---|
| FNV | 3 | 10 | 70 % of `.spt` files unreferenced by vanilla TREE bases (DLC stubs / unused content) |
| FO3 | 9 | 10 | ~90 % referenced |
| Oblivion | 142 | 113 | TREE > `.spt` — multiple TREE bases share `.spt` geometry, vary only by tint / scale params at the TREE-record level |

(TREE counts come from `parse_real_esm.rs` integration test output.)

---

## Magic prefix — unified across all three games

**Major finding.** Every single one of the 133 vanilla `.spt` files
across FNV / FO3 / Oblivion begins with the same 20-byte signature:

```
offset 0  : E8 03 00 00          u32 LE = 1000   (presumed format version code)
offset 4  : 0C 00 00 00          u32 LE = 12     (length of next inline string)
offset 8  : 5F 5F 49 64 76 …     12 ASCII bytes  = "__IdvSpt_02_"
```

Pinned in `crates/spt/src/version.rs::MAGIC_HEAD`.

**Implication for the parser:** there's **no need for a V4-vs-V5 code
split** at the entry point. A single dispatcher recognises every
vanilla `.spt`. Whether the body section layout downstream of the
20-byte magic diverges between Oblivion (SpeedTree 4.x era) and
FO3/FNV (SpeedTree 5.x era) is **not yet confirmed unified** —
that's the next recon pass.

The string `__IdvSpt_02_` is the SpeedTree Reference Application
identifier; presence in vanilla content is observation-only and tells
us nothing about the body bytes. The SpeedTree exporter that produced
these files appears to have been the same build across the entire
2006–2010 Bethesda release window.

---

## Embedded printable strings — what the body carries

Across the three corpora, **printable ASCII runs ≥ 8 chars** found
inside `.spt` bodies fall into three families:

### Family A — exporter authoring paths

Source-asset paths from the SpeedTree authoring environment, e.g.

- `C:\Hope\Fallout3\Bushes\\WastelandShrub01Bark.tga`
- `C:\Projects\Fallout3\Game\Data\Trees\\OasisTreeTopBark01.dds`
- `C:\Hope\Fallout3\Trees\Pine\\PineBark01.tga`
- `C:\Hope\Fallout3\Trees\SugarMaple\\SugarMapleBark01.tga`
- `C:\Hope\IDV\Azalea\\ShrubAzaleaBark.tga`
- `C:\Noah\Oblivion\Trees\ShrubGeneric\\ShrubAzaleaBark.tga`
- `C:\Noah\Fallout\Trees\WastelandShrub01\\WastelandShrub01Bark.tga`

These are **build-time scribbles** stamped into the binary by the
SpeedTree exporter. They will not be present at runtime resolution —
the runtime texture path comes from the cross-referenced TREE record's
ICON sub-record. We can ignore exporter paths during parsing (skip the
length-prefixed string instead of resolving it).

### Family B — `BezierSpline <param>` entries

Hundreds of distinct `BezierSpline <number>` runs:

- `BezierSpline 0`, `BezierSpline 1`, `BezierSpline 0.5`,
  `BezierSpline 0.25`, `BezierSpline 0.05`, …
- Negative values: `BezierSpline -90`
- Larger values: `BezierSpline 3`

These mark **animation curve definitions**. SpeedTree's wind / branch
sway / leaf flutter system is curve-driven — each `BezierSpline` is a
named keyframe-animated parameter. The body bytes immediately
following each `BezierSpline` label likely carry control-point data
(observed printable runs of `<t> <v> <tan_in_x> <tan_in_y> <weight>`
shape — see Family C).

### Family C — control-point quintets

Repeating five-number runs that look like Hermite / Bezier control
points:

- `0 0 0.707107 0.707107 0.079604`
- `1 1 0.707107 0.707107 0.107006`
- `0 1 0.714831 -0.699297 0.079604`
- `1 0.000782381 0.699609 -0.714526 0.107006`
- `0 0.5 0.89466 0.446747 0.113537`

Pattern: `<t> <value> <tangent_x> <tangent_y> <weight>` — the standard
shape for a 2D Bezier control point in animation tooling.

The presence of these as **printable ASCII** is itself a finding:
SpeedTree's exporter is serialising at least some of the body as
text-encoded floats with delimiter spaces, not pure binary IEEE 754.
That's unusual for a binary container. It suggests the format may be
either:
1. Mixed binary + length-prefixed-text-blob sections (a hybrid that
   matches the Family A authoring-path strings), or
2. A "binary container" that's mostly thin wrapping around an inner
   text serialisation — like SpeedTree's Reference Application output
   format, which is documented externally as text-mode in some
   community write-ups.

The next recon pass needs to confirm which one.

---

## Open questions for Phase 1.3

1. **Section structure.** Where does the magic header end and the
   body begin? Is the body chunked (TOC + sized sections) or
   sequential? Recon pass 2: hunt for repeated 4-byte tags after the
   magic; cross-reference against file size to confirm section offsets
   sum to file length.
2. **Text vs binary.** Family C printable runs suggest large stretches
   of body bytes are text-encoded floats. How is the textual region
   delimited? Length-prefixed string blocks (matching the Family A
   pattern), or terminator-driven (newlines / null bytes)?
3. **Geometry layout.** Branches / fronds / leaves are the visible
   primitives. Where do their vertex / index buffers live? If the body
   is mostly text, geometry might be in distinct tail-side binary
   sections.
4. **Leaf billboard layout.** TREE record's `SNAM` carries leaf
   indices. The `.spt` must carry corresponding leaf-card definitions
   keyed by index. Recon: dump bytes between Family-B `BezierSpline`
   labels and look for repeating ~32-byte structs (position + size +
   UV rect).
5. **Oblivion-vs-FO3/FNV body divergence.** Confirmed unified at the
   magic prefix. Body sections may still differ — recon pass 2
   compares section structure across the two eras.

---

## Acceptance gate (per the SpeedTree compatibility plan)

Phase 1.3 commits to a real parser only when the recon harness
partitions ≥ 95 % of FNV's `.spt` corpus into known sections (i.e.
when section count / section types / section size totals are predicted
correctly for ≥ 9 of FNV's 10 vanilla files). Below that, Phase 1
ships the **placeholder fallback**: a yaw-billboard quad textured with
the TREE record's `ICON` and sized by `BNAM`. Strictly better than
today's silent drop.

---

## 2026-05-09 (later) — Phase 1.3 prep, single-file dissection

`spt_dissect` (companion to `spt_recon`) drills into one
representative `.spt` and dumps post-magic hex + every printable
ASCII run + length-prefix string candidates. First target was
`trees\euonymusbush01.spt` (FNV, 6 757 B, magic OK). Findings.

### Body is a TLV stream of (u32 tag, payload) pairs

After the 20-byte magic, the file is a sequence of records keyed on a
4-byte little-endian tag. The payload type depends on the tag:

```
20: ea 03 00 00 d0 07 00 00 31 00 00 00 43 3a 5c 48
36: 6f 70 65 5c 46 61 6c 6c 6f 75 74 33 5c 42 75 73
52: 68 65 73 5c 5c 57 61 73 74 65 6c 61 6e 64 53 68
68: 72 75 62 30 31 42 61 72 6b 2e 74 67 61 d1 07 00
84: 00 00 80 89 44 d2 07 00 00 00 d3 07 00 00 00 00
100: c8 42 d5 07 00 00 61 99 04 00 d6 07 00 00 00 00
```

Decoded:

| Offset | u32 LE | Decoded |
|---:|---:|---|
| 20 | 1002 | tag (`0x3EA`, leading parameter, payload type unknown) |
| 24 | 2000 | tag (`0x7D0` — bark texture string follows) |
| 28 | 49 | u32 length of next inline string |
| 32 | — | 49 ASCII bytes: `C:\Hope\Fallout3\Bushes\\WastelandShrub01Bark.tga` |
| 81 | 2001 | tag (`0x7D1`) |
| 85 | f32 1100.0 | f32 payload |
| 89 | 2002 | tag (`0x7D2`) |
| 93 | f32 0.0 | f32 payload |
| 97 | 2003 | tag (`0x7D3`) |
| 101 | f32 100.0 | f32 payload |
| 105 | 2005 | tag (`0x7D5`) |
| 109 | (inline data) | (next field per tag's dispatch) |

So the runtime parser shape is:

```
loop {
    let tag = read_u32_le()?;
    match tag {
        2000 | 2010 | … => read_length_prefixed_string()?,  // texture / curve
        2001..=2007 | … => read_f32_le()?,                  // numeric params
        1016 | …        => begin_subsection(),              // nested
        _               => bail / log / skip,
    }
}
```

The full tag dictionary is what Phase 1.3 still has to enumerate by
walking the dissector across more `.spt` corpora and building the
observed-tag → payload-type map.

### Strings are u32-length + raw ASCII (no NUL terminator)

Confirmed via the length-prefix candidate scan — every string in the
file (texture path, exporter scribble, **and the BezierSpline curve
blobs**) has the same shape: `u32 LE length | length raw ASCII bytes`.
No NUL terminator. No alignment padding.

The `__IdvSpt_02_` magic itself is one of these length-prefixed
strings (offset 4: `len=12`, payload starts at offset 8).

### `BezierSpline` curves are length-prefixed *text* blobs

The Family-B `BezierSpline` runs identified in the corpus sweep are
not separate fields — they're each a single length-prefixed string
holding the entire curve as text. Sample (FNV bush, offset 142):

```
len=92, payload:
"BezierSpline 0\t1\t0\n{\n\n\t2\n\t0 0 0.707107 0.707107 0.079604\n\t1 1 0.707107 0.707107 0.107006\n\n}\n"
```

Curve format (eyeball-derived; not verified beyond pattern match):

```
BezierSpline <a>\t<b>\t<c>\n
{\n\n
    \t<num_control_points>\n
    \t<t> <v> <tan_x> <tan_y> <weight>\n
    \t<t> <v> <tan_x> <tan_y> <weight>\n
    …
    \n
}\n
```

The text-blob serialisation is unusual but consistent. It means
curve values can be parsed with `str::split_whitespace` /
`f32::parse` rather than IEEE 754 reads — handy for testing, and
rules out per-platform endian / NaN-handling drift.

### Geometry is in the binary tail

Past offset ~5060 the printable-run scan starts producing junk
characters (`?333?`, `?ff&?`, `L=.#`) — that's the binary geometry
+ leaf-billboard payload past the last text curve. File tail:

```
6693: 00 00 00 25 4e 00 00 00 00 80 3f 00 00 80 3f 00
6709: 00 00 00 00 00 80 3f 00 00 00 00 00 00 00 00 00
6725: 00 80 3f 00 00 00 00 21 4e 00 00 08 52 00 00 00
6741: 00 80 3e 09 52 00 00 cd cc cc 3e f0 55 00 00 00
```

The repeated `00 00 80 3f` = f32 `1.0` blocks suggest float-vector
data (positions / UVs / normals). Tag `0x4E25` (= 19 989) and
`0x4E21` (= 19 985) at offsets 6 696 / 6 732 fit the TLV pattern at
much higher tag values than the parameter section's `~2000` band —
likely the geometry subsection IDs.

### Updated parser plan for Phase 1.3

1. Validate the 20-byte magic (`MAGIC_HEAD` already pinned).
2. Implement a TLV-stream reader: read u32 tag, dispatch on tag.
3. Build the tag → payload-type table iteratively from corpus
   dissections (`spt_dissect` runs across additional files).
4. Curves: length-prefix string → `parse_bezier_spline_text(s: &str)`
   — pure text parser, fully unit-testable.
5. Geometry tail: deferred to a follow-up sub-phase once the TLV
   walker reaches the high-tag (19 985+) region cleanly.

Acceptance gate stays: ≥ 95 % of FNV's `.spt` corpus parses through
the TLV walker without falling into the unknown-tag bail-out.

---

## Recon harness — how to reproduce

```bash
# Build with the recon feature.
cargo build -p byroredux-spt --features recon --example spt_recon

# Run over any combination of BSAs, redirect to your scratch file.
cargo run -p byroredux-spt --features recon --example spt_recon -- \
    /path/to/Fallout - Meshes.bsa \
    /path/to/Oblivion - Meshes.bsa \
    > /tmp/spt_recon.md
```

Output is plain Markdown (a per-archive table + per-bucket string
samples) — append it under a dated heading in this file when you run
a follow-up pass.
