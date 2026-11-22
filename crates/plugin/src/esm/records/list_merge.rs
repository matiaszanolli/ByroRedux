//! Levelled lists merged across independent overrides (PRD legacy-mod-compatibility LMC-REC-003, decision Q2).
//!
//! Under the `legacy` policy (LMC-REC-002) the fold keeps the last plugin's record of a form whole, so two mods that
//! each add a sword to the same levelled list leave only the later one's sword: the reason list patches (Bashed,
//! Smashed, Synthesis) exist. The `merge` policy, the default by decision Q2, combines them instead, for the levelled
//! lists (`LVLI`, `LVLN`, `LVLC`, `LVSP`):
//!
//! - **Scope.** Only *independent* overrides merge: a plugin that declares, directly or through its masters, every
//!   plugin whose changes the list holds so far (a patch) wins the list whole, as under `legacy`.
//! - **Per form.** An override is diffed against the record it overrides, its *base*: the latest earlier carrier among
//!   its own masters (the definer at the least). The entries are grouped by the form they spawn and compared as
//!   multisets of `(level, count)`, since levelled lists repeat a form on purpose to shape the odds: equal is kept; only
//!   in the base is a *removal*; only in the override an *addition*; in both but different a *change*, which replaces
//!   the base's entries for that form with the override's. A re-level is one change, never a removal plus an addition,
//!   so two plugins moving one sword to different levels cannot leave it in the list twice.
//! - **Last wins per form.** Operations on different forms combine; on the same form the later plugin in load order
//!   wins and the earlier operation is recorded as overruled. Operations that would leave the list as it is are not
//!   operations.
//! - **Fields.** The list's other fields (editor ID, chance none, flags) follow the same rule: a field the override
//!   changed against its base is an operation, last wins.
//! - **Explained.** [`ListMergeRecord`] keeps, per merged list, every contributor, the winner and overruled operations
//!   of each form and field, and the last wholesale win.
//!
//! The merged list keeps the order of the entries it started from: a changed form takes the place of its first entry,
//! an added form goes at the end. Per-entry extra data (`COED` owner and condition) is not decoded, so it takes no part
//! in the comparison. Other list-valued records (form lists, keywords, inventories, factions, outfits, spell lists)
//! still follow `legacy`.

use std::collections::HashMap;

use super::container::{LeveledEntry, LeveledList};
use super::provenance::FoldProvenance;
use super::EsmIndex;

/// How the fold resolves a form more than one plugin carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MergePolicy {
    /// Levelled lists merge across independent overrides (LMC-REC-003); every other record is `legacy`'s.
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

/// The levelled-list record types, each with its own map in [`EsmIndex`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListKind {
    Items,
    Npcs,
    Creatures,
    Spells,
}

impl ListKind {
    pub const ALL: [Self; 4] = [Self::Items, Self::Npcs, Self::Creatures, Self::Spells];

    /// The record type.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Items => "LVLI",
            Self::Npcs => "LVLN",
            Self::Creatures => "LVLC",
            Self::Spells => "LVSP",
        }
    }

    fn lists(self, index: &EsmIndex) -> &HashMap<u32, LeveledList> {
        match self {
            Self::Items => &index.leveled_items,
            Self::Npcs => &index.leveled_npcs,
            Self::Creatures => &index.leveled_creatures,
            Self::Spells => &index.leveled_spells,
        }
    }

    fn lists_mut(self, index: &mut EsmIndex) -> &mut HashMap<u32, LeveledList> {
        match self {
            Self::Items => &mut index.leveled_items,
            Self::Npcs => &mut index.leveled_npcs,
            Self::Creatures => &mut index.leveled_creatures,
            Self::Spells => &mut index.leveled_spells,
        }
    }
}

/// What an override did to one form's entries, against its base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOp {
    Add,
    Change,
    Remove,
}

/// A levelled list's fields besides its entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListField {
    EditorId,
    ChanceNone,
    Flags,
}

impl ListField {
    const ALL: [Self; 3] = [Self::EditorId, Self::ChanceNone, Self::Flags];

