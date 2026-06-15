//! Walker functions extracted from ../mod.rs (stage B refactor).
//!
//! Functions: parse_cell_group, parse_refr_group, parse_land_record.

use super::helpers::{read_form_id, read_form_id_array, read_zstring};
use super::*;
use crate::esm::reader::GameKind;
use crate::esm::records::common::read_lstring_or_zstring;
use crate::esm::records::{parse_navm, NavmRecord};
use crate::esm::sub_reader::SubReader;

/// Canonical XCLL sub-record sizes per game era. Pinned here so the
/// `xcll_size_sanity_warn` helper and any future variant-enum gate share
/// the same source of truth.
///   - Oblivion (TES4): 28 / 32 / 36 bytes. NOT "padding" — the tail
///     fields ARE authored (xEdit TES4 `wbStruct(XCLL)` / OpenMW
///     `loadcell.cpp` `case 36`): 28 = shared prefix only; 32 = shared +
///     `Directional Fade`(@28); 36 = shared + `Directional Fade`(@28) +
///     `Fog Clip Dist`(@32) — the FULL TES4 Lighting. TES4 has NO
///     `Fog Power` field (that is the FO3/FNV 40-byte addition). #1312.
///   - FNV / FO3 / FO4 / FO76: 40 bytes (shared + 12-byte dir_fade /
///     fog_clip / fog_power tail).
///   - Skyrim LE / SE: 92 bytes (shared + 6-RGBA ambient cube + specular
///     + extended fog + light-fade range).
///   - Starfield: 108 bytes (#1291 / #1293). NOT "Skyrim + 16-byte
///     tail" — Starfield's XCLL shares only bytes 0-39 with Skyrim and
///     then diverges into a distinct volumetric height-fog model (no
///     ambient cube / specular / fresnel): Fog Color Far, Fog Max,
///     Light Fade, Near/Far Height Mid/Range, Fog Color High Near/Far,
///     four fog scales, and an Interior Type enum. It gets a dedicated
///     decode (`game == Starfield && len == 108`) per xEdit SF1
///     `wbStruct(XCLL,'Lighting')`, byte-verified against Starfield.esm.
///     The ~11 985 vanilla Starfield.esm cells all ship at exactly 108.
const XCLL_SIZES_OBLIVION: &[usize] = &[28, 32, 36];
const XCLL_SIZES_FALLOUT_ERA: &[usize] = &[28, 40];
const XCLL_SIZES_SKYRIM: &[usize] = &[28, 92];
const XCLL_SIZES_STARFIELD: &[usize] = &[28, 108];

/// Warn (at WARN level) when an XCLL sub-record size doesn't match the
/// canonical size set for its plugin's game era. Doesn't change parse
/// behavior — the size-based dispatch below still fires whatever extended
/// arm fits. Purpose: surface "your cell lighting authoring isn't matching
/// what the engine reads" so a modder gets a visible signal instead of
/// silent data loss. See `parse_cell_group` docstring for the failure
/// class this guards against.
fn xcll_size_sanity_warn(len: usize, game: GameKind) {
    let canonical = xcll_canonical_sizes(game);
    if !canonical.contains(&len) {
        log::warn!(
            "ESM XCLL with {len} bytes is non-canonical for {game:?} \
             (expected one of {canonical:?}); the size-based dispatch \
             will read whatever extended fields fit but cell lighting \
             may be mis-computed. Often indicates a malformed authoring \
             or cross-game plugin injection.",
        );
    }
}

/// Return the canonical XCLL byte-size set for a game era. Extracted from
/// `xcll_size_sanity_warn` so the (game → expected-sizes) map can be
/// asserted directly in tests; the const arrays + this helper are the
/// single source of truth for "what XCLL shape does this game ship?".
fn xcll_canonical_sizes(game: GameKind) -> &'static [usize] {
    match game {
        GameKind::Oblivion => XCLL_SIZES_OBLIVION,
        GameKind::Fallout3NV | GameKind::Fallout4 | GameKind::Fallout76 => XCLL_SIZES_FALLOUT_ERA,
        GameKind::Skyrim => XCLL_SIZES_SKYRIM,
        GameKind::Starfield => XCLL_SIZES_STARFIELD,
    }
}

#[cfg(test)]
mod xcll_gate_tests {
    //! Pin the (game, canonical-XCLL-size) map and the sanity-warn helper.
    //! The warn itself is invisible to assertions (would need log capture)
    //! but the canonical-size lookup is the source-of-truth the warn keys
    //! on, so pinning it catches any drift in either direction.
    use super::*;

    #[test]
    fn oblivion_xcll_sizes_pinned() {
        assert_eq!(xcll_canonical_sizes(GameKind::Oblivion), &[28, 32, 36]);
    }

    #[test]
    fn fallout_era_xcll_sizes_pinned() {
        // FNV / FO3 / FO4 / FO76 share the 40-byte tail. Starfield
        // was here until #1291 — it ships 108 bytes (Skyrim+ 92-byte
        // body + 16-byte SF-specific tail), now pinned separately.
        for game in [
            GameKind::Fallout3NV,
            GameKind::Fallout4,
            GameKind::Fallout76,
        ] {
            assert_eq!(
                xcll_canonical_sizes(game),
                &[28, 40],
                "{game:?} must use the FNV-era 40-byte XCLL tail",
            );
        }
    }

    #[test]
    fn skyrim_xcll_sizes_pinned() {
        assert_eq!(xcll_canonical_sizes(GameKind::Skyrim), &[28, 92]);
    }

