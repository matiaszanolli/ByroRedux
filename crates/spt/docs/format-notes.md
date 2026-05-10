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
