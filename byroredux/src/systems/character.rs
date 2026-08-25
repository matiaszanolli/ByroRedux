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

use byroredux_core::ecs::components::actor_state::Dead;
use byroredux_core::ecs::components::actor_values::{ActorValues, ActorVitals};
use byroredux_core::ecs::components::water::{WaterFlow, WaterPlane, WaterVolume};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{ActiveCamera, GlobalTransform, TotalTime, Transform, World};
use byroredux_core::math::{Quat, Vec3};

use crate::components::InputState;
use crate::interaction::{ActionState, InputAction};

/// Resource pointing at the player character entity, so other systems
/// (camera follow, audio listener attach, future quest-marker
/// distance computations) can find the player without walking the
/// `CharacterController` storage.
///
/// `None` means the engine isn't in player mode. Set when `scene` creates the
/// player rig. The body is intentionally not cell-owned, so it and this
/// process-local pointer survive live cell reloads.
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
    crate::interaction::refresh_action_state(world);

    let mode = world
        .try_resource::<PlayerMode>()
        .map(|r| *r)
        .unwrap_or_default();
    match mode {
        PlayerMode::FlyCam => super::fly_camera_system(world, dt),
        PlayerMode::Character => character_controller_system(world, dt),
    }
}

fn player_accepts_movement_input(world: &World, player: EntityId) -> bool {
    let controls_allow_movement = world
        .try_resource::<byroredux_scripting::PlayerControlState>()
        .map(|controls| controls.movement_enabled && !controls.ai_driven)
        .unwrap_or(true);
    let restrained = world
        .get::<byroredux_scripting::ActorControlState>(player)
        .is_some_and(|state| state.restrained);
    controls_allow_movement && !restrained
}

