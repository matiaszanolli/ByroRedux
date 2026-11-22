//! List-valued records merged across independent overrides (PRD legacy-mod-compatibility LMC-REC-003, decision Q2).
//!
//! Under the `legacy` policy (LMC-REC-002) the fold keeps the last plugin's record of a form whole, so two mods that
//! each add a sword to the same levelled list leave only the later one's sword: the reason list patches (Bashed,
//! Smashed, Synthesis) exist. The `merge` policy, the default by decision Q2, combines them instead, for the records
//! that are a list and a few scalar fields: the levelled lists (`LVLI`, `LVLN`, `LVLC`, `LVSP`), form lists (`FLST`),
//! outfits (`OTFT`) and containers (`CONT`):
//!
//! - **Scope.** Only *independent* overrides merge: a plugin that declares, directly or through its masters, every
//!   plugin whose changes the record holds so far (a patch) wins it whole, as under `legacy`.
//! - **Per form.** An override is diffed against the record it overrides, its *base*: the latest earlier carrier among
//!   its own masters (the definer at the least). The list's entries are grouped by the form they name and compared as
//!   multisets of the rest of each entry (a levelled entry's level and count, an inventory entry's count), since lists
//!   repeat a form on purpose: equal is kept; only in the base is a *removal*; only in the override an *addition*; in
//!   both but different a *change*, which replaces the base's entries for that form with the override's. A re-level is
//!   one change, never a removal plus an addition, so two plugins moving one sword to different levels cannot leave it
//!   in the list twice.
//! - **Last wins per form.** Operations on different forms combine; on the same form the later plugin in load order
//!   wins and the earlier operation is recorded as overruled. Operations that would leave the list as it is are not
//!   operations.
//! - **Fields.** The record's other fields (a levelled list's chance none, a container's name and weight, ...) follow
//!   the same rule: a field the override changed against its base is an operation, last wins.
//! - **Explained.** [`ListMergeRecord`] keeps, per merged record, every contributor, the winner and overruled operations
//!   of each form and field, and the last wholesale win.
//!
//! The merged list keeps the order of the entries it started from: a changed form takes the place of its first entry,
//! an added form goes at the end, so the indices of a form list's untouched entries stay put. A form list's null
//! entries (`0xFFFFFFFF`, a slot the author left empty) hold a place, not a form: they are never an operation and keep
//! their positions. Per-entry extra data (`COED` owner and condition) is not decoded, so it takes no part in the
//! comparison. The lists inside larger records (an actor's inventory, factions and spells, keyword lists) still follow
//! `legacy`.

use std::collections::HashMap;

use super::container::{ContainerRecord, InventoryEntry, LeveledEntry, LeveledList};
use super::list_record::FlstRecord;
use super::outfit::OtftRecord;
use super::provenance::FoldProvenance;
use super::EsmIndex;

/// How the fold resolves a form more than one plugin carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MergePolicy {
    /// List records merge across independent overrides (LMC-REC-003); every other record is `legacy`'s.
    #[default]
    Merge,
    /// The last plugin's record wins whole, every record (LMC-REC-002).
    Legacy,
}

impl MergePolicy {
    /// `merge` or `legacy`, in any case.
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "merge" => Some(Self::Merge),
            "legacy" => Some(Self::Legacy),
            _ => None,
        }
    }

    /// The policy's name, as [`Self::parse`] reads it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Legacy => "legacy",
        }
    }
}

/// The record types that merge, each with its own map in [`EsmIndex`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListKind {
    LeveledItems,
    LeveledNpcs,
    LeveledCreatures,
    LeveledSpells,
    FormList,
    Outfit,
    Container,
}

impl ListKind {
    pub const ALL: [Self; 7] = [
        Self::LeveledItems,
        Self::LeveledNpcs,
        Self::LeveledCreatures,
        Self::LeveledSpells,
        Self::FormList,
        Self::Outfit,
        Self::Container,
    ];

    /// The record type.
    pub fn tag(self) -> &'static str {
        match self {
            Self::LeveledItems => "LVLI",
            Self::LeveledNpcs => "LVLN",
            Self::LeveledCreatures => "LVLC",
            Self::LeveledSpells => "LVSP",
            Self::FormList => "FLST",
            Self::Outfit => "OTFT",
            Self::Container => "CONT",
        }
    }
}

