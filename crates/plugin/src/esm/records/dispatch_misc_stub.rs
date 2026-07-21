//! Long-tail supplementary-record dispatch — split out of
//! `parse_esm_with_load_order` (#2060). #810 / FNV-D2-NEW-03: 31
//! record types bulk-dispatched via the shared `parse_minimal_esm_record`
//! (EDID + optional FULL only) plus five Oblivion-unique base records
//! (BSGN/CLOT/APPA/SGST/SLGM — the last four dual-target for
//! `cells.statics`).

use super::*;

/// Handles one of this domain's labels; the caller has already
/// verified `label` is one of them.
pub(super) fn dispatch_misc_stub_group(
    label: &[u8; 4],
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    index: &mut EsmIndex,
) -> Result<()> {
    match label {
        // #810 / FNV-D2-NEW-03 — 31 long-tail records that fell
        // through the catch-all skip. Bulk-dispatched here using
        // the shared `parse_minimal_esm_record` (EDID + optional
        // FULL). When a real consumer arrives for any one of
        // these, replace the dispatch arm + `MinimalEsmRecord`
        // map with a dedicated parser pair via the established
        // #808 / #809 pattern.
        //
        // Audio metadata (11):
        b"ALOC" => extract_records(reader, end, b"ALOC", &mut |fid, subs| {
            index
                .audio_locations
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"ANIO" => extract_records(reader, end, b"ANIO", &mut |fid, subs| {
            index
                .animation_objects
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"ASPC" => extract_records(reader, end, b"ASPC", &mut |fid, subs| {
            index
                .acoustic_spaces
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"CAMS" => extract_records(reader, end, b"CAMS", &mut |fid, subs| {
            index
                .camera_shots
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"CPTH" => extract_records(reader, end, b"CPTH", &mut |fid, subs| {
            index
                .camera_paths
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"DOBJ" => extract_records(reader, end, b"DOBJ", &mut |fid, subs| {
            index
                .default_objects
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"MICN" => extract_records(reader, end, b"MICN", &mut |fid, subs| {
            index
                .menu_icons
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"MSET" => extract_records(reader, end, b"MSET", &mut |fid, subs| {
            index
                .media_sets
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"MUSC" => extract_records(reader, end, b"MUSC", &mut |fid, subs| {
            index
                .music_types
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"SOUN" => extract_records(reader, end, b"SOUN", &mut |fid, subs| {
            index
                .sounds
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"VTYP" => extract_records(reader, end, b"VTYP", &mut |fid, subs| {
            index
                .voice_types
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        // Visual / world (8):
        b"AMEF" => extract_records(reader, end, b"AMEF", &mut |fid, subs| {
            index
                .ammo_effects
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"DEBR" => extract_records(reader, end, b"DEBR", &mut |fid, subs| {
            index
                .debris
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"GRAS" => extract_records(reader, end, b"GRAS", &mut |fid, subs| {
            index
                .grasses
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"IMAD" => extract_records(reader, end, b"IMAD", &mut |fid, subs| {
            index
                .imagespace_modifiers
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"LSCR" => extract_records(reader, end, b"LSCR", &mut |fid, subs| {
            index
                .load_screens
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"LSCT" => extract_records(reader, end, b"LSCT", &mut |fid, subs| {
            index
                .load_screen_types
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"PWAT" => extract_records(reader, end, b"PWAT", &mut |fid, subs| {
            index
                .placeable_waters
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"RGDL" => extract_records(reader, end, b"RGDL", &mut |fid, subs| {
            index
                .ragdolls
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        // FNV Hardcore mode (4):
        b"DEHY" => extract_records(reader, end, b"DEHY", &mut |fid, subs| {
            index
                .dehydration_stages
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"HUNG" => extract_records(reader, end, b"HUNG", &mut |fid, subs| {
            index
                .hunger_stages
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"RADS" => extract_records(reader, end, b"RADS", &mut |fid, subs| {
            index
                .radiation_stages
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"SLPD" => extract_records(reader, end, b"SLPD", &mut |fid, subs| {
            index
                .sleep_deprivation_stages
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        // FNV Caravan + Casino (6):
        b"CCRD" => extract_records(reader, end, b"CCRD", &mut |fid, subs| {
            index
                .caravan_cards
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"CDCK" => extract_records(reader, end, b"CDCK", &mut |fid, subs| {
            index
                .caravan_decks
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"CHAL" => extract_records(reader, end, b"CHAL", &mut |fid, subs| {
            index
                .challenges
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"CHIP" => extract_records(reader, end, b"CHIP", &mut |fid, subs| {
            index
                .poker_chips
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"CMNY" => extract_records(reader, end, b"CMNY", &mut |fid, subs| {
            index
                .caravan_money
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"CSNO" => extract_records(reader, end, b"CSNO", &mut |fid, subs| {
            index
                .casinos
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        // Recipe residuals (2):
        b"RCCT" => extract_records(reader, end, b"RCCT", &mut |fid, subs| {
            index
                .recipe_categories
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"RCPE" => extract_records(reader, end, b"RCPE", &mut |fid, subs| {
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
        b"BSGN" => extract_records(reader, end, b"BSGN", &mut |fid, subs| {
            index
                .birthsigns
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"CLOT" => extract_records_with_modl(reader, end, b"CLOT", statics, &mut |fid, subs| {
            index
                .clothing
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"APPA" => extract_records_with_modl(reader, end, b"APPA", statics, &mut |fid, subs| {
            index
                .apparatuses
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"SGST" => extract_records_with_modl(reader, end, b"SGST", statics, &mut |fid, subs| {
            index
                .sigil_stones
                .insert(fid, parse_minimal_esm_record(fid, subs));
        })?,
        b"SLGM" => extract_records_with_modl(reader, end, b"SLGM", statics, &mut |fid, subs| {
            index.soul_gems.insert(fid, parse_slgm(fid, subs));
        })?,
        _ => unreachable!("dispatch_misc_stub_group: unexpected label {label:?}"),
    }
    Ok(())
}
