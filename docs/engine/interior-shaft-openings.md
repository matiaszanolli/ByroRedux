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
- **The exemption is limited to `GameKind::Fallout3NV` (#6731).** Oblivion also ships `fxlightbeam*.nif`, but its
  converter coverage and beam texture paths are unmeasured, so it keeps the synthetic path. FO3 shares the game
  kind and was measured on 2026-11-07 by running `fnv_baked_beam_converter_coverage` against FO3's
  `Fallout - Meshes.bsa`: all 22 beam models it lists that exist there convert (`NVFXLightBeamStreetLight` and
  `FXLightBeamNVStrip` are FNV-only). Widen the gate per game only after the same census.
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

## Placed beam census, every title and official DLC (VLM-INT-006 denominators), measured 2026-11-07

`cargo run --release -p byroredux-plugin --example baked_beam_census -- <master.esm>`, one master at a time,
over every official master and DLC found on this machine (Oblivion, Fallout 3 and its five DLCs, Fallout: New
Vegas and its DLCs, Skyrim SE and its three DLCs, Fallout 4 and its DLCs, Starfield and Shattered Space).
The census counts placed references whose base model path contains `lightbeam` or `godray`; Skyrim's
indoor sunlight is authored as `LIGH` placements and is counted separately.

| Master | Beam base models | Interior placements | …in a Show Sky cell | …within 256 BU of a `LIGH` | Exterior placements |
|---|---|---|---|---|---|
| `Oblivion.esm` | 9 | 1,249 | 2 | 877 | 10 |
| `Fallout3.esm` | 24 | 6,363 | 0 | 5,391 | 71 |
| FO3 DLCs (Anchorage, The Pit, Broken Steel, Point Lookout, Zeta) | 0 | 0 | 0 | 0 | 0 |
| `FalloutNV.esm` | 30 | 3,051 | 0 | 2,425 | 214 |
| `DeadMoney.esm` | 3 | 18 | 0 | 5 | 22 |
| `OldWorldBlues.esm` / `LonesomeRoad.esm` | 2 / 2 | 2 / 1 | 0 | 2 / 0 | 0 |
| Other FNV DLCs (Honest Hearts, Gun Runners' Arsenal, the five item packs) | 0 | 0 | 0 | 0 | 0 |
| `Skyrim.esm` | 3 | 1 | 0 | 0 | 0 |
| `Dawnguard.esm` | 2 | 2 | 2 | 0 | 6 |
| `Update.esm`, `HearthFires.esm`, `Dragonborn.esm` | 0 | 0 | 0 | 0 | 0 |
| `Fallout4.esm` | 21 | 1,604 | 657 | 1,578 | 359 |
| `DLCNukaWorld.esm` | 2 | 51 | 0 | 51 | 0 |
| `DLCCoast.esm` | 1 | 0 | 0 | 0 | 2 |
| Other FO4 DLCs (Robot, the three workshops, Ultra High Resolution) | 0 | 0 | 0 | 0 | 0 |
| `Starfield.esm` | 1 | 0 | 0 | 0 | 1 |
| `ShatteredSpace.esm` | 0 | 0 | 0 | 0 | 0 |

- **Fixture beams dominate every title but Fallout 4.** Oblivion, FO3 and FNV place their beams in cells with
  Show Sky off (FO3 0 of 6,363, FNV 0 of 3,051, Oblivion 2 of 1,249), most within 256 BU of a `LIGH`: fixture
  haze, as the FNV finding above says. **Fallout 4 is the exception**: 657 of its 1,604 interior placements sit in
  Show Sky cells (`WindowLightBeam` 114 of 114, `FluorescentLightBeam` 142 of 258, `LightBeamThinDustyShort02`
  130 of 480), and the busiest cell, `DBTechHighSchool01` (83), is one. Fallout 4 is the title where the
  converted-beam coverage most needs a live measurement.
- **Skyrim places almost no beam meshes.** `Skyrim.esm` has 1 interior beam
  (`FXDweCenterLightBeamObservatory.nif` in `Mzulft03`); its indoor sun is 50 sunlight `LIGH` bases placed 853
  times in interiors (e.g. `AbandonedShackInterior`, `DefaultSunlight01Closer03` and `BounceAmb01NS`), with
  Dawnguard adding 2 bases and 1 placement.
- **Starfield and its DLC place one beam**: `Activator_LightBeamTall01` in the exterior cell `POI070Ext02`. By
  this name filter Starfield has no baked interior beams; a different authoring route (not found by the
  census) is not ruled out.
- **Converter coverage, every title (2026-11-08, #6774).** `baked_beam_converter_coverage` (ignored; `BYROREDUX_BEAM_ARCHIVE`
  and `BYROREDUX_BEAM_MODELS`, the census base-model paths) runs the converter chain on the real meshes of any title:
  - **Fallout 4**: all 9 placed beam models convert their primary mesh (`EmergencyLightBeam*`, `FluorescentLightBeam*`,
    `LightBeamThinDusty*`, `WindowLightBeam`; the second mesh of `EmergencyLightBeam01` and `-Set` is not a beam), so
    about 100 % of its 1,963 placements. They never met the `fxlight` gate (no such name), so they were already live:
    `DBTechHighSchool01` (83 placements, the busiest Show-Sky beam cell) boots with 238 `FogVolume` entities.
  - **Oblivion**: of the 8 listed models 6 convert; `FXLightBeamDust01` (a mesh-less emitter NIF, 80 placements) and
    `FXLightBeamPink01` (9) do not, so 92.9 % of its 1,259 placements. The #6731 gate now includes Oblivion, and live
    `WarehouseCaveGeneric` (52 placements) boots with 66 `FogVolume` entities. Whether the new volumes read well in the
    dungeons is the owner's look.
  - **Skyrim**: 0 of 1 (`FXDweCenterLightBeamObservatory`, five meshes, none claimed); its indoor sun is `LIGH`, not beams.
  - **Starfield**: 0 of 1 (`Activator_LightBeamTall01` imports no mesh from its NIF; its geometry lives outside it).
  - Fallout 3 / FNV as above (22 of 22 and 22 of 24).

## A9 day / night results (VLM-INT-001 to -003), measured 2026-11-08 (#6727)

`docs/smoke-tests/interior-shafts-content.sh` boots each cell held open on the free camera, with the game clock
paused and the exposure fixed, and for up to four poses (`cam.tp` onto its volumes, sky apertures first; a sealed
control onto the entities nearest its spawn) captures the froxel in-scatter view at `time.set 12`, `time.set 0` and
`time.set 12` again. A pose's region is the pixels where the mean noon frame beats midnight by more than
max(3 levels, 2 x p99 of |noon - noon repeat|); a `shaft` cell passes when its best pose's region is at least 0.1 % of
the frame, a `sealed` control when no pose reaches that. The in-scatter view is used, not the finished frame, because
a window or roof hole shows a sky that differs between noon and midnight whether or not any shaft exists. The repeat
noise is large on many of these cells (p99 up to 34 levels): fixture lights flicker.

| Game | Cell | Role | Volumes (Show Sky) | Best pose: region / mean dL / max dL / noise p99 | Result |
|---|---|---|---|---|---|
| FNV | `NellisHangar1` | shaft | 32 `SkyAperture` + 32 `Homogeneous` (off) | 17.2 % / 26.5 / 54.1 / 8.6 | PASS (all four poses show a region above 0.1 %: 14.4, 5.9, 0.2, 17.2 %) |
| FNV | `HELIOSOneDeck` | shaft | 14 `Homogeneous` (off) | 0.00 % / 0 / 3.9 / 13.5 | FAIL |
| FNV | `RepconHQ01` | shaft | 87 `Homogeneous` (off) | 0.04 % / 32.0 / 40.2 / 15.0 | FAIL |
| FO3 | `Tenpenny01` | shaft | 107 `Homogeneous` (off) | 0.04 % / 84.6 / 85.2 / 26.8 | FAIL |
| Skyrim | `AbandonedShackInterior` | shaft | 6 `Homogeneous` (on, portal sun 0.51) | 0.00 % / 0 / 3.2 / 6.4 | FAIL |
| FO4 | `ConcordMuseum01` | shaft | 117 `Homogeneous` + 10 `LightShaft` (on, 0.88) | 91.5 % / 9.3 / 20.7 / 2.8 (one pose of four) | PASS |
| Oblivion | `WarehouseCaveGeneric` | shaft | 52 `Homogeneous` (off) | 0.00 % / 0 / 1.4 / 1.0 | FAIL |
| Starfield | `CityNewAtlantisLodgeInt` | shaft | none (off) | no volume to aim at | FAIL |
| FNV | `HiddenValleyBunker1` | sealed | none | 0.00 % | PASS (no medium, so vacuous) |
| Skyrim | `HillgrundsTomb01` | sealed | 86 `Homogeneous` (off) | 0.00 % (max dL 20.6 at one pose, under a noise floor of 34) | PASS |
| Oblivion | `ImperialSewers03` | sealed | 5 `Homogeneous` (off) | 0.00 % | PASS |
| FO4 | `Vault111Cryo` | sealed | 291 `Homogeneous` (off) | 0.00 % | PASS |

Pass count per game: FNV 2 of 4, FO3 0 of 1, Skyrim 1 of 2, Oblivion 1 of 2, FO4 2 of 2, Starfield 0 of 1.

What it shows:

- **Two cells show a sun-driven shaft above the noise**: `NellisHangar1` (the marked-aperture path) and
  `ConcordMuseum01` (Show Sky on, `LightShaft` cones). Every other `shaft` cell fails, and for most of them that is
  the content: the HELIOS, REPCONN, Tenpenny and `WarehouseCaveGeneric` volumes are all `Homogeneous` fixture-beam
  cones beside local lights with Show Sky off, which have no sun path to follow (see the FNV coverage notes above).
  The A9 list the handoff proposed contains cells of both kinds; the PRD's "each listed cell passes" needs the list
  narrowed to cells whose census shows a sun path, or the engine to give the others one.
- **Skyrim's `AbandonedShackInterior` is the real gap.** Show Sky is on, the portal sun is 0.51, and the engine
  built six passive volumes, yet no pose shows a day / night change in the medium. Its indoor sun is `LIGH` placements,
  which nothing converts into a medium (see the Skyrim census row above).
- **Starfield's Lodge builds no medium at all**, and neither does Oblivion's `AbandonedMine` (inventoried, not run).
  The census found no baked interior beams for Starfield.
- **The sealed controls hold**: three with passive media (86, 5 and 291 volumes) show no day / night change above
  their noise, so the procedure does not manufacture shafts from fixture lights.
- `WarehouseCaveGeneric` booted with 52 `FogVolume` entities here against the 66 recorded under #6774; not reconciled.

Limits: at most four poses per cell, so a FAIL means no sampled pose showed a change, not that none exists. VLM-INT-004
(the same pass through an exterior-to-interior door) is not run: it needs a door route per cell. VLM-INT-001's
"geometry evidence that it borders outdoors" is not produced; the inventory above is the volume census, per profile.

## Next steps

- Reconcile the remaining census placements against live `FogVolume` entities in the cells not yet booted
  (done for `Vault11a`, `SLAbandonedShack03INT`, `HooverDamIntPowerPlant01b`). `baked_beam_census` now prints
  the base model, `disabled` and `xesp` for each sample, so a placement the loader withholds is visible.
- Judge whether the new passive cones read as light shafts or as haze in a finished frame, per cell, before
  the owner reviews them: they are lit by the local light the cone sits beside, not by a sun.
- The day and night pair (VLM-INT-002) is taken for the A9 list above, aimed with `cam.tp` at each volume rather than
  at the slab the noon sun predicts (plane centre minus `t` times the portal sun direction); aiming at the predicted slab
  would sharpen the poses for the marked-aperture cells.
- Give the A9 list a sun-path census (VLM-INT-001's list), and a door route per listed cell for VLM-INT-004.
