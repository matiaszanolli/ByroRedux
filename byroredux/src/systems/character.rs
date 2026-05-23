//! M28.5 — Kinematic character controller.
//!
//! Drives the player body with gravity + collide-and-slide + jump,
//! and pins the active camera to the body's head joint. Replaces the
//! M28 Phase 1 dynamic-body attempt that fought
//! `physics_sync_system` Phase 4 over Transform writes.
//!
//! Layout:
//!   - [`character_controller_system`] — Stage::Early. WASD →
//!     desired horizontal motion + gravity-integrated vertical motion
//!     → Rapier's `KinematicCharacterController.move_shape` →
//!     corrected motion → Transform + Rapier kinematic next-position.
//!   - [`camera_follow_system`] — Stage::Late (after
//!     `physics_sync_system` settles the body). Camera position =
//!     body Transform + `eye_height * Y`; rotation from
//!     `InputState.yaw + .pitch`.
//!
//! Both systems early-return when [`PlayerEntity`] is unset (engine
//! booted in fly-cam mode or pre-character-spawn), so registration is
//! safe even in modes that don't use the character rig.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{ActiveCamera, GlobalTransform, Transform, World};
use byroredux_core::math::{Quat, Vec3};

use crate::components::InputState;

/// Resource pointing at the player character entity, so other systems
/// (camera follow, audio listener attach, future quest-marker
/// distance computations) can find the player without walking the
/// `CharacterController` storage.
///
/// `None` means the engine isn't in player mode. Set by
/// `scene::spawn_player_character`; cleared by
/// `cell_loader::unload_cell` when the player despawns (it's stamped
/// with `CellRoot`, so the cell-unload sweep catches it).
#[derive(Debug, Default, Clone, Copy)]
pub struct PlayerEntity(pub Option<EntityId>);

impl Resource for PlayerEntity {}

/// Engine-wide mode flag. Set at scene-setup based on CLI flags +
/// scene type (interior cell / exterior grid → Character;
/// `--mesh` / `--tree` / `--fly` → FlyCam).
///
/// Used by `fly_camera_system` (gates itself off when Character) and
/// `character_controller_system` / `camera_follow_system` (gates
/// themselves off when FlyCam).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PlayerMode {
    /// Default — original free-fly camera. No gravity, no collision.
    #[default]
    FlyCam,
    /// M28.5 kinematic character + gravity + camera-follows-body.
    Character,
}

impl Resource for PlayerMode {}

/// M27 Phase 3 — single Stage::Early entry point for player control.
///
/// Branches on [`PlayerMode`] and dispatches to either
/// [`super::fly_camera_system`] or [`character_controller_system`]. The
/// two bodies are runtime-mutually-exclusive: each early-returns on
/// the wrong mode, so registering them as separate parallel-stage
/// systems made the scheduler's access analyzer pair them up and
/// surface a `Transform` + `PhysicsWorld` `WriteWrite` conflict that
/// is structurally impossible at runtime. Folding them under one
/// dispatcher removes the conflict cleanly without changing semantics
/// — the inner systems keep their identities + unit-testability and
/// run exactly as before, just through one indirection.
///
/// Access (declared at registration in `byroredux/src/main.rs`) is the
/// union of the two inner systems' accesses. The `PlayerMode` read
/// here is itself part of that union.
pub(crate) fn player_controller_system(world: &World, dt: f32) {
    let mode = world
        .try_resource::<PlayerMode>()
        .map(|r| *r)
        .unwrap_or_default();
    match mode {
        PlayerMode::FlyCam => super::fly_camera_system(world, dt),
        PlayerMode::Character => character_controller_system(world, dt),
    }
}

