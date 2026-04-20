//! Application-specific marker components and resources.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, Resource, SparseSetStorage};
use byroredux_core::string::FixedString;
use std::collections::{HashMap, HashSet};
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

/// Marker component for decal geometry (renders on top of coplanar surfaces).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Decal;
impl Component for Decal {
    type Storage = SparseSetStorage<Self>;
}

/// Bindless texture handle for a normal map (parallels TextureHandle for diffuse).
#[derive(Debug, Clone, Copy)]
pub(crate) struct NormalMapHandle(pub(crate) u32);
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
}
impl Resource for CellLightingRes {}

/// Sky rendering parameters from WTHR records (exterior cells).
/// Stored as an ECS resource so the render loop can read it per-frame.
pub(crate) struct SkyParamsRes {
    pub(crate) zenith_color: [f32; 3],
    pub(crate) horizon_color: [f32; 3],
    pub(crate) sun_direction: [f32; 3],
    pub(crate) sun_color: [f32; 3],
    pub(crate) sun_size: f32,
    pub(crate) sun_intensity: f32,
    pub(crate) is_exterior: bool,
    /// Cloud layer 0 scroll offset accumulated by `weather_system` each frame.
    pub(crate) cloud_scroll: [f32; 2],
    /// Cloud layer 0 UV tile scale. `0.0` disables clouds (shader skips the sample).
    pub(crate) cloud_tile_scale: f32,
    /// Bindless texture handle for cloud_textures[0]. Only meaningful when
    /// `cloud_tile_scale > 0.0`.
    pub(crate) cloud_texture_index: u32,
}
impl Resource for SkyParamsRes {}

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
    /// Cloud layer scroll speeds from WTHR DNAM (0-255, one per layer).
    /// `weather_system` uses index 0 to drive `SkyParamsRes::cloud_scroll`.
    pub(crate) cloud_speeds: [u8; 4],
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
}
impl Resource for WeatherDataRes {}

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
