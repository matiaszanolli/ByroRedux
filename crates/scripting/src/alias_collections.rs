//! M47.2 — the members of Fallout 4's reference-collection aliases
//! (`RefCollectionAlias`) that scripts fill: `AddRef`, `AddArray`,
//! `AddRefCollection`, `RemoveRef`, `RemoveAll`, read back with `GetCount`,
//! `GetAt` and `Find` (the FO4 CK's *RefCollectionAlias Script* pages).
//!
//! A collection a quest fills by conditions or a fill type has no fill
//! runtime yet (`docs/engine/m47-3-quest-alias-design.md`, "Reference
//! collections"), so the Papyrus VM refuses to read or change one of those
//! rather than answer from an empty list: only a collection the quest leaves
//! to its scripts is modelled here. Members are references by FormID, so they
//! outlive a cell unload. Saved.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;

/// The key of a collection: its quest and alias.
pub type CollectionKey = (u32, i16);

/// Which collection aliases the quests leave to their scripts, derived from
/// the installed QUST alias definitions (`true`: no fill type and no match
/// conditions; `false`: filled by the quest). Not saved. The Papyrus VM reads
/// a clone taken before it holds any quest-state lock, since the definitions
/// themselves live behind a lock the alias systems take in the other order.
#[derive(Debug, Default, Clone)]
pub struct CollectionAliasKinds(pub std::sync::Arc<HashMap<CollectionKey, bool>>);

impl Resource for CollectionAliasKinds {}

/// Every script-filled collection's members, in the order they were added.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct AliasCollections {
    #[cfg_attr(feature = "save", serde(with = "crate::serde_pairs"))]
    members: HashMap<(u32, i16), Vec<u32>>,
}

impl Resource for AliasCollections {}

impl AliasCollections {
    pub fn members(&self, key: CollectionKey) -> &[u32] {
        self.members.get(&key).map_or(&[], Vec::as_slice)
    }

    /// Replace a collection's members (an emptied collection is dropped).
    pub fn set(&mut self, key: CollectionKey, members: Vec<u32>) {
        if members.is_empty() {
            self.members.remove(&key);
        } else {
            self.members.insert(key, members);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn members_keep_their_order_and_an_emptied_collection_is_dropped() {
        let mut collections = AliasCollections::default();
        collections.set((1, 2), vec![30, 10, 20]);
        assert_eq!(collections.members((1, 2)), [30, 10, 20]);
        assert!(collections.members((1, 3)).is_empty());
        collections.set((1, 2), Vec::new());
        assert_eq!(collections, AliasCollections::default());
    }
}
