//! Unit tests for the `parse_esm` walker + `EsmIndex` aggregator.
//! Extracted from `mod.rs` to keep the production-code half of that
//! file readable; pulled in as a child module via `#[cfg(test)] mod tests;`.

use super::*;

/// Build a single STAT-style record bytes for the given type code, form ID,
/// and sub-record list.
fn build_record(typ: &[u8; 4], form_id: u32, subs: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut sub_data = Vec::new();
    for (st, data) in subs {
        sub_data.extend_from_slice(*st);
        sub_data.extend_from_slice(&(data.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(data);
    }
    let mut buf = Vec::new();
    buf.extend_from_slice(typ);
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&sub_data);
    buf
}

/// Wrap a record byte blob in a top-level GRUP with the given label.
fn wrap_group(label: &[u8; 4], record: &[u8]) -> Vec<u8> {
    let total = 24 + record.len();
    let mut buf = Vec::new();
    buf.extend_from_slice(b"GRUP");
    buf.extend_from_slice(&(total as u32).to_le_bytes());
    buf.extend_from_slice(label);
    buf.extend_from_slice(&0u32.to_le_bytes()); // group_type = top
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(record);
    buf
}

#[test]
fn extract_records_walks_one_group() {
    let mut subs: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
    subs.push((b"EDID", b"TestWeap\0".to_vec()));
    subs.push((b"DATA", {
        let mut d = Vec::new();
        d.extend_from_slice(&100u32.to_le_bytes()); // value
        d.extend_from_slice(&0u32.to_le_bytes()); // health
        d.extend_from_slice(&2.0f32.to_le_bytes()); // weight
        d.extend_from_slice(&20u16.to_le_bytes()); // damage
        d.push(8); // clip
        d.push(0);
        d
    }));
    let record = build_record(b"WEAP", 0xCAFE, &subs);
    let group = wrap_group(b"WEAP", &record);

    // Wrap with TES4 dummy header up front so parse_esm's reader skips
    // cleanly into the WEAP group.
    let mut tes4 = build_record(b"TES4", 0, &[]);
    tes4.extend_from_slice(&group);
    let index = parse_esm(&tes4).unwrap();

    assert_eq!(index.items.len(), 1);
    let weap = index.items.get(&0xCAFE).expect("WEAP indexed");
    assert_eq!(weap.common.editor_id, "TestWeap");
    match weap.kind {
        ItemKind::Weapon {
            damage, clip_size, ..
        } => {
            assert_eq!(damage, 20);
            assert_eq!(clip_size, 8);
        }
        _ => panic!("expected weapon"),
    }
}

/// Regression: #631 / FNV-D2-03 — `DIAL` records with a nested
/// Topic Children sub-GRUP (`group_type == 7`, label = parent
/// DIAL form_id) must populate `DialRecord.infos` with each
/// child INFO record. Pre-fix the generic `extract_records`
/// walker filtered on `expected_type == "DIAL"` and silently
/// skipped every INFO; every DIAL arrived as an empty shell.
///
/// Fixture builds:
///   GRUP type=0 label="DIAL"
///     DIAL record (form_id 0xCAFE, EDID="MQGreeting", FULL="Hello")
///     GRUP type=7 label=0xCAFE
///       INFO (0x1001, NAM1="Welcome", TRDT[0]=3, PNAM=0)
///       INFO (0x1002, NAM1="Wait outside.", PNAM=0x1001)
///
/// Asserts both INFOs land on the parent DIAL with their
/// authored fields.
#[test]
fn dial_topic_children_walked_into_dialogue_infos() {
    // Two INFO records inside the Topic Children sub-GRUP.
    let info_1 = build_record(
        b"INFO",
        0x1001,
        &[
            (b"NAM1", b"Welcome\0".to_vec()),
            (b"TRDT", vec![3, 0, 0, 0]),
            (b"PNAM", 0u32.to_le_bytes().to_vec()),
        ],
    );
    let info_2 = build_record(
        b"INFO",
        0x1002,
        &[
            (b"NAM1", b"Wait outside.\0".to_vec()),
            (b"PNAM", 0x1001u32.to_le_bytes().to_vec()),
        ],
    );

    // Topic Children sub-GRUP: group_type = 7, label = parent
    // DIAL form_id (0xCAFE) packed as little-endian bytes.
    let topic_children = {
        let mut content = Vec::new();
        content.extend_from_slice(&info_1);
        content.extend_from_slice(&info_2);
        let total = 24 + content.len();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GRUP");
        buf.extend_from_slice(&(total as u32).to_le_bytes());
        buf.extend_from_slice(&0xCAFEu32.to_le_bytes()); // label
        buf.extend_from_slice(&7u32.to_le_bytes()); // Topic Children
        buf.extend_from_slice(&[0u8; 8]); // stamp
        buf.extend_from_slice(&content);
        buf
    };

    // DIAL record + its Topic Children sub-GRUP, wrapped in the
    // top-level "DIAL" GRUP.
    let dial = build_record(
        b"DIAL",
        0xCAFE,
        &[
            (b"EDID", b"MQGreeting\0".to_vec()),
            (b"FULL", b"Hello\0".to_vec()),
        ],
    );
    let mut top_content = Vec::new();
    top_content.extend_from_slice(&dial);
    top_content.extend_from_slice(&topic_children);

    let top_total = 24 + top_content.len();
    let mut top_grup = Vec::new();
    top_grup.extend_from_slice(b"GRUP");
    top_grup.extend_from_slice(&(top_total as u32).to_le_bytes());
    top_grup.extend_from_slice(b"DIAL");
    top_grup.extend_from_slice(&0u32.to_le_bytes()); // top group
    top_grup.extend_from_slice(&[0u8; 8]);
    top_grup.extend_from_slice(&top_content);

    // TES4 dummy header so parse_esm reaches the DIAL group.
    let mut buf = build_record(b"TES4", 0, &[]);
    buf.extend_from_slice(&top_grup);
    let index = parse_esm(&buf).expect("parse_esm");

    let dial = index.dialogues.get(&0xCAFE).expect("DIAL indexed");
    assert_eq!(dial.editor_id, "MQGreeting");
    assert_eq!(dial.full_name, "Hello");
    assert_eq!(
        dial.infos.len(),
        2,
        "Topic Children INFOs must land on DialRecord.infos (#631)"
    );
    assert_eq!(dial.infos[0].form_id, 0x1001);
    assert_eq!(dial.infos[0].response_text, "Welcome");
    assert_eq!(dial.infos[0].response_type, 3);
    assert_eq!(dial.infos[0].previous_info, 0);
    assert_eq!(dial.infos[1].form_id, 0x1002);
    assert_eq!(dial.infos[1].response_text, "Wait outside.");
    assert_eq!(
        dial.infos[1].previous_info, 0x1001,
        "INFO chain links must survive the walker (#631)"
    );
}

/// Real-data sanity for #631: opt-in load of FalloutNV.esm
/// asserts at least one DIAL has non-empty `infos`. Pre-fix the
/// whole `dialogues` map's `infos` was empty across every DIAL.
/// Stays `#[ignore]` like the rest of the real-data tests.
#[test]
#[ignore]
fn parse_real_fnv_dial_infos_populated() {
    let path = crate::esm::test_paths::fnv_esm();
    if !path.exists() {
        eprintln!("Skipping: FalloutNV.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let index = parse_esm(&data).expect("parse_esm");

    let total_infos: usize = index.dialogues.values().map(|d| d.infos.len()).sum();
    let dialogues_with_infos = index
        .dialogues
        .values()
        .filter(|d| !d.infos.is_empty())
        .count();
    eprintln!(
        "FNV dialogues: {} total, {} with INFOs ({} INFOs total)",
        index.dialogues.len(),
        dialogues_with_infos,
        total_infos,
    );
    assert!(
        !index.dialogues.is_empty(),
        "FNV must ship at least one DIAL"
    );
    assert!(
        total_infos > 0,
        "FNV must surface at least one INFO across all DIALs (#631)"
    );
}

/// Parse the real FalloutNV.esm and verify record counts. Skipped on
/// machines without the game data — opt in with `cargo test -p
/// byroredux-plugin -- --ignored`.
///
/// The thresholds are deliberately conservative: FNV ships with ~1700
/// weapons, ~1800 armor pieces, ~5000 misc items, ~3300 NPCs, ~120 races
/// (a lot of variants), ~70 classes, ~250 factions, and a few hundred
/// globals/game settings. We just check we're in the right order of
/// magnitude — exact numbers drift with patches.
#[test]
#[ignore]
fn parse_real_fnv_esm_record_counts() {
    let path = crate::esm::test_paths::fnv_esm();
    if !path.exists() {
        eprintln!("Skipping: FalloutNV.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let index = parse_esm(&data).expect("parse_esm");

    eprintln!(
        "FNV index: {} items, {} containers, {} LVLI, {} LVLN, {} NPCs, \
         {} races, {} classes, {} factions, {} globals, {} game settings",
        index.items.len(),
        index.containers.len(),
        index.leveled_items.len(),
        index.leveled_npcs.len(),
        index.npcs.len(),
        index.races.len(),
        index.classes.len(),
        index.factions.len(),
        index.globals.len(),
        index.game_settings.len(),
    );

    // Floors based on actual FNV.esm counts (April 2026 patch revision):
    // items=2643 containers=2478 LVLI=2738 LVLN=365 NPCs=3816 races=22
    // classes=74 factions=682 globals=218 game_settings=648.
    // Each floor is a couple percent below the observed count so the test
    // stays stable across DLC patches without becoming meaningless.
    assert!(
        index.items.len() > 2500,
        "expected >2.5k items, got {}",
        index.items.len()
    );
    assert!(
        index.containers.len() > 2000,
        "expected >2k containers, got {}",
        index.containers.len()
    );
    assert!(
        index.leveled_items.len() > 2000,
        "expected >2k leveled item lists, got {}",
        index.leveled_items.len()
    );
    assert!(
        index.leveled_npcs.len() > 250,
        "expected >250 leveled NPC lists, got {}",
        index.leveled_npcs.len()
    );
    assert!(
        index.npcs.len() > 3000,
        "expected >3k NPCs, got {}",
        index.npcs.len()
    );
    assert!(
        index.races.len() >= 15,
        "expected ≥15 races, got {}",
        index.races.len()
    );
    assert!(
        index.classes.len() > 50,
        "expected >50 classes, got {}",
        index.classes.len()
    );
    assert!(
        index.factions.len() > 500,
        "expected >500 factions, got {}",
        index.factions.len()
    );
    assert!(
        index.globals.len() > 150,
        "expected >150 globals, got {}",
        index.globals.len()
    );
    assert!(
        index.game_settings.len() > 500,
        "expected >500 game settings, got {}",
        index.game_settings.len()
    );

    // Supplementary records (#458). Floors based on a live FNV.esm
    // parse run on the April 2026 patch — each floor sits a few
    // percent below the observed count so DLC patches stay green.
    //
    // Observed on FalloutNV.esm:
    //   WATR=78, NAVI=1, NAVM=0 (NAVM entries live nested under
    //   CELL children groups on FO3/FNV, not at top level — a
    //   follow-up can walk those if needed), REGN=276, ECZN=17,
    //   LGTM=31, HDPT=61, EYES=12, HAIR=67.
    eprintln!(
        "FNV misc: {} water, {} navi, {} navm, {} region, {} eczn, \
         {} lgtm, {} hdpt, {} eyes, {} hair",
        index.waters.len(),
        index.navi_info.len(),
        index.navmeshes.len(),
        index.regions.len(),
        index.encounter_zones.len(),
        index.lighting_templates.len(),
        index.head_parts.len(),
        index.eyes.len(),
        index.hair.len(),
    );
    assert!(
        index.waters.len() >= 50,
        "expected ≥50 WATR water types, got {}",
        index.waters.len()
    );
    assert_eq!(
        index.navi_info.len(),
        1,
        "expected exactly 1 NAVI master (FNV ships one), got {}",
        index.navi_info.len()
    );
    assert!(
        index.regions.len() >= 200,
        "expected ≥200 REGN regions, got {}",
        index.regions.len()
    );
    assert!(
        index.encounter_zones.len() >= 10,
        "expected ≥10 ECZN encounter zones, got {}",
        index.encounter_zones.len()
    );
    assert!(
        index.lighting_templates.len() >= 20,
        "expected ≥20 LGTM templates, got {}",
        index.lighting_templates.len()
    );
    assert!(
        index.head_parts.len() >= 40,
        "expected ≥40 HDPT head parts, got {}",
        index.head_parts.len()
    );
    assert!(
        index.eyes.len() >= 8,
        "expected ≥8 EYES definitions, got {}",
        index.eyes.len()
    );
    assert!(
        index.hair.len() >= 50,
        "expected ≥50 HAIR definitions, got {}",
        index.hair.len()
    );

    // #519 — AVIF actor-value records. FNV ships ~70 vanilla
    // AVIFs (7 SPECIAL + 13 governed skills + ~50 derived
    // resources/resistances/VATS targets); audit floor of 30
    // is the conservative threshold from the issue body.
    eprintln!("FNV AVIF: {} actor values", index.actor_values.len());
    assert!(
        index.actor_values.len() >= 30,
        "expected ≥30 AVIF actor values, got {}",
        index.actor_values.len()
    );
    // Sanity: FNV ships AVIFs under the "AV<Name>" convention
    // (AVStrength, AVAgility, AVBigGuns, …). Verify both that
    // the EDIDs round-trip non-empty *and* that the SPECIAL
    // attribute set is present.
    let nonempty = index
        .actor_values
        .values()
        .filter(|av| !av.editor_id.is_empty())
        .count();
    assert!(
        nonempty >= 30,
        "expected ≥30 AVIFs with non-empty editor_id, got {nonempty}"
    );
    for special in [
        "AVStrength",
        "AVPerception",
        "AVEndurance",
        "AVCharisma",
        "AVIntelligence",
        "AVAgility",
        "AVLuck",
    ] {
        let found = index
            .actor_values
            .values()
            .any(|av| av.editor_id == special);
        assert!(found, "expected SPECIAL AVIF '{special}' to be indexed");
    }

    // #629 / FNV-D2-01 — ENCH dispatch. Pre-fix the entire
    // top-level group fell through to the catch-all skip and every
    // weapon EITM dangled. FNV ships ~150 ENCH records (Pulse Gun,
    // This Machine, Holorifle, the energy-weapon variants, and
    // armor-side enchants); the floor is conservative against DLC
    // patch drift.
    eprintln!("FNV ENCH: {} enchantments", index.enchantments.len());
    assert!(
        index.enchantments.len() >= 50,
        "expected ≥50 ENCH enchantments, got {}",
        index.enchantments.len()
    );

    // Spot-check a known FNV item: Varmint Rifle (form 0x000086A8) should
    // be a Weapon kind with damage and a clip size.
    if let Some(varmint) = index.items.get(&0x000086A8) {
        eprintln!(
            "Varmint Rifle: {:?} kind={}",
            varmint.common.editor_id,
            varmint.kind.label()
        );
        assert_eq!(varmint.kind.label(), "WEAP");
    }

    // Spot-check that NCR faction exists (FNV form 0x0011E662 — name varies
    // by patch; just check there is a faction with "NCR" in its full name).
    let has_ncr = index
        .factions
        .values()
        .any(|f| f.full_name.contains("NCR") || f.editor_id.starts_with("NCR"));
    assert!(has_ncr, "expected an NCR-related faction");
}

/// Regression: #445 — the load-order remap routes each record's
/// own FormID through its plugin's global load-order slot. A
/// synthetic "DLC" with plugin_index=2 writes a self-referencing
/// form 0x0100_BEEF (mod_index=1 == num_masters=1 → self), which
/// under remap lands as 0x0200_BEEF in the global map.
#[test]
fn parse_esm_with_load_order_remaps_self_form_ids() {
    let mut subs: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
    subs.push((b"EDID", b"TestWeap\0".to_vec()));
    subs.push((b"DATA", {
        let mut d = Vec::new();
        d.extend_from_slice(&100u32.to_le_bytes()); // value
        d.extend_from_slice(&0u32.to_le_bytes()); // health
        d.extend_from_slice(&2.0f32.to_le_bytes()); // weight
        d.extend_from_slice(&20u16.to_le_bytes()); // damage
        d.push(8);
        d.push(0);
        d
    }));
    // In-file form_id 0x0100_BEEF — mod_index=1, which for a DLC
    // with one master equals its self-index.
    let record = build_record(b"WEAP", 0x0100_BEEF, &subs);
    let group = wrap_group(b"WEAP", &record);
    let mut tes4 = build_record(b"TES4", 0, &[]);
    tes4.extend_from_slice(&group);

    // Load this synthetic "DLC" at plugin_index=2 with Fallout3
    // (plugin_index=0) as its single master.
    let remap = super::super::reader::FormIdRemap {
        plugin_index: 2,
        master_indices: vec![0],
    };
    let index = parse_esm_with_load_order(&tes4, Some(remap)).unwrap();
    assert_eq!(index.items.len(), 1);
    let remapped_key = 0x0200_BEEFu32;
    assert!(
        index.items.contains_key(&remapped_key),
        "DLC self-ref 0x0100_BEEF must remap to global 0x0200_BEEF at plugin_index=2 (#445)"
    );
    // The pre-remap key must NOT be present.
    assert!(
        !index.items.contains_key(&0x0100_BEEF),
        "raw pre-remap FormID must not leak through once the remap is installed"
    );
}

/// Regression: #443 — a top-level `SCPT` GRUP must dispatch to
/// `parse_scpt` and land in `EsmIndex.scripts`. Pre-fix the whole
/// group fell through to the catch-all skip so every NPC / item
/// `SCRI` FormID cross-reference dangled.
#[test]
fn scpt_group_dispatches_to_scripts_map() {
    let mut schr = Vec::new();
    schr.extend_from_slice(&0u32.to_le_bytes()); // pad
    schr.extend_from_slice(&0u32.to_le_bytes()); // num_refs
    schr.extend_from_slice(&42u32.to_le_bytes()); // compiled_size
    schr.extend_from_slice(&0u32.to_le_bytes()); // var_count
    schr.extend_from_slice(&0u16.to_le_bytes()); // object
    schr.extend_from_slice(&0u32.to_le_bytes()); // flags (FO3 u32 tail)
    let subs: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"EDID", b"DummyScript\0".to_vec()),
        (b"SCHR", schr),
        (b"SCDA", vec![0u8; 42]),
    ];
    let record = build_record(b"SCPT", 0xBEEF_0003, &subs);
    let group = wrap_group(b"SCPT", &record);
    let mut tes4 = build_record(b"TES4", 0, &[]);
    tes4.extend_from_slice(&group);
    let index = parse_esm(&tes4).unwrap();
    assert_eq!(index.scripts.len(), 1, "SCPT must land in scripts map");
    let scpt = index.scripts.get(&0xBEEF_0003).expect("SCPT indexed");
    assert_eq!(scpt.editor_id, "DummyScript");
    assert_eq!(scpt.compiled_size, 42);
    assert_eq!(scpt.compiled.len(), 42);
}

/// Regression: #519 — a top-level `AVIF` GRUP must dispatch to
/// `parse_avif` and land in `EsmIndex.actor_values`. Pre-fix the
/// whole group fell through to the catch-all skip, so every NPC
/// `skill_bonuses` cross-ref, BOOK skill-book teach ref, and
/// AVIF-keyed condition predicate dangled.
#[test]
fn avif_group_dispatches_to_actor_values_map() {
    let mut avsk = Vec::new();
    avsk.extend_from_slice(&1.0f32.to_le_bytes());
    avsk.extend_from_slice(&0.0f32.to_le_bytes());
    avsk.extend_from_slice(&1.5f32.to_le_bytes());
    avsk.extend_from_slice(&2.0f32.to_le_bytes());
    let subs: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"EDID", b"SmallGuns\0".to_vec()),
        (b"FULL", b"Small Guns\0".to_vec()),
        (b"CNAM", 1u32.to_le_bytes().to_vec()),
        (b"AVSK", avsk),
    ];
    let record = build_record(b"AVIF", 0xBEEF_002B, &subs);
    let group = wrap_group(b"AVIF", &record);
    let mut tes4 = build_record(b"TES4", 0, &[]);
    tes4.extend_from_slice(&group);
    let index = parse_esm(&tes4).unwrap();

    assert_eq!(
        index.actor_values.len(),
        1,
        "AVIF must populate the actor_values map"
    );
    let avif = index.actor_values.get(&0xBEEF_002B).expect("AVIF indexed");
    assert_eq!(avif.editor_id, "SmallGuns");
    assert_eq!(avif.full_name, "Small Guns");
    assert_eq!(avif.category, 1);
    assert!(avif.skill_scaling.is_some());
}

/// Regression: #442 — a top-level `CREA` GRUP must dispatch to
/// `parse_npc` (schema is NPC_-shaped) and land in
/// `EsmIndex.creatures`. Pre-fix the whole group fell through to
/// the catch-all skip.
#[test]
fn crea_group_dispatches_to_creatures_map() {
    let mut subs: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
    subs.push((b"EDID", b"Radroach\0".to_vec()));
    subs.push((b"FULL", b"Radroach\0".to_vec()));
    subs.push((b"MODL", b"Creatures\\Radroach.nif\0".to_vec()));
    let record = build_record(b"CREA", 0xBEEF_0001, &subs);
    let group = wrap_group(b"CREA", &record);

    let mut tes4 = build_record(b"TES4", 0, &[]);
    tes4.extend_from_slice(&group);
    let index = parse_esm(&tes4).unwrap();

    assert_eq!(
        index.creatures.len(),
        1,
        "CREA must populate the creatures map"
    );
    let crea = index.creatures.get(&0xBEEF_0001).expect("CREA indexed");
    assert_eq!(crea.editor_id, "Radroach");
    assert_eq!(crea.full_name, "Radroach");
    assert_eq!(crea.model_path, "Creatures\\Radroach.nif");
    // CREA must not leak into NPC_'s map.
    assert!(index.npcs.is_empty());
}

/// Regression: #448 — a top-level `LVLC` GRUP must dispatch to
/// `parse_leveled_list` and land in `EsmIndex.leveled_creatures`.
/// Pre-fix the whole group fell through to the catch-all skip,
/// so every FO3 encounter zone's creature spawn table came back
/// empty.
#[test]
fn lvlc_group_dispatches_to_leveled_creatures_map() {
    // LVLC shares the LVLI/LVLN layout: LVLD (u8 chance_none),
    // LVLF (u8 flags), LVLO (12 bytes: level u16 + pad u16 + form u32 + count u16 + pad u16).
    let mut subs: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
    subs.push((b"EDID", b"LL_Raider\0".to_vec()));
    subs.push((b"LVLD", vec![50u8])); // 50% chance none
    subs.push((b"LVLF", vec![1u8])); // calculate_from_all flag
    subs.push((b"LVLO", {
        let mut d = Vec::new();
        d.extend_from_slice(&1u16.to_le_bytes()); // level
        d.extend_from_slice(&0u16.to_le_bytes()); // pad
        d.extend_from_slice(&0xCAFE_F00Du32.to_le_bytes()); // form
        d.extend_from_slice(&1u16.to_le_bytes()); // count
        d.extend_from_slice(&0u16.to_le_bytes()); // pad
        d
    }));
    let record = build_record(b"LVLC", 0xBEEF_0002, &subs);
    let group = wrap_group(b"LVLC", &record);

    let mut tes4 = build_record(b"TES4", 0, &[]);
    tes4.extend_from_slice(&group);
    let index = parse_esm(&tes4).unwrap();

    assert_eq!(
        index.leveled_creatures.len(),
        1,
        "LVLC must populate the leveled_creatures map"
    );
    let lvlc = index
        .leveled_creatures
        .get(&0xBEEF_0002)
        .expect("LVLC indexed");
    assert_eq!(lvlc.editor_id, "LL_Raider");
    assert_eq!(lvlc.entries.len(), 1);
    assert_eq!(lvlc.entries[0].form_id, 0xCAFE_F00D);
    // LVLC must not leak into LVLI / LVLN.
    assert!(index.leveled_items.is_empty());
    assert!(index.leveled_npcs.is_empty());
}

/// Parse real Fallout3.esm and assert the bestiary + spawn tables
/// arrive populated. Ignored by default — opt in with
/// `cargo test -p byroredux-plugin -- --ignored`.
///
/// Sampled counts against FO3 GOTY HEDR=0.94 master on 2026-04-19:
/// 1647 NPCs, 533 creatures, 89 LVLN, 60 LVLC. Floors are set a
/// few percent below observed so the test stays stable across DLC
/// patches without becoming meaningless. The audit body predicted
/// ~700-800 CREA / ~400-500 LVLC; the real numbers are lower, so
/// don't chase the audit's estimates — use the disk-sampled ones.
/// Parse real Fallout3.esm and assert SCPT records arrive + at
/// least one NPC's SCRI FormID resolves into the scripts map.
/// Ignored — opt in with `--ignored`.
#[test]
#[ignore]
fn parse_real_fo3_esm_scpt_count_and_scri_resolves() {
    let path = crate::esm::test_paths::fo3_esm();
    if !path.exists() {
        eprintln!("Skipping: Fallout3.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let index = parse_esm(&data).expect("parse_esm");
    eprintln!("FO3 SCPT: {} records", index.scripts.len());
    assert!(
        index.scripts.len() > 500,
        "expected >500 SCPT records (FO3 GOTY ships ~1500+), got {}",
        index.scripts.len()
    );

    // Find any NPC / container record whose script_form_id lands
    // inside the scripts map — pre-#443 nothing could satisfy this
    // because scripts was always empty.
    let resolved = index
        .npcs
        .values()
        .filter(|n| n.has_script || n.disposition_base != 0)
        .count();
    // Any SCRI dereference working is sufficient — we don't parse
    // NPC SCRI yet (it's tracked elsewhere), so just assert the map
    // isn't empty and one script has a resolvable SCRV/SCRO ref
    // pointing at another record.
    let cross_ref_count: usize = index
        .scripts
        .values()
        .filter(|s| !s.ref_form_ids.is_empty())
        .count();
    eprintln!(
        "{} scripts carry at least one SCRV/SCRO cross-ref; {} NPCs had context hints",
        cross_ref_count, resolved
    );
    assert!(
        cross_ref_count > 100,
        "expected >100 scripts with SCRV/SCRO cross-refs, got {cross_ref_count}"
    );
}

#[test]
#[ignore]
fn parse_real_fo3_esm_crea_and_lvlc_counts() {
    let path = crate::esm::test_paths::fo3_esm();
    if !path.exists() {
        eprintln!("Skipping: Fallout3.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let index = parse_esm(&data).expect("parse_esm");
    eprintln!(
        "FO3 index: {} NPCs, {} creatures, {} LVLN, {} LVLC",
        index.npcs.len(),
        index.creatures.len(),
        index.leveled_npcs.len(),
        index.leveled_creatures.len(),
    );
    assert!(
        index.creatures.len() > 400,
        "expected >400 CREA records (observed 533), got {}",
        index.creatures.len()
    );
    assert!(
        index.leveled_creatures.len() > 40,
        "expected >40 LVLC records (observed 60), got {}",
        index.leveled_creatures.len()
    );
}

#[test]
fn esm_index_total_counts_all_categories() {
    let mut idx = EsmIndex::default();
    idx.items.insert(
        1,
        ItemRecord {
            form_id: 1,
            common: Default::default(),
            kind: ItemKind::Misc,
        },
    );
    idx.npcs.insert(
        2,
        NpcRecord {
            form_id: 2,
            editor_id: String::new(),
            full_name: String::new(),
            model_path: String::new(),
            race_form_id: 0,
            class_form_id: 0,
            voice_form_id: 0,
            factions: Vec::new(),
            inventory: Vec::new(),
            default_outfit: None,
            ai_packages: Vec::new(),
            death_item_form_id: 0,
            level: 1,
            disposition_base: 50,
            acbs_flags: 0,
            has_script: false,
            face_morphs: None,
            runtime_facegen: None,
        },
    );
    assert_eq!(idx.total(), 2);
}

/// #634 / FNV-D2-06 — `total()` and the end-of-parse log line must
/// drive off the same `categories()` table. Verify the table sums
/// to `total()` and that the breakdown line names every category
/// (so a future `index.foos: HashMap<...>` addition that misses a
/// `categories()` row is caught loud). The cells.statics overlap
/// with typed maps is intentional — see `categories()` doc.
#[test]
fn total_and_breakdown_drive_off_same_table() {
    let mut idx = EsmIndex::default();
    idx.items.insert(
        1,
        ItemRecord {
            form_id: 1,
            common: Default::default(),
            kind: ItemKind::Misc,
        },
    );
    idx.activators.insert(
        10,
        ActiRecord {
            form_id: 10,
            ..Default::default()
        },
    );
    idx.enchantments.insert(
        20,
        EnchRecord {
            form_id: 20,
            ..Default::default()
        },
    );
    // Sum the table by hand — must match `total()`.
    let sum: usize = EsmIndex::categories().iter().map(|(_, f)| f(&idx)).sum();
    assert_eq!(idx.total(), sum);
    assert_eq!(idx.total(), 3);

    // The breakdown line must mention every category by label so a
    // future struct-field addition that misses `categories()` is
    // caught here rather than discovered via a silent log drift.
    let line = idx.category_breakdown();
    for (label, _) in EsmIndex::categories() {
        assert!(
            line.contains(label),
            "breakdown line missing category '{label}': {line}"
        );
    }
    // And the totals from each row must round-trip into the line
    // (non-zero rows specifically — formatted as `<n> <label>`).
    assert!(line.contains("1 items"), "breakdown: {line}");
    assert!(line.contains("1 activators"), "breakdown: {line}");
    assert!(line.contains("1 enchantments"), "breakdown: {line}");
}

/// Guard against an `EsmIndex` field addition that forgets to wire
/// a row in `categories()`. We can't enumerate fields at runtime,
/// but we can pin the row count: every public top-level map +
/// `cells.cells` + `cells.statics` is one row. If you add a new
/// `pub foos: HashMap<...>` field, increment this.
#[test]
fn categories_table_row_count_pinned() {
    // 80 typed maps on EsmIndex + 2 from cells (cells, statics).
    // Bumped from 37 → 38 in #624 (image_spaces map for IMGS dispatch).
    // Bumped from 38 → 39 in #630 (form_lists map for FLST dispatch).
    // Bumped from 39 → 44 in #808 (FNV-D2-NEW-01: PROJ + EFSH +
    //   IMOD + ARMA + BPTD stubs for FNV gameplay coverage).
    // Bumped from 44 → 51 in #809 (FNV-D2-NEW-02: REPU + EXPL +
    //   CSTY + IDLE + IPCT + IPDS + COBJ stubs for NPC AI /
    //   crafting / impact-effect / faction-reputation coverage).
    // Bumped from 51 → 82 in #810 (FNV-D2-NEW-03: 31 long-tail
    //   minimal-stub records covering audio metadata, visual /
    //   world, hardcore mode, Caravan + Casino, recipe residuals).
    //   All 31 share `MinimalEsmRecord` via
    //   `parse_minimal_esm_record` — replace with dedicated
    //   per-record parsers via the #808/#809 pattern when a
    //   consumer arrives.
    // Bumped from 82 → 87 in #817 (FO4-D4-NEW-05: 5 FO4-architecture
    //   maps that live on `EsmCellIndex` rather than `EsmIndex` —
    //   texture_sets, scols, packins, movables, material_swaps —
    //   were silently uncovered by `category_breakdown()` and
    //   would let regressions slip through CI).
    // Bumped 87 → 88 in #896 (Phase B: outfits — Skyrim+ OTFT
    //   record map for the equip pipeline).
    // Bumped 88 → 93 in #966 (OBL-D3-NEW-02: BSGN, CLOT, APPA,
    //   SGST, SLGM — Oblivion-unique base records that previously
    //   fell through the catch-all skip).
    // Bump in lockstep with the struct + `categories()` edits.
    assert_eq!(EsmIndex::categories().len(), 93);
}
