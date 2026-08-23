//! Application-specific marker components and resources.

mod game_time;

pub(crate) use game_time::GameTimeRes;

use byroredux_audio::Sound;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, Resource, SparseSetStorage};
use byroredux_core::math::Vec3;
use byroredux_core::string::FixedString;
use rustc_hash::FxHashMap;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use winit::event::MouseButton;
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
/// today; the gameplay interaction system now performs the same lookup
/// from an E-key camera-forward activation.
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

/// REFR placement carries an XLOC lock — this entity (door or
/// container) is locked and must not be treated as activatable until
/// the deferred key-check / lockpicking policy exists.
///
/// Captured from `PlacedRef.lock` at spawn time (#3098). Deliberately a
/// component of its own rather than a field bolted onto `DoorTeleport`:
/// containers carry `XLOC` too and have no teleport data at all, so a
/// shared component is the only shape that covers both. The interaction
/// system's `activation_is_blocked` gate is the only consumer today —
/// key checks, lock-level-gated lockpicking, and container activation
/// itself are all deferred follow-up work (see the issue for scope).
///
/// Sparse storage — locked REFRs are a small minority of placements.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Locked {
    /// Lock difficulty as authored (see `byroredux_plugin::esm::cell::LockData`
    /// for the wire-level meaning). Not yet consumed by any policy —
    /// present so the eventual lockpicking system doesn't need a second
    /// data-plumbing pass.
    #[allow(dead_code)]
    pub(crate) lock_level: u8,
    /// FormID of the key that opens this lock, if any. Not yet consumed
    /// — key-carry / key-check gameplay is deferred (see struct doc).
    #[allow(dead_code)]
    pub(crate) key_form_id: Option<u32>,
}
impl Component for Locked {
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
///
/// It does have a **telemetry** consumer, which is not the same thing: EX-10/11
/// (#2371)'s `lod.coverage` audit queries every resident instance of this marker
/// and checks its cell against the resident object-LOD quads, live, every
/// reconcile (`streaming_helpers::resident_vwd_refr_cells` /
/// `LodCoverageStats::vwd_full_model_overlaps`). That always reads 0 today —
/// it is proving the ring-separation argument above holds in a real running
/// session, not exercising a cull — but it is the regression gate that would
/// catch the day someone enables the active cull this doc still says not to.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VisibleWhenDistant;
impl Component for VisibleWhenDistant {
    type Storage = SparseSetStorage<Self>;
}

/// Returns `true` if `texture_path` matches any of the 6 legacy
/// "FX decoration" needles the renderer drops.
///
/// Authored effect-shader and fire-refraction materials are real renderer
/// inputs, even though their texture names deliberately live under
/// `effects\fx*`; never let the path heuristic override those stronger
/// source-format semantics. Pulled out as a shared helper so the spawn-time
/// tagger in `cell_loader::spawn` + `scene::nif_loader` and any future caller
/// stays in lockstep. PERF-D3-NEW-02 / #1136.
pub(crate) fn texture_path_is_fx_mesh(texture_path: &str, material_kind: u32) -> bool {
    if material_kind == byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER
        || material_kind == byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION
    {
        return false;
    }
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

/// Bindless texture handle for the specialized water normal-map path.
///
/// Static NIF materials use [`MaterialTextureHandles`], including the
/// normal-alpha metadata needed by the legacy gloss convention.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NormalMapHandle(pub(crate) u32);
impl Component for NormalMapHandle {
    type Storage = SparseSetStorage<Self>;
}

/// Bindless handles acquired for the three authored WATR noise layers.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WaterNoiseMapHandles(pub(crate) [u32; 3]);
impl Component for WaterNoiseMapHandles {
    type Storage = SparseSetStorage<Self>;
}

/// Authored worldspace LOD-water provenance exposed by `water.dump`.
/// This is diagnostic state attached to the render-only LOD entity; it is not
/// a second gameplay water representation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WaterLodInfo {
    pub(crate) height: f32,
    pub(crate) water_form: Option<u32>,
}
impl Component for WaterLodInfo {
    type Storage = SparseSetStorage<Self>;
}

/// Resolved bindless indices for the common NIF material texture contract.
///
/// Every static material carries the same semantic role set regardless of
/// whether its source was legacy NiTexturingProperty, BSShaderTextureSet,
/// BGSM/BGEM, or BSEffectShaderProperty. Index 0 means the role is absent.
/// Water keeps its specialized [`NormalMapHandle`] because it is emitted by a
/// separate renderer path.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MaterialTextureHandles {
    pub(crate) textures: byroredux_nif::import::MaterialTextureSet<u32>,
    /// Whether the normal DDS carries authored alpha (legacy gloss channel).
    pub(crate) normal_has_alpha: bool,
    /// POM height scale (default 0.04). See #453.
    pub(crate) parallax_height_scale: f32,
    /// POM ray-march sample budget (default 4.0). See #453.
    pub(crate) parallax_max_passes: f32,
}
impl Component for MaterialTextureHandles {
    type Storage = SparseSetStorage<Self>;
}

