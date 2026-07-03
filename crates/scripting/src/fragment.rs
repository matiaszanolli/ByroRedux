//! Quest-stage *fragment* dispatch — the runtime half of the b2 lever
//! ([`docs/engine/m47-2-recognizer-scaling.md`]).
//!
//! A Papyrus quest carries one compiled `Fragment_N` function per stage;
//! the runtime runs stage N's fragment when the quest is `SetStage`-d to
//! N. The [`crate::translate::effects`] lowerer turns each fragment body
//! into a `Vec<Effect>`; this module stores those per `(quest, stage)`
//! and applies them when a [`QuestStageAdvanced`] marker says the stage
//! was set.
//!
//! ## What ships here vs. what's pending
//!
//! - **Engine + contract + dispatch (here, tested):** the
//!   [`QuestStageFragments`] resource, [`apply_effects`] against the
//!   canonical [`QuestStageState`] / [`QuestObjectiveState`], and
//!   [`quest_fragment_dispatch_system`] which consumes `QuestStageAdvanced`
//!   and cascades chained `SetStage`s (bounded).
//! - **Population (pending):** filling [`QuestStageFragments`] from real
//!   game data needs the **QUST VMAD fragment-section decoder** — the
//!   binary table mapping a quest's stage index to its `Fragment_N`
//!   function — which is not decoded yet (see
//!   `crates/plugin/src/esm/records/script_instance.rs`: "fragment decode
//!   is a later phase"). Per the no-guessing policy the format is not
//!   reverse-engineered here. Until it lands, the resource stays empty at
//!   runtime and the dispatcher is a no-op; tests populate it directly.
//!   This mirrors how [`QuestStageAdvanced`] shipped before its consumer.
//!
//! ## Quest-ref resolution
//!
//! A fragment's effect targets `Self` / `Self.GetOwningQuest()` (→ the
//! advancing quest, known at dispatch) or a `Quest Property` (→ the
//! quest's VMAD binding). Only the former resolves without the QUST VMAD;
//! a `Property`-targeted effect with no VMAD on hand is skipped (logged),
//! never guessed.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::script_instance::ScriptInstanceData;

use crate::quest_stages::{
    QuestFormId, QuestObjectiveState, QuestStageAdvanced, QuestStageAdvancedBatch, QuestStageState,
};
use crate::translate::compose::QuestRef;
use crate::translate::effects::Effect;

/// Lowered quest-stage fragments, keyed by `(quest, stage)`. Populated by
/// the (pending) QUST-fragment decoder; consumed by
/// [`quest_fragment_dispatch_system`].
#[derive(Debug, Default)]
pub struct QuestStageFragments {
    map: HashMap<(QuestFormId, u16), Vec<Effect>>,
}

impl Resource for QuestStageFragments {}

impl QuestStageFragments {
    /// Register a stage's lowered fragment effects.
    pub fn insert(&mut self, quest: QuestFormId, stage: u16, effects: Vec<Effect>) {
        self.map.insert((quest, stage), effects);
    }

    /// The lowered effects for a `(quest, stage)`, if any.
    pub fn get(&self, quest: QuestFormId, stage: u16) -> Option<&[Effect]> {
        self.map.get(&(quest, stage)).map(Vec::as_slice)
    }

    /// Number of registered stage fragments.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Resolve an effect's [`QuestRef`] to a concrete quest FormID.
/// `Self`/`GetOwningQuest` resolve to the advancing quest; a
/// `Quest Property` resolves through the quest's VMAD (when supplied).
fn resolve_quest(
    via: &QuestRef,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
) -> Option<QuestFormId> {
    match via {
        QuestRef::SelfRef | QuestRef::OwningQuest => Some(context),
        QuestRef::Property(name) => vmad?
            .scripts
            .iter()
            .find_map(|s| s.object_form_id(name))
            .map(QuestFormId),
    }
}

/// Apply one effect to the canonical stage/objective state. Returns a
/// [`QuestStageAdvanced`] when the effect was a `SetStage` (so the caller
/// can cascade), or `None` otherwise / when the target can't resolve.
pub fn apply_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
) -> Option<QuestStageAdvanced> {
    let Some(quest) = resolve_quest(effect.quest_ref(), context, vmad) else {
        log::debug!("fragment effect skipped: unresolved quest ref {:?}", effect.quest_ref());
        return None;
    };
    match effect {
        Effect::SetStage { stage, .. } => {
            let previous_stage = stages.set_stage(quest, *stage);
            Some(QuestStageAdvanced {
                quest,
                previous_stage,
                new_stage: *stage,
            })
        }
        Effect::SetObjectiveDisplayed { objective, displayed, .. } => {
            objectives.set_displayed(quest, *objective, *displayed);
            None
        }
        Effect::SetObjectiveCompleted { objective, completed, .. } => {
            objectives.set_completed(quest, *objective, *completed);
            None
        }
        Effect::SetObjectiveFailed { objective, failed, .. } => {
            objectives.set_failed(quest, *objective, *failed);
            None
        }
        Effect::CompleteAllObjectives { .. } => {
            objectives.complete_all(quest);
            None
        }
    }
}

