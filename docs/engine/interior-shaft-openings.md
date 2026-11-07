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
| FNV | `SLAbandonedShack03INT` | false | 0 | 0 | none reached the renderer | the census counts 5 `FXLightBeam` placements; the engine has no fog volume and no beam-named entity in the cell |
| FNV | `HooverDamIntPowerPlant01b` | false | 0 | 20, none a beam (`glass01` to `glass07`, `chemistryexperiment01`, `polymsh_extracted`) | none | the census counts 10 beam placements; none became a medium |

## What the inventory shows so far

- Only one real cell produces a converted, marked-aperture shaft: `NellisHangar1`. The census placement counts
  (`baked_beam_census`) overstate what reaches the renderer in the other two FNV cells tried.
- The converted Nellis planes are thin ribbons, so the sun slab through each is about 0.7 m thick. That, with a
  portal sun of about 1.0 and 0.01 / m haze, is why the shaft was invisible before VLM-INT-007 and is faint after it.
- Not yet inventoried: every other interior in every title. Cells to do first, from the PRD's A9 list: Skyrim
  `AbandonedShackInterior`, an Oblivion `CaveDaylight` mine, FO3 Tenpenny, FNV Repcon and Helios, FO4 Concord
  Museum, one Starfield interior.

## Next steps

- Reconcile `baked_beam_census` placements against live `FogVolume` entities per cell, so a placement that is
  counted but never spawned (or spawned and never converted) is listed rather than assumed.
- Take the day and night pair (VLM-INT-002) on each cell that has at least one marked plane or light-shaft cone,
  with a camera aimed at the slab the noon sun predicts (plane centre minus `t` times the portal sun direction).
