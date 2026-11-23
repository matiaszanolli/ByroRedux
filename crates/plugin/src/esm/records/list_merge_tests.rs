use super::*;

const LIST: u32 = 0x0000_0100;
const SWORD: u32 = 0x0000_0A01;
const SHIELD: u32 = 0x0000_0A02;
const BOW: u32 = 0x0000_0A03;
const AXE: u32 = 0x0000_0A04;

fn entry(form_id: u32, level: u16, count: u16) -> LeveledEntry {
    LeveledEntry { level, form_id, count }
}

fn list(entries: &[LeveledEntry]) -> LeveledList {
    LeveledList { form_id: LIST, editor_id: "LItemSwords".into(), chance_none: 0, flags: 0, entries: entries.to_vec() }
}

/// A plugin carrying `list` as its record of form `LIST`, declaring the plugins at `masters` as its masters.
struct Plugin<R = LeveledList> {
    masters: Vec<u32>,
    list: Option<R>,
}

fn plugin<R>(masters: &[u32], list: Option<R>) -> Plugin<R> {
    Plugin { masters: masters.to_vec(), list }
}

/// Fold levelled-list `plugins` in order under `policy`, the first defining `LIST`, and return the folded list and the
/// merge.
fn fold(policy: MergePolicy, plugins: Vec<Plugin>) -> (LeveledList, ListMerge) {
    fold_records(policy, ListKind::LeveledItems, plugins)
}

/// [`fold`] for records of any type that merges, as `kind`.
fn fold_records<R: ListRecord>(policy: MergePolicy, kind: ListKind, plugins: Vec<Plugin<R>>) -> (R, ListMerge) {
    // Each plugin's masters, transitively.
    let mut closure: Vec<std::collections::HashSet<u32>> = Vec::new();
    for plugin in &plugins {
        let mut set = std::collections::HashSet::new();
        for &master in &plugin.masters {
            set.insert(master);
            set.extend(closure[master as usize].iter().copied());
        }
        closure.push(set);
    }
    let depends_on = |a: u32, b: u32| closure[a as usize].contains(&b);
    let mut merged = EsmIndex::default();
    let mut provenance = FoldProvenance::default();
    let mut merge = ListMerge::new(policy);
    for (position, plugin) in plugins.into_iter().enumerate() {
        let mut index = EsmIndex::default();
        if let Some(list) = plugin.list {
            index.record_types.insert(LIST, *record_type(kind));
            R::records_mut(kind, &mut index).insert(LIST, list);
        }
        provenance.note_plugin(position, &index, &merged, |id| Some((id >> 24) as usize));
        let pending = merge.note_plugin(position, &index, &merged, &provenance, &depends_on);
        merged.merge_from(index);
        pending.apply(&mut merged);
    }
    (R::records(kind, &merged)[&LIST].clone(), merge)
}

/// A record type `kind`'s map holds.
fn record_type(kind: ListKind) -> &'static [u8; 4] {
    match kind {
        ListKind::LeveledItems => b"LVLI",
        ListKind::LeveledNpcs => b"LVLN",
        ListKind::LeveledCreatures => b"LVLC",
        ListKind::LeveledSpells => b"LVSP",
        ListKind::FormList => b"FLST",
        ListKind::Outfit => b"OTFT",
        ListKind::Container => b"CONT",
        ListKind::Npc => b"NPC_",
        ListKind::Creature => b"CREA",
        ListKind::Race => b"RACE",
        ListKind::Item => b"ARMO",
        ListKind::ObjectKeywords => b"CONT",
    }
}

fn forms(list: &LeveledList) -> Vec<(u32, u16, u16)> {
    list.entries.iter().map(|entry| (entry.form_id, entry.level, entry.count)).collect()
}

/// How `LIST` was merged.
fn record(merge: &ListMerge) -> &ListMergeRecord {
    merge.records(LIST).next().expect("LIST merged")
}

