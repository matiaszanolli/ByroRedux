//! Runtime boundaries for scripted actor cinematics.
//!
//! Skyrim's MQ101 startup drives Havok idles, vehicle attachment, furniture
//! exits, and rigid-body motion from Papyrus. The renderer does not yet play
//! Havok `.hkx` clips, but these components retain the exact authored request
//! so the gameplay state and the eventual animation backend share one source
//! of truth instead of reducing those calls to no-ops.

use byroredux_core::ecs::components::MotionType;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_core::math::{Quat, Vec3};

/// Animation event awaited by an MQ101 cinematic helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CinematicAnimationEvent {
    PlayImod,
    IdleFurnitureExit,
    ExitCartEnd,
}

/// Per-actor cinematic state written by `PlayIdle`, `SetVehicle`, and the
/// MQ101 `ExitCart` helper.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActorCinematicState {
    /// Live vehicle entity, or `None` after detaching/exiting.
    pub vehicle: Option<EntityId>,
    /// Actor pose relative to the vehicle when attached. The app consumes
    /// these to keep the actor riding a moving cart without ECS parenting
    /// (which would rewrite the authored actor hierarchy).
    pub vehicle_local_translation: Option<Vec3>,
    pub vehicle_local_rotation: Option<Quat>,
    /// Authored cart seat most recently exited (MQ101 uses 1..=5).
    pub cart_seat: Option<u8>,
    /// IDLE record FormID requested by the latest `PlayIdle`-family call.
    pub requested_idle_form_id: Option<u32>,
    /// Monotonic generation so an animation consumer can observe repeated
    /// requests for the same IDLE instead of treating them as unchanged.
    pub idle_request_serial: u64,
    /// Completion event the quest is waiting to receive from the animation.
    pub awaited_event: Option<CinematicAnimationEvent>,
}

impl ActorCinematicState {
    pub fn request_idle(&mut self, idle_form_id: u32) {
        self.requested_idle_form_id = Some(idle_form_id);
        self.idle_request_serial = self.idle_request_serial.wrapping_add(1);
    }
}

impl Component for ActorCinematicState {
    type Storage = SparseSetStorage<Self>;
}

/// A cart hitched by Skyrim's native `ObjectReference.TetherToHorse` call.
///
/// Papyrus supplies the cart as the receiver and the horse as the argument.
/// The app-side cinematic system preserves the captured cart pose relative to
/// the horse, forming the first half of MQ101's movement chain:
/// package-driven horse -> tethered cart -> `SetVehicle` riders.
#[derive(Debug, Clone, PartialEq)]
pub struct HorseTetherState {
    pub horse: EntityId,
    pub horse_local_translation: Vec3,
    pub horse_local_rotation: Quat,
}

impl Component for HorseTetherState {
    type Storage = SparseSetStorage<Self>;
}

/// One-shot request consumed by the binary's Rapier integration system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionTypeChangeRequest {
    pub motion_type: MotionType,
    pub allow_activate: bool,
}

impl Component for MotionTypeChangeRequest {
    type Storage = SparseSetStorage<Self>;
}

/// Engine-wide cinematic presentation state controlled by Skyrim globals and
/// MQ101's animation-event helper functions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CinematicPresentationState {
    pub sitting_rotation_degrees: f32,
    pub player_imod_event_registered: bool,
    pub player_furniture_exit_event_registered: bool,
}

impl Default for CinematicPresentationState {
    fn default() -> Self {
        Self {
            sitting_rotation_degrees: 0.0,
            player_imod_event_registered: false,
            player_furniture_exit_event_registered: false,
        }
    }
}

impl Resource for CinematicPresentationState {}

pub fn register(world: &mut World) {
    world.register::<ActorCinematicState>();
    world.register::<HorseTetherState>();
    world.register::<MotionTypeChangeRequest>();
    if world.try_resource::<CinematicPresentationState>().is_none() {
        world.insert_resource(CinematicPresentationState::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_idle_request_advances_generation() {
        let mut state = ActorCinematicState::default();
        state.request_idle(0x1234);
        state.request_idle(0x1234);

        assert_eq!(state.requested_idle_form_id, Some(0x1234));
        assert_eq!(state.idle_request_serial, 2);
    }

    #[test]
    fn horse_tether_state_retains_authored_relation_and_pose() {
        let state = HorseTetherState {
            horse: 7,
            horse_local_translation: Vec3::new(0.0, 0.0, -140.0),
            horse_local_rotation: Quat::IDENTITY,
        };
        assert_eq!(state.horse, 7);
        assert_eq!(state.horse_local_translation.z, -140.0);
    }
}
