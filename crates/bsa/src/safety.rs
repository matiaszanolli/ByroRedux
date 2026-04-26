//! Allocation-safety helpers for the BSA / BA2 readers.
//!
//! Archive headers expose three classes of attacker-controlled sizes:
//!
//! - **Entry counts** (`file_count`, `folder_count`) → cap
//!   [`MAX_ENTRY_COUNT`] before `Vec::with_capacity` / `HashMap::with_capacity`.
//! - **Compressed / decompressed payload sizes** (`packed_size`,
//!   `unpacked_size`, `original_size`) → cap [`MAX_CHUNK_BYTES`] before
//!   `vec![0u8; n]` / `Vec::with_capacity` into a decompressor.
//! - **Null-terminated name lengths** → already bounded by the archive
//!   format (u8 folder-name, u16 BA2 file-name) to 255 / 65 535 bytes.
//!   No helper needed.
//!
//! The BA2 reader (#586 / FO4-DIM2-01) and the BSA reader are the
//! siblings covered here. The companion NIF sweep landed as closed
//! #388 (`NifStream::allocate_vec` + `check_alloc`).

use std::io;

/// Upper bound on the number of file / folder entries any archive may
/// declare. Vanilla Bethesda archives top out around 600 000 entries
/// in `MeshesExtra.ba2` (Creation Club / Next Gen); 10 M is a paranoid
/// safety margin that still rejects the 4 294 967 295-entry attack
/// from a single corrupted `u32`.
pub const MAX_ENTRY_COUNT: usize = 10_000_000;

/// Upper bound on a single archive chunk's raw / decompressed byte
/// size. Vanilla content tops out around 325 MB on Fallout 76's
/// `SeventySix - Meshes.ba2` (single packed mesh entry); 1 GB gives
/// ~3× headroom against future vanilla growth while still rejecting
/// the u32::MAX attack from a single corrupted size field. Sibling
/// `byroredux_nif::stream::MAX_SINGLE_ALLOC_BYTES` stays at 256 MB
/// because a single block-internal allocation has tighter realistic
/// bounds (the fattest in-block buffer across the 7 supported games
/// is ~12 MB on an FO76 actor NIF).
pub const MAX_CHUNK_BYTES: usize = 1024 * 1024 * 1024;

/// Validate an archive-header entry count before allocating a container
/// sized by it. Rejects any value exceeding [`MAX_ENTRY_COUNT`] with a
/// short `InvalidData` error carrying the `label` so the log line
/// points at the offending field. `u32` in the signature matches the
/// archive wire format — BSA/BA2 never author a u64 count.
pub fn checked_entry_count(count: u32, label: &str) -> io::Result<usize> {
    let n = count as usize;
    if n > MAX_ENTRY_COUNT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{label} count {n} exceeds safety cap {MAX_ENTRY_COUNT} \
                 — archive is corrupt or hostile"
            ),
        ));
    }
    Ok(n)
}

/// Validate a payload size read from archive headers before allocating
/// a buffer for it. Rejects any value exceeding [`MAX_CHUNK_BYTES`].
/// Same failure shape as [`checked_entry_count`] so operators can
/// eyeball the log and tell allocation errors apart from parse errors.
pub fn checked_chunk_size(size: u32, label: &str) -> io::Result<usize> {
    let n = size as usize;
    if n > MAX_CHUNK_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{label} size {n} exceeds safety cap {MAX_CHUNK_BYTES} \
                 — archive is corrupt or hostile"
            ),
        ));
    }
    Ok(n)
}

/// `usize` variant of [`checked_chunk_size`] for call sites that have
/// already widened the field (common when a path computes a derived
/// size via `checked_sub` / `checked_mul`). Semantics are identical.
pub fn checked_chunk_size_usize(size: usize, label: &str) -> io::Result<usize> {
    if size > MAX_CHUNK_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{label} size {size} exceeds safety cap {MAX_CHUNK_BYTES} \
                 — archive is corrupt or hostile"
            ),
        ));
    }
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_count_accepts_vanilla_bounds() {
        // MeshesExtra.ba2 ships ~600k entries; 10 M cap must accept it.
        assert_eq!(checked_entry_count(600_000, "file_count").unwrap(), 600_000);
        assert_eq!(checked_entry_count(0, "file_count").unwrap(), 0);
        // Cap itself must pass (boundary).
        assert_eq!(
            checked_entry_count(MAX_ENTRY_COUNT as u32, "file_count").unwrap(),
            MAX_ENTRY_COUNT
        );
    }

    #[test]
    fn entry_count_rejects_attacker_u32_max() {
        let err = checked_entry_count(u32::MAX, "file_count").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        // Message should name the field + carry the overflowing number
        // so the operator log points at the cause instead of guessing.
        let msg = format!("{err}");
        assert!(msg.contains("file_count"), "got: {msg}");
        assert!(msg.contains(&u32::MAX.to_string()), "got: {msg}");
    }

    #[test]
    fn entry_count_rejects_10m_plus_one() {
        let err = checked_entry_count((MAX_ENTRY_COUNT + 1) as u32, "file_count").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn chunk_size_accepts_vanilla_bounds() {
        // FO76 ships genuine 325 MB packed mesh entries; the cap must
        // clear that with margin while still rejecting u32::MAX.
        assert_eq!(
            checked_chunk_size(325 * 1024 * 1024, "packed_size").unwrap(),
            325 * 1024 * 1024
        );
        // 1 GB boundary must pass.
        assert_eq!(
            checked_chunk_size(MAX_CHUNK_BYTES as u32, "packed_size").unwrap(),
            MAX_CHUNK_BYTES
        );
    }

    #[test]
    fn chunk_size_rejects_attacker_u32_max() {
        let err = checked_chunk_size(u32::MAX, "unpacked_size").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn chunk_size_usize_matches_u32_semantics() {
        assert_eq!(checked_chunk_size_usize(1024, "packed_size").unwrap(), 1024);
        assert!(checked_chunk_size_usize(MAX_CHUNK_BYTES + 1, "packed_size").is_err());
    }
}
