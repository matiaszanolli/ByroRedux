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
pub mod actor_value_derive;
pub mod climate;
pub mod common;
pub mod condition;
pub mod container;
pub mod global;
pub mod items;
pub mod list_record;
pub mod misc;
pub mod movs;
pub mod mswp;
pub mod outfit;
pub mod pkin;
pub mod scol;
pub mod script;
pub mod script_instance;
pub mod tree;
pub mod weather;

pub use list_record::{parse_flst, FlstRecord};
pub use movs::{parse_movs, MovableStaticRecord};
pub use mswp::{parse_mswp, MaterialSwapEntry, MaterialSwapRecord};
pub use outfit::{parse_otft, OtftRecord};
pub use pkin::{parse_pkin, PkinRecord};
pub use scol::{parse_scol, ScolPart, ScolPlacement, ScolRecord};

pub use actor::{
    parse_clas, parse_fact, parse_npc, parse_race, ClassRecord, FactionMembership, FactionRecord,
    FactionRelation, NpcInventoryEntry, NpcRecord, RaceRecord,
};
pub use actor_value_derive::derive_npc_actor_values;
pub use climate::{parse_clmt, ClimateRecord, ClimateWeather};
pub use common::StringsTableGuard;
pub use container::{
    parse_cont, parse_leveled_list, ContainerRecord, InventoryEntry, LeveledEntry, LeveledList,
};
pub use global::{parse_glob, parse_gmst, GameSetting, GlobalRecord, SettingValue};
pub use items::{
    parse_alch, parse_ammo, parse_armo, parse_book, parse_ingr, parse_keym, parse_misc, parse_note,
    parse_weap, ItemKind, ItemRecord,
};
pub use misc::{
    active_escort_location, active_escort_target, active_follow_target, active_guard_location,
    active_package, active_package_is_escort, active_package_is_follow, active_package_is_guard,
    active_package_is_patrol, active_package_is_sandbox, active_package_is_travel,
    active_package_is_wander, active_patrol_location, active_sandbox_location,
    active_travel_location, active_wander_location, parse_acti, parse_arma, parse_avif, parse_bptd,
    parse_cobj, parse_csty, parse_dial, parse_eczn, parse_efsh, parse_ench, parse_expl, parse_eyes,
    parse_hair, parse_hdpt, parse_idle, parse_imgs, parse_imod, parse_info, parse_ipct, parse_ipds,
    parse_lgtm, parse_mesg, parse_mgef, parse_minimal_esm_record, parse_navi, parse_navm,
    parse_pack, parse_perk, parse_proj, parse_qust, parse_regn, parse_repu, parse_slgm, parse_spel,
    parse_term, parse_watr, ActiRecord, AliasFillType, AliasFlags, AliasInjectedData, ArmaRecord,
    AvifRecord, BptdRecord, CobjRecord, CstyRecord, DialRecord, EcznRecord, EfshRecord, EnchRecord,
    ExplRecord, EyesRecord, HairRecord, HdptRecord, IdleRecord, ImgsRecord, ImodRecord, InfoRecord,
    IpctRecord, IpdsRecord, LgtmRecord, MesgRecord, MgefRecord, MinimalEsmRecord, NaviRecord,
    NavmRecord, PackLocation, PackLocationTarget, PackRecord, PackSchedule, PackTarget,
    PackTargetKind, PerkRecord, ProjRecord, QuestAlias, QuestObjective, QuestStage, QustRecord,
    RegnRecord, RepuRecord, SlgmRecord, SpelRecord, TermRecord, WatrRecord,
};
pub use script::{parse_scpt, ScriptLocalVar, ScriptRecord, ScriptType};
pub use tree::{parse_tree, TreeRecord};
pub use weather::{parse_wthr, OblivionHdrLighting, SkyColor, WeatherRecord};

use super::cell::support::{
    parse_ltex_group, parse_modl_group, parse_movs_group, parse_mswp_group, parse_pkin_group,
    parse_scol_group, parse_txst_group,
};
use super::cell::walkers::parse_cell_group;
use super::cell::wrld::parse_wrld_group;
use super::cell::{CellData, EsmCellIndex, StaticObject, TextureSet};
use super::reader::{EsmReader, FormIdRemap, GameKind};
use anyhow::{Context, Result};
use std::collections::HashMap;