/// One entry of a list that merges: the form it names, and the rest of it, which the multiset comparison sees.
pub trait ListEntry: Copy {
    type Data: Ord + Copy;
    fn form(&self) -> u32;
    fn data(&self) -> Self::Data;
}

impl ListEntry for LeveledEntry {
    type Data = (u16, u16);
    fn form(&self) -> u32 {
        self.form_id
    }
    fn data(&self) -> (u16, u16) {
        (self.level, self.count)
    }
}

impl ListEntry for InventoryEntry {
    type Data = i32;
    fn form(&self) -> u32 {
        self.item_form_id
    }
    fn data(&self) -> i32 {
        self.count
    }
}

/// A bare form ID: a form list's or an outfit's entry.
impl ListEntry for u32 {
    type Data = ();
    fn form(&self) -> u32 {
        *self
    }
    fn data(&self) {}
}

/// A form list's empty slot: it holds a place, so it is never one of the merge's operations.
const NULL_FORM: u32 = 0xFFFF_FFFF;

/// A scalar field of a record that merges.
pub struct Field<R> {
    /// The field's name in a report.
    pub name: &'static str,
    same: fn(&R, &R) -> bool,
    copy: fn(&R, &mut R),
}

/// A record type that merges: one list and its scalar fields (see the module docs), implemented by `list_record!`.
pub trait ListRecord: Clone + 'static {
    type Entry: ListEntry;
    /// The kinds whose map holds this type.
    const KINDS: &'static [ListKind];
    /// Every field besides the form ID and the list.
    const FIELDS: &'static [Field<Self>];
    fn entries(&self) -> &[Self::Entry];
    fn entries_mut(&mut self) -> &mut Vec<Self::Entry>;
    fn records(kind: ListKind, index: &EsmIndex) -> &HashMap<u32, Self>;
    fn records_mut(kind: ListKind, index: &mut EsmIndex) -> &mut HashMap<u32, Self>;
}

/// Implement [`ListRecord`] for `$record`, whose list is `$list` and whose every other field but `form_id` is named in
/// `fields`. The record is destructured whole, so a field added to it does not compile until it is named here.
macro_rules! list_record {
    ($record:ident, $entry:ty, $list:ident, fields [$($field:ident: $name:literal),* $(,)?],
     maps [$($kind:ident => $map:ident),+ $(,)?]) => {
        impl ListRecord for $record {
            type Entry = $entry;
            const KINDS: &'static [ListKind] = &[$(ListKind::$kind),+];
            const FIELDS: &'static [Field<Self>] = &[$(Field {
                name: $name,
                same: |a, b| a.$field == b.$field,
                copy: |from, to| to.$field.clone_from(&from.$field),
            }),*];
            fn entries(&self) -> &[$entry] {
                let $record { form_id: _, $list, $($field: _),* } = self;
                $list
            }
            fn entries_mut(&mut self) -> &mut Vec<$entry> {
                &mut self.$list
            }
            fn records(kind: ListKind, index: &EsmIndex) -> &HashMap<u32, Self> {
                match kind {
                    $(ListKind::$kind => &index.$map,)+
                    _ => unreachable!("{kind:?} is not a {}", stringify!($record)),
                }
            }
            fn records_mut(kind: ListKind, index: &mut EsmIndex) -> &mut HashMap<u32, Self> {
                match kind {
                    $(ListKind::$kind => &mut index.$map,)+
                    _ => unreachable!("{kind:?} is not a {}", stringify!($record)),
                }
            }
        }
    };
}

list_record!(LeveledList, LeveledEntry, entries,
    fields [editor_id: "editor ID", chance_none: "chance none", flags: "flags"],
    maps [LeveledItems => leveled_items, LeveledNpcs => leveled_npcs, LeveledCreatures => leveled_creatures,
          LeveledSpells => leveled_spells]);
list_record!(FlstRecord, u32, entries, fields [editor_id: "editor ID"], maps [FormList => form_lists]);
list_record!(OtftRecord, u32, items, fields [editor_id: "editor ID"], maps [Outfit => outfits]);
list_record!(ContainerRecord, InventoryEntry, contents,
    fields [editor_id: "editor ID", full_name: "name", model_path: "model", weight: "weight", flags: "flags",
            open_sound: "open sound", close_sound: "close sound", script_form_id: "script",
            script_instance: "script properties"],
    maps [Container => containers]);

