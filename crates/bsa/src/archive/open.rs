//! BSA header + folder/file record table walk.
//!
//! `BsaArchive::open` reads the 36-byte header, dispatches on the version
//! word, walks the folder records (16 bytes v103/v104, 24 bytes v105), then
//! the folder-name + file-record blocks, and finally the file-name table.
//! Result: a `HashMap<normalized_path, FileEntry>` ready for `extract`.

use super::{BsaArchive, FileEntry, BSA_V_FO3_SKYRIM, BSA_V_OBLIVION, BSA_V_SKYRIM_SE};
use crate::safety::checked_entry_count;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek};
use std::path::Path;
use std::sync::Mutex;

impl BsaArchive {
    /// Open a BSA archive and read its directory structure.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let mut reader = BufReader::new(File::open(path)?);

        // -- Header (36 bytes) --------------------------------------------------
        let mut header = [0u8; 36];
        reader.read_exact(&mut header)?;

        let magic = &header[0..4];
        if magic != b"BSA\0" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("not a BSA file (magic: {:?})", magic),
            ));
        }

        let version = u32::from_le_bytes(header[4..8].try_into().unwrap());
        if version != BSA_V_OBLIVION && version != BSA_V_FO3_SKYRIM && version != BSA_V_SKYRIM_SE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported BSA version {} (expected 103, 104, or 105)",
                    version
                ),
            ));
        }

        let archive_flags = u32::from_le_bytes(header[12..16].try_into().unwrap());
        // Cap folder / file counts before the downstream
        // `Vec::with_capacity` / `HashMap::with_capacity` allocations.
        // Vanilla archives top out at ~20 K folders / 1 M files (Skyrim
        // SE Meshes0.bsa); 10 M is a paranoid cap that still catches the
        // `u32::MAX` attack from a single corrupted header word. See
        // #586 / FO4-DIM2-01.
        let folder_count_raw = u32::from_le_bytes(header[16..20].try_into().unwrap());
        let folder_count = checked_entry_count(folder_count_raw, "BSA folder_count")?;
        let file_count_raw = u32::from_le_bytes(header[20..24].try_into().unwrap());
        let file_count = checked_entry_count(file_count_raw, "BSA file_count")?;
        let _total_folder_name_length = u32::from_le_bytes(header[24..28].try_into().unwrap());
        let _total_file_name_length = u32::from_le_bytes(header[28..32].try_into().unwrap());

        let include_dir_names = archive_flags & 1 != 0;
        let include_file_names = archive_flags & 2 != 0;
        let compressed_by_default = archive_flags & 4 != 0;
        // Bit 0x100 has different meaning across versions:
        //   v103 (Oblivion): "Xbox archive" — irrelevant on PC. Several
        //     vanilla v103 archives set this bit; ignoring it for embed-
        //     name purposes is what allows their 100% extraction rate.
        //   v104+ (FO3/Skyrim): "embed file names" — extract path skips a
        //     bstring prefix in each file body.
        // Source: UESP `Oblivion_Mod:BSA_File_Format#Archive_Flags`,
        // libbsarch `bsa_open.cpp` flag table.
        let embed_file_names = version >= BSA_V_FO3_SKYRIM && archive_flags & 0x100 != 0;

        if !include_dir_names || !include_file_names {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "BSA missing directory or file names",
            ));
        }

        log::debug!(
            "BSA v{}: {} folders, {} files, compressed_default={}",
            version,
            folder_count,
            file_count,
            compressed_by_default
        );

        // -- Folder Records (16 bytes v104 / 24 bytes v105) ----------------------
        // v104: [hash:u64, count:u32, offset:u32]
        // v105: [hash:u64, count:u32, _padding:u32, offset:u64]
        //
        // The offset is the absolute file position where the folder's
        // name + file records start, **with the `_total_file_name_length`
        // header quantity added to it** on disk — subtract that at
        // validation time. See `expected_offset` below. (#362)
        let folder_record_size: usize = if version == BSA_V_SKYRIM_SE { 24 } else { 16 };
        struct FolderRecord {
            /// Stored folder-name hash. Only retained in debug builds
            /// for the per-folder hash validation in the name-pass
            /// loop below (#361). Release builds drop the field
            /// entirely so the dead-code warning doesn't fire — same
            /// pattern as the sibling `RawFileRecord.hash`. See #622 /
            /// SK-D2-07.
            #[cfg(debug_assertions)]
            hash: u64,
            count: usize,
            /// v104: u32 at [12..16]. v105: u64 at [16..24]. Used to
            /// validate folder-block layout in debug builds (#362).
            /// Release-only dead-code per #622 / SK-D2-07.
            #[cfg(debug_assertions)]
            offset: u64,
        }
        let mut folder_records: Vec<FolderRecord> = Vec::with_capacity(folder_count);
        for _ in 0..folder_count {
            let mut rec = [0u8; 24];
            reader.read_exact(&mut rec[..folder_record_size])?;
            #[cfg(debug_assertions)]
            let hash = u64::from_le_bytes(rec[0..8].try_into().unwrap());
            // Per-folder file count: also attacker-controlled, and the
            // inner loop below pushes one entry per iteration — a
            // `u32::MAX` count would push past `file_count`'s pre-sized
            // `raw_files` capacity (triggering unbounded grow) and
            // sink the whole read. Cap it the same way. See #586.
            let count_raw = u32::from_le_bytes(rec[8..12].try_into().unwrap());
            let count = checked_entry_count(count_raw, "BSA folder.count")?;
            #[cfg(debug_assertions)]
            let offset = if version == BSA_V_SKYRIM_SE {
                u64::from_le_bytes(rec[16..24].try_into().unwrap())
            } else {
                // v103/v104 offset is u32 at [12..16].
                u32::from_le_bytes(rec[12..16].try_into().unwrap()) as u64
            };
            folder_records.push(FolderRecord {
                #[cfg(debug_assertions)]
                hash,
                count,
                #[cfg(debug_assertions)]
                offset,
            });
        }

        // -- Folder Name Blocks + File Records ----------------------------------
        struct RawFileRecord {
            folder_name: String,
            size: u32,
            offset: u32,
            compression_toggle: bool,
            /// Bit 31 of the on-disk size word — XOR'd against
            /// `embed_file_names` at extract time. See #616 / SK-D2-03.
            embed_name_toggle: bool,
            /// Stored file hash — only retained in debug builds for the
            /// later file-name-pass validation (#361). Release builds
            /// drop the field entirely.
            #[cfg(debug_assertions)]
            hash: u64,
        }

        let mut raw_files: Vec<RawFileRecord> = Vec::with_capacity(file_count);
        // #622 / SK-D2-05: track running consumed lengths for the two
        // header total fields the original parse silently dropped on
        // the floor (`_total_folder_name_length`, `_total_file_name_length`).
        // After both passes complete, log a warn if the running totals
        // disagree with the header — surfaces malformed / hand-crafted
        // archives early instead of letting them fail later in the
        // file-name pass with a misleading "read past end" error. Same
        // debug-only approach #361/#362 use for the per-folder hash +
        // offset checks.
        #[cfg(debug_assertions)]
        let mut folder_names_consumed: u64 = 0;
        #[cfg(debug_assertions)]
        let mut file_names_consumed: u64 = 0;

        for folder in &folder_records {
            // B2-04 (#362): verify the folder offset in the header matches
            // where we actually are. The on-disk offset is biased by
            // `total_file_name_length`, so we subtract it back out.
            // Mismatch means the table was reordered or padded by a
            // tool — not impossible but worth surfacing during dev.
            #[cfg(debug_assertions)]
            {
                let here = reader.stream_position().unwrap_or(0);
                let expected = folder.offset.saturating_sub(_total_file_name_length as u64);
                if expected != here {
                    log::warn!(
                        "BSA folder offset mismatch: expected {} (from record), got {} — archive may have been reordered",
                        expected,
                        here,
                    );
                }
            }

            // Read folder name (u8 length + null-terminated string)
            let mut len_buf = [0u8; 1];
            reader.read_exact(&mut len_buf)?;
            let name_len = len_buf[0] as usize;
            let mut name_buf = vec![0u8; name_len];
            reader.read_exact(&mut name_buf)?;
            // SK-D2-05 (#622): per UESP, `total_folder_name_length`
            // counts the name + trailing NUL but NOT the 1-byte length
            // prefix. `name_len` here already includes the NUL, so
            // accumulate it directly.
            #[cfg(debug_assertions)]
            {
                folder_names_consumed += name_len as u64;
            }
            // Remove null terminator
            if name_buf.last() == Some(&0) {
                name_buf.pop();
            }
            let folder_name = String::from_utf8_lossy(&name_buf).to_lowercase();

            // B2-03 (#361): warn if the stored folder hash disagrees
            // with the computed hash of the name we just read. A
            // mismatch points at either a hand-crafted archive or a
            // bug in our hash algorithm — either way, worth surfacing
            // in debug builds.
            #[cfg(debug_assertions)]
            {
                let computed = super::hash::genhash_folder(folder_name.as_bytes());
                if computed != folder.hash {
                    log::warn!(
                        "BSA folder hash mismatch for '{}': stored {:#018x}, computed {:#018x}",
                        folder_name,
                        folder.hash,
                        computed,
                    );
                }
            }

            // Read file records (16 bytes each)
            for _ in 0..folder.count {
                let mut frec = [0u8; 16];
                reader.read_exact(&mut frec)?;
                let hash = u64::from_le_bytes(frec[0..8].try_into().unwrap());
                let size_raw = u32::from_le_bytes(frec[8..12].try_into().unwrap());
                let offset = u32::from_le_bytes(frec[12..16].try_into().unwrap());
                let compression_toggle = size_raw & 0x40000000 != 0;
                // #616 / SK-D2-03: bit 31 of the on-disk size word is
                // a per-file embed-name override that XORs against the
                // archive-level `embed_file_names` flag. Vanilla
                // archives use a uniform per-archive policy so this bit
                // is always zero on shipped content; mods may flip it
                // per file. Pre-fix it was masked off as part of
                // `size & 0x3FFFFFFF` and never re-tested, so mixed-mode
                // BSAs extracted with the wrong path-prefix consumption.
                let embed_name_toggle = size_raw & 0x80000000 != 0;
                let size = size_raw & 0x3FFFFFFF;

                raw_files.push(RawFileRecord {
                    folder_name: folder_name.clone(),
                    size,
                    offset,
                    compression_toggle,
                    embed_name_toggle,
                    #[cfg(debug_assertions)]
                    hash,
                });
                #[cfg(not(debug_assertions))]
                let _ = hash;
            }
        }

        // -- File Name Table ----------------------------------------------------
        let mut files = HashMap::with_capacity(file_count);

        for raw in &raw_files {
            // Read null-terminated file name
            let mut name = Vec::new();
            loop {
                let mut byte = [0u8; 1];
                reader.read_exact(&mut byte)?;
                if byte[0] == 0 {
                    break;
                }
                name.push(byte[0]);
            }
            // SK-D2-05 (#622): file names contribute (name bytes + 1
            // NUL) each to `total_file_name_length`. `name.len()` here
            // is already without the NUL; +1 covers the terminator.
            #[cfg(debug_assertions)]
            {
                file_names_consumed += name.len() as u64 + 1;
            }
            let file_name = String::from_utf8_lossy(&name).to_lowercase();

            // B2-03 (#361): file hash validation mirrors the folder one.
            // A mismatch in either points at a mangled archive or a
            // bug in our hash algorithm — either way, surface in debug.
            #[cfg(debug_assertions)]
            {
                let computed = super::hash::genhash_file(file_name.as_bytes());
                if computed != raw.hash {
                    log::warn!(
                        "BSA file hash mismatch for '{}\\{}': stored {:#018x}, computed {:#018x}",
                        raw.folder_name,
                        file_name,
                        raw.hash,
                        computed,
                    );
                }
            }

            let full_path = format!("{}\\{}", raw.folder_name, file_name);

            files.insert(
                full_path,
                FileEntry {
                    offset: raw.offset as u64,
                    size: raw.size,
                    compression_toggle: raw.compression_toggle,
                    embed_name_toggle: raw.embed_name_toggle,
                },
            );
        }

        // #622 / SK-D2-05: validate the running totals against the
        // header. A mismatch points at a malformed / hand-crafted
        // archive — the header says the name tables are N bytes total
        // but we actually consumed M reading them. Surface it in
        // debug builds; release builds skip the bookkeeping entirely.
        #[cfg(debug_assertions)]
        {
            if folder_names_consumed != _total_folder_name_length as u64 {
                log::warn!(
                    "BSA total_folder_name_length mismatch: header {} vs consumed {}",
                    _total_folder_name_length,
                    folder_names_consumed,
                );
            }
            if file_names_consumed != _total_file_name_length as u64 {
                log::warn!(
                    "BSA total_file_name_length mismatch: header {} vs consumed {}",
                    _total_file_name_length,
                    file_names_consumed,
                );
            }
        }

        // Take ownership of the file handle (BufReader::into_inner is
        // infallible — it just returns the wrapped reader). The buffered
        // reader was right for the sequential header parse above; for
        // the random-access seek-and-read pattern in `extract`, an
        // unbuffered `File` is what we want anyway (each seek would
        // invalidate the BufReader's read-ahead). See #360.
        let file = reader.into_inner();
        Ok(BsaArchive {
            file: Mutex::new(file),
            version,
            compressed_by_default,
            embed_file_names,
            files,
        })
    }
}
