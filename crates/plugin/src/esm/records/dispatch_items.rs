//! Item record dispatch — split out of `parse_esm_with_load_order`
//! (#2060). Every label here is dual-target: typed `EsmIndex.items`
//! entry AND `cells.statics` for visual placement (a REFR pointing at
//! the form still needs a model_path / VMAD-script flag). Pre-#527 these
//! were walked twice; `extract_records_with_modl` fuses both consumers
//! into one pass over the same `subs` slice.

use super::*;

/// Handles one of the item labels; the caller has already verified
/// `label` is one of this domain's.
pub(super) fn dispatch_item_group(
    label: &[u8; 4],
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    index: &mut EsmIndex,
    game: GameKind,
) -> Result<()> {
    match label {
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
        b"WEAP" => extract_records_with_modl(reader, end, b"WEAP", statics, &mut |fid, subs| {
            index.items.insert(fid, parse_weap(fid, subs, game));
        })?,
        b"ARMO" => extract_records_with_modl(reader, end, b"ARMO", statics, &mut |fid, subs| {
            index.items.insert(fid, parse_armo(fid, subs, game));
        })?,
        b"AMMO" => extract_records_with_modl(reader, end, b"AMMO", statics, &mut |fid, subs| {
            index.items.insert(fid, parse_ammo(fid, subs, game));
        })?,
        b"MISC" => extract_records_with_modl(reader, end, b"MISC", statics, &mut |fid, subs| {
            index.items.insert(fid, parse_misc(fid, subs));
        })?,
        b"KEYM" => extract_records_with_modl(reader, end, b"KEYM", statics, &mut |fid, subs| {
            index.items.insert(fid, parse_keym(fid, subs));
        })?,
        b"ALCH" => extract_records_with_modl(reader, end, b"ALCH", statics, &mut |fid, subs| {
            index.items.insert(fid, parse_alch(fid, subs));
        })?,
        b"INGR" => extract_records_with_modl(reader, end, b"INGR", statics, &mut |fid, subs| {
            index.items.insert(fid, parse_ingr(fid, subs));
        })?,
        b"BOOK" => extract_records_with_modl(reader, end, b"BOOK", statics, &mut |fid, subs| {
            index.items.insert(fid, parse_book(fid, subs));
        })?,
        b"NOTE" => extract_records_with_modl(reader, end, b"NOTE", statics, &mut |fid, subs| {
            index.items.insert(fid, parse_note(fid, subs));
        })?,
        _ => unreachable!("dispatch_item_group: unexpected label {label:?}"),
    }
    Ok(())
}
