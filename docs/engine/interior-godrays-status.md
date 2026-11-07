# Interior godrays and sky apertures: handoff

**State on 2026-09-23 (local time), worktree based on `e12825186`: in progress.**
The goal is real, time-of-day-dependent light shafts in interiors, with outdoor
sky visible through actual windows and holes, and legacy painted rays replaced
by participating media. A controlled vertical slice and selected shipped
assets work. There is no evidence yet for every opening or every title.

The changes described here shipped in `0572bfd5a` ("Implement interior godrays
and sky apertures"); later commits have refined them (for example #4902 / #4909
/ #4915's single canonical outdoor sky and sun for interiors), so check `git log`
on the files named below before trusting a detail here.

## What is implemented

- `byroredux/src/render/sky.rs` supplies an outdoor palette and live sun to an
  interior, including a procedural fallback for direct `--cell` boots. Interior
  surface lighting stays separate from that outdoor portal lane.
- `crates/renderer/shaders/volumetrics_inject.comp` scatters directional and
  clustered local lights in the froxel medium. Sunlight needs a clear geometry
  ray through a candidate aperture. A marked source plane can stop that ray at
  the plane when a legacy opaque shell sits immediately behind it.
- `crates/renderer/shaders/composite.frag` renders sky at Show Sky depth misses,
  bounded unmarked openings, and explicitly marked source planes. The plane
  test intersects the camera ray before the opaque surface, with a 24-BU rim
  tolerance. Its CPU upload is in `crates/renderer/src/vulkan/context/draw.rs`.
- `FogProfile::SkyAperture` in
  `crates/core/src/ecs/components/fog_volume.rs` marks a **proved source
  plane**, distinct from an ordinary `LightShaft` scattering volume. GPU
  translation in `byroredux/src/render/fog_volumes.rs` preserves the existing
  shaft behavior and sets `GpuFogVolume::profile_params.w` only for the marked
  plane. The composite and sun-portal paths use that marker, so a broad fog
  box cannot cut a broad hole through a roof. Composite currently accepts at
  most 256 source planes (12.8 KiB parameter UBO); overflow and performance
  under dense openings are not characterized.
- At cell import, `byroredux/src/cell_loader/spawn/mesh_instance.rs` can replace
  specific painted mesh submeshes before texture upload, raster entity creation,
  and BLAS building. Geometry-checked converters in `byroredux/src/fog.rs`
  cover Oblivion dungeon cards, common FO3/FNV cone fans, Repcon cones, Nellis
  hangar fans, FO4 ambient lamp/window beams, and selected passive effects.
  These are asset-specific conversions, not a general claim about every beam.
- Nellis's `NVNellisHangarInteriorLightBeam.nif` is split into four media and
  four bounded `SkyAperture` source planes per placed asset. FO4's
  `WindowLightBeam.nif` remains an **unmarked** scattering box: its 264-vertex,
  zero-thickness painted card spans the effect, not a proven opening plane.

## Evidence collected

| Check | Result and limit |
| --- | --- |
| `cargo test -q -p byroredux-core` | 771 unit tests plus 3 additional tests passed after the `SkyAperture` change. |
| `cargo test -q -p byroredux-renderer` | 1,176 passed, 1 ignored; fixed-array UBO reflection and source-marker tests passed. |
| Rust 1.96 binary-crate test command below | 2,375 passed, 45 ignored. The shipped Nellis asset contract also passed with `BYRO_FNV_MESHES_BSA` set. |
| `docs/smoke-tests/interior-godrays.sh` on lavapipe | Open room versus sealed room: shaft center delta 43.2 RGB, side delta 6.7; opening sky 159.2 versus roof 12.1. This proves the controlled scene, not game-wide coverage. |
| Headless Nellis Hangar capture | Outdoor sky appears along the authored roof seam after the marker change; Vulkan validation emitted no errors. At 320×180 the narrow seam looks dotted, so visual quality remains open. |
| Headless FO4 Concord Museum capture | Visually unchanged after restricting the sky mask to marked planes. This camera view does not prove all FO4 windows or beams. |

The current captures are transient files under `/tmp`:
`byro-godray-smoke.EXRhvH/{open,sealed}.png`,
`byro-nellis-marked-noon.png`, and `byro-godray-fo4-marked.png`. Reproduce them
for durable comparisons. `cargo fmt --all --check` reports extensive unrelated
formatting drift in the current repository; `git diff --check` passed.

The binary crate requires the installed Rust 1.96 toolchain; default distro
Rust 1.93.1 cannot resolve its current `cranelift` dependency:

```bash
TC=$(rustup which --toolchain 1.96.0 cargo)
PATH="$(dirname "$TC"):$PATH" "$TC" test -p byroredux --bin byroredux
PATH="$(dirname "$TC"):$PATH" "$TC" build -p byroredux --bin byroredux
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
  BYRO_ALLOW_CPU_VULKAN_DEVICE=1 docs/smoke-tests/interior-godrays.sh
```

If shader source changes, rebuild the corresponding `.spv` with
`glslangValidator -V -I. <shader> -o <shader>.spv` from
`crates/renderer/shaders`, then rerun the renderer contract tests.

## Base-game asset survey, not a completion metric

`crates/plugin/examples/baked_beam_census.rs` counts ESM `STAT` paths containing
`lightbeam` or `godray` and their placements. It does not include DLC, mods,
beam assets under different names, or proof that a placement borders outdoors.
The current base-game results are:

| Game | Matching beam base forms | Interior placed refs | Other observation |
| --- | ---: | ---: | --- |
| Oblivion | 9 | 1,249 | Most sampled dungeon cards sit near local `CaveDaylight` lights; Show Sky is usually false. |
| Fallout 3 | 24 | 6,363 | Most common fan families sit near local lights; Show Sky was false for the counted interior placements. |
| Fallout: New Vegas | 30 | 3,051 | Nellis has 24 refs across the base game; Repcon and Helios are separate patterns. |
| Skyrim SE | 3 | 1 | The base ESM instead has 50 sunlight-named LIGH bases and 853 interior placements. Many are bounce or omni lights, not source-plane evidence. |

Run the census on an installed title with:

```bash
cargo run -p byroredux-plugin --example baked_beam_census -- <Game.esm> [model-filter] [cell-filter]
cargo run -p byroredux-nif --example beam_geometry_census -- <Meshes.bsa-or.ba2> [name-filter] --vertices
```

The FO4/Starfield base ESMs and DLC have not been surveyed to the same level.
Skyrim's sunlight LIGH placements should be investigated by light type and
actual architectural opening before using them as portals. A light named
"Sunlight" is evidence of intended illumination, not proof that sky is
visible at that position.

## 2026-11-07 findings (VLM-INT, #6727, #6730)

- **The lab had gone stale, not the engine.** `interior-godrays.sh` failed (centre delta 0.0, 43.2 when written)
  because `--godray-lab` relied on two things the engine stopped supplying to a cell-less harness: the froxel
  pass's open-sky gate `InteriorSkyExposureRes` (#4902 removed the render-loop fallback) and the portal sun's
  `weather.sunlight_color` (#4909). The lab now sets both and passes again (centre delta 45.6, side 22.2). At a
  real weather sunlight colour (about 1, not the lab's 6) the same room reads centre delta 8.
- **A real cell, FNV `NellisHangar1`, shows no visible sun-driven shaft in the finished frame.** `sky.dump`
  now prints the portal lane: at noon the interior receives sun dir (0.378, 0.914, 0.148), radiance (1.0, 0.95,
  0.8), `show_sky=false`; at midnight the radiance is zero. `BYRO_HOUR` is not applied to an interior boot (the
  midnight frame kept a day sky); use `byro-dbg` `time.set`, and close the Paused menu if it opens over a capture.
  With a camera under the roof openings, the finished frame differs between `time.set 12` and `time.set 0` only
  in the sky patch seen through the opening (and TAA edge noise, repeat noise 1.06); no beam is visible in either.
  VLM-INT-002 therefore fails on this cell as the converters stand.
- **`render.debug volumetric_term` is not a radiance view.** It outputs `max(1 - exp(-radiance), opacity)`, so
  where a medium is dense the picture is the medium's opacity, which does not depend on lighting. The diagonal
  streaks seen there on this cell (identical at noon and midnight) are the Nellis beam media's extinction, not
  lit shafts; they say nothing about whether the sun lights them. A radiance-only view is needed to measure
  the sun-through-aperture contribution directly; until then judge a shaft in the finished frame.
- **The marked-aperture sun path works; the real cell's shafts are physically tiny.** `--godray-lab-marked` is the
  sealed lab room plus one marked `SkyAperture` plane under its opaque roof and no Show Sky flag (how a converted
  beam reaches the sun). In `render.debug volumetric_radiance` the column under it reads 40.63, identical to the
  open hole (40.63), against 0.00 for the sealed control; the finished frame reads 59.26 against 9.68. On
  `NellisHangar1` the same path gives a noon-minus-midnight radiance difference of about 0 over six cameras around
  the predicted lit region (best case 0.03% of pixels above 3 levels), with the room's haze at 0.01/m and again
  with it forced to 0.04/m. The converted planes are thin ribbons (about 8 m x 1.4 m, tilted 40 degrees from
  vertical, so the sun slab is about 0.7 m thick) and the canonical portal sun is about 1.0 where the lab uses 6:
  the shaft is real and faint, not missing. A visible one needs more density in the slab or a shaft gain, which is
  an engine choice like the exterior haze floor (VLM-HF-008).
- **Interior shaft dust (VLM-INT-007): the shaft is now visible.** Where the sun's ray is proven to pass through an
  authored aperture, the sun term scatters off an extra 0.04 / m (`INTERIOR_SHAFT_DUST_PER_METER`), sun term only,
  so nothing changes in a dark room, for the local lights, or at night. On `NellisHangar1` the shaft region (6.6%
  of the frame) reads 28.3 at noon against 21.6 at midnight in the radiance view and 46.4 against 42.1 in the
  finished frame, against repeat noise of 0.16 and 1.06. `interior-godrays.sh` (45.5) and `exterior-shafts.sh`
  still pass. The shaft is faint in the finished frame (about 9 % over its surroundings), and **dust density is not the
  lever**: 0.10 / m gives the same shaft as 0.04 / m (pose 4, radiance 28.12 against 28.3, finished +4.1 against +4.3
  levels), so the constant stays at 0.04. What bounds it is the portal sun's radiance (about 1.0) and the slab's
  thickness (0.7 m), not the haze; a stronger shaft would need a sun gain on the slab, which is an artistic choice.
- **Superseded:** the owner's earlier decision (sun-driven beams with a night floor, #6727) was made on the opacity
  misreading and a first implementation was reverted; the shaft dust achieves its intent (a shaft that follows the
  sun and is gone at night) on the correct term. The reverted patch is not in the repository.

## Open work and next experiments

1. **Build an opening inventory, not just a beam-name inventory.** In each
   representative interior, inspect depth gaps, glass, modeled window frames,
   LIGH placement and orientation, and any painted effect's source boundary.
   Keep a source plane marked only when its geometry and placement support an
   actual outdoor opening. Test both direct `--cell` boot and exterior-to-
   interior transitions at day and night.
2. **Extend asset conversion with measured geometry.** The interrupted next
   investigation was FNV `effects/FXHelios_Godray1.nif` and `...2.nif`.
   Each imports as 84 vertices / 70 triangles, effect material, peak vertex
   alpha about 0.514. Fourteen six-vertex fans run diagonally from a narrow
   ring near Y=3369 to a broad base near Y=0. These are not handled by the
   current converters. Derive a bounded oriented medium from the fan geometry,
   verify both shipped files and their cell placements, then decide separately
   whether any source ring is a real sky aperture. The raw vertex/index dump
   used for investigation is transient `/tmp/byro-helios-geometry.txt`.
3. **Validate more live cells.** Candidate checks include Skyrim's
   `AbandonedShackInterior` (Show Sky plus direct/bounce sunlight lights), an
   Oblivion mine with `CaveDaylight`, FO3 Tenpenny, FNV Repcon/Helios, FO4
   Concord Museum, and a Starfield interior. A controlled lab alone cannot
   validate the cross-game promise. Record day/night sky color, beam position,
   opaque occlusion, and missing-texture diagnostics for each.
4. **Improve the sky mask.** Nellis's narrow roof seam aliases visibly at
   320×180. Measure the aperture against actual roof geometry and compare
   multiple resolutions and temporal histories before widening it. Check
   aperture-count overflow and composite cost in dense cells. Preserve the
   rule that an unmarked fog volume cannot erase solid geometry.

Completion still means the requested behavior across Bethesda titles and
actual openings, not just passing these selected fixtures. The goal remains
active.
