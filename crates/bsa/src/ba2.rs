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
//!   observed across 129 installed Starfield archives / 50 vanilla,
//!   measured 2026-08-30; installed corpus for the other BTDX games that
//!   same run: FO4 187 archives, FO76 101). v3 adds a `compression_method`
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
//! | 7            | FO4 Next Gen textures               | 24-byte header, zlib only                           |
//! | 8            | FO4 Next Gen Update                 | 24-byte header, zlib only; GNRL **and** DX10 both ship (`Fallout4 - TexturesPatch.ba2` is a real vanilla v8 DX10 archive, not mesh-only) |
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
//!    (no decode). Independent of the codec choice above. The GNRL and
//!    DX10 extract paths both branch on this per chunk (`read_chunk_payload`)
//!    before invoking the codec.
//!
//! Treating compression as "the archive is zlib" or "the archive is LZ4"
//! loses the per-chunk axis — vanilla FO4 archives ship a non-trivial
//! fraction of pre-compressed-too-small or stored-raw chunks, and Starfield
//! v3 DX10 mips mix raw and LZ4-compressed within one texture.
//!
//! # Usage
//!
//! `no_run` rather than `ignore` — see the crate-root docs (#3348).
//! ```no_run
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let archive = byroredux_bsa::Ba2Archive::open("Fallout4 - Meshes.ba2")?;
//!     let bytes = archive.extract("meshes/interiors/desk01.nif")?;
//!     Ok(())
//! }
//! ```

use crate::read_at::ReadAt;
use crate::safety::{
    capacity_hint, checked_chunk_size, checked_chunk_size_usize, checked_chunk_total,
    checked_entry_count,
};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

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

