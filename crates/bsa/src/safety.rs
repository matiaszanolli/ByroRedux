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
use std::io::Read;

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

/// Inflate a compressed stream with a hard output ceiling.
///
/// Every decompressor in this crate reads a declared uncompressed size out of
/// the archive, validates it through [`checked_chunk_size`], and then needs to
/// actually *hold the decoder to it*. `Read::read_to_end` has no output limit:
/// it grows the buffer until the decoder reaches end-of-stream, so the
/// validated ceiling was only ever a `Vec::with_capacity` hint. A crafted or
/// corrupt archive — the ordinary distribution format for mods, i.e. this
/// crate's real untrusted-input surface — could therefore inflate far past the
/// declared size and terminate the process on allocation failure, which is not
/// an `Err` any caller can handle and which no `catch_unwind` can intercept.
/// LZ4 blocks top out near 255:1 and DEFLATE near 1000:1 against a payload the
/// 30-bit BSA size field already bounds at 1 GB.
///
/// This reads at most `declared + 1` bytes. Landing on that extra byte proves
/// the stream had more to give, which is rejected as `InvalidData` rather than
/// silently truncated. A *short* decode is deliberately still `Ok` — several
/// shipped archives carry known padding deltas where the payload reads a
/// handful of bytes under its declared size (#622 / #812), and callers log
/// that themselves.
///
/// See #3410 (SKY-2026-08-27b-D5-01).
pub fn inflate_bounded<R: io::Read>(
    reader: R,
    declared: usize,
    label: &str,
) -> io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(declared);
    // `declared + 1`: reading the extra byte is how an over-run is detected.
    // `as u64 + 1` cannot overflow — `declared` is `usize`-bounded well below
    // `u64::MAX` by the `checked_chunk_size` call every caller makes first.
    reader.take(declared as u64 + 1).read_to_end(&mut buf)?;
    if buf.len() > declared {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{label} inflated past its declared uncompressed size \
                 {declared} — archive is corrupt or hostile (decompression bomb)"
            ),
        ));
    }
    Ok(buf)
}

