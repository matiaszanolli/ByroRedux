//! Misc-gameplay dispatch, part B — split out of
//! `parse_esm_with_load_order` (#2060). Combat/magic/supporting
//! records (SPEL/ENCH/MGEF/AVIF/ACTI/TERM/FLST/PROJ/EFSH/IMOD/ARMA/OTFT/
//! BPTD/REPU/EXPL/CSTY/IDLE/IPCT/IPDS/COBJ). ACTI/TERM are dual-target
//! (typed + `cells.statics`); the rest are typed-only. See
//! `dispatch_misc_gameplay_a.rs` for the weather/AI/dialogue half.

use super::*;

/// Handles one of this domain's labels; the caller has already
/// verified `label` is one of them.
pub(super) fn dispatch_misc_gameplay_b_group(
    label: &[u8; 4],
    reader: &mut EsmReader,
    end: usize,
    statics: &mut HashMap<u32, StaticObject>,
    index: &mut EsmIndex,
    game: GameKind,
) -> Result<()> {
    match label {
        b"SPEL" => extract_records(reader, end, b"SPEL", &mut |fid, subs| {
            index.spells.insert(fid, parse_spel(fid, subs));
        })?,
        // ENCH enchantments (#629 / FNV-D2-01). Same scaffolding as
        // SPEL — ENIT carries type/charge/cost/flags; full effect
        // chain decoding lands with MGEF application.
        b"ENCH" => extract_records(reader, end, b"ENCH", &mut |fid, subs| {
            index.enchantments.insert(fid, parse_ench(fid, subs));
        })?,
        b"MGEF" => extract_records(reader, end, b"MGEF", &mut |fid, subs| {
            let rec = parse_mgef(fid, subs);
            // #969 / OBL-D3-NEW-05 — Oblivion's SPEL/ENCH/ALCH/INGR
            // EFID values are the 4-char effect code (raw bytes),
            // NOT a u32 FormID like every other Bethesda game. Build
            // a code→FormID side index so the (pending) magic-system
            // runtime can resolve EFID lookups on Oblivion content.
            // Gated on the game variant so an FNV/Skyrim MGEF that
            // happens to have a 4-char EDID prefix can't shadow an
            // Oblivion entry on a multi-game session. `read_zstring`
            // already strips the trailing null, so a real Oblivion
            // code lands here as `editor_id.len() == 4`.
            if game == GameKind::Oblivion {
                if let Ok(code) = <[u8; 4]>::try_from(rec.editor_id.as_bytes()) {
                    index.magic_effects_by_code.insert(code, fid);
                }
            }
            index.magic_effects.insert(fid, rec);
        })?,
        // AVIF actor-value records (#519). Pre-fix every NPC
        // skill-bonus, BOOK skill-book teach ref, and AVIF-keyed
        // condition predicate dangled because the top-level group
        // hit the catch-all skip.
        b"AVIF" => {
            let avif_remap = reader.get_form_id_remap();
            extract_records(reader, end, b"AVIF", &mut |fid, subs| {
                index
                    .actor_values
                    .insert(fid, parse_avif(fid, subs, &avif_remap));
            })?;
        }
        // ACTI / TERM #521 — dual-target: typed map for SCRI /
        // menu-tree cross-refs AND `cells.statics` for visual
        // placement. Pre-#527 the cell first-pass walked them via
        // the MODL catch-all and the records second-pass walked
        // them again for the typed parser; the fused helper does
        // both in one walk.
        b"ACTI" => extract_records_with_modl(reader, end, b"ACTI", statics, &mut |fid, subs| {
            index.activators.insert(fid, parse_acti(fid, subs));
        })?,
        b"TERM" => extract_records_with_modl(reader, end, b"TERM", statics, &mut |fid, subs| {
            index.terminals.insert(fid, parse_term(fid, subs));
        })?,
        // FLST FormID lists — flat arrays referenced by
        // `IsInList <flst>` perk-entry-point conditions, COBJ
        // recipe filters, FNV Caravan deck composition, and quest
        // objective lookups. Pre-#630 the top-level group fell
        // through to the catch-all skip and every `IsInList`
        // returned "not in list", silently disabling ~50 vanilla
        // FNV PERKs and the entire Caravan mini-game.
        b"FLST" => extract_records(reader, end, b"FLST", &mut |fid, subs| {
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
        b"PROJ" => extract_records(reader, end, b"PROJ", &mut |fid, subs| {
            index.projectiles.insert(fid, parse_proj(fid, subs));
        })?,
        b"EFSH" => extract_records(reader, end, b"EFSH", &mut |fid, subs| {
            index.effect_shaders.insert(fid, parse_efsh(fid, subs));
        })?,
        b"IMOD" => extract_records(reader, end, b"IMOD", &mut |fid, subs| {
            index.item_mods.insert(fid, parse_imod(fid, subs));
        })?,
        b"ARMA" => extract_records(reader, end, b"ARMA", &mut |fid, subs| {
            index.armor_addons.insert(fid, parse_arma(fid, subs, game));
        })?,
        // OTFT — Skyrim+ outfit (default-equipped armor list).
        // Pre-Skyrim plugins don't ship OTFT groups; the walker
        // skips them silently when absent (no group hit).
        // `INAM` entries are plugin-local; remap to global here
        // (#2079) so `index.items` / `.leveled_items` lookups hit.
        b"OTFT" => {
            let otft_remap = reader.get_form_id_remap();
            extract_records(reader, end, b"OTFT", &mut |fid, subs| {
                index
                    .outfits
                    .insert(fid, parse_otft(fid, subs, &otft_remap));
            })?
        }
        b"BPTD" => extract_records(reader, end, b"BPTD", &mut |fid, subs| {
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
        b"REPU" => extract_records(reader, end, b"REPU", &mut |fid, subs| {
            index.reputations.insert(fid, parse_repu(fid, subs));
        })?,
        b"EXPL" => extract_records(reader, end, b"EXPL", &mut |fid, subs| {
            index.explosions.insert(fid, parse_expl(fid, subs));
        })?,
        b"CSTY" => extract_records(reader, end, b"CSTY", &mut |fid, subs| {
            index.combat_styles.insert(fid, parse_csty(fid, subs));
        })?,
        b"IDLE" => extract_records(reader, end, b"IDLE", &mut |fid, subs| {
            index.idle_animations.insert(fid, parse_idle(fid, subs));
        })?,
        b"IPCT" => extract_records(reader, end, b"IPCT", &mut |fid, subs| {
            index.impacts.insert(fid, parse_ipct(fid, subs));
        })?,
        b"IPDS" => extract_records(reader, end, b"IPDS", &mut |fid, subs| {
            index.impact_data_sets.insert(fid, parse_ipds(fid, subs));
        })?,
        b"COBJ" => extract_records(reader, end, b"COBJ", &mut |fid, subs| {
            index.recipes.insert(fid, parse_cobj(fid, subs));
        })?,
        _ => unreachable!("dispatch_misc_gameplay_b_group: unexpected label {label:?}"),
    }
    Ok(())
}
