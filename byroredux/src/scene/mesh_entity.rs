//! The one material-to-component sequence every imported mesh takes (#6690).
//!
//! The loose-NIF / NPC path (`scene/nif_loader.rs::spawn_nif_mesh`) and the
//! placed-reference path (`cell_loader/spawn/mesh_instance.rs::spawn_mesh_instance`)
//! carried this sequence as two copies, so every fix had to land twice. A caller
//! now supplies only what really differs ([`MeshEntitySpawn`]): the source
//! material, the resolved paths, and the entity's placement. What stays with a
//! caller afterwards is its own: GPU upload before this step, then skin binding
//! on the NIF path, and the collider and `LightSource` fallbacks on the cell path.
//!
//! [`spawn_mesh_entity`] does the work that needs a device, and hands the rest to
//! [`attach_mesh_components`], which needs only a `World` and so can be driven by
//! a test.

use byroredux_core::ecs::components::material::Material;
use byroredux_core::ecs::components::{
    escalate_small_static_to_clutter, render_layer_with_decal_escalation, RenderLayer,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    Billboard, BillboardMode, GlobalTransform, LocalBound, MeshHandle, Name, Parent, SceneFlags,
    SpeedTreeWind, TextureHandle, Transform, World, WorldBound,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::FixedString;
use byroredux_nif::import::{ImportedMaterial, ImportedMesh, MaterialTextureSet};
use byroredux_renderer::VulkanContext;

use crate::asset_provider::{
    build_material_texture_handles, resolve_texture_with_clamp, TextureProvider,
};
use crate::components::{
    texture_path_is_fx_mesh, IsFxMesh, MaterialTextureDebugInfo, MaterialTextureHandles,
    MaterialTextureSource,
};
use crate::material_translate::{MeshWaterSource, ResolvedPaths};

/// Whether `mesh` is a dismemberment cap, which neither spawn path draws.
///
/// M41.0 Phase 1b.x — FO3 / FNV actor body and outfit NIFs ship dismemberment-cap
/// sub-meshes alongside the visible geometry. The legacy engine shows them only
/// once a body part is severed; with no dismemberment system yet, they would render
/// as bloody geometry at the joints and neck. Identified by their body-part data
/// rather than by name: the old name list (`bodycaps` / `limbcaps` / `meatneck01` /
/// `meathead01`) missed outfit caps such as `outfitf.nif`'s `headmeat` / `bodymeat`,
/// which drew a gore band under every clothed NPC's chin.
///
/// #6691 — the cell path asks too. A placed armor with no world model draws its worn
/// outfit mesh, caps included. No shipped FO3 or FNV reference does
/// (`dismemberment_cap_census_tests`), but nothing stops a mod's.
///
/// Each path asks this before it uploads, not inside [`spawn_mesh_entity`]: by then
/// the geometry is on the GPU and the caller holds a handle nothing would release.
pub(crate) fn is_hidden_dismemberment_cap(mesh: &ImportedMesh) -> bool {
    let is_cap = mesh.is_dismemberment_cap();
    if is_cap {
        log::debug!(
            "Phase 1b.x: skipping dismemberment cap '{}' until dismemberment exists",
            mesh.name.as_deref().unwrap_or(""),
        );
    }
    is_cap
}

/// One imported mesh, ready to become an entity. Its geometry is already on the
/// GPU as `mesh_handle`.
pub(crate) struct MeshEntitySpawn<'a> {
    pub mesh: &'a ImportedMesh,
    /// The raw material to translate: `mesh.material`, or the target of a REFR
    /// material swap on the cell path (#4290).
    pub source_material: &'a ImportedMaterial,
    pub mesh_handle: u32,
    pub paths: ResolvedPaths,
    pub texture_sources: MaterialTextureSet<MaterialTextureSource>,
    /// The cell path's REFR-overlay material flags; 0 on the loose path.
    pub extra_material_flags: u32,
    pub placement: MeshPlacement,
}

/// What the two spawn paths decide for themselves about where a mesh sits and
/// what it hangs from.
pub(crate) struct MeshPlacement {
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    /// The layer before the per-mesh escalations: the REFR's record type on the
    /// cell path, and Architecture on the loose path, which has no REFR.
    pub base_layer: RenderLayer,
    /// World-space bound radius for the small-STAT escalation.
    pub layer_bound_radius: f32,
    pub parent: Option<EntityId>,
    pub name: Option<FixedString>,
    /// `(response, stiffness)` of a SpeedTree mesh.
    pub speedtree_wind: Option<(f32, f32)>,
    pub water: MeshWaterPlacement,
}

