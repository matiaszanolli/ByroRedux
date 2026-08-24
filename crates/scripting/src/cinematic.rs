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
use byroredux_plugin::esm::records::{ImadColorKey, ImadRecord, ImadScalarKey};
use std::collections::HashMap;

use crate::quest_stages::{QuestFormId, QuestStageAdvanced, QuestStageState};

/// Animation event awaited by an MQ101 cinematic helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
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
/// an animation callback.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ImageSpaceModifierApplication {
    pub form_id: u32,
    pub strength: f32,
}

/// Fully sampled image-space state for the current frame. Identity values
/// make the renderer path a no-op outside authored cinematic effects.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ImageSpaceModifierFrame {
    pub blur_radius_pixels: f32,
    pub double_vision_strength: f32,
    pub motion_blur_strength: f32,
    pub radial_blur_strength: f32,
    pub radial_blur_ramp_up: f32,
    pub radial_blur_start: f32,
    pub radial_blur_ramp_down: f32,
    pub radial_blur_down_start: f32,
    pub radial_blur_center: [f32; 2],
    pub saturation: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub tint_color: [f32; 4],
    pub fade_color: [f32; 4],
}

impl Default for ImageSpaceModifierFrame {
    fn default() -> Self {
        Self {
            blur_radius_pixels: 0.0,
            double_vision_strength: 0.0,
            motion_blur_strength: 0.0,
            radial_blur_strength: 0.0,
            radial_blur_ramp_up: 0.0,
            radial_blur_start: 0.0,
            radial_blur_ramp_down: 0.0,
            radial_blur_down_start: 1.0,
            radial_blur_center: [0.5, 0.5],
            saturation: 1.0,
            brightness: 1.0,
            contrast: 1.0,
            tint_color: [1.0, 1.0, 1.0, 0.0],
            fade_color: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
struct ActiveImageSpaceModifier {
    application: ImageSpaceModifierApplication,
    elapsed_seconds: f32,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
struct PlayerAnimationEventRegistration {
    quest: QuestFormId,
    image_space_modifiers: Vec<ImageSpaceModifierApplication>,
}

/// Per-actor cinematic state written by `PlayIdle`, `SetVehicle`, and the
/// MQ101 `ExitCart` helper.
///
/// # Save registry (#2380 / SAVE-D1-15)
///
/// The #2294 "believed self-correcting on reload" assumption did NOT hold:
/// `quest_fragment_dispatch_system` is edge-triggered off an acknowledged
/// event journal, so the `SetStage` transition that drove this state never
/// replays on reload, unlike `ScenePlayer`'s `SceneStartRequest` mechanism.
/// Registered via full `register_component` (restore_world preserves
/// entity ids verbatim) but deliberately NOT in `MUTABLE_DELTA_COLUMNS`:
/// `vehicle: Option<EntityId>` is a session-local reference (the `#1696`
/// hazard that excluded `AnimationPlayer.root_entity`), the same
/// FollowState/EscortState treatment.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
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
///
/// # Save registry (#2380 / SAVE-D1-15)
///
/// Same rationale as [`ActorCinematicState`]'s doc comment: registered via
/// full `register_component`, not `MUTABLE_DELTA_COLUMNS` — `horse:
/// EntityId` carries the same session-local-reference hazard.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct HorseTetherState {
    pub horse: EntityId,
    pub horse_local_translation: Vec3,
    pub horse_local_rotation: Quat,
    /// Current XLKR waypoint followed by the horse while this native cart
    /// tether is active. `None` lazily resolves from the horse's placed ref.
    pub route_target_form_id: Option<u32>,
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
///
/// # Save registry (#2380 / SAVE-D1-15)
///
/// Registered as a resource — no `EntityId`/`FixedString` anywhere in this
/// struct, unlike its `ActorCinematicState`/`HorseTetherState` siblings.
/// `image_space_modifier_catalog` is static ESM-derived reference data
/// (populated once by [`install_image_space_modifiers`], no runtime
/// mutator) that rides along redundantly — harmless, and simpler than
/// skip-and-repopulate given a full round-trip's
/// `restore_world`/`restore_resources` path never re-runs cell/plugin
/// load to refill it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct CinematicPresentationState {
    pub sitting_rotation_degrees: f32,
    pub last_player_animation_event: Option<CinematicAnimationEvent>,
    pub player_animation_event_serial: u64,
    /// Historical IMAD applications issued by animation callbacks.
    pub applied_image_space_modifiers: Vec<ImageSpaceModifierApplication>,
    /// Sampled aggregate consumed by the renderer this frame.
    pub image_space_modifier_frame: ImageSpaceModifierFrame,
    image_space_modifier_catalog: HashMap<u32, ImadRecord>,
    active_image_space_modifiers: Vec<ActiveImageSpaceModifier>,
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
            image_space_modifier_frame: ImageSpaceModifierFrame::default(),
            image_space_modifier_catalog: HashMap::new(),
            active_image_space_modifiers: Vec::new(),
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
        presentation.active_image_space_modifiers.extend(
            registration
                .image_space_modifiers
                .iter()
                .copied()
                .map(|application| ActiveImageSpaceModifier {
                    application,
                    elapsed_seconds: 0.0,
                }),
        );
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

/// Replace the runtime IMAD catalog with the resolved plugin load-order view.
pub fn install_image_space_modifiers(
    world: &mut World,
    records: impl IntoIterator<Item = ImadRecord>,
) -> usize {
    if world.try_resource::<CinematicPresentationState>().is_none() {
        world.insert_resource(CinematicPresentationState::default());
    }
    let catalog: HashMap<_, _> = records
        .into_iter()
        .map(|record| (record.form_id, record))
        .collect();
    let count = catalog.len();
    world
        .resource_mut::<CinematicPresentationState>()
        .image_space_modifier_catalog = catalog;
    count
}

/// Sample every active IMAD and advance its authored lifetime. Schedule after
/// animation-event delivery so a callback is visible in the same frame.
pub fn image_space_modifier_system(world: &World, dt: f32) {
    let Some(mut state) = world.try_resource_mut::<CinematicPresentationState>() else {
        return;
    };
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    for active in &mut state.active_image_space_modifiers {
        active.elapsed_seconds += dt;
    }
    state.image_space_modifier_frame = assemble_image_space_frame(
        &state.image_space_modifier_catalog,
        &state.active_image_space_modifiers,
    );

    let CinematicPresentationState {
        image_space_modifier_catalog: catalog,
        active_image_space_modifiers: active,
        ..
    } = &mut *state;
    active.retain(|instance| {
        catalog
            .get(&instance.application.form_id)
            .is_some_and(|record| instance.elapsed_seconds < record.duration_seconds.max(0.0))
    });
}

fn assemble_image_space_frame(
    catalog: &HashMap<u32, ImadRecord>,
    active: &[ActiveImageSpaceModifier],
) -> ImageSpaceModifierFrame {
    let mut frame = ImageSpaceModifierFrame::default();
    let mut radial_center_weight = 0.0;
    let mut radial_center_sum = [0.0; 2];

    for active in active {
        let Some(record) = catalog.get(&active.application.form_id) else {
            continue;
        };
        let strength = active.application.strength;
        if !strength.is_finite() || strength <= 0.0 {
            continue;
        }
        let time = if record.duration_seconds > 0.0 {
            (active.elapsed_seconds / record.duration_seconds).clamp(0.0, 1.0)
        } else {
            1.0
        };

        frame.blur_radius_pixels += sample_scalar(&record.blur_radius, time, 0.0) * strength;
        frame.double_vision_strength +=
            sample_scalar(&record.double_vision_strength, time, 0.0) * strength;
        frame.motion_blur_strength +=
            sample_scalar(&record.motion_blur_strength, time, 0.0) * strength;
        let radial = sample_scalar(&record.radial_blur_strength, time, 0.0) * strength;
        frame.radial_blur_strength += radial;
        frame.radial_blur_ramp_up +=
            sample_scalar(&record.radial_blur_ramp_up, time, 0.0) * strength;
        frame.radial_blur_start += sample_scalar(&record.radial_blur_start, time, 0.0) * strength;
        frame.radial_blur_ramp_down +=
            sample_scalar(&record.radial_blur_ramp_down, time, 0.0) * strength;
        frame.radial_blur_down_start +=
            (sample_scalar(&record.radial_blur_down_start, time, 1.0) - 1.0) * strength;
        if radial.abs() > f32::EPSILON {
            let weight = radial.abs();
            radial_center_sum[0] += record.radial_blur_center[0] * weight;
            radial_center_sum[1] += record.radial_blur_center[1] * weight;
            radial_center_weight += weight;
        }

        frame.saturation *= grade_factor(
            &record.saturation_mult,
            &record.saturation_add,
            time,
            strength,
        );
        frame.brightness *= grade_factor(
            &record.brightness_mult,
            &record.brightness_add,
            time,
            strength,
        );
        frame.contrast *= grade_factor(&record.contrast_mult, &record.contrast_add, time, strength);
        composite_color(
            &mut frame.tint_color,
            sample_color(&record.tint_color, time, [1.0, 1.0, 1.0, 0.0]),
            strength,
        );
        composite_color(
            &mut frame.fade_color,
            sample_color(&record.fade_color, time, [0.0; 4]),
            strength,
        );
    }

    if radial_center_weight > 0.0 {
        frame.radial_blur_center = [
            radial_center_sum[0] / radial_center_weight,
            radial_center_sum[1] / radial_center_weight,
        ];
    }
    frame
}

fn grade_factor(mult: &[ImadScalarKey], add: &[ImadScalarKey], time: f32, strength: f32) -> f32 {
    let authored = sample_scalar(mult, time, 1.0) + sample_scalar(add, time, 0.0);
    (1.0 + (authored - 1.0) * strength).max(0.0)
}

/// Shared keyed-linear-interpolation control flow behind `sample_scalar` /
/// `sample_color` (#2260 / TD2-101): before the first key holds the first
/// key's value; scans `keys.windows(2)` for the bracketing pair and
/// clamp-lerps between them by fractional distance (degenerate
/// zero-width pairs snap to the right key); after the last key holds the
/// last key's value. `time_of`/`value_of` extract the two fields a key
/// type carries; `lerp` interpolates the value type — the only thing that
/// actually differs between a scalar and an RGBA channel.
fn sample_keyed<T, V: Copy>(
    keys: &[T],
    time: f32,
    default: V,
    time_of: impl Fn(&T) -> f32,
    value_of: impl Fn(&T) -> V,
    lerp: impl Fn(V, V, f32) -> V,
) -> V {
    let Some(first) = keys.first() else {
        return default;
    };
    if time <= time_of(first) {
        return value_of(first);
    }
    for pair in keys.windows(2) {
        let [left, right] = pair else { unreachable!() };
        let right_time = time_of(right);
        if time <= right_time {
            let left_time = time_of(left);
            let width = right_time - left_time;
            let alpha = if width.abs() <= f32::EPSILON {
                1.0
            } else {
                ((time - left_time) / width).clamp(0.0, 1.0)
            };
            return lerp(value_of(left), value_of(right), alpha);
        }
    }
    keys.last().map_or(default, value_of)
}

fn sample_scalar(keys: &[ImadScalarKey], time: f32, default: f32) -> f32 {
    sample_keyed(
        keys,
        time,
        default,
        |k| k.time,
        |k| k.value,
        |left, right, alpha| left + (right - left) * alpha,
    )
}

fn sample_color(keys: &[ImadColorKey], time: f32, default: [f32; 4]) -> [f32; 4] {
    sample_keyed(
        keys,
        time,
        default,
        |k| k.time,
        |k| k.color,
        |left, right, alpha| std::array::from_fn(|i| left[i] + (right[i] - left[i]) * alpha),
    )
}

fn composite_color(target: &mut [f32; 4], source: [f32; 4], strength: f32) {
    let alpha = (source[3] * strength).clamp(0.0, 1.0);
    let prior_alpha = target[3].clamp(0.0, 1.0);
    let combined_alpha = alpha + prior_alpha * (1.0 - alpha);
    if combined_alpha > f32::EPSILON {
        for channel in 0..3 {
            target[channel] = (source[channel] * alpha
                + target[channel] * prior_alpha * (1.0 - alpha))
                / combined_alpha;
        }
    }
    target[3] = combined_alpha;
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
            route_target_form_id: Some(0x1234),
        };
        assert_eq!(state.horse, 7);
        assert_eq!(state.horse_local_translation.z, -140.0);
        assert_eq!(state.route_target_form_id, Some(0x1234));
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

    #[test]
    fn scalar_curves_interpolate_and_hold_endpoints() {
        let keys = [
            ImadScalarKey {
                time: 0.0,
                value: 0.0,
            },
            ImadScalarKey {
                time: 0.5,
                value: 4.0,
            },
            ImadScalarKey {
                time: 1.0,
                value: 2.0,
            },
        ];
        assert_eq!(sample_scalar(&keys, -1.0, 9.0), 0.0);
        assert_eq!(sample_scalar(&keys, 0.25, 9.0), 2.0);
        assert_eq!(sample_scalar(&keys, 0.75, 9.0), 3.0);
        assert_eq!(sample_scalar(&keys, 2.0, 9.0), 2.0);
        assert_eq!(sample_scalar(&[], 0.5, 9.0), 9.0);
    }

    #[test]
    fn callback_drives_timed_image_space_frame_then_expires() {
        const IMAD: u32 = 0x0010_1DAC;
        let mut world = World::new();
        register(&mut world);
        world.insert_resource(QuestStageState::default());
        let curve = vec![
            ImadScalarKey {
                time: 0.0,
                value: 0.0,
            },
            ImadScalarKey {
                time: 0.5,
                value: 4.0,
            },
            ImadScalarKey {
                time: 1.0,
                value: 0.0,
            },
        ];
        install_image_space_modifiers(
            &mut world,
            [ImadRecord {
                form_id: IMAD,
                duration_seconds: 2.0,
                radial_blur_center: [0.25, 0.75],
                blur_radius: curve.clone(),
                radial_blur_strength: vec![ImadScalarKey {
                    time: 0.5,
                    value: 1.0,
                }],
                saturation_mult: vec![ImadScalarKey {
                    time: 0.5,
                    value: 0.5,
                }],
                tint_color: vec![ImadColorKey {
                    time: 0.5,
                    color: [0.5, 0.75, 1.0, 0.4],
                }],
                ..Default::default()
            }],
        );
        world
            .resource_mut::<CinematicPresentationState>()
            .register_player_animation_event(
                CinematicAnimationEvent::PlayImod,
                QuestFormId(0x0003_372B),
                vec![ImageSpaceModifierApplication {
                    form_id: IMAD,
                    strength: 1.0,
                }],
            );
        dispatch_player_cinematic_animation_event(&world, CinematicAnimationEvent::PlayImod)
            .expect("registered callback");

        image_space_modifier_system(&world, 1.0);
        let frame = world
            .resource::<CinematicPresentationState>()
            .image_space_modifier_frame;
        assert_eq!(frame.blur_radius_pixels, 4.0);
        assert_eq!(frame.radial_blur_strength, 1.0);
        assert_eq!(frame.radial_blur_center, [0.25, 0.75]);
        assert_eq!(frame.saturation, 0.5);
        assert!((frame.tint_color[3] - 0.4).abs() < 1.0e-6);

        image_space_modifier_system(&world, 1.1);
        assert_eq!(
            world
                .resource::<CinematicPresentationState>()
                .image_space_modifier_frame
                .blur_radius_pixels,
            0.0
        );
        image_space_modifier_system(&world, 0.0);
        assert_eq!(
            world
                .resource::<CinematicPresentationState>()
                .image_space_modifier_frame,
            ImageSpaceModifierFrame::default()
        );
    }

    /// Regression for #2260 (TD2-101): pin `sample_scalar`'s keyed-lerp
    /// contract directly now that it's a thin wrapper over the shared
    /// `sample_keyed` helper — before-first-key, exact hit, interpolated
    /// midpoint, after-last-key, degenerate zero-width pair, and the
    /// empty-keys default all in one place so a future edit to the shared
    /// helper can't silently change one value type's behavior without
    /// this catching it.
    #[test]
    fn sample_scalar_covers_the_full_keyed_lerp_contract() {
        assert_eq!(sample_scalar(&[], 5.0, 9.0), 9.0, "empty keys use default");

        let keys = [
            ImadScalarKey {
                time: 1.0,
                value: 10.0,
            },
            ImadScalarKey {
                time: 3.0,
                value: 30.0,
            },
            ImadScalarKey {
                time: 5.0,
                value: 50.0,
            },
        ];
        assert_eq!(
            sample_scalar(&keys, 0.0, 0.0),
            10.0,
            "before the first key holds the first key's value"
        );
        assert_eq!(sample_scalar(&keys, 1.0, 0.0), 10.0, "exact first-key hit");
        assert_eq!(
            sample_scalar(&keys, 2.0, 0.0),
            20.0,
            "interpolated midpoint between key 0 and key 1"
        );
        assert_eq!(
            sample_scalar(&keys, 6.0, 0.0),
            50.0,
            "after the last key holds the last key's value"
        );

        // A repeated timestamp (zero-width pair) at the query time is
        // caught by the earlier "before/at first key" and "at this pair's
        // right edge" checks before the `width.abs() <= EPSILON` guard
        // itself can be reached — querying exactly at the shared time
        // must still resolve to a single, well-defined value rather than
        // panicking or dividing by zero.
        let repeated_time = [
            ImadScalarKey {
                time: 2.0,
                value: 20.0,
            },
            ImadScalarKey {
                time: 2.0,
                value: 25.0,
            },
            ImadScalarKey {
                time: 5.0,
                value: 50.0,
            },
        ];
        assert_eq!(
            sample_scalar(&repeated_time, 2.0, 0.0),
            20.0,
            "querying exactly at a repeated timestamp resolves to the first matching key"
        );
    }

    /// Same contract, per-channel, for the color sampler — the only
    /// difference between it and `sample_scalar` should be the value type.
    #[test]
    fn sample_color_covers_the_full_keyed_lerp_contract() {
        assert_eq!(
            sample_color(&[], 5.0, [9.0; 4]),
            [9.0; 4],
            "empty keys use default"
        );

        let keys = [
            ImadColorKey {
                time: 1.0,
                color: [0.0, 0.0, 0.0, 0.0],
            },
            ImadColorKey {
                time: 2.0,
                color: [1.0, 1.0, 1.0, 1.0],
            },
        ];
        assert_eq!(
            sample_color(&keys, 0.0, [0.0; 4]),
            [0.0, 0.0, 0.0, 0.0],
            "before the first key holds the first key's value"
        );
        assert_eq!(
            sample_color(&keys, 1.5, [0.0; 4]),
            [0.5, 0.5, 0.5, 0.5],
            "interpolated midpoint, per channel"
        );
        assert_eq!(
            sample_color(&keys, 5.0, [0.0; 4]),
            [1.0, 1.0, 1.0, 1.0],
            "after the last key holds the last key's value"
        );
    }
}