    /// #1291 — Starfield authors a 108-byte XCLL on every vanilla
    /// interior cell (empirically verified across 14 808 cells in
    /// Starfield.esm + ShatteredSpace.esm + BlueprintShips-Starfield.esm
    /// — all exactly 108 bytes, no other variants). Pre-#1291 this
    /// was bucketed with the FNV-era 40-byte tail, which tripped the
    /// sanity-warn 11 985× on a vanilla Starfield.esm parse and
    /// (more importantly) misled future SF cell-layout debugging by
    /// suggesting the SF authoring shape matched FO4.
    ///
    /// #1293 corrected the original assumption here: Starfield's XCLL
    /// does NOT decode "through 92 bytes" as Skyrim — it shares only
    /// bytes 0-39 and then diverges into a volumetric height-fog model
    /// (no ambient cube). The `b"XCLL"` arm now has a dedicated
    /// `game == Starfield && len == 108` branch decoding the full SF
    /// layout (xEdit SF1, byte-verified against Starfield.esm); see
    /// `parse_cell_starfield_xcll_decodes_volumetric_height_fog_tail`.
    /// Adding 108 to the canonical set keeps the sanity-warn quiet.
    #[test]
    fn starfield_xcll_sizes_pinned() {
        assert_eq!(
            xcll_canonical_sizes(GameKind::Starfield),
            &[28, 108],
            "Starfield's vanilla XCLL is 108 bytes (Skyrim+ 92-byte \
             body + 16-byte SF tail). See #1291.",
        );
    }

    /// Counterpart to `fnv_xcll_at_88_bytes_is_non_canonical`: a
    /// Starfield cell with a 40-byte XCLL (the FNV-era authoring) is
    /// non-canonical and must trip the warn. Cross-game plugin
    /// stacks that inject a FO4 cell into a SF master with FNV-era
    /// XCLL authoring would surface here.
    #[test]
    fn starfield_xcll_at_40_bytes_is_non_canonical() {
        let canonical = xcll_canonical_sizes(GameKind::Starfield);
        assert!(
            !canonical.contains(&40),
            "Starfield canonical sizes {canonical:?} must NOT include 40 — \
             the FNV-era 40-byte tail is a cross-game injection signal, \
             not a vanilla SF shape",
        );
    }

    /// And the inverse: a FNV/FO4/FO76 cell with a 108-byte XCLL
    /// (Starfield authoring injected into a non-SF master) must
    /// trip the warn. Mirror of `starfield_xcll_at_40_bytes_is_non_canonical`.
    #[test]
    fn fallout_xcll_at_108_bytes_is_non_canonical() {
        for game in [
            GameKind::Fallout3NV,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Skyrim,
        ] {
            let canonical = xcll_canonical_sizes(game);
            assert!(
                !canonical.contains(&108),
                "{game:?} canonical sizes {canonical:?} must NOT include 108 — \
                 the 108-byte XCLL is Starfield-only authoring; finding \
                 one in a non-SF master is a cross-game injection signal",
            );
        }
    }

    /// The classic failure class from the survey: a FNV cell with an
    /// 88-byte XCLL. Pre-#1277-Task4 this silently parsed as
    /// "Oblivion + partial FNV tail" (length-only dispatch fires the
    /// ≥40 branch since 88 ≥ 40 but skips the ≥92 branch). The warn
    /// helper detects this — 88 isn't in {28, 40} for Fallout3NV.
    #[test]
    fn fnv_xcll_at_88_bytes_is_non_canonical() {
        let canonical = xcll_canonical_sizes(GameKind::Fallout3NV);
        assert!(
            !canonical.contains(&88),
            "FNV canonical sizes {canonical:?} must NOT include 88 — \
             else the survey's 'silently parses as Oblivion + partial FNV' \
             regression wouldn't surface a warn",
        );
    }

    /// Inverse: an Oblivion cell with a 40-byte XCLL (someone using the
    /// FNV tail by accident). Pre-task this parsed the FNV tail fields
    /// into Oblivion data. The warn helper detects this — 40 isn't in
    /// the Oblivion canonical set.
    #[test]
    fn oblivion_xcll_at_40_bytes_is_non_canonical() {
        let canonical = xcll_canonical_sizes(GameKind::Oblivion);
        assert!(
            !canonical.contains(&40),
            "Oblivion canonical sizes {canonical:?} must NOT include 40 — \
             else an Oblivion plugin with an accidentally-FNV-shaped \
             XCLL would silently consume the FNV tail",
        );
    }

    /// Inverse: a Skyrim cell with a 40-byte XCLL (someone using the FNV
    /// tail). 40 isn't in Skyrim's set, so warn fires.
    #[test]
    fn skyrim_xcll_at_40_bytes_is_non_canonical() {
        let canonical = xcll_canonical_sizes(GameKind::Skyrim);
        assert!(
            !canonical.contains(&40),
            "Skyrim canonical sizes {canonical:?} must NOT include 40",
        );
    }

    /// #1579 — the SF XCLL dispatch gate must be `>= 108`, not `== 108`, so a
    /// future-DLC SF cell with trailing pad still takes the SF arm instead of
    /// falling through to the Skyrim `>= 92` ambient-cube path. Mirrors the
    /// in-decoder predicate (`game == Starfield && len >= 108`).
    #[test]
    fn starfield_xcll_above_108_still_takes_sf_arm() {
        let takes_sf_arm = |game: GameKind, len: usize| game == GameKind::Starfield && len >= 108;
        assert!(takes_sf_arm(GameKind::Starfield, 108), "vanilla 108 still SF");
        assert!(
            takes_sf_arm(GameKind::Starfield, 112),
            "112-byte SF cell must stay SF, not fall to the Skyrim arm",
        );
        assert!(
            !takes_sf_arm(GameKind::Skyrim, 112),
            "a non-SF 112-byte XCLL must NOT take the SF arm",
        );
    }
}

