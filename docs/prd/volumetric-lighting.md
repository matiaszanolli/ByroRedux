# Volumetric Lighting — Product Requirements

**Status**: ACTIVE (2026-11-07). Revision 4 files the tracking issues. Revision 3 records the owner's §10 decisions: every one the recommendation except Q7, which the owner decided for the exterior haze floor so that the goal, realistic procedural god rays in interiors and exteriors, holds on clear weather (VLM-SH-010, VLM-SH-011). Revision 2, same day, folds in review
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
| Region weather resolution (`RDWT` tables, Override / Priority, `XCCM` sky-from-region) | **No design yet** — EXAL covers the climate and the region `CNAM` chain only; Phase 0 writes a region-weather section into [EXAL](../engine/exal.md) |
| Authored god-ray records (Skyrim SE `VOLI`, FO4 `GDRY`) | **No design yet** — Phase 0 adds their census and semantics to the fog design's [corpus findings](../engine/procedural-volumetric-fog.md#corpus-findings) |
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
| 11 | Weather choice | Shipped (climate only) | `resolve_default_weather` takes the highest-chance climate row (`env_translate/climate.rs:269`, the one rule EXAL fixes, #5424); `apply_cell_climate_override` re-resolves per cell crossing and re-applies with the same 8-second `WeatherTransitionRes` cross-fade a worldspace change uses (`scene/world_setup.rs:781-784`; [EXAL](../engine/exal.md)) |
| 12 | Skyrim / FO4 interior `XCCM` sky-from-region | Open → VLM-REG | `CellData::sky_region` decoded, documented "not yet consumed" (`crates/plugin/src/esm/cell/cell_data.rs:105`) |
| 13 | Forced weather | Shipped over the climate, lock unread → VLM-REG | `forced_or` makes a forced weather replace the climate default (`scene/world_setup.rs:305-315`); `ForcedWeather::locked` is "recorded and saved but read by nothing" (`crates/scripting/src/obscript_state.rs:100`) |
| 14 | Interior shafts and sky apertures | Shipped slice, coverage open → VLM-INT; **its own gate is red at HEAD (#6730)** | `--godray-lab` + `docs/smoke-tests/interior-godrays.sh` fails with no shaft (centre delta 0.0, 43.2 when written), on HEAD and on a pre-2026-11-07 build; the lab's sky aperture works and its room is black. Aperture upload now keeps the nearest apertures and logs the overflow once (VLM-INT-005, `context/sky_apertures.rs`) |
| 15 | Analytic tail for distant geometry | Shipped, sky agreement open → VLM-AP | `composite.frag` continues the grid boundary radiance, clamped to four times the authored fog luminance |
| 16 | Aerial-perspective volume (Hillaire 2020) | Open → VLM-AP, §10 Q4 | No aerial LUT in `crates/renderer` (`rg -i "aerial.*lut"` is empty); [measurement table](../engine/procedural-volumetric-fog.md#measurement-table) row "Aerial LUT on" is empty |
| 17 | Alpha-tested cutouts in the froxel sun trace | Open → VLM-SH-009 | `traceShadowBinary` traces with `gl_RayFlagsOpaqueEXT` (`shaders/include/shadow_common.glsl:35`) and static BLAS geometry is built `GeometryFlagsKHR::OPAQUE` (`vulkan/acceleration/blas_static.rs:597`), so the trace sees no alpha coverage |
| 18 | Authored god-ray profiles (Skyrim SE `VOLI`, FO4 `GDRY`) | Open → VLM-HF-010, -011, §10 Q8 | Not parsed: `rg "VOLI\|GDRY\|WGDR\|XGDR" crates/plugin/src` is empty (only the probe `crates/plugin/examples/dump_voli_subs.rs` exists); the fog design records zero `VOLI` in the FO4 masters ([corpus findings](../engine/procedural-volumetric-fog.md#corpus-findings)) |
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
pages describe the region list's chances but none says so outright; settled
by the same census and in-game observation as VLM-REG-002.

- **VLM-REG-001.** For an exterior cell, the weather table in effect MUST be
  the Weather section with the highest Priority among the cell's regions
  whose section has the Override flag set. Today no non-test code in
  `byroredux/src` reads `RegionDataPayload::Weather` (at HEAD on
  2026-11-01). Test: `region_weather_override_priority_wins` (new).
- **VLM-REG-002.** When no region of the cell sets Override on a Weather
  section, the choice among non-override Weather sections MUST follow the
  original editors' rule. **Phase 0: verify** — none of the three wiki pages
  states which non-override region wins when several overlap; settled by a
  corpus census of overlapping non-override Weather sections in the vanilla
  masters (if none overlap, the rule is moot and recorded as such) and, if
  some do, by an in-game observation recorded with its save and cell.
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
- **VLM-REG-008.** Region membership MUST be read from the cell's `XCLR`
  list (`CellData::regions`). **Phase 0: verify** — the CK page says "it is
  the exact shape of the region as drawn in the Region Preview Window that
  will decide its actual area of effect", not the per-cell list; settled by
  a census comparing each vanilla exterior cell's `XCLR` set with the cells
  its regions' `RPLI`/`RPLD` polygons cover, per game. If they disagree on
  any cell carrying a Weather section, this requirement is revised to the
  polygon test before Phase 1.
- **VLM-REG-009.** On Skyrim and FO4, an interior whose `XCCM` names a
  region (xEdit: "Sky/Weather from Region",
  [TES5 L6544](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsTES5.pas#L6544),
  [FO4 L9052](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L9052))
  MUST take its portal palette and portal sun from that region's weather
  table. Today `CellData::sky_region` is decoded and unread. On
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
  rule; settled by the VLM-REG-002 census (ties among overlapping overriding
  Weather sections; if none exist, moot and recorded).
- **VLM-REG-013.** On Oblivion / FO3 / FNV, the order between a cell's
  `XCCM` climate override and its regions' Weather sections MUST follow the
  original engine's rule. **Phase 0: verify** — no cited page states it;
  settled by the census counting exterior cells that carry both, and, if
  any do, by an in-game observation recorded with its save and cell. If
  none carry both, the order is moot and recorded as such.
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
  takes near, far and maximum opacity only. **Phase 0: verify** the curve's
  exact form (the wiki gives its direction, not its formula); settled by
  disassembling the fog term in one title's shipped shader package, or
  failing that recorded as an engine choice under VLM-HF-008.
- **VLM-HF-002.** A power of 1 MUST reproduce today's fitted extinction
  exactly, so that every Oblivion record (its `WTHR` `FNAM` is day / night
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
  distance blend, or a density multiplier above the mid height); settled by
  a corpus census of the authored values per weather together with matched
  in-game captures of two weathers that differ only in those fields. Until
  then the fields stay unread, as `height_mid` is today.
- **VLM-HF-004.** The FO4 and FO76 height-fog tail MUST be decoded from
  both `CELL` `XCLL` and `LGTM` `DATA`. xEdit defines it after the
  Skyrim-shaped body: Near Height Mid, Near Height Range, Fog Color High
  Near / High Far, High Density Scale, four near / far / high scales, Far
  Height Mid, Far Height Range
  ([XCLL, wbDefinitionsFO4.pas L9003-L9013](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L9003-L9013);
  [LGTM, L11667-L11677](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L11667-L11677)).
  Today the parser accepts the FO4 128 / 136-byte `XCLL` sizes
  (`fo4_xcll_sizes_pinned`) but decodes the tail of either record only for
  Starfield. Tests: `parse_cell_fo4_xcll_decodes_height_fog_tail`,
  `parse_lgtm_fo4_decodes_height_fog_tail` (new).
- **VLM-HF-005.** Cell height fog from VLM-HF-004 and from Starfield's
  `StarfieldLighting` MUST be translated by one shared function into the
  same `FogMedium` fields the weather path fills, with the Phase 0
  semantics of VLM-HF-003. Today `CellLightingRes.starfield` is forwarded
  and unread.
- **VLM-HF-006.** Every new `FogMedium` field MUST interpolate in
  `FogMedium::lerp` so weather and time-of-day cross-fades carry it, as the
  scale height does today (`lerp_interpolates_the_scale_height`).
- **VLM-HF-007.** The froxel grid and the analytic tail MUST evaluate the
  same height profile: with a pinned camera on an exterior fixture,
  changing `--fog-grid-far-m` from its default to its maximum MUST NOT move
  the mean luminance of pixels beyond the larger far plane by more than the
  run-to-run noise of a repeated default capture (the regression
  `composite.frag` records as "the mountains changing appearance when the
  grid's reach changes").
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
  both under `IsSSE`). Today neither is parsed. Test:
  `parse_wthr_sse_hnam_and_voli` (new). Whether any decoded field reaches
  `FogMedium` is §10 Q8; until then each is listed under VLM-HF-009.
- **VLM-HF-011.** FO4's `GDRY` god-ray records and the four links to them —
  `WTHR` `WGDR` per time of day, `CELL` `XGDR`, `LGTM` `WGDR`, `LIGH` `WGDR`
  with its near clip — MUST be decoded (xEdit FO4:
  [WTHR L15795-L15804](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L15795-L15804),
  [CELL L9057](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L9057),
  [LGTM L11680](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L11680),
  [LIGH L12849-L12856](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L12849-L12856),
  [GDRY L15956-L15969](https://github.com/TES5Edit/TES5Edit/blob/9058a79437367c2cb1b757a9021ab443d743012d/Core/wbDefinitionsFO4.pas#L15956-L15969)).
  The Fallout CK documents the light-side use: "Volumetric Light: Whether
  this light should cast Volumetric rays (GodRays)" and "If 'Volumetric
  Light' is checked here you HAVE TO choose the God Ray profile"
  (`falloutck-uesp-wiki/(main)/Light.wiki`). Today none is parsed. Test:
  `parse_fo4_gdry_and_links` (new). Consumption follows §10 Q8.

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
  contrast still passes VLM-SH-002.
- **VLM-SH-006.** On each exterior fixture of §7.2, a noon and a low-sun
  capture under the fixture's vanilla weather MUST be recorded with the
  shaft contrast across a named occluder. **Phase 0: verify** whether
  vanilla clear-weather extinction makes any exterior shaft visible at all
  (`env.health` prints `gpu_extinction=` per weather); if it does not, §10 Q7
  decides whether an engine-choice haze floor is added.
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
  **Phase 0, measured 2026-11-07:** cutout 116.36, geometry 129.16, solid 93.40, a 9.9 % gap, so this fails at
  HEAD, as the opaque traversal (§3 row 17) predicts. The criterion this requirement first carried, shaft
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
  (`Vault11a` 14 ÷ 14, `SLAbandonedShack03INT` 5 ÷ 5 afterwards).

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
  geometry). Today the analytic tail's source is the grid boundary radiance
  clamped to the authored fog luminance, not the sky.
- **VLM-AP-002.** The tail's air term MUST be the same function SKYAL uses
  for cloud layers (`sky_air_terms`, `shaders/include/sky.glsl:270`), so the
  haze in front of a mountain and in front of a cloud at the same distance
  match; a pinned shader-source test
  (`analytic_tail_uses_the_sky_air_terms`, new) keeps them shared.
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
| A2 | Region weather census | new `crates/plugin/examples` probe, per master | Oblivion, FO3, FNV, Skyrim masters | every exterior cell with a Weather-bearing region reports a region source; tie and `XCCM`-plus-region counts recorded (VLM-REG-012, -013); counts published (§8) |
| A3 | Live region weather | `--bench-hold` + `byro-dbg`: `env.health` at two fixture cells in different regions, named by A2's census | FNV `WastelandNV`, Skyrim `Tamriel` | the two `weather.source=` lines name different regions, and a cross-fade runs between them (VLM-REG-007, -011) |
| A4 | Sky-from-region interior | `--bench-hold` + `byro-dbg`: `env.health` in one `XCCM` interior | Skyrim (an interior from Phase 0's `XCCM` census) | the `weather.source=` line names the `XCCM` region, not the worldspace climate (VLM-REG-009, -011) |
| A5 | Fog profile translation | `cargo test` names in VLM-HF-002, -004, -010, -011 plus the Phase 1 translate tests | none | all pass |
| A6 | Grid seam | `--bench-hold`, pinned `--camera-pos` / `--camera-forward`, captures at two `--fog-grid-far-m` values | FNV and Skyrim exterior fixtures | VLM-HF-007 holds |
| A7 | Exterior shaft lab | `docs/smoke-tests/exterior-shafts.sh` (new), `shaft_lab_golden_frame` (new) | none | VLM-SH-002, VLM-SH-003 and VLM-SH-009 pass |
| A8 | Exterior shafts on content | `--bench-hold` + `byro-dbg`: `time.set 12`, `render.debug composite_term`, `screenshot`, then a low-sun hour | the `m-exteriors.sh` fixture cells for Oblivion, FO3, FNV, Skyrim, FO4 | contrast and per-pass ms recorded (VLM-SH-006, -008) |
| A9 | Interior day / night | `--bench-hold` + `byro-dbg` per VLM-INT-002, a new `docs/smoke-tests/interior-shafts-content.sh` | the handoff's candidates: Skyrim `AbandonedShackInterior`, an Oblivion `CaveDaylight` mine, FO3 Tenpenny, FNV Repcon / Helios / Nellis, FO4 Concord Museum, one Starfield interior named in VLM-INT-001's inventory | each listed cell passes VLM-INT-002 to -004 |
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
Starfield; capture the exterior baseline (A6, A8) at HEAD. Exit: the verify
list is closed or filed as issues, the two design sections, the census
tables and the baseline captures with their per-pass milliseconds are
checked in, and §10 carries **Decided** on every item.

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
