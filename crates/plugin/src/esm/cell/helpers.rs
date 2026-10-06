//! Internal sub-record decode helpers — used by every walker in this module.
//!
//! Extracted from mod.rs in stage B of the cell-monolith refactor.
//! `pub(super)` so the walker siblings can call them; not part of the
//! public `esm::cell` API.

/// Read a null-terminated string from sub-record data.
///
/// Re-exported from [`crate::esm::records::common`] rather than reimplemented
/// — the two were byte-identical copies (#1318 / TD3-NEW-A). Callers keep
/// using `super::helpers::read_zstring`; the single definition now lives in
/// `records::common` alongside the localized-lstring `read_lstring` variant.
pub(super) use crate::esm::records::common::{read_mesh_path, read_zstring};

use super::CellOwnership;
use crate::esm::reader::EsmReader;
use crate::esm::records::common::read_lstring_or_zstring;
use crate::esm::sub_reader::SubReader;

/// The CELL sub-record accumulator shared by the interior walker
/// (`walkers.rs::parse_cell_group_inner`) and the exterior walker
/// (`wrld.rs::parse_wrld_children_inner`). TD2-2026-10-05-01 / #5309 —
/// the two walkers used to declare these ~20 locals and decode the same
/// ~18 sub-record arms each, spelled slightly differently, and the
/// duplication already produced a one-sided fix (#1220: the exterior
/// walker hardcoded empty XCRI/XPRI after the interior one gained them;
/// LEGACY_COMPAT 2026-05-19 found the same shape). New CELL sub-records
/// (Starfield, FO76, mods) now need exactly one edit: an arm in
/// [`CellSubrecordFields::absorb`].
///
/// Each walker keeps only its own arms (interior: `DATA` + `XCLL`;
/// exterior: `XCLC`) and falls through to [`CellSubrecordFields::absorb`]
/// for everything else.
#[derive(Default)]
pub(super) struct CellSubrecordFields {
    pub(super) editor_id: String,
    pub(super) display_name: Option<String>,
    pub(super) water_height: Option<f32>,
    pub(super) water_height_is_explicit: bool,
    pub(super) image_space_form: Option<u32>,
    pub(super) water_type_form: Option<u32>,
    pub(super) acoustic_space_form: Option<u32>,
    pub(super) music_type_form: Option<u32>,
    /// #693 / O3-N-05 — pre-Skyrim XCMT (1-byte enum) and Skyrim XCCM
    /// (4-byte CLMT FormID). Both fell to the catch-all `_` arm pre-fix.
    pub(super) music_type_enum: Option<u8>,
    pub(super) climate_override: Option<u32>,
    pub(super) location_form: Option<u32>,
    /// #4173 — XEZN encounter-zone FormID (references an ECZN record;
    /// spawn scaling / faction ownership per cell).
    pub(super) encounter_zone_form: Option<u32>,
    pub(super) regions: Vec<u32>,
    /// SK-D6-02 / #566 — LTMP lighting-template FormID. Cells that omit
    /// XCLL fall back to this LGTM reference.
    pub(super) lighting_template_form: Option<u32>,
    /// #692 — XOWN / XRNK / XGLB ownership tuple. All three sub-records
    /// optional; the cell ends up with `Some` only when at least XOWN is
    /// present. XRNK and XGLB without XOWN are nonsensical and dropped
    /// (the consumer would have nothing to gate against).
    pub(super) ownership_owner: Option<u32>,
    pub(super) ownership_rank: Option<i32>,
    pub(super) ownership_global: Option<u32>,
    /// #970 / OBL-D3-NEW-06 — Oblivion-era cell-level RGB tint override
    /// (`RCLR`, 3 bytes). Rare even on Oblivion (editor-authored), absent
    /// on FO3+ vanilla. Parsed cross-game; the field is harmless when
    /// None and lets modded post-Oblivion cells still surface the
    /// override.
    pub(super) regional_color_override: Option<[u8; 3]>,
    /// #1188 / #1220 — FO4+ PreCombined Mesh references. XCRI holds
    /// (u32 mesh_count + u32 ref_count + N×u32 hashes + M×u32
    /// absorbed-refr formids). XPRI holds the additional list of refr
    /// formids absorbed by the precombines. Empirically decoded against
    /// vanilla `DmndDugoutInn01` (form 0x00001E5D, 39 hashes / 962 XCRI
    /// refs / 102 XPRI refs) — see the audit memory.
    pub(super) precombined_mesh_hashes: Vec<u32>,
    pub(super) absorbed_refs: std::collections::HashSet<u32>,
}

