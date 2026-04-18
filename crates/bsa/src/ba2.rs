//! BA2 (BTDX) archive reader for Fallout 4, Fallout 76, and Starfield.
//!
//! BA2 is the post-BSA format. Three variants are relevant:
//!
//! - **GNRL** — general files (meshes, sounds, animations). Each file has a
//!   36-byte record with a u64 offset and optional zlib compression. This
//!   is what we need to load NIFs from FO4/FO76.
//! - **DX10** — texture archive. Each texture has a 24-byte base record plus
//!   per-mip-chain chunk records; the DDS header is not stored and must be
//!   reconstructed from the record fields (format, dimensions, mip count).
//! - **Starfield (v2/v3)** — extends the archive header by 8 (v2) or 12 (v3)
//!   bytes. v3 adds a `compression_method` field: 0 = zlib, 3 = LZ4 block.
//!   Both GNRL and DX10 extraction are fully supported.
//!
//! # Version mapping
//!
//! | BTDX version | Games                          | Notes                          |
//! |--------------|--------------------------------|--------------------------------|
//! | 1            | FO4 (original), FO76           | 24-byte header, zlib           |
//! | 2            | FO4 (patches), Starfield meshes | 32-byte header (base + 8)      |
//! | 3            | Starfield textures             | 36-byte header (base + 12, +compression method) |
//! | 7            | FO4 Next Gen textures          | 24-byte header, zlib           |
//! | 8            | FO4 Next Gen meshes            | 24-byte header, zlib           |
//!
//! # Usage
//!
//! ```ignore
//! let archive = byroredux_bsa::Ba2Archive::open("Fallout4 - Meshes.ba2")?;
//! let bytes = archive.extract("meshes/interiors/desk01.nif")?;
//! ```

use flate2::read::ZlibDecoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

const MAGIC_BTDX: &[u8; 4] = b"BTDX";
const MAGIC_GNRL: &[u8; 4] = b"GNRL";
const MAGIC_DX10: &[u8; 4] = b"DX10";
const PADDING_BAADFOOD: u32 = 0xBAAD_F00D;

/// Which file layout the archive uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ba2Variant {
    /// General archive — one 36-byte record per file, raw or zlib blob.
    General,
    /// Texture archive — one 24-byte base record per DDS with per-mip chunks.
    Dx10,
}

/// Compression codec for the archive's data chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ba2Compression {
    /// Standard zlib deflate (FO4, FO76, default).
    Zlib,
    /// LZ4 block format (Starfield v3).
    Lz4Block,
}

/// BA2 archive opened for reading.
pub struct Ba2Archive {
    /// Long-lived file handle reused across `extract` calls — see #360.
    /// Mutex serialises seek/read pairs so concurrent extracts can't
    /// trample each other's file cursor.
    file: Mutex<File>,
    version: u32,
    variant: Ba2Variant,
    compression: Ba2Compression,
    files: HashMap<String, Ba2Entry>,
}

#[derive(Debug, Clone)]
enum Ba2Entry {
    /// GNRL: a single blob.
    General {
        offset: u64,
        packed_size: u32,
        unpacked_size: u32,
    },
    /// DX10: DDS header fields + one or more compressed chunks.
    Dx10 {
        dxgi_format: u8,
        width: u16,
        height: u16,
        num_mips: u8,
        is_cubemap: bool,
        chunks: Vec<Dx10Chunk>,
    },
}

#[derive(Debug, Clone)]
struct Dx10Chunk {
    offset: u64,
    packed_size: u32,
    unpacked_size: u32,
    #[allow(dead_code)]
    start_mip: u16,
    #[allow(dead_code)]
    end_mip: u16,
}

impl Ba2Archive {
    /// Open a BA2 archive and read its directory + name table.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let mut reader = BufReader::new(File::open(path)?);

        // ── Header (24 bytes base, +8 for Starfield v2+) ────────────
        let mut hdr = [0u8; 24];
        reader.read_exact(&mut hdr)?;

