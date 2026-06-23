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
///       INFO (0x1001, NAM1="Welcome", TRDT emotion=3/response#=5, PNAM=0)
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
            // TES4 TRDT (16 B): EmotionType(u32)=3 (EMO_Fear) +
            // EmotionValue(i32) + unused[4] + Response number(u8)=5 @12
            // + unused[3]. Byte 0 is the emotion, NOT a response number
            // (#1304).
            (
                b"TRDT",
                vec![3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0],
            ),
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
    // TRDT byte 0 is the EmotionType (3 = EMO_Fear), not a response
    // number; the real response index is at offset 12 (#1304).
    assert_eq!(dial.infos[0].emotion_type, 3);
    assert_eq!(dial.infos[0].response_number, 5);
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
    let remap = super::super::reader::FormIdRemap::regular(2, vec![0]);
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

/// Regression: #1568 / SF-D4-02 — a top-level `PDCL` GRUP (Starfield
/// `BGSProjectedDecal`) has no consumer yet. It must be CONSCIOUSLY
/// skipped: named in the `skipped_unconsumed_groups` telemetry (not lost
/// in the anonymous `_ => skip_group` catch-all) and never routed into
/// `cells.statics` (decals carry no MODL). Pre-fix the whole group fell
/// through the catch-all and the 1846 Cydonia decal REFRs dangled with
/// zero signal.
#[test]
fn pdcl_group_consciously_skipped_and_counted() {
    let pdcl: [u8; 4] = *b"PDCL";
    let subs: Vec<(&[u8; 4], Vec<u8>)> = vec![(b"EDID", b"DecalGrime01\0".to_vec())];
    let record = build_record(b"PDCL", 0xBEEF_00DE, &subs);
    let group = wrap_group(b"PDCL", &record);
    let mut tes4 = build_record(b"TES4", 0, &[]);
    tes4.extend_from_slice(&group);
    let index = parse_esm(&tes4).unwrap();

    // Counted in telemetry — named, not silently dropped.
    assert!(
        index.skipped_unconsumed_groups.contains(&pdcl),
        "PDCL must be recorded in skip telemetry, not lost to the catch-all"
    );
    // Warned-once: a single PDCL group records exactly one entry, no
    // per-record spam (matches the warned_scol / warned_movs contract).
    assert_eq!(
        index
            .skipped_unconsumed_groups
            .iter()
            .filter(|&&l| l == pdcl)
            .count(),
        1,
        "PDCL skip must be recorded once per parse, not per record"
    );
    // Never routed into statics — decals have no MODL.
    assert!(
        !index.cells.statics.contains_key(&0xBEEF_00DE),
        "PDCL must not leak into cells.statics"
    );
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
            script_form_id: 0,
            script_instance: None,
            face_morphs: None,
            runtime_facegen: None,
            template_form_id: 0,
            template_flags: 0,
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
    // Bumped 93 → 94 in #969 (OBL-D3-NEW-05: magic_effects_by_code —
    //   Oblivion-only 4-char-code → MGEF FormID secondary index for
    //   SPEL/ENCH/ALCH/INGR EFID resolution).
    // Bump in lockstep with the struct + `categories()` edits.
    assert_eq!(EsmIndex::categories().len(), 94);
}

