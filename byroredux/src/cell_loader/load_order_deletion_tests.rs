//! LMC-REC-004 — a deleted base record leaves the merged index and a deleted reference leaves its cell, measured on
//! real load orders against a raw record walk that shares nothing with the engine's reader or fold: for every form,
//! the last plugin in load order that carries it decides, and when that carrier is a deletion (the record header's
//! `0x20` flag) the form must be nowhere in the index the fold produced.

use super::{parse_record_indexes_in_load_order, LoadOrder};
use byroredux_plugin::esm::records::EsmIndex;
use std::collections::HashMap;

/// One record as the raw walk saw it.
#[derive(Debug, Clone, Copy)]
struct RawCarrier {
    record_type: [u8; 4],
    deleted: bool,
}

/// Every record header in `path` (FO3+ layout: 24-byte record and group headers), with its raw form ID; and the
/// plugin's master list from its TES4 header.
fn raw_records(path: &str) -> (Vec<String>, Vec<(u32, RawCarrier)>) {
    let data = std::fs::read(path).expect("read plugin");
    let u32_at = |at: usize| u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
    const HEADER: usize = 24;
    // TES4: its sub-records hold the masters (MAST, a NUL-terminated name).
    let tes4_size = u32_at(4) as usize;
    let mut masters = Vec::new();
    let mut at = HEADER;
    while at < HEADER + tes4_size {
        let tag = &data[at..at + 4];
        let size = u16::from_le_bytes([data[at + 4], data[at + 5]]) as usize;
        if tag == b"MAST" {
            let name = &data[at + 6..at + 6 + size];
            masters.push(String::from_utf8_lossy(name.split(|&b| b == 0).next().unwrap()).to_lowercase());
        }
        at += 6 + size;
    }
    let mut records = Vec::new();
    let mut at = HEADER + tes4_size;
    // Groups nest, and a group's size includes its header, so stepping into a group is stepping past its header.
    while at + HEADER <= data.len() {
        let size = u32_at(at + 4) as usize;
        if &data[at..at + 4] == b"GRUP" {
            at += HEADER;
            continue;
        }
        let record_type: [u8; 4] = data[at..at + 4].try_into().unwrap();
        let flags = u32_at(at + 8);
        records.push((u32_at(at + 12), RawCarrier { record_type, deleted: flags & 0x20 != 0 }));
        at += HEADER + size;
    }
    (masters, records)
}

/// Whether a record of `record_type` is a cell, a placement, a navmesh or a dialogue response: kept outside the
/// category maps, or in a category whose prune is a no-op (`cells`).
fn nested(record_type: [u8; 4]) -> bool {
    matches!(
        &record_type,
        b"CELL"
            | b"REFR"
            | b"ACHR"
            | b"ACRE"
            | b"PGRE"
            | b"PMIS"
            | b"PHZD"
            | b"PARW"
            | b"PBAR"
            | b"PBEA"
            | b"PCON"
            | b"PFLA"
            | b"NAVM"
            | b"INFO"
    )
}

/// Every cell the index holds: interior, exterior and worldspace-persistent.
fn all_cells(index: &EsmIndex) -> impl Iterator<Item = &byroredux_plugin::esm::cell::CellData> {
    let cells = &index.cells;
    cells.cells.values().chain(cells.exterior_cells.values().flat_map(|grid| grid.values())).chain(cells.worldspace_persistent_cells.values())
}

/// Whether `form_id`, a record of a [`nested`] type, is anywhere in `index` such a record lives.
fn nested_present(index: &EsmIndex, record_type: [u8; 4], form_id: u32) -> bool {
    match &record_type {
        b"CELL" => all_cells(index).any(|cell| cell.form_id == form_id),
        b"NAVM" => index.navmeshes.contains_key(&form_id),
        b"INFO" => index.dialogues.values().any(|topic| topic.infos.iter().any(|info| info.form_id == form_id)),
        _ => all_cells(index).any(|cell| cell.references.iter().any(|placed| placed.form_id == form_id)),
    }
}

