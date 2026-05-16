//! R5 follow-up — translation of `MG07LabyrinthianDoorScript.psc`,
//! the canonical "stage-gated activation that fires a cross-reference
//! method call after a latent wait" pattern.
//!
//! Source: [`docs/r5/source/MG07LabyrinthianDoorScript.psc`](../../../../docs/r5/source/MG07LabyrinthianDoorScript.psc).
//! Sibling demos: the parent module's `defaultRumbleOnActivate`
//! (state machine + wait), [`super::quest_advance`] (SetStage), and
//! [`super::dlc2_ttr4a`] (RegisterForUpdate).
//!
//! ## The source script in full (47 LOC raw, 23 of actual code)
//!
//! ```papyrus
//! ScriptName MG07LabyrinthianDoorSCRIPT Extends ObjectReference
//!
//! Bool beenOpened = False
//!
//! Quest Property MG07 Auto
//! MiscObject Property MG07Keystone Auto
//! Float Property delayAfterInsert = 1.0 Auto
//! Message Property dunLabyrinthianDenialMSG Auto
//! ObjectReference Property myDoor Auto
//!
//! Event onLoad()
//!   If beenOpened == False
//!     Self.blockActivation(True)
//!   Else
//!     Self.disable(False)
//!   EndIf
//!   Self.GotoState("waiting")
//! EndEvent
//!
//! State inactive
//! EndState
//!
//! State waiting
//!   Event onActivate(ObjectReference actronaut)
//!     If (actronaut == Game.getPlayer()) && MG07.getStageDone(10) && \
//!        Game.getPlayer().getItemCount(MG07Keystone) >= 1
//!       Self.GotoState("inactive")
//!       Game.getPlayer().removeItem(MG07Keystone, ..., False, None)
//!       Self.playAnimationAndWait("Insert", "Done")
//!       beenOpened == False    ; (note: source typo — == not = — Papyrus
//!                              ; compiles this as a discarded expression)
//!       Utility.wait(delayAfterInsert)
//!       Self.disable(False)
//!       myDoor.activate(actronaut, False)
//!     Else
//!       dunLabyrinthianDenialMSG.show(...)
//!     EndIf
//!   EndEvent
//! EndState
//! ```
//!
//! ## Why this fixture for R5
//!
//! It's the script the original R5 spec was really asking for: the
//! `cross-script callback` axis in its purest Bethesda-vanilla
//! form. `myDoor.activate(actronaut, False)` is Papyrus's "tell another
//! reference to do something" idiom — and the ECS translation is
//! the load-bearing finding of this follow-up:
//!
//! > **Cross-reference method calls become marker components on the
//! > target entity's storage.** The same `ActivateEvent` the engine
//! > emits when the player presses E on a door is what we emit when
//! > one script tells another reference to activate. No proxy, no
//! > vtable, no method dispatch — the call site INSERTS the event
//! > the target already handles. The cross-script boundary collapses
//! > to the same surface as player-driven input.
//!
//! Other patterns this fixture incidentally re-exercises (already
//! validated by prior demos; reproduced here for end-to-end fidelity):
//!
//! - Multi-state machine (`Auto State` is implicit; named states
//!   `waiting`, `inactive`). Already covered by `defaultRumbleOnActivate`.
//! - Latent wait inside an event handler. Already covered.
//! - `Quest.GetStageDone(N)` predicate. Already covered (DA10).
//! - Persistent script-instance state (`Bool beenOpened`). New — but
//!   trivial: a `bool` field on the component, persistence is the
//!   save system's job (M45).
//!
//! ## What's deliberately stubbed
//!
//! The source touches more engine surface area than the prototype
//! can wire up cleanly. Stubs (each documented at its call site):
//!
//! - **`Game.GetPlayer().GetItemCount(MG07Keystone)` / `RemoveItem`**:
//!   stubbed via a [`KeystoneInventory`] component on the player.
//!   The real inventory system (M41 phase 2 — `Inventory` +
//!   `ItemInstancePool`) is a separate ECS surface; for prototype
//!   purposes the boolean flag is enough.
//! - **`Self.blockActivation(True)` / `Self.disable(False)`**: stored
//!   as fields on the script component. A real engine integration
//!   would route them through dedicated component flags consumed by
//!   the activation pipeline + entity-enable system.
//! - **`Self.playAnimationAndWait("Insert", "Done")`** is collapsed
//!   into the same wait counter as `Utility.wait(delayAfterInsert)`.
//!   Both are latent waits at the script-semantic level; the
//!   animation duration is engine surface area not in scope for R5.
//! - **`dunLabyrinthianDenialMSG.show(...)`** emits a
//!   [`UiMessageCommand`] marker on the player entity (same pattern
//!   as `CameraShakeCommand` from the rumble demo).
//!
//! Stubbing these doesn't change the load-bearing observation about
//! cross-reference activation — that pattern fully translates with
//! the existing infrastructure.