/// Drive the kinematic character body forward one frame.
///
/// Reads:
///   - [`PlayerEntity`] resource (target body entity)
///   - [`PlayerMode`] resource (early-return on FlyCam)
///   - [`InputState`] (yaw for movement alignment)
///   - [`ActionState`] (movement, jump, and sprint gameplay intents)
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
    // #2886 — the clamp value lives on `CharacterController` so the preset's
    // own invariant test can state the terminal-velocity bound in velocity
    // units instead of comparing against a bare acceleration.
    let dt = dt.min(byroredux_physics::CharacterController::MAX_FRAME_DT);
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
    if world.get::<Dead>(player_entity).is_some() {
        return;
    }

    // Papyrus startup fragments can independently disable movement, hand the
    // player to AI, or restrain the actor. Keep gravity/collision ticking in
    // all three cases; only suppress user-authored horizontal/jump intent.
    let accepts_movement_input = player_accepts_movement_input(world, player_entity);

    let yaw = world
        .try_resource::<InputState>()
        .map(|input| input.yaw)
        .unwrap_or_default();
    let Some(actions) = world.try_resource::<ActionState>() else {
        return;
    };
    let mut move_dir = Vec3::ZERO;
    if accepts_movement_input && actions.is_held(InputAction::MoveForward) {
        move_dir.z += 1.0;
    }
    if accepts_movement_input && actions.is_held(InputAction::MoveBackward) {
        move_dir.z -= 1.0;
    }
    if accepts_movement_input && actions.is_held(InputAction::StrafeLeft) {
        move_dir.x -= 1.0;
    }
    if accepts_movement_input && actions.is_held(InputAction::StrafeRight) {
        move_dir.x += 1.0;
    }
    let want_jump_now = accepts_movement_input && actions.is_held(InputAction::Jump);
    let want_sprint = accepts_movement_input && actions.is_held(InputAction::Sprint);
    drop(actions);

    // Snapshot character params + current state.
    let (controller, current_pos, collider_handle, body_handle) = {
        let Some(cq) = world.query::<byroredux_physics::CharacterController>() else {
            return;
        };
        let Some(c) = cq.get(player_entity).copied() else {
            return;
        };
        // The `Transform` read guard is dropped (block ends) before
        // `RapierHandles` is acquired below — `pull_dynamic` (`sync.rs`)
        // acquires the reverse pair (`RapierHandles`/`RigidBodyData` held
        // across a `Transform` *write*), so overlapping the two orders
        // here would be an ABBA risk (#2135).
        let pos = {
            let Some(tq) = world.query::<Transform>() else {
                return;
            };
            let Some(t) = tq.get(player_entity) else {
                return;
            };
            t.translation
        };
        let handles = world
            .query::<byroredux_physics::RapierHandles>()
            .and_then(|q| q.get(player_entity).copied());
        let (col, body) = handles.map(|h| (h.collider, h.body)).unzip();
        (c, pos, col, body)
    };

    // Kinematic actors do not participate in the dynamic-body buoyancy pass.
    // Sample the same authored AABB here so the player gets a stable swim
    // state instead of continuing terrestrial gravity through a lake or river.
    // The snapshot is completed before borrowing PhysicsWorld below, keeping
    // ECS query guards out of the physics lock interval.
    let water_contact = player_water_state(
        world,
        current_pos,
        controller.half_height + controller.radius,
    );
    // Match the OpenMW swimlevel convention: merely wetting the capsule's
    // feet does not switch the controller from walking to swimming. The
    // center must pass the engine-defined fraction of the capsule height;
    // surface waves are already included in `surface_y`.
    let head_submerged = water_contact
        .map(|(surface_y, _, _, _)| current_pos.y + controller.eye_height <= surface_y)
        .unwrap_or(false);
    let swim = water_contact.filter(|(surface_y, _, _, _)| {
        swimlevel_reached(
            current_pos.y,
            *surface_y,
            controller.half_height + controller.radius,
        )
    });
    let (breath_remaining, drowning_damage) = advance_breath(
        controller.breath_remaining,
        controller.drowning_damage_accumulator,
        head_submerged,
        dt,
    );

    // Compute the desired horizontal motion in world-space, yaw-aligned.
    // The helper normalises the WASD vector before scaling so diagonal
    // strafe doesn't go √2× faster than pure forward.
    let speed_mul = if want_sprint { 2.0 } else { 1.0 };
    let mut horizontal_translation =
        horizontal_motion(yaw, move_dir, controller.move_speed * speed_mul, dt);
    if let Some((_, fraction, Some(flow), _)) = swim {
        // Currents push a swimmer, but are deliberately bounded below the
        // authored flow speed so a river cannot turn the controller into an
        // uncontrollable projectile. Waterfalls keep their vertical flow in
        // the buoyancy path and contribute no horizontal drift.
        let current = Vec3::new(flow.direction[0], 0.0, flow.direction[2])
            * (flow.speed * 0.35 * fraction.clamp(0.0, 1.0) * dt);
        horizontal_translation += current;
    }

    // Integrate gravity into a fresh local vertical_velocity. Then
    // apply the jump impulse if requested + allowed (grounded +
    // not-already-pressed — see `wants_jump` latch on the controller).
    let jump_fired =
        want_jump_now && (controller.is_grounded || swim.is_some()) && !controller.wants_jump;
    let vertical_velocity = match swim {
        Some((surface_y, fraction, _, _)) => swim_vertical_velocity(
            controller.vertical_velocity,
            current_pos.y,
            surface_y,
            controller.half_height + controller.radius,
            fraction,
            dt,
            controller.jump_velocity,
            jump_fired,
        ),
        None => integrate_vertical(
            controller.vertical_velocity,
            controller.gravity,
            controller.terminal_velocity,
            dt,
            controller.jump_velocity,
            jump_fired,
        ),
    };

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
    // #2857 — the probe must be bounded by the ACTUAL support surface, not
    // sent as an unclamped fixed `-step_height`.
    //
    // The KCC resolves motion by sweeping the capsule along the requested
    // direction with `target_distance = offset`. A grounded capsule rests
    // ~`offset` above its floor, i.e. already inside that band, and in that
    // configuration parry's cast against a CONVEX primitive frequently
    // reports no interference at all — whereupon rapier's `else` branch
    // (`character_controller.rs:317-322`) applies the ENTIRE requested
    // translation and `break`s. A fixed -32 BU request therefore drove the
    // capsule 32 BU/frame straight through solid `BhkBoxShape` floors and
    // every synthesized packed-Havok AABB proxy, staying `grounded` for 2-3
    // frames (post-loop `snap_to_ground` re-asserts it) before free-falling
    // out of the world. TriMesh floors happen to be immune, which is why
    // interiors mostly worked and this survived. Measured: 48/80 convex
    // configurations sank; whether it fires is selected by the collider's
    // absolute world Y, so it reads as intermittent.
    //
    // Bounding the request alone is not enough — any positive amount walks
    // the capsule down over successive frames once the cast stops clamping.
    // So measure the gap to the real support first and ask for exactly that,
    // which keeps `snap_to_ground` engaged (the anti-drift property the fixed
    // probe existed for) while making tunnelling arithmetically impossible.
    let kcc_offset = world
        .try_resource::<byroredux_physics::ContactConfig>()
        .map(|r| r.kcc_offset_bu)
        .unwrap_or(byroredux_physics::ContactConfig::DEFAULT.kcc_offset_bu);
    let pw = world.resource::<byroredux_physics::PhysicsWorld>();
    let desired_vertical = if swim.is_none() && controller.is_grounded && !jump_fired {
        // Probe down for the surface the capsule is standing on. The player's
        // own body must be excluded or the sweep instantly self-hits (#2859).
        let support_y = pw.cast_capsule_down(
            current_pos,
            controller.half_height,
            controller.radius,
            controller.step_height + kcc_offset.max(0.0),
            body_handle,
        );
        match support_y {
            Some(surface_y) => {
                // Move exactly to resting contact — `offset` above the
                // support — and no further. Signed on purpose: correcting
                // a capsule that has crept slightly BELOW the contact band
                // is the anti-drift property the old fixed probe provided
                // (via the KCC clamping -32 back to the offset every frame),
                // and losing it would reintroduce the 0.05 BU/frame creep on
                // inclined TriMeshes that the probe was added for. Bounded
                // below by `step_height` so a legitimate step-down still
                // resolves in one frame, and above by `offset` so the
                // correction can never launch the capsule.
                let feet_y = current_pos.y - controller.half_height - controller.radius;
                let offset = kcc_offset.max(0.0);
                let correction = -(feet_y - surface_y - offset);
                correction.clamp(-controller.step_height, offset)
            }
            // Nothing within reach: we are grounded per the last frame but
            // have walked off an edge taller than the probe. Hand back to
            // gravity rather than inventing a descent.
            None => vertical_velocity * dt,
        }
    } else {
        vertical_velocity * dt
    };
    let desired_translation = horizontal_translation + Vec3::Y * desired_vertical;

    // Ask Rapier's KCC for the collide-and-slide-corrected motion.
    let result = pw.move_character(byroredux_physics::CharacterMoveParams {
        capsule_half_height: controller.half_height,
        capsule_radius: controller.radius,
        position: current_pos,
        desired_translation,
        dt,
        max_slope_climb_deg: controller.max_slope_climb_deg,
        step_height: controller.step_height,
        step_min_width: controller.step_min_width,
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
    if frame < 5 || grounded_transition || (!result.grounded && frame.is_multiple_of(60)) {
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
            c.breath_remaining = breath_remaining;
            c.drowning_damage_accumulator = drowning_damage.remainder;
        }
    }
    if drowning_damage.whole > 0.0 {
        apply_player_drowning_damage(world, player_entity, drowning_damage.whole);
    }
    if let Some((_, fraction, _, damage_per_second)) = swim {
        let damage = water_damage_for_contact(damage_per_second, fraction, dt);
        if damage > 0.0 {
            apply_player_drowning_damage(world, player_entity, damage);
        }
    }
    // Push the new pose into Rapier so other bodies' queries see the
    // player at the right spot. KinematicPositionBased bodies apply
    // this on their next step. Best-effort — failures are physics-
    // backend-internal and we still wrote the engine-side Transform
    // above. `body_handle` is the same `body` field on `RapierHandles`
    // — the EntityId-keyed helper does the actual lookup. (`body_handle`
    // itself is consumed by the grounded support probe above, #2857.)
    byroredux_physics::set_kinematic_translation(world, player_entity, new_pos);
}

