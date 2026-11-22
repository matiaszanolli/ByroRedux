//! Who wrote each record of a load-order fold (PRD legacy-mod-compatibility LMC-REC-005, -007).
//!
//! The fold reads the plugins in load order and keeps, per form, the last plugin's record whole (LMC-REC-002, the
//! `legacy` policy, the only one built). [`FoldProvenance`] is the record of that fold: for every form more than one
//! plugin carried, every carrier in order, the last being the winner and the rest the losers; for every form a plugin
//! defined in an ID space it does not own (an *injected* record), the plugin that defined it; and which carrier, if
//! any, deleted it. It is built beside the fold, before each plugin's [`EsmIndex::merge_from`], from the record
//! headers the walk traced (`record_types`, `deleted_record_metadata`), so it costs a lookup per record and stores
//! only the forms that are overridden, injected or deleted.

use std::collections::HashMap;

use super::EsmIndex;

/// One plugin's record for a form, in fold order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Carrier {
    /// The plugin's load-order position.
    pub plugin: u32,
    /// Whether its record is a deletion (`DELE`, LMC-REC-004).
    pub deletes: bool,
}

/// See the module docs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FoldProvenance {
    /// Forms more than one plugin carried: every carrier in fold order, the definer first.
    overridden: HashMap<u32, Vec<Carrier>>,
    /// Forms first carried by a plugin that does not own their ID space, with that plugin's position (LMC-REC-005).
    injected: HashMap<u32, u32>,
}

impl FoldProvenance {
    /// Note `plugin`, the plugin at load-order `position`, before it is folded into `merged`. `owner_of` names the
    /// position of the plugin that owns a form's ID space (its slot), `None` for a slot no plugin holds.
    pub fn note_plugin(&mut self, position: usize, plugin: &EsmIndex, merged: &EsmIndex, owner_of: impl Fn(u32) -> Option<usize>) {
        let position = position as u32;
        let carried = plugin.record_types.keys().map(|&id| (id, false));
        let deleted = plugin.deleted_record_metadata.iter().map(|&id| (id, true));
        for (form_id, deletes) in carried.chain(deleted) {
            let carrier = Carrier { plugin: position, deletes };
            if let Some(carriers) = self.overridden.get_mut(&form_id) {
                carriers.push(carrier);
                continue;
            }
            let owner = owner_of(form_id).map(|owner| owner as u32);
            if merged.record_types.contains_key(&form_id) || merged.deleted_record_metadata.contains(&form_id) {
                // An earlier plugin carried it: the definer is the injecting plugin, else the slot owner.
                let definer = self.injected.get(&form_id).copied().or(owner);
                let mut carriers = Vec::with_capacity(2);
                if let Some(definer) = definer.filter(|&definer| definer != position) {
                    carriers.push(Carrier { plugin: definer, deletes: false });
                }
                carriers.push(carrier);
                self.overridden.insert(form_id, carriers);
            } else if owner != Some(position) && !deletes {
                self.injected.insert(form_id, position);
            }
        }
    }

    /// Record that the plugin at `position` wrote `form_id`, as [`Self::note_plugin`] would when folding it: the first
    /// call names the definer, each later call an override. For fixtures that fold no real plugins.
    pub fn note_writer(&mut self, form_id: u32, position: usize) {
        let carrier = Carrier { plugin: position as u32, deletes: false };
        match self.overridden.get_mut(&form_id) {
            Some(carriers) => carriers.push(carrier),
            None => match self.injected.remove(&form_id) {
                Some(definer) => {
                    self.overridden.insert(form_id, vec![Carrier { plugin: definer, deletes: false }, carrier]);
                }
                None => {
                    self.injected.insert(form_id, carrier.plugin);
                }
            },
        }
    }

    /// Every plugin that carried `form_id`, in fold order, when more than one did; empty when one plugin or none did.
    pub fn carriers(&self, form_id: u32) -> &[Carrier] {
        self.overridden.get(&form_id).map_or(&[], Vec::as_slice)
    }

    /// The position of the plugin whose record for `form_id` won the fold, when the provenance knows better than the
    /// slot owner: the last carrier of an overridden form, or the plugin that injected it. `None` means the slot owner
    /// carried it alone (or the form is unknown), and the caller names the slot owner.
    pub fn winner(&self, form_id: u32) -> Option<usize> {
        self.overridden
            .get(&form_id)
            .and_then(|carriers| carriers.last())
            .map(|carrier| carrier.plugin as usize)
            .or_else(|| self.injected.get(&form_id).map(|&plugin| plugin as usize))
    }

    /// The overridden forms, in no particular order, with their carriers.
    pub fn overridden(&self) -> impl Iterator<Item = (u32, &[Carrier])> {
        self.overridden.iter().map(|(&form_id, carriers)| (form_id, carriers.as_slice()))
    }