use super::PlayerEntity;
use crate::events::ActivateEvent;
use crate::quest_stages::{QuestFormId, QuestStageState};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;

/// Translation of the script's properties + persistent state + the
/// state-machine variant the source uses for sequencing.
#[derive(Debug, Clone, Copy)]
pub struct MG07LabyrinthianDoor {
    // ── Properties (resolved at VMAD-attach time) ─────────
    /// Papyrus `Quest Property MG07 Auto`.
    pub mg07_quest: QuestFormId,
    /// Papyrus `ObjectReference Property myDoor Auto`. The
    /// inner-door entity that gets activated *via the keystone door*
    /// when the player succeeds. THIS is the cross-reference target
    /// — the load-bearing piece of R5's third axis.
    pub my_door: EntityId,
    /// Papyrus `Float Property delayAfterInsert = 1.0 Auto`. Used
    /// for the post-success wait. The translation collapses
    /// `playAnimationAndWait + Utility.wait(delay)` into a single
    /// `wait_remaining_secs` counter sized at this value — the
    /// animation-duration component is engine surface not in scope.
    pub delay_after_insert: f32,
    /// Papyrus `Message Property dunLabyrinthianDenialMSG Auto`.
    /// The UI message form ID shown when the player tries to use
    /// the door before the quest stage 10 / before holding the
    /// keystone. Carried as a raw u32 form-ID — the UI system's
    /// message dispatcher will resolve it.
    pub denial_message_form_id: u32,
    // ── Persistent instance state ────────────────────────
    /// Papyrus `Bool beenOpened = False`. Survives across saves
    /// in Papyrus; in the ECS shape it's just a field on the
    /// component (M45 save system serializes it alongside every
    /// other component). The source has a typo at line 28 — it
    /// writes `beenOpened == False` (comparison, not assignment),
    /// which Papyrus compiles as a discarded expression. So in
    /// shipped Skyrim, `beenOpened` is NEVER actually flipped to
    /// true post-success — the script unintentionally relies on
    /// `Self.disable()` for the lockout. We preserve the
    /// observable behaviour (no-op on the typo'd line) rather
    /// than "fix" what shipped — note in
    /// [`mg07_on_activate_system`].
    pub been_opened: bool,
    // ── Runtime state ────────────────────────────────────
    /// Current Papyrus-state mapping. `Uninitialized` is the
    /// implicit default state before `OnLoad` fires; the source's
    /// explicit `State waiting` / `State inactive` cover the rest.
    pub state: MG07State,
    /// Papyrus `Self.blockActivation(True)`. If true, the
    /// activation pipeline should skip this entity. Set on
    /// OnLoad for the "first visit, locked" case. Not wired to a
    /// real activation pipeline yet — the prototype stores it
    /// here for inspection.
    pub activation_blocked: bool,
    /// Papyrus `Self.disable(False)`. If true, the entity is no
    /// longer visible / interactable. The source uses
    /// `disable(False)` (the `False` is the "fade out" argument,
    /// not "set disabled to false"). Same activation-pipeline note
    /// applies.
    pub disabled: bool,
}