/// Apply a whole fragment's effects, returning the chained
/// [`QuestStageAdvanced`]s its `SetStage`s produced.
pub fn apply_effects(
    effects: &[Effect],
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
) -> Vec<QuestStageAdvanced> {
    effects
        .iter()
        .filter_map(|e| apply_effect(e, context, vmad, stages, objectives))
        .collect()
}

/// Maximum stage-fragment cascade depth in one dispatch pass — a fragment
/// `SetStage`-ing the next stage runs that stage's fragment too. The cap
/// is a backstop against a cyclic SetStage chain (a fragment that sets a
/// stage whose fragment sets it back); real quest chains are short.
const MAX_CASCADE: usize = 64;

/// Register the fragment-dispatch resources. Both are empty
/// default-constructible runtime stores (unlike `PlayerEntity` /
/// `QuestStageState`, which carry per-app-instance state and stay
/// caller-inserted), so initialising them at world-init is safe and
/// keeps [`quest_fragment_dispatch_system`] panic-free out of the box.
pub fn register(world: &mut World) {
    world.insert_resource(QuestStageFragments::default());
    world.insert_resource(QuestObjectiveState::default());
}

/// Consume [`QuestStageAdvanced`] markers and run the matching
/// `(quest, stage)` fragments, cascading any `SetStage`s they perform
/// (bounded by [`MAX_CASCADE`]). Runs after `quest_advance_system` (which
/// emits the initial markers) and before end-of-frame cleanup.
///
/// Resolution uses no QUST VMAD (not decoded yet), so only `Self`/
/// owning-quest-targeted effects apply; property-targeted effects are
/// skipped. The resource is empty until the QUST-fragment decoder lands,
/// making this a safe no-op at runtime today.
pub fn quest_fragment_dispatch_system(world: &World) {
    // Snapshot the stage advances this frame. #1864 / SCR-D7-NEW-01 — a
    // batch can hold >1 advance from the same frame; iterate every entry,
    // not just the sink entity's single (pre-fix) marker value.
    let mut queue: Vec<(QuestFormId, u16)> = Vec::new();
    if let Some(markers) = world.query::<QuestStageAdvancedBatch>() {
        for (_entity, batch) in markers.iter() {
            for ev in &batch.0 {
                queue.push((ev.quest, ev.new_stage));
            }
        }
    }
    if queue.is_empty() {
        return;
    }

    // Nothing to dispatch if no fragments are registered (today's runtime).
    {
        let frags = world.resource::<QuestStageFragments>();
        if frags.is_empty() {
            return;
        }
    }

    let mut chained: Vec<QuestStageAdvanced> = Vec::new();
    let mut steps = 0usize;
    {
        let frags = world.resource::<QuestStageFragments>();
        let mut stages = world.resource_mut::<QuestStageState>();
        let mut objectives = world.resource_mut::<QuestObjectiveState>();
        while let Some((quest, stage)) = queue.pop() {
            steps += 1;
            if steps > MAX_CASCADE {
                log::warn!(
                    "quest fragment cascade exceeded {MAX_CASCADE} steps at quest {:?} stage {stage}; \
                     stopping (possible cyclic SetStage)",
                    quest
                );
                break;
            }
            let Some(effects) = frags.get(quest, stage) else {
                continue;
            };
            // QUST VMAD is not decoded yet → no property binding available.
            let advances = apply_effects(effects, quest, None, &mut stages, &mut objectives);
            for adv in advances {
                // Only cascade genuine transitions (skip a no-op re-set of
                // the same stage to avoid trivial self-loops).
                if adv.new_stage != stage {
                    queue.push((adv.quest, adv.new_stage));
                }
                chained.push(adv);
            }
        }
    }

    // Emit markers for the chained advances so other consumers (journal
    // UI, further-frame dispatch) observe them. Co-opts the same
    // player-entity sink quest_advance_system uses.
    //
    // #1864 / SCR-D7-NEW-01 — insert the whole batch ONCE. A single
    // `apply_effects` call (let alone the whole cascade) can produce >1
    // chained advance; looping `insert()` onto this one shared sink entity
    // would silently collapse every advance but the last.
    if chained.is_empty() {
        return;
    }
    let player_entity = world.resource::<crate::papyrus_demo::PlayerEntity>().0;
    if let Some(mut q) = world.query_mut::<QuestStageAdvancedBatch>() {
        q.insert(player_entity, QuestStageAdvancedBatch(chained));
    }
}

#[cfg(test)]
mod tests;
