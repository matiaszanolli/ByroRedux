//! Fly camera — WASD + mouse look against the active camera.

use byroredux_core::ecs::{ActiveCamera, Transform, World};
use byroredux_core::math::{Quat, Vec3};

use crate::components::InputState;

/// Fly camera system: WASD + mouse look. Updates the active camera's Transform.
pub(crate) fn fly_camera_system(world: &World, dt: f32) {
    let Some(active) = world.try_resource::<ActiveCamera>() else {
        return;
    };
    let cam_entity = active.0;
    drop(active);

    let Some(input) = world.try_resource::<InputState>() else {
        return;
    };
    if !input.mouse_captured {
        return;
    }

    let speed = input.move_speed * dt;
    let yaw = input.yaw;
    let pitch = input.pitch;

    // Build movement vector from held keys.
    let mut move_dir = Vec3::ZERO;
    if input.keys_held.contains(&winit::keyboard::KeyCode::KeyW) {
        move_dir.z += 1.0;
    }
    if input.keys_held.contains(&winit::keyboard::KeyCode::KeyS) {
        move_dir.z -= 1.0;
    }
    if input.keys_held.contains(&winit::keyboard::KeyCode::KeyA) {
        move_dir.x -= 1.0;
    }
    if input.keys_held.contains(&winit::keyboard::KeyCode::KeyD) {
        move_dir.x += 1.0;
    }
    if input.keys_held.contains(&winit::keyboard::KeyCode::Space) {
        move_dir.y += 1.0;
    }
    if input
        .keys_held
        .contains(&winit::keyboard::KeyCode::ShiftLeft)
    {
        move_dir.y -= 1.0;
    }

    // Speed boost with Ctrl.
    let boost = if input
        .keys_held
        .contains(&winit::keyboard::KeyCode::ControlLeft)
    {
        3.0
    } else {
        1.0
    };
    drop(input);

    // Build rotation from yaw/pitch.
    let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);

    // Compute desired world-space move vector (yaw-only, so Y stays level).
    let move_world = if move_dir != Vec3::ZERO {
        let dir = move_dir.normalize();
        let forward = Quat::from_rotation_y(yaw) * -Vec3::Z;
        let right = Quat::from_rotation_y(yaw) * Vec3::X;
        let up = Vec3::Y;
        (forward * dir.z + right * dir.x + up * dir.y) * boost
    } else {
        Vec3::ZERO
    };

    // Branch: physics-driven (camera has RapierHandles) vs free-fly fallback.
    let has_physics = world
        .query::<byroredux_physics::RapierHandles>()
        .map(|q| q.contains(cam_entity))
        .unwrap_or(false);

    if has_physics {
        // Always update rotation on the Transform — Rapier Phase 4 only
        // writes translation/rotation for dynamic bodies, but we want the
        // rotation to reflect input instantly.
        if let Some(mut tq) = world.query_mut::<Transform>() {
            if let Some(transform) = tq.get_mut(cam_entity) {
                transform.rotation = rotation;
            }
        }
        // Write linear velocity into the Rapier body. `speed` from
        // InputState is already per-frame — divide out dt to get per-second.
        let velocity_per_sec = if dt > 0.0 { speed / dt } else { 0.0 };
        let v = move_world * velocity_per_sec;
        byroredux_physics::set_linear_velocity(world, cam_entity, v);
    } else if let Some(mut tq) = world.query_mut::<Transform>() {
        if let Some(transform) = tq.get_mut(cam_entity) {
            transform.rotation = rotation;
            if move_world != Vec3::ZERO {
                transform.translation += move_world * speed;
            }
        }
    }
}