impl Component for MG07LabyrinthianDoor {
    type Storage = SparseSetStorage<Self>;
}

/// State-machine variant. Same shape as Papyrus's named-state
/// dispatch, lowered to a Rust enum for the same reasons documented
/// at [`super::RumbleState`] in the rumble demo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MG07State {
    /// Implicit-default state — script attached but `OnLoad` hasn't
    /// fired yet. The source's first frame.
    Uninitialized,
    /// Papyrus's `State waiting` — accepts activation, runs the
    /// stage-gate predicate, transitions on success.
    Waiting,
    /// After a successful activation. Carries the wait counter
    /// replacing Papyrus's `playAnimationAndWait + Utility.wait`
    /// pair. When the counter hits zero,
    /// [`mg07_tick_system`] fires the cross-reference activation +
    /// transitions to `Inactive`.
    Inserting { wait_remaining_secs: f32 },
    /// Papyrus's `State inactive` — door has opened, no further
    /// activations. Equivalent to the rumble demo's terminal
    /// `Inactive`.
    Inactive,
}

/// Stub for "the player has the MG07 keystone in their inventory."
///
/// Inserted on the player entity by test setup (or, in a real
/// engine integration, by the inventory system when the keystone
/// reaches the player). Read by [`mg07_on_activate_system`] to
/// gate the success branch; flipped to `false` in the same system
/// to model Papyrus's `Game.GetPlayer().RemoveItem(MG07Keystone)`.
///
/// The real inventory surface (M41 phase 2 — `Inventory`,
/// `EquipmentSlots`, `ItemInstancePool`) is a much larger system;
/// this boolean is enough to validate the cross-reference call
/// pattern without dragging it in.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeystoneInventory {
    pub has_mg07_keystone: bool,
}

impl Component for KeystoneInventory {
    type Storage = SparseSetStorage<Self>;
}

/// Cross-subsystem command — Papyrus's `Message.Show()` translated
/// to a marker on the player entity. Same lifecycle as
/// [`super::CameraShakeCommand`] (transient, drained by
/// `event_cleanup_system` at end of frame).
#[derive(Debug, Clone, Copy)]
pub struct UiMessageCommand {
    /// FormID of the `Message` record (referenced in Papyrus as a
    /// `Message Property X Auto` and stored in the parsed
    /// `MesgRecord` map on `EsmIndex.messages`).
    pub message_form_id: u32,
}

impl Component for UiMessageCommand {
    type Storage = SparseSetStorage<Self>;
}

pub fn register(world: &mut World) {
    world.register::<MG07LabyrinthianDoor>();
    world.register::<KeystoneInventory>();
    world.register::<UiMessageCommand>();
}

/// Translation of `Event OnLoad()`. Runs every frame against any
/// `MG07LabyrinthianDoor` whose `state == Uninitialized`,
/// transitioning to `Waiting` and applying the initial
/// `blockActivation` / `disable` decisions based on `been_opened`.
///
/// Papyrus's `OnLoad` is a cell-streaming lifecycle event — it
/// fires when the script's owning REFR's cell loads into memory.
/// In ECS, the same observable behaviour comes from a tick-driven
/// "Uninitialized → first run" transition, since the script
/// component lands on the entity at REFR-spawn time (which IS the
/// cell-load equivalent). When M40 phase 2 wires up cell unload
/// → entity-removal, the next cell load re-creates the component
/// → re-runs `OnLoad`. Matches Papyrus's contract.
pub fn mg07_on_load_system(world: &World) {
    let Some(mut doors) = world.query_mut::<MG07LabyrinthianDoor>() else {
        return;
    };
    for (_entity, door) in doors.iter_mut() {
        if door.state != MG07State::Uninitialized {
            continue;
        }
        // Papyrus body:
        //   If beenOpened == False
        //     Self.blockActivation(True)
        //   Else
        //     Self.disable(False)
        //   EndIf
        //   Self.GotoState("waiting")
        if !door.been_opened {
            door.activation_blocked = true;
        } else {
            door.disabled = true;
        }
        door.state = MG07State::Waiting;
    }
}