/// Regression test for #989 — `.STRINGS` companion file resolves lstring
/// placeholders when a [`StringsTableGuard`] is active during `parse_esm`.
///
/// Fixture: a localized WEAP record with FULL = 4-byte lstring ID 0x0001.
/// A synthetic `.STRINGS` table maps 0x0001 → "Iron Sword".
/// After parsing with the guard active, `item.common.full_name` must be
/// "Iron Sword" — NOT the `<lstring 0x00000001>` placeholder.
#[test]
fn lstring_resolved_via_strings_table_guard() {
    use crate::esm::strings_table::{StringTableSet, StringsTable};

    // ── Build a synthetic .STRINGS table ─────────────────────────────
    // Binary layout: [count:u32][data_size:u32][id:u32, offset:u32 × count][blob]
    let string_data = b"Iron Sword\0";
    let mut strings_bytes = Vec::new();
    strings_bytes.extend_from_slice(&1u32.to_le_bytes()); // count = 1
    strings_bytes.extend_from_slice(&(string_data.len() as u32).to_le_bytes()); // data_size
    strings_bytes.extend_from_slice(&0x0001u32.to_le_bytes()); // id = 1
    strings_bytes.extend_from_slice(&0u32.to_le_bytes()); // offset = 0
    strings_bytes.extend_from_slice(string_data);

    let table = StringsTable::parse(&strings_bytes, false).unwrap();
    assert_eq!(table.get(0x0001), Some("Iron Sword"), "table round-trip");

    let set = StringTableSet {
        strings: Some(table),
        dlstrings: None,
        ilstrings: None,
    };

    // ── Build a localized ESM fixture ─────────────────────────────────
    // TES4 record with flags = 0x80 (Localized bit), then a WEAP GRUP.
    let lstring_id: u32 = 0x0001;

    // WEAP record: EDID="TestBlade" + FULL=4-byte lstring ID
    let mut weap_subs = Vec::<(&[u8; 4], Vec<u8>)>::new();
    weap_subs.push((b"EDID", b"TestBlade\0".to_vec()));
    weap_subs.push((b"FULL", lstring_id.to_le_bytes().to_vec()));
    // Minimal DATA sub-record (FNV-style: value + health + weight + damage + clip)
    weap_subs.push((b"DATA", {
        let mut d = Vec::new();
        d.extend_from_slice(&100u32.to_le_bytes()); // value
        d.extend_from_slice(&0u32.to_le_bytes()); // health
        d.extend_from_slice(&1.5f32.to_le_bytes()); // weight
        d.extend_from_slice(&15u16.to_le_bytes()); // damage
        d.push(0); // clip_size
        d.push(0);
        d
    }));
    let weap_record = build_record(b"WEAP", 0xBEEF, &weap_subs);
    let weap_group = wrap_group(b"WEAP", &weap_record);

    // TES4 with Localized flag (bit 0x80)
    let tes4 = build_localized_tes4();

    let mut esm_bytes = tes4;
    esm_bytes.extend_from_slice(&weap_group);

    // ── Parse without guard: must get placeholder ─────────────────────
    {
        let index = parse_esm(&esm_bytes).unwrap();
        let item = index.items.get(&0xBEEF).expect("WEAP indexed");
        assert_eq!(
            item.common.full_name, "<lstring 0x00000001>",
            "without StringsTableGuard the placeholder must survive"
        );
    }

    // ── Parse with guard: must get the resolved string ─────────────────
    {
        let _guard = StringsTableGuard::new(set);
        let index = parse_esm(&esm_bytes).unwrap();
        let item = index.items.get(&0xBEEF).expect("WEAP indexed");
        assert_eq!(
            item.common.full_name, "Iron Sword",
            "StringsTableGuard must resolve the lstring placeholder"
        );
    }

    // ── After guard drop: back to placeholder ─────────────────────────
    {
        let index = parse_esm(&esm_bytes).unwrap();
        let item = index.items.get(&0xBEEF).expect("WEAP indexed");
        assert_eq!(
            item.common.full_name, "<lstring 0x00000001>",
            "guard drop must restore placeholder behaviour"
        );
    }
}

/// Build a TES4 record with the `Localized` flag set (bit `0x80`).
fn build_localized_tes4() -> Vec<u8> {
    // Minimal TES4: HEDR sub-record (12 bytes) only.
    let mut hedr = Vec::new();
    hedr.extend_from_slice(b"HEDR");
    hedr.extend_from_slice(&12u16.to_le_bytes());
    hedr.extend_from_slice(&1.7f32.to_le_bytes()); // version (Skyrim)
    hedr.extend_from_slice(&0u32.to_le_bytes()); // record_count
    hedr.extend_from_slice(&0u32.to_le_bytes()); // next_object_id

    let mut buf = Vec::new();
    buf.extend_from_slice(b"TES4");
    buf.extend_from_slice(&(hedr.len() as u32).to_le_bytes()); // data size
    buf.extend_from_slice(&0x80u32.to_le_bytes()); // flags = 0x80 (Localized)
    buf.extend_from_slice(&0u32.to_le_bytes()); // form_id
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&hedr);
    buf
}

