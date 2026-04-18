//! BSA archive reading and file extraction.

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
    // folds into the high word on top of the stem's contribution.
    let mut hash_high = (hash >> 32) as u32;
    for &c in ext_bytes {
        hash_high = hash_high.wrapping_mul(0x1003f).wrapping_add(c as u32);
    }

    // Preserve the low-word XOR by folding back in; this matches
    // BSArch's final combine step for filenames.
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
        let folder_count = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;
        let file_count = u32::from_le_bytes(header[20..24].try_into().unwrap()) as usize;
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
            let count = u32::from_le_bytes(rec[8..12].try_into().unwrap()) as usize;
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
            let original_size = u32::from_le_bytes(size_buf) as usize;

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

    fn skip_if_missing() -> bool {
        !Path::new(FNV_MESHES_BSA).exists()
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
}
