//! ECS components carrying Rapier handles.

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use rapier3d::prelude::{ColliderHandle, MultibodyJointHandle, RigidBodyHandle};

/// Handles into the `PhysicsWorld` Rapier sets for one simulated entity.
///
/// Inserted by `physics_sync_system` Phase 1 when it registers a new
/// body. Absence of this component is the signal that an entity with
/// `CollisionShape` + `RigidBodyData` still needs to be registered.
#[derive(Debug, Clone, Copy)]
pub struct RapierHandles {
    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,
}

impl Component for RapierHandles {
    type Storage = SparseSetStorage<Self>;
}

/// Marks an entity whose collider belongs to a **live actor's skeleton** —
/// one of the ~18 ragdoll bone bodies `keyframe_live_ragdoll_bones` flips
/// from Dynamic to Keyframed before first registration (#1698).
///
/// `physics_sync_system` Phase 1 reads this to put the resulting colliders in
/// [`crate::ACTOR_BONE_GROUP`], which every downward floor probe masks out.
/// Without it a ground-snap ray cast from above an actor's root hits the
/// actor's own upper-body bone instead of the floor, and because the bones
/// are driven from the root's animated transform the actor climbs a little
/// further every tick — a monotonic elevator, not a fixed offset (#2873).
///
/// Purely a query-side label: contact generation is unaffected (the bones'
/// collision *filter* mask stays `Group::ALL`), and death-time ragdoll
/// activation rebuilds its own simulated bodies regardless.
#[derive(Debug, Clone, Copy)]
pub struct ActorBoneCollider;

impl Component for ActorBoneCollider {
    type Storage = SparseSetStorage<Self>;
}

/// Canonical actor-root ownership for a live skeleton collider.
///
/// Ray casts return Rapier body handles, which resolve to the individual bone
/// entity carrying that body. Combat needs the placement root that owns
/// ActorValues and lifecycle state; storing it at skeleton registration makes
/// that resolution direct and independent of scene-graph name/form heuristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorColliderOwner(pub EntityId);

impl Component for ActorColliderOwner {
    type Storage = SparseSetStorage<Self>;
}

/// Kinematic character-controller body (M28.5). The high-level
/// player rig — combines the capsule shape used by the physics layer
/// with the movement-state fields the per-frame controller system
/// reads/writes.
///
/// **Lifecycle**:
///   - At entity spawn the controller carries authored capsule dims,
///     movement params, and zero-initialised runtime state.
///   - `physics_sync_system::Phase 1` (Path C) sees the marker,
///     registers a `RigidBodyType::KinematicPositionBased` body +
///     capsule collider with `RapierHandles`.
///   - Each frame `character_controller_system` integrates gravity
///     + jump, asks Rapier's `KinematicCharacterController.move_shape`
///       for the collide-and-slide-corrected motion, and writes the
///       resulting translation onto the kinematic body via
///       `set_next_kinematic_translation`. Runtime state
///       (`vertical_velocity`, `is_grounded`) is updated in-place.
///   - `camera_follow_system` reads body position + `eye_height` to
///     place the active camera each frame after `physics_sync_system`
///     applies the kinematic step.
///
/// **Coordinate frame**: capsule is `capsule_y` (vertical), so
/// `half_height` excludes the hemispherical caps — total visible
/// height = `2 * (half_height + radius)`. Default `HUMAN` matches
/// vanilla Skyrim actor-capsule dimensions (128 BU tall, 36 BU wide).
#[derive(Debug, Clone, Copy)]
pub struct CharacterController {
    // ── Shape ────────────────────────────────────────────────────
    /// Capsule half-height (Y-axis), excludes caps. BU.
    pub half_height: f32,
    /// Capsule radius. BU.
    pub radius: f32,

    // ── Camera mount ─────────────────────────────────────────────
    /// Camera offset above body centre. Typical eye height for a
    /// 144 BU humanoid: ~58 BU above the centre (so eyes at
    /// `half_height - 14` BU below the top), matching Bethesda
    /// 1st-person camera defaults.
    pub eye_height: f32,

