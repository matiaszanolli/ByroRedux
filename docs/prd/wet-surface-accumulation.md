# Wet Surface and Storm Accumulation — Product Requirements

**Status**: PROPOSED (2026-11-05). Revision 3 records `GpuInstance` moving
into one struct-only header. Revision 2 (2026-11-01) folds in review
round 1 (water-contact writers and NPC coverage corrected at HEAD, GPU
transport realigned with the ROADMAP row and the M59 PRD, M59 ownership of
MATO and puddle decals, BGSM porosity, per-entity exposure, split
multi-observable requirements, acceptance made runnable). Nothing below
is built as a result of this document yet; the decisions it needs from the
project owner are collected in §10. Statements about how an original
engine, file format, paper or tool behaves cite external sources (the local
CK / GECK wiki dumps, xEdit definitions, `nif.xml`, published papers); a
claim that could not be verified is marked **Phase 0: verify** instead of
being stated as fact.

**Scope**: ROADMAP milestone M61. Exterior weather and water contact leave
a visible, history-dependent coating on surfaces: a **wet** film (rain,
submersion, splashes, drip after leaving WATAL water) that darkens porous
albedo and raises specular toward a water film, then dries; a **snow**
coverage that builds while the active weather is snow-classified and melts
otherwise; and, for Fallout titles that author radstorms, a darker **grime**
coating driven by the same state machine. The promise covers exterior LAND
terrain (partly shipped), static geometry, actors and what they wear. It
is visual only. Out of scope: gameplay effects of weather (radiation damage,
slipping, footstep sound changes), interior weather, a new render pass, and
any per-game branch past the translation boundary.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| WTHR decode and the classification → canonical weather translation | [EXAL](../engine/exal.md), [SKYAL](../engine/skyal.md) |
| Water contact, submersion, splash and ripple markers | [WATAL](../engine/watal.md) |
| Canonical `Material` and its GPU record | [NIFAL](../engine/nifal.md) (live contract), [Canonical Material Abstraction](../engine/material-abstraction.md) (history) |
| `GpuMaterial` / `GpuInstance` layouts and their GLSL mirror contract | [Shader Pipeline § GPU Data Types](../engine/shader-pipeline.md#gpu-data-types), [Renderer § Material table (R1)](../engine/renderer.md#material-table-r1) |
| How a new `GpuMaterial` lane is fed without growing `DrawCommand` | Sibling M59 PRD, [Material and Decal Layering](material-decal-layering.md) (MDL-GPU-002/004, its decision Q1) |
| Save exclusion of history-dependent simulation state | [Save/Load Round-Trip](../engine/save-load-roundtrip.md) |
| The surface-state model, its GPU channel, exposure and drying | **Missing** — Phase 0 writes `docs/engine/wet-surface.md` (the ROADMAP M61 row requires a design doc before work starts) |
| Decal projection (wet footprints, puddle decals) and the surface-response decal class | [Material and Decal Layering](material-decal-layering.md) (MDL-DEC-008, "the interface M61 depends on for puddles") and the decal design its Phase 0 writes |
| Skyrim directional materials (STAT `DNAM`, `MATO`) decode and translation | [Material and Decal Layering](material-decal-layering.md) decision Q6 (recommended there: M59 owns it, M61 reuses it) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `WET-` prefix.

---

## 1. Product thesis

The engine already renders weather as something that happens *in the sky*:
WTHR classification drives cloud coverage, storm morphology and a
screen-space rain/snow veil (SKYAL). What it does not yet render is weather
as something that happens *to the world*. A swimmer climbs out of Lake Mead
bone-dry; a Whiterun rooftop stays bare through a blizzard; a radstorm in
the Commonwealth tints the sky and nothing else. M61 closes that gap for
the surfaces players look at most — ground, buildings, people.

The feature is for players of every supported title with exterior weather,
and for mod authors whose weathers and water should get the same response
as vanilla without authoring anything new. It is a Tier 8 visual-fidelity
item ([ROADMAP § Milestone tiers](../../ROADMAP.md#milestone-tiers)): it
builds on the RT investment and on WATAL's contact geometry rather than
adding a parallel pipeline.

Three statements shape it:

1. **One state machine, three coatings.** Rain → wet, snow → coverage,
   radstorm → grime are channels of one per-surface weather-contact state,
   not three systems (the ROADMAP M61 row says so explicitly).
2. **Authored data first, engine choice second, guess never.** Where a game
   authors the input (WTHR classification, FO4 wetness envelopes, Skyrim
   directional snow materials) it is translated; where nothing is authored
   (drying rates, porosity of a vanilla Oblivion stone) the value is a
   documented engine choice; where the signal itself is unknown (radstorm)
   the feature waits for Phase 0 instead of inventing one.
3. **Material extension, not a render pass.** The response rides on the
   existing geometry pass and material record, sized like M59.

The corollary that shapes every requirement below: **the coating is
history-dependent state owned by the ECS, and the renderer only reads it.**
The renderer never decides whether something is wet; it shades a value a
system computed from translated weather and water contact. A reader should
come away knowing the engine will not simulate water flow over surfaces,
will not apply gameplay effects, and will not ship a radstorm look keyed on
a guessed signal.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Surface state** | The history-dependent coating values a surface carries: wetness, snow coverage, grime. Normalised to `[0, 1]`. At HEAD only a worldspace-wide instance exists, the `WeatherSurfaceState` resource (`wetness`, `snow`). |
| **Channel** | One coating in the surface state: *wet*, *snow*, *grime*. Each has a source, an accumulation rule and a decay rule. |
| **Worldspace state** | The single surface state for exposed exterior ground, advanced by `weather_system`. Shipped (§3). |
| **Per-entity state** | A surface state attached to one entity (an actor, a worn item, a dynamic body, optionally a static). Proposed by this PRD; the component is named by the Phase 0 design. |
| **Source** | What raises a channel: precipitation (rain, snow), water contact (`WaterContact`), a `SplashEvent`, a storm-grime signal. |
| **Exposure** | Whether a surface point can receive precipitation from the sky — the fraction not occluded from above. The "exposure function" of Hsu and Wong, as used by Ohlsson and Seipel (§3.3). |
| **Wet response** | The shading change a wet channel produces: monochrome darkening of porous albedo, raised gloss (lower roughness), water-film F0 on puddles, after Lagarde (§3.3). |
| **Porosity** | The per-material scalar that scales the wet response; non-porous materials gain a film, porous ones darken (Lagarde 3a/3b). |
| **Classification** | WTHR `DATA` flags bits 0–3 — Pleasant / Cloudy / Rainy / Snow — shared by every target game (xEdit, §3.3). The field `WeatherRecord::classification`. |
| **Radstorm** | A weather that applies atmospheric radiation; appears in Fallout 4 and Fallout 76 only (§3.3). |
| **Grime** | The storm-deposited coating channel for radstorms (dark ash/particulate). |
| **Translate step** | The EXAL / WATAL / NIFAL boundary where per-game data becomes canonical values (`env_translate`, `material_translate`). The only place a per-game difference may appear. |

---

## 3. Core model

### 3.1 Channels and their rules

| Channel | Sources | Accumulates when | Decays when | Shading (direction only; values belong to the design) | Reported by |
|---|---|---|---|---|---|
| Wet | Rain (exposed), submersion, splash, drip after exit | Source active | Source inactive: drying, faster with sun and wind; specular dries before diffuse; puddles dry last | Darker porous albedo, higher gloss, water-film F0 on puddles | `sky.dump` line (worldspace, WET-DX-001); new per-entity command (WET-DX-002) |
| Snow | Snow-classified weather (exposed, upward-facing) | Snow precipitation active | Rain or sun melts | Bright, rough, non-metallic cover; relief softened | Same |
| Grime | Radstorm signal (Phase 0) | Storm active, exposed | Rain washes; otherwise persists until a reset rule the design defines | Darker, rougher deposit | Same |

The three channels share one advance function and one GPU transport; a
surface with snow hides its wet response, as the shipped terrain path
already does.

Exposure is applied at two different points, and the requirements keep them
apart. Surfaces that read the worldspace state (terrain, statics) are
masked per pixel at shading time (WET-COV-001): the state is global, the
shelter is local. Per-entity state is history, so it is gated per entity
when it accumulates (WET-COV-006): an actor who stood under a roof through
the storm must come out dry, which a shading-time mask cannot express once
the actor walks into the open.

### 3.2 Inventory at HEAD

The inventory below is the state at HEAD on 2026-11-01. It is
hand-written; Phase 0's design doc replaces it with a per-mechanism
status table and this table is then trimmed to a link.

| # | Mechanism | Owner | Today |
|---|---|---|---|
| 1 | Classification decode | plugin | Parsed from `DATA` byte 11 into `WeatherRecord::classification` (`crates/plugin/src/esm/records/weather.rs`, test `parse_wthr_classification_is_byte_11`). Constants for bits 0–5 only. |
| 2 | Classification consumers | byroredux | **Consumed** — contrary to the ROADMAP M61 row's "no downstream consumer": `env_translate/sky_weather.rs` maps it to cloud coverage (`fog_coverage_from_weather`), the scalar `precipitation_from_weather` and the rain/snow pair `precipitation_components`; the scripting crate keeps a per-WTHR `WeatherClassifications` resource read by the Papyrus `GetClassification` native (`fragment/vm/natives/forms.rs`), ObScript `IsRaining` (`obscript_more.rs`) and `weather.force`. |
| 3 | FO4+ flag bits 6–7 (*Rain Occlusion*, *HUD Rain Effects*) | plugin | Not decoded: no constant in `weather.rs`. |
| 4 | FO4+ `UNAM` *Magic* (lightning-strike spell, weather-activate spell) | plugin | Not decoded: no `UNAM` arm in `weather.rs`. |
| 5 | Worldspace state (wet, snow) | byroredux | **Shipped** (`01451d5ef`, 2026-09-02): `WeatherSurfaceState` (`components/environment.rs`), advanced by `advance_weather_surface` in `systems/weather.rs`, exterior-only (skipped while `CellLightingRes::is_interior`). Rates are documented engine choices (comment above `SURFACE_MAX_DT`). Tests `rain_wets_ground_without_accumulating_snow`, `clear_sun_and_wind_dry_existing_ground`, `snowstorm_builds_coverage_and_rain_sun_melt_it`, `invalid_or_large_dt_cannot_make_surface_state_non_finite`. |
| 6 | GPU transport of the worldspace state | renderer | Packed as two unorm16 halves into the camera UBO's `renderDebug.w` by `pack_weather_surface` (`vulkan/context/assemble_camera_and_lights.rs`, test `packs_wetness_low_and_snow_high`); decoded by `decodeWeatherSurface()` in `triangle.frag`. |
| 7 | Terrain response | renderer | **Shipped, LAND only**: the weather block in `triangle.frag` runs when `terrainSplatActive` and the exterior flag (`jitter.w`) are set; slope-gated wet film, noise-placed puddles, snow cover. Shading: albedo multiplied toward a fixed cool tint, puddles lower roughness and pull specular colour toward `0.02`; snow brightens, roughens and removes metalness. No sky-exposure test: ground under a bridge wets like open ground. |
| 8 | Static, actor, worn-item, dynamic-body response | — | None. No fragment outside the terrain splat path reads the surface state. |
| 9 | Water contact | core, physics, byroredux | `WaterContact` (`crates/core/src/ecs/components/water.rs`) carries `submerged_fraction`, `head_submerged` and `waterline` (an `Option`; the player writes `None`). Writers at HEAD: the buoyancy scan in `crates/physics/src/water.rs` (`MotionType::Dynamic` bodies and ragdoll bones only) and `sync_player_water_contact` (`systems/character.rs`, the player). The `water.contacts` command only reads it. **Live NPCs carry none**: they are `CharacterKinematic`, which the buoyancy scan skips, so "today the player is the only live" wet actor (doc comment on `water_damage_system`, `systems/water.rs`). No surface-state consumer. |
| 10 | Splash and ripple markers | scripting, byroredux | `SplashEvent` / `RippleEvent` (`crates/scripting/src/events.rs`) are produced by `systems/water.rs`; consumed by audio (`systems/audio.rs`) and, for ripples, by the water renderer (`render/water.rs`). No surface-state consumer. |
| 11 | FO4 authored wetness envelope | nif, core | `BSSPWetnessParams` reaches `Material::wetness` as `WetnessShading` (`crates/core/src/ecs/components/material.rs`; test `translate_material_copies_wetness_and_luminance_envelopes`). No GPU lane: nothing under `crates/renderer/src` reads it. The BGSM wetness-control fields and the porosity pair (`custom_porosity`, `porosity_value`) decode in `crates/bgsm` but have no sink (the #2704 ledger in `asset_provider/material/merge.rs`). |
| 12 | Skyrim directional snow material (STAT `DNAM`, `MATO`) | plugin | Not decoded: `RecordType::MATO` exists in `crates/plugin/src/record.rs`; no parser reads either (the only `DNAM` arms in `esm/cell/support.rs` are ADDN and TXST). Ownership is the M59 PRD's decision Q6. |
| 13 | Persistence | save | `WeatherSurfaceState` is in `NOT_SAVED_BY_DESIGN` (`save_io/registry_completeness_tests.rs`, test `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`): rebuilt dry on load. |
| 14 | Radstorm signal | — | Unknown; **Phase 0: verify** (§3.3, WET-WX-004). |

### 3.3 External references

| Claim | Source |
|---|---|
| Wet porous surfaces darken because water fills the pores; non-porous surfaces only gain a thin specular film | S. Lagarde, *Water drop 3a – Physically based wet surfaces*, 2013, <https://seblagarde.wordpress.com/2013/03/19/water-drop-3a-physically-based-wet-surfaces/> |
| Darkening scaled by porosity and wet level, gloss raised rather than F0, thick water (puddles) reflects from a flat normal; "the wet specular reflection goes away quicker than the wet diffuse reflection", and a puddle "should not dry as quickly as the other area"; a game "could simply lerp quickly (1 min) to dry"; metals do not darken; porosity may be linked to roughness as an approximation | S. Lagarde, *Water drop 3b – Physically based wet surfaces*, 2013, <https://seblagarde.wordpress.com/2013/04/14/water-drop-3b-physically-based-wet-surfaces/> |
| Snow coverage = inclination term × exposure-to-sky term, exposure from a depth map rendered from above; based on Hsu and Wong's dust-accumulation exposure function | P. Ohlsson, S. Seipel, *Real-time Rendering of Accumulated Snow*, SIGRAD 2004, <https://ep.liu.se/ecp/013/007/ecp01307.pdf> |
| Classification bits Pleasant / Cloudy / Rainy / Snow in every target game; FO4, FO76 and Starfield add *Rain Occlusion* (bit 6) and *HUD Rain Effects* (bit 7); in FO4, FO76 and Starfield `UNAM` *Magic* holds a *Lighting Strike* spell (xEdit's spelling) and a *Weather Activate* spell, each with a threshold | xEdit `Core/wbDefinitions{TES4,FO3,FNV,TES5,FO4,FO76,SF1}.pas` and `wbDefinitionsCommon.pas` (`wbWeatherMagic`), branch `dev-4.1.6`, <https://github.com/TES5Edit/TES5Edit> |
| Classification affects "water surfaces, NPC dialogue, and other game logic" | CK wiki, *Weather* (local dump `ck-uesp-wiki/(main)/Weather.wiki`, Weather Classification) |
| Papyrus `GetClassification` returns −1..3 for none / pleasant / cloudy / rainy / snow, identical in Skyrim and FO4 | CK and FO4 CK wiki dumps, *GetClassification - Weather* |
| FO4 and FO76 shader properties carry a wetness envelope (`BSSPWetnessParams`: spec scale, spec power, min var, env-map scale, fresnel power, metalness); Skyrim's does not | `nif.xml` (`/mnt/data/src/reference/nifxml/nif.xml`, struct `BSSPWetnessParams`, versions `#FO4# #F76#`) |
| FO4 BGSM materials carry *Wet Spec Scale / Power Scale / Min Var / Env Map Scale / Fresnel Power / Metalness* and *Wetness Control SSR*; each wetness-control field defaults to `-1` | Material-Editor (`/mnt/data/src/reference/Material-Editor/ControlNames.cs`; defaults in `MaterialLib/BGSM.cs`) |
| BGSM files from version 9 carry *Custom Porosity* (bool) and *Porosity Value* (float) | Material-Editor `MaterialLib/BGSM.cs` (read under `Version >= 9`), `ControlNames.cs` |
| The local FO4 CK wiki's *Weather* page documents no WTHR flag; it does not say what *Rain Occlusion* does | FO4 CK wiki dump, `falloutck-uesp-wiki/(main)/Weather.wiki` |
| The FO4 Creation Kit has a `bEnableWetnessMaterials` display setting | FO4 CK wiki dump, `Category/CreationKit.ini.wiki` |
| Skyrim authors snow on statics as a projected *directional material*: STAT `DNAM` (max angle, `MATO`, SSE *Considered Snow*), `MATO` `DATA` (falloff, projection vector, SSE *Is Snow*); shader types *Snow* and *Sparkle Snow* exist | xEdit `wbDefinitionsTES5.pas` (STAT, MATO); CK wiki *Material Object*; `nif.xml` shader-type enum |
| Radstorms appear in Fallout 4 and Fallout 76; lightning flashes spike radiation; interiors shelter; NPCs are not affected by the rads; the page describes no surface coating | Fallout Wiki, *Radstorm* (infobox `games = FO4,FO76`, section *Characteristics*), <https://fallout.fandom.com/wiki/Radstorm> |
What the original engines did *at runtime* with these data (does FO4
actually wet surfaces in rain, does Skyrim's directional material change
with weather) is not documented in any source above. It is listed as
**Phase 0: verify** in WET-MAT-003 and WET-COV-004.

---

## 4. Goals and non-goals

### 4.1 Goals

- **WET-G-001 — World gets wet.** In rain, exposed exterior terrain, statics
  and actors show the wet response; sheltered ones do not.
- **WET-G-002 — Water leaves a mark.** An actor or dynamic body that leaves
  WATAL water is wet below the highest waterline it reached, then dries.
- **WET-G-003 — Snow builds up.** Under snow-classified weather, exposed
  upward-facing terrain and statics accumulate coverage and melt afterwards.
- **WET-G-004 — Radstorms leave grime** on FO4 exteriors, keyed on a
  verified per-record signal, or the extension is withdrawn (Q2).
- **WET-G-005 — No new pass, bounded cost.** The feature adds no render
  pass and stays within the frame-cost budget the design sets (§8).

### 4.2 Non-goals

- **WET-NG-001.** No gameplay effects: radiation damage, slipping,
  footstep-sound or traction changes. Weather spells belong to the
  gameplay milestones; footstep surfaces to the Wave-4 surface-aware
  footsteps work (ROADMAP Parking lot).
- **WET-NG-002.** No interior weather accumulation. The shipped gate already
  skips interiors (`systems/weather.rs`); window-portal sky is unaffected.
- **WET-NG-003.** No fluid simulation of water running over surfaces, no
  rivulets or drip particles; the coating is a scalar field per surface.
- **WET-NG-004.** No new render pass or pipeline, and no growth of
  `DrawCommand` or of the `GpuInstance` lockstep surface — the
  ROADMAP M61 row sizes the work "like M59's material-slot approach
  (`GpuMaterial` extension, no `DrawCommand`/shader-lockstep growth)". How
  the per-entity value reaches the GPU within that bound is Q1.
- **WET-NG-005.** No snow *geometry* (displaced depth, deformable trails);
  coverage is a shading layer. Footprint decals are M59's.
- **WET-NG-006.** No per-pixel parity with any original engine's wet look
  ([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).

---

## 5. Requirements by area

### 5.1 Surface state (`WET-STATE`)

- **WET-STATE-001.** Wet, snow and grime MUST be channels of one surface
  state advanced by one function. Today `advance_weather_surface` advances
  wet and snow on the worldspace state; grime does not exist.
- **WET-STATE-002.** The worldspace state MUST stay a resource and remain
  the input for exposed terrain. Today: `WeatherSurfaceState`.
- **WET-STATE-003.** Actors, their worn items and dynamic bodies MUST carry
  a per-entity state when any channel is non-zero, and MUST NOT carry one
  when all channels are zero (sparse storage; no component on bodies that
  never got wet).
- **WET-STATE-004.** Every channel MUST stay finite and within `[0, 1]` for
  any `dt`, including non-finite and very large steps. Today the shipped
  test `invalid_or_large_dt_cannot_make_surface_state_non_finite` covers
  wet and snow.
- **WET-STATE-005.** The wet channel SHOULD decay in two stages (specular
  first, diffuse later), and puddle wetness SHOULD decay slower than the
  film, per Lagarde 3b. Today one scalar decays at one rate.
- **WET-STATE-006.** Every rate MUST be a named constant with a provenance
  comment stating it is an engine choice or citing its source. Today the
  shipped rates carry such a comment.
- **WET-STATE-007.** A worn item's state MUST follow its wearer: equipping
  a dry item onto a wet actor leaves the item dry until a source acts on it;
  unequipping keeps the item's own value.

### 5.2 Weather input and translation (`WET-WX`)

- **WET-WX-001.** The rain and snow sources MUST come from the translated
  pair `precipitation_components` (or its successor) in the translate step,
  never from the classification byte read downstream. Today
  `weather_system` feeds `advance_weather_surface` from the translated
  `weather.precipitation`.
- **WET-WX-002.** During a WTHR cross-fade the sources MUST follow the
  blended precipitation, not the target weather's. Today
  `weather_system` feeds the rain/snow pair through `lerp_weather_sky`
  by `transition_t` before calling `advance_weather_surface`.
- **WET-WX-003.** The FO4+ *Rain Occlusion* and *HUD Rain Effects* flags
  (bits 6–7) MUST be decoded and exposed as named constants. **Phase 0:
  verify** what *Rain Occlusion* controls in FO4 (settled by a CK-side
  description or a vanilla census correlating the bit with sheltered rain);
  if it means "precipitation is occluded by geometry", WET-COV-001 uses it
  as the per-weather switch for exposure.
- **WET-WX-004.** The grime source MUST key on a per-record radstorm
  signal verified in Phase 0, translated in `env_translate` into a canonical
  value. **Phase 0: verify** the signal, candidates in order: (a) `UNAM`
  *Weather Activate* or *Lightning Strike* spell whose effect damages the
  radiation actor value (consistent with the Fallout Wiki's "lightning
  flashes spike radiation"); (b) the precipitation `SPGD` or visual-effect
  `RFCT` form; (c) a script trigger outside WTHR. Settled by a census of
  every `Fallout4.esm` (and DLC) WTHR with `dump_wthr_subs`, listing
  `EDID`, classification, `UNAM` spells and their `MGEF` archetype and
  actor value. An `EDID` substring MUST NOT be the signal.
- **WET-WX-005.** Games whose WTHR authors no radstorm signal (all but FO4
  and FO76) MUST translate grime to zero in the translate step; the shader
  and the advance function MUST NOT test the game.
- **WET-WX-006.** Exterior-only gating MUST hold for every channel: no
  channel rises while `CellLightingRes::is_interior` is set. Decay MAY
  continue indoors for per-entity state (a wet actor dries inside). Today
  the worldspace state neither rises nor decays indoors.

### 5.3 Water contact (`WET-WATER`)

- **WET-WATER-001.** An entity with a wet `WaterContact` MUST raise its wet
  channel to full below its waterline.
- **WET-WATER-002.** Every actor in water, player and NPC, MUST carry a
  `WaterContact` so WET-WATER-001 applies to it. At HEAD on 2026-11-01
  only the player and dynamic bodies (including ragdoll bones) do: live
  NPCs are `CharacterKinematic` and the buoyancy scan in
  `crates/physics/src/water.rs` skips every non-`Dynamic` body (§3.2
  row 9). Phase 1 adds the NPC writer; the observable is `water.contacts`
  listing a wading NPC.
- **WET-WATER-003.** A `SplashEvent` SHOULD raise the wet channel of
  entities within a radius the design sets, scaled by the event's
  `intensity`.
- **WET-WATER-004.** Leaving the water MUST start decay from the wet value,
  not reset it; the dry-sentinel transition of `WaterContact` (one dry
  frame) is the trigger.
- **WET-WATER-005.** Wet footprints from an actor leaving water SHOULD be
  emitted as M59 decals that dry by the same decay rule. This requirement
  is blocked until M59 decal projection ships.
- **WET-WATER-006.** Water with authored damage (FO3/FNV radioactive water,
  `WaterContact::damage_per_second`) MAY tint the wet channel through a
  canonical water-tint value from WATAL's translate step; no game test.
- **WET-WATER-007.** The per-entity state MUST keep the highest waterline
  the entity reached while wet, so a wader reports wet only below the knee
  and a swimmer head to toe; `WaterContact::waterline` is optional (the
  player writes `None`), so the design states the fallback (origin plus
  `depth`, as `WaterContact`'s consumers already use).

### 5.4 Material response (`WET-MAT`)

- **WET-MAT-001.** The wet response MUST darken albedo monochromatically,
  scaled by porosity and wet level, per Lagarde 3b. Today the terrain path
  multiplies albedo toward a fixed cool tint (§3.2 row 7); statics and
  actors have no response. Gloss, metals and puddle normals are
  WET-MAT-008..010.
- **WET-MAT-002.** Porosity MUST be a canonical per-material value resolved
  in `material_translate`. Where a material authors it (BGSM *Custom
  Porosity* / *Porosity Value*, version 9 and later per Material-Editor;
  captured and unused at HEAD, §3.2 row 11) the authored value MUST be
  used; elsewhere it MUST be derived by one documented engine rule for
  every game (for example from roughness, per Lagarde 3b), not per game.
  Which BGSM versions the in-scope titles ship, and so whether any
  in-scope material authors porosity, is a Phase 0 census item.
- **WET-MAT-003.** FO4's authored wetness envelope (`Material::wetness`,
  BGSM wetness-control fields) SHOULD translate into the canonical
  wet-response parameters at the NIFAL boundary. **Phase 0: verify** the
  semantics of each field (min var, spec power scale, the `-1` defaults)
  against a source that describes them (Material-Editor, CK, or a
  reverse-engineered shader reference) before any field is consumed; an
  unverified field stays captured and unused.
- **WET-MAT-004.** The per-material response parameters (porosity, the
  authored-snow flag, translated FO4 envelope values) MUST travel in
  `GpuMaterial` lanes added under the R1 MaterialTable contract, with the
  size test (`gpu_material_size_is_432_bytes`, renamed to the new size),
  `gpu_material_field_offsets_match_shader_contract` and
  `gpu_material_glsl_field_order_matches_rust_struct` updated in the same
  commit — the rule the M59 PRD states as MDL-GPU-002. How a lane reaches
  `GpuMaterial` without a `DrawCommand` field (WET-PERF-001) follows the M59
  PRD's decision Q1; this PRD does not decide it a second time.
  Per-entity *state* is not a material parameter and does not travel here
  (Q1 of this PRD).
- **WET-MAT-005.** Snow MUST NOT stack on a surface already authored as
  snow: shader types *Snow* / *Sparkle Snow* and, once decoded, Skyrim
  directional materials flagged *Is Snow* or *Considered Snow* translate
  to a canonical "authored snow" flag that caps the snow channel's
  visible effect.
- **WET-MAT-006.** Emissive, glass, water and effect-shader materials MUST
  NOT receive the wet or snow response; the exclusion is a canonical
  material class test, not a game test.
- **WET-MAT-007.** The renderer MUST contain no game-identity branch for
  any of this: a source-shape test SHOULD assert that the wet block in
  `triangle.frag` reads only canonical lanes.
- **WET-MAT-008.** The wet response MUST raise gloss (lower roughness)
  rather than change specular colour, per Lagarde 3b.
- **WET-MAT-009.** Metallic materials MUST NOT darken under the wet
  response (Lagarde 3b's `(1 - Metalness) * Porosity` term).
- **WET-MAT-010.** Where the puddle term is active, the shading normal
  MUST be flattened toward the geometric normal (Lagarde 3b's thick-water
  case). Today the terrain path does this for snow only (`N` mixed toward
  `terrainGeometryNormal` by the snow term in `triangle.frag`).
- **WET-MAT-011.** Puddles placed at positions (beyond the shipped terrain
  noise mask) SHOULD be M59 surface-response decals (MDL-DEC-008) driven by
  the wet channel, not a second puddle mechanism; blocked until that decal
  class ships. The ROADMAP M59 row lists "water puddles" under decal
  projection.

### 5.5 Coverage and exposure (`WET-COV`)

- **WET-COV-001.** Rain wetness and snow from precipitation MUST be scaled
  by exposure: a surface point occluded from the sky MUST NOT gain them.
  RT-first: exposure SHOULD come from a ray query against the existing
  TLAS rather than a top-down depth pass (WET-NG-004); the design sets the
  ray budget.
- **WET-COV-002.** Snow MUST be scaled by an inclination term so that
  vertical and downward-facing surfaces gain little or none (Ohlsson and
  Seipel). Today the terrain path gates both channels on slope.
- **WET-COV-003.** Distant terrain LOD and near terrain MUST show the same
  coating at the LOD join; a coating seam at the ring edge is a defect.
- **WET-COV-004.** Once Skyrim's directional materials (STAT `DNAM` +
  `MATO`) are decoded and translated — by M59 if the owner takes the M59
  PRD's recommended Q6, otherwise in this PRD's Phase 3 — their projection
  vector and falloff SHOULD seed the inclination term on those statics.
  **Phase 0: verify** whether the
  original engine varies a directional material with weather (settled by
  a CK-wiki statement or an in-game comparison of one snowy STAT across a
  clear and a snow weather); if it does not, Redux's dynamic layer is a
  deliberate deviation (§6 row 3).
- **WET-COV-005.** Statics MAY use a per-cell or per-reference state instead
  of the worldspace state when the design shows the worldspace scalar is
  insufficient (sheltered sides of one building). The doc comment on
  `WeatherSurfaceState` (`components/environment.rs`) already anticipates
  "per-cell exposure/retention maps".
- **WET-COV-006.** Rain and snow MUST NOT raise an entity's per-entity
  state while the entity is sheltered from the sky; the shelter test is
  evaluated per entity at accumulation time, by a method the design sets
  (§3.1). Observable: an actor standing under a roof through forced rain
  reports wet = 0 in WET-DX-002's command.

### 5.6 Budget and invariants (`WET-PERF`)

- **WET-PERF-001.** The feature MUST add no render pass, no pipeline and
  no `DrawCommand` field.
- **WET-PERF-002.** `GpuInstance` MUST stay at 160 bytes with every GLSL
  declaration MDL-GPU-001 enumerates (including the stride-only
  `GpuBoundaryInstance` mirror) unchanged, pinned by `gpu_instance_is_160_bytes_std430_compatible`
  and `gpu_instance_glsl_field_order_matches_rust_struct` passing unmodified (the
  M59 PRD's MDL-GPU-001 applied to M61), unless the owner takes Q1's
  alternative A and amends the ROADMAP row.
- **WET-PERF-003.** With rain forced on a bench-of-record exterior, the
  `main_render` delta against a same-machine control at the parent commit
  MUST stay within the budget the Phase 0 design sets, measured with the
  [bench methodology](../../ROADMAP.md#how-to-run-a-bench).
- **WET-PERF-004.** With clear weather and all channels zero, the per-frame
  CPU and GPU cost MUST be indistinguishable from the control within the
  measured noise floor.

### 5.7 Observability (`WET-DX`)

- **WET-DX-001.** `sky.dump` MUST print the worldspace state's channels
  (wet, snow, grime). Today it prints coverage, glare, precipitation and
  thunder (`commands/env_health.rs`), and neither `sky.dump` nor
  `env.health` prints the surface state.
- **WET-DX-002.** A new console command MUST print an entity's per-entity
  channels and the source that last raised each one.
- **WET-DX-003.** A console command MUST set the worldspace and per-entity
  channels directly, so smokes and captures need not wait for accumulation.
- **WET-DX-004.** A structured render-debug mode SHOULD visualise each
  channel as a greyscale overlay, alongside the existing `RENDER_DEBUG_*`
  modes.

### 5.8 Persistence (`WET-SAV`)

- **WET-SAV-001.** Per-entity state MUST follow the posture Q3 decides. On
  the recommended answer it is added to `NOT_SAVED_BY_DESIGN` with its
  reason, like `WeatherSurfaceState`, and no `FORMAT_MAJOR` change occurs.
- **WET-SAV-002.** A save/load round trip MUST NOT leave any channel
  non-finite or out of range, whichever posture Q3 takes.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Oblivion, FO3, FNV and Skyrim author no wetness data in their shader formats (`nif.xml`: `BSSPWetnessParams` is FO4/FO76 only); whether their runtimes wet surfaces at all is **Phase 0: verify** (in-game comparison in rain) | Every title gets a wet response from one canonical model (WET-MAT-001) | Weather-wetness overlay mods for those titles (Phase 0 names corpus examples with the legacy-mod PRD's catalog) | A mod that already wets surfaces double-darkens → the design exposes a per-profile off switch; WET-DX-003 lets a tester zero the channels |
| 2 | FO4 authors a wetness envelope per material (`nif.xml`, Material-Editor) | Translated into the canonical model only where its semantics are verified (WET-MAT-003) | None | Divergence from FO4's own look → accepted under the anti-scope's no-parity rule; unverified fields stay unused |
| 3 | Skyrim authors static snow through directional materials (xEdit STAT/MATO, CK *Material Object*) | Dynamic snow layer on top, capped where snow is authored (WET-MAT-005, WET-COV-004) | None | Double snow on already-snowy statics → WET-MAT-005; a weather-varying look the original may lack → WET-COV-004 verify |
| 4 | Radstorms have a look (green sky, lightning) and radiation (Fallout Wiki) but, by any source found, no surface coating | Grime coating keyed on a verified signal (WET-WX-004) | None | Grime on a non-radstorm weather if the signal is wrong → no EDID heuristic (WET-WX-004); withdraw if no signal (Q2) |
| 5 | Wet/snow state not persisted by Redux (shipped worldspace state rebuilt dry) | Same for per-entity state on the recommended Q3 | — | A reload during rain shows dry ground briefly → bounded by the accumulation rate; WET-SAV-001 |
| 6 | Classification in Skyrim also drives water surfaces and NPC dialogue (CK wiki) | This PRD consumes it only through the translated precipitation pair; its script uses (`GetClassification`, `IsRaining`) belong to the scripting runtime | — | None for this PRD |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 2. Classification bits match (xEdit TES4); the Tamriel exterior passes its readiness gates (ROADMAP compatibility matrix). No radstorm: grime translates to zero. |
| Fallout 3 | yes | Wave 2. Exterior wired; WATAL water present. Radstorms do not appear in FO3 (Fallout Wiki infobox lists FO4, FO76): grime zero. Radioactive water tint is WET-WATER-006. |
| Fallout: New Vegas | yes | Wave 1. Reference title; Lake Mead traversal (WATAL W1) makes water-contact wetness testable. Which vanilla FNV weathers are Rainy or Snow is a Phase 0 census item. Grime zero. |
| Skyrim (LE / SE / AE) | yes (SE/AE wave 1; LE inherits) | Rain and snow weathers; Tamriel exterior with `.btr` LOD; directional snow materials (WET-COV-004). LE shares the records and follows SE through the same translate step once its runtime loads cells (today parse only). Grime zero. |
| Fallout 4 | yes | Wave 2. Commonwealth exterior streaming; authored wetness envelopes (WET-MAT-003); the only title where the grime extension can land (WET-WX-004). |
| Fallout 76 | no | Parse only at HEAD; no exterior runtime. Its WTHR records MAY join the Phase 0 radstorm census as corroboration. |
| Starfield | no | Walkable interior only at HEAD; no exterior planet runtime. WTHR flags match FO4 (xEdit SF1), so the translate step will cover it when exteriors exist. |

Waves follow exterior readiness: wave 1 is the titles whose exteriors and
water are live-gated today; wave 2 adds the remaining exterior titles and
the FO4-only grime channel.

### 7.2 Acceptance

Data needed: installed vanilla `FalloutNV.esm` + archives, `Skyrim.esm` +
archives (SE), `Fallout4.esm` + archives, `Oblivion.esm`, `Fallout3.esm`.
Nothing is checked in; scripts read the profiles in
`assets/debug_profiles.toml`.

A scenario passes when:

1. **State unit tests.** The shipped tests in `systems/weather.rs` stay
   green, and new tests `grime_builds_under_storm_signal_and_rain_washes_it`,
   `water_contact_wets_below_waterline_only`,
   `leaving_water_decays_from_wet_value`, and
   `specular_dries_before_diffuse` pass (`cargo test -p byroredux --bin
   byroredux`). Covers WET-STATE-001/004/005, WET-WATER-001/004.
2. **Translate tests.** `weather_classification_drives_density_coverage_with_precipitation_priority`
   stays green; a new test `grime_translates_to_zero_without_signal` passes
   for every `GameKind` (WET-WX-005).
3. **Material contract.** `gpu_material_size_is_*_bytes` (renamed to the
   new size) and `gpu_material_glsl_field_order_matches_rust_struct`
   (`scene_buffer/shader_contract_tests.rs`) pass with the new lanes
   (WET-MAT-004).
4. **Wave-1 live smoke, new script `docs/smoke-tests/m61-wet-surface.sh`.**
   FNV: the Lake Mead profile of the existing WATAL route
   (`docs/smoke-tests/w1-water-traversal.sh`, shore → swim → shore through
   the real character controller), `--bench-hold`, `byro-dbg` attaches;
   after the route, the new per-entity command reports wet = 1 below the
   reached waterline (WET-WATER-001/007) and a lower, non-zero value a
   fixed time later (WET-WATER-004); a wading NPC appears in
   `water.contacts` (WET-WATER-002). Skyrim: Whiterun exterior,
   `weather.force <snow WTHR>`, `time.advance`, `sky.dump` reports
   snow > 0; `weather.force <rain WTHR>` raises wet and melts snow; a probe
   actor under a roof reports wet = 0 (WET-COV-006). Sheltered statics are
   checked by the capture in scenario 5, not by a command: their exposure
   is a shading-time mask (WET-COV-001).
5. **Capture check.** The same script captures `screenshot` pairs (clear vs
   forced rain, set via WET-DX-003) at a pinned `--camera-pos`; the
   per-channel debug view (WET-DX-004) is non-zero exactly on exposed
   pixels.
6. **Bench.** WET-PERF-003/004 on the FNV and Skyrim bench-of-record
   exteriors, same-machine control, pinned camera.
7. **Radstorm census (Phase 0).** The census table over `Fallout4.esm` and
   DLC lists every WTHR and the chosen signal; pass = every weather the
   Fallout Wiki or in-game observation calls a radstorm is selected and no
   Pleasant/Cloudy/plain Rainy weather is.

---

## 8. Metrics

The numbers live in the M61 row of [ROADMAP § Milestone
tiers](../../ROADMAP.md#milestone-tiers) until the milestone closes, then
in its Completed Milestones entry; frame-cost numbers go in the
bench-of-record table.

| Metric | Definition | Granularity |
|---|---|---|
| Surface-class coverage | Surface classes reading the state ÷ the five classes (near terrain, LOD terrain, statics, actors, worn items) | Per game, per channel |
| Water-contact coverage | Actors in water carrying a wet per-entity state ÷ actors in water, on the smoke's sample | Per game |
| Exposure correctness | Sheltered probe references gaining rain wetness or snow ÷ sheltered probes in the smoke | Per game |
| Radstorm signal precision | WTHRs selected by the signal that are radstorms ÷ WTHRs selected | FO4 (FO76 corroboration) |
| Frame cost | `main_render` delta, rain forced vs control, same machine, pinned camera | Per bench-of-record exterior |

---

## 9. Delivery sequence

Depends on WATAL (contact geometry, shipped for dynamic bodies and the
player) and on M59: its decision Q1 (how a `GpuMaterial` lane is fed,
needed before Phase 2), its decision Q6 (who decodes `MATO`, needed before
Phase 3), and its surface-response decal class MDL-DEC-008 (WET-WATER-005
and WET-MAT-011 only). The M59 PRD asks for MDL-DEC-008's contract to be
written in its decal design before this PRD is approved.

**Phase 0 — Design, census and verify (no engine code).** Write
`docs/engine/wet-surface.md` (the design the ROADMAP row requires) and
register it in the design index: the per-entity component, the GPU channel
per Q1, the exposure method and ray budget, drying rates with provenance,
the frame-cost budget for WET-PERF-003, the per-entity shelter test of
WET-COV-006. Run the WTHR census over every installed title with
`dump_wthr_subs` (classification, bits 6–7, `UNAM`) and the BGSM version
census of WET-MAT-002. Close the verify items: WET-WX-003 (*Rain
Occlusion*), WET-WX-004 (radstorm signal), WET-MAT-003 (FO4 envelope
semantics), WET-COV-004 (directional material vs weather), §6 row 1
(original runtime wetness). Needs Q1–Q4 decided. Exit: the design doc is
merged and indexed, both census tables are in it, and each of the five
verify items is closed with a source or filed as an issue.

**Phase 1 — Per-entity state and water contact.** The shared advance
function with the grime slot (zero everywhere), per-entity state on actors,
worn items and dynamic bodies, water-contact and splash sources, the
observability commands, the NPC `WaterContact` writer, the per-entity
shelter test. Satisfies WET-STATE-001..007, WET-WATER-001..004 and 007,
WET-WX-005/006, WET-COV-006, WET-DX-001..003, WET-SAV-001/002. Exit:
acceptance 1, 2 and the FNV leg of 4 pass.

**Phase 2 — Material response on statics and actors.** Canonical porosity
and response lanes, the Lagarde model in the geometry pass for every
non-excluded material, authored-snow cap, exposure ray query, the debug
view, terrain path moved onto the same model. Satisfies WET-MAT-001/002/
004..010, WET-COV-001..003, WET-DX-004, WET-PERF-001..004. Exit:
acceptance 3, the Skyrim leg of 4, 5 and 6 pass, and surface-class coverage
is published in ROADMAP for wave 1.

**Phase 3 — Wave 2 and authored inputs.** Oblivion, FO3 and FO4 smokes;
FO4 wetness envelope translation (WET-MAT-003); Skyrim directional
materials (WET-COV-004); *Rain Occlusion* (WET-WX-003). Exit: acceptance 4
extended to the three wave-2 titles passes and coverage is published for
them.

**Phase 4 — Radstorm grime (conditional on Q2).** The grime source from the
verified signal (WET-WX-004), and wet footprints and placed puddles if
M59's surface-response decals have shipped (WET-WATER-005, WET-MAT-011). Exit: acceptance 7 passes with precision 1.0 on the
census, and a forced-radstorm FO4 smoke reports grime > 0 outdoors and 0
under cover.

---

## 10. Decisions required

1. **Q1 — How does per-entity state reach the GPU?** The ROADMAP M61 row
   sizes M61 with "no `DrawCommand`/shader-lockstep growth", and the M59
   PRD holds `GpuInstance` and its GLSL declarations fixed
   (MDL-GPU-001); per-material parameters go to `GpuMaterial` either way
   (WET-MAT-004). Recommended: a per-instance side buffer of packed
   channels, uploaded in the same order as the instance SSBO and declared
   once in `include/bindings.glsl`, following the `GpuMaterial` pattern
   and the M59 PRD's MDL-GPU-005 rule for new GPU structs (size, offset and
   field-order tests in the same commit) — no `GpuInstance` edit and no
   material-table churn; consequence: one new descriptor binding in set 1,
   one small per-frame upload, and the design must show the ray-hit path
   can index it the way it indexes `GpuInstance`. Alternative A: pack the
   channels into `GpuInstance`'s reserved lanes (`_reserved`,
   `_reserved2a..c` in `scene_buffer/gpu_types.rs`) — no size change and
   no new binding, but every MDL-GPU-001 GLSL declaration changes in one lockstep
   commit, which needs the ROADMAP M61 row amended and departs from the
   rule M59 sets for itself. Alternative B: per-entity state as
   `GpuMaterial` variants — no new buffer, but each distinct wet level
   splits the material dedup and grows the table every frame it changes.
   Alternative C: worldspace scalar only, no per-entity state — smallest
   change, but WET-G-002 (water leaves a mark) and sheltered actors cannot
   be met; the PRD's scope shrinks to terrain and statics.
2. **Q2 — What if Phase 0 finds no authored radstorm signal?**
   Recommended: withdraw the grime extension (WET-G-004 and Phase 4 marked
   withdrawn) — the ROADMAP row forbids a guessed signal. Alternative: an
   engine-owned, reviewed table of vanilla radstorm WTHR FormIDs in the
   translate step — ships the look for vanilla, but modded radstorms get no
   grime and the table is data the engine must maintain.
3. **Q3 — Is per-entity surface state saved?** Recommended: no; add it to
   `NOT_SAVED_BY_DESIGN` like `WeatherSurfaceState` — no `FORMAT_MAJOR`
   bump, and a reload shows briefly dry actors. Alternative: save it — a
   save-format change (README trigger T2) and a `FORMAT_MAJOR` bump, for
   continuity that lasts only until the coating decays.
4. **Q4 — One canonical wet model for every game, or FO4's authored
   envelope as FO4's model?** Recommended: one canonical model; FO4's
   verified envelope fields translate into its parameters at the NIFAL
   boundary (WET-MAT-003) — consistent response across games, R5-clean.
   Alternative: drive FO4 materials from their own envelope semantics —
   closer to FO4's authoring, but it needs a second response path in the
   shader keyed on which data is present, the "diverges by data presence"
   leak the material abstraction closed.

---

## 11. Document map

| Need | Go to |
|---|---|
| Weather decode, classification and its sky consumers | [SKYAL — weather-driven cloud morphology](../engine/skyal.md#weather-driven-cloud-morphology-2026-09-14), [EXAL — Weather / TOD](../engine/exal.md#weather--tod--canonical-the-cleanest-of-the-dynamic-categories) |
| Water contact and splash markers | [WATAL §5.4 `WaterContact`](../engine/watal.md#54-watercontact--per-body-submersion-generalises-camera-only-submersionstate), [WATAL §5.5 markers](../engine/watal.md#55-splashevent--rippleevent--transient-markers) |
| The canonical material and its translate boundary | [NIFAL §3 Materials](../engine/nifal.md#3-materials--the-reference-realisation), [material abstraction history](../engine/material-abstraction.md) |
| `GpuMaterial` / `GpuInstance` layouts and mirror tests | [Shader Pipeline § GPU Data Types](../engine/shader-pipeline.md#gpu-data-types), [Renderer § Material table (R1)](../engine/renderer.md#material-table-r1) |
| M59's GPU budget, MATO ownership and the surface-response decal hook | [Material and Decal Layering PRD](material-decal-layering.md) |
| Save exclusions | [Save/Load Round-Trip](../engine/save-load-roundtrip.md) |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md) |
| Bench methodology for WET-PERF | [ROADMAP § How to run a bench](../../ROADMAP.md#how-to-run-a-bench) |
| Anti-scope and keep list | [ROADMAP § anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope), [ROADMAP § Architecture Decisions](../../ROADMAP.md#architecture-decisions) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: water-contact writers and NPC coverage corrected at HEAD, Q1 realigned with the ROADMAP row's no-lockstep sizing and the M59 PRD (side buffer recommended), M59 links (MDL-GPU, Q1, Q6, MDL-DEC-008), BGSM porosity, per-entity exposure (WET-COV-006), WET-MAT-001 and WET-WATER-001 split (WET-MAT-008..011, WET-WATER-007), Lagarde/xEdit wording, acceptance 4 made runnable; coordinator cross-PRD sync (sibling links and citations) | claude: revise |
| 3 | 2026-11-05 | `GpuInstance` moved into one struct-only header: WET-NG-004, WET-PERF-002 and Q1 follow MDL-GPU-001's new definition and the renamed guard. No requirement changes | — |
