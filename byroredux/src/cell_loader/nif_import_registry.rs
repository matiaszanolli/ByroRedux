//! Process-lifetime cache of parsed-and-imported NIF scenes.
//!
//! Cells frequently re-use the same model across hundreds of placements
//! (e.g. 40 chairs sharing one `chair.nif`); without a process-lifetime
//! cache every cell-load would re-parse every NIF, wasting CPU and
//! flooding the parser-warning log. [`NifImportRegistry`] is the
//! `World` resource that keeps the parsed scenes alive across cell
//! transitions.
//!
//! Two layers:
//!
//! * [`CachedNifImport`] holds the parsed-and-imported scene data for
//!   one model — meshes, collisions, lights, particle emitters, the
//!   embedded animation clip — wrapped in `Arc` so REFR placements
//!   share the same allocation.
//! * [`NifImportRegistry`] is the `World` resource that owns a shared
//!   [`ParsedNifCache`] (the bookkeeping core — keys + lifetime
//!   counters) plus the LRU machinery (`access_tick`, `evictions`)
//!   and per-key animation-clip handle memoisation
//!   (`clip_handles`, #544 / #863).
//!
//! See #381 (process-lifetime promotion), #635 (LRU cap via
//! `BYRO_NIF_CACHE_MAX`), #523 (batched touch invariant), and #544
//! (clip-handle memoisation).

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::{BillboardMode, Resource};

use crate::parsed_nif_cache::ParsedNifCache;

/// Canonical [`NifImportRegistry`] cache key for a raw (authored)
/// `model_path`. Lowercases and prefixes `meshes\` when the path is
/// stored folder-relative — the archive-path form `extract_mesh`
/// expects, and the form the majority of `StaticRecord::model_path`
/// values already carry.
///
/// #3038 / FNV-2026-08-16-D1-02 — every producer of a registry key MUST
/// route through this one function rather than building the key
/// inline. Before the fix, the synchronous REFR loader
/// (`references/synth_child.rs`) prefixed `meshes\` before lowercasing
/// while the exterior-streaming loader (`streaming/`) only
/// lowercased — so the same asset landed under two different keys,
/// was parsed and imported twice, and cache-hit telemetry undercounted
/// reuse. Idempotent: re-applying it to an already-canonical key (e.g.
/// one already threaded through from the streaming payload) is a
/// no-op.
pub(crate) fn canonical_model_path_key(model_path: &str) -> String {
    let lower = model_path.to_ascii_lowercase().replace('/', "\\");
    if lower.starts_with("meshes\\") {
        lower
    } else {
        format!("meshes\\{lower}")
    }
}

#[cfg(test)]
mod canonical_key_tests {
    use super::canonical_model_path_key;

    /// The dominant authored form — folder-relative, no `meshes\`
    /// prefix — gets the prefix added and is lowercased.
    #[test]
    fn prefixes_and_lowercases_folder_relative_path() {
        assert_eq!(
            canonical_model_path_key("Clutter\\Barrel02FireLight.nif"),
            "meshes\\clutter\\barrel02firelight.nif"
        );
    }

    /// An already-prefixed path (backslash form) is lowercased only —
    /// no double prefix.
    #[test]
    fn does_not_double_prefix_backslash_form() {
        assert_eq!(
            canonical_model_path_key("Meshes\\Clutter\\Barrel02FireLight.nif"),
            "meshes\\clutter\\barrel02firelight.nif"
        );
    }

    /// Archive-listing separators converge with authored backslashes, so the
    /// same asset cannot occupy two registry keys (#3214).
    #[test]
    fn does_not_double_prefix_forward_slash_form() {
        assert_eq!(
            canonical_model_path_key("meshes/clutter/barrel02firelight.nif"),
            "meshes\\clutter\\barrel02firelight.nif"
        );
    }

    /// #3038's regression case: this is the exact pair the live
    /// `mesh.cache failed` probe on WastelandNV surfaced as two
    /// entries for one asset. Both the sync-loader form (already
    /// `meshes\`-prefixed) and the streaming-loader form (bare) must
    /// canonicalise to the same key.
    #[test]
    fn sync_and_streaming_forms_converge_on_one_key() {
        let sync_form = canonical_model_path_key("meshes\\clutter\\barrel02firelight.nif");
        let streaming_form = canonical_model_path_key("clutter\\barrel02firelight.nif");
        assert_eq!(sync_form, streaming_form);
        assert_eq!(
            sync_form,
            canonical_model_path_key("meshes/clutter/barrel02firelight.nif")
        );
    }

    /// Applying the function twice (e.g. once at the streaming
    /// producer, once more at a downstream consumer that
    /// unconditionally re-normalises) must not change the key.
    #[test]
    fn is_idempotent() {
        let once = canonical_model_path_key("Clutter\\Barrel02FireLight.nif");
        let twice = canonical_model_path_key(&once);
        assert_eq!(once, twice);
    }
}

/// [`NifImportRegistry`] cache key for a `.spt` SpeedTree import.
///
/// #3750 — unlike a NIF, whose parse depends only on the file,
/// `parse_and_import_spt` bakes per-TREE-record metadata (ICON / OBND /
/// MODB / BNAM) into the returned [`CachedNifImport`]. Keying purely on
/// `canonical_model_path_key(model_path)` — as every NIF import does —
/// would collapse every TREE record that points at one shared `.spt` onto
/// whichever record was imported first, silently handing its texture and
/// billboard size to the rest. Suffixing the canonical path key with the
/// TREE record's own form id gives every distinct record its own entry
/// while identical records (same path, same form id revisited across
/// cells) still share one parse.
pub(crate) fn spt_cache_key(model_path: &str, tree_form_id: u32) -> String {
    format!(
        "{}#{tree_form_id:08x}",
        canonical_model_path_key(model_path)
    )
}

#[cfg(test)]
mod spt_cache_key_tests {
    use super::spt_cache_key;

    /// #3750's regression case: two TREE records sharing one `.spt`
    /// MODL value must land in distinct cache entries.
    #[test]
    fn distinct_tree_records_sharing_one_spt_get_distinct_keys() {
        let a = spt_cache_key("\\ShrubVineMapleSU.spt", 0x0002_32db);
        let b = spt_cache_key("\\ShrubVineMapleSU.spt", 0x17fc);
        assert_ne!(a, b);
    }

