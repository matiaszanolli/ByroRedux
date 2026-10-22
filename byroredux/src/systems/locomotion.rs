//! Shared straight-line walk-to-point locomotion step, used by every AI
//! locomotion procedure (Wander/Travel/Follow/Escort/Guard/Patrol) — the
//! systems that need an actor to physically move. Extracted once a
//! second consumer needed the exact same ground-snap + turn-to-face math;
//! each system still owns its own higher-level state machine (Wander
//! oscillates Walking ⇄ Paused forever, Travel goes Walking → terminal
//! once), only the per-tick move itself is shared.
//!
//! M42.10 — the per-tick move is now physics-backed: with a
//! `PhysicsWorld` present, [`step_toward`] drives the XZ displacement
//! through Rapier's kinematic character controller (collide-and-slide,
//! autostep, ground snap — see [`step_toward_detailed`] for the full
//! parameterization), so a walking NPC is blocked by walls and clutter
//! instead of ghosting through them, and only falls back to the raw XZ
//! move + `cast_ray_down` snap when no physics world is installed.
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

/// #5587 — the fallback radius (world units) every M42 AI procedure uses
/// when its `PACK` data authors no radius (a missing PLDT or radius 0):
/// "how far does this ambient behavior reach without an authored radius"
/// for FNV-interior/yard-scale content. One fact — it was copied as five
/// per-procedure `*_DEFAULT_RADIUS` constants that cross-referenced each
/// other in prose to stay in step; wander/patrol/travel/guard/escort all
/// point here now, and a retune moves them together.
pub(crate) const AI_PACKAGE_DEFAULT_RADIUS: f32 = 512.0;

/// Distance (world units) within which an actor is considered to have
/// arrived at its target.
pub(crate) const LOCOMOTION_ARRIVAL_EPSILON: f32 = 8.0;

/// Facing turn rate (fraction of the remaining turn closed per second,
/// clamped to `[0,1]` per tick via `(LOCOMOTION_TURN_RATE * dt).clamp(0.0, 1.0)`
/// as the `Quat::slerp` interpolation factor). Engine default.
pub(crate) const LOCOMOTION_TURN_RATE: f32 = 4.0;

/// Raycast origin is lifted this far above the actor's last known Y
/// before casting down, so walking uphill doesn't cast from underground.
pub(crate) const LOCOMOTION_GROUND_RAY_UP_OFFSET: f32 = 256.0;

/// M42.10 — NPC KCC capsule half-height (BU, excludes caps), matching the
/// shape-less-actor fallback collider (`npc_spawn.rs::install_fallback_
/// actor_collider`) so every walking NPC sweeps with the same body the
/// combat ray targeting already assumes. Derived from the shared const
/// pair, not copied (#4689 — a retune of the fallback body now moves the
/// KCC with it instead of silently diverging).
pub(crate) const LOCOMOTION_NPC_CAPSULE_HALF_HEIGHT: f32 =
    crate::npc_spawn::FALLBACK_ACTOR_CAPSULE_HALF_HEIGHT;
/// M42.10 — NPC KCC capsule radius (BU). Total height
/// `2 * (32 + 20) = 104` BU ≈ 1.5 m at the 70 BU/m Havok scale.
pub(crate) const LOCOMOTION_NPC_CAPSULE_RADIUS: f32 =
    crate::npc_spawn::FALLBACK_ACTOR_CAPSULE_RADIUS;
/// M42.10 — autostep ceiling (BU). The KCC default 32 BU (~46 cm at 70
/// BU/m) is sized to canonical Bethesda stair treads; NPC locomotion has
/// no reason to diverge from the player controller's step, so this reads
/// `CharacterController::HUMAN` directly (#4689).
pub(crate) const LOCOMOTION_NPC_STEP_HEIGHT: f32 =
    byroredux_physics::CharacterController::HUMAN.step_height;
