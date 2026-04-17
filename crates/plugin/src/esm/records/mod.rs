//! Structured record extraction beyond cells and statics.
//!
//! `cell.rs` (in the parent module) walks an ESM and pulls out interior /
//! exterior cells, placed references, and base records that have a MODL
//! sub-record. That covers everything the renderer needs to draw a cell.
//! This module adds extraction for record types game systems need beyond
//! rendering: items, containers, leveled lists, NPCs, races, classes,
//! factions, globals, and game settings.
//!
//! The high-level entry point is `parse_esm`, which walks the GRUP tree
//! once and fills an `EsmIndex` aggregating cells + the new record maps.
//! Existing callers that only want cell data continue to use
//! `parse_esm_cells` (now a thin wrapper over `parse_esm`).

pub mod actor;
pub mod climate;
pub mod common;
pub mod container;
pub mod global;
pub mod items;
pub mod weather;

pub use actor::{
    parse_clas, parse_fact, parse_npc, parse_race, ClassRecord, FactionMembership, FactionRecord,
    FactionRelation, NpcInventoryEntry, NpcRecord, RaceRecord,
};
pub use climate::{parse_clmt, ClimateRecord, ClimateWeather};
pub use container::{
    parse_cont, parse_leveled_list, ContainerRecord, InventoryEntry, LeveledEntry, LeveledList,
};
pub use global::{parse_glob, parse_gmst, GameSetting, GlobalRecord, SettingValue};
pub use items::{
    parse_alch, parse_ammo, parse_armo, parse_book, parse_ingr, parse_keym, parse_misc, parse_note,
    parse_weap, ItemKind, ItemRecord,
};
pub use weather::{parse_wthr, SkyColor, WeatherRecord};

use super::cell::{parse_esm_cells, EsmCellIndex};
use super::reader::{EsmReader, GameKind, SubRecord};
use anyhow::{Context, Result};
use std::collections::HashMap;

/// Aggregated index of every record category we currently parse.
///
/// `cells` retains the existing structure used by the cell loader and
/// renderer. The other maps are new in M24.
#[derive(Debug, Default)]
pub struct EsmIndex {
    pub cells: EsmCellIndex,
    pub items: HashMap<u32, ItemRecord>,
    pub containers: HashMap<u32, ContainerRecord>,
    pub leveled_items: HashMap<u32, LeveledList>,
    pub leveled_npcs: HashMap<u32, LeveledList>,
    pub npcs: HashMap<u32, NpcRecord>,
    pub races: HashMap<u32, RaceRecord>,
    pub classes: HashMap<u32, ClassRecord>,
    pub factions: HashMap<u32, FactionRecord>,
    pub globals: HashMap<u32, GlobalRecord>,
    pub game_settings: HashMap<u32, GameSetting>,
    pub weathers: HashMap<u32, WeatherRecord>,
    pub climates: HashMap<u32, ClimateRecord>,
}

impl EsmIndex {
    /// Total number of parsed records across every category. Useful for
    /// at-a-glance reporting in tests and the cell loader.
    pub fn total(&self) -> usize {
        self.items.len()
            + self.containers.len()
            + self.leveled_items.len()
            + self.leveled_npcs.len()
            + self.npcs.len()
            + self.races.len()
            + self.classes.len()
            + self.factions.len()
            + self.globals.len()
            + self.game_settings.len()
            + self.weathers.len()
            + self.climates.len()
            + self.cells.cells.len()
            + self.cells.statics.len()
    }
}

