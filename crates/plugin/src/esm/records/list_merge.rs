//! List-valued records merged across independent overrides (PRD legacy-mod-compatibility LMC-REC-003, decision Q2).
//!
//! Under the `legacy` policy (LMC-REC-002) the fold keeps the last plugin's record of a form whole, so two mods that
//! each add a sword to the same levelled list leave only the later one's sword: the reason list patches (Bashed,
//! Smashed, Synthesis) exist. The `merge` policy, the default by decision Q2, combines them instead, for the records
//! that carry lists: the levelled lists (`LVLI`, `LVLN`, `LVLC`, `LVSP`), form lists (`FLST`), outfits (`OTFT`),
//! containers (`CONT`), actors (`NPC_`, `CREA`: inventory, factions, spells, perks, keywords, actor values), races
//! (`RACE`: spells, keywords, race relations), carried items (`ALCH`, `AMMO`, `ARMO`, `BOOK`, `MISC`, `WEAP`, ...:
//! keywords) and the keyword lists of activator, flora, furniture and container bases, which the index keeps apart from
//! their records (`EsmIndex::placed_object_keywords`) and which merge on their own:
//!
//! - **Scope.** Only *independent* overrides merge: a plugin that declares, directly or through its masters, every
//!   plugin whose changes the record holds so far (a patch) wins it whole, as under `legacy`.
//! - **Ownership (LMC-REC-006).** An override identical to the record it would replace is a *no-op*: it takes no
//!   ownership, so it neither wins the record whole nor becomes a contributor a later patch must master. An
//!   independent override identical to its own base *changes nothing*: it is no contributor either, and the changes
//!   before it stand, where `legacy` would let its record undo them. A record counts as merged when more than one
//!   plugin's changes are in it or when it differs from the record `legacy` keeps.
//! - **Per form.** An override is diffed against the record it overrides, its *base*: the latest earlier carrier among
//!   its own masters (the definer at the least). Each list's entries are grouped by the form they name and compared as
//!   multisets of the rest of each entry (a levelled entry's level and count, an inventory entry's count, a faction's
//!   rank, a perk's rank, an actor value's value), since lists repeat a form on purpose: equal is kept; only in the base is a *removal*; only in the override an *addition*; in
//!   both but different a *change*, which replaces the base's entries for that form with the override's. A re-level is
//!   one change, never a removal plus an addition, so two plugins moving one sword to different levels cannot leave it
//!   in the list twice.
//! - **Last wins per form.** Operations on different forms combine; on the same form the later plugin in load order
//!   wins and the earlier operation is recorded as overruled. Operations that would leave the list as it is are not
//!   operations.
//! - **Fields.** The record's other fields (a levelled list's chance none, a container's name and weight, an actor's
//!   level, ...) follow the same rule: a field the override changed against its base is an operation, last wins. A
//!   list whose order is its meaning is one field: an actor's AI packages, in priority order, and a consumable's
//!   effects, inside its type data. An item's kind is compared as its own record authored it, before the fold promoted
//!   a `MISC` an `OMOD` names to a loose object mod.
//! - **Explained.** [`ListMergeRecord`] keeps, per merged record, every contributor, the winner and overruled operations
//!   of each form and field, and the last wholesale win.
//!
//! The merged list keeps the order of the entries it started from: a changed form takes the place of its first entry,
//! an added form goes at the end, so the indices of a form list's untouched entries stay put. A form list's null
//! entries (`0xFFFFFFFF`, a slot the author left empty) hold a place, not a form: they are never an operation and keep
//! their positions. Per-entry extra data (`COED` owner and condition) is not decoded, so it takes no part in the
//! comparison. Locations (`LCTN`) and quests (`QUST`) still follow `legacy`: Fallout 4 and Starfield write overrides of
//! both as *partial forms* (record flag `0x4000`, xEdit "Partial Form"), which carry only the sub-records they change.
//! The fold overlays a partial `LCTN` or `QUST` on the record it overrides (#7275, `LctnRecord::overlaid_on`,
//! `QustRecord::overlaid_on`), so their absent members are inherited; extending the per-field merge to them is still
//! to do. Every record without a list follows `legacy` too.

use std::collections::HashMap;

use super::actor::{FactionMembership, NpcInventoryEntry, NpcRecord, RaceRecord};
use super::common::CommonItemFields;
use super::container::{ContainerRecord, InventoryEntry, LeveledEntry, LeveledList};
use super::items::{ItemKind, ItemRecord};
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

