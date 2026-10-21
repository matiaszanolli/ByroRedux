//! Render and material marker components plus texture-handle components. (#5467 split from `components.rs`)

use byroredux_core::ecs::{Component, Resource, SparseSetStorage};
// #5173 — the per-lane ground-cover arrays below size from the same splat
// constant the terrain tile + shader loops size from, so a lane bump cannot
// leave this component silently 8-wide while the rest of the chain moves.

/// Marker component for entities that should spin in the demo scene.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Spinning;
impl Component for Spinning {
    type Storage = SparseSetStorage<Self>;
}

/// Marker for the root of a Creation-era loading-screen model stage
/// (Skyrim/FO4 LSCR NNAM art). Attached only by the loading-screen
/// backend; the turntable system reads `no_rotation` (the LSCR header's
/// "No Rotation" flag, 0x8000 — FO4+ only, always false on Skyrim, whose
/// LSCR flags have no such bit) and everything else about the stage lives
/// on the `LoadingScreen` owner, not the ECS.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoadingModelStage {
    pub(crate) no_rotation: bool,
}
impl Component for LoadingModelStage {
    type Storage = SparseSetStorage<Self>;
}

/// #5306 — the loading cover's own frame clock: wall-clock frame advance
/// while the cover is up, `0.0` while it is down. Stamped next to
/// [`byroredux_core::ecs::DeltaTime`] in `about_to_wait` and read by
/// `loading_model_turntable_system`.
///
/// Why this exists: the loading branch holds the scheduler's `dt` at `0.0`
/// for exactly the frames a `LoadingModelStage` exists (simulation time
/// must stand still under the cover), so a system driven by scheduler-`dt`
/// can never animate the cover. This resource is the wall-clock escape
/// hatch for the one piece of cover-side motion. Absent resource degrades
/// to no rotation rather than a panic, so a registration gap shows up as a
/// static model, not a crash.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LoadingCoverClock(pub(crate) f32);

impl Resource for LoadingCoverClock {}

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