/// Drive the kinematic character body forward one frame.
///
/// Reads:
///   - [`PlayerEntity`] resource (target body entity)
///   - [`PlayerMode`] resource (early-return on FlyCam)
///   - [`InputState`] (WASD, jump key, yaw for movement alignment)
///   - The body's `Transform` (current world position)
///   - The body's `byroredux_physics::CharacterController` (state + params)
///   - The body's `byroredux_physics::RapierHandles` (collider id to exclude)
///   - The `PhysicsWorld` resource (KCC.move_shape against world colliders)
///
/// Writes:
///   - Body `Transform.translation` (new world position)
///   - Body `CharacterController.{vertical_velocity, is_grounded, wants_jump}`
///   - Rapier body's `set_next_kinematic_translation` (so the
///     simulation knows the player is there for other bodies' queries)
pub(crate) fn character_controller_system(world: &World, dt: f32) {
    if dt <= 0.0 {
        return;
    }
    // M28.5 — clamp dt at 1/30 s (33 ms). The first scheduler tick
    // after engine boot ships a `dt` equal to "wall-clock from App
    // construction to first frame" — for a Whiterun cell load that's
    // ~8 seconds of BSA decode + NIF parse + Vulkan upload. Without
    // the clamp, gravity × dt = -1373 × 8 = -11000 BU/s, instantly
    // capped at terminal velocity -2000, producing a -15 700 BU
    // first-frame translation. Character ends up 15 km below the
    // cell with no chance of recovery, camera follows, user sees a
    // black screen. Bethesda engines do the same clamp for the same
    // reason (a frame hitch should never teleport the player across
    // a room).
    //
    // 1/30 s is a reasonable cap — frames above that are perceived
    // as hitches anyway, and the simulation behaviour for any frame
    // taking >33 ms degrades to "freeze for one tick", not "teleport".
    const MAX_DT: f32 = 1.0 / 30.0;
    let dt = dt.min(MAX_DT);
    let mode = world
        .try_resource::<PlayerMode>()
        .map(|r| *r)
        .unwrap_or_default();
    if mode != PlayerMode::Character {
        return;
    }
    let Some(player_res) = world.try_resource::<PlayerEntity>() else {
        return;
    };
    let Some(player_entity) = player_res.0 else {
        return;
    };
    drop(player_res);

    let Some(input) = world.try_resource::<InputState>() else {
        return;
    };
    let yaw = input.yaw;
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
    let want_jump_now = input.keys_held.contains(&winit::keyboard::KeyCode::Space);
    let want_sprint = input
        .keys_held
        .contains(&winit::keyboard::KeyCode::ControlLeft);
    drop(input);

    // Snapshot character params + current state.
    let (controller, current_pos, collider_handle, body_handle) = {
        let Some(cq) = world.query::<byroredux_physics::CharacterController>() else {
            return;
        };
        let Some(c) = cq.get(player_entity).copied() else {
            return;
        };
        let Some(tq) = world.query::<Transform>() else {
            return;
        };
        let Some(t) = tq.get(player_entity) else {
            return;
        };
        let pos = t.translation;
        let handles = world
            .query::<byroredux_physics::RapierHandles>()
            .and_then(|q| q.get(player_entity).copied());
        let (col, body) = handles.map(|h| (h.collider, h.body)).unzip();
        (c, pos, col, body)
    };

    // Compute the desired horizontal motion in world-space, yaw-aligned.
    // The helper normalises the WASD vector before scaling so diagonal
    // strafe doesn't go √2× faster than pure forward.
    let speed_mul = if want_sprint { 2.0 } else { 1.0 };
    let horizontal_translation = horizontal_motion(yaw, move_dir, controller.move_speed * speed_mul, dt);

    // Integrate gravity into a fresh local vertical_velocity. Then
    // apply the jump impulse if requested + allowed (grounded +
    // not-already-pressed — see `wants_jump` latch on the controller).
    let jump_fired = want_jump_now && controller.is_grounded && !controller.wants_jump;
    let vertical_velocity = integrate_vertical(
        controller.vertical_velocity,
        controller.gravity,
        controller.terminal_velocity,
        dt,
        controller.jump_velocity,
        jump_fired,
    );

    // M28.5 follow-up — when grounded and not jumping, send a small
    // *fixed* downward probe instead of the gravity-integrated motion.
    // The integrated motion is `g * dt² = -23 * dt = ~-0.4 BU` per
    // 60 fps frame, which the KCC tries to satisfy via collide-and-
    // slide; numerical drift on inclined floor TriMeshes lets the
    // character creep down 0.05 BU/frame even while reported grounded.
    // After ~800 frames that's a 40 BU sink, by which point the
    // capsule's lower edge has slipped past a floor tile and snap-to-
    // ground fails. Replacing the integration with a `step_height`-
    // tall downward probe keeps `snap_to_ground` engaged every frame
    // (Rapier triggers snap on grounded→airborne transitions) without
    // accumulating any velocity that would survive landing. This is
    // the Bethesda-engine convention: gravity is suppressed while
    // grounded; only the falling-edge of ground contact unlocks it.
    let desired_vertical = if controller.is_grounded && !jump_fired {
        -controller.step_height
    } else {
        vertical_velocity * dt
    };
    let desired_translation = horizontal_translation + Vec3::Y * desired_vertical;

    // Ask Rapier's KCC for the collide-and-slide-corrected motion.
    // Snapshot ContactConfig once per tick — the offset is the only
    // value the KCC consumes and we don't want to hold a separate
    // resource borrow across the PhysicsWorld read.
    let kcc_offset = world
        .try_resource::<byroredux_physics::ContactConfig>()
        .map(|r| r.kcc_offset_bu)
        .unwrap_or(byroredux_physics::ContactConfig::DEFAULT.kcc_offset_bu);
    let pw = world.resource::<byroredux_physics::PhysicsWorld>();
    let result = pw.move_character(byroredux_physics::CharacterMoveParams {
        capsule_half_height: controller.half_height,
        capsule_radius: controller.radius,
        position: current_pos,
        desired_translation,
        dt,
        max_slope_climb_deg: controller.max_slope_climb_deg,
        step_height: controller.step_height,
        snap_to_ground: controller.snap_to_ground,
        exclude_collider: collider_handle,
        kcc_offset_bu: kcc_offset,
    });
    drop(pw);

    let new_pos = current_pos + result.translation;

    // Diagnostic for M28.5 smoke-testing — log body state for the
    // first 5 frames + when grounded transitions + every 60 frames
    // if airborne. Surfaces "I fell into the void" / "I'm stuck in a
    // wall" failure modes that otherwise present as black-screen with
    // no other signal.
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    static FRAME: AtomicU32 = AtomicU32::new(0);
    static WAS_GROUNDED: AtomicBool = AtomicBool::new(false);
    let frame = FRAME.fetch_add(1, Ordering::Relaxed);
    let prev_grounded = WAS_GROUNDED.swap(result.grounded, Ordering::Relaxed);
    let grounded_transition = prev_grounded != result.grounded;
    if frame < 5
        || grounded_transition
        || (!result.grounded && frame.is_multiple_of(60))
    {
        let pw = world.resource::<byroredux_physics::PhysicsWorld>();
        let body_count = pw.body_count();
        // Dump the AABB of all static colliders ONCE (frame 0) so the
        // operator can see whether the collision world overlaps the
        // character's XZ position. If the bhk import scale is wrong,
        // the colliders cluster tiny near origin while the character
        // spawns at architectural coordinates — KCC traces miss
        // everything.
        if frame == 0 {
            match pw.static_colliders_aabb() {
                Some((min, max, count)) => log::info!(
                    "M28.5 static collider AABB: x [{:.1}, {:.1}], y [{:.1}, {:.1}], \
                     z [{:.1}, {:.1}] ({} fixed colliders); character at \
                     ({:.1}, {:.1}, {:.1})",
                    min[0],
                    max[0],
                    min[1],
                    max[1],
                    min[2],
                    max[2],
                    count,
                    current_pos.x,
                    current_pos.y,
                    current_pos.z,
                ),
                None => log::warn!(
                    "M28.5 NO STATIC COLLIDERS in the Rapier world — every body is \
                     Dynamic/Kinematic. Cell has no parsed bhk static architecture."
                ),
            }
        }
        log::info!(
            "M28.5 frame {}: body Y {:.1}→{:.1} (Δ {:.3}), v {:.1}, grounded={}, rapier_bodies={}{}",
            frame,
            current_pos.y,
            new_pos.y,
            result.translation.y,
            vertical_velocity,
            result.grounded,
            body_count,
            if grounded_transition { " [TRANSITION]" } else { "" },
        );
    }

    // Write back: Transform + CharacterController state + Rapier
    // kinematic-next-position (so other bodies see the player at the
    // step-corrected location).
    {
        let Some(mut tq) = world.query_mut::<Transform>() else {
            return;
        };
        if let Some(t) = tq.get_mut(player_entity) {
            t.translation = new_pos;
            // Keep rotation as identity — the capsule is rotationally
            // symmetric and yaw lives on the camera, not the body.
            t.rotation = Quat::IDENTITY;
        }
    }
    {
        let Some(mut cq) = world.query_mut::<byroredux_physics::CharacterController>() else {
            return;
        };
        if let Some(c) = cq.get_mut(player_entity) {
            // If we just landed (was airborne, now grounded), zero
            // out the residual downward velocity so the next
            // frame's gravity integration starts fresh.
            if result.grounded && vertical_velocity < 0.0 {
                c.vertical_velocity = 0.0;
            } else {
                c.vertical_velocity = vertical_velocity;
            }
            c.is_grounded = result.grounded;
            // Re-arm the jump latch: holding Space keeps `wants_jump`
            // true so a single keypress doesn't fire repeatedly; release
            // clears it.
            c.wants_jump = want_jump_now;
        }
    }
    // Push the new pose into Rapier so other bodies' queries see the
    // player at the right spot. KinematicPositionBased bodies apply
    // this on their next step. Best-effort — failures are physics-
    // backend-internal and we still wrote the engine-side Transform
    // above. `body_handle` is the same `body` field on `RapierHandles`
    // — the EntityId-keyed helper does the actual lookup.
    let _ = body_handle; // suppress unused (the helper takes the EntityId)
    byroredux_physics::set_kinematic_translation(world, player_entity, new_pos);
}

