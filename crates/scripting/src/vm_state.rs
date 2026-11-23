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


const IS_OPEN_VARIABLE: &str = "::isOpen_var";
const IS_ANIMATING_VARIABLE: &str = "::isAnimating_var";

/// Numeric Papyrus instance variables addressable by CTDA string parameter.
///
/// A reference variable is kept as two exact `f32` halves under `<name>#lo` / `<name>#hi`
/// ([`Self::set_reference_by_name`]). Only the names' hashes are stored, so the variables also remember which pairs
/// hold a reference: the save writes each as its stable identity (LMC-SAV-002..004) and the load splits it again
/// through the current load order.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "save", serde(from = "SavedScriptVariables", into = "SavedScriptVariables"))]
pub struct ScriptVariables {
    values: HashMap<ConditionStringId, f32>,
    /// The `(low, high)` half keys of every reference variable set through [`Self::set_reference_by_name`].
    references: Vec<(ConditionStringId, ConditionStringId)>,
}

/// [`ScriptVariables`] as saved: the plain values, and each reference variable as its half keys and the one FormID
/// they make, saved as a stable identity.
#[cfg(feature = "save")]
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedScriptVariables {
    values: HashMap<ConditionStringId, f32>,
    references: Vec<(ConditionStringId, ConditionStringId, byroredux_core::stable_form_id::SavedForm)>,
}

#[cfg(feature = "save")]
impl From<ScriptVariables> for SavedScriptVariables {
    fn from(mut variables: ScriptVariables) -> Self {
        let references = variables
            .references
            .iter()
            .map(|&(low, high)| {
                let form = crate::obscript_vm::form_from_halves(
                    variables.values.remove(&low).unwrap_or(0.0),
                    variables.values.remove(&high).unwrap_or(0.0),
                );
                (low, high, byroredux_core::stable_form_id::SavedForm::save(form))
            })
            .collect();
        Self { values: variables.values, references }
    }
}

#[cfg(feature = "save")]
impl From<SavedScriptVariables> for ScriptVariables {
    /// A reference to an absent plugin's form is left unset, which reads as no reference.
    fn from(saved: SavedScriptVariables) -> Self {
        let mut variables = Self { values: saved.values, references: Vec::new() };
        for (low, high, form) in saved.references {
            if let Some(form) = form.load() {
                let (low_value, high_value) = crate::obscript_vm::form_halves(form);
                variables.values.insert(low, low_value);
                variables.values.insert(high, high_value);
                variables.references.push((low, high));
            }
        }
        variables
    }
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

    /// Set the reference variable `name` to `form`, as its two exact halves under `<name>#lo` / `<name>#hi`, and
    /// remember the pair so a save can write the reference as a stable identity.
    pub fn set_reference_by_name(&mut self, name: &str, form: u32) {
        let (low_name, high_name) = crate::obscript_objects::ref_variable_halves(name);
        let (low_key, high_key) = (ConditionStringId::from_text(&low_name), ConditionStringId::from_text(&high_name));
        let (low, high) = crate::obscript_vm::form_halves(form);
        self.set(low_key, low);
        self.set(high_key, high);
        if !self.references.contains(&(low_key, high_key)) {
            self.references.push((low_key, high_key));
        }
    }

    /// #7338 — set the `default2StateActivator` mirror (`::isOpen_var`, `::isAnimating_var`) from `state`, the
    /// pair the recognizer stamps beside a [`TwoStateActivator`] and the toggle keeps in step.
    pub fn mirror_two_state(&mut self, state: &TwoStateActivator) {
        self.set_by_name(IS_OPEN_VARIABLE, f32::from(state.is_open));
        self.set_by_name(IS_ANIMATING_VARIABLE, f32::from(state.is_animating));
    }
}