/// Marker component for `NiAlphaProperty.flags` bit 13 (0x2000, "No
/// Sorter") — the shape author opted this draw out of Gamebryo's
/// `NiAlphaAccumulator` depth sort. See #3797.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NoSorter;
impl Component for NoSorter {
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

/// Marker on a placement root spawned *disabled* — a scripted `Disable()`
/// or the authored "Initially Disabled" flag (#3278 / #4813) — so it carries
/// its identity and payloads (`DoorTeleport`, `Locked`, `Inventory`) but no
/// mesh or collider.
///
/// #4820 — interaction reads this, not only the live `ReferenceEnableState`
/// ledger. A runtime `Enable()` clears the ledger entry, but there is no live
/// re-spawn, so the root stays invisible and non-solid until its cell next
/// loads; without the marker it became an invisible Open/Take prompt and a
/// door that still teleported. Stamped at spawn, never saved: the reload
/// re-derives it from the same inputs. The live re-spawn is the #5065
/// tracker (ROADMAP M47 known issues).
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlacementContentWithheld;
impl Component for PlacementContentWithheld {
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

/// Marker component for distant LOD entities beyond the full-detail streamed
/// ring: terrain LOD blocks (engine-generated from the worldspace heightmaps),
/// `.bto` object LOD and `.btt` tree LOD quads. They build **no BLAS** at
/// spawn. They draw through the ordinary `collect_static_mesh_draws` loop,
/// whose `tlas_exclusion` keeps them out of the TLAS unless their bound comes
/// within `LOD_SHADOW_CASTER_DISTANCE` of the camera; those camera-local ones
/// are admitted as RT shadow casters (#4180), and everything beyond that ring
/// costs zero ray-tracing budget.
///
/// Set in [`crate::cell_loader::terrain_lod::spawn_lod_block`], the object
/// LOD and the tree LOD spawns.
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
/// **No render-time consumer today, by design.** LOD reconciliation now tests
/// actual full-cell residency and rejects any proxy footprint that intersects
/// it, so a full model and its LOD proxy still never coexist. This component
/// materialises the signal for a future finer-grained handoff. See #1889.
///
/// **Correction (2026-08-26, #3307)**: radius decoupling turns out not to be
/// the only obstacle — a cull also needs something to suppress. Still true:
/// the bake carries **no per-source-object identity**. A quad's sub-meshes
/// are material/type groups (`Obj`, `obj-LargeRef`, `objsnowHD-LargeRef`, …),
/// never object names, and nothing in the chain carries a FormID —
/// `BSDistantObjectLargeRefExtraData`, which sounds like it would, is a
/// single `bool` per nif.xml. So a *per-object* cull is not expressible.
///
/// **Correction (2026-08-29, #3307)**: two claims in the paragraph above as
/// originally written were measured wrong, and the conclusion they supported
/// does not follow. Evidence:
/// `crates/nif/examples/bto_segment_census.rs` over Skyrim SE
/// `Skyrim - Meshes1.bsa` (1078 quads, 1856 level-4 sub-meshes).
///
/// 1. A quad is **not** "one merged, per-quad draw". `spawn_object_lod_block`
///    calls `world.spawn()` once per `imported.meshes` entry, so a level-4
///    quad spawns 1–5 entities, one per material/type group.
/// 2. The finest available lever is **not** "hide the whole quad". A
///    `BSSubIndexTriShape` carries a segment table which, for LOD, nif.xml
///    describes as "segmented in a **grid**". Measured: at level 4
///    (`4x4 = 16` cells per quad) `num_segments` caps at exactly **16** and
///    the non-zero segments exactly partition the sub-mesh's triangles — one
///    segment **per cell**. That is precisely the granularity Redux's
///    streaming already works at, since full REFRs spawn per cell. A
///    per-object cull was never what this engine needs; a **per-cell** one is,
///    and the bake does expose it.
///
/// The real limitation is narrower than "no hook exists": the per-cell grid
/// is **level-4 only**. Levels 8 and 16 ship exactly one segment on every
/// sub-mesh (384/384 and 152/152 measured), so for those the lever genuinely
/// is per-quad, with the pop-out cost the earlier note describes.
///
/// Still deliberately not built, but for the remaining reason rather than an
/// impossibility: consuming per-cell segments means drawing a LOD sub-mesh as
/// index sub-ranges, and combined with the radius decoupling it is a
/// render-visible change whose failure modes (pop, seam, z-fight at the
/// transition) are invisible to `cargo test`. It needs the live validation
/// EXAL §5.2 asks for before it can be enabled. See #3307.
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
    /// The worldspace `NAM3`/`NAM4` default water height — provenance; the
    /// distant mesh itself is per-cell (#5243).
    pub(crate) height: f32,
    pub(crate) water_form: Option<u32>,
    /// `(height, quad count)` histogram of the live distant-water mesh, the
    /// per-cell effective heights actually drawn. Sorted by height.
    pub(crate) quad_heights: [(f32, u32); 8],
    /// Number of valid entries in `quad_heights` (the histogram is capped at
    /// 8 distinct heights; worldspaces measure 1–3 in practice, with Oblivion
    /// Tamriel's lake ladder the observed maximum).
    pub(crate) quad_height_count: u8,
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
    /// #4423 — whether the tint DDS carries authored alpha. The tint role's
    /// vanilla producer is the Skin Tint shader's `*_sk.dds` map — a
    /// light-diffusion (subsurface) input that ships BC1, so every sampled
    /// `.a` would read 1.0 and the weighted tint multiply would run at full
    /// force on textures that cannot express a weight. Only an
    /// alpha-bearing tint may drive the shader's `mix(albedo, albedo *
    /// tint.rgb, tint.a)` block.
    pub(crate) tint_has_alpha: bool,
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
    /// The effective path came from an inline NIF texture set/property.
    NifTextureSet,
    /// An external BGSM chain filled the canonical role.
    Bgsm,
    /// An external BGEM filled the canonical role.
    Bgem,
    /// A Starfield `.mat` source filled the canonical role — the loose
    /// JSON file (#4277 Stage A) or the compiled CDB row
    /// (`apply_cdb_material`). #3906 removed an earlier copy of this arm
    /// because nothing could produce it and `mat.dump`'s `src=mat` label
    /// lied by absence; the arm returns with the Phase-2 code that fills
    /// roles, at the site that fills them, exactly as #3906 prescribed.
    Mat,
    /// A placement-level XATO/XTNM/XTXR texture-set override won.
    TxstOverride,
    /// The legacy `<base>_n.dds` convention synthesized a normal path.
    DerivedNormal,
    /// A runtime caller replaced an authored path (FaceGen tint, an NPC's race
    /// body / head / eye texture).
    RuntimeOverride,
}

impl MaterialTextureSource {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::NifTextureSet => "nif-texture-set",
            Self::Bgsm => "bgsm",
            Self::Bgem => "bgem",
            Self::Mat => "mat",
            Self::TxstOverride => "txst-override",
            Self::DerivedNormal => "derived-normal",
            Self::RuntimeOverride => "runtime-override",
        }
    }
}

impl From<byroredux_nif::import::ImportedTextureSource> for MaterialTextureSource {
    fn from(source: byroredux_nif::import::ImportedTextureSource) -> Self {
        match source {
            byroredux_nif::import::ImportedTextureSource::NifTextureSet => Self::NifTextureSet,
            byroredux_nif::import::ImportedTextureSource::Bgsm => Self::Bgsm,
            byroredux_nif::import::ImportedTextureSource::Bgem => Self::Bgem,
            // #4277 — the Starfield `.mat` sidecar source (loose JSON file
            // or compiled CDB row). Shares the provenance spelling
            // `mat.dump` documents.
            byroredux_nif::import::ImportedTextureSource::Mat => Self::Mat,
            byroredux_nif::import::ImportedTextureSource::RuntimeOverride => Self::RuntimeOverride,
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
