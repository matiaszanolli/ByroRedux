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

## Sun-path census (VLM-INT-001), measured 2026-11-09 (#6727)

`interior_sun_path_census` (ignored test in `byroredux/src/cell_loader/nif_import_registry_tests.rs`, beside
`baked_beam_converter_coverage`; `BYROREDUX_SUNPATH_ESM=<master.esm> BYROREDUX_BEAM_ARCHIVE=<mesh archives,
comma-separated> cargo test -p byroredux --bin byroredux --release -- --ignored --nocapture interior_sun_path_census`)
runs the engine's own beam converter chain on every distinct interior base model, then counts per interior cell the
volumes the loader would build:

- `shaft`: `LightShaft` volumes; `aperture`: `SkyAperture` volumes (a proved source plane). The two producers that can
  follow the sun. Every sun-capable producer is one of these; authored fog meshes are always `Homogeneous`.
- `passive`: `Homogeneous` volumes from the beam chain: fixture beams beside local lights (authored fog meshes are not
  counted, so this under-counts).
- `sunlight`: placed `LIGH` bases with `sunlight` in the editor id (Skyrim's indoor sun, which nothing turns into a
  medium: a gap marker, not a sun path).

| Game | Interior cells | With `shaft` or `aperture` | …with `aperture` | …in a Show Sky cell | Passive only | Show Sky cells | …with no medium at all | With sunlight `LIGH` |
|---|---|---|---|---|---|---|---|---|
| FNV | 388 | 6 | 3 | 0 | 179 | 0 | 0 | 7 |
| FO3 | 421 | 0 | 0 | 0 | 283 | 2 | 2 | 73 |
| Oblivion | 1,855 | 4 | 0 | 0 | 225 | 59 | 58 | 0 |
| Skyrim SE | 590 | 0 | 0 | 0 | 0 | 230 | 230 | 205 |
| FO4 | 1,195 | 6 | 0 | 6 | 183 | 83 | 29 | 0 |
| Starfield | 11,985 | 0 | 0 | 0 | 0 | 0 (flag unverified) | 0 | 0 |

The sun-capable models are four: FNV `NVNellisHangarInteriorLightBeam.nif` (4 apertures and 4 passive volumes per
placement) and `RepLightBeams.nif` (5 `LightShaft` cones), Oblivion `Dungeons\Misc\LightBeam01.NIF`, and FO4
`WindowLightBeam.nif`. Every other beam model converts to a passive cone or is not claimed.

- **FNV**: the three Nellis hangars (`NellisHangar1`, `1b`, `2`: 32 apertures each) and three `RepLightBeams` cells
  (`RocketLabTop` 60, `NCRPrisonBlockB` 30, `2EOVault21GiftShop` 25), none in a Show Sky cell.
- **Oblivion**: `TestJuanGoblinLair` 11, `TestCave01` 7, `AnvilCastleSmugglersCave` 2, `TestShrine` 2 `LightShaft`
  volumes, none Show Sky. 58 of its 59 Show Sky interiors build no medium.
- **FO4**: six cells, all Show Sky: `DBTechHighSchool01` 76, `TrinityTower01` 12, `ConcordMuseum01` 10, `CambridgePD01` 6,
  `zInstituteRotunda` 6, `UniversityPoint01` 4 `LightShaft` volumes. 29 of its 83 Show Sky interiors build no medium.
- **FO3, Skyrim, Starfield**: no sun-capable volume in any interior. Skyrim has 230 Show Sky interiors and 205 with
  sunlight `LIGH`, none with a medium; the chain builds no volume in any Starfield interior.

The census only says what the engine would build. A sun-capable volume is not sun-lit: the loader's design needs a
clear geometry ray through a real opening (see the handoff). The measurement below finds the split.

## A9 day / night results (VLM-INT-001 to -003), measured 2026-11-08 and 09 (#6727)

`docs/smoke-tests/interior-shafts-content.sh` boots each cell held open on the free camera, with the game clock
paused and the exposure fixed, and for up to four poses (`cam.tp` onto its volumes: marked apertures first, then
light-shaft volumes, then any; a control with no volume uses the entities nearest its spawn) captures the froxel
in-scatter view at `time.set 12`, `time.set 0` and `time.set 12` again. A pose's region is the pixels where the mean
noon frame beats midnight by more than max(3 levels, 2 x p99 of |noon - noon repeat|); a `shaft` cell passes when its
best pose's region is at least 0.1 % of the frame, a `sealed` control when no pose reaches that. The in-scatter view
is used, not the finished frame, because a window or roof hole shows a sky that differs between noon and midnight
whether or not any shaft exists. The repeat noise is large on many cells (p99 up to 39 levels): fixture lights flicker.