impl CellSubrecordFields {
    /// Absorb one walker-shared CELL sub-record. Returns `true` when
    /// `sub` was consumed — the callers match their walker-specific arms
    /// first (`DATA`/`XCLL` interior, `XCLC` exterior) and fall through
    /// here for everything else. `cell_form_id` names the owning CELL in
    /// the XCRI size-mismatch warning.
    pub(super) fn absorb(
        &mut self,
        reader: &EsmReader,
        cell_form_id: u32,
        sub: &crate::esm::reader::SubRecord,
    ) -> bool {
        match &sub.sub_type {
            b"EDID" => self.editor_id = read_zstring(&sub.data),
            // #624 / SK-D6-NEW-02 — cells DO ship FULL (e.g.
            // WhiterunBanneredMare's FULL = "The Bannered Mare",
            // SolitudeWorld tiles). The lstring helper auto-routes the
            // 4-byte STRINGS-table case for localized plugins.
            b"FULL" => self.display_name = Some(read_lstring_or_zstring(&sub.data)),
            // XCLW: f32 water plane height in world units (Z-up). Same
            // layout across Oblivion / FO3 / FNV / Skyrim — the cell's
            // water surface sits at this Z (interior) or Z-in-worldspace
            // (exterior). `gated_water_height` returns None for the
            // `#INT_MIN#` / FLT_MAX "no water" sentinels;
            // `water_height_is_explicit` keeps that None distinct from an
            // absent XCLW. See #397 / #356 / #1305.
            b"XCLW" => {
                self.water_height_is_explicit = true;
                self.water_height = gated_water_height(&sub.data);
            }
            // Skyrim extended CELL sub-records (#356). Each is a 4-byte
            // FormID; the walkers previously dropped them on the `_` arm
            // so the renderer / audio / quest system had no per-cell
            // context.
            b"XCIM" => self.image_space_form = read_form_id(reader, &sub.data),
            b"XCWT" => self.water_type_form = read_form_id(reader, &sub.data),
            b"XCAS" => self.acoustic_space_form = read_form_id(reader, &sub.data),
            b"XCMO" => self.music_type_form = read_form_id(reader, &sub.data),
            // LTMP — lighting-template FormID (SK-D6-02 / #566).
            b"LTMP" => self.lighting_template_form = read_form_id(reader, &sub.data),
            // #1188 / #1220 — XCRI: FO4+ PreCombined Mesh references.
            //   `u32 mesh_count + u32 ref_count
            //    + mesh_count × u32 hashes
            //    + ref_count × u32 visibility-group refs`
            // For each hash, the precombined NIF file lives at
            // `meshes\precombined\<cell_fid:08x>_<hash:08x>_oc.nif`.
            //
            // The `ref_count`-sized tail is the **visibility group** for
            // the precombines — refs participating in the combined-cull
            // bake. It is NOT "refs to skip individual spawn" (the Dmnd
            // Dugout Inn first iteration regressed the bar / couch /
            // lamps because we treated these as absorbed). Skip-placement
            // is XPRI's job, below.
            b"XCRI" if sub.data.len() >= 8 => {
                let mesh_count =
                    u32::from_le_bytes(sub.data[0..4].try_into().unwrap()) as usize;
                let ref_count =
                    u32::from_le_bytes(sub.data[4..8].try_into().unwrap()) as usize;
                let expected = 8 + mesh_count.saturating_mul(4) + ref_count.saturating_mul(4);
                if expected != sub.data.len() {
                    log::warn!(
                        "CELL {:08X} XCRI size mismatch: hdr={}+{} expected_payload={} \
                         actual={} — skipping",
                        cell_form_id,
                        mesh_count,
                        ref_count,
                        expected,
                        sub.data.len(),
                    );
                } else {
                    self.precombined_mesh_hashes.reserve(mesh_count);
                    let mut off = 8;
                    for _ in 0..mesh_count {
                        let h =
                            u32::from_le_bytes(sub.data[off..off + 4].try_into().unwrap());
                        self.precombined_mesh_hashes.push(h);
                        off += 4;
                    }
                    // We intentionally do NOT consume the ref_count tail
                    // into `absorbed_refs`. See XPRI below for the
                    // skip-placement source of truth.
                }
            }
            // #1188 / #1220 — XPRI: list of REFR formids absorbed into
            // precombines (~100 entries for FO4 interiors; matches the
            // architecture-only shell). The cell loader MUST skip these
            // REFRs' individual placement — their geometry is already
            // baked into the `_oc.nif` files referenced by
            // `precombined_mesh_hashes`. Format: pure `N × u32`.
            b"XPRI" if sub.data.len().is_multiple_of(4) => {
                self.absorbed_refs.reserve(sub.data.len() / 4);
                for chunk in sub.data.as_chunks::<4>().0 {
                    let fid = u32::from_le_bytes(*chunk);
                    self.absorbed_refs.insert(reader.remap_form_id(fid));
                }
            }
            // #693 / O3-N-05 — XCMT pre-Skyrim music enum (Oblivion /
            // FO3 / FNV). 1-byte payload. Rare on exterior cells (most
            // use the worldspace default music) but pinned for
            // completeness.
            b"XCMT" if !sub.data.is_empty() => self.music_type_enum = Some(sub.data[0]),
            // #693 / O3-N-05 — XCCM Skyrim climate override (per-cell
            // CLMT FormID, exterior cells in vanilla — boss arenas,
            // scripted-weather pockets — but a few interior mods have
            // been seen with it for "outside through window" effects).
            b"XCCM" => self.climate_override = read_form_id(reader, &sub.data),
            b"XLCN" => self.location_form = read_form_id(reader, &sub.data),
            // #4173 — XEZN encounter zone (ECZN FormID), the CELL-side
            // half of the spawn-scaling pair; the ECZN records themselves
            // were already parsed.
            b"XEZN" => self.encounter_zone_form = read_form_id(reader, &sub.data),
            // XCLR is a packed FormID array — region tags referenced by
            // REGN records. Variable length; empty list is normal.
            b"XCLR" => self.regions = read_form_id_array(reader, &sub.data),
            // #692 — XOWN owner, XRNK faction-rank gate, XGLB
            // global-variable FormID. Same shape on CELL + REFR.
            // Cross-game (Oblivion / FO3 / FNV / Skyrim+). `read_form_id`
            // declines a truncated payload, so no separate length guard.
            b"XOWN" => self.ownership_owner = read_form_id(reader, &sub.data),
            b"XRNK" => self.ownership_rank = SubReader::new(&sub.data).i32().ok(),
            b"XGLB" => self.ownership_global = read_form_id(reader, &sub.data),
            // #970 / OBL-D3-NEW-06 — Oblivion CELL regional tint.
            // On disk it's `RCLR` with 3 RGB bytes (no alpha). Some
            // plugins ship a 4-byte payload with a trailing pad — accept
            // >= 3 and read the first three bytes only.
            b"RCLR" if sub.data.len() >= 3 => {
                self.regional_color_override = Some([sub.data[0], sub.data[1], sub.data[2]]);
            }
            _ => return false,
        }
        true
    }