/// Provenance retained alongside each resolved material texture role.
///
/// [`MaterialTextureHandles`] is the compact render-facing contract; this
/// sibling is the correctness-oracle contract used by `mat.dump`. Keeping the
/// two separate avoids putting owned paths on the per-frame hot path while
/// preserving enough information to prove that an XTXR/TXST override landed
/// in the intended semantic role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum MaterialTextureSource {
    /// No path was authored or synthesized for this semantic role.
    #[default]
    Absent,
    /// The effective path arrived with the imported mesh material. This can
    /// be an inline NIF texture set or a path filled by BGSM/BGEM/.mat merge.
    MeshMaterial,
    /// A placement-level XATO/XTNM/XTXR texture-set override won.
    TxstOverride,
    /// The legacy `<base>_n.dds` convention synthesized a normal path.
    DerivedNormal,
    /// A runtime caller replaced an authored path (currently FaceGen tint).
    RuntimeOverride,
}

impl MaterialTextureSource {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::MeshMaterial => "mesh-material",
            Self::TxstOverride => "txst-override",
            Self::DerivedNormal => "derived-normal",
            Self::RuntimeOverride => "runtime-override",
        }
    }
}

/// Cold-path material texture oracle attached at mesh spawn time.
///
/// `paths` and `sources` use the same canonical role order as the NIF import
/// and renderer contracts. `mat.dump` zips them with
/// [`MaterialTextureHandles`] to expose path → provenance → bindless index.
#[derive(Debug, Clone)]
pub(crate) struct MaterialTextureDebugInfo {
    pub(crate) paths: byroredux_nif::import::MaterialTextureSet<Option<String>>,
    pub(crate) sources: byroredux_nif::import::MaterialTextureSet<MaterialTextureSource>,
    pub(crate) clamp_mode: u8,
}

impl Component for MaterialTextureDebugInfo {
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
    /// Engine-native participating medium fitted once from the authored
    /// XCLL ramp. Volumetric rendering consumes this instead of evaluating
    /// `fog_near..fog_far` at runtime.
    pub(crate) fog_medium: crate::fog::FogMedium,
    // ── Extended XCLL fields (FNV 40-byte tail + Skyrim 92-byte tail) ──
    // All of these are now read by the `cell.lighting` console command
    // and/or the lighting plumbing (see #865 for the fog curve), so the
    // former per-field `#[allow(dead_code)]` guards have been removed.
    /// Directional light fade multiplier — bytes 28-31 of the XCLL.
    /// FNV+ XCLL.
    pub(crate) directional_fade: Option<f32>,
    /// Cubic-fog clip distance — bytes 32-35. FNV+ XCLL. Retained with
    /// `fog_power` for diagnostics and explicit legacy compatibility;
    /// the physical volumetric path consumes `fog_medium`.
    pub(crate) fog_clip: Option<f32>,
    /// Cubic-fog falloff exponent — bytes 36-39. FNV+ XCLL.
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
    /// Raw Skyrim+ XCLL per-field LTMP inheritance mask. Values in this
    /// resource are already resolved; the mask is retained as provenance
    /// for `light.dump` and fixture diagnostics.
    pub(crate) inheritance_flags: Option<u32>,
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
            fog_medium: crate::fog::FogMedium::from_legacy_ramp(
                lit.fog_near,
                lit.fog_far,
                lit.fog_max,
            ),
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
            inheritance_flags: lit.inheritance_flags,
        }
    }
}