    /// The field's name in a report.
    pub fn name(self) -> &'static str {
        match self {
            Self::EditorId => "editor ID",
            Self::ChanceNone => "chance none",
            Self::Flags => "flags",
        }
    }

    fn same(self, a: &LeveledList, b: &LeveledList) -> bool {
        match self {
            Self::EditorId => a.editor_id == b.editor_id,
            Self::ChanceNone => a.chance_none == b.chance_none,
            Self::Flags => a.flags == b.flags,
        }
    }

    fn copy(self, from: &LeveledList, to: &mut LeveledList) {
        match self {
            Self::EditorId => to.editor_id.clone_from(&from.editor_id),
            Self::ChanceNone => to.chance_none = from.chance_none,
            Self::Flags => to.flags = from.flags,
        }
    }
}

/// Which plugin's operation decided one form or field, and the operations it overruled, earliest first. Plugins are
/// load-order positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution<Op> {
    pub winner: u32,
    pub op: Op,
    pub overruled: Vec<(u32, Op)>,
}

/// How one levelled list was merged (see the module docs).
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
    /// Per field a contributor changed, likewise.
    pub fields: Vec<(ListField, Resolution<()>)>,
}

impl ListMergeRecord {
    /// Whether more than one plugin's changes are in the list.
    pub fn is_merged(&self) -> bool {
        self.contributors.len() > 1
    }

    /// How many operations were overruled.
    pub fn overruled(&self) -> usize {
        self.entries.iter().map(|(_, r)| r.overruled.len()).sum::<usize>()
            + self.fields.iter().map(|(_, r)| r.overruled.len()).sum::<usize>()
    }
}

/// One overridden list's state in the fold.
#[derive(Debug, Clone)]
struct ListState {
    record: ListMergeRecord,
    /// Every carrier's own record, in fold order, from the one the state started at.
    carriers: Vec<(u32, LeveledList)>,
    /// The merged list as it stands.
    current: LeveledList,
}

/// The entries of `list` grouped by the form they spawn, in order of first appearance.
fn groups(list: &LeveledList) -> Vec<(u32, Vec<LeveledEntry>)> {
    let mut at: HashMap<u32, usize> = HashMap::new();
    let mut groups: Vec<(u32, Vec<LeveledEntry>)> = Vec::new();
    for entry in &list.entries {
        let slot = *at.entry(entry.form_id).or_insert_with(|| {
            groups.push((entry.form_id, Vec::new()));
            groups.len() - 1
        });
        groups[slot].1.push(*entry);
    }
    groups
}

/// The entries of one form as a multiset of `(level, count)`.
fn multiset(entries: &[LeveledEntry]) -> Vec<(u16, u16)> {
    let mut set: Vec<(u16, u16)> = entries.iter().map(|entry| (entry.level, entry.count)).collect();
    set.sort_unstable();
    set
}