/// Walk the CELL group hierarchy to find interior cells and their placed references.
///
/// `game` is the HEDR-derived [`GameKind`] of the plugin; the XCLL parser
/// uses it to warn on (game, size) mismatches that would otherwise let a
/// malformed FNV XCLL silently parse as Oblivion + partial-FNV (the
/// canonical sizes are Oblivion ≤ 36 / FNV-era 40 / Skyrim 92 bytes —
/// pre-#1277-Task4 the dispatch was length-only and accepted any
/// arrangement). The plumbed game also unlocks future per-era CELL
/// sub-record routing (XCMT vs XCCM today gates on absence/presence;
/// same pattern applies).
pub(crate) fn parse_cell_group(
    reader: &mut EsmReader,
    end: usize,
    cells: &mut HashMap<String, CellData>,
    game: GameKind,
) -> Result<()> {
    // Track the last parsed interior cell so we can attach children groups to it.
    let mut current_cell: Option<(u32, String)> = None;

    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);

            match sub_group.group_type {
                // Interior cell block (2) and sub-block (3): recurse.
                2 | 3 => {
                    parse_cell_group(reader, sub_end, cells, game)?;
                }
                // Cell children groups (6=temporary, 8=persistent, 9=visible distant).
                6 | 8 | 9 => {
                    if let Some((_, ref editor_id)) = current_cell {
                        let key = editor_id.to_ascii_lowercase();
                        let mut refs = Vec::new();
                        let mut _land = None; // Interior cells don't have LAND records
                        let mut navmeshes = Vec::new();
                        parse_refr_group(reader, sub_end, &mut refs, &mut _land, &mut navmeshes)?;
                        if let Some(cell) = cells.get_mut(&key) {
                            cell.references.extend(refs);
                            cell.navmeshes.extend(navmeshes);
                        }
                    } else {
                        reader.skip_group(&sub_group);
                    }
                }
                _ => {
                    reader.skip_group(&sub_group);
                }
            }
        } else {
            let header = reader.read_record_header()?;
            if &header.record_type == b"CELL" {
                let subs = reader.read_sub_records(&header)?;
                let mut editor_id = String::new();
                // #624 / SK-D6-NEW-02 — display name from FULL. Routes
                // through the lstring helper so localized Skyrim plugins
                // get a `<lstring 0x…>` placeholder instead of garbage
                // 3-byte cstrings.
                let mut display_name: Option<String> = None;
                let mut is_interior = false;
                let mut lighting = None;
                let mut water_height: Option<f32> = None;
                let mut image_space_form: Option<u32> = None;
                let mut water_type_form: Option<u32> = None;
                let mut acoustic_space_form: Option<u32> = None;
                let mut music_type_form: Option<u32> = None;
                // #693 / O3-N-05 — pre-Skyrim XCMT (1-byte enum) and
                // Skyrim XCCM (4-byte CLMT FormID). Both fell to the
                // catch-all `_` arm pre-fix.
                let mut music_type_enum: Option<u8> = None;
                let mut climate_override: Option<u32> = None;
                let mut location_form: Option<u32> = None;
                let mut regions: Vec<u32> = Vec::new();
                // SK-D6-02 / #566 — LTMP lighting-template FormID. Skyrim+
                // cells that omit XCLL fall back to this LGTM reference;
                // pre-#566 the link was dropped on the catch-all `_` arm
                // and the cell rendered with the engine default ambient.
                let mut lighting_template_form: Option<u32> = None;
                // #692 — XOWN / XRNK / XGLB ownership tuple. All three
                // sub-records optional; cell ends up with `Some` only
                // when at least XOWN is present. XRNK and XGLB without
                // XOWN are nonsensical and dropped (the consumer would
                // have nothing to gate against).
                let mut ownership_owner: Option<u32> = None;
                let mut ownership_rank: Option<i32> = None;
                let mut ownership_global: Option<u32> = None;
                // #970 / OBL-D3-NEW-06 — Oblivion-era cell-level RGB
                // tint override (`RCLR`, 3 bytes). Rare even on
                // Oblivion (editor-authored), absent on FO3+ vanilla.
                // Parsed cross-game; the field is harmless when None
                // and lets modded post-Oblivion cells still surface
                // the override.
                let mut regional_color_override: Option<[u8; 3]> = None;
                // #1188 — FO4+ PreCombined Mesh references. XCRI holds
                // (u32 mesh_count + u32 ref_count + N×u32 hashes +
                // M×u32 absorbed-refr formids). XPRI holds a smaller
                // additional list of refr formids absorbed by the
                // precombines. Both lists feed `absorbed_refs` which
                // the cell loader uses to skip individual REFR placement
                // for the geometry baked into the `_oc.nif` files
                // referenced by `precombined_mesh_hashes`.
                //
                // Empirically decoded against vanilla
                // `DmndDugoutInn01` (form 0x00001E5D, 39 hashes / 962
                // XCRI refs / 102 XPRI refs) — see the audit memory.
                let mut precombined_mesh_hashes: Vec<u32> = Vec::new();
                let mut absorbed_refs: std::collections::HashSet<u32> =
                    std::collections::HashSet::new();

                for sub in &subs {
                    match &sub.sub_type {
                        b"EDID" => editor_id = read_zstring(&sub.data),
                        // #624 / SK-D6-NEW-02 — Skyrim cells DO ship FULL
                        // (e.g. WhiterunBanneredMare's FULL = "The
                        // Bannered Mare"). Pre-fix this fell to the
                        // catch-all `_` arm and the display name was
                        // dropped. The lstring helper auto-routes the
                        // 4-byte STRINGS-table case for localized
                        // plugins.
                        b"FULL" => display_name = Some(read_lstring_or_zstring(&sub.data)),
                        b"DATA" if !sub.data.is_empty() => is_interior = sub.data[0] & 1 != 0,
                        b"XCLW" => {
                            // XCLW: f32 water plane height in world units
                            // (Z-up). Same layout across Oblivion / FO3 / FNV
                            // / Skyrim — the cell's water surface sits at
                            // this Z (interior) or Z-in-worldspace (exterior).
                            // `xclw_water_height` returns None for the
                            // `#INT_MIN#` "no water" sentinel. See #397 /
                            // #356 / #1305.
                            water_height = super::helpers::xclw_water_height(&sub.data);
                        }
                        // Skyrim extended CELL sub-records (#356). Each is
                        // a 4-byte FormID; the walker previously dropped
                        // them on the `_` arm so the renderer / audio /
                        // quest system had no per-cell context.
                        b"XCIM" => image_space_form = read_form_id(&sub.data),
                        b"XCWT" => water_type_form = read_form_id(&sub.data),
                        // LTMP — lighting-template FormID (SK-D6-02 / #566).
                        // Same shape as the other 4-byte FormID slots; the
                        // cell loader walks `EsmIndex.lighting_templates`
                        // when XCLL is absent so vanilla Skyrim interior
                        // cells (Solitude inn cluster, Dragonsreach
                        // throne room, Markarth cells) render with the
                        // template ambient instead of the engine default.
                        b"LTMP" => lighting_template_form = read_form_id(&sub.data),
                        b"XCAS" => acoustic_space_form = read_form_id(&sub.data),
                        b"XCMO" => music_type_form = read_form_id(&sub.data),
                        // #1188 — XCRI: FO4+ PreCombined Mesh references.
                        //   `u32 mesh_count + u32 ref_count
                        //    + mesh_count × u32 hashes
                        //    + ref_count × u32 visibility-group refs`
                        // For each hash, the precombined NIF file lives
                        // at `meshes\precombined\<cell_fid:08x>_<hash:08x>_oc.nif`.
                        //
                        // The `ref_count`-sized tail is the
                        // **visibility group** for the precombines —
                        // refs participating in the combined-cull bake.
                        // It is NOT "refs to skip individual spawn"
                        // (the Dmnd Dugout Inn first-iteration regressed
                        // the bar / couch / lamps because we treated
                        // these as absorbed). Skip-placement is XPRI's
                        // job, below.
                        b"XCRI" if sub.data.len() >= 8 => {
                            let mesh_count =
                                u32::from_le_bytes(sub.data[0..4].try_into().unwrap()) as usize;
                            let ref_count =
                                u32::from_le_bytes(sub.data[4..8].try_into().unwrap()) as usize;
                            let expected =
                                8 + mesh_count.saturating_mul(4) + ref_count.saturating_mul(4);
                            if expected != sub.data.len() {
                                log::warn!(
                                    "CELL {:08X} XCRI size mismatch: hdr={}+{} expected_payload={} \
                                     actual={} — skipping",
                                    header.form_id,
                                    mesh_count,
                                    ref_count,
                                    expected,
                                    sub.data.len(),
                                );
                            } else {
                                precombined_mesh_hashes.reserve(mesh_count);
                                let mut off = 8;
                                for _ in 0..mesh_count {
                                    let h = u32::from_le_bytes(
                                        sub.data[off..off + 4].try_into().unwrap(),
                                    );
                                    precombined_mesh_hashes.push(h);
                                    off += 4;
                                }
                                // We intentionally do NOT consume the
                                // ref_count tail into `absorbed_refs`.
                                // See XPRI below for the skip-placement
                                // source of truth.
                            }
                        }
                        // #1188 — XPRI: list of REFR formids absorbed
                        // into precombines (~100 entries for FO4
                        // interiors; matches the architecture-only
                        // shell). The cell loader MUST skip these
                        // REFRs' individual placement — their geometry
                        // is already baked into the `_oc.nif` files
                        // referenced by `precombined_mesh_hashes`.
                        // Format: pure `N × u32`.
                        b"XPRI" if sub.data.len() % 4 == 0 => {
                            absorbed_refs.reserve(sub.data.len() / 4);
                            for chunk in sub.data.chunks_exact(4) {
                                let fid = u32::from_le_bytes(chunk.try_into().unwrap());
                                absorbed_refs.insert(fid);
                            }
                        }
                        // #693 / O3-N-05 — XCMT pre-Skyrim music enum
                        // (Oblivion / FO3 / FNV). 1-byte payload.
                        b"XCMT" if !sub.data.is_empty() => {
                            music_type_enum = Some(sub.data[0]);
                        }
                        // #693 / O3-N-05 — XCCM Skyrim climate override
                        // (per-cell CLMT FormID, exterior cells only,
                        // but a few interior mods have been seen with
                        // it for "outside through window" effects).
                        b"XCCM" => climate_override = read_form_id(&sub.data),
                        b"XLCN" => location_form = read_form_id(&sub.data),
                        // XCLR is a packed FormID array — region tags
                        // referenced by REGN records. Variable length;
                        // empty list is normal.
                        b"XCLR" => regions = read_form_id_array(&sub.data),
                        // #692 — XOWN owner, XRNK faction-rank gate,
                        // XGLB global-variable FormID. Same shape on
                        // CELL + REFR. Cross-game (Oblivion / FO3 /
                        // FNV / Skyrim+).
                        b"XOWN" => {
                            ownership_owner = read_form_id(&sub.data);
                        }
                        b"XRNK" => {
                            ownership_rank = SubReader::new(&sub.data).i32().ok();
                        }
                        b"XGLB" => {
                            ownership_global = read_form_id(&sub.data);
                        }
                        // #970 / OBL-D3-NEW-06 — Oblivion CELL regional
                        // tint. nif.xml-equivalent here is the GECK
                        // CELL Edit, "Regional Map Color" picker; on
                        // disk it's `RCLR` with 3 RGB bytes (no alpha).
                        // Some plugins ship a 4-byte payload with a
                        // trailing pad — accept >= 3 and read the
                        // first three bytes only.
                        b"RCLR" if sub.data.len() >= 3 => {
                            regional_color_override = Some([sub.data[0], sub.data[1], sub.data[2]]);
                        }
                        b"XCLL" if sub.data.len() >= 28 => {
                            // XCLL layout (shared 28-byte prefix across all games):
                            //   ambient RGBA, directional RGBA, fog-near RGBA
                            //   fog_near f32, fog_far f32
                            //   directional rotation X (i32, degrees)
                            //   directional rotation Y (i32, degrees)
                            //
                            // Oblivion stops at 36 bytes (no extended tail).
                            // FNV adds 12 bytes — dir_fade / fog_clip /
                            // fog_power. Skyrim+ extends to 92 bytes with
                            // the 6×RGBA ambient cube + specular + extended
                            // fog + light fade range. Two separate gates
                            // mirror the on-disk shape (#379).
                            //
                            // #1277 Task 4: warn on (game, size) mismatch.
                            // The dispatch below is still purely length-based
                            // (preserves pre-task behavior for any malformed
                            // cell already in test fixtures), but the warning
                            // surfaces "this XCLL doesn't match what this
                            // game ships" so the modder sees their cell
                            // lighting is being silently shortened or
                            // mis-parsed. The classic symptom — a malformed
                            // FNV cell at 88 bytes silently parsing as
                            // Oblivion + partial-FNV — would fire two warns
                            // here: (FNV, 88) isn't 40, and 88 < 92 doesn't
                            // reach the Skyrim path either.
                            xcll_size_sanity_warn(sub.data.len(), game);
                            //
                            // Byte order in colour fields is RGB, not
                            // BGR/D3DCOLOR; the 4th byte is unused padding
                            // — xEdit defines XCLL colours as (Red, Green,
                            // Blue, Unknown). The cool-blue saloon ambient
                            // is correct; warmth comes from placed LIGH
                            // oil-lanterns, not from the cell fill.
                            // See #389 / FNV-2-L3.
                            let mut r = SubReader::new(&sub.data);
                            let ambient = r.rgb_color().unwrap_or([0.0; 3]);
                            let directional_color = r.rgb_color().unwrap_or([0.0; 3]);
                            let fog_color = r.rgb_color().unwrap_or([0.0; 3]);
                            let fog_near = r.f32_or_default();
                            let fog_far = r.f32_or_default();
                            let rot_x = (r.i32_or_default() as f32).to_radians();
                            let rot_y = (r.i32_or_default() as f32).to_radians();

                            // #1293 — Starfield's 108-byte XCLL diverges from
                            // the Skyrim layout at offset 40: bytes 40-107 are a
                            // volumetric height-fog model (xEdit SF1
                            // `wbStruct(XCLL,'Lighting')`), NOT the Skyrim
                            // ambient-cube / specular / fresnel. The Skyrim arm
                            // below would misread ~52 bytes per SF cell, so
                            // Starfield gets a dedicated decode here. `r` is at
                            // offset 28 (shared 0-27 prefix read above). Byte 28
                            // is `Gravity Scale` (Skyrim's `Directional Fade`
                            // slot); 32/36 fog clip/power are shared; 40-55
                            // fog-far-colour / max / light-fade map onto the
                            // base fields; the rest is SF-only height-fog.
                            // `>= 108` (not `== 108`): a future-DLC SF XCLL with
                            // trailing pad must still take the SF arm and not
                            // fall through to the Skyrim `>= 92` ambient-cube
                            // path, which would misread the height-fog bytes.
                            // The SF decode reads a fixed 108-byte body and
                            // ignores any excess. See #1579.
                            if game == GameKind::Starfield && sub.data.len() >= 108 {
                                let gravity_scale = r.f32_or_default(); // 28
                                let fog_clip = r.f32_or_default(); // 32 Fog Clip Distance
                                let fog_power = r.f32_or_default(); // 36 Fog Power
                                let fog_far_color = r.rgb_color().unwrap_or([0.0; 3]); // 40 Fog Color Far
                                let fog_max = r.f32_or_default(); // 44 Fog Max
                                let lf_begin = r.f32_or_default(); // 48 Light Fade Begin
                                let lf_end = r.f32_or_default(); // 52 Light Fade End
                                let unknown_color = r.rgb_color().unwrap_or([0.0; 3]); // 56 Unknown
                                let near_height_mid = r.f32_or_default(); // 60 Near Height Mid
                                let near_height_range = r.f32_or_default(); // 64 Near Height Range
                                let fog_color_high_near = r.rgb_color().unwrap_or([0.0; 3]); // 68
                                let fog_color_high_far = r.rgb_color().unwrap_or([0.0; 3]); // 72
                                let high_density_scale = r.f32_or_default(); // 76 High Density Scale
                                let fog_near_scale = r.f32_or_default(); // 80 Fog Near Scale
                                let fog_far_scale = r.f32_or_default(); // 84 Fog Far Scale
                                let fog_high_near_scale = r.f32_or_default(); // 88 Fog High Near Scale
                                let fog_high_far_scale = r.f32_or_default(); // 92 Fog High Far Scale
                                let far_height_mid = r.f32_or_default(); // 96 Far Height Mid
                                let far_height_range = r.f32_or_default(); // 100 Far Height Range
                                let interior_type = r.u8_or_default(); // 104 (105-107 unused pad)
                                lighting = Some(CellLighting {
                                    ambient,
                                    directional_color,
                                    directional_rotation: [rot_x, rot_y],
                                    fog_color,
                                    fog_near,
                                    fog_far,
                                    // SF reuses byte 28 as gravity_scale, and has
                                    // no ambient cube / specular / fresnel.
                                    directional_fade: None,
                                    fog_clip: Some(fog_clip),
                                    fog_power: Some(fog_power),
                                    fog_far_color: Some(fog_far_color),
                                    fog_max: Some(fog_max),
                                    light_fade_begin: Some(lf_begin),
                                    light_fade_end: Some(lf_end),
                                    directional_ambient: None,
                                    specular_color: None,
                                    specular_alpha: None,
                                    fresnel_power: None,
                                    starfield: Some(StarfieldLighting {
                                        gravity_scale,
                                        unknown_color,
                                        near_height_mid,
                                        near_height_range,
                                        fog_color_high_near,
                                        fog_color_high_far,
                                        high_density_scale,
                                        fog_near_scale,
                                        fog_far_scale,
                                        fog_high_near_scale,
                                        fog_high_far_scale,
                                        far_height_mid,
                                        far_height_range,
                                        interior_type,
                                    }),
                                });
                                // SF XCLL fully decoded — skip the Skyrim path.
                                continue;
                            }

                            // #1312 — per-field gating, NOT a single `len >= 40`
                            // gate. The extended fields are independently sized:
                            // `Directional Fade`(@28) needs len >= 32,
                            // `Fog Clip Dist`(@32) needs len >= 36, `Fog Power`
                            // (@36, FO3/FNV-only) needs len >= 40. A 36-byte
                            // Oblivion XCLL is the FULL TES4 Lighting (dir_fade +
                            // fog_clip, no fog_power) — the old `>= 40` gate
                            // silently dropped both. `.then(|| …)` only advances
                            // the reader when the field is present, so the cursor
                            // stays aligned at every size. (xEdit TES4 +
                            // OpenMW `loadcell.cpp` case 36.)
                            let dir_fade = (sub.data.len() >= 32).then(|| r.f32_or_default());
                            let fog_clip = (sub.data.len() >= 36).then(|| r.f32_or_default());
                            let fog_power = (sub.data.len() >= 40).then(|| r.f32_or_default());

                            let (
                                directional_ambient,
                                specular_color,
                                specular_alpha,
                                fresnel_power,
                                fog_far_color,
                                fog_max,
                                lf_begin,
                                lf_end,
                            ) = if sub.data.len() >= 92 {
                                // 6 × RGBA ambient cube (#367) — alpha pad
                                // discarded. Specular's 4th byte IS used as
                                // an alpha (handled below).
                                let mut ambient_cube = [[0.0f32; 3]; 6];
                                for face in &mut ambient_cube {
                                    *face = r.rgb_color().unwrap_or([0.0; 3]);
                                }
                                let spec = r.rgba_color().unwrap_or([0.0; 4]);
                                (
                                    Some(ambient_cube),
                                    Some([spec[0], spec[1], spec[2]]),
                                    Some(spec[3]),
                                    Some(r.f32_or_default()),
                                    Some(r.rgb_color().unwrap_or([0.0; 3])),
                                    Some(r.f32_or_default()),
                                    Some(r.f32_or_default()),
                                    Some(r.f32_or_default()),
                                )
                            } else {
                                (None, None, None, None, None, None, None, None)
                            };

                            lighting = Some(CellLighting {
                                ambient,
                                directional_color,
                                directional_rotation: [rot_x, rot_y],
                                fog_color,
                                fog_near,
                                fog_far,
                                directional_fade: dir_fade,
                                fog_clip,
                                fog_power,
                                fog_far_color,
                                fog_max,
                                light_fade_begin: lf_begin,
                                light_fade_end: lf_end,
                                directional_ambient,
                                specular_color,
                                specular_alpha,
                                fresnel_power,
                                // Skyrim/FNV/Oblivion path — no SF tail.
                                starfield: None,
                            });
                        }
                        _ => {}
                    }
                }

                if is_interior && !editor_id.is_empty() {
                    let key = editor_id.to_ascii_lowercase();
                    let ownership = ownership_owner.map(|owner| CellOwnership {
                        owner_form_id: owner,
                        faction_rank: ownership_rank,
                        global_var_form_id: ownership_global,
                    });
                    cells.insert(
                        key,
                        CellData {
                            form_id: header.form_id,
                            editor_id: editor_id.clone(),
                            display_name: display_name.clone(),
                            references: Vec::new(),
                            is_interior: true,
                            grid: None,
                            lighting: lighting.clone(),
                            landscape: None,
                            water_height,
                            image_space_form,
                            water_type_form,
                            acoustic_space_form,
                            music_type_form,
                            music_type_enum,
                            climate_override,
                            location_form,
                            regions: regions.clone(),
                            lighting_template_form,
                            ownership,
                            regional_color_override,
                            precombined_mesh_hashes,
                            absorbed_refs,
                            navmeshes: Vec::new(),
                        },
                    );
                    current_cell = Some((header.form_id, editor_id));
                } else {
                    current_cell = None;
                }
            } else {
                reader.skip_record(&header);
            }
        }
    }
    Ok(())
}

