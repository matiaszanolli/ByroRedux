use super::*;
use crate::esm::records::container::{LeveledEntry, LeveledList};
use crate::esm::records::{NpcInventoryEntry, NpcRecord};

fn list(id: u32, entries: &[(u16, u32)]) -> LeveledList {
    LeveledList {
        form_id: id,
        editor_id: String::new(),
        chance_none: 0,
        flags: 0,
        entries: entries
            .iter()
            .map(|&(level, form_id)| LeveledEntry {
                level,
                form_id,
                count: 1,
            })
            .collect(),
    }
}

fn shell() -> NpcRecord {
    NpcRecord {
        form_id: 1,
        template_form_id: 2,
        template_flags: TEMPLATE_FLAG_USE_TRAITS
            | TEMPLATE_FLAG_USE_STATS
            | TEMPLATE_FLAG_USE_INVENTORY
            | TEMPLATE_FLAG_USE_FACTIONS
            | TEMPLATE_FLAG_USE_AI_PACKAGES,
        ..Default::default()
    }
}

fn fixture(creature: bool) -> (NpcRecord, EsmIndex) {
    let mut index = EsmIndex::default();
    let leaf = NpcRecord {
        form_id: 4,
        race_form_id: 0xD53,
        inventory: vec![NpcInventoryEntry {
            item_form_id: 100,
            count: 1,
        }],
        ..Default::default()
    };
    if creature {
        index.creatures.insert(4, leaf);
        index.leveled_creatures.insert(2, list(2, &[(1, 3)]));
        index.leveled_creatures.insert(3, list(3, &[(1, 4)]));
    } else {
        index.npcs.insert(4, leaf);
        index.leveled_npcs.insert(2, list(2, &[(1, 3)]));
        index.leveled_npcs.insert(3, list(3, &[(1, 4)]));
    }
    (shell(), index)
}

#[test]
fn nested_actor_lists_feed_every_inherited_category() {
    for creature in [false, true] {
        let (npc, index) = fixture(creature);
        assert_eq!(resolve_inherited_traits(&npc, 1, &index).form_id, 4);
        assert_eq!(resolve_inherited_stats(&npc, 1, &index).form_id, 4);
        assert_eq!(resolve_inherited_factions(&npc, 1, &index).form_id, 4);
        assert_eq!(resolve_inherited_ai_packages(&npc, 1, &index).form_id, 4);
        assert_eq!(
            resolve_inherited_inventory(&npc, 1, &index)[0].item_form_id,
            100
        );
    }
}

#[test]
fn nested_lists_keep_level_gate_tie_order_and_category_flag() {
    let (mut npc, mut index) = fixture(false);
    index.npcs.insert(
        5,
        NpcRecord {
            form_id: 5,
            ..Default::default()
        },
    );
    index.npcs.insert(
        6,
        NpcRecord {
            form_id: 6,
            ..Default::default()
        },
    );
    index
        .leveled_npcs
        .insert(3, list(3, &[(1, 4), (8, 5), (8, 6)]));
    assert_eq!(resolve_inherited_traits(&npc, 7, &index).form_id, 4);
    assert_eq!(resolve_inherited_traits(&npc, 8, &index).form_id, 6);
    assert_eq!(resolve_inherited_traits(&npc, 0, &index).form_id, 1);
    npc.template_flags &= !TEMPLATE_FLAG_USE_TRAITS;
    assert_eq!(resolve_inherited_traits(&npc, 8, &index).form_id, 1);
    assert_eq!(resolve_inherited_stats(&npc, 8, &index).form_id, 6);
}

#[test]
fn nested_list_missing_target_and_list_cycle_retain_shell() {
    let (npc, mut index) = fixture(false);
    index.leveled_npcs.insert(3, list(3, &[(1, 999)]));
    assert_eq!(resolve_inherited_traits(&npc, 1, &index).form_id, 1);
    index.leveled_npcs.insert(3, list(3, &[(1, 2)]));
    assert_eq!(resolve_inherited_traits(&npc, 1, &index).form_id, 1);
}

#[test]
fn mixed_npc_list_cycles_share_one_budget() {
    let (npc, mut index) = fixture(false);
    let leaf = index.npcs.get_mut(&4).unwrap();
    leaf.template_form_id = 2;
    leaf.template_flags = TEMPLATE_FLAG_USE_TRAITS;
    let mut visits = 0;
    let result = resolve_inherited_field(&npc, 1, &index, TEMPLATE_FLAG_USE_TRAITS, |n| {
        Some(n.form_id)
    });
    assert_eq!(result, Some(4));
    walk_inherited_records(&npc, 1, &index, TEMPLATE_FLAG_USE_TRAITS, 0, &mut |_| {
        visits += 1
    });
    assert!(visits <= TPLT_MAX_DEPTH + 1);
}

#[test]
fn nested_lists_preserve_intermediate_authored_fields() {
    let (npc, mut index) = fixture(false);
    let middle = index.npcs.get_mut(&4).unwrap();
    middle.template_form_id = 5;
    middle.template_flags = TEMPLATE_FLAG_USE_TRAITS;
    index.leveled_npcs.insert(5, list(5, &[(1, 6)]));
    index.npcs.insert(
        6,
        NpcRecord {
            form_id: 6,
            ..Default::default()
        },
    );
    assert_eq!(resolve_inherited_traits(&npc, 1, &index).form_id, 6);
    assert_eq!(
        resolve_inherited_field(&npc, 1, &index, TEMPLATE_FLAG_USE_TRAITS, |n| (n
            .race_form_id
            != 0)
            .then_some(n.race_form_id)),
        Some(0xD53)
    );
}