    /// The same TREE record revisited (e.g. a second placement, or the
    /// same cell reloaded) must still converge on one key so the cache
    /// actually caches.
    #[test]
    fn same_tree_record_converges_on_one_key() {
        let once = spt_cache_key("\\ShrubVineMapleSU.spt", 0x0002_32db);
        let twice = spt_cache_key("\\shrubvinemaplesu.spt", 0x0002_32db);
        assert_eq!(once, twice);
    }
}

/// Merge each mesh's own external material (BGSM/BGEM/`.mat`) into its
/// `ImportedMaterial`, returning the pre-merge snapshot of every mesh the
/// merge could have touched — [`CachedNifImport::pre_merge_materials`].
///
/// The one cache-fill merge loop, shared by the synchronous
/// (`references/import.rs`) and streaming (`partial.rs`) paths. Each
/// outcome is discarded deliberately (#2709): neither path has a per-cell
/// material tally to feed it into.
pub(super) fn merge_external_materials(
    meshes: &mut [byroredux_nif::import::ImportedMesh],
    provider: &mut crate::asset_provider::MaterialProvider,
    pool: &mut byroredux_core::string::StringPool,
    // #4636 — spawn-time texture-existence probe for the merge's
    // dead-path repair (`fill`): a NIF slot whose path resolves in no
    // loaded archive yields to the sidecar chain's resolvable path.
    texture_exists: &dyn Fn(&str) -> bool,
) -> Vec<Option<byroredux_nif::import::ImportedMaterial>> {
    meshes
        .iter_mut()
        .map(|mesh| {
            let pre_merge = mesh.material.material_path.map(|_| mesh.material.clone());
            let _ = crate::asset_provider::merge_external_material(
                &mut mesh.material,
                provider,
                pool,
                texture_exists,
            );
            pre_merge
        })
        .collect()
}