/// The resolution of `form` in the list whose entries are called `list`.
fn resolution_in<'a>(merge: &'a ListMerge, list: &str, form: u32) -> &'a Resolution<EntryOp> {
    &record(&merge).entries.iter().find(|((l, f), _)| *l == list && *f == form).unwrap().1
}

fn resolution(merge: &ListMerge, form: u32) -> &Resolution<EntryOp> {
    resolution_in(merge, "entry", form)
}

/// Two mods that each add a form to a vanilla list both keep their addition, which `legacy` loses.
#[test]
fn independent_additions_both_survive() {
    let base = list(&[entry(SWORD, 1, 1)]);
    let plugins = || {
        vec![
            plugin(&[], Some(base.clone())),
            plugin(&[0], Some(list(&[entry(SWORD, 1, 1), entry(SHIELD, 5, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 1, 1), entry(BOW, 8, 1)]))),
        ]
    };
    let (merged, merge) = fold(MergePolicy::Merge, plugins());
    assert_eq!(forms(&merged), [(SWORD, 1, 1), (SHIELD, 5, 1), (BOW, 8, 1)]);
    let record = record(&merge);
    assert_eq!(record.contributors, [1, 2], "the first override masters the definer and replaced its record whole");
    assert_eq!(record.wholesale, Some((1, vec![0])));
    assert_eq!(resolution(&merge, SHIELD), &Resolution { winner: 1, op: EntryOp::Add, overruled: vec![] });
    assert_eq!(resolution(&merge, BOW), &Resolution { winner: 2, op: EntryOp::Add, overruled: vec![] });
    assert_eq!(merge.counts(), (1, 0));

    let (legacy, merge) = fold(MergePolicy::Legacy, plugins());
    assert_eq!(forms(&legacy), [(SWORD, 1, 1), (BOW, 8, 1)], "legacy keeps the last record whole");
    assert!(merge.records(LIST).next().is_none());
}

/// Two plugins re-levelling the same form conflict; the later wins and the form is in the list once.
#[test]
fn a_relevel_is_one_change_and_the_later_plugin_wins_it() {
    let (merged, merge) = fold(
        MergePolicy::Merge,
        vec![
            plugin(&[], Some(list(&[entry(SWORD, 5, 1), entry(AXE, 3, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 10, 1), entry(AXE, 3, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 20, 1), entry(AXE, 3, 1)]))),
        ],
    );
    assert_eq!(forms(&merged), [(SWORD, 20, 1), (AXE, 3, 1)]);
    assert_eq!(resolution(&merge, SWORD), &Resolution { winner: 2, op: EntryOp::Change, overruled: vec![(1, EntryOp::Change)] });
    assert_eq!(merge.counts(), (1, 1));
}

/// A removal and an addition by independent plugins combine; a form listed twice on purpose stays twice.
#[test]
fn removals_combine_and_duplicates_are_a_multiset() {
    let (merged, merge) = fold(
        MergePolicy::Merge,
        vec![
            plugin(&[], Some(list(&[entry(SWORD, 1, 1), entry(SWORD, 1, 1), entry(AXE, 3, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 1, 1), entry(SWORD, 1, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 1, 1), entry(AXE, 3, 1), entry(SWORD, 1, 1), entry(BOW, 2, 1)]))),
        ],
    );
    assert_eq!(forms(&merged), [(SWORD, 1, 1), (SWORD, 1, 1), (BOW, 2, 1)]);
    assert_eq!(resolution(&merge, AXE).op, EntryOp::Remove);
    assert!(record(&merge).entries.iter().all(|((_, form), _)| *form != SWORD), "reordering is no change");
}