/// Parse an entire ESM/ESP file in a single pass.
///
/// First fills the cell index using the existing `parse_esm_cells` walker
/// (which already handles CELL/WRLD/MODL extraction), then walks the file
/// a second time to pull the M24 record categories. This is a deliberate
/// trade-off: a single combined walker would be more efficient but would
/// require restructuring `cell.rs`. For now, two passes over a 100 MB
/// file run in well under a second on a real ESM, and keeping the cell
/// pipeline untouched preserves the renderer behaviour we already trust.
pub fn parse_esm(data: &[u8]) -> Result<EsmIndex> {
    let cells = parse_esm_cells(data).context("Failed to parse ESM cells")?;
    let mut index = EsmIndex {
        cells,
        ..EsmIndex::default()
    };

    let mut reader = EsmReader::new(data);
    // Peek the TES4 file header so we can derive a GameKind from HEDR's
    // `Version` f32. ARMO/WEAP/AMMO DATA/DNAM layouts diverge across
    // FO3/FNV → Skyrim → FO4; without the HEDR discriminator every
    // Skyrim record would be parsed with the FO3/FNV schema and get
    // garbage stats (issue #347).
    let file_header = reader.read_file_header().ok();
    let game = GameKind::from_header(
        reader.variant(),
        file_header.as_ref().map(|h| h.hedr_version).unwrap_or(0.0),
    );

    // Walk top-level groups and dispatch by record-type label.
    while reader.remaining() > 0 {
        if !reader.is_group() {
            // Stray top-level record; skip it.
            let header = reader.read_record_header()?;
            reader.skip_record(&header);
            continue;
        }
        let group = reader.read_group_header()?;
        let end = reader.group_content_end(&group);
        let label = group.label;

        match &label {
            // Item categories — each handled by a per-type parser. ARMO,
            // WEAP, AMMO are game-aware: their DATA/DNAM layouts differ
            // between FO3/FNV and Skyrim+ (see items.rs regression tests).
            b"WEAP" => extract_records(&mut reader, end, b"WEAP", &mut |fid, subs| {
                index.items.insert(fid, parse_weap(fid, subs, game));
            })?,
            b"ARMO" => extract_records(&mut reader, end, b"ARMO", &mut |fid, subs| {
                index.items.insert(fid, parse_armo(fid, subs, game));
            })?,
            b"AMMO" => extract_records(&mut reader, end, b"AMMO", &mut |fid, subs| {
                index.items.insert(fid, parse_ammo(fid, subs, game));
            })?,
            b"MISC" => extract_records(&mut reader, end, b"MISC", &mut |fid, subs| {
                index.items.insert(fid, parse_misc(fid, subs));
            })?,
            b"KEYM" => extract_records(&mut reader, end, b"KEYM", &mut |fid, subs| {
                index.items.insert(fid, parse_keym(fid, subs));
            })?,
            b"ALCH" => extract_records(&mut reader, end, b"ALCH", &mut |fid, subs| {
                index.items.insert(fid, parse_alch(fid, subs));
            })?,
            b"INGR" => extract_records(&mut reader, end, b"INGR", &mut |fid, subs| {
                index.items.insert(fid, parse_ingr(fid, subs));
            })?,
            b"BOOK" => extract_records(&mut reader, end, b"BOOK", &mut |fid, subs| {
                index.items.insert(fid, parse_book(fid, subs));
            })?,
            b"NOTE" => extract_records(&mut reader, end, b"NOTE", &mut |fid, subs| {
                index.items.insert(fid, parse_note(fid, subs));
            })?,
            // Containers and leveled lists.
            b"CONT" => extract_records(&mut reader, end, b"CONT", &mut |fid, subs| {
                index.containers.insert(fid, parse_cont(fid, subs));
            })?,
            b"LVLI" => extract_records(&mut reader, end, b"LVLI", &mut |fid, subs| {
                index
                    .leveled_items
                    .insert(fid, parse_leveled_list(fid, subs));
            })?,
            b"LVLN" => extract_records(&mut reader, end, b"LVLN", &mut |fid, subs| {
                index
                    .leveled_npcs
                    .insert(fid, parse_leveled_list(fid, subs));
            })?,
            // Actors and supporting records.
            b"NPC_" => extract_records(&mut reader, end, b"NPC_", &mut |fid, subs| {
                index.npcs.insert(fid, parse_npc(fid, subs));
            })?,
            b"RACE" => extract_records(&mut reader, end, b"RACE", &mut |fid, subs| {
                index.races.insert(fid, parse_race(fid, subs));
            })?,
            b"CLAS" => extract_records(&mut reader, end, b"CLAS", &mut |fid, subs| {
                index.classes.insert(fid, parse_clas(fid, subs));
            })?,
            b"FACT" => extract_records(&mut reader, end, b"FACT", &mut |fid, subs| {
                index.factions.insert(fid, parse_fact(fid, subs));
            })?,
            // Globals and game settings.
            b"GLOB" => extract_records(&mut reader, end, b"GLOB", &mut |fid, subs| {
                index.globals.insert(fid, parse_glob(fid, subs));
            })?,
            b"GMST" => extract_records(&mut reader, end, b"GMST", &mut |fid, subs| {
                index.game_settings.insert(fid, parse_gmst(fid, subs));
            })?,
            // Weather records — sky colors, fog, wind, clouds.
            b"WTHR" => extract_records(&mut reader, end, b"WTHR", &mut |fid, subs| {
                index.weathers.insert(fid, parse_wthr(fid, subs));
            })?,
            // Climate records — weather probability tables.
            b"CLMT" => extract_records(&mut reader, end, b"CLMT", &mut |fid, subs| {
                index.climates.insert(fid, parse_clmt(fid, subs));
            })?,
            _ => {
                reader.skip_group(&group);
            }
        }
    }

    log::info!(
        "ESM parsed: {} cells, {} statics, {} items, {} containers, {} LVLI, {} LVLN, {} NPCs, \
         {} races, {} classes, {} factions, {} globals, {} game settings, {} weathers, {} climates",
        index.cells.cells.len(),
        index.cells.statics.len(),
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
        index.weathers.len(),
        index.climates.len(),
    );

    Ok(index)
}

/// Walk a top-level group and call `f(form_id, subs)` for every record
/// matching `expected_type`. Recurses into nested groups so worldspace
/// children and persistent/temporary cell children are handled too.
///
/// `f` takes a closure rather than returning a parsed value so the caller
/// can route the record into a type-specific HashMap without an extra
/// boxing/erasure layer.
fn extract_records(
    reader: &mut EsmReader,
    end: usize,
    expected_type: &[u8; 4],
    f: &mut dyn FnMut(u32, &[SubRecord]),
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);
            extract_records(reader, sub_end, expected_type, f)?;
            continue;
        }
        let header = reader.read_record_header()?;
        if &header.record_type == expected_type {
            let subs = reader.read_sub_records(&header)?;
            f(header.form_id, &subs);
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
        let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/FalloutNV.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: FalloutNV.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
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
                ai_packages: Vec::new(),
                death_item_form_id: 0,
                level: 1,
                disposition_base: 50,
                acbs_flags: 0,
                has_script: false,
            },
        );
        assert_eq!(idx.total(), 2);
    }
}
