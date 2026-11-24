//! #7407 — Hand Equipment's slot model measured on the real Skyrim SE load order. The unit tests build their EQUP
//! records and weapons by hand; this one folds the official plugins through the production path and checks what the
//! hand rules rest on: the EQUP hand slots, the ARMO `ETYP` shields of every official plugin and the WEAP `ETYP`
//! both-hands classification. (Every DLC shield names `Skyrim.esm`'s Shield slot through master index 0, which the
//! official order maps to itself, so this order does not exercise a non-identity `ETYP` remap; the unit test
//! `weap_etyp_equip_type_is_remapped` does.)

use super::deletion_tests::paths_in;
use super::parse_record_indexes_in_load_order;
use byroredux_plugin::esm::records::{slot_hands, ItemKind, SlotHands};
use byroredux_scripting::hands::HandCatalog;

#[test]
#[ignore = "parses Skyrim.esm and its DLC; run with --ignored"]
fn skyrim_hand_slots_shields_and_two_handers_decode_from_the_official_load_order() {
    let names = ["Skyrim.esm", "Update.esm", "Dawnguard.esm", "HearthFires.esm", "Dragonborn.esm"];
    let Some(paths) = paths_in(byroredux_plugin::esm::test_paths::skyrim_se_data_dir(), &names) else {
        byroredux_plugin::esm::test_paths::require_or_skip("Skyrim SE with its DLC");
        return;
    };
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let (index, _order) = parse_record_indexes_in_load_order(&refs).expect("fold");

    let slot = |editor_id: &str| {
        *index.equip_slots.iter().find(|(_, record)| record.editor_id == editor_id).unwrap_or_else(|| panic!("EQUP {editor_id}")).0
    };
    assert_eq!(index.equip_slots.len(), 7, "Skyrim.esm authors seven EQUP slots and no DLC adds one");
    for (name, hands) in [("RightHand", SlotHands::Right), ("LeftHand", SlotHands::Left), ("EitherHand", SlotHands::Either), ("BothHands", SlotHands::Both), ("Shield", SlotHands::Left)] {
        assert_eq!(slot_hands(&index.equip_slots, slot(name)), hands, "{name}");
    }

    let catalog = HandCatalog::from_index(&index);
    let mut shields_by_plugin = [0usize; 256];
    let mut both = 0usize;
    for (&form, item) in &index.items {
        match &item.kind {
            ItemKind::Armor { equip_type, .. } if *equip_type == slot("Shield") => {
                assert!(catalog.is_shield(form), "{form:08X} names the Shield slot and must be a catalog shield");
                shields_by_plugin[(form >> 24) as usize] += 1;
            }
            ItemKind::Weapon { .. } if catalog.two_handed(form) => both += 1,
            _ => {}
        }
    }
    // By load-order slot: Skyrim.esm 237 (Update.esm's two shield records override Skyrim.esm forms), Dawnguard 7,
    // Dragonborn 66. A broken ARMO `ETYP` decode, EQUP decode or `slot_hands` drops them all.
    assert_eq!(&shields_by_plugin[..5], &[237, 0, 7, 0, 66], "shields per plugin, measured 2026-11-23");
    assert_eq!(both, 1526, "weapons whose ETYP slot takes both hands, measured 2026-11-23");

    // These stock styles disagree between the record header and DATA. DATA is the authored gate.
    assert!(index.combat_styles[&0x0001_6E25].allows_dual_wield, "csAlikrBerserker sets DATA only");
    assert!(!index.combat_styles[&0x0010_611C].allows_dual_wield, "csWEMageShieldStaff sets the header bit only");
    let delphine = &index.npcs[&0x0001_3478];
    assert_eq!(delphine.combat_style_form_id, Some(0x0010_D4BE), "Delphine's ZNAM resolves to csDelphine");
    assert!(index.combat_styles[&0x0010_D4BE].allows_dual_wield);
    assert!(index.races[&delphine.race_form_id].skyrim_can_dual_wield());
}
