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
//! ## What ships here
//!
//! - **Engine + contract + dispatch:** the [`QuestStageFragments`]
//!   resource, [`apply_effects`] against the canonical [`QuestStageState`]
//!   / [`QuestObjectiveState`], and [`quest_fragment_dispatch_system`]
//!   which consumes `QuestStageAdvanced` and cascades chained `SetStage`s
//!   (bounded).
//! - **Population (shipped, #1739 / `8a70b81a`):** [`QuestStageFragments`]
//!   is filled from real game data via the QUST `VMAD` fragment-section
//!   decoder
//!   (`byroredux_plugin::esm::records::script_instance::parse_quest_fragments`)
//!   feeding [`populate_quest_fragments_from_pex`], wired live from the
//!   cell loader. Validated end-to-end on real Skyrim data.
//!
//! ## Quest-ref resolution
//!
//! A fragment's effect targets `Self` / `Self.GetOwningQuest()` (→ the
//! advancing quest, known at dispatch) or a `Quest Property` (→ a *different*
//! quest bound by the QUST's own VMAD). The former always resolves; the
//! latter needs the quest's VMAD scripts-section registered via
//! [`QuestStageFragments::insert_vmad`] (the same bytes the fragment-binding
//! decoder reads, decoded a second time for its property table) — a
//! `Property`-targeted effect with no VMAD on hand, or naming a property the
//! VMAD doesn't carry, is skipped (logged), never guessed.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::script_instance::ScriptInstanceData;

use crate::quest_stages::{
    QuestFormId, QuestObjectiveState, QuestStageAdvanced, QuestStageAdvancedBatch, QuestStageState,
};
use byroredux_papyrus::ast::{Script, ScriptItem, StateItem, Stmt};
use byroredux_papyrus::span::Spanned;

use crate::translate::compose::QuestRef;
use crate::translate::effects::{lower_fragment, Effect};

/// Lowered quest-stage fragments, keyed by `(quest, stage)`. Populated at
/// cell load by [`populate_quest_fragments_from_pex`] from the QUST `VMAD`
/// fragment bindings the decoder recovers; consumed by
/// [`quest_fragment_dispatch_system`].
#[derive(Debug, Default)]
pub struct QuestStageFragments {
    map: HashMap<(QuestFormId, u16), Vec<Effect>>,
    /// The QF_ script's own property table per quest — the same VMAD
    /// scripts-section bytes the QUST record's fragment section is read
    /// alongside. Lets a fragment's cross-quest `Property`-targeted
    /// effect (`SomeOtherQuest.SetStage(..)` via a `Quest Property`)
    /// resolve at dispatch time instead of always skipping.
    vmad: HashMap<QuestFormId, ScriptInstanceData>,
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

    /// Register a quest's own VMAD scripts section (its declared
    /// property bindings), so `Property`-targeted effects in its
    /// fragments can resolve. A no-op for a VMAD with no attached
    /// scripts — nothing a `Property` lookup could ever match.
    pub fn insert_vmad(&mut self, quest: QuestFormId, vmad: ScriptInstanceData) {
        if vmad.has_script() {
            self.vmad.insert(quest, vmad);
        }
    }

