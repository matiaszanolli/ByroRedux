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

/// Read a 4-byte FormID from a sub-record payload. Returns `None` when
/// the payload is too short to hold a u32 — defensive against truncated
/// records the walker would otherwise pass through. Used by the
/// Skyrim-extended CELL sub-record arms (XCIM / XCWT / XCAS / XCMO /
/// XLCN — see #356).
pub(super) fn read_form_id(data: &[u8]) -> Option<u32> {
    (data.len() >= 4).then(|| u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// Read an array of 4-byte FormIDs packed back-to-back. Used for XCLR
/// (region list) and any other list-of-FormIDs sub-record. Trailing
/// bytes that don't make a full FormID are silently dropped — they're
/// always alignment padding rather than a partial entry.
pub(super) fn read_form_id_array(data: &[u8]) -> Vec<u32> {
    data.chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Parse an `XCLW` water-plane height (f32, Z-up world units). Returns
/// `None` for the Bethesda "no water" sentinel `#INT_MIN#`
/// (-2147483648.0, nif.xml line 59), which marks a cell that explicitly
/// has no water surface. Without this, such a cell spawns a water plane
/// ~2.1e9 units below everything — a wasted BLAS entry + RT-reflection
/// cost (~170 vanilla Oblivion cells). Also `None` when the payload is
/// too short. Same XCLW layout across Oblivion / FO3 / FNV / Skyrim, so
/// shared by both the interior and exterior walkers. #1305 / OBL-D6-NEW-02.
pub(super) fn xclw_water_height(data: &[u8]) -> Option<f32> {
    if data.len() < 4 {
        return None;
    }
    let h = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    // The sentinel is `#INT_MIN#` (-2.147e9). Use a magnitude threshold
    // rather than exact float-equality: lint-clean and robust to any
    // writer emitting a near-INT_MIN value, and no real Oblivion water
    // plane sits anywhere near -1e9 (vanilla XCLW spans roughly
    // -4000..7000). Non-finite values are likewise treated as absent.
    if h.is_finite() && h > -1.0e9 {
        Some(h)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::xclw_water_height;

    #[test]
    fn xclw_normal_height_passes_through() {
        assert_eq!(
            xclw_water_height(&(-2000.0f32).to_le_bytes()),
            Some(-2000.0)
        );
        assert_eq!(xclw_water_height(&3450.0f32.to_le_bytes()), Some(3450.0));
        assert_eq!(xclw_water_height(&0.0f32.to_le_bytes()), Some(0.0));
    }

    #[test]
    fn xclw_int_min_sentinel_is_no_water() {
        // The #INT_MIN# "no water" marker — must NOT spawn a water plane.
        assert_eq!(
            xclw_water_height(&(-2_147_483_648.0f32).to_le_bytes()),
            None
        );
    }

    #[test]
    fn xclw_short_or_nonfinite_is_none() {
        assert_eq!(xclw_water_height(&[0u8; 3]), None);
        assert_eq!(xclw_water_height(&f32::NAN.to_le_bytes()), None);
    }
}
