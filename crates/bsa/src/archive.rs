//! BSA archive reading and file extraction.

use flate2::read::ZlibDecoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// A BSA v103/v104/v105 archive opened for reading.
///
/// v103: Oblivion (16-byte folder records, zlib compression)
/// v104: Fallout 3, Fallout NV, Skyrim LE (16-byte folder records, zlib compression)
/// v105: Skyrim SE, Fallout 4 (24-byte folder records, LZ4 compression, u64 offsets)
pub struct BsaArchive {
    path: std::path::PathBuf,
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
        let embed_file_names = archive_flags & 0x100 != 0;

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
        let folder_record_size: usize = if version == 105 { 24 } else { 16 };
        let mut folder_records = Vec::with_capacity(folder_count);
        for _ in 0..folder_count {
            let mut rec = [0u8; 24];
            reader.read_exact(&mut rec[..folder_record_size])?;
            let _hash = u64::from_le_bytes(rec[0..8].try_into().unwrap());
            let count = u32::from_le_bytes(rec[8..12].try_into().unwrap()) as usize;
            // v105 has padding at [12..16] and u64 offset at [16..24]; we don't use offset.
            folder_records.push(count);
        }

        // -- Folder Name Blocks + File Records ----------------------------------
        struct RawFileRecord {
            folder_name: String,
            size: u32,
            offset: u32,
            compression_toggle: bool,
        }

        let mut raw_files: Vec<RawFileRecord> = Vec::with_capacity(file_count);

        for &count in &folder_records {
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

            // Read file records (16 bytes each)
            for _ in 0..count {
                let mut frec = [0u8; 16];
                reader.read_exact(&mut frec)?;
                let _hash = u64::from_le_bytes(frec[0..8].try_into().unwrap());
                let size_raw = u32::from_le_bytes(frec[8..12].try_into().unwrap());
                let offset = u32::from_le_bytes(frec[12..16].try_into().unwrap());
                let compression_toggle = size_raw & 0x40000000 != 0;
                let size = size_raw & 0x3FFFFFFF;

                raw_files.push(RawFileRecord {
                    folder_name: folder_name.clone(),
                    size,
                    offset,
                    compression_toggle,
                });
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

        Ok(BsaArchive {
            path: path.to_path_buf(),
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

        let mut reader = BufReader::new(File::open(&self.path)?);
        reader.seek(SeekFrom::Start(entry.offset))?;

        // Skip embedded file name prefix (bstring: 1 byte length + name).
        // Present when archive flag 0x100 is set. The size field includes these bytes.
        let name_prefix_len = if self.embed_file_names {
            let mut len_buf = [0u8; 1];
            reader.read_exact(&mut len_buf)?;
            let name_len = len_buf[0] as usize;
            reader.seek(SeekFrom::Current(name_len as i64))?;
            1 + name_len
        } else {
            0
        };

        // Determine if this file is compressed
        let is_compressed = self.compressed_by_default != entry.compression_toggle;
        let data_size = entry.size as usize - name_prefix_len;

        if is_compressed {
            // First 4 bytes are the original uncompressed size
            let mut size_buf = [0u8; 4];
            reader.read_exact(&mut size_buf)?;
            let original_size = u32::from_le_bytes(size_buf) as usize;

            // Read remaining compressed data
            let compressed_len = data_size - 4;
            let mut compressed = vec![0u8; compressed_len];
            reader.read_exact(&mut compressed)?;

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
            reader.read_exact(&mut data)?;
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
}
