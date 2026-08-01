//! Canonical state for Papyrus variables consumed by CTDA conditions.
//!
//! This starts with Skyrim's ubiquitous `default2StateActivator`: package
//! activation emits the normal [`ActivateEvent`], this system toggles its
//! open state, and `GetVMScriptVariable(..., "::isOpen_var")` observes the
//! same state through [`ScriptVariables`].

use std::collections::HashMap;

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::ConditionStringId;

use crate::events::ActivateEvent;

const IS_OPEN_VARIABLE: &str = "::isOpen_var";
const IS_ANIMATING_VARIABLE: &str = "::isAnimating_var";

/// Numeric Papyrus instance variables addressable by CTDA string parameter.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptVariables {
    values: HashMap<ConditionStringId, f32>,
}

impl Component for ScriptVariables {
    type Storage = SparseSetStorage<Self>;
}

impl ScriptVariables {
    pub fn get(&self, id: ConditionStringId) -> Option<f32> {
        self.values.get(&id).copied()
    }

    pub fn get_by_name(&self, name: &str) -> Option<f32> {
        self.get(ConditionStringId::from_text(name))
    }

    pub fn set(&mut self, id: ConditionStringId, value: f32) {
        self.values.insert(id, value);
    }

    pub fn set_by_name(&mut self, name: &str, value: f32) {
        self.set(ConditionStringId::from_text(name), value);
    }
}

/// Runtime state extracted from the `default2StateActivator` Papyrus class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TwoStateActivator {
    pub is_open: bool,
    pub is_animating: bool,
    pub do_once: bool,
    pub activated_once: bool,
}

impl Component for TwoStateActivator {
    type Storage = SparseSetStorage<Self>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwoStateTransition {
    Opened,
    Closed,
}

/// One-frame presentation boundary for model animation, collision, and audio.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TwoStateTransitionBatch(pub Vec<TwoStateTransition>);

impl Component for TwoStateTransitionBatch {
    type Storage = SparseSetStorage<Self>;
}

pub fn register(world: &mut World) {
    world.register::<ScriptVariables>();
    world.register::<TwoStateActivator>();
    world.register::<TwoStateTransitionBatch>();
}

/// Apply ObjectReference.SetOpen to a recognized two-state activator.
/// Returns false when the target has no compatible runtime state.
pub fn set_two_state_open(world: &World, entity: EntityId, open: bool) -> bool {
    let changed = {
        let Some(mut states) = world.query_mut::<TwoStateActivator>() else {
            return false;
        };
        let Some(state) = states.get_mut(entity) else {
            return false;
        };
        let changed = state.is_open != open;
        state.is_open = open;
        state.is_animating = false;
        changed
    };

    if let Some(mut variables) = world.query_mut::<ScriptVariables>() {
        if let Some(state) = variables.get_mut(entity) {
            state.set_by_name(IS_OPEN_VARIABLE, f32::from(open));
            state.set_by_name(IS_ANIMATING_VARIABLE, 0.0);
        }
    }
    if changed {
        if let Some(mut transitions) = world.query_mut::<TwoStateTransitionBatch>() {
            transitions.insert(
                entity,
                TwoStateTransitionBatch(vec![if open {
                    TwoStateTransition::Opened
                } else {
                    TwoStateTransition::Closed
                }]),
            );
        }
    }
    true
}

fn drain_transitions(world: &World) {
    let Some(mut transitions) = world.query_mut::<TwoStateTransitionBatch>() else {
        return;
    };
    let entities: Vec<EntityId> = transitions.iter().map(|(entity, _)| entity).collect();
    for entity in entities {
        transitions.remove(entity);
    }
}

/// Consume `OnActivate` for canonical two-state activators.
pub fn two_state_activator_system(world: &World, _dt: f32) {
    drain_transitions(world);
    let activated: Vec<EntityId> = world
        .query::<ActivateEvent>()
        .map(|events| events.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    if activated.is_empty() {
        return;
    }

    let mut updates = Vec::new();
    if let Some(mut states) = world.query_mut::<TwoStateActivator>() {
        for entity in activated {
            let Some(state) = states.get_mut(entity) else {
                continue;
            };
            if state.do_once && state.activated_once {
                continue;
            }
            state.is_open = !state.is_open;
            state.is_animating = false;
            state.activated_once = true;
            updates.push((entity, state.is_open));
        }
    }

    if let Some(mut variables) = world.query_mut::<ScriptVariables>() {
        for (entity, is_open) in &updates {
            if let Some(state) = variables.get_mut(*entity) {
                state.set_by_name(IS_OPEN_VARIABLE, f32::from(*is_open));
            }
        }
    }
    if let Some(mut transitions) = world.query_mut::<TwoStateTransitionBatch>() {
        for (entity, is_open) in updates {
            transitions.insert(
                entity,
                TwoStateTransitionBatch(vec![if is_open {
                    TwoStateTransition::Opened
                } else {
                    TwoStateTransition::Closed
                }]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn do_once_activator_opens_and_publishes_vm_variable() {
        let mut world = World::new();
        crate::register(&mut world);
        let activator = world.spawn();
        let actor = world.spawn();
        world.insert(
            activator,
            TwoStateActivator {
                do_once: true,
                ..Default::default()
            },
        );
        let mut variables = ScriptVariables::default();
        variables.set_by_name(IS_OPEN_VARIABLE, 0.0);
        world.insert(activator, variables);
        world.insert(activator, ActivateEvent { activator: actor });

        two_state_activator_system(&world, 0.0);

        assert!(world.get::<TwoStateActivator>(activator).unwrap().is_open);
        assert_eq!(
            world
                .get::<ScriptVariables>(activator)
                .unwrap()
                .get_by_name(IS_OPEN_VARIABLE),
            Some(1.0)
        );
        assert_eq!(
            world.get::<TwoStateTransitionBatch>(activator).unwrap().0,
            vec![TwoStateTransition::Opened]
        );

        two_state_activator_system(&world, 0.0);
        assert!(world.get::<TwoStateActivator>(activator).unwrap().is_open);
    }

    #[test]
    fn set_open_updates_state_variables_and_transition() {
        let mut world = World::new();
        crate::register(&mut world);
        let gate = world.spawn();
        world.insert(gate, TwoStateActivator::default());
        world.insert(gate, ScriptVariables::default());

        assert!(set_two_state_open(&world, gate, true));

        assert!(world.get::<TwoStateActivator>(gate).unwrap().is_open);
        let variables = world.get::<ScriptVariables>(gate).unwrap();
        assert_eq!(variables.get_by_name(IS_OPEN_VARIABLE), Some(1.0));
        assert_eq!(variables.get_by_name(IS_ANIMATING_VARIABLE), Some(0.0));
        assert_eq!(
            world.get::<TwoStateTransitionBatch>(gate).unwrap().0,
            vec![TwoStateTransition::Opened]
        );
    }
}