    // ── Movement params ──────────────────────────────────────────
    /// Horizontal speed when WASD is held. BU/sec.
    pub move_speed: f32,
    /// Initial vertical velocity on jump trigger. BU/sec.
    pub jump_velocity: f32,
    /// Downward acceleration. BU/sec². Earth gravity ≈ -686.7 BU/sec²
    /// (PhysicsWorld's `gravity.y`); scaled here for arcade-feel jumps
    /// (Bethesda-engine convention).
    pub gravity: f32,
    /// Cap on downward velocity so terminal-velocity falls don't
    /// tunnel through thin floors at high frame_dt. BU/sec.
    pub terminal_velocity: f32,

    // ── KCC tuning ───────────────────────────────────────────────
    /// Auto-step max climb height. BU. Bethesda stairs are typically
    /// 16-24 BU per step; 32 BU covers all canonical interior
    /// architecture.
    pub step_height: f32,
    /// Auto-step minimum platform width (tread depth in the direction
    /// of movement). BU. Rapier only steps up when the surface above
    /// the obstacle extends at least this far. FNV doorstep treads are
    /// typically 8-16 BU; using capsule_radius (18 BU) here blocks
    /// autostep on narrow thresholds.
    pub step_min_width: f32,
    /// Max slope angle the character can walk up. Above this, slides
    /// down. 50° matches Bethesda's NavMesh slope limit.
    pub max_slope_climb_deg: f32,
    /// Ground-snap distance. BU. Holds the character on terrain
    /// rolls without bouncing per-step. Rapier KCC engages this on
    /// the grounded → airborne transition: if the next frame's
    /// motion would leave the character ungrounded but solid ground
    /// exists within this distance below, the KCC pulls the body
    /// down to maintain contact. Must be ≥ `step_height` to handle
    /// Bethesda interior floor TriMesh gaps (~1-2 BU vertex stitching
    /// errors between adjacent floor tiles in stock content — the
    /// classic Whiterun Bannered Mare plank gap that drops a 0.5
    /// BU offset capsule straight through to the void).
    pub snap_to_ground: f32,

    // ── Runtime state (written by character_controller_system) ───
    /// Current vertical velocity. Resets to 0 on ground contact and
    /// to `jump_velocity` on jump trigger.
    pub vertical_velocity: f32,
    /// Set by `KCC.move_shape`'s `EffectiveCharacterMovement.grounded`
    /// every frame.
    pub is_grounded: bool,
    /// Set true by input handler when jump key is hit; consumed
    /// (cleared) by `character_controller_system` after applying.
    /// Avoids double-jumps from repeat-key autorepeat.
    pub wants_jump: bool,
    /// Remaining breath while the player's head is submerged. Seconds.
    /// The character system replenishes this at the surface and applies
    /// drowning damage only after it reaches zero.
    pub breath_remaining: f32,
    /// Accumulated drowning damage is kept on the controller so save/load and
    /// fixed-step updates do not lose fractional damage between ticks.
    pub drowning_damage_accumulator: f32,
}

impl CharacterController {
    /// Vanilla-Skyrim-sized humanoid character — 128 BU tall, 36 BU
    /// wide (matches CommonLib's `bhkCharController` for a Nord male),
    /// 50° slope, 32 BU step, 220 BU/sec walk speed (~3.14 m/s,
    /// Skyrim's documented player walk speed).
    ///
    /// `eye_height = 52` puts the camera 116 BU above feet on a 128 BU
    /// capsule — matches Skyrim's 1st-person eye height. The test
    /// `character_controller_human_dimensions` asserts `eye_height <
    /// half_height + radius` to keep the eye inside the visible capsule.
    /// Worst-case frame `dt` the character integration is allowed to see.
    ///
    /// `character_controller_system` clamps its incoming `dt` to this before
    /// integrating: a frame hitch must degrade to "freeze for one tick", never
    /// to "teleport across the cell". Frames slower than 30 fps read as
    /// hitches anyway.
    ///
    /// Lives here rather than in the consumer (#2886) so
    /// `character_controller_human_dimensions` can state the
    /// terminal-velocity invariant in the units it actually holds in —
    /// `gravity × dt` is a velocity, `gravity` alone is not.
    pub const MAX_FRAME_DT: f32 = 1.0 / 30.0;