    /// The registered VMAD for `quest`, if any.
    pub fn vmad(&self, quest: QuestFormId) -> Option<&ScriptInstanceData> {
        self.vmad.get(&quest)
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

/// A top-level (or state) function body from a decompiled script, by name
/// (Papyrus identifiers are case-insensitive). Quest `Fragment_N`
/// functions are top-level, but state functions are checked too so the
/// lookup is robust.
fn function_body<'a>(script: &'a Script, name: &str) -> Option<&'a [Spanned<Stmt>]> {
    for item in &script.body {
        match &item.node {
            ScriptItem::Function(f) if f.name.node.0.eq_ignore_ascii_case(name) => {
                return Some(&f.body);
            }
            ScriptItem::State(st) => {
                for si in &st.body {
                    if let StateItem::Function(f) = &si.node {
                        if f.name.node.0.eq_ignore_ascii_case(name) {
                            return Some(&f.body);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Populate [`QuestStageFragments`] for one quest from its compiled quest
/// script (`.pex`). Each `(stage, fragment_name)` binding comes from the
/// QUST `VMAD` fragment section
/// ([`byroredux_plugin::esm::records::script_instance::parse_quest_fragments`]);
/// all fragments of a quest share a single `QF_` script, so the caller
/// resolves and passes its bytes once. Returns the number of stage
/// fragments inserted (non-empty, fully-lowered ones).
///
/// This is the runtime half of the M47.2 keystone: it takes the
/// stage→`Fragment_N` binding the decoder recovered and turns each
/// fragment body into the canonical [`Effect`]s the dispatcher applies —
/// closing the loop from real game data to quest behavior on screen.
///
/// Mirrors [`crate::translate::translate_pex`]'s hostile-input contract:
/// a `.pex` that fails to parse/decompile — including a decompiler panic
/// (#1816) — inserts nothing (logged at debug), never aborts the load. A
/// fragment carrying a statement no effect primitive claims lowers to
/// `None` and is declined (safe — no behavior attached), never partially
/// applied.
pub fn populate_quest_fragments_from_pex(
    frags: &mut QuestStageFragments,
    quest: QuestFormId,
    pex_bytes: &[u8],
    bindings: &[(u16, &str)],
) -> usize {
    let pex = match byroredux_pex::parse(pex_bytes) {
        Ok(p) => p,
        Err(e) => {
            log::debug!("populate_quest_fragments: .pex parse failed (quest {:08X}): {e}", quest.0);
            return 0;
        }
    };
    let script = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        byroredux_pex::decompile::decompile_script(&pex)
    })) {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            log::debug!("populate_quest_fragments: decompile failed (quest {:08X}): {e}", quest.0);
            return 0;
        }
        Err(_) => {
            log::debug!("populate_quest_fragments: decompile panicked (quest {:08X})", quest.0);
            return 0;
        }
    };
    populate_quest_fragments_from_script(frags, quest, &script, bindings)
}

/// The AST half of [`populate_quest_fragments_from_pex`] — lower each
/// `(stage, fragment_name)` binding against an already-decompiled (or
/// source-parsed) [`Script`] and register the non-empty lowerings.
/// Split out so the lowering path is unit-testable from a `.psc` source
/// without game-data `.pex` bytes.
pub fn populate_quest_fragments_from_script(
    frags: &mut QuestStageFragments,
    quest: QuestFormId,
    script: &Script,
    bindings: &[(u16, &str)],
) -> usize {
    let mut inserted = 0;
    for (stage, fragment_name) in bindings {
        let Some(body) = function_body(script, fragment_name) else {
            log::debug!(
                "populate_quest_fragments: fn '{fragment_name}' absent in quest {:08X} .pex",
                quest.0
            );
            continue;
        };
        // Decline-on-any-unmodeled-term: a fragment the effect table can't
        // fully lower is skipped, not partially applied. An empty
        // fully-lowered fragment carries no effects, so it needn't occupy
        // the map (a lookup miss is equivalent to an empty entry).
        if let Some(effects) = lower_fragment(body) {
            if !effects.is_empty() {
                frags.insert(quest, *stage, effects);
                inserted += 1;
            }
        }
    }
    inserted
}

/// Consume [`QuestStageAdvanced`] markers and run the matching
/// `(quest, stage)` fragments, cascading any `SetStage`s they perform
/// (bounded by [`MAX_CASCADE`]). Runs after `quest_advance_system` (which
/// emits the initial markers) and before end-of-frame cleanup.
///
/// Effect resolution passes the quest's own registered VMAD (see
/// [`QuestStageFragments::insert_vmad`]), when one was registered, so
/// `Self`/owning-quest-targeted effects always apply and a cross-quest
/// `Property`-targeted effect resolves too, as long as the named property
/// is an `Object`-typed binding on the quest's own VMAD. Object-targeting
/// effects (`Enable`/`Disable`/`MoveTo`/…) still decline at the *lowering*
/// stage — [`lower_fragment`] doesn't emit them yet, a separate gap from
/// VMAD resolution. The table is empty (and this a no-op) on loads
/// without `--scripts-bsa` or on pre-Papyrus games.
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
            let advances = apply_effects(
                effects,
                quest,
                frags.vmad(quest),
                &mut stages,
                &mut objectives,
            );
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
