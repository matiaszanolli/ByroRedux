//! Application-specific marker components and resources.

use byroredux_audio::Sound;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, Resource, SparseSetStorage};
use byroredux_core::math::Vec3;
use byroredux_core::string::FixedString;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use winit::keyboard::KeyCode;

/// Marker component for entities that should spin in the demo scene.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Spinning;
impl Component for Spinning {
    type Storage = SparseSetStorage<Self>;
}

/// Component for entities that use alpha blending, carrying the Gamebryo
/// blend factors extracted from NiAlphaProperty flags.
///
/// Gamebryo AlphaFunction enum (bits 1–4 = src, bits 5–8 = dst):
///   0=ONE, 1=ZERO, 2=SRC_COLOR, 3=INV_SRC_COLOR, 4=DEST_COLOR,
///   5=INV_DEST_COLOR, 6=SRC_ALPHA, 7=INV_SRC_ALPHA, 8=DEST_ALPHA,
///   9=INV_DEST_ALPHA, 10=SRC_ALPHA_SATURATE.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AlphaBlend {
    pub(crate) src_blend: u8,
    pub(crate) dst_blend: u8,
}
impl Component for AlphaBlend {
    type Storage = SparseSetStorage<Self>;
}

/// Marker component for entities that need two-sided rendering (no backface culling).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TwoSided;
impl Component for TwoSided {
    type Storage = SparseSetStorage<Self>;
}

/// REFR placement carries an XTEL teleport destination — this entity is
/// a door whose activation transports the player to another cell.
///
/// Captured from `PlacedRef.teleport` at spawn time (M40 Phase 2 Stage 1,
/// data plumbing). The destination FormID is the *target* REFR
/// (typically another door in the destination cell); the position +
/// rotation are the spot the player materializes at, in Bethesda Z-up
/// world units / radians.
///
/// `EsmCellIndex::cell_for_refr_form_id` resolves `destination_form_id`
/// back to the parent cell (interior editor-ID or exterior worldspace +
/// grid). The `door.teleport` console command consumes both halves
/// today; Stage 4 wires an `ActivateEvent`-driven system around the
/// same lookup for F-key proximity activate.
///
/// Sparse storage — Bethesda content ships ~0.1% of REFRs as doors
/// (rough estimate from FNV Goodsprings exterior + Megaton interior).
#[derive(Debug, Clone, Copy)]
pub(crate) struct DoorTeleport {
    /// Destination REFR's FormID. Resolved against the load-order
    /// remapped plugin index via `EsmCellIndex::cell_for_refr_form_id`.
    pub(crate) destination_form_id: u32,
    /// Destination position in Bethesda units (Z-up). The consumer
    /// flips to engine Y-up via the `coord` helper at camera-set time.
    pub(crate) position_zup: [f32; 3],
    /// Destination Euler rotation in radians (X, Y, Z). Conversion to
    /// the renderer's quaternion is the consumer's responsibility.
    pub(crate) rotation_zup: [f32; 3],
}
impl Component for DoorTeleport {
    type Storage = SparseSetStorage<Self>;
}

/// Marker component for "FX" decorative meshes (`effects/fx*`, `fxsoftglow`,
/// `fxpartglow`, `fxparttiny`, `fxlightrays`) that the renderer drops on
/// the floor. Lifted from a per-draw, per-frame substring scan over the
/// material's texture path to a one-time spawn-time classification per
/// PERF-D3-NEW-02 / #1136 — at MedTek scale (~7000 draws × 6 needles)
/// this saved ~26 M byte comparisons per frame.
///
/// Set in [`crate::cell_loader::spawn`] and [`crate::scene`] (the two
/// `Material`-spawning sites); consumed in [`crate::render::build_render_data`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct IsFxMesh;
impl Component for IsFxMesh {
    type Storage = SparseSetStorage<Self>;
}

/// Marker for an authored decal surface, distinct from the broader
/// [`RenderLayer::Decal`](byroredux_core::ecs::RenderLayer::Decal) depth-bias
/// class which also contains ordinary alpha-tested cutouts. FO4 BGSM decals
/// need dedicated depth/RT handling even when their generic blend state is
/// disabled. The low-threshold soft-coverage subset is classified separately
/// by [`decal_uses_implicit_alpha_blend`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct IsDecalMesh;
impl Component for IsDecalMesh {
    type Storage = SparseSetStorage<Self>;
}

/// FO4's dedicated decal pass treats low-threshold alpha-tested BGSM decals as
/// texture-alpha coverage even when the material's generic blend function is
/// `None`. High-threshold decals are binary cutouts (posters/signs) and must
/// remain opaque after their alpha test; applying alpha-over to those washes
/// them out.
///
/// The 0.25 boundary deliberately sits between the authored clusters observed
/// in vanilla FO4: grime/damage/mud decals use references 6..44 (0.024..0.173),
/// while poster/sign cutouts use 128 (0.502). Explicitly blended decals never
/// need this compatibility path because their authored factors take priority.
pub(crate) fn decal_uses_implicit_alpha_blend(
    is_decal: bool,
    has_alpha_blend: bool,
    alpha_test: bool,
    alpha_threshold: f32,
) -> bool {
    is_decal && !has_alpha_blend && alpha_test && alpha_threshold < 0.25
}

/// Marker component for distant-terrain LOD blocks (engine-generated from
/// the worldspace heightmaps beyond the full-detail streamed ring). These
/// are coarse multi-cell meshes with a single base texture, no splat
/// layers, and **no BLAS** — they rasterize for far view distance but stay
/// out of the TLAS (RT shadows/GI/reflections sample the full-detail
/// near terrain only), so they cost zero ray-tracing budget.
///
/// Set in [`crate::cell_loader::terrain_lod::spawn_lod_block`]; consumed by
/// [`crate::render::collect_lod_terrain_draws`] (their dedicated lean draw
/// path) and skipped by the heavy `collect_static_mesh_draws` loop so they
/// are never emitted twice or charged the ~13-component per-entity walk.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IsLodTerrain;
impl Component for IsLodTerrain {
    type Storage = SparseSetStorage<Self>;
}

/// Marker for a placement whose base record carries the **Visible-When-Distant**
/// / "Has Distant LOD" header flag, materialised at cell-load from
/// [`byroredux_plugin::esm::cell::StaticObject::visible_when_distant`] (which the
/// ESM reader surfaces via `RecordHeader::is_visible_when_distant`, #1731).
///
/// It records, per placement root, that this object is one the world's
/// distant-LOD baker folded into its `.bto` / `DistantLOD\*.lod` proxies — i.e.
/// a full model that *has* a distant substitute. It is the per-record signal
/// EXAL §5.2 calls for: the hook a full-model cull would read to suppress the
/// full mesh once its LOD proxy covers the cell.
///
/// **No render-time consumer today, by design.** Full REFRs only ever spawn
/// inside the cell-streaming ring (`d <= radius_unload`) and both LOD rings load
/// strictly outside it (`d > radius_unload`, #1866), so a full model and its LOD
/// proxy never coexist — the conservative ring already prevents the z-fighting a
/// per-record cull would. Turning this marker into an active cull requires
/// decoupling the full-detail radius from the streaming ring (which would
/// reintroduce the #1866 overlap risk and needs real-game visual validation
/// before it can be enabled). This component materialises the signal so that
/// work does not have to re-derive the parse→spawn plumbing. See #1889.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VisibleWhenDistant;
impl Component for VisibleWhenDistant {
    type Storage = SparseSetStorage<Self>;
}