/// The transforms [`crate::material_translate::attach_mesh_water`] measures a
/// mesh-authored water volume in.
pub(crate) struct MeshWaterPlacement {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    /// The NIF's authored phantom volume, NIF-root space.
    pub phantom_bounds: Option<([f32; 3], [f32; 3])>,
    /// World transform of the NIF root the phantom bounds are relative to.
    pub root_transform: (Vec3, Quat, f32),
}

/// What [`spawn_mesh_entity`] resolved for [`attach_mesh_components`].
struct ResolvedMeshMaterial {
    material: Material,
    mirror_pane_forced: bool,
    textures: MaterialTextureSet<Option<String>>,
    texture_sources: MaterialTextureSet<MaterialTextureSource>,
    tex_handle: u32,
    texture_handles: MaterialTextureHandles,
}

/// Translate the material, resolve its textures, and spawn the mesh's entity.
pub(crate) fn spawn_mesh_entity(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    spawn: MeshEntitySpawn<'_>,
) -> EntityId {
    let MeshEntitySpawn {
        mesh,
        source_material,
        mesh_handle,
        paths,
        mut texture_sources,
        extra_material_flags,
        placement,
    } = spawn;
    let textures = paths.textures.clone();

    // Canonical material translation — the single boundary that resolves a raw
    // `ImportedMesh` into the engine `Material` (PBR resolved, glass classified
    // once, flag union packed). See `material_translate.rs`.
    //
    // #2571 / OBL-D5-01 — computed ahead of the texture-clamp resolve just below,
    // so every `texture_clamp_mode`/`src_blend_mode`/`dst_blend_mode` read goes
    // through this one canonical `Material` instead of re-reading the raw
    // material tier at each use site.
    // #5230 — the provenance variant: the spawn-time resolvers need to know when
    // the glass classifier forced a deliberate mirror state, so they do not read
    // its 0.04 floor as a BGSM clamp-floor pin.
    let (material, translate_provenance) =
        crate::material_translate::translate_material_with_provenance(
            source_material,
            mesh.name.as_deref(),
            paths,
            extra_material_flags,
        );

    // Load texture (shared resolve: cache → BSA → fallback).
    // #610 — pass the diffuse-slot `TexClampMode` so the bindless descriptor's
    // sampler picks the matching `VkSamplerAddressMode` pair. CLAMP-authored
    // decals / scope reticles / Oblivion architecture trim no longer render with
    // the legacy REPEAT/REPEAT bleed.
    let tex_handle = resolve_texture_with_clamp(
        ctx,
        tex_provider,
        textures.base_color.as_deref(),
        material.texture_clamp_mode,
    );
    // Resolve every secondary semantic role with the SAME authored clamp mode as
    // base colour, and derive the channel-presence flags, through the one shared
    // producer (#4529).
    // #3073 (NIFAL-D1) — read the already-resolved canonical parallax values
    // instead of re-deriving `.unwrap_or(0.04)` / `.unwrap_or(4.0)` from the raw
    // material tier.
    let mut texture_handles = build_material_texture_handles(
        ctx,
        tex_provider,
        &textures,
        tex_handle,
        material.texture_clamp_mode,
        material.parallax_height_scale,
        material.parallax_max_passes,
    );
    // Prototype: a tiling wall or rock face that authors no height map gets one
    // integrated from its normal map. Off unless `render.static_parallax_depth_m`
    // is above 0; see `static_height`.
    crate::static_height::bind_synthetic_height(
        world,
        ctx,
        tex_provider,
        mesh,
        source_material,
        &material,
        &textures,
        &mut texture_handles,
        &mut texture_sources,
    );

    let entity = world.spawn();
    world.insert(entity, MeshHandle(mesh_handle));
    attach_mesh_components(
        world,
        entity,
        mesh,
        source_material,
        placement,
        ResolvedMeshMaterial {
            material,
            mirror_pane_forced: translate_provenance.mirror_pane_forced,
            textures,
            texture_sources,
            tex_handle,
            texture_handles,
        },
    );
    entity
}

