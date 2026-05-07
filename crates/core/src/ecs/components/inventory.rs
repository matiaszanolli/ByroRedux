//! Inventory + equipment ECS components.
//!
//! Architectural model (settled 2026-05-07): base records live in
//! [`DataStore`](../../../../../../crates/plugin/src/datastore.rs) as a
//! Resource keyed by `FormId`. Per-instance state, where it diverges
//! from the base record, lives in
//! [`ItemInstancePool`](crate::ecs::resources::ItemInstancePool). The
//! `Inventory` Component on each container / actor / world ref carries
//! the actual stack list.
//!
//! The vast majority of inventory rows are stack-count-only (e.g. "100
//! stimpaks") and need no per-instance state — `ItemStack.instance` is
//! `None`. Only stacks that pick up modlists, condition deltas, or
//! charge state allocate from the instance pool.
//!
//! This is the runtime layer; the ESM-side parsed records are over in
//! [`crates/plugin/src/esm/records/container.rs`] and `actor.rs`.
//!
//! See [issue #896](https://github.com/matiaszanolli/ByroRedux/issues/896)
//! for the M41 equip slice that lights up these components.
//!
//! ## Why Component-on-container, not entity-per-item
//!
//! Bethesda's runtime spawns an entity per carried item, which leaks
//! into save format (one change-form per modification, accumulating
//! across the save's lifetime). ECS-as-truth + Component-on-container
//! caps the data shape at "live entities × Vec of stacks per
//! container" — bounded by current cell load, not playthrough length.
//!
//! Pickup + drop nets to zero on disk because the instance-pool
//! free-list reclaims released slots.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::form_id::FormId;
use std::num::NonZeroU32;

/// Index into the per-world [`ItemInstancePool`](crate::ecs::resources::ItemInstancePool).
///
/// `NonZeroU32` so `Option<ItemInstanceId>` is one `u32` wide. The
/// pool's slot 0 is reserved as a sentinel; legal IDs start at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemInstanceId(pub NonZeroU32);

/// One row in an [`Inventory`].
///
/// The common case (`instance == None`) is a pure stack of identical
/// items — e.g. 100 stimpaks. Stacks diverging from base (named items,
/// modded weapons, partial-condition armor) point at an
/// [`ItemInstance`](crate::ecs::resources::ItemInstance) in the
/// per-world pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemStack {
    /// Base record this stack is an instance of (ARMO / WEAP / MISC / etc.).
    pub base_form_id: FormId,
    /// Stack count. Always non-negative at runtime — the parsed CNTO's
    /// `i32` is normalised to `u32` at the spawn boundary (negative
    /// counts on disk are remove-from-inventory deltas, never live
    /// state).
    pub count: u32,
    /// Pool index for stacks with per-instance state. `None` for the
    /// stack-only common case.
    pub instance: Option<ItemInstanceId>,
}

impl ItemStack {
    /// Build a stack of `count` items of `base_form_id` with no per-
    /// instance state. Use this for any item that doesn't yet need an
    /// instance pool slot — which is the overwhelming majority.
    pub const fn new(base_form_id: FormId, count: u32) -> Self {
        Self {
            base_form_id,
            count,
            instance: None,
        }
    }
}

/// What an actor / container / world ref currently holds.
///
/// `items` is a flat `Vec` rather than a map keyed by FormId so that
/// duplicate entries with different per-instance state are
/// representable (two distinct instances of the same base armor with
/// different condition values).
#[derive(Debug, Default, Clone)]
pub struct Inventory {
    pub items: Vec<ItemStack>,
}

