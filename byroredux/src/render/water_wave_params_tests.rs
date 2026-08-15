//! Regression for #2240 (REN-D15-02): authored WATR `wave_amplitude` /
//! `wave_frequency` were parsed and round-tripped into the canonical
//! `WaterMaterial` but never reached the GPU — `reemit_water_planes`
//! zeroed the two push-constant slots that now carry them
//! (`WaterPush::tune.w` / `WaterPush::misc.y`).

use super::*;
use byroredux_core::ecs::components::water::{WaterKind, WaterMaterial, WaterPlane};
use byroredux_core::ecs::{ActiveCamera, Camera, GlobalTransform, MeshHandle, World};

fn world_with_water_plane(wave_amplitude: f32, wave_frequency: f32) -> World {
    let mut world = World::new();

    let cam = world.spawn();
    world.insert(cam, Transform::IDENTITY);
    world.insert(cam, GlobalTransform::IDENTITY);
    world.insert(cam, Camera::default());
    world.insert_resource(ActiveCamera(cam));

    let water = world.spawn();
    world.insert(water, Transform::IDENTITY);
    world.insert(water, GlobalTransform::IDENTITY);
    world.insert(water, MeshHandle(1));
    world.insert(
        water,
        WaterPlane {
            kind: WaterKind::Calm,
            material: WaterMaterial {
                wave_amplitude,
                wave_frequency,
                ..WaterMaterial::default()
            },
        },
    );

    world
}

fn run_build(world: &World) -> Vec<byroredux_renderer::vulkan::water::WaterDrawCommand> {
    let mut draw_commands = Vec::new();
    let mut water_commands = Vec::new();
    let mut gpu_lights = Vec::new();
    let mut bone_world = Vec::new();
    let mut skin_offsets = rustc_hash::FxHashMap::default();
    let max_skinned = ((byroredux_renderer::vulkan::scene_buffer::MAX_TOTAL_BONES
        / byroredux_core::ecs::components::MAX_BONES_PER_MESH)
        - 1) as u32;
    let mut skin_slot_pool = byroredux_core::ecs::resources::SkinSlotPool::new(max_skinned);
    let mut material_table = byroredux_renderer::MaterialTable::new();
    let _ = build_render_data(
        world,
        &mut draw_commands,
        &mut water_commands,
        &mut gpu_lights,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut bone_world,
        &mut skin_offsets,
        &mut skin_slot_pool,
        &mut material_table,
        None,
    );
    water_commands
}

#[test]
fn wave_amplitude_and_frequency_reach_the_water_push_constants() {
    let world = world_with_water_plane(1.5, 2.0);
    let water_commands = run_build(&world);

    assert_eq!(water_commands.len(), 1, "expected exactly one water draw");
    let push = &water_commands[0].push;
    assert_eq!(
        push.tune[3], 1.5,
        "WaterMaterial::wave_amplitude must reach WaterPush::tune.w"
    );
    assert_eq!(
        push.misc[1], 2.0,
        "WaterMaterial::wave_frequency must reach WaterPush::misc.y"
    );
}

#[test]
fn default_wave_params_are_the_sentinel_the_shader_normalises_against() {
    // `WaterMaterial::default()` (no XCWT / no WATR) must keep resolving
    // to the sentinel `water.frag` treats as "no chop change" — see
    // `sampleScrollingNormal`'s doc comment (0.05 / 0.6).
    let world = world_with_water_plane(0.05, 0.6);
    let water_commands = run_build(&world);

    let push = &water_commands[0].push;
    assert_eq!(push.tune[3], 0.05);
    assert_eq!(push.misc[1], 0.6);
}