/// Runtime state extracted from the `default2StateActivator` Papyrus class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
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

    if let Some(mut variables) = world.query_mut::<ScriptVariables>()
        && let Some(state) = variables.get_mut(entity) {
            state.set_by_name(IS_OPEN_VARIABLE, f32::from(open));
            state.set_by_name(IS_ANIMATING_VARIABLE, 0.0);
        }
    if changed
        && let Some(mut transitions) = world.query_mut::<TwoStateTransitionBatch>() {
            transitions.insert(
                entity,
                TwoStateTransitionBatch(vec![if open {
                    TwoStateTransition::Opened
                } else {
                    TwoStateTransition::Closed
                }]),
            );
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

/// The activated entities whose two-state toggle runs this frame. A door runtime verdict ([`crate::DefaultActivation`]) decides for
/// the doors: `Refused` keeps the state, `Admitted` toggles even on a [`crate::NativeActivation`] (a door's default processing is
/// the toggle). With no verdict the Papyrus rule stands: a native activation sends no `OnActivate`, so it does not toggle.
fn default_toggle_targets(world: &World) -> Vec<EntityId> {
    let Some(events) = world.query::<crate::ActivateEvent>() else {
        return Vec::new();
    };
    let native = world.query::<crate::NativeActivation>();
    let verdicts = world.query::<crate::DefaultActivation>();
    events
        .iter()
        .filter(|(entity, _)| match verdicts.as_ref().and_then(|verdicts| verdicts.get(*entity)) {
            Some(crate::DefaultActivation::Refused) => false,
            Some(crate::DefaultActivation::Admitted) => true,
            None => native.as_ref().is_none_or(|marks| marks.get(*entity).is_none()),
        })
        .map(|(entity, _)| entity)
        .collect()
}

/// Consume `OnActivate` for canonical two-state activators.
pub fn two_state_activator_system(world: &World, _dt: f32) {
    drain_transitions(world);
    // #6218 — `Activate(…, true)` is default processing only: "no OnActivate event
    // will be sent to any scripts" (CK), so a `NativeActivation` does not toggle.
    let activated = default_toggle_targets(world);
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
            // M66 — an activation while the door's clip is running is ignored (DRT-ACT-001); the presentation
            // system sets `is_animating` for a clip's length and clears it at the end.
            if state.is_animating {
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

/// #6705 — this subsystem's saved column(s). Called by the binary's `build_save_registry` in its
/// row order (the schema fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    // #2291 — levers, switches, and puzzle doors recognized as
    // default2StateActivator mutate both the state machine and its
    // CTDA-visible VM variables at runtime. Both are delta-safe.
    registry.register_component::<TwoStateActivator>("TwoStateActivator");
    registry.register_component::<ScriptVariables>("ScriptVariables");
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
        world.insert(activator, crate::events::ActivateEvent { activator: actor });

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

    /// #6218 — a default-processing-only activation is no `OnActivate`: the lever does not flip.
    #[test]
    fn a_native_only_activation_does_not_toggle_the_activator() {
        let mut world = World::new();
        crate::register(&mut world);
        let lever = world.spawn();
        world.insert(lever, TwoStateActivator::default());
        world.insert(lever, ScriptVariables::default());
        world.insert(lever, crate::events::ActivateEvent { activator: lever });
        world.insert(lever, crate::events::NativeActivation);
        two_state_activator_system(&world, 0.0);
        assert!(!world.get::<TwoStateActivator>(lever).unwrap().is_open);
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

/// GECK / CK *GetOpenState*: 0 none (no open / closed states), 1 open, 2 opening, 3 closed, 4 closing —
/// read from the two-state activator, the only open / closed state the runtime keeps (doors included);
/// a reference without one has none. The ObScript and Papyrus `GetOpenState` both read this.
pub fn open_state(world: &World, entity: byroredux_core::ecs::EntityId) -> i32 {
    match world.get::<TwoStateActivator>(entity) {
        None => 0,
        Some(state) => match (state.is_open, state.is_animating) {
            (true, false) => 1,
            (true, true) => 2,
            (false, false) => 3,
            (false, true) => 4,
        },
    }
}
