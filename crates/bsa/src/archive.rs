//! BSA archive reading and file extraction.

use crate::safety::{checked_chunk_size, checked_chunk_size_usize, checked_entry_count};
use flate2::read::ZlibDecoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

/// Bethesda BSA v103+ folder-name hash. Used by the v103/v104/v105
/// directory tables to identify folders without scanning names.
///
/// Algorithm (lower-cased, UTF-8):
/// - `hash_low` packs: last char (b7..b0), second-to-last char (b15..b8),
///   length (b23..b16), first char (b31..b24).
/// - `hash_high` is a rolling `(h * 0x1003f) + c` over the middle chars
///   `[1 .. len-2)`.
///
/// See UESP `Oblivion_Mod:BSA_File_Format#Hash_Calculation` and the
/// BSArch / libbsarch reference implementations. See #361.
#[allow(dead_code)]
fn genhash_folder(name: &str) -> u64 {
    let lower: Vec<u8> = name
        .as_bytes()
        .iter()
        .map(|b| b.to_ascii_lowercase())
        .collect();
    let len = lower.len();

    let mut hash_low: u32 = 0;
    if len > 0 {
        hash_low |= lower[len - 1] as u32;
    }
    if len >= 3 {
        hash_low |= (lower[len - 2] as u32) << 8;
    }
    hash_low |= (len as u32) << 16;
    if len > 0 {
        hash_low |= (lower[0] as u32) << 24;
    }

    let mut hash_high: u32 = 0;
    // Middle range `[1, len - 2)` — empty for len <= 3.
    if len > 3 {
        for &c in &lower[1..len - 2] {
            hash_high = hash_high.wrapping_mul(0x1003f).wrapping_add(c as u32);
        }
    }

    ((hash_high as u64) << 32) | (hash_low as u64)
}

/// Bethesda BSA v103+ file-name hash. The stem uses the same algorithm
/// as `genhash_folder`; the extension contributes both a stem XOR (for a
/// handful of privileged extensions) and an extra rolling hash pass
/// that gets folded into the high word.
///
/// `name` is the filename only — no directory component.
#[allow(dead_code)]
fn genhash_file(name: &str) -> u64 {
    let lower: Vec<u8> = name
        .as_bytes()
        .iter()
        .map(|b| b.to_ascii_lowercase())
        .collect();
    let (stem_bytes, ext_bytes) = match lower.iter().rposition(|&c| c == b'.') {
        Some(i) => (&lower[..i], &lower[i..]),
        None => (&lower[..], &lower[..0]),
    };

    // Base hash over the stem.
    let stem = std::str::from_utf8(stem_bytes).unwrap_or("");
    let mut hash = genhash_folder(stem);

    // Extension adds a known XOR constant to the low word for the most
    // common asset types.
    let ext = std::str::from_utf8(ext_bytes).unwrap_or("");
    let ext_xor: u32 = match ext {
        ".kf" => 0x80,
        ".nif" => 0x8000,
        ".dds" => 0x8080,
        ".wav" => 0x80000000,
        ".adp" => 0x00202e1a,
        _ => 0,
    };
    let hash_low = (hash as u32) ^ ext_xor;

    // Rolling hash over the whole extension (including the leading dot)
    // is computed INDEPENDENTLY from zero, then added into the stem's
    // high word. Pre-#449 this path folded the ext bytes on top of the
    // stem_high via sequential multiplication (`hash_high * 0x1003f + c`
    // starting from `stem_high`), which produces the wrong high word for
    // every file with stem length > 3. Low word matches either way so
    // HashMap lookup (path-keyed) worked, but the #361 debug-assertion
    // validation emitted 119k warnings per FO3 archive open.
    //
    // Verified against BSArchPro / libbsarch reference and a real FNV
    // stored hash: `meshes\armor\raiderarmor01\f\glover.nif` stores
    // `0xc86aec30_6706e572`; `rolling("lov") + rolling(".nif")` =
    // `0x359da633 + 0x92cd45fd` = `0xc86aec30` matches.
    let mut hash_ext = 0u32;
    for &c in ext_bytes {
        hash_ext = hash_ext.wrapping_mul(0x1003f).wrapping_add(c as u32);
    }
    let hash_high = ((hash >> 32) as u32).wrapping_add(hash_ext);

    hash = ((hash_high as u64) << 32) | (hash_low as u64);
    hash
}