/// Parsed + imported NIF scene data cached per unique model path.
pub(crate) struct CachedNifImport {
    /// Model-local beam media, materialized once on the first placement.
    /// The import registry keys this immutable scene by model path, so both
    /// successful matches and rejections can be reused across all placements.
    pub(super) beam_volumes: std::sync::OnceLock<Vec<Option<Arc<[byroredux_core::ecs::FogVolume]>>>>,
    pub(super) meshes: Vec<byroredux_nif::import::ImportedMesh>,
    /// #3510 — per-mesh geometry representative, parallel to `meshes`.
    ///
    /// `geometry_dedup[i]` names the index whose *geometry* mesh `i` shares.
    /// The identity mapping (and the empty vector, which means the same
    /// thing) is the normal case: one entry per distinct sub-mesh.
    ///
    /// FO4 precombines are the exception. `build_precombine_meshes`
    /// materialises one `ImportedMesh` per `BSPackedGeomDataCombined`
    /// instance transform, all cloned from one decoded shared geometry and
    /// differing only in `translation`/`rotation`/`scale` — so N byte-
    /// identical meshes claim N distinct `(path, sub_mesh_index)` cache
    /// slots, N vertex/index uploads and N BLAS builds, and the draw
    /// batcher (which batches on mesh handle) cannot merge them. Measured
    /// over 4 000 `_oc.nif` files: 1.37x VRAM overall, 4.6x on the worst
    /// tile, where 2 257 meshes carry ~500 distinct geometries.
    ///
    /// Pointing every instance at its object's first mesh collapses the
    /// uploads and the BLASes while each instance keeps its own entity and
    /// `Transform`, which is where the placement actually belongs.
    pub(super) geometry_dedup: Vec<u32>,
    pub(super) collisions: Vec<byroredux_nif::import::ImportedCollision>,
    /// Collision-object subclasses retained independently of decoded shapes.
    /// An empty `collisions` vector plus `new_physics > 0` means FO4+ packed
    /// Havok was authored and the spawn path should use its compatibility
    /// proxy rather than treating the asset as intentionally non-colliding.
    pub(super) collision_authoring: byroredux_nif::import::collision::CollisionAuthoringSummary,
    pub(super) lights: Vec<byroredux_nif::import::ImportedLight>,
    /// Particle emitters detected in the NIF scene graph
    /// (`NiParticleSystem` and friends). Carries NIF-local position +
    /// the nearest named ancestor's name. The spawn step composes the
    /// REFR placement on top and translates to a heuristic
    /// [`crate::components::ParticleEmitter`] preset. See #401.
    pub(super) particle_emitters: Vec<byroredux_nif::import::ImportedParticleEmitterFlat>,
    /// Ambient animation clip collecting every mesh-embedded controller
    /// (alpha fade, UV scroll, visibility flicker, material colour
    /// pulse, shader float/colour). Shared across REFR placements: the
    /// clip handle is registered once per cache load; each placement
    /// spawns its own `AnimationPlayer` scoped to the spawned root
    /// entity so the subtree-local name lookup matches the authored
    /// node names. `None` when the NIF authored no supported
    /// controllers. See #261.
    ///
    /// Read at spawn: `synth_child.rs` converts it once into a clip handle
    /// (kept in `clip_handles`, #544) and attaches an `AnimationPlayer` per
    /// placement. Standalone controllers only: the NIF's sequences are
    /// [`Self::object_sequences`], played on a player of their own (#6253).
    pub(super) embedded_clip: Option<byroredux_nif::anim::AnimationClip>,
    /// Every `NiControllerSequence` of a NIF that names a behavior graph: the
    /// clips the graph's Gamebryo sequence generators play, by sequence name.
    pub(super) graph_sequences: Vec<byroredux_nif::anim::AnimationClip>,
    /// Every `NiControllerSequence` of the NIF, by name: the animation groups an object's script
    /// can start (`PlayGroup Forward`, M47.3). The first also plays from spawn, on its own player
    /// beside the `embedded_clip` one (#3602 / #6253); `PlayGroup` retargets that player only, so
    /// the standalone controllers keep running.
    pub(super) object_sequences: Vec<byroredux_nif::anim::AnimationClip>,
    /// Billboard mode to attach to the spawned placement root entity —
    /// a seam for a future `NiBillboardNode`-rooted NIF producer, not a
    /// live path today. #3076 moved the SpeedTree billboard from the
    /// placement root onto the renderable mesh itself
    /// (`mesh_instance.rs` attaches `Billboard` alongside the mesh), so
    /// `import_spt_scene` now builds its root with
    /// `placeholder_root_node(/* billboard */ false)` and this field is
    /// structurally `None` at every construction site in the tree — its
    /// consumer in `spawn.rs` is currently unreachable. Kept as the seam
    /// for a NIF path that would root-billboard instead of mesh-billboard;
    /// no such producer exists yet (see #994).
    pub(super) placement_root_billboard: Option<BillboardMode>,
    /// Neutral runtime response/stiffness constant (`(1.0, 0.0)`) for
    /// SpeedTree sway, set by the SPT importer. `TreeRecord.canopy_params`
    /// (TREE's `CNAM`) is parsed but not consumed until a citable field
    /// layout lands (#3190). `None` for NIF and generated imports, which use
    /// the shared weather response fallback.
    pub(super) speedtree_wind: Option<(f32, f32)>,
    /// `BSXFlags` bits authored on the NIF root (havok-managed,
    /// ragdoll, editor-marker, articulated, etc.). Captured at parse
    /// time so the spawn site can attach a `BSXFlags` ECS row on the
    /// placement root without re-reading the scene. See #1214 /
    /// D1-NEW-03. `0` when the NIF authors no BSXFlags.
    pub(super) bsx_flags: u32,
    /// The behavior-graph project the NIF names (`BSBehaviorGraphExtraData`),
    /// stamped on the placement root as a `BehaviorGraphRef` for the animation
    /// graph loader. `None` for NIFs without one and for generated imports.
    pub(super) behavior_graph_file: Option<std::sync::Arc<str>>,
    /// The node tree of a behavior-graph NIF, so the spawn can keep the
    /// NiNodes the graph's clips animate (see `graph_nodes`). `None` for every
    /// other NIF.
    pub(super) graph_nodes: Option<std::sync::Arc<super::graph_nodes::GraphNodes>>,
    /// `(min, max)` of the scene's `bhkSimpleShapePhantom` in NIF-root Y-up
    /// space — Skyrim's authored water volume for placed water meshes, read
    /// at spawn by `attach_mesh_water`. `None` for NIFs without one and for
    /// generated imports. See `collision::extract_phantom_bounds`.
    pub(super) phantom_bounds: Option<([f32; 3], [f32; 3])>,
    /// `NiAVObject.flags` from the root NiNode (SELECTIVE_UPDATE,
    /// DISABLE_SORTING, DISPLAY_OBJECT, IS_NODE, …). Captured at parse
    /// time so the spawn site can attach a `SceneFlags` ECS row on the
    /// placement root — parity with the loose-NIF loader's per-node
    /// `SceneFlags::from_nif` insert. APP_CULLED (bit 0) is already
    /// filtered import-side, so reaching the cache means the bit is
    /// clear. See #1235 / LC-D1-NEW-01. `0` when the NIF has no scene
    /// graph (SpeedTree placeholder, generated content).
    pub(super) root_flags: u32,
    /// Root-local (Y-up) position of the model's `AttachLight` node —
    /// where a LIGH-with-model's light belongs (barrel fires, torches,
    /// lamps), as opposed to the model's base where the REFR sits.
    /// Composed through the whole node chain at parse time by
    /// `byroredux_nif::import::find_attach_light_offset`, because the node
    /// array does not survive into this struct.
    ///
    /// `None` when the model authors no `AttachLight` — the spawn path then
    /// keeps the ESM light on the placement's first mesh entity.
    pub(super) attach_light_offset: Option<[f32; 3]>,
    /// The root-local position of the NIF's `ProjectileNode` (the muzzle of a trap's
    /// dart port), stamped on the placement root as a `ProjectileMuzzle`.
    pub(super) projectile_offset: Option<[f32; 3]>,
    /// FO4+ weapon-mod / power-armor attach points the NIF *exposes*
    /// (`BSConnectPoint::Parents`). Interned to the `AttachPoints` ECS
    /// component at parse time (the node array doesn't survive into this
    /// cache) and stamped onto the placement root at spawn. `None` when the
    /// NIF authors no connect-point parents — the dominant case (only
    /// modular weapons / armor carry them). The visible attachment of mod
    /// parts is the #973 OMOD consumer's job; this just lands the graph in
    /// the ECS. See #985 / #1594.
    pub(super) attach_points: Option<byroredux_core::ecs::components::AttachPoints>,
    /// FO4+ attach-point names the NIF *connects back to* on its host
    /// (`BSConnectPoint::Children`), interned to `ChildAttachConnections`.
    /// `None` when absent. See #985 / #1594.
    pub(super) child_attach_connections:
        Option<byroredux_core::ecs::components::ChildAttachConnections>,
    /// Sit/sleep/lean entry markers from a `BSFurnitureMarker` block,
    /// lifted to the `Furniture` ECS component at parse time (the node
    /// array doesn't survive into this cache) and stamped onto the
    /// placement root at spawn. `None` for the dominant non-furniture
    /// case. Surfacing the markers is the foundation for actor seating
    /// (M41.5 Phase C); this just lands them in the ECS. See M41.5 Phase B.
    pub(super) furniture: Option<byroredux_core::ecs::components::Furniture>,
    /// #4290 — each mesh's `ImportedMaterial` as the NIF authored it, taken
    /// just before `merge_external_material` folded the mesh's OWN sidecar
    /// in. Parallel to `meshes`; `None` for a mesh with no `material_path`
    /// (the merge never runs, so `meshes[i].material` already is the
    /// pre-merge state), and empty when no material provider was present.
    ///
    /// A REFR material swap (MSWP / XMSP, or a TXST MNAM) replaces the whole
    /// material, and the merge is fill-if-unset with one-way `two_sided` /
    /// `alpha_test` / `is_decal` flags — so merging the swap target over the
    /// already-merged `meshes[i].material` would keep the source sidecar's
    /// values. The spawn path merges the target onto this snapshot instead
    /// (`resolve_mesh_paths_with_pre_merge`).
    pub(super) pre_merge_materials: Vec<Option<byroredux_nif::import::ImportedMaterial>>,
    // SPT-NEW-03 / #1711 — route divergence, documented intentionally.
    //
    // `ImportedScene::bs_bound` (an OBND/BSBound-derived AABB) is deliberately
    // NOT carried through this adapter, so cell-loaded NIFs do NOT get a
    // `BSBound` ECS component — only the loose `--tree`/`--mesh` loader
    // (`scene::nif_loader`) attaches one. This is not a correctness gap: the
    // cell route's per-mesh `LocalBound` *sphere* (from `local_bound_center`/
    // `local_bound_radius`) is the canonical bound used for culling/picking on
    // both routes. `BSBound` currently has no functional consumer (only
    // debug-server inspection), so threading the AABB through the ~7
    // construction sites here (two of which — the streaming-partial path and
    // the precombine path — never run a full import and so have no AABB to
    // forward) buys nothing today. When an AABB-keyed consumer lands (e.g. a
    // tight-bounds culler), add a `bs_bound: Option<([f32; 3], [f32; 3])>`
    // field here mirroring `bsx_flags` and attach `BSBound` in `spawn.rs` for
    // route parity. A future consumer keying off `BSBound` must not assume it
    // exists on cell-loaded entities.
}