impl Resource for CellLightingRes {}

/// Resolved ambient-audio directive for the currently resident cell's
/// highest-priority `REGN` `Sound` entry (EX-16 item 1, #2372). CPU-only —
/// carries FormIDs, not decoded audio; `asset_provider::audio`'s
/// `resolve_sound_path`/`sound_archive_path` resolve them to archive
/// paths, and a consumer (item 5's REGN-keyed `AudioEmitter`, not yet
/// built) dispatches actual playback. Mirrors `CellLightingRes`'s shape
/// and lifecycle: computed once at cell-apply time from data already
/// parsed into `EsmIndex`, not recomputed per-frame.
///
/// Deliberately carries only `music`/`incidental` — both are single
/// authored FormIDs (`RDMD`/`RDMO`/`RDSB` and `RDSI`). The RDAT
/// `sounds: Vec<RegionSound>` ambient-loop list is NOT surfaced here:
/// each entry's `chance_raw` selection probability has an unresolved
/// fixed-point scale (see `RegionSound`'s doc in `misc/world.rs`), and
/// picking one without a verified scale would be exactly the kind of
/// guessed threshold this project's no-guessing policy exists to
/// prevent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RegionAmbientRes {
    /// `RDMD` (Oblivion) / `RDMO` (Skyrim) / `RDSB` (FNV) — the winning
    /// entry's background-music/ambient-bed FormID.
    pub music_form: Option<u32>,
    /// `RDSI` — FNV-only incidental sound FormID.
    pub incidental_form: Option<u32>,
}

impl RegionAmbientRes {
    /// Resolve from a resident cell's `regions` (XCLR) FormID list against
    /// the parsed `REGN` map. Empty/no-match input yields `Self::default()`
    /// (both fields `None`) rather than an `Option<Self>` — an unambiguous
    /// "no ambient directive" state a consumer can read unconditionally,
    /// same posture as `CellLightingRes`'s always-present-resource design.
    pub(crate) fn resolve(
        region_form_ids: &[u32],
        regions: &HashMap<u32, byroredux_plugin::esm::records::RegnRecord>,
    ) -> Self {
        use byroredux_plugin::esm::records::{select_active_region_sound, RegionDataPayload};
        let Some(entry) = select_active_region_sound(region_form_ids, regions) else {
            return Self::default();
        };
        match &entry.payload {
            RegionDataPayload::Sound {
                music, incidental, ..
            } => Self {
                music_form: *music,
                incidental_form: *incidental,
            },
            // `select_active_region_sound` only ever returns a `Sound`-kind
            // entry, so this arm is unreachable in practice; kept instead
            // of `unreachable!()` so a future payload-shape change fails
            // soft (no ambient directive) rather than panicking mid-load.
            _ => Self::default(),
        }
    }
}

impl Resource for RegionAmbientRes {}

#[cfg(test)]
mod region_ambient_res_tests {
    use super::*;
    use byroredux_plugin::esm::records::{
        RegionDataEntry, RegionDataKind, RegionDataPayload, RegnRecord,
    };
    use std::collections::HashMap;

    fn regn(entries: Vec<RegionDataEntry>) -> RegnRecord {
        RegnRecord {
            entries,
            ..Default::default()
        }
    }

    fn sound_entry(priority: u8, music: Option<u32>, incidental: Option<u32>) -> RegionDataEntry {
        RegionDataEntry {
            kind: RegionDataKind::Sound,
            flags: 0,
            priority,
            payload: RegionDataPayload::Sound {
                music,
                incidental,
                sounds: Vec::new(),
            },
        }
    }

    #[test]
    fn resolves_music_and_incidental_from_the_winning_entry() {
        let mut regions = HashMap::new();
        regions.insert(
            0x10,
            regn(vec![sound_entry(50, Some(0xAAAA), Some(0xBBBB))]),
        );
        let res = RegionAmbientRes::resolve(&[0x10], &regions);
        assert_eq!(res.music_form, Some(0xAAAA));
        assert_eq!(res.incidental_form, Some(0xBBBB));
    }

    #[test]
    fn no_tagging_regions_yields_default() {
        let regions = HashMap::new();
        assert_eq!(
            RegionAmbientRes::resolve(&[], &regions),
            RegionAmbientRes::default()
        );
    }