/// Parse REFR, LAND, and NAVM records within a cell children group.
///
/// `navmeshes` collects per-cell `NAVM` records (#1272). NAVMs nest
/// inside the cell's persistent/temporary children GRUPs and never
/// appear at top level in vanilla Bethesda masters — pre-fix the
/// catch-all skipped them on every game (`navmeshes=0` on FO3/FNV/
/// Skyrim SE/FO4 despite ~30k NAVMs per master).
pub(crate) fn parse_refr_group(
    reader: &mut EsmReader,
    end: usize,
    refs: &mut Vec<PlacedRef>,
    landscape: &mut Option<LandscapeData>,
    navmeshes: &mut Vec<NavmRecord>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            // Nested groups within cell children — recurse.
            let sub = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub);
            parse_refr_group(reader, sub_end, refs, landscape, navmeshes)?;
            continue;
        }

        let header = reader.read_record_header()?;
        // ACRE — Oblivion-only placed-creature reference (#396). FO3+
        // folded creature placements into ACHR; ACRE's wire layout
        // matches ACHR byte-for-byte on Oblivion (NAME/DATA/XSCL/XESP),
        // so it routes through the same handler.
        if &header.record_type == b"REFR"
            || &header.record_type == b"ACHR"
            || &header.record_type == b"ACRE"
        {
            let subs = reader.read_sub_records(&header)?;
            let mut base_form_id = 0u32;
            let mut position = [0.0f32; 3];
            let mut rotation = [0.0f32; 3];
            let mut scale = 1.0f32;
            let mut enable_parent: Option<EnableParent> = None;
            let mut teleport: Option<TeleportDest> = None;
            let mut primitive: Option<PrimitiveBounds> = None;
            let mut linked_refs: Vec<LinkedRef> = Vec::new();
            let mut rooms: Vec<u32> = Vec::new();
            let mut portals: Vec<PortalLink> = Vec::new();
            let mut radius_override: Option<f32> = None;
            let mut alt_texture_ref: Option<u32> = None;
            let mut land_texture_ref: Option<u32> = None;
            let mut texture_slot_swaps: Vec<TextureSlotSwap> = Vec::new();
            let mut emissive_light_ref: Option<u32> = None;
            let mut material_swap_ref: Option<u32> = None;
            // #692 — per-REFR ownership override. Scopes ownership to a
            // single placed object (chest, bed, locker) when the parent
            // cell is public. Same XOWN / XRNK / XGLB layout as CELL.
            let mut ownership_owner: Option<u32> = None;
            let mut ownership_rank: Option<i32> = None;
            let mut ownership_global: Option<u32> = None;

            for sub in &subs {
                let mut r = SubReader::new(&sub.data);
                match &sub.sub_type {
                    b"NAME" => {
                        base_form_id = r.u32_or_default();
                    }
                    b"DATA" if sub.data.len() >= 24 => {
                        // 6 floats: posX, posY, posZ, rotX, rotY, rotZ
                        position = r.f32_array::<3>().unwrap_or([0.0; 3]);
                        rotation = r.f32_array::<3>().unwrap_or([0.0; 3]);
                    }
                    b"XSCL" => {
                        scale = r.f32().unwrap_or(1.0);
                    }
                    // XESP — enable-parent gating (Skyrim+). 4-byte
                    // parent FormID + 1-byte flags; bit 0 = inverted.
                    // Pre-#349 every default-disabled "spawn after
                    // quest stage" REFR rendered immediately on cell
                    // load; the cell loader now skips these via
                    // `enable_parent.default_disabled()`.
                    b"XESP" if sub.data.len() >= 5 => {
                        let form_id = r.u32_or_default();
                        let inverted = r.u8_or_default() & 1 != 0;
                        enable_parent = Some(EnableParent { form_id, inverted });
                    }
                    // XTEL — Teleport destination (doors). Layout per
                    // UESP: DestRef(u32) + pos(3×f32) + rot(3×f32) +
                    // optional flags(u32) = 28 or 32 bytes. We read the
                    // first 28 bytes and ignore the trailing flags so
                    // FO3/FNV (sometimes 28-byte variant) parses the
                    // same as Skyrim/FO4. Pre-#412 every interior door
                    // was dead on activation.
                    b"XTEL" if sub.data.len() >= 28 => {
                        let destination = r.u32_or_default();
                        let dest_pos = r.f32_array::<3>().unwrap_or([0.0; 3]);
                        let dest_rot = r.f32_array::<3>().unwrap_or([0.0; 3]);
                        teleport = Some(TeleportDest {
                            destination,
                            position: dest_pos,
                            rotation: dest_rot,
                        });
                    }
                    // XPRM — Primitive bounds for trigger / activator
                    // volumes. Layout per UESP: bounds(3×f32) +
                    // color(3×f32) + unknown(f32) + shape_type(u32) =
                    // 32 bytes. Pre-#412 triggers were invisible.
                    b"XPRM" if sub.data.len() >= 32 => {
                        let bounds = r.f32_array::<3>().unwrap_or([0.0; 3]);
                        let color = r.f32_array::<3>().unwrap_or([0.0; 3]);
                        let unknown = r.f32_or_default();
                        let shape_type = r.u32_or_default();
                        primitive = Some(PrimitiveBounds {
                            bounds,
                            color,
                            unknown,
                            shape_type,
                        });
                    }
                    // XLKR — Linked refs. Layout: keyword(u32) +
                    // target(u32) = 8 bytes. Multiple XLKR allowed per
                    // REFR (patrol markers, door pairs, activator
                    // targets). Pre-#412 NPCs didn't patrol and doors
                    // didn't pair.
                    b"XLKR" if sub.data.len() >= 8 => {
                        let keyword = r.u32_or_default();
                        let target = r.u32_or_default();
                        linked_refs.push(LinkedRef { keyword, target });
                    }
                    // XRMR — Room membership. Layout per UESP:
                    // count(u32) + count × room_ref(u32). Pre-#412 FO4
                    // cell-subdivided interior culling couldn't work.
                    // Guard the count against the remaining bytes so a
                    // corrupt record can't over-read.
                    b"XRMR" if sub.data.len() >= 4 => {
                        let count = r.u32_or_default() as usize;
                        let max = r.remaining() / 4;
                        let n = count.min(max);
                        rooms.reserve(n);
                        for _ in 0..n {
                            rooms.push(r.u32_or_default());
                        }
                    }
                    // XPOD — Portal origin/destination pair. 8 bytes.
                    // Multiple XPOD sub-records allowed (one per portal
                    // connected to this REFR).
                    b"XPOD" if sub.data.len() >= 8 => {
                        let origin = r.u32_or_default();
                        let destination = r.u32_or_default();
                        portals.push(PortalLink {
                            origin,
                            destination,
                        });
                    }
                    // XRDS — Light radius override. Single f32.
                    b"XRDS" => {
                        radius_override = r.f32().ok();
                    }
                    // XATO — alternate TXST form ID. 4 bytes. When this
                    // REFR places a base mesh whose textures should be
                    // swapped for another TXST (the 140 MNAM-only vanilla
                    // FO4 TXSTs land here), the cell loader resolves
                    // this against `EsmCellIndex.texture_sets` and
                    // overlays the 8 slot paths + MNAM onto the spawned
                    // mesh. See audit FO4-DIM6-02 / #584.
                    b"XATO" => {
                        alt_texture_ref = r.u32().ok();
                    }
                    // XTNM — landscape TXST override form ID. Same
                    // 4-byte layout as XATO but scopes the override to
                    // LAND references (LTEX default swap). Kept for
                    // completeness; the LAND-side consumer path is
                    // separate from the mesh path wired for XATO. #584.
                    b"XTNM" => {
                        land_texture_ref = r.u32().ok();
                    }
                    // XTXR — per-slot texture swap. 8 bytes: TXST
                    // FormID(u32) + slot_index(u32). Lets a REFR
                    // override a single slot of the host mesh's texture
                    // set without replacing the whole TXST. Multiple
                    // XTXR sub-records allowed per REFR; we collect
                    // them in authoring order. #584.
                    b"XTXR" if sub.data.len() >= 8 => {
                        let texture_set = r.u32_or_default();
                        let slot_index = r.u32_or_default();
                        texture_slot_swaps.push(TextureSlotSwap {
                            texture_set,
                            slot_index,
                        });
                    }
                    // XEMI — per-REFR emissive LIGH FormID. 4 bytes.
                    // Used by FO4 signage / floodlights to attach a
                    // placement-specific light to the base mesh. Parsed
                    // for completeness; consumer wiring (per-REFR
                    // emissive light spawn) is follow-up work. #584.
                    b"XEMI" => {
                        emissive_light_ref = r.u32().ok();
                    }
                    // XMSP — per-REFR MSWP (Material Swap) FormID. 4 bytes.
                    // Resolves against `EsmCellIndex.material_swaps` at
                    // cell-load to substitute BGSM/BGEM material paths on
                    // the base mesh. Pre-#971 the ~2,500 vanilla FO4
                    // MSWP records sat indexed but unused because the
                    // REFR arm was missing — Raider armour colour
                    // variants, settlement clutter colours, station-
                    // wagon rust, and Vault decay overlays all rendered
                    // with the base mesh's textures. See FO4-D4-NEW-08.
                    b"XMSP" => {
                        material_swap_ref = r.u32().ok();
                    }
                    // #692 — per-REFR ownership override (mirrors the
                    // CELL walker arms). XOWN owner FormID, XRNK
                    // faction-rank gate, XGLB global-var gate. Same
                    // 4-byte layout as the CELL form. Cross-game.
                    b"XOWN" => {
                        ownership_owner = r.u32().ok();
                    }
                    b"XRNK" => {
                        ownership_rank = r.i32().ok();
                    }
                    b"XGLB" => {
                        ownership_global = r.u32().ok();
                    }
                    _ => {}
                }
            }

            if base_form_id != 0 {
                let ownership = ownership_owner.map(|owner| CellOwnership {
                    owner_form_id: owner,
                    faction_rank: ownership_rank,
                    global_var_form_id: ownership_global,
                });
                refs.push(PlacedRef {
                    // REFR's own form ID for the #1188 precombined-
                    // absorption filter. Pre-fix this was dropped on
                    // the floor; only `base_form_id` (the STAT this
                    // REFR placed) was retained.
                    form_id: header.form_id,
                    base_form_id,
                    position,
                    rotation,
                    scale,
                    enable_parent,
                    teleport,
                    primitive,
                    linked_refs,
                    rooms,
                    portals,
                    radius_override,
                    alt_texture_ref,
                    land_texture_ref,
                    texture_slot_swaps,
                    emissive_light_ref,
                    material_swap_ref,
                    ownership,
                });
            }
        } else if &header.record_type == b"LAND" {
            // Parse landscape heightmap, normals, and vertex colors.
            //
            // At least one vanilla FNV LAND record (form `0x00150FC0`)
            // reliably fails the body read on every ESM open — cause
            // not yet identified, single cell affected. Observable
            // symptom is a flat/untextured tile if that cell is ever
            // rendered. Demoted from `warn` to `debug` per the audit's
            // soft-fail guidance (#385 / D5-F5); the error context
            // rides through so anyone investigating sees the real
            // failure mode instead of a generic message.
            match parse_land_record(reader, &header) {
                Ok(land) => *landscape = Some(land),
                Err(e) => log::debug!(
                    "LAND record parse failed (form {:08X}): {e:#}",
                    header.form_id
                ),
            }
        } else if &header.record_type == b"NAVM" {
            // #1272 — NAVMs nest under cell persistent/temporary
            // children GRUPs (group_type 6 / 8); the top-level NAVM
            // dispatch in `parse_esm` is vestigial because no vanilla
            // master ships top-level NAVMs.
            let subs = reader.read_sub_records(&header)?;
            navmeshes.push(parse_navm(header.form_id, &subs));
        } else {
            // Skip other record types (PGRE, PMIS, etc.)
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Decode a LAND record's VHGT, VNML, and VCLR sub-records.
///
/// VHGT encoding (from UESP): the heightmap is delta-encoded with a
/// column-then-row accumulator scheme. See the UESP wiki "Vertex Height
/// Data" section for the canonical algorithm.
pub(crate) fn parse_land_record(
    reader: &mut EsmReader,
    header: &crate::esm::reader::RecordHeader,
) -> Result<LandscapeData> {
    let subs = reader.read_sub_records(header)?;

    let mut heights = vec![0.0f32; 33 * 33];
    let mut normals: Option<Vec<u8>> = None;
    let mut vertex_colors: Option<Vec<u8>> = None;
    let mut quadrants: [TerrainQuadrant; 4] = Default::default();
    // Track the most recently parsed ATXT header so we can attach the
    // following VTXT alpha data to it. ESM sub-records are ordered:
    // ...ATXT, VTXT, ATXT, VTXT... (each ATXT is followed by its VTXT).
    let mut pending_atxt: Option<(usize, u32, u16)> = None; // (quadrant, ltex_form_id, layer)

    for sub in &subs {
        match sub.sub_type.as_slice() {
            b"VHGT" if sub.data.len() >= 1093 => {
                // UESP VHGT algorithm: delta-encoded heightmap.
                let mut r = SubReader::new(&sub.data);
                let base_offset = r.f32_or_default();
                let mut offset = base_offset * 8.0;
                for row in 0..33usize {
                    let first_delta = r.u8_or_default() as i8 as f32 * 8.0;
                    offset += first_delta;
                    heights[row * 33] = offset;
                    let mut col_accum = offset;
                    for col in 1..33usize {
                        let d = r.u8_or_default() as i8 as f32 * 8.0;
                        col_accum += d;
                        heights[row * 33 + col] = col_accum;
                    }
                }
            }
            b"VNML" if sub.data.len() >= 3267 => {
                normals = Some(sub.data[..3267].to_vec());
            }
            b"VCLR" if sub.data.len() >= 3267 => {
                vertex_colors = Some(sub.data[..3267].to_vec());
            }
            b"BTXT" if sub.data.len() >= 8 => {
                // Base texture: formid(4) + quadrant(1) + unused(3).
                let mut r = SubReader::new(&sub.data);
                let ltex_id = r.u32_or_default();
                let quadrant = r.u8_or_default() as usize;
                if quadrant < 4 {
                    quadrants[quadrant].base = Some(ltex_id);
                }
            }
            b"ATXT" if sub.data.len() >= 8 => {
                // Additional texture header: formid(4) + quadrant(1) + unused(1) + layer(u16).
                let mut r = SubReader::new(&sub.data);
                let ltex_id = r.u32_or_default();
                let quadrant = r.u8_or_default() as usize;
                let _unused = r.u8_or_default();
                let layer = r.u16_or_default();
                if quadrant < 4 {
                    pending_atxt = Some((quadrant, ltex_id, layer));
                }
            }
            b"VTXT" => {
                // Alpha layer data for the preceding ATXT.
                // Array of 8-byte entries: position(u16) + unused(u16) + opacity(f32).
                if let Some((quadrant, ltex_id, layer)) = pending_atxt.take() {
                    let mut alpha = vec![0.0f32; 17 * 17];
                    let mut r = SubReader::new(&sub.data);
                    while r.remaining() >= 8 {
                        let pos = r.u16_or_default() as usize;
                        let _unused = r.u16_or_default();
                        let opacity = r.f32_or_default();
                        if pos < 17 * 17 {
                            alpha[pos] = opacity;
                        }
                    }
                    quadrants[quadrant].layers.push(TerrainTextureLayer {
                        ltex_form_id: ltex_id,
                        layer,
                        alpha: Some(alpha),
                    });
                }
            }
            _ => {}
        }
    }

    // Sort additional layers by layer index within each quadrant.
    for q in &mut quadrants {
        q.layers.sort_by_key(|l| l.layer);
    }

    Ok(LandscapeData {
        heights,
        normals,
        vertex_colors,
        quadrants,
    })
}