/// #5795 — the scene-derived satellites of a parsed NIF, extracted in one
/// place for both cache-fill paths (synchronous `parse_and_import_nif` and
/// the streaming `partial.rs` rebuild). The two paths used to spell this
/// extraction and the struct literal twice, and the streaming copy lagged
/// twice on exactly this seam (#3072 furniture, #3074 attach-light — both
/// hard-coded `None` citing a blocker that was false).
pub(super) struct SceneSatellites {
    pub(super) collision_authoring:
        byroredux_nif::import::collision::CollisionAuthoringSummary,
    pub(super) attach_light_offset: Option<[f32; 3]>,
    pub(super) projectile_offset: Option<[f32; 3]>,
    pub(super) furniture: Option<byroredux_core::ecs::components::Furniture>,
    pub(super) behavior_graph_file: Option<Arc<str>>,
    pub(super) graph_nodes: Option<Arc<super::graph_nodes::GraphNodes>>,
    pub(super) graph_sequences: Vec<byroredux_nif::anim::AnimationClip>,
    /// Every `NiControllerSequence` of the NIF, by name — what an ObScript `PlayGroup` chooses
    /// among (M47.3). Empty for a mesh with none.
    pub(super) object_sequences: Vec<byroredux_nif::anim::AnimationClip>,
    pub(super) phantom_bounds: Option<([f32; 3], [f32; 3])>,
}

/// Extract [`SceneSatellites`] from a parsed scene. `meshes` /
/// `collisions` are the already-imported geometry (the behavior-graph
/// node tree indexes them).
pub(super) fn scene_satellites(
    scene: &byroredux_nif::scene::NifScene,
    meshes: &[byroredux_nif::import::ImportedMesh],
    collisions: &[byroredux_nif::import::ImportedCollision],
) -> SceneSatellites {
    // M47.2 — an animated prop keeps its NiNode tree (see `graph_nodes`).
    let behavior_graph_file: Option<Arc<str>> =
        byroredux_nif::import::extract_behavior_graph_file(scene).map(Into::into);
    let graph_nodes = behavior_graph_file.as_ref().and_then(|_| {
        super::graph_nodes::build(
            scene,
            meshes,
            collisions,
            &mut byroredux_core::string::StringPool::new(),
        )
    });
    let object_sequences = byroredux_nif::anim::import_kf(scene);
    let graph_sequences = if behavior_graph_file.is_some() { object_sequences.clone() } else { Vec::new() };
    // M41.5 Phase B — lift `BSFurnitureMarker` sit/sleep/lean positions
    // to the `Furniture` ECS component. `None` for the dominant
    // non-furniture case.
    let furniture = {
        let markers = byroredux_nif::import::extract_furniture_markers(scene);
        if markers.is_empty() {
            None
        } else {
            Some(super::references::furniture_component(&markers))
        }
    };
    SceneSatellites {
        collision_authoring:
            byroredux_nif::import::collision::summarize_collision_authoring(scene),
        attach_light_offset: byroredux_nif::import::find_attach_light_offset(scene),
        projectile_offset: byroredux_nif::import::find_projectile_node_offset(scene),
        furniture,
        behavior_graph_file,
        graph_nodes,
        graph_sequences,
        object_sequences,
        phantom_bounds: byroredux_nif::import::collision::extract_phantom_bounds(scene),
    }
}

impl CachedNifImport {
    /// #5795 — the one constructor both NIF cache-fill paths build
    /// through, so a satellite extracted from the scene cannot land on
    /// only one path again. `attach_points` /
    /// `child_attach_connections` stay `None` here by design: the sync
    /// path materializes the FO4+ attach graph after construction (it
    /// needs the `StringPool`), and the streaming partial keeps `None`
    /// (#1594 — cell-streamed REFRs are architecture / clutter, not
    /// modular weapons).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_scene(
        meshes: Vec<byroredux_nif::import::ImportedMesh>,
        collisions: Vec<byroredux_nif::import::ImportedCollision>,
        lights: Vec<byroredux_nif::import::ImportedLight>,
        particle_emitters: Vec<byroredux_nif::import::ImportedParticleEmitterFlat>,
        embedded_clip: Option<byroredux_nif::anim::AnimationClip>,
        bsx_flags: u32,
        root_flags: u32,
        satellites: SceneSatellites,
        pre_merge_materials: Vec<Option<byroredux_nif::import::ImportedMaterial>>,
    ) -> Self {
        Self {
            meshes,
            beam_volumes: Default::default(),
            geometry_dedup: Vec::new(),
            collisions,
            collision_authoring: satellites.collision_authoring,
            lights,
            particle_emitters,
            embedded_clip,
            graph_sequences: satellites.graph_sequences,
            object_sequences: satellites.object_sequences,
            // Both NIF routes leave root billboard / SpeedTree wind to
            // their own producers (see the field docs: #994, #3190).
            placement_root_billboard: None,
            speedtree_wind: None,
            bsx_flags,
            behavior_graph_file: satellites.behavior_graph_file,
            graph_nodes: satellites.graph_nodes,
            phantom_bounds: satellites.phantom_bounds,
            root_flags,
            attach_light_offset: satellites.attach_light_offset,
            projectile_offset: satellites.projectile_offset,
            attach_points: None,
            child_attach_connections: None,
            furniture: satellites.furniture,
            pre_merge_materials,
        }
    }

    /// Texture paths a placement of this model resolves when no REFR overlay
    /// applies: every mesh's merged material slot, plus the normal map the
    /// spawn pre-pass derives when a mesh authors none
    /// (`resolve_mesh_paths_with_pre_merge`). Raw paths, in mesh order,
    /// possibly repeated — the prefetch canonicalises and deduplicates.
    ///
    /// Overlay-driven paths (XATO / XTXR / MSWP) are deliberately absent:
    /// those resolves fall back to the inline read.
    pub(crate) fn texture_prefetch_paths(
        &self,
        pool: &byroredux_core::string::StringPool,
        tex_provider: &crate::asset_provider::TextureProvider,
    ) -> Vec<String> {
        let mut paths = Vec::new();
        for mesh in &self.meshes {
            let textures = &mesh.material.textures;
            paths.extend(
                textures
                    .roles()
                    .filter_map(|(_, sym)| sym.and_then(|s| pool.resolve(s)))
                    .map(str::to_owned),
            );
            if textures.normal.is_none() {
                if let Some(base) = textures.base_color.and_then(|s| pool.resolve(s)) {
                    paths.extend(crate::asset_provider::derive_present_normal_map_path(
                        tex_provider,
                        base,
                    ));
                }
            }
        }
        paths
    }

    pub(super) fn beam_volumes(
        &self,
        model_path: Option<&str>,
    ) -> &[Option<Arc<[byroredux_core::ecs::FogVolume]>>] {
        self.beam_volumes.get_or_init(|| {
            self.meshes.iter().map(|mesh| {
                crate::fog::fnv_nellis_hangar_beam_volumes_from_mesh(model_path, mesh)
                    .or_else(|| {
                        crate::fog::window_beam_volume_from_mesh(model_path, mesh)
                            .or_else(|| crate::fog::oblivion_dungeon_beam_volume_from_mesh(model_path, mesh))
                            .or_else(|| crate::fog::authored_cone_beam_volume_from_mesh(model_path, mesh))
                            .or_else(|| crate::fog::fnv_superwide_beam_volume_from_mesh(model_path, mesh))
                            .or_else(|| crate::fog::vault_window_beam_volume_from_mesh(model_path, mesh))
                            .or_else(|| crate::fog::fo4_ambient_lamp_beam_volume_from_mesh(model_path, mesh))
                            .map(|volume| vec![volume])
                    })
                    .map(Arc::from)
            }).collect()
        })
    }
}

