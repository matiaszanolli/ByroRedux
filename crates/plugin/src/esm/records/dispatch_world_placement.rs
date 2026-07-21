//! MODL-only static-placement dispatch — split out of
//! `parse_esm_with_load_order` (#2060) to shrink that 949-line, 110-arm
//! dispatch table. STAT/MSTT/FURN/DOOR/LIGH/FLOR/IDLM/BNDS/ADDN/TACT carry
//! a MODL but no dedicated record-side parser; TREE is dual-target (typed
//! `EsmIndex.trees` entry + `cells.statics`).

use super::*;

/// Handles one of the world-placement labels; the caller has already
/// verified `label` is one of this domain's via the outer dispatch match.
pub(super) fn dispatch_world_placement_group(
    label: &[u8; 4],
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    index: &mut EsmIndex,
) -> Result<()> {
    match label {
        // MODL-only labels — populate `cells.statics` for visual
        // placement, no typed map. STAT / MSTT / FURN / DOOR /
        // LIGH / FLOR / IDLM / BNDS / ADDN / TACT all carry a MODL
        // but no record-side parser yet. TREE was here too pre-#TREE
        // (SpeedTree Phase 1.1) but split out below so ICON / SNAM /
        // CNAM / BNAM / PFIG don't silently fall on the floor.
        b"STAT" | b"MSTT" | b"FURN" | b"DOOR" | b"LIGH" | b"FLOR" | b"IDLM" | b"BNDS" | b"ADDN"
        | b"TACT" => {
            parse_modl_group(reader, end, statics)?;
        }
        // TREE — dual-target: typed `EsmIndex.trees` entry AND
        // `cells.statics` for the existing REFR placement path.
        // Same fused-walk pattern as WEAP / ARMO etc. so we don't
        // pay for the sub-record decode twice.
        b"TREE" => extract_records_with_modl(reader, end, b"TREE", statics, &mut |fid, subs| {
            index.trees.insert(fid, parse_tree(fid, subs));
        })?,
        _ => unreachable!("dispatch_world_placement_group: unexpected label {label:?}"),
    }
    Ok(())
}
