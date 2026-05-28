# Cell-Based Lighting Architecture

> Status as of Session 42 (2026-05-28). The "Per-mesh NiLight" and
> "Where the data lives today" sections describe what is actually wired
> in the current tree; the Tier 0/1/2 material below is the original
> design vision (authored Session ~9, M28/N26 era) — kept for intent,
> annotated where the implementation has caught up or diverged.

## Per-mesh NiLight sources (N26 audit follow-up)

Cell lighting isn't the only source of light in a Bethesda world.
Individual meshes can embed `NiLight` subclasses — ambient, directional,
point, spot — right in the scene graph. Every Oblivion torch, candle,
and magic effect carries one (and FO3+ lamps / Skyrim braziers too).
Until the N26 audit these were silently dropped in Oblivion, leaving
the cell XCLL ambient / directional as the only illumination source
and making night-time scenes unplayable.

The NIF parser now handles the full `NiLight` hierarchy (#156):

- [`crates/nif/src/blocks/light.rs`](../../crates/nif/src/blocks/light.rs)
  implements `NiLightBase` + `NiAmbientLight` / `NiDirectionalLight` /
  `NiPointLight` / `NiSpotLight` with per-version gates for the
  `NiDynamicEffect` base (switch_state + affected-node ptr list) and the
  `NiLight` scalar fields (dimmer + ambient / diffuse / specular color3).
- The `attenuation_radius` solver in
  [`crates/nif/src/import/walk/mod.rs`](../../crates/nif/src/import/walk/mod.rs)
  (formerly `import/walk.rs`, split into `walk/` during the Session 34/35
  refactors) derives each point/spot light's effective radius from the
  attenuation polynomial: it solves `1/(const + lin·d + quad·d²) = 1/256`
  for `d` — the distance where contribution drops to ~0.4 % of peak,
  matching Bethesda's shader cull threshold. Lights with no attenuation
  (and ambient / directional) clamp to a `2048.0` fallback.
- `walk_node_lights` (same file) pulls lights out of the scene graph in
  world space, composing the parent chain via `compose_transforms`,
  skipping editor markers and culled/hidden nodes, and honouring
  `NiSwitchNode` / `NiLODNode` active-child selection (#718). Each light
  becomes an `ImportedLight`
  ([`crates/nif/src/import/types.rs`](../../crates/nif/src/import/types.rs)):
  world-space `translation` / `direction`, diffuse `color`, derived
  `radius`, a `LightKind` tag, the spot `outer_angle`, the
  `affected_node_names` from the `NiDynamicEffect.Affected Nodes` Ptr
  list (#335), and the light's own block `name` (so the animation system
  can resolve `NiLight*Controller` channels onto the spawned entity, #983).
- The cell-spawn path in
  [`byroredux/src/cell_loader/spawn.rs`](../../byroredux/src/cell_loader/spawn.rs)
  spawns a `Transform + GlobalTransform + LightSource` ECS entity per
  extracted light, parented through the reference transform so torches
  inside cell references contribute to the same `GpuLight` buffer that
  the XCLL directional light flows into. `is_spawnable_nif_light` /
  `count_spawnable_nif_lights` gate which extracted lights actually
  materialise as `LightSource` entities (real radius + positive
  contribution), and `LightFlicker` is attached for animated
  flame/candle sources. (The cell loader itself lives in
  [`byroredux/src/cell_loader.rs`](../../byroredux/src/cell_loader.rs) +
  the `cell_loader/` submodule directory: `load.rs`, `spawn.rs`,
  `references.rs`, `exterior.rs`, `terrain.rs`, `water.rs`,
  `transition.rs`, `unload.rs`, … — the original monolithic
  `cell_loader.rs` was split during the Session 34/35 refactors.)

Result: the renderer sees both "cell environment light" (XCLL) and
"per-object point lights" (NiLight) through the same
[`build_render_data()`](../../byroredux/src/render/mod.rs) path
(light collection itself lives in
[`byroredux/src/render/lights.rs::collect_lights`](../../byroredux/src/render/lights.rs)),
feeding the same SSBO and the same ray query shadow pass. From the
renderer's perspective there's no difference between a torch inside a
REFR and a cell's ambient colour.

**Update (Session 42):** the FO4+ caveat below is now stale. The parser
*does* handle FO4 / FO76 / Starfield `NiLight` (#721): for `bsver >=
FALLOUT4` (130) `NiLightBase::parse` skips the `NiDynamicEffect` base
(no `switch_state`, no `affected_nodes`) per nif.xml `vercond
="#NI_BS_LT_FO4#"` and reads the scalar fields off `NiAVObject`
directly. Pre-#721 those bytes were misread as `switch_state +
affected_nodes`, throwing every mesh-embedded FO4-era light through
`block_size` recovery as `NiUnknown`. The `affected_node_names` list is
simply always empty on these games (FO4 drops it at the wire level).

### LIGH → standard-light translation

`collect_lights` is also the translation boundary between Bethesda LIGH
record semantics and the renderer's light contract — no LIGH-specific
knowledge leaks into GLSL (same directive as the BGSM→PBR translation —
the "translate at the parser→Material boundary, never branch per-game in
the shader" rule). Two constants in `render/lights.rs` capture engine
policy:

- `LIGHT_RANGE_EXTENSION = 2.5` — Bethesda's LIGH `radius` is a "design
  value"; the runtime contributes visible light past it. The translator
  multiplies authored `radius` by this to get the renderer-facing
  `effective_range`. Tuned against densely-lit FO4 interiors.
- `FALLOFF_EXPONENT_DEFAULT` — used when a LIGH carries no positive
  `falloff_exponent`. The standardized attenuation-curve shape rides on
  `GpuLight.params.x`; the shader reads it verbatim with no sentinel
  handling (#983 per-light `falloff_exponent`).

The per-frame `dimmer * intensity` product (mutated by
`NiLight{Dimmer,Intensity}Controller`) scales the diffuse colour, and
`NiLightRadiusController` animates `radius` in place — the renderer only
sees the resolved factors.

---

## Where the cell-lighting data lives today

The flat `CellLighting`/`CellAmbientLight`/`CellFog`/`CellInterior`
component set sketched in the "ECS Component Design" section further down
was the original design. The shipped layout is different — fewer, fatter
resources plus a dedicated weather cube — so it is documented here as
ground truth:

### `CellLighting` (plugin parse output) — XCLL subrecord

[`crates/plugin/src/esm/cell/mod.rs`](../../crates/plugin/src/esm/cell/mod.rs)
parses the interior `XCLL` sub-record into a `CellLighting` struct. Base
fields (`ambient`, `directional_color`, `directional_rotation`,
`fog_color`, `fog_near`, `fog_far`) are shared across all games. The
Skyrim+ 92-byte XCLL adds `Option` fields, all `None` on pre-Skyrim:
`directional_fade`, `fog_clip`, `fog_power`, `fog_far_color`, `fog_max`,
`light_fade_begin`, `light_fade_end`, `directional_ambient`
(`[[f32;3];6]` — the `[+X,-X,+Y,-Y,+Z,-Z]` ambient cube, #367),
`specular_color`, `specular_alpha`, `fresnel_power`.

XCLL byte-length is game-era-dependent and dispatch-checked. The
canonical sizes are pinned in `cell/walkers.rs` and validated by an
`xcll_size_sanity_warn` helper (WARN-only, doesn't change parsing):

| Game era            | Canonical XCLL sizes (bytes) |
|---------------------|------------------------------|
| Oblivion            | 28 / 32 / 36                 |
| FO3 / FNV / FO4 / FO76 | 28 / 40                   |
| Skyrim              | 28 / 92                      |
| Starfield           | 28 / 108                     |

Starfield's vanilla 108-byte XCLL was split onto its own set in #1291
(it had been tripping the sanity-warn ~12 k× per `Starfield.esm` parse);
the size-based field dispatch reads the Skyrim 92-byte prefix and ignores
the trailing 16 bytes for now (#1277 Task 4 hardened the sanity gate).

### `CellLightingRes` (renderer-facing resource)

[`byroredux/src/components.rs`](../../byroredux/src/components.rs) holds
`CellLightingRes`, the World resource the renderer reads. It carries the
3 base fog/ambient/directional fields plus a computed `directional_dir`
(Y-up direction vector) and `is_interior`, then the same eleven extended
XCLL `Option`s as `CellLighting`. `CellLightingRes::from_cell_lighting`
copies them through verbatim (#861 — pre-fix every optional past the base
fog fields was dropped at this boundary). The extended fields not yet
consumed by a shader are marked `#[allow(dead_code)]`; each allow is
removed in lockstep with its shader-side consumer landing.

Consumption status (Session 42):

- `directional_color` / `directional_dir` / `is_interior` → directional
  fill light (below).
- `fog_color` / `fog_near` / `fog_far` → linear depth fog.
- `fog_clip` / `fog_power` → **consumed.** Plumbed to the composite
  shader as `fog_params.z` / `.w` for a non-linear `pow(distance /
  fog_clip, fog_power)` fog curve (#865 / FNV-D3-NEW-06), gated on
  `fog_clip > 0 && fog_power > 0` in
  [`crates/renderer/shaders/composite.frag`](../../crates/renderer/shaders/composite.frag).
- `directional_ambient` (the XCLL ambient cube), `fog_far_color`,
  `fog_max`, `light_fade_begin` / `_end`, `specular_color` /
  `specular_alpha`, `fresnel_power`, `directional_fade` → **parsed and
  carried, not yet pushed to the GPU.** Only a debug console query in
  [`byroredux/src/commands.rs`](../../byroredux/src/commands.rs) reads the
  XCLL ambient cube today. (Note: the *weather* ambient cube — WTHR.DALC,
  below — *is* GPU-wired; the per-cell XCLL cube is the follow-up.)

### `DalcCubeYup` + `GpuDalcCube` — the Skyrim WTHR.DALC ambient cube (shipped)

The first-order-SH ambient idea from "Tier 1" below has partially landed,
sourced from Skyrim weather (`WTHR.DALC`) rather than the per-cell XCLL
cube:

- [`crates/plugin/src/esm/records/weather.rs`](../../crates/plugin/src/esm/records/weather.rs)
  parses `WTHR.DALC` into a `SkyrimAmbientCube` per TOD slot
  (sunrise / day / sunset / night).
- `DalcCubeYup` ([`byroredux/src/components.rs`](../../byroredux/src/components.rs))
  converts Bethesda Z-up axes to engine Y-up once per TOD slot via
  `from_skyrim_zup` (the project-wide `(x, y, z) → (x, z, -y)` swap):
  Bethesda +Z sky-fill → engine `pos_y`, -Z ground-bounce → `neg_y`,
  ±Y north/south → engine ∓Z, ±X unchanged. `DalcCubeYup::lerp`
  per-component-blends two TOD slots.
- `weather_system`
  ([`byroredux/src/systems/weather.rs`](../../byroredux/src/systems/weather.rs))
  picks and lerps the active TOD pair (see "TOD clock" below) and writes
  the result to `SkyParamsRes.current_dalc_cube`.
- The render path assembles a `GpuDalcCube`
  ([`crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`](../../crates/renderer/src/vulkan/scene_buffer/gpu_types.rs)),
  uploads it via `upload_dalc` to a per-frame UBO (set 1, binding 14),
  and `triangle.frag::sampleDalcCube` samples it along the fragment
  normal. `flags.x == 1.0` means an authored cube is present (Skyrim
  cells); otherwise the shader falls back to the legacy
  `ambient * max(combinedAO, AMBIENT_AO_FLOOR)` path, so FNV / FO3 /
  Oblivion exterior rendering is unchanged. This replaced the hand-tuned
  `AMBIENT_AO_FLOOR = 0.3` Skyrim-canyon-dimness fudge (#993).

### Fog as a component — `FogVolume`

[`crates/core/src/ecs/components/fog_volume.rs`](../../crates/core/src/ecs/components/fog_volume.rs)
defines a `FogVolume` component (NiFogProperty- and XCLL-fed depth fog,
optional cubic clip/power curve). The XCLL→cell-scope `FogVolume` spawn
is a deferred follow-up; the active cell-fog path today is the
`CellLightingRes` fog fields → composite-shader plumbing above.

### Lighting template fallback — LGTM / LTMP (#566)

Vanilla Skyrim ships interior cells (Solitude inn cluster, Dragonsreach
throne room, Markarth) that omit `XCLL` and inherit from a Lighting
Template. `CellData.lighting_template_form` carries the `LTMP` FormID;
`LgtmRecord` lives in
[`crates/plugin/src/esm/records/misc/world.rs`](../../crates/plugin/src/esm/records/misc/world.rs)
and is indexed in `EsmIndex.lighting_templates`. The resolution chain is
`resolve_cell_lighting`
([`byroredux/src/cell_loader/load.rs`](../../byroredux/src/cell_loader/load.rs)):

1. **Explicit XCLL wins** — the cell's parsed `CellLighting` is returned
   verbatim.
2. **LGTM synthesises a `CellLighting`** — when the cell has no XCLL but
   its `LTMP` resolves through `index.lighting_templates`, the template's
   ambient / directional / fog scalars (plus `directional_fade` /
   `fog_clip` / `fog_power`) project into a fresh `CellLighting`. Fields
   the LGTM stub doesn't carry (`directional_rotation`, the ambient cube,
   specular) stay at pre-XCLL defaults — `directional_rotation = [0, 0]`
   (sun-from-+X), Skyrim-extended optionals `None`.
3. **No XCLL and no resolvable LGTM** → `None` → engine-default ambient.

Pre-#566 the LTMP link was unparsed and every template-only cell rendered
with the engine default.

### How the directional reaches `GpuLight`

`collect_lights` reads `CellLightingRes` and pushes one directional
`GpuLight` (`color_type.w = 2.0` marks it directional).
`compute_directional_upload` ([`byroredux/src/render/mod.rs`](../../byroredux/src/render/mod.rs))
splits interior vs exterior:

- **Interior:** `0.6×` of `cell_lit.directional_color` as a constant
  *fill* (independent of weather `sun_intensity` — the XCLL value is the
  fill source, not a physical sun) and a `radius = -1.0` sentinel.
- **Exterior:** ramp the contribution by
  `sun_intensity / SUN_INTENSITY_PEAK` (`SUN_INTENSITY_PEAK = 4.0`),
  so the directional tracks the TOD sun.

Three independent gates keep the exterior sun from drawing a hard-edged
light shaft on an interior floor (regression-guarded by #1282 tests):
the `0.6×` interior scale + `radius = -1` sentinel; the shader treating
`radius < 0` as an isotropic fill (no Lambert / N·L term); and the RT
shadow-ray loop skipping the directional when it's an interior fill (no
`vkRayQuery` cast).

### TOD clock — `weather_system`

`weather_system` advances the game clock and samples the climate-driven
time-of-day colour/intensity. `build_tod_keys(tod_hours)` builds a
7-entry `(hour, TOD slot)` table from a climate's `tod_hours =
[sunrise_begin, sunrise_end, sunset_begin, sunset_end]` (CLMT TNAM,
#463); `pick_tod_pair` walks it to the bracketing slot pair + blend
fraction; `compute_sun_arc` derives sun direction + intensity along the
visible-sun arc; `tod_slot_night_factor` lerps fog distance through the
same slot pair so palette and fog stay in lockstep. This is the clock the
DALC cube, sky params, and exterior directional all sample.

---

## Legacy: How Bethesda Uses CELL Lighting Data

Each CELL record stores lighting parameters across two groups:

### Lighting Tab (template-inheritable)
- **Ambient Color** — flat color applied uniformly to all surfaces
- **Directional Color** — simulates a dominant light source (sun, key light)
- **Directional Rotation/Fade** — orientation and falloff of the directional light
- **Fog Color/Near/Far/Power/Max** — distance fog with separate near and far colors
- **High Near/Far Color** — altitude-dependent fog tint (Fallout 4+)
- **Clip Distance** — draw distance cutoff
- **Show Sky / Use Sky Lighting / Sunlight Shadows** — exterior-like behavior flags

### Directional Ambient Lighting Tab
Six RGB values representing ambient light from each axis direction:
- **X+** (east), **X-** (west)
- **Y+** (north), **Y-** (south)
- **Z+** (up), **Z-** (down)

Plus a "Set From Ambient" option that copies the flat ambient color to all 6 axes.

In the original engine, these 6 values are interpolated per-vertex or per-pixel
based on the surface normal direction. A surface facing up gets mostly the Z+
color, a surface facing east gets mostly X+, etc. This is essentially a
**first-order spherical harmonics** approximation of ambient lighting — cheap
but surprisingly effective for conveying directional ambient.

> Implemented for the *weather* path (`WTHR.DALC`) as `GpuDalcCube` —
> see above. The per-cell *XCLL* ambient cube is parsed
> (`CellLighting.directional_ambient`) but not yet on the GPU.

### Lighting Templates
Cells reference a Lighting Template and selectively override individual fields.
A dungeon template might set fog and ambient, while a specific cell overrides
only the directional light to create a shaft of light from a window. This is
the same selective-override pattern as our plugin system's `Patch` strategy.

> The XCLL-wins → LGTM-synthesise → engine-default chain is live
> (`resolve_cell_lighting`, #566). Per-*field* LGTM inheritance (a cell
> overriding only directional while inheriting fog from its template) is
> not yet modelled — LGTM is consulted only when XCLL is absent
> entirely. Tracked under #379.

---

## Redux: From Flat Shading to Ray-Traced GI

> Original design vision (M28/N26 era). The renderer is now an RT
> multi-light pipeline with streaming RIS, SVGF denoising, and a
> composite/ACES pass — closer to Tier 2 than this section's "pick a
> tier by hardware" framing. The CELL data still drives it as a control
> layer, which is the durable idea here.

The key insight: **the CELL lighting data encodes artistic intent about the
light environment**, not a rendering technique. "Light comes mostly from above
and is warm, fog is thick and bluish" is useful information regardless of
whether you render it with flat ambient, SH probes, or full path tracing.

### Tier 0: Legacy-Compatible (flat shading)
Reproduce the original engine's behavior:
- Apply ambient color uniformly
- Use the 6-axis values as a normal-weighted ambient term
- Directional light as a single shadow-casting light
- Distance fog with near/far color interpolation

This is the baseline for loading existing game content and having it look
correct relative to the original engine.

### Tier 1: Irradiance Probes (medium hardware)
Upgrade the 6-axis ambient to a proper probe grid:
- **Seed from CELL data** — the 6 directional ambient values initialize
  probe coefficients (L1 spherical harmonics) at the cell center
- **Probe grid placement** — subdivide the cell volume, place probes at
  regular intervals, initialize each from the cell's 6-axis data
- **Runtime refinement** — optionally update probes by casting rays from
  each probe position, accumulating actual indirect lighting
- **Cell transitions** — interpolate between adjacent cell probe grids
  at boundaries (interior doors, exterior cell seams)

The 6-axis data gives us a reasonable starting state without any precompute.
A dungeon cell with Z+ = dark, Z- = warm orange already produces plausible
floor bounce light from the probe grid.

> **Partially shipped (Session 42).** The 6-axis normal-weighted ambient
> term is live for Skyrim via `GpuDalcCube`/`sampleDalcCube` (TOD-lerped
> in engine Y-up). A full probe *grid* with cross-cell interpolation has
> not been built; the current sample is a single per-frame cube.

### Tier 2: Ray-Traced GI (high-end hardware)
Full hardware ray tracing, using CELL data as the control layer:
- **Per-cell probe placement** driven by the cell's spatial extent
- **Directional ambient as sky model** — the 6-axis colors define a
  low-resolution environment map for rays that escape the cell geometry
- **Fog as participating media** — the fog near/far/color parameters
  seed volumetric ray marching density and scattering color
- **Template inheritance = LOD cascading** — cells that inherit lighting
  from a template share probe data, reducing memory for large worldspaces

> The RT pipeline (ray-query shadows / reflections / GI, SVGF temporal
> denoise, M55 volumetric froxel grid) is live; CELL fog drives a depth
> fog curve in the composite pass. The specific "6-axis colors as the
> escaped-ray environment map" and "fog params seed the froxel density"
> couplings described here are not wired — the RT GI and the volumetrics
> grid don't currently read XCLL.

### Hybrid Fallback
All three tiers read from the same per-frame lighting state. Originally
sketched against flat `CellLighting`/`CellAmbientLight` components; in the
shipped tree the state lives on the `CellLightingRes` resource plus the
`GpuDalcCube` UBO (see "Where the cell-lighting data lives today"). The
renderer picks behaviour from data availability (`dalcFlags.x`,
`fog_clip`/`fog_power` activation, the interior `radius < 0` sentinel)
rather than a hardware-tier switch:

```
WTHR.DALC cube (Skyrim)            XCLL fog (all games)
        │                                  │
 DalcCubeYup (Y-up, TOD-lerped)     CellLightingRes.fog_*
        │                                  │
 GpuDalcCube UBO (set 1, binding 14) composite fog_params.{x,y,z,w}
        │                                  │
 triangle.frag::sampleDalcCube      composite.frag linear/cubic fog
```

The artistic intent survives: a modder setting the Skyrim sky-fill bright
blue gets "light from above is blue," and the renderer degrades to the
`AMBIENT_AO_FLOOR` path on games / cells without an authored cube.

---

## ECS Component Design (original sketch — see "Where the data lives" for current)

> This block is the original design. The shipped layout collapsed the
> four components below into the `CellLightingRes` World resource +
> `DalcCubeYup`/`GpuDalcCube` + `FogVolume` documented above. Kept for
> design intent; the field names here are **not** the current API.

```rust
/// Per-cell ambient light, derived from the 6-axis directional ambient.
/// Stored as linear RGB (convert from sRGB on load).
pub struct CellAmbientLight {
    pub x_pos: Vec3,  // east
    pub x_neg: Vec3,  // west
    pub y_pos: Vec3,  // north
    pub y_neg: Vec3,  // south
    pub z_pos: Vec3,  // up
    pub z_neg: Vec3,  // down
}

/// Per-cell lighting parameters (from Lighting tab).
pub struct CellLighting {
    pub ambient_color: Vec3,
    pub directional_color: Vec3,
    pub directional_rotation: Vec2,  // azimuth, elevation
    pub directional_fade: f32,
    pub fog: CellFog,
    pub clip_distance: f32,
    pub use_sky_lighting: bool,
    pub sunlight_shadows: bool,
    pub template: Option<FormId>,        // lighting template reference
    pub template_overrides: u32,         // bitfield: which fields are overridden
}

/// Fog parameters extracted from CELL.
pub struct CellFog {
    pub near_color: Vec3,
    pub far_color: Vec3,
    pub near: f32,
    pub far: f32,
    pub power: f32,
    pub max: f32,
    pub high_near_color: Vec3,   // altitude-dependent (Fallout 4+)
    pub high_far_color: Vec3,
    pub near_height: f32,
    pub far_height: f32,
}

/// Interior-specific cell data.
pub struct CellInterior {
    pub name: FixedString,
    pub encounter_zone: Option<FormId>,
    pub owner_npc: Option<FormId>,
    pub owner_faction: Option<FormId>,
    pub public_area: bool,
    pub off_limits: bool,
    pub cant_wait: bool,
    pub offset: Vec3,
}
```

> Reconciliation note: the real `CellLighting` (plugin parse output) and
> `CellLightingRes` (renderer resource) keep the extended fields as
> `Option`s rather than a `template_overrides` bitfield, and per-field
> template inheritance is still future work (#379). Cell colour-space
> handling is *not* `srgb_to_linear` — Gamebryo colours are raw
> monitor-space floats (verified against 2.3 source); the
> "convert from sRGB on load" comment above is incorrect for this engine.
> Cell metadata fields the original `CellInterior` sketch covers (owner,
> public-area, encounter zone) live on `CellData` / `CellOwnership` in
> the plugin layer rather than a single ECS component.

---

## Mod Compatibility

Because lighting templates use selective field inheritance, and our plugin
system resolves overrides via the dependency DAG:

- A mod that changes only fog in a cell produces a `Patch` override touching
  only the fog fields — ambient and directional light are untouched
- Two mods that change different fields of the same cell can be auto-merged
  (one changes fog, the other changes ambient → no conflict)
- Two mods that change the same field → `TieBreak` conflict, flagged for user
  review, resolved deterministically by PluginId order

This is a massive improvement over the legacy system where any cell edit
replaces the entire lighting record, making multi-mod lighting changes
inherently incompatible.

> Status: this is the plugin system's design goal. The field-level
> override granularity for *lighting specifically* depends on per-field
> LGTM inheritance (#379), which is not yet implemented — today XCLL is
> all-or-nothing per cell and LGTM only fills in when XCLL is absent.

---

## References

- [CELL record (Fallout 4 CK Wiki)](https://falloutck.uesp.net/wiki/Cell)
- [Lighting Template (CK Wiki)](https://falloutck.uesp.net/wiki/Lighting_Template)
- Spherical Harmonics for ambient: Ramamoorthi & Hanrahan, 2001
- Cell record structure saved in memory: `cell_record_structure.md`
- Per-mesh light parser: [`crates/nif/src/blocks/light.rs`](../../crates/nif/src/blocks/light.rs)
- Scene-graph light extraction: [`crates/nif/src/import/walk/mod.rs`](../../crates/nif/src/import/walk/mod.rs)
- Cell-load light spawn: [`byroredux/src/cell_loader/spawn.rs`](../../byroredux/src/cell_loader/spawn.rs)
- Light collection / LIGH translation: [`byroredux/src/render/lights.rs`](../../byroredux/src/render/lights.rs)
- DALC ambient cube (GPU): [`crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`](../../crates/renderer/src/vulkan/scene_buffer/gpu_types.rs)
