//! Destructible objects (M47.2): `ObjectReference.DamageObject`,
//! `GetCurrentDestructionStage`, `ClearDestruction`, `SetDestroyed` and
//! `IsDestroyed`, and the `OnDestructionStageChanged` event.
//!
//! The data is the base record's destructible block (xEdit `wbDEST`): a health
//! and `DSTD` stages. CK *Destructible Objects*: "Each stage is a state where
//! something will happen, depending on the percentage of health the object has.
//! So if I have a stage with Health Percentage at 90, this object will go to
//! this particular stage when it hits 90% health. The model damage stage will be
//! what scripts, etc. use as the data to reference the stage." Stage flags: Cap
//! Damage ("will stop skipping stages if too much damage is done: if you've got
//! 20 health on your object and you cap with a health percentage of 80%, the
//! first hit only brings health down to 80%"), Disable and Destroy (the
//! tutorial checks both for an object that is to go), Ignore External Dmg
//! ("external damage (not self damage) will be ignored while at this stage").
//! CK *GetCurrentDestructionStage*: "a completely undamaged object appears to
//! have a destruction stage of -1"; *OnDestructionStageChanged(aiOldStage,
//! aiCurrentStage)*: "called when the object's destruction stage worsens"
//! (skipped stages are why both are passed); *ClearDestruction*: "clears all
//! effects of destruction from the object. Will NOT fire off destruction stage
//! changed events"; *SetDestroyed*: "sets or clears this object's destroyed
//! flag".
//!
//! Each reference's damage and destroyed flag is a saved [`DestructionStates`]
//! keyed by the reference's FormID. What is *not* modelled is the visible side
//! of a stage: replacement models, explosions and debris. A stage's Disable flag
//! disables the reference, as the tutorial's destroyed-and-disabled barricade.
//! An object with a health of 0 or less has no percentage to cross: any damage
//! takes it to 0%.

use std::collections::{BTreeMap, HashMap};

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::cell::{DestructionData, DestructionStage};
use byroredux_plugin::esm::records::EsmIndex;

/// Base object → its destructible data. `NOT_SAVED_BY_DESIGN`: rebuilt from the
/// records every boot.
#[derive(Debug, Default, Clone)]
pub struct DestructionTable(pub(crate) HashMap<u32, DestructionData>);

impl Resource for DestructionTable {}

impl DestructionTable {
    pub fn from_index(index: &EsmIndex) -> Self {
        Self(
            index
                .cells
                .statics
                .iter()
                .filter_map(|(form, object)| object.destruction.clone().map(|data| (*form, data)))
                .collect(),
        )
    }

    pub fn get(&self, base: u32) -> Option<&DestructionData> {
        self.0.get(&base)
    }
}

/// One reference's destruction.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ReferenceDestruction {
    /// Damage taken so far.
    pub damage: f32,
    /// The destroyed flag: a Destroy stage's, or a script's `SetDestroyed`.
    pub destroyed: bool,
}

/// Every damaged or destroyed reference, by FormID. Saved.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct DestructionStates(pub BTreeMap<u32, ReferenceDestruction>);

impl Resource for DestructionStates {}

/// Install the load order's [`DestructionTable`]; returns how many base objects
/// are destructible.
pub fn install_destruction_table(world: &mut World, index: &EsmIndex) -> usize {
    let table = DestructionTable::from_index(index);
    let count = table.0.len();
    world.insert_resource(table);
    count
}

fn remaining_percent(data: &DestructionData, damage: f32) -> f32 {
    if data.health <= 0 {
        return if damage > 0.0 { 0.0 } else { 100.0 };
    }
    (100.0 * (data.health as f32 - damage) / data.health as f32).clamp(0.0, 100.0)
}

/// The stage the object is at after `damage`: the most damaged stage whose
/// percentage it has reached (the later one on a tie), if any.
fn stage_at(data: &DestructionData, damage: f32) -> Option<DestructionStage> {
    if damage <= 0.0 {
        return None;
    }
    let remaining = remaining_percent(data, damage);
    data.stages
        .iter()
        .filter(|stage| f32::from(stage.health_percent) >= remaining)
        .min_by(|a, b| a.health_percent.cmp(&b.health_percent).then(b.index.cmp(&a.index)))
        .copied()
}

fn stage_number(stage: Option<DestructionStage>) -> i32 {
    stage.map_or(-1, |stage| i32::from(stage.model_damage_stage))
}

/// The destruction stage after `damage` (-1 when undamaged).
pub fn current_stage(data: &DestructionData, damage: f32) -> i32 {
    stage_number(stage_at(data, damage))
}

/// What a `DamageObject` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageOutcome {
    pub old_stage: i32,
    pub new_stage: i32,
    /// The new stage destroys the object.
    pub destroyed: bool,
    /// The new stage disables the object.
    pub disable: bool,
}

