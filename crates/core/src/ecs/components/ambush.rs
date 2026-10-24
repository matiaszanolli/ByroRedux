//! Ambush AI-behavior marker (M42, FO3/FNV procedure 9).
//!
//! GECK *Ambush Package*: "wait to attack enemies until they come within a
//! specified location". The actor waits at the `PLDT` wait location — that
//! half runs on the Guard runtime, installed beside this marker — and
//! ambient hostility (`faction_hostility_system`) starts the fight, but only
//! against an enemy inside the `PLD2` ambush location.
//!
//! This component carries that gate. Like the other package behaviors it is
//! rebuilt from the winning package at spawn and has no runtime state.

use super::eat_sleep::EatSleepLocation;
use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Marks an actor that runs the Ambush procedure.
///
/// `zone` is the ambush location's anchor (the `PLD2` type, resolved the
/// way an Eat/Sleep anchor is) and `zone_radius` its radius; only a
/// positive radius gates anything. `target_form_id` is the optional `PTDT`
/// target: when set, only that actor springs the ambush.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AmbushBehavior {
    pub zone: EatSleepLocation,
    pub zone_radius: f32,
    pub target_form_id: Option<u32>,
}

impl Component for AmbushBehavior {
    type Storage = SparseSetStorage<Self>;
}
