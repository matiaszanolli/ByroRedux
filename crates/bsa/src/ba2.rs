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
//!   bytes. v2 carries both GNRL (mesh) and DX10 (texture) archives in
//!   vanilla Starfield; v3 is DX10-only in the shipped game (no v3 GNRL
//!   observed across 108 vanilla archives). v3 adds a `compression_method`
//!   field: 0 = zlib, 3 = LZ4 block. Both GNRL and DX10 extraction are
//!   fully supported for v2 and v3.
//!
//! # Version mapping
//!
//! | BTDX version | Games                              | Notes                                               |
//! |--------------|------------------------------------|-----------------------------------------------------|
//! | 1            | FO4 (original), FO76               | 24-byte header, zlib only                           |
//! | 2            | FO4 (patches), Starfield GNRL+DX10 | 32-byte header (base + 8); v2 DX10 exists in vanilla |
//! | 3            | Starfield DX10                     | 36-byte header (base + 12, +compression_method); 0=zlib, 3=LZ4 block |
//! | 7            | FO4 Next Gen textures              | 24-byte header, zlib only                           |
//! | 8            | FO4 Next Gen meshes                | 24-byte header, zlib only                           |
//!
//! # Compression model
//!
//! Compression is two-axis (#596 / FO4-DIM2-06):
//!
//! 1. **Archive-wide codec** — fixed for the whole archive at header parse
//!    time. v1/v2/v7/v8 always use zlib; v3 carries an explicit
//!    `compression_method` field (`0 = zlib`, `3 = LZ4 block`). Stored on
//!    [`Ba2Archive::compression`] and consulted once per extracted chunk.
//! 2. **Per-chunk on-off** — a `packed_size == 0` marker on a GNRL file
//!    record or a DX10 chunk record means the payload is stored RAW
//!    (no decode). Independent of the codec choice above. Both
//!    [`Ba2Archive::extract_general`] and [`Ba2Archive::extract_dx10`]
//!    branch on this per chunk before invoking the codec.
//!
//! Treating compression as "the archive is zlib" or "the archive is LZ4"
//! loses the per-chunk axis — vanilla FO4 archives ship a non-trivial
//! fraction of pre-compressed-too-small or stored-raw chunks, and Starfield
//! v3 DX10 mips mix raw and LZ4-compressed within one texture.
//!
//! # Usage
//!
//! ```ignore
//! let archive = byroredux_bsa::Ba2Archive::open("Fallout4 - Meshes.ba2")?;
//! let bytes = archive.extract("meshes/interiors/desk01.nif")?;
//! ```

use crate::safety::{checked_chunk_size, checked_chunk_size_usize, checked_entry_count};
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

