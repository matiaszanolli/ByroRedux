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
use crate::condition::{evaluate as evaluate_condition_list, ConditionContext};
use crate::events::{ActivateEvent, OnTriggerEnterEvent};
use crate::quest_stages::{
    QuestFormId, QuestStageAdvanced, QuestStageAdvancedBatch, QuestStageState,
};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_core::ecs::Resource;
use byroredux_core::math::Vec3;
use std::collections::HashSet;

use byroredux_plugin::esm::records::condition::{
    ComparisonOp, Condition, ConditionList, ConditionValue, RunOn,
};

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
    /// M47.1 — Papyrus stage predicates lowered to a generic
    /// [`ConditionList`]. The DA10 source's
    /// `GetStageDone(37) == 1 && GetStageDone(40) == 0` becomes
    /// two CTDAs (function 59, comparator Eq, comparand 1.0 and 0.0)
    /// AND-combined. The advantage over the previous bespoke
    /// `require_done` / `forbid_done` vecs is that the same data
    /// shape now covers ALL Papyrus pre-condition patterns —
    /// `HasPerk(...) || GetActorValue(...) >= 50`, faction gates,
    /// distance checks, the lot — without per-script schema
    /// expansion. Empty list = "no precondition" (`evaluate` returns
    /// `true` on empty), preserving the "advance unconditionally"
    /// semantics the bespoke vecs covered.
    pub conditions: ConditionList,
    /// Stage written via `SetStage` when conditions are satisfied.
    pub target_stage: u16,
    /// Activator gate — if `Some(activator_kind)`, the activation's
    /// `actronaut` must match (today supports the "player only"
    /// idiom via [`ActivatorGate::PlayerOnly`]; future expansion
    /// can cover faction / NPC-specific gates).
    pub activator_gate: ActivatorGate,
    /// Remove this trigger behavior after its first passing advance. Mirrors
    /// the vanilla `disableWhenDone` / `onlyOnce` trigger-script options.
    pub disable_after_advance: bool,
}

impl Component for QuestAdvanceOnActivate {
    type Storage = SparseSetStorage<Self>;
}

/// Lightweight process-lifetime catalog for actor-specific trigger scripts
/// whose cells are not currently resident. The logical trigger entity owns
/// the same [`QuestAdvanceOnActivate`] component as a streamed REFR; its
/// authored position lets cinematic locomotion reach and signal it without
/// keeping the cell's render/physics payload loaded.
#[derive(Debug, Clone, Copy)]
pub struct QuestTriggerApproach {
    pub reference_form_id: u32,
    pub trigger_entity: EntityId,
    pub center: Vec3,
}

#[derive(Debug, Clone, Default)]
pub struct QuestTriggerApproachRegistry {
    entries: Vec<QuestTriggerApproach>,
}

impl Resource for QuestTriggerApproachRegistry {}

impl QuestTriggerApproachRegistry {
    pub fn entries(&self) -> &[QuestTriggerApproach] {
        &self.entries
    }
}

/// Install one static trigger-script entry and its logical ECS source.
pub fn install_quest_trigger_approach(
    world: &mut World,
    reference_form_id: u32,
    center: Vec3,
    advance: QuestAdvanceOnActivate,
) -> EntityId {
    if world
        .try_resource::<QuestTriggerApproachRegistry>()
        .is_none()
    {
        world.insert_resource(QuestTriggerApproachRegistry::default());
    }
    if let Some(existing) = world
        .resource::<QuestTriggerApproachRegistry>()
        .entries
        .iter()
        .find(|entry| entry.reference_form_id == reference_form_id)
        .copied()
    {
        return existing.trigger_entity;
    }
    let trigger_entity = world.spawn();
    world.insert(trigger_entity, advance);
    world
        .resource_mut::<QuestTriggerApproachRegistry>()
        .entries
        .push(QuestTriggerApproach {
            reference_form_id,
            trigger_entity,
            center,
        });
    trigger_entity
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
    /// Only a placed actor whose canonical NPC/creature base matches.
    BaseForm(u32),
}