The first A9 list (the handoff's candidates) failed on cells the census shows have no sun-capable volume (HELIOS One,
REPCONN HQ, Tenpenny, an Oblivion cave of passive cones). The list is now the census's: a `shaft` cell is one the census
predicts can show a shaft (a marked aperture, or `LightShaft` volumes in a Show Sky cell), a `sealed` control has no such
path, and a `gap` cell has an authored sun and no medium.

| Role | Cell | Census | Best pose: region / mean dL / noise p99 | Result |
|---|---|---|---|---|
| shaft | FNV `NellisHangar1` | 32 apertures | 23.5 % / 18.4 / 5.5 | PASS |
| shaft | FNV `NellisHangar1b` | 32 apertures | 14.6 % / 21.6 / 6.5 | PASS |
| shaft | FNV `NellisHangar2` | 32 apertures | 78.4 % / 13.0 / 2.9 | PASS |
| shaft | FO4 `ConcordMuseum01` | 10 shaft, Show Sky | 92.3 % / 9.3 / 2.8 | PASS |
| shaft | FO4 `DBTechHighSchool01` | 76 shaft, Show Sky | 3.5 % / 10.4 / 3.1 | PASS (after aiming at its `LightShaft` volumes; aimed at all volumes evenly it showed 0.01 %) |
| shaft | FO4 `TrinityTower01` | 12 shaft, Show Sky | 4.4 % / 8.9 / 3.7 | PASS |
| shaft | FO4 `zInstituteRotunda` | 6 shaft, Show Sky | 100 % / 29.5 / 2.0 | PASS |
| shaft | FO4 `CambridgePD01` | 6 shaft, Show Sky | 42.8 % / 15.8 / 6.0 | PASS |
| shaft | FO4 `UniversityPoint01` | 4 shaft, Show Sky | 14.4 % / 23.1 / 6.0 | PASS |
| gap | Skyrim `AbandonedShackInterior` | Show Sky, 4 sunlight `LIGH`, no medium | 0.00 % / 0 / 6.7 | GAP-OPEN |
| control | FNV `RocketLabTop` | 60 shaft, Show Sky off | 0.00 % (one earlier run 0.60 %, not repeated) | PASS |
| control | FNV `NCRPrisonBlockB`, `2EOVault21GiftShop` | 30 / 25 shaft, Show Sky off | 0.00 % | PASS |
| control | Oblivion `AnvilCastleSmugglersCave`, `TestCave01` | 2 / 7 shaft, Show Sky off | 0.00 % | PASS |
| control | FNV `HELIOSOneDeck`, `HiddenValleyBunker1`; FO3 `Tenpenny01`; Skyrim `HillgrundsTomb01`; Oblivion `WarehouseCaveGeneric`, `ImperialSewers03`; FO4 `Vault111Cryo` | passive or no medium | 0.00 % to 0.01 % | PASS |

Per game: FNV shaft 3 of 3, controls 5 of 5; FO4 shaft 6 of 6, control 1 of 1; Oblivion controls 4 of 4 (no shaft
cell); FO3 control 1 of 1 (no shaft cell); Skyrim control 1 of 1 and one open gap; Starfield has no entry.

What it shows:

- **The census predicts the measurement on this sample.** The nine census-predicted cells all show a sun-driven change
  above the noise; the five cells with `LightShaft` volumes but Show Sky off and no marked aperture (REPCONN labs, NCR
  prison, Vault 21 gift shop, two Oblivion caves) show none. The first ten cells measured fixed the rule (a marked
  aperture, or `LightShaft` in a Show Sky cell); the next four (`NellisHangar1b`, `NellisHangar2`, `CambridgePD01`,
  `UniversityPoint01`) were predicted before they were measured and all passed. Ten cells is a small sample: it is
  evidence for the rule, not proof, and a `LightShaft` cell without Show Sky could still pass if a clear ray exists.
