//! Starfield condition forms (M47.2): `ConditionForm.IsTrue(akRefObject,
//! akTargetObject)`.
//!
//! A `CNDF` record is a named `CTDA` list. Starfield's `ConditionForm.pex` declares
//! the native `IsTrue(akRefObject, akTargetObject)` with no body; it evaluates the
//! form's conditions. The pairing of the two arguments with the condition
//! context is read from the corpus: of the 2 535 conditions in Starfield.esm's 899
//! condition forms (`examples/cndf_census.rs`), 2 334 run on the *Subject* and 7 on
//! the *Target*, which is how every condition list in the game reads its two
//! references — `akRefObject` is the subject, `akTargetObject` the target.
//!
//! The list is evaluated only when every condition is modeled end to end
//! ([`is_modeled`](crate::condition::is_modeled)) and none reads state the
//! interpreter holds locked while it runs (quest stages, alias bindings); a form
//! with any other condition stops the run instead of answering with the engine's
//! fail-open pass. 338 of the 899 forms qualify today.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::{Condition, ConditionList, RunOn};
use byroredux_plugin::esm::records::EsmIndex;

use crate::condition::{evaluate, is_modeled, ConditionContext, ConditionFunction};

/// Every `CNDF`'s conditions, by FormID. `NOT_SAVED_BY_DESIGN`: re-derived from
/// the load order every boot.
#[derive(Debug, Default, Clone)]
pub struct ConditionForms(Arc<HashMap<u32, ConditionList>>);

impl Resource for ConditionForms {}

/// Install the load order's condition forms; returns how many.
pub fn install_condition_forms(world: &mut World, index: &EsmIndex) -> usize {
    let forms: HashMap<u32, ConditionList> = index
        .condition_forms
        .iter()
        .map(|(&form_id, form)| (form_id, form.conditions.clone()))
        .collect();
    let count = forms.len();
    world.insert_resource(ConditionForms(Arc::new(forms)));
    count
}

impl ConditionForms {
    /// The conditions of `form`, for the debug console.
    pub fn conditions(&self, form: u32) -> Option<&ConditionList> {
        self.0.get(&form)
    }

    /// The loaded forms the interpreter can answer: every condition modeled and runnable
    /// inside a run, in FormID order (the debug console lists them).
    pub fn runnable(&self, world: &World) -> Vec<u32> {
        let mut forms: Vec<u32> = self
            .0
            .iter()
            .filter(|(_, list)| list.iter().all(|c| runnable_inside_a_run(world, c)))
            .map(|(&form, _)| form)
            .collect();
        forms.sort_unstable();
        forms
    }

    /// Whether `form` is a loaded condition form.
    pub fn contains(&self, form: u32) -> bool {
        self.0.contains_key(&form)
    }
}

/// Whether the interpreter can evaluate `condition` while it holds the quest
/// state: modeled, and not a read of quest stages or alias bindings.
fn runnable_inside_a_run(world: &World, condition: &Condition) -> bool {
    is_modeled(world, condition)
        && condition.run_on != RunOn::QuestAlias
        && !matches!(
            crate::condition::function_of(world, condition.function_index),
            ConditionFunction::GetStage
                | ConditionFunction::GetStageDone
                | ConditionFunction::GetQuestRunning
                | ConditionFunction::GetQuestVariable
                | ConditionFunction::GetIsAliasRef
        )
}

/// `form.IsTrue(subject, target)`: `None` when `form` is not a loaded condition
/// form or has a condition this call cannot answer.
pub fn is_true(world: &World, form: u32, subject: EntityId, target: Option<EntityId>) -> Option<bool> {
    let forms = world.try_resource::<ConditionForms>()?;
    let conditions = forms.0.get(&form)?;
    if !conditions.iter().all(|condition| runnable_inside_a_run(world, condition)) {
        return None;
    }
    let mut ctx = ConditionContext::for_subject(subject);
    ctx.target = target;
    Some(evaluate(conditions, world, &ctx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::condition::{ComparisonOp, ConditionValue};
    use byroredux_plugin::esm::records::CndfRecord;

    fn global_equals(global: u32, value: f32) -> Condition {
        Condition {
            function_index: 74,
            param_1: global,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(value),
            ..Default::default()
        }
    }

    #[test]
    fn a_modeled_form_evaluates_and_an_unmodeled_one_declines() {
        let mut index = EsmIndex::default();
        index.condition_forms.insert(
            1,
            CndfRecord { form_id: 1, conditions: vec![global_equals(0x50, 3.0)], ..Default::default() },
        );
        index.condition_forms.insert(
            2,
            CndfRecord {
                form_id: 2,
                conditions: vec![Condition { function_index: 837, ..Default::default() }],
                ..Default::default()
            },
        );
        index.condition_forms.insert(
            3,
            CndfRecord {
                form_id: 3,
                conditions: vec![Condition { function_index: 59, ..Default::default() }],
                ..Default::default()
            },
        );
        let mut world = World::new();
        assert_eq!(install_condition_forms(&mut world, &index), 3);
        let actor = world.spawn();
        world.insert_resource(crate::Globals::default());
        assert_eq!(is_true(&world, 1, actor, None), Some(false), "the global reads 0");
        world.resource_mut::<crate::Globals>().set(0x50, 3.0);
        assert_eq!(is_true(&world, 1, actor, None), Some(true));
        assert_eq!(is_true(&world, 2, actor, None), None, "function 837 is uncatalogued");
        assert_eq!(is_true(&world, 3, actor, None), None, "GetStageDone reads locked quest state");
        assert_eq!(is_true(&world, 9, actor, None), None, "not a condition form");
    }
}
