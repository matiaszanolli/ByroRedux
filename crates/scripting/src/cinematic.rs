//! Runtime boundaries for scripted actor cinematics.
//!
//! Skyrim's MQ101 startup drives Havok idles, vehicle attachment, furniture
//! exits, and rigid-body motion from Papyrus. The binary resolves and plays
//! the supported Skyrim IDLEs through its archive-backed HKX catalog; these
//! components retain authored requests, vehicle-relative exit orientation,
//! and behavior-event delivery while scripting stays backend-independent.

use byroredux_core::ecs::components::MotionType;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_core::math::{Quat, Vec3};

use crate::quest_stages::{QuestFormId, QuestStageAdvanced, QuestStageState};

/// Animation event awaited by an MQ101 cinematic helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CinematicAnimationEvent {
    PlayImod,
    IdleFurnitureExit,
    ExitCartEnd,
}

impl CinematicAnimationEvent {
    fn mq101_callback_stage(self) -> Option<u16> {
        match self {
            Self::PlayImod => Some(145),
            Self::IdleFurnitureExit => Some(160),
            Self::ExitCartEnd => None,
        }
    }
}

/// One vanilla `ImageSpaceModifier.Apply(strength)` invocation delivered by
/// an animation callback. The renderer does not interpret IMAD curves yet;
/// retaining the concrete FormID and authored strength keeps the presentation
/// request explicit instead of losing it at the scripting boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageSpaceModifierApplication {
    pub form_id: u32,
    pub strength: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct PlayerAnimationEventRegistration {
    quest: QuestFormId,
    image_space_modifiers: Vec<ImageSpaceModifierApplication>,
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
    /// World orientation captured while detaching from a vehicle. Root-motion
    /// deltas use it until the exit clip reaches `ExitCartEnd`.
    pub exit_root_motion_rotation: Option<Quat>,
    /// Most recent behavior-level animation event delivered for this actor.
    pub last_animation_event: Option<CinematicAnimationEvent>,
    /// Monotonic delivery generation, including repeated identical events.
    pub animation_event_serial: u64,
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
#[derive(Debug, Clone, PartialEq)]
pub struct CinematicPresentationState {
    pub sitting_rotation_degrees: f32,
    pub last_player_animation_event: Option<CinematicAnimationEvent>,
    pub player_animation_event_serial: u64,
    /// IMAD applications issued by callbacks and awaiting a renderer-side
    /// image-space implementation.
    pub applied_image_space_modifiers: Vec<ImageSpaceModifierApplication>,
    player_imod_event: Option<PlayerAnimationEventRegistration>,
    player_furniture_exit_event: Option<PlayerAnimationEventRegistration>,
}

impl Default for CinematicPresentationState {
    fn default() -> Self {
        Self {
            sitting_rotation_degrees: 0.0,
            last_player_animation_event: None,
            player_animation_event_serial: 0,
            applied_image_space_modifiers: Vec::new(),
            player_imod_event: None,
            player_furniture_exit_event: None,
        }
    }
}

impl Resource for CinematicPresentationState {}

impl CinematicPresentationState {
    /// Register one of MQ101's player animation callbacks. Re-registering the
    /// same event replaces the previous one, matching Papyrus subscription
    /// identity `(listener, source, event-name)`.
    pub fn register_player_animation_event(
        &mut self,
        event: CinematicAnimationEvent,
        quest: QuestFormId,
        image_space_modifiers: Vec<ImageSpaceModifierApplication>,
    ) -> bool {
        let registration = PlayerAnimationEventRegistration {
            quest,
            image_space_modifiers,
        };
        match event {
            CinematicAnimationEvent::PlayImod => self.player_imod_event = Some(registration),
            CinematicAnimationEvent::IdleFurnitureExit => {
                self.player_furniture_exit_event = Some(registration)
            }
            CinematicAnimationEvent::ExitCartEnd => return false,
        }
        true
    }

    pub fn is_player_animation_event_registered(&self, event: CinematicAnimationEvent) -> bool {
        match event {
            CinematicAnimationEvent::PlayImod => self.player_imod_event.is_some(),
            CinematicAnimationEvent::IdleFurnitureExit => {
                self.player_furniture_exit_event.is_some()
            }
            CinematicAnimationEvent::ExitCartEnd => false,
        }
    }

    fn take_player_animation_event(
        &mut self,
        event: CinematicAnimationEvent,
    ) -> Option<PlayerAnimationEventRegistration> {
        match event {
            CinematicAnimationEvent::PlayImod => self.player_imod_event.take(),
            CinematicAnimationEvent::IdleFurnitureExit => self.player_furniture_exit_event.take(),
            CinematicAnimationEvent::ExitCartEnd => None,
        }
    }
}

/// Invoke the vanilla MQ101 quest callback registered for a player animation
/// event. Both handlers are one-shot: they advance their owning quest to the
/// stage proven by `MQ101QuestScript.OnAnimationEvent`, then unregister.
pub fn dispatch_player_cinematic_animation_event(
    world: &World,
    event: CinematicAnimationEvent,
) -> Option<QuestStageAdvanced> {
    let target_stage = event.mq101_callback_stage()?;
    // Do not consume a one-shot registration if canonical quest state is not
    // installed. Structural resources cannot disappear while `&World` lives,
    // so the subsequent mutable lookup is guaranteed to remain available.
    world.try_resource::<QuestStageState>()?;

    let registration = {
        let mut presentation = world.try_resource_mut::<CinematicPresentationState>()?;
        let registration = presentation.take_player_animation_event(event)?;
        presentation.last_player_animation_event = Some(event);
        presentation.player_animation_event_serial =
            presentation.player_animation_event_serial.wrapping_add(1);
        presentation
            .applied_image_space_modifiers
            .extend(registration.image_space_modifiers.iter().copied());
        registration
    };

    let mut stages = world.resource_mut::<QuestStageState>();
    let previous_stage = stages.set_stage(registration.quest, target_stage);
    Some(QuestStageAdvanced {
        quest: registration.quest,
        previous_stage,
        new_stage: target_stage,
    })
}

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

    #[test]
    fn mq101_player_callbacks_apply_stage_and_unregister_once() {
        let mut world = World::new();
        register(&mut world);
        world.insert_resource(QuestStageState::default());
        let quest = QuestFormId(0x0003_372B);
        let applications = vec![
            ImageSpaceModifierApplication {
                form_id: 0x0010_1DAC,
                strength: 1.0,
            },
            ImageSpaceModifierApplication {
                form_id: 0x0010_CDC7,
                strength: 1.0,
            },
        ];
        world
            .resource_mut::<CinematicPresentationState>()
            .register_player_animation_event(
                CinematicAnimationEvent::PlayImod,
                quest,
                applications.clone(),
            );

        let advance =
            dispatch_player_cinematic_animation_event(&world, CinematicAnimationEvent::PlayImod)
                .expect("registered callback");
        assert_eq!(advance.new_stage, 145);
        assert_eq!(world.resource::<QuestStageState>().get_stage(quest), 145);

        let presentation = world.resource::<CinematicPresentationState>();
        assert!(
            !presentation.is_player_animation_event_registered(CinematicAnimationEvent::PlayImod)
        );
        assert_eq!(presentation.applied_image_space_modifiers, applications);
        assert_eq!(presentation.player_animation_event_serial, 1);
        drop(presentation);

        assert!(dispatch_player_cinematic_animation_event(
            &world,
            CinematicAnimationEvent::PlayImod
        )
        .is_none());
        assert_eq!(world.resource::<QuestStageState>().get_stage(quest), 145);
    }
}