/// Every component of a mesh entity but its `MeshHandle`.
fn attach_mesh_components(
    world: &mut World,
    entity: EntityId,
    mesh: &ImportedMesh,
    source_material: &ImportedMaterial,
    placement: MeshPlacement,
    resolved: ResolvedMeshMaterial,
) {
    let MeshPlacement {
        transform,
        global_transform,
        base_layer,
        layer_bound_radius,
        parent,
        name,
        speedtree_wind,
        water,
    } = placement;
    let ResolvedMeshMaterial {
        material,
        mirror_pane_forced,
        textures,
        texture_sources,
        tex_handle,
        texture_handles,
    } = resolved;

    world.insert(entity, transform);
    world.insert(entity, global_transform);

    // #1213 / D1-NEW-02 — seed LocalBound from the mesh-local bounding sphere
    // (`ImportedMesh.local_bound_center`, `.local_bound_radius`, both extracted
    // by the NIF importer from `NiTriShapeData.center` / `BsTriShape.center` or
    // computed from vertex positions). The bounds-propagation system reads this
    // row and produces a world-space `WorldBound` each frame.
    let local_bound_center = Vec3::new(
        mesh.local_bound_center[0],
        mesh.local_bound_center[1],
        mesh.local_bound_center[2],
    );
    world.insert(
        entity,
        LocalBound::new(local_bound_center, mesh.local_bound_radius),
    );
    // The propagation pass only *updates* a pre-existing `WorldBound` row — it
    // does not insert one — so a missing seed row means the entity stays a zero
    // sphere and is invisible to ray-cast picking, frustum culling, and the
    // skinned-LRU bounds heuristic. It overwrites this `ZERO` on the next tick.
    world.insert(entity, WorldBound::ZERO);

    // #222 / #1235 — carry `NiAVObject.flags` across: gameplay systems branch on
    // DISABLE_SORTING / SELECTIVE_UPDATE / DISPLAY_OBJECT bits without touching
    // the NIF source. APP_CULLED shapes never reach this point (filtered
    // import-side in `walk/mod.rs`).
    if mesh.flags != 0 {
        world.insert(entity, SceneFlags::from_nif(mesh.flags));
    }
    // #2206 / #2527 — attach to the mesh itself. The flat cell walk spawns no
    // node entities, so the nearest-ancestor `NiBillboardNode` mode rides on the
    // mesh. On the loose path the node entity does exist, but
    // `make_billboard_system` writes `GlobalTransform.rotation` directly on it,
    // which transform propagation never re-walks into, so the rotation would
    // never reach a child mesh.
    if let Some(raw) = mesh.billboard_mode {
        world.insert(entity, Billboard::new(BillboardMode::from_nif(raw)));
    }
    if let Some((response, stiffness)) = speedtree_wind {
        world.insert(entity, SpeedTreeWind::new(response, stiffness));
    }
    // #544 — the Parent/Children edge is how an embedded animation clip's subtree
    // walk discovers this mesh.
    if let Some(parent) = parent {
        world.insert(entity, Parent(parent));
        crate::helpers::add_child(world, parent, entity);
    }
    // The clip's node-keyed channels resolve through `build_subtree_name_map` to
    // this entity by this name.
    if let Some(sym) = name {
        world.insert(entity, Name(sym));
    }
    world.insert(entity, TextureHandle(tex_handle));

    // `world.insert` moves `material`, so pull copies of the small Copy fields
    // the rest of this function still reads (#2571).
    let material_kind = material.material_kind;
    let mesh_water = material.is_water_shader;
    let canonical_clamp_mode = material.texture_clamp_mode;
    let canonical_src_blend_mode = material.src_blend_mode;
    let canonical_dst_blend_mode = material.dst_blend_mode;
    world.insert(entity, material);
    // PERF-D3-NEW-02 / #1136 — classify FX-decoration meshes at spawn time so
    // build_render_data can skip them via a component query instead of running 6
    // substring scans per draw per frame.
    if let Some(ref base_color) = textures.base_color {
        if texture_path_is_fx_mesh(base_color, material_kind) {
            world.insert(entity, IsFxMesh);
        }
    }
    world.insert(entity, texture_handles);
    if mesh_water {
        crate::material_translate::attach_mesh_water(
            world,
            entity,
            texture_handles.textures.normal,
            texture_handles.textures.flow,
            MeshWaterSource {
                name: mesh.name.as_deref(),
                positions: &mesh.positions,
                indices: &mesh.indices,
                position: water.position,
                rotation: water.rotation,
                scale: water.scale,
                local_bound_center,
                local_bound_radius: mesh.local_bound_radius,
                phantom_bounds: water.phantom_bounds,
                root_transform: water.root_transform,
            },
        );
    }
    world.insert(
        entity,
        MaterialTextureDebugInfo {
            paths: textures,
            sources: texture_sources,
            clamp_mode: canonical_clamp_mode,
        },
    );
    // #1480 / REN-D22-NEW-01 — resolve the normal-alpha-as-spec roughness ONCE
    // into the canonical Material now that MaterialTextureHandles is attached,
    // instead of recomputing it per draw in the render path. Reads the same
    // components the renderer reads, so the value is identical — only canonical +
    // tooling-visible.
    // #2606 — pass the "a real BGSM authored the PBR scalars" signal so the legacy
    // fallback cannot clobber them.
    crate::material_translate::resolve_normal_alpha_spec_roughness(
        world,
        entity,
        source_material.bgsm_pbr_scalars_authored,
    );
    // #2826 (REN-D19-02) — same "resolve once from MaterialTextureHandles"
    // pattern, for whether the model-space normal map's blue channel carries
    // authored Z.
    crate::material_translate::resolve_msn_z_source(world, entity);
    // #3905 (NIFAL-2026-09-05-D1-01) — same pattern again, for a BGSM pinned at
    // the near-mirror clamp floor whose authored gloss map did not resolve. Must
    // run AFTER MaterialTextureHandles is attached: the whole point is to use the
    // shader's resolved-handle predicate rather than the merge boundary's
    // authored-path one.
    // #5230 — exempt the classifier's forced mirror panes: their 0.04 is
    // deliberate, not a clamp-floor pin.
    crate::material_translate::resolve_unresolved_gloss_neutral_roughness(
        world,
        entity,
        source_material.bgsm_pbr_scalars_authored,
        mirror_pane_forced,
    );
    // #2490 — the blend/decal/facing markers derive from the raw
    // `ImportedMaterial` at the same single boundary the `Material` literal does.
    crate::material_translate::attach_blend_and_facing_markers(
        world,
        entity,
        source_material,
        canonical_src_blend_mode,
        canonical_dst_blend_mode,
    );
    // #renderlayer — derive the per-entity content-class layer. Two per-mesh
    // signals escalate the base layer, so a coplanar overlay wins its z-fight
    // against the surface beneath:
    //   - `is_decal` (NIF-flagged decals — blood splats, scorch marks) →
    //     `RenderLayer::Decal`, the strongest bias;
    //   - `alpha_test` (alpha-tested rugs / posters / fences / cutout foliage) →
    //     `RenderLayer::Clutter`, a gentle bias, and only when the base was
    //     Architecture.
    //
    // #2446 (MAT-D3-04) — the second signal is `alpha_test`, not
    // `alpha_test_func`: the func defaults to `6` (GREATEREQUAL) on every imported
    // material whether or not testing is on, so gating on it would escalate every
    // architectural mesh in the cell (see
    // `alpha_test_disabled_does_not_escalate_regardless_of_default_func`).
    //
    // Small-STAT escalation runs first so decorative clutter authored as STAT
    // (paper piles, folders, clipboards on desks — Bethesda's record-type
    // classifier can't tell these from architectural STATs without spatial
    // extent) gets the Clutter bias before the decal gate sees it. Decal
    // escalation still wins for alpha-tested overlays and NIF-flagged decals
    // regardless of size.
    //
    // The post-escalation layer is the RENDER-z-bias signal only. NPC body / head
    // / armor meshes overwrite it with Actor in
    // `npc_spawn::tag_descendants_as_actor` after the spawn returns, and #1294
    // keeps the cell path's collision fallback on its pre-escalation `base_layer`.
    let layer = escalate_small_static_to_clutter(base_layer, layer_bound_radius);
    let layer = render_layer_with_decal_escalation(
        layer,
        source_material.is_decal,
        source_material.alpha_test,
    );
    world.insert(entity, layer);
}

