# Cell-Based Lighting Architecture

## Per-mesh NiLight sources (N26 audit follow-up)

Cell lighting isn't the only source of light in a Bethesda world.
Individual meshes can embed `NiLight` subclasses — ambient, directional,
point, spot — right in the scene graph. Every Oblivion torch, candle,
and magic effect carries one (and FO3+ lamps / Skyrim braziers too).
Until the N26 audit these were silently dropped in Oblivion, leaving
the cell XCLL ambient / directional as the only illumination source
and making night-time scenes unplayable.

The NIF parser now handles the full `NiLight` hierarchy (#156):

- `crates/nif/src/blocks/light.rs` implements `NiLightBase` +
  `NiAmbientLight` / `NiDirectionalLight` / `NiPointLight` /
  `NiSpotLight` with per-version gates for the NiDynamicEffect base
  (switch_state + affected-node ptr list) and NiLight scalar fields
  (dimmer + ambient / diffuse / specular color3).
- `crates/nif/src/import/walk.rs::walk_node_lights` pulls lights out
  of the scene graph in world space, composing the parent chain and
  deriving each light's effective radius from the attenuation
  polynomial (solves for the distance where contribution drops below
  1/256 of peak — Bethesda's shader cull threshold). Ambient /
  directional lights clamp to a 2048-unit fallback.
- `byroredux/src/cell_loader.rs::load_nif_placed` spawns a
  `Transform + GlobalTransform + LightSource` ECS entity per extracted
  light, parented through the reference transform so torches inside
  cell references contribute to the same `GpuLight` buffer that the
  XCLL directional light already flows into.

Result: the renderer sees both "cell environment light" (XCLL) and
"per-object point lights" (NiLight) through the same
`build_render_data()` path, feeding the same SSBO and the same ray
query shadow pass. From the renderer's perspective there's no
difference between a torch inside a REFR and a cell's ambient colour.

FO4+ (BSVER ≥ 130) reparents `NiLight` onto `NiAVObject` and drops
the `NiDynamicEffect` base — not implemented until FO4 cell rendering
becomes a target.

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

### Lighting Templates
Cells reference a Lighting Template and selectively override individual fields.
A dungeon template might set fog and ambient, while a specific cell overrides
only the directional light to create a shaft of light from a window. This is
the same selective-override pattern as our plugin system's `Patch` strategy.

---

## Redux: From Flat Shading to Ray-Traced GI

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

### Tier 2: Ray-Traced GI (high-end hardware)
Full hardware ray tracing, using CELL data as the control layer:
- **Per-cell probe placement** driven by the cell's spatial extent
- **Directional ambient as sky model** — the 6-axis colors define a
  low-resolution environment map for rays that escape the cell geometry
- **Fog as participating media** — the fog near/far/color parameters
  seed volumetric ray marching density and scattering color
- **Template inheritance = LOD cascading** — cells that inherit lighting
  from a template share probe data, reducing memory for large worldspaces

### Hybrid Fallback
All three tiers read from the same ECS components (`CellLighting`,
`CellAmbientLight`). The renderer picks the technique based on hardware:

```
CellAmbientLight { x_pos, x_neg, y_pos, y_neg, z_pos, z_neg }
         │
         ├── Tier 0: normal-weighted flat ambient
         ├── Tier 1: seed SH probes, interpolate per-fragment
         └── Tier 2: environment cubemap for escaped rays
```

The artistic intent survives across all tiers. A modder setting Z+ to bright
blue gets "light from above is blue" whether the player runs on integrated
graphics or an RTX 5090.

---

## ECS Component Design

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

All components use `SparseSetStorage` — cells are sparse relative to entities.
Systems query whichever component they need: the fog system reads `CellFog`,
the probe system reads `CellAmbientLight`, the UI reads `CellInterior::name`.

---

## Mod Compatibility

Because lighting templates use selective field inheritance, and our plugin
system resolves overrides via the dependency DAG:

- A mod that changes only fog in a cell produces a `Patch` override touching
  only the `CellFog` fields — ambient and directional light are untouched
- Two mods that change different fields of the same cell can be auto-merged
  (one changes fog, the other changes ambient → no conflict)
- Two mods that change the same field → `TieBreak` conflict, flagged for user
  review, resolved deterministically by PluginId order

This is a massive improvement over the legacy system where any cell edit
replaces the entire lighting record, making multi-mod lighting changes
inherently incompatible.

---

## References

- [CELL record (Fallout 4 CK Wiki)](https://falloutck.uesp.net/wiki/Cell)
- [Lighting Template (CK Wiki)](https://falloutck.uesp.net/wiki/Lighting_Template)
- Spherical Harmonics for ambient: Ramamoorthi & Hanrahan, 2001
- Cell record structure saved in memory: `cell_record_structure.md`