/// A BSA v103/v104/v105 archive opened for reading.
///
/// v103: Oblivion (16-byte folder records, zlib compression)
/// v104: Fallout 3, Fallout NV, Skyrim LE (16-byte folder records, zlib compression)
/// v105: Skyrim SE, Fallout 4 (24-byte folder records, LZ4 compression, u64 offsets)
pub struct BsaArchive {
    /// Long-lived file handle reused across `extract` calls. Pre-#360
    /// every extract reopened the archive (one `open()` syscall per
    /// extracted file — hundreds per cell load); the mutex lets us
    /// reuse a single FD even though `extract` takes `&self`.
    file: Mutex<File>,
    version: u32,
    compressed_by_default: bool,
    /// When set (flag 0x100), each file's data starts with a bstring name prefix to skip.
    embed_file_names: bool,
    /// Maps normalized file path to FileEntry.
    files: HashMap<String, FileEntry>,
}

struct FileEntry {
    /// Byte offset from start of BSA file where file data begins.
    offset: u64,
    /// Raw size field from the file record (with compression toggle bit masked off).
    size: u32,
    /// Whether compression is toggled relative to archive default.
    compression_toggle: bool,
}

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
        if version != 103 && version != 104 && version != 105 {
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
        // Bit 0x100 means "embed file names" only in v104+ (FO3/Skyrim).
        // Oblivion v103 uses different flag semantics for bits 7-10.
        let embed_file_names = version >= 104 && archive_flags & 0x100 != 0;

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
        let folder_record_size: usize = if version == 105 { 24 } else { 16 };
        struct FolderRecord {
            hash: u64,
            count: usize,
            /// v104: u32 at [12..16]. v105: u64 at [16..24]. Used to
            /// validate folder-block layout in debug builds (#362).
            offset: u64,
        }
        let mut folder_records: Vec<FolderRecord> = Vec::with_capacity(folder_count);
        for _ in 0..folder_count {
            let mut rec = [0u8; 24];
            reader.read_exact(&mut rec[..folder_record_size])?;
            let hash = u64::from_le_bytes(rec[0..8].try_into().unwrap());
            // Per-folder file count: also attacker-controlled, and the
            // inner loop below pushes one entry per iteration — a
            // `u32::MAX` count would push past `file_count`'s pre-sized
            // `raw_files` capacity (triggering unbounded grow) and
            // sink the whole read. Cap it the same way. See #586.
            let count_raw = u32::from_le_bytes(rec[8..12].try_into().unwrap());
            let count = checked_entry_count(count_raw, "BSA folder.count")?;
            let offset = if version == 105 {
                u64::from_le_bytes(rec[16..24].try_into().unwrap())
            } else {
                // v103/v104 offset is u32 at [12..16].
                u32::from_le_bytes(rec[12..16].try_into().unwrap()) as u64
            };
            folder_records.push(FolderRecord {
                hash,
                count,
                offset,
            });
        }

        // -- Folder Name Blocks + File Records ----------------------------------
        struct RawFileRecord {
            folder_name: String,
            size: u32,
            offset: u32,
            compression_toggle: bool,
            /// Stored file hash — only retained in debug builds for the
            /// later file-name-pass validation (#361). Release builds
            /// drop the field entirely.
            #[cfg(debug_assertions)]
            hash: u64,
        }

        let mut raw_files: Vec<RawFileRecord> = Vec::with_capacity(file_count);

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
                let computed = genhash_folder(&folder_name);
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
                let size = size_raw & 0x3FFFFFFF;

                raw_files.push(RawFileRecord {
                    folder_name: folder_name.clone(),
                    size,
                    offset,
                    compression_toggle,
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
            let file_name = String::from_utf8_lossy(&name).to_lowercase();

            // B2-03 (#361): file hash validation mirrors the folder one.
            // A mismatch in either points at a mangled archive or a
            // bug in our hash algorithm — either way, surface in debug.
            #[cfg(debug_assertions)]
            {
                let computed = genhash_file(&file_name);
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
                },
            );
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

    /// List all file paths in the archive (lowercase, backslash-separated).
    pub fn list_files(&self) -> Vec<&str> {
        self.files.keys().map(|s| s.as_str()).collect()
    }

    /// Check if the archive contains a file at the given path.
    /// Path matching is case-insensitive and normalizes separators.
    pub fn contains(&self, path: &str) -> bool {
        let key = normalize_path(path);
        self.files.contains_key(&key)
    }

    /// Number of files in the archive.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

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
        // Present when archive flag 0x100 is set. The size field includes these bytes.
        let name_prefix_len = if self.embed_file_names {
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
            let decompressed = if self.version >= 105 {
                let mut decoder = lz4_flex::frame::FrameDecoder::new(&compressed[..]);
                let mut buf = Vec::with_capacity(original_size);
                decoder.read_to_end(&mut buf)?;
                buf
            } else {
                let mut decoder = ZlibDecoder::new(&compressed[..]);
                let mut buf = Vec::with_capacity(original_size);
                decoder.read_to_end(&mut buf)?;
                buf
            };

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

/// Normalize a file path for lookup: lowercase, forward slashes to backslashes.
fn normalize_path(path: &str) -> String {
    path.to_lowercase().replace('/', "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FNV_MESHES_BSA: &str =
        "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/Fallout - Meshes.bsa";

    // Skyrim SE BSA v105 (LZ4) — the only Bethesda format that uses the
    // LZ4 frame compression path. Pre-#569 the test surface had no
    // gated regression against real v105 archives; any change to the
    // frame-decoder dispatch, 24-byte folder record sizing, or u64
    // file-record offset read would slip through. See SK-D2-01.
    const SKYRIM_MESHES0_BSA: &str =
        "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim - Meshes0.bsa";
    const SKYRIM_MESHES1_BSA: &str =
        "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim - Meshes1.bsa";
    const SKYRIM_TEXTURES0_BSA: &str =
        "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim - Textures0.bsa";

    fn skip_if_missing() -> bool {
        !Path::new(FNV_MESHES_BSA).exists()
    }

    /// Per-archive availability gate so a test that needs Skyrim data
    /// stays green when only FNV is installed (and vice versa). Mirrors
    /// the FNV `skip_if_missing` pattern.
    fn skip_if_skyrim_missing(path: &str) -> bool {
        !Path::new(path).exists()
    }

    // ── Hash function unit tests (#361) ────────────────────────────────

    #[test]
    fn genhash_folder_empty_string_is_zero() {
        // Edge case: empty folder name. Algorithm returns 0 because no
        // bytes contribute to either word.
        assert_eq!(genhash_folder(""), 0);
    }

    #[test]
    fn genhash_folder_is_case_insensitive() {
        assert_eq!(
            genhash_folder("meshes\\clutter"),
            genhash_folder("MESHES\\CLUTTER"),
        );
    }

    #[test]
    fn genhash_folder_depends_on_content() {
        // Different folder names should produce different hashes.
        // (Not cryptographically guaranteed, but true for any two
        // distinct non-trivial Bethesda folder names.)
        assert_ne!(
            genhash_folder("meshes\\clutter"),
            genhash_folder("meshes\\architecture"),
        );
    }

    #[test]
    fn genhash_file_splits_on_last_dot() {
        // Extension should affect the hash; two files with the same
        // stem but different extensions must hash differently.
        assert_ne!(
            genhash_file("beerbottle01.nif"),
            genhash_file("beerbottle01.dds"),
        );
    }

    #[test]
    fn genhash_file_handles_no_extension() {
        // A name without `.` shouldn't panic. Falls back to empty ext.
        let _ = genhash_file("noextension");
    }

    /// Regression: #449 — `genhash_file` must produce the same hash the
    /// authoring tools wrote into real archives. Pinned against a known
    /// stored hash from vanilla FNV `Fallout - Meshes.bsa`:
    ///
    /// - path: `meshes\armor\raiderarmor01\f\glover.nif`
    /// - stored hash: `0xc86aec30_6706e572` (verified via hex dump)
    ///
    /// Pre-#449 the high word was computed by folding the extension
    /// rolling hash on top of `stem_high` sequentially (`stem_high *
    /// 0x1003f + c`), giving `0xd91bd930` — incorrect. The spec-
    /// compliant formula computes the extension hash from zero and
    /// adds it to `stem_high`: `0x359da633 + 0x92cd45fd = 0xc86aec30`.
    ///
    /// The low word (`0x6706e572`) is unaffected by the bug — it was
    /// correct before too, which is why HashMap path lookups worked
    /// even while 119k validation warnings fired per FO3 archive open.
    #[test]
    fn genhash_file_matches_stored_fnv_meshes_bsa_entry() {
        // `glover.nif` is the filename component; the folder is hashed
        // separately by genhash_folder. genhash_file takes only the
        // filename.
        let computed = genhash_file("glover.nif");
        assert_eq!(
            computed, 0xc86aec30_6706e572,
            "glover.nif must match FNV Meshes.bsa stored hash (low=0x{:08x} high=0x{:08x})",
            computed as u32,
            (computed >> 32) as u32,
        );
    }

    #[test]
    #[ignore]
    fn open_fnv_meshes_bsa() {
        if skip_if_missing() {
            return;
        }
        let archive = BsaArchive::open(FNV_MESHES_BSA).unwrap();
        assert_eq!(archive.file_count(), 19587);
    }

    #[test]
    #[ignore]
    fn list_files_contains_nif() {
        if skip_if_missing() {
            return;
        }
        let archive = BsaArchive::open(FNV_MESHES_BSA).unwrap();
        let files = archive.list_files();
        let nif_count = files.iter().filter(|f| f.ends_with(".nif")).count();
        assert!(
            nif_count > 10000,
            "expected >10k nif files, got {}",
            nif_count
        );
    }

    #[test]
    #[ignore]
    fn contains_beer_bottle() {
        if skip_if_missing() {
            return;
        }
        let archive = BsaArchive::open(FNV_MESHES_BSA).unwrap();
        assert!(archive.contains("meshes\\clutter\\food\\beerbottle01.nif"));
        // Case insensitive
        assert!(archive.contains("Meshes\\Clutter\\Food\\BeerBottle01.nif"));
        // Forward slashes
        assert!(archive.contains("meshes/clutter/food/beerbottle01.nif"));
        // Nonexistent
        assert!(!archive.contains("meshes\\nonexistent.nif"));
    }

    #[test]
    #[ignore]
    fn extract_beer_bottle() {
        if skip_if_missing() {
            return;
        }
        let archive = BsaArchive::open(FNV_MESHES_BSA).unwrap();
        let data = archive
            .extract("meshes\\clutter\\food\\beerbottle01.nif")
            .unwrap();
        // Should start with Gamebryo header
        assert!(
            data.starts_with(b"Gamebryo File Format"),
            "extracted data should start with NIF header, got {:?}",
            &data[..20.min(data.len())]
        );
        assert!(data.len() > 1000, "bottle nif should be >1KB");
    }

    #[test]
    #[ignore]
    fn extract_and_parse_nif() {
        if skip_if_missing() {
            return;
        }
        let archive = BsaArchive::open(FNV_MESHES_BSA).unwrap();
        let data = archive
            .extract("meshes\\clutter\\food\\beerbottle01.nif")
            .unwrap();
        // Write to temp file so NIF parser can read it
        std::fs::write("/tmp/test_bsa_bottle.nif", &data).unwrap();
        eprintln!("Extracted {} bytes to /tmp/test_bsa_bottle.nif", data.len());
    }

    #[test]
    #[ignore]
    fn extract_nonexistent_fails() {
        if skip_if_missing() {
            return;
        }
        let archive = BsaArchive::open(FNV_MESHES_BSA).unwrap();
        let result = archive.extract("meshes\\nonexistent.nif");
        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn texture_bsa_extract_dds() {
        let tex_bsa =
            "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/Fallout - Textures.bsa";
        if !Path::new(tex_bsa).exists() {
            return;
        }
        let archive = BsaArchive::open(tex_bsa).unwrap();
        eprintln!("Textures BSA: {} files", archive.file_count());

        assert!(
            archive.contains(r"textures\clutter\food\beerbottle.dds"),
            "should contain beerbottle texture"
        );

        let data = archive
            .extract(r"textures\clutter\food\beerbottle.dds")
            .unwrap();
        eprintln!("Extracted {} bytes, first 4: {:?}", data.len(), &data[..4]);
        assert_eq!(&data[..4], b"DDS ", "should start with DDS magic");
    }

    #[test]
    fn reject_non_bsa_file() {
        let result = BsaArchive::open("/dev/null");
        assert!(result.is_err());
    }

    #[test]
    fn normalize_path_works() {
        assert_eq!(
            normalize_path("Meshes/Clutter/Food/Bottle.nif"),
            "meshes\\clutter\\food\\bottle.nif"
        );
        assert_eq!(
            normalize_path("MESHES\\ARMOR\\test.NIF"),
            "meshes\\armor\\test.nif"
        );
    }

    /// Build a `BsaArchive` directly from in-memory state for tests that
    /// need to exercise `extract` with a hand-crafted `FileEntry`. The
    /// constructed archive points at a small temp file containing
    /// `payload`; the test controls every field of the `FileEntry` it
    /// inserts so it can drive specific malformed-record paths without
    /// having to forge a complete BSA on-disk header.
    fn archive_with_payload(
        payload: &[u8],
        embed_file_names: bool,
        compressed_by_default: bool,
        version: u32,
        entry_path: &str,
        entry: FileEntry,
    ) -> BsaArchive {
        // Write the payload to a unique temp file. Using a process-id +
        // entry-path key avoids collisions when the test runner runs
        // multiple tests concurrently.
        let path = std::env::temp_dir().join(format!(
            "byroredux-bsa-#352-{}-{}.bsa",
            std::process::id(),
            entry_path.replace(['\\', '/', ':'], "_"),
        ));
        std::fs::write(&path, payload).expect("write temp BSA payload");
        let file = File::open(&path).expect("open temp BSA");
        let mut files = HashMap::new();
        files.insert(normalize_path(entry_path), entry);
        BsaArchive {
            file: Mutex::new(file),
            version,
            compressed_by_default,
            embed_file_names,
            files,
        }
    }

    /// Regression: #352 — extracting an entry whose record `size` is
    /// smaller than the embedded-name prefix (impossible in vanilla
    /// Bethesda BSAs but achievable in a hostile or corrupt third-party
    /// archive) used to underflow `entry.size - name_prefix_len` in the
    /// release build (wrapping to ~4 GB → giant `vec![0u8; ...]` abort)
    /// and panic in the debug build. The fix uses `checked_sub` and
    /// returns `InvalidData`.
    #[test]
    fn extract_rejects_size_smaller_than_embedded_name_prefix() {
        // Payload: 1 byte name length (5) + 5 name bytes. The total
        // recorded `size` (3) is intentionally less than 1 + 5 = 6.
        let payload = [5u8, b'h', b'e', b'l', b'l', b'o', 0, 0, 0, 0];
        let archive = archive_with_payload(
            &payload,
            true, // embed_file_names ON
            false,
            104,
            "x.dds",
            FileEntry {
                offset: 0,
                size: 3,
                compression_toggle: false,
            },
        );
        let err = archive
            .extract("x.dds")
            .expect_err("malformed entry must be rejected");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "got: {err}");
        let msg = err.to_string();
        assert!(
            msg.contains("smaller than embedded name prefix"),
            "expected name-prefix error, got: {msg}"
        );
    }

    /// Regression: #352 — extracting a compressed entry whose payload
    /// size after the embedded-name strip is smaller than 4 (too short
    /// to even hold the original-size header) used to underflow
    /// `data_size - 4`. Same wrap-then-OOM/abort vector.
    #[test]
    fn extract_rejects_compressed_payload_too_short() {
        // 4 bytes are needed for the original-size header alone. We
        // make `entry.size = 3` with no embedded-name prefix; the
        // `data_size.checked_sub(4)` must reject before we read past
        // the (1-byte-too-short) buffer.
        let payload = [0u8, 0, 0, 0, 0, 0, 0, 0]; // 8 bytes is plenty for the test
        let archive = archive_with_payload(
            &payload,
            false, // no embedded names
            true,  // compressed-by-default ON
            104,
            "y.dds",
            FileEntry {
                offset: 0,
                size: 3, // < 4 bytes — too short to hold the size header
                compression_toggle: false,
            },
        );
        let err = archive
            .extract("y.dds")
            .expect_err("compressed-but-too-short entry must be rejected");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "got: {err}");
        let msg = err.to_string();
        assert!(
            msg.contains("compressed payload too short"),
            "expected payload-too-short error, got: {msg}"
        );
    }

    /// Sibling check — a record whose `size` exactly equals
    /// `1 + name_len` (so `data_size = 0`) is technically valid (an
    /// empty file with an embedded name), and must NOT be rejected by
    /// the new `checked_sub` guard. This pins the boundary so the
    /// guard doesn't overshoot.
    #[test]
    fn extract_zero_data_size_with_embedded_name_is_ok() {
        let payload = [5u8, b'h', b'e', b'l', b'l', b'o'];
        let archive = archive_with_payload(
            &payload,
            true,  // embed_file_names ON
            false, // not compressed
            104,
            "z.dds",
            FileEntry {
                offset: 0,
                size: 6, // exactly 1 + 5
                compression_toggle: false,
            },
        );
        let data = archive
            .extract("z.dds")
            .expect("zero-data-size entry must extract as empty Vec");
        assert!(data.is_empty());
    }

    /// Regression for #586 (FO4-DIM2-01, sibling) — a BSA with
    /// `folder_count = u32::MAX` must return `InvalidData` from
    /// `open()` before the reader allocates a `Vec::with_capacity`
    /// backing 4 billion folder records. Pre-fix this would abort on
    /// 64-bit targets.
    #[test]
    fn malicious_bsa_folder_count_u32_max_rejected() {
        use std::io::Write;
        // Build a minimal 36-byte BSA v104 header: magic + version +
        // offset + flags + folder_count = u32::MAX + rest zero. We
        // set `archive_flags` bits 1 + 2 so the early "missing names"
        // guard is cleared; the reader then hits the folder-count cap.
        let mut hdr = Vec::with_capacity(36);
        hdr.extend_from_slice(b"BSA\0"); // magic
        hdr.extend_from_slice(&104u32.to_le_bytes()); // version
        hdr.extend_from_slice(&36u32.to_le_bytes()); // offset (header size)
        hdr.extend_from_slice(&0b111u32.to_le_bytes()); // flags: dir + file names
        hdr.extend_from_slice(&u32::MAX.to_le_bytes()); // malicious folder_count
        hdr.extend_from_slice(&0u32.to_le_bytes()); // file_count
        hdr.extend_from_slice(&0u32.to_le_bytes()); // total_folder_name_length
        hdr.extend_from_slice(&0u32.to_le_bytes()); // total_file_name_length
        hdr.extend_from_slice(&0u32.to_le_bytes()); // trailing file_flags (BSA header is 36 bytes)
        assert_eq!(hdr.len(), 36);

        let path = std::env::temp_dir().join(format!(
            "byroredux_bsa_malicious_{}.bsa",
            std::process::id()
        ));
        {
            let mut f = File::create(&path).expect("create temp BSA");
            f.write_all(&hdr).expect("write malicious header");
        }
        let result = BsaArchive::open(&path);
        let _ = std::fs::remove_file(&path);
        let err = match result {
            Ok(_) => panic!("u32::MAX folder_count must not be accepted"),
            Err(e) => e,
        };
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let msg = format!("{err}");
        assert!(msg.contains("folder_count"), "got: {msg}");
    }

    // ── #569 SK-D2-01: Skyrim SE BSA v105 (LZ4) on-disk regression tests ──
    //
    // These tests exercise the v105 + LZ4 frame format end-to-end against
    // real Skyrim SE archives. They mirror the FNV pattern above —
    // `#[ignore]`'d so CI without Steam stays green; the user runs them
    // explicitly with `cargo test -- --ignored` against a real install.
    //
    // Pre-#569 the v104 + zlib path had on-disk coverage but the v105 +
    // LZ4 path did not, so a regression in the frame-decoder dispatch,
    // 24-byte folder record sizing, u64 file-record offset, or the
    // archive-level vs per-file compression toggle would slip through.

    /// Skyrim - Meshes0.bsa: largest vanilla SSE mesh archive (19,443
    /// files; ~18,862 NIFs, the rest are BGSM/BGEM/HKX/etc.). Pinned
    /// against the audit's Dim 2 corpus survey (`AUDIT_SKYRIM_2026-04-22`
    /// / `2026-04-24`). A drift in either count is the signal that a
    /// regression has landed in the v105 directory parse.
    #[test]
    #[ignore]
    fn skyrim_meshes0_opens_and_counts_match_baseline() {
        if skip_if_skyrim_missing(SKYRIM_MESHES0_BSA) {
            return;
        }
        let archive = BsaArchive::open(SKYRIM_MESHES0_BSA).unwrap();
        assert_eq!(
            archive.file_count(),
            19_443,
            "Skyrim - Meshes0.bsa file count drifted from the 2026-04 baseline"
        );
        let files = archive.list_files();
        let nif_count = files.iter().filter(|f| f.ends_with(".nif")).count();
        assert!(
            nif_count > 18_000,
            "expected >18k NIFs in Meshes0, got {nif_count}"
        );
    }

    /// Sweetroll round-trip: extract a known-size NIF and assert the
    /// LZ4 frame decoder produces exactly the expected byte count + a
    /// valid Gamebryo header. The 10,245-byte size is pinned by the
    /// audit's Dim 5 capture (`/tmp/audit/skyrim/sweetroll01.nif`).
    /// A drift here is the dominant signal for v105 frame-decoder
    /// regressions — Sweetroll is small enough to be a single LZ4
    /// frame yet large enough to exercise the full decode path.
    #[test]
    #[ignore]
    fn skyrim_meshes0_extracts_sweetroll_with_exact_size() {
        if skip_if_skyrim_missing(SKYRIM_MESHES0_BSA) {
            return;
        }
        let archive = BsaArchive::open(SKYRIM_MESHES0_BSA).unwrap();
        let path = "meshes\\clutter\\ingredients\\sweetroll01.nif";
        assert!(
            archive.contains(path),
            "Sweetroll path missing from Meshes0 archive — directory parse may be broken"
        );
        let data = archive.extract(path).unwrap();
        assert_eq!(
            data.len(),
            10_245,
            "Sweetroll decompressed size drifted — LZ4 frame decoder regression?"
        );
        assert!(
            data.starts_with(b"Gamebryo File Format"),
            "extracted Sweetroll missing NIF header magic: {:?}",
            &data[..20.min(data.len())]
        );
    }

    /// Path normalization: BSA paths are stored lowercased with
    /// backslashes. Verify that mixed-case / forward-slash inputs to
    /// `contains()` still hit on a known path. Mirrors the FNV
    /// equivalent at `contains_beer_bottle` so the SSE path doesn't
    /// silently regress on case-folding.
    #[test]
    #[ignore]
    fn skyrim_meshes0_path_normalization_matches_sweetroll() {
        if skip_if_skyrim_missing(SKYRIM_MESHES0_BSA) {
            return;
        }
        let archive = BsaArchive::open(SKYRIM_MESHES0_BSA).unwrap();
        let path = "meshes\\clutter\\ingredients\\sweetroll01.nif";
        assert!(archive.contains(path));
        assert!(archive.contains("MESHES\\CLUTTER\\INGREDIENTS\\SWEETROLL01.NIF"));
        assert!(archive.contains("meshes/clutter/ingredients/sweetroll01.nif"));
        assert!(!archive.contains("meshes\\clutter\\ingredients\\nonexistent01.nif"));
    }

    /// Skyrim - Meshes1.bsa is the DLC overflow archive (Dawnguard,
    /// Dragonborn, HearthFires content + post-launch additions). Pinned
    /// at 14,242 files — drift indicates the v105 multi-file-table
    /// indexing has changed.
    #[test]
    #[ignore]
    fn skyrim_meshes1_dlc_overflow_opens_and_counts_match_baseline() {
        if skip_if_skyrim_missing(SKYRIM_MESHES1_BSA) {
            return;
        }
        let archive = BsaArchive::open(SKYRIM_MESHES1_BSA).unwrap();
        assert_eq!(
            archive.file_count(),
            14_242,
            "Skyrim - Meshes1.bsa file count drifted from the 2026-04 baseline"
        );
    }

    /// Skyrim - Textures0.bsa: vanilla diffuse textures. Pinned at
    /// 5,891 files. Verifies the v105 path also handles texture-only
    /// archives (different file-extension distribution + no embedded
    /// names on this layout per the audit's Dim 2 sample).
    #[test]
    #[ignore]
    fn skyrim_textures0_opens_and_first_dds_decodes() {
        if skip_if_skyrim_missing(SKYRIM_TEXTURES0_BSA) {
            return;
        }
        let archive = BsaArchive::open(SKYRIM_TEXTURES0_BSA).unwrap();
        assert_eq!(
            archive.file_count(),
            5_891,
            "Skyrim - Textures0.bsa file count drifted from the 2026-04 baseline"
        );
        // Pick the first DDS in the listing and assert its header magic
        // round-trips. We don't pin a specific file path here — the
        // archive is large and any DDS exercises the same v105 + LZ4
        // path. The first-listed DDS keeps the test fast.
        let files = archive.list_files();
        let first_dds = files
            .iter()
            .find(|f| f.ends_with(".dds"))
            .expect("Textures0 must contain at least one .dds file");
        let path = first_dds.to_string();
        let data = archive.extract(&path).unwrap();
        // DDS magic: "DDS " (0x20534444 little-endian) at offset 0.
        assert!(
            data.len() >= 4 && &data[..4] == b"DDS ",
            "first DDS missing magic — decompression regression? path={path}, head={:?}",
            &data[..16.min(data.len())]
        );
    }
}