// ── #1118 / TD9-003 split — see `index.rs` and `grup_walker.rs` ─────
mod grup_walker;
mod index;

pub use index::EsmIndex;

use grup_walker::{extract_dial_with_info, extract_records, extract_records_with_modl};

// ── #2060 split — `parse_esm_with_load_order`'s per-domain dispatch
// tables. The cell-only group (CELL/WRLD/LTEX/TXST/SCOL/PKIN/MOVS/MSWP/
// PDCL) stays inline: its dozen-odd interdependent locals (cells,
// exterior_cells, per-game warn flags, …) don't decompose into a small
// parameter list the way the typed-record domains below do.
mod dispatch_actor;
mod dispatch_container;
mod dispatch_global;
mod dispatch_items;
mod dispatch_misc_gameplay_a;
mod dispatch_misc_gameplay_b;
mod dispatch_misc_stub;
mod dispatch_world_placement;

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
/// pass `Some(FormIdRemap { plugin_slot, master_slots })` when
/// loading a DLC or mod in a multi-plugin stack so Anchorage's new
/// form 0x01012345 doesn't collide with BrokenSteel's 0x01012345 in
/// the same map. See #445.
///
/// The current CLI entry point (`--esm <path>`) only wires a single
/// plugin, so both paths produce the same output for vanilla content.
/// The multi-plugin wiring is tracked as follow-up work — this
/// function exists so downstream code can opt in without another
/// parse-layer refactor when the CLI grows multi-plugin support.
pub fn parse_esm_with_load_order(data: &[u8], remap: Option<FormIdRemap>) -> Result<EsmIndex> {
    let mut index = EsmIndex::default();

    let mut reader = EsmReader::new(data);
    if let Some(r) = remap {
        reader.set_form_id_remap(r);
    }
    // Peek the TES4 file header so we can derive a GameKind from HEDR's
    // `Version` f32. ARMO/WEAP/AMMO DATA/DNAM layouts diverge across
    // FO3/FNV → Skyrim → FO4; without the HEDR discriminator every
    // Skyrim record would be parsed with the FO3/FNV schema and get
    // garbage stats (issue #347).
    let file_header = reader
        .read_file_header()
        .context("Failed to read ESM file header")?;
    let game = GameKind::from_header(reader.variant(), file_header.hedr_version);
    // M41.0 Phase 1b — preserve game on the index so consumers
    // (NPC spawn dispatcher) can route per-version without
    // re-deriving from a HEDR they no longer have.
    index.game = game;
    log::info!(
        "ESM file: {} records, {} master files",
        file_header.record_count,
        file_header.master_files.len(),
    );

    // Cell-side scratch state — fed directly into `index.cells` after
    // the unified walk. Pre-#527 these were owned by a separate first
    // pass through the file (`parse_esm_cells_with_load_order`); the
    // fused walker collapses both passes into one so a 70 MB ESM
    // doesn't go through the header decoder + sub-record walker
    // twice. See audit FNV-ESM-2.
    let mut cells: HashMap<String, CellData> = HashMap::new();
    let mut exterior_cells: HashMap<String, HashMap<(i32, i32), CellData>> = HashMap::new();
    let mut statics: HashMap<u32, StaticObject> = HashMap::new();
    let mut landscape_textures: HashMap<u32, String> = HashMap::new();
    let mut worldspaces: HashMap<String, super::cell::WorldspaceRecord> = HashMap::new();
    let mut worldspace_climates: HashMap<String, u32> = HashMap::new();
    let mut txst_textures: HashMap<u32, String> = HashMap::new();
    let mut texture_sets: HashMap<u32, TextureSet> = HashMap::new();
    let mut ltex_to_txst: HashMap<u32, u32> = HashMap::new();
    let mut scols: HashMap<u32, ScolRecord> = HashMap::new();
    let mut packins: HashMap<u32, PkinRecord> = HashMap::new();
    let mut movables: HashMap<u32, MovableStaticRecord> = HashMap::new();
    let mut material_swaps: HashMap<u32, MaterialSwapRecord> = HashMap::new();

    // #348 / #624 — push the TES4 `Localized` flag into a thread-local
    // so every record parser's FULL/DESC decoder can route through the
    // lstring helper. The RAII guard restores the previous value on
    // drop (including unwind from a panic mid-walk), so:
    //   * A subsequent parse of a non-localized plugin in the same
    //     process can't inherit a stale `Localized = true` flag.
    //   * Overlapping parses on the same thread (nested `parse_esm`
    //     calls) correctly stack — the outer parse's flag is restored
    //     when the inner guard drops.
    let _localized_guard = common::LocalizedPluginGuard::new(file_header.localized);

    // FO4-era record-type gate. PKIN (pack-ins), MOVS (movable statics), and
    // MSWP (material swaps) were introduced in Fallout 4 and don't exist in
    // vanilla Oblivion / FO3 / FNV / Skyrim masters (confirmed absent from
    // FalloutNV.esm by byte-scan). Pre-#1277-task3 these GRUPs were parsed
    // unconditionally, so a cross-game plugin stack injecting them into a
    // non-FO4 master would silently consume them — REFRs referencing those
    // form ids would then mis-resolve at cell-load time. The gate skips the
    // whole GRUP when game isn't FO4+ and warns once per record-type per
    // parse so a modder gets a single visible signal that their authoring
    // was dropped (and why) without log spam on every record.
    let is_fo4_plus = matches!(
        game,
        GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield,
    );
    // SCOL (static collections) is NOT FO4-only — it's a Gamebryo-Fallout
    // record present since FO3 (Oblivion 0, FO3 54, FNV 98, Skyrim 0,
    // FO4+ many). FalloutNV.esm carries 98 SCOL records referenced by 1084
    // REFRs (road segments, guardrails, debris LOD clusters — predominantly
    // exterior worldspace geometry). The FNV DATA layout is byte-identical to
    // the FO4 layout `parse_scol_group` already decodes, so the same arm
    // serves both eras; only the gate had wrongly excluded FNV/FO3 (#1538).
    let is_scol_era = is_fo4_plus || matches!(game, GameKind::Fallout3NV);
    let mut warned_scol = false;
    let mut warned_pkin = false;
    let mut warned_movs = false;
    let mut warned_mswp = false;
    let mut warned_pdcl = false;

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
            // ── Cell-only labels — formerly the entire first walker (#527 fusion). ──
            //
            // CELL / WRLD / LTEX / TXST / SCOL / PKIN / MOVS / MSWP have
            // no typed `EsmIndex.<map>` consumer; they only feed the
            // `cells` sub-tree. Calling the existing cell helpers
            // inline keeps the cell-loader's already-trusted parsing
            // path byte-identical while letting the unified walker
            // own the outer loop.
            b"CELL" => parse_cell_group(&mut reader, end, &mut cells, game)?,
            b"WRLD" => parse_wrld_group(
                &mut reader,
                end,
                &mut exterior_cells,
                &mut worldspaces,
                &mut worldspace_climates,
            )?,
            b"LTEX" => {
                parse_ltex_group(&mut reader, end, &mut ltex_to_txst, &mut landscape_textures)?
            }
            b"TXST" => parse_txst_group(&mut reader, end, &mut txst_textures, &mut texture_sets)?,
            // FO3/FNV + FO4+ — see is_scol_era rationale above. Skipped only
            // for the eras that genuinely lack SCOL (Oblivion, Skyrim).
            b"SCOL" if is_scol_era => parse_scol_group(&mut reader, end, &mut statics, &mut scols)?,
            b"SCOL" => {
                if !warned_scol {
                    warned_scol = true;
                    log::warn!(
                        "ESM: SCOL GRUP encountered with GameKind::{:?} \
                         (HEDR {:.2}); SCOL doesn't exist in this era — skipping. \
                         Cross-game plugin risk: REFRs referencing the \
                         dropped form ids won't resolve at cell-load time.",
                        game,
                        file_header.hedr_version,
                    );
                }
                reader.skip_group(&group);
            }
            b"PKIN" if is_fo4_plus => {
                parse_pkin_group(&mut reader, end, &mut statics, &mut packins)?
            }
            b"PKIN" => {
                if !warned_pkin {
                    warned_pkin = true;
                    log::warn!(
                        "ESM: PKIN GRUP encountered with GameKind::{:?} \
                         (HEDR {:.2}); PKIN is FO4+ only — skipping. \
                         Cross-game plugin risk: pack-in contents won't resolve.",
                        game,
                        file_header.hedr_version,
                    );
                }
                reader.skip_group(&group);
            }
            b"MOVS" if is_fo4_plus => {
                parse_movs_group(&mut reader, end, &mut statics, &mut movables)?
            }
            b"MOVS" => {
                if !warned_movs {
                    warned_movs = true;
                    log::warn!(
                        "ESM: MOVS GRUP encountered with GameKind::{:?} \
                         (HEDR {:.2}); MOVS is FO4+ only — skipping. \
                         Cross-game plugin risk: movable-static REFRs \
                         referencing the dropped form ids won't resolve.",
                        game,
                        file_header.hedr_version,
                    );
                }
                reader.skip_group(&group);
            }
            b"MSWP" if is_fo4_plus => parse_mswp_group(&mut reader, end, &mut material_swaps)?,
            b"MSWP" => {
                if !warned_mswp {
                    warned_mswp = true;
                    log::warn!(
                        "ESM: MSWP GRUP encountered with GameKind::{:?} \
                         (HEDR {:.2}); MSWP is FO4+ only — skipping. \
                         Cross-game plugin risk: material-swap references \
                         won't resolve.",
                        game,
                        file_header.hedr_version,
                    );
                }
                reader.skip_group(&group);
            }
            // PDCL (Starfield BGSProjectedDecal, #1568 / SF-D4-02) — the
            // single most frequent unresolved base type in Cydonia
            // (1846 REFRs / 67 forms). Decals are projected onto the
            // surrounding geometry and carry no MODL, so they could never
            // ride the `statics` path even if dispatched; a real
            // decal-projection consumer has to land before they mean
            // anything. Until then, skip consciously — name the skip in
            // telemetry (`skipped_unconsumed_groups`) so it stops
            // vanishing into the anonymous catch-all and warn once so a
            // Starfield load gets a single visible signal. Do NOT route
            // into `statics`.
            b"PDCL" => {
                if !warned_pdcl {
                    warned_pdcl = true;
                    index.skipped_unconsumed_groups.push(*b"PDCL");
                    log::warn!(
                        "ESM: PDCL GRUP encountered (Starfield \
                         BGSProjectedDecal, GameKind::{:?}, HEDR {:.2}); \
                         no decal-projection consumer exists yet — \
                         skipping. Placed decal REFRs won't resolve at \
                         cell-load time (cosmetic only: no collision or \
                         structural geometry is lost).",
                        game,
                        file_header.hedr_version,
                    );
                }
                reader.skip_group(&group);
            }
            // #2060 — everything below is grouped into per-domain dispatch
            // tables (mirroring the records/{actor,items,container,global,
            // misc}.rs split) instead of one 700-line flat arm list. Each
            // group below is a single OR-pattern that routes to a
            // `dispatch_*` function; the original per-label bodies moved
            // verbatim into those functions — see them for the per-record
            // history/rationale comments.
            b"STAT" | b"MSTT" | b"FURN" | b"DOOR" | b"LIGH" | b"FLOR" | b"IDLM" | b"BNDS"
            | b"ADDN" | b"TACT" | b"TREE" => {
                dispatch_world_placement::dispatch_world_placement_group(
                    &label,
                    &mut reader,
                    end,
                    &mut statics,
                    &mut index,
                )?;
            }
            b"WEAP" | b"ARMO" | b"AMMO" | b"MISC" | b"KEYM" | b"ALCH" | b"INGR" | b"BOOK"
            | b"NOTE" => {
                dispatch_items::dispatch_item_group(
                    &label,
                    &mut reader,
                    end,
                    &mut statics,
                    &mut index,
                    game,
                )?;
            }
            b"CONT" | b"LVLI" | b"LVLN" | b"LVLC" => {
                dispatch_container::dispatch_container_group(
                    &label,
                    &mut reader,
                    end,
                    &mut statics,
                    &mut index,
                )?;
            }
            b"NPC_" | b"CREA" | b"RACE" | b"CLAS" | b"FACT" => {
                dispatch_actor::dispatch_actor_group(
                    &label,
                    &mut reader,
                    end,
                    &mut statics,
                    &mut index,
                    game,
                )?;
            }
            b"GLOB" | b"GMST" => {
                dispatch_global::dispatch_global_group(&label, &mut reader, end, &mut index)?;
            }
            b"WTHR" | b"CLMT" | b"SCPT" | b"WATR" | b"NAVI" | b"NAVM" | b"REGN" | b"ECZN"
            | b"LGTM" | b"IMGS" | b"HDPT" | b"EYES" | b"HAIR" | b"PACK" | b"QUST" | b"DIAL"
            | b"MESG" | b"PERK" => {
                dispatch_misc_gameplay_a::dispatch_misc_gameplay_a_group(
                    &label,
                    &mut reader,
                    end,
                    &mut index,
                    game,
                )?;
            }
            b"SPEL" | b"ENCH" | b"MGEF" | b"AVIF" | b"ACTI" | b"TERM" | b"FLST" | b"PROJ"
            | b"EFSH" | b"IMOD" | b"ARMA" | b"OTFT" | b"BPTD" | b"REPU" | b"EXPL" | b"CSTY"
            | b"IDLE" | b"IPCT" | b"IPDS" | b"COBJ" => {
                dispatch_misc_gameplay_b::dispatch_misc_gameplay_b_group(
                    &label,
                    &mut reader,
                    end,
                    &mut statics,
                    &mut index,
                    game,
                )?;
            }
            b"ALOC" | b"ANIO" | b"ASPC" | b"CAMS" | b"CPTH" | b"DOBJ" | b"MICN" | b"MSET"
            | b"MUSC" | b"SOUN" | b"VTYP" | b"AMEF" | b"DEBR" | b"GRAS" | b"IMAD" | b"LSCR"
            | b"LSCT" | b"PWAT" | b"RGDL" | b"DEHY" | b"HUNG" | b"RADS" | b"SLPD" | b"CCRD"
            | b"CDCK" | b"CHAL" | b"CHIP" | b"CMNY" | b"CSNO" | b"RCCT" | b"RCPE" | b"BSGN"
            | b"CLOT" | b"APPA" | b"SGST" | b"SLGM" => {
                dispatch_misc_stub::dispatch_misc_stub_group(
                    &label,
                    &mut reader,
                    end,
                    &mut statics,
                    &mut index,
                )?;
            }
            _ => {
                reader.skip_group(&group);
            }
        }
    }

    // Resolve LTEX → texture path via TXST indirection.
    // FO3/FNV: LTEX.TNAM → TXST form ID → TXST.TX00 diffuse path.
    // Oblivion: LTEX.ICON is a direct texture path (already in
    // `landscape_textures` from `parse_ltex_group`).
    for (ltex_id, txst_id) in &ltex_to_txst {
        if let Some(path) = txst_textures.get(txst_id) {
            landscape_textures.insert(*ltex_id, path.clone());
        }
    }

    let total_exterior: usize = exterior_cells.values().map(|m| m.len()).sum();
    let wrld_names: Vec<&str> = exterior_cells.keys().map(|s| s.as_str()).collect();
    log::info!(
        "ESM parsed: {} interior cells, {} exterior cells across {} worldspaces, {} base objects, {} landscape textures",
        cells.len(),
        total_exterior,
        exterior_cells.len(),
        statics.len(),
        landscape_textures.len(),
    );
    if !wrld_names.is_empty() {
        log::info!("  Worldspaces: {:?}", wrld_names);
    }

    // #1272 — drain per-cell navmeshes (collected by the cell walker's
    // child-GRUP loop) into the global `EsmIndex.navmeshes` map. The
    // top-level NAVM dispatch at the `b"NAVM"` arm above only ever
    // fires for non-vanilla mods that flatten NAVMs out of cell
    // children; vanilla Bethesda masters route every authored NAVM
    // through the cell tier.
    for cell in cells.values_mut() {
        for navm in cell.navmeshes.drain(..) {
            index.navmeshes.insert(navm.form_id, navm);
        }
    }
    for wrld_cells in exterior_cells.values_mut() {
        for cell in wrld_cells.values_mut() {
            for navm in cell.navmeshes.drain(..) {
                index.navmeshes.insert(navm.form_id, navm);
            }
        }
    }

    index.cells = EsmCellIndex {
        cells,
        exterior_cells,
        statics,
        landscape_textures,
        worldspaces,
        worldspace_climates,
        texture_sets,
        scols,
        packins,
        movables,
        material_swaps,
    };

    // Single source of truth — both this line and `index.total()` walk
    // the same `EsmIndex::categories` table so adding a new record
    // category is a single-edit operation. See #634 / FNV-D2-06.
    log::info!("{}", index.category_breakdown());

    // Localized thread-local restored automatically when
    // `_localized_guard` drops — including on early-return paths and
    // panic unwinds. See #624.
    Ok(index)
}

#[cfg(test)]
mod tests;