/// What an override did to one form's entries, against its base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOp {
    Add,
    Change,
    Remove,
}

/// Which plugin's operation decided one form or field, and the operations it overruled, earliest first. Plugins are
/// load-order positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution<Op> {
    pub winner: u32,
    pub op: Op,
    pub overruled: Vec<(u32, Op)>,
}

/// How one record was merged (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListMergeRecord {
    pub kind: ListKind,
    /// The plugins whose changes the list holds, in fold order: the plugin whose record the merge started from (the
    /// definer, or the last wholesale winner) first.
    pub contributors: Vec<u32>,
    /// The last plugin that won the list whole, as a dependent of every contributor before it, with those contributors.
    pub wholesale: Option<(u32, Vec<u32>)>,
    /// Per form whose entries a contributor changed: the winning operation and those it overruled. A wholesale winner's
    /// operations are what it changed in the list it replaced.
    pub entries: Vec<(u32, Resolution<EntryOp>)>,
    /// Per field a contributor changed, by its name, likewise.
    pub fields: Vec<(&'static str, Resolution<()>)>,
}

impl ListMergeRecord {
    /// Whether more than one plugin's changes are in the record.
    pub fn is_merged(&self) -> bool {
        self.contributors.len() > 1
    }

    /// How many operations were overruled.
    pub fn overruled(&self) -> usize {
        self.entries.iter().map(|(_, r)| r.overruled.len()).sum::<usize>()
            + self.fields.iter().map(|(_, r)| r.overruled.len()).sum::<usize>()
    }
}

/// One overridden record's state in the fold.
#[derive(Debug, Clone)]
struct ListState<R> {
    record: ListMergeRecord,
    /// Every carrier's own record, in fold order, from the one the state started at.
    carriers: Vec<(u32, R)>,
    /// The merged record as it stands.
    current: R,
}

/// The entries of `list` grouped by the form they name, in order of first appearance; null entries are left out.
fn groups<E: ListEntry>(list: &[E]) -> Vec<(u32, Vec<E>)> {
    let mut at: HashMap<u32, usize> = HashMap::new();
    let mut groups: Vec<(u32, Vec<E>)> = Vec::new();
    for entry in list.iter().filter(|entry| entry.form() != NULL_FORM) {
        let slot = *at.entry(entry.form()).or_insert_with(|| {
            groups.push((entry.form(), Vec::new()));
            groups.len() - 1
        });
        groups[slot].1.push(*entry);
    }
    groups
}

/// The entries of one form as a multiset of the rest of each entry.
fn multiset<E: ListEntry>(entries: &[E]) -> Vec<E::Data> {
    let mut set: Vec<E::Data> = entries.iter().map(ListEntry::data).collect();
    set.sort_unstable();
    set
}

/// The per-form operations `incoming` makes against `base`: additions and changes in `incoming`'s order, then removals
/// in `base`'s, each with `incoming`'s entries for the form (none for a removal).
fn diff_entries<E: ListEntry>(base: &[E], incoming: &[E]) -> Vec<(u32, EntryOp, Vec<E>)> {
    let base_groups = groups(base);
    let incoming_groups = groups(incoming);
    let base_of: HashMap<u32, &Vec<E>> = base_groups.iter().map(|(form, entries)| (*form, entries)).collect();
    let mut ops = Vec::new();
    for (form, entries) in &incoming_groups {
        match base_of.get(form) {
            None => ops.push((*form, EntryOp::Add, entries.clone())),
            Some(base_entries) if multiset(base_entries) != multiset(entries) => {
                ops.push((*form, EntryOp::Change, entries.clone()));
            }
            Some(_) => {}
        }
    }
    let incoming_forms: std::collections::HashSet<u32> = incoming_groups.iter().map(|(form, _)| *form).collect();
    ops.extend(
        base_groups
            .iter()
            .filter(|(form, _)| !incoming_forms.contains(form))
            .map(|(form, _)| (*form, EntryOp::Remove, Vec::new())),
    );
    ops
}