/// The per-form operations `incoming` makes against `base`: additions and changes in `incoming`'s order, then removals
/// in `base`'s, each with `incoming`'s entries for the form (none for a removal).
fn diff_entries(base: &LeveledList, incoming: &LeveledList) -> Vec<(u32, EntryOp, Vec<LeveledEntry>)> {
    let base_groups = groups(base);
    let incoming_groups = groups(incoming);
    let base_of: HashMap<u32, &Vec<LeveledEntry>> = base_groups.iter().map(|(form, entries)| (*form, entries)).collect();
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
fn replace_form(current: &mut Vec<LeveledEntry>, form: u32, entries: &[LeveledEntry]) -> bool {
    let present: Vec<LeveledEntry> = current.iter().filter(|entry| entry.form_id == form).copied().collect();
    if multiset(&present) == multiset(entries) {
        return false;
    }
    let at = current.iter().position(|entry| entry.form_id == form).unwrap_or(current.len());
    current.retain(|entry| entry.form_id != form);
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

impl ListState {
    fn new(kind: ListKind, previous: u32, record: &LeveledList) -> Self {
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
    fn fold(&mut self, position: u32, incoming: &LeveledList, depends_on: &dyn Fn(u32, u32) -> bool) -> bool {
        let contributors = &self.record.contributors;
        if contributors.iter().all(|&contributor| depends_on(position, contributor)) {
            // A dependent override wins whole. What it changed in the list it replaced is still its operations, so
            // a later independent override that undoes one of them overrules it in the report.
            self.record.wholesale = Some((position, contributors.clone()));
            self.record.contributors = vec![position];
            self.record.entries = diff_entries(&self.current, incoming)
                .into_iter()
                .map(|(form, op, _)| (form, Resolution { winner: position, op, overruled: Vec::new() }))
                .collect();
            self.record.fields = ListField::ALL
                .into_iter()
                .filter(|field| !field.same(&self.current, incoming))
                .map(|field| (field, Resolution { winner: position, op: (), overruled: Vec::new() }))
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
        for (form, op, entries) in diff_entries(&base, incoming) {
            if replace_form(&mut self.current.entries, form, &entries) {
                decide(&mut self.record.entries, form, position, op);
            }
        }
        for field in ListField::ALL {
            if !field.same(&base, incoming) && !field.same(&self.current, incoming) {
                field.copy(incoming, &mut self.current);
                decide(&mut self.record.fields, field, position, ());
            }
        }
        self.record.contributors.push(position);
        self.carriers.push((position, incoming.clone()));
        true
    }
}

/// The merged lists one plugin's fold produced, to write over its own records once it is folded.
#[derive(Debug, Default)]
#[must_use = "the merged lists are lost unless applied after the plugin's merge"]
pub struct PendingLists(Vec<(ListKind, u32, LeveledList)>);

impl PendingLists {
    /// Write the merged lists into `merged`, after the plugin's [`EsmIndex::merge_from`] put its own records there.
    pub fn apply(self, merged: &mut EsmIndex) {
        for (kind, form, list) in self.0 {
            kind.lists_mut(merged).insert(form, list);
        }
    }
}

/// The levelled-list merge of a load-order fold (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct ListMerge {
    policy: MergePolicy,
    lists: HashMap<(ListKind, u32), ListState>,
}

impl ListMerge {
    pub fn new(policy: MergePolicy) -> Self {
        Self { policy, lists: HashMap::new() }
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
        for &form in &plugin.deleted_record_metadata {
            for kind in ListKind::ALL {
                self.lists.remove(&(kind, form));
            }
        }
        for kind in ListKind::ALL {
            for (&form, incoming) in kind.lists(plugin) {
                let Some(current) = kind.lists(merged).get(&form) else {
                    continue; // not an override
                };
                let state = self.lists.entry((kind, form)).or_insert_with(|| {
                    // The carrier before this one wrote `current`.
                    let carriers = provenance.carriers(form);
                    let previous = carriers.len().checked_sub(2).map_or(0, |at| carriers[at].plugin);
                    ListState::new(kind, previous, current)
                });
                if state.fold(position, incoming, depends_on) {
                    pending.0.push((kind, form, state.current.clone()));
                }
            }
        }
        pending
    }

    /// How `form` was merged, when it is a levelled list more than one plugin carried.
    pub fn record(&self, form: u32) -> Option<&ListMergeRecord> {
        ListKind::ALL.iter().find_map(|&kind| self.lists.get(&(kind, form))).map(|state| &state.record)
    }

    /// The levelled lists that hold more than one plugin's changes, in no particular order.
    pub fn merged(&self) -> impl Iterator<Item = (u32, &ListMergeRecord)> {
        self.lists.iter().filter(|(_, state)| state.record.is_merged()).map(|(&(_, form), state)| (form, &state.record))
    }

    /// How many lists hold more than one plugin's changes, and how many operations were overruled in them.
    pub fn counts(&self) -> (usize, usize) {
        self.merged().fold((0, 0), |(lists, overruled), (_, record)| (lists + 1, overruled + record.overruled()))
    }
}

#[cfg(test)]
#[path = "list_merge_tests.rs"]
mod tests;