/// Pin the active camera to the player body's eye-height position
/// each frame.
///
/// Runs late (after `physics_sync_system` finalises any reaction
/// kinematics) so the camera lands on the *post-step* body position
/// — no one-frame lag, no smearing through walls.
pub(crate) fn camera_follow_system(world: &World, _dt: f32) {
    let mode = world
        .try_resource::<PlayerMode>()
        .map(|r| *r)
        .unwrap_or_default();
    if mode != PlayerMode::Character {
        return;
    }
    let Some(player_res) = world.try_resource::<PlayerEntity>() else {
        return;
    };
    let Some(player_entity) = player_res.0 else {
        return;
    };
    drop(player_res);

    // Read body position + controller params + camera yaw/pitch.
    let (body_pos, eye_height) = {
        let Some(gq) = world.query::<GlobalTransform>() else {
            return;
        };
        let Some(g) = gq.get(player_entity) else {
            return;
        };
        let Some(cq) = world.query::<byroredux_physics::CharacterController>() else {
            return;
        };
        let Some(c) = cq.get(player_entity) else {
            return;
        };
        (g.translation, c.eye_height)
    };

    let Some(input) = world.try_resource::<InputState>() else {
        return;
    };
    let yaw = input.yaw;
    let pitch = input.pitch;
    drop(input);

    let Some(active) = world.try_resource::<ActiveCamera>() else {
        return;
    };
    let cam_entity = active.0;
    drop(active);

    let cam_pos = body_pos + Vec3::Y * eye_height;
    let cam_rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);

    // Write both Transform and GlobalTransform. The camera is a root
    // entity (no Parent), so for it the two are identical — and
    // because `camera_follow_system` runs in Stage::Late AFTER
    // `transform_propagation` (PostUpdate), there's no propagation
    // pass left this frame to update GlobalTransform. Audio listener
    // sync + submersion detection both read GlobalTransform during
    // Late stage, so the explicit write here keeps the camera pose
    // current within the same frame.
    {
        let Some(mut tq) = world.query_mut::<Transform>() else {
            return;
        };
        if let Some(t) = tq.get_mut(cam_entity) {
            t.translation = cam_pos;
            t.rotation = cam_rot;
        }
    }
    {
        let Some(mut gq) = world.query_mut::<GlobalTransform>() else {
            return;
        };
        if let Some(g) = gq.get_mut(cam_entity) {
            g.translation = cam_pos;
            g.rotation = cam_rot;
        }
    }
}