/// Helper: build the DA10MainDoorScript-equivalent component with
/// the constants extracted from the source body.
///
/// Lives here as documentation — the transpiler will produce
/// equivalent constructions from the AST. Tests use this builder
/// to validate the translation is byte-faithful to the .psc source.
pub fn da10_main_door(owning_quest: QuestFormId) -> QuestAdvanceOnActivate {
    // M47.1 — bespoke `require_done`/`forbid_done` vecs replaced
    // by a `ConditionList` of CTDAs. The two source predicates
    // (`GetStageDone(37) == 1`, `GetStageDone(40) == 0`) lower to
    // two CTDAs with function_index=59 (GetStageDone), comparator
    // Eq, comparands 1.0 and 0.0. AND-combined (both `or_next=false`).
    let cond_get_stage_done = |stage: u16, expected: f32| Condition {
        function_index: 59, // GetStageDone
        comparator: ComparisonOp::Eq,
        comparand: ConditionValue::Literal(expected),
        param_1: owning_quest.0,
        param_2: stage as u32,
        run_on: RunOn::Subject,
        reference_form_id: 0,
        extra_data_id: 0,
        or_next: false,
        ..Default::default()
    };
    QuestAdvanceOnActivate {
        owning_quest,
        conditions: vec![
            // Papyrus: `GetStageDone(37) == 1` → stage 37 must be done.
            cond_get_stage_done(37, 1.0),
            // Papyrus: `GetStageDone(40) == 0` → stage 40 must NOT
            // be done. (Self-forbids re-firing once 40 has been set
            // — idempotency.)
            cond_get_stage_done(40, 0.0),
        ],
        // Papyrus: `SetStage(40)` → target 40.
        target_stage: 40,
        // DA10's source has no player gate.
        activator_gate: ActivatorGate::Any,
        disable_after_advance: false,
    }
}

/// Register the [`QuestAdvanceOnActivate`] component storage with
/// the ECS world. Sibling to [`super::register`].
pub fn register(world: &mut World) {
    world.register::<QuestAdvanceOnActivate>();
    world.register::<QuestStageAdvancedBatch>();
    if world
        .try_resource::<QuestTriggerApproachRegistry>()
        .is_none()
    {
        world.insert_resource(QuestTriggerApproachRegistry::default());
    }
}