/// Process-lifetime cache of parsed-and-imported NIF scenes keyed by
/// lowercased model path. Promotes the per-`load_references`
/// `import_cache` (#383) to a world-resource so cell-to-cell traversal
/// re-uses every previously-parsed mesh.
///
/// `None` entries (in `core.cache`) record a model that failed to
/// parse (or had zero useful geometry) so re-parses don't fire on
/// every placement.
///
/// **Memory bound:** LRU cap via `BYRO_NIF_CACHE_MAX` env var (#635 /
/// FNV-D3-05). Default `2048` entries. Set `BYRO_NIF_CACHE_MAX=N` to
/// override; set `BYRO_NIF_CACHE_MAX=0` to disable the cap entirely
/// (unlimited, pre-#635 behaviour — a `warn!` fires at startup to flag
/// the risk in long streaming sessions). The LRU victim is the entry
/// with the smallest access tick (least-recently inserted *or* hit).
/// Eviction counts surface in the `mesh.cache` debug command.
pub(crate) struct NifImportRegistry {
    /// Shared bookkeeping core — entries, hits, misses, parsed/failed
    /// counts. Counterpart of `SceneImportCache.core`.
    pub(crate) core: ParsedNifCache<CachedNifImport>,
    /// Monotonic access tick per cached key. Bumped on every batched
    /// touch (insert + hit-set commits at end-of-load). Larger value =
    /// more recently accessed. Eviction picks the entry with the
    /// smallest tick when `core.len() > max_entries`.
    pub(super) access_tick: HashMap<String, u64>,
    /// Next access tick value to assign. Wraps at u64::MAX (~10^19
    /// touches — the cache will OOM long before this overflows).
    pub(super) next_tick: u64,
    /// LRU cap. `0` = unlimited (explicit via `BYRO_NIF_CACHE_MAX=0`).
    /// Default `2048`. Read once at construction from the env var.
    pub(super) max_entries: usize,
    /// LRU evictions across the process lifetime. Stays at 0 when
    /// `max_entries == 0` (unlimited mode).
    pub(crate) evictions: u64,
    /// Memoised `AnimationClipRegistry` handles per cache key (#544).
    /// Each parsed NIF whose `embedded_clip` is non-empty is registered
    /// with the global `AnimationClipRegistry` once and the resulting
    /// handle is stashed here so subsequent REFRs of the same model —
    /// within this cell or any future cell — re-use the same clip
    /// without re-converting the channel arrays. Cleared in lockstep
    /// with `core` and `access_tick` on `clear()` / LRU eviction so a
    /// stale handle can never reach the player after the underlying
    /// cache entry has been dropped.
    pub(super) clip_handles: HashMap<String, u32>,
    /// A shared (non-precombine) model has been LRU-evicted: the cap is below
    /// the streaming ring's working set (#6114). Latch for the one warning.
    pub(super) warned_shared_eviction: bool,
    /// A streaming batch is in flight (#6114): eviction waits for it, up to
    /// [`DEFERRED_EVICTION_HEADROOM`] x the cap. The worker's `cached_keys`
    /// snapshot and dispatch-batch memo skip a model on the premise it is
    /// resident when a later cell applies; evicting mid-batch turns that skip
    /// into a main-thread re-parse of work already done.
    pub(super) eviction_deferred: bool,
}

/// How far past `max_entries` the registry may grow while a streaming batch is
/// in flight (#6114). Fallout 4's default radius peaks near 9,700 distinct
/// models against the 2,048 default, so 8x covers it; the ceiling keeps a
/// pathological load order bounded rather than unlimited.
pub(super) const DEFERRED_EVICTION_HEADROOM: usize = 8;

/// FO4 `_oc.nif` precombine entries are keyed by a path unique to one cell
/// (`meshes\precombined\<cell>_<hash>_oc.nif`) and never serve another cell, so
/// the LRU evicts them before any shared model (#6114).
fn is_cell_unique_key(key: &str) -> bool {
    key.ends_with("_oc.nif") && key.contains("\\precombined\\")
}

impl Default for NifImportRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl NifImportRegistry {
    pub(crate) fn new() -> Self {
        // Default cap is 2048. `BYRO_NIF_CACHE_MAX=N` overrides it;
        // `BYRO_NIF_CACHE_MAX=0` forces unlimited (pre-#635 behaviour).
        // The eviction loop is gated on `max_entries > 0` so the
        // unlimited path stays allocation-free aside from HashMap inserts.
        let max_entries = std::env::var("BYRO_NIF_CACHE_MAX")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(2048);
        if max_entries == 0 {
            log::warn!(
                "NifImportRegistry: LRU cap disabled (BYRO_NIF_CACHE_MAX=0). \
                 RAM usage grows without bound during long exterior streaming sessions.",
            );
        }
        Self {
            core: ParsedNifCache::new(),
            access_tick: HashMap::new(),
            next_tick: 0,
            max_entries,
            evictions: 0,
            clip_handles: HashMap::new(),
            warned_shared_eviction: false,
            eviction_deferred: false,
        }
    }

