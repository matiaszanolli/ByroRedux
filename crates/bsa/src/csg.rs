//! `.csg` — Fallout 4 Shared-Geometry blob reader (M49).
//!
//! `<Plugin> - Geometry.csg` stores the vertex / triangle data for the
//! precombined `_oc.nif` files whose `BSPackedCombinedSharedGeomDataExtra`
//! blocks carry only a `(filename_hash, data_offset)` pointer. Vanilla
//! FO4 precombines are 100 % this shared variant, so without this reader
//! the precombined pass spawns no geometry. Full byte-layout spec (with
//! validation notes) lives at `docs/engine/fo4-csg-format.md`.
//!
//! Container:
//! ```text
//! 0   "bcsg"
//! 4   u32 num_objects
//! 8   u32 num_chunks
//! 12  ChunkEntry[num_chunks]   { u32 compressed_size, u32 file_offset }
//! ..  ObjectEntry[num_objects] (20 bytes each — CK index, not read here)
//! ..  zlib chunks              each inflates to 65 536 B (last partial)
//! ```
//!
//! The **uncompressed PSG space** is the concatenation of every inflated
//! chunk, so chunk `i` covers PSG bytes `[i*65536, (i+1)*65536)`. A NIF
//! `data_offset` indexes straight into that space; [`CsgArchive::read_psg`]
//! resolves it, decompressing (and caching) only the chunks it touches.

use crate::safety::{checked_chunk_size, checked_entry_count};
use flate2::read::ZlibDecoder;
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Every CSG chunk inflates to exactly this many bytes (the final chunk
/// may be shorter). The PSG-space offset of chunk `i` is `i * CHUNK_SIZE`.
pub const CSG_CHUNK_SIZE: usize = 65_536;

const MAGIC: &[u8; 4] = b"bcsg";

/// One entry of the chunk table: where a zlib stream lives in the file
/// and how many compressed bytes it spans.
#[derive(Debug, Clone, Copy)]
struct ChunkEntry {
    compressed_size: u32,
    file_offset: u32,
}

/// Fixed-capacity FIFO cache of inflated 64 KiB chunks. A single cell's
/// precombine load hits the same chunk from many objects and straddles
/// chunk boundaries, so even a tiny cache amortises the zlib cost; the
/// FIFO bound keeps the 240 MB-class blob from materialising in full.
struct ChunkCache {
    cap: usize,
    order: VecDeque<u32>,
    map: HashMap<u32, Arc<[u8]>>,
}

impl ChunkCache {
    fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            order: VecDeque::new(),
            map: HashMap::new(),
        }
    }

    fn get(&self, idx: u32) -> Option<Arc<[u8]>> {
        self.map.get(&idx).cloned()
    }

    fn insert(&mut self, idx: u32, bytes: Arc<[u8]>) {
        if self.map.contains_key(&idx) {
            return;
        }
        while self.order.len() >= self.cap {
            if let Some(evict) = self.order.pop_front() {
                self.map.remove(&evict);
            } else {
                break;
            }
        }
        self.order.push_back(idx);
        self.map.insert(idx, bytes);
    }
}

struct CsgInner {
    file: File,
    cache: ChunkCache,
}

/// Random-access reader over a `<Plugin> - Geometry.csg` blob.
pub struct CsgArchive {
    inner: Mutex<CsgInner>,
    chunks: Vec<ChunkEntry>,
    num_objects: u32,
    file_len: u64,
}