/// A patch that masters every contributor wins the list whole, as under `legacy`; an ordinary dependent override
/// does too.
#[test]
fn a_patch_over_every_contributor_wins_the_list_whole() {
    let patch = list(&[entry(SWORD, 1, 1), entry(SHIELD, 5, 1)]);
    let (merged, merge) = fold(
        MergePolicy::Merge,
        vec![
            plugin(&[], Some(list(&[entry(SWORD, 1, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 1, 1), entry(SHIELD, 5, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 1, 1), entry(BOW, 8, 1)]))),
            plugin(&[0, 1, 2], Some(patch.clone())),
        ],
    );
    assert_eq!(forms(&merged), forms(&patch), "the patch dropped the bow on purpose");
    let record = record(&merge);
    assert_eq!(record.wholesale, Some((3, vec![1, 2])));
    assert!(!record.is_merged());

    let (merged, merge) = fold(
        MergePolicy::Merge,
        vec![plugin(&[], Some(list(&[entry(SWORD, 1, 1)]))), plugin(&[0], Some(list(&[entry(BOW, 2, 1)])))],
    );
    assert_eq!(forms(&merged), [(BOW, 2, 1)], "a single override is the plain legacy override");
    assert_eq!(merge.counts(), (0, 0));
}

/// A patch over one of two independent mods diffs against that mod's record: it undoes that mod's addition and keeps
/// the other mod's.
#[test]
fn a_partial_patch_diffs_against_the_master_it_overrides() {
    let (merged, merge) = fold(
        MergePolicy::Merge,
        vec![
            plugin(&[], Some(list(&[entry(SWORD, 1, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 1, 1), entry(SHIELD, 5, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 1, 1), entry(BOW, 8, 1)]))),
            plugin(&[0, 1], Some(list(&[entry(SWORD, 1, 1)]))),
        ],
    );
    assert_eq!(forms(&merged), [(SWORD, 1, 1), (BOW, 8, 1)]);
    assert_eq!(resolution(&merge, SHIELD), &Resolution { winner: 3, op: EntryOp::Remove, overruled: vec![(1, EntryOp::Add)] });
    assert_eq!(record(&merge).contributors, [1, 2, 3]);
}

/// LMC-REC-006 — an override identical to the record it would replace is a no-op: it takes no ownership, so the
/// record is not a merge and a later patch need not master it.
#[test]
fn an_identical_override_is_a_no_op_and_takes_no_ownership() {
    let modded = list(&[entry(SWORD, 1, 1), entry(SHIELD, 5, 1)]);
    let (merged, merge) = fold(
        MergePolicy::Merge,
        vec![
            plugin(&[], Some(list(&[entry(SWORD, 1, 1)]))),
            plugin(&[0], Some(modded.clone())),
            plugin(&[0, 1], Some(modded.clone())),
        ],
    );
    assert_eq!(forms(&merged), forms(&modded));
    let record = record(&merge);
    assert_eq!(record.no_ops, [2]);
    assert_eq!(record.contributors, [1], "the no-op does not take the record");
    assert_eq!(record.wholesale, Some((1, vec![0])));
    assert!(!record.is_merged(), "the record is what legacy keeps");
    assert_eq!(merge.counts(), (0, 0));
}

/// An independent override identical to the record it overrides changes nothing under `merge`: it is no contributor,
/// the earlier mod's change stands, and the report marks the record merged because `legacy` would keep the override's
/// record whole. A later patch need master only the contributors to win the record whole.
#[test]
fn an_unchanged_override_contributes_nothing_and_a_patch_need_not_master_it() {
    let (merged, merge) = fold(
        MergePolicy::Merge,
        vec![
            plugin(&[], Some(list(&[entry(SWORD, 1, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 1, 1), entry(SHIELD, 5, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 1, 1)]))),
        ],
    );
    assert_eq!(forms(&merged), [(SWORD, 1, 1), (SHIELD, 5, 1)], "the first mod's addition stands");
    let state = record(&merge);
    assert_eq!(state.unchanged, [2]);
    assert_eq!(state.contributors, [1]);
    assert!(state.legacy_differs && state.is_merged());
    assert_eq!(merge.counts(), (1, 0));

    let (merged, merge) = fold(
        MergePolicy::Merge,
        vec![
            plugin(&[], Some(list(&[entry(SWORD, 1, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 1, 1), entry(SHIELD, 5, 1)]))),
            plugin(&[0], Some(list(&[entry(SWORD, 1, 1)]))),
            plugin(&[0, 1], Some(list(&[entry(SHIELD, 5, 1)]))),
        ],
    );
    assert_eq!(forms(&merged), [(SHIELD, 5, 1)], "the patch masters every contributor and wins whole");
    assert_eq!(record(&merge).wholesale, Some((3, vec![1])));
}