        if &hdr[0..4] != MAGIC_BTDX {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("not a BA2 file (magic: {:?})", &hdr[0..4]),
            ));
        }

        let version = u32::from_le_bytes(hdr[4..8].try_into().unwrap());
        let type_tag: &[u8; 4] = hdr[8..12].try_into().unwrap();
        let file_count = u32::from_le_bytes(hdr[12..16].try_into().unwrap()) as usize;
        let name_table_offset = u64::from_le_bytes(hdr[16..24].try_into().unwrap());

        let variant = match type_tag {
            MAGIC_GNRL => Ba2Variant::General,
            MAGIC_DX10 => Ba2Variant::Dx10,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsupported BA2 type tag: {:?}", other),
                ));
            }
        };

        log::debug!(
            "BA2 v{}: {:?}, {} files, name_table@{:#x}",
            version,
            variant,
            file_count,
            name_table_offset
        );

        // Starfield archives extend the header beyond the base 24 bytes.
        // The version numbering is NOT monotonic across games: BTDX v1 =
        // FO4 original / FO76, v2/v3 = Starfield, v7/v8 = FO4 Next Gen
        // patches with the same 24-byte header as v1.
        //
        // v2 (Starfield GNRL): +8 bytes (2×u32 unknown, likely compressed
        //   name-table metadata). Compression is always zlib.
        // v3 (Starfield DX10): +12 bytes (2×u32 unknown + u32 compression
        //   method). Method 0 = zlib, 3 = LZ4 block.
        let mut compression = Ba2Compression::Zlib;
        if version == 2 || version == 3 {
            let mut extra = [0u8; 8];
            reader.read_exact(&mut extra)?;
        }
        if version == 3 {
            let mut method_buf = [0u8; 4];
            reader.read_exact(&mut method_buf)?;
            let method = u32::from_le_bytes(method_buf);
            compression = match method {
                0 => Ba2Compression::Zlib,
                3 => Ba2Compression::Lz4Block,
                other => {
                    log::warn!(
                        "BA2 v3: unknown compression method {}, assuming zlib",
                        other
                    );
                    Ba2Compression::Zlib
                }
            };
            log::debug!("BA2 v3 compression method: {:?}", compression);
        }

        // ── File records ────────────────────────────────────────────
        let files = match variant {
            Ba2Variant::General => read_general_records(&mut reader, file_count)?,
            Ba2Variant::Dx10 => read_dx10_records(&mut reader, file_count)?,
        };

        // ── Name table ──────────────────────────────────────────────
        reader.seek(SeekFrom::Start(name_table_offset))?;
        let mut names = Vec::with_capacity(file_count);
        for _ in 0..file_count {
            let mut len_buf = [0u8; 2];
            reader.read_exact(&mut len_buf)?;
            let name_len = u16::from_le_bytes(len_buf) as usize;
            let mut name_buf = vec![0u8; name_len];
            reader.read_exact(&mut name_buf)?;
            names.push(normalize_path(&String::from_utf8_lossy(&name_buf)));
        }

        if names.len() != files.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "BA2 name table length {} does not match record count {}",
                    names.len(),
                    files.len()
                ),
            ));
        }

        let mut map = HashMap::with_capacity(file_count);
        for (name, entry) in names.into_iter().zip(files.into_iter()) {
            map.insert(name, entry);
        }

        // Take ownership of the file handle for reuse across extracts
        // — see the `BsaArchive` Drop / locking notes; the same #360
        // rationale applies. BufReader was right for the sequential
        // header parse above; for the random-access extract path we
        // use the bare File so each seek doesn't waste read-ahead.
        let file = reader.into_inner();
        Ok(Self {
            file: Mutex::new(file),
            version,
            variant,
            compression,
            files: map,
        })
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn variant(&self) -> Ba2Variant {
        self.variant
    }

    /// List every file in the archive, normalized to lowercase with
    /// backslash separators (matching the BSA reader convention).
    pub fn list_files(&self) -> Vec<&str> {
        self.files.keys().map(|s| s.as_str()).collect()
    }

    /// Case-insensitive, slash-agnostic path lookup.
    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(&normalize_path(path))
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Extract a file from the archive.
    ///
    /// For GNRL entries, returns the raw (decompressed if needed) bytes.
    /// For DX10 entries, returns a complete `.dds` byte stream with a
    /// reconstructed DDS header followed by the assembled mip chunks.
    pub fn extract(&self, path: &str) -> io::Result<Vec<u8>> {
        let key = normalize_path(path);
        let entry = self.files.get(&key).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("file not found in BA2: {}", path),
            )
        })?;

        // Reuse the long-lived file handle — see #360.
        let mut file = self.file.lock().expect("BA2 file mutex poisoned");
        match entry {
            Ba2Entry::General {
                offset,
                packed_size,
                unpacked_size,
            } => extract_general(
                &mut *file,
                *offset,
                *packed_size,
                *unpacked_size,
                self.compression,
            ),
            Ba2Entry::Dx10 {
                dxgi_format,
                width,
                height,
                num_mips,
                is_cubemap,
                chunks,
            } => extract_dx10(
                &mut *file,
                *dxgi_format,
                *width,
                *height,
                *num_mips,
                *is_cubemap,
                chunks,
                self.compression,
            ),
        }
    }
}