/// M42.10 — autostep minimum platform width (BU). 8 BU handles FNV
/// doorsteps whose treads are often 8-16 BU deep (same rationale as the
/// player's `CharacterController::step_min_width`, shared by derivation).
pub(crate) const LOCOMOTION_NPC_STEP_MIN_WIDTH: f32 =
    byroredux_physics::CharacterController::HUMAN.step_min_width;
/// M42.10 / #5539 — ground-snap distance (BU) for the NPC KCC. **0**:
/// disabled. Rapier 0.36 fires snap on every horizontal move from inside
/// the contact-offset band, and its snap cast — applied unchecked, past
/// the collision clamp — misreads CONVEX floors there; against the
/// probe-bounded vertical request that produced a runaway (probe reads a
/// phantom-deep surface → requests descent → snap pushes below the band
/// → deeper phantom; measured sinking 1.3+ BU in 100 frames on a cuboid
/// slab). The stay-attached-to-downslopes job snap used to do now belongs
/// to the pre-move support probe (`step_toward_detailed`): it requests
/// exactly the descent to resting contact each frame, bounded by the
/// step height, and the collision clamp bounds it — no unchecked
/// application. TriMesh terrain was never affected either way. Ledges
/// are the [`LOCOMOTION_MAX_DROP`] fallback ray's job, unchanged.
pub(crate) const LOCOMOTION_NPC_SNAP_TO_GROUND: f32 = 0.0;
/// M42.10 — max climbable slope (degrees). KCC default 50°, shared with
/// the player controller by derivation (#4689).
pub(crate) const LOCOMOTION_NPC_MAX_SLOPE_DEG: f32 =
    byroredux_physics::CharacterController::HUMAN.max_slope_climb_deg;
/// M42.10 — KCC contact skin offset (BU). Derived from `ContactConfig::
/// DEFAULT.kcc_offset_bu` (4 BU ≈ 5.7 cm at the Havok scale); NPC
/// locomotion reads no resource, so the default is pinned as a constant
/// and the invariant `> 2 * contact skin` from #2885 is inherited. A
/// retune of the resource default now reaches NPCs in the same commit
/// (#4689 — the copy previously aged independently).
pub(crate) const LOCOMOTION_NPC_KCC_OFFSET_BU: f32 =
    byroredux_physics::ContactConfig::DEFAULT.kcc_offset_bu;
/// M42.10 — when the KCC reports the actor airborne after a step (walked
/// off a ledge, or spawned over a hole), the legacy downward ray may only
/// relocate the actor this far before it is judged to be genuinely
/// falling/floating rather than "standing on ground far below". The
/// pre-KCC behavior snapped to *any* solid within 4096 BU below —
/// walking off an exterior ledge teleported the actor to the terrain
/// under the interior shell in a single tick.
pub(crate) const LOCOMOTION_MAX_DROP: f32 = 256.0;
/// M42.10 — a step counts as **blocked** when the KCC delivered less than
/// this fraction of the requested horizontal move (the rest eaten by a
/// wall/furniture contact). Pure threshold on the collide-and-slide
/// result; deliberately lenient so slope climbing and corner scraping
/// don't read as blocked.
pub(crate) const LOCOMOTION_BLOCKED_FRACTION: f32 = 0.25;
/// M42.10 — how long (seconds) an oscillating walker (Wander/Patrol) may
/// stay continuously blocked before it re-picks its target instead of
/// grinding into the obstacle forever. Straight-line fallback legs (no
/// resident navmesh tile) can legitimately aim into architecture; a
/// deterministic re-pick gives the actor a fresh direction, matching the
/// save-stable no-RNG convention of the rest of Wander.
pub(crate) const LOCOMOTION_STUCK_REPICK_SECS: f32 = 2.5;

