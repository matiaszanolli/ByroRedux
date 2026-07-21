//! Actor + supporting-record dispatch — split out of
//! `parse_esm_with_load_order` (#2060). NPC_/CREA are dual-target (typed
//! plus `cells.statics` for REFR base-form resolution); RACE/CLAS/FACT are
//! typed-only. Embedded FormIDs are plugin-local; each dual-target arm
//! remaps to global via `reader.get_form_id_remap()` (#1996).

use super::*;

/// Handles one of the actor-domain labels; the caller has already
/// verified `label` is one of this domain's.
pub(super) fn dispatch_actor_group(
    label: &[u8; 4],
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    index: &mut EsmIndex,
    game: GameKind,
) -> Result<()> {
    match label {
        // Actors and supporting records — dual-target via the
        // fused walker so the cell-side STAT-equivalent
        // registration in `statics` still happens (REFR base-form
        // resolution against named NPCs / creatures keeps working).
        // Embedded FormIDs (RNAM/CNAM/VTCK/SNAM/PKID/DOFT/INAM/TPLT/…)
        // are plugin-local; remap to global here (#1996) so
        // `index.packages` / `index.races` / etc. lookups (all keyed
        // by remapped global FormIDs via `read_record_header`) hit.
        b"NPC_" => {
            let npc_remap = reader.get_form_id_remap();
            extract_records_with_modl(reader, end, b"NPC_", statics, &mut |fid, subs| {
                index
                    .npcs
                    .insert(fid, parse_npc(fid, subs, game, &npc_remap));
            })?
        }
        // Creatures share EDID / FULL / MODL / RNAM / CNAM / SNAM /
        // CNTO / PKID / ACBS with NPC_ — `parse_npc` populates the
        // same `NpcRecord` shape. FO3 bestiary (super mutants,
        // deathclaws, radroaches, robots) lives here; pre-fix the
        // whole top-level group was dropped at the catch-all skip.
        // See #442 / audit FO3-3-02.
        b"CREA" => {
            let crea_remap = reader.get_form_id_remap();
            extract_records_with_modl(reader, end, b"CREA", statics, &mut |fid, subs| {
                index
                    .creatures
                    .insert(fid, parse_npc(fid, subs, game, &crea_remap));
            })?
        }
        b"RACE" => extract_records(reader, end, b"RACE", &mut |fid, subs| {
            index.races.insert(fid, parse_race(fid, subs, game));
        })?,
        b"CLAS" => extract_records(reader, end, b"CLAS", &mut |fid, subs| {
            index.classes.insert(fid, parse_clas(fid, subs, game));
        })?,
        b"FACT" => extract_records(reader, end, b"FACT", &mut |fid, subs| {
            index.factions.insert(fid, parse_fact(fid, subs));
        })?,
        _ => unreachable!("dispatch_actor_group: unexpected label {label:?}"),
    }
    Ok(())
}