/// Apply `amount` of external damage to `state` under `data`.
pub fn apply_damage(data: &DestructionData, state: &mut ReferenceDestruction, amount: f32) -> DamageOutcome {
    let old = stage_at(data, state.damage);
    let old_stage = stage_number(old);
    let unchanged = DamageOutcome { old_stage, new_stage: old_stage, destroyed: false, disable: false };
    if !amount.is_finite() || amount <= 0.0 {
        return unchanged;
    }
    if old.is_some_and(|stage| stage.flags & DestructionStage::IGNORE_EXTERNAL_DAMAGE != 0) {
        return unchanged;
    }
    let health = data.health.max(0) as f32;
    let before = remaining_percent(data, state.damage);
    let mut damage = (state.damage + amount).min(health.max(amount));
    if data.health > 0 {
        damage = damage.min(health);
        let after = remaining_percent(data, damage);
        // Cap Damage: the first capped stage this hit would pass stops it there.
        let cap = data
            .stages
            .iter()
            .filter(|stage| stage.flags & DestructionStage::CAP_DAMAGE != 0)
            .filter(|stage| before > f32::from(stage.health_percent) && after < f32::from(stage.health_percent))
            .max_by_key(|stage| stage.health_percent);
        if let Some(cap) = cap {
            damage = health * (1.0 - f32::from(cap.health_percent) / 100.0);
        }
    }
    state.damage = damage;
    let new = stage_at(data, damage);
    let flagged = |flag: u8| new.is_some_and(|stage| stage.flags & flag != 0);
    let destroyed = flagged(DestructionStage::DESTROY);
    if destroyed {
        state.destroyed = true;
    }
    DamageOutcome {
        old_stage,
        new_stage: stage_number(new),
        destroyed,
        disable: flagged(DestructionStage::DISABLE),
    }
}

/// The base object `entity` was placed from.
fn base_of(world: &World, entity: EntityId) -> Option<u32> {
    world
        .get::<crate::scene::SceneAliasCandidate>(entity)
        .map(|candidate| candidate.base_form_id)
        .filter(|&base| base != 0)
}

/// `reference.DamageObject(amount)`. Returns the outcome when the reference's
/// base is destructible; the caller delivers the stage-changed event and
/// applies a Disable.
pub fn damage_object(world: &World, entity: EntityId, form: u32, amount: f32) -> Option<DamageOutcome> {
    let base = base_of(world, entity)?;
    let data = world.try_resource::<DestructionTable>()?.get(base)?.clone();
    let mut states = world.try_resource_mut::<DestructionStates>()?;
    let state = states.0.entry(form).or_default();
    let outcome = apply_damage(&data, state, amount);
    drop(states);
    if outcome.old_stage != outcome.new_stage {
        if let Some(mut changed) = world.try_resource_mut::<PendingStageChanges>() {
            changed.0.push(entity);
        }
    }
    Some(outcome)
}

/// The references whose destruction stage changed since the ObScript `OnDestructionStageChange` blocks
/// last ran: "run once whenever the scripted object changes its current Damage Stage" (GECK).
/// `NOT_SAVED_BY_DESIGN`: drained the frame it is filled.
#[derive(Debug, Default)]
pub struct PendingStageChanges(pub Vec<EntityId>);

impl Resource for PendingStageChanges {}

/// Whether `entity`'s base object carries destruction data.
pub fn is_destructible(world: &World, entity: EntityId) -> bool {
    let Some(base) = base_of(world, entity) else { return false };
    world.try_resource::<DestructionTable>().is_some_and(|table| table.get(base).is_some())
}

/// `reference.GetCurrentDestructionStage()`.
pub fn destruction_stage(world: &World, entity: EntityId, form: u32) -> i32 {
    let Some(base) = base_of(world, entity) else { return -1 };
    let Some(table) = world.try_resource::<DestructionTable>() else { return -1 };
    let Some(data) = table.get(base) else { return -1 };
    let damage = world
        .try_resource::<DestructionStates>()
        .and_then(|states| states.0.get(&form).map(|state| state.damage))
        .unwrap_or(0.0);
    current_stage(data, damage)
}

/// `reference.ClearDestruction()`: no damage, not destroyed, no event.
pub fn clear_destruction(world: &World, form: u32) {
    if let Some(mut states) = world.try_resource_mut::<DestructionStates>() {
        states.0.remove(&form);
    }
}

/// `reference.SetDestroyed(flag)`.
pub fn set_destroyed(world: &World, form: u32, destroyed: bool) {
    if let Some(mut states) = world.try_resource_mut::<DestructionStates>() {
        if destroyed {
            states.0.entry(form).or_default().destroyed = true;
        } else if let Some(state) = states.0.get_mut(&form) {
            state.destroyed = false;
            if state.damage == 0.0 {
                states.0.remove(&form);
            }
        }
    }
}

/// `reference.IsDestroyed()`.
pub fn is_destroyed(world: &World, form: u32) -> bool {
    world
        .try_resource::<DestructionStates>()
        .is_some_and(|states| states.0.get(&form).is_some_and(|state| state.destroyed))
}

#[cfg(test)]
mod tests;
