//! R5 — Papyrus quest prototype: hand-translation of one real Skyrim
//! script into the ECS-native shape, to validate or refute the
//! "no-VM" bet documented at `ROADMAP.md` Tier 3 / R5.
//!
//! The demo is a faithful translation of
//! `defaultRumbleOnActivate.psc` (47 LOC of Papyrus, ships attached to
//! hundreds of vanilla Skyrim references — pressure plates, activator
//! buttons, shrines). The source `.psc` is reproduced under
//! `docs/r5/source/defaultRumbleOnActivate.psc` for reference. The
//! evaluation lives at `docs/r5-evaluation.md`.
//!
//! ## Why this script
//!
//! The R5 spec calls for one quest exercising:
//!
//! - **latent `Utility.Wait()`** — handled by [`RumbleState::Busy`]'s
//!   `wait_remaining_secs` field. The script's suspended-stack frame
//!   becomes plain data inside the component.
//! - **state changes** — Papyrus's `Auto State active / State busy /
//!   State inactive` becomes the [`RumbleState`] enum. The state-keyed
//!   `Event OnActivate` dispatch (which event body runs depends on
//!   current state) becomes a `match` inside
//!   [`rumble_on_activate_system`].
//! - **cross-subsystem callback** — `Game.shakeCamera()` and
//!   `Game.shakeController()` become marker components
//!   ([`CameraShakeCommand`] / [`ControllerRumbleCommand`]) emitted on
//!   the player entity for the engine's camera / input subsystems to
//!   consume. Until those subsystems land they accumulate as
//!   inspectable evidence — the test surface uses them as the
//!   observable proof the script fired.
//!
//! ## What this proves (or doesn't)
//!
//! - The full script translates to ~30 lines of Rust components +
//!   ~40 lines of systems. No interpreter, no stack-frame heap, no
//!   state-string parsing. Deterministic per-frame, no scheduler
//!   surprises.
//! - The latent-wait pattern collapses into the state field — no need
//!   for the existing [`crate::ScriptTimer`] abstraction at all when
//!   the wait is the script's only suspendable operation.
//! - The state machine is exhaustive at compile time. Pre-fix
//!   Papyrus's `GoToState("active")` was a runtime string lookup that
//!   couldn't catch typos until that branch fired.
//! - What this does NOT cover yet:
//!     * **user-script-to-user-script call across references** (Papyrus
//!       `someActor.MyScript.DoSomething()`). The R5 candidate doesn't
//!       exercise it; documented as outstanding in
//!       `docs/r5-evaluation.md`.
//!     * **`RegisterFor*` event subscriptions** — Papyrus's
//!       broadcast-pub/sub. Not in this script.
//!     * **Quest stages / `SetStage` cross-quest state**. Not in this
//!       script either; covered by the secondary fixture
//!       `MG07LabyrinthianDoorScript.psc` (also at
//!       `docs/r5/source/`) which is left untranslated as future work.

use crate::events::ActivateEvent;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;

/// Resource: which entity Papyrus's `Game.shakeCamera(None, …)` /
/// `Game.shakeController(…)` resolves to.
///
/// Papyrus's `Game` pseudo-singleton always targets the player camera
/// / controller. In ECS we model "the player" as an explicit entity
/// reference handed out at world init; the alternative (a
/// `CameraShakeQueue` resource) was ruled out because it would force
/// every camera-shake source to share one queue and lose per-frame
/// causal ordering against the script systems.
#[derive(Debug, Clone, Copy)]
pub struct PlayerEntity(pub EntityId);

impl byroredux_core::ecs::resource::Resource for PlayerEntity {}

/// Translation of `defaultRumbleOnActivate.psc`.
///
/// Attach one of these to any reference that the .psc was bound to in
/// the original ESP (terminal pedestals, ritual buttons, shrines, …).
/// Properties are the same shape as Bethesda's `VMAD` subrecord —
/// the values copy out of the ESM at REFR-script-attach time. Default
/// values mirror the script's `Property X = 0.25 Auto` defaults so a
/// REFR with no VMAD override behaves identically to one with all
/// defaults.
///
/// Component is the entire script. There is no separate "script
/// instance" object: the component IS the instance. Property values
/// + `state` = total observable script state.
#[derive(Debug, Clone, Copy)]
pub struct RumbleOnActivate {
    /// Papyrus `Float Property cameraIntensity = 0.25 Auto`.
    pub camera_intensity: f32,
    /// Papyrus `Float Property duration = 0.25 Auto`. Also the
    /// `Utility.wait(duration)` argument — Papyrus reuses the same
    /// property for the controller rumble length and the post-shake
    /// wait, so a single Rust field covers both call sites.
    pub duration: f32,
    /// Papyrus `Bool Property repeatable = True Auto`. When `true`,
    /// the post-wait transition returns to `Active`; when `false`, it
    /// falls through to `Inactive` (one-shot).
    pub repeatable: bool,
    /// Papyrus `Float Property shakeLeft = 0.25 Auto`.
    pub shake_left: f32,
    /// Papyrus `Float Property shakeRight = 0.25 Auto`.
    pub shake_right: f32,
    /// Translation of Papyrus's `Auto State active / busy / inactive`
    /// trio. The current state replaces Papyrus's GoToState-string +
    /// per-state event dispatch.
    pub state: RumbleState,
}