    /// Fully invalidate the cache: drop every cached scene, LRU touch
    /// tick, and memoised animation-clip handle, and reset the
    /// lifetime hit/miss/eviction telemetry to match (a fresh registry
    /// with no history, not a registry that remembers stats from
    /// content it no longer holds).
    ///
    /// The registry is normally never cleared mid-process by design —
    /// that's the whole point of a process-lifetime cache reused across
    /// cell-to-cell streaming. The one caller that needs this is the
    /// debug `cell.load` console command: it can re-point the archive
    /// set (`--bsa`/`--esm`/`--master`) on every request against the
    /// same World, but the cache key is a bare lowercased model path
    /// with no archive-set discriminant. Without an explicit clear on
    /// an archive-set change, a model-path collision across two such
    /// requests (e.g. comparing vanilla content against a mod's
    /// overriding BSA in one debug session) would silently keep
    /// serving the first-loaded content. See FNV-D1-02 / #2078.
    ///
    /// Returns the embedded-clip handles the cleared entries owned, exactly as
    /// [`Self::insert`] does for an LRU eviction (#863): the caller releases
    /// them from the `AnimationClipRegistry`, or every clear strands the
    /// keyframe arrays of each animated cached model (#6117).
    #[must_use = "the freed clip handles must be released from the AnimationClipRegistry"]
    pub(crate) fn clear(&mut self) -> Vec<u32> {
        self.core = ParsedNifCache::new();
        self.access_tick.clear();
        self.next_tick = 0;
        self.evictions = 0;
        self.clip_handles.drain().map(|(_, handle)| handle).collect()
    }

    /// Look up a previously-registered embedded-clip handle for `key`.
    /// Returns `None` when the cache key has never been parsed, when
    /// the parsed NIF authored no controllers, or when the entry was
    /// evicted by the LRU sweep. The lookup is read-only — used in the
    /// spawn hot path before falling through to the (write-locked)
    /// registration site. See #544.
    pub(crate) fn clip_handle_for(&self, key: &str) -> Option<u32> {
        self.clip_handles.get(key).copied()
    }

    /// Memoise an embedded-clip handle for `key`. Called once per
    /// unique parsed NIF that authored controllers — the spawn loop
    /// then reads the handle through [`Self::clip_handle_for`] without
    /// re-converting the channel arrays. See #544.
    pub(crate) fn set_clip_handle(&mut self, key: String, handle: u32) {
        self.clip_handles.insert(key, handle);
    }

    /// Total number of cached entries (parsed + failed). Delegates to
    /// the shared core.
    pub(crate) fn len(&self) -> usize {
        self.core.len()
    }

    /// #4684 — test-only seeding so other modules' tests can populate the
    /// registry's authoring totals without duplicating the field-by-field
    /// literal or reaching through `pub(super)` fields.
    #[cfg(test)]
    pub(crate) fn test_support_seed_authoring(&mut self, key: &str, classic: u32, new_physics: u32) {
        let mut entry = crate::cell_loader::nif_import_registry_tests::dummy_cached();
        let e = std::sync::Arc::get_mut(&mut entry).unwrap();
        e.collision_authoring.classic = classic;
        e.collision_authoring.new_physics = new_physics;
        // Test seeding: no clip handles exist to release, so the eviction
        // return is deliberately dropped.
        let _ = self.insert(key.to_string(), Some(entry));
    }

    /// Sum every cached model's [`CollisionAuthoringSummary`] into the
    /// spawn-census view of "was collision authored at all?" (#2874).
    ///
    /// The spawn census reads the Rapier side only, so its `0 colliders in
    /// this column` bucket cannot tell "the source NIFs author no collision"
    /// from "they author it and every shape was dropped in translation" —
    /// the exact conflation `CollisionAuthoringSummary` exists to remove.
    /// These totals supply the discriminator.
    ///
    /// Scoped to the *cache*, not to one cell: the registry is keyed by model
    /// path and lives for the process, so on a cell-to-cell traversal the
    /// totals cover every model loaded so far. That is a superset of the
    /// current cell, which is the safe direction for this diagnostic —
    /// nonzero totals never prove the drop happened in *this* cell, but zero
    /// totals do prove nothing colliding was ever authored, and that is the
    /// arm the census was mis-reporting.
    ///
    /// [`CollisionAuthoringSummary`]: byroredux_nif::import::collision::CollisionAuthoringSummary
    ///
    /// #4684 (PHYS-D6-2026-09-21-01) — the name states the scope the old
    /// name hid: this sums EVERY cached NIF in the process (up to the LRU
    /// cap, across all cells visited), not any particular cell or column.
    /// The census verdict must not read it as column-scoped authoring. A
    /// column-scoped sum needs a placement→cache-key join on spawned
    /// entities, which does not exist yet.
    pub(crate) fn collision_authoring_totals_registry_wide(
        &self,
    ) -> byroredux_physics::SpawnCensusAuthoring {
        let mut out = byroredux_physics::SpawnCensusAuthoring::default();
        for key in self.core.keys() {
            let Some(Some(entry)) = self.core.get(key) else {
                continue;
            };
            out.classic = out
                .classic
                .saturating_add(entry.collision_authoring.classic);
            out.new_physics = out
                .new_physics
                .saturating_add(entry.collision_authoring.new_physics);
            out.phantom = out
                .phantom
                .saturating_add(entry.collision_authoring.phantom);
            // #4407 — the plane counter joins its three siblings so the
            // deliberate `bhkPlaneShape` drop is visible at runtime.
            out.plane_shapes = out
                .plane_shapes
                .saturating_add(entry.collision_authoring.plane_shapes);
        }
        out
    }

    /// Configured LRU cap (`0` = unlimited).
    pub(crate) fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Hit rate as a percentage `[0, 100]`. Delegates to the shared
    /// core. `0.0` when no lookups have happened yet (avoid NaN).
    pub(crate) fn hit_rate_pct(&self) -> f64 {
        self.core.hit_rate_pct()
    }

    /// Look up `key` in the cache (read-only). Access-tick updates
    /// happen via [`Self::touch_keys`] in the batched end-of-load
    /// commit so the read path can stay on a shared lock. Hit/miss
    /// tracking is also batched (see [`Self::accumulate_hits`] /
    /// [`Self::accumulate_misses`]) — `get` itself does NOT bump
    /// the lifetime counters.
    pub(crate) fn get(&self, key: &str) -> Option<&Option<Arc<CachedNifImport>>> {
        self.core.get(key)
    }

