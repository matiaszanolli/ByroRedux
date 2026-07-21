//! Misc-gameplay dispatch, part A — split out of
//! `parse_esm_with_load_order` (#2060). Weather/climate/world-state
//! (WTHR/CLMT/SCPT/WATR/NAVI/NAVM/REGN/ECZN/LGTM/IMGS/HDPT/EYES/HAIR)
//! plus AI/dialogue records (PACK/QUST/DIAL/MESG/PERK) — all typed-only,
//! no `cells.statics` placement. See `dispatch_misc_gameplay_b.rs` for
//! the combat/magic/supporting half of this domain.

use super::*;

/// Handles one of this domain's labels; the caller has already
/// verified `label` is one of them.
pub(super) fn dispatch_misc_gameplay_a_group(
    label: &[u8; 4],
    reader: &mut EsmReader,
    end: usize,
    index: &mut EsmIndex,
    game: GameKind,
) -> Result<()> {
    match label {
        // Weather records — sky colors, fog, wind, clouds.
        b"WTHR" => extract_records(reader, end, b"WTHR", &mut |fid, subs| {
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
        b"CLMT" => extract_records(reader, end, b"CLMT", &mut |fid, subs| {
            index.climates.insert(fid, parse_clmt(fid, subs, game));
        })?,
        // FO3 / FNV / Oblivion pre-Papyrus SCPT scripts — bytecode
        // blob + source text + local-var table. Pre-#443 the group
        // fell through to the catch-all skip and every NPC / item
        // SCRI cross-reference dangled. Runtime execution is out
        // of scope for this fix — extraction only.
        b"SCPT" => extract_records(reader, end, b"SCPT", &mut |fid, subs| {
            index.scripts.insert(fid, parse_scpt(fid, subs));
        })?,
        // Supplementary records previously catch-all-skipped (#458).
        // Stubs capture EDID + form refs + scalar fields; full
        // per-record decoding lands with the consuming subsystem.
        b"WATR" => extract_records(reader, end, b"WATR", &mut |fid, subs| {
            index.waters.insert(fid, parse_watr(fid, subs));
        })?,
        b"NAVI" => extract_records(reader, end, b"NAVI", &mut |fid, subs| {
            index.navi_info.insert(fid, parse_navi(fid, subs));
        })?,
        b"NAVM" => extract_records(reader, end, b"NAVM", &mut |fid, subs| {
            index.navmeshes.insert(fid, parse_navm(fid, subs));
        })?,
        b"REGN" => extract_records(reader, end, b"REGN", &mut |fid, subs| {
            index.regions.insert(fid, parse_regn(fid, subs));
        })?,
        b"ECZN" => extract_records(reader, end, b"ECZN", &mut |fid, subs| {
            index.encounter_zones.insert(fid, parse_eczn(fid, subs));
        })?,
        // LGTM lighting templates — consumer lands alongside #379
        // (per-field inheritance fallback on cells without XCLL).
        b"LGTM" => extract_records(reader, end, b"LGTM", &mut |fid, subs| {
            index.lighting_templates.insert(fid, parse_lgtm(fid, subs));
        })?,
        // #624 / SK-D6-NEW-03 — IMGS imagespace records. CELL.XCIM
        // cross-references resolve here. Currently a stub (EDID +
        // raw DNAM payload); full DNAM struct decode + IMAD
        // modifier graph deferred to M48 alongside the per-cell
        // HDR-LUT renderer consumer.
        b"IMGS" => extract_records(reader, end, b"IMGS", &mut |fid, subs| {
            index.image_spaces.insert(fid, parse_imgs(fid, subs));
        })?,
        b"HDPT" => extract_records(reader, end, b"HDPT", &mut |fid, subs| {
            index.head_parts.insert(fid, parse_hdpt(fid, subs));
        })?,
        b"EYES" => extract_records(reader, end, b"EYES", &mut |fid, subs| {
            index.eyes.insert(fid, parse_eyes(fid, subs));
        })?,
        b"HAIR" => extract_records(reader, end, b"HAIR", &mut |fid, subs| {
            index.hair.insert(fid, parse_hair(fid, subs));
        })?,
        // AI / dialogue / effect stubs (#446, #447). Follow the
        // #458 supplementary-record pattern: minimal struct
        // (EDID + FULL + a few scalars), no deep decoding.
        b"PACK" => {
            // PLDT's Near Reference / In Cell / Object ID FormIDs are
            // plugin-local; remap to global here (same #1666 pattern
            // as QUST/PERK) so consumers compare against entities'
            // global FormIdComponents.
            let pack_remap = reader.get_form_id_remap();
            extract_records(reader, end, b"PACK", &mut |fid, subs| {
                index
                    .packages
                    .insert(fid, parse_pack(fid, subs, &pack_remap, game));
            })?;
        }
        b"QUST" => {
            // Stage CTDA params live in plugin-local FormID space; remap
            // them to global here (#1666) so the condition evaluator can
            // compare against entities' global FormIdComponents.
            let qust_remap = reader.get_form_id_remap();
            extract_records(reader, end, b"QUST", &mut |fid, subs| {
                index.quests.insert(fid, parse_qust(fid, subs, &qust_remap));
            })?;
        }
        // DIAL tops a nested GRUP tree: a top-level GRUP labelled
        // "DIAL" containing DIAL records, each (often) followed by
        // a Topic Children sub-GRUP (group_type == 7) whose label
        // is the parent DIAL's form_id u32 and whose contents are
        // INFO records. The generic `extract_records` walker
        // filters on a single `expected_type` and silently drops
        // every INFO. The dedicated walker below threads both
        // record types through. See #631 / #447.
        b"DIAL" => extract_dial_with_info(reader, end, &mut index.dialogues)?,
        b"MESG" => extract_records(reader, end, b"MESG", &mut |fid, subs| {
            index.messages.insert(fid, parse_mesg(fid, subs));
        })?,
        b"PERK" => {
            // Entry CTDA params are plugin-local FormIDs; remap to global
            // (#1666) so `HasPerk` / `GetIsID` compare in the same space as
            // entities' global FormIdComponents.
            let perk_remap = reader.get_form_id_remap();
            extract_records(reader, end, b"PERK", &mut |fid, subs| {
                index.perks.insert(fid, parse_perk(fid, subs, &perk_remap));
            })?;
        }
        _ => unreachable!("dispatch_misc_gameplay_a_group: unexpected label {label:?}"),
    }
    Ok(())
}
