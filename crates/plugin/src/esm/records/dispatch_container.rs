//! Container / leveled-list dispatch — split out of
//! `parse_esm_with_load_order` (#2060). CONT is dual-target (typed +
//! `cells.statics`); LVLI/LVLN/LVLC are typed-only. Embedded FormIDs
//! (CNTO/SNAM/QNAM/SCRI, LVLO) are plugin-local — each arm remaps to
//! global via `reader.get_form_id_remap()` before parsing (#2079).

use super::*;

/// Handles one of the container/leveled-list labels; the caller has
/// already verified `label` is one of this domain's.
pub(super) fn dispatch_container_group(
    label: &[u8; 4],
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    index: &mut EsmIndex,
) -> Result<()> {
    match label {
        // Containers and leveled lists. Embedded FormIDs
        // (CNTO/SNAM/QNAM/SCRI, LVLO) are plugin-local; remap to
        // global here (#2079) so `index.items` / `.leveled_items` /
        // `.leveled_npcs` / `.leveled_creatures` lookups hit.
        b"CONT" => {
            let cont_remap = reader.get_form_id_remap();
            extract_records_with_modl(reader, end, b"CONT", statics, &mut |fid, subs| {
                index
                    .containers
                    .insert(fid, parse_cont(fid, subs, &cont_remap));
            })?
        }
        b"LVLI" => {
            let lvli_remap = reader.get_form_id_remap();
            extract_records(reader, end, b"LVLI", &mut |fid, subs| {
                index
                    .leveled_items
                    .insert(fid, parse_leveled_list(fid, subs, &lvli_remap));
            })?
        }
        b"LVLN" => {
            let lvln_remap = reader.get_form_id_remap();
            extract_records(reader, end, b"LVLN", &mut |fid, subs| {
                index
                    .leveled_npcs
                    .insert(fid, parse_leveled_list(fid, subs, &lvln_remap));
            })?
        }
        // Leveled creatures (CREA spawn tables) — byte-identical to
        // LVLI / LVLN. FO3 wires most enemy encounters through LVLC;
        // FNV migrated most combat to LVLN but still ships legacy
        // LVLC entries. See #448 / audit FO3-3-06.
        b"LVLC" => {
            let lvlc_remap = reader.get_form_id_remap();
            extract_records(reader, end, b"LVLC", &mut |fid, subs| {
                index
                    .leveled_creatures
                    .insert(fid, parse_leveled_list(fid, subs, &lvlc_remap));
            })?
        }
        _ => unreachable!("dispatch_container_group: unexpected label {label:?}"),
    }
    Ok(())
}