// ── #969 / OBL-D3-NEW-05 — Oblivion `magic_effects_by_code` map ─────
//
// Oblivion uses a 20-byte record / group header. The `EsmVariant`
// detector triggers Oblivion mode when bytes 20..24 of the file are
// `b"HEDR"`. The Tes5Plus helpers above all use 24-byte headers so we
// add Oblivion-shaped builders here, scoped to this regression.

/// Build an Oblivion-format record (20-byte header).
fn build_record_obl(typ: &[u8; 4], form_id: u32, subs: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
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
    buf.extend_from_slice(&[0u8; 4]); // vc_info (4 bytes on Oblivion vs 8 on Tes5Plus)
    buf.extend_from_slice(&sub_data);
    buf
}

/// Wrap an Oblivion record blob in a top-level GRUP (20-byte header).
fn wrap_group_obl(label: &[u8; 4], record: &[u8]) -> Vec<u8> {
    let total = 20 + record.len();
    let mut buf = Vec::new();
    buf.extend_from_slice(b"GRUP");
    buf.extend_from_slice(&(total as u32).to_le_bytes());
    buf.extend_from_slice(label);
    buf.extend_from_slice(&0u32.to_le_bytes()); // group_type = top
    buf.extend_from_slice(&[0u8; 4]); // stamp (4 bytes on Oblivion)
    buf.extend_from_slice(record);
    buf
}