// DDS header `dwFlags` bits governing the `dwPitchOrLinearSize`
// field's meaning. Shared at module scope so both `build_dds_header`
// and the `pitch_or_linear_size_for` helper can reference them.
// See audit FO4-DIM2-03 / #594.
const DDSD_PITCH: u32 = 0x8;
const DDSD_LINEARSIZE: u32 = 0x80000;

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
    // MILESTONE: M40 streaming (partial-mip-range texture upload) — see #1049.
    // Today the BA2 reader extracts full DDS files; once M40 streams mip
    // ranges, these per-chunk bounds are how the renderer requests the
    // subset it needs without re-reading the whole texture.
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
        // Cap `file_count` before any downstream `Vec::with_capacity` /
        // `HashMap::with_capacity`. Vanilla archives top out ~600 K
        // entries; anything beyond 10 M is either corruption or a DoS
        // attempt. See #586 / FO4-DIM2-01.
        let file_count_raw = u32::from_le_bytes(hdr[12..16].try_into().unwrap());
        let file_count = checked_entry_count(file_count_raw, "BA2 file_count")?;
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
        // v1 / v7 / v8: 24-byte base header, zlib compression (per-game
        //   variant; the parser treats them identically here).
        // v2 (Starfield GNRL and DX10): +8 bytes (2×u32 unknown, likely
        //   compressed name-table metadata). Compression is always zlib.
        // v3 (Starfield DX10 only in vanilla; no v3 GNRL observed across 108
        //   vanilla archives): +12 bytes (2×u32 unknown + u32 compression
        //   method). Method 0 = zlib, 3 = LZ4 block.
        //
        // #811 / FO4-D2-NEW-01 — match exhaustively over the supported
        // version set so unknown majors (0, 4, 5, 6, 9, ..., u32::MAX)
        // bail with a clear error instead of silently falling through to
        // the v1 record-layout path. Mirrors the BSA reader's allowlist
        // discipline at `archive.rs:165-173`.
        let compression = match version {
            1 | 7 | 8 => Ba2Compression::Zlib,
            2 => {
                let mut extra = [0u8; 8];
                reader.read_exact(&mut extra)?;
                Ba2Compression::Zlib
            }
            3 => {
                let mut extra = [0u8; 8];
                reader.read_exact(&mut extra)?;
                let mut method_buf = [0u8; 4];
                reader.read_exact(&mut method_buf)?;
                let method = u32::from_le_bytes(method_buf);
                let c = match method {
                    0 => Ba2Compression::Zlib,
                    3 => Ba2Compression::Lz4Block,
                    other => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "BA2 v3: unsupported compression method {} \
                                 (expected 0=zlib or 3=lz4_block)",
                                other
                            ),
                        ));
                    }
                };
                log::debug!("BA2 v3 compression method: {:?}", c);
                c
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "unsupported BA2 version: {} \
                         (expected 1, 2, 3, 7, or 8)",
                        other
                    ),
                ));
            }
        };

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

    /// Iterate `(path, packed_size, unpacked_size)` for every GNRL
    /// entry. DX10 entries are skipped — their packed/unpacked sizes
    /// live on each mip chunk and the archive-level "single file size"
    /// abstraction doesn't apply.
    ///
    /// Used by `examples/ba2_ratio_anomaly.rs` (#598) to surface GNRL
    /// records where `packed_size > unpacked_size` — a ratio impossible
    /// for well-formed deflate, and worth investigating as either a
    /// benign block-alignment quirk or a parser mis-interpretation.
    pub fn iter_general_sizes(&self) -> impl Iterator<Item = (&str, u32, u32)> + '_ {
        self.files.iter().filter_map(|(name, entry)| match entry {
            Ba2Entry::General {
                packed_size,
                unpacked_size,
                ..
            } => Some((name.as_str(), *packed_size, *unpacked_size)),
            Ba2Entry::Dx10 { .. } => None,
        })
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
    // `count` was already capped by `checked_entry_count` at the header
    // parse site; the `Vec::with_capacity` below is therefore safe.
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
        // #586 — reject obviously-hostile sizes at record-read time so
        // `extract` never has to trust them. Vanilla FO4 GNRL entries
        // top out around 8 MB decompressed; 256 MB is a comfortable
        // margin. Single check catches a `u32::MAX` entry that would
        // otherwise flow into `vec![0u8; n]` at extract time.
        checked_chunk_size(packed_size, "BA2 GNRL packed_size")?;
        checked_chunk_size(unpacked_size, "BA2 GNRL unpacked_size")?;
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
    // `count` was capped at the header parse site by
    // `checked_entry_count`; the `Vec::with_capacity` is safe.
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
        // base[22..24] flags — bit 0 = cubemap (verified against
        //                     vanilla Textures1.ba2 cubemap entries).
        //                     The 0x0800 bit is something else
        //                     (community-reverse-engineered as "tile
        //                     mode") and is NOT the cubemap flag — see
        //                     #595 for the prior stale-comment trap.
        let num_chunks = base[13] as usize;
        let _chunk_hdr_len = u16::from_le_bytes(base[14..16].try_into().unwrap());
        let height = u16::from_le_bytes(base[16..18].try_into().unwrap());
        let width = u16::from_le_bytes(base[18..20].try_into().unwrap());
        let num_mips = base[20];
        let dxgi_format = base[21];
        let flags = u16::from_le_bytes(base[22..24].try_into().unwrap());
        // Bit 0 of the flags is the "is cubemap" indicator in FO4 DX10 archives.
        let is_cubemap = flags & 0x1 != 0;

        // `num_mips == 0` is malformed — the DX10 record should always
        // declare at least a base (level-0) mip for the top-level
        // image. Vanilla FO4 archives never trip this; third-party
        // tooling occasionally writes 0 and we've been silently
        // clamping in `build_dds_header` (via `num_mips.max(1)`). Surface
        // the anomaly as `warn!` so operators can spot bad archives in
        // the logs while still producing a working 1-mip DDS. See
        // audit FO4-DIM2-07 / #597.
        if num_mips == 0 {
            log::warn!(
                "BA2 DX10 record at chunk 0x{:016x} declares num_mips = 0 \
                 (malformed) — clamping to 1 mip in the synthesized DDS header; \
                 archive is likely third-party-repackaged",
                reader.stream_position().unwrap_or(0)
            );
        }

        // `num_chunks` is a u8 (max 255), so the `Vec::with_capacity`
        // here is inherently bounded and needs no extra check.
        let mut chunks = Vec::with_capacity(num_chunks);
        for _ in 0..num_chunks {
            let mut chunk = [0u8; 24];
            reader.read_exact(&mut chunk)?;
            let offset = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
            let packed_size = u32::from_le_bytes(chunk[8..12].try_into().unwrap());
            let unpacked_size = u32::from_le_bytes(chunk[12..16].try_into().unwrap());
            // #586 — cap DX10 chunk sizes at record-read time.
            checked_chunk_size(packed_size, "BA2 DX10 chunk packed_size")?;
            checked_chunk_size(unpacked_size, "BA2 DX10 chunk unpacked_size")?;
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
    // Defense-in-depth: entries that reached this function already had
    // `unpacked_size` capped at record-read time, but `decompress_chunk`
    // is also reachable from the test harness with arbitrary inputs.
    // Re-check so a direct caller can't bypass the safety net. #586.
    let unpacked_size = checked_chunk_size_usize(unpacked_size, "BA2 decompress unpacked_size")?;
    match compression {
        Ba2Compression::Zlib => {
            let mut decoder = ZlibDecoder::new(packed);
            let mut buf = Vec::with_capacity(unpacked_size);
            decoder.read_to_end(&mut buf)?;
            if buf.len() != unpacked_size {
                // #812 / FO4-D2-NEW-02 — the LZ4 branch hard-errors
                // on the same condition (lz4_flex inherently size-
                // checks); zlib's `read_to_end` honours deflate's
                // self-terminating end-of-stream marker mid-buffer
                // so a truncated archive returns a short blob.
                // Promoted from `log::debug!` so operators see the
                // mismatch in standard log output without changing
                // the lenient semantics — a synthetic `unpacked_size
                // = 100 / actual stream = 20` archive currently
                // pivots into the NIF / DDS parser at the wrong
                // size with no signalling above debug-log noise.
                // Sibling of #622's BSA-side hardening
                // (SK-D2-04). The optional `Strict` mode that
                // would convert this to an `InvalidData` error is
                // gated on #598's investigation of vanilla
                // `Fallout4 - Meshes.ba2`'s `packed_size >
                // unpacked_size` anomaly.
                log::warn!(
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

    let mut flags = DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PIXELFORMAT;
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

    // Per DDS spec, `dwPitchOrLinearSize` has two meanings gated by
    // the DDSD_PITCH / DDSD_LINEARSIZE flag:
    //
    //   - Block-compressed formats (BC1-BC7) → DDSD_LINEARSIZE, value
    //     is the full-size-of-top-mip in bytes.
    //   - Uncompressed formats → DDSD_PITCH, value is the row pitch
    //     (`width * bytes_per_pixel`) of the top mip.
    //
    // Pre-#594 the header unconditionally set DDSD_LINEARSIZE and
    // wrote `total_bytes` for every format — technically invalid for
    // R8G8B8A8 / R8 / R16 / B8G8R8A8. Vulkan / D3D11+ loaders ignore
    // the legacy field (the DX10 extended header at offset 128
    // disambiguates), but strict validators and legacy tools
    // (texconv, DirectXTex, Paint.NET DDS plugin) reject it. See
    // audit FO4-DIM2-03.
    let (pitch_or_linear_size, pitch_flag) =
        pitch_or_linear_size_for(dxgi_format, width as u32, height as u32, pixel_data.len());
    flags |= pitch_flag;

    let mut hdr = Vec::with_capacity(148);
    hdr.extend_from_slice(&DDS_MAGIC.to_le_bytes());
    hdr.extend_from_slice(&124u32.to_le_bytes()); // dwSize
    hdr.extend_from_slice(&flags.to_le_bytes());
    hdr.extend_from_slice(&(height as u32).to_le_bytes());
    hdr.extend_from_slice(&(width as u32).to_le_bytes());
    hdr.extend_from_slice(&pitch_or_linear_size.to_le_bytes());
    hdr.extend_from_slice(&0u32.to_le_bytes()); // depth
                                                // `num_mips.max(1)` is an intentional clamp: the DDS spec says
                                                // when `DDSD_MIPMAPCOUNT` is unset the loader MUST treat the image
                                                // as single-mip regardless of what `dwMipMapCount` says, so both
                                                // values are always self-consistent:
                                                //   - `num_mips == 0` or `1` → flag cleared, field = 1 (top mip only).
                                                //   - `num_mips > 1`         → flag set, field = authored value.
                                                // The malformed `num_mips = 0` path is warned at record-read time
                                                // (`read_dx10_records`) so the clamp isn't silent. See #597.
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
    // arraySize — DDS_HEADER_DXT10 spec requires `6 × N` (default N=1)
    // for cubemaps; non-cubemaps use 1. Pre-#593 this was hardcoded to
    // `1`, which DXGI loaders (`CreateTexture2D` with
    // `D3D10_RESOURCE_MISC_TEXTURECUBE`) reject as "arraySize must be
    // a multiple of 6". The in-engine renderer's `dds.rs` is lenient
    // (reads miscFlag, ignores arraySize) so this is observable only
    // in DirectXTex / texconv / third-party DDS viewers — but the
    // synthesized headers are now spec-compliant either way.
    let array_size: u32 = if is_cubemap { 6 } else { 1 };
    hdr.extend_from_slice(&array_size.to_le_bytes());
    hdr.extend_from_slice(&0u32.to_le_bytes()); // miscFlags2

    debug_assert_eq!(hdr.len(), 148, "DDS + DX10 header must be 148 bytes");
    hdr
}

/// Compute `dwPitchOrLinearSize` + the matching `DDSD_*` flag for a
/// DDS texture header. Block-compressed formats yield
/// `(LinearSize, DDSD_LINEARSIZE)`; uncompressed formats yield
/// `(RowPitch, DDSD_PITCH)`; unknown formats fall back to the legacy
/// `(total_bytes, DDSD_LINEARSIZE)` behaviour so malformed inputs
/// don't make the DDS unreadable.
///
/// The caller is responsible for OR-ing the returned flag into
/// `dwFlags`. See audit FO4-DIM2-03 / #594.
fn pitch_or_linear_size_for(
    dxgi_format: u8,
    width: u32,
    height: u32,
    total_bytes: usize,
) -> (u32, u32) {
    // Block-compressed formats we encounter in Bethesda BA2s. Block
    // sizes are per 4×4 texel block.
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
        return (bw * bh * bb, DDSD_LINEARSIZE);
    }

    // Uncompressed DXGI formats observed in Bethesda BA2 DX10
    // archives. Bytes-per-pixel per Microsoft's DXGI_FORMAT enum:
    //   28 = R8G8B8A8_UNORM       (4 bpp)
    //   29 = R8G8B8A8_UNORM_SRGB  (4 bpp)
    //   87 = B8G8R8A8_UNORM       (4 bpp) — FO4 normal maps
    //   91 = B8G8R8A8_UNORM_SRGB  (4 bpp)
    //   56 = R16_UNORM            (2 bpp) — height / mask textures
    //   61 = R8_UNORM             (1 bpp) — mono masks
    // For uncompressed, the pitch is the byte length of one row of
    // pixels (`width * bpp`) — NOT the total buffer size.
    let bpp: Option<u32> = match dxgi_format {
        28 | 29 | 87 | 91 => Some(4),
        56 => Some(2),
        61 => Some(1),
        _ => None,
    };
    if let Some(b) = bpp {
        return (width * b, DDSD_PITCH);
    }

    // Unknown format — report the entire pixel payload with
    // LINEARSIZE so loaders at least size their buffer. Pre-#594
    // behaviour for formats not on either list.
    (total_bytes as u32, DDSD_LINEARSIZE)
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
        // 256×256 BC1: 64×64 blocks × 8 bytes = 32768. Block-compressed
        // → DDSD_LINEARSIZE.
        let (size, flag) = pitch_or_linear_size_for(71, 256, 256, 0);
        assert_eq!(size, 32768);
        assert_eq!(flag, DDSD_LINEARSIZE);
    }

    #[test]
    fn linear_size_bc7_512x256() {
        // 512×256 BC7: 128×64 blocks × 16 bytes = 131072.
        let (size, flag) = pitch_or_linear_size_for(98, 512, 256, 0);
        assert_eq!(size, 131072);
        assert_eq!(flag, DDSD_LINEARSIZE);
    }

    #[test]
    fn linear_size_unknown_format_uses_total() {
        // Unknown format falls back to legacy LINEARSIZE + total byte
        // payload so malformed inputs still produce a readable DDS.
        let (size, flag) = pitch_or_linear_size_for(0, 128, 128, 9999);
        assert_eq!(size, 9999);
        assert_eq!(flag, DDSD_LINEARSIZE);
    }

    /// Regression for #594 (FO4-DIM2-03) — uncompressed DXGI formats
    /// must report row pitch + DDSD_PITCH rather than LinearSize +
    /// DDSD_LINEARSIZE. `pitchOrLinearSize` encodes `width * bpp` (one
    /// row of pixels), not `width * height * bpp`. Pre-fix the helper
    /// bucketed every non-BC format into the legacy fallback, emitting
    /// an invalid DDS for vanilla FO4 UI / mask textures that
    /// texconv / DirectXTex / Paint.NET DDS plugin rejected.
    #[test]
    fn pitch_rgba8_unorm_matches_row_size_with_pitch_flag() {
        // 256×128 R8G8B8A8_UNORM (28): row pitch = 256 * 4 = 1024.
        let (pitch, flag) = pitch_or_linear_size_for(28, 256, 128, 0);
        assert_eq!(pitch, 1024);
        assert_eq!(flag, DDSD_PITCH);
    }

    #[test]
    fn pitch_bgra8_unorm_srgb_matches_row_size_with_pitch_flag() {
        // 512×256 B8G8R8A8_UNORM_SRGB (91): row pitch = 512 * 4 = 2048.
        let (pitch, flag) = pitch_or_linear_size_for(91, 512, 256, 0);
        assert_eq!(pitch, 2048);
        assert_eq!(flag, DDSD_PITCH);
    }

    #[test]
    fn pitch_r16_unorm_matches_row_size_with_pitch_flag() {
        // 128×128 R16_UNORM (56): row pitch = 128 * 2 = 256.
        let (pitch, flag) = pitch_or_linear_size_for(56, 128, 128, 0);
        assert_eq!(pitch, 256);
        assert_eq!(flag, DDSD_PITCH);
    }

    #[test]
    fn pitch_r8_unorm_matches_row_size_with_pitch_flag() {
        // 64×64 R8_UNORM (61): row pitch = 64 * 1 = 64.
        let (pitch, flag) = pitch_or_linear_size_for(61, 64, 64, 0);
        assert_eq!(pitch, 64);
        assert_eq!(flag, DDSD_PITCH);
    }

    /// Integration: the emitted DDS header for an uncompressed format
    /// must set DDSD_PITCH (bit 0x8) in dwFlags and write the row
    /// pitch into `dwPitchOrLinearSize`. Guards the seam between
    /// `build_dds_header` and `pitch_or_linear_size_for` — pre-#594
    /// the flag was hardcoded to LINEARSIZE regardless of format.
    #[test]
    fn build_dds_header_uses_pitch_flag_for_uncompressed_rgba() {
        let hdr = build_dds_header(28, 256, 128, 1, false, &[]);
        let flags = u32::from_le_bytes(hdr[8..12].try_into().unwrap());
        assert_eq!(
            flags & DDSD_PITCH,
            DDSD_PITCH,
            "DDSD_PITCH must be set for uncompressed DXGI format 28 (flags=0x{:08x})",
            flags
        );
        assert_eq!(
            flags & DDSD_LINEARSIZE,
            0,
            "DDSD_LINEARSIZE must NOT be set alongside DDSD_PITCH (flags=0x{:08x})",
            flags
        );
        // dwPitchOrLinearSize at offset 20 should be the row pitch.
        let pitch = u32::from_le_bytes(hdr[20..24].try_into().unwrap());
        assert_eq!(pitch, 256 * 4, "uncompressed RGBA pitch = width * 4 bpp");
    }

    /// Sibling: block-compressed formats keep LINEARSIZE semantics
    /// after the refactor. Without this guard a careless swap of the
    /// pitch_or_linear_size_for branches would silently invert the
    /// flag for BC1-BC7 textures.
    #[test]
    fn build_dds_header_keeps_linearsize_flag_for_bc_formats() {
        let hdr = build_dds_header(71, 256, 256, 1, false, &[]);
        let flags = u32::from_le_bytes(hdr[8..12].try_into().unwrap());
        assert_eq!(flags & DDSD_LINEARSIZE, DDSD_LINEARSIZE);
        assert_eq!(flags & DDSD_PITCH, 0);
    }

    /// Regression for #597 (FO4-DIM2-07) — when a BA2 DX10 record
    /// declares `num_mips = 0` (malformed but observed in third-party
    /// repacked archives), the synthesized DDS header must:
    ///   1. Clear `DDSD_MIPMAPCOUNT` (bit 0x20000) in `flags`.
    ///   2. Clear `DDSCAPS_MIPMAP` (bit 0x400000) in `caps1`.
    ///   3. Write `dwMipMapCount = 1` (DDS loaders must treat the
    ///      texture as single-mip regardless of the field value when
    ///      the flag is cleared — the `.max(1)` clamp keeps the field
    ///      and the flag self-consistent).
    /// The `warn!` at record-read time is orthogonal to this header
    /// shape and is exercised indirectly via runtime log capture — not
    /// tested here because setting up `log` in a unit test drags in a
    /// global logger.
    #[test]
    fn build_dds_header_clamps_num_mips_zero_to_one_and_clears_mip_flags() {
        let hdr = build_dds_header(71, 128, 128, 0, false, &[]);
        assert_eq!(hdr.len(), 148);

        // DDSD_MIPMAPCOUNT = 0x20000. `flags` lives at bytes 8..12.
        let flags = u32::from_le_bytes(hdr[8..12].try_into().unwrap());
        assert_eq!(
            flags & 0x20000,
            0,
            "num_mips=0 must NOT set DDSD_MIPMAPCOUNT (flags=0x{:08x})",
            flags
        );

        // DDSCAPS_MIPMAP = 0x400000. `caps1` lives at bytes 108..112.
        let caps1 = u32::from_le_bytes(hdr[108..112].try_into().unwrap());
        assert_eq!(
            caps1 & 0x400000,
            0,
            "num_mips=0 must NOT set DDSCAPS_MIPMAP (caps1=0x{:08x})",
            caps1
        );

        // `dwMipMapCount` lives at bytes 28..32 (see header layout).
        let mip_count = u32::from_le_bytes(hdr[28..32].try_into().unwrap());
        assert_eq!(
            mip_count, 1,
            "num_mips=0 must clamp dwMipMapCount to 1 (got {})",
            mip_count
        );
    }

    /// Sibling: `num_mips = 1` (the vanilla single-mip baseline) must
    /// produce the same header shape as the `num_mips = 0` clamp —
    /// the spec treats them identically when `DDSD_MIPMAPCOUNT` is
    /// cleared. Guards against a future change that special-cases 0.
    #[test]
    fn build_dds_header_num_mips_one_matches_zero_clamp() {
        let hdr_zero = build_dds_header(71, 128, 128, 0, false, &[]);
        let hdr_one = build_dds_header(71, 128, 128, 1, false, &[]);
        assert_eq!(hdr_zero, hdr_one);
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

    /// Regression: #593 / FO4-DIM2-02 — synthesized DDS headers for
    /// cubemap entries must declare `arraySize = 6` per the
    /// DDS_HEADER_DXT10 spec (cubemaps store the 6 face slices as a
    /// 6-element array, optionally a multiple thereof for cube
    /// arrays). Pre-fix the field was hardcoded to `1` regardless of
    /// `is_cubemap` — DXGI loaders reject "arraySize must be a
    /// multiple of 6" on cubemap miscFlag inputs. The in-engine
    /// renderer's `dds.rs` is lenient so this only burns external
    /// tooling (DirectXTex, texconv) but the spec contract is now
    /// correct either way.
    ///
    /// `arraySize` lives at offset 140 in the synthesized header
    /// (84 byte DDS_HEADER + 4 byte FourCC + 16 + 4 + 4 = 144 ... err
    /// the layout: DDS_HEADER through dwReserved2 = 128 bytes, then
    /// DX10 extended starts: dxgi_format (128–131), resource_dim
    /// (132–135), miscFlag (136–139), arraySize (140–143),
    /// miscFlags2 (144–147)).
    #[test]
    fn build_dds_header_cubemap_array_size_is_six() {
        let cubemap = build_dds_header(71, 128, 128, 1, true, &[]);
        let plain = build_dds_header(71, 128, 128, 1, false, &[]);
        assert_eq!(cubemap.len(), 148);
        assert_eq!(plain.len(), 148);

        // arraySize at offset 140.
        let cube_array_size = u32::from_le_bytes(cubemap[140..144].try_into().unwrap());
        let plain_array_size = u32::from_le_bytes(plain[140..144].try_into().unwrap());
        assert_eq!(
            cube_array_size, 6,
            "DDS_HEADER_DXT10.arraySize must be 6 for cubemaps (#593)",
        );
        assert_eq!(
            plain_array_size, 1,
            "DDS_HEADER_DXT10.arraySize must be 1 for non-cubemaps",
        );

        // Sanity: miscFlag at offset 136 must declare TEXTURECUBE
        // (0x4) on the cubemap path and 0 on the plain path. Locks
        // the cubemap-bit-source contract alongside arraySize so a
        // refactor can't accidentally route one path through both
        // branches.
        let cube_misc = u32::from_le_bytes(cubemap[136..140].try_into().unwrap());
        let plain_misc = u32::from_le_bytes(plain[136..140].try_into().unwrap());
        assert_eq!(cube_misc, 0x4, "miscFlag must set D3D10_MISC_TEXTURECUBE");
        assert_eq!(plain_misc, 0, "miscFlag must be 0 on non-cubemaps");
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

    /// Regression for #812 / FO4-D2-NEW-02: a zlib stream whose
    /// actual decompressed length differs from the record's declared
    /// `unpacked_size` returns the actual decompressed bytes (lenient
    /// path, matches openMW's `Z_OK + short return`-style fallback)
    /// AND emits a warn-level diagnostic. Pre-#812 the diagnostic was
    /// debug-level and invisible in standard logs while the LZ4
    /// branch hard-errored on the same condition.
    ///
    /// This pins the LENIENT-mode behaviour: the buffer length is
    /// what zlib actually decoded, and `decompress_chunk` returns
    /// `Ok` rather than `Err`. Once the strictness toggle from the
    /// fix-sketch's stage 3 lands (gated on #598), a sibling test
    /// will pin the `Err(InvalidData)` path.
    #[test]
    fn decompress_chunk_zlib_short_stream_returns_actual_length() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        // Compress 20 bytes; declare unpacked_size = 100 to force
        // the size-mismatch branch.
        let actual_payload = b"twenty-bytes-payloadx";
        assert_eq!(actual_payload.len(), 21, "fixture sanity check");
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(actual_payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let result = decompress_chunk(&compressed, 100, Ba2Compression::Zlib)
            .expect("lenient mode: short zlib stream must NOT error");
        // Buffer length is what zlib actually decoded, NOT the
        // declared `unpacked_size`. Downstream parsers (NIF, DDS)
        // see the actual short payload — exactly the gap #812
        // surfaces. The warn-level log is the operator-visible
        // signal that this happened (not directly assertable here
        // without a test logger; behaviour pinned by the
        // log::warn! call site at decompress_chunk).
        assert_eq!(
            result.len(),
            actual_payload.len(),
            "lenient zlib path returns the actual decoded length, \
             not the record-declared unpacked_size",
        );
        assert_eq!(result.as_slice(), actual_payload);
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

    /// Regression for #755 (SF-DIM2-02) — a v3 BA2 header with an
    /// unrecognised `compression_method` (2 in this case) must return
    /// `InvalidData` immediately at `open()` time, not emit a warn and
    /// proceed to decompress garbage bytes.
    #[test]
    fn v3_unknown_compression_method_rejected() {
        use std::io::Write;
        // Build a minimal v3 BA2 header:
        //   24-byte base + 8-byte v2/v3 extra + 4-byte compression_method
        // name_table_offset points just past those 36 bytes so the seek
        // would succeed even if we got that far (we don't).
        let mut hdr = Vec::with_capacity(36);
        hdr.extend_from_slice(b"BTDX"); // magic
        hdr.extend_from_slice(&3u32.to_le_bytes()); // version = 3 (Starfield DX10)
        hdr.extend_from_slice(b"GNRL"); // type_tag
        hdr.extend_from_slice(&0u32.to_le_bytes()); // file_count = 0
        hdr.extend_from_slice(&36u64.to_le_bytes()); // name_table_offset = 36
        hdr.extend_from_slice(&[0u8; 8]); // 2×u32 unknown (v2/v3 extra)
        hdr.extend_from_slice(&2u32.to_le_bytes()); // compression_method = 2 (unknown)
        assert_eq!(hdr.len(), 36);

        let mut path = std::env::temp_dir();
        path.push(format!(
            "byroredux_ba2_v3_bad_compression_{}.ba2",
            std::process::id()
        ));
        {
            let mut f = std::fs::File::create(&path).expect("create temp BA2");
            f.write_all(&hdr).expect("write header");
        }
        let result = Ba2Archive::open(&path);
        let _ = std::fs::remove_file(&path);
        let err = match result {
            Ok(_) => panic!("unknown compression_method must not be accepted"),
            Err(e) => e,
        };
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let msg = format!("{err}");
        assert!(msg.contains("unsupported compression method"), "got: {msg}");
    }

    /// Regression for #811 (FO4-D2-NEW-01) — a BA2 header with a
    /// version outside the supported allowlist `{1, 2, 3, 7, 8}` must
    /// bail with `InvalidData` at `open()` time. Pre-fix the cascading
    /// `if version == 2 || version == 3` / `if version == 3` arms
    /// silently fell through to the v1 record-layout path for any
    /// other version, including hypothetical future BTDX revisions
    /// that might add header fields, where the reader would either
    /// fail confusingly mid-extract or return corrupted bytes.
    #[test]
    fn unknown_version_rejected() {
        use std::io::Write;
        // Minimal 24-byte BA2 header with version=5 (not in the
        // supported set). file_count=0 keeps the v1-layout fall-through
        // path silent — the version check is the only thing that should
        // trip here.
        let mut hdr = Vec::with_capacity(24);
        hdr.extend_from_slice(b"BTDX"); // magic
        hdr.extend_from_slice(&5u32.to_le_bytes()); // version = 5 (not in {1,2,3,7,8})
        hdr.extend_from_slice(b"GNRL"); // type_tag
        hdr.extend_from_slice(&0u32.to_le_bytes()); // file_count = 0
        hdr.extend_from_slice(&24u64.to_le_bytes()); // name_table_offset = 24
        assert_eq!(hdr.len(), 24);

        let mut path = std::env::temp_dir();
        path.push(format!(
            "byroredux_ba2_unknown_version_{}.ba2",
            std::process::id()
        ));
        {
            let mut f = std::fs::File::create(&path).expect("create temp BA2");
            f.write_all(&hdr).expect("write header");
        }
        let result = Ba2Archive::open(&path);
        let _ = std::fs::remove_file(&path);
        let err = match result {
            Ok(_) => panic!("unsupported BA2 version must not be accepted"),
            Err(e) => e,
        };
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let msg = format!("{err}");
        assert!(
            msg.contains("unsupported BA2 version") && msg.contains("expected 1, 2, 3, 7, or 8"),
            "error message must name the offending version + supported set, got: {msg}"
        );
    }

    /// Regression for #586 (FO4-DIM2-01) — a corrupted / hostile BA2
    /// header with `file_count = u32::MAX` must bail with an
    /// `InvalidData` error BEFORE the reader allocates a
    /// 4-billion-entry `Vec` / `HashMap`. Pre-fix this would abort the
    /// process on 64-bit targets.
    #[test]
    fn malicious_file_count_u32_max_rejected_before_allocation() {
        use std::io::Write;
        // Build a minimal 24-byte BA2 header: BTDX + v1 + GNRL + u32::MAX
        // file count + bogus name-table offset. The reader should hit
        // `checked_entry_count` and return `InvalidData` immediately.
        let mut hdr = Vec::with_capacity(24);
        hdr.extend_from_slice(b"BTDX"); // magic
        hdr.extend_from_slice(&1u32.to_le_bytes()); // version (FO4 original)
        hdr.extend_from_slice(b"GNRL"); // type_tag
        hdr.extend_from_slice(&u32::MAX.to_le_bytes()); // malicious file_count
        hdr.extend_from_slice(&0u64.to_le_bytes()); // name_table_offset
        assert_eq!(hdr.len(), 24);

        // Write to a unique temp path so concurrent test runs don't
        // collide. `env::temp_dir()` is sufficient — we clean up
        // explicitly on both success and failure.
        let mut path = std::env::temp_dir();
        path.push(format!(
            "byroredux_ba2_malicious_{}.ba2",
            std::process::id()
        ));
        {
            let mut f = std::fs::File::create(&path).expect("create temp BA2");
            f.write_all(&hdr).expect("write header");
        }
        let result = Ba2Archive::open(&path);
        let _ = std::fs::remove_file(&path);
        let err = match result {
            Ok(_) => panic!("u32::MAX file_count must not be accepted"),
            Err(e) => e,
        };
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let msg = format!("{err}");
        assert!(msg.contains("file_count"), "got: {msg}");
    }
}
