use super::*;
use crate::components::CellLightingRes;
use byroredux_core::ecs::{ActiveCamera, Camera, GlobalTransform, World};

fn run_view(world: &World) -> RenderFrameView {
    let mut draw_commands = Vec::new();
    let mut gpu_lights = Vec::new();
    let mut bone_palette = Vec::new();
    let mut skin_offsets = HashMap::new();
    let mut palette_scratch = Vec::new();
    let mut material_table = byroredux_renderer::MaterialTable::new();
    let mut water_commands = Vec::new();
    build_render_data(
        world,
        &mut draw_commands,
        &mut water_commands,
        &mut gpu_lights,
        &mut bone_palette,
        &mut skin_offsets,
        &mut palette_scratch,
        &mut material_table,
        None,
    )
}

fn world_with_camera() -> World {
    let mut world = World::new();
    let cam = world.spawn();
    world.insert(cam, Transform::IDENTITY);
    world.insert(cam, GlobalTransform::IDENTITY);
    world.insert(cam, Camera::default());
    world.insert_resource(ActiveCamera(cam));
    world
}

#[test]
fn fnv_authored_curve_propagates_to_frame_view() {
    let mut world = world_with_camera();
    world.insert_resource(CellLightingRes {
        ambient: [0.1, 0.1, 0.1],
        directional_color: [0.0; 3],
        directional_dir: [0.0, 1.0, 0.0],
        is_interior: true,
        fog_color: [0.5, 0.4, 0.3],
        fog_near: 64.0,
        fog_far: 8192.0,
        directional_fade: None,
        // Doc Mitchell's House values per the audit fixture.
        fog_clip: Some(4096.0),
        fog_power: Some(2.0),
        fog_far_color: None,
        fog_max: None,
        light_fade_begin: None,
        light_fade_end: None,
        directional_ambient: None,
        specular_color: None,
        specular_alpha: None,
        fresnel_power: None,
    });
    let view = run_view(&world);
    assert_eq!(view.fog_clip, 4096.0);
    assert_eq!(view.fog_power, 2.0);
}

#[test]
fn missing_curve_yields_zero_so_shader_falls_back_to_linear() {
    let mut world = world_with_camera();
    world.insert_resource(CellLightingRes {
        ambient: [0.1, 0.1, 0.1],
        directional_color: [0.0; 3],
        directional_dir: [0.0, 1.0, 0.0],
        is_interior: true,
        fog_color: [0.5, 0.4, 0.3],
        fog_near: 64.0,
        fog_far: 8192.0,
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
    });
    let view = run_view(&world);
    assert_eq!(view.fog_clip, 0.0, "no curve authored → shader pickup must be 0");
    assert_eq!(view.fog_power, 0.0);
}

#[test]
fn no_cell_lighting_resource_defaults_to_zero() {
    let world = world_with_camera();
    let view = run_view(&world);
    assert_eq!(view.fog_clip, 0.0);
    assert_eq!(view.fog_power, 0.0);
}
