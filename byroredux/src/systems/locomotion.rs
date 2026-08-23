//! Shared straight-line walk-to-point locomotion step, used by every AI
//! locomotion procedure (Wander/Travel/Follow/Escort/Guard/Patrol) — the
//! systems that need an actor to physically move. Extracted once a
//! second consumer needed the exact same ground-snap + turn-to-face math;
//! each system still owns its own higher-level state machine (Wander
//! oscillates Walking ⇄ Paused forever, Travel goes Walking → terminal
//! once), only the per-tick move itself is shared.
//!
//! [`step_toward`] itself still knows nothing about NAVM — it walks
//! straight at whatever point it's given. Single-tile NAVM pathing
//! (EX-16 item 3, `docs/engine/navmesh-pathfinding.md`) sits one layer
//! up: [`step_along_waypoints`]/[`pop_reached_waypoint`] below turn a
//! cached waypoint queue (`crate::components::NavPath`, resolved via
//! `navmesh_path::path_from_resident_tiles`) into the single point
//! `step_toward` steps toward each tick, so a wall or obstacle *within* a
//! resident tile's navmesh is routed around; a destination beyond the
//! actor's own tile (Phase 2, genuinely blocked — see the design doc)
//! still falls back to a straight line for the unreachable remainder.

use byroredux_core::math::{Quat, Vec3};
use std::collections::VecDeque;

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
        // `None` is correct here, not a gap (#2873, fixed). The thing this
        // ray must not hit is the moving actor's own ~18 keyframed ragdoll
        // bones, and `exclude_rigid_body` could never cover them: each bone
        // is a *separate* `KinematicPositionBased` body, and `step_toward`
        // receives neither the actor's `EntityId` nor its `RapierHandles`.
        // They are masked out wholesale instead — the bones carry
        // `ACTOR_BONE_GROUP` and `cast_ray_down` filters that group out for
        // every caller. Pre-fix, the ray hit the actor's upper body, re-seated
        // its root at that bone's height, and — since the bones follow the
        // root — climbed again next tick: an elevator, not an offset.
        if let Some(ground_y) =
            pw.cast_ray_down(ray_origin, LOCOMOTION_GROUND_RAY_MAX_DISTANCE, None)
        {
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

/// Pop `waypoints`' front entry once `new_pos` has arrived within
/// [`LOCOMOTION_ARRIVAL_EPSILON`] of it. Split out from
/// [`step_along_waypoints`] below so a caller whose own `step_toward`
/// call is nested inside another function — `wander_system`'s
/// `step_oscillating_wander`, which must keep deciding the Walking→Paused
/// transition itself — can still share the exact same pop decision
/// rather than re-deriving the epsilon comparison.
pub(crate) fn pop_reached_waypoint(new_pos: Vec3, waypoints: &mut VecDeque<Vec3>) {
    let Some(&next) = waypoints.front() else {
        return;
    };
    let delta = Vec3::new(new_pos.x - next.x, 0.0, new_pos.z - next.z);
    if delta.length_squared() <= LOCOMOTION_ARRIVAL_EPSILON * LOCOMOTION_ARRIVAL_EPSILON {
        waypoints.pop_front();
    }
}

/// [`step_toward`], but stepping toward the *next unconsumed waypoint* in
/// `waypoints` instead of straight at `goal` — the shape every "frozen
/// destination" locomotion system (Travel, Guard, Escort's lead phase)
/// and every "live destination" one (Follow, Escort's collect phase)
/// both need, once each resolves its own `waypoints` queue via
/// `navmesh_path::resolve_cached_waypoints`. Falls back to walking
/// straight at `goal` when `waypoints` is empty (no resident-tile path
/// was found) — identical to every pre-pathing caller's prior behavior.
///
/// Deliberately does **not** decide "has the actor arrived at `goal`
/// overall" — callers already compute that themselves against their own
/// criterion (an epsilon for Travel/Guard/Escort's lead phase, a
/// stand-off distance for Follow/Escort's collect phase), so this only
/// owns the waypoint-consumption mechanics every caller shares. Returns
/// the new position/rotation from the underlying `step_toward` call and
/// `waypoints` with the just-reached entry (if any) popped.
pub(crate) fn step_along_waypoints(
    current: Vec3,
    current_rotation: Quat,
    mut waypoints: VecDeque<Vec3>,
    goal: Vec3,
    dt: f32,
    physics: Option<&byroredux_physics::PhysicsWorld>,
) -> (Vec3, Option<Quat>, VecDeque<Vec3>) {
    let step_point = waypoints.front().copied().unwrap_or(goal);
    let step_xz = Vec3::new(step_point.x, current.y, step_point.z);
    let (new_pos, rotation) = step_toward(current, current_rotation, step_xz, dt, physics);
    pop_reached_waypoint(new_pos, &mut waypoints);
    (new_pos, rotation, waypoints)
}