/// #6690 — the shared step driven directly. `spawn_mesh_entity` itself needs a
/// device, but everything it decides about components happens here.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{AlphaBlend, IsDecalMesh, NoSorter, TwoSided};
    use byroredux_core::ecs::components::water::{WaterPlane, WaterVolume};
    use byroredux_core::ecs::Children;
    use byroredux_core::string::StringPool;

    const LARGE_BOUND: f32 = 1000.0;

    fn mesh(local_bound_radius: f32) -> ImportedMesh {
        let mut mesh = ImportedMesh::from_geometry(
            vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0]],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![0, 1, 2],
        );
        mesh.local_bound_center = [1.0, 2.0, 3.0];
        mesh.local_bound_radius = local_bound_radius;
        mesh
    }

    fn placement() -> MeshPlacement {
        MeshPlacement {
            transform: Transform::new(Vec3::new(7.0, 0.0, 0.0), Quat::IDENTITY, 2.0),
            global_transform: GlobalTransform::new(Vec3::new(9.0, 0.0, 0.0), Quat::IDENTITY, 3.0),
            base_layer: RenderLayer::Architecture,
            layer_bound_radius: LARGE_BOUND,
            parent: None,
            name: None,
            speedtree_wind: None,
            water: MeshWaterPlacement {
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: 1.0,
                phantom_bounds: None,
                root_transform: (Vec3::ZERO, Quat::IDENTITY, 1.0),
            },
        }
    }

    fn resolved(material: Material) -> ResolvedMeshMaterial {
        ResolvedMeshMaterial {
            material,
            mirror_pane_forced: false,
            textures: MaterialTextureSet::default(),
            texture_sources: MaterialTextureSet::default(),
            tex_handle: 11,
            texture_handles: MaterialTextureHandles {
                textures: MaterialTextureSet::default(),
                normal_has_alpha: false,
                tint_has_alpha: false,
                parallax_height_scale: 0.04,
                parallax_max_passes: 4.0,
            },
        }
    }

    fn attach(
        mesh: &ImportedMesh,
        source: &ImportedMaterial,
        placement: MeshPlacement,
        resolved: ResolvedMeshMaterial,
    ) -> (World, EntityId) {
        let mut world = World::new();
        let entity = world.spawn();
        attach_mesh_components(&mut world, entity, mesh, source, placement, resolved);
        (world, entity)
    }

    #[test]
    fn every_mesh_gets_the_core_components_and_no_optional_ones() {
        let mesh = mesh(LARGE_BOUND);
        let (world, entity) = attach(
            &mesh,
            &ImportedMaterial::default(),
            placement(),
            resolved(Material::default()),
        );

        let transform = world.get::<Transform>(entity).unwrap();
        assert_eq!((transform.translation.x, transform.scale), (7.0, 2.0));
        let global = world.get::<GlobalTransform>(entity).unwrap();
        assert_eq!((global.translation.x, global.scale), (9.0, 3.0));
        let bound = world.get::<LocalBound>(entity).unwrap();
        assert_eq!(bound.center, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(bound.radius, LARGE_BOUND);
        assert!(world.get::<WorldBound>(entity).is_some());
        assert_eq!(world.get::<TextureHandle>(entity).unwrap().0, 11);
        assert!(world.get::<Material>(entity).is_some());
        assert!(world.get::<MaterialTextureHandles>(entity).is_some());
        assert!(world.get::<MaterialTextureDebugInfo>(entity).is_some());
        assert_eq!(
            *world.get::<RenderLayer>(entity).unwrap(),
            RenderLayer::Architecture
        );

        assert!(world.get::<SceneFlags>(entity).is_none());
        assert!(world.get::<Billboard>(entity).is_none());
        assert!(world.get::<SpeedTreeWind>(entity).is_none());
        assert!(world.get::<Parent>(entity).is_none());
        assert!(world.get::<Name>(entity).is_none());
        assert!(world.get::<IsFxMesh>(entity).is_none());
        assert!(world.get::<AlphaBlend>(entity).is_none());
        assert!(world.get::<WaterPlane>(entity).is_none());
    }

    #[test]
    fn what_a_caller_or_mesh_asks_for_is_attached() {
        let mut mesh = mesh(LARGE_BOUND);
        mesh.flags = 0x20;
        mesh.billboard_mode = Some(0);
        let mut pool = StringPool::new();
        let sym = pool.intern("Probe");

        let mut world = World::new();
        let parent = world.spawn();
        let entity = world.spawn();
        let mut resolved = resolved(Material::default());
        resolved.textures.base_color = Some(r"textures\effects\fxsoftglow.dds".to_string());
        attach_mesh_components(
            &mut world,
            entity,
            &mesh,
            &ImportedMaterial::default(),
            MeshPlacement {
                parent: Some(parent),
                name: Some(sym),
                speedtree_wind: Some((1.0, 0.0)),
                ..placement()
            },
            resolved,
        );

        assert!(world.get::<SceneFlags>(entity).is_some());
        assert!(world.get::<Billboard>(entity).is_some());
        assert!(world.get::<SpeedTreeWind>(entity).is_some());
        assert_eq!(world.get::<Parent>(entity).unwrap().0, parent);
        assert_eq!(world.get::<Children>(parent).unwrap().0, [entity]);
        assert_eq!(world.get::<Name>(entity).unwrap().0, sym);
        assert!(world.get::<IsFxMesh>(entity).is_some());
    }

    /// #2490 / #4290 — the markers and the layer come from `source_material`, the
    /// REFR material-swap target on the cell path, not from the mesh's own material.
    #[test]
    fn markers_and_layer_read_the_source_material_not_the_meshes_own() {
        let mesh = mesh(LARGE_BOUND);
        let source = ImportedMaterial {
            has_alpha: true,
            is_decal: true,
            two_sided: true,
            no_sorter: true,
            ..ImportedMaterial::default()
        };
        let material = Material {
            src_blend_mode: 3,
            dst_blend_mode: 9,
            ..Material::default()
        };
        let (world, entity) = attach(&mesh, &source, placement(), resolved(material));

        let blend = world
            .get::<AlphaBlend>(entity)
            .expect("AlphaBlend from the source");
        assert_eq!((blend.src_blend, blend.dst_blend), (3, 9));
        assert!(world.get::<IsDecalMesh>(entity).is_some());
        assert!(world.get::<TwoSided>(entity).is_some());
        assert!(world.get::<NoSorter>(entity).is_some());
        assert_eq!(
            *world.get::<RenderLayer>(entity).unwrap(),
            RenderLayer::Decal
        );
    }

    /// #6817 — the synthetic-height gate must be handed the entity's carried
    /// material (`source_material`), not left to read `mesh.material`: the same
    /// bug class #6690 guards for the markers. The behavioural half is
    /// `static_height::tests::gate_reads_the_carried_material_not_the_meshes_own`;
    /// this pins that the shared step actually passes it.
    #[test]
    fn the_synthetic_height_gate_is_handed_the_source_material() {
        let production =
            byroredux_core::source_scan::production_text(include_str!("mesh_entity.rs"));
        let call = production
            .split("bind_synthetic_height(")
            .nth(1)
            .expect("spawn_mesh_entity calls bind_synthetic_height");
        let args = call.split(");").next().unwrap();
        assert!(
            args.contains("source_material"),
            "bind_synthetic_height must receive source_material: {args}"
        );
    }

    /// The caller's base layer and bound radius drive the escalation: the cell path
    /// passes a REFR-scaled radius, so a small mesh on a large REFR stays Architecture.
    #[test]
    fn the_layer_escalates_from_the_callers_base_and_radius() {
        let source = ImportedMaterial::default();
        let layer_for = |base_layer, layer_bound_radius| {
            let mesh = mesh(1.0);
            let (world, entity) = attach(
                &mesh,
                &source,
                MeshPlacement {
                    base_layer,
                    layer_bound_radius,
                    ..placement()
                },
                resolved(Material::default()),
            );
            let layer = *world.get::<RenderLayer>(entity).unwrap();
            layer
        };
        assert_eq!(
            layer_for(RenderLayer::Architecture, 1.0),
            RenderLayer::Clutter
        );
        assert_eq!(
            layer_for(RenderLayer::Architecture, LARGE_BOUND),
            RenderLayer::Architecture
        );
        assert_eq!(layer_for(RenderLayer::Actor, 1.0), RenderLayer::Actor);
    }

    #[test]
    fn debug_info_records_the_paths_sources_and_clamp() {
        let mesh = mesh(LARGE_BOUND);
        let mut resolved = resolved(Material {
            texture_clamp_mode: 2,
            ..Material::default()
        });
        resolved.textures.normal = Some(r"textures\wall_n.dds".to_string());
        resolved.texture_sources.normal = MaterialTextureSource::DerivedNormal;
        let (world, entity) = attach(&mesh, &ImportedMaterial::default(), placement(), resolved);

        let info = world.get::<MaterialTextureDebugInfo>(entity).unwrap();
        assert_eq!(info.paths.normal.as_deref(), Some(r"textures\wall_n.dds"));
        assert!(matches!(
            info.sources.normal,
            MaterialTextureSource::DerivedNormal
        ));
        assert_eq!(info.clamp_mode, 2);
    }

    /// `is_water_shader` is the one gate, and the volume is measured in the
    /// transforms the caller supplied: the loose path's and the cell path's differ.
    #[test]
    fn mesh_water_attaches_and_is_measured_in_the_callers_transforms() {
        let water_material = || Material {
            is_water_shader: true,
            ..Material::default()
        };
        let mesh = mesh(LARGE_BOUND);
        let source = ImportedMaterial::default();

        let at_position = MeshPlacement {
            water: MeshWaterPlacement {
                position: Vec3::new(500.0, 0.0, 0.0),
                ..placement().water
            },
            ..placement()
        };
        let (world, entity) = attach(&mesh, &source, at_position, resolved(water_material()));
        assert!(world.get::<WaterPlane>(entity).is_some());
        let volume = world.get::<WaterVolume>(entity).expect("a measured volume");
        assert!(volume.min[0] >= 490.0, "min {:?}", volume.min);

        let from_phantom = MeshPlacement {
            water: MeshWaterPlacement {
                phantom_bounds: Some(([0.0; 3], [2.0; 3])),
                root_transform: (Vec3::new(100.0, 0.0, 0.0), Quat::IDENTITY, 1.0),
                ..placement().water
            },
            ..placement()
        };
        let (world, entity) = attach(&mesh, &source, from_phantom, resolved(water_material()));
        let volume = world
            .get::<WaterVolume>(entity)
            .expect("the authored volume");
        assert!(
            (99.0..=103.0).contains(&volume.min[0]),
            "the phantom bounds sit at the root transform, min {:?}",
            volume.min
        );
    }

    /// The resolvers run after `MaterialTextureHandles` is attached and read what
    /// they are given. Fixture from
    /// `resolve_normal_alpha_spec_roughness_overwrites_classifier_roughness_after_translate`.
    #[test]
    fn the_roughness_resolvers_run_with_the_source_materials_signals() {
        let mesh = mesh(LARGE_BOUND);
        let material = || Material {
            material_kind: 0,
            metalness: 0.1,
            roughness: 0.6,
            specular_strength: 2.5,
            env_map_scale: 0.0,
            ..Material::default()
        };
        let with_normal = |material| {
            let mut resolved = resolved(material);
            resolved.texture_handles.textures.normal = 42;
            resolved
        };

        let (world, entity) = attach(
            &mesh,
            &ImportedMaterial::default(),
            placement(),
            with_normal(material()),
        );
        let roughness = world.get::<Material>(entity).unwrap().roughness;
        assert!(
            (roughness - 0.70).abs() < 1e-5,
            "resolver result {roughness}"
        );

        // #2606 — a BGSM-authored roughness is not clobbered.
        let authored = ImportedMaterial {
            bgsm_pbr_scalars_authored: true,
            ..ImportedMaterial::default()
        };
        let (world, entity) = attach(&mesh, &authored, placement(), with_normal(material()));
        assert_eq!(world.get::<Material>(entity).unwrap().roughness, 0.6);
    }

    /// #5230 — a classifier-forced mirror keeps its 0.04 floor, and without the
    /// provenance bit that value reads as a BGSM clamp-floor pin and is neutralised.
    #[test]
    fn mirror_pane_provenance_reaches_the_gloss_resolver() {
        let mesh = mesh(LARGE_BOUND);
        let authored = ImportedMaterial {
            bgsm_pbr_scalars_authored: true,
            ..ImportedMaterial::default()
        };
        let roughness_with = |mirror_pane_forced| {
            let mut resolved = resolved(Material {
                roughness: 0.04,
                ..Material::default()
            });
            resolved.mirror_pane_forced = mirror_pane_forced;
            let (world, entity) = attach(&mesh, &authored, placement(), resolved);
            let roughness = world.get::<Material>(entity).unwrap().roughness;
            roughness
        };
        assert_eq!(roughness_with(true), 0.04);
        assert_ne!(roughness_with(false), 0.04);
    }

    /// #2826 — a model-space normal map with authored alpha sets the flag.
    #[test]
    fn the_msn_resolver_runs() {
        use byroredux_renderer::vulkan::material::material_flag;
        let mesh = mesh(LARGE_BOUND);
        let mut resolved = resolved(Material {
            effect_shader_flags: material_flag::MODEL_SPACE_NORMALS,
            ..Material::default()
        });
        resolved.texture_handles.normal_has_alpha = true;
        let (world, entity) = attach(&mesh, &ImportedMaterial::default(), placement(), resolved);
        assert_ne!(
            world.get::<Material>(entity).unwrap().effect_shader_flags
                & material_flag::MSN_HAS_AUTHORED_Z,
            0
        );
    }

    /// #6692 — what a test of the shared step cannot see is whether the spawn
    /// paths still use it, because both need a device. So this pins the shape:
    /// each path calls the step once and attaches nothing to the mesh entity
    /// itself, and the step keeps the two boundaries it owns. A path that grows
    /// its own copy of a component or a material read fails here, which is the
    /// drift #2490, #4529 and #2444/#2300 each had to close by hand.
    #[test]
    fn both_spawn_paths_attach_through_the_shared_step() {
        use byroredux_core::source_scan::function_text;

        let shared_step = function_text(
            include_str!("mesh_entity.rs"),
            "pub(crate) fn spawn_mesh_entity(",
        );
        for owned in [
            "translate_material_with_provenance(",
            "resolve_texture_with_clamp(",
            "build_material_texture_handles(",
        ] {
            assert_eq!(
                shared_step.matches(owned).count(),
                1,
                "spawn_mesh_entity must call `{owned}` exactly once"
            );
        }

        for (label, path) in [
            (
                "spawn_nif_mesh",
                function_text(include_str!("nif_loader.rs"), "fn spawn_nif_mesh("),
            ),
            (
                "spawn_mesh_instance",
                function_text(
                    include_str!("../cell_loader/spawn/mesh_instance.rs"),
                    "pub(super) fn spawn_mesh_instance(",
                ),
            ),
        ] {
            assert_eq!(
                path.matches("spawn_mesh_entity(").count(),
                1,
                "{label} must attach its mesh through spawn_mesh_entity, once"
            );
            for own in [
                "world.insert(entity,",
                "attach_blend_and_facing_markers(",
                "attach_mesh_water(",
                "build_material_texture_handles(",
                "handle_has_alpha(",
                "resolve_texture_with_clamp(",
                "translate_material",
                "AlphaBlend {",
            ] {
                assert!(
                    !path.contains(own),
                    "{label} holds its own `{own}`: this belongs in the shared step, or the \
                     two paths can drift apart again (#6690)"
                );
            }
        }
    }

    #[test]
    fn caps_are_hidden_and_other_skinned_meshes_are_not() {
        let with_parts = |triangle_body_parts| {
            let mut mesh = mesh(LARGE_BOUND);
            mesh.skin = Some(byroredux_nif::import::ImportedSkin {
                triangle_body_parts,
                ..Default::default()
            });
            mesh
        };
        assert!(is_hidden_dismemberment_cap(&with_parts(vec![101, 201])));
        assert!(!is_hidden_dismemberment_cap(&with_parts(vec![0, 101])));
        assert!(!is_hidden_dismemberment_cap(&mesh(LARGE_BOUND)));
    }

    /// #6691 — the two paths used to differ here: only the NIF path filtered caps.
    /// The gate has to come before the first registry call on both, because a mesh
    /// that is uploaded or acquired and then skipped leaks its handle. Text, because
    /// both upload stages need a device.
    #[test]
    fn both_spawn_paths_hide_caps_before_touching_the_mesh_registry() {
        use byroredux_core::source_scan::function_text;

        for (label, body) in [
            (
                "spawn_nif_mesh",
                function_text(include_str!("nif_loader.rs"), "fn spawn_nif_mesh("),
            ),
            (
                "prepare_mesh_upload_range",
                function_text(
                    include_str!("../cell_loader/spawn/mesh_instance.rs"),
                    "pub(super) fn prepare_mesh_upload_range(",
                ),
            ),
        ] {
            let gate = body
                .find("is_hidden_dismemberment_cap(mesh)")
                .unwrap_or_else(|| panic!("{label} must ask the shared cap gate"));
            let registry = body
                .find("ctx.mesh_registry")
                .unwrap_or_else(|| panic!("{label} no longer reaches the mesh registry"));
            assert!(
                gate < registry,
                "{label} must hide caps before it reaches the mesh registry"
            );
        }
    }
}