/// Read `count` 36-byte GNRL file records.
fn read_general_records(reader: &mut BufReader<File>, count: usize) -> io::Result<Vec<Ba2Entry>> {
    let mut out = Vec::with_capacity(count);
    let mut rec = [0u8; 36];
    for _ in 0..count {
        reader.read_exact(&mut rec)?;
        // rec[0..4]   name_hash
        // rec[4..8]   ext
        // rec[8..12]  dir_hash
        // rec[12..16] flags
        let offset = u64::from_le_bytes(rec[16..24].try_into().unwrap());
        let packed_size = u32::from_le_bytes(rec[24..28].try_into().unwrap());
        let unpacked_size = u32::from_le_bytes(rec[28..32].try_into().unwrap());
        let padding = u32::from_le_bytes(rec[32..36].try_into().unwrap());
        if padding != PADDING_BAADFOOD {
            log::debug!(
                "BA2 GNRL record padding 0x{:08x} != 0xBAADF00D (offset {})",
                padding,
                offset
            );
        }
        out.push(Ba2Entry::General {
            offset,
            packed_size,
            unpacked_size,
        });
    }
    Ok(out)
}

/// Read `count` DX10 file records. Each record has a 24-byte base header
/// followed by `num_chunks` chunk headers (24 bytes each).
fn read_dx10_records(reader: &mut BufReader<File>, count: usize) -> io::Result<Vec<Ba2Entry>> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut base = [0u8; 24];
        reader.read_exact(&mut base)?;
        // base[0..4]  name_hash
        // base[4..8]  ext ("dds\0")
        // base[8..12] dir_hash
        // base[12]    unknown (0)
        // base[13]    num_chunks
        // base[14..16] chunk_hdr_len (24)
        // base[16..18] height
        // base[18..20] width
        // base[20]    num_mips
        // base[21]    dxgi format
        // base[22..24] flags (0x0800 = cubemap?)
        let num_chunks = base[13] as usize;
        let _chunk_hdr_len = u16::from_le_bytes(base[14..16].try_into().unwrap());
        let height = u16::from_le_bytes(base[16..18].try_into().unwrap());
        let width = u16::from_le_bytes(base[18..20].try_into().unwrap());
        let num_mips = base[20];
        let dxgi_format = base[21];
        let flags = u16::from_le_bytes(base[22..24].try_into().unwrap());
        // Bit 0 of the flags is the "is cubemap" indicator in FO4 DX10 archives.
        let is_cubemap = flags & 0x1 != 0;

        let mut chunks = Vec::with_capacity(num_chunks);
        for _ in 0..num_chunks {
            let mut chunk = [0u8; 24];
            reader.read_exact(&mut chunk)?;
            let offset = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
            let packed_size = u32::from_le_bytes(chunk[8..12].try_into().unwrap());
            let unpacked_size = u32::from_le_bytes(chunk[12..16].try_into().unwrap());
            let start_mip = u16::from_le_bytes(chunk[16..18].try_into().unwrap());
            let end_mip = u16::from_le_bytes(chunk[18..20].try_into().unwrap());
            let padding = u32::from_le_bytes(chunk[20..24].try_into().unwrap());
            if padding != PADDING_BAADFOOD {
                log::debug!("BA2 DX10 chunk padding 0x{:08x} != 0xBAADF00D", padding);
            }
            chunks.push(Dx10Chunk {
                offset,
                packed_size,
                unpacked_size,
                start_mip,
                end_mip,
            });
        }

        out.push(Ba2Entry::Dx10 {
            dxgi_format,
            width,
            height,
            num_mips,
            is_cubemap,
            chunks,
        });
    }
    Ok(out)
}

