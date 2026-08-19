//! Water-material GPU handoff regressions. Authored WATR wave parameters
//! and sun-specular power must survive canonical translation and reach the
//! exact `GpuWaterParams` slots consumed by `water.frag`.

use super::*;
use byroredux_core::ecs::components::water::{WaterKind, WaterMaterial, WaterPlane};
use byroredux_core::ecs::{ActiveCamera, Camera, GlobalTransform, MeshHandle, World};

fn world_with_water_plane(
    wave_amplitude: f32,
    wave_frequency: f32,
    sun_specular_power: f32,
    uv_scale_c: f32,
    noise_amplitude_scales: [f32; 3],
    depth_weights: [f32; 4],
    effect_controls: [f32; 4],
) -> World {
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
                sun_specular_power,
                uv_scale_c,
                noise_amplitude_scales,
                depth_weights,
                effect_controls,
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
fn authored_wave_and_sun_params_reach_the_water_gpu_record() {
    let world = world_with_water_plane(
        1.5,
        2.0,
        73.0,
        1.0 / 488.0,
        [0.7, 0.6, 0.5],
        [0.9, 0.5, 0.1, 0.2],
        [9.0, 500.0, 0.34, 3.2],
    );
    let water_commands = run_build(&world);

    assert_eq!(water_commands.len(), 1, "expected exactly one water draw");
    let params = &water_commands[0].params;
    assert_eq!(
        params.tune[3], 1.5,
        "WaterMaterial::wave_amplitude must reach GpuWaterParams::tune.w"
    );
    assert_eq!(
        params.misc[1], 2.0,
        "WaterMaterial::wave_frequency must reach GpuWaterParams::misc.y"
    );
    assert_eq!(
        params.misc[3], 73.0,
        "WaterMaterial::sun_specular_power must reach GpuWaterParams::misc.w"
    );
    assert_eq!(
        params.detail[0],
        1.0 / 488.0,
        "WaterMaterial::uv_scale_c must reach the authored NAM4 GPU slot"
    );
    assert_eq!(params.detail[1..4], [0.7, 0.6, 0.5]);
    assert_eq!(params.depth, [0.9, 0.5, 0.1, 0.2]);
    assert_eq!(params.effects, [9.0, 500.0, 0.34, 3.2]);
}

#[test]
fn default_wave_params_are_the_sentinel_the_shader_normalises_against() {
    // `WaterMaterial::default()` (no XCWT / no WATR) must keep resolving
    // to the sentinel `water.frag` treats as "no chop change" — see
    // `sampleScrollingNormal`'s doc comment (0.05 / 0.6).
    let world = world_with_water_plane(
        0.05,
        0.6,
        50.0,
        1.0 / 512.0,
        [1.0; 3],
        [1.0; 4],
        [0.0, 0.0, 1.0, 1.0],
    );
    let water_commands = run_build(&world);

    let params = &water_commands[0].params;
    assert_eq!(params.tune[3], 0.05);
    assert_eq!(params.misc[1], 0.6);
    assert_eq!(params.misc[3], 50.0);
}