/// Translation of `State waiting / Event OnActivate(actronaut)`.
///
/// **The R5 third-axis demonstration lives in this body.** The
/// success branch ends with a cross-reference activation against
/// `door.my_door` — Papyrus's `myDoor.activate(actronaut, False)`
/// — but it's split across two systems (this one + the tick) to
/// honour the latent-wait barrier. The pre-wait work happens here;
/// the cross-reference call itself fires in
/// [`mg07_tick_system`] after the wait elapses.
///
/// Pre-wait body:
/// 1. Check predicates (player + stage 10 done + keystone held).
/// 2. On success: GotoState("inactive"), remove keystone,
///    transition to `Inserting { wait_remaining_secs }` so the tick
///    system picks up the rest.
/// 3. On failure: emit a `UiMessageCommand` marker on the player.
///
/// The source has a Papyrus-compiler-tolerated typo at the
/// equivalent of line 28 (`beenOpened == False` instead of
/// `beenOpened = False`) — compiles as a discarded expression, so
/// `been_opened` is NEVER flipped in shipped vanilla. We
/// **preserve** that bug here rather than silently "fixing" it —
/// faithful translation is the R5 contract, and a real corpus walk
/// will encounter this exact compiler-tolerated pattern many times.
pub fn mg07_on_activate_system(world: &World) {
    let player = world.resource::<PlayerEntity>().0;

    // Two-phase: collect (read), then apply (write). Same shape as
    // the other R5 systems.
    enum Outcome {
        Success {
            door_entity: EntityId,
            wait_secs: f32,
        },
        Denied {
            denial_message_form_id: u32,
        },
    }
    let mut outcomes: Vec<Outcome> = Vec::new();
    {
        let Some(events) = world.query::<ActivateEvent>() else {
            return;
        };
        let Some(doors) = world.query::<MG07LabyrinthianDoor>() else {
            return;
        };
        let keystone_q = world.query::<KeystoneInventory>();
        let stage_state = world.resource::<QuestStageState>();

        for (entity, ev) in events.iter() {
            let Some(door) = doors.get(entity) else {
                continue;
            };
            // Per-state OnActivate dispatch — only `Waiting`
            // responds. `Inactive` and `Inserting` swallow per the
            // Papyrus `State inactive` empty-body contract; same
            // shape as `RumbleState::Busy / Inactive`.
            if door.state != MG07State::Waiting {
                continue;
            }
            // `Self.blockActivation(True)` from OnLoad: if the
            // activation pipeline were wired, blocked entities
            // wouldn't even emit ActivateEvent. The prototype isn't
            // wired, so we honor the flag explicitly here — the
            // source's `blockActivation` semantic short-circuits
            // any path that would reach this point.
            if door.activation_blocked {
                continue;
            }

            let actronaut_is_player = ev.activator == player;
            let stage_10_done = stage_state.get_stage_done(door.mg07_quest, 10);
            let has_keystone = keystone_q
                .as_ref()
                .and_then(|q| q.get(player))
                .map(|k| k.has_mg07_keystone)
                .unwrap_or(false);

            if actronaut_is_player && stage_10_done && has_keystone {
                outcomes.push(Outcome::Success {
                    door_entity: entity,
                    wait_secs: door.delay_after_insert,
                });
            } else {
                // Else branch — show denial message.
                outcomes.push(Outcome::Denied {
                    denial_message_form_id: door.denial_message_form_id,
                });
            }
        }
    }

    if outcomes.is_empty() {
        return;
    }

    // Phase 2 — apply. Mutate state, drop keystone, emit UI command.
    // The actual cross-reference activation lands in
    // `mg07_tick_system` post-wait, not here.
    for outcome in &outcomes {
        match outcome {
            Outcome::Success {
                door_entity,
                wait_secs,
                ..
            } => {
                {
                    let mut doors = world.query_mut::<MG07LabyrinthianDoor>().unwrap();
                    if let Some(door) = doors.get_mut(*door_entity) {
                        // GotoState("inactive") in the source —
                        // BUT the next two lines (RemoveItem + the
                        // wait pair) happen BEFORE the actual
                        // `inactive` state takes effect (Papyrus
                        // runs the rest of the handler body in the
                        // OLD state; the GotoState merely marks the
                        // transition for the next event dispatch).
                        // In our shape the wait + cross-ref are
                        // hoisted to the tick system, so we
                        // transition to `Inserting` here instead —
                        // the door stays unresponsive to fresh
                        // OnActivate events for the duration of the
                        // wait, matching the Papyrus-runtime
                        // observable behaviour.
                        door.state = MG07State::Inserting {
                            wait_remaining_secs: *wait_secs,
                        };
                        // The source's typo'd `beenOpened == False`
                        // is a no-op — we faithfully reproduce it
                        // by NOT setting `door.been_opened = true`
                        // here.
                    }
                }
                // RemoveItem stub — flip the keystone flag off.
                if let Some(mut keystones) = world.query_mut::<KeystoneInventory>() {
                    if let Some(k) = keystones.get_mut(player) {
                        k.has_mg07_keystone = false;
                    }
                }
            }
            Outcome::Denied {
                denial_message_form_id,
            } => {
                if let Some(mut ui) = world.query_mut::<UiMessageCommand>() {
                    ui.insert(
                        player,
                        UiMessageCommand {
                            message_form_id: *denial_message_form_id,
                        },
                    );
                }
            }
        }
    }
}

