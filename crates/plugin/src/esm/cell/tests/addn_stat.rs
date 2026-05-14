//! ADDN + STAT + MODL group walk tests.
//!
//! Addon-node DATA/DNAM extraction, STAT with/without VMAD scripts, Oblivion
//! MODL 20-byte group header walk, CREA record indexing.

use super::super::super::reader::EsmReader;
use super::super::support::parse_modl_group;
use super::super::*;

// Helper: build minimal STAT record bytes.
fn build_stat_record(form_id: u32, editor_id: &str, model_path: &str) -> Vec<u8> {
    let mut sub_data = Vec::new();
    // EDID
    let edid = format!("{}\0", editor_id);
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());
    // MODL
    let modl = format!("{}\0", model_path);
    sub_data.extend_from_slice(b"MODL");
    sub_data.extend_from_slice(&(modl.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(modl.as_bytes());

    let mut buf = Vec::new();
    buf.extend_from_slice(b"STAT");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&sub_data);
    buf
}

// Helper: build minimal ADDN record bytes with DATA (s32 index) +
// DNAM (u16 cap, u16 flags). Optional EDID / MODL included. See #370.
fn build_addn_record(
    form_id: u32,
    editor_id: &str,
    model_path: &str,
    addon_index: i32,
    cap: u16,
    flags: u16,
) -> Vec<u8> {
    let mut sub_data = Vec::new();
    // EDID
    let edid = format!("{}\0", editor_id);
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());
    // MODL
    let modl = format!("{}\0", model_path);
    sub_data.extend_from_slice(b"MODL");
    sub_data.extend_from_slice(&(modl.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(modl.as_bytes());
    // DATA: s32 addon_index
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&addon_index.to_le_bytes());
    // DNAM: u16 cap + u16 flags
    sub_data.extend_from_slice(b"DNAM");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&cap.to_le_bytes());
    sub_data.extend_from_slice(&flags.to_le_bytes());

    let mut buf = Vec::new();
    buf.extend_from_slice(b"ADDN");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&sub_data);
    buf
}


#[test]
fn parse_addn_extracts_data_and_dnam() {
    // Regression: #370 — ADDN DATA (s32 addon index) and DNAM
    // (u16 cap + u16 flags) must both land on StaticObject.addon_data.
    let addn = build_addn_record(
        0x4567,
        "MothSwarm01",
        "meshes\\effects\\moths.nif",
        7,
        64,
        1,
    );
    let total_size = 24 + addn.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"ADDN");
    group.extend_from_slice(&0u32.to_le_bytes());
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&addn);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let s = statics.get(&0x4567).expect("ADDN entry present");
    assert_eq!(s.editor_id, "MothSwarm01");
    assert_eq!(s.model_path, "meshes\\effects\\moths.nif");
    let ad = s.addon_data.expect("addon_data populated");
    assert_eq!(ad.addon_index, 7);
    assert_eq!(ad.master_particle_cap, 64);
    assert_eq!(ad.flags, 1);
}