/// A field an override changed against its base is an operation; one it left alone is not, so it does not undo an
/// earlier plugin's change.
#[test]
fn fields_merge_by_what_each_override_changed() {
    let chance = |chance_none: u8, entries: &[LeveledEntry]| LeveledList { chance_none, ..list(entries) };
    let (merged, merge) = fold(
        MergePolicy::Merge,
        vec![
            plugin(&[], Some(chance(0, &[entry(SWORD, 1, 1)]))),
            plugin(&[0], Some(chance(50, &[entry(SWORD, 1, 1)]))),
            plugin(&[0], Some(chance(0, &[entry(SWORD, 1, 1), entry(BOW, 8, 1)]))),
        ],
    );
    assert_eq!(merged.chance_none, 50);
    assert_eq!(forms(&merged), [(SWORD, 1, 1), (BOW, 8, 1)]);
    let fields = &record(&merge).fields;
    assert_eq!(fields, &[("chance none", Resolution { winner: 1, op: (), overruled: vec![] })]);
}

fn container(name: &str, contents: &[(u32, i32)]) -> ContainerRecord {
    ContainerRecord {
        form_id: LIST,
        editor_id: "MerchantChest".into(),
        full_name: name.into(),
        model_path: String::new(),
        weight: 0.0,
        flags: 0,
        open_sound: 0,
        close_sound: 0,
        script_form_id: 0,
        script_instance: None,
        contents: contents.iter().map(|&(item_form_id, count)| InventoryEntry { item_form_id, count }).collect(),
    }
}

/// Two DLC that each stock a vanilla container keep both stocks; a recount is a change, and a renamed container keeps
/// the other plugin's stock.
#[test]
fn containers_merge_their_contents_and_fields() {
    let (merged, merge) = fold_records(
        MergePolicy::Merge,
        ListKind::Container,
        vec![
            plugin(&[], Some(container("Chest", &[(SWORD, 1)]))),
            plugin(&[0], Some(container("Chest", &[(SWORD, 3), (SHIELD, 1)]))),
            plugin(&[0], Some(container("Strongbox", &[(SWORD, 1), (BOW, 2)]))),
        ],
    );
    let contents: Vec<(u32, i32)> = merged.contents.iter().map(|entry| (entry.item_form_id, entry.count)).collect();
    assert_eq!(contents, [(SWORD, 3), (SHIELD, 1), (BOW, 2)]);
    assert_eq!(merged.full_name, "Strongbox");
    let record = record(&merge);
    assert_eq!(record.kind, ListKind::Container);
    assert_eq!(resolution(&merge, SWORD).op, EntryOp::Change);
    assert_eq!(record.fields, [("name", Resolution { winner: 2, op: (), overruled: vec![] })]);
}

fn form_list(entries: &[u32]) -> FlstRecord {
    FlstRecord { form_id: LIST, editor_id: "WeapTypeList".into(), entries: entries.to_vec() }
}

/// A form list keeps its untouched entries at their indices and its empty slots where they were; additions append.
#[test]
fn form_lists_append_and_keep_their_slots() {
    const NULL: u32 = 0xFFFF_FFFF;
    let (merged, merge) = fold_records(
        MergePolicy::Merge,
        ListKind::FormList,
        vec![
            plugin(&[], Some(form_list(&[SWORD, NULL, AXE]))),
            plugin(&[0], Some(form_list(&[SWORD, NULL, AXE, SHIELD]))),
            plugin(&[0], Some(form_list(&[SWORD, NULL, NULL, AXE, BOW]))),
        ],
    );
    assert_eq!(merged.entries, [SWORD, NULL, AXE, SHIELD, BOW], "a null slot is never an operation");
    assert!(record(&merge).entries.iter().all(|((_, form), _)| *form != NULL));
    assert_eq!(merge.counts(), (1, 0));
}