    /// The ownership tuple: `Some` only when XOWN was present.
    pub(super) fn ownership(&self) -> Option<CellOwnership> {
        self.ownership_owner.map(|owner| CellOwnership {
            owner_form_id: owner,
            faction_rank: self.ownership_rank,
            global_var_form_id: self.ownership_global,
        })
    }
}

/// Read a 4-byte FormID from a sub-record payload. Returns `None` when
/// the payload is too short to hold a u32 — defensive against truncated
/// records the walker would otherwise pass through. Used by the
/// Skyrim-extended CELL sub-record arms (XCIM / XCWT / XCAS / XCMO /
/// XLCN — see #356).
/// #3314 / FNV-2026-08-26-D1-01 — the reader is a *required* parameter, not a
/// convenience: every `EsmIndex` map is keyed in global load-order space
/// (`EsmReader::read_record_header` remaps every record FormID), so a
/// sub-record reference read raw misses every lookup the moment the remap is
/// non-identity. Taking `&EsmReader` here makes that impossible to forget —
/// the old bare-`&[u8]` signature let 26 cell/worldspace/landscape call sites
/// silently bypass the convention the REFR walker beside them follows.
pub(super) fn read_form_id(reader: &EsmReader, data: &[u8]) -> Option<u32> {
    (data.len() >= 4)
        .then(|| u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
        .map(|raw| reader.remap_form_id(raw))
}

/// Read an array of 4-byte FormIDs packed back-to-back. Used for XCLR
/// (region list) and any other list-of-FormIDs sub-record. Trailing
/// bytes that don't make a full FormID are silently dropped — they're
/// always alignment padding rather than a partial entry.
pub(super) fn read_form_id_array(reader: &EsmReader, data: &[u8]) -> Vec<u32> {
    data.as_chunks::<4>().0
        .iter()
        .map(|c| reader.remap_form_id(u32::from_le_bytes([c[0], c[1], c[2], c[3]])))
        .collect()
}

/// Gate a wire f32 water-plane height (Z-up world units). Shared by the
/// three sub-records that author one: CELL `XCLW` (cell level), WRLD
/// `DNAM` (worldspace-default, second f32 of the payload), and WRLD
/// `NAM4` (distant-LOD ring). Returns `None` for Bethesda's "no water"
/// sentinels: `#INT_MIN#` (-2147483648.0, nif.xml line 59) and
/// `f32::MAX` (observed in Skyrim exterior CELL records). Without this,
/// sentinel — or corrupt — cells spawn planes at impossible heights,
/// poisoning water bounds and render work; a NaN that slips through
/// culls every triangle of the plane (NaN `<=` is always false) and
/// silently drains the worldspace (#4487). Also `None` when the payload
/// is too short. Same f32 layout across Oblivion / FO3 / FNV / Skyrim+,
/// so shared by the interior walker, the exterior walker, and the WRLD
/// walker. #1305 / OBL-D6-NEW-02.
pub(super) fn gated_water_height(data: &[u8]) -> Option<f32> {
    if data.len() < 4 {
        return None;
    }
    let h = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    // Use a symmetric magnitude threshold rather than exact sentinel
    // equality: it covers both legacy #INT_MIN# and Skyrim's FLT_MAX,
    // is robust to near-sentinel writers, and no authored water plane
    // sits anywhere near +/-1e9 world units. Non-finite values are also
    // treated as absent.
    if h.is_finite() && h.abs() < 1.0e9 {
        Some(h)
    } else {
        None
    }
}

/// Linear velocity from a REFR `XWCU` water-current array (Gamebryo Z-up).
///
/// `XWCU` is an array of 16-byte entries whose length is announced by the
/// preceding `XWCN` count — not a single `vec3`. Censused over the installed
/// masters (2026-09-14, `examples/water_current_census.rs`): every one of
/// Skyrim.esm's 128 REFR payloads is `XWCN = 3` + 48 bytes, and entry 0's
/// first three floats equal the placed water activator's WATR `NAM0` linear
/// velocity (`Water1024RiverFlowNE` → `2.54, 1.35, 0`), with entries 1–2
/// zero. Fallout 4 carries the same shape on 171 REFRs; FO3/FNV/Oblivion
/// author none. Only entry 0 has verified semantics, so only it is read.
///
/// Returns `None` for a payload that is not a whole number of entries or
/// whose velocity is non-finite.
pub(super) fn xwcu_linear_velocity(data: &[u8]) -> Option<[f32; 3]> {
    if data.len() < 16 || !data.len().is_multiple_of(16) {
        return None;
    }
    let float = |offset: usize| f32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    let velocity = [float(0), float(4), float(8)];
    velocity
        .iter()
        .all(|value| value.is_finite())
        .then_some(velocity)
}

/// Decode a placement's `XRGD` ragdoll pose (#5015): whole 28-byte
/// `wbRagdoll` entries, in file order. A trailing partial entry is dropped
/// (vanilla lengths are all multiples of 28); see [`super::RagdollPoseBone`]
/// for what the fields do and do not mean.
pub(super) fn decode_ragdoll_pose(data: &[u8]) -> Vec<super::RagdollPoseBone> {
    const ENTRY: usize = 28;
    if !data.len().is_multiple_of(ENTRY) {
        log::debug!(
            "XRGD length {} is not a multiple of {ENTRY}; trailing bytes ignored",
            data.len()
        );
    }
    data.as_chunks::<ENTRY>().0
        .iter()
        .map(|entry| {
            let float =
                |offset: usize| f32::from_le_bytes(entry[offset..offset + 4].try_into().unwrap());
            super::RagdollPoseBone {
                bone_id: entry[0],
                position: [float(4), float(8), float(12)],
                rotation: [float(16), float(20), float(24)],
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::gated_water_height;

    #[test]
    fn gated_water_height_normal_height_passes_through() {
        assert_eq!(
            gated_water_height(&(-2000.0f32).to_le_bytes()),
            Some(-2000.0)
        );
        assert_eq!(gated_water_height(&3450.0f32.to_le_bytes()), Some(3450.0));
        assert_eq!(gated_water_height(&0.0f32.to_le_bytes()), Some(0.0));
    }

    #[test]
    fn gated_water_height_int_min_sentinel_is_no_water() {
        // The #INT_MIN# "no water" marker — must NOT spawn a water plane.
        assert_eq!(
            gated_water_height(&(-2_147_483_648.0f32).to_le_bytes()),
            None
        );
    }

    #[test]
    fn gated_water_height_float_max_sentinel_is_no_water() {
        // Skyrim exterior CELLs use FLT_MAX for an explicitly dry tile.
        assert_eq!(gated_water_height(&f32::MAX.to_le_bytes()), None);
    }

    #[test]
    fn gated_water_height_short_or_nonfinite_is_none() {
        assert_eq!(gated_water_height(&[0u8; 3]), None);
        assert_eq!(gated_water_height(&f32::NAN.to_le_bytes()), None);
        // #4487 — +Inf must not become a canonical height either.
        assert_eq!(gated_water_height(&f32::INFINITY.to_le_bytes()), None);
    }
}