#[test]
fn parse_stat_with_vmad_sets_has_script() {
    // Regression: #369 — Skyrim VMAD sub-records on STAT records
    // were dropped on the walker's `_` arm. The minimum-viable
    // signal is a `has_script: bool` on `StaticObject` so the count
    // of script-bearing records is at least visible. Full VMAD
    // decoding (Papyrus script names + property bindings) stays
    // gated on the scripting-as-ECS work.
    let mut sub_data = Vec::new();
    let edid = "ScriptedDoor\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());
    let modl = "meshes\\door.nif\0";
    sub_data.extend_from_slice(b"MODL");
    sub_data.extend_from_slice(&(modl.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(modl.as_bytes());
    // VMAD: opaque payload — content doesn't matter for the
    // presence flag, only that the sub-record exists.
    let vmad_payload: &[u8] = b"\x05\x00\x02\x00\x00\x00";
    sub_data.extend_from_slice(b"VMAD");
    sub_data.extend_from_slice(&(vmad_payload.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(vmad_payload);

    let mut stat = Vec::new();
    stat.extend_from_slice(b"STAT");
    stat.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    stat.extend_from_slice(&0u32.to_le_bytes());
    stat.extend_from_slice(&0x77u32.to_le_bytes());
    stat.extend_from_slice(&[0u8; 8]);
    stat.extend_from_slice(&sub_data);

    let total_size = 24 + stat.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"STAT");
    group.extend_from_slice(&0u32.to_le_bytes());
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&stat);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let s = statics.get(&0x77).expect("STAT entry present");
    assert!(s.has_script, "VMAD presence must flip has_script");
}

#[test]
fn parse_stat_without_vmad_leaves_has_script_false() {
    // Sibling check — a STAT with only EDID + MODL keeps has_script
    // at false. Catches a regression where the new arm captures
    // some other neighbour sub-record.
    let stat = build_stat_record(0x88, "PlainStatic", "meshes\\stat.nif");
    let total_size = 24 + stat.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"STAT");
    group.extend_from_slice(&0u32.to_le_bytes());
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&stat);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let s = statics.get(&0x88).expect("STAT entry present");
    assert!(!s.has_script);
}

#[test]
fn parse_non_addn_record_has_no_addon_data() {
    // STATs must not accidentally populate addon_data even if a
    // same-named DATA sub-record happens to exist.
    let stat = build_stat_record(0x9999, "RegularWall", "meshes\\wall.nif");
    let total_size = 24 + stat.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"STAT");
    group.extend_from_slice(&0u32.to_le_bytes());
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&stat);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let s = statics.get(&0x9999).expect("STAT entry");
    assert!(s.addon_data.is_none(), "STAT must not carry addon data");
}

#[test]
fn parse_stat_record() {
    let stat = build_stat_record(0x1234, "TestWall", "meshes\\architecture\\wall01.nif");
    // Wrap in a GRUP.
    let total_size = 24 + stat.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"STAT");
    group.extend_from_slice(&0u32.to_le_bytes()); // group_type = 0 (top)
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&stat);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    assert_eq!(statics.len(), 1);
    let s = statics.get(&0x1234).unwrap();
    assert_eq!(s.editor_id, "TestWall");
    assert_eq!(s.model_path, "meshes\\architecture\\wall01.nif");
}

#[test]
fn parse_modl_group_walks_oblivion_20byte_headers() {
    // Regression: #391 — the walker used to compute a group's content
    // end as `position + total_size - 24`, hardcoding the Tes5Plus
    // header size. On Oblivion that over-reads by 4 bytes; symptoms
    // were latent (the next read happened to land on a self-delimiting
    // GRUP) but any bounds-checked nested parse would have read junk.
    //
    // Build an Oblivion-shaped (20-byte header) STAT group with two
    // STAT records, run it through `parse_modl_group` using the
    // explicit `Oblivion` reader variant, and assert: both records
    // extracted, no leftover bytes, no junk record dispatched after
    // the second.
    use super::super::super::reader::EsmVariant;

    // Build a 20-byte-header STAT record (Oblivion layout).
    fn build_stat_oblivion(form_id: u32, edid: &str, modl: &str) -> Vec<u8> {
        let mut sub_data = Vec::new();
        let edid_z = format!("{}\0", edid);
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid_z.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid_z.as_bytes());
        let modl_z = format!("{}\0", modl);
        sub_data.extend_from_slice(b"MODL");
        sub_data.extend_from_slice(&(modl_z.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(modl_z.as_bytes());

        let mut buf = Vec::new();
        buf.extend_from_slice(b"STAT");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes());
        buf.extend_from_slice(&[0u8; 4]); // Oblivion vc_info (4 bytes)
        buf.extend_from_slice(&sub_data);
        buf
    }

    let r1 = build_stat_oblivion(0x111, "WallA", "meshes\\a.nif");
    let r2 = build_stat_oblivion(0x222, "WallB", "meshes\\b.nif");
    let mut content = Vec::new();
    content.extend_from_slice(&r1);
    content.extend_from_slice(&r2);

    // 20-byte group header.
    let total_size = 20 + content.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"STAT");
    group.extend_from_slice(&0u32.to_le_bytes()); // group_type
    group.extend_from_slice(&[0u8; 4]); // Oblivion stamp (4 bytes)
    group.extend_from_slice(&content);

    // Append a sentinel byte beyond the group end. With the old
    // `-24` walker this byte would land inside the computed end and
    // get dispatched as part of the next read; with the fix the
    // walker stops cleanly at byte `total_size`.
    group.push(0xEE);

    let mut reader = EsmReader::with_variant(&group, EsmVariant::Oblivion);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    // Content end must sit immediately before the sentinel, not past it.
    assert_eq!(end, total_size);

    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    assert_eq!(statics.len(), 2, "both Oblivion STATs must be parsed");
    assert_eq!(statics.get(&0x111).unwrap().editor_id, "WallA");
    assert_eq!(statics.get(&0x222).unwrap().editor_id, "WallB");
    // Walker must have stopped exactly at `end`, leaving the
    // sentinel byte for the caller.
    assert_eq!(reader.position(), end);
    assert_eq!(reader.remaining(), 1);
}