impl Default for RumbleOnActivate {
    /// Matches the `.psc` Auto property defaults exactly.
    fn default() -> Self {
        Self {
            camera_intensity: 0.25,
            duration: 0.25,
            repeatable: true,
            shake_left: 0.25,
            shake_right: 0.25,
            // `Auto State active` — boot into Active.
            state: RumbleState::Active,
        }
    }
}

impl Component for RumbleOnActivate {
    type Storage = SparseSetStorage<Self>;
}

/// Translation of Papyrus's `Auto State active / State busy / State
/// inactive` trio.
///
/// Papyrus picks which `Event OnActivate` body runs by string-keying
/// off the current state; the Rust translation is an exhaustive
/// `match` in [`rumble_on_activate_system`]. The `Busy` variant
/// carries the remaining wait time, replacing Papyrus's
/// stack-suspended `Utility.wait()` frame with plain data — no VM,
/// no fiber, no async runtime, no per-script heap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RumbleState {
    /// Ready to fire. Matches Papyrus's `Auto State active`.
    Active,
    /// Currently in the post-shake wait. `wait_remaining_secs`
    /// counts down to zero, then [`rumble_tick_system`] transitions
    /// back to [`Self::Active`] (if `repeatable`) or
    /// [`Self::Inactive`] (one-shot). Matches Papyrus's
    /// `State busy`, where the empty `OnActivate` body swallows
    /// re-activations during the wait.
    Busy { wait_remaining_secs: f32 },
    /// One-shot completed; future activations are no-ops. Matches
    /// Papyrus's `State inactive` (also empty OnActivate body).
    Inactive,
}

/// Cross-subsystem command emitted by Papyrus `Game.shakeCamera(…)`.
///
/// Marker component placed on the [`PlayerEntity`] target. The
/// camera subsystem (when it lands) will drain these via a system
/// that runs late in the frame; until then, tests assert their
/// presence as the observable proof the script's `Active`-state
/// branch fired.
///
/// Holding the data on a component (rather than a resource-queue)
/// keeps every shake source traceable to its triggering frame and
/// the originating entity via the cleanup-system's standard
/// end-of-frame sweep.
#[derive(Debug, Clone, Copy)]
pub struct CameraShakeCommand {
    pub intensity: f32,
    pub duration_secs: f32,
}

impl Component for CameraShakeCommand {
    type Storage = SparseSetStorage<Self>;
}

/// Cross-subsystem command emitted by Papyrus `Game.shakeController(…)`.
///
/// Same lifecycle as [`CameraShakeCommand`] — marker on the player
/// entity, drained by the (future) input subsystem.
#[derive(Debug, Clone, Copy)]
pub struct ControllerRumbleCommand {
    pub left: f32,
    pub right: f32,
    pub duration_secs: f32,
}

impl Component for ControllerRumbleCommand {
    type Storage = SparseSetStorage<Self>;
}

pub mod actor_stats;
pub mod dlc2_ttr4a;
pub mod quest_advance;

/// Register every component this module + its submodules introduce.
///
/// Mirrors [`crate::register`]'s shape so the App-level setup
/// initialises the demo storages alongside the rest of the
/// scripting subsystem. Resources backing the demos
/// ([`PlayerEntity`], [`crate::quest_stages::QuestStageState`]) are
/// inserted by the caller — they're per-app instance state, not
/// per-world-init.
pub fn register(world: &mut World) {
    world.register::<RumbleOnActivate>();
    world.register::<CameraShakeCommand>();
    world.register::<ControllerRumbleCommand>();
    quest_advance::register(world);
    actor_stats::register(world);
    dlc2_ttr4a::register(world);
}