/// `current`'s entries with `form`'s replaced by `entries` (removed when empty), at the place of its first entry, or
/// appended. `false` when that leaves them as they were.
fn replace_form<E: ListEntry>(current: &mut Vec<E>, form: u32, entries: &[E]) -> bool {
    let present: Vec<E> = current.iter().filter(|entry| entry.form() == form).copied().collect();
    if multiset(&present) == multiset(entries) {
        return false;
    }
    let at = current.iter().position(|entry| entry.form() == form).unwrap_or(current.len());
    current.retain(|entry| entry.form() != form);
    let at = at.min(current.len());
    current.splice(at..at, entries.iter().copied());
    true
}

/// Record that `winner`'s `op` decided `key`, overruling the operation that had.
fn decide<K: PartialEq + Copy, Op: Copy>(resolutions: &mut Vec<(K, Resolution<Op>)>, key: K, winner: u32, op: Op) {
    match resolutions.iter_mut().find(|(k, _)| *k == key) {
        Some((_, resolution)) => {
            resolution.overruled.push((resolution.winner, resolution.op));
            resolution.winner = winner;
            resolution.op = op;
        }
        None => resolutions.push((key, Resolution { winner, op, overruled: Vec::new() })),
    }
}

impl<R: ListRecord> ListState<R> {
    fn new(kind: ListKind, previous: u32, record: &R) -> Self {
        Self {
            record: ListMergeRecord {
                kind,
                contributors: vec![previous],
                wholesale: None,
                entries: Vec::new(),
                fields: Vec::new(),
            },
            carriers: vec![(previous, record.clone())],
            current: record.clone(),
        }
    }

    /// Fold `incoming`, the record of the plugin at `position`. `true` when the result is a merge the fold must write
    /// back over the plugin's own record; `false` when the plugin won the list whole.
    fn fold(&mut self, position: u32, incoming: &R, depends_on: &dyn Fn(u32, u32) -> bool) -> bool {
        let contributors = &self.record.contributors;
        if contributors.iter().all(|&contributor| depends_on(position, contributor)) {
            // A dependent override wins whole. What it changed in the list it replaced is still its operations, so
            // a later independent override that undoes one of them overrules it in the report.
            self.record.wholesale = Some((position, contributors.clone()));
            self.record.contributors = vec![position];
            self.record.entries = diff_entries(self.current.entries(), incoming.entries())
                .into_iter()
                .map(|(form, op, _)| (form, Resolution { winner: position, op, overruled: Vec::new() }))
                .collect();
            self.record.fields = R::FIELDS
                .iter()
                .filter(|field| !(field.same)(&self.current, incoming))
                .map(|field| (field.name, Resolution { winner: position, op: (), overruled: Vec::new() }))
                .collect();
            self.current = incoming.clone();
            self.carriers.push((position, incoming.clone()));
            return false;
        }
        let base = self
            .carriers
            .iter()
            .rev()
            .find(|(carrier, _)| depends_on(position, *carrier))
            .unwrap_or(&self.carriers[0])
            .1
            .clone();
        for (form, op, entries) in diff_entries(base.entries(), incoming.entries()) {
            if replace_form(self.current.entries_mut(), form, &entries) {
                decide(&mut self.record.entries, form, position, op);
            }
        }
        for field in R::FIELDS {
            if !(field.same)(&base, incoming) && !(field.same)(&self.current, incoming) {
                (field.copy)(incoming, &mut self.current);
                decide(&mut self.record.fields, field.name, position, ());
            }
        }
        self.record.contributors.push(position);
        self.carriers.push((position, incoming.clone()));
        true
    }
}

/// The merged records one plugin's fold produced, to write over its own records once it is folded.
#[derive(Default)]
#[must_use = "the merged records are lost unless applied after the plugin's merge"]
pub struct PendingLists(Vec<WriteBack>);

/// One merged record's write into the folded index.
type WriteBack = Box<dyn FnOnce(&mut EsmIndex)>;

impl std::fmt::Debug for PendingLists {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PendingLists({} records)", self.0.len())
    }
}

impl PendingLists {
    /// Write the merged records into `merged`, after the plugin's [`EsmIndex::merge_from`] put its own records there.
    pub fn apply(self, merged: &mut EsmIndex) {
        for write in self.0 {
            write(merged);
        }
    }
}

/// The states of one record type's merged records.
type States<R> = HashMap<(ListKind, u32), ListState<R>>;

/// The list merge of a load-order fold (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct ListMerge {
    policy: MergePolicy,
    leveled: States<LeveledList>,
    form_lists: States<FlstRecord>,
    outfits: States<OtftRecord>,
    containers: States<ContainerRecord>,
}

