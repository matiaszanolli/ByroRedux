//! Portable, callback-local inventory and equipment summaries.

use thiserror::Error;

use crate::identity::FormRef;

/// Maximum distinct base forms exposed for one inventory in one callback.
pub const MAX_INVENTORY_ENTRIES_PER_ENTITY: usize = 1_024;

/// Aggregated inventory state for one portable base form.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InventoryEntry {
    item: FormRef,
    count: u64,
    biped_slots: u32,
    weapon_equipped: bool,
}

impl InventoryEntry {
    pub fn new(
        item: FormRef,
        count: u64,
        biped_slots: u32,
        weapon_equipped: bool,
    ) -> Result<Self, InventoryError> {
        if item.local() == 0 {
            return Err(InventoryError::NullItem);
        }
        Ok(Self {
            item,
            count,
            biped_slots,
            weapon_equipped,
        })
    }

    pub const fn item(self) -> FormRef {
        self.item
    }

    pub const fn count(self) -> u64 {
        self.count
    }

    /// Engine-neutral authored biped-slot bit mask occupied by this item.
    pub const fn biped_slots(self) -> u32 {
        self.biped_slots
    }

    pub const fn weapon_equipped(self) -> bool {
        self.weapon_equipped
    }

    pub const fn is_equipped(self) -> bool {
        self.biped_slots != 0 || self.weapon_equipped
    }
}

/// Complete or explicitly truncated inventory projection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InventorySnapshot {
    entries: Vec<InventoryEntry>,
    truncated: bool,
}

impl InventorySnapshot {
    pub fn new(entries: Vec<InventoryEntry>, truncated: bool) -> Result<Self, InventoryError> {
        if entries.len() > MAX_INVENTORY_ENTRIES_PER_ENTITY {
            return Err(InventoryError::EntryBudgetExceeded {
                maximum: MAX_INVENTORY_ENTRIES_PER_ENTITY,
            });
        }
        if entries
            .windows(2)
            .any(|pair| pair[0].item() >= pair[1].item())
        {
            return Err(InventoryError::EntriesNotStrictlySorted);
        }
        Ok(Self { entries, truncated })
    }

    pub fn entries(&self) -> &[InventoryEntry] {
        &self.entries
    }

    /// True when the host omitted unresolved or over-budget base forms.
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum InventoryError {
    #[error("inventory item form identity reserves local zero")]
    NullItem,
    #[error("inventory snapshot exceeds the per-entity limit of {maximum}")]
    EntryBudgetExceeded { maximum: usize },
    #[error("inventory entries must be unique and strictly sorted by portable form identity")]
    EntriesNotStrictlySorted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_snapshots_are_portable_sorted_and_explicit_about_truncation() {
        let first = InventoryEntry::new(FormRef::new([1; 16], 1), 3, 0b101, false).unwrap();
        let second = InventoryEntry::new(FormRef::new([2; 16], 1), 1, 0, true).unwrap();
        let snapshot = InventorySnapshot::new(vec![first, second], true).unwrap();
        assert_eq!(snapshot.entries(), &[first, second]);
        assert!(snapshot.truncated());
        assert!(first.is_equipped());
        assert!(second.is_equipped());
        assert_eq!(
            InventorySnapshot::new(vec![second, first], false),
            Err(InventoryError::EntriesNotStrictlySorted)
        );
    }
}
