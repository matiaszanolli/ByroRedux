# #4052 — EXAL ground cover: §11.1 terrain-attribute sampling bench (gates Phase 1)

**State**: OPEN · **Labels**: enhancement renderer performance terrain-exterior 

Split from #3807 (item 1). **This is the only actionable task in the ground-cover
epic today — every other phase is downstream of it.**

Design: [`docs/engine/exal-groundcover.md`](../blob/main/docs/engine/exal-groundcover.md) §11.1.

## What to measure

The scatter pass and the blade raster pass both need terrain height, normal and
splat weights at arbitrary points. Two candidate paths:

- **Read the global vertex SSBO directly**, via a base-vertex offset carried on
  the terrain-tile record. Bakes nothing, stays automatically in lockstep with
  the terrain. Indirection cost per sample is unmeasured.
- **A baked per-cell attribute texture.** Cheaper per sample, costs memory and a
  bake step, and can drift from the terrain.

## Measure BOTH consumers, not just the scatter

§11.1 was written as a question about the scatter pass. That is the *smaller*
half (scope correction, 2026-09-06):

| Consumer | Sampling rate | Needs |
|---|---|---|
| `groundcover_scatter.comp` | once per **candidate point** | height, normal, splat weights |
| blade vertex shader | once per **vertex per accepted blade** | normal (blade orientation), terrain albedo (§12.3 coupling) |

The raster side is the larger consumer by a wide margin, and it is the one the
§4 blade-record trade turns on: store the normal and albedo per blade (roughly
doubling the record, and the blade buffer is the one structure whose size scales
with the visible population) or re-sample them in the vertex shader. A bench that
measures only the scatter will make the SSBO path look cheaper than it is.

## How

Needs a Vulkan device and real terrain — out of `cargo test` scope. Follow the
`--bench-hold` → `byro-dbg`-attach pattern in
[`docs/smoke-tests/README.md`](../blob/main/docs/smoke-tests/README.md).

Suggested worldspaces: a Skyrim tundra exterior and an FNV Mojave grid — §11.4
notes the two have very different sight lines, and sample counts scale with
visible chunk count.

## Done when

- Per-sample cost for both paths, for both consumers, on real terrain.
- A recommendation for §11.1 and a recommendation for §4's store-vs-resample.
- Numbers recorded in the design doc, replacing §11.1's open question.

## Blocks

#4054 (Phase 1 scatter) and everything downstream of it: #4055, #4056, #4057, #4058.