/// BTDX version for FO4 (original release) and FO76.
/// 24-byte header, zlib compression only.
const BA2_V_FO4: u32 = 1;
/// BTDX version for Starfield GNRL+DX10 and FO4 patches.
/// 32-byte header (base + 8 extra bytes), zlib compression.
const BA2_V_STARFIELD_V2: u32 = 2;
/// BTDX version for Starfield DX10 with LZ4 block compression support.
/// 36-byte header (base + 12 extra bytes, including compression_method field).
const BA2_V_STARFIELD_V3: u32 = 3;
/// BTDX version for FO4 Next Gen Update texture archives.
/// 24-byte header, zlib compression only.
const BA2_V_FO4_NEXT_GEN_TEX: u32 = 7;
/// BTDX version for FO4 Next Gen Update archives. Despite the constant's
/// name (kept for historical continuity with the pre-#2596 doc), v8 is not
/// mesh-only in vanilla content — `Fallout4 - TexturesPatch.ba2` is a real
/// v8 **DX10** texture archive (verified against on-disk header bytes,
/// AUDIT_FO4_2026-07-16). 24-byte header, zlib compression only. Variant
/// dispatch (GNRL vs. DX10) is by the archive's own type tag, independent
/// of this version number, so the naming mismatch has no functional effect.
const BA2_V_FO4_NEXT_GEN_MESH: u32 = 8;

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
    /// Read only through positional reads ([`ReadAt`]), so concurrent
    /// extracts share it without a lock.
    file: File,
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
    //
    // `start_mip` is already a live read (the monotonic-order validation
    // below), so it needs no `#[allow(dead_code)]` — only `end_mip` is
    // still write-only, reserved for M40 (#1761/TD8-004).
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
        // #4670 (PAR-D6-2026-09-21-01) — name the field BEFORE anything
        // seeks to it, mirroring the BSA sibling's folders_offset check
        // (#3368, `archive/open.rs`). A truncated download (measured:
        // `cuwp - textures.ba2` declaring 1,250,980,735 in a
        // 214,135,265-byte file) used to seek past EOF and fail the
        // subsequent name-table `read_exact` with a bare "failed to fill
        // whole buffer" that named neither the offset nor the file size —
        // indistinguishable from a reader bug.
        {
            let len = reader.get_ref().metadata()?.len();
            if name_table_offset > len {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "BA2 name_table_offset {name_table_offset} is past end of file ({len} bytes) \
                         — truncated or corrupt archive"
                    ),
                ));
            }
        }

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
        // v3 (Starfield DX10 only in vanilla; no v3 GNRL observed across 129
        //   installed Starfield archives / 50 vanilla, measured 2026-08-30):
        //   +12 bytes (2×u32 unknown + u32 compression
        //   method). Method 0 = zlib, 3 = LZ4 block.
        //
        // #811 / FO4-D2-NEW-01 — match exhaustively over the supported
        // version set so unknown majors (0, 4, 5, 6, 9, ..., u32::MAX)
        // bail with a clear error instead of silently falling through to
        // the v1 record-layout path. Mirrors the BSA reader's allowlist
        // discipline at `archive/open.rs:40-48` (the monolithic
        // `archive.rs` was split into the `archive/` dir under #1118).
        let compression = match version {
            BA2_V_FO4 | BA2_V_FO4_NEXT_GEN_TEX | BA2_V_FO4_NEXT_GEN_MESH => Ba2Compression::Zlib,
            BA2_V_STARFIELD_V2 => {
                let mut extra = [0u8; 8];
                reader.read_exact(&mut extra)?;
                // #1186 — defense-in-depth: log the trailing 2×u32 the
                // community RE'd as "compressed name-table size +
                // reserved" so a malformed archive failure surfaces here
                // (header boundary) instead of 50 records deep in
                // read_general_records. Mirrors the `0xBAADF00D` padding
                // debug-log inside read_general_records.
                log_v2_v3_extra_bytes("v2", &extra, name_table_offset, reader.stream_position()?);
                Ba2Compression::Zlib
            }
            BA2_V_STARFIELD_V3 => {
                let mut extra = [0u8; 8];
                reader.read_exact(&mut extra)?;
                let mut method_buf = [0u8; 4];
                reader.read_exact(&mut method_buf)?;
                // #2360 / SF-BA2-02 — log AFTER the compression-method read,
                // not before it. The point of `stream_pos` in this diagnostic
                // is "where does the header end", to be compared against
                // `name_table_offset`; capturing it before the 4-byte method
                // field reported 32 bytes in rather than the true 36-byte v3
                // header end, understating the boundary by exactly that field.
                // The v2 arm above is already correct by construction — v2 has
                // nothing left to read at that point, so its capture site *is*
                // the header end.
                log_v2_v3_extra_bytes("v3", &extra, name_table_offset, reader.stream_position()?);
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
        // #4661 — sized off the records actually read, not the header's
        // `file_count`: `files.len()` equals it here, but only because every
        // one of those records was really present in the file.
        let mut names = Vec::with_capacity(files.len());
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

        let mut map = HashMap::with_capacity(files.len());
        // #4671 — count duplicate names instead of silently letting
        // `HashMap::insert` make them last-wins; logged once per archive
        // (the #3637 shadow-count precedent, mirrored on the BSA side).
        let mut duplicate_names = 0usize;
        for (name, entry) in names.into_iter().zip(files) {
            if map.insert(name, entry).is_some() {
                duplicate_names += 1;
            }
        }
        if duplicate_names > 0 {
            log::warn!(
                "BA2: {} duplicate file name(s) — last record wins. Distinct \
                 files: {} of {} declared.",
                duplicate_names,
                map.len(),
                file_count,
            );
        }

        // Take ownership of the file handle for reuse across extracts
        // — the same #360 rationale as `BsaArchive`. BufReader was right
        // for the sequential header parse above; the random-access
        // extract path uses positional reads on the bare File.
        let file = reader.into_inner();
        Ok(Self {
            file,
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

    /// The decoded size the archive declares for `path`, without extracting
    /// it — straight from the file table: a GNRL record's unpacked size, or
    /// a DX10 record's synthesized DDS header plus its chunks' unpacked
    /// sizes. Lets a caller budget memory before paying for the read +
    /// inflate. A chunk whose decode runs short (tolerated, see
    /// `decompress_chunk`) extracts a few bytes under this.
    pub fn declared_size(&self, path: &str) -> io::Result<usize> {
        let key = normalize_path(path);
        let entry = self.files.get(&key).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("file not found in BA2: {}", path),
            )
        })?;
        Ok(match entry {
            Ba2Entry::General { unpacked_size, .. } => *unpacked_size as usize,
            Ba2Entry::Dx10 {
                dxgi_format,
                width,
                height,
                num_mips,
                is_cubemap,
                chunks,
            } => {
                // The header's length depends only on the format fields;
                // the pixel data feeds nothing but its pitch value.
                build_dds_header(*dxgi_format, *width, *height, *num_mips, *is_cubemap, &[]).len()
                    + chunks
                        .iter()
                        .map(|chunk| chunk.unpacked_size as usize)
                        .sum::<usize>()
            }
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

        // Positional reads against the long-lived handle (#360): no
        // cursor, so no lock, and each extract inflates on its own thread
        // without waiting on another's read (#3659 kept the old mutex off
        // the inflate; this removes it from the read too).
        match entry {
            Ba2Entry::General {
                offset,
                packed_size,
                unpacked_size,
            } => finish_chunk_payload(
                read_chunk_payload(&self.file, *offset, *packed_size, *unpacked_size)?,
                self.compression,
                path,
            ),
            Ba2Entry::Dx10 {
                dxgi_format,
                width,
                height,
                num_mips,
                is_cubemap,
                chunks,
            } => finish_dx10_payload(
                Dx10TexInfo {
                    dxgi_format: *dxgi_format,
                    width: *width,
                    height: *height,
                    num_mips: *num_mips,
                    is_cubemap: *is_cubemap,
                },
                read_dx10_chunk_payloads(&self.file, chunks)?,
                self.compression,
                path,
            ),
        }
    }
}

/// Defense-in-depth header sanity log for the Starfield v2 / v3 trailing
/// 8 bytes. Originally community-RE'd as `compressed_name_table_size: u32`
/// followed by `reserved: u32`, but #2629 / SF-D1-04 found all 129 sampled vanilla
/// archives byte-identical on this field (`0100000000000000` LE — i.e.
/// `unknown_1 == 1`, `unknown_2 == 0`): a constant tag, not a size. The
/// original "compressed name-table size" reading was never actually
/// exercised — the `stream_pos + size > name_table_offset` malformed-
/// header heuristic built on it was dead code (a constant `1` can never
/// project past `name_table_offset`) and has been dropped. Logs both
/// fields at trace level purely as a header-boundary sanity record, so a
/// future archive that DOES vary here surfaces at the header boundary
/// instead of 50 records deep inside `read_general_records` (where the
/// symptom is the confusing `failed to fill whole buffer`).
///
/// Models the `padding != 0xBAADF00D` debug-log inside
/// [`read_general_records`] — cheap header-boundary sanity check.
/// See #1186, #2629.
fn log_v2_v3_extra_bytes(label: &str, extra: &[u8; 8], name_table_offset: u64, stream_pos: u64) {
    let unknown_1 = u32::from_le_bytes(extra[0..4].try_into().unwrap());
    let unknown_2 = u32::from_le_bytes(extra[4..8].try_into().unwrap());
    log::trace!(
        "BA2 {} extra header bytes: unknown_1={} unknown_2={:#x} \
         (stream_pos={:#x}, name_table_offset={:#x})",
        label,
        unknown_1,
        unknown_2,
        stream_pos,
        name_table_offset,
    );
    if unknown_1 != 1 || unknown_2 != 0 {
        log::debug!(
            "BA2 {} extra header bytes deviate from the observed-constant \
             {{unknown_1=1, unknown_2=0}}: unknown_1={} unknown_2={:#x} \
             (stream_pos={:#x}, name_table_offset={:#x}); worth a closer \
             look, though the format may simply vary here",
            label,
            unknown_1,
            unknown_2,
            stream_pos,
            name_table_offset,
        );
    }
}

/// Bytes left in the file after the reader's current position — the
/// ceiling on how many records a declared count can really describe.
fn remaining_bytes(reader: &mut BufReader<File>) -> io::Result<u64> {
    let len = reader.get_ref().metadata()?.len();
    Ok(len.saturating_sub(reader.stream_position()?))
}

/// #4661 — name the record that ran out. A bare `read_exact` failure
/// ("failed to fill whole buffer") named neither the record kind, its
/// index, nor the declared count, so a lying `file_count` was
/// indistinguishable from a reader bug. The kind is kept so callers still
/// see `UnexpectedEof`.
fn truncated_record(e: io::Error, what: &str, index: usize, count: usize) -> io::Error {
    io::Error::new(
        e.kind(),
        format!("BA2 {what} {index} of {count} declared: {e} — truncated or corrupt archive"),
    )
}

/// Read `count` 36-byte GNRL file records.
fn read_general_records(reader: &mut BufReader<File>, count: usize) -> io::Result<Vec<Ba2Entry>> {
    // `count` was already capped by `checked_entry_count` at the header
    // parse site; #4661 additionally bounds the reservation by the bytes
    // the file has left for 36-byte records.
    let mut out = Vec::with_capacity(capacity_hint(count, remaining_bytes(reader)?, 36));
    let mut rec = [0u8; 36];
    for i in 0..count {
        reader
            .read_exact(&mut rec)
            .map_err(|e| truncated_record(e, "GNRL file record", i, count))?;
        // rec[0..4]   name_hash
        // rec[4..8]   ext
        // rec[8..12]  dir_hash
        // rec[12..16] flags
        let offset = u64::from_le_bytes(rec[16..24].try_into().unwrap());
        let packed_size = u32::from_le_bytes(rec[24..28].try_into().unwrap());
        let unpacked_size = u32::from_le_bytes(rec[28..32].try_into().unwrap());
        // #586 — reject obviously-hostile sizes at record-read time so
        // `extract` never has to trust them. Vanilla FO4 GNRL entries
        // top out around 8 MB decompressed; `MAX_CHUNK_BYTES` (1 GB,
        // widened by `4a2b8200` to fit FO76 content) is a comfortable
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
    // `checked_entry_count`; #4661 additionally bounds the reservation by
    // the bytes left for records of at least the 24-byte base header.
    let mut out = Vec::with_capacity(capacity_hint(count, remaining_bytes(reader)?, 24));
    for i in 0..count {
        let mut base = [0u8; 24];
        reader
            .read_exact(&mut base)
            .map_err(|e| truncated_record(e, "DX10 file record", i, count))?;
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
        let chunk_hdr_len = u16::from_le_bytes(base[14..16].try_into().unwrap());
        // #1079 / FO4-D2-009 — every vanilla FO4 DX10 record sets
        // chunk_hdr_len = 24 (matches the 24-byte chunk struct decoded in
        // the per-chunk loop below). A different value would indicate a
        // future format extension or a corrupt archive, and the reader
        // cannot parse any other chunk stride — the loop below reads a
        // fixed 24-byte chunk regardless of the parsed value, so
        // continuing would misparse every following chunk.
        // #5008 / PAR-D2-2026-09-29-01 — this used to be a
        // `debug_assert` + release `warn!` (#1825), which made a debug
        // build panic at boot on file-controlled bytes; the reader
        // contract is "malformed bytes give Err, never a panic", so the
        // malformed stride is now a hard `InvalidData` in every build.
        if chunk_hdr_len != 24 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "BA2 DX10 record {i}/{count} declares chunk_hdr_len={chunk_hdr_len} \
                     (expected 24) — unknown variant or corrupt archive; the reader \
                     cannot parse any other chunk stride"
                ),
            ));
        }
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
        // here is inherently bounded and needs no extra check. The
        // per-chunk sizes below are each capped individually, but
        // `packed_total`/`unpacked_total` additionally cap the *sum*
        // across the whole chunk list — see #2356 (SF-BA2-01).
        let mut chunks = Vec::with_capacity(num_chunks);
        let mut packed_total = 0usize;
        let mut unpacked_total = 0usize;
        for _ in 0..num_chunks {
            let mut chunk = [0u8; 24];
            reader
                .read_exact(&mut chunk)
                .map_err(|e| truncated_record(e, "DX10 chunk header of file record", i, count))?;
            let offset = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
            let packed_size = u32::from_le_bytes(chunk[8..12].try_into().unwrap());
            let unpacked_size = u32::from_le_bytes(chunk[12..16].try_into().unwrap());
            // #586 — cap DX10 chunk sizes at record-read time.
            checked_chunk_size(packed_size, "BA2 DX10 chunk packed_size")?;
            checked_chunk_size(unpacked_size, "BA2 DX10 chunk unpacked_size")?;
            // #2356 — cap the running sum across this record's chunk
            // list; a per-chunk cap alone lets up to 255 near-1 GiB
            // chunks through per texture.
            packed_total = checked_chunk_total(
                packed_total,
                packed_size as usize,
                "BA2 DX10 record packed_size",
            )?;
            unpacked_total = checked_chunk_total(
                unpacked_total,
                unpacked_size as usize,
                "BA2 DX10 record unpacked_size",
            )?;
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

        // #1176 / FO4-D2-NEW-01 — DX10 chunks are concatenated in file
        // order during `extract_dx10`; the synthesized DDS header
        // declares dimensions matching `mip 0`, so the payload MUST
        // start at the largest mip and walk downward. Vanilla FO4 /
        // FO76 / Starfield archives always author in canonical
        // mip-0-first order (`start_mip` monotonically non-decreasing).
        // A hand-crafted or third-party-repacked archive that wrote
        // chunks in non-canonical order (e.g. streaming-mip-tail-first)
        // would silently produce a DDS whose header and pixel data
        // disagreed — downstream loaders read garbage.
        //
        // Surface the anomaly as `warn!` so an operator can spot the bad
        // archive in the logs. Don't auto-sort — that would mask the
        // malformed archive. Unlike the `chunk_hdr_len` stride above,
        // the record itself still parses correctly, so this stays
        // tolerant. #5008 / PAR-D2-2026-09-29-01 removed the
        // `debug_assert!` that used to accompany it: the field is
        // file-controlled, and a debug build must not panic at boot
        // where release only warns (reader contract: Err, never panic).
        let monotonic = chunks.windows(2).all(|w| w[0].start_mip <= w[1].start_mip);
        if !monotonic {
            log::warn!(
                "BA2 DX10 record at chunk 0x{:016x}: chunk start_mip \
                 sequence is non-monotonic ({:?}) — archive likely \
                 third-party-repackaged; downstream DDS payload will \
                 mismatch the synthesized header",
                reader.stream_position().unwrap_or(0),
                chunks.iter().map(|c| c.start_mip).collect::<Vec<_>>(),
            );
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
/// `path` is the archive entry being extracted — carried only so the
/// diagnostics below name it (#4662), mirroring the BSA sibling.
fn decompress_chunk(
    packed: &[u8],
    unpacked_size: usize,
    compression: Ba2Compression,
    path: &str,
) -> io::Result<Vec<u8>> {
    // Defense-in-depth: entries that reached this function already had
    // `unpacked_size` capped at record-read time, but `decompress_chunk`
    // is also reachable from the test harness with arbitrary inputs.
    // Re-check so a direct caller can't bypass the safety net. #586.
    let unpacked_size = checked_chunk_size_usize(unpacked_size, "BA2 decompress unpacked_size")?;
    match compression {
        Ba2Compression::Zlib => {
            // #3410 — bounded inflate. `unpacked_size` was validated above and
            // then used only as a capacity hint; `read_to_end` would grow past
            // it without limit. Under-runs keep the lenient `Ok` + warn below
            // (#812 / #2618); over-runs are rejected. Sibling of the BSA-side
            // fix in `archive/extract.rs`.
            // #3812 — the zlib-specific sibling, so a checksum-only
            // failure can retry as raw DEFLATE (#3720's recovery, ported).
            let buf = crate::safety::inflate_bounded_zlib(
                packed,
                unpacked_size,
                &format!("BA2 zlib chunk of '{path}'"),
            )?;
            if buf.len() != unpacked_size {
                // #812 / FO4-D2-NEW-02 — `read_to_end` honours deflate's
                // self-terminating end-of-stream marker mid-buffer so a
                // truncated archive returns a short blob. #2618 / SF-D1-01
                // corrected the LZ4 arm below to warn on the identical
                // condition too — pre-#2618 this comment claimed LZ4
                // "hard-errors on the same condition," which was
                // factually wrong: `lz4_flex::block::decompress` only
                // hard-errors when the block decodes to MORE than
                // `unpacked_size` (an over-run); a short decode
                // (under-run) silently truncates and returns `Ok`, same
                // as this zlib branch. Promoted from `log::debug!` so
                // operators see the mismatch in standard log output
                // without changing the lenient semantics — a synthetic
                // `unpacked_size = 100 / actual stream = 20` archive
                // currently pivots into the NIF / DDS parser at the
                // wrong size with no signalling above debug-log noise.
                // Sibling of #622's BSA-side hardening (SK-D2-04). The
                // optional `Strict` mode that would convert this to an
                // `InvalidData` error is gated on #598's investigation of
                // vanilla `Fallout4 - Meshes.ba2`'s `packed_size >
                // unpacked_size` anomaly.
                log::warn!(
                    "BA2 zlib decompressed {} bytes but record declared {} for '{}'",
                    buf.len(),
                    unpacked_size,
                    path,
                );
            }
            Ok(buf)
        }
        Ba2Compression::Lz4Block => {
            // #2097 / LZ4-01 — `lz4_flex::block::decompress`'s own docs say it
            // "may panic" when the `min_uncompressed_size` hint undershoots the
            // true decompressed size. Empirical fuzzing (constructed payloads,
            // undersized from 1 byte down to 0) found zero panics on the
            // then-pinned 0.11.6, and the behavioural test below keeps
            // checking the current pin on every run.
            //
            // #3392 — that absence is a property of the `safe-decode` FEATURE
            // (pinned on at `Cargo.toml`'s `lz4_flex` entry, and still the
            // default in 0.14.0), not of a version: with it on, an undersized
            // hint comes back as `Err(OutputTooSmall)` — the behaviour the
            // test pins — so the documented panic is not reachable on the
            // shipped configuration. The feature is the actual mitigation for
            // the undersized-hint case — NOT this guard.
            //
            // This `catch_unwind` is kept as cheap defence-in-depth for any
            // residual panic on the safe path (and for a future dependency bump
            // that changes the internal discipline within its still-compatible
            // public contract). Note it would NOT save us if `safe-decode` were
            // ever switched off: that path writes through raw pointers with no
            // capacity check, so the failure would be a heap overflow, not an
            // unwind. Archive bytes are attacker-controlled for modded content.
            //
            // Converting the unwind into the `Err` path this arm already has
            // costs nothing on the success path and makes the failure mode
            // match the zlib arm's. `AssertUnwindSafe` is sound here:
            // `packed` is a shared slice we do not mutate, and the closure
            // returns the decoded buffer by value, so a panic leaves no
            // half-updated state behind for a later observer.
            let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                lz4_flex::block::decompress(packed, unpacked_size)
            }))
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "BA2 LZ4 block decompression of '{path}' panicked (malformed \
                         chunk; see #2097 — lz4_flex documents `decompress` as \
                         may-panic when the size hint undershoots)"
                    ),
                )
            })?;
            let buf = decoded.map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("BA2 LZ4 block decompression of '{}' failed: {}", path, e),
                )
            })?;
            // #2618 / SF-D1-01 — mirror the zlib arm's mismatch warning.
            // `lz4_flex::block::decompress`'s `unpacked_size` parameter is a
            // hard output bound on the `safe-decode` build this workspace pins
            // (`vec![0; n]` + bounds-checked `SliceSink`, then `truncate` to
            // the true length — #3392; the "capacity hint / `Vec::with_capacity`"
            // description this comment used to carry belongs to the unsafe
            // module we do not compile). The observable behaviour is unchanged:
            // when the block actually decodes to FEWER bytes (an under-run —
            // declared size larger than actual), the function silently returns
            // the shorter buffer with no error, unlike an over-run (more
            // bytes than declared), which it DOES hard-error on. LZ4 is
            // the only codec for every Starfield v3 texture archive, and
            // a DX10 texture is a concatenation of per-mip chunks — a
            // short decode on a non-final chunk shifts every subsequent
            // mip, and the synthesized DDS header then misdescribes its
            // own payload. Pre-#2618 there was no signal above
            // debug-log noise; this promotes it to the same visibility
            // level as the zlib arm above.
            if buf.len() != unpacked_size {
                log::warn!(
                    "BA2 LZ4 decompressed {} bytes but record declared {} for '{}' \
                     (under-run — lz4_flex silently truncates rather than erroring in \
                     this direction)",
                    buf.len(),
                    unpacked_size,
                    path,
                );
            }
            Ok(buf)
        }
    }
}

