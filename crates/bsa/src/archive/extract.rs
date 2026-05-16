//! Per-file extraction from an opened BSA archive.
//!
//! Walks the embed-name prefix (when set), dispatches to the version-
//! appropriate decompressor (zlib for v103/v104, LZ4 frame for v105),
//! and returns the decompressed bytes. Same code path serves all three
//! versions — the version branch is the codec dispatch only.

use super::{normalize_path, BsaArchive, BSA_V_SKYRIM_SE};
use crate::safety::{checked_chunk_size, checked_chunk_size_usize};
use flate2::read::ZlibDecoder;
use std::io::{self, Read, Seek, SeekFrom};

impl BsaArchive {
    /// Extract a file's contents from the archive.
    /// Path matching is case-insensitive and normalizes separators.
    pub fn extract(&self, path: &str) -> io::Result<Vec<u8>> {
        let key = normalize_path(path);
        let entry = self.files.get(&key).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("file not found in BSA: {}", path),
            )
        })?;

        // Reuse the long-lived file handle stored at open time. Pre-#360
        // every extract did `BufReader::new(File::open(&self.path)?)` —
        // one `open()` syscall per file with hundreds of meshes per cell
        // load. Mutex serialises the seek/read pair so concurrent
        // extracts can't trample each other's file cursor.
        let mut file = self.file.lock().expect("BSA file mutex poisoned");
        file.seek(SeekFrom::Start(entry.offset))?;

        // Skip embedded file name prefix (bstring: 1 byte length + name).
        // Present when archive flag 0x100 is set, modulo the per-file
        // override at bit 31 of the size word — mirrors the
        // compression-toggle XOR pattern used immediately below. See
        // #616 / SK-D2-03. Vanilla Bethesda BSAs always carry a uniform
        // per-archive embed-name policy (the toggle bit is always
        // zero), so this XOR is a no-op on shipped content; modded
        // mixed-mode archives now extract correctly.
        let file_embeds_name = self.embed_file_names != entry.embed_name_toggle;
        let name_prefix_len = if file_embeds_name {
            let mut len_buf = [0u8; 1];
            file.read_exact(&mut len_buf)?;
            let name_len = len_buf[0] as usize;
            file.seek(SeekFrom::Current(name_len as i64))?;
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

        if is_compressed {
            // First 4 bytes are the original uncompressed size
            let mut size_buf = [0u8; 4];
            file.read_exact(&mut size_buf)?;
            // Cap the decompression target buffer. BSA compressed files
            // top out at vanilla mesh LODs around ~30 MB uncompressed;
            // 256 MB is a safe margin that still rejects `u32::MAX`.
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
            // (30-bit mask → max 1 GB) — the explicit cap brings it
            // into line with the 256 MB ceiling used elsewhere. #586.
            let compressed_len = checked_chunk_size_usize(compressed_len, "BSA compressed_len")?;
            let mut compressed = vec![0u8; compressed_len];
            file.read_exact(&mut compressed)?;
            // Drop the lock before the decompression CPU work — the file
            // handle isn't needed for decompression and other extracts
            // shouldn't have to wait.
            drop(file);

            // v104 uses zlib, v105 uses LZ4 frame format.
            let (decompressed, codec) = if self.version >= BSA_V_SKYRIM_SE {
                let mut decoder = lz4_flex::frame::FrameDecoder::new(&compressed[..]);
                let mut buf = Vec::with_capacity(original_size);
                decoder.read_to_end(&mut buf)?;
                (buf, "LZ4 frame")
            } else {
                let mut decoder = ZlibDecoder::new(&compressed[..]);
                let mut buf = Vec::with_capacity(original_size);
                decoder.read_to_end(&mut buf)?;
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
            // on `entry.size` already bounds this at 1 GB, but 256 MB
            // aligns the uncompressed and compressed paths. #586.
            let data_size = checked_chunk_size_usize(data_size, "BSA data_size")?;
            let mut data = vec![0u8; data_size];
            file.read_exact(&mut data)?;
            Ok(data)
        }
    }
}