    #[test]
    fn a_tagging_region_with_no_sound_entry_yields_default() {
        let mut regions = HashMap::new();
        regions.insert(
            0x10,
            regn(vec![RegionDataEntry {
                kind: RegionDataKind::Weather,
                flags: 0,
                priority: 100,
                payload: RegionDataPayload::Weather(Vec::new()),
            }]),
        );
        assert_eq!(
            RegionAmbientRes::resolve(&[0x10], &regions),
            RegionAmbientRes::default()
        );
    }
}

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
            directional_azimuth: 0.0,
            directional_elevation: 0.0,
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
            inheritance_flags: None,
            starfield: None,
        }
    }

    fn skyrim_xcll_with_full_extension() -> CellLighting {
        CellLighting {
            ambient: [0.05, 0.05, 0.06],
            directional_color: [0.8, 0.85, 1.0],
            directional_azimuth: 0.5,
            directional_elevation: 0.3,
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
            inheritance_flags: Some(0),
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
            directional_azimuth: 0.0,
            directional_elevation: 0.0,
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
            inheritance_flags: None,
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
    fn from_xcll_zup_maps_ordered_faces_to_engine_axes() {
        let faces = [
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [6.0, 0.0, 0.0],
        ];
        let yup = DalcCubeYup::from_xcll_zup(&faces, [0.2, 0.4, 0.6], 1.5);

        assert_eq!(yup.pos_x, faces[0]);
        assert_eq!(yup.neg_x, faces[1]);
        assert_eq!(yup.neg_z, faces[2]);
        assert_eq!(yup.pos_z, faces[3]);
        assert_eq!(yup.pos_y, faces[4]);
        assert_eq!(yup.neg_y, faces[5]);
        assert_eq!(yup.specular, [0.2, 0.4, 0.6]);
        assert_eq!(yup.fresnel_power, 1.5);
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
/// (x, z, -y)` permutation as
/// [`byroredux_core::math::coord::zup_to_yup_pos`] (also wrapped by
/// `crates/nif/src/import/coord.rs:18` for every other importer) — just
/// spread across 6 named cube-face fields instead of one `[f32; 3]`, so
/// it isn't a `zup_to_yup_pos` *call site* and won't turn up by grepping
/// for its callers (#2062).
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
    /// Convert the Skyrim XCLL directional-ambient face array from
    /// Bethesda Z-up into engine Y-up axes.
    ///
    /// XCLL stores faces in `[+X, -X, +Y, -Y, +Z, -Z]` order. This is
    /// the same axis permutation as [`Self::from_skyrim_zup`], but the
    /// source values have already been decoded to linear RGB floats.
    pub(crate) fn from_xcll_zup(
        faces: &[[f32; 3]; 6],
        specular: [f32; 3],
        fresnel_power: f32,
    ) -> Self {
        Self {
            pos_x: faces[0],
            neg_x: faces[1],
            // Z-up "up" → Y-up "up".
            pos_y: faces[4],
            // Z-up "down" → Y-up "down".
            neg_y: faces[5],
            // Z-up "north" (+Y) → Y-up "south" (-Z).
            neg_z: faces[2],
            // Z-up "south" (-Y) → Y-up "north" (+Z).
            pos_z: faces[3],
            specular,
            fresnel_power,
        }
    }

    /// Convert a Bethesda-Z-up `SkyrimAmbientCube` into engine Y-up
    /// axes. The byte→field mapping in the WTHR parser is literal
    /// (no axis swap), so we apply the swap here once per TOD slot,
    /// not per fragment. Same `(x, y, z) → (x, z, -y)` permutation as
    /// [`byroredux_core::math::coord::zup_to_yup_pos`] (see the struct
    /// doc above for why this can't just call it directly).
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

/// Full WTHR NAM0 sky color data stored for per-frame time-of-day interpolation.
/// Inserted alongside SkyParamsRes when loading an exterior cell with weather.
pub(crate) struct WeatherDataRes {
    /// 10 color groups × 6 time-of-day slots, raw monitor-space f32 per 0e8efc6.
    /// Indexed by `weather::SKY_*` and `weather::TOD_*` constants.
    pub(crate) sky_colors: [[[f32; 3]; 6]; 10],
    /// Fog distances: [day_near, day_far, night_near, night_far].
    pub(crate) fog: [f32; 4],
    /// Engine-native day/night media fitted once at WTHR translation.
    /// `weather_system` interpolates these coefficients directly and never
    /// reconstructs a medium from the legacy distance ramps.
    pub(crate) fog_media: [crate::fog::FogMedium; 2],
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
    /// WTHR precipitation intensity: rain is `1`, snow is a subtle wet-surface
    /// contribution, and clear/cloudy/unclassified weather is `0`.
    pub(crate) precipitation: f32,
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

/// FormId (raw, global load-order `u32` space — the same key space
/// `resolve_entity_by_global_form_id` resolves through) → `Entity` index,
/// scoped to the active worldspace's *persistent* CELL rather than every
/// entity in the world. Build/query logic lives in
/// `cell_loader::persistent_ref_index` (mirrors `CellRootIndex`: the
/// resource is a plain data holder here, populated by a free function
/// in the module that owns the concept it indexes).
///
/// # EX-09 (#2370) — why persistent refs only, not a general FormId index
/// `World::find_by_form_id` / `resolve_entity_by_global_form_id` already
/// cover the general case (an O(n) scan over every `FormIdComponent`,
/// "fine for the rare condition-eval path" per the latter's own doc) —
/// reused for this index's rebuild, not duplicated. A cache over *every*
/// `FormIdComponent` entity would need invalidating on every ordinary
/// cell-streaming tick, since ordinary REFRs churn constantly as cells
/// load/unload; a persistent CELL's contents are stable for as long as
/// its root entity is resident, so scoping to it keeps invalidation rare
/// and tied to something that actually changes infrequently: a
/// worldspace crossing (which always allocates a fresh persistent-cell
/// root entity — see `built_for` below).
///
/// This is the foundational identity mechanism EX-14/15's cross-worldspace
/// persistent-ref continuity and EX-16's actor/package migration both
/// need before they can be built — see
/// `docs/engine/exterior-readiness-plan.md`.
///
/// Landed ahead of its consumer, same posture as `groundcover_translate`'s
/// Phase 0 constants: fully exercised by `cell_loader::persistent_ref_index`'s
/// test suite, a *pending* production consumer (EX-14/15, EX-16) rather than
/// unused code — hence the field-level `#[allow(dead_code)]` below.
pub(crate) struct PersistentRefIndex {
    #[allow(dead_code)] // see the struct doc — EX-14/15/EX-16 is the pending consumer
    pub(crate) map: HashMap<u32, EntityId>,
    /// The persistent-cell root entity this index was last built against.
    /// `None` before the first build. A worldspace crossing always
    /// allocates a fresh root entity for the destination's persistent
    /// CELL, so comparing this to the caller's current `persistent_root`
    /// is a precise (not merely probable) invalidation signal — unlike
    /// `NameIndex`/`SubtreeCache`'s component-count heuristic, which is
    /// the right tool when nothing more specific is available but isn't
    /// needed here.
    #[allow(dead_code)] // see the struct doc — EX-14/15/EX-16 is the pending consumer
    pub(crate) built_for: Option<EntityId>,
}
impl Resource for PersistentRefIndex {}
impl PersistentRefIndex {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
            built_for: None,
        }
    }
}

/// FormId → `Entity` index scoped to whichever **ordinary** `CellRoot` a
/// caller names — the sibling [`PersistentRefIndex`] doesn't cover, since
/// that one is deliberately scoped to only the worldspace's persistent
/// CELL (see its own doc for why). Build/query logic lives in
/// `cell_loader::cell_root_ref_index`, which (like
/// `cell_loader::persistent_ref_index`) is a thin wrapper over the shared
/// single-root build/rebuild walk in `cell_loader::form_id_root_index`.
///
/// # Why a separate resource, not a generalized `PersistentRefIndex`
/// Both are single-slot caches (`map` + `built_for: Option<EntityId>`
/// naming the one root they're currently built for) — reusing one slot for
/// both scopes would thrash the moment a caller needs a persistent-CELL
/// lookup and an ordinary-cell-root lookup in the same tick (each build
/// would invalidate the other's cached root). Keeping two independent
/// resource instances, sharing only the build logic underneath, avoids
/// that without adding real complexity — same precedent as
/// `CellRootIndex` and `PersistentRefIndex` already being separate
/// indices over overlapping `CellRoot` data.
///
/// # Why "ordinary," not "every" `CellRoot`
/// An ordinary cell's contents churn constantly as the streaming grid
/// loads/unloads tiles — unlike a persistent CELL, which is stable for as
/// long as its root is resident. A cache proactively maintained for every
/// resident ordinary root would need invalidating on essentially every
/// streaming tick. This index is designed to be built **lazily, for one
/// root at a time, at the moment a caller actually needs to resolve a
/// reference within it** — e.g. a stream-boundary state-continuity
/// snapshot restore resolving a FormID-keyed reference (like
/// `Seated::furniture`) back to a live entity the instant that entity's
/// owning `CellRoot` respawns. See
/// `docs/engine/stream-boundary-state-continuity.md` §3 for the design
/// this index was built to unblock (EX-14/15 item C2's reconcile half,
/// EX-16 item 4's snapshot/restore).
///
/// Landed ahead of its consumer, same posture as `PersistentRefIndex`
/// itself: fully exercised by `cell_loader::cell_root_ref_index`'s test
/// suite, a *pending* production consumer rather than unused code.
pub(crate) struct CellRootRefIndex {
    #[allow(dead_code)] // see the struct doc — pending stream-boundary-state-continuity consumer
    pub(crate) map: HashMap<u32, EntityId>,
    /// The `CellRoot` entity this index was last built against. `None`
    /// before the first build.
    #[allow(dead_code)]
    // see the struct doc — pending stream-boundary-state-continuity consumer
    pub(crate) built_for: Option<EntityId>,
}
impl Resource for CellRootRefIndex {}
impl CellRootRefIndex {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
            built_for: None,
        }
    }
}