/// Build an Oblivion TES4 header with HEDR 1.0. The variant detector
/// picks `EsmVariant::Oblivion` because bytes 20..24 spell `"HEDR"`
/// (only possible with the 20-byte record header).
fn build_oblivion_tes4() -> Vec<u8> {
    let mut hedr = Vec::new();
    hedr.extend_from_slice(b"HEDR");
    hedr.extend_from_slice(&12u16.to_le_bytes());
    hedr.extend_from_slice(&1.0f32.to_le_bytes()); // Oblivion HEDR version
    hedr.extend_from_slice(&0u32.to_le_bytes()); // record_count
    hedr.extend_from_slice(&0u32.to_le_bytes()); // next_object_id

    let mut buf = Vec::new();
    buf.extend_from_slice(b"TES4");
    buf.extend_from_slice(&(hedr.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&0u32.to_le_bytes()); // form_id
    buf.extend_from_slice(&[0u8; 4]); // vc_info — keeps total header at 20 bytes
    buf.extend_from_slice(&hedr);
    buf
}

/// Regression for #969 / OBL-D3-NEW-05. Oblivion MGEF records carry
/// EDIDs that are the fixed-format 4-char effect code (e.g., `b"FIDG"`
/// for Feather, `b"DGFA"` for Damage Fatigue). SPEL/ENCH/ALCH/INGR
/// reference effects via `EFID` whose raw bytes ARE the 4-char code,
/// NOT a u32 FormID. The `magic_effects_by_code` side index lets a
/// (pending) magic-system runtime resolve EFID lookups on Oblivion
/// content without re-walking the FormID-keyed map.
#[test]
fn oblivion_mgef_populates_magic_effects_by_code() {
    let mut buf = build_oblivion_tes4();

    // Two MGEF records: "FIDG" (Feather) at form_id 0x111, "DGFA"
    // (Damage Fatigue) at 0x222. Both use a 5-byte EDID payload
    // (4 chars + trailing null).
    let mgef_feather = build_record_obl(
        b"MGEF",
        0x0000_0111,
        &[
            (b"EDID", b"FIDG\0".to_vec()),
            (b"FULL", b"Feather\0".to_vec()),
            (b"DATA", 0x0000_0001u32.to_le_bytes().to_vec()),
        ],
    );
    let mgef_damage_fatigue = build_record_obl(
        b"MGEF",
        0x0000_0222,
        &[
            (b"EDID", b"DGFA\0".to_vec()),
            (b"FULL", b"Damage Fatigue\0".to_vec()),
            (b"DATA", 0x0000_0002u32.to_le_bytes().to_vec()),
        ],
    );

    let mut group_content = Vec::new();
    group_content.extend_from_slice(&mgef_feather);
    group_content.extend_from_slice(&mgef_damage_fatigue);
    let mgef_group = wrap_group_obl(b"MGEF", &group_content);
    buf.extend_from_slice(&mgef_group);

    let index = parse_esm(&buf).expect("parse_esm");
    assert_eq!(
        index.game,
        GameKind::Oblivion,
        "fixture must classify as Oblivion (HEDR 1.0 + 20-byte header)"
    );

    // FormID-keyed map still populated unchanged.
    assert_eq!(index.magic_effects.len(), 2);
    assert!(index.magic_effects.contains_key(&0x0000_0111));
    assert!(index.magic_effects.contains_key(&0x0000_0222));

    // Secondary 4-char-code map populated for both effects.
    assert_eq!(
        index.magic_effects_by_code.len(),
        2,
        "Oblivion MGEFs with 4-char EDIDs must populate magic_effects_by_code (#969)"
    );
    assert_eq!(index.magic_effects_by_code.get(b"FIDG"), Some(&0x0000_0111));
    assert_eq!(index.magic_effects_by_code.get(b"DGFA"), Some(&0x0000_0222));
}

/// Sibling check for #969 — FO3/FNV/Skyrim+ MGEF EDIDs are full names
/// (e.g., `"RadiationPoisoning"`) and use FormID-keyed EFID lookups.
/// The 4-char-code secondary map must stay empty on non-Oblivion
/// content so it can't shadow the FormID-keyed lookup or accidentally
/// resolve to the wrong MGEF if the consumer queries it.
#[test]
fn non_oblivion_mgef_leaves_magic_effects_by_code_empty() {
    let mut subs: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
    subs.push((b"EDID", b"RadiationPoisoning\0".to_vec()));
    subs.push((b"DATA", 0x0000_0009u32.to_le_bytes().to_vec()));
    let mgef = build_record(b"MGEF", 0x0000_00A7, &subs);
    let mgef_group = wrap_group(b"MGEF", &mgef);

    // Default TES4 (Tes5Plus, HEDR 0.0) classifies as Fallout3NV.
    let mut buf = build_record(b"TES4", 0, &[]);
    buf.extend_from_slice(&mgef_group);

    let index = parse_esm(&buf).expect("parse_esm");
    assert_eq!(index.game, GameKind::Fallout3NV);
    assert_eq!(index.magic_effects.len(), 1);
    assert!(
        index.magic_effects_by_code.is_empty(),
        "non-Oblivion MGEFs must NOT populate magic_effects_by_code (#969) — \
         FormID-keyed `magic_effects` is the only valid lookup for these games"
    );
}

/// Defensive case for #969 — an Oblivion MGEF with an unexpected
/// EDID length (not exactly 4 bytes after null-strip) must not panic
/// and must not pollute `magic_effects_by_code`. Real Oblivion content
/// always uses 4-char codes, but mod content / corrupt files might
/// not, and the parser must remain robust.
#[test]
fn oblivion_mgef_with_non_4char_edid_skips_by_code_map() {
    let mut buf = build_oblivion_tes4();
    let mgef = build_record_obl(
        b"MGEF",
        0x0000_0333,
        &[
            (b"EDID", b"TooLong\0".to_vec()), // 7 chars — not Oblivion shape
            (b"DATA", 0u32.to_le_bytes().to_vec()),
        ],
    );
    buf.extend_from_slice(&wrap_group_obl(b"MGEF", &mgef));

    let index = parse_esm(&buf).expect("parse_esm");
    assert_eq!(index.game, GameKind::Oblivion);
    assert_eq!(index.magic_effects.len(), 1);
    assert!(
        index.magic_effects_by_code.is_empty(),
        "Oblivion MGEFs with non-4-char EDIDs must NOT populate the map (#969)"
    );
}

// ── #1277 Task 3: FO4-only record-type gate ─────────────────────────────
//
// SCOL / PKIN / MOVS / MSWP didn't exist before Fallout 4. Pre-gate they
// were parsed unconditionally so a cross-game plugin stack that injected
// these GRUPs into a non-FO4 master would silently consume them. The gate
// at `parse_esm` skips the whole GRUP when `GameKind` isn't FO4-or-later
// and warns once per record-type per parse. These tests pin the skip
// behavior so a regression that re-enables the unconditional dispatch
// produces visible test failures.

/// Build a TES4 file header with an HEDR sub-record carrying a specific
/// `Version` float. The HEDR sub-record's first 12 bytes are
/// `(version: f32, record_count: u32, next_form_id: u32)` per
/// `reader::read_file_header`'s decode. The `Version` float is what
/// `GameKind::from_header` keys on (FNV = 1.34, FO4 = 1.0, …).
fn tes4_with_hedr(version: f32) -> Vec<u8> {
    let mut hedr = Vec::with_capacity(12);
    hedr.extend_from_slice(&version.to_le_bytes());
    hedr.extend_from_slice(&0u32.to_le_bytes()); // record_count
    hedr.extend_from_slice(&0u32.to_le_bytes()); // next_form_id
    build_record(b"TES4", 0, &[(b"HEDR", hedr)])
}

/// Build a minimal record of the given type — no sub-records. Sufficient
/// for the FO4-gate test since we only care whether the GRUP was walked
/// (record landed in the typed map) or skipped (map stays empty).
fn minimal_record(typ: &[u8; 4], form_id: u32) -> Vec<u8> {
    build_record(typ, form_id, &[])
}

/// Pin: HEDR routes correctly. Sanity check before the gate tests — if
/// `GameKind::from_header` mis-classified the synthetic HEDR these
/// fixtures use, the gate tests below would be testing the wrong gate.
#[test]
fn synthetic_hedr_routes_to_expected_gamekind() {
    // FNV (1.34) → Fallout3NV
    let esm = tes4_with_hedr(1.34);
    let index = parse_esm(&esm).expect("parse_esm FNV");
    assert!(
        matches!(index.game, GameKind::Fallout3NV),
        "HEDR 1.34 must route to Fallout3NV, got {:?}",
        index.game,
    );
    // FO4 (1.0) → Fallout4
    let esm = tes4_with_hedr(1.0);
    let index = parse_esm(&esm).expect("parse_esm FO4");
    assert!(
        matches!(index.game, GameKind::Fallout4),
        "HEDR 1.0 must route to Fallout4, got {:?}",
        index.game,
    );
}

/// #1538 — SCOL is a Gamebryo-Fallout record (FO3 54, FNV 98), NOT FO4-only.
/// FNV must PARSE its SCOL GRUP; pre-fix the `is_fo4_plus` gate skipped it,
/// dropping all 98 FalloutNV.esm SCOL bases and orphaning 1084 REFRs.
#[test]
fn scol_grup_parsed_when_game_is_fnv() {
    let mut esm = tes4_with_hedr(1.34); // FNV
    esm.extend_from_slice(&wrap_group(b"SCOL", &minimal_record(b"SCOL", 0x0001_2345)));
    let index = parse_esm(&esm).expect("parse_esm");
    assert_eq!(
        index.cells.scols.len(),
        1,
        "FNV plugin's SCOL GRUP must be parsed (the is_fo4_plus gate wrongly \
         skipped it), got {} records",
        index.cells.scols.len(),
    );
    assert!(
        index.cells.scols.contains_key(&0x0001_2345),
        "form id 0x00012345 must appear in scols map",
    );
}

#[test]
fn scol_grup_parsed_when_game_is_fo4() {
    let mut esm = tes4_with_hedr(1.0); // FO4
    esm.extend_from_slice(&wrap_group(b"SCOL", &minimal_record(b"SCOL", 0x0001_2345)));
    let index = parse_esm(&esm).expect("parse_esm");
    assert_eq!(
        index.cells.scols.len(),
        1,
        "FO4 plugin's SCOL GRUP must be parsed, but the gate dropped it",
    );
    assert!(
        index.cells.scols.contains_key(&0x0001_2345),
        "form id 0x00012345 must appear in scols map",
    );
}

#[test]
fn pkin_grup_skipped_when_game_is_not_fo4_plus() {
    let mut esm = tes4_with_hedr(1.34); // FNV
    esm.extend_from_slice(&wrap_group(b"PKIN", &minimal_record(b"PKIN", 0x0055_0001)));
    let index = parse_esm(&esm).expect("parse_esm");
    assert!(
        index.cells.packins.is_empty(),
        "FNV plugin's PKIN GRUP must be skipped, found {} records",
        index.cells.packins.len(),
    );
}

#[test]
fn movs_grup_skipped_when_game_is_not_fo4_plus() {
    let mut esm = tes4_with_hedr(1.34); // FNV
    esm.extend_from_slice(&wrap_group(b"MOVS", &minimal_record(b"MOVS", 0x0044_0001)));
    let index = parse_esm(&esm).expect("parse_esm");
    assert!(
        index.cells.movables.is_empty(),
        "FNV plugin's MOVS GRUP must be skipped, found {} records",
        index.cells.movables.len(),
    );
}

#[test]
fn mswp_grup_skipped_when_game_is_not_fo4_plus() {
    let mut esm = tes4_with_hedr(1.34); // FNV
    esm.extend_from_slice(&wrap_group(b"MSWP", &minimal_record(b"MSWP", 0x0024_9A4E)));
    let index = parse_esm(&esm).expect("parse_esm");
    assert!(
        index.cells.material_swaps.is_empty(),
        "FNV plugin's MSWP GRUP must be skipped, found {} records",
        index.cells.material_swaps.len(),
    );
}

/// Skyrim (HEDR 1.7) also predates FO4; the gate must skip there too.
/// Separate test from FNV so a regression that only flipped the FNV arm
/// shows up as a specific Skyrim failure (and vice versa).
#[test]
fn scol_grup_skipped_for_skyrim_too() {
    let mut esm = tes4_with_hedr(1.7); // Skyrim
    esm.extend_from_slice(&wrap_group(b"SCOL", &minimal_record(b"SCOL", 0x0001_2345)));
    let index = parse_esm(&esm).expect("parse_esm");
    assert!(
        matches!(index.game, GameKind::Skyrim),
        "sanity: HEDR 1.7 routes to Skyrim",
    );
    assert!(
        index.cells.scols.is_empty(),
        "Skyrim plugin's SCOL GRUP must be skipped, found {} records",
        index.cells.scols.len(),
    );
}

/// FO76 (HEDR 68.0) is FO4+ — should parse. Pins the upper end of the
/// `is_fo4_plus` predicate against drift.
#[test]
fn scol_grup_parsed_for_fo76() {
    let mut esm = tes4_with_hedr(68.0); // FO76
    esm.extend_from_slice(&wrap_group(b"SCOL", &minimal_record(b"SCOL", 0x0001_2345)));
    let index = parse_esm(&esm).expect("parse_esm");
    assert!(
        matches!(index.game, GameKind::Fallout76),
        "sanity: HEDR 68.0 routes to Fallout76",
    );
    assert_eq!(
        index.cells.scols.len(),
        1,
        "FO76 plugin's SCOL GRUP must be parsed, but the gate dropped it",
    );
}

/// `merge_from` must carry `magic_effects_by_code` across DLC merges
/// with last-write-wins semantics, matching the `magic_effects` map
/// itself. Without this, a multi-plugin Oblivion load (Oblivion.esm +
/// DLC) would lose the side index from earlier plugins.
#[test]
fn merge_from_carries_magic_effects_by_code() {
    let mut a = EsmIndex::default();
    a.magic_effects_by_code.insert(*b"FIDG", 0x0000_0111);
    a.magic_effects_by_code.insert(*b"DGFA", 0x0000_0222);

    let mut b = EsmIndex::default();
    // Overrides "FIDG" — last-write-wins.
    b.magic_effects_by_code.insert(*b"FIDG", 0x0000_0AAA);
    b.magic_effects_by_code.insert(*b"DGAT", 0x0000_0BBB);

    a.merge_from(b);

    assert_eq!(a.magic_effects_by_code.len(), 3);
    assert_eq!(a.magic_effects_by_code.get(b"FIDG"), Some(&0x0000_0AAA));
    assert_eq!(a.magic_effects_by_code.get(b"DGFA"), Some(&0x0000_0222));
    assert_eq!(a.magic_effects_by_code.get(b"DGAT"), Some(&0x0000_0BBB));
}

/// Integration test for DIAL conversation tree resolution (WI-1.3).
/// Builds a DIAL + multiple INFOs with PNAM chains and TCLT edges;
/// asserts the tree correctly orders by PNAM and surfaces TCLT links.
#[test]
fn dial_conversation_tree_resolves_pnam_chains_and_tclt_edges() {
    use super::misc::ai::build_conversation_tree;

    // Build INFOs in scrambled order with a PNAM chain: A (head) <- B <- C
    // and TCLT edges from C to another topic.
    let info_a = build_record(
        b"INFO",
        0x1001,
        &[
            (b"NAM1", b"First response\0".to_vec()),
            (b"PNAM", 0u32.to_le_bytes().to_vec()), // Head: previous_info == 0
        ],
    );
    let info_c = build_record(
        b"INFO",
        0x1003,
        &[
            (b"NAM1", b"Third response\0".to_vec()),
            (b"PNAM", 0x1002u32.to_le_bytes().to_vec()), // Points back to B
            // TCLT to another topic
            (b"TCLT", 0x5555u32.to_le_bytes().to_vec()),
        ],
    );
    let info_b = build_record(
        b"INFO",
        0x1002,
        &[
            (b"NAM1", b"Second response\0".to_vec()),
            (b"PNAM", 0x1001u32.to_le_bytes().to_vec()), // Points back to A
        ],
    );

    // Topic Children sub-GRUP with all three INFOs (in C, B, A order to test scrambling).
    let topic_children = {
        let mut content = Vec::new();
        content.extend_from_slice(&info_c);
        content.extend_from_slice(&info_b);
        content.extend_from_slice(&info_a);
        let total = 24 + content.len();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GRUP");
        buf.extend_from_slice(&(total as u32).to_le_bytes());
        buf.extend_from_slice(&0xCAFEu32.to_le_bytes()); // label
        buf.extend_from_slice(&7u32.to_le_bytes()); // Topic Children
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&content);
        buf
    };

    // Parent DIAL.
    let dial = build_record(
        b"DIAL",
        0xCAFE,
        &[
            (b"EDID", b"MultiBranch\0".to_vec()),
            (b"FULL", b"Complex Topic\0".to_vec()),
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
    top_grup.extend_from_slice(&0u32.to_le_bytes());
    top_grup.extend_from_slice(&[0u8; 8]);
    top_grup.extend_from_slice(&top_content);

    // TES4 header + DIAL group.
    let mut buf = build_record(b"TES4", 0, &[]);
    buf.extend_from_slice(&top_grup);
    let index = parse_esm(&buf).expect("parse_esm");

    let dial = index.dialogues.get(&0xCAFE).expect("DIAL indexed");
    assert_eq!(dial.infos.len(), 3, "should have all 3 INFOs");

    // Build conversation tree from the parsed INFOs.
    let tree = build_conversation_tree(&dial.infos).expect("build_conversation_tree");

    // Should have one chain: [0x1001 (A), 0x1002 (B), 0x1003 (C)]
    assert_eq!(tree.chains.len(), 1, "should have 1 PNAM chain");
    assert_eq!(
        tree.chains[0],
        vec![0x1001, 0x1002, 0x1003],
        "PNAM chain should be ordered A→B→C"
    );

    // INFO C (0x1003) should have TCLT edge to 0x5555.
    assert_eq!(
        tree.topic_links.len(),
        1,
        "should have 1 INFO with TCLT edges"
    );
    assert_eq!(
        tree.topic_links.get(&0x1003),
        Some(&vec![0x5555]),
        "INFO C should link to topic 0x5555"
    );
}
