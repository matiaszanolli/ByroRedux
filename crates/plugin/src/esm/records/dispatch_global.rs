//! Global variable / game-setting dispatch — split out of
//! `parse_esm_with_load_order` (#2060). GLOB/GMST are typed-only, no
//! `cells.statics` placement.

use super::*;

/// Handles GLOB or GMST; the caller has already verified `label` is one
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
            index.game_settings.insert(fid, parse_gmst(fid, subs));
        })?,
        _ => unreachable!("dispatch_global_group: unexpected label {label:?}"),
    }
    Ok(())
}