/// One tick of straight-line walk-toward-target (M42.10 revision).
///
/// With a `PhysicsWorld` present, the XZ move is driven through Rapier's
/// `KinematicCharacterController` via `PhysicsWorld::move_character`
/// (collide-and-slide against fixed colliders, autostep up stair treads,
/// ground snap on terrain rolls) — the same body the player controller
/// uses, parameterized for NPCs. This is the "active physics" half of
/// NPC locomotion: an NPC no longer ghosts through walls. What the sweep
/// does NOT do (#4690 — the old doc overclaimed both): it does not SHOVE
/// dynamic clutter (blocked, but no collision impulses, and autostep
/// refuses dynamic bodies), and the `ACTOR_BONE_GROUP` mask excludes
/// EVERY actor's bones, so NPC-vs-NPC contact is ghosted by design
/// (`byroredux_physics::
/// actor_move_interaction_groups`), the multi-body analogue of the
/// `cast_ray_down` self-hit fix (#2873). When the KCC reports the actor
/// airborne after the step, a downward ray (clamped to
/// [`LOCOMOTION_MAX_DROP`]) decides between "ground just out of snap
/// range" and "genuinely over a drop".
///
/// Without a `PhysicsWorld` (synthetic test worlds), falls back to the
/// original unobstructed XZ move so the pure-geometry tests stay
/// meaningful.
///
/// `target_xz` should have `.y` pre-set to `current.y` by the caller
/// (only `.x`/`.z` are meaningful here — `.y` is re-derived from the
/// ground below, not interpolated toward a stale authored/picked Y that
/// drifts from real terrain on sloped ground). Returns the new position,
/// and, when the actor moved enough this tick to have a meaningful
/// facing direction, the new rotation (`None` when already at the
/// target — e.g. paused, or arrived).
pub(crate) fn step_toward(
    current: Vec3,
    current_rotation: Quat,
    target_xz: Vec3,
    dt: f32,
    speed: f32,
    physics: Option<&byroredux_physics::PhysicsWorld>,
) -> (Vec3, Option<Quat>) {
    let (new_pos, rotation, _blocked) =
        step_toward_detailed(current, current_rotation, target_xz, dt, speed, physics);
    (new_pos, rotation)
}

/// The unit horizontal direction from `from` to `to`, level (no Y); `+X` when the two stand on one
/// spot, so a push or a flight always goes somewhere. What fleeing and a knockdown push share (#6150).
pub(crate) fn level_away(from: Vec3, to: Vec3) -> Vec3 {
    let away = Vec3::new(to.x - from.x, 0.0, to.z - from.z).normalize_or_zero();
    if away == Vec3::ZERO {
        Vec3::X
    } else {
        away
    }
}

