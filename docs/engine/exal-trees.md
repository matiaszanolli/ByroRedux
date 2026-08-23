# EXAL — SpeedTree full rendering (branches, fronds, leaf cards)

A focused sub-document of [`exal.md`](exal.md), in the same relation to it as
[`exal-groundcover.md`](exal-groundcover.md) — EXAL owns the outdoors
environment; this document owns the **tree** stratum above ground cover:
trunks, branches, fronds, and canopy leaf cards decoded from `.spt`
(SpeedTree) content. It does not own the distant tree/object LOD ring
(`exal.md` §5, the `.bto`/`.btr` prebaked-asset system) — that already exists
and stays exactly as it is; this document is what fills the near-field ring
inside it with real geometry instead of a billboard.

**Status**: PROPOSED (2026-08-23) — no code lands from this document by
itself. Written per EX-14/15 (#2369) thread B's own instruction: get a design
authority in place before Phase 2 code, the same order ground cover's Phase 0
preceded its Phase 1.

**Current state, precisely**: `crates/spt` already ships a real `.spt` parser
(`parse_spt`, TLV walker over the parameter section, ≥ 95% FNV-corpus
acceptance) and a real *consumer* — but the consumer is deliberately just a
billboard. Every tree in every loaded cell today is one yaw-facing alpha-cutout
quad (`crates/spt/src/import/mod.rs`, `byroredux/src/systems/billboard.rs`).
That's strictly better than the treeless baseline it replaced, and it is *not*
a bug — the geometry-tail decode this document scopes was always deferred to
"Phase 2," named but not attempted, in the importer's own doc comment. This
document is that deferred design.

---

## 1. Why the billboard isn't enough, and why it's still worth keeping

A billboard is the right *fallback*, not a defect to eliminate outright:

- It's what covers a `.spt` file that decodes past the parameter section but
  still bails in the geometry tail (unknown/undictionaried tags — see §3).
- It's what covers every tree at LOD distances where full geometry would be
  wasted triangle budget for a few on-screen pixels — see §6.
- It's the existing, working, tested path. Nothing in this document proposes
  removing it; Phase 2 adds a *better* result when the geometry tail decodes
  cleanly and falls back to exactly today's behavior when it doesn't.

What it costs, uncorrected: every tree in every screenshot from this session's
showcase captures reads as "flat cardboard cutout," most visible from oblique
angles, in dense forest (many overlapping flat cards read as a scribbly mess
rather than volume), and up close (the single-quad silhouette is legible at
conversational distance). That's the concrete, observed motivation — not a
hypothetical one.

---

## 2. The substrate that already exists

No new parser architecture is required — this extends `crates/spt`, it
doesn't replace it:

- **The TLV parameter-section walker** (`crates/spt/src/{tag.rs, stream.rs,
  scene.rs, parser.rs}`) is done and stays exactly as-is. `SptScene` already
  exposes `bark_textures()`, `leaf_textures()`, `curves()` (BezierSpline wind
  curves), `tail_offset` (byte offset where the parameter section ends),
  `reached_eof`, and `unknown_tags`. Phase 2 reads past `tail_offset`; it does
  not touch anything before it.
- **Per-tree TREE-record parameters already flow through**
  (`SptImportParams`): leaf texture override (`ICON`), silhouette bounds
  (`OBND`, falling back to `MODB` bound radius, falling back to `BNAM`
  billboard size — precedence already pinned by real corpus checks, see the
  struct's own doc), wind sensitivity (`CNAM`, `Option<(f32, f32)>`), and the
  source FormID (for deterministic per-tree phase/variation seeding). All of
  these carry into Phase 2 unchanged; the geometry decode is additive to this
  existing parameter set, not a replacement for it.
- **The canonical `WindField` resource** (`byroredux/src/components.rs`,
  landed for ground cover) already exists engine-wide:
  `WindField { direction, speed, gust_amplitude, gust_frequency }`, sanitized
  once at install time (`sanitize_wind`). Trees reuse the same resource ground
  cover reads — no second wind system.
- **Auto-instancing precedent** — the renderer's existing batching path
  (referenced in the importer's own Phase 2 doc comment as #272) already
  groups repeated draws of the same mesh; per-leaf-card billboards around one
  canopy are exactly that shape (many small quads sharing one mesh/material).
- **BLAS static/skinned split precedent** (`crates/renderer/src/vulkan/
  acceleration/{blas_static,blas_skinned}.rs`) and the LOD-terrain BLAS
  exclusion precedent (`IsLodTerrain`, `terrain_lod.rs`) are the two existing
  answers to "how does new near-field geometry avoid becoming a BLAS-cost or
  RT-budget problem" — §5 picks between them rather than inventing a third.
- **The distant LOD ring** (`exal.md` §5, `.bto`/`.btr`) already owns
  everything past the near-field ring boundary. Nothing here changes that
  system or its ring-separation invariant (§5.2's "full REFRs only inside
  `radius_unload`" rule) — full tree geometry is just what now occupies the
  full-REFR side of that boundary instead of a billboard occupying it.

### The binding constraint

Same one ground cover names for itself: **the geometry-tail decode is
genuinely unstarted, not merely unpolished.** `format-notes.md`'s own log
(2026-05-09, "Geometry is in the binary tail") identifies two candidate
high-tag markers (`0x4E25` = 19989, `0x4E21` = 19985) past `tail_offset` and
observes repeated `00 00 80 3F` (`f32` `1.0`) runs consistent with
float-vector data — and stops there. No vertex/index/UV layout is confirmed.
Phase 2's first real step (§3) is exactly the kind of format-cracking work
that produced the parameter-section dictionary, aimed at the tail instead.

---

## 3. Geometry-tail decode (the actual unknown)

Not answerable from source reading — needs the same iterative
dissect-then-dictionary method `format-notes.md` already used successfully
for the parameter section, restarted at `tail_offset` instead of byte 0:

1. **Extend (or fork) the `spt_transitions`/`spt_dissect` recon tools**
   (`crates/spt`, `--features recon`) to start their scan at
   `SptScene::tail_offset` instead of the file start, over the same
   FNV/FO3/Oblivion corpus already proven to reach that offset cleanly.
2. **Confirm or refute the two candidate markers.** If `19985`/`19989` are
   real section tags (not float-data byte coincidences — `0x4E25`/`0x4E21`
   both decode as plausible `f32` fractions too, so this needs the same
   transition-distance cross-check that resolved the parameter section's
   false-tag confounders, not a single dissection), they're the anchor for
   a geometry-subsection walker mirroring the parameter TLV walker's shape.
3. **Recover a vertex layout.** SpeedTree's public SDK generations (v3–v6,
   spanning Oblivion through FNV/Skyrim's likely export era) are documented
   third-party as position + normal + UV + (branch-only) wind-weight per
   vertex, and indexed triangle lists per material/LOD tier — a plausible
   starting hypothesis to test against the byte stream, **not** a layout to
   assume without confirming stride/count fields in the actual bytes the way
   the parameter section's payload-size table was built from measured modal
   byte distances, not documentation.
4. **Separate branch/frond geometry from leaf-card geometry.** SpeedTree
   source content conventionally splits these into separate material-keyed
   sub-meshes (bark texture vs. leaf texture); `SptScene::bark_textures()`
   and `leaf_textures()` already return multiple paths per file in stream
   order, which — once the geometry tail is walked — should cross-reference
   against however many distinct geometry sub-blocks the tail contains. If
   the counts don't line up, that's a real signal the layout hypothesis in
   step 3 is wrong, not something to paper over.
5. **Acceptance gate**, mirroring the parameter-section precedent exactly:
   pick a real threshold (the parameter walker used ≥ 95% FNV-corpus
   clean-parse) once real dissection data exists to calibrate it against —
   not before. A file that doesn't clear the gate keeps using the billboard
   fallback; that's the existing, working degrade path, not a new one to
   build.

This is the one genuinely open-ended item in this whole document. Everything
in §4 onward assumes step 3 succeeds; if it doesn't (SpeedTree's tail format
turns out to need per-version handling this codebase's three source games
don't share, or turns out to be compressed/obfuscated rather than raw TLV),
this document's Phase 2/3 need re-scoping, not silent abandonment — record
that finding here rather than letting it go stale.

---

## 4. Consuming the decoded geometry

Once §3 produces real vertex/index buffers per tree:

- **Import shape**: extends `crates/spt/src/import/mod.rs`'s existing
  `SptScene → ImportedScene` adapter. Reuses `byroredux_nif::import::
  ImportedScene`/`ImportedMesh`/`ImportedNode` — the same types the
  billboard fallback already produces, so `byroredux/src/scene.rs`'s spawn
  path needs zero new code paths, exactly as the module doc for the current
  billboard path already promises for this transition.
- **Branch/frond mesh**: one `ImportedMesh` per bark-texture-keyed sub-block,
  static (no skinning — trees don't animate a skeleton), normal `Vertex`
  layout (`crates/renderer/src/vertex.rs`) — no new vertex format needed
  *unless* §6's wind response turns out to require a per-vertex sway weight
  the shader can't reconstruct procedurally (see §6's own note on this).
- **Leaf-card mesh(es)**: per-leaf-card billboards positioned around the
  canopy, keyed by `leaf_textures()`. Auto-instanced via the existing
  batching path (#272) rather than one draw call per card — a mature canopy
  can carry hundreds of cards, and that count needs to come from the actual
  decoded data (§3), not be assumed.
- **Material**: reuses the existing alpha-cutout leaf material
  (`threshold 0.5`, two-sided) the billboard path already established as
  correct for vanilla content; bark gets an opaque/two-sided-off material
  keyed by `bark_textures()`.

---

## 5. Ray-tracing boundary

Same question ground cover's own §5 asks, different answer likely needed —
tree branch/trunk geometry is comparatively low-poly per-tree (tens to low
hundreds of triangles, not grass's per-blade-times-thousands density) and
*does* want to cast real shadows and appear in reflections, unlike grass
blades where the existing design deliberately defers full RT participation.

- **Branch/frond/trunk geometry**: build a real BLAS, static-family
  (`blas_static.rs`) — same treatment ordinary static-mesh REFRs already get.
  No new acceleration-structure code needed; this is "a tree becomes a normal
  RT-participating static mesh once its geometry is known," not a new GPU
  path.
- **Leaf cards**: worth a real design decision once §3's actual card count
  per tree is known — hundreds of individual alpha-cutout quads in one BLAS
  is a legitimate scratch-memory/build-time cost question `blas_static.rs`'s
  existing capacity-aware rebuild batching (landed for the FO4 global-geometry
  work) may already answer, or may need its own capacity margin tuned once
  real numbers exist. Flagged, not decided — needs §3's data before this is
  answerable, same "don't guess a number you can measure" posture the
  ground-cover doc's own §11 open questions use.
- **LOD-ring exclusion**: full tree geometry only exists inside the same
  full-REFR ring `exal.md` §5.2 already gates ordinary statics on — a tree
  and its `.bto`/`.btr` distant-LOD proxy never coexist, by the same
  ring-separation construction ground cover's own §5 cites for its RT
  boundary, not a new invariant to build.

---

## 6. Wind response

`SptImportParams::wind: Option<(f32, f32)>` (from TREE `CNAM`) already
carries a per-tree wind-sensitivity pair through to the billboard path today,
where it "modulates its response to the shared weather `WindField`" per the
field's own doc comment — meaning some wind response already exists for the
*billboard*. Phase 2 extends the same idea to real geometry:

- **Trunk/branch sway**: a world-space vertex-shader displacement
  proportional to height-above-trunk-base, driven by the same `WindField`
  ground cover's blade shader samples (§8 of the groundcover doc) — same
  technique, different geometry, no second wind system. `CNAM`'s
  wind-sensitivity pair scales the response amplitude per tree; the source
  FormID (`SptImportParams::form_id`) seeds a per-tree phase offset the same
  way ground cover seeds a per-blade one, so a stand of the same tree species
  doesn't sway in perfect lockstep.
- **BezierSpline curves** (`SptScene::curves()`, already parsed as raw
  `(tag, text)` pairs — `parse_bezier_spline_text` per `format-notes.md`'s
  Phase 1.3 plan) are the more detailed, per-tree-authored wind-response
  curve SpeedTree's own export pipeline produced — richer than the CNAM
  pair alone. Whether the geometry-tail decode (§3) needs these curves to
  place vertices correctly (i.e., they're structural, not just animation
  data) or whether they're purely an animation-response input layered on
  top of static geometry is itself one of §3's open questions, not assumed
  here either way.
- **No new vertex attribute, provisionally**: like ground cover's blade
  bend, sway is computed procedurally from world-space vertex position
  (height above the tree's root) rather than requiring a baked per-vertex
  sway-weight attribute — avoiding any change to the shared `Vertex` layout.
  This is a *default assumption*, flagged for revisiting once §3's real
  vertex layout is known: if SpeedTree's own export format turns out to bake
  a per-vertex wind weight (plausible — third-party SDK documentation
  describes exactly this for branch geometry), reading and using that
  authored weight would likely look more correct than a purely procedural
  height-based approximation, and *would* need a vertex-format decision.
  Not decided here; recorded so it isn't silently assumed away.

---

## 7. The LOD chain

Unlike ground cover (which has no pre-existing LOD system to slot into),
trees already have one half of theirs: `exal.md` §5's `.bto`/`.btr` distant
proxy ring. This document only needs to define the *near* half:

| Tier | Content | Existing or new |
|---|---:|---|
| Near (this document, §3-§6) | Full branch/frond/trunk geometry + leaf-card canopy | New |
| Mid (this document, deferred) | Leaf cards only, branches dropped — a cheaper stand-in before the imposter kicks in | New, **not** designed here — flag as a Phase 3 sub-step once §3 lands, the same way ground cover's own tier-3 layer was sequenced before its other LOD tiers |
| Far | Today's billboard (`crates/spt/src/import/mod.rs`'s existing placeholder path) | Existing, unchanged — becomes the mid-distance tier instead of the only tier |
| Distant | `.bto`/`.btr` prebaked proxy | Existing, unchanged (`exal.md` §5) |

The mid tier is explicitly deferred rather than designed in this pass — it's
a real question (does dropping branches first look better than dropping leaf
density first?) that wants the same real-corpus/screenshot-comparison
validation ground cover's own tier distances (§11.4 of that doc) are flagged
as needing, and guessing it here would be exactly the kind of unverified
threshold this project avoids elsewhere.

---

## 8. Rollout order

Mirrors the ground-cover document's own phased structure (§9 there):

1. **Phase 2.1 — geometry-tail dissection** (§3). Research-only: recon tool
   extension, dictionary-building, layout hypothesis testing. No engine code
   changes. Exit gate: a confirmed vertex/index layout with a measured
   corpus-clean-parse rate, or a documented reason it can't be recovered from
   this codebase's three source games' export era.
2. **Phase 2.2 — branch/frond import + static BLAS** (§4, §5's static-mesh
   half). The `SptScene → ImportedScene` extension; trees render as real
   opaque geometry, casting shadows and appearing in reflections. Leaf cards
   still fall back to the existing billboard at this point — this phase is
   deliberately narrower than "full tree," to get real geometry shipped
   without also solving the leaf-card BLAS-cost question in §5.
3. **Phase 2.3 — leaf-card canopy** (§4's leaf-card half, §5's leaf-card BLAS
   decision made with real per-tree card counts in hand).
4. **Phase 2.4 — wind response** (§6). Builds on the `WindField` plumbing
   ground cover already established; can land any time after 2.2 lands real
   vertex positions to displace.
5. **Phase 3 — mid-distance LOD tier** (§7). Deferred design, sequenced last
   because it needs 2.1-2.3's real geometry to compare against, the same way
   ground cover's own Phase 3 (LOD chain) needed its scatter/blade phases to
   exist first.

Each phase, like ground cover's, plugs into the existing billboard path
without changing its public shape — a `.spt` file that doesn't clear a given
phase's acceptance gate keeps falling back exactly as far as it already does
today.

---

## 9. What stays out of scope

- **The distant LOD ring itself** (`.bto`/`.btr`) — `exal.md` §5 owns it
  entirely; this document only decides what occupies the *near* side of that
  boundary.
- **Ground cover** — `exal-groundcover.md` owns grass/scrub; this document
  stops at tree geometry, per that document's own §10 scope line.
- **Tree collision.** Whether/how a decoded trunk gets a physics collider is
  a separate question from rendering it — out of scope here, flagged for
  whoever picks that up to consult this document's §4 for the geometry shape
  once it exists.
- **Interactive tree damage / destructible foliage.** Not a Bethesda-source
  feature for the target game range; no reason to design for it.
- **Per-species procedural variation beyond what `.spt` already authors.**
  This document decodes and renders what SpeedTree's export already
  contains; it doesn't add engine-side procedural tree generation.

---

## 10. Open questions requiring real-data verification

Same posture as the ground-cover document's own §11 — not answerable from
source reading, each needs a real dissection/bench session before the phase
that depends on it:

1. **Are `19985`/`19989` real geometry-section tags, or float-data
   coincidences?** (§3.2) First question, blocks everything else.
2. **What is the actual per-vertex layout, and does it vary by source game /
   export-tool version** across Oblivion/FO3/FNV's `.spt` corpora? (§3.3)
3. **Do bark/leaf texture-path counts from the existing parameter-section
   parser line up with however many geometry sub-blocks the tail contains?**
   (§3.4) — the cheapest available cross-check once a candidate layout
   exists, before trusting it.
4. **Do BezierSpline wind curves affect geometry placement, or are they pure
   animation-response data layered on static geometry?** (§6)
5. **Does the export format bake a per-vertex wind weight**, and if so, does
   using it look meaningfully better than the procedural height-based
   approximation this document defaults to? (§6's own flagged note)
6. **What's a real per-tree leaf-card count**, to inform the BLAS-cost
   decision in §5 instead of guessing a scratch-memory margin ahead of time?
7. **Mid-distance LOD tier design** (§7) — genuinely deferred, not just
   unmeasured; needs its own follow-up pass once near-field geometry exists
   to compare degrade strategies against.