/// Translation of the `OnActivate` / `OnTriggerEnter` event-handler body.
///
/// For every `ActivateEvent` **or** `OnTriggerEnterEvent` on an entity
/// that has a `QuestAdvanceOnActivate`, evaluate the predicates against
/// [`QuestStageState`] and, if they hold, write the new stage +
/// emit a [`QuestStageAdvanced`] marker for downstream consumers. The two
/// events are the same advance signal from different sources — a use-key
/// activation vs. an actor entering a trigger volume — so one system
/// covers both the `default*OnActivate` door family and the
/// `default*Trigger` volume family.
///
/// Run-order: between the engine's activation-pipeline emission of
/// `ActivateEvent` / the trigger-detection system's `OnTriggerEnterEvent`
/// and the end-of-frame cleanup. Sits alongside the parent module's
/// `rumble_on_activate_system` in the scripting stage.
///
/// ## How the Papyrus predicates translate
///
/// ```papyrus
/// If (GetStageDone(37) == 1) && (GetStageDone(40) == 0)
///   SetStage(40)
/// EndIf
/// ```
///
/// becomes (in roughly equivalent pseudo-code — tagged `text`, not
/// `rust,ignore`, because pseudo-code cannot compile; see #3348):
///
/// ```text
/// if comp.require_done.iter().all(|s| stage_state.get_stage_done(quest, *s))
///    && comp.forbid_done.iter().all(|s| !stage_state.get_stage_done(quest, *s)) {
///     stage_state.set_stage(quest, comp.target_stage);
/// }
/// ```
///
/// The `all()` reductions are vacuously true on empty vectors —
/// `require_done: vec![]` means "no precondition", consistent with
/// scripts that advance unconditionally on activate.
pub fn quest_advance_system(world: &World) {
    let Some(advances) = world.query::<QuestAdvanceOnActivate>() else {
        return;
    };
    let player_entity = world.resource::<PlayerEntity>().0;

    // Two activation signals converge on the same advance: a use-key /
    // console `ActivateEvent` (doors, levers) and an `OnTriggerEnterEvent`
    // from an actor crossing a trigger volume (the `default*Trigger`
    // family). Collecting `(entity, triggerer)` from both unifies the
    // dispatch.
    //
    // #2130 / SCR-D7-NEW3-01 — at most ONE signal per entity per frame.
    // Today the two populations happen to be disjoint (a `TriggerVolume`
    // only ever lands on a mesh-less REFR, and no live system emits
    // `ActivateEvent` yet — only the debug console does), so this dedup is
    // a no-op. It is here because that disjointness is a property of
    // unbuilt code: the recognizer test `on_activate_wins_over_on_trigger_enter`
    // proves a single script can legitimately define *both* handlers, so
    // once boot.rs's "Stage 4" (the real player-activates-a-REFR system)
    // lands, one player action against a trigger-volume-bearing REFR would
    // otherwise push the entity twice. The stage write itself is
    // idempotent, but the `QuestStageAdvanced` marker is not — a
    // non-idempotent fragment effect downstream (`AddItem`) would apply
    // twice. Enforcing it here costs one hash insert per event and keeps
    // the invariant local to the system that depends on it, rather than
    // as an unwritten precondition on a system that does not exist yet.
    //
    // First-write-wins ordering is deliberate: `ActivateEvent` is drained
    // first, matching the recognizer's `OnActivate`-beats-`OnTriggerEnter`
    // precedence.
    let mut triggered: Vec<(EntityId, EntityId)> = Vec::new();
    let mut signalled: HashSet<EntityId> = HashSet::new();
    if let Some(events) = world.query::<ActivateEvent>() {
        for (entity, ev) in events.iter() {
            if signalled.insert(entity) {
                triggered.push((entity, ev.activator));
            }
        }
    }
    if let Some(events) = world.query::<OnTriggerEnterEvent>() {
        for (entity, ev) in events.iter() {
            if signalled.insert(entity) {
                triggered.extend(ev.triggerers.iter().map(|triggerer| (entity, *triggerer)));
            }
        }
    }

    // Two-phase: collect (read), apply (write). Releases the
    // QuestStageState read borrow before we acquire the write.
    struct PendingAdvance {
        source: EntityId,
        quest: QuestFormId,
        target_stage: u16,
        disable_after_advance: bool,
    }
    let mut pending: Vec<PendingAdvance> = Vec::new();
    for (entity, triggerer) in triggered {
        let Some(comp) = advances.get(entity) else {
            continue;
        };
        // Activator gate — the entity that triggered (activator /
        // triggerer) must be the player when the gate is PlayerOnly.
        let gate_passes = match comp.activator_gate {
            ActivatorGate::Any => true,
            ActivatorGate::PlayerOnly => triggerer == player_entity,
            ActivatorGate::BaseForm(base_form_id) => world
                .get::<crate::scene::SceneAliasCandidate>(triggerer)
                .is_some_and(|identity| identity.base_form_id == base_form_id),
        };
        if !gate_passes {
            log::debug!(
                "quest advance source={entity} triggerer={triggerer} quest=0x{:08X} target={} gate={:?} passes=false",
                comp.owning_quest.0,
                comp.target_stage,
                comp.activator_gate,
            );
            continue;
        }
        // M47.1 — stage predicates evaluated through the generic
        // `ConditionList` evaluator. The subject for the condition
        // context is the triggered REFR (Papyrus's `Self` on this script
        // type binds to the alias's REFR, which is what `entity`
        // here represents). The evaluator handles the OR-precedence
        // grouping automatically — DA10's two-AND case is trivial,
        // but the same code path covers HasPerk-or-FactionRank
        // disjunctions, multi-condition gates, etc.
        let ctx = ConditionContext::for_subject(entity);
        let conditions_pass = evaluate_condition_list(&comp.conditions, world, &ctx);
        log::debug!(
            "quest advance source={entity} triggerer={triggerer} quest=0x{:08X} target={} gate={:?} passes=true conditions={conditions_pass}",
            comp.owning_quest.0,
            comp.target_stage,
            comp.activator_gate,
        );
        if conditions_pass {
            pending.push(PendingAdvance {
                source: entity,
                quest: comp.owning_quest,
                target_stage: comp.target_stage,
                disable_after_advance: comp.disable_after_advance,
            });
        }
    }
    drop(advances);

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
    let mut disable_sources = Vec::new();
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        for p in pending {
            let prev = stage_state.set_stage(p.quest, p.target_stage);
            advances_emitted.push(QuestStageAdvanced {
                quest: p.quest,
                previous_stage: prev,
                new_stage: p.target_stage,
            });
            if p.disable_after_advance {
                disable_sources.push(p.source);
            }
        }
    }
    if !disable_sources.is_empty() {
        if let Some(mut components) = world.query_mut::<QuestAdvanceOnActivate>() {
            for entity in disable_sources {
                components.remove(entity);
            }
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
    //
    // #1864 / SCR-D7-NEW-01 — append the whole producer batch while holding
    // the storage write lock. Another same-frame producer may already have
    // populated the compatibility sink; replacing it would lose its events.
    // The sequenced journal in QuestStageState remains authoritative.
    let Some(mut q) = world.query_mut::<QuestStageAdvancedBatch>() else {
        return;
    };
    if let Some(batch) = q.get_mut(player_entity) {
        batch.0.extend(advances_emitted);
    } else {
        q.insert(player_entity, QuestStageAdvancedBatch(advances_emitted));
    }
}

#[cfg(test)]
mod tests;
