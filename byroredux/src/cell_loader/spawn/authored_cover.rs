//! #4413 — authored ground-cover templates (EXAL §12.12 Phase C).
//!
//! Every `GRAS` record in the worldspace's [`AuthoredCover`] draws its own
//! model, instanced by the GPU across the terrain. The instances are written
//! by the renderer, but each shape still needs what a placed object gets
//! from the spawn path: its mesh in the global pools, its textures in the
//! bindless registry and its canonical `Material`, resolved through NIFAL
//! exactly as for any placed reference. So each record's model is spawned
//! once, through [`spawn_mesh_instance`], as a hidden template: its shape
//! entities carry [`AuthoredCoverTemplate`] and are never drawn themselves
//! (see `render::static_meshes`).
//!
//! The templates hang off one pseudo cell root registered in
//! `CellRootIndex`, so leaving the worldspace reclaims their meshes and
//! textures through the ordinary `unload_cells` path.

use super::mesh_instance::{
    prepare_mesh_uploads, resolve_mesh_paths_with_pre_merge, spawn_mesh_instance, PlacementCtx,
};
use super::MissingCollisionFallback;
use crate::asset_provider::{MaterialProvider, TextureProvider};
use crate::cell_loader::load::{register_cell_root, stamp_cell_root_range};
use crate::cell_loader::nif_import_registry::canonical_model_path_key;
use crate::cell_loader::references::parse_and_import_nif_pub;
use crate::components::AuthoredCoverTemplate;
use byroredux_core::ecs::components::groundcover::AuthoredCover;
use byroredux_core::ecs::components::{
    Children, GlobalTransform, MeshHandle, RenderLayer, Transform,
};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_renderer::VulkanContext;

/// Spawn every record's model as a hidden template and return the pseudo
/// cell root that owns them, or `None` when no record's model loaded.
pub(crate) fn spawn_authored_cover_templates(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mat_provider: &mut MaterialProvider,
    cover: &AuthoredCover,
) -> Option<EntityId> {
    let cover_root = world.spawn();
    register_cell_root(world, cover_root);
    let first = world.next_entity_id();
    let mut shapes = 0usize;
    let mut records = 0usize;
    for (record_index, record) in cover.records.iter().enumerate() {
        let spawned = spawn_template(
            world,
            ctx,
            tex_provider,
            mat_provider,
            record_index as u32,
            &record.model_path,
        );
        if spawned > 0 {
            records += 1;
            shapes += spawned;
        } else {
            log::warn!(
                target: "engine::groundcover",
                "authored ground cover: GRAS {:08X} ({}) model '{}' did not load",
                record.form_id,
                record.editor_id,
                record.model_path,
            );
        }
    }
    let last = world.next_entity_id();
    stamp_cell_root_range(world, cover_root, first, last);
    log::info!(
        target: "engine::groundcover",
        "authored ground cover: {records}/{} GRAS models loaded as {shapes} template shapes \
         (grid {} units)",
        cover.records.len(),
        cover.grid_spacing,
    );
    Some(cover_root)
}

/// Spawn one record's model under its own template root; returns how many
/// shapes were tagged.
fn spawn_template(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mat_provider: &mut MaterialProvider,
    record: u32,
    model_path: &str,
) -> usize {
    let cache_key = canonical_model_path_key(model_path);
    let Some(bytes) = tex_provider.extract_mesh(&cache_key) else {
        return 0;
    };
    let cached = {
        let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
        parse_and_import_nif_pub(
            &bytes,
            &cache_key,
            Some(&mut *mat_provider),
            &mut pool,
            Some(tex_provider),
            &|p| tex_provider.has_texture(p),
        )
    };
    let Some(cached) = cached else {
        return 0;
    };

    let root = world.spawn();
    world.insert(root, Transform::IDENTITY);
    world.insert(root, GlobalTransform::IDENTITY);
    let resolved = resolve_mesh_paths_with_pre_merge(
        world,
        &cached.meshes,
        &cached.pre_merge_materials,
        None,
        Some(mat_provider),
        Some(tex_provider),
    );
    let pc = PlacementCtx {
        tex_provider,
        geometry_dedup: &cached.geometry_dedup,
        ref_pos: Vec3::ZERO,
        ref_rot: Quat::IDENTITY,
        ref_scale: 1.0,
        // Vegetation is rooted ground, the layer the spawn path would give a
        // placed plant; small shapes still escalate to Clutter as usual.
        base_layer: RenderLayer::Architecture,
        mesh_cache_key: Some(&cache_key),
        refr_overlay: None,
        light_data: None,
        light_animation_flags: 0,
        light_shadow_flags: 0,
        light_kind: byroredux_core::ecs::LightKind::Point,
        light_direction: [0.0; 3],
        light_outer_angle: 0.0,
        light_falloff_exponent: 1.0,
        placement_root: root,
        // A template is never placed: no collider, fallback or otherwise.
        collision_fallback: MissingCollisionFallback::None,
        spawned_nif_lights: 0,
    };
    let prepared = prepare_mesh_uploads(ctx, &pc, &cached.meshes, &resolved);
    // Templates never enter the TLAS (the tier is receive-only), so the BLAS
    // specs a fresh upload queues are dropped rather than built.
    let mut blas_specs = Vec::new();
    let mut proxy = false;
    for (index, (mesh, prepared)) in cached.meshes.iter().zip(prepared).enumerate() {
        spawn_mesh_instance(
            world,
            ctx,
            &pc,
            &cached,
            mesh,
            &resolved[index],
            index,
            prepared,
            &mut blas_specs,
            &mut proxy,
        );
    }
    let children: Vec<EntityId> = world
        .get::<Children>(root)
        .map(|c| c.0.clone())
        .unwrap_or_default();
    let mut tagged = 0;
    for child in children {
        if world.get::<MeshHandle>(child).is_some() {
            world.insert(child, AuthoredCoverTemplate { record });
            tagged += 1;
        }
    }
    tagged
}