    pub const HUMAN: Self = Self {
        half_height: 46.0,
        radius: 18.0,
        eye_height: 52.0,
        move_speed: 220.0,
        // Jump apex height h = v0²/(2·|g|); hang time t = 2·v0/|g|.
        // Tuned for 2× the original jump height (52.6 → 105.2 BU) and
        // 1.5× the original hang time (0.55s → 0.83s) simultaneously:
        // solving h_new/h_old=2, t_new/t_old=1.5 gives v0 ×= 4/3 and
        // |g| ×= 8/9 relative to the original 380.0 / -1373.4 pair.
        jump_velocity: 506.6667,
        gravity: -1220.8,
        terminal_velocity: -2000.0,
        step_height: 32.0,
        step_min_width: 8.0,
        max_slope_climb_deg: 50.0,
        snap_to_ground: 32.0,
        vertical_velocity: 0.0,
        is_grounded: false,
        wants_jump: false,
        breath_remaining: 15.0,
        drowning_damage_accumulator: 0.0,
    };
}

impl Component for CharacterController {
    type Storage = SparseSetStorage<Self>;
}

/// An active Havok ragdoll running on our Rapier solver (M41.x).
///
/// Attached to the actor (placement) entity by the `ragdoll` console
/// command via [`crate::ragdoll::build_ragdoll`]. Holds the mapping from
/// each skeleton bone `EntityId` to its Rapier rigid body so the
/// per-frame writeback can copy simulated poses back onto the bone
/// entities (which the skinned mesh already reads). `joints` is retained
/// only for teardown bookkeeping; removal cascades through
/// [`crate::world::PhysicsWorld::remove_ragdoll`].
#[derive(Debug, Clone)]
pub struct Ragdoll {
    /// `(bone entity, rapier body, seed-time bone scale)` for every ragdoll
    /// body, in build order. The third element is a snapshot of the bone's
    /// `GlobalTransform.scale` at activation (`RagdollBodySpec::scale`) —
    /// the per-frame writeback must decompose the simulated pose using
    /// this snapshot, not a fresh live `GlobalTransform` read, or a bone
    /// rescaled after activation drifts. See #1852.
    pub bodies: Vec<(EntityId, RigidBodyHandle, f32)>,
    /// Multibody joint handles created for this ragdoll.
    pub joints: Vec<MultibodyJointHandle>,
}

impl Component for Ragdoll {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_controller_human_dimensions() {
        let c = CharacterController::HUMAN;
        assert!(c.half_height > 0.0);
        assert!(c.radius > 0.0);
        assert!(c.eye_height > 0.0);
        assert!(
            c.eye_height < c.half_height + c.radius,
            "eyes must sit inside capsule"
        );
        assert!(c.move_speed > 0.0);
        assert!(c.jump_velocity > 0.0);
        assert!(c.gravity < 0.0, "gravity is downward (negative Y)");
        assert!(c.terminal_velocity < 0.0);
        // #2886 — this used to read `terminal_velocity < gravity`, comparing
        // a BU/s velocity against a BU/s² acceleration. It happened to hold,
        // but it did not test what its message claimed. The real invariant is
        // that the clamp must be unreachable in a single worst-case frame, so
        // terminal velocity is a multi-frame free-fall limit rather than an
        // instantaneous cap on the very first integration step.
        let one_frame_of_gravity = c.gravity * CharacterController::MAX_FRAME_DT;
        assert!(
            c.terminal_velocity < one_frame_of_gravity,
            "terminal velocity ({}) must be more negative than one worst-case \
             frame of gravity ({} BU/s = {} BU/s² × {} s), or the clamp fires \
             on the first airborne tick instead of bounding free fall",
            c.terminal_velocity,
            one_frame_of_gravity,
            c.gravity,
            CharacterController::MAX_FRAME_DT,
        );
        assert!(c.step_height > 0.0);
        assert!(c.step_min_width > 0.0);
        assert!(c.max_slope_climb_deg > 0.0 && c.max_slope_climb_deg < 90.0);
    }

    #[test]
    fn character_controller_default_runtime_state_is_zero() {
        let c = CharacterController::HUMAN;
        assert_eq!(c.vertical_velocity, 0.0);
        assert!(!c.is_grounded);
        assert!(!c.wants_jump);
    }

    #[test]
    fn actor_collider_owner_preserves_actor_root() {
        assert_eq!(ActorColliderOwner(42).0, 42);
    }
}
