//! Internal sub-record decode helpers — used by every walker in this module.
//!
//! Extracted from mod.rs in stage B of the cell-monolith refactor.
//! `pub(super)` so the walker siblings can call them; not part of the
//! public `esm::cell` API.

/// Read a null-terminated string from sub-record data.
pub(super) fn read_zstring(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).to_string()
}

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