    /// Snapshot every cached key (positive + negative entries) into an
    /// `Arc<HashSet<String>>` the cell-stream worker can consult to
    /// skip BSA-extract + parse for models the main thread already
    /// holds. See #862 / FNV-D3-NEW-03 — without this, a 7×7 grid
    /// crossing in WastelandNV re-parses every shared rock / roadway /
    /// junkpile NIF on every cell boundary even though >95% are
    /// cached. Snapshot building is O(N) on registry size and runs at
    /// most once per cell-crossing batch, so the cost is bounded.
    ///
    /// Negative entries (`Some(None)` — failed parses) are included so
    /// the worker doesn't pointlessly retry a known-failed parse.
    pub(crate) fn snapshot_keys(&self) -> std::sync::Arc<std::collections::HashSet<String>> {
        std::sync::Arc::new(self.core.keys().cloned().collect())
    }

    /// Bump the access tick for every key in `keys` so freshly-hit
    /// entries are protected from LRU eviction. Called from the
    /// end-of-load batched commit (one write lock instead of one per
    /// REFR — preserves #523's batching invariant).
    ///
    /// Keys absent from the cache are skipped: this only refreshes
    /// residents, it never resurrects an entry evicted earlier in the
    /// same load.
    ///
    /// #3387 — was `contains_key(key)` followed by
    /// `insert(key.to_string(), t)`, which allocated a `String` and
    /// re-hashed the key to overwrite a `u64` in a slot it had already
    /// located. Same shape as #832. `get_mut` does one hash and no
    /// allocation.
    pub(crate) fn touch_keys<'a, I: IntoIterator<Item = &'a str>>(&mut self, keys: I) {
        for key in keys {
            if let Some(slot) = self.access_tick.get_mut(key) {
                *slot = self.next_tick;
                self.next_tick = self.next_tick.wrapping_add(1);
            }
        }
    }

    /// Add a per-call hit tally to the lifetime counter. Used by
    /// `cell_loader::load_references` to commit a whole cell's
    /// accumulated hit count under a single resource write lock at
    /// end-of-load (#523).
    pub(crate) fn accumulate_hits(&mut self, n: u64) {
        self.core.accumulate_hits(n);
    }

    /// Add a per-call miss tally to the lifetime counter (companion
    /// of [`Self::accumulate_hits`]).
    pub(crate) fn accumulate_misses(&mut self, n: u64) {
        self.core.accumulate_misses(n);
    }

    /// Insert (or overwrite) a cache entry, refresh its access tick,
    /// and evict LRU entries while `len > max_entries`. The eviction
    /// loop is a no-op when `max_entries == 0` (unlimited mode), so
    /// the default path stays O(1) per insert.
    ///
    /// Returns a `Vec<u32>` of `AnimationClipRegistry` handles whose
    /// owning cache entries were evicted by this call — the caller
    /// MUST forward each to
    /// [`byroredux_core::animation::AnimationClipRegistry::release`]
    /// to free the underlying keyframe arrays. Pre-#863 the eviction
    /// path silently dropped the path binding and the clip's keyframe
    /// memory leaked under long-running `BYRO_NIF_CACHE_MAX > 0`
    /// sessions. The `must_use` attribute makes a forgetful caller a
    /// compile warning rather than a silent leak. Empty Vec on the
    /// no-eviction path (`BYRO_NIF_CACHE_MAX=0`, the opt-in *unlimited*
    /// mode — NOT the default, which is a 2048-entry cap that does
    /// evict); no allocation cost there.
    #[must_use = "evicted clip handles must be released into AnimationClipRegistry to free their keyframe arrays — see #863"]
    pub(crate) fn insert(&mut self, key: String, value: Option<Arc<CachedNifImport>>) -> Vec<u32> {
        // Core handles parsed/failed counter adjustment + map insertion.
        self.core.insert(key.clone(), value);
        let t = self.next_tick;
        self.next_tick = self.next_tick.wrapping_add(1);
        // #3705 — kept so the eviction loop below can exclude the entry
        // just inserted by THIS call from victim candidacy (its own tick
        // is always the newest, so plain LRU could never self-evict it,
        // but the clip-handle-aware filter below picks by handle
        // presence first — and a caller that sets the handle in a
        // separate call *after* `insert` returns, rather than the
        // batched before-insert order #1854 established, would make the
        // freshly-inserted entry look handle-less for the very eviction
        // sweep this call triggers).
        let inserted_key = key.clone();
        self.access_tick.insert(key, t);

        self.evict_over_cap(Some(inserted_key.as_str()))
    }

    /// The entry count eviction enforces right now: the cap, or
    /// [`DEFERRED_EVICTION_HEADROOM`] x the cap while a streaming batch is in
    /// flight (#6114). Meaningless in unlimited mode (`max_entries == 0`).
    fn effective_cap(&self) -> usize {
        if self.eviction_deferred {
            self.max_entries.saturating_mul(DEFERRED_EVICTION_HEADROOM)
        } else {
            self.max_entries
        }
    }

    /// Hold or release the eviction deferral (#6114). Releasing trims straight
    /// back to the cap (least-recently-touched first, per-cell precombines
    /// first) and returns the freed clip handles, which the caller MUST release
    /// from the `AnimationClipRegistry`, exactly as for [`Self::insert`].
    #[must_use = "evicted clip handles must be released into AnimationClipRegistry to free their keyframe arrays — see #863"]
    pub(crate) fn set_eviction_deferred(&mut self, deferred: bool) -> Vec<u32> {
        let was = std::mem::replace(&mut self.eviction_deferred, deferred);
        if was && !deferred {
            self.evict_over_cap(None)
        } else {
            Vec::new()
        }
    }

    /// Whether an in-flight streaming batch is currently holding eviction off.
    pub(crate) fn eviction_deferred(&self) -> bool {
        self.eviction_deferred
    }

