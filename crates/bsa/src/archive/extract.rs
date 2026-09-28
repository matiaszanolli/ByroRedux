//! Per-file extraction from an opened BSA archive.
//!
//! Walks the embed-name prefix (when set), dispatches to the version-
//! appropriate decompressor (zlib for v103/v104, LZ4 frame for v105),
//! and returns the decompressed bytes. Same code path serves all three
//! versions — the version branch is the codec dispatch only.

use super::{normalize_path, BsaArchive, BSA_V_SKYRIM_SE};
use crate::read_at::ReadAt;
use crate::safety::{checked_chunk_size, checked_chunk_size_usize};
use std::io;

/// Where a file's stored body starts and how it is stored, once the
/// embedded-name prefix (if any) has been stepped over.
struct StoredBody {
    /// Absolute offset of the body (the original-size header, for a
    /// compressed file).
    pos: u64,
    /// Body length on disk, name prefix excluded.
    data_size: usize,
    compressed: bool,
}

impl BsaArchive {
    /// Locate `path`'s stored body. Shared by [`Self::extract`] and
    /// [`Self::declared_size`] so both step over the name prefix and apply
    /// the #352 size guard identically.
    fn stored_body(&self, path: &str) -> io::Result<StoredBody> {
        let key = normalize_path(path);
        let entry = self.files.get(&key).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("file not found in BSA: {}", path),
            )
        })?;

        // Positional reads against the long-lived handle stored at open
        // time. Pre-#360 every extract reopened the archive (one `open()`
        // syscall per file, hundreds per cell load); after #360 a
        // `Mutex<File>` serialised the seek/read pair. `read_exact_at`
        // carries its own offset, so concurrent extracts share the handle
        // without a lock (and without the #1170 poison-recovery path).
        let file = &self.file;
        let mut pos = entry.offset;

        // Skip embedded file name prefix (bstring: 1 byte length + name).
        // Driven by the archive-level 0x100 flag alone, matching openmw
        // (`compressedbsafile.cpp:271`).
        //
        // #3367 — this used to XOR in a per-file "override" from bit 31 of the
        // size word, mirroring the compression toggle below. Unlike the
        // compression bit (0x40000000, which openmw declares and which is set
        // on 5,221 real Oblivion files), bit 31 has no source assigning it that
        // meaning and is set on zero files across every installed vanilla
        // archive. Guessing wrong here consumes a bstring prefix that isn't
        // there and shifts the returned body — silent corruption. See
        // [`super::FileEntry::unknown_size_flag`].
        let file_embeds_name = self.embed_file_names;
        let name_prefix_len = if file_embeds_name {
            let mut len_buf = [0u8; 1];
            file.read_exact_at(&mut len_buf, pos)?;
            let name_len = len_buf[0] as usize;
            pos += 1 + name_len as u64;
            1 + name_len
        } else {
            0
        };

        // Determine if this file is compressed
        let is_compressed = self.compressed_by_default != entry.compression_toggle;
        // Guard against malformed records whose `entry.size` is smaller
        // than the embedded-name prefix the same record claimed. Pre-#352
        // this underflowed in release builds (wrapping to ~4 GB → giant
        // `vec![0u8; ...]` abort) and panicked in debug builds. Vanilla
        // Bethesda archives never trip either path; this is a defense
        // against hostile or corrupt third-party BSAs.
        let data_size = (entry.size as usize)
            .checked_sub(name_prefix_len)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "BSA file '{}' record size {} smaller than embedded name prefix {}",
                        path, entry.size, name_prefix_len
                    ),
                )
            })?;

        Ok(StoredBody {
            pos,
            data_size,
            compressed: is_compressed,
        })
    }

    /// The decoded size the archive declares for `path`, without extracting
    /// it: the record size for a stored file, or the 4-byte original-size
    /// header for a compressed one (one small positional read). Lets a
    /// caller budget memory before paying for the read + inflate. A
    /// compressed file with a known padding delta can extract a few bytes
    /// short of this (see the post-decompression warning in `extract`).
    pub fn declared_size(&self, path: &str) -> io::Result<usize> {
        let body = self.stored_body(path)?;
        if body.compressed {
            let mut size_buf = [0u8; 4];
            self.file.read_exact_at(&mut size_buf, body.pos)?;
            checked_chunk_size(u32::from_le_bytes(size_buf), "BSA original_size")
        } else {
            checked_chunk_size_usize(body.data_size, "BSA data_size")
        }
    }

    /// Extract a file's contents from the archive.
    /// Path matching is case-insensitive and normalizes separators.
    pub fn extract(&self, path: &str) -> io::Result<Vec<u8>> {
        let file = &self.file;
        let StoredBody {
            mut pos,
            data_size,
            compressed: is_compressed,
        } = self.stored_body(path)?;

        if is_compressed {
            // First 4 bytes are the original uncompressed size
            let mut size_buf = [0u8; 4];
            file.read_exact_at(&mut size_buf, pos)?;
            pos += 4;
            // Cap the decompression target buffer. BSA compressed files
            // top out at vanilla mesh LODs around ~30 MB uncompressed;
            // `MAX_CHUNK_BYTES` (1 GB, widened by `4a2b8200` to fit FO76
            // content) is a safe margin that still rejects `u32::MAX`.
            // #586.
            let original_size =
                checked_chunk_size(u32::from_le_bytes(size_buf), "BSA original_size")?;

            // Read remaining compressed data. Same #352 underflow guard
            // as above: a malformed record can flag the file compressed
            // while sizing the payload at < 4 bytes (too short to even
            // hold the original-size header we just read).
            let compressed_len = data_size.checked_sub(4).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "BSA file '{}' compressed payload too short \
                         ({} bytes) to hold the 4-byte original-size header",
                        path, data_size
                    ),
                )
            })?;
            // `data_size` itself came from `entry.size & 0x3FFFFFFF`
            // (30-bit mask → max 1 GB) — the explicit `checked_chunk_size_usize`
            // call still matters as defense-in-depth (it's the same
            // `MAX_CHUNK_BYTES` ceiling used elsewhere, not merely a
            // restatement of the mask). #586.
            let compressed_len = checked_chunk_size_usize(compressed_len, "BSA compressed_len")?;
            let mut compressed = vec![0u8; compressed_len];
            file.read_exact_at(&mut compressed, pos)?;

            // v103/v104 uses zlib, v105 uses LZ4 frame format.
            //
            // #3410 — `inflate_bounded`, not `read_to_end`. `original_size` was
            // validated above and then spent only as a capacity hint, so the
            // archive's own declared ceiling never actually stopped the
            // allocation it was checked for. Short decodes stay `Ok` (the
            // padding deltas the warn below exists for); an over-run is now a
            // hard `InvalidData`.
            let (decompressed, codec) = if self.version >= BSA_V_SKYRIM_SE {
                let decoder = lz4_flex::frame::FrameDecoder::new(&compressed[..]);
                let buf = crate::safety::inflate_bounded(
                    decoder,
                    original_size,
                    &format!("BSA LZ4 frame '{path}'"),
                )?;
                (buf, "LZ4 frame")
            } else {
                // #3812 — the zlib-specific sibling, not `inflate_bounded`
                // directly: it needs the raw compressed bytes so it can
                // retry as raw DEFLATE if the stream fails only its
                // Adler-32 trailer check (#3720's recovery, ported).
                let buf = crate::safety::inflate_bounded_zlib(
                    &compressed,
                    original_size,
                    &format!("BSA zlib '{path}'"),
                )?;
                (buf, "zlib")
            };

            // #622 / SK-D2-04: post-decompression sanity. Pre-fix a
            // truncated frame would silently produce a short buffer and
            // the downstream parser would error with a misleading
            // message ("NIF magic not found", "data underflow", etc.)
            // far from the actual cause. Surface the real cause clearly.
            // Mirrors the BA2 zlib path at `ba2.rs:457-462` — `log` not
            // hard-fail because some shipped archives have known
            // padding deltas where the decompressed payload reads short
            // by a handful of bytes; bumping to `warn` (BA2 uses
            // `debug`) keeps the signal visible without breaking
            // parse-rate on borderline content.
            if decompressed.len() != original_size {
                log::warn!(
                    "BSA {} decompression for '{}' produced {} bytes \
                     but original_size declared {} (delta {:+})",
                    codec,
                    path,
                    decompressed.len(),
                    original_size,
                    decompressed.len() as i64 - original_size as i64,
                );
            }

            Ok(decompressed)
        } else {
            // Uncompressed path: cap `data_size` too. The 30-bit mask
            // on `entry.size` already bounds this at 1 GB, but the
            // explicit `MAX_CHUNK_BYTES` (1 GB) call aligns the
            // uncompressed and compressed paths through the same named
            // constant rather than relying on the mask alone. #586.
            let data_size = checked_chunk_size_usize(data_size, "BSA data_size")?;
            let mut data = vec![0u8; data_size];
            file.read_exact_at(&mut data, pos)?;
            Ok(data)
        }
    }
}