/// The record maps of [`EsmIndex`] whose records merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListKind {
    LeveledItems,
    LeveledNpcs,
    LeveledCreatures,
    LeveledSpells,
    FormList,
    Outfit,
    Container,
    Npc,
    Creature,
    Race,
    /// Every carried item type (`ALCH`, `AMMO`, `ARMO`, `BOOK`, `KEYM`, `MISC`, `WEAP`, ...), one map.
    Item,
    /// The keyword lists of activator, flora, furniture and container bases, kept apart from their records
    /// (`EsmIndex::placed_object_keywords`).
    ObjectKeywords,
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

impl ListEntry for NpcInventoryEntry {
    type Data = i32;
    fn form(&self) -> u32 {
        self.item_form_id
    }
    fn data(&self) -> i32 {
        self.count
    }
}

impl ListEntry for FactionMembership {
    type Data = i8;
    fn form(&self) -> u32 {
        self.faction_form_id
    }
    fn data(&self) -> i8 {
        self.rank
    }
}

/// An actor's perk (`PRKR`): the perk and its rank.
impl ListEntry for (u32, u8) {
    type Data = u8;
    fn form(&self) -> u32 {
        self.0
    }
    fn data(&self) -> u8 {
        self.1
    }
}

/// A race's relation to another race (`XNAM`): the race and the disposition adjustment.
impl ListEntry for (u32, i32) {
    type Data = i32;
    fn form(&self) -> u32 {
        self.0
    }
    fn data(&self) -> i32 {
        self.1
    }
}

/// An actor's actor value (`PRPS`): the actor value and its value, compared by its bits.
impl ListEntry for (u32, f32) {
    type Data = u32;
    fn form(&self) -> u32 {
        self.0
    }
    fn data(&self) -> u32 {
        self.1.to_bits()
    }
}

/// A bare form ID: a form list's, an outfit's, a spell list's or a keyword list's entry.
impl ListEntry for u32 {
    type Data = ();
    fn form(&self) -> u32 {
        *self
    }
    fn data(&self) {}
}

/// A form list's empty slot: it holds a place, so it is never one of the merge's operations.
const NULL_FORM: u32 = 0xFFFF_FFFF;

/// A list of a record type that merges.
pub struct List<R> {
    /// What one of the list's entries is called in a report.
    pub entry: &'static str,
    /// The per-form operations the second record makes against the first.
    diff: fn(&R, &R) -> Vec<(u32, EntryOp)>,
    /// Apply the operations `incoming` (the second record) makes against `base` (the first) to `current` (the third),
    /// returning those that changed it.
    fold: fn(&R, &R, &mut R) -> Vec<(u32, EntryOp)>,
}

/// A scalar field of a record that merges.
pub struct Field<R> {
    /// The field's name in a report.
    pub name: &'static str,
    same: fn(&R, &R) -> bool,
    copy: fn(&R, &mut R),
}

/// A record type that merges: its lists and its scalar fields (see the module docs), implemented by `list_record!`.
pub trait ListRecord: Clone + 'static {
    /// The kinds whose map holds this type.
    const KINDS: &'static [ListKind];
    /// The lists that merge per form.
    const LISTS: &'static [List<Self>];
    /// Every other field besides the form ID.
    const FIELDS: &'static [Field<Self>];
    fn records(kind: ListKind, index: &EsmIndex) -> &HashMap<u32, Self>;
    fn records_mut(kind: ListKind, index: &mut EsmIndex) -> &mut HashMap<u32, Self>;
}

/// [`List::diff`] for one list of entries.
fn list_diff<E: ListEntry>(base: &[E], incoming: &[E]) -> Vec<(u32, EntryOp)> {
    diff_entries(base, incoming).into_iter().map(|(form, op, _)| (form, op)).collect()
}

/// [`List::fold`] for one list of entries.
fn list_fold<E: ListEntry>(base: &[E], incoming: &[E], current: &mut Vec<E>) -> Vec<(u32, EntryOp)> {
    diff_entries(base, incoming)
        .into_iter()
        .filter_map(|(form, op, entries)| replace_form(current, form, &entries).then_some((form, op)))
        .collect()
}

