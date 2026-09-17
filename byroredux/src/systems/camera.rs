//! Fly camera — WASD + mouse look against the active camera.

use byroredux_core::ecs::{ActiveCamera, GlobalTransform, Transform, World};
use byroredux_core::math::{Quat, Vec3};

use crate::components::InputState;
use crate::interaction::{ActionState, InputAction};
use crate::systems::character::PlayerMode;

/// The camera orientation for a gameplay look accumulator, shared by the fly
/// camera, the character camera, and every command or restore path that
/// poses either: `forward = rotation * -Z`, yaw about +Y applied after pitch
/// about +X (positive pitch looks up).
pub(crate) fn camera_look_rotation(yaw: f32, pitch: f32) -> Quat {
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)
}

/// Fly camera system: WASD + mouse look. Updates the active camera's Transform.
///
/// Early-returns when `PlayerMode == Character` so the M28.5 character
/// rig is the sole driver of camera + body motion in player mode.
/// FlyCam stays the default for `--mesh` / `--tree` / `--fly` debug
/// modes.
pub(crate) fn fly_camera_system(world: &World, dt: f32) {
    // M28.5 gate — character mode owns the camera via
    // `camera_follow_system`; the fly camera would fight it for
    // Transform writes.
    let mode = world
        .try_resource::<PlayerMode>()
        .map(|r| *r)
        .unwrap_or_default();
    if mode == PlayerMode::Character {
        return;
    }
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
    let descend = input.keys_held.contains(&winit::keyboard::KeyCode::KeyQ);
    drop(input);

    let Some(actions) = world.try_resource::<ActionState>() else {
        return;
    };

    // Build movement from gameplay actions. Q remains a raw debug-camera
    // descend axis; it has no on-foot gameplay intent to expose.
    let mut move_dir = Vec3::ZERO;
    if actions.is_held(InputAction::MoveForward) {
        move_dir.z += 1.0;
    }
    if actions.is_held(InputAction::MoveBackward) {
        move_dir.z -= 1.0;
    }
    if actions.is_held(InputAction::StrafeLeft) {
        move_dir.x -= 1.0;
    }
    if actions.is_held(InputAction::StrafeRight) {
        move_dir.x += 1.0;
    }
    if actions.is_held(InputAction::Jump) {
        move_dir.y += 1.0;
    }
    if descend {
        move_dir.y -= 1.0;
    }

    // Speed boost uses the same Sprint action as on-foot movement.
    let boost = if actions.is_held(InputAction::Sprint) {
        3.0
    } else {
        1.0
    };
    drop(actions);

    // Build rotation from yaw/pitch.
    let rotation = camera_look_rotation(yaw, pitch);

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

    // The fly-camera system runs after the normal transform-propagation
    // phase.  Its local Transform therefore cannot wait until next frame to
    // reach GlobalTransform: interaction, picking, audio, and the debug
    // camera commands all consume the global pose in this same frame.  The
    // active fly camera is a root entity, so its global pose is exactly its
    // local TRS.
    let local_pose = world
        .query::<Transform>()
        .and_then(|transforms| transforms.get(cam_entity).copied());
    if let (Some(local), Some(mut globals)) = (local_pose, world.query_mut::<GlobalTransform>()) {
        if let Some(global) = globals.get_mut(cam_entity) {
            *global = GlobalTransform::new(local.translation, local.rotation, local.scale);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::{ActiveCamera, GlobalTransform, Transform, World};

    #[test]
    fn fly_camera_publishes_its_current_pose_to_global_transform() {
        let mut world = World::new();
        let camera = world.spawn();
        world.insert(
            camera,
            Transform::from_translation(Vec3::new(10.0, 20.0, 30.0)),
        );
        // Deliberately stale, reproducing the post-propagation benchmark
        // camera state that made a rendered door differ from its interaction
        // raycast.
        world.insert(
            camera,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert_resource(ActiveCamera(camera));
        world.insert_resource(PlayerMode::FlyCam);
        world.insert_resource(ActionState::default());
        world.insert_resource(InputState {
            mouse_captured: true,
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 0.0,
            ..Default::default()
        });

        fly_camera_system(&world, 1.0 / 60.0);

        let local = world.query::<Transform>().unwrap().get(camera).copied().unwrap();
        let global = world
            .query::<GlobalTransform>()
            .unwrap()
            .get(camera)
            .copied()
            .unwrap();
        assert_eq!(global.translation, local.translation);
        assert_eq!(global.rotation, local.rotation);
    }
}