/// Returns `true` if `texture_path` matches any of the 6 "FX decoration"
/// needles the renderer drops. Pulled out as a shared helper so the
/// spawn-time tagger in `cell_loader::spawn` + `scene::nif_loader` and
/// any future caller stays in lockstep. PERF-D3-NEW-02 / #1136.
pub(crate) fn texture_path_is_fx_mesh(texture_path: &str) -> bool {
    fn contains_ci(haystack: &str, needle: &str) -> bool {
        haystack
            .as_bytes()
            .windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
    }
    contains_ci(texture_path, "effects\\fx")
        || contains_ci(texture_path, "effects/fx")
        || contains_ci(texture_path, "fxsoftglow")
        || contains_ci(texture_path, "fxpartglow")
        || contains_ci(texture_path, "fxparttiny")
        || contains_ci(texture_path, "fxlightrays")
}

// The old generic `Decal` marker was retired in #renderlayer: depth ordering
// is expressed by `RenderLayer::Decal`. `IsDecalMesh` above is intentionally
// narrower and retains only authored decal-surface semantics needed for
// alpha compositing, depth writes, and RT participation.

/// Bindless texture handle for a normal map (parallels TextureHandle for
/// diffuse). `.1` = whether the loaded DDS carries an alpha channel — Skyrim/
/// Gamebryo author the gloss mask in the normal alpha, so this gates the
/// normal-alpha-as-spec path (false for BC5/alpha-less normals → scalar
/// fallback). Captured from the texture registry at load. See static_meshes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NormalMapHandle(pub(crate) u32, pub(crate) bool);
impl Component for NormalMapHandle {
    type Storage = SparseSetStorage<Self>;
}

/// Bindless texture handle for a dark/lightmap (NiTexturingProperty slot 1).
/// Multiplicative modulation: `albedo.rgb *= dark_sample.rgb`. See #264.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DarkMapHandle(pub(crate) u32);
impl Component for DarkMapHandle {
    type Storage = SparseSetStorage<Self>;
}

/// Bindless texture handle for a `BSEffectShaderProperty.greyscale_texture`
/// colour palette LUT (Skyrim+). When attached, the fragment shader's
/// `MATERIAL_KIND_EFFECT_SHADER` branch samples
/// `textures[handle]` at `vec2(source.r, 0.5)` to remap a greyscale-
/// authored source texture (typical for fire / explosion / VFX atlases)
/// into authored colour. Only attached when the importer captured a
/// non-empty `greyscale_texture` path AND it resolved to a real texture
/// (so the shader's `handle != 0` gate is meaningful). See #890 Stage 2c.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GreyscaleLutHandle(pub(crate) u32);
impl Component for GreyscaleLutHandle {
    type Storage = SparseSetStorage<Self>;
}

/// Bindless texture indices for the three NiTexturingProperty slots that
/// previously populated `Material` but never reached `GpuInstance`:
/// glow (slot 4 — emissive overlay), detail (slot 2 — high-frequency
/// 2× UV overlay), and gloss (slot 3 — per-texel specular mask). All
/// three default to `0` (= no map; shader falls through to the inline
/// material constants). Combined into a single component to keep the
/// per-frame query count fixed regardless of which slots a mesh uses.
/// See #399 (OBL-D4-H3).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExtraTextureMaps {
    pub(crate) glow: u32,
    pub(crate) detail: u32,
    pub(crate) gloss: u32,
    /// Parallax / height map (`BSShaderTextureSet` slot 3). 0 = no POM.
    /// See #453 (renderer-side plumbing) and #452 (import-side path).
    pub(crate) parallax: u32,
    /// Env reflection map (slot 4). 0 = no env map. See #453.
    pub(crate) env: u32,
    /// Env reflection mask (slot 5). 0 = unmasked. See #453.
    pub(crate) env_mask: u32,
    /// POM height scale (default 0.04). See #453.
    pub(crate) parallax_height_scale: f32,
    /// POM ray-march sample budget (default 4.0). See #453.
    pub(crate) parallax_max_passes: f32,
}
impl Component for ExtraTextureMaps {
    type Storage = SparseSetStorage<Self>;
}

/// Terrain splat-layer tile index into the renderer's
/// `GpuTerrainTile` SSBO (scene set 1, binding 10). Attached only to
/// LAND terrain entities when ATXT/VTXT splat layers are present.
/// `render.rs` forwards this into `DrawCommand::terrain_tile_index`,
/// which `draw.rs` packs into the top 16 bits of `GpuInstance.flags`
/// alongside the `INSTANCE_FLAG_TERRAIN_SPLAT` bit. See #470.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TerrainTileSlot(pub(crate) u32);
impl Component for TerrainTileSlot {
    type Storage = SparseSetStorage<Self>;
}

// SystemList moved to byroredux_core::ecs::resources::SystemList