/// Fold the records of type `R` that `plugin` overrides into `states`, queueing the merged ones on `pending`.
fn note_records<R: ListRecord>(
    states: &mut States<R>,
    position: u32,
    plugin: &EsmIndex,
    merged: &EsmIndex,
    provenance: &FoldProvenance,
    depends_on: &dyn Fn(u32, u32) -> bool,
    pending: &mut PendingLists,
) {
    for &form in &plugin.deleted_record_metadata {
        for &kind in R::KINDS {
            states.remove(&(kind, form));
        }
    }
    for &kind in R::KINDS {
        for (&form, incoming) in R::records(kind, plugin) {
            let Some(current) = R::records(kind, merged).get(&form) else {
                continue; // not an override
            };
            let state = states.entry((kind, form)).or_insert_with(|| {
                // The carrier before this one wrote `current`.
                let carriers = provenance.carriers(form);
                let previous = carriers.len().checked_sub(2).map_or(0, |at| carriers[at].plugin);
                ListState::new(kind, previous, current)
            });
            if state.fold(position, incoming, depends_on) {
                let record = state.current.clone();
                pending.0.push(Box::new(move |merged| {
                    R::records_mut(kind, merged).insert(form, record);
                }));
            }
        }
    }
}

/// `form`'s state in `states`, under whichever of `R`'s kinds it is.
fn state_of<R: ListRecord>(states: &States<R>, form: u32) -> Option<&ListMergeRecord> {
    R::KINDS.iter().find_map(|&kind| states.get(&(kind, form))).map(|state| &state.record)
}

/// Every record in `states`.
fn records_in<R: ListRecord>(states: &States<R>) -> impl Iterator<Item = (u32, &ListMergeRecord)> {
    states.iter().map(|(&(_, form), state)| (form, &state.record))
}

impl ListMerge {
    pub fn new(policy: MergePolicy) -> Self {
        Self { policy, ..Self::default() }
    }

    pub fn policy(&self) -> MergePolicy {
        self.policy
    }

    /// Note `plugin`, the plugin at load-order `position`, before it is folded into `merged`; `provenance` has already
    /// noted it. `depends_on(a, b)` answers whether the plugin at `a` declares the one at `b` as a master, directly or
    /// through its masters. Apply the result after the plugin's [`EsmIndex::merge_from`].
    pub fn note_plugin(
        &mut self,
        position: usize,
        plugin: &EsmIndex,
        merged: &EsmIndex,
        provenance: &FoldProvenance,
        depends_on: &dyn Fn(u32, u32) -> bool,
    ) -> PendingLists {
        let mut pending = PendingLists::default();
        if self.policy == MergePolicy::Legacy {
            return pending;
        }
        let position = position as u32;
        note_records(&mut self.leveled, position, plugin, merged, provenance, depends_on, &mut pending);
        note_records(&mut self.form_lists, position, plugin, merged, provenance, depends_on, &mut pending);
        note_records(&mut self.outfits, position, plugin, merged, provenance, depends_on, &mut pending);
        note_records(&mut self.containers, position, plugin, merged, provenance, depends_on, &mut pending);
        pending
    }

    /// How `form` was merged, when it is a record that merges and more than one plugin carried it.
    pub fn record(&self, form: u32) -> Option<&ListMergeRecord> {
        state_of(&self.leveled, form)
            .or_else(|| state_of(&self.form_lists, form))
            .or_else(|| state_of(&self.outfits, form))
            .or_else(|| state_of(&self.containers, form))
    }

    /// The records that hold more than one plugin's changes, in no particular order.
    pub fn merged(&self) -> impl Iterator<Item = (u32, &ListMergeRecord)> {
        records_in(&self.leveled)
            .chain(records_in(&self.form_lists))
            .chain(records_in(&self.outfits))
            .chain(records_in(&self.containers))
            .filter(|(_, record)| record.is_merged())
    }

    /// How many records hold more than one plugin's changes, and how many operations were overruled in them.
    pub fn counts(&self) -> (usize, usize) {
        self.merged().fold((0, 0), |(lists, overruled), (_, record)| (lists + 1, overruled + record.overruled()))
    }
}

#[cfg(test)]
#[path = "list_merge_tests.rs"]
mod tests;
