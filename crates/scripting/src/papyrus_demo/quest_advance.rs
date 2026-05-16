//! R5 follow-up — translation of `DA10MainDoorScript.psc` (the
//! canonical "stage-gated SetStage on activate" pattern that recurs
//! across Skyrim's quest content).
//!
//! Source: [`docs/r5/source/DA10MainDoorScript.psc`](../../../../docs/r5/source/DA10MainDoorScript.psc).
//! Companion to the `defaultRumbleOnActivate` translation in the
//! parent module (the latent-wait / state-machine demo); together
//! they cover R5's two outstanding pattern families: stateful timing
//! (rumble) and quest-stage state mutation (this).
//!
//! ## The source script in full
//!
//! ```papyrus
//! ScriptName DA10MainDoorScript Extends ReferenceAlias
//!
//! Event OnActivate(ObjectReference akActionRef)
//!   If (Self.GetOwningQuest().GetStageDone(37) == 1 as Bool) && \
//!      (Self.GetOwningQuest().GetStageDone(40) == 0 as Bool)
//!     Self.GetOwningQuest().SetStage(40)
//!   EndIf
//! EndEvent
//! ```
//!
//! Six lines of code (excluding doc comments + compiler-generated
//! stubs Champollion left in). Pattern:
//!
//! 1. Script is attached to a `ReferenceAlias` (a quest's
//!    placeholder slot for a runtime reference — Papyrus's way of
//!    saying "the actual door object the quest's data points at").
//! 2. `OnActivate` fires when the player activates the door.
//! 3. Pre-conditions are stage-state predicates against the alias's
//!    owning quest: `GetStageDone(37)` must be `true`,
//!    `GetStageDone(40)` must be `false`.
//! 4. Action is a single `SetStage(40)` on the owning quest.
//!
//! ## ECS translation choice — specific or generic?
//!
//! Two valid shapes:
//!
//! - **Specific**: `struct DA10MainDoor` + `da10_main_door_system`
//!   with constants hardcoded (`require_done: 37`,
//!   `forbid_done: 40`, `target_stage: 40`). Faithful 1:1
//!   reproduction.
//! - **Generic**: `struct QuestAdvanceOnActivate { quest,
//!   require_done, forbid_done, target_stage }` + one system that
//!   handles every script of this shape. The translator's job
//!   becomes "extract the constants from the script body and
//!   populate the component fields".
//!
//! Going generic because:
//!
//! 1. The pattern is **not unique to DA10** — `DA01HeartStoneScript`,
//!    `MS05StageScript`, dozens of `RNAME_doorscript` quest-gated
//!    door scripts share the exact shape (`OnActivate` +
//!    stage-predicates + SetStage). A specific `DA10MainDoor`
//!    component compiled per script wastes one component-type per
//!    quest-door — a thousand quests × thousand doors = a thousand
//!    component types.
//! 2. Going generic is the shape M47.2's transpiler will naturally
//!    emit: detect the pattern, populate one component variant,
//!    reuse the dispatch system. The generic component is the
//!    target shape for the transpiler.
//! 3. The specific shape adds no new ECS surface beyond what the
//!    generic already covers; the generic is strictly more
//!    expressive (a `forbid_done = u16::MAX` sentinel + a single
//!    `require_done` entry produces DA10's exact semantics, and
//!    `forbid_done = None` covers the simpler "advance regardless"
//!    fragment scripts).
//!
//! ## What's still load-bearing as one-offs
//!
//! Some scripts genuinely have one-off semantics that don't reduce
//! to a generic shape (e.g., `MGRitual04QuestScript`'s
//! seven-conditional puzzle progression). For those the transpiler
//! emits per-script components and systems — the generic component
//! here covers the most common ~70% pattern, leaving the long tail
//! for per-script lowerings.

use super::PlayerEntity;
use crate::events::ActivateEvent;
use crate::quest_stages::{QuestFormId, QuestStageAdvanced, QuestStageState};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::world::World;