/// Cell lighting from the ESM (ambient + directional + fog).
///
/// The base block (`ambient`/`directional_*`/`fog_*`) is populated for
/// every cell — interior XCLL on `load_cell_with_masters`, exterior
/// WTHR on `apply_worldspace_weather`, or engine-default constants
/// when no plugin data is available.
///
/// The optional Skyrim/FNV extended block (`directional_fade` ..
/// `fresnel_power`) is populated only when the source cell carries an
/// XCLL with the matching tail length. Pre-#861 every consumer dropped
/// these on the floor at the renderer-facing boundary even though the
/// plugin parser had been extracting them since #379 (FNV) / #367
/// (Skyrim). Now flowed through end-to-end on the CPU side; shader
/// consumption follows in #865 (fog curve) and a future Skyrim
/// ambient-cube uniform. Per-field `#[allow(dead_code)]` is the
/// staged-rollout marker — removed in lockstep with each shader-side
/// consumer landing, so an unused-after-shader-lands field surfaces
/// as a warning instead of silently growing dead.
pub(crate) struct CellLightingRes {
    pub(crate) ambient: [f32; 3],
    pub(crate) directional_color: [f32; 3],
    /// Direction vector in Y-up space (computed from rotation).
    pub(crate) directional_dir: [f32; 3],
    /// True when the cell is interior. Interior XCLL directional is a
    /// subtle tint, not a physical sun — we skip it as a scene light to
    /// avoid leak artifacts on walls that shouldn't see the sky.
    pub(crate) is_interior: bool,
    /// Fog color (RGB 0-1).
    pub(crate) fog_color: [f32; 3],
    /// Fog near distance (game units).
    pub(crate) fog_near: f32,
    /// Fog far distance (game units).
    pub(crate) fog_far: f32,
    // ── Extended XCLL fields (FNV 40-byte tail + Skyrim 92-byte tail) ──
    // All of these are now read by the `cell.lighting` console command
    // and/or the lighting plumbing (see #865 for the fog curve), so the
    // former per-field `#[allow(dead_code)]` guards have been removed.
    /// Directional light fade multiplier — bytes 28-31 of the XCLL.
    /// FNV+ XCLL.
    pub(crate) directional_fade: Option<f32>,
    /// Cubic-fog clip distance — bytes 32-35. FNV+ XCLL. Used together
    /// with `fog_power` to drive a non-linear fog curve in place of
    /// the linear `fog_near..fog_far` ramp. Plumbed to the composite
    /// shader via `fog_params.z` (#865 / FNV-D3-NEW-06).
    pub(crate) fog_clip: Option<f32>,
    /// Cubic-fog falloff exponent — bytes 36-39. FNV+ XCLL. Plumbed
    /// to the composite shader via `fog_params.w` (#865).
    pub(crate) fog_power: Option<f32>,
    /// Fog far color (RGB 0-1) — bytes 72-74. Skyrim+ XCLL. Distinct
    /// from `fog_color` (which is the near-distance fog tint).
    pub(crate) fog_far_color: Option<[f32; 3]>,
    /// Maximum fog opacity — bytes 76-79. Skyrim+ XCLL.
    pub(crate) fog_max: Option<f32>,
    /// Light fade begin distance — bytes 80-83. Skyrim+ XCLL.
    pub(crate) light_fade_begin: Option<f32>,
    /// Light fade end distance — bytes 84-87. Skyrim+ XCLL.
    pub(crate) light_fade_end: Option<f32>,
    /// Directional ambient cube — `[+X, -X, +Y, -Y, +Z, -Z]` RGB
    /// triplets from bytes 40-63. Drives the per-cell ambient probe;
    /// ±Z asymmetry is what makes Skyrim cave floors read warm while
    /// ceilings read cool without a dedicated IBL pass. Skyrim+ XCLL.
    pub(crate) directional_ambient: Option<[[f32; 3]; 6]>,
    /// Specular tint (RGB) — bytes 64-66. Skyrim+ XCLL.
    pub(crate) specular_color: Option<[f32; 3]>,
    /// Specular alpha — byte 67. Stored separately so consumers can
    /// decide whether the RGBA packing was intentional or padding.
    pub(crate) specular_alpha: Option<f32>,
    /// Fresnel power exponent — bytes 68-71. Skyrim+ XCLL.
    pub(crate) fresnel_power: Option<f32>,
}

impl CellLightingRes {
    /// Construct a `CellLightingRes` from a fully-resolved
    /// [`byroredux_plugin::esm::cell::CellLighting`] (the plugin
    /// layer's parser output) plus the renderer-facing direction
    /// vector and the interior/exterior flag. Carries the 9 extended
    /// XCLL fields through verbatim — see #861. Producers in
    /// `scene.rs` use this helper for the interior `--esm --cell`
    /// arm; exterior weather (`apply_worldspace_weather`) and the
    /// engine-default constant path build the struct directly with
    /// `extended_*` set to `None`.
    pub(crate) fn from_cell_lighting(
        lit: &byroredux_plugin::esm::cell::CellLighting,
        directional_dir: [f32; 3],
        is_interior: bool,
    ) -> Self {
        Self {
            ambient: lit.ambient,
            directional_color: lit.directional_color,
            directional_dir,
            is_interior,
            fog_color: lit.fog_color,
            fog_near: lit.fog_near,
            fog_far: lit.fog_far,
            directional_fade: lit.directional_fade,
            fog_clip: lit.fog_clip,
            fog_power: lit.fog_power,
            fog_far_color: lit.fog_far_color,
            fog_max: lit.fog_max,
            light_fade_begin: lit.light_fade_begin,
            light_fade_end: lit.light_fade_end,
            directional_ambient: lit.directional_ambient,
            specular_color: lit.specular_color,
            specular_alpha: lit.specular_alpha,
            fresnel_power: lit.fresnel_power,
        }
    }
}

impl Resource for CellLightingRes {}

#[cfg(test)]
mod cell_lighting_res_tests {
    //! Regression for #861 — extended XCLL fields propagate through the
    //! `CellLighting → CellLightingRes` boundary (FNV 40-byte tail and
    //! Skyrim 92-byte tail). Pre-fix every Optional past the 3 base
    //! fog fields was dropped on the floor at the renderer-facing
    //! resource layer.
    use super::*;
    use byroredux_plugin::esm::cell::CellLighting;

    fn fnv_xcll_with_fog_curve() -> CellLighting {
        // Mirrors the FNV 40-byte fixture in
        // crates/plugin/src/esm/cell/tests.rs:1683-1740 — directional_fade,
        // fog_clip, fog_power populated; Skyrim-only fields stay None.
        CellLighting {
            ambient: [0.10, 0.10, 0.12],
            directional_color: [1.0, 0.95, 0.80],
            directional_rotation: [0.0, 0.0],
            fog_color: [0.50, 0.45, 0.30],
            fog_near: 100.0,
            fog_far: 8000.0,
            directional_fade: Some(0.80),
            fog_clip: Some(7500.0),
            fog_power: Some(2.0),
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
            starfield: None,
        }
    }

    fn skyrim_xcll_with_full_extension() -> CellLighting {
        CellLighting {
            ambient: [0.05, 0.05, 0.06],
            directional_color: [0.8, 0.85, 1.0],
            directional_rotation: [0.5, 0.3],
            fog_color: [0.20, 0.25, 0.35],
            fog_near: 50.0,
            fog_far: 5000.0,
            directional_fade: Some(0.65),
            fog_clip: Some(4500.0),
            fog_power: Some(1.5),
            fog_far_color: Some([0.10, 0.10, 0.15]),
            fog_max: Some(0.85),
            light_fade_begin: Some(800.0),
            light_fade_end: Some(2400.0),
            directional_ambient: Some([
                [0.20, 0.18, 0.15], // +X
                [0.18, 0.16, 0.13], // -X
                [0.10, 0.10, 0.10], // +Y
                [0.05, 0.05, 0.05], // -Y
                [0.25, 0.25, 0.30], // +Z (warmer ceiling)
                [0.15, 0.13, 0.10], // -Z (cooler floor)
            ]),
            specular_color: Some([0.30, 0.30, 0.32]),
            specular_alpha: Some(1.0),
            fresnel_power: Some(2.5),
            starfield: None,
        }
    }

    #[test]
    fn from_cell_lighting_propagates_fnv_fog_curve_fields() {
        let lit = fnv_xcll_with_fog_curve();
        let res = CellLightingRes::from_cell_lighting(&lit, [0.0, 1.0, 0.0], true);

        // Base block.
        assert_eq!(res.ambient, [0.10, 0.10, 0.12]);
        assert_eq!(res.directional_color, [1.0, 0.95, 0.80]);
        assert_eq!(res.directional_dir, [0.0, 1.0, 0.0]);
        assert!(res.is_interior);
        assert_eq!(res.fog_color, [0.50, 0.45, 0.30]);
        assert_eq!(res.fog_near, 100.0);
        assert_eq!(res.fog_far, 8000.0);

        // FNV 40-byte tail — must be Some(...) and round-trip exactly.
        assert_eq!(res.directional_fade, Some(0.80));
        assert_eq!(res.fog_clip, Some(7500.0));
        assert_eq!(res.fog_power, Some(2.0));

        // Skyrim 92-byte tail — must stay None on a 40-byte FNV XCLL.
        assert!(res.fog_far_color.is_none());
        assert!(res.fog_max.is_none());
        assert!(res.light_fade_begin.is_none());
        assert!(res.light_fade_end.is_none());
        assert!(res.directional_ambient.is_none());
        assert!(res.specular_color.is_none());
        assert!(res.specular_alpha.is_none());
        assert!(res.fresnel_power.is_none());
    }

