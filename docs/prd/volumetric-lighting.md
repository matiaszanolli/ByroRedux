# Volumetric Lighting — Product Requirements

**Status**: ACTIVE (2026-11-10). Revision 4 files the tracking issues. Revision 3 records the owner's §10 decisions: every one the recommendation except Q7, which the owner decided for the exterior haze floor so that the goal, realistic procedural god rays in interiors and exteriors, holds on clear weather (VLM-SH-010, VLM-SH-011). Revision 2, same day, folds in review
round 1 (two HEAD corrections — the per-cell `XCCM` cross-fade and the
region records that are read — the authored god-ray records Skyrim SE and
FO4 carry, the region tie rule and forced-weather lock, the FO4 `LGTM`
tail, the froxel trace's opaque-only visibility, the non-RT cascade
non-goal ROADMAP already decided, the parity reference ROADMAP already
names, and sibling-PRD links). It covers the remainder of ROADMAP milestone
M55 after the froxel fog slice, the transported combustion solver and the
interior godray vertical slice shipped. Statements about how an original
engine, a record format, a paper or another engine behaves cite external
sources (the local CK / GECK / CS / Fallout CK wiki dumps, xEdit's record
definitions, published papers, vendor documentation); a claim that could
not be verified is marked **Phase 0: verify** instead of being stated as
fact.