/// An outfit is a form list by another name: independent additions combine and a removal holds.
#[test]
fn outfits_merge_their_items() {
    let outfit = |items: &[u32]| OtftRecord { form_id: LIST, editor_id: "GuardOutfit".into(), items: items.to_vec() };
    let (merged, _) = fold_records(
        MergePolicy::Merge,
        ListKind::Outfit,
        vec![
            plugin(&[], Some(outfit(&[SWORD, SHIELD]))),
            plugin(&[0], Some(outfit(&[SWORD]))),
            plugin(&[0], Some(outfit(&[SWORD, SHIELD, BOW]))),
        ],
    );
    assert_eq!(merged.items, [SWORD, BOW]);
}

const GUARDS: u32 = 0x0000_0F01;
const MERCHANTS: u32 = 0x0000_0F02;
const FIREBALL: u32 = 0x0000_0C01;
const HEAL: u32 = 0x0000_0C02;
const ARMOR_HEAVY: u32 = 0x0000_0D01;
const VENDOR_ITEM: u32 = 0x0000_0D02;

fn npc(level: i16, inventory: &[(u32, i32)], factions: &[(u32, i8)], spells: &[u32]) -> NpcRecord {
    NpcRecord {
        form_id: LIST,
        editor_id: "Guard".into(),
        level,
        inventory: inventory.iter().map(|&(item_form_id, count)| NpcInventoryEntry { item_form_id, count }).collect(),
        factions: factions.iter().map(|&(faction_form_id, rank)| FactionMembership { faction_form_id, rank }).collect(),
        spells: spells.to_vec(),
        ..NpcRecord::default()
    }
}

/// An actor merges each of its lists per form, and its scalar fields by what each override changed: one mod's
/// equipment and promotion and another's spell and level all stand.
#[test]
fn actors_merge_each_list_and_their_fields() {
    let (merged, merge) = fold_records(
        MergePolicy::Merge,
        ListKind::Npc,
        vec![
            plugin(&[], Some(npc(5, &[(SWORD, 1)], &[(GUARDS, 0)], &[]))),
            plugin(&[0], Some(npc(5, &[(SWORD, 1), (SHIELD, 1)], &[(GUARDS, 2)], &[]))),
            plugin(&[0], Some(npc(10, &[(SWORD, 1)], &[(GUARDS, 0), (MERCHANTS, 0)], &[FIREBALL]))),
        ],
    );
    let inventory: Vec<(u32, i32)> = merged.inventory.iter().map(|entry| (entry.item_form_id, entry.count)).collect();
    assert_eq!(inventory, [(SWORD, 1), (SHIELD, 1)]);
    let factions: Vec<(u32, i8)> = merged.factions.iter().map(|f| (f.faction_form_id, f.rank)).collect();
    assert_eq!(factions, [(GUARDS, 2), (MERCHANTS, 0)], "the promotion and the new membership both stand");
    assert_eq!(merged.spells, [FIREBALL]);
    assert_eq!(merged.level, 10);
    assert_eq!(resolution_in(&merge, "inventory", SHIELD).op, EntryOp::Add);
    assert_eq!(resolution_in(&merge, "faction", GUARDS), &Resolution { winner: 1, op: EntryOp::Change, overruled: vec![] });
    assert_eq!(resolution_in(&merge, "spell", FIREBALL).winner, 2);
    let record = record(&merge);
    assert_eq!(record.kind, ListKind::Npc);
    assert_eq!(record.fields, [("level", Resolution { winner: 2, op: (), overruled: vec![] })]);

    let (legacy, _) = fold_records(
        MergePolicy::Legacy,
        ListKind::Npc,
        vec![
            plugin(&[], Some(npc(5, &[(SWORD, 1)], &[(GUARDS, 0)], &[]))),
            plugin(&[0], Some(npc(5, &[(SWORD, 1), (SHIELD, 1)], &[(GUARDS, 2)], &[]))),
            plugin(&[0], Some(npc(10, &[(SWORD, 1)], &[(GUARDS, 0), (MERCHANTS, 0)], &[FIREBALL]))),
        ],
    );
    assert_eq!(legacy.inventory.len(), 1, "legacy keeps the last record whole");
}