    #[test]
    fn from_cell_lighting_propagates_skyrim_92byte_extension() {
        let lit = skyrim_xcll_with_full_extension();
        let res = CellLightingRes::from_cell_lighting(&lit, [0.1, 0.9, -0.3], true);

        // FNV 40-byte tail.
        assert_eq!(res.directional_fade, Some(0.65));
        assert_eq!(res.fog_clip, Some(4500.0));
        assert_eq!(res.fog_power, Some(1.5));

        // Skyrim 92-byte tail.
        assert_eq!(res.fog_far_color, Some([0.10, 0.10, 0.15]));
        assert_eq!(res.fog_max, Some(0.85));
        assert_eq!(res.light_fade_begin, Some(800.0));
        assert_eq!(res.light_fade_end, Some(2400.0));
        let cube = res.directional_ambient.expect("Skyrim XCLL ambient cube");
        assert_eq!(cube[4], [0.25, 0.25, 0.30], "+Z (ceiling) face");
        assert_eq!(cube[5], [0.15, 0.13, 0.10], "-Z (floor) face");
        assert_eq!(res.specular_color, Some([0.30, 0.30, 0.32]));
        assert_eq!(res.specular_alpha, Some(1.0));
        assert_eq!(res.fresnel_power, Some(2.5));
    }

    #[test]
    fn from_cell_lighting_handles_pre_skyrim_xcll_with_no_extension() {
        // Oblivion-shape XCLL (28-byte head, no tail at all) — every
        // optional must stay None even though the parser emits the
        // CellLighting struct with the same field set.
        let lit = CellLighting {
            ambient: [0.30, 0.30, 0.30],
            directional_color: [0.90, 0.90, 0.85],
            directional_rotation: [0.0, 0.0],
            fog_color: [0.40, 0.40, 0.50],
            fog_near: 200.0,
            fog_far: 4000.0,
            directional_fade: None,
            fog_clip: None,
            fog_power: None,
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
            starfield: None,
        };
        let res = CellLightingRes::from_cell_lighting(&lit, [0.0, 1.0, 0.0], true);
        assert!(res.directional_fade.is_none());
        assert!(res.fog_clip.is_none());
        assert!(res.fog_power.is_none());
        assert!(res.directional_ambient.is_none());
    }
}

#[cfg(test)]
mod dalc_cube_tests {
    //! Regression for #993 — Skyrim DALC 6-axis ambient cube
    //! conversion and per-component lerp. Pinned distinctive colours
    //! per axis so a future refactor that swaps two axes can't
    //! silently land.
    use super::*;
    use byroredux_plugin::esm::records::weather::{SkyColor, SkyrimAmbientCube};

    fn rgb(r: u8, g: u8, b: u8) -> SkyColor {
        SkyColor { r, g, b, a: 0 }
    }

    fn distinctive_cube() -> SkyrimAmbientCube {
        // Each axis flagged with a unique R channel so the Z-up→Y-up
        // remap can't silently scramble axes.
        SkyrimAmbientCube {
            pos_x: rgb(0x10, 0, 0), // east
            neg_x: rgb(0x20, 0, 0), // west
            pos_y: rgb(0x30, 0, 0), // bethesda north
            neg_y: rgb(0x40, 0, 0), // bethesda south
            pos_z: rgb(0x50, 0, 0), // sky (bethesda up)
            neg_z: rgb(0x60, 0, 0), // ground (bethesda down)
            specular: rgb(0x70, 0, 0),
            fresnel_power: 1.0,
        }
    }

    #[test]
    fn from_skyrim_zup_maps_bethesda_up_to_engine_pos_y() {
        let yup = DalcCubeYup::from_skyrim_zup(&distinctive_cube());
        // Bethesda +Z (sky-fill) lands on engine +Y.
        let pos_y_r = (yup.pos_y[0] * 255.0).round() as u8;
        assert_eq!(pos_y_r, 0x50, "engine +Y must carry bethesda +Z sky-fill");
        // Bethesda -Z (ground-bounce) lands on engine -Y.
        let neg_y_r = (yup.neg_y[0] * 255.0).round() as u8;
        assert_eq!(neg_y_r, 0x60, "engine -Y must carry bethesda -Z ground");
    }

    #[test]
    fn from_skyrim_zup_swaps_lateral_north_south_to_engine_z() {
        let yup = DalcCubeYup::from_skyrim_zup(&distinctive_cube());
        // Bethesda +Y (north) → engine -Z per the project-wide
        // `(x, y, z) → (x, z, -y)` swap. Verified against
        // `crates/nif/src/import/coord.rs:18`.
        let neg_z_r = (yup.neg_z[0] * 255.0).round() as u8;
        let pos_z_r = (yup.pos_z[0] * 255.0).round() as u8;
        assert_eq!(neg_z_r, 0x30, "engine -Z must carry bethesda +Y north");
        assert_eq!(pos_z_r, 0x40, "engine +Z must carry bethesda -Y south");
    }

    #[test]
    fn from_skyrim_zup_preserves_lateral_x() {
        let yup = DalcCubeYup::from_skyrim_zup(&distinctive_cube());
        let pos_x_r = (yup.pos_x[0] * 255.0).round() as u8;
        let neg_x_r = (yup.neg_x[0] * 255.0).round() as u8;
        assert_eq!(pos_x_r, 0x10, "X axis is identical across the swap");
        assert_eq!(neg_x_r, 0x20);
    }

    #[test]
    fn from_skyrim_zup_carries_specular_and_fresnel() {
        let yup = DalcCubeYup::from_skyrim_zup(&distinctive_cube());
        let spec_r = (yup.specular[0] * 255.0).round() as u8;
        assert_eq!(spec_r, 0x70);
        assert!((yup.fresnel_power - 1.0).abs() < 1e-6);
    }

    #[test]
    fn lerp_at_half_returns_midpoint_per_axis() {
        let a = DalcCubeYup {
            pos_x: [0.0, 0.0, 0.0],
            neg_x: [1.0, 1.0, 1.0],
            pos_y: [0.0, 0.5, 1.0],
            neg_y: [0.0; 3],
            pos_z: [0.0; 3],
            neg_z: [0.0; 3],
            specular: [0.0, 0.0, 0.0],
            fresnel_power: 1.0,
        };
        let b = DalcCubeYup {
            pos_x: [1.0, 1.0, 1.0],
            neg_x: [0.0, 0.0, 0.0],
            pos_y: [1.0, 0.5, 0.0],
            neg_y: [1.0; 3],
            pos_z: [1.0; 3],
            neg_z: [1.0; 3],
            specular: [1.0, 1.0, 1.0],
            fresnel_power: 3.0,
        };
        let m = DalcCubeYup::lerp(&a, &b, 0.5);
        assert_eq!(m.pos_x, [0.5; 3]);
        assert_eq!(m.neg_x, [0.5; 3]);
        assert_eq!(m.pos_y, [0.5, 0.5, 0.5]);
        assert_eq!(m.neg_y, [0.5; 3]);
        assert_eq!(m.specular, [0.5; 3]);
        assert!((m.fresnel_power - 2.0).abs() < 1e-6);
    }