/// Upper bound on the summed byte total across every chunk of a single
/// multi-chunk record (e.g. one BA2 DX10 texture's per-mip chunk list).
/// Each chunk's own size is already capped at [`MAX_CHUNK_BYTES`], but a
/// DX10 record's `num_chunks` is a `u8` (up to 255 chunks), and nothing
/// capped the *sum* — a corrupted/hostile archive can declare up to 255
/// near-`MAX_CHUNK_BYTES` chunks per texture, driving up to ~255x the
/// single-chunk cap in eager allocation attempts before a single pixel
/// byte is read. Real vanilla DX10 textures (BC7 8K cubemaps with a
/// full mip chain included) stay well under 2 GiB fully decompressed
/// across every mip and cube face combined; this leaves generous
/// headroom for legitimate content while bounding the amplification
/// factor a per-chunk cap alone can't catch. See #2356 (SF-BA2-01).
pub const MAX_RECORD_TOTAL_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// Validate a running sum of chunk byte sizes within a single record
/// against [`MAX_RECORD_TOTAL_BYTES`]. Each individual `added` size must
/// already have passed [`checked_chunk_size`] / [`checked_chunk_size_usize`];
/// this closes the aggregate-across-chunks gap those per-chunk checks
/// can't see. See #2356 (SF-BA2-01).
pub fn checked_chunk_total(running_total: usize, added: usize, label: &str) -> io::Result<usize> {
    let total = running_total.saturating_add(added);
    if total > MAX_RECORD_TOTAL_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{label} cumulative size {total} across the record's chunk \
                 list exceeds safety cap {MAX_RECORD_TOTAL_BYTES} \
                 — archive is corrupt or hostile"
            ),
        ));
    }
    Ok(total)
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

    // #2356 (SF-BA2-01) — a single record's chunk sizes each pass
    // `checked_chunk_size` individually, but nothing capped the sum
    // across the chunk list. `num_chunks` is a u8 (up to 255), so a
    // hostile archive could declare 255 near-`MAX_CHUNK_BYTES` chunks
    // per texture record, driving up to ~255x the single-chunk cap in
    // allocation attempts before a single pixel byte is read.

    #[test]
    fn chunk_total_accepts_vanilla_multi_mip_texture() {
        // A real multi-mip DX10 texture: several chunks summing well
        // under the cap must accumulate cleanly.
        let mut total = 0usize;
        for _ in 0..12 {
            total = checked_chunk_total(total, 8 * 1024 * 1024, "unpacked_size").unwrap();
        }
        assert_eq!(total, 12 * 8 * 1024 * 1024);
    }

    #[test]
    fn chunk_total_rejects_255_near_cap_chunks() {
        // The exact attack shape from #2356: 255 chunks (the u8 max),
        // each individually valid under MAX_CHUNK_BYTES, must still be
        // rejected once their sum crosses MAX_RECORD_TOTAL_BYTES —
        // the per-chunk cap alone can't see this.
        let per_chunk = MAX_CHUNK_BYTES; // each chunk passes checked_chunk_size on its own
        let mut total = 0usize;
        let mut rejected = false;
        for _ in 0..255u32 {
            match checked_chunk_total(total, per_chunk, "unpacked_size") {
                Ok(t) => total = t,
                Err(err) => {
                    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
                    rejected = true;
                    break;
                }
            }
        }
        assert!(
            rejected,
            "255 near-1 GiB chunks must trip the aggregate cap before all are accepted"
        );
    }

    #[test]
    fn chunk_total_accepts_exactly_the_cap_boundary() {
        assert_eq!(
            checked_chunk_total(0, MAX_RECORD_TOTAL_BYTES, "unpacked_size").unwrap(),
            MAX_RECORD_TOTAL_BYTES
        );
    }

    #[test]
    fn chunk_total_rejects_cap_plus_one() {
        let err = checked_chunk_total(0, MAX_RECORD_TOTAL_BYTES + 1, "unpacked_size").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn chunk_total_never_overflows_on_saturating_add() {
        // Two near-usize::MAX additions must saturate, not panic/wrap.
        let err = checked_chunk_total(usize::MAX - 10, 100, "unpacked_size").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}

#[cfg(test)]
mod inflate_bounded_tests {
    use super::inflate_bounded;
    use flate2::read::ZlibDecoder;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(data).expect("encode");
        e.finish().expect("finish")
    }

    /// #3410 — the decompression bomb. A payload that inflates far past the
    /// size the archive declared must be rejected AT the ceiling, not
    /// inflated first and noticed afterwards. 4 MiB of zeros compresses to a
    /// few KiB, so a 1 GB-bounded compressed field can carry orders of
    /// magnitude more than this.
    #[test]
    fn over_ratio_payload_is_rejected_at_the_ceiling() {
        let bomb = zlib(&vec![0u8; 4 * 1024 * 1024]);
        let err = inflate_bounded(ZlibDecoder::new(&bomb[..]), 128, "test")
            .expect_err("a stream that inflates past the declared size must Err");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        let msg = format!("{err}");
        assert!(
            msg.contains("inflated past its declared uncompressed size"),
            "the error must name the real cause, got: {msg}"
        );
    }

    /// A lying size prefix in the other direction — the archive declares MORE
    /// than the stream holds — stays `Ok`. Several shipped archives carry
    /// known padding deltas (#622 / #812) and callers warn on the mismatch
    /// themselves; turning that into an error would break parse-rate on
    /// borderline vanilla content.
    #[test]
    fn short_decode_stays_ok_for_the_shipped_padding_deltas() {
        let data = b"twenty-eight bytes of body!!";
        let out = inflate_bounded(ZlibDecoder::new(&zlib(data)[..]), 4096, "test")
            .expect("short decode is Ok");
        assert_eq!(out, data, "the short payload must come back byte-exact");
    }

    /// The exact-fit case must not be mistaken for an over-run: the helper
    /// reads `declared + 1` bytes to detect the over-run, so an
    /// exactly-`declared` stream lands one byte under the read limit.
    #[test]
    fn exact_size_round_trips() {
        let data: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
        let out = inflate_bounded(ZlibDecoder::new(&zlib(&data)[..]), data.len(), "test")
            .expect("exact fit is Ok");
        assert_eq!(out, data);
    }

    /// One byte over is still over. Pins the boundary rather than only the
    /// dramatic case above.
    #[test]
    fn one_byte_over_is_rejected() {
        let data = vec![7u8; 1024];
        assert!(inflate_bounded(ZlibDecoder::new(&zlib(&data)[..]), 1023, "test").is_err());
        assert!(inflate_bounded(ZlibDecoder::new(&zlib(&data)[..]), 1024, "test").is_ok());
    }
}