/// A creature is an actor in its own map.
#[test]
fn creatures_merge_as_actors() {
    let creature = |spells: &[u32]| NpcRecord { is_creature: true, ..npc(1, &[], &[], spells) };
    let (merged, merge) = fold_records(
        MergePolicy::Merge,
        ListKind::Creature,
        vec![
            plugin(&[], Some(creature(&[FIREBALL]))),
            plugin(&[0], Some(creature(&[]))),
            plugin(&[0], Some(creature(&[FIREBALL, HEAL]))),
        ],
    );
    assert_eq!(merged.spells, [HEAL], "the removal and the addition combine");
    assert_eq!(record(&merge).kind, ListKind::Creature);
}

/// A race merges its spells and keywords; a field one mod changed holds against another that left it alone.
#[test]
fn races_merge_spells_and_keywords() {
    let race = |height: f32, spells: &[u32], keywords: &[u32]| RaceRecord {
        form_id: LIST,
        editor_id: "NordRace".into(),
        base_height: (height, height),
        spells: spells.to_vec(),
        keywords: keywords.to_vec(),
        ..RaceRecord::default()
    };
    let (merged, merge) = fold_records(
        MergePolicy::Merge,
        ListKind::Race,
        vec![
            plugin(&[], Some(race(1.0, &[FIREBALL], &[ARMOR_HEAVY]))),
            plugin(&[0], Some(race(1.1, &[FIREBALL, HEAL], &[ARMOR_HEAVY]))),
            plugin(&[0], Some(race(1.0, &[FIREBALL], &[]))),
        ],
    );
    assert_eq!(merged.spells, [FIREBALL, HEAL]);
    assert!(merged.keywords.is_empty());
    assert_eq!(merged.base_height, (1.1, 1.1));
    assert_eq!(resolution_in(&merge, "keyword", ARMOR_HEAVY).op, EntryOp::Remove);
}

fn item(name: &str, weight: f32, keywords: &[u32], kind: ItemKind) -> ItemRecord {
    ItemRecord {
        form_id: LIST,
        common: CommonItemFields {
            editor_id: "Wrench".into(),
            full_name: name.into(),
            weight,
            keywords: keywords.to_vec(),
            ..CommonItemFields::default()
        },
        kind,
    }
}

/// An item merges its keyword list and its fields: a rename, a weight change and a keyword added by three
/// independent mods all stand. A loose object mod the fold promoted is the kind its record authored, so an override
/// that carries the record's own kind changes nothing.
#[test]
fn items_merge_keywords_and_fields() {
    let promoted = ItemKind::Mod { was_junk: false };
    let (merged, merge) = fold_records(
        MergePolicy::Merge,
        ListKind::Item,
        vec![
            plugin(&[], Some(item("Wrench", 2.0, &[], promoted.clone()))),
            plugin(&[0], Some(item("[Junk] Wrench", 2.0, &[], ItemKind::Misc))),
            plugin(&[0], Some(item("Wrench", 0.0, &[], ItemKind::Misc))),
            plugin(&[0], Some(item("Wrench", 2.0, &[VENDOR_ITEM], ItemKind::Misc))),
        ],
    );
    assert_eq!(merged.common.full_name, "[Junk] Wrench");
    assert_eq!(merged.common.weight, 0.0);
    assert_eq!(merged.common.keywords, [VENDOR_ITEM]);
    let record = record(&merge);
    assert_eq!(record.contributors, [1, 2, 3]);
    let fields: Vec<&str> = record.fields.iter().map(|(name, _)| *name).collect();
    assert_eq!(fields, ["name", "weight"], "a promoted kind is not a change");
    assert!(ItemKind::same_authored(&promoted, &ItemKind::Misc));
    assert!(!ItemKind::same_authored(&ItemKind::Mod { was_junk: true }, &ItemKind::Misc));
}