    #[test]
    fn lerp_endpoints_return_inputs_exactly() {
        let a = DalcCubeYup {
            pos_y: [0.1, 0.2, 0.3],
            fresnel_power: 1.5,
            ..Default::default()
        };
        let b = DalcCubeYup {
            pos_y: [0.7, 0.8, 0.9],
            fresnel_power: 2.5,
            ..Default::default()
        };
        let at_zero = DalcCubeYup::lerp(&a, &b, 0.0);
        assert_eq!(at_zero.pos_y, a.pos_y);
        assert!((at_zero.fresnel_power - a.fresnel_power).abs() < 1e-6);
        let at_one = DalcCubeYup::lerp(&a, &b, 1.0);
        assert_eq!(at_one.pos_y, b.pos_y);
        assert!((at_one.fresnel_power - b.fresnel_power).abs() < 1e-6);
    }
}

/// 6-axis directional ambient cube interpolated for the current TOD,
/// stored in **engine Y-up** coordinates so the renderer can sample
/// along a fragment normal without coordinate conversion.
///
/// Sourced from Skyrim WTHR `DALC` sub-records (4 entries per record,
/// one per TOD slot — sunrise / day / sunset / night). The original
/// authoring is Bethesda Z-up; the converter
/// [`DalcCubeYup::from_skyrim_zup`] applies the same `(x, y, z) →
/// (x, z, -y)` axis swap used by every other importer
/// (`crates/nif/src/import/coord.rs:18`).
///
/// Engine sampling semantics:
/// - `pos_y` = sky-fill (was Bethesda +Z, the "up" axis)
/// - `neg_y` = ground-bounce / cavity-fill (was Bethesda -Z, "down")
/// - `pos_x` / `neg_x` = lateral east / west
/// - `pos_z` / `neg_z` = lateral south / north (Bethesda's ±Y after
///   the swap collapses to engine ∓Z)
///
/// Future GPU consumer pushes this through the per-frame UBO; the
/// shader samples with weights `max(N, 0)` / `max(-N, 0)`. See #993
/// for the renderer-side wiring.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct DalcCubeYup {
    pub(crate) pos_x: [f32; 3],
    pub(crate) neg_x: [f32; 3],
    /// Engine +Y (Bethesda +Z) — sky fill.
    pub(crate) pos_y: [f32; 3],
    /// Engine -Y (Bethesda -Z) — ground-bounce / cavity fill.
    pub(crate) neg_y: [f32; 3],
    pub(crate) pos_z: [f32; 3],
    pub(crate) neg_z: [f32; 3],
    /// DALC specular tint (raw monitor-space RGB).
    pub(crate) specular: [f32; 3],
    /// DALC fresnel power tail — vanilla Skyrim ships `1.0`.
    pub(crate) fresnel_power: f32,
}

impl DalcCubeYup {
    /// Convert a Bethesda-Z-up `SkyrimAmbientCube` into engine Y-up
    /// axes. The byte→field mapping in the WTHR parser is literal
    /// (no axis swap), so we apply the swap here once per TOD slot,
    /// not per fragment.
    ///
    /// Mapping:
    /// - Bethesda +Z (sky) → engine +Y
    /// - Bethesda -Z (ground) → engine -Y
    /// - Bethesda +Y (north) → engine -Z
    /// - Bethesda -Y (south) → engine +Z
    /// - Bethesda ±X unchanged
    pub(crate) fn from_skyrim_zup(
        cube: &byroredux_plugin::esm::records::weather::SkyrimAmbientCube,
    ) -> Self {
        Self {
            pos_x: cube.pos_x.to_rgb_f32(),
            neg_x: cube.neg_x.to_rgb_f32(),
            // Z-up "up" → Y-up "up".
            pos_y: cube.pos_z.to_rgb_f32(),
            // Z-up "down" → Y-up "down".
            neg_y: cube.neg_z.to_rgb_f32(),
            // Z-up "north" (+Y) → Y-up "south" (-Z), so engine `neg_z`
            // sees Bethesda's "north" fill.
            neg_z: cube.pos_y.to_rgb_f32(),
            // Z-up "south" (-Y) → Y-up "north" (+Z).
            pos_z: cube.neg_y.to_rgb_f32(),
            specular: cube.specular.to_rgb_f32(),
            fresnel_power: cube.fresnel_power,
        }
    }

    /// Per-component lerp between two cubes. Used by `weather_system`
    /// to interpolate between adjacent TOD slot pairs (sunrise→day,
    /// day→sunset, …).
    pub(crate) fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]
        }
        Self {
            pos_x: lerp3(a.pos_x, b.pos_x, t),
            neg_x: lerp3(a.neg_x, b.neg_x, t),
            pos_y: lerp3(a.pos_y, b.pos_y, t),
            neg_y: lerp3(a.neg_y, b.neg_y, t),
            pos_z: lerp3(a.pos_z, b.pos_z, t),
            neg_z: lerp3(a.neg_z, b.neg_z, t),
            specular: lerp3(a.specular, b.specular, t),
            fresnel_power: a.fresnel_power + (b.fresnel_power - a.fresnel_power) * t,
        }
    }
}

