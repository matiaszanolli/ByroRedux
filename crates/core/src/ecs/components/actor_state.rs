//! Sparse actor lifecycle markers.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Marks an actor as dead.
///
/// Absence means alive. Keeping death as a sparse marker makes it cheap for
/// the common live-actor case and gives combat, scripts, resurrection, and
/// condition evaluation one shared source of truth.
///
/// Registered in the save schema because combat inserts it at the live
/// Health→death transition. A dead actor must not revive on reload.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Dead;

impl Component for Dead {
    type Storage = SparseSetStorage<Self>;
}

/// M65 (SNK-STN-002) — the player's sneak stance: present while the player is
/// crouched, absent otherwise ("Returns 1 if the actor is sneaking (crouched)",
/// GECK *IsSneaking*). Only the player carries it: actors other than the player
/// do not sneak (SNK-NG-001).
///
/// Written only through the scripting crate's `stance::set_sneaking`, the one
/// path the key, `stealth.sneak` and Papyrus `StartSneaking` share. Saved
/// (SNK-SAVE-001, Q4): the replacing registration makes a saved absence stand
/// the live player up on load.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Sneaking;

impl Component for Sneaking {
    type Storage = SparseSetStorage<Self>;
}
