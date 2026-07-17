//! #2043 / PERF-D9-04 regression guard — `RenderFrameView::render_origin`
//! is computed exactly once (in `camera::assemble_camera`) and threaded
//! through to `FrameInputs`/`context::draw::draw_frame` rather than each
//! consumer independently calling `snap_render_origin` on its own copy of
//! the camera position. This pins the single-source-of-truth invariant at
//! the `RenderFrameView` boundary: the threaded `render_origin` must equal
//! `snap_render_origin` applied to the frame's own `camera_pos`.

use super::*;
use byroredux_core::ecs::{ActiveCamera, Camera, GlobalTransform, World};

fn run_view(world: &World) -> RenderFrameView {
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
    build_render_data(
        world,
        &mut draw_commands,
        &mut water_commands,
        &mut gpu_lights,
        &mut bone_world,
        &mut skin_offsets,
        &mut skin_slot_pool,
        &mut material_table,
        None,
    )
}

fn world_with_camera_at(pos: byroredux_core::math::Vec3) -> World {
    let mut world = World::new();
    let cam = world.spawn();
    world.insert(
        cam,
        Transform::new(pos, byroredux_core::math::Quat::IDENTITY, 1.0),
    );
    world.insert(cam, GlobalTransform::IDENTITY);
    world.insert(cam, Camera::default());
    world.insert_resource(ActiveCamera(cam));
    world
}

/// Camera positioned well past a cell-grid boundary on every axis (and one
/// negative, Markarth-style) so the snap actually moves the origin off
/// zero — a same-origin bug at the identity position (0,0,0) would be
/// invisible.
#[test]
fn render_origin_matches_snap_of_camera_pos() {
    let pos = byroredux_core::math::Vec3::new(9500.0, -4200.0, 100.0);
    let world = world_with_camera_at(pos);
    let frame = run_view(&world);

    let expected = byroredux_renderer::vulkan::scene_buffer::snap_render_origin(
        byroredux_core::math::Vec3::from_array(frame.camera_pos),
    );
    assert_eq!(
        frame.render_origin,
        expected.to_array(),
        "RenderFrameView::render_origin must be the single snap of camera_pos \
         threaded through to FrameInputs — a second, independent \
         snap_render_origin call (e.g. in context::draw::draw_frame) could \
         silently diverge from this one if the shared field regresses"
    );
    // Sanity: the snap actually moved off zero for this position, so the
    // equality above isn't trivially satisfied by both sides being ZERO.
    assert_ne!(frame.render_origin, [0.0, 0.0, 0.0]);
}

/// Camera at the origin — the snap is `[0,0,0]` on both sides, so this
/// guards the degenerate case doesn't accidentally short-circuit true.
#[test]
fn render_origin_is_zero_at_world_origin() {
    let world = world_with_camera_at(byroredux_core::math::Vec3::ZERO);
    let frame = run_view(&world);
    assert_eq!(frame.render_origin, [0.0, 0.0, 0.0]);
}