enum ChunkPayload {
    Raw(Vec<u8>),
    Compressed {
        bytes: Vec<u8>,
        unpacked_size: usize,
    },
}

fn read_chunk_payload<R: ReadAt + ?Sized>(
    reader: &R,
    offset: u64,
    packed_size: u32,
    unpacked_size: u32,
) -> io::Result<ChunkPayload> {
    if packed_size == 0 {
        let mut buf = vec![0u8; unpacked_size as usize];
        reader.read_exact_at(&mut buf, offset)?;
        Ok(ChunkPayload::Raw(buf))
    } else {
        let mut packed = vec![0u8; packed_size as usize];
        reader.read_exact_at(&mut packed, offset)?;
        Ok(ChunkPayload::Compressed {
            bytes: packed,
            unpacked_size: unpacked_size as usize,
        })
    }
}

fn finish_chunk_payload(
    payload: ChunkPayload,
    compression: Ba2Compression,
    path: &str,
) -> io::Result<Vec<u8>> {
    match payload {
        ChunkPayload::Raw(bytes) => Ok(bytes),
        ChunkPayload::Compressed {
            bytes,
            unpacked_size,
        } => decompress_chunk(&bytes, unpacked_size, compression, path),
    }
}

/// The DX10 texture-header fields needed to synthesize a DDS header for
/// a BA2 DX10 entry. Bundled so `extract_dx10` stays within the argument
/// budget (`clippy::too_many_arguments`); mirrors the `Ba2Entry::Dx10`
/// header fields one-to-one.
struct Dx10TexInfo {
    dxgi_format: u8,
    width: u16,
    height: u16,
    num_mips: u8,
    is_cubemap: bool,
}