- **The mechanism covers very little.** Nine interiors across the six titles (three in FNV, six of FO4's 1,195) have a
  candidate the sun can light. FO3, Skyrim and Starfield have none; Skyrim's 230 Show Sky interiors and 205 with
  sunlight `LIGH`, and Oblivion's 58 Show Sky interiors with no medium, are the gap `AbandonedShackInterior` stands for.
- **The gate is not noise-free.** Across the runs two controls each showed one marginal false positive (`RocketLabTop`
  0.60 % once, `Vault111Cryo` 0.19 % once, both in cells with flickering fixture lights) and one cell hit a harness error
  (`TestCave01`, a screenshot that never appeared). Each passed on repeat. The 0.1 % floor sits close to the noise on
  cells with flicker.
- `WarehouseCaveGeneric` booted with 52 `FogVolume` entities here against the 66 recorded under #6774; not reconciled.

Limits: at most four poses per cell, so a FAIL means no sampled pose showed a change, not that none exists. VLM-INT-004
(the same pass through an exterior-to-interior door) is run for eight of the nine listed cells; see the section below.
VLM-INT-001's
"geometry evidence that it borders outdoors" is not produced beyond Show Sky and the marked plane; the census is
the volume and flag census, per cell. The census reads the Show Sky flag, which reads 0 for every Starfield interior
and is unverified there.

## A9 through a door (VLM-INT-004), measured 2026-11-09 and 2026-11-10 (#6727)

`BYRO_INTERIOR_ROUTE=door docs/smoke-tests/interior-shafts-content.sh` boots each listed cell as before, then takes its own
exit door out (`door.teleport`, the console frontend of the E-key path) and, from the exterior arrival, the door that
leads back to the cell, and runs the same day / night pass on the cell that second transition delivered. `door.list`
(read-only, new) names the loaded teleport doors with the cell each leads to, so the way back in is chosen by
destination. The first measurement (2026-11-09) took whichever door the camera ray hit and could not resolve two
cells; that was the picker, not the cells: the Nellis hangars' exterior doors stand 112 units apart, and the ray took
Hangar 1's door when 1b's was meant. The exterior leg is not asked for a stable entity count (a streaming exterior
never has one); the door is polled for, up to five minutes, because FO4's exterior streams in at about one frame per
second under xvfb.

| Cell | Result through the door | Best pose: region / noise p99 | Note |
|---|---|---|---|
| FNV `NellisHangar1` | PASS | 29.6 % / 8.6 (earlier run 10.6 %) | direct boot: 23.5 % / 5.5 |
| FNV `NellisHangar1b` | PASS | 29.2 % / 8.7 (earlier run 85.9 %) | its own exterior door, beside Hangar 1's |
| FNV `NellisHangar2` | PASS | 89.4 % / 2.1 | direct boot: 78.4 % / 2.9 |
| FO4 `ConcordMuseum01` | PASS | 99.8 % / 2.0 | |
| FO4 `zInstituteRotunda` | PASS | 100 % / 4.2 | its exit door leads to an exterior |
| FO4 `UniversityPoint01` | PASS | 15.1 % / 4.0 (earlier 24.5 %) | one run found no door to it in five minutes; the next two did |
| FO4 `DBTechHighSchool01` | PASS | 4.3 % / 3.0 (earlier 3.5 %) | |
| FO4 `CambridgePD01` | PASS, pose-dependent | 0.7 %, 1.0 %, 6.7 % over three runs / 5 to 8 | direct boot: 44 % on the first pose, 0 on the others (see below) |
| FO4 `TrinityTower01` | no door route | none | both exit doors arrive inside the tower's shaft next to an elevator lift panel (`near` lists `elevatorliftpanel01`); no loaded door leads back, so the way in is the elevator, not a door |

Eight of nine pass through a door. `TrinityTower01` has no door route; its first measurement (0.09 to 0.13 %) was
taken with the ray picker and cannot be reproduced with destination-based picking, so it is withdrawn, not carried
forward. Its direct boot (4.4 %) stands.

**Two routes, two populations.** `CambridgePD01` booted directly has 7,499 named entities and 37 `FogVolume`s; the
same cell reached through its door (after the direct boot, an exit, and the return) has 2,578 and 30, stable for four
minutes. The difference is mostly actors (every `*_finger`, `spine`, `neck_skin` and outfit bone is missing) and a share
of the placed objects (cabinets, crates, debris: 20 against 8, 27 against 15). The shaft pose that reads 44 % on the
direct boot reads 0 % on the door route. Both routes run the same `InteriorCellApplyJob`, so this is not a loader fork;
a revisit effect (the second load of a cell in one session) is not excluded, since the door route always loads the
cell for the second time. Not explained, and it bounds how much the door route says about a fresh entry.

## Next steps

- Reconcile the remaining census placements against live `FogVolume` entities in the cells not yet booted
  (done for `Vault11a`, `SLAbandonedShack03INT`, `HooverDamIntPowerPlant01b`). `baked_beam_census` now prints
  the base model, `disabled` and `xesp` for each sample, so a placement the loader withholds is visible.
- Judge whether the new passive cones read as light shafts or as haze in a finished frame, per cell, before
  the owner reviews them: they are lit by the local light the cone sits beside, not by a sun.
- The day and night pair (VLM-INT-002) is taken for the A9 list above, aimed with `cam.tp` at each volume rather than
  at the slab the noon sun predicts (plane centre minus `t` times the portal sun direction); aiming at the predicted slab
  would sharpen the poses for the marked-aperture cells.
- Explain the two-population difference above: enter `CambridgePD01` through its door from a fresh boot at the exterior
  (no earlier load of the cell) and compare with the direct boot; if the population still differs, the transition
  path drops actors and objects, which is an engine defect and a VLM-INT-004 finding in its own right. Give
  `TrinityTower01` an elevator route (activate the lift panel) or accept it as direct-boot only.
- Decide the gap: Skyrim's sunlight `LIGH` (205 interiors), Oblivion's and FO4's Show Sky interiors with no medium (58
  and 29) have a sun and nothing to carry it. A converter for them is new engine work, not a list change.
