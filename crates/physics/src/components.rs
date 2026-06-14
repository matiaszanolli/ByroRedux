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
    /// (PhysicsWorld's `gravity.y`); doubled here for snappier
    /// arcade-feel jumps (Bethesda-engine convention).
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
    pub const HUMAN: Self = Self {
        half_height: 46.0,
        radius: 18.0,
        eye_height: 52.0,
        move_speed: 220.0,
        jump_velocity: 380.0,
        gravity: -1373.4, // 2× PhysicsWorld earth gravity for snappier feel
        terminal_velocity: -2000.0,
        step_height: 32.0,
        step_min_width: 8.0,
        max_slope_climb_deg: 50.0,
        snap_to_ground: 32.0,
        vertical_velocity: 0.0,
        is_grounded: false,
        wants_jump: false,
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
    /// `(bone entity, rapier body)` for every ragdoll body, in build order.
    pub bodies: Vec<(EntityId, RigidBodyHandle)>,
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
        assert!(
            c.terminal_velocity < c.gravity,
            "terminal velocity must be more negative than 1-frame gravity"
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
}
