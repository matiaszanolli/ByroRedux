//! Authored ownership for a placed reference.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// REFR placement carries an `XOWN` owner — this reference (typically a
/// container) belongs to the named NPC_ or FACT FormID, and taking from it
/// without belonging is theft.
///
/// Captured from `PlacedRef.ownership` (`CellOwnership`) at spawn time.
/// Deliberately a component of its own rather than read ad hoc from the
/// plugin index: ownership must survive on the live entity after the record
/// data has been flattened into ECS state, the same reason [`Locked`] is a
/// component (#3159).
///
/// P3's consumer is the minimal theft rule in the binary's loot path: a
/// transfer from an owned source is classified stolen when the owner is
/// neither the player reference (0x14) nor a faction the player holds rank
/// in. There is deliberately **no** witness, bounty, or crime-response
/// system yet — the classification is recorded (notification + `stolen` on
/// `ItemTransfer` rows), not punished.
///
/// Sparse storage — owned REFRs are a minority of placements.
///
/// [`Locked`]: super::Locked
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owned {
    /// Owning NPC_ or FACT FormID (`XOWN`).
    pub owner_form_id: u32,
    /// Minimum faction rank required to count as the owner, when the owner
    /// is a FACT (`XRNK`). `None` means any rank qualifies.
    pub faction_rank: Option<i32>,
    /// Global variable that gates ownership at runtime (`XGLB`), if authored.
    /// Not consumed by any policy yet; carried so the eventual check does not
    /// need a second data-plumbing pass.
    pub global_var_form_id: Option<u32>,
}

impl Component for Owned {
    type Storage = SparseSetStorage<Self>;
}