impl CsgArchive {
    /// Open a `.csg`, parsing the header and chunk table (the object
    /// table and payload are read lazily). Caches up to 16 inflated
    /// chunks (~1 MiB).
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::open_with_cache(path, 16)
    }

    /// Open with an explicit inflated-chunk cache capacity.
    pub fn open_with_cache(path: impl AsRef<Path>, cache_chunks: usize) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let file_len = file.metadata()?.len();

        let mut head = [0u8; 12];
        file.read_exact(&mut head)?;
        if &head[0..4] != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("not a CSG blob: magic {:02x?} != \"bcsg\"", &head[0..4]),
            ));
        }
        let num_objects = u32::from_le_bytes(head[4..8].try_into().unwrap());
        let num_chunks_raw = u32::from_le_bytes(head[8..12].try_into().unwrap());
        let num_chunks = checked_entry_count(num_chunks_raw, "CSG chunk")?;

        // Chunk table: num_chunks × 8 bytes, immediately after the header.
        let mut table = vec![0u8; num_chunks * 8];
        file.read_exact(&mut table)?;
        let mut chunks = Vec::with_capacity(num_chunks);
        for i in 0..num_chunks {
            let b = &table[i * 8..i * 8 + 8];
            let compressed_size = u32::from_le_bytes(b[0..4].try_into().unwrap());
            let file_offset = u32::from_le_bytes(b[4..8].try_into().unwrap());
            if file_offset as u64 > file_len {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("CSG chunk {i} file_offset {file_offset} past EOF {file_len}"),
                ));
            }
            chunks.push(ChunkEntry {
                compressed_size,
                file_offset,
            });
        }

        Ok(Self {
            inner: Mutex::new(CsgInner {
                file,
                cache: ChunkCache::new(cache_chunks),
            }),
            chunks,
            num_objects,
            file_len,
        })
    }

    /// Number of geometry objects declared in the header (CK index size).
    pub fn num_objects(&self) -> u32 {
        self.num_objects
    }

    /// Number of compressed chunks.
    pub fn num_chunks(&self) -> usize {
        self.chunks.len()
    }

    /// Total size of the uncompressed PSG space (sum of inflated chunk
    /// sizes). All chunks but the last are exactly `CSG_CHUNK_SIZE`; the
    /// last is computed by inflating it. Cheap after the first call (the
    /// last chunk is then cached).
    pub fn psg_len(&self) -> io::Result<u64> {
        if self.chunks.is_empty() {
            return Ok(0);
        }
        let last = self.chunks.len() - 1;
        let last_len = self.chunk_bytes(last as u32)?.len() as u64;
        Ok((self.chunks.len() as u64 - 1) * CSG_CHUNK_SIZE as u64 + last_len)
    }

    /// Read `len` bytes from PSG-uncompressed space starting at `offset`,
    /// transparently decompressing and stitching across 64 KiB chunk
    /// boundaries.
    pub fn read_psg(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        let mut remaining = len;
        let mut pos = offset;
        while remaining > 0 {
            let idx = (pos / CSG_CHUNK_SIZE as u64) as u32;
            let local = (pos % CSG_CHUNK_SIZE as u64) as usize;
            if idx as usize >= self.chunks.len() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!(
                        "CSG read_psg: offset {pos} past PSG space ({} chunks)",
                        self.chunks.len()
                    ),
                ));
            }
            let chunk = self.chunk_bytes(idx)?;
            if local >= chunk.len() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!(
                        "CSG read_psg: local {local} past chunk {idx} len {}",
                        chunk.len()
                    ),
                ));
            }
            // `local < chunk.len()` (checked above) and `remaining > 0` (loop
            // guard), so `take >= 1` always — `remaining` strictly decreases
            // and the loop terminates. (Removed a dead `take == 0` break,
            // FO4-2026-06-23-L02 / #1735.)
            let take = remaining.min(chunk.len() - local);
            out.extend_from_slice(&chunk[local..local + take]);
            pos += take as u64;
            remaining -= take;
        }
        Ok(out)
    }

    /// Inflate chunk `idx` (cached). Returns the decompressed bytes —
    /// `CSG_CHUNK_SIZE` for every chunk but the last.
    fn chunk_bytes(&self, idx: u32) -> io::Result<Arc<[u8]>> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(hit) = inner.cache.get(idx) {
            return Ok(hit);
        }
        let entry = self.chunks[idx as usize];
        let comp_len = checked_chunk_size(entry.compressed_size, "CSG chunk compressed")?;
        // Clamp the compressed read to the file tail for the last chunk
        // (its stored compressed_size runs to EOF).
        let avail = self.file_len.saturating_sub(entry.file_offset as u64) as usize;
        let read_len = comp_len.min(avail);
        let mut comp = vec![0u8; read_len];
        inner.file.seek(SeekFrom::Start(entry.file_offset as u64))?;
        inner.file.read_exact(&mut comp)?;

        let mut raw = Vec::with_capacity(CSG_CHUNK_SIZE);
        ZlibDecoder::new(&comp[..]).read_to_end(&mut raw)?;
        if raw.len() > CSG_CHUNK_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "CSG chunk {idx} inflated to {} > {CSG_CHUNK_SIZE}",
                    raw.len()
                ),
            ));
        }
        let arc: Arc<[u8]> = Arc::from(raw.into_boxed_slice());
        inner.cache.insert(idx, arc.clone());
        Ok(arc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn zlib(raw: &[u8]) -> Vec<u8> {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(raw).unwrap();
        e.finish().unwrap()
    }

    /// Build an in-memory CSG with an empty object table and the given
    /// already-uncompressed chunk payloads.
    fn build_csg(chunks: &[Vec<u8>]) -> Vec<u8> {
        let num_chunks = chunks.len();
        let comp: Vec<Vec<u8>> = chunks.iter().map(|c| zlib(c)).collect();
        let mut out = Vec::new();
        out.extend_from_slice(b"bcsg");
        out.extend_from_slice(&0u32.to_le_bytes()); // num_objects = 0 (no object table)
        out.extend_from_slice(&(num_chunks as u32).to_le_bytes());
        // chunk table: (compressed_size, file_offset)
        let first = 12 + num_chunks * 8;
        let mut acc = first;
        let mut offs = Vec::new();
        for c in &comp {
            offs.push(acc);
            acc += c.len();
        }
        for (i, c) in comp.iter().enumerate() {
            out.extend_from_slice(&(c.len() as u32).to_le_bytes());
            out.extend_from_slice(&(offs[i] as u32).to_le_bytes());
        }
        for c in &comp {
            out.extend_from_slice(c);
        }
        out
    }

    fn write_temp(bytes: &[u8], tag: &str) -> std::path::PathBuf {
        let p =
            std::env::temp_dir().join(format!("byro_csg_test_{tag}_{}.csg", std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn rejects_bad_magic() {
        let p = write_temp(b"NOPE........", "badmagic");
        match CsgArchive::open(&p) {
            Ok(_) => panic!("expected bad-magic rejection"),
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
        }
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn reads_psg_across_chunk_boundary() {
        // chunk0 = full 64 KiB, chunk1 = 1000-byte partial. Distinct
        // patterns so a boundary stitch error is visible.
        let c0: Vec<u8> = (0..CSG_CHUNK_SIZE).map(|i| (i % 251) as u8).collect();
        let c1: Vec<u8> = (0..1000).map(|i| (200 + (i % 7)) as u8).collect();
        let p = write_temp(&build_csg(&[c0.clone(), c1.clone()]), "boundary");

        let csg = CsgArchive::open(&p).unwrap();
        assert_eq!(csg.num_chunks(), 2);
        assert_eq!(csg.num_objects(), 0);
        assert_eq!(csg.psg_len().unwrap(), (CSG_CHUNK_SIZE + 1000) as u64);

        // Wholly inside chunk 0.
        assert_eq!(csg.read_psg(0, 8).unwrap(), &c0[0..8]);
        assert_eq!(csg.read_psg(100, 16).unwrap(), &c0[100..116]);
        // Straddle the 64 KiB boundary: 6 bytes tail of c0 + 6 head of c1.
        let mut expect = c0[CSG_CHUNK_SIZE - 6..].to_vec();
        expect.extend_from_slice(&c1[0..6]);
        assert_eq!(
            csg.read_psg((CSG_CHUNK_SIZE - 6) as u64, 12).unwrap(),
            expect
        );
        // Tail of the partial last chunk.
        assert_eq!(
            csg.read_psg((CSG_CHUNK_SIZE + 990) as u64, 10).unwrap(),
            &c1[990..1000]
        );
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn read_past_psg_end_errors() {
        let c0: Vec<u8> = (0..100).map(|i| i as u8).collect();
        let p = write_temp(&build_csg(&[c0]), "pasteof");
        let csg = CsgArchive::open(&p).unwrap();
        assert!(csg.read_psg(50, 100).is_err());
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn cache_eviction_keeps_reads_correct() {
        // 4 chunks, cache cap 1 → every read evicts; results must stay right.
        let chunks: Vec<Vec<u8>> = (0..4)
            .map(|k| {
                (0..CSG_CHUNK_SIZE)
                    .map(|i| ((i + k * 13) % 251) as u8)
                    .collect()
            })
            .collect();
        let p = write_temp(&build_csg(&chunks), "evict");
        let csg = CsgArchive::open_with_cache(&p, 1).unwrap();
        for k in 0..4u64 {
            let got = csg.read_psg(k * CSG_CHUNK_SIZE as u64, 4).unwrap();
            assert_eq!(got, &chunks[k as usize][0..4], "chunk {k} after eviction");
        }
        // Re-read chunk 0 (long since evicted) — must re-inflate correctly.
        assert_eq!(csg.read_psg(0, 4).unwrap(), &chunks[0][0..4]);
        std::fs::remove_file(&p).ok();
    }
}