/// Pin the active camera to the player body's eye-height position
/// each frame.
///
/// Runs late (after `physics_sync_system` finalises any reaction
/// kinematics) so the camera lands on the *post-step* body position
/// — no one-frame lag, no smearing through walls.
pub(crate) fn camera_follow_system(world: &World, dt: f32) {
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

    // cam_entity needed early so we can read its previous Y for smoothing.
    let Some(active) = world.try_resource::<ActiveCamera>() else {
        return;
    };
    let cam_entity = active.0;
    drop(active);

    // #3260 — GlobalTransform and CharacterController must NOT be held
    // concurrently here. `character_controller_system` holds
    // CharacterController -> Transform, and transform_propagation holds
    // Transform -> GlobalTransform; composing those with a
    // GlobalTransform -> CharacterController edge in this system would
    // close a live three-lock cycle. Snapshotting both GlobalTransform
    // reads into a tuple and dropping `gq` (block end) before acquiring
    // `cq` below breaks that edge — see
    // `camera_follow_does_not_close_character_lock_cycle` for the
    // regression test that pins this ordering.
    let (body_pos, previous_camera_y) = {
        let Some(gq) = world.query::<GlobalTransform>() else {
            return;
        };
        let Some(g) = gq.get(player_entity) else {
            return;
        };
        // Absence is resolved after eye_height is snapshotted below so the
        // first-frame fallback remains the exact target Y.
        let prev_y = gq.get(cam_entity).map(|cg| cg.translation.y);
        (g.translation, prev_y)
    };
    let eye_height = {
        let Some(cq) = world.query::<byroredux_physics::CharacterController>() else {
            return;
        };
        let Some(c) = cq.get(player_entity) else {
            return;
        };
        c.eye_height
    };
    let prev_cam_y = previous_camera_y.unwrap_or(body_pos.y + eye_height);

    let Some(input) = world.try_resource::<InputState>() else {
        return;
    };
    let yaw = input.yaw;
    let pitch = input.pitch;
    drop(input);

    // Smooth camera Y upward (step-up QoL: ease onto stairs/doorsteps
    // rather than snapping), but follow downward immediately so the
    // camera doesn't lag during falls or descents.
    let target_cam_y = body_pos.y + eye_height;
    let smooth_cam_y = if target_cam_y > prev_cam_y + 0.5 {
        // Exponential ease-up: settles within ~0.15 s at k=20.
        let alpha = 1.0 - (-20.0_f32 * dt).exp();
        prev_cam_y + (target_cam_y - prev_cam_y) * alpha
    } else {
        target_cam_y
    };

    let cam_pos = Vec3::new(body_pos.x, smooth_cam_y, body_pos.z);
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
/// Snap the character body (physics capsule + `Transform`) to the
/// active camera's current position, minus `eye_height` so the eyes
/// end up where the camera was. Vertical velocity zeroed; grounded
/// reset to false so gravity re-engages next tick.
///
/// Shared by [`toggle_player_mode`] (Fly → Character) and
/// `cell_loader::transition`'s runtime door-transition path (#1874).
/// The transition path calls [`reposition_camera`] to jump the camera
/// to the destination spawn point but — pre-#1874-fix — never touched
/// the character capsule at all, leaving it behind in the just-
/// unloaded source cell. `camera_follow_system` (Stage::Late, every
/// frame) pins the camera to "body position + eye_height," so on the
/// very next tick it snapped the camera straight back toward the
/// stale (now often ungrounded/free-falling through unloaded
/// geometry) capsule — undoing the transition and re-triggering a
/// fresh, UNSIGNALED camera discontinuity每 frame until the capsule
/// physically settled somewhere. That's the mechanism behind the
/// "ghosted double-image, sticks after camera parks" symptom: TAA/SVGF
/// history never got a chance to recover because the source of the
/// bad motion vector kept recurring every frame, not because the
/// initial `signal_temporal_discontinuity` call (which the transition
/// path already made correctly) failed to protect a single jump.
///
/// Returns `false` (with a warning already logged) when there's no
/// player body to snap — safe to call unconditionally; the caller
/// doesn't need to gate on `PlayerMode` itself, since a fly-cam-only
/// boot (no `PlayerEntity`) just no-ops here.
pub fn snap_character_body_to_camera(world: &mut byroredux_core::ecs::World) -> bool {
    let player_entity = world.try_resource::<PlayerEntity>().and_then(|r| r.0);
    let Some(player) = player_entity else {
        log::warn!(
            "snap_character_body_to_camera: no PlayerEntity registered \
             (engine booted without a character body — \
             `--mesh` / `--tree` / `--fly`? Use a `--cell` \
             invocation to spawn one). No-op."
        );
        return false;
    };
    let cam_entity = match world.try_resource::<ActiveCamera>() {
        Some(active) => active.0,
        None => {
            log::warn!("snap_character_body_to_camera: no ActiveCamera resource. Aborting.");
            return false;
        }
    };
    let (cam_pos, eye_height) = {
        let Some(tq) = world.query::<Transform>() else {
            return false;
        };
        let Some(cam_t) = tq.get(cam_entity) else {
            log::warn!(
                "snap_character_body_to_camera: ActiveCamera entity has no Transform. Aborting."
            );
            return false;
        };
        let pos = cam_t.translation;
        drop(tq);
        let Some(cq) = world.query::<byroredux_physics::CharacterController>() else {
            return false;
        };
        let height = cq.get(player).map(|c| c.eye_height).unwrap_or(52.0);
        (pos, height)
    };
    let body_pos = cam_pos - Vec3::Y * eye_height;
    {
        let Some(mut tq) = world.query_mut::<Transform>() else {
            return false;
        };
        if let Some(t) = tq.get_mut(player) {
            t.translation = body_pos;
            t.rotation = Quat::IDENTITY;
        }
    }
    {
        let Some(mut cq) = world.query_mut::<byroredux_physics::CharacterController>() else {
            return false;
        };
        if let Some(c) = cq.get_mut(player) {
            // Clear momentum so the body doesn't carry a stale
            // free-fall velocity from before the snap. Gravity
            // re-engages on the next controller tick.
            c.vertical_velocity = 0.0;
            c.is_grounded = false;
            c.wants_jump = false;
        }
    }
    // Sync the kinematic Rapier body to the new transform so the
    // KCC's next-frame collide-and-slide query starts from the
    // correct position rather than the pre-snap frozen one.
    byroredux_physics::set_kinematic_translation(world, player, body_pos);
    true
}

/// Ground the character capsule at a **floor-level** destination — the arrival
/// half of a runtime door transition.
///
/// #2869. The transition path used to call [`snap_character_body_to_camera`]
/// right after [`reposition_camera`](crate::cell_loader::reposition_camera)
/// had placed the camera at the raw XTEL destination. That helper is correct
/// for its original caller (`toggle_player_mode`, where the camera genuinely
/// *is* at eye height) but wrong here: it subtracts `eye_height` from a pose
/// that is already at floor level, putting the capsule *centre* at
/// `dest.y - eye_height` and its feet a further `half_height + radius` below
/// that. Cold start, meanwhile, places the capsule centre at
/// `floor + half_height + radius + kcc_offset` — so the two engine paths
/// disagreed by ~120 BU for the very same door, and the transition path ran no
/// ground probe, no walkable-normal check and no grounded verification at all.
/// It handed a capsule buried in the destination floor to gravity, and since
/// the body is kinematic and Rapier's penetration recovery is a stub, nothing
/// pushed it back out.
///
/// Routes through the same two pieces cold start uses —
/// [`probe_walkable_floor_near`](crate::scene::probe_walkable_floor_near) then
/// [`character_spawn_center_y`](crate::scene::character_spawn_center_y) — so
/// the door-arrival ladder cannot drift from the boot ladder again. The probe
/// excludes the player's own capsule, which (unlike at cold start) already
/// exists and is standing in the sweep.
///
/// A probe miss falls back to the authored destination height, matching the
/// cold-start ladder's own fallback: the destination is floor level by
/// construction, so standing on it beats sinking below it.
///
/// The camera is pulled to `body + eye_height` here rather than left for
/// `camera_follow_system` to fix next tick — the caller has just signalled a
/// temporal discontinuity for this jump, and letting the camera spend a frame
/// at the floor-level pose would produce exactly the unsignalled second jump
/// #1874 was about.
pub fn ground_character_body_at(world: &byroredux_core::ecs::World, destination: Vec3) -> bool {
    let Some(player) = world.try_resource::<PlayerEntity>().and_then(|r| r.0) else {
        log::warn!(
            "ground_character_body_at: no PlayerEntity registered — \
             fly-cam-only boot, nothing to ground. No-op."
        );
        return false;
    };
    let Some(cc) = world
        .query::<byroredux_physics::CharacterController>()
        .and_then(|q| q.get(player).copied())
    else {
        log::warn!("ground_character_body_at: player has no CharacterController. Aborting.");
        return false;
    };

    // The destination cell's colliders were inserted into `ColliderSet` by the
    // load, but the query pipeline only learns about them via a pipeline step —
    // without this flush the probe sweeps an empty BVH and always misses. Same
    // dt=0 register-then-flush the cold-start probe does.
    byroredux_physics::physics_sync_system(world, 0.0);
    {
        let mut pw = world.resource_mut::<byroredux_physics::PhysicsWorld>();
        pw.update_query_pipeline();
    }

    let floor_y = crate::scene::probe_walkable_floor_near(
        world,
        destination.x,
        destination.z,
        destination.y,
        cc,
        Some(player),
    );
    let center_y =
        crate::scene::character_spawn_center_y(world, floor_y.unwrap_or(destination.y), cc);
    let body_pos = Vec3::new(destination.x, center_y, destination.z);
    log::info!(
        "Transition arrival: grounding capsule at ({:.1}, {:.1}, {:.1}) — floor {} \
         (destination y={:.1})",
        body_pos.x,
        body_pos.y,
        body_pos.z,
        match floor_y {
            Some(y) => format!("probed at {y:.1}"),
            None => "probe MISSED, using destination height".to_string(),
        },
        destination.y,
    );

    {
        let Some(mut tq) = world.query_mut::<Transform>() else {
            return false;
        };
        if let Some(t) = tq.get_mut(player) {
            t.translation = body_pos;
            t.rotation = Quat::IDENTITY;
        }
    }
    {
        let Some(mut cq) = world.query_mut::<byroredux_physics::CharacterController>() else {
            return false;
        };
        if let Some(c) = cq.get_mut(player) {
            c.vertical_velocity = 0.0;
            c.is_grounded = false;
            c.wants_jump = false;
        }
    }
    byroredux_physics::set_kinematic_translation(world, player, body_pos);
    pin_camera_above_body(world, body_pos, cc.eye_height);
    true
}

/// Move the active camera to `body + eye_height` without disturbing its
/// rotation, writing `Transform` and `GlobalTransform` together the way
/// `camera_follow_system` does — the transition path runs outside the
/// scheduler, so there is no propagation pass left to refresh the global.
fn pin_camera_above_body(world: &byroredux_core::ecs::World, body_pos: Vec3, eye_height: f32) {
    let Some(cam_entity) = world.try_resource::<ActiveCamera>().map(|active| active.0) else {
        return;
    };
    let cam_pos = body_pos + Vec3::Y * eye_height;
    if let Some(mut tq) = world.query_mut::<Transform>() {
        if let Some(t) = tq.get_mut(cam_entity) {
            t.translation = cam_pos;
        }
    }
    if let Some(mut gq) = world.query_mut::<GlobalTransform>() {
        if let Some(g) = gq.get_mut(cam_entity) {
            g.translation = cam_pos;
        }
    }
}

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
    // camera's position. Abort the toggle (stay in FlyCam) if the
    // snap couldn't complete — matches the pre-extraction behavior:
    // a body-less boot (`--mesh` / `--tree` / `--fly`) shouldn't
    // silently flip into a PlayerMode with no body to control.
    if matches!(next, PlayerMode::Character) && !snap_character_body_to_camera(world) {
        return;
    }

    *world.resource_mut::<PlayerMode>() = next;
    log::info!("Player mode → {:?} (F key — toggle walk/fly)", next);
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

/// Return the nearest water column intersecting a capsule centred at `pos`.
/// The fraction is the capsule's vertical span below the wave-adjusted surface.
/// The final tuple element carries FO3/FNV authored water damage per second.
/// This mirrors the dynamic-body `WaterContact` calculation without creating
/// a transient component for the kinematic player.
fn player_water_state(
    world: &World,
    pos: Vec3,
    half_span: f32,
) -> Option<(f32, f32, Option<WaterFlow>, f32)> {
    let (Some(wq), Some(vq)) = (world.query::<WaterPlane>(), world.query::<WaterVolume>()) else {
        return None;
    };
    let flow_q = world.query::<WaterFlow>();
    let bottom = pos.y - half_span;
    let top = pos.y + half_span;
    let mut best: Option<(f32, f32, Option<WaterFlow>, f32, f32)> = None;
    for (entity, plane) in wq.iter() {
        let Some(volume) = vq.get(entity) else {
            continue;
        };
        if pos.x < volume.min[0]
            || pos.x > volume.max[0]
            || pos.z < volume.min[2]
            || pos.z > volume.max[2]
        {
            continue;
        }
        let wave_height = world
            .try_resource::<TotalTime>()
            .map(|time| {
                let (weather_scroll, wind_wave_scale) =
                    byroredux_physics::weather_wave_adjustment(world, time.0);
                byroredux_physics::authored_wave_height_with_weather(
                    &plane.material,
                    pos,
                    time.0,
                    weather_scroll,
                    wind_wave_scale,
                )
            })
            .unwrap_or(0.0);
        let surface_y = volume.max[1] + wave_height;
        if top < volume.min[1] || bottom > surface_y {
            continue;
        }
        let fraction = ((surface_y - bottom) / (top - bottom).max(f32::EPSILON)).clamp(0.0, 1.0);
        if fraction <= 0.0 {
            continue;
        }
        let distance = (surface_y - pos.y).abs();
        let flow = flow_q.as_ref().and_then(|q| q.get(entity).copied());
        if best.as_ref().is_none_or(|candidate| distance < candidate.4) {
            best = Some((
                surface_y,
                fraction,
                flow,
                wq.get(entity)
                    .map(|plane| plane.damage_per_second)
                    .unwrap_or(0.0),
                distance,
            ));
        }
    }
    best.map(|(surface_y, fraction, flow, damage, _)| (surface_y, fraction, flow, damage))
}

/// OpenMW's `swimlevel = waterLevel - halfExtentsZ * fSwimHeightScale`
/// expressed for this capsule controller. Keep the scale engine-defined and
/// game-invariant; water records do not author movement physics.
const SWIM_HEIGHT_SCALE: f32 = 0.35;

/// Per-second exponential decay rate for the swim vertical-velocity spring
/// (see [`swim_vertical_velocity`]). Chosen so `exp(-SWIM_DAMPING / 60.0) ==
/// 0.72`, reproducing the originally tuned 60 fps feel while making the
/// integrator dt-correct at any refresh rate (#3125).
const SWIM_DAMPING: f32 = 19.71;

#[inline]
fn swimlevel_reached(center_y: f32, surface_y: f32, half_span: f32) -> bool {
    center_y < surface_y - half_span * SWIM_HEIGHT_SCALE
}

/// Integrate a swimmer toward a neutral buoyancy point near the waterline.
/// Gravity is replaced by a critically-damped buoyancy spring; jump remains a
/// bounded upward stroke while submerged. This keeps entry/exit continuous and
/// prevents a falling player from tunnelling through a shallow water volume.
#[allow(clippy::too_many_arguments)] // Pure scalar integrator; grouping would obscure units.
pub(crate) fn swim_vertical_velocity(
    prev_velocity: f32,
    center_y: f32,
    surface_y: f32,
    half_span: f32,
    fraction: f32,
    dt: f32,
    jump_velocity: f32,
    jump_fired: bool,
) -> f32 {
    if jump_fired {
        return jump_velocity
            .mul_add(0.55, prev_velocity * 0.15)
            .clamp(-120.0, 220.0);
    }
    let target_y = surface_y - half_span * SWIM_HEIGHT_SCALE;
    let spring = (target_y - center_y) * (5.0 + 7.0 * fraction.clamp(0.0, 1.0));
    // #3125 — the decay must be per-second, not per-frame, or the swimmer's
    // approach speed to the waterline varies with refresh rate (a fixed
    // `* 0.72` per call decays ~4.8x faster in wall-clock terms at 144 fps
    // than at 30 fps). `integrate_vertical`'s terrestrial sibling is already
    // dt-correct via `gravity * dt`; this mirrors that.
    (prev_velocity * (-SWIM_DAMPING * dt).exp() + spring * dt).clamp(-120.0, 160.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DrowningDamage {
    whole: f32,
    remainder: f32,
}

/// Advance the game-invariant breath reserve. Bethesda records do not author
/// a breath duration; the 15-second reserve and 12 HP/s damage rate are
/// engine constants shared by the supported families. Fractional damage is
/// retained so a variable render frame cannot lose health over time.
fn advance_breath(
    previous_breath: f32,
    previous_damage_remainder: f32,
    head_submerged: bool,
    dt: f32,
) -> (f32, DrowningDamage) {
    const MAX_BREATH: f32 = 15.0;
    const DROWNING_DAMAGE_PER_SECOND: f32 = 12.0;
    if !head_submerged {
        return (
            MAX_BREATH,
            DrowningDamage {
                whole: 0.0,
                remainder: 0.0,
            },
        );
    }
    if dt <= 0.0 {
        // #3128 — no time passed, so this must be a no-op: preserve the
        // accumulated breath and fractional damage instead of refilling the
        // reserve. `!head_submerged` above is the only case that should
        // reset state; collapsing the two into one guard let a zero-dt tick
        // (paused / re-entrant call) refill a drowning player to full air.
        return (
            previous_breath.clamp(0.0, MAX_BREATH),
            DrowningDamage {
                whole: 0.0,
                remainder: previous_damage_remainder.max(0.0),
            },
        );
    }
    let previous = previous_breath.clamp(0.0, MAX_BREATH);
    let breath = (previous - dt).max(0.0);
    let exhausted_seconds = (dt - previous).max(0.0);
    let damage = previous_damage_remainder
        .max(0.0)
        .mul_add(1.0, exhausted_seconds * DROWNING_DAMAGE_PER_SECOND);
    let whole = damage.floor();
    (
        breath,
        DrowningDamage {
            whole,
            remainder: damage - whole,
        },
    )
}

fn apply_player_drowning_damage(world: &World, player: EntityId, damage: f32) {
    let Some(vitals) = world.get::<ActorVitals>(player).map(|v| *v) else {
        return;
    };
    let Some(mut values) = world.query_mut::<ActorValues>() else {
        return;
    };
    let Some(actor_values) = values.get_mut(player) else {
        return;
    };
    actor_values.apply_damage(vitals.health, damage);
    let dead = actor_values.current(vitals.health) <= 0.0;
    drop(values);
    if dead {
        if let Some(mut dead_q) = world.query_mut::<Dead>() {
            dead_q.insert(player, Dead);
        }
        crate::combat::queue_dead_actor_reconciliation(world, player);
    }
}

#[inline]
fn water_damage_for_contact(damage_per_second: f32, submerged_fraction: f32, dt: f32) -> f32 {
    damage_per_second.max(0.0) * submerged_fraction.clamp(0.0, 1.0) * dt.max(0.0)
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

    /// #3260 — recreate the two established sides of the production lock
    /// triangle, then drive the real camera system. Under the CI lock-order
    /// detector, holding GlobalTransform while acquiring CharacterController
    /// closes the cycle and panics here.
    #[test]
    fn camera_follow_does_not_close_character_lock_cycle() {
        if std::env::var_os("BYRO_LOCK_ORDER_CHECK").as_deref() != Some(std::ffi::OsStr::new("1")) {
            return;
        }

        let mut world = World::new();
        let player = world.spawn();
        let camera = world.spawn();
        world.insert_resource(PlayerMode::Character);
        world.insert_resource(PlayerEntity(Some(player)));
        world.insert_resource(ActiveCamera(camera));
        world.insert_resource(InputState::default());
        world.insert(player, Transform::default());
        world.insert(player, GlobalTransform::default());
        world.insert(player, byroredux_physics::CharacterController::HUMAN);
        world.insert(camera, Transform::default());
        world.insert(camera, GlobalTransform::default());

        // Transform -> GlobalTransform (transform propagation's order).
        {
            let _transform = world.query::<Transform>().unwrap();
            let _global = world.query::<GlobalTransform>().unwrap();
        }
        // CharacterController -> Transform (character_controller_system's
        // order). A subsequent GlobalTransform -> CharacterController edge
        // would now close the three-lock cycle.
        {
            let _controller = world
                .query::<byroredux_physics::CharacterController>()
                .unwrap();
            let _transform = world.query::<Transform>().unwrap();
        }

        camera_follow_system(&world, 1.0 / 60.0);
    }
    use byroredux_core::ecs::components::water::WaterMaterial;

    #[test]
    fn papyrus_control_and_restraint_state_gate_player_movement() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let player = world.spawn();
        assert!(player_accepts_movement_input(&world, player));

        world
            .resource_mut::<byroredux_scripting::PlayerControlState>()
            .ai_driven = true;
        assert!(!player_accepts_movement_input(&world, player));

        world
            .resource_mut::<byroredux_scripting::PlayerControlState>()
            .ai_driven = false;
        world.insert(
            player,
            byroredux_scripting::ActorControlState { restrained: true },
        );
        assert!(!player_accepts_movement_input(&world, player));
    }

    /// #2869 — a door arrival must land the capsule ON the destination floor,
    /// standing at the same height cold start would place it, not `eye_height`
    /// below the floor-level XTEL pose.
    ///
    /// The two engine paths disagreed by ~120 BU for the same door: cold start
    /// puts the capsule *centre* at `floor + half_height + radius +
    /// kcc_offset` (= +68 for HUMAN), while the transition path handed the
    /// floor-level camera pose to `snap_character_body_to_camera`, which
    /// subtracts `eye_height` (= -52). The assertion is written against the
    /// cold-start formula rather than a literal so a controller re-tune moves
    /// both together, plus an explicit floor-not-below check that fails on the
    /// pre-fix number for any tuning.
    #[test]
    fn door_arrival_grounds_the_capsule_on_the_destination_floor() {
        const FLOOR_Y: f32 = 200.0;

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<byroredux_core::ecs::components::collision::CollisionShape>();
        world.register::<byroredux_core::ecs::components::collision::RigidBodyData>();
        world.register::<byroredux_physics::CharacterController>();
        world.register::<byroredux_physics::RapierHandles>();
        world.insert_resource(byroredux_physics::PhysicsWorld::new());

        // A wide static slab whose TOP surface is the destination floor.
        let floor = world.spawn();
        let slab_half_height = 10.0;
        let slab_center = Vec3::new(0.0, FLOOR_Y - slab_half_height, 0.0);
        world.insert(floor, Transform::new(slab_center, Quat::IDENTITY, 1.0));
        world.insert(
            floor,
            GlobalTransform::new(slab_center, Quat::IDENTITY, 1.0),
        );
        world.insert(
            floor,
            byroredux_core::ecs::components::collision::CollisionShape::Cuboid {
                half_extents: Vec3::new(500.0, slab_half_height, 500.0),
            },
        );
        world.insert(
            floor,
            byroredux_core::ecs::components::collision::RigidBodyData::STATIC,
        );

        // The player capsule, parked far away in the "source cell".
        let cc = byroredux_physics::CharacterController::HUMAN;
        let player = world.spawn();
        let stale = Vec3::new(-9000.0, -9000.0, -9000.0);
        world.insert(player, Transform::new(stale, Quat::IDENTITY, 1.0));
        world.insert(player, GlobalTransform::new(stale, Quat::IDENTITY, 1.0));
        world.insert(player, cc);
        world.insert_resource(PlayerEntity(Some(player)));

        let camera = world.spawn();
        world.insert(camera, Transform::IDENTITY);
        world.insert(camera, GlobalTransform::IDENTITY);
        world.insert_resource(ActiveCamera(camera));

        // The XTEL destination: floor level, as authored.
        let destination = Vec3::new(0.0, FLOOR_Y, 0.0);
        assert!(ground_character_body_at(&mut world, destination));

        let body_y = world
            .query::<Transform>()
            .unwrap()
            .get(player)
            .unwrap()
            .translation
            .y;
        let kcc_offset = byroredux_physics::ContactConfig::DEFAULT.kcc_offset_bu;
        let expected = FLOOR_Y + cc.half_height + cc.radius + kcc_offset;
        assert!(
            (body_y - expected).abs() < 1.0,
            "capsule centre landed at {body_y}, want the cold-start height {expected}"
        );

        // The pre-fix path put the capsule CENTRE below the floor (and its
        // feet a further `half_height + radius` under that). Independent of
        // any tuning, the feet must end up at or above the floor.
        let feet_y = body_y - cc.half_height - cc.radius;
        assert!(
            feet_y >= FLOOR_Y - 0.5,
            "capsule feet at {feet_y} are below the destination floor {FLOOR_Y} — \
             buried, exactly the #2869 failure"
        );

        // The camera follows FROM the body, not the other way round.
        let camera_y = world
            .query::<Transform>()
            .unwrap()
            .get(camera)
            .unwrap()
            .translation
            .y;
        assert!((camera_y - (body_y + cc.eye_height)).abs() < 1e-3);
    }

    /// A destination with no floor beneath it must fall back to the authored
    /// height rather than to the eye-height subtraction — the cold-start
    /// ladder's own fallback. Nothing is spawned to stand on here, so the
    /// probe misses by construction.
    #[test]
    fn door_arrival_with_no_probe_hit_falls_back_to_the_authored_height() {
        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<byroredux_core::ecs::components::collision::CollisionShape>();
        world.register::<byroredux_core::ecs::components::collision::RigidBodyData>();
        world.register::<byroredux_physics::CharacterController>();
        world.register::<byroredux_physics::RapierHandles>();
        world.insert_resource(byroredux_physics::PhysicsWorld::new());

        let cc = byroredux_physics::CharacterController::HUMAN;
        let player = world.spawn();
        world.insert(player, Transform::IDENTITY);
        world.insert(player, GlobalTransform::IDENTITY);
        world.insert(player, cc);
        world.insert_resource(PlayerEntity(Some(player)));

        let destination = Vec3::new(12.0, 500.0, -34.0);
        assert!(ground_character_body_at(&mut world, destination));

        let body = world
            .query::<Transform>()
            .unwrap()
            .get(player)
            .unwrap()
            .translation;
        let kcc_offset = byroredux_physics::ContactConfig::DEFAULT.kcc_offset_bu;
        let expected = destination.y + cc.half_height + cc.radius + kcc_offset;
        assert!(
            (body.y - expected).abs() < 1e-3,
            "probe miss should stand ON the authored destination height, got {}",
            body.y
        );
        assert!((body.x - destination.x).abs() < 1e-3);
        assert!((body.z - destination.z).abs() < 1e-3);
    }

    /// Free-fall: gravity accumulates frame-by-frame, capped at
    /// terminal velocity. Pin the integration so a refactor can't
    /// silently swap to a different integrator.
    ///
    /// #2886 — the constants are READ from the preset, not transcribed. The
    /// pre-fix literals (`g = -1373.4`, `jv = 380.0`) had been superseded by
    /// a 2× jump-height / 1.5× hang-time retune (`gravity = -1220.8`,
    /// `jump_velocity = 506.6667`) that these tests never noticed, while
    /// their comments claimed to pin the preset. A link asserted only in a
    /// comment is not a link.
    #[test]
    fn integrate_vertical_free_fall_accumulates_to_terminal() {
        let human = byroredux_physics::CharacterController::HUMAN;
        let g = human.gravity;
        let tv = human.terminal_velocity;
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
        let human = byroredux_physics::CharacterController::HUMAN;
        let g = human.gravity;
        let tv = human.terminal_velocity;
        // Start already at terminal — one more step shouldn't go past.
        let v = integrate_vertical(tv, g, tv, 1.0 / 60.0, 0.0, false);
        assert_eq!(v, tv);
    }

    /// Jump fires regardless of falling velocity; that's correct
    /// behaviour for a discrete impulse, and matches Bethesda's
    /// "always available when grounded" jump model.
    #[test]
    fn integrate_vertical_jump_replaces_velocity() {
        let human = byroredux_physics::CharacterController::HUMAN;
        let g = human.gravity;
        let tv = human.terminal_velocity;
        let jv = human.jump_velocity;
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

    #[test]
    fn swimlevel_keeps_shallow_wading_out_of_swim_mode() {
        assert!(!swimlevel_reached(90.0, 100.0, 50.0));
        assert!(swimlevel_reached(80.0, 100.0, 50.0));
        assert!(!swimlevel_reached(82.5, 100.0, 50.0));
    }

    #[test]
    fn swimming_replaces_gravity_with_bounded_buoyancy() {
        let v = swim_vertical_velocity(0.0, 0.0, 100.0, 50.0, 1.0, 1.0 / 60.0, 380.0, false);
        assert!(v > 0.0, "a submerged swimmer below the neutral point rises");
        assert!(v < 160.0, "buoyancy must remain bounded");

        let jump = swim_vertical_velocity(-80.0, 80.0, 100.0, 50.0, 0.5, 1.0 / 60.0, 380.0, true);
        assert!(
            jump > 0.0 && jump <= 220.0,
            "swim stroke is a bounded upward impulse"
        );
    }

    #[test]
    fn swimming_damps_downward_velocity_near_surface() {
        let v = swim_vertical_velocity(-120.0, 82.5, 100.0, 50.0, 0.8, 1.0 / 60.0, 380.0, false);
        assert!(v > -120.0, "water drag must reduce a falling speed");
    }

    /// #3125 — one step at 1/60s and two half-steps at 1/120s must land close
    /// together. The old `prev_velocity * 0.72` decay was per-frame, not
    /// per-second, so it diverged sharply across frame rates (~7.8 BU/s here
    /// vs the ~0.15 BU/s of remaining Euler discretization error below).
    #[test]
    fn swim_damping_is_frame_rate_independent() {
        let (center_y, surface_y, half_span, fraction, jump_velocity) =
            (70.0, 100.0, 50.0, 0.6, 380.0);
        let full = swim_vertical_velocity(
            -40.0,
            center_y,
            surface_y,
            half_span,
            fraction,
            1.0 / 60.0,
            jump_velocity,
            false,
        );
        let half1 = swim_vertical_velocity(
            -40.0,
            center_y,
            surface_y,
            half_span,
            fraction,
            1.0 / 120.0,
            jump_velocity,
            false,
        );
        let half2 = swim_vertical_velocity(
            half1,
            center_y,
            surface_y,
            half_span,
            fraction,
            1.0 / 120.0,
            jump_velocity,
            false,
        );
        assert!(
            (full - half2).abs() < 0.3,
            "swim damping should be ~frame-rate independent: one 1/60s step = {full}, two 1/120s steps = {half2}"
        );
    }

    #[test]
    fn authored_water_wave_height_is_bounded_and_time_varying() {
        let material = WaterMaterial {
            wave_amplitude: 4.0,
            wave_frequency: 1.0,
            scroll_a: [0.0228254, 0.0],
            scroll_b: [0.0, 0.0286531],
            ..WaterMaterial::default()
        };
        let position = Vec3::new(113.0, 0.0, -47.0);
        let at_start = byroredux_physics::authored_wave_height_with_weather(
            &material, position, 0.0, [0.0; 2], 1.0,
        );
        let later = byroredux_physics::authored_wave_height_with_weather(
            &material, position, 0.37, [0.0; 2], 1.0,
        );
        assert!(at_start.is_finite() && later.is_finite());
        assert!(at_start.abs() <= 4.0 && later.abs() <= 4.0);
        assert!(
            (at_start - later).abs() > 1.0e-4,
            "authored waves must move the player contact surface over time"
        );
    }

    #[test]
    fn breath_replenishes_at_surface_and_drowns_after_reserve() {
        let (breath, damage) = advance_breath(2.0, 0.25, false, 1.0);
        assert_eq!(breath, 15.0);
        assert_eq!(damage.whole, 0.0);
        assert_eq!(damage.remainder, 0.0);

        let (breath, damage) = advance_breath(0.0, 0.25, true, 0.25);
        assert_eq!(breath, 0.0);
        assert_eq!(damage.whole, 3.0);
        assert!((damage.remainder - 0.25).abs() < 1e-6);
    }

    #[test]
    fn breath_preserves_fractional_damage_between_ticks() {
        let (_, first) = advance_breath(0.0, 0.0, true, 1.0 / 60.0);
        assert_eq!(first.whole, 0.0);
        let (_, second) = advance_breath(0.0, first.remainder, true, 1.0 / 60.0);
        assert!(second.whole >= 0.0);
        assert!(second.remainder < 1.0);
    }

    /// #3128 — a zero-dt tick while submerged must be a no-op, not a full
    /// refill. Only `!head_submerged` (surfacing) should reset the reserve.
    #[test]
    fn zero_dt_tick_while_submerged_does_not_refill_breath() {
        let (breath, damage) = advance_breath(3.0, 0.6, true, 0.0);
        assert_eq!(breath, 3.0, "no time passed — breath must not be refilled");
        assert_eq!(damage.whole, 0.0, "no time passed — no damage accrues");
        assert_eq!(
            damage.remainder, 0.6,
            "accumulated fractional damage must survive a zero-dt tick"
        );
    }

    #[test]
    fn authored_water_damage_scales_with_contact_fraction_and_dt() {
        assert_eq!(water_damage_for_contact(20.0, 0.5, 0.25), 2.5);
        assert_eq!(water_damage_for_contact(20.0, 2.0, 0.25), 5.0);
        assert_eq!(water_damage_for_contact(-1.0, 0.5, 1.0), 0.0);
        assert_eq!(water_damage_for_contact(20.0, 0.5, -1.0), 0.0);
    }

    #[test]
    fn drowning_damage_uses_actor_vitals_and_marks_death() {
        let mut world = World::new();
        world.register::<ActorValues>();
        world.register::<ActorVitals>();
        world.register::<Dead>();
        world.insert_resource(crate::combat::PendingDeathReconciliations::default());
        let player = world.spawn();
        world.insert(player, ActorVitals { health: 7 });
        world.insert(player, ActorValues::from_pairs([(7, 5.0)]));

        apply_player_drowning_damage(&world, player, 5.0);

        assert_eq!(world.get::<ActorValues>(player).unwrap().current(7), 0.0);
        assert!(world.get::<Dead>(player).is_some());
    }
}