/// Whether `form_id`, a base record, is in `index`: its type metadata, the modelled base objects (`cells.statics`,
/// whose category prune is a no-op), or any other category's records. The last is asked by pruning the form and
/// counting, which removes it, so ask once per form.
fn base_present_then_prune(index: &mut EsmIndex, form_id: u32) -> bool {
    if index.record_types.contains_key(&form_id) || index.cells.statics.contains_key(&form_id) {
        return true;
    }
    let before: Vec<usize> = EsmIndex::categories().iter().map(|(_, count, _, _)| count(index)).collect();
    let gone = std::iter::once(form_id).collect();
    for (_, _, _, prune) in EsmIndex::categories() {
        prune(index, &gone);
    }
    EsmIndex::categories().iter().zip(before).any(|((_, count, _, _), before)| count(index) != before)
}

/// What the probe measured on one load order.
struct Measured {
    /// Per record type, the forms whose chain ends in a deletion.
    deletions: HashMap<[u8; 4], usize>,
    /// Those the fold kept: form, type, the deleting plugin.
    kept: Vec<(u32, [u8; 4], String)>,
    /// The control: overrides of the same types whose last carrier is live, and how many the lookups found.
    live_overrides: usize,
    live_found: usize,
}

/// The forms whose last carrier deletes them but which the fold kept, and the control that shows the lookups find a
/// live override of the same types.
fn kept_deletions(paths: &[String]) -> Measured {
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let (mut index, order): (EsmIndex, LoadOrder) = parse_record_indexes_in_load_order(&refs).expect("fold");
    // Last carrier per global form, in load order.
    let mut last: HashMap<u32, (usize, RawCarrier)> = HashMap::new();
    let mut carried: HashMap<u32, usize> = HashMap::new();
    for (position, path) in paths.iter().enumerate() {
        let (masters, records) = raw_records(path);
        let slot_of = |raw: u32| {
            let master = (raw >> 24) as usize;
            let position = masters
                .get(master)
                .map(|name| order.iter().position(|plugin| plugin.eq_ignore_ascii_case(name)).expect("master in the order"))
                .unwrap_or(position);
            order.slots()[position]
        };
        for (raw, carrier) in records {
            if raw == 0 {
                continue;
            }
            let form = slot_of(raw).compose(raw);
            last.insert(form, (position, carrier));
            *carried.entry(form).or_default() += 1;
        }
    }
    let mut deletions: HashMap<[u8; 4], usize> = HashMap::new();
    let deleted_types: std::collections::HashSet<[u8; 4]> =
        last.values().filter(|(_, carrier)| carrier.deleted).map(|(_, carrier)| carrier.record_type).collect();
    // The control: a live override of a type some deletion has must be found by the same lookup.
    let mut live: Vec<(u32, [u8; 4])> = last
        .iter()
        .filter(|(form, (_, carrier))| !carrier.deleted && carried[*form] > 1 && deleted_types.contains(&carrier.record_type))
        .map(|(&form, (_, carrier))| (form, carrier.record_type))
        .collect();
    live.sort_unstable_by_key(|&(form, record_type)| (!nested(record_type), form));
    let live_overrides = live.len();
    let live_found = live
        .iter()
        .filter(|&&(form, record_type)| nested(record_type) && nested_present(&index, record_type, form))
        .count();
    let mut ended: Vec<(u32, usize, RawCarrier)> =
        last.into_iter().filter(|(_, (_, carrier))| carrier.deleted).map(|(form, (position, carrier))| (form, position, carrier)).collect();
    // The nested records first: pruning a base form must not run before every lookup that reads the index whole.
    ended.sort_unstable_by_key(|&(form, _, carrier)| (!nested(carrier.record_type), form));
    let mut kept = Vec::new();
    for (form_id, position, carrier) in ended {
        *deletions.entry(carrier.record_type).or_default() += 1;
        let present = if nested(carrier.record_type) {
            nested_present(&index, carrier.record_type, form_id)
        } else {
            base_present_then_prune(&mut index, form_id)
        };
        if present {
            kept.push((form_id, carrier.record_type, order[position].clone()));
        }
    }
    kept.sort_unstable();
    // The base-record half of the control last: its lookup prunes what it finds.
    let live_found = live_found
        + live.iter().filter(|&&(form, record_type)| !nested(record_type) && base_present_then_prune(&mut index, form)).count();
    Measured { deletions, kept, live_overrides, live_found }
}