#[cfg(test)]
fn extract_dx10<R: ReadAt + ?Sized>(
    reader: &R,
    info: Dx10TexInfo,
    chunks: &[Dx10Chunk],
    compression: Ba2Compression,
    path: &str,
) -> io::Result<Vec<u8>> {
    let payloads = read_dx10_chunk_payloads(reader, chunks)?;
    finish_dx10_payload(info, payloads, compression, path)
}

fn read_dx10_chunk_payloads<R: ReadAt + ?Sized>(
    reader: &R,
    chunks: &[Dx10Chunk],
) -> io::Result<Vec<ChunkPayload>> {
    chunks
        .iter()
        .map(|chunk| {
            read_chunk_payload(reader, chunk.offset, chunk.packed_size, chunk.unpacked_size)
        })
        .collect()
}

fn finish_dx10_payload(
    info: Dx10TexInfo,
    chunks: Vec<ChunkPayload>,
    compression: Ba2Compression,
    path: &str,
) -> io::Result<Vec<u8>> {
    let mut pixel_data = Vec::new();
    let last = chunks.len().saturating_sub(1);
    for (i, chunk) in chunks.into_iter().enumerate() {
        let declared = match &chunk {
            ChunkPayload::Raw(bytes) => bytes.len(),
            ChunkPayload::Compressed { unpacked_size, .. } => *unpacked_size,
        };
        let decoded = finish_chunk_payload(chunk, compression, path)?;
        // #4662 — chunks are concatenated back to back under one synthesized
        // header, so a non-final chunk that decodes short shifts every later
        // mip onto the wrong bytes. The renderer's too-short-payload guard
        // (#4511) only catches that when nothing over-delivers to compensate,
        // and then blames the DDS, not this archive. Reject it here, naming
        // the entry — the CSG reader does the same (#1986). A short FINAL
        // chunk shifts nothing and keeps the lenient warn-only path
        // `decompress_chunk` already logs. Measured 2026-09-28 by extracting
        // all 475,421 entries of the 137 installed FO4 / FO76 / Starfield DX10
        // archives: zero rejections.
        if i != last && decoded.len() != declared {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "BA2 DX10 '{path}': chunk {i} of {} decoded {} bytes but declared {declared} \
                     — every later mip would shift under the synthesized DDS header",
                    last + 1,
                    decoded.len(),
                ),
            ));
        }
        pixel_data.extend_from_slice(&decoded);
    }
    let mut dds = build_dds_header(
        info.dxgi_format,
        info.width,
        info.height,
        info.num_mips,
        info.is_cubemap,
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
    // a multiple of 6". The in-engine renderer's `dds.rs` accepts 1 or 6
    // for a cubemap (and requires 1 otherwise), so this is observable only
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
        // #4656 — widen before multiplying. Both dimensions are file-controlled
        // u16s: 16384 blocks a side × 16 bytes/block is exactly 2^32, so a
        // crafted 65535² BC7 record overflowed `u32` here — a debug-build panic
        // on the main thread at extract time, a silent wrap in release. The DDS
        // field is only 32 bits wide, so saturate: a size that cannot be
        // represented is reported as the largest one that can (loaders that
        // read this legacy field at all only size a buffer from it; the
        // engine's own `dds.rs` recomputes mip sizes and ignores it).
        let bw = u64::from(width.div_ceil(4).max(1));
        let bh = u64::from(height.div_ceil(4).max(1));
        let linear_size = u32::try_from(bw * bh * u64::from(bb)).unwrap_or(u32::MAX);
        return (linear_size, DDSD_LINEARSIZE);
    }

    // Uncompressed DXGI formats observed in Bethesda BA2 DX10
    // archives. Bytes-per-pixel per Microsoft's DXGI_FORMAT enum:
    //   28 = R8G8B8A8_UNORM       (4 bpp)
    //   29 = R8G8B8A8_UNORM_SRGB  (4 bpp)
    //   87 = B8G8R8A8_UNORM       (4 bpp) — FO4 normal maps
    //   88 = B8G8R8X8_UNORM       (4 bpp) — BGRX, UFO4P + mods (#1596)
    //   91 = B8G8R8A8_UNORM_SRGB  (4 bpp)
    //   56 = R16_UNORM            (2 bpp) — height / mask textures
    //   61 = R8_UNORM             (1 bpp) — mono masks
    //   10 = R16G16B16A16_FLOAT   (8 bpp) — cubemaps + the LTC area-light
    //                                       LUT (#2628 / SF-D1-02)
    //   11 = R16G16B16A16_UNORM   (8 bpp) — gas-giant gradient textures
    //   31 = R8G8B8A8_SNORM       (4 bpp) — chargen face normal maps
    // For uncompressed, the pitch is the byte length of one row of
    // pixels (`width * bpp`) — NOT the total buffer size.
    let bpp: Option<u32> = match dxgi_format {
        28 | 29 | 87 | 88 | 91 => Some(4),
        56 => Some(2),
        61 => Some(1),
        10 | 11 => Some(8),
        31 => Some(4),
        _ => None,
    };
    if let Some(b) = bpp {
        // Cannot overflow: `width` came from a u16 and `b` is at most 8.
        return (width * b, DDSD_PITCH);
    }

    // Unknown format — report the entire pixel payload with
    // LINEARSIZE so loaders at least size their buffer. Pre-#594
    // behaviour for formats not on either list. #4656 — saturate rather
    // than truncate a payload past 4 GiB, same as the block-compressed arm.
    (
        u32::try_from(total_bytes).unwrap_or(u32::MAX),
        DDSD_LINEARSIZE,
    )
}