/// Decompress a packed chunk using the archive's compression codec.
fn decompress_chunk(
    packed: &[u8],
    unpacked_size: usize,
    compression: Ba2Compression,
) -> io::Result<Vec<u8>> {
    match compression {
        Ba2Compression::Zlib => {
            let mut decoder = ZlibDecoder::new(packed);
            let mut buf = Vec::with_capacity(unpacked_size);
            decoder.read_to_end(&mut buf)?;
            if buf.len() != unpacked_size {
                log::debug!(
                    "BA2 zlib decompressed {} bytes but record declared {}",
                    buf.len(),
                    unpacked_size
                );
            }
            Ok(buf)
        }
        Ba2Compression::Lz4Block => {
            lz4_flex::block::decompress(packed, unpacked_size).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("BA2 LZ4 block decompression failed: {}", e),
                )
            })
        }
    }
}

fn extract_general<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    packed_size: u32,
    unpacked_size: u32,
    compression: Ba2Compression,
) -> io::Result<Vec<u8>> {
    reader.seek(SeekFrom::Start(offset))?;
    if packed_size == 0 {
        // Uncompressed.
        let mut buf = vec![0u8; unpacked_size as usize];
        reader.read_exact(&mut buf)?;
        Ok(buf)
    } else {
        let mut packed = vec![0u8; packed_size as usize];
        reader.read_exact(&mut packed)?;
        decompress_chunk(&packed, unpacked_size as usize, compression)
    }
}

fn extract_dx10<R: Read + Seek>(
    reader: &mut R,
    dxgi_format: u8,
    width: u16,
    height: u16,
    num_mips: u8,
    is_cubemap: bool,
    chunks: &[Dx10Chunk],
    compression: Ba2Compression,
) -> io::Result<Vec<u8>> {
    // Pull each chunk's bytes (compressed or raw) and concatenate.
    let mut pixel_data = Vec::new();
    for chunk in chunks {
        reader.seek(SeekFrom::Start(chunk.offset))?;
        if chunk.packed_size == 0 {
            let mut buf = vec![0u8; chunk.unpacked_size as usize];
            reader.read_exact(&mut buf)?;
            pixel_data.extend_from_slice(&buf);
        } else {
            let mut packed = vec![0u8; chunk.packed_size as usize];
            reader.read_exact(&mut packed)?;
            let buf = decompress_chunk(&packed, chunk.unpacked_size as usize, compression)?;
            pixel_data.extend_from_slice(&buf);
        }
    }

    // Reconstruct a DDS header in front of the pixel data.
    let mut dds = build_dds_header(
        dxgi_format,
        width,
        height,
        num_mips,
        is_cubemap,
        &pixel_data,
    );
    dds.extend_from_slice(&pixel_data);
    Ok(dds)
}

