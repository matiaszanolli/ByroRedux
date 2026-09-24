# Interior godrays and sky apertures: handoff

**State on 2026-09-23 (local time), worktree based on `e12825186`: in progress.**
The goal is real, time-of-day-dependent light shafts in interiors, with outdoor
sky visible through actual windows and holes, and legacy painted rays replaced
by participating media. A controlled vertical slice and selected shipped
assets work. There is no evidence yet for every opening or every title.

The changes described here are **uncommitted**. Start with `git status --short`
before editing. `byroredux/src/app_events.rs` already had a separate user edit;
do not discard it while working on godrays.

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