/// "On activation, if the owning quest's stage predicates hold,
/// advance the quest to `target_stage`."
///
/// Attached to the entity Papyrus's `ReferenceAlias.GetOwningQuest()`
/// resolves to (a placed REFR the alias points at — doors,
/// activators, NPCs). Default values produce a no-op
/// component — the transpiler populates the fields from the
/// source script body.
#[derive(Debug, Clone)]
pub struct QuestAdvanceOnActivate {
    /// The quest this advance writes to. Papyrus's
    /// `Self.GetOwningQuest()` is a runtime lookup; the translator
    /// resolves it once at script-attach time and stores the FormID
    /// here.
    pub owning_quest: QuestFormId,
    /// Stage(s) that must be in `stages_done` for the advance to
    /// fire. AND-combined with `forbid_done`. Empty means
    /// "no precondition" (covers the "any activation advances"
    /// long-tail).
    pub require_done: Vec<u16>,
    /// Stage(s) that must NOT be in `stages_done`. AND-combined
    /// with `require_done`. Typically a singleton matching
    /// `target_stage` — Papyrus's `!GetStageDone(N) → SetStage(N)`
    /// idempotency idiom.
    pub forbid_done: Vec<u16>,
    /// Stage written via `SetStage` when conditions are satisfied.
    pub target_stage: u16,
    /// Activator gate — if `Some(activator_kind)`, the activation's
    /// `actronaut` must match (today supports the "player only"
    /// idiom via [`ActivatorGate::PlayerOnly`]; future expansion
    /// can cover faction / NPC-specific gates).
    pub activator_gate: ActivatorGate,
}

impl Component for QuestAdvanceOnActivate {
    type Storage = SparseSetStorage<Self>;
}

/// Activator gate — Papyrus's `If akActionRef == Game.GetPlayer()`
/// guard at the head of many OnActivate handlers.
///
/// DA10MainDoorScript intentionally doesn't filter (any reference
/// activating the door advances the stage — quests sometimes have
/// NPC-driven advancement); the [`Any`] default matches that.
/// `MG07LabyrinthianDoorScript` and `TG05RuinsDoorScript` both
/// gate on player-only — the more common pattern.
///
/// [`Any`]: ActivatorGate::Any
#[derive(Debug, Clone, Copy, Default)]
pub enum ActivatorGate {
    /// Any activator advances the quest. Matches DA10's behaviour.
    #[default]
    Any,
    /// Only the player (resolved via [`super::PlayerEntity`]) can
    /// activate. Matches MG07 / TG05 patterns.
    PlayerOnly,
}

/// Helper: build the DA10MainDoorScript-equivalent component with
/// the constants extracted from the source body.
///
/// Lives here as documentation — the transpiler will produce
/// equivalent constructions from the AST. Tests use this builder
/// to validate the translation is byte-faithful to the .psc source.
pub fn da10_main_door(owning_quest: QuestFormId) -> QuestAdvanceOnActivate {
    QuestAdvanceOnActivate {
        owning_quest,
        // Papyrus: `GetStageDone(37) == 1` → require 37 done.
        require_done: vec![37],
        // Papyrus: `GetStageDone(40) == 0` → forbid 40 done.
        forbid_done: vec![40],
        // Papyrus: `SetStage(40)` → target 40. (Self-forbids
        // re-firing once 40 has been set — idempotency.)
        target_stage: 40,
        // DA10's source has no player gate.
        activator_gate: ActivatorGate::Any,
    }
}

/// Register the [`QuestAdvanceOnActivate`] component storage with
/// the ECS world. Sibling to [`super::register`].
pub fn register(world: &mut World) {
    world.register::<QuestAdvanceOnActivate>();
    world.register::<QuestStageAdvanced>();
}