impl Inventory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a stack. Returns the new stack's index (the value that
    /// goes into [`EquipmentSlots`]).
    pub fn push(&mut self, stack: ItemStack) -> InventoryIndex {
        let idx = self.items.len() as u32;
        self.items.push(stack);
        InventoryIndex(idx)
    }

    pub fn get(&self, idx: InventoryIndex) -> Option<&ItemStack> {
        self.items.get(idx.0 as usize)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl Component for Inventory {
    type Storage = SparseSetStorage<Self>;
}

/// Index into an [`Inventory::items`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InventoryIndex(pub u32);

/// The number of biped-slot bits an actor's equipment can occupy.
///
/// FO3 / FNV `BMDT.biped_flags` is the low 16 bits; Skyrim+ `BOD2` is
/// the full 32. Either fits in this array. The slot's *meaning* (head
/// vs torso vs eyes etc.) is game-specific and lives near the parser
/// (`crates/plugin/src/esm/records/items.rs`), not here — this layer
/// just tracks "which inventory entry occupies bit N."
pub const MAX_BIPED_SLOTS: usize = 32;

/// Per-actor map: biped-slot bit → which `Inventory` entry currently
/// fills it.
///
/// Multiple bits can point at the same inventory entry (one armor
/// piece covering UpperBody + Arms). Equipping a new item that
/// overlaps an existing item's slot mask supersedes the old occupant
/// in the overlapping bits — visualisation responsibility, not save-
/// state churn.
#[derive(Debug, Clone)]
pub struct EquipmentSlots {
    pub occupants: [Option<InventoryIndex>; MAX_BIPED_SLOTS],
}

impl Default for EquipmentSlots {
    fn default() -> Self {
        Self {
            occupants: [None; MAX_BIPED_SLOTS],
        }
    }
}

impl EquipmentSlots {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark every set bit in `slot_mask` as occupied by `idx`. Returns
    /// the set of inventory indices that were displaced (an empty
    /// `Vec` if the slots were free). Callers can use the return
    /// value to drive "remove the displaced item from the visible
    /// mesh set" follow-up logic in spawn paths.
    pub fn equip(&mut self, slot_mask: u32, idx: InventoryIndex) -> Vec<InventoryIndex> {
        let mut displaced = Vec::new();
        for bit in 0..MAX_BIPED_SLOTS {
            if slot_mask & (1u32 << bit) == 0 {
                continue;
            }
            if let Some(prev) = self.occupants[bit].replace(idx) {
                if prev != idx && !displaced.contains(&prev) {
                    displaced.push(prev);
                }
            }
        }
        displaced
    }

    /// Clear every set bit in `slot_mask`. Returns the (deduplicated)
    /// set of inventory indices that were occupying those bits.
    pub fn unequip(&mut self, slot_mask: u32) -> Vec<InventoryIndex> {
        let mut released = Vec::new();
        for bit in 0..MAX_BIPED_SLOTS {
            if slot_mask & (1u32 << bit) == 0 {
                continue;
            }
            if let Some(prev) = self.occupants[bit].take() {
                if !released.contains(&prev) {
                    released.push(prev);
                }
            }
        }
        released
    }

    /// What occupies the given biped bit, if anything. Returns `None`
    /// for bits beyond [`MAX_BIPED_SLOTS`] rather than panicking.
    pub fn at(&self, bit: u8) -> Option<InventoryIndex> {
        self.occupants.get(usize::from(bit)).copied().flatten()
    }
}

impl Component for EquipmentSlots {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    fn fid(local: u32) -> FormId {
        // Helper: intern a FormId for tests via a fresh pool. Tests
        // only need stable identity within a single assertion.
        let mut pool = FormIdPool::default();
        pool.intern(FormIdPair {
            plugin: PluginId(0),
            local: LocalFormId(local),
        })
    }

    #[test]
    fn item_stack_defaults_to_no_instance() {
        let s = ItemStack::new(fid(1), 5);
        assert_eq!(s.count, 5);
        assert!(s.instance.is_none());
    }

    #[test]
    fn inventory_push_returns_sequential_indices() {
        let mut inv = Inventory::new();
        let a = inv.push(ItemStack::new(fid(1), 1));
        let b = inv.push(ItemStack::new(fid(2), 2));
        assert_eq!(a, InventoryIndex(0));
        assert_eq!(b, InventoryIndex(1));
        assert_eq!(inv.get(a).unwrap().count, 1);
        assert_eq!(inv.get(b).unwrap().count, 2);
    }

    #[test]
    fn equipment_slots_default_is_all_unoccupied() {
        let slots = EquipmentSlots::new();
        for o in slots.occupants.iter() {
            assert!(o.is_none());
        }
    }

    #[test]
    fn equip_marks_every_set_bit_in_mask() {
        let mut slots = EquipmentSlots::new();
        // Bits 2 + 3 + 5 set.
        let mask = 0b00101100u32;
        let displaced = slots.equip(mask, InventoryIndex(7));
        assert!(displaced.is_empty());
        assert_eq!(slots.occupants[2], Some(InventoryIndex(7)));
        assert_eq!(slots.occupants[3], Some(InventoryIndex(7)));
        assert_eq!(slots.occupants[5], Some(InventoryIndex(7)));
        assert!(slots.occupants[4].is_none());
    }

    #[test]
    fn equip_returns_deduplicated_displaced_indices() {
        let mut slots = EquipmentSlots::new();
        // Existing armor covers bits 2 + 3.
        slots.equip(0b00001100, InventoryIndex(1));
        // New armor covers bits 3 + 4 — overlaps on bit 3.
        let displaced = slots.equip(0b00011000, InventoryIndex(2));
        assert_eq!(displaced, vec![InventoryIndex(1)]);
        assert_eq!(slots.occupants[2], Some(InventoryIndex(1)));
        assert_eq!(slots.occupants[3], Some(InventoryIndex(2)));
        assert_eq!(slots.occupants[4], Some(InventoryIndex(2)));
    }

    #[test]
    fn unequip_clears_and_returns_released() {
        let mut slots = EquipmentSlots::new();
        slots.equip(0b00001100, InventoryIndex(1));
        slots.equip(0b00010000, InventoryIndex(2));
        let released = slots.unequip(0b00011100);
        assert!(released.contains(&InventoryIndex(1)));
        assert!(released.contains(&InventoryIndex(2)));
        assert_eq!(released.len(), 2);
        for o in slots.occupants.iter() {
            assert!(o.is_none());
        }
    }
}