/// Build a DDS header (with DX10 extended header) for a texture extracted
/// from a BA2 DX10 archive. BA2 does not store DDS bytes — the header has
/// to be synthesized from the record's `dxgi_format`/dimensions/mips.
fn build_dds_header(
    dxgi_format: u8,
    width: u16,
    height: u16,
    num_mips: u8,
    is_cubemap: bool,
    pixel_data: &[u8],
) -> Vec<u8> {
    // DDS constants we need.
    const DDS_MAGIC: u32 = 0x20534444; // "DDS "
    const DDSD_CAPS: u32 = 0x1;
    const DDSD_HEIGHT: u32 = 0x2;
    const DDSD_WIDTH: u32 = 0x4;
    const DDSD_PIXELFORMAT: u32 = 0x1000;
    const DDSD_MIPMAPCOUNT: u32 = 0x20000;
    const DDSD_LINEARSIZE: u32 = 0x80000;
    const DDSCAPS_TEXTURE: u32 = 0x1000;
    const DDSCAPS_MIPMAP: u32 = 0x400000;
    const DDSCAPS_COMPLEX: u32 = 0x8;
    const DDSCAPS2_CUBEMAP: u32 = 0x200;
    const DDSCAPS2_CUBEMAP_ALLFACES: u32 = 0xFE00;
    const DDPF_FOURCC: u32 = 0x4;

    const FOURCC_DX10: u32 = 0x30315844; // "DX10"

    // Resource dimension codes (for DX10 header).
    const D3D10_RESOURCE_DIMENSION_TEXTURE2D: u32 = 3;

    // Misc flag values.
    const D3D10_MISC_TEXTURECUBE: u32 = 0x4;

    let mut flags = DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PIXELFORMAT | DDSD_LINEARSIZE;
    if num_mips > 1 {
        flags |= DDSD_MIPMAPCOUNT;
    }

    let mut caps1 = DDSCAPS_TEXTURE;
    if num_mips > 1 {
        caps1 |= DDSCAPS_MIPMAP | DDSCAPS_COMPLEX;
    }
    let mut caps2 = 0;
    if is_cubemap {
        caps1 |= DDSCAPS_COMPLEX;
        caps2 |= DDSCAPS2_CUBEMAP | DDSCAPS2_CUBEMAP_ALLFACES;
    }

    // Linear size = full size of top-mip in bytes. For block-compressed
    // formats (BC1/3/5/7) this is max(1, ((w+3)/4)) * max(1, ((h+3)/4)) * block_size.
    // For uncompressed formats it's width * height * bytes_per_pixel. We
    // don't strictly need this to be accurate for loaders that ignore it,
    // but we compute a reasonable value using a format → block-size table.
    let linear_size = linear_size_for(dxgi_format, width as u32, height as u32, pixel_data.len());

    let mut hdr = Vec::with_capacity(148);
    hdr.extend_from_slice(&DDS_MAGIC.to_le_bytes());
    hdr.extend_from_slice(&124u32.to_le_bytes()); // dwSize
    hdr.extend_from_slice(&flags.to_le_bytes());
    hdr.extend_from_slice(&(height as u32).to_le_bytes());
    hdr.extend_from_slice(&(width as u32).to_le_bytes());
    hdr.extend_from_slice(&linear_size.to_le_bytes());
    hdr.extend_from_slice(&0u32.to_le_bytes()); // depth
    hdr.extend_from_slice(&(num_mips.max(1) as u32).to_le_bytes());
    // 11 reserved u32
    for _ in 0..11 {
        hdr.extend_from_slice(&0u32.to_le_bytes());
    }
    // Pixel format (32 bytes): always use the DX10 extended header path.
    hdr.extend_from_slice(&32u32.to_le_bytes()); // pf.size
    hdr.extend_from_slice(&DDPF_FOURCC.to_le_bytes());
    hdr.extend_from_slice(&FOURCC_DX10.to_le_bytes());
    for _ in 0..5 {
        hdr.extend_from_slice(&0u32.to_le_bytes());
    }
    // caps1..caps4 (16 bytes)
    hdr.extend_from_slice(&caps1.to_le_bytes());
    hdr.extend_from_slice(&caps2.to_le_bytes());
    hdr.extend_from_slice(&0u32.to_le_bytes());
    hdr.extend_from_slice(&0u32.to_le_bytes());
    hdr.extend_from_slice(&0u32.to_le_bytes()); // dwReserved2

    // DX10 extended header (20 bytes)
    hdr.extend_from_slice(&(dxgi_format as u32).to_le_bytes());
    hdr.extend_from_slice(&D3D10_RESOURCE_DIMENSION_TEXTURE2D.to_le_bytes());
    let misc_flag = if is_cubemap {
        D3D10_MISC_TEXTURECUBE
    } else {
        0
    };
    hdr.extend_from_slice(&misc_flag.to_le_bytes());
    hdr.extend_from_slice(&1u32.to_le_bytes()); // arraySize
    hdr.extend_from_slice(&0u32.to_le_bytes()); // miscFlags2

    debug_assert_eq!(hdr.len(), 148, "DDS + DX10 header must be 148 bytes");
    hdr
}

