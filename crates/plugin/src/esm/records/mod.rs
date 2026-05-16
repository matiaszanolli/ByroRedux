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
pub mod list_record;
pub mod misc;
pub mod movs;
pub mod mswp;
pub mod outfit;
pub mod pkin;
pub mod scol;
pub mod script;
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
pub use climate::{parse_clmt, ClimateRecord, ClimateWeather};
pub use container::{
    parse_cont, parse_leveled_list, ContainerRecord, InventoryEntry, LeveledEntry, LeveledList,
};
pub use global::{parse_glob, parse_gmst, GameSetting, GlobalRecord, SettingValue};
pub use items::{
    parse_alch, parse_ammo, parse_armo, parse_book, parse_ingr, parse_keym, parse_misc, parse_note,
    parse_weap, ItemKind, ItemRecord,
};
pub use misc::{
    parse_acti, parse_arma, parse_avif, parse_bptd, parse_cobj, parse_csty, parse_dial, parse_eczn,
    parse_efsh, parse_ench, parse_expl, parse_eyes, parse_hair, parse_hdpt, parse_idle, parse_imgs,
    parse_imod, parse_info, parse_ipct, parse_ipds, parse_lgtm, parse_mesg, parse_mgef,
    parse_minimal_esm_record, parse_navi, parse_navm, parse_pack, parse_perk, parse_proj,
    parse_qust, parse_regn, parse_repu, parse_slgm, parse_spel, parse_term, parse_watr, ActiRecord,
    ArmaRecord, AvifRecord, BptdRecord, CobjRecord, CstyRecord, DialRecord, EcznRecord, EfshRecord,
    EnchRecord, ExplRecord, EyesRecord, HairRecord, HdptRecord, IdleRecord, ImgsRecord, ImodRecord,
    InfoRecord, IpctRecord, IpdsRecord, LgtmRecord, MesgRecord, MgefRecord, MinimalEsmRecord,
    NaviRecord, NavmRecord, PackRecord, PerkRecord, ProjRecord, QustRecord, RegnRecord, RepuRecord,
    SlgmRecord, SpelRecord, TermRecord, WatrRecord,
};
pub use common::StringsTableGuard;
pub use script::{parse_scpt, ScriptLocalVar, ScriptRecord, ScriptType};
pub use tree::{parse_tree, ObjectBounds as TreeObjectBounds, TreeRecord};
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
            b"CELL" => parse_cell_group(&mut reader, end, &mut cells)?,
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
            b"SCOL" => parse_scol_group(&mut reader, end, &mut statics, &mut scols)?,
            b"PKIN" => parse_pkin_group(&mut reader, end, &mut statics, &mut packins)?,
            b"MOVS" => parse_movs_group(&mut reader, end, &mut statics, &mut movables)?,
            b"MSWP" => parse_mswp_group(&mut reader, end, &mut material_swaps)?,
            // MODL-only labels — populate `cells.statics` for visual
            // placement, no typed map. STAT / MSTT / FURN / DOOR /
            // LIGH / FLOR / IDLM / BNDS / ADDN / TACT all carry a MODL
            // but no record-side parser yet. TREE was here too pre-#TREE
            // (SpeedTree Phase 1.1) but split out below so ICON / SNAM /
            // CNAM / BNAM / PFIG don't silently fall on the floor.
            b"STAT" | b"MSTT" | b"FURN" | b"DOOR" | b"LIGH" | b"FLOR" | b"IDLM" | b"BNDS"
            | b"ADDN" | b"TACT" => {
                parse_modl_group(&mut reader, end, &mut statics)?;
            }
            // TREE — dual-target: typed `EsmIndex.trees` entry AND
            // `cells.statics` for the existing REFR placement path.
            // Same fused-walk pattern as WEAP / ARMO etc. so we don't
            // pay for the sub-record decode twice.
            b"TREE" => extract_records_with_modl(
                &mut reader,
                end,
                b"TREE",
                &mut statics,
                &mut |fid, subs| {
                    index.trees.insert(fid, parse_tree(fid, subs));
                },
            )?,
            // ── Dual-target labels — typed record + cells.statics in one walk. ──
            //
            // Every label below ships BOTH a typed `EsmIndex.<map>`
            // entry AND wants `cells.statics` populated for visual
            // placement (REFRs targeting the form ID still need a
            // model_path / VMAD-script flag). Pre-#527 they were
            // walked twice — once by the cell first-pass, once by the
            // records second-pass. The fused helper walks each group
            // once and dispatches both consumers from the same
            // `subs` slice.
            b"WEAP" => extract_records_with_modl(
                &mut reader,
                end,
                b"WEAP",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_weap(fid, subs, game));
                },
            )?,
            b"ARMO" => extract_records_with_modl(
                &mut reader,
                end,
                b"ARMO",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_armo(fid, subs, game));
                },
            )?,
            b"AMMO" => extract_records_with_modl(
                &mut reader,
                end,
                b"AMMO",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_ammo(fid, subs, game));
                },
            )?,
            b"MISC" => extract_records_with_modl(
                &mut reader,
                end,
                b"MISC",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_misc(fid, subs));
                },
            )?,
            b"KEYM" => extract_records_with_modl(
                &mut reader,
                end,
                b"KEYM",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_keym(fid, subs));
                },
            )?,
            b"ALCH" => extract_records_with_modl(
                &mut reader,
                end,
                b"ALCH",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_alch(fid, subs));
                },
            )?,
            b"INGR" => extract_records_with_modl(
                &mut reader,
                end,
                b"INGR",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_ingr(fid, subs));
                },
            )?,
            b"BOOK" => extract_records_with_modl(
                &mut reader,
                end,
                b"BOOK",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_book(fid, subs));
                },
            )?,
            b"NOTE" => extract_records_with_modl(
                &mut reader,
                end,
                b"NOTE",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_note(fid, subs));
                },
            )?,
            // Containers and leveled lists.
            b"CONT" => extract_records_with_modl(
                &mut reader,
                end,
                b"CONT",
                &mut statics,
                &mut |fid, subs| {
                    index.containers.insert(fid, parse_cont(fid, subs));
                },
            )?,
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
            // Actors and supporting records — dual-target via the
            // fused walker so the cell-side STAT-equivalent
            // registration in `statics` still happens (REFR base-form
            // resolution against named NPCs / creatures keeps working).
            b"NPC_" => extract_records_with_modl(
                &mut reader,
                end,
                b"NPC_",
                &mut statics,
                &mut |fid, subs| {
                    index.npcs.insert(fid, parse_npc(fid, subs, game));
                },
            )?,
            // Creatures share EDID / FULL / MODL / RNAM / CNAM / SNAM /
            // CNTO / PKID / ACBS with NPC_ — `parse_npc` populates the
            // same `NpcRecord` shape. FO3 bestiary (super mutants,
            // deathclaws, radroaches, robots) lives here; pre-fix the
            // whole top-level group was dropped at the catch-all skip.
            // See #442 / audit FO3-3-02.
            b"CREA" => extract_records_with_modl(
                &mut reader,
                end,
                b"CREA",
                &mut statics,
                &mut |fid, subs| {
                    index.creatures.insert(fid, parse_npc(fid, subs, game));
                },
            )?,
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
                // `game` threaded through (#539 / M33-07) — Skyrim WTHR
                // has a different sub-record schema and the FNV-only
                // arm needs gating so a 320-B Skyrim NAM0 doesn't get
                // truncated to "first 240 B = FNV colours" silently
                // once M32.5 routes Skyrim.esm through this dispatch.
                index.weathers.insert(fid, parse_wthr(fid, subs, game));
            })?,
            // Climate records — weather probability tables. The WLST
            // entry size dispatches off `game` (M33-08 / #540) so
            // multi-of-3-entry Oblivion CLMTs don't autodetect to the
            // 12-byte FO3+ schema and mis-thread their FormID slots.
            b"CLMT" => extract_records(&mut reader, end, b"CLMT", &mut |fid, subs| {
                index.climates.insert(fid, parse_clmt(fid, subs, game));
            })?,
            // FO3 / FNV / Oblivion pre-Papyrus SCPT scripts — bytecode
            // blob + source text + local-var table. Pre-#443 the group
            // fell through to the catch-all skip and every NPC / item
            // SCRI cross-reference dangled. Runtime execution is out
            // of scope for this fix — extraction only.
            b"SCPT" => extract_records(&mut reader, end, b"SCPT", &mut |fid, subs| {
                index.scripts.insert(fid, parse_scpt(fid, subs));
            })?,
            // Supplementary records previously catch-all-skipped (#458).
            // Stubs capture EDID + form refs + scalar fields; full
            // per-record decoding lands with the consuming subsystem.
            b"WATR" => extract_records(&mut reader, end, b"WATR", &mut |fid, subs| {
                index.waters.insert(fid, parse_watr(fid, subs));
            })?,
            b"NAVI" => extract_records(&mut reader, end, b"NAVI", &mut |fid, subs| {
                index.navi_info.insert(fid, parse_navi(fid, subs));
            })?,
            b"NAVM" => extract_records(&mut reader, end, b"NAVM", &mut |fid, subs| {
                index.navmeshes.insert(fid, parse_navm(fid, subs));
            })?,
            b"REGN" => extract_records(&mut reader, end, b"REGN", &mut |fid, subs| {
                index.regions.insert(fid, parse_regn(fid, subs));
            })?,
            b"ECZN" => extract_records(&mut reader, end, b"ECZN", &mut |fid, subs| {
                index.encounter_zones.insert(fid, parse_eczn(fid, subs));
            })?,
            // LGTM lighting templates — consumer lands alongside #379
            // (per-field inheritance fallback on cells without XCLL).
            b"LGTM" => extract_records(&mut reader, end, b"LGTM", &mut |fid, subs| {
                index.lighting_templates.insert(fid, parse_lgtm(fid, subs));
            })?,
            // #624 / SK-D6-NEW-03 — IMGS imagespace records. CELL.XCIM
            // cross-references resolve here. Currently a stub (EDID +
            // raw DNAM payload); full DNAM struct decode + IMAD
            // modifier graph deferred to M48 alongside the per-cell
            // HDR-LUT renderer consumer.
            b"IMGS" => extract_records(&mut reader, end, b"IMGS", &mut |fid, subs| {
                index.image_spaces.insert(fid, parse_imgs(fid, subs));
            })?,
            b"HDPT" => extract_records(&mut reader, end, b"HDPT", &mut |fid, subs| {
                index.head_parts.insert(fid, parse_hdpt(fid, subs));
            })?,
            b"EYES" => extract_records(&mut reader, end, b"EYES", &mut |fid, subs| {
                index.eyes.insert(fid, parse_eyes(fid, subs));
            })?,
            b"HAIR" => extract_records(&mut reader, end, b"HAIR", &mut |fid, subs| {
                index.hair.insert(fid, parse_hair(fid, subs));
            })?,
            // AI / dialogue / effect stubs (#446, #447). Follow the
            // #458 supplementary-record pattern: minimal struct
            // (EDID + FULL + a few scalars), no deep decoding.
            b"PACK" => extract_records(&mut reader, end, b"PACK", &mut |fid, subs| {
                index.packages.insert(fid, parse_pack(fid, subs));
            })?,
            b"QUST" => extract_records(&mut reader, end, b"QUST", &mut |fid, subs| {
                index.quests.insert(fid, parse_qust(fid, subs));
            })?,
            // DIAL tops a nested GRUP tree: a top-level GRUP labelled
            // "DIAL" containing DIAL records, each (often) followed by
            // a Topic Children sub-GRUP (group_type == 7) whose label
            // is the parent DIAL's form_id u32 and whose contents are
            // INFO records. The generic `extract_records` walker
            // filters on a single `expected_type` and silently drops
            // every INFO. The dedicated walker below threads both
            // record types through. See #631 / #447.
            b"DIAL" => extract_dial_with_info(&mut reader, end, &mut index.dialogues)?,
            b"MESG" => extract_records(&mut reader, end, b"MESG", &mut |fid, subs| {
                index.messages.insert(fid, parse_mesg(fid, subs));
            })?,
            b"PERK" => extract_records(&mut reader, end, b"PERK", &mut |fid, subs| {
                index.perks.insert(fid, parse_perk(fid, subs));
            })?,
            b"SPEL" => extract_records(&mut reader, end, b"SPEL", &mut |fid, subs| {
                index.spells.insert(fid, parse_spel(fid, subs));
            })?,
            // ENCH enchantments (#629 / FNV-D2-01). Same scaffolding as
            // SPEL — ENIT carries type/charge/cost/flags; full effect
            // chain decoding lands with MGEF application.
            b"ENCH" => extract_records(&mut reader, end, b"ENCH", &mut |fid, subs| {
                index.enchantments.insert(fid, parse_ench(fid, subs));
            })?,
            b"MGEF" => extract_records(&mut reader, end, b"MGEF", &mut |fid, subs| {
                index.magic_effects.insert(fid, parse_mgef(fid, subs));
            })?,
            // AVIF actor-value records (#519). Pre-fix every NPC
            // skill-bonus, BOOK skill-book teach ref, and AVIF-keyed
            // condition predicate dangled because the top-level group
            // hit the catch-all skip.
            b"AVIF" => extract_records(&mut reader, end, b"AVIF", &mut |fid, subs| {
                index.actor_values.insert(fid, parse_avif(fid, subs));
            })?,
            // ACTI / TERM #521 — dual-target: typed map for SCRI /
            // menu-tree cross-refs AND `cells.statics` for visual
            // placement. Pre-#527 the cell first-pass walked them via
            // the MODL catch-all and the records second-pass walked
            // them again for the typed parser; the fused helper does
            // both in one walk.
            b"ACTI" => extract_records_with_modl(
                &mut reader,
                end,
                b"ACTI",
                &mut statics,
                &mut |fid, subs| {
                    index.activators.insert(fid, parse_acti(fid, subs));
                },
            )?,
            b"TERM" => extract_records_with_modl(
                &mut reader,
                end,
                b"TERM",
                &mut statics,
                &mut |fid, subs| {
                    index.terminals.insert(fid, parse_term(fid, subs));
                },
            )?,
            // FLST FormID lists — flat arrays referenced by
            // `IsInList <flst>` perk-entry-point conditions, COBJ
            // recipe filters, FNV Caravan deck composition, and quest
            // objective lookups. Pre-#630 the top-level group fell
            // through to the catch-all skip and every `IsInList`
            // returned "not in list", silently disabling ~50 vanilla
            // FNV PERKs and the entire Caravan mini-game.
            b"FLST" => extract_records(&mut reader, end, b"FLST", &mut |fid, subs| {
                index.form_lists.insert(fid, parse_flst(fid, subs));
            })?,
            // #808 / FNV-D2-NEW-01 — five gameplay-critical record
            // types that previously fell through to the catch-all
            // skip. Stub-form parsing (EDID + a handful of key
            // scalar / form-ref fields); full sub-record decoding
            // lands when the consuming subsystem arrives.
            //
            // PROJ — projectiles (every WEAP references one)
            // EFSH — effect shaders (visual effects)
            // IMOD — item mods (FNV-CORE: weapon attachments)
            // ARMA — armor addons (race-specific biped variants)
            // BPTD — body part data (NPC dismemberment routing)
            b"PROJ" => extract_records(&mut reader, end, b"PROJ", &mut |fid, subs| {
                index.projectiles.insert(fid, parse_proj(fid, subs));
            })?,
            b"EFSH" => extract_records(&mut reader, end, b"EFSH", &mut |fid, subs| {
                index.effect_shaders.insert(fid, parse_efsh(fid, subs));
            })?,
            b"IMOD" => extract_records(&mut reader, end, b"IMOD", &mut |fid, subs| {
                index.item_mods.insert(fid, parse_imod(fid, subs));
            })?,
            b"ARMA" => extract_records(&mut reader, end, b"ARMA", &mut |fid, subs| {
                index.armor_addons.insert(fid, parse_arma(fid, subs, game));
            })?,
            // OTFT — Skyrim+ outfit (default-equipped armor list).
            // Pre-Skyrim plugins don't ship OTFT groups; the walker
            // skips them silently when absent (no group hit).
            b"OTFT" => extract_records(&mut reader, end, b"OTFT", &mut |fid, subs| {
                index.outfits.insert(fid, parse_otft(fid, subs));
            })?,
            b"BPTD" => extract_records(&mut reader, end, b"BPTD", &mut |fid, subs| {
                index.body_parts.insert(fid, parse_bptd(fid, subs));
            })?,
            // #809 / FNV-D2-NEW-02 — seven supporting records that
            // gate FNV NPC AI / crafting / impact-effect / faction-
            // reputation subsystems. Same stub-form pattern as #808.
            //
            // REPU — reputation (FNV-CORE: NCR / Legion / etc.)
            // EXPL — explosion (PROJ → EXPL → EFSH chain)
            // CSTY — combat style (NPC AI profile)
            // IDLE — idle animation (NPC behavior tree)
            // IPCT — impact (per-material bullet impact effect)
            // IPDS — impact data set (material-kind → IPCT table)
            // COBJ — constructible object (FNV crafting recipe)
            b"REPU" => extract_records(&mut reader, end, b"REPU", &mut |fid, subs| {
                index.reputations.insert(fid, parse_repu(fid, subs));
            })?,
            b"EXPL" => extract_records(&mut reader, end, b"EXPL", &mut |fid, subs| {
                index.explosions.insert(fid, parse_expl(fid, subs));
            })?,
            b"CSTY" => extract_records(&mut reader, end, b"CSTY", &mut |fid, subs| {
                index.combat_styles.insert(fid, parse_csty(fid, subs));
            })?,
            b"IDLE" => extract_records(&mut reader, end, b"IDLE", &mut |fid, subs| {
                index.idle_animations.insert(fid, parse_idle(fid, subs));
            })?,
            b"IPCT" => extract_records(&mut reader, end, b"IPCT", &mut |fid, subs| {
                index.impacts.insert(fid, parse_ipct(fid, subs));
            })?,
            b"IPDS" => extract_records(&mut reader, end, b"IPDS", &mut |fid, subs| {
                index.impact_data_sets.insert(fid, parse_ipds(fid, subs));
            })?,
            b"COBJ" => extract_records(&mut reader, end, b"COBJ", &mut |fid, subs| {
                index.recipes.insert(fid, parse_cobj(fid, subs));
            })?,
            // #810 / FNV-D2-NEW-03 — 31 long-tail records that fell
            // through the catch-all skip. Bulk-dispatched here using
            // the shared `parse_minimal_esm_record` (EDID + optional
            // FULL). When a real consumer arrives for any one of
            // these, replace the dispatch arm + `MinimalEsmRecord`
            // map with a dedicated parser pair via the established
            // #808 / #809 pattern.
            //
            // Audio metadata (11):
            b"ALOC" => extract_records(&mut reader, end, b"ALOC", &mut |fid, subs| {
                index
                    .audio_locations
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"ANIO" => extract_records(&mut reader, end, b"ANIO", &mut |fid, subs| {
                index
                    .animation_objects
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"ASPC" => extract_records(&mut reader, end, b"ASPC", &mut |fid, subs| {
                index
                    .acoustic_spaces
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CAMS" => extract_records(&mut reader, end, b"CAMS", &mut |fid, subs| {
                index
                    .camera_shots
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CPTH" => extract_records(&mut reader, end, b"CPTH", &mut |fid, subs| {
                index
                    .camera_paths
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"DOBJ" => extract_records(&mut reader, end, b"DOBJ", &mut |fid, subs| {
                index
                    .default_objects
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"MICN" => extract_records(&mut reader, end, b"MICN", &mut |fid, subs| {
                index
                    .menu_icons
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"MSET" => extract_records(&mut reader, end, b"MSET", &mut |fid, subs| {
                index
                    .media_sets
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"MUSC" => extract_records(&mut reader, end, b"MUSC", &mut |fid, subs| {
                index
                    .music_types
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"SOUN" => extract_records(&mut reader, end, b"SOUN", &mut |fid, subs| {
                index
                    .sounds
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"VTYP" => extract_records(&mut reader, end, b"VTYP", &mut |fid, subs| {
                index
                    .voice_types
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            // Visual / world (8):
            b"AMEF" => extract_records(&mut reader, end, b"AMEF", &mut |fid, subs| {
                index
                    .ammo_effects
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"DEBR" => extract_records(&mut reader, end, b"DEBR", &mut |fid, subs| {
                index
                    .debris
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"GRAS" => extract_records(&mut reader, end, b"GRAS", &mut |fid, subs| {
                index
                    .grasses
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"IMAD" => extract_records(&mut reader, end, b"IMAD", &mut |fid, subs| {
                index
                    .imagespace_modifiers
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"LSCR" => extract_records(&mut reader, end, b"LSCR", &mut |fid, subs| {
                index
                    .load_screens
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"LSCT" => extract_records(&mut reader, end, b"LSCT", &mut |fid, subs| {
                index
                    .load_screen_types
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"PWAT" => extract_records(&mut reader, end, b"PWAT", &mut |fid, subs| {
                index
                    .placeable_waters
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"RGDL" => extract_records(&mut reader, end, b"RGDL", &mut |fid, subs| {
                index
                    .ragdolls
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            // FNV Hardcore mode (4):
            b"DEHY" => extract_records(&mut reader, end, b"DEHY", &mut |fid, subs| {
                index
                    .dehydration_stages
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"HUNG" => extract_records(&mut reader, end, b"HUNG", &mut |fid, subs| {
                index
                    .hunger_stages
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"RADS" => extract_records(&mut reader, end, b"RADS", &mut |fid, subs| {
                index
                    .radiation_stages
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"SLPD" => extract_records(&mut reader, end, b"SLPD", &mut |fid, subs| {
                index
                    .sleep_deprivation_stages
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            // FNV Caravan + Casino (6):
            b"CCRD" => extract_records(&mut reader, end, b"CCRD", &mut |fid, subs| {
                index
                    .caravan_cards
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CDCK" => extract_records(&mut reader, end, b"CDCK", &mut |fid, subs| {
                index
                    .caravan_decks
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CHAL" => extract_records(&mut reader, end, b"CHAL", &mut |fid, subs| {
                index
                    .challenges
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CHIP" => extract_records(&mut reader, end, b"CHIP", &mut |fid, subs| {
                index
                    .poker_chips
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CMNY" => extract_records(&mut reader, end, b"CMNY", &mut |fid, subs| {
                index
                    .caravan_money
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CSNO" => extract_records(&mut reader, end, b"CSNO", &mut |fid, subs| {
                index
                    .casinos
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            // Recipe residuals (2):
            b"RCCT" => extract_records(&mut reader, end, b"RCCT", &mut |fid, subs| {
                index
                    .recipe_categories
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"RCPE" => extract_records(&mut reader, end, b"RCPE", &mut |fid, subs| {
                index
                    .recipe_records
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            // #966 / OBL-D3-NEW-02 — Oblivion-unique base records.
            // BSGN has no MODL (birthsign — UI / starting-bonus only)
            // so it's a plain minimal dispatch. CLOT / APPA / SGST /
            // SLGM all carry MODL and need cells.statics for visual
            // placement when a REFR points at them (e.g. world-placed
            // sigil stones in Oblivion Gates).
            b"BSGN" => extract_records(&mut reader, end, b"BSGN", &mut |fid, subs| {
                index
                    .birthsigns
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CLOT" => extract_records_with_modl(
                &mut reader,
                end,
                b"CLOT",
                &mut statics,
                &mut |fid, subs| {
                    index
                        .clothing
                        .insert(fid, parse_minimal_esm_record(fid, subs));
                },
            )?,
            b"APPA" => extract_records_with_modl(
                &mut reader,
                end,
                b"APPA",
                &mut statics,
                &mut |fid, subs| {
                    index
                        .apparatuses
                        .insert(fid, parse_minimal_esm_record(fid, subs));
                },
            )?,
            b"SGST" => extract_records_with_modl(
                &mut reader,
                end,
                b"SGST",
                &mut statics,
                &mut |fid, subs| {
                    index
                        .sigil_stones
                        .insert(fid, parse_minimal_esm_record(fid, subs));
                },
            )?,
            b"SLGM" => extract_records_with_modl(
                &mut reader,
                end,
                b"SLGM",
                &mut statics,
                &mut |fid, subs| {
                    index.soul_gems.insert(fid, parse_slgm(fid, subs));
                },
            )?,
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

/// Walk a top-level group once, dispatching every matching record to
/// BOTH a typed-record callback AND the [`StaticObject`] builder for
/// `cells.statics`. Used by `parse_esm_with_load_order` for dual-target
/// labels (WEAP / ARMO / AMMO / MISC / KEYM / ALCH / INGR / BOOK /
/// NOTE / CONT / NPC_ / CREA / ACTI / TERM) — every label that ships
/// both a typed record AND wants visual-placement coverage in
/// `cells.statics`.
///
/// Pre-#527 these labels were walked TWICE: once by
/// `parse_esm_cells_with_load_order` to populate `statics`, and again
/// by the typed dispatcher in `parse_esm_with_load_order`. The fused
/// helper calls `read_sub_records` once and routes the same `subs`
/// slice to both consumers, halving the sub-record decode cost on
/// each dual-target group. Recurses into nested groups so worldspace
/// children and persistent/temporary cell children are handled too —
/// same recursion shape as `extract_records`.


#[cfg(test)]
mod tests;