/// Implement [`ListRecord`] for `$record`, whose lists are named in `lists` (by the field and what an entry is called)
/// and whose every other field but `form_id` in `fields` (by the field and its name in a report, and optionally the
/// function that compares it, when `==` would see a difference the record's author did not make).
///
/// The record is destructured whole, so a field added to it does not compile until it is named here. A record whose
/// lists or fields are nested names them by path, after the `@impl` marker, and checks its fields itself.
macro_rules! list_record {
    (@impl $record:ident, lists [$($($list:ident).+: $entry:literal),+ $(,)?],
     fields [$($($field:ident).+: $name:literal $(by $same:path)?),* $(,)?],
     maps [$($kind:ident => $map:ident),+ $(,)?]) => {
        impl ListRecord for $record {
            const KINDS: &'static [ListKind] = &[$(ListKind::$kind),+];
            const LISTS: &'static [List<Self>] = &[$(List {
                entry: $entry,
                diff: |base, incoming| list_diff(&base.$($list).+, &incoming.$($list).+),
                fold: |base, incoming, current| list_fold(&base.$($list).+, &incoming.$($list).+, &mut current.$($list).+),
            }),+];
            const FIELDS: &'static [Field<Self>] = &[$(Field {
                name: $name,
                same: |a, b| list_record!(@same a.$($field).+, b.$($field).+ $(, $same)?),
                copy: |from, to| to.$($field).+.clone_from(&from.$($field).+),
            }),*];
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
    (@same $a:expr, $b:expr) => { $a == $b };
    (@same $a:expr, $b:expr, $same:path) => { $same(&$a, &$b) };
    ($record:ident, lists [$($list:ident: $entry:literal),+ $(,)?],
     fields [$($field:ident: $name:literal $(by $same:path)?),* $(,)?],
     maps [$($kind:ident => $map:ident),+ $(,)?]) => {
        const _: fn(&$record) = |record| {
            let $record { form_id: _, $($list: _,)+ $($field: _),* } = record;
        };
        list_record!(@impl $record, lists [$($list: $entry),+], fields [$($field: $name $(by $same)?),*],
                     maps [$($kind => $map),+]);
    };
}

list_record!(LeveledList, lists [entries: "entry"],
    fields [editor_id: "editor ID", chance_none: "chance none", flags: "flags"],
    maps [LeveledItems => leveled_items, LeveledNpcs => leveled_npcs, LeveledCreatures => leveled_creatures,
          LeveledSpells => leveled_spells]);
list_record!(FlstRecord, lists [entries: "entry"], fields [editor_id: "editor ID"], maps [FormList => form_lists]);
list_record!(OtftRecord, lists [items: "entry"], fields [editor_id: "editor ID"], maps [Outfit => outfits]);
list_record!(ContainerRecord, lists [contents: "entry"],
    fields [editor_id: "editor ID", full_name: "name", model_path: "model", weight: "weight", flags: "flags",
            open_sound: "open sound", close_sound: "close sound", script_form_id: "script",
            script_instance: "script properties"],
    maps [Container => containers]);
list_record!(NpcRecord,
    lists [inventory: "inventory", factions: "faction", spells: "spell", perks: "perk", keywords: "keyword",
           actor_value_props: "actor value"],
    fields [crime_faction_form_id: "crime faction", editor_id: "editor ID", full_name: "name",
            starts_dead: "starts dead", model_path: "model", body_part_models: "body part models",
            is_creature: "creature", body_part_form: "body part data", creature_stats: "creature stats",
            oblivion_npc_stats: "stats", oblivion_creature_stats: "creature stats (Oblivion)",
            oblivion_pools: "health and magicka", oblivion_ai_values: "AI values", data_base_health: "base health",
            data_attributes: "attributes", race_form_id: "race", class_form_id: "class", voice_form_id: "voice",
            default_outfit: "default outfit", worn_skin: "skin", ai_packages: "AI packages", ai_data: "AI data",
            death_item_form_id: "death item", level: "level", calc_min: "calc min", magicka_offset: "magicka offset",
            stamina_offset: "stamina offset", health_offset: "health offset", disposition_base: "disposition",
            acbs_flags: "flags", essential: "essential", protected: "protected", bleedout_override: "bleedout",
            skill_values: "skills", skyrim_calculated_pools: "calculated pools", has_script: "has script",
            script_form_id: "script", script_instance: "script properties", face_morphs: "face morphs",
            runtime_facegen: "face", template_form_id: "template", template_flags: "template flags",
            template_actors: "template actors", calculated_health: "calculated health",
            calculated_action_points: "calculated action points"],
    maps [Npc => npcs, Creature => creatures]);
list_record!(RaceRecord, lists [spells: "spell", keywords: "keyword", race_reactions: "race relation"],
    fields [editor_id: "editor ID", full_name: "name", description: "description", skill_bonuses: "skill bonuses",
            body_models: "body models", skeleton_models: "skeletons", head_parts: "head parts",
            base_head_parts: "base head parts", head_part_textures: "head part textures",
            body_part_textures: "body part textures", tail_models: "tail models", base_height: "height",
            base_weight: "weight", race_flags: "flags", starting_health: "starting health",
            starting_magicka: "starting magicka", starting_stamina: "starting stamina",
            base_attributes: "attributes", default_hair: "default hair", voice_forms: "voices",
            facegen_main_clamp: "face clamp (main)", facegen_face_clamp: "face clamp (face)",
            default_skin: "skin", subgraph_template: "subgraph template",
            subgraph_additive: "subgraph additive race", subgraph_data: "subgraphs", bleedout: "bleedout"],
    maps [Race => races]);

