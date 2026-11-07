# Interior shaft openings: inventory (VLM-INT-001)

**State on 2026-11-07.** A partial inventory: the cells below were measured in the running engine
(`entities FogVolume`, `inspect`, `sky.dump`, `render.debug volumetric_radiance`), not read from the ESM.
It is the evidence for [VLM-INT-001 / -006](../prd/volumetric-lighting.md) so far, and it records where the
base-ESM census and the live engine disagree. The design and the findings behind it are in
[Interior Godrays Status](interior-godrays-status.md).

## How an entry was made

1. Boot the cell held open: `byroredux --esm <Game.esm> --cell <Cell> --bsa … --bench-hold`, with
   `BYRO_DEBUG_SERVER=1 BYRO_DEBUG_PORT=<port>` in the environment (use a fresh port: a just-killed engine can
   leave the old one bound, and the server then reports `did not bind`).
2. `entities FogVolume` lists every fog volume; `inspect <id>` gives its profile, bounds and world transform.
3. `sky.dump` prints the portal sun and the Show Sky gate the cell's volume receives; `time.set 12` and
   `time.set 0` give the day and night pair (`BYRO_HOUR` is not applied to an interior boot).
4. `render.debug volumetric_radiance` isolates the sun-driven light; `volumetric_term` is opacity-floored and
   must not be used to judge a shaft.

## Cells

| Game | Cell | Show Sky | Marked planes | Media | Converter | Result |
|---|---|---|---|---|---|---|
| none | `--godray-lab` (lab) | set by the lab | 0, an unmarked 200 BU hole | 1 room dust box | the unmarked-hole path | `interior-godrays.sh` passes (centre delta 45.5, side 22.2) |
| none | `--godray-lab-marked` (lab) | false | 1 (200 x 200 BU under the roof) | 1 room dust box | the marked-aperture path | column 40.63 against 40.63 for an open hole, sealed control 0.00 |
| FNV | `NellisHangar1` | false | 32 `SkyAperture` planes (about 8 m x 1.4 m, tilted 40 degrees from vertical) | 32 `Homogeneous` boxes | `NVNellisHangarInteriorLightBeam` (4 panels per placed asset) | sun-driven shaft after VLM-INT-007: shaft region 28.3 at noon against 21.6 at midnight (radiance), 46.4 against 42.1 (finished) |
| FNV | `Vault11a` | false | 0 | 14 passive `Homogeneous` cones (the census counts 14 `FXLightBeamSuperWide01` placements); 0 before the loader fix | `fnv_superwide_beam_volume_from_mesh` | passive medium beside local lights, not a sky shaft |
| FNV | `SLAbandonedShack03INT` | false | 0 | 5 `Homogeneous` cones (the census counts 5); 0 before the loader fix | `authored_cone_beam_volume_from_mesh` | passive medium, no marked plane |
| FNV | `HooverDamIntPowerPlant01b` | false | 0 | 47 cones and 19 boxes; 0 cones before the loader fix | the cone converters | passive media beside local lights |

## What the inventory shows so far

- Only one real cell produces a converted, marked-aperture shaft: `NellisHangar1`. The other FNV cells' beams are
  passive cones beside local lights: the converters mark no `SkyAperture` for them, by design.
- **The census and the engine now agree, after a loader bug.** `synth_child.rs` returned a bare placement
  root for every model whose name contains `fxlight`, so no `FXLightBeam*` placement ever loaded its NIF and no
  beam converter ran in the live engine (the converters' unit tests used synthetic meshes, which is why nothing
  caught it). The branch now keeps the early return only for `fxlightrays` and for `fxlight` bases with `LIGH`
  data (`is_fxlight_sprite_placement`). Measured after the change: `Vault11a` 14 cones for 14 placements,
  `SLAbandonedShack03INT` 5 for 5. A beam card no converter claims is still dropped from the raster by the
  `IsFxMesh` texture rule, so it adds an entity and no draw.
- The converted Nellis planes are thin ribbons, so the sun slab through each is about 0.7 m thick. That, with a
  portal sun of about 1.0 and 0.01 / m haze, is why the shaft was invisible before VLM-INT-007 and is faint after it.
- Not yet inventoried: every other interior in every title. Cells to do first, from the PRD's A9 list: Skyrim
  `AbandonedShackInterior`, an Oblivion `CaveDaylight` mine, FO3 Tenpenny, FNV Repcon and Helios, FO4 Concord
  Museum, one Starfield interior.

## FNV converter coverage (VLM-INT-006), measured 2026-11-07

- **What the beams are.** `baked_beam_census` over `FalloutNV.esm`: of the roughly 3,600 baked-beam placements
  (`FXLightBeam*`, `Brt*`, `W*`, `SuperWide01`, `VaultWidow*`, `RepLightBeams`, `Helios`), every interior
  population is in a cell with Show Sky off and most sit within 256 BU of a `LIGH` (for example
  `FXLightBeamBrt04`: 557 interior placements, 472 near a light). They are fixture beams, not sun shafts. The
  one sun-driven interior in FNV is `NellisHangar1`.
- **What the converters claim.** `fnv_baked_beam_converter_coverage` (ignored, needs `BYROREDUX_FNV_DATA`) runs
  the converter chain on the real meshes: 22 of the 24 beam models it lists convert, the exceptions being
  `NVFXLightBeamStreetLight` (an exterior street light) and `FXLightBeamNVStrip` (deliberately left to the raster,
  its 81-vertex effect mesh would otherwise draw twice). By placements that is about 99 %.
- **Before the loader fix that was 0 % live**, because no beam placement loaded its NIF (the `fxlight` early
  return). So about 3,500 placements gained a passive local-light cone in one change.
- **Worst case.** `2CCampGolfResort` (286 placements, the busiest) boots with 104 fog volumes against the 512
  cap, `gpu_volumetrics_inject` 0.68 ms, no log of dropped volumes, and a finished frame without visible artifacts.
  `JacobstownLodge` (249) and `HooverDamIntOliverArea` (208) were not booted.

## Next steps

- Reconcile the remaining census placements against live `FogVolume` entities in the cells not yet booted
  (done for `Vault11a`, `SLAbandonedShack03INT`, `HooverDamIntPowerPlant01b`). `baked_beam_census` now prints
  the base model, `disabled` and `xesp` for each sample, so a placement the loader withholds is visible.
- Judge whether the new passive cones read as light shafts or as haze in a finished frame, per cell, before
  the owner reviews them: they are lit by the local light the cone sits beside, not by a sun.
- Take the day and night pair (VLM-INT-002) on each cell that has at least one marked plane or light-shaft cone,
  with a camera aimed at the slab the noon sun predicts (plane centre minus `t` times the portal sun direction).