    /// The injected forms that no later plugin overrode, with the injecting plugin.
    pub fn injected(&self) -> impl Iterator<Item = (u32, usize)> + '_ {
        self.injected.iter().map(|(&form_id, &plugin)| (form_id, plugin as usize))
    }

    /// How many forms were overridden and how many injected.
    pub fn counts(&self) -> (usize, usize) {
        (self.overridden.len(), self.injected.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An index whose walk traced `carried` and deleted `deleted`.
    fn plugin(carried: &[u32], deleted: &[u32]) -> EsmIndex {
        let mut index = EsmIndex::default();
        for &id in carried {
            index.record_types.insert(id, *b"MISC");
        }
        index.deleted_record_metadata.extend(deleted.iter().copied());
        index
    }

    /// Fold `plugins` in order, noting each before its merge, with each plugin owning the slot of its position.
    fn fold(plugins: Vec<EsmIndex>) -> (EsmIndex, FoldProvenance) {
        let mut merged = EsmIndex::default();
        let mut provenance = FoldProvenance::default();
        let count = plugins.len();
        for (position, plugin) in plugins.into_iter().enumerate() {
            provenance.note_plugin(position, &plugin, &merged, |id| Some((id >> 24) as usize).filter(|&p| p < count));
            merged.merge_from(plugin);
        }
        (merged, provenance)
    }

    const fn carrier(plugin: u32) -> Carrier {
        Carrier { plugin, deletes: false }
    }

    /// LMC-REC-007 — a form carried by the master and two later plugins lists all three in fold order, and the last
    /// wins; a form only its master carries is not stored at all.
    #[test]
    fn an_overridden_form_lists_every_carrier_in_fold_order() {
        let (_, provenance) = fold(vec![
            plugin(&[0x0000_0001, 0x0000_0002], &[]),
            plugin(&[0x0000_0001, 0x0100_0010], &[]),
            plugin(&[0x0000_0001], &[]),
        ]);
        assert_eq!(provenance.carriers(0x0000_0001), [carrier(0), carrier(1), carrier(2)]);
        assert_eq!(provenance.winner(0x0000_0001), Some(2));
        assert!(provenance.carriers(0x0000_0002).is_empty(), "a form its master alone carries costs nothing");
        assert_eq!(provenance.winner(0x0000_0002), None, "and the slot owner names its winner");
        assert_eq!(provenance.winner(0x0100_0010), None, "a plugin's own new form is its own");
        assert_eq!(provenance.counts(), (1, 0));
    }

    /// LMC-REC-005 — a plugin that defines a form in its master's ID space injects it; a later override lists the
    /// injector, not the slot owner, as the definer.
    #[test]
    fn an_injected_form_names_its_injector_and_a_later_override_follows_it() {
        let (_, provenance) = fold(vec![plugin(&[0x0000_0001], &[]), plugin(&[0x0000_0F00], &[]), plugin(&[], &[])]);
        assert_eq!(provenance.winner(0x0000_0F00), Some(1));
        assert_eq!(provenance.injected().collect::<Vec<_>>(), [(0x0000_0F00, 1)]);

        let (_, provenance) =
            fold(vec![plugin(&[0x0000_0001], &[]), plugin(&[0x0000_0F00], &[]), plugin(&[0x0000_0F00], &[])]);
        assert_eq!(provenance.carriers(0x0000_0F00), [carrier(1), carrier(2)], "the injector is the definer");
        assert_eq!(provenance.winner(0x0000_0F00), Some(2));
    }

    /// LMC-REC-004 — a deletion is a carrier like any other, marked as one, and it is the last word on the form.
    #[test]
    fn a_deletion_is_a_carrier() {
        let (merged, provenance) = fold(vec![plugin(&[0x0000_0001], &[]), plugin(&[], &[0x0000_0001])]);
        assert!(!merged.record_types.contains_key(&0x0000_0001), "the fold dropped it");
        assert_eq!(provenance.carriers(0x0000_0001), [carrier(0), Carrier { plugin: 1, deletes: true }]);
        assert_eq!(provenance.winner(0x0000_0001), Some(1));
    }

    /// The fixture path agrees with the fold: the first writer defines, each later writer overrides.
    #[test]
    fn note_writer_builds_the_same_record_as_a_fold() {
        let mut provenance = FoldProvenance::default();
        provenance.note_writer(0x0000_0001, 0);
        assert_eq!(provenance.winner(0x0000_0001), Some(0));
        provenance.note_writer(0x0000_0001, 1);
        assert_eq!(provenance.carriers(0x0000_0001), [carrier(0), carrier(1)]);
        assert_eq!(provenance.winner(0x0000_0001), Some(1));
    }
}
