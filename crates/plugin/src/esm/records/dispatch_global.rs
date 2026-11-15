//! Global variable / game-setting dispatch — split out of
//! `parse_esm_with_load_order` (#2060). GLOB/GMST are typed-only, no
//! `cells.statics` placement.

use super::*;

/// Handles GLOB, GMST or MOVT; the caller has already verified `label` is one
/// of this domain's.
pub(super) fn dispatch_global_group(
    label: &[u8; 4],
    reader: &mut EsmReader,
    end: usize,
    index: &mut EsmIndex,
) -> Result<()> {
    match label {
        // Globals and game settings.
        b"GLOB" => extract_records(reader, end, b"GLOB", &mut |fid, subs| {
            index.globals.insert(fid, parse_glob(fid, subs));
        })?,
        b"GMST" => extract_records(reader, end, b"GMST", &mut |fid, subs| {
            index.insert_game_setting(parse_gmst(fid, subs));
        })?,
        // M65 (SNK-STN-005) — movement types: the sneak stance's speed on
        // Skyrim and Fallout 4 is a ratio of two of them.
        b"MOVT" => extract_records(reader, end, b"MOVT", &mut |fid, subs| {
            index.movement_types.insert(fid, parse_movt(fid, subs));
        })?,
        _ => unreachable!("dispatch_global_group: unexpected label {label:?}"),
    }
    Ok(())
}