/// Sky rendering parameters from WTHR records (exterior cells).
/// Stored as an ECS resource so the render loop can read it per-frame.
pub(crate) struct SkyParamsRes {
    pub(crate) zenith_color: [f32; 3],
    pub(crate) horizon_color: [f32; 3],
    /// Below-horizon ground / lower-hemisphere tint from WTHR's
    /// `SKY_LOWER` group (real Sky-Lower at NAM0 slot 7 per nif.xml,
    /// post-#729). Per-frame `weather_system` interpolates the
    /// authored TOD slots; the renderer's `compute_sky` branches on
    /// negative elevation and uses this colour instead of the pre-#541
    /// `horizon * 0.3` fake.
    pub(crate) lower_color: [f32; 3],
    pub(crate) sun_direction: [f32; 3],
    pub(crate) sun_color: [f32; 3],
    pub(crate) sun_size: f32,
    pub(crate) sun_intensity: f32,
    /// Tangent-plane half-radius of the directional-light disk in
    /// radians. Drives PCSS-lite shadow jitter in triangle.frag (#1023
    /// / REN-D20-NEW-01). Default 0.020 (~1.15°) matches the pre-fix
    /// hardcoded shader const; future per-cell / per-TOD tuning will
    /// drive this from CLMT / weather metadata.
    pub(crate) sun_angular_radius: f32,
    pub(crate) is_exterior: bool,
    /// Cloud layer 0 UV tile scale. `0.0` disables clouds (shader skips the sample).
    pub(crate) cloud_tile_scale: f32,
    /// Bindless texture handle for cloud_textures[0]. Only meaningful when
    /// `cloud_tile_scale > 0.0`.
    pub(crate) cloud_texture_index: u32,
    /// Bindless texture handle for the CLMT FNAM sun sprite. `0` = use
    /// the composite shader's procedural sun disc (pre-#478 behaviour).
    /// Populated at cell load when the worldspace has a CLMT with a
    /// resolvable FNAM path. See #478.
    pub(crate) sun_texture_index: u32,
    /// Cloud layer 1 UV tile scale. `0.0` disables the layer (shader
    /// branch-skips the sample). Set to `0.0` when no CNAM texture
    /// is available for the current weather.
    pub(crate) cloud_tile_scale_1: f32,
    /// Bindless texture handle for cloud_textures[1] (WTHR CNAM).
    /// Only meaningful when `cloud_tile_scale_1 > 0.0`.
    pub(crate) cloud_texture_index_1: u32,
    /// Cloud layer 2 UV tile scale. `0.0` disables the layer.
    /// Set to `0.0` when no ANAM texture is available.
    pub(crate) cloud_tile_scale_2: f32,
    /// Bindless texture handle for cloud_textures[2] (WTHR ANAM).
    pub(crate) cloud_texture_index_2: u32,
    /// Cloud layer 3 UV tile scale. `0.0` disables the layer.
    /// Set to `0.0` when no BNAM texture is available.
    pub(crate) cloud_tile_scale_3: f32,
    /// Bindless texture handle for cloud_textures[3] (WTHR BNAM).
    pub(crate) cloud_texture_index_3: u32,
    /// Current TOD-interpolated Skyrim DALC ambient cube in engine
    /// Y-up coordinates. `Some` only when a Skyrim WTHR record drove
    /// the cell load (FNV/FO3/Oblivion stay `None`). The renderer's
    /// future GPU consumer (#993) uploads this through the per-frame
    /// UBO and replaces the temporary `AMBIENT_AO_FLOOR` constant in
    /// `triangle.frag` with a normal-driven cube sample.
    pub(crate) current_dalc_cube: Option<DalcCubeYup>,
}
impl Resource for SkyParamsRes {}

/// Continuous-simulation cloud scroll accumulators — survive cell
/// transitions because the player exiting an exterior cell to an
/// interior shouldn't snap the cloud frame back to origin on
/// re-entry. Mirrors the `GameTimeRes` survives-transitions pattern.
///
/// Pre-#803 the four scroll fields lived on `SkyParamsRes`, which
/// `cell_loader::unload_cell` removes on every cell unload; the next
/// `apply_worldspace_weather` rebuilt the resource with `[0, 0]`
/// scroll, producing a visible cloud snap-back on every exterior
/// re-entry (~0.5 UV per 30 s of interior time, hard-cap at 1.0 via
/// the `weather_system` `rem_euclid(1.0)` wrap). Lifting the
/// accumulator into its own resource means `unload_cell` leaves it
/// alone, the renderer reads the live values per-frame, and
/// `weather_system` advances them across cell boundaries.
#[derive(Debug, Default)]
pub(crate) struct CloudSimState {
    /// Cloud layer 0 scroll offset (matches the scroll vector that
    /// formerly lived on `SkyParamsRes.cloud_scroll`).
    pub(crate) cloud_scroll: [f32; 2],
    /// Cloud layer 1 scroll offset (WTHR CNAM).
    pub(crate) cloud_scroll_1: [f32; 2],
    /// Cloud layer 2 scroll offset (WTHR ANAM).
    pub(crate) cloud_scroll_2: [f32; 2],
    /// Cloud layer 3 scroll offset (WTHR BNAM).
    pub(crate) cloud_scroll_3: [f32; 2],
}
impl Resource for CloudSimState {}

impl SkyParamsRes {
    /// Bindless texture handles owned by this resource.
    ///
    /// Acquired in `scene/world_setup.rs` via `texture_registry.load_dds`
    /// (sun) and `acquire_by_path` (cloud layers); each call bumps the
    /// registry refcount once. Resource is worldspace-scoped (#1199) —
    /// the per-cell drop loop was removed from `unload_cell`. The
    /// matching release will live in a future worldspace-transition
    /// hook (door-walking interior↔exterior). Update this list whenever
    /// a new bindless slot is added to the struct.
    #[allow(dead_code)] // future worldspace-transition release hook (#1199)
    pub(crate) fn texture_indices(&self) -> [u32; 5] {
        [
            self.cloud_texture_index,
            self.cloud_texture_index_1,
            self.cloud_texture_index_2,
            self.cloud_texture_index_3,
            self.sun_texture_index,
        ]
    }
}

/// Game time resource — tracks current hour of day (0.0–24.0).
/// Advances each frame based on real elapsed time × time scale.
pub(crate) struct GameTimeRes {
    /// Current game hour (0.0 = midnight, 6.0 = 6am, 12.0 = noon, etc.)
    pub(crate) hour: f32,
    /// Game-time multiplier: how many game-hours per real-second.
    /// Default 1.0 = 1 game-hour per real-minute (Bethesda default ~30:1).
    pub(crate) time_scale: f32,
}
impl Resource for GameTimeRes {}

impl Default for GameTimeRes {
    fn default() -> Self {
        Self {
            hour: 10.0,       // late morning
            time_scale: 30.0, // 30× = ~2 min per game hour (Bethesda default)
        }
    }
}

/// Full WTHR NAM0 sky color data stored for per-frame time-of-day interpolation.
/// Inserted alongside SkyParamsRes when loading an exterior cell with weather.
pub(crate) struct WeatherDataRes {
    /// 10 color groups × 6 time-of-day slots, raw monitor-space f32 per 0e8efc6.
    /// Indexed by `weather::SKY_*` and `weather::TOD_*` constants.
    pub(crate) sky_colors: [[[f32; 3]; 6]; 10],
    /// Fog distances: [day_near, day_far, night_near, night_far].
    pub(crate) fog: [f32; 4],
    /// Per-climate sunrise/sunset hour breakpoints — `weather_system`
    /// uses these to drive the TOD slot interpolator so Capital
    /// Wasteland and Mojave run on their own schedules (FO3 sunrise
    /// is ~0.3 hr earlier than FNV). Sourced from CLMT TNAM bytes
    /// (10-minute units converted to floating hours: `hour = byte / 6`).
    /// See #463.
    ///
    /// `[sunrise_begin, sunrise_end, sunset_begin, sunset_end]` in hours.
    /// Defaults (6.0, 10.0, 18.0, 22.0) match the pre-#463 hardcoded
    /// values so synthetic test cells and non-climate content keep
    /// their old behaviour.
    pub(crate) tod_hours: [f32; 4],
    /// Skyrim DALC 6-axis ambient cube, four entries
    /// (sunrise / day / sunset / night) already converted to engine
    /// Y-up. `None` on FNV / FO3 / Oblivion (different ambient model).
    /// `weather_system` interpolates between the (slot_a, slot_b)
    /// DALC pair the TOD picker chose for `sky_colors`; TOD slots 4/5
    /// (high_noon / midnight) fold into day / night per the WTHR
    /// parser's padding convention. See #993.
    pub(crate) skyrim_dalc_per_tod: Option<[DalcCubeYup; 4]>,
    /// WTHR DATA `wind_speed` byte (#1033 / REN-D15-NEW-12). Drives
    /// the cloud-layer scroll rate in `weather_system` so calm vs
    /// storm weather animates at different rates. Pre-#1033 the byte
    /// was parsed onto `WeatherRecord` but never projected into the
    /// runtime resource; the cloud animation used a hardcoded
    /// `0.018 UV/sec` literal regardless of weather. `0` on the
    /// synthetic-fallback path (no WTHR record loaded) — produces a
    /// static cloud layer, which is the safe default.
    pub(crate) wind_speed: u8,
}
impl Resource for WeatherDataRes {}