/// Normalize a path for case-insensitive, slash-agnostic lookup.
/// `pub(crate)`: the BSA side reuses it for its assembled keys so both
/// archive formats answer the same queries (#4671).
pub(crate) fn normalize_path(path: &str) -> String {
    path.to_lowercase().replace('/', "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Concurrent extracts from one archive must each read their own
    /// record's bytes. Pins the positional-read contract: with a shared
    /// seek cursor and no lock, interleaved extracts would return another
    /// entry's payload.
    #[test]
    fn concurrent_extracts_read_their_own_entries() {
        const N: u32 = 16;
        let payload = |i: u32| {
            format!("entry {i:02} ")
                .repeat(64 + i as usize)
                .into_bytes()
        };
        let records_end = 24u64 + N as u64 * 36;
        let mut data = Vec::new();
        let mut records = Vec::new();
        for i in 0..N {
            let body = payload(i);
            let mut rec = [0u8; 36];
            rec[0..4].copy_from_slice(&i.to_le_bytes());
            rec[16..24].copy_from_slice(&(records_end + data.len() as u64).to_le_bytes());
            rec[24..28].copy_from_slice(&0u32.to_le_bytes()); // stored raw
            rec[28..32].copy_from_slice(&(body.len() as u32).to_le_bytes());
            rec[32..36].copy_from_slice(&0xBAADF00Du32.to_le_bytes());
            records.extend_from_slice(&rec);
            data.extend_from_slice(&body);
        }
        let name_table_offset = records_end + data.len() as u64;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"BTDX");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(b"GNRL");
        bytes.extend_from_slice(&N.to_le_bytes());
        bytes.extend_from_slice(&name_table_offset.to_le_bytes());
        bytes.extend_from_slice(&records);
        bytes.extend_from_slice(&data);
        for i in 0..N {
            let name = format!("meshes\\e{i:02}.nif");
            bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
            bytes.extend_from_slice(name.as_bytes());
        }

        let path = std::env::temp_dir().join(format!(
            "byroredux_concurrent_ba2_{}_{}.ba2",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::write(&path, &bytes).expect("write temp BA2");
        let archive = Ba2Archive::open(&path).expect("open synthetic BA2");
        let _ = std::fs::remove_file(&path);

        std::thread::scope(|scope| {
            for t in 0..8u32 {
                let archive = &archive;
                scope.spawn(move || {
                    for round in 0..200u32 {
                        let i = (t * 7 + round) % N;
                        let got = archive
                            .extract(&format!("meshes/e{i:02}.nif"))
                            .expect("extract");
                        assert_eq!(got, payload(i), "thread {t} round {round} entry {i}");
                    }
                });
            }
        });
    }

    #[test]
    fn normalize_path_works() {
        assert_eq!(
            normalize_path("Meshes/Interiors/Test.nif"),
            "meshes\\interiors\\test.nif"
        );
        assert_eq!(normalize_path("MESHES\\foo.NIF"), "meshes\\foo.nif");
    }

    /// #4671 (PAR-D6-2026-09-21-02) — two records whose names normalise
    /// to the same key must open successfully with the collision counted
    /// (warn-logged) and last-wins, exactly like the pre-fix behaviour —
    /// the fix makes the collision VISIBLE, not fatal. The dup-scan
    /// census found zero duplicates on 441 installed archives across
    /// eight titles, so this is hand-crafted-content hygiene.
    #[test]
    fn duplicate_ba2_names_open_last_wins_with_a_distinct_count() {
        let names = ["Meshes/a.nif", "meshes/a.nif"];
        let name_table_offset = 24u64 + 2 * 36;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"BTDX");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(b"GNRL");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&name_table_offset.to_le_bytes());
        for i in 0..2u32 {
            let mut rec = [0u8; 36];
            rec[0..4].copy_from_slice(&i.to_le_bytes()); // distinct hashes
            rec[8..12].copy_from_slice(&i.to_le_bytes());
            rec[24..28].copy_from_slice(&0u32.to_le_bytes());
            rec[28..32].copy_from_slice(&0u32.to_le_bytes());
            rec[32..36].copy_from_slice(&0xBAADF00Du32.to_le_bytes());
            bytes.extend_from_slice(&rec);
        }
        for name in names {
            bytes.extend_from_slice(&[name.len() as u8, 0]);
            bytes.extend_from_slice(name.as_bytes());
        }

        let path = std::env::temp_dir().join(format!(
            "byroredux_dup_ba2_{}_{}.ba2",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::write(&path, &bytes).expect("write temp BA2");
        let archive = Ba2Archive::open(&path).expect("duplicate names must still open");
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            archive.file_count(),
            1,
            "two names normalising to one key collapse to one map entry"
        );
        assert!(
            archive.contains("meshes\\a.nif"),
            "the survivor must be reachable under the normalised key"
        );
    }

    /// #4670 (PAR-D6-2026-09-21-01) — a truncated BA2 whose
    /// `name_table_offset` points past EOF must fail with a NAMED error
    /// carrying the offset and the file size, not the bare
    /// "failed to fill whole buffer" the unchecked seek+read_exact
    /// produced. Measured real case: `cuwp - textures.ba2` declaring
    /// 1,250,980,735 in a 214,135,265-byte file.
    #[test]
    fn truncated_ba2_names_the_name_table_offset() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"BTDX");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(b"GNRL");
        bytes.extend_from_slice(&1u32.to_le_bytes()); // file_count
        bytes.extend_from_slice(&1_000_000_000u64.to_le_bytes()); // name_table_offset
        bytes.extend_from_slice(&[0u8; 36]); // one GNRL record (zero hashes/sizes)
        assert_eq!(bytes.len(), 60);

        let path = std::env::temp_dir().join(format!(
            "byroredux_truncated_ba2_{}_{}.ba2",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::write(&path, &bytes).expect("write temp BA2");
        let err = match Ba2Archive::open(&path) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("the truncated archive must not open"),
        };
        let _ = std::fs::remove_file(&path);

        assert!(
            err.contains("name_table_offset"),
            "the error must name the field: {err}"
        );
        assert!(
            err.contains("past end of file") && err.contains("60 bytes"),
            "the error must carry the offset and the real file size: {err}"
        );
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

    /// #4656 — the largest file-controlled dimensions a DX10 record can carry
    /// (u16::MAX²) overflowed the old `u32` product for every 16-byte-block
    /// format and panicked debug builds at extract time. The product now
    /// saturates; sizes that fit are unchanged.
    #[test]
    fn linear_size_saturates_instead_of_overflowing_at_u16_max_dimensions() {
        for fmt in [74u8, 77, 83, 95, 98] {
            let (size, flag) = pitch_or_linear_size_for(fmt, 65535, 65535, 0);
            assert_eq!(size, u32::MAX, "format {fmt}: 16384² blocks × 16 B is 2^32");
            assert_eq!(flag, DDSD_LINEARSIZE);
        }
        // 65532² BC7 is the largest that still fits: 16383² × 16.
        assert_eq!(
            pitch_or_linear_size_for(98, 65532, 65532, 0).0,
            4_294_443_024
        );
        // BC1 (8-byte blocks) at u16::MAX² fits exactly as before.
        assert_eq!(
            pitch_or_linear_size_for(71, 65535, 65535, 0).0,
            2_147_483_648
        );
        // And the whole synthesized header survives it.
        let hdr = build_dds_header(98, 65535, 65535, 1, false, &[]);
        assert_eq!(
            u32::from_le_bytes(hdr[20..24].try_into().unwrap()),
            u32::MAX
        );
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
    fn pitch_bgrx8_unorm_matches_row_size_with_pitch_flag() {
        // 256×256 B8G8R8X8_UNORM (88): BGRX is 4 bpp uncompressed →
        // row pitch = 256 * 4 = 1024, DDSD_PITCH. #1596.
        let (pitch, flag) = pitch_or_linear_size_for(88, 256, 256, 0);
        assert_eq!(pitch, 1024);
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

    /// #2628 (SF-D1-02) — DXGI 10/11/31 fell through to the legacy
    /// `(total_bytes, DDSD_LINEARSIZE)` fallback pre-fix, matching #594's
    /// original defect class on formats that fix never enumerated: 78
    /// vanilla Starfield textures (62 chargen face normal maps, 12
    /// cubemaps + the LTC area-light LUT, 2 gas-giant gradients) got an
    /// invalid `dwPitchOrLinearSize`.
    #[test]
    fn pitch_r16g16b16a16_float_matches_row_size_with_pitch_flag() {
        // 64×64 R16G16B16A16_FLOAT (10): 8 bpp → row pitch = 64 * 8 = 512.
        let (pitch, flag) = pitch_or_linear_size_for(10, 64, 64, 0);
        assert_eq!(pitch, 512);
        assert_eq!(flag, DDSD_PITCH);
    }

    #[test]
    fn pitch_r16g16b16a16_unorm_matches_row_size_with_pitch_flag() {
        // 64×64 R16G16B16A16_UNORM (11): 8 bpp → row pitch = 64 * 8 = 512.
        let (pitch, flag) = pitch_or_linear_size_for(11, 64, 64, 0);
        assert_eq!(pitch, 512);
        assert_eq!(flag, DDSD_PITCH);
    }

    #[test]
    fn pitch_r8g8b8a8_snorm_matches_row_size_with_pitch_flag() {
        // 256×256 R8G8B8A8_SNORM (31): 4 bpp → row pitch = 256 * 4 = 1024.
        let (pitch, flag) = pitch_or_linear_size_for(31, 256, 256, 0);
        assert_eq!(pitch, 1024);
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
    ///
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
    /// renderer's `dds.rs` accepts 1 or 6 for a cubemap, so this only
    /// burns external tooling (DirectXTex, texconv) but the spec contract
    /// is now correct either way.
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

        let result = decompress_chunk(
            &compressed,
            original.len(),
            Ba2Compression::Zlib,
            "test.bin",
        )
        .unwrap();
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

        let result = decompress_chunk(&compressed, 100, Ba2Compression::Zlib, "test.bin")
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

        let result = decompress_chunk(
            &compressed,
            original.len(),
            Ba2Compression::Lz4Block,
            "test.bin",
        )
        .unwrap();
        assert_eq!(result, original.as_slice());
    }

    /// #4267 / SF-DIM1-02 — byte-literal fixture for the documented mixed
    /// raw+LZ4-chunk DX10 record population (measured 3.66% of the real
    /// Starfield corpus), exercised only by the opt-in real-data sweep
    /// until now. Chunk 0 is stored raw (`packed_size == 0`, per-chunk
    /// header-declared compression override); chunk 1 is LZ4-block
    /// compressed — the exact per-chunk branch `extract_dx10`'s loop
    /// takes on `chunk.packed_size == 0` vs `!= 0`. Pins that both
    /// branches concatenate correctly into one contiguous pixel-data
    /// blob under the synthesized DDS header, in one record.
    #[test]
    fn extract_dx10_concatenates_a_raw_chunk_and_an_lz4_chunk_in_one_record() {
        let raw_chunk_payload = b"RAWCHUNK-uncompressed-mip-bytes";
        let lz4_chunk_original = b"LZ4CHUNK-compressed-mip-bytes-for-the-next-mip-range";
        let lz4_chunk_compressed = lz4_flex::block::compress(lz4_chunk_original);

        // Lay out a synthetic archive body: [raw chunk bytes][lz4-compressed
        // chunk bytes], each `Dx10Chunk.offset` pointing at its own start —
        // matching how real BA2 chunk offsets are absolute file positions.
        let mut body = Vec::new();
        let raw_offset = body.len() as u64;
        body.extend_from_slice(raw_chunk_payload);
        let lz4_offset = body.len() as u64;
        body.extend_from_slice(&lz4_chunk_compressed);

        let chunks = vec![
            Dx10Chunk {
                offset: raw_offset,
                packed_size: 0, // stored raw
                unpacked_size: raw_chunk_payload.len() as u32,
                start_mip: 0,
                end_mip: 0,
            },
            Dx10Chunk {
                offset: lz4_offset,
                packed_size: lz4_chunk_compressed.len() as u32,
                unpacked_size: lz4_chunk_original.len() as u32,
                start_mip: 1,
                end_mip: 1,
            },
        ];

        let dds = extract_dx10(
            &body[..],
            Dx10TexInfo {
                dxgi_format: 71, // BC1 — matches the fixed-148-byte-header test above
                width: 256,
                height: 256,
                num_mips: 2,
                is_cubemap: false,
            },
            &chunks,
            Ba2Compression::Lz4Block,
            "textures\\test.dds",
        )
        .expect("mixed raw+LZ4-chunk record must decode cleanly");

        let mut expected_pixel_data = Vec::new();
        expected_pixel_data.extend_from_slice(raw_chunk_payload);
        expected_pixel_data.extend_from_slice(lz4_chunk_original);
        let header_len = dds.len() - expected_pixel_data.len();
        assert_eq!(
            &dds[..4],
            b"DDS ",
            "the synthesized DDS header must still lead the output"
        );
        assert_eq!(
            &dds[header_len..],
            expected_pixel_data.as_slice(),
            "the raw chunk's bytes must pass through unchanged and the LZ4 \
             chunk must decompress correctly, concatenated in chunk order"
        );
    }

    /// Two LZ4 chunks back to back; the first declares `first_declared`
    /// bytes, the second is honest.
    fn two_chunk_record(first_declared: u32, last_declared: u32) -> (Vec<u8>, Vec<Dx10Chunk>) {
        let first = lz4_flex::block::compress(b"mip0-mip0-mip0-mip0");
        let second = lz4_flex::block::compress(b"mip1-mip1");
        let mut body = first.clone();
        body.extend_from_slice(&second);
        let chunks = vec![
            Dx10Chunk {
                offset: 0,
                packed_size: first.len() as u32,
                unpacked_size: first_declared,
                start_mip: 0,
                end_mip: 0,
            },
            Dx10Chunk {
                offset: first.len() as u64,
                packed_size: second.len() as u32,
                unpacked_size: last_declared,
                start_mip: 1,
                end_mip: 1,
            },
        ];
        (body, chunks)
    }

    fn bc1_info() -> Dx10TexInfo {
        Dx10TexInfo {
            dxgi_format: 71,
            width: 8,
            height: 8,
            num_mips: 2,
            is_cubemap: false,
        }
    }

    /// #4662 — a non-final chunk decoding short would shift every later mip
    /// under the synthesized header. It must be rejected, and the error must
    /// name the archive entry and the chunk.
    #[test]
    fn extract_dx10_rejects_a_short_non_final_chunk_naming_the_entry() {
        // Chunk 0 decodes to 19 bytes but claims 64.
        let (body, chunks) = two_chunk_record(64, 9);
        let err = extract_dx10(
            &body[..],
            bc1_info(),
            &chunks,
            Ba2Compression::Lz4Block,
            r"textures\short.dds",
        )
        .expect_err("a short non-final chunk must not extract");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let msg = err.to_string();
        assert!(
            msg.contains(r"textures\short.dds"),
            "entry not named: {msg}"
        );
        assert!(msg.contains("chunk 0 of 2"), "chunk not named: {msg}");
    }

    /// #4662 — a short FINAL chunk shifts nothing, so it keeps the lenient
    /// (warn-only) behaviour of `decompress_chunk`.
    #[test]
    fn extract_dx10_tolerates_a_short_final_chunk() {
        let (body, chunks) = two_chunk_record(19, 64);
        let dds = extract_dx10(
            &body[..],
            bc1_info(),
            &chunks,
            Ba2Compression::Lz4Block,
            r"textures\short_tail.dds",
        )
        .expect("a short final chunk stays lenient");
        assert!(dds.ends_with(b"mip0-mip0-mip0-mip0mip1-mip1"));
    }

    /// #4662 — decode failures name the entry instead of reporting a bare
    /// "BA2 LZ4 block decompression failed".
    #[test]
    fn decompress_chunk_errors_name_the_entry() {
        let err = decompress_chunk(
            &[0xFF, 0xFE, 0xFD],
            64,
            Ba2Compression::Lz4Block,
            r"textures\broken.dds",
        )
        .unwrap_err();
        assert!(
            err.to_string().contains(r"textures\broken.dds"),
            "got: {err}"
        );
    }

    #[test]
    fn decompress_chunk_lz4_corrupt_data_fails() {
        // Garbage input should fail LZ4 decompression.
        let garbage = [0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA];
        let result = decompress_chunk(&garbage, 1024, Ba2Compression::Lz4Block, "test.bin");
        assert!(result.is_err());
    }

    /// Regression for #2618 / SF-D1-01: an LZ4 block that decodes to
    /// FEWER bytes than the record's declared `unpacked_size` (an
    /// "under-run") is a distinct case from `decompress_chunk_lz4_corrupt_data_fails`'s
    /// outright-garbage input — `lz4_flex::block::decompress`'s
    /// `unpacked_size` parameter bounds the output buffer but does not
    /// validate the decoded length against it (the safe decoder allocates
    /// `vec![0; n]` and then `truncate`s to the true length — #3392), so
    /// this still returns `Ok` with the shorter buffer
    /// (mirrors `decompress_chunk_zlib_short_stream_returns_actual_length`'s
    /// lenient-mode pin on the sibling zlib arm — this asymmetry with an
    /// over-run, which DOES hard-error, is deliberate upstream behaviour
    /// this function cannot and should not change).
    ///
    /// The warn-level log promoted by #2618 is the operator-visible
    /// signal that this happened — not directly assertable here without
    /// a test logger (same caveat as the zlib sibling test above);
    /// pinned instead by the `log::warn!` call site at `decompress_chunk`'s
    /// LZ4 arm. Pre-#2618 this condition was completely silent (no log
    /// at any level) and the doc comment on the zlib arm actively
    /// claimed LZ4 "hard-errors on the same condition," which was false.
    ///
    /// This is also the coverage pin #2630 (SF-D1-05) asked for. #4663
    /// removed its byte-identical twin, whose doc still called #2618 open
    /// and `min_uncompressed_size` a mere `Vec::with_capacity` hint — under
    /// the pinned `safe-decode` feature it is a hard output bound (#3392).
    #[test]
    fn decompress_chunk_lz4_under_run_returns_actual_length_not_declared() {
        let actual_payload = b"short lz4 payload, well under the declared size";
        let compressed = lz4_flex::block::compress(actual_payload);

        let result = decompress_chunk(
            &compressed,
            actual_payload.len() + 500,
            Ba2Compression::Lz4Block,
            "test.bin",
        )
        .expect("lenient mode: an over-declared LZ4 unpacked_size must NOT error");
        assert_eq!(
            result.as_slice(),
            actual_payload,
            "buffer length is what LZ4 actually decoded, NOT the record-declared \
             unpacked_size — downstream NIF/DDS parsers see the actual short payload"
        );
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

    /// SF-D1-05 (#2630) — `compression_method == 0` (zlib) on a v3
    /// archive has zero prior test coverage: every other v3 fixture in
    /// this module exercises method 3 (LZ4) or an outright-rejected
    /// unknown method. v3+zlib doesn't occur in vanilla Starfield
    /// content (v3 is DX10-only there, and DX10 zlib is covered
    /// elsewhere), but the reader accepts it (`0 => Ba2Compression::Zlib`
    /// at the header-parse match), so a mod-authored or future-game v3
    /// GNRL+zlib archive is a real reachable path that was silently
    /// untested.
    ///
    /// Also serves as the "byte-literal fixture built from the
    /// documented v3 header layout" the finding asked for: every offset
    /// below is a literal constant annotated against the module's own
    /// `# Version mapping` doc table (top of file) and the 36-byte GNRL
    /// record layout in `read_general_records`, rather than being
    /// generated through any of the parser's own encoding helpers (this
    /// module doesn't have a BA2 *writer*, so there is no shared code
    /// path that could drift in lockstep with a parser bug the way a
    /// round-trip-through-the-same-helpers fixture would).
    #[test]
    fn v3_zlib_gnrl_header_literal_fixture_roundtrips() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let payload = b"Starfield v3+zlib GNRL payload - SF-D1-05";
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(payload).unwrap();
        let compressed = encoder.finish().unwrap();

        // v3 header: 24-byte base + 8-byte v2/v3 extra + 4-byte
        // compression_method = 36 bytes, immediately followed by one
        // 36-byte GNRL record (72 bytes total), then the compressed
        // payload, then the 1-entry name table.
        const HEADER_LEN: u64 = 36;
        const RECORD_LEN: u64 = 36;
        let payload_offset = HEADER_LEN + RECORD_LEN; // 72
        let name_table_offset = payload_offset + compressed.len() as u64;

        let mut buf = Vec::new();
        buf.extend_from_slice(b"BTDX"); // magic
        buf.extend_from_slice(&3u32.to_le_bytes()); // version = 3 (Starfield)
        buf.extend_from_slice(b"GNRL"); // type_tag
        buf.extend_from_slice(&1u32.to_le_bytes()); // file_count = 1
        buf.extend_from_slice(&name_table_offset.to_le_bytes());
        // v2/v3 extra 8 bytes — observed-constant {unknown_1=1, unknown_2=0}
        // per #2629 / SF-D1-04, NOT the size the field was originally
        // (mis-)documented as.
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // compression_method = 0 (zlib)
        assert_eq!(buf.len() as u64, HEADER_LEN, "header layout drifted");

        // GNRL record: name_hash/ext/dir_hash/flags (4×4=16, unused) +
        // offset(8) + packed_size(4) + unpacked_size(4) + padding(4).
        buf.extend_from_slice(&[0u8; 16]);
        buf.extend_from_slice(&payload_offset.to_le_bytes());
        buf.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(&PADDING_BAADFOOD.to_le_bytes());
        assert_eq!(
            buf.len() as u64,
            HEADER_LEN + RECORD_LEN,
            "record layout drifted"
        );

        buf.extend_from_slice(&compressed);
        assert_eq!(
            buf.len() as u64,
            name_table_offset,
            "payload placement drifted"
        );

        // Name table: one 2-byte-length-prefixed name.
        let name = b"meshes/sfd1_05.nif";
        buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
        buf.extend_from_slice(name);

        let mut path = std::env::temp_dir();
        path.push(format!(
            "byroredux_ba2_v3_zlib_literal_{}.ba2",
            std::process::id()
        ));
        {
            let mut f = std::fs::File::create(&path).expect("create temp BA2");
            f.write_all(&buf).expect("write header");
        }
        let archive = Ba2Archive::open(&path);
        let _ = std::fs::remove_file(&path);
        let archive = archive.expect("byte-literal v3+zlib fixture must open cleanly");

        assert_eq!(archive.version(), 3);
        assert_eq!(archive.variant(), Ba2Variant::General);
        assert_eq!(archive.file_count(), 1);
        assert!(archive.contains("meshes/sfd1_05.nif"));
        let extracted = archive
            .extract("meshes/sfd1_05.nif")
            .expect("v3+zlib GNRL entry must extract");
        assert_eq!(
            extracted, payload,
            "v3+zlib decompression must round-trip the exact authored payload"
        );
    }

    /// #2097 / LZ4-01 — an aggressively undersized `unpacked_size` hint must
    /// come back as `Ok` or `Err`, never as an unwind through the caller.
    ///
    /// `lz4_flex::block::decompress` documents itself as "may panic" when the
    /// hint undershoots the true decompressed size. On the pinned 0.14.0
    /// (as on 0.11.6 before it) it does not — every case below returns
    /// normally — so this test cannot fail today on behaviour alone. That
    /// is precisely why it is paired with
    /// [`lz4_decompress_is_panic_guarded`]: this one exercises the arm and
    /// documents the sizes that were probed, and that one is what actually
    /// fails if the guard is removed.
    #[test]
    fn decompress_chunk_lz4_undersized_hint_never_unwinds() {
        let actual_payload = b"a payload comfortably larger than the hints probed below";
        let compressed = lz4_flex::block::compress(actual_payload);

        // #3394 — `0` IS reachable and is included deliberately. The previous
        // comment here claimed "0 is rejected upstream by
        // `checked_chunk_size_usize`", which is false: that helper only rejects
        // sizes ABOVE `MAX_CHUNK_BYTES` and returns `Ok(0)` for zero
        // (`crates/bsa/src/safety.rs`). A malformed archive can therefore reach
        // this arm with `unpacked_size == 0` and a non-zero `packed_size` —
        // `read_dx10_records` accepts it on the same rule, and `extract_dx10`
        // takes the decompress branch whenever `packed_size != 0`. Zero is the
        // most-undersized hint possible, i.e. exactly what this test exists to
        // probe, so excluding it omitted the boundary case. (The safe decoder
        // handles it: an undersized hint is `Err(OutputTooSmall)`, which
        // this arm maps to `InvalidData`.) No vanilla archive exercises it — a
        // scan of 19,656 v3 DX10 records found zero such chunks — but the
        // hostile-input case is the point.
        for hint in [0usize, 1, 2, 8, 32, actual_payload.len()] {
            let result = decompress_chunk(&compressed, hint, Ba2Compression::Lz4Block, "test.bin");
            // Either outcome is acceptable — the contract under test is that
            // control returns here at all.
            match result {
                Ok(buf) => assert!(
                    !buf.is_empty(),
                    "hint {hint}: a successful decode must not be empty"
                ),
                Err(e) => assert_eq!(
                    e.kind(),
                    io::ErrorKind::InvalidData,
                    "hint {hint}: failures must surface as InvalidData, not a foreign kind"
                ),
            }
        }
    }

    /// #3393 — take at most `max_bytes` of `s`, backing up to the nearest
    /// char boundary.
    ///
    /// The source-order pins below scope their search to the head of a match
    /// arm by byte budget. A bare `&s[..s.len().min(2000)]` panics with
    /// `byte index 2000 is not a char boundary` whenever byte 2000 lands
    /// inside a multi-byte scalar — and both arms carry em dashes in their
    /// comments, so the cut moving onto one is a comment edit away. Backing
    /// up (never widening) keeps the scoping guarantee: widening to the whole
    /// file would let an unrelated `catch_unwind` satisfy the assertion.
    fn prefix_up_to(s: &str, max_bytes: usize) -> &str {
        let mut end = max_bytes.min(s.len());
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }

    /// #3392 — pins that `lz4_flex` still builds with `safe-decode`.
    ///
    /// This is the actual mitigation for the undersized-hint case the
    /// `catch_unwind` below is often mistaken for. With `safe-decode` on,
    /// `decompress` allocates `vec![0; n]` and writes through a
    /// bounds-checked `SliceSink` under `forbid(unsafe_code)`, so a short
    /// hint is `Err(OutputTooSmall)`. With it off, the same call writes
    /// through raw pointers with no capacity check and a short hint is a
    /// heap overflow — UB, which no `catch_unwind` can intercept. Archive
    /// bytes are attacker-controlled for modded content.
    ///
    /// A source-order pin because Cargo feature resolution is not visible to
    /// `cfg!` from a dependent crate: `safe-decode` is `lz4_flex`'s feature,
    /// not ours. `byroredux-bsa` is its only dependent, so a
    /// `default-features = false` here would silently swap the decoder with
    /// nothing else in the workspace to re-enable it.
    #[test]
    fn lz4_flex_is_pinned_to_the_safe_decoder() {
        const MANIFEST: &str = include_str!("../../../Cargo.toml");
        let entry = MANIFEST
            .split_once("lz4_flex = ")
            .expect("the workspace must still declare lz4_flex")
            .1;
        let decl = prefix_up_to(entry, 400);
        assert!(
            decl.contains("safe-decode"),
            "lz4_flex no longer requests `safe-decode` — BA2 LZ4 chunk decoding \
             would fall back to the raw-pointer decoder, where an undersized \
             size hint from a malformed archive is a heap buffer overflow \
             rather than an error (#3392)"
        );
        assert!(
            decl.contains("default-features = false"),
            "lz4_flex's feature set is no longer pinned explicitly — the \
             memory-safety property would revert to depending on upstream's \
             `default` staying unchanged (#3392)"
        );
    }

    /// #3393 — `prefix_up_to` must back up to a char boundary rather than
    /// panicking, and must never widen past its budget.
    #[test]
    fn prefix_up_to_backs_up_to_a_char_boundary() {
        // 'é' is 2 bytes at 1..3, so a budget of 2 cuts mid-scalar.
        assert_eq!(prefix_up_to("aéb", 2), "a");
        // An exact boundary is taken as-is.
        assert_eq!(prefix_up_to("aéb", 3), "aé");
        // A budget past the end clamps to the whole string, never panics.
        assert_eq!(prefix_up_to("aéb", 999), "aéb");
        // Never widens: the result is always within budget.
        assert!(prefix_up_to("—————", 7).len() <= 7);
        // Degenerate: a budget landing inside the very first scalar yields "".
        assert_eq!(prefix_up_to("—", 1), "");
    }

    /// #2097 / LZ4-01 — pins that the LZ4 arm still routes through
    /// `catch_unwind`.
    ///
    /// The behavioural guard above cannot fail on the pinned
    /// `lz4_flex 0.14.0` (nor on 0.11.6 before it), because those versions
    /// simply do not panic on the inputs their own docs warn
    /// about. So the thing worth guarding is not the behaviour but the
    /// *defence*: delete the `catch_unwind` and this test fails, which is the
    /// only way this fix can be kept from silently regressing on a future
    /// dependency bump — exactly the scenario the issue was filed about.
    #[test]
    fn lz4_decompress_is_panic_guarded() {
        const SRC: &str = include_str!("ba2.rs");
        let arm = SRC
            .split("Ba2Compression::Lz4Block =>")
            .nth(1)
            .expect("the Lz4Block match arm must exist in this file");
        // Look only at the arm body, not the whole file, so an unrelated
        // `catch_unwind` elsewhere cannot satisfy this.
        let body = prefix_up_to(arm, 2000);
        assert!(
            body.contains("catch_unwind"),
            "the Lz4Block arm no longer wraps lz4_flex::block::decompress in \
             catch_unwind — lz4_flex documents `decompress` as may-panic when the \
             size hint undershoots, and archive bytes are attacker-controlled for \
             modded content (#2097)"
        );
        assert!(
            body.contains("lz4_flex::block::decompress"),
            "the Lz4Block arm no longer calls lz4_flex::block::decompress — if the \
             codec call moved, move this guard with it"
        );
    }

    /// #2360 / SF-BA2-02 — the v3 header-boundary diagnostic must capture
    /// `stream_position()` *after* the 4-byte compression-method field, so
    /// the logged offset is the true 36-byte header end rather than 32.
    ///
    /// This is a source-order pin because the thing under test is when a
    /// `log::trace!` observes the cursor, and this workspace has no
    /// log-capture harness (only `env_logger`, a writer) — the issue's own
    /// "assert logic equivalence if untestable via log capture" case. The
    /// v2 arm needs no equivalent: it has nothing left to read at its
    /// capture site, so its position *is* the header end by construction.
    #[test]
    fn v3_header_boundary_log_is_captured_after_the_compression_method() {
        const SRC: &str = include_str!("ba2.rs");
        let arm = SRC
            .split("BA2_V_STARFIELD_V3 =>")
            .nth(1)
            .expect("the BA2_V_STARFIELD_V3 match arm must exist in this file");
        let body = prefix_up_to(arm, 2000);
        let method_read = body
            .find("reader.read_exact(&mut method_buf)")
            .expect("the v3 arm must still read the 4-byte compression method");
        let log_call = body
            .find(r#"log_v2_v3_extra_bytes("v3""#)
            .expect("the v3 arm must still emit the header-boundary diagnostic");
        assert!(
            method_read < log_call,
            "the v3 header-boundary log is captured BEFORE the 4-byte compression \
             method is read, so it reports a 32-byte offset where the real v3 header \
             ends at 36 — understating the boundary by exactly that field (#2360)"
        );
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
    /// #4661 — a header-only BA2 declaring 10 M records (under the absolute
    /// cap) used to fail with a bare "failed to fill whole buffer". The error
    /// now names the record kind, index and declared count, and keeps the
    /// `UnexpectedEof` kind. (The reservation itself is bounded by
    /// `capacity_hint` — see its unit test in `safety.rs`.)
    #[test]
    fn truncated_record_table_names_the_record_and_declared_count() {
        use std::io::Write;
        let mut hdr = Vec::with_capacity(24);
        hdr.extend_from_slice(b"BTDX");
        hdr.extend_from_slice(&1u32.to_le_bytes());
        hdr.extend_from_slice(b"GNRL");
        hdr.extend_from_slice(&10_000_000u32.to_le_bytes());
        hdr.extend_from_slice(&24u64.to_le_bytes()); // name table at EOF
        let mut path = std::env::temp_dir();
        path.push(format!("byroredux_ba2_4661_{}.ba2", std::process::id()));
        std::fs::File::create(&path)
            .and_then(|mut f| f.write_all(&hdr))
            .expect("write temp BA2");
        let result = Ba2Archive::open(&path);
        let _ = std::fs::remove_file(&path);
        let err = match result {
            Ok(_) => panic!("a 24-byte archive cannot hold 10 M records"),
            Err(e) => e,
        };
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        let msg = err.to_string();
        assert!(msg.contains("GNRL file record 0 of 10000000"), "got: {msg}");
    }

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

    /// Build a one-record DX10 v1 BA2 with `num_chunks` chunks whose
    /// 24-byte headers all declare BAADF00D padding and empty payloads.
    /// `chunk_hdr_len` lands in the base record; `start_mips` drives the
    /// per-chunk start_mip sequence (shorter slices pad with 0).
    fn build_dx10_ba2(chunk_hdr_len: u16, start_mips: &[u16]) -> Vec<u8> {
        let num_chunks = start_mips.len().max(1);
        let name_table_offset = 24u64 + 24 + num_chunks as u64 * 24;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"BTDX");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(b"DX10");
        bytes.extend_from_slice(&1u32.to_le_bytes()); // file_count
        bytes.extend_from_slice(&name_table_offset.to_le_bytes());
        let mut base = [0u8; 24];
        base[0..4].copy_from_slice(&0x1234_5678u32.to_le_bytes()); // name_hash
        base[4..8].copy_from_slice(b"dds\0");
        base[8..12].copy_from_slice(&0x9ABC_DEF0u32.to_le_bytes()); // dir_hash
        base[13] = num_chunks as u8;
        base[14..16].copy_from_slice(&chunk_hdr_len.to_le_bytes());
        base[16..18].copy_from_slice(&4u16.to_le_bytes()); // height
        base[18..20].copy_from_slice(&4u16.to_le_bytes()); // width
        base[20] = 1; // num_mips
        base[21] = 98; // dxgi format (BC1_UNORM)
        bytes.extend_from_slice(&base);
        for i in 0..num_chunks {
            let mut chunk = [0u8; 24];
            chunk[8..12].copy_from_slice(&0u32.to_le_bytes()); // packed_size
            chunk[12..16].copy_from_slice(&0u32.to_le_bytes()); // unpacked_size
            chunk[16..18]
                .copy_from_slice(&start_mips.get(i).copied().unwrap_or(0).to_le_bytes());
            chunk[18..20].copy_from_slice(&1u16.to_le_bytes()); // end_mip
            chunk[20..24].copy_from_slice(&0xBAADF00Du32.to_le_bytes());
            bytes.extend_from_slice(&chunk);
        }
        let name = "textures\\probe.dds";
        bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());
        bytes
    }

    fn write_temp_ba2(bytes: &[u8], tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "byroredux_dx10_{tag}_{}_{}.ba2",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos(),
        ));
        std::fs::write(&path, bytes).expect("write temp BA2");
        path
    }

    /// #5008 / PAR-D2-2026-09-29-01 — `chunk_hdr_len != 24` is a
    /// file-controlled field; the old `debug_assert_eq!` panicked in debug
    /// builds at boot while release only warned. The reader cannot parse
    /// any other chunk stride, so it must be a hard `InvalidData` in every
    /// build — never a panic.
    #[test]
    fn dx10_open_rejects_a_non_24_chunk_hdr_len_as_invalid_data() {
        let path = write_temp_ba2(&build_dx10_ba2(32, &[0]), "hdrlen");
        let result = Ba2Archive::open(&path);
        let _ = std::fs::remove_file(&path);
        let err = match result {
            Ok(_) => panic!("chunk_hdr_len=32 must not be accepted"),
            Err(e) => e,
        };
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let msg = format!("{err}");
        assert!(
            msg.contains("chunk_hdr_len=32"),
            "error must name the field and value, got: {msg}"
        );
        // The control record with the canonical stride still opens.
        let path = write_temp_ba2(&build_dx10_ba2(24, &[0]), "hdrlen_ok");
        let ok = Ba2Archive::open(&path);
        let _ = std::fs::remove_file(&path);
        assert!(ok.is_ok(), "chunk_hdr_len=24 control must open: {:?}", ok.err());
    }

    /// #5008 / PAR-D2-2026-09-29-01 — a non-monotonic `start_mip` sequence
    /// is file-controlled and used to trip a `debug_assert!` at boot. The
    /// record itself still parses, so open must succeed and only warn.
    #[test]
    fn dx10_open_tolerates_non_monotonic_start_mip_without_panicking() {
        let path = write_temp_ba2(&build_dx10_ba2(24, &[1, 0]), "startmip");
        let result = Ba2Archive::open(&path);
        let _ = std::fs::remove_file(&path);
        let archive = result.expect("non-monotonic start_mip must open, not fail");
        assert_eq!(archive.file_count(), 1);
    }
}
