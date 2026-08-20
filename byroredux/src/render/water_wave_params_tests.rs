//! Water-material GPU handoff regressions. Authored WATR wave parameters
//! and sun-specular power must survive canonical translation and reach the
//! exact `GpuWaterParams` slots consumed by `water.frag`.

use super::*;
use byroredux_core::ecs::components::groundcover::WindField;
use byroredux_core::ecs::components::water::{
    WaterFlow, WaterKind, WaterMaterial, WaterPlane,
};
use byroredux_core::ecs::{ActiveCamera, Camera, GlobalTransform, MeshHandle, World};
use byroredux_scripting::RippleEvent;

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
                underwater_color: [0.12, 0.24, 0.36],
                underwater_fog_near: 14.0,
                underwater_fog_far: 260.0,
                underwater_fog_amount: 0.8,
                ..WaterMaterial::default()
            },
            damage_per_second: 0.0,
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
    assert_eq!([params.scroll_c[2], params.scroll_c[3]], [14.0, 260.0]);
    assert_eq!(params.underwater, [0.12, 0.24, 0.36, 0.8]);
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

#[test]
fn weather_wind_reaches_water_scroll_alongside_speedtree_wind_field() {
    let mut world = world_with_water_plane(
        0.05,
        0.6,
        50.0,
        1.0 / 512.0,
        [1.0; 3],
        [1.0; 4],
        [0.0, 0.0, 1.0, 1.0],
    );
    let calm = run_build(&world);
    world.insert_resource(WindField {
        direction: [1.0, 0.0],
        speed: 100.0,
        gust_amplitude: 0.0,
        gust_frequency: 0.0,
    });
    let windy = run_build(&world);
    assert!(windy[0].params.scroll[0] > calm[0].params.scroll[0]);
    assert_eq!(windy[0].params.scroll[1], calm[0].params.scroll[1]);
    assert!(windy[0].params.tune[3] > calm[0].params.tune[3]);
    let expected_scale = 1.0 + (100.0 / 220.0) * 0.5;
    assert!((windy[0].params.tune[3] - 0.05 * expected_scale).abs() < 1e-6);
}

#[test]
fn non_unit_weather_direction_is_normalized_for_water_scroll() {
    let mut world = world_with_water_plane(
        0.05,
        0.6,
        50.0,
        1.0 / 512.0,
        [1.0; 3],
        [1.0; 4],
        [0.0, 0.0, 1.0, 1.0],
    );
    let calm = run_build(&world);
    world.insert_resource(WindField {
        direction: [3.0, 4.0],
        speed: 100.0,
        gust_amplitude: 0.0,
        gust_frequency: 0.0,
    });

    let params = &run_build(&world)[0].params;
    let expected = 100.0 * 0.0015;
    assert!((params.scroll[0] - calm[0].params.scroll[0] - expected * 0.6).abs() < 1e-6);
    assert!((params.scroll[1] - calm[0].params.scroll[1] - expected * 0.8).abs() < 1e-6);
}

#[test]
fn non_finite_weather_gust_keeps_water_params_finite() {
    let mut world = world_with_water_plane(
        0.05,
        0.6,
        50.0,
        1.0 / 512.0,
        [1.0; 3],
        [1.0; 4],
        [0.0, 0.0, 1.0, 1.0],
    );
    world.insert_resource(WindField {
        direction: [1.0, 0.0],
        speed: f32::NAN,
        gust_amplitude: 10.0,
        gust_frequency: 1.0,
    });

    let params = &run_build(&world)[0].params;
    assert!(params.scroll.iter().all(|value| value.is_finite()));
    assert!(params.tune[3].is_finite());
}

#[test]
fn authored_flow_direction_reaches_gpu_flow_and_scroll_payload() {
    let mut world = world_with_water_plane(
        0.05,
        0.6,
        50.0,
        1.0 / 512.0,
        [1.0; 3],
        [1.0; 4],
        [0.0, 0.0, 1.0, 1.0],
    );
    let water = world
        .query::<WaterPlane>()
        .expect("water plane")
        .iter()
        .next()
        .expect("one water plane")
        .0;
    let mut plane_q = world
        .query_mut::<WaterPlane>()
        .expect("water plane storage");
    let plane = plane_q.get_mut(water).expect("water plane");
    plane.kind = WaterKind::River;
    plane.material.scroll_a = [0.12, 0.0];
    plane.material.scroll_b = [0.0, 0.06];
    drop(plane_q);
    world.insert(
        water,
        WaterFlow {
            direction: [1.0, 0.0, 0.0],
            speed: 8.0,
        },
    );

    let water_commands = run_build(&world);
    let params = &water_commands[0].params;
    assert_eq!(params.flow, [1.0, 0.0, 0.0, 8.0]);
    assert!(params.scroll[0] > params.scroll[1].abs());
    assert!(params.scroll[2].abs() < 1.0e-6);
    assert!(params.scroll[3] > 0.0);
}

#[test]
fn authored_flowmap_scale_changes_visual_scroll_not_flow_physics() {
    let world = world_with_water_plane(
        0.05,
        0.6,
        50.0,
        1.0 / 512.0,
        [1.0; 3],
        [1.0; 4],
        [0.0, 0.0, 1.0, 1.0],
    );
    let water = world
        .query::<WaterPlane>()
        .unwrap()
        .iter()
        .next()
        .map(|(entity, _)| entity)
        .expect("water plane");
    {
        let mut query = world.query_mut::<WaterPlane>().unwrap();
        let material = &mut query.get_mut(water).unwrap().material;
        material.flowmap_scale = 2.5;
        material.scroll_a = [0.2, 0.0];
    }
    let draws = run_build(&world);
    assert_eq!(draws[0].params.scroll[0], 0.5);
    assert_eq!(draws[0].params.flow[3], 0.0);
}

#[test]
fn starfield_absorption_ranges_reach_water_gpu_params() {
    let world = world_with_water_plane(
        0.05,
        0.6,
        50.0,
        1.0 / 512.0,
        [1.0; 3],
        [1.0; 4],
        [0.0, 0.0, 1.0, 1.0],
    );
    let water = world
        .query::<WaterPlane>()
        .unwrap()
        .iter()
        .next()
        .map(|(entity, _)| entity)
        .expect("water plane");
    {
        let mut query = world.query_mut::<WaterPlane>().unwrap();
        query.get_mut(water).unwrap().material.absorption_ranges = [12.0, 34.0, 56.0];
    }
    let draws = run_build(&world);
    assert_eq!(draws[0].params.absorption, [12.0, 34.0, 56.0, 0.0]);
}

#[test]
fn ripple_event_reaches_water_gpu_params() {
    let mut world = world_with_water_plane(
        0.05,
        0.6,
        50.0,
        1.0 / 512.0,
        [1.0; 3],
        [1.0; 4],
        [0.0, 0.0, 1.0, 1.0],
    );
    let water = world
        .query::<WaterPlane>()
        .unwrap()
        .iter()
        .next()
        .map(|(entity, _)| entity)
        .expect("water plane");
    world.insert(
        water,
        RippleEvent {
            actor: water,
            intensity: 0.75,
            position: [10.0, 0.0, 20.0],
        },
    );
    let draws = run_build(&world);
    assert_eq!(draws[0].params.ripple, [10.0, 20.0, 0.75, 19.0]);
}
