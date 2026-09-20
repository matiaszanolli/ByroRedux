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

use crate::esm::reader::EsmReader;

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
    data.chunks_exact(4)
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
