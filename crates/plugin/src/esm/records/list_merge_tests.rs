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
            index.record_types.insert(LIST, kind.tag().as_bytes().try_into().unwrap());
            R::records_mut(kind, &mut index).insert(LIST, list);
        }
        provenance.note_plugin(position, &index, &merged, |id| Some((id >> 24) as usize));
        let pending = merge.note_plugin(position, &index, &merged, &provenance, &depends_on);
        merged.merge_from(index);
        pending.apply(&mut merged);
    }
    (R::records(kind, &merged)[&LIST].clone(), merge)
}

fn forms(list: &LeveledList) -> Vec<(u32, u16, u16)> {
    list.entries.iter().map(|entry| (entry.form_id, entry.level, entry.count)).collect()
}

fn resolution(merge: &ListMerge, form: u32) -> &Resolution<EntryOp> {
    &merge.record(LIST).unwrap().entries.iter().find(|(f, _)| *f == form).unwrap().1
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
    let record = merge.record(LIST).unwrap();
    assert_eq!(record.contributors, [1, 2], "the first override masters the definer and replaced its record whole");
    assert_eq!(record.wholesale, Some((1, vec![0])));
    assert_eq!(resolution(&merge, SHIELD), &Resolution { winner: 1, op: EntryOp::Add, overruled: vec![] });
    assert_eq!(resolution(&merge, BOW), &Resolution { winner: 2, op: EntryOp::Add, overruled: vec![] });
    assert_eq!(merge.counts(), (1, 0));

    let (legacy, merge) = fold(MergePolicy::Legacy, plugins());
    assert_eq!(forms(&legacy), [(SWORD, 1, 1), (BOW, 8, 1)], "legacy keeps the last record whole");
    assert!(merge.record(LIST).is_none());
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
    assert!(merge.record(LIST).unwrap().entries.iter().all(|(form, _)| *form != SWORD), "reordering is no change");
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
    let record = merge.record(LIST).unwrap();
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
    assert_eq!(merge.record(LIST).unwrap().contributors, [1, 2, 3]);
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
    let fields = &merge.record(LIST).unwrap().fields;
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
    let record = merge.record(LIST).unwrap();
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
    assert!(merge.record(LIST).unwrap().entries.iter().all(|(form, _)| *form != NULL));
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

#[test]
fn the_policy_reads_its_name() {
    assert_eq!(MergePolicy::parse("Legacy"), Some(MergePolicy::Legacy));
    assert_eq!(MergePolicy::parse("merge").map(MergePolicy::name), Some("merge"));
    assert_eq!(MergePolicy::parse("smash"), None);
    assert_eq!(MergePolicy::default(), MergePolicy::Merge, "decision Q2: merge by default");
}
