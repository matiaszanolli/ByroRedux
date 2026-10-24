//! FleeNotCombat AI-behavior marker (M42, FO3/FNV procedure 10).
//!
//! An actor whose active PACK is a **FleeNotCombat** package gets a
//! [`FleePackageBehavior`] at spawn. The GECK *Flee not Combat Package*
//! page names two variants — "fleeing from something, or fleeing to
//! somewhere" — and the vanilla masters author them through the same
//! sub-records every locomotion package reads: `PTDT` is the thing to flee
//! from (its `count_or_distance` is the GECK's *Minimum distance from
//! target*: 256–2500 on every FNV/FO3 flee-from package) and `PLDT` is the
//! place to flee to.
//!
//! No runtime state: the flight is a pure function of where the actor and
//! the threat stand this tick, so nothing needs saving and the component is
//! rebuilt from the winning package at spawn like [`super::guard::GuardBehavior`].

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Marks an actor that runs the FleeNotCombat procedure.
///
/// `threat_form_id` is the `PTDT` reference to flee from (`0x14` is the
/// player). `min_distance` is that `PTDT`'s authored distance, `None` when it
/// is absent or zero. `refuge_form_id` is the `PLDT` `NearReference` to flee
/// to; only used when there is no threat.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct FleePackageBehavior {
    pub threat_form_id: Option<u32>,
    pub min_distance: Option<f32>,
    pub refuge_form_id: Option<u32>,
}

impl Component for FleePackageBehavior {
    type Storage = SparseSetStorage<Self>;
}