/// Compute a reasonable `dwPitchOrLinearSize` value for a DDS texture.
/// Falls back to the total pixel-data length divided across mips if we
/// don't recognize the DXGI format.
fn linear_size_for(dxgi_format: u8, width: u32, height: u32, total_bytes: usize) -> u32 {
    // DXGI formats we encounter in Bethesda BA2s. Block sizes per 4x4 block.
    let block_bytes: Option<u32> = match dxgi_format {
        71 | 72 => Some(8),  // BC1_UNORM / BC1_UNORM_SRGB
        74 | 75 => Some(16), // BC2_UNORM / BC2_UNORM_SRGB
        77 | 78 => Some(16), // BC3_UNORM / BC3_UNORM_SRGB
        80 | 81 => Some(8),  // BC4_UNORM / BC4_SNORM
        83 | 84 => Some(16), // BC5_UNORM / BC5_SNORM
        95 | 96 => Some(16), // BC6H
        98 | 99 => Some(16), // BC7_UNORM / BC7_UNORM_SRGB
        _ => None,
    };

    if let Some(bb) = block_bytes {
        let bw = width.div_ceil(4).max(1);
        let bh = height.div_ceil(4).max(1);
        bw * bh * bb
    } else {
        // Unknown format — report the entire pixel payload, which at least
        // lets a loader size its buffer without truncation.
        total_bytes as u32
    }
}

/// Normalize a path for case-insensitive, slash-agnostic lookup.
fn normalize_path(path: &str) -> String {
    path.to_lowercase().replace('/', "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_works() {
        assert_eq!(
            normalize_path("Meshes/Interiors/Test.nif"),
            "meshes\\interiors\\test.nif"
        );
        assert_eq!(normalize_path("MESHES\\foo.NIF"), "meshes\\foo.nif");
    }

    #[test]
    fn reject_non_ba2_file() {
        let result = Ba2Archive::open("/dev/null");
        assert!(result.is_err());
    }

    #[test]
    fn linear_size_bc1_256x256() {
        // 256×256 BC1: 64×64 blocks × 8 bytes = 32768
        assert_eq!(linear_size_for(71, 256, 256, 0), 32768);
    }

    #[test]
    fn linear_size_bc7_512x256() {
        // 512×256 BC7: 128×64 blocks × 16 bytes = 131072
        assert_eq!(linear_size_for(98, 512, 256, 0), 131072);
    }

    #[test]
    fn linear_size_unknown_format_uses_total() {
        assert_eq!(linear_size_for(0, 128, 128, 9999), 9999);
    }

    #[test]
    fn build_dds_header_is_148_bytes() {
        // Validate header layout invariants independent of an actual archive.
        let hdr = build_dds_header(71, 256, 256, 9, false, &[]);
        assert_eq!(hdr.len(), 148);
        // Magic
        assert_eq!(&hdr[0..4], b"DDS ");
        // Struct size = 124
        assert_eq!(u32::from_le_bytes(hdr[4..8].try_into().unwrap()), 124);
        // Width/height
        assert_eq!(u32::from_le_bytes(hdr[12..16].try_into().unwrap()), 256); // height
        assert_eq!(u32::from_le_bytes(hdr[16..20].try_into().unwrap()), 256); // width
                                                                              // mip count
        assert_eq!(u32::from_le_bytes(hdr[28..32].try_into().unwrap()), 9);
        // FourCC at offset 84 should be "DX10"
        assert_eq!(&hdr[84..88], b"DX10");
        // DX10 extended: dxgi_format at offset 128
        assert_eq!(u32::from_le_bytes(hdr[128..132].try_into().unwrap()), 71);
    }

    #[test]
    fn decompress_chunk_zlib_roundtrip() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let original = b"Hello, Starfield BA2 textures!";
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();

        let result = decompress_chunk(&compressed, original.len(), Ba2Compression::Zlib).unwrap();
        assert_eq!(result, original);
    }

    #[test]
    fn decompress_chunk_lz4_roundtrip() {
        let original = b"Starfield LZ4 block compressed texture chunk data - test payload";
        let compressed = lz4_flex::block::compress(original);

        let result =
            decompress_chunk(&compressed, original.len(), Ba2Compression::Lz4Block).unwrap();
        assert_eq!(result, original.as_slice());
    }

    #[test]
    fn decompress_chunk_lz4_corrupt_data_fails() {
        // Garbage input should fail LZ4 decompression.
        let garbage = [0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA];
        let result = decompress_chunk(&garbage, 1024, Ba2Compression::Lz4Block);
        assert!(result.is_err());
    }
}