/// Toggle [`PlayerMode`] between `Character` and `FlyCam` with
/// position-snap semantics modelled on Bethesda's `tcl` (toggle
/// collision) console command. Called from the keyboard handler in
/// `main.rs` when F is tapped (edge-triggered, no key-repeat).
///
/// **Fly → Character**: snap the character body to the camera's
/// current position (minus `eye_height` so the eyes end up where the
/// camera was). Vertical velocity zeroed; grounded reset to false so
/// gravity re-engages next tick. Net effect: player "lands" wherever
/// the freeflight camera was looking from.
///
/// **Character → Fly**: no position writes required.
/// `camera_follow_system` had been writing the active camera at
/// `body_pos + eye_height`, so the fly cam takes over from the same
/// world position. The character body stays alive — its controller
/// system early-returns on FlyCam mode, freezing the body in place
/// until the user toggles back.
///
/// Logs the new mode at INFO so the user gets feedback without an
/// in-engine console.
pub fn toggle_player_mode(world: &mut byroredux_core::ecs::World) {
    let current = world
        .try_resource::<PlayerMode>()
        .map(|r| *r)
        .unwrap_or_default();
    let next = match current {
        PlayerMode::FlyCam => PlayerMode::Character,
        PlayerMode::Character => PlayerMode::FlyCam,
    };

    // On Fly → Character, snap the character body to the active
    // camera's position. The player entity may be absent (engine
    // booted with `--mesh` / `--tree` / `--fly` flags that didn't
    // spawn a character body); in that case bail with a warn — the
    // toggle is a no-op and we stay in FlyCam.
    if matches!(next, PlayerMode::Character) {
        let player_entity = world
            .try_resource::<PlayerEntity>()
            .and_then(|r| r.0);
        let Some(player) = player_entity else {
            log::warn!(
                "Walk/Fly toggle: no PlayerEntity registered \
                 (engine booted without a character body — \
                 `--mesh` / `--tree` / `--fly`? Use a `--cell` \
                 invocation to spawn one). Staying in FlyCam mode."
            );
            return;
        };
        let cam_entity = match world.try_resource::<ActiveCamera>() {
            Some(active) => active.0,
            None => {
                log::warn!("Walk/Fly toggle: no ActiveCamera resource. Aborting toggle.");
                return;
            }
        };
        let (cam_pos, eye_height) = {
            let Some(tq) = world.query::<Transform>() else {
                return;
            };
            let Some(cam_t) = tq.get(cam_entity) else {
                log::warn!("Walk/Fly toggle: ActiveCamera entity has no Transform. Aborting.");
                return;
            };
            let pos = cam_t.translation;
            drop(tq);
            let Some(cq) = world.query::<byroredux_physics::CharacterController>() else {
                return;
            };
            let height = cq.get(player).map(|c| c.eye_height).unwrap_or(52.0);
            (pos, height)
        };
        let body_pos = cam_pos - Vec3::Y * eye_height;
        {
            let Some(mut tq) = world.query_mut::<Transform>() else {
                return;
            };
            if let Some(t) = tq.get_mut(player) {
                t.translation = body_pos;
                t.rotation = Quat::IDENTITY;
            }
        }
        {
            let Some(mut cq) = world.query_mut::<byroredux_physics::CharacterController>() else {
                return;
            };
            if let Some(c) = cq.get_mut(player) {
                // Clear momentum so the body doesn't carry a stale
                // free-fall velocity from before the user entered
                // FlyCam mode. Gravity re-engages on the next
                // controller tick.
                c.vertical_velocity = 0.0;
                c.is_grounded = false;
                c.wants_jump = false;
            }
        }
        // Sync the kinematic Rapier body to the new transform so the
        // KCC's next-frame collide-and-slide query starts from the
        // correct position rather than the pre-toggle frozen one.
        byroredux_physics::set_kinematic_translation(world, player, body_pos);
    }

    *world.resource_mut::<PlayerMode>() = next;
    log::info!(
        "Player mode → {:?} (F key — toggle walk/fly)",
        next
    );
}