/// [`step_toward`], plus a `blocked` flag: `true` when the tick asked for
/// meaningful horizontal motion and the KCC delivered less than
/// [`LOCOMOTION_BLOCKED_FRACTION`] of it. `step_toward` discards the flag;
/// the oscillating walkers (Wander/Patrol) consume it to re-pick targets
/// they are grinding against instead of pressing into a wall forever.
pub(crate) fn step_toward_detailed(
    current: Vec3,
    current_rotation: Quat,
    target_xz: Vec3,
    dt: f32,
    speed: f32,
    physics: Option<&byroredux_physics::PhysicsWorld>,
) -> (Vec3, Option<Quat>, bool) {
    let desired_xz = Vec3::new(target_xz.x - current.x, 0.0, target_xz.z - current.z);
    let desired_length = desired_xz.length();

    let mut new_pos = current.move_towards(target_xz, speed * dt);
    let mut blocked = false;

    if let Some(pw) = physics {
        // KCC sweep. The capsule is centered at `position`, so the actor's
        // feet position is lifted by half the capsule height before the
        // query and the resulting translation delta applied back to the
        // feet directly (a delta is reference-point independent).
        let lift = LOCOMOTION_NPC_CAPSULE_HALF_HEIGHT + LOCOMOTION_NPC_CAPSULE_RADIUS;
        // #5539 — rapier 0.36's snap-to-ground now fires on every
        // horizontal move (the `<= 0.0` vertical trigger, 0.36
        // `character_controller.rs:466`), and from a configuration already
        // inside the 4 BU contact-offset band the downward shape cast
        // against a CONVEX floor misreads its time of impact — snap then
        // applied the misread unchecked, sinking NPCs knee- to waist-deep
        // into box / convex-hull floors and through thin ones, while the
        // KCC still reported `grounded`, so the clamped fallback ray below
        // never corrected it. The NPC step now owns its vertical exactly,
        // in the spirit of the player's #2857 treatment but with a RAY
        // for the gap measurement: the capsule shape-cast misreads
        // against convex slabs from inside the band by a little every
        // time, and feeding that phantom into an "exact" request ran away
        // (measured: a steady ~0.4 BU/frame descent that neither the
        // probe nor the collision clamp caught, because the capsule never
        // leaves the band the clamp lives in). A straight-down ray hits
        // the floor's top face exactly on every geometry kind, so the
        // requested descent is exact and snap (now 0,
        // [`LOCOMOTION_NPC_SNAP_TO_GROUND`]) has nothing to amplify.
        let kcc_offset = LOCOMOTION_NPC_KCC_OFFSET_BU.max(0.0);
        let probe = pw.cast_ray_down(
            Vec3::new(current.x, current.y + lift, current.z),
            lift + LOCOMOTION_NPC_STEP_HEIGHT + kcc_offset,
            None,
        )
;
        // Clamped to the remaining distance (`move_towards` semantics) so
        // the final step of a leg lands on the target instead of
        // overshooting past it. `speed` is the actor's authored stride
        // (M42.11 `WalkSpeed`) falling back to LOCOMOTION_WALK_SPEED.
        let desired = if desired_length > 1e-6 {
            let step = (speed * dt).min(desired_length);
            desired_xz * (step / desired_length)
        } else {
            Vec3::ZERO
        };
        // Exactly to resting contact — `offset` above the support — and no
        // further. Signed on purpose: a capsule that crept slightly BELOW
        // the band is lifted back (the player controller's anti-drift
        // property, `character.rs` #2857). Bounded below by the step height
        // (a legitimate step-down still resolves in one frame) and above by
        // the offset (the correction can never launch the capsule). No
        // support within a step of the feet = walked off a ledge or a
        // hole: request the horizontal move alone and let the clamped
        // fallback ray below decide.
        let desired = match probe {
            Some(surface_y) => {
                let correction = -(current.y - surface_y - kcc_offset);
                desired + Vec3::Y * correction.clamp(-LOCOMOTION_NPC_STEP_HEIGHT, kcc_offset)
            }
            None => desired,
        };
        let result = pw.move_character(byroredux_physics::CharacterMoveParams {
            capsule_half_height: LOCOMOTION_NPC_CAPSULE_HALF_HEIGHT,
            capsule_radius: LOCOMOTION_NPC_CAPSULE_RADIUS,
            position: Vec3::new(current.x, current.y + lift, current.z),
            desired_translation: desired,
            dt,
            max_slope_climb_deg: LOCOMOTION_NPC_MAX_SLOPE_DEG,
            step_height: LOCOMOTION_NPC_STEP_HEIGHT,
            step_min_width: LOCOMOTION_NPC_STEP_MIN_WIDTH,
            snap_to_ground: LOCOMOTION_NPC_SNAP_TO_GROUND,
            exclude_collider: None,
            filter_groups: Some(byroredux_physics::actor_move_interaction_groups()),
            kcc_offset_bu: LOCOMOTION_NPC_KCC_OFFSET_BU,
        });
        new_pos = current + result.translation;

        if !result.grounded {
            // Airborne after the step — ledge or hole. The KCC's snap only
            // reaches [`LOCOMOTION_NPC_SNAP_TO_GROUND`]; use the legacy
            // downward ray, clamped to [`LOCOMOTION_MAX_DROP`], to catch
            // "ground slightly beyond snap range" without teleporting the
            // actor to the bottom of an exterior cell. `None` is correct
            // here, not a gap (#2873, fixed) — the actor's own ~18
            // keyframed bone bodies all carry `ACTOR_BONE_GROUP` and are
            // masked out by the group filter, exactly as in the KCC sweep.
            let ray_origin = Vec3::new(new_pos.x, new_pos.y + LOCOMOTION_GROUND_RAY_UP_OFFSET, new_pos.z);
            if let Some(ground_y) = pw.cast_ray_down(
                ray_origin,
                LOCOMOTION_GROUND_RAY_UP_OFFSET + LOCOMOTION_MAX_DROP,
                None,
            ) {
                new_pos.y = ground_y;
            }
            // No collider hit (synthetic world, or a genuinely bottomless
            // drop) → keep the KCC result rather than snapping to a wrong
            // height.
        }

        if desired_length > 1e-6 {
            let moved_xz = Vec3::new(new_pos.x - current.x, 0.0, new_pos.z - current.z);
            blocked = moved_xz.length() < desired_length * LOCOMOTION_BLOCKED_FRACTION;
        }
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

    (new_pos, rotation, blocked)
}

/// M42.10 — accumulate a blocked-stuck timer and decide whether the
/// oscillating walker should re-pick its target this tick. Shared by
/// `step_oscillating_wander` (Wander) and Patrol's identical conversion so
/// both procedures unstick identically. The `secs` cell is the caller's
/// per-actor [`crate::components::WalkStuckTimer`] scratch; persistence
/// across ticks lives there, not in the save-shaped `WanderState`/
/// `PatrolState`.
///
/// Returns `true` exactly when the timer crossed
/// [`LOCOMOTION_STUCK_REPICK_SECS`] this tick (the caller re-picks and the
/// timer resets). Unblocked ticks clear the timer immediately — brushing a
/// corner for a frame must not poison the next leg.
pub(crate) fn advance_stuck_repick(secs: &mut f32, blocked: bool, dt: f32) -> bool {
    if !blocked {
        *secs = 0.0;
        return false;
    }
    *secs += dt;
    if *secs >= LOCOMOTION_STUCK_REPICK_SECS {
        *secs = 0.0;
        true
    } else {
        false
    }
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
    waypoints: VecDeque<Vec3>,
    goal: Vec3,
    dt: f32,
    speed: f32,
    physics: Option<&byroredux_physics::PhysicsWorld>,
) -> (Vec3, Option<Quat>, VecDeque<Vec3>) {
    let (new_pos, rotation, waypoints, _blocked) = step_along_waypoints_detailed(
        current,
        current_rotation,
        waypoints,
        goal,
        dt,
        speed,
        physics,
    );
    (new_pos, rotation, waypoints)
}

/// [`step_along_waypoints`], plus the underlying step's `blocked` flag (see
/// [`step_toward_detailed`]) — for movers that must notice a KCC-blocked
/// walk and give up on it (#5452) rather than grinding into the obstacle.
pub(crate) fn step_along_waypoints_detailed(
    current: Vec3,
    current_rotation: Quat,
    mut waypoints: VecDeque<Vec3>,
    goal: Vec3,
    dt: f32,
    speed: f32,
    physics: Option<&byroredux_physics::PhysicsWorld>,
) -> (Vec3, Option<Quat>, VecDeque<Vec3>, bool) {
    let step_point = waypoints.front().copied().unwrap_or(goal);
    let step_xz = Vec3::new(step_point.x, current.y, step_point.z);
    let (new_pos, rotation, blocked) =
        step_toward_detailed(current, current_rotation, step_xz, dt, speed, physics);
    pop_reached_waypoint(new_pos, &mut waypoints);
    (new_pos, rotation, waypoints, blocked)
}

#[cfg(test)]
mod stuck_tests {
    use super::advance_stuck_repick;

    /// #5587 — the v0 fallback radius and the walk-speed fallback are one
    /// fact each, not per-procedure copies: the `512.0` literal lives only
    /// in `AI_PACKAGE_DEFAULT_RADIUS` (the five procedure consts alias it),
    /// `64.0` repath thresholds exist only in `follow.rs`, and every
    /// walk-speed read goes through `WalkSpeed::or_default` instead of a
    /// hand-rolled `.map(|s| s.0).unwrap_or(LOCOMOTION_WALK_SPEED)` chain.
    #[test]
    fn ai_package_defaults_have_one_home_each() {
        let procedures = [
            "wander", "patrol", "travel", "guard", "escort", "follow",
            "forcegreet", "eat_sleep", "combat_ai",
        ];
        for name in procedures {
            let path = format!("{}/src/systems/{name}.rs", env!("CARGO_MANIFEST_DIR"));
            let src = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
            assert!(
                !src.contains(": f32 = 512.0"),
                "{name}.rs re-declares the 512.0 default radius — alias \
                 locomotion::AI_PACKAGE_DEFAULT_RADIUS instead (#5587)"
            );
            assert!(
                !src.contains(".map(|s| s.0)\n")
                    || !src.contains("LOCOMOTION_WALK_SPEED"),
                "{name}.rs hand-rolls the walk-speed fallback chain — use \
                 WalkSpeed::or_default (#5587)"
            );
            // A 64.0 that is not a repath threshold (eat_sleep's
            // ARRIVE_RADIUS) is a different fact and stays.
            assert!(
                !src.contains("REPATH_THRESHOLD: f32 = 64.0") || name == "follow",
                "{name}.rs re-declares the 64.0 repath threshold — import \
                 follow.rs::FOLLOW_REPATH_THRESHOLD (#5587)"
            );
        }
        // The shared homes themselves exist and keep their values.
        assert_eq!(super::AI_PACKAGE_DEFAULT_RADIUS, 512.0);
        assert_eq!(super::LOCOMOTION_WALK_SPEED, 100.0);
    }

    #[test]
    fn blocked_ticks_accumulate_and_fire_repick_once_at_the_threshold() {
        let mut secs = 0.0;
        // Unblocked ticks never accumulate and clear any prior accumulation.
        assert!(!advance_stuck_repick(&mut secs, false, 1.0));
        assert_eq!(secs, 0.0);

        // 2 s of continuous blocking at the 2.5 s threshold: not yet.
        assert!(!advance_stuck_repick(&mut secs, true, 1.0));
        assert!(!advance_stuck_repick(&mut secs, true, 1.0));
        assert!((secs - 2.0).abs() < 1e-6);
        // The tick that crosses the threshold fires exactly once and resets.
        assert!(advance_stuck_repick(&mut secs, true, 1.0));
        assert_eq!(secs, 0.0);
        // Post-fire blocking starts accumulating from zero again.
        assert!(!advance_stuck_repick(&mut secs, true, 1.0));
        assert!((secs - 1.0).abs() < 1e-6);
    }

    #[test]
    fn one_unblocked_tick_clears_the_timer() {
        let mut secs = 0.0;
        assert!(!advance_stuck_repick(&mut secs, true, 2.0));
        assert!(!advance_stuck_repick(&mut secs, true, 0.4)); // 2.4 s, just under
        // Brushing a corner for a frame must not poison the next leg.
        assert!(!advance_stuck_repick(&mut secs, false, 0.1));
        assert_eq!(secs, 0.0);
        assert!(!advance_stuck_repick(&mut secs, true, 2.4));
    }

    /// #4689 (PHYS-D4-2026-09-21-01) — the NPC locomotion constants are
    /// DERIVED from their sources (`ContactConfig::DEFAULT`,
    /// `CharacterController::HUMAN`, the fallback-capsule const pair), not
    /// copied. The const initializers make divergence structurally
    /// impossible today; this pin survives a future revert-to-literal and
    /// fails on a one-sided retune (#2885 moved these values once and the
    /// copies would have silently aged, the #2193
    /// blocked-but-never-grounded shape).
    #[allow(clippy::assertions_on_constants)] // intentional cross-crate const pin — a retuned ContactConfig::DEFAULT that violates it must fail here (#2885)
    #[test]
    fn npc_locomotion_constants_match_their_sources() {
        use super::{
            LOCOMOTION_NPC_CAPSULE_HALF_HEIGHT, LOCOMOTION_NPC_CAPSULE_RADIUS,
            LOCOMOTION_NPC_KCC_OFFSET_BU, LOCOMOTION_NPC_MAX_SLOPE_DEG,
            LOCOMOTION_NPC_STEP_HEIGHT, LOCOMOTION_NPC_STEP_MIN_WIDTH,
        };
        use byroredux_physics::{CharacterController, ContactConfig};

        assert_eq!(
            LOCOMOTION_NPC_CAPSULE_HALF_HEIGHT,
            crate::npc_spawn::FALLBACK_ACTOR_CAPSULE_HALF_HEIGHT
        );
        assert_eq!(
            LOCOMOTION_NPC_CAPSULE_RADIUS,
            crate::npc_spawn::FALLBACK_ACTOR_CAPSULE_RADIUS
        );
        assert_eq!(LOCOMOTION_NPC_STEP_HEIGHT, CharacterController::HUMAN.step_height);
        assert_eq!(
            LOCOMOTION_NPC_STEP_MIN_WIDTH,
            CharacterController::HUMAN.step_min_width
        );
        assert_eq!(
            LOCOMOTION_NPC_MAX_SLOPE_DEG,
            CharacterController::HUMAN.max_slope_climb_deg
        );
        assert_eq!(LOCOMOTION_NPC_KCC_OFFSET_BU, ContactConfig::DEFAULT.kcc_offset_bu);
        // The #2885 invariant the derivation inherits, restated here so a
        // retuned `ContactConfig::DEFAULT` that violates it fails on the
        // consumer side too, not just in the physics crate's own pin.
        assert!(
            LOCOMOTION_NPC_KCC_OFFSET_BU > 2.0 * ContactConfig::DEFAULT.default_contact_skin_bu,
            "KCC offset must clear the combined contact skin (#2885)"
        );
    }

    /// #5559 — rapier 0.36's `move_shape` runs `check_and_fix_penetrations`
    /// whenever `|desired| < 1e-5`, pushing the capsule out of every
    /// collider until the offset gap is restored (up to a quarter of the
    /// capsule's height per call); 0.22 had only a commented-out stub. The
    /// boot tick (`scheduler.run(&world, 0.0)`) steps every walking NPC with
    /// `dt = 0`, so a zero `desired` would pop an NPC whose feet are embedded
    /// in its floor by `offset + embed` at spawn.
    ///
    /// It does not, because `step_toward_detailed` never sends a zero
    /// request: its anti-drift vertical correction (`desired += Y·clamp(
    /// −(feet − support − offset), −step, offset)`, #5539) is nonzero for any
    /// NPC not already exactly at the offset gap, so the move is a normal
    /// slide and a `dt = 0` first step is IDENTICAL to a normal first step.
    /// (The audit's probe P2 measured the pop with a bare `Vec3::ZERO`,
    /// which no production caller sends.) Pinned over the embeds the probe
    /// used, so removing the vertical correction — which would route boot
    /// ticks onto the depenetration path — fails here.
    #[test]
    fn a_zero_dt_boot_step_matches_a_normal_first_step_for_embedded_npcs() {
        use super::step_toward_detailed;
        use byroredux_core::math::{Quat, Vec3};
        use byroredux_physics::world::PHYSICS_DT;

        for embed in [0.0_f32, 0.5, 2.0, 6.0] {
            let first_step = |dt: f32| {
                let mut pw = byroredux_physics::PhysicsWorld::new();
                // Top face at y = 0.
                let _floor =
                    pw.spawn_fixed_cuboid(Vec3::new(500.0, 20.0, 500.0), Vec3::new(0.0, -20.0, 0.0));
                let start = Vec3::new(0.0, -embed, 0.0);
                let target = Vec3::new(100.0 * PHYSICS_DT, start.y, 0.0);
                let (next, _, _) =
                    step_toward_detailed(start, Quat::IDENTITY, target, dt, 100.0, Some(&pw));
                next.y - start.y
            };
            let boot = first_step(0.0);
            let normal = first_step(PHYSICS_DT);
            assert!(
                (boot - normal).abs() < 0.05,
                "embed {embed}: the dt = 0 boot step lifts {boot} BU but a normal \
                 first step lifts {normal} — the zero-motion depenetration path fired"
            );
            assert!(
                boot <= super::LOCOMOTION_NPC_KCC_OFFSET_BU + 1e-3,
                "embed {embed}: a boot step must not lift beyond the {} BU offset (got {boot})",
                super::LOCOMOTION_NPC_KCC_OFFSET_BU
            );
        }
    }

    /// #5539 — the NPC analogue of the player's #2857 convex-floor sink
    /// gate, driving the REAL `step_toward_detailed` parameters. Under
    /// rapier 0.36 the snap-to-ground cast fires on every horizontal move
    /// from inside the 4 BU offset band, and against a CONVEX floor it
    /// intermittently returns a large bogus time of impact which snap
    /// applies — the audit's probe measured NPCs sinking > 1 BU on 82/336
    /// height × thickness × heading configurations and below the slab's
    /// bottom face on 63/336. The pre-fix step (a purely horizontal
    /// request) reproduced that here; the probe-bounded vertical request
    /// must hold the capsule within 1 BU of the floor on every
    /// configuration and never through a thin slab.
    #[test]
    fn grounded_npc_does_not_sink_through_convex_floors_across_absolute_y() {
        use super::step_toward_detailed;
        use byroredux_core::math::Vec3;
        use byroredux_physics::world::PHYSICS_DT;

        let speed = 100.0_f32;
        // #2857's floor-height set is absolute-Y sensitive (float precision
        // of the resting gap selects the failure); walk a representative
        // slice of it × 4 headings × a thick and a thin convex slab.
        let heights = [0.0_f32, 137.0, -60.0 + 7.3 * 5.0];
        let headings = [0.0_f32, std::f32::consts::FRAC_PI_2, -0.75, 2.4];
        let thicknesses = [20.0_f32, 4.0];
        for &top in &heights {
            for &half in &thicknesses {
                let mut pw = byroredux_physics::PhysicsWorld::new();
                // `spawn_fixed_cuboid` refreshes the query pipeline; the
                // zero gravity keeps the gate about the KCC snap alone.
                let _floor =
                    pw.spawn_fixed_cuboid(Vec3::new(500.0, half, 500.0), Vec3::new(0.0, top - half, 0.0));
                for &yaw in &headings {
                    // Start grounded: feet `offset` above the top face.
                    let start = Vec3::new(-400.0, top + super::LOCOMOTION_NPC_KCC_OFFSET_BU, 0.0);
                    let dir = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                    let mut pos = start;
                    for frame in 0..240 {
                        let target = pos + dir * (speed * PHYSICS_DT * 2.0);
                        let (next, _, _) = step_toward_detailed(
                            pos,
                            byroredux_core::math::Quat::IDENTITY,
                            Vec3::new(target.x, pos.y, target.z),
                            PHYSICS_DT,
                            speed,
                            Some(&pw),
                        );
                        pos = next;
                        if pos.y < top - 1.0 {
                            panic!(
                                "NPC sank to y={} on a convex floor topped at {top} \
                                 (half {half}, yaw {yaw}, frame {frame})",
                                pos.y
                            );
                        }
                        if pos.y < top - 2.0 * half {
                            panic!(
                                "NPC dropped THROUGH the slab to y={} (top {top}, \
                                 half {half}, yaw {yaw}, frame {frame})",
                                pos.y
                            );
                        }
                    }
                }
            }
        }
    }
}
