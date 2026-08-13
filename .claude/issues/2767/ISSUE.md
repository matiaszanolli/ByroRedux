# REN-D8-NEW-01: both SVGF passes mask ALPHA_BLEND_NO_HISTORY off before comparing mesh IDs, so an opaque pixel can be matched against an alpha-blended fragment's draw index

- **Severity**: MEDIUM
- **Dimension**: 8 — Denoiser/Composite. See Cluster A.
- **Location**: `crates/renderer/shaders/svgf_temporal.comp` — the bilinear-tap predicate `if ((prevID & 0x7FFFFFFFu) != (currID & 0x7FFFFFFFu)) continue;` and the sub-pixel-motion fallback; `crates/renderer/shaders/svgf_atrous.comp` — the spatial tap rejection `if ((idQ & 0x7FFFFFFFu) != (idP & 0x7FFFFFFFu)) continue;`

## Description
Bits 0–30 carry two namespaces (ECS entity index for opaque, per-frame sorted draw index for alpha-blended), and bit 31 is the only discriminator. Both predicates mask it away and compare the low 31 bits. Both namespaces are small dense counters from overlapping ranges, so this is a **systematic aliasing condition**, not a wide-hash collision: whichever (entity id, draw index) pair coincides keeps coinciding frame after frame. The two consumers behave very differently, and the distinction is load-bearing for the severity. `svgf_temporal.comp` is currently self-limiting **by accident** — an alpha-blended pixel takes the early-out and writes history age 0, and `prevMeshIdTex` / `prevMomentsHistTex` bind the same `prev` slot, so any colliding tap carries `histAge == 0` → `invN = 1.0` → `alphaC = 1.0` → the no-history result exactly. The residue is the *mixed* bilinear case, where a colliding tap dilutes `histAge` and injects a foreign pixel's indirect at its bilinear weight. `svgf_atrous.comp` has **no such bound**: mesh-ID rejection is the only identity gate in its tap loop, and a colliding neighbour contributes at full weight. Related to CLOSED #904, #1159, #992 — the masking those added was correct when both halves of the encoding meant the same thing; `883f57cd` changed the opaque half's meaning and neither predicate was revisited.

## Evidence
`triangle.frag` — `uint meshIdBase = alphaBlendFrag ? sortedInstanceId : stableSurfaceId;` / `outMeshID = meshIdBase | (alphaBlendFrag ? 0x80000000u : 0u);`. `crates/renderer/src/vulkan/context/draw.rs` — `surface_id: draw_cmd.entity_id.wrapping_add(1)`. `crates/renderer/shaders/triangle.vert` — `fragInstanceIndex = gl_InstanceIndex;`. `crates/renderer/src/vulkan/pipeline.rs` — the non-`preserve_opaque_gbuffer` blend attachment array marks slot 3 (mesh_id) and slot 1 (normal) `overwrite`, so **every** particle, smoke, decal, fade and BSEffect draw overwrites the opaque mesh ID. Two of the à-trous filter's four guides are weak in exactly this situation: the same branch overwrites the *normal* attachment (so a camera-facing billboard passes `pow(dot, 128)` against a camera-facing wall), and alpha-blended draws never write depth (so `wZ` compares the receiver's depth against itself, ≈ 1). `crates/renderer/src/vulkan/svgf.rs::write_descriptor_sets` binds `mesh_id_views[prev]` and `moments_history[prev].view` from the same index — the basis for the `histAge == 0` argument.

Spot-checked against live code during publish: `svgf_temporal.comp:158` and `svgf_atrous.comp:194` confirmed the `& 0x7FFFFFFFu` masking exactly as described.

## Impact
Spatial leak of a transparent fragment's demodulated indirect into an unrelated opaque surface's à-trous filter, at up to a 14-render-pixel radius (`ATROUS_ITERATIONS = 3`), localized to colliding pairs and therefore **stable frame-to-frame rather than noise-like**. Secondary: accelerated temporal history decay at particle silhouettes. Visual only — no GPU or memory hazard. Affects every scene with alpha-blended content, all games.

## Related
`883f57cd`, #904, #1159, #992, #2116 (the sibling namespace bug already fixed on the caustic side), #2160 (the same collision class fixed on the CPU rigid motion-history map), REN-D13-01 (the identical predicate in `taa.comp` — fix both together).

## Suggested Fix
In all three predicates, treat *bit 31 set on the other sample* as an outright non-match instead of masking it. This cannot regress #904/#1159's motivating case: refractive glass now takes the `preserve_opaque_gbuffer` path and does not write mesh ID at all, so "a single instance toggling between opaque and blended" is no longer representable. A cheap source-order guard test in `crates/renderer/src/vulkan/svgf.rs` (same shape as `svgf_atrous_stops_on_depth_and_albedo_edges`) would pin the predicate.

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2767