**Scope**: What M55 still promises on top of the shipped participating
medium, for every player of the supported titles: weather and fog chosen per
exterior cell by the region records that paint it; the full authored fog
profile (falloff power, the second height layer, the cell and lighting-
template height-fog tails, and the authored god-ray records of Skyrim SE and
FO4) translated into the one canonical medium or listed with the reason it
is not; sunlight shafts outdoors that are visible, stable and measured;
interior shafts validated on real openings across titles rather than on one
fixture; and an aerial-perspective tail for distant geometry that agrees
with the sky drawn behind it. Fire, smoke and explosions are **outside**
this document: the transported combustion solver, its local grid, its
surface lights, the calibration of its solid boundary and the Known Issue
on their derived reach are owned by the six-phase
[fire volumetrics plan](../engine/fire-volumetrics-plan.md) (tracking #6531)
and the fog design's
[follow-up boundary](../engine/procedural-volumetric-fog.md#follow-up-boundary),
which this PRD cites and does not re-own (§10 Q1). Path-traced
heterogeneous media belong to the
[path-tracing reference mode PRD](path-tracing-reference-mode.md)
(PTR-CNT-004; VLM-NG-002).

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Froxel medium, temporal reprojection, analytic beyond-grid tail, legacy-ramp fit, measurement table, decoded-field corpus findings | [Procedural Volumetric Fog](../engine/procedural-volumetric-fog.md) |
| Fire, smoke, explosions: combustion solver, local grid, fire surface lights, boundary calibration | [Fire/Smoke/Explosions Plan](../engine/fire-volumetrics-plan.md), [Phase 0 survey](../engine/fire-volumetrics-phase0.md) |
| Interior shafts, sky apertures, painted-beam converters | [Interior Godrays Status](../engine/interior-godrays-status.md) |
| Sky, cloud shadows, portal sun, sky-side aerial perspective | [SKYAL](../engine/skyal.md) |
| Climate / weather translation boundary, base-fog strength choice | [EXAL](../engine/exal.md) |
| Region weather resolution (`RDWT` tables, Override / Priority, `XCCM` sky-from-region) | Design input written: [EXAL §3.2](../engine/exal.md#32-region-weather-resolution-vlm-reg-phase-0-census-2026-11-07) records the census; the resolver itself is Phase 1 |
| Authored god-ray records (Skyrim SE `VOLI`, FO4 `GDRY`) | Census recorded in the fog design's [corpus findings](../engine/procedural-volumetric-fog.md#corpus-findings); decode and semantics are Phase 1 (VLM-HF-010, -011) |
| Units, emitters, visibility masks, adaptive ray budget | [Physical Lighting Backbone](../engine/physical-lighting-backbone.md) |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md) |
| Path-traced media (majorant, ratio tracking) | [Path Tracing Reference Mode PRD](path-tracing-reference-mode.md) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `VLM-` prefix.


**Tracking**: #6729 (parent). Phases: #6724 (0), #6725 (1), #6726 (2),
#6727 (3), #6728 (4).
---

## 1. Product thesis

The ROADMAP's M55 row names the moment it exists for: "sunlight through
Megaton's church windows, dust motes in Doc Mitchell's hallway, fog rolling
off WastelandNV grass at dusk" — the cinematic beat Bethesda interiors and
exteriors most lack ([ROADMAP Tier 8](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon)).
The medium that delivers it already exists: one froxel calculation for fog,
shafts and emissive media, ray-traced against the scene TLAS
([Decision record](../engine/procedural-volumetric-fog.md#decision-record)).
What is missing is the data feeding it and the evidence that it shows:

1. **The world's fog is not yet the authored fog.** Exterior weather comes
   from the climate alone. Region records are read for ambient sound and
   for a mod-only climate link, but their weather tables — what the
   original editors use to give a valley its own weather — are parsed and
   never read. Several authored fog fields (falloff power, the far height
   layer, the FO4 / FO76 cell and lighting-template height-fog tail,
   Starfield's cell height fog) stop at the parser, and the god-ray records
   Skyrim SE and FO4 author are not parsed at all.
2. **Outdoor shafts exist mechanically but are unproven.** The froxel sun
   term traces exteriors already; nothing measures whether a shaft is
   visible under vanilla weather, how far it reaches, or what it costs.
3. **Interior shafts are proven on one lab and a few assets.** The godray
   handoff says so itself: "Completion still means the requested behavior
   across Bethesda titles and actual openings".

The owner's standing direction is to benchmark against modern engines, not
the legacy renderers: matching a 2008–2015 forward renderer's look is
explicitly anti-scope
([What we are NOT doing](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
The ROADMAP M55 row already names the modern reference: Hillaire's
Frostbite volumetrics (SIGGRAPH 2015). This PRD measures against it
(VLM-G-005) and asks the owner only whether to change it (§10 Q6).

The corollary that shapes every requirement below: **authored data decides
*where* and *how much* medium there is; physics decides how it looks.** Every
legacy fog, weather and region field is translated once at the EXAL boundary
into the canonical `FogMedium`; nothing past that boundary branches on game,
record or provenance. The engine will not reproduce the legacy distance-ramp
look, will not reintroduce painted beam cards, and will not invent a medium
the content did not author without recording it as an engine choice.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Froxel grid** | The view-aligned volume the inject and integrate passes fill (`crates/renderer/src/vulkan/volumetrics.rs`, `volumetrics_{inject,integrate}.comp`); range set by `--fog-grid-far-m`. |
| **Analytic tail** | The beyond-grid exponential height-fog term in `composite.frag` (`heightFogOpticalDepth`) that continues the grid's boundary radiance to distant geometry and sky. |
| **`FogMedium`** | The canonical medium (`byroredux/src/fog.rs`): extinction per metre, single-scatter albedo, coverage, scale height. The only fog type the renderer consumes. |
| **Legacy ramp** | An authored near / far (and, where present, power and maximum-opacity) fog distance set in `XCLL` or `WTHR` `FNAM`, fitted once into a `FogMedium` (`FogMedium::from_legacy_ramp`). |
| **Height profile** | The authored altitude fields of a medium: FO4 / FO76 `WTHR` `FNAM` near / far height mid and range plus high-density scale (`WeatherHeightFog`), the FO4 / FO76 `XCLL` and `LGTM` tail, and Starfield's `XCLL` / `LGTM` height-fog block (`StarfieldLighting`). |
| **Authored god-ray profile** | A record that tunes the original renderer's volumetric light: Skyrim SE `VOLI` (intensity, colour, density, phase function, sampling), referenced per time of day by `WTHR` `HNAM`; FO4 `GDRY` (back / forward colour and phase, air colour, intensity), referenced by `WTHR` `WGDR`, `CELL` `XGDR`, `LGTM` `WGDR` and `LIGH` `WGDR`. |
| **Region weather table** | A `REGN` `RDAT` section of type Weather and its `RDWT` rows (weather, chance, optional global), with the section's Override flag and Priority (`RegionDataEntry`, `RegionWeather`). |
| **Weather source** | Which authored record chose the weather in effect: a forced weather, a region weather table, a cell `XCCM`, or the climate. |
| **Override lock** | The optional second argument of ObScript `ForceWeather` that keeps a forced weather until `ReleaseWeatherOverride` (`ForcedWeather::locked`). |
| **Shaft** | A visible difference in in-scattered radiance between lit and occluded parts of the medium along the view, produced by the froxel sun or local-light term and RT visibility. |
| **Shaft contrast** | The mean linear radiance in a shaft region minus that in an adjacent occluded region of the same frame, in a `render.debug composite_term` capture. |
| **Sky aperture** | A proved source plane, `FogProfile::SkyAperture` ([handoff](../engine/interior-godrays-status.md#what-is-implemented)). |
| **Portal sun** | The outdoor sun an interior's medium sees through an opening ([SKYAL interior lane](../engine/skyal.md#the-interior-outdoor-sky-lane-4927-2026-10-01)). |
| **Combustion field** | The transported fire / smoke state; owned by the fire plan, named here only to scope it out. |

---

## 3. Core model

M55's mechanisms fall into four ownership classes. The class is how a
reader finds who answers for a mechanism, and how the ROADMAP row reports it.

| Class | Meaning | Reported in ROADMAP as |
|---|---|---|
| **Shipped** | In the engine at HEAD; this PRD only cites it. | "shipped" in the M55 row |
| **Open (VLM)** | Promised by this PRD; requirements in §5. | the requirement area and its metric |
| **Owned elsewhere** | Promised by another document with its own tracking; listed so it is not lost. | the owning document's tracking issue |
| **Non-goal** | Deliberately out (§4.2). | not listed |

The inventory below is the state at HEAD on 2026-11-01. It is hand-written;
Phase 0 replaces the "Today" column by the ROADMAP M55 row's link to the
measurement table and the per-area acceptance results, and this table is
then frozen.

| # | Mechanism | Class | Today |
|---|---|---|---|
| 1 | Froxel grid: inject + integrate, temporal history, RT sun and local-light visibility | Shipped | `volumetrics_inject.comp` traces the sun with `traceShadowBinary(…, VISIBILITY_MASK_ALL_OPAQUE)`; defaults 8 px per froxel column, 64 slices, 128 m far, far plane validated to 32–512 m (`VolumetricsConfig::DEFAULT`, `crates/renderer/src/vulkan/upscaling.rs`) |
| 2 | Exterior sun in-scatter in the froxels | Shipped mechanism, unmeasured → VLM-SH | `volumetric_open_sky_flag` returns open sky for every exterior (`crates/renderer/src/vulkan/context/post_passes.rs:105`); the only volumetric script in `docs/smoke-tests/` is the interior `interior-godrays.sh` |
| 3 | Cloud shadow on the froxel sun | Shipped as one value per frame → VLM-SH | `volumetric_sun` multiplies the sun by the camera column's `camera_cloud_sun_transmittance` (`post_passes.rs:84`) |
| 4 | Legacy ramp → `FogMedium` fit (near, far, maximum opacity) | Shipped | `FogMedium::from_legacy_ramp(near, far, max_opacity)` (`byroredux/src/fog.rs:124`) |
| 5 | Authored fog power in the fit | Open → VLM-HF | Decoded (`read_fog_power`, `crates/plugin/src/esm/records/weather.rs:476`), carried as `CellLightingRes::fog_power` to `CompositeParams.fog_params.w` (`context/frame_params.rs:948`), and read by no fit and no shader (`composite.frag` reads `fog_params.y` only) |
| 6 | FO4 / FO76 `WTHR` near height range | Shipped | `FogMedium::with_authored_height_range` (#3956, `fog.rs:170`); `height_mid` deliberately unread |
| 7 | FO4 / FO76 `WTHR` far height layer and high-density scale | Open → VLM-HF | Decoded into `WeatherHeightFog`; no consumer |
| 8 | FO4 / FO76 `XCLL` and `LGTM` height-fog tail | Open → VLM-HF | The `XCLL` arm (`crates/plugin/src/esm/cell/walkers.rs:291`) and the `LGTM` `DATA` arm (`crates/plugin/src/esm/records/misc/world.rs:1423`) decode the tail only under `game == GameKind::Starfield && len >= 108`; the FO4 tail is not decoded |
| 9 | Starfield `XCLL` / `LGTM` height fog | Open → VLM-HF | Decoded into `StarfieldLighting` and forwarded on `CellLightingRes.starfield` (`cell_loader/load.rs:874`); no fog translation reads it |
| 10 | Region weather tables (`RDWT`) | Open → VLM-REG | Parsed by `decode_weather_rows` (`crates/plugin/src/esm/records/misc/world.rs:1048`); no non-test reader of `RegionDataPayload::Weather` in `byroredux/src`. Regions are read for ambient sound (`apply_cell_region_ambient`) and the region `CNAM` climate rung (`region_climate_for_center`, `env_translate/climate.rs`), which is inert on vanilla data |
| 11 | Weather choice | Shipped (Phase 1: climate and region tables) | `resolve_cell_weather` (`env_translate/region_weather.rs`) takes the cell's `XCCM` climate, else the region tables' weather (VLM-REG), else `resolve_default_weather`'s highest-chance climate row (`env_translate/climate.rs`, the one rule EXAL fixes, #5424); `apply_cell_weather` re-resolves per cell crossing and re-applies with the same 8-second `WeatherTransitionRes` cross-fade a worldspace change uses (`scene/world_setup.rs`; [EXAL](../engine/exal.md), §3.2) |
| 12 | Skyrim / FO4 interior `XCCM` sky-from-region | Open → VLM-REG | `CellData::sky_region` decoded, documented "not yet consumed" (`crates/plugin/src/esm/cell/cell_data.rs:105`) |
| 13 | Forced weather | Shipped over the climate, lock unread → VLM-REG | `forced_or` makes a forced weather replace the climate default (`scene/world_setup.rs:305-315`); `ForcedWeather::locked` is "recorded and saved but read by nothing" (`crates/scripting/src/obscript_state.rs:100`) |
| 14 | Interior shafts and sky apertures | Shipped slice, coverage open → VLM-INT; its own gate is green again (#6730) | `--godray-lab` + `docs/smoke-tests/interior-godrays.sh` had gone red (centre delta 0.0, 43.2 when written) because the lab relied on two values the engine stopped supplying to a cell-less harness: #4902 removed the render-loop fallback for `InteriorSkyExposureRes` and #4909 moved an interior's portal sun onto `weather.sunlight_color`, which the fallback sky leaves at zero. The lab now states both (`db28aa9b2`): centre delta 45.6, side 22.2. At a real weather sunlight colour (about 1) the same room reads centre delta 8, so the 25-level bar is calibrated to the lab's radiance, not to weather. Aperture upload now keeps the nearest apertures and logs the overflow once (VLM-INT-005, `context/sky_apertures.rs`) |
| 15 | Analytic tail for distant geometry | Shipped, sky agreement open → VLM-AP | `composite.frag` continues the grid boundary radiance, clamped to four times the authored fog luminance |
| 16 | Aerial-perspective volume (Hillaire 2020) | Open → VLM-AP, §10 Q4 | No aerial LUT in `crates/renderer` (`rg -i "aerial.*lut"` is empty); [measurement table](../engine/procedural-volumetric-fog.md#measurement-table) row "Aerial LUT on" is empty |
| 17 | Alpha-tested cutouts in the froxel sun trace | Shipped for open sky → VLM-SH-009 | `volumetrics_inject.comp` re-walks a sun ray the binary trace calls blocked (`sunRayBlockedAlphaAware`, open sky only), stepping past alpha holes with the shadow transport's 8-layer budget and the shared `alphaComparePass` (`include/alpha_compare.glsl`). It reads the material table (binding 32, as flat words) and the bindless textures (the pipeline layout's second set). `traceShadowBinary` itself stays opaque (`shadow_common.glsl:35`, BLAS built `OPAQUE`, `blas_static.rs:597`), so interiors and the local-light traces are unchanged |
| 18 | Authored god-ray profiles (Skyrim SE `VOLI`, FO4 `GDRY`) | Open → VLM-HF-010, -011, §10 Q8 | Not parsed: `rg "VOLI\|GDRY\|WGDR\|XGDR" crates/plugin/src` is empty (only the probe `crates/plugin/examples/dump_voli_subs.rs` exists); census 2026-11-07 ([corpus findings](../engine/procedural-volumetric-fog.md#corpus-findings)): Skyrim SE `VOLI` is 42 records in `Update.esm` / `Dawnguard.esm` / `Dragonborn.esm` (none in `Skyrim.esm`) with six of nine fields constant; FO4 has no `VOLI` but 61 `GDRY` reached by 63 weathers, 7 cells and 20 lights |
| 19 | Combustion solid boundary: no-through, inward-normal removal, tangential slip, wall friction, transported overpressure | Shipped; calibration owned elsewhere (fire plan, fog design follow-up 1) | `include/combustion_transport.glsl:896-911` removes the inward normal component and applies `COMBUSTION_WALL_FRICTION_PER_SECOND` (since `715b9230b`); overpressure transport since `2155cc917`. ROADMAP's "boundary-normal slip/pressure response" open item is stale (§11 follow-ups) |
| 20 | Fire surface-light reach | Owned elsewhere ([fire plan Phase 5](../engine/fire-volumetrics-plan.md#phase-5-fire-as-a-light-source-in-the-ray-traced-path)) | Range is `sqrt(luma / COMBUSTION_LIGHT_CUTOFF_IRRADIANCE)` clamped to `COMBUSTION_LIGHT_MAX_RANGE_METERS` (`crates/renderer/src/vulkan/volumetrics/combustion.rs:177`); [Known Issue](../../ROADMAP.md#correctness-and-content) |
| 21 | Majorant grid / ratio tracking for path-traced media | Non-goal (VLM-NG-002) | No code (`rg -i "majorant\|ratio.track"` is empty) |
| 22 | Non-RT cascade visibility for the froxels | Non-goal (VLM-NG-006) | No cascade path; [measurement table](../engine/procedural-volumetric-fog.md#measurement-table) row "cascade, 1 tap" is empty |

---

## 4. Goals and non-goals

### 4.1 Goals

- **VLM-G-001 — Region weather.** Every exterior cell whose regions author
  a weather table takes its weather from that table, and the source is
  reported (VLM-REG).
- **VLM-G-002 — The whole authored profile.** Every authored fog or
  god-ray field the parser decodes reaches `FogMedium` or is listed with the
  reason it does not (VLM-HF).
- **VLM-G-003 — Measured outdoor shafts.** Exterior shafts have a lab, a
  real-content capture per exterior title, a contrast metric and a
  per-pass cost (VLM-SH).
- **VLM-G-004 — Interior coverage beyond the lab.** Interior shafts pass on
  a named cell list per title, day and night (VLM-INT).
- **VLM-G-005 — Modern parity.** The feature list of the modern reference
  (ROADMAP's Frostbite 2015 unless §10 Q6 changes it) is met or each gap
  is recorded as a non-goal (§8).

### 4.2 Non-goals

- **VLM-NG-001.** Fire, smoke and explosion quality, cost and lighting,
  including the derived-reach Known Issue and the calibration of the
  combustion boundary: owned by the
  [fire plan](../engine/fire-volumetrics-plan.md) (#6531) and the fog
  design's [follow-up boundary](../engine/procedural-volumetric-fog.md#follow-up-boundary),
  §10 Q1.
- **VLM-NG-002.** Path-traced heterogeneous media (majorant grid, ratio
  tracking): owned by the
  [path-tracing reference mode PRD](path-tracing-reference-mode.md)
  (PTR-CNT-004), milestone M51.
- **VLM-NG-003.** Reproducing the legacy distance-ramp fog or the painted
  `fxlightrays` beam look. The design already suppresses those cards
  ([Runtime contract](../engine/procedural-volumetric-fog.md#runtime-contract)),
  and per-pixel legacy parity is anti-scope.
- **VLM-NG-004.** Volumetric clouds and their shadows: built and owned by
  [SKYAL §2.5](../engine/skyal.md#25-cloud-coverage-per-column-and-cloud-shadows-2026-10-20);
  this PRD only consumes the shadow map (VLM-SH-004).
- **VLM-NG-005.** Rain and snow surface state (`WeatherSurfaceState`): owned
  by the [weathered ground response](../engine/procedural-volumetric-fog.md#weathered-ground-response)
  and the [wet-surface accumulation PRD](wet-surface-accumulation.md)
  (M61). Region weather changes which weather is active and therefore what
  that PRD's classification sees; it does not change the classification.
- **VLM-NG-006.** A non-RT shadow-map cascade path for the froxels. The
  ROADMAP M55 row already decides it: "RT-shadowed (no shadow-map cascade
  hack — we already have RT visibility)"
  ([Tier 8](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon)).
  The design's "cascade, 1 tap" measurement row stays empty.

---

## 5. Requirements by area

### 5.1 Region-driven weather (`VLM-REG`)

*Phase 1 (2026-11-08) shipped VLM-REG-001 to -014:* `resolve_cell_weather` / `resolve_region_weather` / `resolve_interior_region_weather` in `byroredux/src/env_translate/region_weather.rs`, `apply_cell_weather` and `release_unlocked_forced_weather` in `scene/world_setup.rs`, and the `weather.source=` line of `env.health`. The one rule still resting on an inference rather than an observation is VLM-REG-002's non-override merge (#6772). Test names below are the shipped ones.


The `REGN` record carries no fog field: xEdit's `RDAT` type enum is
Objects, Weather, Map, Land, Grass, Sound, Imposter and unknowns, and only
the Weather section points at fog-bearing data
([wbDefinitionsTES5.pas L12293-L12311](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsTES5.pas#L12293-L12311);
the TES4, FNV and FO4 `REGN` definitions start at L4218, L9426 and L14937
of their files at the same commit). The CK, GECK and CS editors give a
region a *Weather* tab whose rows are a weather type and a percentage
chance, optionally represented by a global variable (local dumps under
`/mnt/data/src/reference/`: `ck-uesp-wiki/(main)/Region.wiki`, *Weather
Tab*; `geck-uesp-wiki/(main)/Regions.wiki`;
`cs-uesp-wiki/(main)/Regions.wiki`). Per-cell fog is therefore the fog of
the weather the cell's regions choose. xEdit defines the rows as `RDWT`
`Weather Types` = weather, chance, global
([wbDefinitionsTES5.pas L12373](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsTES5.pas#L12373));
Oblivion's rows are weather and chance only
([wbDefinitionsTES4.pas L4310](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsTES4.pas#L4310)).
All three wikis state that a region's Priority is compared only when its
Override box is checked, per data tab, and that higher priority wins; only
the CK page states a tie rule. **Phase 0: verify** that a region's Weather
section replaces the climate's weather list rather than adding to it — the
pages describe the region list's chances but none says so outright. The census
(`region_weather_census`, 2026-11-07) cannot settle it: it needs the original
engine. Still open, an in-game observation (filed as #6772), see VLM-REG-002.

- **VLM-REG-001.** For an exterior cell, the weather table in effect MUST be
  the Weather section with the highest Priority among the cell's regions
  whose section has the Override flag set. Today no non-test code in
  `byroredux/src` reads `RegionDataPayload::Weather` (at HEAD on
  2026-11-01). Test: `region_weather_override_priority_wins` (new).
- **VLM-REG-002.** When no region of the cell sets Override on a Weather section, the rows of every covering non-override section MUST form one table and the highest-chance row MUST win (§10 Q2's rule across the merged table), ties going to the higher section Priority, then the lower region FormID, then the authored row order. *Owner decision 2026-11-07, shipped in Phase 1 as an engine choice recorded under VLM-HF-008 until #6772 observes the original engine.* **Phase 0, census:** none of the three wiki pages
  states which non-override region wins when several overlap. **Census,
  2026-11-07** (`crates/plugin/examples/region_weather_census.rs`, one master at a
  time; exterior cells whose `XCLR` names two or more Weather-carrying regions,
  none overriding, with differing weather tables): Oblivion 527 cells (of 1,108
  overlaps), every one with equal Priority on both sections, so Priority cannot
  decide them; FNV 8, all with different Priorities; FO4 11 (of 75), all with
  different Priorities; Fallout 3 and Skyrim 0. The rule is **not moot** on
  Oblivion, FNV or FO4. Still open (filed as #6772): the in-game observation recorded with its save and cell, which needs the original engine; until it exists the choice is an owner decision, not an inference.
- **VLM-REG-003.** When none of the cell's regions authors a Weather
  section, the weather MUST come from the climate exactly as today
  (`resolve_exterior_climate` → `resolve_default_weather`). Test:
  `cell_without_region_weather_keeps_climate_weather` (new).
- **VLM-REG-004.** Within a chosen table, the weather picked MUST follow the
  rule §10 Q2 decides (recommended: the highest-chance row, the rule
  `resolve_default_weather` applies to climates today).
- **VLM-REG-005.** A row whose global is set MUST take its chance from that
  global's value at resolution time (CK *Weather Tab*: "you may optionally
  choose a global variable that will represent the chance"). Oblivion's
  rows carry no global (xEdit TES4 L4310 above), so the rule is a no-op
  there by data, not by branch.
- **VLM-REG-006.** A forced weather set with the override lock
  (`ForcedWeather { locked: true }`, set by ObScript `ForceWeather <w> 1` and
  `weather.force <w> lock`) MUST outrank the region and climate choices until
  released, as the GECK `ForceWeather` page states ("If
  OverrideWeatherFlag is 1, the weather will not change again until
  ReleaseWeatherOverride is called"). Today `forced_or` already outranks the
  climate default (`scene/world_setup.rs:305-315`). Test:
  `forced_weather_outranks_region_weather` (new).
- **VLM-REG-007.** Crossing from one exterior cell into another whose
  resolved weather differs MUST start the existing `WeatherTransitionRes`
  cross-fade instead of replacing `WeatherDataRes` directly. Today the
  cross-fade runs on worldspace changes and on per-cell `XCCM` climate
  changes through `apply_cell_climate_override` → `apply_environment`
  (`scene/world_setup.rs:781-784`); the region path MUST reuse that call,
  not a second install path.
- **VLM-REG-008.** Region membership MUST be the region's `RPLD` polygon overlapping the cell's square, within the region's own `WNAM` worldspace (`RegnRecord::covers_cell`); the cell's `XCLR` list (`CellData::regions`) MUST NOT be consulted for weather. *Shipped in Phase 1.* **Phase 0, settled:** the CK page says "it is
  the exact shape of the region as drawn in the Region Preview Window that
  will decide its actual area of effect", not the per-cell list. **Census,
  2026-11-07** (same probe; a region covers a cell when its `RPLD` polygon
  overlaps the cell's 4096-unit square, restricted to the region's own `WNAM`
  worldspace): the two agree on almost every Weather-region pair (Oblivion 9,892
  in both; FO3 4,493; FNV 1,589; Skyrim 9,935; FO4 4,234) and disagree on a
  small, real remainder. Pairs whose polygon holds the cell centre but whose
  cell `XCLR` omits the region: Oblivion 1, FO3 23, FNV 16, Skyrim 1, FO4 12
  (FO3's Washington-top cells have an empty `XCLR` under a covering polygon).
  Pairs `XCLR` names that the polygon does not touch: Oblivion 24, FO3 28,
  FNV 0, Skyrim 42, FO4 88; the `RPLI` edge fall-off explains only 6 of them
  (Oblivion). `XCLR` is a stale snapshot in both directions, so the gate's
  condition is met: **this requirement is revised to the polygon test** (the
  region polygon covers the cell, per the CK page) before Phase 1, and `XCLR`
  is not consulted for weather.
- **VLM-REG-009.** On Skyrim and FO4, an interior whose `XCCM` names a
  region (xEdit: "Sky/Weather from Region",
  [TES5 L6544](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsTES5.pas#L6544),
  [FO4 L9052](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L9052))
  MUST take its portal palette and portal sun from that region's weather
  table. Today `CellData::sky_region` is decoded and unread. Census, 2026-11-07 (`region_weather_census`): Skyrim.esm has 214 such interiors, all naming a region present in the master, 213 of them with a Weather section and none with more than one overriding section; Fallout4.esm has 79, 77 with a Weather section. On
  Oblivion / FO3 / FNV the same tag is a climate
  ([FNV L5236](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFNV.pas#L5236));
  the parser already splits the two readings, so no per-game branch is added
  downstream.
- **VLM-REG-010.** Region weather resolution MUST run once per cell
  crossing inside `env_translate`, and its output MUST be a `WeatherRecord`
  choice only: no region, game or record identity reaches `WeatherDataRes`,
  `SkyParamsRes` or the renderer
  ([EXAL single boundary](../engine/exal.md#3-the-single-boundary-proposed)).
- **VLM-REG-011.** `env.health` MUST print one `weather.source=` line naming
  the weather source class (§2), the deciding record's EDID and form ID, and
  for a region its Priority and Override bit; in an interior the line names
  the source of the portal weather.
- **VLM-REG-012.** When two or more overriding Weather sections of a cell's
  regions share the highest Priority, none of them MUST take precedence and
  the section with the next-highest Priority MUST be used, as the CK page
  states ("If two or more overlapping regions have the same priority, none
  of them take precedence in the common area and the region with the next
  highest priority will take effect"). Test:
  `region_weather_priority_tie_falls_through` (new). **Phase 0: verify**
  the rule on Oblivion / FO3 / FNV, whose CS and GECK pages state no tie
  rule. **Census, 2026-11-07: moot there.** Cells with two or more overriding
  Weather sections: Oblivion 0, FNV 0, FO3 1,258, and no FO3 cell shares its
  highest Priority between two regions. No vanilla Oblivion / FO3 / FNV
  exterior cell exercises the rule. Skyrim (where the CK page states it) has 9
  cells whose top Priority is shared by regions with different weather tables
  (first: Tamriel (-13,26), regions `000C5852` and `000C5858`, Priority 50), which
  `region_weather_priority_tie_falls_through` should cover with real data; FO4
  has 0.
- **VLM-REG-013.** On Oblivion / FO3 / FNV, the order between a cell's
  `XCCM` climate override and its regions' Weather sections MUST follow the
  original engine's rule. **Phase 0: verify** — no cited page states it.
  **Census, 2026-11-07: moot.** No exterior cell of `Oblivion.esm`,
  `Fallout3.esm` or `FalloutNV.esm` carries an `XCCM` (0 of 33,540, 41,958 and
  30,096 cells); the tag appears on interiors only (Oblivion 55 of 1,855, FO3 1
  of 421, FNV 0 of 388). No vanilla master exercises the order; DLC masters and
  mods are unsurveyed (the probe resolves one master at a time).
- **VLM-REG-014.** A forced weather set without the override lock MUST
  yield to the resolved region or climate weather at the next weather
  resolution (the cell crossing of VLM-REG-010). This is the deterministic
  reading, under §10 Q2's recommendation, of the GECK page's "Otherwise, the
  weather will change back in the normal fashion according to the weather
  system", and is the use `ForcedWeather::locked` was kept for
  (`obscript_state.rs:100-103`). Test:
  `unlocked_forced_weather_yields_at_next_crossing` (new).

### 5.2 Authored fog profile (`VLM-HF`)

- **VLM-HF-001.** The legacy-ramp fit MUST include the authored fog power
  where the record carries one (FO3 / FNV / Skyrim / FO4 / FO76 `WTHR`
  `FNAM`; `XCLL` on FO3 / FNV and later). The GECK *Weather* page defines
  it: "Fog effects are calculated as a curve. A value of 1 is linear, less
  that one pushes the curve toward the near value, and a value above 1
  pushes the curve toward the far value" (sic). Today `from_legacy_ramp`
  takes near, far and maximum opacity only. **Phase 0, settled 2026-11-07 by disassembly** (`scripts/sdp-fog-term.py` over
  Fallout: New Vegas's `Data/Shaders/shaderpackage010.sdp`): 121 of its 198 fog-reading shaders are `vs_2_0`
  vertex shaders with one identical sequence, in which `FogParam` is read as `c14.xyz` and the result is
  written to the vertex colour's alpha: `d = length(clip.xyz)`; `t = clamp((FogParam.x - d) * (1 / FogParam.y), 0, 1)`;
  `fog = exp(log(1 - t) * FogParam.z)`, that is `fog = (1 - t)^power`, with the fog colour passed through
  unchanged. Read with `x` as the far distance, `y` as far minus near and `z` as the power (the component
  assignment is the uploader's, inferred from the arithmetic, not read from the shader), `fog = saturate((d - near) / (far - near))^power`:
  a power of 1 is linear, below 1 raises the fog sooner (toward the near value), above 1 later (toward the far
  value), which is the GECK page's direction. The distance `d` the shader uses is the length of the three clip-space
  coordinates before the perspective divide, not a view-space distance; whether the engine reproduces that or uses
  view distance is an engine choice to record under VLM-HF-008. The pixel-shader fog (34 `ps_2_x` shaders with `pow`)
  is not read here.
- **VLM-HF-002.** *Shipped in Phase 1 (`FogMedium::from_legacy_ramp_with_power`, `unit_fog_power_keeps_the_linear_fit`; the power-1 fit is bit-identical).* A power of 1 MUST reproduce today's fitted extinction exactly, so that every Oblivion record (its `WTHR` `FNAM` is day / night
  near and far only,
  [wbDefinitionsTES4.pas L4726-L4731](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsTES4.pas#L4726-L4731))
  and every power-1 record is unchanged. Test:
  `unit_fog_power_keeps_the_linear_fit` (new).
- **VLM-HF-003.** The FO4 / FO76 `WTHR` far height layer (Far Height Mid,
  Far Height Range) and High Density Scale (`WeatherHeightFog`) MUST reach
  `FogMedium`. xEdit names the fields
  ([wbDefinitionsFO4.pas L15730-L15748](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L15730-L15748))
  but documents no semantics. **Phase 0: verify** what the near / far
  layers and the density scale mean (two stacked exponentials, a near-to-far
  distance blend, or a density multiplier above the mid height); settled by a corpus census of the authored values per weather together with matched in-game captures of two weathers that differ only in those fields. **Census, 2026-11-07** (`fog_godray_census`): 63 of 71 `Fallout4.esm` weathers carry the layer, in 16 distinct day tuples; the commonest (24 weathers, e.g. `FXNukeWeather`) is the neutral default (near mid 0 / range 10,000, density scale 1.00, far mid 0 / range 10,000); the authored spread is near mid 0 to 1004, near range 1,000 to 25,000, density scale 0.90 to 1.00, far mid -6,400 to 12,000, far range 10,000 to 15,000 (two values). The semantics are still unverified and need the matched captures (filed as #6771). Those captures have to be of the original engine: the fields are unread here, so captures from this engine cannot differ. The census finds the pairs to observe, same distance ramp, power and maximum opacity and a different height tuple: `CommonwealthClear` (`0002B52A`) against `CommonwealthClearestSkies` (`001D670E`), which differ only in high density scale 0.95 against 0.90, and `CommonwealthGSFoggy` (`001BD481`) against `CommonwealthGSRadstorm` (`001C3D5E`), which differ only in far height mid 12,000 against 64 (their colours and clouds also differ). Until
  then the fields stay unread, as `height_mid` is today.
- **VLM-HF-004.** The FO4 and FO76 height-fog tail MUST be decoded from
  both `CELL` `XCLL` and `LGTM` `DATA`. xEdit defines it after the
  Skyrim-shaped body: Near Height Mid, Near Height Range, Fog Color High
  Near / High Far, High Density Scale, four near / far / high scales, Far
  Height Mid, Far Height Range
  ([XCLL, wbDefinitionsFO4.pas L9003-L9013](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L9003-L9013);
  [LGTM, L11667-L11677](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L11667-L11677)).
  *Shipped in Phase 1: `CellLighting::height_fog` / `LgtmRecord::height_fog` hold the `HeightFogTail` (FO4 / FO76, bytes 92..; `CellLighting::height_fog_tail()` maps Starfield's onto the same type).* Before Phase 1 the parser accepted the FO4 128 / 136-byte `XCLL` sizes (`fo4_xcll_sizes_pinned`) but decoded the tail of either record only for Starfield. Tests: `parse_cell_fo4_xcll_decodes_height_fog_tail`, `parse_lgtm_fo4_decodes_height_fog_tail`.
- **VLM-HF-005.** Cell height fog from VLM-HF-004 and from Starfield's `StarfieldLighting` MUST be translated by one shared function into the same `FogMedium` fields the weather path fills, with the Phase 0 semantics of VLM-HF-003. *Shipped in Phase 1: `FogMedium::from_authored_fog` is the one translation (ramp, power, maximum opacity and near height range) the weather day / night media, the day snapshot and `CellLightingRes::from_cell_lighting` all use; only the near height range is consumed, per VLM-HF-003.* Before Phase 1 `CellLightingRes.starfield` was forwarded and unread.
- **VLM-HF-006.** Every new `FogMedium` field MUST interpolate in
  `FogMedium::lerp` so weather and time-of-day cross-fades carry it, as the
  scale height does today (`lerp_interpolates_the_scale_height`).
- **VLM-HF-007.** The froxel grid and the analytic tail MUST evaluate the
  same height profile: with a pinned camera on an exterior fixture,
  changing `--fog-grid-far-m` from its default to its maximum MUST NOT move
  the mean luminance of pixels beyond the larger far plane by more than the
  run-to-run noise of a repeated default capture (the regression `composite.frag` records as "the mountains changing appearance when the grid's reach changes"). **Shipped in Phase 4 (2026-11-08).** The cause was two disagreements between the grid and the tail, both fixed: the grid multiplies the medium by a noise factor (`proceduralDensityScale`, `mix(0.2, 1.35, density)`) whose mean at the default coverage is below 1 while the tail integrated the unmodulated coefficient (the host now passes the tail `fog_mean_density_scale(coverage)`, computed from the noise volumes' histograms), and the tail's height term did not clamp at the reference height as the grid's does (`heightFogOpticalDepth` now integrates the clamped profile piecewise). Re-measured with `fog-grid-seam.sh` (128 m against 512 m, repeat noise ~0): noon FNV hills right -14.2 to -1.8 (earlier build: -5.6 to -1.8), Skyrim terrain haze -4.2 to -0.1, sky +3.7 to +0.3, and at night FNV hills left +5.3 to +0.6, sky band +5.5 to +0.04; every region now moves at most about 2 levels. The literal bar (no more than the repeat's noise, which is zero in `renderer-stepped`) cannot hold where the grid resolves real shadows beyond 128 m, so the gate is a tolerance of 2.5 levels (`SEAM_TOLERANCE`), an engine choice for the owner to confirm. **Baseline at HEAD before Phase 4, 2026-11-07 (`docs/smoke-tests/fog-grid-seam.sh`): the requirement did not hold.** In `renderer-stepped` mode a repeat capture differs by at most 0.004 levels per region, and moving the grid from 128 m to 512 m shifts the distant regions by: FNV Goodsprings, hills right -5.56, hills left +0.18, sky band -0.63, whole frame -0.54; Skyrim Whiterun elevated pose, terrain haze -4.18, far ridge +6.02, forest +0.02, sky +4.83, whole frame +0.42. The regions are windows over distant content (no depth view exists), not a depth mask.
- **VLM-HF-008.** Any fog-profile value the engine supplies that the
  content did not author (a default scale height, a curve chosen because
  VLM-HF-001 could not be verified, an exterior dust floor from §10 Q7)
  MUST be recorded as an engine choice in the style of
  [EXAL §3.1](../engine/exal.md#31-engine-choice-base-fog-strength-ext-d1-2026-09-29-02)
  and printed by `env.health`. `BASE_FOG_STRENGTH` is one such choice and is
  not reopened here; it MUST scale the new fields' extinction identically.
- **VLM-HF-009.** A decoded fog or god-ray field that Phase 0 rules out of
  the medium MUST be listed with its reason in the design's
  [corpus findings](../engine/procedural-volumetric-fog.md#corpus-findings).
- **VLM-HF-010.** Skyrim SE's per-time-of-day `WTHR` `HNAM` "Volumetric
  Lighting" references and the `VOLI` records they name MUST be decoded
  (xEdit:
  [WTHR HNAM L13145-L13150, VOLI L13164-L13178](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsTES5.pas#L13145-L13178),
  both under `IsSSE`). Today neither is parsed. *Shipped in Phase 1: `parse_voli`, `WeatherRecord::volumetric_lighting`, `EsmIndex::volumetric_lighting`.* Test: `parse_wthr_sse_hnam_and_voli`. **Starfield:** `Starfield.esm` also has 42 `VOLI` records (and `ShatteredSpace.esm` 2), but they are 1,832-byte `BETH` reflection blobs (`REFL`, `BGSVolumetricLighting`: `VLFog_Weather_Clear`, `VLFogHeightUniqueRedmile`, …), not Skyrim's thirteen floats; Phase 1 walks and does not store them, and they need the reflection decoder (the one `WTHS` uses). **Census, 2026-11-07:** the records are in `Update.esm` (36 `VOLI`, 69 weathers with a 16-byte `HNAM`), `Dawnguard.esm` (1 and 15) and `Dragonborn.esm` (5 and 9), not in `Skyrim.esm`, so the decode must run over the merged load order (Dawnguard names 9 distinct `VOLI`, 1 in its own file). Of the nine float fields, six are the same in all 42 records (Custom Color - Contribution 0, Density - Contribution 0.3, Size 10, Wind Speed 15, Falling Speed 0.3, Range Factor 40); only `Intensity` (1.0 to 4.0) and the two phase fields (0.70 to 0.85) vary. Whether any decoded field reaches
  `FogMedium` is §10 Q8; until then each is listed under VLM-HF-009.
- **VLM-HF-011.** FO4's `GDRY` god-ray records and the four links to them —
  `WTHR` `WGDR` per time of day, `CELL` `XGDR`, `LGTM` `WGDR`, `LIGH` `WGDR`
  with its god-ray near clip (`DATA` byte 52, `LightData::god_rays_near_clip`; byte 24 is the
  separate shadow-bias `Near Clip`, #6795) — MUST be decoded (xEdit FO4:
  [WTHR L15795-L15804](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L15795-L15804),
  [CELL L9057](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L9057),
  [LGTM L11680](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L11680),
  [LIGH L12849-L12856](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L12849-L12856),
  [GDRY L15956-L15969](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L15956-L15969)).
  The Fallout CK documents the light-side use: "Volumetric Light: Whether
  this light should cast Volumetric rays (GodRays)" and "If 'Volumetric
  Light' is checked here you HAVE TO choose the God Ray profile"
  (`falloutck-uesp-wiki/(main)/Light.wiki`). **Census, 2026-11-07** (`Fallout4.esm`): 61 `GDRY` with a 60-byte `DATA` in every record; 63 weathers carry a 32-byte `WGDR` (8 time-of-day slots, 40 weathers non-null, 54 distinct `GDRY`), 7 cells an `XGDR`, 20 lights a `WGDR`, no lighting template one; every reference resolves. `Intensity` runs 0 to 50 (mean 3.5). *Shipped in Phase 1: `parse_gdry`, `EsmIndex::god_rays`, `WeatherRecord::god_rays`, `CellData::god_rays_form`, `LgtmRecord::god_rays_form`, `LightData::god_rays_form` and `near_clip`; the parsed index agrees with the raw census (61 `GDRY`, 63 weathers, 7 cells, 20 lights).* Tests: `parse_fo4_gdry_and_links`, `parse_cell_xgdr_god_rays`, `parse_ligh_wgdr_and_near_clip`. Consumption follows §10 Q8.

### 5.3 Exterior light shafts (`VLM-SH`)

UE5's Volumetric Fog supports "a single Directional Light, with shadowing
from Cascaded Shadow Maps or static shadowing" and shows fog "coming from the
directional light source off screen through the arch", using "a heavy
temporal reprojection filter"; its View Distance is "the distance from the
camera over which Volumetric Fog will be computed"
([Epic, Volumetric Fog in Unreal Engine](https://dev.epicgames.com/documentation/en-us/unreal-engine/volumetric-fog-in-unreal-engine)).
The froxel grid follows Wronski's unified compute-shader froxel scheme
([Wronski 2014](https://bartwronski.com/wp-content/uploads/2014/08/bwronski_volumetric_fog_siggraph2014.pdf))
and Hillaire's physically based, energy-conserving integration
([Hillaire 2015, Physically Based and Unified Volumetric Rendering in Frostbite](https://www.frostbite.com/2015/08/physically-based-unified-volumetric-rendering-in-frostbite/)).
The RT visibility replaces cascades (VLM-NG-006).

- **VLM-SH-001.** A game-data-free exterior lab (`--shaft-lab`, new, a
  sibling of `--godray-lab`) MUST place a slotted occluder under a low sun
  in open sky with an authored homogeneous medium, in an open and a solid
  variant.
- **VLM-SH-002.** The new script `docs/smoke-tests/exterior-shafts.sh` MUST
  capture both lab variants and fail unless the open variant's shaft
  contrast exceeds the solid variant's by more than twice the run-to-run
  noise of a repeated open capture.
- **VLM-SH-003.** The lab MUST be pinned by a golden frame,
  `shaft_lab_golden_frame` (new, `byroredux/tests/golden_frames.rs`), in the
  pattern of `combustion_lab_golden_frame`.
- **VLM-SH-004.** The froxel sun term MUST take its cloud shadow per froxel
  from the SKYAL cloud-shadow map when the shaft range decided in §10 Q3
  exceeds one cloud-shadow texel; while it does not, the camera-column value
  (`volumetric_sun`) MAY remain.
- **VLM-SH-005.** Shafts MUST reach the range §10 Q3 decides; the range is
  observable as the farthest occluder distance at which the lab's shaft
  contrast still passes VLM-SH-002. **Measured 2026-11-07** (`--shaft-lab-distance`, default 128 m grid, the
  gate's 0.05 bar, open minus solid contrast): 25 m 0.233, 40 m 0.237, 55 m 0.154, 70 m 0.131, 90 m 0.071,
  110 m 0.031, 128 m 0.004, 150 m 0.001. The shaft passes out to somewhere between 90 and 110 m and is gone at
  the grid's far plane, so the observable range is about 90 to 100 m of the 128 m grid. Whether that is the range
  the owner meant by "the froxel grid only" (§10 Q3) is theirs to say.
- **VLM-SH-006.** On each exterior fixture of §7.2, a noon and a low-sun
  capture under the fixture's vanilla weather MUST be recorded with the
  shaft contrast across a named occluder. **Phase 0: verify** whether
  vanilla clear-weather extinction makes any exterior shaft visible at all
  (`env.health` prints `gpu_extinction=` per weather); if it does not, §10 Q7 decides whether an engine-choice haze floor is added. **Content measurement, 2026-11-08** (`docs/smoke-tests/exterior-shafts-content.sh`, #6773): the in-scatter term at noon, 07:00 and 17:30 on the five fixtures is recorded in the fog design (the sun response is large on every fixture), but a contrast across a named occluder is not measurable at the default poses: the froxel view samples at pixel depth, so an occluder's column reads darker from nearness, not shadow. Restated need: a depth-matched pose per fixture (a far surface behind an occluder), which is pose authoring for the owner to scope.
- **VLM-SH-007.** Under a pinned camera and frozen clock, frame-to-frame
  luminance variation inside a shaft region MUST be captured as a frame
  sequence (`--bench-capture-every`, `--bench-capture-dir`) and reviewed by
  the owner; the author does not judge noise, as the fire plan's ground rule
  sets ([ground rules](../engine/fire-volumetrics-plan.md#ground-rules)).
- **VLM-SH-008.** Inject and integrate GPU time MUST be recorded
  separately, not combined, at the default grid on each exterior fixture,
  following ROADMAP's bench methodology
  ([How to run a bench](../../ROADMAP.md#how-to-run-a-bench)), and published
  in the design's
  [measurement table](../engine/procedural-volumetric-fog.md#measurement-table)
  (the table itself asks for this split).
- **VLM-SH-009.** The lab MUST include an alpha-tested card in three matched variants (`--shaft-lab-card`,
  `--shaft-lab-card-geo`, `--shaft-lab-card-solid`): one panel whose odd stripes are cut out by alpha, the
  same stripes as real holes, and one unbroken panel. Their finished frames are identical, so the in-scatter
  term over the hole columns differs only where the froxel sun trace stops at a cutout. The cutout panel MUST
  come within 2 % of the geometry holes (`exterior-shafts.sh`), the backbone's "Cutouts use alpha coverage"
  rule ([runtime contract](../engine/physical-lighting-backbone.md#runtime-contract)) observed in the froxels.
  **Phase 0, measured 2026-11-07:** cutout 116.36, geometry 129.16, solid 93.40, a 9.9 % gap, so it failed
  with the opaque traversal, as predicted. **Met the same day** by the alpha-aware sun trace (§3 row 17):
  cutout 129.18, geometry 129.16, solid 93.40, a gap of 0.0 %. The criterion this requirement first carried, shaft
  contrast behind cutout texels against behind opaque texels, is withdrawn: it reads 0.106 on an opaque trace,
  because a view ray through a cutout also sees the lit haze beyond the panel and a ray that ends at an opaque
  texel does not. Only a see-through reference that is real geometry removes that difference.

- **VLM-SH-010.** The engine MUST supply an exterior haze floor, a minimum
  outdoor extinction recorded as an engine choice under VLM-HF-008 (§10 Q7).
  It MUST raise the global medium's extinction only where the authored
  extinction is below it, so it never thins authored haze; it MUST be a smooth
  function of the sun's elevation that rises toward the horizon, where the
  slanted path through air and dust is longer; and it MUST be zero in an
  interior, where the interior dust floor already applies, and below the
  horizon at night.
- **VLM-SH-011.** The floor's default MUST be chosen from captures and
  published: on each exterior fixture of §7.2 a noon and a low-sun capture with
  the floor off and on, the shaft contrast across a named occluder (VLM-SH-006)
  and the mean luminance change of pixels outside any shaft. The default MUST
  make the low-sun shaft pass VLM-SH-002's contrast rule and MUST keep the
  noon non-shaft luminance change inside the tolerance the owner reviews in
  the capture set, so the floor makes shafts visible without washing out the
  authored look. The floor MUST be adjustable at run time through the existing
  volumetrics settings surface and MUST report its live value on `env.health`.

### 5.4 Interior coverage (`VLM-INT`)

- **VLM-INT-001.** An opening inventory MUST be checked in for the cells of
  §7.2's interior list: per cell, each sky aperture or shaft medium, the
  geometry evidence that it borders outdoors, and its converter, as the
  handoff's open item 1 asks
  ([open work](../engine/interior-godrays-status.md#open-work-and-next-experiments)).
- **VLM-INT-002.** Each listed cell MUST pass a day / night pair through
  `--bench-hold` + `byro-dbg` (`time.set 12`, `screenshot`, `time.set 0`,
  `screenshot`): the shaft region's luminance at noon exceeds midnight by
  more than twice the noise of a repeated noon capture.
- **VLM-INT-003.** A sealed interior with no opening in the list MUST show
  no shaft in the same procedure (noon and midnight within noise), the
  property `interior-godrays.sh` proves on the lab.
- **VLM-INT-004.** Each listed cell MUST pass VLM-INT-002 on both a direct
  `--cell` boot and an exterior-to-interior door transition.
- **VLM-INT-005.** When marked sky apertures exceed `MAX_SKY_APERTURES`,
  the renderer MUST log the number dropped once per cell and MUST keep the
  apertures nearest the camera. Today the upload stops at the cap in
  iteration order without a log (`context/frame_params.rs:903`).
- **VLM-INT-006.** Converter coverage MUST be published per game as
  converted placed beam references ÷ placed beam references from
  `crates/plugin/examples/baked_beam_census.rs`, extended to the DLC masters
  and to FO4 and Starfield, which the handoff says are unsurveyed. The ratio MUST be read from a live cell,
  not from the unit tests: a converter that no loader path reaches measures 0 ÷ N however well its tests pass.
  Found on 2026-11-07: the `fxlight` early return in `synth_child.rs` kept every FNV `FXLightBeam*` placement from
  loading its NIF, so all FNV beam converters measured 0 live until the return was narrowed
  (`Vault11a` 14 ÷ 14, `SLAbandonedShack03INT` 5 ÷ 5 afterwards). **Denominators, 2026-11-07:** placed beam references for every official master and DLC are in [interior-shaft-openings](../engine/interior-shaft-openings.md#placed-beam-census-every-title-and-official-dlc-vlm-int-006-denominators-measured-2026-11-07); Fallout 4 is the one title with many beams in Show Sky cells (657 of 1,604 interior placements), and, from #6774 (2026-11-08), every title has one: FO4 about 100 % (all 9 placed models), Oblivion 92.9 % (the #6731 gate now includes it; 66 volumes live in a 52-placement cell), Skyrim 0 of 1, Starfield 0 of 1 ([interior-shaft-openings](../engine/interior-shaft-openings.md)).

- **VLM-INT-007.** Where the sun's ray has been proven to pass through an authored aperture (a marked
  `SkyAperture` plane or a `LightShaft` cone), the sun term MUST scatter off an engine-supplied shaft dust
  (`INTERIOR_SHAFT_DUST_PER_METER`, an engine choice recorded under VLM-HF-008, the interior counterpart of
  VLM-SH-010). It MUST ride the sun term only, so the dark room, the local lights and the night gain nothing from
  it, and it MUST NOT apply to open sky, glass or rim-probe visibility. Measured on FNV `NellisHangar1` (#6727): the
  converted planes are thin ribbons (about 8 m x 1.4 m), and without the dust the noon-minus-midnight radiance
  difference is about 0 over six cameras; with 0.04 / m it is 3.6 to 7.3 % of pixels above 3 levels (max 19), and
  in the shaft region noon reads 28.3 against 21.6 at midnight (repeat noise 0.16).

### 5.5 Aerial perspective for distant geometry (`VLM-AP`)

SKYAL already applies Hillaire's Earth atmosphere to cloud layers through a
closed-form Chapman integral, chromatically neutral by the owner's rule
([SKYAL §2.4](../engine/skyal.md#24-far-to-near-composite-upper-cloud-plane-aerial-perspective-2026-10-20)).
Hillaire 2020 adds a camera-frustum aerial-perspective volume applied to
opaque geometry as a post process, 32 × 32 over the screen by default
([Hillaire 2020, A Scalable and Production Ready Sky and Atmosphere Rendering Technique](https://sebh.github.io/publications/egsr2020.pdf), §5.4).

- **VLM-AP-001.** Distant geometry MUST fade toward the sky drawn behind it:
  on an exterior fixture with a far ridge against the horizon, the linear
  luminance difference between ridge pixels and the adjacent sky pixels
  MUST decrease monotonically with ridge distance across the distant-LOD
  band ([distant-LOD completion PRD](distant-lod-completion.md) owns the
  geometry). Before Phase 4 the analytic tail's source was the grid boundary radiance clamped to the authored fog luminance, not the sky. *Phase 4 shipped the air fade: every exterior surface now fades through `sky_air_terms` toward `sky_clear_radiance` (the clouds' own haze colour).* The monotonic ridge sweep is not measured: it needs a fixture with a far ridge at controlled distances under distant LOD (the distant-LOD PRD owns the geometry), and moving this repo's fixtures' cameras km back leaves the loaded area and the frame stops being the same scene.
- **VLM-AP-002.** The tail's air term MUST be the same function SKYAL uses
  for cloud layers (`sky_air_terms`, `shaders/include/sky.glsl:270`), so the
  haze in front of a mountain and in front of a cloud at the same distance
  match; a pinned shader-source test (`analytic_tail_uses_the_sky_air_terms`) keeps them shared. *Shipped in Phase 4.*
- **VLM-AP-003.** If §10 Q4 chooses the aerial-perspective volume, its GPU
  time MUST be recorded in the measurement table's "Aerial LUT on" row and
  the visual difference against the analytic tail captured for the owner.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Fog follows the authored near / far ramp with a power curve (GECK *Weather*, *Pow*) | A physical medium fitted once at the EXAL boundary ([runtime contract](../engine/procedural-volumetric-fog.md#runtime-contract)) | Fog-distance tweak mods aimed at the ramp's banding | Density or hue drift from the authored look → VLM-HF-001, VLM-HF-002, VLM-HF-008 |
| 2 | Painted beam cards and cone fans draw shafts | Cards suppressed; shafts emerge from medium and RT visibility | Godray texture packs | An opening with no converter loses its beam → VLM-INT-006 |
| 3 | The weather system changes weather over time by chance, as often as the climate's volatility sets (CK / GECK *Climate*: "Use volatility slider to determine how often the weather should change"; GECK *ForceWeather*: "the weather will change back in the normal fashion according to the weather system") | Per §10 Q2: a deterministic highest-chance pick per table | — | Less weather variety than the original → VLM-REG-004, VLM-REG-014, Q2 |
| 4 | Fog thickness as authored | Every medium thinned by `BASE_FOG_STRENGTH` ([EXAL §3.1](../engine/exal.md#31-engine-choice-base-fog-strength-ext-d1-2026-09-29-02)) | — | Faint outdoor shafts → VLM-SH-006, Q7 |
| 5 | Skyrim SE and FO4 author god-ray tuning records (`VOLI` per weather time of day; `GDRY` per weather, cell, lighting template and light — xEdit, §5.2; Fallout CK *Light*), and FO4 scripts can switch god rays off (`falloutck-uesp-wiki/(main)/ForceDisableSSRGodraysDirLight - Game.wiki`) | One medium for fog, shafts and local media, RT-shadowed; the authored god-ray records are decoded and translated only as §10 Q8 decides | Screen-space godray mods | Shaft range bounded by the grid → VLM-SH-005, Q3; authored god-ray intent lost → VLM-HF-010, VLM-HF-011, Q8. **Phase 0: verify** how many `VOLI` and `GDRY` records and links the vanilla masters carry (the fog design already counts zero `VOLI` in FO4), so a reviewer can compare like with like |
| 6 | An interior shows the sky of its region (Skyrim / FO4 `XCCM`) | Same, once VLM-REG-009 lands | — | Until then, those interiors show the worldspace palette → VLM-REG-009 |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

The waves follow the engine tiers in
[Game Compatibility](../engine/game-compatibility.md#per-game-support-detail):
the region and profile work is translation-only and lands for every title
whose records carry it; the acceptance captures need a loading cell route.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 1. Region weather tables with the weather-and-chance row (VLM-REG); no fog power (VLM-HF-002 keeps it unchanged); exterior fixtures already run in `m-exteriors.sh` |
| Fallout 3 | yes | Wave 1. Region weather with globals; fog power; MegatonWorld exterior and the painted-fan interiors the handoff lists |
| Fallout: New Vegas | yes | Wave 1, reference title. WastelandNV exteriors; Repcon / Helios / Nellis interiors |
| Skyrim (LE / SE / AE) | yes | Wave 1. Region weather; interior `XCCM` sky-from-region (VLM-REG-009); Tamriel exteriors. SE / AE also carry `VOLI` (VLM-HF-010); LE does not (xEdit gates it on `IsSSE`) |
| Fallout 4 | yes | Wave 2. `WTHR` far height layer, the `XCLL` / `LGTM` tail and `GDRY` (VLM-HF-003, -004, -011), gated on their Phase 0 semantics; Commonwealth exteriors |
| Fallout 76 | translation only | The `WTHR`, `XCLL` and `LGTM` translation is shared with FO4 and lands with it; no acceptance capture, because FO76 exteriors are a named policy skip in `m-exteriors.sh` (#4507) |
| Starfield | interiors only | Wave 2. `XCLL` / `LGTM` height fog for walkable interiors (VLM-HF-005); Starfield exteriors are a named policy skip in `m-exteriors.sh`, so VLM-REG and VLM-SH do not apply |

No requirement adds a per-game branch past `env_translate`: the `RDWT`
stride, the `XCCM` reading, the `XCLL` / `LGTM` tail layouts and the
god-ray records are settled at the parser, and region and profile choices
at the EXAL boundary.

### 7.2 Acceptance

Each scenario names its data. Game files are read from the user's install
through `--game <profile>`; nothing is checked in but scripts, fixtures that
name cells, and captured numbers. New smoke scripts are indexed in
[`docs/smoke-tests/README.md`](../smoke-tests/README.md) in the commit that
adds them.

| # | Scenario | Drives | Data | Passes when |
|---|---|---|---|---|
| A1 | Region weather resolution | `cargo test` names in VLM-REG-001, -003, -006, -012, -014 | none (synthetic records) | all pass |
| A2 | Region weather census | `crates/plugin/examples/region_weather_census.rs` (rules and counts) and the ignored `region_weather_coverage_census` test in `env_translate/region_weather.rs` (the resolver over every exterior cell: `BYROREDUX_REGION_CENSUS_ESM=<master> cargo test -p byroredux --bin byroredux -- --ignored --nocapture region_weather_coverage_census`) | Oblivion, FO3, FNV, Skyrim masters | every exterior cell with a Weather-bearing region reports a region source; tie and `XCCM`-plus-region counts recorded (VLM-REG-012, -013); counts published (§8). **Coverage at Phase 1, cells a Weather-bearing region polygon covers that resolve to a region:** Oblivion 8,476 of 8,533, FO3 3,068 of 3,068, FNV 1,595 of 1,595, Skyrim 7,756 of 7,760, FO4 3,911 of 3,920 |
| A3 | Live region weather | `--bench-hold` + `byro-dbg`: `env.health` at two fixture cells in different regions, named by A2's census | FNV `WastelandNV`, Skyrim `Tamriel` | the two `weather.source=` lines name different regions, and a cross-fade runs between them (VLM-REG-007, -011) |
| A4 | Sky-from-region interior | `--bench-hold` + `byro-dbg`: `env.health` in one `XCCM` interior | Skyrim (an interior from Phase 0's `XCCM` census) | the `weather.source=` line names the `XCCM` region, not the worldspace climate (VLM-REG-009, -011) |
| A5 | Fog profile translation | `cargo test` names in VLM-HF-002, -004, -010, -011 plus the Phase 1 translate tests | none | all pass |
| A6 | Grid seam | `docs/smoke-tests/fog-grid-seam.sh`: pinned `--camera-pos` / `--camera-forward`, `renderer-stepped`, captures at two `--fog-grid-far-m` values (a gate since Phase 4, tolerance 2.5 levels) | FNV and Skyrim exterior fixtures | VLM-HF-007 holds |
| A7 | Exterior shaft lab | `docs/smoke-tests/exterior-shafts.sh` (new), `shaft_lab_golden_frame` (new) | none | VLM-SH-002, VLM-SH-003 and VLM-SH-009 pass |
| A8 | Exterior shafts on content | `--bench-hold` + `byro-dbg`: `time.set 12`, `render.debug composite_term`, `screenshot`, then a low-sun hour | the `m-exteriors.sh` fixture cells for Oblivion, FO3, FNV, Skyrim, FO4 | contrast and per-pass ms recorded (VLM-SH-006, -008) |
| A9 | Interior day / night | `--bench-hold` + `byro-dbg` per VLM-INT-002, `docs/smoke-tests/interior-shafts-content.sh` | the sun-path census list in [interior-shaft-openings](../engine/interior-shaft-openings.md#sun-path-census-vlm-int-001-measured-2026-11-09-6727): the cells with a marked aperture or `LightShaft` volumes in a Show Sky cell (FNV Nellis hangars, six FO4 cells), the controls with no such path, and the gap cells (the handoff's candidates, first list, mostly had none) | each listed `shaft` cell passes VLM-INT-002, each control VLM-INT-003, and VLM-INT-004 on both routes (eight of nine through a door; Trinity has no door route) |
| A10 | Interior lab regression | `docs/smoke-tests/interior-godrays.sh` (existing) | none | unchanged pass |

A scenario passes when every row it names passes on the same commit and the
captures are retained under the run's output directory.

---

## 8. Metrics

One home for the numbers: the ROADMAP M55 row
([Tier 8](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon))
holds the coverage and pass counts and links the design's
[measurement table](../engine/procedural-volumetric-fog.md#measurement-table)
for per-pass milliseconds.

| Metric | Definition | Granularity |
|---|---|---|
| Region weather coverage | exterior cells whose weather source is a region ÷ exterior cells with a Weather-bearing region in `XCLR` | per game master |
| Profile field coverage | decoded fog and god-ray fields reaching `FogMedium` ÷ decoded fields not ruled out under VLM-HF-009 | per game |
| Exterior shaft contrast | VLM-SH shaft contrast at noon and low sun | per exterior fixture |
| Shaft range | farthest passing occluder distance in the lab (VLM-SH-005) | per grid configuration |
| Interior pass count | listed cells passing VLM-INT-002 to -004 ÷ listed cells | per game |
| Converter coverage | VLM-INT-006 | per game, base and DLC |
| Volumetric cost | inject ms and integrate ms, separately | per fixture, per grid configuration, per bench record |
| Modern parity | items of the reference's feature list met ÷ items not declared non-goals | per revision |

---

## 9. Delivery sequence

Dependencies: M34 (exterior lighting and time of day, completed), M44 (the
REGN-parsing dependency the ROADMAP row names, completed),
SKYAL cloud shadows (shipped), and the exterior fixtures of
`m-exteriors.sh`. The fire plan (#6531) runs independently; neither blocks
the other.

**Phase 0 — Verify, census and baseline (no engine code).** Resolve §10.
Close every **Phase 0: verify** item: region weather replacing the climate
list (§5.1), the non-override region rule (VLM-REG-002), `XCLR` versus
region polygons (VLM-REG-008), the tie rule off Skyrim (VLM-REG-012), the
`XCCM`-versus-region order (VLM-REG-013), the fog-power curve (VLM-HF-001),
the far height layer and density-scale semantics (VLM-HF-003), the
`VOLI` / `GDRY` census (§6 row 5), whether vanilla clear weather yields
visible exterior shafts (VLM-SH-006), and the cutout behaviour of the
froxel trace (VLM-SH-009). Write the region-weather section into
[EXAL](../engine/exal.md) and the god-ray findings into the fog design's
corpus findings. Run the region census probe (read-only example) and the
`XCCM` interior census; extend `baked_beam_census` to DLC, FO4 and
Starfield; capture the exterior baseline (A6, A8) at HEAD. Exit: the verify list is closed or filed as issues, the two design sections, the census
tables and the baseline captures with their per-pass milliseconds are
checked in, and §10 carries **Decided** on every item.

*Status, 2026-11-07 (#6724).* Closed: VLM-REG-008 (revised to the polygon
test), -012 and -013 (moot on Oblivion / FO3 / FNV), VLM-HF-001 (curve form
read from shipped bytecode), VLM-SH-006's extinction question (the haze floor,
Q7) and VLM-SH-009 (the alpha-aware trace). Filed: VLM-REG-002 and the
replace-versus-add question (#6772), VLM-HF-003 (#6771), VLM-SH-006's contrast
across a named occluder (#6773), converter coverage beyond FO3 / FNV (#6774),
and a bench-harness defect found on the way (#6775). Written: EXAL §3.2, the
fog design's corpus findings, the beam-census table. Baselines: A6 (fails at
HEAD, `fog-grid-seam.sh`) and A8's per-pass milliseconds (revision 14).

**Phase 1 — Region weather and the authored profile.** VLM-REG-001 to -014
and VLM-HF-001 to -011, translation and parser only. Exit: A1, A2, A3, A4
and A5 pass; region weather coverage and profile field coverage are
published in the ROADMAP M55 row; `m-exteriors.sh all cycle` stays green.

**Phase 2 — Exterior shafts.** VLM-SH-001 to -009, with the range and the
cloud-shadow sampling Q3 decides and any haze floor Q7 decides. Exit: A7
passes; A8's contrast and inject / integrate milliseconds are published
against the Phase 0 baseline on the same machine; the owner has reviewed
the VLM-SH-007 sequences.

**Phase 3 — Interior coverage.** VLM-INT-001 to -006. Exit: A9's interior
pass count is published per game with every listed cell passing, and A10
still passes.

**Phase 4 — Aerial perspective.** VLM-AP-001 to -003, in the form Q4
decides. Exit: A6 and the VLM-AP-001 ridge sweep pass on the FNV and Skyrim
fixtures, and the modern-parity metric is published.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — Does fire belong under this PRD?** Recommended: no. The
   [fire plan](../engine/fire-volumetrics-plan.md) predates the PRD gate,
   has its own tracking (#6531 and phase issues), owner-reviewed stages and
   ground rules; this PRD lists its items in §3 and treats them as owned
   elsewhere (VLM-NG-001). The ROADMAP M55 row and its Known Issue would then
   name #6531 as the owner of the combustion follow-ups and of the derived
   reach. Alternative: absorb it as an area `VLM-CMB` restating the plan's
   phase exits as requirements, with the consequence that the same work is
   tracked twice and the already-reviewed stages need re-review under the
   rubric.
   **Decided** (2026-11-07): the recommendation — fire stays under the fire plan (#6531); VLM-NG-001 stands.
2. **Q2 — How is a weather picked from a region's table?** Recommended: the
   highest-chance row, the one rule EXAL already fixes for climates
   (`resolve_default_weather`, #5424), so a cell's weather is deterministic,
   saves need no new state, and an unlocked forced weather yields at the
   next cell crossing (VLM-REG-014). Alternative: chance-weighted rolls over
   game time, as the original weather system does; the consequence is a
   seeded random stream in the save (M45), a roll interval derived from the
   climate's volatility (CK / GECK *Climate* name the slider but give no
   formula, so it starts as a **Phase 0: verify** item), and a scope that
   reaches past M55 into weather simulation — EXAL's one-rule default would
   be reopened for climates too.
   **Decided** (2026-11-07): the recommendation — the highest-chance row; an unlocked forced weather yields at the next cell crossing (VLM-REG-014).
3. **Q3 — How far must outdoor shafts reach?** Recommended: the froxel grid
   only — the default far plane, adjustable to the existing
   `--fog-grid-far-m` maximum — with the analytic tail unshadowed; this is
   the bounded-range model UE5 also uses ("View Distance … over which
   Volumetric Fog will be computed", §5.3), and VLM-SH-004 stays on the
   camera-column cloud value. Alternative A: a second, coarser far cascade
   of froxels for long shafts from ridges and cloud gaps, costing a second
   inject / integrate pair and per-froxel cloud-shadow sampling (VLM-SH-004
   active). Alternative B: shadowed in-scattering in the analytic tail by a
   ray march against the cloud-shadow map only, giving crepuscular rays from
   clouds but not from terrain.
   **Decided** (2026-11-07): the recommendation — the froxel grid only, with the analytic tail unshadowed; VLM-SH-004 stays on the camera-column cloud value.
4. **Q4 — Aerial perspective for geometry.** Recommended: keep the analytic
   tail and give it SKYAL's closed-form air term (VLM-AP-002), adding the
   Hillaire 2020 volume only if VLM-AP-001 fails with it. Alternative: build
   the camera-frustum aerial-perspective volume now (the fog design's
   follow-up 4), which adds a pass and a resident 3D texture before any
   measurement says the analytic tail is insufficient.
   **Decided** (2026-11-07): the recommendation — the analytic tail gets SKYAL's closed-form air term (VLM-AP-002); the Hillaire 2020 volume only if VLM-AP-001 fails with it.

**Q5** — withdrawn, rev 2. Rev 1 asked whether to build a non-RT cascade
visibility path; the ROADMAP M55 row already rules it out, so it is cited
as VLM-NG-006 instead of being reopened.
**Decided** (2026-11-07): withdrawn at rev 2; settled by VLM-NG-006.

6. **Q6 — Which modern reference sets the parity checklist?** Recommended:
   keep the one ROADMAP's M55 row names, Hillaire's Frostbite volumetrics
   (SIGGRAPH 2015), which is also the design's reference, so the PRD and the
   design measure against one source and nothing already decided is
   reopened. Alternative A: UE5 Volumetric Fog with Exponential Height Fog,
   publicly documented and current (§5.3); the consequence is a ROADMAP row
   edit, and the parity metric is still a feature-list reading, because no
   UE5 build renders the same content. Alternative B: Red Dead Redemption
   2's atmosphere as a higher bar, which has no public build or
   documentation beyond talks and would make the metric a reading of a talk.
   **Decided** (2026-11-07): the recommendation — Hillaire's Frostbite volumetrics (SIGGRAPH 2015) stays the one parity reference.
7. **Q7 — An exterior haze floor if vanilla weather shows no shafts.**
   Recommended: none; shafts appear where the content authors haze, and
   VLM-SH-006 reports where they do not. Alternative: a minimum outdoor
   extinction recorded as an engine choice (VLM-HF-008), like the interior
   `with_sky_aperture_dust_floor`, which makes shafts visible everywhere at
   the cost of thickening every clear exterior beyond its authored look.
   **Decided** (2026-11-07): the alternative, not the recommendation — the engine supplies a minimum outdoor extinction recorded as an engine choice under VLM-HF-008. It exists so exterior shafts are visible on clear weather, which the owner's goal requires; its form is bounded by VLM-SH-010 so it never thins authored haze and is sun-angle aware, and VLM-SH-011 owns its measurement. The consequence the alternative names (a thicker clear exterior than the authored look) is accepted, and VLM-SH-011 caps it.
8. **Q8 — What do the authored god-ray records feed?** Recommended: decode
   and census them (VLM-HF-010, -011) and feed nothing into `FogMedium`
   until Phase 0 settles each field's meaning from a source; fields that
   tune the original screen-space effect rather than describe the medium
   are listed under VLM-HF-009, consistent with VLM-NG-003. Alternative A:
   map them now onto the medium (density and phase-function fields onto
   extinction and anisotropy, colour onto albedo), which gives Skyrim SE and
   FO4 their authored shaft look at the cost of guessing semantics no source
   documents. Alternative B: declare them a non-goal and skip the decode,
   which saves parser work but leaves VLM-G-002's "whole authored profile"
   short by two record types on two titles.
   **Decided** (2026-11-07): the recommendation — decode and census the authored god-ray records (VLM-HF-010, -011); feed nothing into `FogMedium` until Phase 0 settles each field's meaning from a source.

---

## 11. Document map

| Need | Go to |
|---|---|
| How the froxel medium, the fit and the analytic tail work | [Procedural Volumetric Fog](../engine/procedural-volumetric-fog.md) |
| Per-pass milliseconds and the cost targets | [Measurement table](../engine/procedural-volumetric-fog.md#measurement-table) |
| Decoded fog fields and why some are unread | [Corpus findings](../engine/procedural-volumetric-fog.md#corpus-findings) |
| Combustion boundary calibration, aerial LUT and majorant follow-ups | [Follow-up boundary](../engine/procedural-volumetric-fog.md#follow-up-boundary) |
| Fire, smoke, explosions and the fire-light reach | [Fire/Smoke/Explosions Plan](../engine/fire-volumetrics-plan.md) |
| Interior shafts, apertures, converters, candidate cells | [Interior Godrays Status](../engine/interior-godrays-status.md) |
| Cloud shadows, portal sun, sky aerial perspective | [SKYAL](../engine/skyal.md) |
| Climate / weather boundary, base-fog strength, region weather (after Phase 0) | [EXAL](../engine/exal.md) |
| Visibility masks, units, adaptive ray budget | [Physical Lighting Backbone](../engine/physical-lighting-backbone.md) |
| Engine tiers per game | [Game Compatibility](../engine/game-compatibility.md) |
| Path-traced media | [Path Tracing Reference Mode PRD](path-tracing-reference-mode.md) |
| Shadows, contact shadows, sky IBL and GI (M-LIGHT; M55 is out of its scope) | [Reference-Quality Lighting PRD](reference-lighting.md) |
| Distant-LOD geometry the aerial tail fades | [Distant LOD Completion PRD](distant-lod-completion.md) |
| Weather-driven surface wetness | [Wet-Surface Accumulation PRD](wet-surface-accumulation.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: per-cell `XCCM` cross-fade and region-record reads corrected from HEAD; authored god-ray records (VLM-HF-010, -011, Q8); FO4 `LGTM` tail; region tie rule, `XCCM`-versus-region order and forced-weather lock (VLM-REG-012 to -014); froxel cutout visibility (VLM-SH-009); non-RT cascade cited as ROADMAP-decided (VLM-NG-006, Q5 withdrawn); Q6 keeps ROADMAP's Frostbite reference; REGN no-fog claim cited; sibling PRDs linked | claude: revise |
| 3 | 2026-11-07 | Owner decisions recorded: Q1–Q4, Q6 and Q8 the recommendation, Q5 closed against VLM-NG-006, **Q7 decided for the exterior haze floor** (new VLM-SH-010, VLM-SH-011). Phase 0 evidence taken the same day on FNV Goodsprings: clear-weather `gpu_extinction` is 0.000349/m, about 4.5 % optical depth over the 128 m froxel range, so the recommendation for Q7 would have left clear-day exterior shafts faint | owner: approved |
| 4 | 2026-11-07 | → ACTIVE: parent #6729 and phase issues #6724–#6728 filed; Phase 2 carries the haze floor (VLM-SH-010, -011) and Phase 3 the interior coverage the goal needs | — |
| 5 | 2026-11-07 | Phase 2 and 3 first slices landed: VLM-SH-001, -002, -010, -011 (the shaft lab, its gate, the haze floor and its measured default) and VLM-INT-005 (nearest-first aperture cap with a once-per-episode log). §3 row 14 corrected: the interior lab gate A10 is red at HEAD and before this work (#6730). Open under #6726: VLM-SH-003, -004, -005, -006, -007, -008, -009 | — |
| 6 | 2026-11-07 | VLM-INT-007 added: the interior shaft dust, an engine choice that makes a proven aperture's sun shaft visible. Found with the new `render.debug volumetric_radiance` (the existing `volumetric_term` is opacity-floored) and the `--godray-lab-marked` control, which shows the marked-aperture path itself works (column 40.63 against 40.63 for an open hole, sealed 0.00). VLM-INT-002 now measurable and met on NellisHangar1 | claude: revise |
| 7 | 2026-11-07 | VLM-SH-003 met: `shaft_lab_golden_frame` pins the exterior shaft lab under the default haze floor (passes unchanged; the solid-wall control differs on 32.6 % of pixels against the 1 % gate). Content measurement redone with exposure settled and a drift control: the floor changes structure (aerial perspective), not brightness; the earlier percentages are withdrawn | — |
| 8 | 2026-11-07 | VLM-INT-006 clarified: the live ratio, not the unit tests, is the measure. The census-versus-engine reconciliation found the loader's `fxlight` early return starving every FNV `FXLightBeam*` converter; fixed, and `Vault11a`, `SLAbandonedShack03INT` and `HooverDamIntPowerPlant01b` now carry passive cones | — |
| 9 | 2026-11-07 | VLM-SH-009 reworked and Phase 0 measured: the first criterion (cutout against opaque texels) passes on an opaque trace because of the haze beyond the panel, so it is replaced by a matched-geometry comparison (cutout card against the same stripes as real holes). The lab now has the three variants; the cutout reads 9.9 % below the holes, so the gate is known red until the trace honours alpha | claude: revise |
| 10 | 2026-11-07 | VLM-SH-009 met: the froxel sun trace honours alpha coverage under open sky (`sunRayBlockedAlphaAware`; material table at binding 32 and the bindless set as the pipeline's second descriptor set). Lab gate: cutout 129.18 against geometry 129.16. Cost on FNV Goodsprings: inject +0.08 ms at noon, +0.15 ms at 07:00 (0.18 to 0.26, 0.19 to 0.35); finished frames identical (no cutout casters in view). Interiors keep the binary trace | claude: revise |
| 11 | 2026-11-07 | VLM-SH-005 measured: the lab shaft passes the 0.05 bar out to between 90 and 110 m (0.071 at 90 m, 0.031 at 110 m) and vanishes at the 128 m far plane; the lab takes `--shaft-lab-distance <m>` for the sweep. Cloud-shadow sampling (VLM-SH-004) stays on the camera-column value per Q3 | claude: revise |
| 12 | 2026-11-07 | VLM-SH-007 sequence prepared for the owner, not judged: `byroredux --shaft-lab --fly --window-size 1280x720 --upscaler taa --bench-mode renderer-static --bench-camera static --bench-frames 180 --bench-capture-every 15 --bench-capture-dir <dir>` writes 12 frames. Over the wall body (rows 335-520, columns 375-905) the finished frame's mean absolute change from the previous captured frame is 0.8 to 2.5 levels of 255, the per-pixel temporal std over frames 30 to 165 averages 1.0 (max 36.8, at slat edges). The in-scatter term alone is bit-identical frame to frame under `renderer-stepped` (noise 0.000000, `exterior-shafts.sh`) | claude: revise |
| 13 | 2026-11-07 | VLM-INT-006 FNV coverage measured on real meshes: 22 of 24 listed beam models convert (about 99 % of placements), all but one population are Show-Sky-off fixture beams beside a local light, and the converters were 0 % live until the `fxlight` loader fix. `2CCampGolfResort`, the busiest cell, runs 104 volumes at 0.68 ms inject. FO3, Oblivion, Skyrim, FO4 and Starfield remain unsurveyed | claude: revise |
| 14 | 2026-11-07 | VLM-SH-008 recorded: inject and integrate separately on the five exterior fixtures at noon and 07:00 (inject 0.30 to 0.77 ms, integrate 0.02 ms), with the alpha-aware trace's share (+0.11 to +0.34 ms at 07:00) from a same-binary `BYRO_SUN_ALPHA=0` A/B; table in the fog design doc. VLM-SH-006 contrast across a named occluder is not recorded for these default-spawn poses | claude: revise |
| 15 | 2026-11-07 | Candidate engine choice for the owner, built behind `BYRO_SUN_DUST` and off by default: an open-sky sun-term-only dust, the exterior counterpart of VLM-INT-007. At 2.0 per metre it gave a real-content shaft on one pose (Oblivion canopy, 07:00) without the frame wash of a stronger extinction floor, but more poses and hours show it is not a usable default: 17:30 and second-grid captures change the frame by 10 to 72 levels, and even 0.25 per metre changes four of the strongest cases by 4 to 32 levels (see the fog design doc). Not a decided Q7 value: it is an artistic scattering strength with no matching extinction, one pose per title. Elevation shaping since added (`1 - smoothstep(0.2, 0.7, sun_y)`): across the five fixtures at 2.0 every noon frame is unchanged and the 07:00 shafts remain. Adopting it still needs the owner's choice of value and shaping edges, then a VLM-HF-008 entry | claude: revise |
| 16 | 2026-11-07 | Phase 0 census landed (#6724): `region_weather_census` and `fog_godray_census`. VLM-REG-012 and -013 moot on Oblivion / FO3 / FNV; VLM-REG-002 not moot (Oblivion 527 equal-Priority cells, FNV 8, FO4 11) and keeps an in-game observation open; VLM-REG-008 revised to the polygon test (`XCLR` is stale in both directions); interior `XCCM`, `VOLI` / `GDRY` / height-fog and per-title placed-beam censuses recorded (the beam denominators for every official master and DLC: [interior-shaft-openings](../engine/interior-shaft-openings.md#placed-beam-census-every-title-and-official-dlc-vlm-int-006-denominators-measured-2026-11-07)) (REG-009, HF-003, -010, -011; EXAL §3.2; fog design corpus findings). Open: HF-001 curve form, HF-003 semantics (matched captures), the REG-002 / replace-vs-add observation, live converter coverage for Oblivion, FO4 and Starfield beams, A6 / A8 baselines | — |
| 17 | 2026-11-07 | A6 baseline captured (#6724): `docs/smoke-tests/fog-grid-seam.sh`. VLM-HF-007 fails at HEAD on both fixtures (distant regions move 4.2 to 6.0 levels against a repeat noise of at most 0.004 when the grid reach goes 128 m to 512 m; FNV hills left, Skyrim forest and both whole frames stay under 1). A8's per-pass milliseconds were already recorded in revision 14; its contrast across a named occluder (VLM-SH-006) is not. Found while capturing: on the FNV pose `--bench-mode renderer-stepped --bench-camera static` renders the 3D scene rolled 180 degrees (HUD upright; `system-live` is upright; the Skyrim pose is upright in both), so the script rotates only the FNV frames back | — |
| 18 | 2026-11-07 | VLM-HF-001 settled by disassembly (#6724): FNV's `shaderpackage010.sdp` fog term is `fog = saturate((d - near) / (far - near)) ^ power` in the vertex shader (121 identical sequences), with `d` the clip-space xyz length; `scripts/sdp-fog-term.py` reproduces it. VLM-HF-002 holds by construction (power 1 is the linear ramp). Open: the distance metric as an engine choice (HF-008), and the 34 `pow` pixel shaders | — |
| 19 | 2026-11-07 | Phase 0 closed out (#6724): the census, the two design sections and the baselines are checked in; the verify items still open are filed (#6771 to #6775) and the Phase 0 paragraph of §9 carries the status. Revision 13's "FO3 unsurveyed" is superseded: FO3 was measured through the shared `GameKind` (interior-shaft-openings), and the placed-beam denominators now cover every official master and DLC. Phase 1 inherits two decisions the census forces: the polygon test for region membership (VLM-REG-008) and a fog distance metric (the shipped shader uses the clip-space xyz length, not view distance) to record under VLM-HF-008 | — |
| 20 | 2026-11-08 | Phase 1 landed (#6725): region weather (VLM-REG-001 to -014: `resolve_cell_weather`, `apply_cell_weather`, interior sky-from-region, `env.health` `weather.source=`; live on FNV Goodsprings / Black Mountain and Skyrim Jorrvaskr), the authored fog power in the ramp fit (HF-001, -002), the FO4 / FO76 height-fog tail and one authored-fog translation (HF-004 to -006), and the `VOLI` / `GDRY` decode (HF-010, -011). New finding: Starfield's `VOLI` is a 1,832-byte reflection blob, not Skyrim's layout. Effect to review: with the authored power in the fit, clear-weather exterior extinction rises about 4.6x (FNV Goodsprings noon 0.000582 -> 0.002704 /m, Skyrim Whiterun 0.000583 -> 0.003347 /m), above the exterior haze floor (VLM-SH-010), which the floor's measured default was chosen against (filed for owner review as #6776) | — |
| 21 | 2026-11-08 | Phase 1 region-weather coverage published (A2): the resolver, run over every exterior cell of each master, takes a region's weather for Oblivion 8,476 of the 8,533 cells a Weather-bearing region polygon covers, FO3 3,068 of 3,068, FNV 1,595 of 1,595, Skyrim 7,756 of 7,760, FO4 3,911 of 3,920; the shortfall is read off the resolver's own output: Oblivion's `ms14world` and FO4's `diamondcity` regions author a Weather section with no rows (the cells keep the climate's weather; one Oblivion cell resolves to no weather at all), and Skyrim's four are Tamriel cells such as (-13, 26) where two Override regions share Priority 50 and no lower tier exists, so the CK tie rule leaves neither to apply (VLM-REG-012) | — |
| 22 | 2026-11-08 | A8 content measurement (#6773): `exterior-shafts-content.sh` records the in-scatter radiance at 12:00 / 07:00 / 17:30 on the five fixtures (fog design table); VLM-SH-006's occluder contrast is not measurable at the default poses (depth confound) and is restated as a depth-matched-pose need | — |
| 23 | 2026-11-08 | VLM-INT-006 live converter coverage for every title (#6774): FO4 ~100 % (DBTechHighSchool01: 83 placements, 238 volumes), Oblivion 92.9 % (the #6731 gate widened to include it; WarehouseCaveGeneric 52 placements, 66 volumes), Skyrim 0 of 1, Starfield 0 of 1; `baked_beam_converter_coverage` generalises the FNV-only test | — |
| 24 | 2026-11-08 | Phase 4 landed (#6728), measured: VLM-HF-007 holds within a 2.5-level tolerance (the tail now shares the grid's mean density factor and clamped height profile; A6 is a gate in `fog-grid-seam.sh`), VLM-AP-002 shipped with its source pin, and every exterior surface fades through `sky_air_terms` toward the clear sky (VLM-AP-001's fade). Not measured: VLM-AP-001's monotonic ridge sweep (needs a distant-LOD fixture) and a Q4 volume (not built; Q4 decided the analytic tail). Look change to review: distant terrain now reads as layered aerial perspective rather than a single blue wash on the Skyrim pose | — |
| 25 | 2026-11-08 | A9 interior day / night measured on real cells (#6727, VLM-INT-001 to -003): `interior-shafts-content.sh` (free camera, paused clock, fixed exposure, in-scatter view, noon / midnight / noon with the repeat as the noise floor). Pass count per game: FNV 2 of 4, FO3 0 of 1, Skyrim 1 of 2, Oblivion 1 of 2, FO4 2 of 2, Starfield 0 of 1. Only `NellisHangar1` and `ConcordMuseum01` show a sun-driven shaft above the noise; the four sealed controls hold. The failures are mostly fixture-beam cones with no sun path (HELIOS, REPCONN, Tenpenny, `WarehouseCaveGeneric`), so the list the handoff proposed needs narrowing to cells whose census shows a sun path; Skyrim `AbandonedShackInterior` (Show Sky on, sun 0.51, six passive volumes, indoor sun as `LIGH`) and Starfield (no medium) are engine gaps. VLM-INT-004 (door transition) is not run; VLM-INT-001's outdoor-border geometry evidence is not produced. Phase 3's exit (every listed cell passing) is not met | — |
| 26 | 2026-11-09 | Sun-path census (#6727, VLM-INT-001) and the A9 list narrowed by it: `interior_sun_path_census` runs the converter chain over every interior base model of FNV, FO3, Oblivion, Skyrim, FO4 and Starfield. Only nine interiors can carry a sun-driven shaft (FNV: three Nellis hangars with `SkyAperture` planes; FO4: six Show Sky cells with `LightShaft`); FO3, Skyrim and Starfield have none, Oblivion's four `LightShaft` cells are test and cave cells with Show Sky off. On the census list A9 measures 9 of 9 `shaft` cells positive, and 12 of 12 controls clean on their last run (two controls showed a marginal single-run false positive near flickering lights, one a harness error; none repeated); the five `LightShaft` cells with Show Sky off and no marked aperture show no change. Open: VLM-INT-004, the outdoor-border geometry evidence, and the gap (Skyrim `AbandonedShackInterior`: Show Sky, sunlight `LIGH`, no medium; 205 Skyrim, 58 Oblivion and 29 FO4 interiors like it), which is new converter work. The census list is evidence for the rule over ten cells, not proof | — |
| 27 | 2026-11-09 | VLM-INT-004 first measurement (#6727): `interior-shafts-content.sh` gains `BYRO_INTERIOR_ROUTE=door` (out through the cell's exit door, back in through the exterior door, then the same day / night pass). Of the nine census cells seven pass through the door (`NellisHangar1`, `NellisHangar2`, FO4 `ConcordMuseum01`, `zInstituteRotunda`, `UniversityPoint01`, `DBTechHighSchool01`, and `TrinityTower01` narrowly: 0.09 % on one run, 0.13 % on the repeat, floor 0.1 %); two are not resolved because the route cannot start (`NellisHangar1b`: the door at its exterior arrival led to `NellisHangar1` on one run and no door was in reach on the next; `CambridgePD01`: no door in reach at spawn). Phase 3's exit (every listed cell passing on both routes) is not met; the gap, the outdoor-border geometry evidence and the pass-count publication stay open | — |
| 28 | 2026-11-10 | VLM-INT-004 door route completed (#6727): the way back in is now chosen by destination (`door.list`, a read-only console command) instead of by the camera ray, which resolved the two cells the first measurement could not start (`NellisHangar1b`: its exterior door stands 112 units from Hangar 1's; `CambridgePD01` passes through its door). Eight of nine census cells pass through a door; `TrinityTower01` has no door route (both exit doors arrive inside the tower beside an elevator lift panel) and its earlier door result is withdrawn. Open: a door-route interior of `CambridgePD01` carries 2,578 named entities and 30 fog volumes against 7,499 and 37 on a direct boot (actors and part of the placed objects missing), with the shaft that reads 44 % direct reading 0 to 7 % through the door; whether that is a revisit effect or a transition-path defect is not yet known. FO4 door runs are flaky in time (one run found no door to `UniversityPoint01` in five minutes). The gap and the outdoor-border geometry evidence stay open | — |