    /// Evict until the registry is within [`Self::effective_cap`], never
    /// choosing `protected` (the entry the calling insert just wrote).
    fn evict_over_cap(&mut self, protected: Option<&str>) -> Vec<u32> {
        let mut freed_clip_handles: Vec<u32> = Vec::new();
        if self.max_entries > 0 {
            let cap = self.effective_cap();
            while self.core.len() > cap {
                // #3705 / ECS-2026-08-30-D10-05 — plain LRU has no way to
                // tell whether a live `AnimationPlayer` / `AnimationLayer`
                // elsewhere in the `World` still plays a clip handle
                // memoised here; a still-visible animated REFR whose model
                // path just happens to be the coldest cache entry gets its
                // clip silently and permanently emptied by
                // `AnimationClipRegistry::release` below. A full fix needs
                // either despawn-aware refcounting against every live
                // holder or storing the source path on every animation
                // component so a released clip can self-heal via
                // `get_or_insert_by_path` — both bigger, riskier changes
                // than this cache's own eviction policy (the latter also
                // touches the save format, see #3701's `FORMAT_MAJOR`
                // bump for the same component family). Short of that,
                // bias eviction away from entries that carry a memoised
                // clip handle at all: most cached models author no
                // embedded controllers, so preferring the non-animated
                // majority as LRU victims materially cuts the odds of
                // hitting an animated one, without weakening the cache's
                // actual memory bound (an all-animated cache still evicts
                // via plain LRU once every candidate carries a handle).
                //
                // O(N) sweep(s) over `access_tick`, same cost class as the
                // pre-existing plain-LRU scan below.
                // #6114 — a per-cell precombine entry is the cheapest victim:
                // it is shared with no other cell, so evicting the oldest one
                // costs a re-parse only if its own cell is still to apply,
                // while the models every cell shares stay resident. Left to
                // plain LRU these (the newest inserts) pushed the shared
                // statics out.
                let victim = self
                    .access_tick
                    .iter()
                    .filter(|(k, _)| {
                        Some(k.as_str()) != protected
                            && is_cell_unique_key(k)
                            && !self.clip_handles.contains_key(k.as_str())
                    })
                    .min_by_key(|(_, &tick)| tick)
                    .map(|(k, _)| k.clone())
                    .or_else(|| {
                        self.access_tick
                            .iter()
                            .filter(|(k, _)| {
                                Some(k.as_str()) != protected
                                    && !self.clip_handles.contains_key(k.as_str())
                            })
                            .min_by_key(|(_, &tick)| tick)
                            .map(|(k, _)| k.clone())
                    })
                    .or_else(|| {
                        self.access_tick
                            .iter()
                            .filter(|(k, _)| Some(k.as_str()) != protected)
                            .min_by_key(|(_, &tick)| tick)
                            .map(|(k, _)| k.clone())
                    });
                let Some(victim_key) = victim else {
                    break;
                };
                if !is_cell_unique_key(&victim_key) && !self.warned_shared_eviction {
                    self.warned_shared_eviction = true;
                    log::warn!(
                        "NifImportRegistry: evicting a shared model ('{victim_key}') at its {}-entry cap. \
                         When the streaming ring references more distinct models than that (Fallout 4's \
                         default radius peaks near 9,700), cells whose worker skipped a model as cached \
                         re-parse it on the main thread; raise BYRO_NIF_CACHE_MAX (#6114).",
                        self.max_entries,
                    );
                }
                let _ = self.core.remove(&victim_key);
                self.access_tick.remove(&victim_key);
                // #544 — drop the memoised clip handle in lockstep so
                // a future re-parse of the same key registers a fresh
                // clip rather than reaching into the
                // `AnimationClipRegistry` for a stale handle pointing
                // at a clip that was logically discarded.
                //
                // #863 — capture the freed handle (if any) so the
                // caller can `release()` it into the
                // AnimationClipRegistry. Pre-fix the handle was
                // dropped on the floor and the keyframe arrays
                // leaked.
                if let Some(handle) = self.clip_handles.remove(&victim_key) {
                    freed_clip_handles.push(handle);
                }
                self.evictions = self.evictions.saturating_add(1);
            }
        }
        freed_clip_handles
    }
}

impl Resource for NifImportRegistry {}

#[cfg(test)]
mod beam_chain_doc_pin_tests {
    /// #5285 — nifal.md's declared mesh→medium inventory must name every
    /// converter the `beam_volumes` chain actually runs. #5102's list said
    /// six while the chain had a seventh (`fo4_ambient_lamp_beam_volume_
    /// from_mesh`, FO4's ambient lamp shafts), so the inventory
    /// under-reported which titles lose drawn submeshes on the cell path.
    /// This pins doc ↔ chain in both directions: every chain arm is named
    /// in the doc's beam-converter bullet, and every name in that bullet is
    /// a real chain arm. Scans this file's production text only (the
    /// appended test module is stripped, so the scan cannot match itself).
    #[test]
    fn the_doc_names_every_beam_converter_the_chain_runs() {
        // This file interleaves test modules with production code (two
        // before the chain), so the first-cut production_text would drop
        // the chain — strip every test module instead (#5650's lesson).
        let registry = byroredux_core::source_scan::strip_test_modules(include_str!(
            "nif_import_registry.rs"
        ));
        let chain = registry
            .split_once("beam_volumes.get_or_init")
            .and_then(|(_, rest)| rest.split_once(".collect()"))
            .expect("the beam_volumes chain must keep its get_or_init…collect shape")
            .0;
        let arm_prefix = format!("{}{}", "crate::fog::", "");
        let mut chain_arms: Vec<String> = Vec::new();
        for chunk in chain.split(arm_prefix.as_str()).skip(1) {
            let name: String = chunk
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if name.ends_with("_beam_volume_from_mesh")
                || name.ends_with("_beam_volumes_from_mesh")
            {
                chain_arms.push(name);
            }
        }
        assert!(
            chain_arms.len() >= 7,
            "the chain-scan found only {} arms — the parse broke, not the doc",
            chain_arms.len()
        );

        let doc = include_str!("../../../docs/engine/nifal.md");
        let bullet = doc
            .split_once("asset-signature beam converters")
            .and_then(|(_, rest)| rest.split_once("\n- the older fog-token"))
            .expect(
                "nifal.md must keep the beam-converter bullet bounded by the \
                 fog-token bullet that follows it",
            )
            .0;
        let doc_arms: Vec<String> = bullet
            .split('`')
            .skip(1)
            .step_by(2)
            .filter(|t| {
                t.ends_with("_beam_volume_from_mesh") || t.ends_with("_beam_volumes_from_mesh")
            })
            .map(str::to_owned)
            .collect();

        for arm in &chain_arms {
            assert!(
                doc_arms.contains(arm),
                "`{arm}` runs in the beam_volumes chain but nifal.md's converter \
                 list does not name it (#5285)"
            );
        }
        for arm in &doc_arms {
            assert!(
                chain_arms.contains(arm),
                "nifal.md names `{arm}` but the chain never calls it — a stale \
                 doc entry (#5285)"
            );
        }
    }
}
