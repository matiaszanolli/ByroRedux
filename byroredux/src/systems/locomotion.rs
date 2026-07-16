//! Shared straight-line walk-to-point locomotion step, used by both
//! `wander_system` (M42.3) and `travel_system` (M42.4) — the two AI
//! procedures that need an actor to physically move. Extracted once a
//! second consumer needed the exact same ground-snap + turn-to-face math;
//! each system still owns its own higher-level state machine (Wander
//! oscillates Walking ⇄ Paused forever, Travel goes Walking → terminal
//! once), only the per-tick move itself is shared.
//!
//! No pathing/NAVM — a wall or obstacle between the actor and its target
//! isn't routed around. Fine for open ground; a walled room would need
//! real navigation, out of scope here (see both systems' module docs).

use byroredux_core::math::{Quat, Vec3};

/// Walk speed (world units/second). Engine default — no authored
/// equivalent exists in PACK data, so this is a plain constant subject to
/// tuning, not a value derived from game content.
pub(crate) const LOCOMOTION_WALK_SPEED: f32 = 100.0;

/// Distance (world units) within which an actor is considered to have
/// arrived at its target.
pub(crate) const LOCOMOTION_ARRIVAL_EPSILON: f32 = 8.0;

/// Facing turn rate (fraction of the remaining turn closed per second,
/// clamped to `[0,1]` per tick via `(LOCOMOTION_TURN_RATE * dt).clamp(0.0, 1.0)`
/// as the `Quat::slerp` interpolation factor). Engine default.
pub(crate) const LOCOMOTION_TURN_RATE: f32 = 4.0;

/// Downward-raycast cast distance (world units) for ground-snapping —
/// generous enough to cover a full cell's vertical extent.
pub(crate) const LOCOMOTION_GROUND_RAY_MAX_DISTANCE: f32 = 4096.0;
/// Raycast origin is lifted this far above the actor's last known Y
/// before casting down, so walking uphill doesn't cast from underground.
pub(crate) const LOCOMOTION_GROUND_RAY_UP_OFFSET: f32 = 256.0;

/// One tick of straight-line walk-toward-target: move on the XZ plane at
/// [`LOCOMOTION_WALK_SPEED`], ground-snap Y via
/// `PhysicsWorld::cast_ray_down` when a physics world is available (the
/// same mechanism `scene.rs` uses for camera placement), and turn to face
/// the direction of travel via `Quat::slerp`.
///
/// `target_xz` should have `.y` pre-set to `current.y` by the caller
/// (only `.x`/`.z` are meaningful here — `.y` is re-derived from the
/// ground below, not interpolated toward a stale authored/picked Y that
/// drifts from real terrain on sloped ground). Returns the new position
/// and, when the actor moved enough this tick to have a meaningful
/// facing direction, the new rotation (`None` when already at the
/// target — e.g. paused, or arrived).
pub(crate) fn step_toward(
    current: Vec3,
    current_rotation: Quat,
    target_xz: Vec3,
    dt: f32,
    physics: Option<&byroredux_physics::PhysicsWorld>,
) -> (Vec3, Option<Quat>) {
    let mut new_pos = current.move_towards(target_xz, LOCOMOTION_WALK_SPEED * dt);

    if let Some(pw) = physics {
        let ray_origin = Vec3::new(
            new_pos.x,
            current.y + LOCOMOTION_GROUND_RAY_UP_OFFSET,
            new_pos.z,
        );
        if let Some(ground_y) = pw.cast_ray_down(ray_origin, LOCOMOTION_GROUND_RAY_MAX_DISTANCE) {
            new_pos.y = ground_y;
        }
        // No collider hit (e.g. a synthetic test World, or a stale query
        // pipeline) → keep the XZ-moved Y as-is rather than snapping to a
        // wrong height.
    }

    let delta = Vec3::new(target_xz.x - current.x, 0.0, target_xz.z - current.z);
    let rotation = if delta.length_squared() > 1e-6 {
        let desired_yaw = delta.x.atan2(delta.z);
        let desired_rot = Quat::from_rotation_y(desired_yaw);
        let t = (LOCOMOTION_TURN_RATE * dt).clamp(0.0, 1.0);
        Some(current_rotation.slerp(desired_rot, t))
    } else {
        None
    };

    (new_pos, rotation)
}