/// A base object's keyword list kept apart from its record: the list is all there is to merge.
impl ListRecord for Vec<u32> {
    const KINDS: &'static [ListKind] = &[ListKind::ObjectKeywords];
    const LISTS: &'static [List<Self>] = &[List {
        entry: "keyword",
        diff: |base, incoming| list_diff(base, incoming),
        fold: |base, incoming, current| list_fold(base, incoming, current),
    }];
    const FIELDS: &'static [Field<Self>] = &[];
    fn records(kind: ListKind, index: &EsmIndex) -> &HashMap<u32, Self> {
        match kind {
            ListKind::ObjectKeywords => &index.placed_object_keywords,
            _ => unreachable!("{kind:?} is not a keyword list"),
        }
    }
    fn records_mut(kind: ListKind, index: &mut EsmIndex) -> &mut HashMap<u32, Self> {
        match kind {
            ListKind::ObjectKeywords => &mut index.placed_object_keywords,
            _ => unreachable!("{kind:?} is not a keyword list"),
        }
    }
}

// An item's keyword list sits in its shared fields, so it is named by path and the fields are checked here.
const _: fn(&ItemRecord) = |item| {
    let ItemRecord {
        form_id: _,
        common:
            CommonItemFields {
                editor_id: _,
                full_name: _,
                model_path: _,
                icon_path: _,
                script_form_id: _,
                value: _,
                weight: _,
                has_script: _,
                script_instance: _,
                keywords: _,
                enchantment: _,
            },
        kind: _,
    } = item;
};
list_record!(@impl ItemRecord, lists [common.keywords: "keyword"],
    fields [common.editor_id: "editor ID", common.full_name: "name", common.model_path: "model",
            common.icon_path: "icon", common.script_form_id: "script", common.value: "value",
            common.weight: "weight", common.has_script: "has script", common.script_instance: "script properties",
            common.enchantment: "enchantment", kind: "type data" by ItemKind::same_authored],
    maps [Item => items]);

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
    /// Per list (by what its entries are called) and form whose entries a contributor changed: the winning operation
    /// and those it overruled. A wholesale winner's operations are what it changed in the record it replaced.
    pub entries: Vec<((&'static str, u32), Resolution<EntryOp>)>,
    /// Per field a contributor changed, by its name, likewise.
    pub fields: Vec<(&'static str, Resolution<()>)>,
    /// Overrides identical to the record they would replace (LMC-REC-006): no-ops, which take no ownership.
    pub no_ops: Vec<u32>,
    /// Independent overrides identical to the record they override (their base), which therefore change nothing
    /// under `merge`; under `legacy` such an override wins the record whole and undoes the changes before it.
    pub unchanged: Vec<u32>,
    /// The merged record differs from the last carrier's own record, the one `legacy` keeps.
    pub legacy_differs: bool,
}

impl ListMergeRecord {
    /// Whether more than one plugin's changes are in the record, or the record differs from what `legacy` keeps.
    pub fn is_merged(&self) -> bool {
        self.contributors.len() > 1 || self.legacy_differs
    }

    /// The plugin that owns the record: the last contributor. A later no-op or unchanged override takes no ownership
    /// (LMC-REC-006), so the owner is not always the last carrier.
    pub fn owner(&self) -> u32 {
        *self.contributors.last().expect("a merge record starts with the record it merged from")
    }

    /// Whether the plugin at `position` overrode the record without taking ownership: a no-op, or an independent
    /// override that changes nothing (LMC-REC-006).
    pub fn took_no_ownership(&self, position: u32) -> bool {
        self.no_ops.contains(&position) || self.unchanged.contains(&position)
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

/// Whether `a` and `b` are the same record as the merge reads them: every field equal, every list holding the same
/// entries per form (order aside).
fn same_record<R: ListRecord>(a: &R, b: &R) -> bool {
    R::FIELDS.iter().all(|field| (field.same)(a, b)) && R::LISTS.iter().all(|list| (list.diff)(a, b).is_empty())
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
                no_ops: Vec::new(),
                unchanged: Vec::new(),
                legacy_differs: false,
            },
            carriers: vec![(previous, record.clone())],
            current: record.clone(),
        }
    }

    /// Fold `incoming`, the record of the plugin at `position`. `true` when the result is a merge the fold must write
    /// back over the plugin's own record; `false` when the plugin won the record whole.
    fn fold(&mut self, position: u32, incoming: &R, depends_on: &dyn Fn(u32, u32) -> bool) -> bool {
        // LMC-REC-006 — an override identical to the record it would replace is a no-op: it takes no ownership, so
        // it neither wins the record whole nor becomes a contributor a later patch must master.
        if same_record(&self.current, incoming) {
            self.record.no_ops.push(position);
            self.carriers.push((position, incoming.clone()));
            return true;
        }
        let contributors = &self.record.contributors;
        if contributors.iter().all(|&contributor| depends_on(position, contributor)) {
            // A dependent override wins whole. What it changed in the list it replaced is still its operations, so
            // a later independent override that undoes one of them overrules it in the report.
            self.record.wholesale = Some((position, contributors.clone()));
            self.record.contributors = vec![position];
            self.record.entries = R::LISTS
                .iter()
                .flat_map(|list| {
                    (list.diff)(&self.current, incoming).into_iter().map(|(form, op)| {
                        ((list.entry, form), Resolution { winner: position, op, overruled: Vec::new() })
                    })
                })
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
        let mut changed = false;
        for list in R::LISTS {
            for (form, op) in (list.fold)(&base, incoming, &mut self.current) {
                decide(&mut self.record.entries, (list.entry, form), position, op);
                changed = true;
            }
        }
        for field in R::FIELDS {
            if !(field.same)(&base, incoming) && !(field.same)(&self.current, incoming) {
                (field.copy)(incoming, &mut self.current);
                decide(&mut self.record.fields, field.name, position, ());
                changed = true;
            }
        }
        // An override that decided nothing holds none of the record's changes: it is not a contributor.
        if changed {
            self.record.contributors.push(position);
        } else {
            self.record.unchanged.push(position);
        }
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
        if self.0.is_empty() {
            return;
        }
        for write in self.0 {
            write(merged);
        }
        // A merged item is written after the plugin's merge classified the items, as its own record's kind.
        merged.classify_fallout_inventory_kinds();
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
    actors: States<NpcRecord>,
    races: States<RaceRecord>,
    items: States<ItemRecord>,
    object_keywords: States<Vec<u32>>,
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
            let write_back = state.fold(position, incoming, depends_on);
            // `legacy` keeps this plugin's own record whole; the merge reports where it does not.
            state.record.legacy_differs = !same_record(&state.current, incoming);
            if write_back {
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
        note_records(&mut self.actors, position, plugin, merged, provenance, depends_on, &mut pending);
        note_records(&mut self.races, position, plugin, merged, provenance, depends_on, &mut pending);
        note_records(&mut self.items, position, plugin, merged, provenance, depends_on, &mut pending);
        note_records(&mut self.object_keywords, position, plugin, merged, provenance, depends_on, &mut pending);
        pending
    }

    /// How `form` was merged, when it is a record that merges and more than one plugin carried it: one entry, or two
    /// for a base whose keyword list is kept apart from its record (a container's contents and its keywords).
    pub fn records(&self, form: u32) -> impl Iterator<Item = &ListMergeRecord> {
        [
            state_of(&self.leveled, form),
            state_of(&self.form_lists, form),
            state_of(&self.outfits, form),
            state_of(&self.containers, form),
            state_of(&self.actors, form),
            state_of(&self.races, form),
            state_of(&self.items, form),
            state_of(&self.object_keywords, form),
        ]
        .into_iter()
        .flatten()
    }

    /// The records that hold more than one plugin's changes, in no particular order.
    pub fn merged(&self) -> impl Iterator<Item = (u32, &ListMergeRecord)> {
        records_in(&self.leveled)
            .chain(records_in(&self.form_lists))
            .chain(records_in(&self.outfits))
            .chain(records_in(&self.containers))
            .chain(records_in(&self.actors))
            .chain(records_in(&self.races))
            .chain(records_in(&self.items))
            .chain(records_in(&self.object_keywords))
            .filter(|(_, record)| record.is_merged())
    }

    /// How many forms hold more than one plugin's changes, and how many operations were overruled in them.
    pub fn counts(&self) -> (usize, usize) {
        let (mut forms, mut overruled) = (std::collections::HashSet::new(), 0);
        for (form, record) in self.merged() {
            forms.insert(form);
            overruled += record.overruled();
        }
        (forms.len(), overruled)
    }
}

#[cfg(test)]
#[path = "list_merge_tests.rs"]
mod tests;
