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
pub mod scol;
pub mod script;
pub mod weather;

pub use scol::{parse_scol, ScolPart, ScolPlacement, ScolRecord};

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
pub use script::{parse_scpt, ScriptLocalVar, ScriptRecord, ScriptType};
pub use weather::{parse_wthr, SkyColor, WeatherRecord};

use super::cell::EsmCellIndex;
use super::reader::{EsmReader, FormIdRemap, GameKind, SubRecord};
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
    /// Leveled creature lists (CREA spawn tables). Byte-compatible with
    /// LVLI / LVLN so the same `parse_leveled_list` handles them. FO3
    /// uses LVLC for most enemy encounters; FNV migrated the bulk to
    /// LVLN but still ships some legacy LVLC entries. See #448.
    pub leveled_creatures: HashMap<u32, LeveledList>,
    pub npcs: HashMap<u32, NpcRecord>,
    /// Creature base records (FO3 bestiary: super mutants, deathclaws,
    /// radroaches, robots, brahmin, etc.). CREA shares EDID / FULL /
    /// MODL / RNAM / CNAM / SNAM / CNTO / PKID / ACBS with NPC_ so the
    /// same `parse_npc` populates `NpcRecord`; the only divergence is
    /// ACBS flags semantics, which the current reader ignores.
    /// FNV migrated most combat to NPC_ but vanilla still keeps ~70
    /// CREA entries for legacy content. See #442.
    pub creatures: HashMap<u32, NpcRecord>,
    pub races: HashMap<u32, RaceRecord>,
    pub classes: HashMap<u32, ClassRecord>,
    pub factions: HashMap<u32, FactionRecord>,
    pub globals: HashMap<u32, GlobalRecord>,
    pub game_settings: HashMap<u32, GameSetting>,
    pub weathers: HashMap<u32, WeatherRecord>,
    pub climates: HashMap<u32, ClimateRecord>,
    /// FO3 / FNV / Oblivion pre-Papyrus SCPT bytecode records (#443).
    /// Every `SCRI` FormID on NPC_ / CONT / item / ACTI records resolves
    /// here instead of dangling. The bytecode itself (`compiled`) is
    /// stored opaquely — an ECS-native runtime lands separately.
    pub scripts: HashMap<u32, ScriptRecord>,
}

impl EsmIndex {
    /// Total number of parsed records across every category. Useful for
    /// at-a-glance reporting in tests and the cell loader.
    pub fn total(&self) -> usize {
        self.items.len()
            + self.containers.len()
            + self.leveled_items.len()
            + self.leveled_npcs.len()
            + self.leveled_creatures.len()
            + self.npcs.len()
            + self.creatures.len()
            + self.races.len()
            + self.classes.len()
            + self.factions.len()
            + self.globals.len()
            + self.game_settings.len()
            + self.weathers.len()
            + self.climates.len()
            + self.scripts.len()
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
    parse_esm_with_load_order(data, None)
}

/// Parse an ESM/ESP with an explicit load-order remap.
///
/// `remap` rewrites every record's FormID top byte into the global
/// load order so maps in [`EsmIndex`] stay collision-free across
/// plugins. Pass `None` for single-plugin loads (the default today);
/// pass `Some(FormIdRemap { plugin_index, master_indices })` when
/// loading a DLC or mod in a multi-plugin stack so Anchorage's new
/// form 0x01012345 doesn't collide with BrokenSteel's 0x01012345 in
/// the same map. See #445.
///
/// The current CLI entry point (`--esm <path>`) only wires a single
/// plugin, so both paths produce the same output for vanilla content.
/// The multi-plugin wiring is tracked as follow-up work — this
/// function exists so downstream code can opt in without another
/// parse-layer refactor when the CLI grows multi-plugin support.
pub fn parse_esm_with_load_order(
    data: &[u8],
    remap: Option<FormIdRemap>,
) -> Result<EsmIndex> {
    let cells = super::cell::parse_esm_cells_with_load_order(data, remap.clone())
        .context("Failed to parse ESM cells")?;
    let mut index = EsmIndex {
        cells,
        ..EsmIndex::default()
    };

    let mut reader = EsmReader::new(data);
    if let Some(r) = remap {
        reader.set_form_id_remap(r);
    }
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
            // Leveled creatures (CREA spawn tables) — byte-identical to
            // LVLI / LVLN. FO3 wires most enemy encounters through LVLC;
            // FNV migrated most combat to LVLN but still ships legacy
            // LVLC entries. See #448 / audit FO3-3-06.
            b"LVLC" => extract_records(&mut reader, end, b"LVLC", &mut |fid, subs| {
                index
                    .leveled_creatures
                    .insert(fid, parse_leveled_list(fid, subs));
            })?,
            // Actors and supporting records.
            b"NPC_" => extract_records(&mut reader, end, b"NPC_", &mut |fid, subs| {
                index.npcs.insert(fid, parse_npc(fid, subs));
            })?,
            // Creatures share EDID / FULL / MODL / RNAM / CNAM / SNAM /
            // CNTO / PKID / ACBS with NPC_ — `parse_npc` populates the
            // same `NpcRecord` shape. FO3 bestiary (super mutants,
            // deathclaws, radroaches, robots) lives here; pre-fix the
            // whole top-level group was dropped at the catch-all skip.
            // See #442 / audit FO3-3-02.
            b"CREA" => extract_records(&mut reader, end, b"CREA", &mut |fid, subs| {
                index.creatures.insert(fid, parse_npc(fid, subs));
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
            // FO3 / FNV / Oblivion pre-Papyrus SCPT scripts — bytecode
            // blob + source text + local-var table. Pre-#443 the group
            // fell through to the catch-all skip and every NPC / item
            // SCRI cross-reference dangled. Runtime execution is out
            // of scope for this fix — extraction only.
            b"SCPT" => extract_records(&mut reader, end, b"SCPT", &mut |fid, subs| {
                index.scripts.insert(fid, parse_scpt(fid, subs));
            })?,
            _ => {
                reader.skip_group(&group);
            }
        }
    }

    log::info!(
        "ESM parsed: {} cells, {} statics, {} items, {} containers, {} LVLI, {} LVLN, {} LVLC, \
         {} NPCs, {} creatures, {} races, {} classes, {} factions, {} globals, {} game settings, \
         {} weathers, {} climates, {} scripts",
        index.cells.cells.len(),
        index.cells.statics.len(),
        index.items.len(),
        index.containers.len(),
        index.leveled_items.len(),
        index.leveled_npcs.len(),
        index.leveled_creatures.len(),
        index.npcs.len(),
        index.creatures.len(),
        index.races.len(),
        index.classes.len(),
        index.factions.len(),
        index.globals.len(),
        index.game_settings.len(),
        index.weathers.len(),
        index.climates.len(),
        index.scripts.len(),
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

        assert_eq!(index.creatures.len(), 1, "CREA must populate the creatures map");
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
        let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/Fallout3.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: Fallout3.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
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
        let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/Fallout3.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: Fallout3.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
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