/// Translation of Papyrus's per-state `Event OnActivate(actronaut)`
/// dispatch.
///
/// Run order: between [`crate::events::ActivateEvent`] arrival and
/// the end-of-frame [`crate::event_cleanup_system`] sweep.
///
/// Equivalent Papyrus (active state):
///
/// ```papyrus
/// Event onActivate(ObjectReference actronaut)
///   Game.shakeCamera(None, cameraIntensity, 0.0)
///   Game.shakeController(shakeLeft, shakeRight, duration)
///   Self.GotoState("busy")
///   Utility.wait(duration)
///   If repeatable
///     Self.GotoState("active")
///   Else
///     Self.GotoState("inactive")
///   EndIf
/// EndEvent
/// ```
///
/// Equivalent Papyrus (busy / inactive states):
///
/// ```papyrus
/// Event onActivate(ObjectReference actronaut)
///   ; empty body — no-op
/// EndEvent
/// ```
///
/// The split between "fire now" and "tick the wait" lives in
/// [`rumble_tick_system`]. The system here handles only the
/// non-latent part (the camera shake + the state transition INTO
/// Busy); the wait counter ticks in the dt-driven sibling system.
/// That split is the structural deviation from Papyrus and the
/// load-bearing R5 finding: a single Papyrus event body splits
/// into one immediate system + one tick system because ECS
/// doesn't suspend.
pub fn rumble_on_activate_system(world: &World) {
    let Some(events) = world.query::<ActivateEvent>() else {
        return;
    };
    let Some(mut rumbles) = world.query_mut::<RumbleOnActivate>() else {
        return;
    };
    // Papyrus's `Game.shakeCamera(None, …)` resolves to the player's
    // camera. Caller must `insert_resource(PlayerEntity(eid))` at
    // world init — `world.resource()` panics if missing, matching the
    // engine's invariant that the player exists for every active
    // scene. Tests in `tests.rs` set this up explicitly.
    let player = world.resource::<PlayerEntity>().0;

    let mut to_shake: Vec<(EntityId, RumbleOnActivate)> = Vec::new();

    for (entity, _ev) in events.iter() {
        let Some(rumble) = rumbles.get_mut(entity) else {
            continue;
        };
        match rumble.state {
            RumbleState::Active => {
                // Stash the property values so we can emit the
                // cross-subsystem commands AFTER releasing the
                // borrows below — interior mutability would let us
                // do it inline but the two-phase shape is clearer
                // about who owns what.
                to_shake.push((entity, *rumble));
                // Transition Active → Busy. The wait counter
                // starts at `duration` and ticks down in
                // `rumble_tick_system`.
                rumble.state = RumbleState::Busy {
                    wait_remaining_secs: rumble.duration,
                };
            }
            RumbleState::Busy { .. } | RumbleState::Inactive => {
                // Papyrus `State busy / inactive` have empty
                // OnActivate bodies — re-activations during the
                // wait or after a one-shot finishes are swallowed.
            }
        }
    }

    drop(rumbles);
    drop(events);

    if to_shake.is_empty() {
        return;
    }

    // Emit the cross-subsystem command markers on the player
    // entity. Both commands target the same recipient (Papyrus's
    // `Game` resolves to the player camera / controller); merging
    // them onto one entity matches that semantic and avoids
    // querying two separate "command sink" entities downstream.
    let Some(mut cam_q) = world.query_mut::<CameraShakeCommand>() else {
        return;
    };
    for (_entity, rumble) in &to_shake {
        cam_q.insert(
            player,
            CameraShakeCommand {
                intensity: rumble.camera_intensity,
                // Papyrus `Game.shakeCamera(None, intensity, 0.0)`
                // — third arg is duration but the .psc passes 0.0,
                // which in Game.shakeCamera's contract means "use
                // engine default". Preserve the same call shape;
                // the camera subsystem can apply its default when
                // the duration field is 0.
                duration_secs: 0.0,
            },
        );
    }
    drop(cam_q);

    let Some(mut rumble_cmd_q) = world.query_mut::<ControllerRumbleCommand>() else {
        return;
    };
    for (_entity, rumble) in &to_shake {
        rumble_cmd_q.insert(
            player,
            ControllerRumbleCommand {
                left: rumble.shake_left,
                right: rumble.shake_right,
                duration_secs: rumble.duration,
            },
        );
    }
}

/// Translation of Papyrus's `Utility.wait(duration)` + the
/// post-wait `If repeatable / Else` branch.
///
/// Ticks every `Busy` rumble's wait counter by `dt`. When the
/// counter hits zero or below, transitions back to `Active` (if
/// `repeatable`) or `Inactive` (one-shot). Equivalent to the
/// continuation of the Papyrus event handler past the
/// `Utility.wait()` call.
///
/// Single tick fully completes the post-wait branching — no second
/// system pass needed. The same dt that pushes the wait below zero
/// also writes the destination state, so a frame whose dt happens
/// to exceed the remaining wait still resolves cleanly.
pub fn rumble_tick_system(world: &World, dt: f32) {
    let Some(mut rumbles) = world.query_mut::<RumbleOnActivate>() else {
        return;
    };
    for (_entity, rumble) in rumbles.iter_mut() {
        let RumbleState::Busy { wait_remaining_secs } = &mut rumble.state else {
            continue;
        };
        *wait_remaining_secs -= dt;
        if *wait_remaining_secs <= 0.0 {
            rumble.state = if rumble.repeatable {
                RumbleState::Active
            } else {
                RumbleState::Inactive
            };
        }
    }
}

#[cfg(test)]
mod tests;