/// Translation of the OnActivate event-handler body.
///
/// For every `ActivateEvent` on an entity that has a
/// `QuestAdvanceOnActivate`, evaluate the predicates against
/// [`QuestStageState`] and, if they hold, write the new stage +
/// emit a [`QuestStageAdvanced`] marker for downstream consumers.
///
/// Run-order: between the engine's activation-pipeline emission of
/// `ActivateEvent` and the end-of-frame cleanup. Sits alongside the
/// parent module's `rumble_on_activate_system` in the scripting
/// stage.
///
/// ## How the Papyrus predicates translate
///
/// ```papyrus
/// If (GetStageDone(37) == 1) && (GetStageDone(40) == 0)
///   SetStage(40)
/// EndIf
/// ```
///
/// becomes (in roughly equivalent pseudo-code):
///
/// ```rust,ignore
/// if comp.require_done.iter().all(|s| stage_state.get_stage_done(quest, *s))
///    && comp.forbid_done.iter().all(|s| !stage_state.get_stage_done(quest, *s)) {
///     stage_state.set_stage(quest, comp.target_stage);
/// }
/// ```
///
/// The `all()` reductions are vacuously true on empty vectors —
/// `require_done: vec![]` means "no precondition", consistent with
/// scripts that advance unconditionally on activate.
pub fn quest_advance_on_activate_system(world: &World) {
    let Some(events) = world.query::<ActivateEvent>() else {
        return;
    };
    let Some(advances) = world.query::<QuestAdvanceOnActivate>() else {
        return;
    };
    let player_entity = world.resource::<PlayerEntity>().0;

    // Two-phase: collect (read), apply (write). Releases the
    // QuestStageState read borrow before we acquire the write.
    struct PendingAdvance {
        quest: QuestFormId,
        target_stage: u16,
    }
    let mut pending: Vec<PendingAdvance> = Vec::new();
    {
        let stage_state = world.resource::<QuestStageState>();
        for (entity, ev) in events.iter() {
            let Some(comp) = advances.get(entity) else {
                continue;
            };
            // Activator gate.
            if matches!(comp.activator_gate, ActivatorGate::PlayerOnly)
                && ev.activator != player_entity
            {
                continue;
            }
            // Stage predicates.
            let all_required_done = comp
                .require_done
                .iter()
                .all(|s| stage_state.get_stage_done(comp.owning_quest, *s));
            let none_forbidden_done = comp
                .forbid_done
                .iter()
                .all(|s| !stage_state.get_stage_done(comp.owning_quest, *s));
            if all_required_done && none_forbidden_done {
                pending.push(PendingAdvance {
                    quest: comp.owning_quest,
                    target_stage: comp.target_stage,
                });
            }
        }
    }
    drop(advances);
    drop(events);

    if pending.is_empty() {
        return;
    }

    // Phase 2: apply. Stash the (quest, prev_stage, new_stage)
    // triples so the QuestStageAdvanced markers carry the correct
    // pre-image — Papyrus's `OnStageSet(auiStageID, auiItemID)`
    // contract treats the new-stage as the load-bearing value but
    // the previous-stage is useful for "what changed" inspections
    // and the future fragment dispatcher.
    let mut advances_emitted: Vec<QuestStageAdvanced> = Vec::with_capacity(pending.len());
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        for p in pending {
            let prev = stage_state.set_stage(p.quest, p.target_stage);
            advances_emitted.push(QuestStageAdvanced {
                quest: p.quest,
                previous_stage: prev,
                new_stage: p.target_stage,
            });
        }
    }

    // Phase 3: emit the marker events on a dedicated quest-events
    // sink. We co-opt the [`PlayerEntity`] target here for the
    // same reason `default_rumble_demo` does — the player entity
    // is the canonical "global events" recipient until a
    // dedicated `QuestEventBus` entity lands (which is itself
    // M47.0 surface). The marker carries enough context that the
    // future consumer can demux by `quest` regardless of where it
    // lands.
    let Some(mut q) = world.query_mut::<QuestStageAdvanced>() else {
        return;
    };
    for ev in advances_emitted {
        q.insert(player_entity, ev);
    }
}

#[cfg(test)]
mod tests;