/// In-flight cross-fade between two `WeatherDataRes` snapshots (M33.1).
///
/// When a cell load encounters a different weather while one is already
/// active, `scene.rs` keeps the current `WeatherDataRes` in place and
/// inserts this resource carrying the new target plus the WTHR TNAM
/// transition duration. `weather_system` advances `elapsed_secs` each
/// frame, blends the post-TOD-sample colours by `t = elapsed/duration`,
/// and on completion swaps the live `WeatherDataRes` to `target` and
/// removes the transition resource.
///
/// Interpolation happens after each side runs its own TOD-slot pick so
/// the transition stays correct across the midnight wrap (each weather
/// can be on a different slot).
pub(crate) struct WeatherTransitionRes {
    pub(crate) target: WeatherDataRes,
    pub(crate) elapsed_secs: f32,
    pub(crate) duration_secs: f32,
    /// `true` once `weather_system` has promoted `target` into the
    /// live `WeatherDataRes`. Subsequent frames skip the timer
    /// advance + blend path, so the resource stays resident as a
    /// dormant idempotent record of "the last cross-fade landed
    /// successfully" without further state mutation.
    ///
    /// Pre-fix the dormant state was encoded as
    /// `duration_secs = f32::INFINITY`, which (a) used float
    /// arithmetic as a state machine — the `elapsed/duration`
    /// computation produced 0 by chance — and (b) let
    /// `elapsed_secs += dt` accumulate every frame forever until it
    /// saturated to INFINITY itself and made the ratio NaN. The
    /// explicit bool drops both hazards. See REN-D15-NEW-07 (audit
    /// 2026-05-09).
    pub(crate) done: bool,
}
impl Resource for WeatherTransitionRes {}

/// Cached name→entity mapping for the animation system.
///
/// Rebuilt only when the count of `Name` components changes. Previously
/// the generation tracked `world.next_entity_id()`, which forced a full
/// rebuild on every entity spawn regardless of whether the spawn
/// involved a `Name` — a 3000-entity cell load with only 500 named
/// entities still triggered one rebuild on the next frame. Using the
/// `Name` storage size as the generation means only spawns/despawns
/// that actually touch `Name` invalidate the cache. See #249.
///
/// Edge case: in-place `Name` replacement (re-inserting `Name` on an
/// existing entity without removing it first) does not change the
/// count and therefore does not invalidate the index. No code in the
/// engine currently renames entities after spawn, so this is not a
/// concern today — add an explicit `invalidate()` call if that
/// changes.
pub(crate) struct NameIndex {
    pub(crate) map: HashMap<FixedString, EntityId>,
    /// Count of `Name` components seen at the last rebuild. `usize::MAX`
    /// on a fresh index so the first comparison always rebuilds.
    pub(crate) generation: usize,
}
impl Resource for NameIndex {}

/// Persisted subtree name maps for animation — maps root entity →
/// (bone name → entity) so the BFS walk isn't repeated every frame.
/// Invalidated alongside `NameIndex` when the Name component count changes. #278.
pub(crate) struct SubtreeCache {
    pub(crate) map: HashMap<EntityId, HashMap<FixedString, EntityId>>,
    /// Name component count at last rebuild — same invalidation signal as NameIndex.
    pub(crate) generation: usize,
}
impl Resource for SubtreeCache {}
impl SubtreeCache {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
            generation: usize::MAX,
        }
    }
}

/// Inverted index of `CellRoot → owned entities`, populated by
/// `cell_loader::stamp_cell_root` and drained by `unload_cell`. Pre-#791
/// the unload path iterated the entire `CellRoot` SparseSet to filter
/// down to victims of a single cell, scanning ~13.5k rows on a
/// radius-3 streaming grid (49 cells × ~1.5k entities) to find the
/// ~1.5k that belong to the unloading cell. With this index the lookup
/// is `HashMap::remove`, independent of the number of resident cells.
///
/// Memory cost is ~8 B per cell-owned entity (one `EntityId` per slot
/// in the inner `Vec`), dwarfed by the entity's component data.
pub(crate) struct CellRootIndex {
    pub(crate) map: HashMap<EntityId, Vec<EntityId>>,
}
impl Resource for CellRootIndex {}
impl CellRootIndex {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
}

impl NameIndex {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
            generation: usize::MAX, // Force rebuild on first use.
        }
    }
}

/// Tracks keyboard and mouse input state for the fly camera.
pub(crate) struct InputState {
    pub(crate) keys_held: HashSet<KeyCode>,
    /// Yaw (horizontal) and pitch (vertical) in radians.
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) mouse_captured: bool,
    pub(crate) move_speed: f32,
    pub(crate) look_sensitivity: f32,
}

impl Resource for InputState {}

impl Default for InputState {
    fn default() -> Self {
        Self {
            keys_held: HashSet::new(),
            yaw: 0.0,
            pitch: 0.0,
            mouse_captured: false,
            move_speed: 200.0, // Bethesda units per second
            look_sensitivity: 0.002,
        }
    }
}

// ── M44 Phase 3.5 — footstep gameplay loop ──────────────────────────

/// Marker + accumulator on the entity whose horizontal movement
/// should produce footstep sounds (today the fly-camera entity; an
/// `M28.5` character controller will own this in future).
///
/// `last_position` and `accumulated_stride` are mutated each frame
/// by `footstep_system`; `stride_threshold` is read-only configuration
/// — a stride distance that triggers one footstep. Defaults to 1.5
/// game-units (~1.5m at FNV scale; reasonable walking cadence).
#[derive(Debug, Clone, Copy)]
pub(crate) struct FootstepEmitter {
    /// World position last frame, in renderer Y-up game units. Set
    /// from the entity's `GlobalTransform.translation` on first tick;
    /// updated each frame.
    pub(crate) last_position: Vec3,
    /// Horizontal distance walked since the last footstep fire,
    /// game-units. XZ plane only — vertical motion (jumping, falling)
    /// doesn't count toward stride.
    pub(crate) accumulated_stride: f32,
    /// Stride distance that triggers a footstep dispatch.
    pub(crate) stride_threshold: f32,
    /// Whether `last_position` has been initialised. False on first
    /// tick so the system seeds it without computing a bogus delta
    /// against the default zero pose.
    pub(crate) initialised: bool,
}

impl Component for FootstepEmitter {
    type Storage = SparseSetStorage<Self>;
}

