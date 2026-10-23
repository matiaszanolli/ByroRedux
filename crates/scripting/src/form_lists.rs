//! Form lists scripts can read (M47.2): `GetSize`, `GetAt`, `HasForm` and `Find`
//! on a `FLST` record.
//!
//! CK *FormList Script*: `GetSize` "the number of forms in the list", `GetAt`
//! "the form at index aiIndex ... none in case of error (such as wrong index
//! value)", `HasForm` "whether the passed-in form exists in this form list",
//! `Find` "the index ... or -1 if it's not a member". The lists are the
//! records' own, plus the forms scripts added (`AddForm`: "Adds the given form
//! to the form list"; `RemoveAddedForm`: "Only forms added to the list via calls
//! to AddForm can be removed"; `Revert`: "Removes all script added forms from the
//! list"), kept in the saved [`FormListAdditions`]. The CK does not say whether
//! adding a form already present adds it again or whether removing takes one
//! occurrence or all: an add always appends, and a removal takes every added
//! occurrence. A list's null entries (`0xFFFFFFFF`, the sentinel the parser
//! keeps for slot positions) are no forms and are dropped here. The records'
//! lists are derived, never saved.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

/// Every `FLST` record's forms, by FormID.
#[derive(Debug, Default, Clone)]
pub struct FormLists(Arc<HashMap<u32, Vec<u32>>>);

impl Resource for FormLists {}

impl FormLists {
    /// The forms of the list `list`, or `None` when no such list is loaded.
    pub fn get(&self, list: u32) -> Option<&[u32]> {
        self.0.get(&list).map(Vec::as_slice)
    }
}

/// One script edit of a form list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormListChange {
    Add { list: u32, form: u32 },
    RemoveAdded { list: u32, form: u32 },
    Revert { list: u32 },
}

/// The forms scripts added to each list, in order. Saved: a script's `AddForm`
/// must survive a reload.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct FormListAdditions {
    added: std::collections::BTreeMap<u32, Vec<u32>>,
}

impl Resource for FormListAdditions {}

impl FormListAdditions {
    pub fn apply(&mut self, change: FormListChange) {
        match change {
            FormListChange::Add { list, form } => self.added.entry(list).or_default().push(form),
            FormListChange::RemoveAdded { list, form } => {
                if let Some(forms) = self.added.get_mut(&list) {
                    forms.retain(|&added| added != form);
                    if forms.is_empty() {
                        self.added.remove(&list);
                    }
                }
            }
            FormListChange::Revert { list } => {
                self.added.remove(&list);
            }
        }
    }

    /// The forms scripts added to `list`.
    pub fn of(&self, list: u32) -> &[u32] {
        self.added.get(&list).map_or(&[], Vec::as_slice)
    }
}

/// `list`'s forms as scripts see them: the record's, then the added ones, with
/// the `pending` edits of a run still in flight applied on top.
pub fn forms_of(world: &World, list: u32, pending: &[FormListChange]) -> Option<Vec<u32>> {
    let mut forms = world.try_resource::<FormLists>()?.get(list)?.to_vec();
    let mut additions = world
        .try_resource::<FormListAdditions>()
        .map(|additions| additions.clone())
        .unwrap_or_default();
    for &change in pending {
        additions.apply(change);
    }
    forms.extend_from_slice(additions.of(list));
    Some(forms)
}

/// Install the load order's form lists; returns how many.
pub fn install_form_lists(world: &mut World, index: &EsmIndex) -> usize {
    let lists: HashMap<u32, Vec<u32>> = index
        .form_lists
        .iter()
        .map(|(&form_id, list)| {
            let forms = list.entries.iter().copied().filter(|&form| form != 0 && form != 0xFFFF_FFFF).collect();
            (form_id, forms)
        })
        .collect();
    let count = lists.len();
    world.insert_resource(FormLists(Arc::new(lists)));
    count
}

/// #5802 — this subsystem's world setup, owned here instead of the crate's
/// `register` hub: the form lists scripts read (derived from the load order) and the forms scripts added to them (saved).
pub fn install(world: &mut World) {
    world.init_resource::<FormLists>();
    world.init_resource::<FormListAdditions>();
}

/// #5802 — this subsystem's saved column. Called by the binary's
/// `build_save_registry` at its place in the row order (the schema
/// fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<FormListAdditions>("FormListAdditions");
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::FlstRecord;

    #[test]
    fn script_edits_add_remove_and_revert() {
        let mut additions = FormListAdditions::default();
        additions.apply(FormListChange::Add { list: 1, form: 0xA });
        additions.apply(FormListChange::Add { list: 1, form: 0xB });
        additions.apply(FormListChange::Add { list: 1, form: 0xA });
        assert_eq!(additions.of(1), &[0xA, 0xB, 0xA]);
        additions.apply(FormListChange::RemoveAdded { list: 1, form: 0xA });
        assert_eq!(additions.of(1), &[0xB]);
        additions.apply(FormListChange::Revert { list: 1 });
        assert!(additions.of(1).is_empty());
    }

    #[test]
    fn null_entries_are_not_forms() {
        let mut index = EsmIndex::default();
        index.form_lists.insert(
            0x10,
            FlstRecord {
                form_id: 0x10,
                entries: vec![0xA, 0xFFFF_FFFF, 0xB],
                ..Default::default()
            },
        );
        let mut world = World::new();
        assert_eq!(install_form_lists(&mut world, &index), 1);
        let lists = world.resource::<FormLists>();
        assert_eq!(lists.get(0x10), Some(&[0xA, 0xB][..]));
        assert_eq!(lists.get(0x11), None);
    }
}