/// Regression: #396 — Oblivion CREA (base creature record) must
/// reach `parse_modl_group` so its EDID + MODL land in the
/// statics map. Pre-fix `parse_esm_cells` didn't include CREA in
/// the MODL match arm, so the CREA group was skipped wholesale
/// before parse_modl_group ever saw a record. CREA uses the
/// standard MODL sub-record (identical layout to STAT), so once
/// the dispatcher routes it through, the data path is unchanged.
///
/// Mirrors `parse_modl_group_walks_oblivion_20byte_headers` but
/// with a single CREA record + Oblivion 20-byte headers (CREA
/// only ships on Oblivion / FO3 / FNV — FO3+ folded creatures
/// into NPC_).
#[test]
fn parse_modl_group_indexes_oblivion_crea_records() {
    use super::super::super::reader::EsmVariant;

    // CREA record with EDID + MODL (Oblivion 20-byte header).
    let mut crea_sub = Vec::new();
    let edid = "Goblin\0";
    crea_sub.extend_from_slice(b"EDID");
    crea_sub.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    crea_sub.extend_from_slice(edid.as_bytes());
    let model = "creatures\\goblin\\goblin.nif\0";
    crea_sub.extend_from_slice(b"MODL");
    crea_sub.extend_from_slice(&(model.len() as u16).to_le_bytes());
    crea_sub.extend_from_slice(model.as_bytes());

    let mut crea_record = Vec::new();
    crea_record.extend_from_slice(b"CREA");
    crea_record.extend_from_slice(&(crea_sub.len() as u32).to_le_bytes());
    crea_record.extend_from_slice(&0u32.to_le_bytes()); // flags
    crea_record.extend_from_slice(&0x000A_0001u32.to_le_bytes()); // form_id
    crea_record.extend_from_slice(&[0u8; 4]); // Oblivion vc_info (4 bytes)
    crea_record.extend_from_slice(&crea_sub);

    // 20-byte GRUP header wrapping the CREA record.
    let total_size = 20 + crea_record.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"CREA");
    group.extend_from_slice(&0u32.to_le_bytes()); // group_type = 0 (top-level)
    group.extend_from_slice(&[0u8; 4]); // Oblivion stamp (4 bytes)
    group.extend_from_slice(&crea_record);

    let mut reader = EsmReader::with_variant(&group, EsmVariant::Oblivion);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let crea = statics
        .get(&0x000A_0001)
        .expect("CREA record must be indexed by form_id");
    assert_eq!(crea.editor_id, "Goblin");
    assert_eq!(crea.model_path, "creatures\\goblin\\goblin.nif");
}