impl FootstepEmitter {
    pub(crate) fn new() -> Self {
        Self {
            last_position: Vec3::ZERO,
            accumulated_stride: 0.0,
            stride_threshold: 1.5,
            initialised: false,
        }
    }
}

/// Resource — engine-wide footstep configuration. Today carries one
/// hardcoded sound (loaded at startup from a vanilla BSA when
/// available). Phase 3.5b replaces the single sound with a per-
/// material lookup (FOOT records).
pub(crate) struct FootstepConfig {
    /// Default footstep sound. `None` when the BSA-load failed
    /// (no archive on disk, decode error, audio inactive).
    pub(crate) default_sound: Option<Arc<Sound>>,
    /// Volume multiplier applied to every footstep play. 0.6 keeps
    /// footsteps mixed below dialogue / weapons / music; tweak per
    /// gameplay feel.
    pub(crate) volume: f32,
}

impl Resource for FootstepConfig {}

impl Default for FootstepConfig {
    fn default() -> Self {
        Self {
            default_sound: None,
            volume: 0.6,
        }
    }
}

/// Per-frame scratch buffer for `footstep_system`'s two-phase pattern
/// (collect trigger positions while walking emitters, then drain to
/// `AudioWorld::play_oneshot`). Pre-#932 the system allocated a fresh
/// `Vec<Vec3>` every frame; with this resource the same backing buffer
/// is `clear()`-ed and refilled, so capacity persists across frames.
///
/// Sized at 32 — typical loaded cell has 5–10 walking NPCs, peak
/// burst ~50 in dense exteriors. 32 covers the common case without
/// re-growing; the Vec doubles past that anyway if a giant crowd ever
/// triggers all at once.
pub(crate) struct FootstepScratch {
    pub(crate) triggers: Vec<Vec3>,
}

impl Resource for FootstepScratch {}

impl Default for FootstepScratch {
    fn default() -> Self {
        Self {
            triggers: Vec::with_capacity(32),
        }
    }
}

/// REND-#1451 — live tuning knobs for the point/spot light attenuation
/// model, pushed into the renderer (`VulkanContext::light_atten_knee` /
/// `light_atten_legacy`) each frame so the REND-#1451 controlled bench
/// can sweep the attenuation without a rebuild. Mutated by the
/// `light.atten` console command; read in the draw path just before
/// `draw_frame`.
///
/// `knee_frac` is the authored radius as a fraction of the cull radius
/// (`= 1 / LIGHT_RANGE_EXTENSION` geometry → default `0.5` for the
/// current `2.0` extension). Lower ⇒ light is dimmer at the authored
/// radius. `legacy` reverts the shader to the pre-fix window-only
/// formula (75% at the authored radius — the bright near-zone ring) for
/// A/B comparison.
pub(crate) struct LightTuning {
    pub(crate) knee_frac: f32,
    pub(crate) legacy: bool,
}

impl Resource for LightTuning {}

impl Default for LightTuning {
    fn default() -> Self {
        Self {
            knee_frac: 0.5,
            legacy: false,
        }
    }
}

/// M42 — furniture seats currently occupied (or claimed this frame) by a
/// sandboxing actor, keyed by `(furniture entity, sit-marker index)` so a
/// single multi-seat furniture (bar counter, bench, table with several
/// chair markers — one FURN, many `BSFurnitureMarker` positions) can seat
/// one actor *per marker* rather than one actor for the whole piece (M42.2
/// seat-polish; before this the key was the furniture entity alone and a
/// six-stool counter seated exactly one NPC). `sandbox_seat_system` checks
/// this before assigning a seat so two NPCs never share one marker, and
/// inserts on assignment. Cleared on cell load (entity ids reset on unload,
/// so stale reservations would be meaningless). The companion
/// [`Seated`](byroredux_core::ecs::components::Seated) component on the
/// actor is the per-actor one-shot guard.
#[derive(Default)]
pub(crate) struct SeatReservations(pub(crate) HashSet<(EntityId, u32)>);
impl Resource for SeatReservations {}

/// M42.1 — the per-cell sit-**enter** clip `(handle, hold_time)`
/// (`meshes\characters\_male\idleanims\chairskirt_leftenter.kf`), resolved once
/// at cell load where the archive provider is available and read by
/// `sandbox_seat_system` (which has no provider). `hold_time` is the clip
/// duration; the seat system parks the player at `local_time = hold_time` with
/// `playing = false` to hold the final seated frame. `None` for Skyrim+/Havok-
/// animation games or when the clip isn't archived — those actors keep their
/// idle and are not seated.
#[derive(Default)]
pub(crate) struct SandboxSitClip(pub(crate) Option<(u32, f32)>);
impl Resource for SandboxSitClip {}

#[cfg(test)]
mod fx_mesh_classification_tests {
    //! PERF-D3-NEW-02 / #1136 — pin the 6-needle FX-decoration set so a
    //! future refactor that adds, removes, or case-changes one needle can't
    //! silently regress the per-frame skip path.
    use super::texture_path_is_fx_mesh;

    #[test]
    fn matches_each_needle() {
        // Mixed-case + both slash conventions to verify case-insensitive +
        // path-style-insensitive matching.
        for path in [
            "meshes\\effects\\fx\\smoke01.nif",
            "Meshes/Effects/Fx/Smoke01.nif",
            "FxSoftGlow.nif",
            "fxpartglow_red.nif",
            "fxparttiny.nif",
            "fxlightrays\\rays01.nif",
        ] {
            assert!(
                texture_path_is_fx_mesh(path),
                "expected FX classification on {path}",
            );
        }
    }

    #[test]
    fn rejects_non_fx_paths() {
        for path in [
            "meshes\\architecture\\house01.nif",
            "meshes\\clutter\\ingredients\\sweetroll01.nif",
            "fxbutreal.nif",  // not in the needle set
            "effectsbox.nif", // missing the trailing \\ or /
            "",
        ] {
            assert!(
                !texture_path_is_fx_mesh(path),
                "expected NO FX classification on {path}",
            );
        }
    }
}

#[cfg(test)]
mod decal_blend_classification_tests {
    use super::decal_uses_implicit_alpha_blend;

    #[test]
    fn low_threshold_fo4_decal_uses_texture_alpha_coverage() {
        assert!(decal_uses_implicit_alpha_blend(
            true,
            false,
            true,
            6.0 / 255.0
        ));
        assert!(decal_uses_implicit_alpha_blend(
            true,
            false,
            true,
            44.0 / 255.0
        ));
    }

    #[test]
    fn high_threshold_poster_remains_an_opaque_cutout() {
        assert!(!decal_uses_implicit_alpha_blend(
            true,
            false,
            true,
            128.0 / 255.0,
        ));
    }

    #[test]
    fn non_decal_or_explicit_blend_does_not_use_compatibility_path() {
        assert!(!decal_uses_implicit_alpha_blend(
            false,
            false,
            true,
            6.0 / 255.0
        ));
        assert!(!decal_uses_implicit_alpha_blend(
            true,
            true,
            true,
            6.0 / 255.0
        ));
        assert!(!decal_uses_implicit_alpha_blend(true, false, false, 0.0));
    }
}