/// Tracks keyboard and mouse input state for the fly camera.
pub(crate) const DEFAULT_LOOK_SENSITIVITY: f32 = 0.002;

pub(crate) struct InputState {
    pub(crate) keys_held: HashSet<KeyCode>,
    pub(crate) mouse_buttons_held: HashSet<MouseButton>,
    /// Yaw (horizontal) and pitch (vertical) in radians.
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) mouse_captured: bool,
    pub(crate) move_speed: f32,
    pub(crate) look_sensitivity: f32,
    pub(crate) invert_look_y: bool,
}

impl Resource for InputState {}

impl Default for InputState {
    fn default() -> Self {
        Self {
            keys_held: HashSet::new(),
            mouse_buttons_held: HashSet::new(),
            yaw: 0.0,
            pitch: 0.0,
            mouse_captured: false,
            move_speed: 200.0, // Bethesda units per second
            look_sensitivity: DEFAULT_LOOK_SENSITIVITY,
            invert_look_y: false,
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

/// Resource — one-shot sound used for actor/water-surface interaction.
/// `None` is a valid headless/no-archive state and keeps water gameplay
/// independent from audio-device availability.
pub(crate) struct WaterAudioConfig {
    pub(crate) splash_sound: Option<Arc<Sound>>,
    pub(crate) volume: f32,
}

impl Resource for WaterAudioConfig {}

impl Default for WaterAudioConfig {
    fn default() -> Self {
        Self {
            splash_sound: None,
            volume: 0.7,
        }
    }
}

/// Reusable cadence state for recurring near-surface ripple sounds. Splash
/// edges are never delayed; this cooldown only prevents a stationary actor
/// from producing a one-shot every frame while `RippleEvent` is present.
pub(crate) struct WaterAudioState {
    pub(crate) ripple_cooldowns: FxHashMap<EntityId, f32>,
}

impl Resource for WaterAudioState {}

impl Default for WaterAudioState {
    fn default() -> Self {
        Self {
            ripple_cooldowns: FxHashMap::default(),
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

/// Deferred operator control for named renderer views and the one-record
/// selected visibility-ray probe. Console commands mutate this resource;
/// `render_one_frame` applies requests at the next frame boundary, where it
/// has exclusive access to `VulkanContext`.
pub(crate) struct RenderDebugControl {
    pub(crate) active_mode: byroredux_renderer::RenderDebugMode,
    pub(crate) pending_mode: Option<byroredux_renderer::RenderDebugMode>,
    pub(crate) pending_probe_pixel: Option<[u32; 2]>,
    pub(crate) pending_probe_generation: Option<u32>,
    pub(crate) last_probe: Option<byroredux_renderer::SelectedRayProbeResult>,
    pub(crate) last_error: Option<String>,
}

impl Resource for RenderDebugControl {}

impl Default for RenderDebugControl {
    fn default() -> Self {
        Self {
            active_mode: byroredux_renderer::RenderDebugMode::default(),
            pending_mode: None,
            pending_probe_pixel: None,
            pending_probe_generation: None,
            last_probe: None,
            last_error: None,
        }
    }
}

/// M42 — furniture seats currently occupied (or claimed this frame) by a
/// sandboxing actor. Each `(furniture entity, sit-marker index)` key maps to
/// its claimant so streaming cleanup can release a seat when either side
/// despawns. The per-marker key means a
/// single multi-seat furniture (bar counter, bench, table with several
/// chair markers — one FURN, many `BSFurnitureMarker` positions) can seat
/// one actor *per marker* rather than one actor for the whole piece (M42.2
/// seat-polish; before this the key was the furniture entity alone and a
/// six-stool counter seated exactly one NPC). `sandbox_seat_system` checks
/// this before assigning a seat so two NPCs never share one marker, and
/// inserts on assignment. Pruned on each cell-reference load against both
/// live furniture and the claimant's [`Seated`] component. The companion
/// [`Seated`](byroredux_core::ecs::components::Seated) component on the
/// actor is the per-actor one-shot guard.
#[derive(Default)]
pub(crate) struct SeatReservations(pub(crate) HashMap<(EntityId, u32), EntityId>);
impl Resource for SeatReservations {}

/// M42.9 / #2652 — actor-owned ambient PACK selection state.
///
/// Candidate and winner IDs live in global load-order FormID space, so this
/// component never retains a borrow into the plugin index. The minute marker
/// bounds schedule checks to once per game minute; `None` forces a first-tick
/// confirmation after cell loading has installed scripting resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AmbientPackageRuntime {
    pub(crate) package_candidates: Vec<u32>,
    pub(crate) active_package_form_id: Option<u32>,
    pub(crate) actor_form_id: u32,
    pub(crate) last_evaluated_game_minute: Option<u16>,
}

impl Component for AmbientPackageRuntime {
    type Storage = SparseSetStorage<Self>;
}

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

/// Skeleton subtree owned by an actor that can consume Skyrim behavior-IDLE
/// requests. The request serial makes repeated `PlayIdle` calls observable
/// even when they target the same IDLE FormID.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HavokAnimationTarget {
    pub(crate) skeleton_root: EntityId,
    pub(crate) consumed_idle_serial: u64,
}

impl Component for HavokAnimationTarget {
    type Storage = SparseSetStorage<Self>;
}

/// Process-lifetime IDLE FormID → decoded animation handle mapping.
///
/// Clips live in `AnimationClipRegistry`; this small companion preserves the
/// plugin-level identity used by Papyrus `PlayIdle` calls.
#[derive(Default)]
pub(crate) struct HavokIdleCatalog {
    pub(crate) handles: HashMap<u32, u32>,
}

impl Resource for HavokIdleCatalog {}

/// A resident navmesh tile — CPU-only pathing data, no GPU footprint.
///
/// EX-16 item 2 (#2372): makes `NAVM` records "resident" alongside their
/// owning interior cell / exterior grid tile, so a future pathfinder (item
/// 3, deliberately scoped as its own follow-up issue — "pathfinding needs
/// resident tiles first") has something to query. Today this is pure
/// residency plumbing with zero consumers, same posture as
/// `PersistentRefIndex` landing ahead of its EX-14/15/EX-16 callers.
///
/// Spawned as a plain entity within the same `first_entity..last_entity`
/// range every other cell-owned entity is spawned in
/// (`cell_loader::load::load_cell_with_masters` for interiors,
/// `ExteriorCellApplyJob::begin` for exterior tiles), so it rides the
/// existing `stamp_cell_root_range` → `CellRootIndex` → `unload_cell`
/// teardown chain automatically — no bespoke reclaim path needed the way
/// `LodBlock`/`ObjectLodBlock` need one, because there's no GPU handle
/// here to leak.
pub(crate) struct NavmeshTile(pub(crate) byroredux_plugin::esm::records::NavmRecord);

impl Component for NavmeshTile {
    type Storage = SparseSetStorage<Self>;
}

/// Spawn one [`NavmeshTile`] entity per `navmeshes` entry. Shared by the
/// interior and exterior cell loaders so the spawn shape can't drift
/// between them. Caller is responsible for calling this within the same
/// `first_entity..last_entity` window its `stamp_cell_root_range` call
/// covers — this function only spawns entities, it does not stamp them.
pub(crate) fn spawn_navmesh_tiles(
    world: &mut byroredux_core::ecs::World,
    navmeshes: &[byroredux_plugin::esm::records::NavmRecord],
) {
    for navm in navmeshes {
        let entity = world.spawn();
        world.insert(entity, NavmeshTile(navm.clone()));
    }
}

/// Cached single-tile NAVM path toward `goal`, computed by
/// `navmesh_path::path_from_resident_tiles` and consumed incrementally by
/// a `step_toward`-driven locomotion system (EX-16 item 3 Phase 3,
/// `docs/engine/navmesh-pathfinding.md`) — currently `travel_system`.
///
/// Computed **once per (entity, goal) pair, not every tick** — the design
/// doc's §7 cost posture. A caller recomputes only when its current goal
/// no longer matches [`goal`](Self::goal); until then it consumes
/// [`waypoints`](Self::waypoints) by popping the front entry once the
/// actor arrives within `LOCOMOTION_ARRIVAL_EPSILON` of it.
///
/// Deliberately **not** save-registered: rederived on demand from
/// resident `NavmeshTile` data, same posture as `NavmeshTile` itself — an
/// empty/missing `NavPath` after a load just means "repath on the next
/// tick that needs one," harmless and cheap, never lossy gameplay state.
///
/// An empty `waypoints` with a matching `goal` is itself a meaningful,
/// intentionally-cached result ("already tried, no resident-tile path
/// found for this goal") — see `travel_system`'s Pass 1a for why that
/// negative result is cached too, not just successful paths.
#[derive(Clone)]
pub(crate) struct NavPath {
    /// The destination this path was computed for.
    pub(crate) goal: Vec3,
    /// Remaining stepping points, nearest first, always ending with
    /// [`goal`](Self::goal) itself while non-empty. Empty means "no
    /// resident-tile path was found for this goal" — callers fall back
    /// to walking straight at `goal`, today's pre-pathing behavior.
    pub(crate) waypoints: VecDeque<Vec3>,
}
impl Component for NavPath {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod navmesh_tile_tests {
    use super::*;
    use byroredux_plugin::esm::records::NavmRecord;

    fn navm(form_id: u32) -> NavmRecord {
        NavmRecord {
            form_id,
            ..NavmRecord::default()
        }
    }

    #[test]
    fn spawns_one_entity_per_navmesh() {
        let mut world = byroredux_core::ecs::World::new();
        spawn_navmesh_tiles(&mut world, &[navm(0x100), navm(0x101)]);

        let q = world.query::<NavmeshTile>().expect("NavmeshTile storage");
        let form_ids: std::collections::HashSet<u32> =
            q.iter().map(|(_, tile)| tile.0.form_id).collect();
        assert_eq!(
            form_ids,
            [0x100, 0x101].into_iter().collect(),
            "one resident tile per NavmRecord, identity preserved"
        );
    }

    #[test]
    fn empty_navmesh_list_spawns_nothing() {
        let mut world = byroredux_core::ecs::World::new();
        spawn_navmesh_tiles(&mut world, &[]);
        assert!(
            world
                .query::<NavmeshTile>()
                .is_none_or(|q| q.iter().count() == 0),
            "a cell with no NAVM children must spawn zero tile entities"
        );
    }
}

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
                texture_path_is_fx_mesh(path, 0),
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
                !texture_path_is_fx_mesh(path, 0),
                "expected NO FX classification on {path}",
            );
        }
    }

    #[test]
    fn authored_effect_semantics_override_the_path_heuristic() {
        let path = "effects\\fxfireatlas04.dds";
        assert!(!texture_path_is_fx_mesh(
            path,
            byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER,
        ));
        assert!(!texture_path_is_fx_mesh(
            path,
            byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION,
        ));
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
