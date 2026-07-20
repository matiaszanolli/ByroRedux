use super::*;
use crate::components::{AlphaBlend, IsDecalMesh, IsFxMesh};
use byroredux_core::ecs::{ActiveCamera, Camera, GlobalTransform, MeshHandle, World};

fn run_build(world: &World) -> Vec<DrawCommand> {
    let mut draw_commands = Vec::new();
    let mut gpu_lights = Vec::new();
    let mut bone_world = Vec::new();
    let mut skin_offsets = HashMap::new();
    let max_skinned = ((byroredux_renderer::vulkan::scene_buffer::MAX_TOTAL_BONES
        / byroredux_core::ecs::components::MAX_BONES_PER_MESH)
        - 1) as u32;
    let mut skin_slot_pool = byroredux_core::ecs::resources::SkinSlotPool::new(max_skinned);
    let mut material_table = byroredux_renderer::MaterialTable::new();
    let mut water_commands = Vec::new();
    let _ = build_render_data(
        world,
        &mut draw_commands,
        &mut water_commands,
        &mut gpu_lights,
        &mut bone_world,
        &mut skin_offsets,
        &mut skin_slot_pool,
        &mut material_table,
        None,
    );
    draw_commands
}

fn world_with_mesh(is_fx: bool) -> World {
    let mut world = World::new();

    let cam = world.spawn();
    world.insert(cam, Transform::IDENTITY);
    world.insert(cam, GlobalTransform::IDENTITY);
    world.insert(cam, Camera::default());
    world.insert_resource(ActiveCamera(cam));

    let mesh_e = world.spawn();
    world.insert(mesh_e, Transform::IDENTITY);
    world.insert(mesh_e, GlobalTransform::IDENTITY);
    world.insert(mesh_e, MeshHandle(1));
    if is_fx {
        world.insert(mesh_e, IsFxMesh);
    }

    world
}

/// Regression for #1805 (D2-NEW-04): the `IsFxMesh` skip was hoisted from
/// deep inside the per-entity block (after the frustum test and ~12
/// optional-component gets) to immediately after the visibility gate.
/// The hoist must not change which entities get skipped — an FX-tagged
/// entity must still never emit a `DrawCommand`.
#[test]
fn fx_tagged_entity_is_still_skipped_after_hoisting_the_gate() {
    let world = world_with_mesh(true);
    let cmds = run_build(&world);
    assert_eq!(
        cmds.len(),
        0,
        "IsFxMesh entity must be skipped regardless of where the gate runs"
    );
}

/// Regression for #1805 (D2-NEW-04): collapsing the `tq.get(entity)`
/// presence probe and the later re-fetch into a single
/// `let Some(transform) = tq.get(entity) else { continue };` binding must
/// not drop or duplicate the entity's draw — a plain mesh with no
/// optional components still draws exactly once.
#[test]
fn non_fx_entity_draws_exactly_once_with_single_transform_lookup() {
    let world = world_with_mesh(false);
    let cmds = run_build(&world);
    assert_eq!(
        cmds.len(),
        1,
        "the single-lookup transform binding must not drop or duplicate the entity"
    );
}

#[test]
fn authored_decal_is_alpha_composited_without_depth_or_tlas_occlusion() {
    let mut world = world_with_mesh(false);
    let mesh_entity = {
        let meshes = world.query::<MeshHandle>().expect("mesh query");
        let entity = meshes.iter().next().expect("test mesh").0;
        entity
    };
    world.insert(mesh_entity, IsDecalMesh);
    world.insert(
        mesh_entity,
        AlphaBlend {
            src_blend: 6,
            dst_blend: 7,
        },
    );

    let cmds = run_build(&world);
    assert_eq!(cmds.len(), 1);
    assert!(cmds[0].alpha_blend);
    assert_eq!((cmds[0].src_blend, cmds[0].dst_blend), (6, 7));
    assert!(!cmds[0].z_write);
    assert!(!cmds[0].in_tlas);
}
