//! Portable, callback-local inventory and equipment summaries.

use thiserror::Error;

use crate::identity::FormRef;

/// Maximum distinct base forms exposed for one inventory in one callback.
pub const MAX_INVENTORY_ENTRIES_PER_ENTITY: usize = 1_024;
/// Maximum UTF-8 bytes exposed for one authored/fallback item name.
pub const MAX_ITEM_NAME_BYTES: usize = 1_024;

/// Stable semantic category shared across supported game families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemCategory {
    Misc,
    Junk,
    Mod,
    Book,
    Note,
    Ingredient,
    Aid,
    Key,
    Ammo,
    Armor,
    Weapon,
}

/// Validated presentation and economic metadata for one base item.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemMetadata {
    name: String,
    category: ItemCategory,
    value: u32,
    weight: f32,
}

impl ItemMetadata {
    pub fn new(
        name: String,
        category: ItemCategory,
        value: u32,
        weight: f32,
    ) -> Result<Self, InventoryError> {
        if name.len() > MAX_ITEM_NAME_BYTES {
            return Err(InventoryError::NameTooLarge {
                actual: name.len(),
                maximum: MAX_ITEM_NAME_BYTES,
            });
        }
        if !weight.is_finite() || weight < 0.0 {
            return Err(InventoryError::InvalidWeight);
        }
        Ok(Self {
            name,
            category,
            value,
            weight,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn category(&self) -> ItemCategory {
        self.category
    }

    pub const fn value(&self) -> u32 {
        self.value
    }

    pub const fn weight(&self) -> f32 {
        self.weight
    }
}

/// Aggregated inventory state for one portable base form.
#[derive(Clone, Debug, PartialEq)]
pub struct InventoryEntry {
    item: FormRef,
    count: u64,
    biped_slots: u32,
    weapon_equipped: bool,
    metadata: Option<ItemMetadata>,
}

impl InventoryEntry {
    pub fn new(
        item: FormRef,
        count: u64,
        biped_slots: u32,
        weapon_equipped: bool,
        metadata: Option<ItemMetadata>,
    ) -> Result<Self, InventoryError> {
        if item.local() == 0 {
            return Err(InventoryError::NullItem);
        }
        Ok(Self {
            item,
            count,
            biped_slots,
            weapon_equipped,
            metadata,
        })
    }

    pub const fn item(&self) -> FormRef {
        self.item
    }

    pub const fn count(&self) -> u64 {
        self.count
    }

    /// Engine-neutral authored biped-slot bit mask occupied by this item.
    pub const fn biped_slots(&self) -> u32 {
        self.biped_slots
    }

    pub const fn weapon_equipped(&self) -> bool {
        self.weapon_equipped
    }

    pub const fn is_equipped(&self) -> bool {
        self.biped_slots != 0 || self.weapon_equipped
    }

    pub const fn metadata(&self) -> Option<&ItemMetadata> {
        self.metadata.as_ref()
    }
}

/// Complete or explicitly truncated inventory projection.
#[derive(Clone, Debug, Default, PartialEq)]
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
    #[error("item name contains {actual} bytes, exceeding the limit of {maximum}")]
    NameTooLarge { actual: usize, maximum: usize },
    #[error("item weight must be finite and non-negative")]
    InvalidWeight,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_snapshots_are_portable_sorted_and_explicit_about_truncation() {
        let metadata = ItemMetadata::new("Armor".to_owned(), ItemCategory::Armor, 50, 2.5).unwrap();
        let first =
            InventoryEntry::new(FormRef::new([1; 16], 1), 3, 0b101, false, Some(metadata)).unwrap();
        let second = InventoryEntry::new(FormRef::new([2; 16], 1), 1, 0, true, None).unwrap();
        let snapshot = InventorySnapshot::new(vec![first.clone(), second.clone()], true).unwrap();
        assert_eq!(snapshot.entries(), &[first.clone(), second.clone()]);
        assert!(snapshot.truncated());
        assert!(first.is_equipped());
        assert!(second.is_equipped());
        assert_eq!(
            InventorySnapshot::new(vec![second, first], false),
            Err(InventoryError::EntriesNotStrictlySorted)
        );
        assert_eq!(snapshot.entries()[0].metadata().unwrap().weight(), 2.5);
        assert_eq!(
            ItemMetadata::new("Bad".to_owned(), ItemCategory::Misc, 0, f32::NAN),
            Err(InventoryError::InvalidWeight)
        );
    }
}