/// Compute the world-space horizontal motion vector for the character
/// from yaw, WASD-direction, speed, and dt. Pure function — pulled
/// out for test pinning.
///
/// `move_dir` is the local-space WASD vector (`x = strafe`,
/// `z = forward`). Yaw rotates it into world space; the result is
/// scaled by `speed * dt`. Y component is always zero — vertical
/// motion goes through the gravity-integrated path.
pub(crate) fn horizontal_motion(yaw: f32, move_dir: Vec3, speed: f32, dt: f32) -> Vec3 {
    if move_dir == Vec3::ZERO {
        return Vec3::ZERO;
    }
    let dir = move_dir.normalize();
    let forward = Quat::from_rotation_y(yaw) * -Vec3::Z;
    let right = Quat::from_rotation_y(yaw) * Vec3::X;
    (forward * dir.z + right * dir.x) * speed * dt
}

/// Compute the next-frame vertical velocity given current state, the
/// jump trigger, and dt. Pure function — pulled out for test
/// pinning. Mirrors the inline math in
/// [`character_controller_system`].
pub(crate) fn integrate_vertical(
    prev_velocity: f32,
    gravity: f32,
    terminal_velocity: f32,
    dt: f32,
    jump_velocity: f32,
    jump_fired: bool,
) -> f32 {
    let mut v = prev_velocity + gravity * dt;
    if v < terminal_velocity {
        v = terminal_velocity;
    }
    if jump_fired {
        v = jump_velocity;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Free-fall: gravity accumulates frame-by-frame, capped at
    /// terminal velocity. Pin the integration so a refactor can't
    /// silently swap to a different integrator.
    #[test]
    fn integrate_vertical_free_fall_accumulates_to_terminal() {
        let g = -1373.4; // CharacterController::HUMAN.gravity
        let tv = -2000.0; // CharacterController::HUMAN.terminal_velocity
        let mut v = 0.0;
        let dt = 1.0 / 60.0;
        for _ in 0..60 {
            v = integrate_vertical(v, g, tv, dt, 0.0, false);
        }
        // After ~1 second of free-fall, velocity is g (capped well above
        // tv since |g| < |tv|).
        assert!(v < 0.0, "must be falling");
        assert!(v > tv, "must not exceed terminal velocity");
        assert!(
            (v - g).abs() < 1.0,
            "velocity ≈ g after 1 s of accumulation; got {v}"
        );
    }

    /// Terminal velocity is a clamp on the downward direction.
    #[test]
    fn integrate_vertical_clamps_at_terminal() {
        let g = -1373.4;
        let tv = -2000.0;
        // Start already at terminal — one more step shouldn't go past.
        let v = integrate_vertical(tv, g, tv, 1.0 / 60.0, 0.0, false);
        assert_eq!(v, tv);
    }

    /// Jump fires regardless of falling velocity; that's correct
    /// behaviour for a discrete impulse, and matches Bethesda's
    /// "always available when grounded" jump model.
    #[test]
    fn integrate_vertical_jump_replaces_velocity() {
        let g = -1373.4;
        let tv = -2000.0;
        let jv = 380.0;
        let v = integrate_vertical(tv, g, tv, 1.0 / 60.0, jv, true);
        assert_eq!(v, jv, "jump must set velocity to jump_velocity exactly");
    }

    /// WASD strafe-right + yaw=0 → world +X motion (no Z).
    #[test]
    fn horizontal_motion_strafe_right_at_zero_yaw() {
        let motion = horizontal_motion(0.0, Vec3::new(1.0, 0.0, 0.0), 220.0, 1.0 / 60.0);
        let expected_speed = 220.0 / 60.0;
        assert!(
            (motion.x - expected_speed).abs() < 0.01,
            "x ≈ {expected_speed}; got {}",
            motion.x
        );
        assert!(motion.y.abs() < 1e-6, "y always zero");
        assert!(motion.z.abs() < 0.01, "z ≈ 0 at yaw=0; got {}", motion.z);
    }

    /// WASD forward + yaw=0 → world -Z motion. Camera looks down -Z
    /// in engine space.
    #[test]
    fn horizontal_motion_forward_at_zero_yaw() {
        let motion = horizontal_motion(0.0, Vec3::new(0.0, 0.0, 1.0), 220.0, 1.0 / 60.0);
        let expected_speed = 220.0 / 60.0;
        assert!(motion.x.abs() < 0.01);
        assert!(motion.y.abs() < 1e-6);
        assert!(
            (motion.z - (-expected_speed)).abs() < 0.01,
            "z ≈ {} (negative); got {}",
            -expected_speed,
            motion.z
        );
    }

    /// Yaw=90° rotates "forward" (-Z) to "right" (+X) in standard
    /// Y-up right-handed coords. WASD-forward + yaw=90° → world -X.
    #[test]
    fn horizontal_motion_forward_at_90_yaw() {
        let motion = horizontal_motion(
            std::f32::consts::FRAC_PI_2,
            Vec3::new(0.0, 0.0, 1.0),
            220.0,
            1.0 / 60.0,
        );
        let expected_speed = 220.0 / 60.0;
        // 90° yaw rotates -Z → -X, so forward becomes -X.
        assert!(
            (motion.x - (-expected_speed)).abs() < 0.01,
            "x ≈ {} at yaw=90°; got {}",
            -expected_speed,
            motion.x
        );
        assert!(motion.z.abs() < 0.01);
    }

    /// Zero input → zero output (no NaN from normalising the zero
    /// vector).
    #[test]
    fn horizontal_motion_zero_input_is_zero() {
        let motion = horizontal_motion(1.234, Vec3::ZERO, 220.0, 1.0 / 60.0);
        assert_eq!(motion, Vec3::ZERO);
    }

    /// Diagonal motion preserves the speed cap — strafe-and-forward
    /// shouldn't go √2× faster than pure forward.
    #[test]
    fn horizontal_motion_diagonal_does_not_exceed_speed() {
        let dt = 1.0 / 60.0;
        let speed = 220.0;
        let forward_only = horizontal_motion(0.0, Vec3::new(0.0, 0.0, 1.0), speed, dt);
        let diag = horizontal_motion(0.0, Vec3::new(1.0, 0.0, 1.0), speed, dt);
        let forward_len = forward_only.length();
        let diag_len = diag.length();
        assert!(
            (forward_len - diag_len).abs() < 0.01,
            "diagonal length must match forward-only length (input is normalised); \
             forward={forward_len}, diag={diag_len}"
        );
    }
}