/// Translation of the post-wait continuation in the `OnActivate`
/// success branch — runs after the
/// `playAnimationAndWait + Utility.wait(delayAfterInsert)` pair
/// elapses (collapsed in our shape to one counter).
///
/// On wait completion:
/// 1. `Self.disable(False)` — mark the keystone door disabled.
/// 2. `myDoor.activate(actronaut, False)` — **THE cross-reference
///    method call**, lowered to inserting an [`ActivateEvent`]
///    marker on `door.my_door`'s entity. The target's own
///    OnActivate handler picks it up next frame.
/// 3. Transition to `MG07State::Inactive`.
///
/// The `actronaut` argument that Papyrus threads through the wait
/// (its suspended stack frame held it) is lost in the ECS shape
/// — `Self` knows which target to activate (`my_door`), but the
/// activator argument doesn't survive across the wait barrier. We
/// reconstruct it by re-resolving "the player" via
/// `PlayerEntity`, which matches Papyrus's observable behaviour
/// (the original `actronaut` IS the player in every reachable
/// branch — the gate at the head of the handler enforces this).
pub fn mg07_tick_system(world: &World, dt: f32) {
    let player = world.resource::<PlayerEntity>().0;

    let mut to_fire_door_activate: Vec<(EntityId, EntityId)> = Vec::new();
    {
        let Some(mut doors) = world.query_mut::<MG07LabyrinthianDoor>() else {
            return;
        };
        for (entity, door) in doors.iter_mut() {
            let MG07State::Inserting {
                wait_remaining_secs,
            } = &mut door.state
            else {
                continue;
            };
            *wait_remaining_secs -= dt;
            if *wait_remaining_secs <= 0.0 {
                door.disabled = true;
                door.state = MG07State::Inactive;
                to_fire_door_activate.push((entity, door.my_door));
            }
        }
    }

    if to_fire_door_activate.is_empty() {
        return;
    }

    // Fire the cross-reference activation on each my_door. This is
    // the R5 load-bearing line: a Papyrus `myDoor.activate(actronaut)`
    // collapses to inserting an ActivateEvent on the target entity.
    // The target's own OnActivate handler — whether a Papyrus
    // translation, a built-in door-open system, or anything else
    // wired to ActivateEvent — picks it up on its next system pass.
    let Some(mut events) = world.query_mut::<ActivateEvent>() else {
        return;
    };
    for (_self_entity, target_door) in to_fire_door_activate {
        events.insert(target_door, ActivateEvent { activator: player });
    }
}

#[cfg(test)]
mod tests;