const DUNGEON: u32 = 0x0000_0E01;
const CLEARED: u32 = 0x0000_0E02;
const MARKED: u32 = 0x0000_0E03;
/// A base object's keyword list merges on its own; an override that drops every keyword carries an empty list,
/// which removes them under both policies.
#[test]
fn object_keyword_lists_merge_and_an_empty_list_removes() {
    let (merged, merge) = fold_records(
        MergePolicy::Merge,
        ListKind::ObjectKeywords,
        vec![
            plugin(&[], Some(vec![DUNGEON, MARKED])),
            plugin(&[0], Some(vec![])),
            plugin(&[0], Some(vec![DUNGEON, MARKED, CLEARED])),
        ],
    );
    assert_eq!(merged, [CLEARED], "the first mod's removals and the second's addition combine");
    assert_eq!(resolution_in(&merge, "keyword", DUNGEON).op, EntryOp::Remove);
    assert_eq!(record(&merge).kind, ListKind::ObjectKeywords);

    let (legacy, _) = fold_records(
        MergePolicy::Legacy,
        ListKind::ObjectKeywords,
        vec![plugin(&[], Some(vec![DUNGEON])), plugin(&[0], Some(vec![]))],
    );
    assert!(legacy.is_empty(), "the override's empty list replaces the master's");
}

/// A container's keyword list is kept apart from its record, so a container can merge twice; it counts once.
#[test]
fn a_container_and_its_keywords_merge_apart_and_count_once() {
    let mut merge = ListMerge::new(MergePolicy::Merge);
    let mut closure: Vec<Vec<u32>> = Vec::new();
    let mut merged = EsmIndex::default();
    let mut provenance = FoldProvenance::default();
    let plugins = [
        (vec![], container("Chest", &[(SWORD, 1)]), vec![DUNGEON]),
        (vec![0], container("Chest", &[(SWORD, 1), (SHIELD, 1)]), vec![DUNGEON]),
        (vec![0], container("Chest", &[(SWORD, 1), (BOW, 1)]), vec![DUNGEON, CLEARED]),
    ];
    for (position, (masters, contents, keywords)) in plugins.into_iter().enumerate() {
        closure.push(masters);
        let mut index = EsmIndex::default();
        index.record_types.insert(LIST, *b"CONT");
        index.containers.insert(LIST, contents);
        index.placed_object_keywords.insert(LIST, keywords);
        provenance.note_plugin(position, &index, &merged, |id| Some((id >> 24) as usize));
        let depends_on = |a: u32, b: u32| closure[a as usize].contains(&b);
        let pending = merge.note_plugin(position, &index, &merged, &provenance, &depends_on);
        merged.merge_from(index);
        pending.apply(&mut merged);
    }
    let kinds: Vec<ListKind> = merge.records(LIST).map(|record| record.kind).collect();
    assert_eq!(kinds, [ListKind::Container, ListKind::ObjectKeywords]);
    assert_eq!(merged.placed_object_keywords[&LIST], [DUNGEON, CLEARED]);
    assert_eq!(merged.containers[&LIST].contents.len(), 3);
    assert_eq!(merge.counts(), (1, 0));
}

#[test]
fn the_policy_reads_its_name() {
    assert_eq!(MergePolicy::parse("Legacy"), Some(MergePolicy::Legacy));
    assert_eq!(MergePolicy::parse("merge").map(MergePolicy::name), Some("merge"));
    assert_eq!(MergePolicy::parse("smash"), None);
    assert_eq!(MergePolicy::default(), MergePolicy::Merge, "decision Q2: merge by default");
}