fn report(game: &str, paths: &[String]) -> Vec<(u32, [u8; 4], String)> {
    let Measured { deletions, kept, live_overrides, live_found } = kept_deletions(paths);
    eprintln!("{game}: control: {live_found} of {live_overrides} live overrides of the deleted types found");
    assert!(live_found * 100 >= live_overrides * 99, "{game}: the lookups miss live overrides, so they cannot see a kept deletion");
    let mut types: Vec<_> = deletions.into_iter().collect();
    types.sort_unstable();
    let types: Vec<String> = types.iter().map(|(tag, count)| format!("{} {count}", String::from_utf8_lossy(tag))).collect();
    eprintln!("{game}: deletions that end their form's chain: {}", types.join(", "));
    for (form_id, tag, plugin) in &kept {
        eprintln!("  kept: {form_id:08X} {} deleted by {plugin}", String::from_utf8_lossy(tag));
    }
    kept
}

/// `names` found in `data`, whatever their case on disk; `None` when any is missing.
fn paths_in(data: std::path::PathBuf, names: &[&str]) -> Option<Vec<String>> {
    let files: Vec<std::path::PathBuf> = std::fs::read_dir(&data).ok()?.flatten().map(|entry| entry.path()).collect();
    names
        .iter()
        .map(|name| {
            files
                .iter()
                .find(|path| path.file_name().is_some_and(|file| file.to_string_lossy().eq_ignore_ascii_case(name)))
                .map(|path| path.to_string_lossy().into_owned())
        })
        .collect()
}

#[test]
#[ignore = "parses FalloutNV.esm and its DLC; run with --ignored"]
fn fnv_deletions_leave_the_merged_index() {
    let names = ["FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm", "GunRunnersArsenal.esm"];
    let Some(paths) = paths_in(byroredux_plugin::esm::test_paths::fnv_data_dir(), &names) else {
        byroredux_plugin::esm::test_paths::require_or_skip("Fallout: New Vegas with its DLC");
        return;
    };
    assert!(report("FNV", &paths).is_empty());
}

#[test]
#[ignore = "parses Skyrim.esm and its DLC; run with --ignored"]
fn skyrim_deletions_leave_the_merged_index() {
    let names = ["Skyrim.esm", "Update.esm", "Dawnguard.esm", "HearthFires.esm", "Dragonborn.esm"];
    let Some(paths) = paths_in(byroredux_plugin::esm::test_paths::skyrim_se_data_dir(), &names) else {
        byroredux_plugin::esm::test_paths::require_or_skip("Skyrim SE with its DLC");
        return;
    };
    assert!(report("Skyrim SE", &paths).is_empty());
}

#[test]
#[ignore = "parses Fallout4.esm, its DLC, the Creation Club plugins the Unofficial Fallout 4 Patch masters and the patch; run with --ignored"]
fn fo4_deletions_leave_the_merged_index() {
    let names = [
        "Fallout4.esm", "DLCRobot.esm", "DLCworkshop01.esm", "DLCCoast.esm", "DLCworkshop02.esm", "DLCworkshop03.esm",
        "DLCNukaWorld.esm", "ccbgsfo4044-hellfirepowerarmor.esl", "ccbgsfo4115-x02.esl", "ccbgsfo4116-heavyflamer.esl",
        "ccbgsfo4110-ws_enclave.esl", "ccbgsfo4096-as_enclave.esl", "ccfsvfo4007-halloween.esl", "ccbgsfo4046-tescan.esl",
        "ccsbjfo4003-grenade.esl", "ccotmfo4001-remnants.esl", "unofficial fallout 4 patch.esp",
    ];
    let Some(paths) = paths_in(byroredux_plugin::esm::test_paths::fo4_data_dir(), &names) else {
        byroredux_plugin::esm::test_paths::require_or_skip("Fallout 4 with its DLC and the Unofficial Fallout 4 Patch");
        return;
    };
    assert!(report("Fallout 4", &paths).is_empty());
}
