//! Version-aware binary stream reader for NIF files.
//!
//! NifStream wraps a byte cursor and carries the header context so that
//! version-dependent reads (string format, block references, etc.) are
//! handled in one place rather than scattered through block parsers.

use crate::header::NifHeader;
use crate::types::{BlockRef, NiColor, NiMatrix3, NiPoint3, NiQuatTransform, NiTransform};
use crate::version::{NifVariant, NifVersion};
use std::io::{self, Cursor, Read};
use std::sync::Arc;

/// Binary reader with NIF header context for version-aware parsing.
pub struct NifStream<'a> {
    cursor: Cursor<&'a [u8]>,
    header: &'a NifHeader,
    variant: NifVariant,
}

/// Hard cap on any single file-driven allocation. A corrupt or malicious
/// NIF can claim an arbitrary 32-bit size in a `ByteArray`, `read_bytes`
/// caller, or `vec![0u8; n]` bulk read; without a cap the parser would
/// allocate gigabytes before `read_exact` fails.
///
/// 256 MB is well above any legitimate single-block payload (the fattest
/// Gamebryo shader-map binary or Havok physics blob we've seen on the
/// seven supported games is ~12 MB on FO76 actor NIFs), and well below
/// host RAM pressure on our 16-GB dev target. See #113 / audit NIF-13.
pub const MAX_SINGLE_ALLOC_BYTES: usize = 256 * 1024 * 1024;

impl<'a> NifStream<'a> {
    pub fn new(data: &'a [u8], header: &'a NifHeader) -> Self {
        let variant =
            NifVariant::detect(header.version, header.user_version, header.user_version_2);
        Self {
            cursor: Cursor::new(data),
            header,
            variant,
        }
    }

    pub fn version(&self) -> NifVersion {
        self.header.version
    }

    pub fn user_version(&self) -> u32 {
        self.header.user_version
    }

    pub fn user_version_2(&self) -> u32 {
        self.header.user_version_2
    }

    /// The detected game variant — use this for feature queries instead of raw version numbers.
    pub fn variant(&self) -> NifVariant {
        self.variant
    }

    /// Actual BSVER from the header (user_version_2).
    /// Use this for fine-grained binary format decisions instead of the variant's
    /// hardcoded bsver(), which represents the "typical" value for that game.
    pub fn bsver(&self) -> u32 {
        self.header.user_version_2
    }

    pub fn position(&self) -> u64 {
        self.cursor.position()
    }

    pub fn set_position(&mut self, pos: u64) {
        self.cursor.set_position(pos);
    }

    /// Advance the cursor by `n` bytes.
    ///
    /// Returns `UnexpectedEof` if the skip would move past the end of
    /// the backing data, or if `pos + n` overflows `u64`. The cursor is
    /// NOT advanced on error, so callers can rely on block_size recovery.
    pub fn skip(&mut self, n: u64) -> io::Result<()> {
        let pos = self.cursor.position();
        let end = pos
            .checked_add(n)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "skip overflow"))?;
        let len = self.cursor.get_ref().len() as u64;
        if end > len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("skip({n}) at position {pos} would exceed data length {len}"),
            ));
        }
        self.cursor.set_position(end);
        Ok(())
    }

    // ── Primitive reads ────────────────────────────────────────────────

    pub fn read_u8(&mut self) -> io::Result<u8> {
        let mut buf = [0u8; 1];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    pub fn read_u16_le(&mut self) -> io::Result<u16> {
        let mut buf = [0u8; 2];
        self.cursor.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    pub fn read_u32_le(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    pub fn read_i32_le(&mut self) -> io::Result<i32> {
        let mut buf = [0u8; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(i32::from_le_bytes(buf))
    }

    pub fn read_u64_le(&mut self) -> io::Result<u64> {
        let mut buf = [0u8; 8];
        self.cursor.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    pub fn read_f32_le(&mut self) -> io::Result<f32> {
        let mut buf = [0u8; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(f32::from_le_bytes(buf))
    }

    /// Read a NiBool (version-dependent size).
    ///
    /// Per nif.xml's `<basic name="bool">` entry:
    /// > A boolean; 32-bit up to and including 4.0.0.2, 8-bit from 4.1.0.1 on.
    ///
    /// So every game Redux targets (Morrowind 4.0.0.0, Oblivion 20.0.0.5,
    /// FO3/FNV 20.2.0.7, Skyrim+) reads a **single byte**. Only pre-4.1
    /// NetImmerse content uses the 4-byte form.
    ///
    /// A previous version of this function had the comparison inverted
    /// and the test cases documented the wrong behavior; that bug made
    /// `NiTriShape::parse` over-read by 3 bytes on every Oblivion NIF
    /// that had a shader, which in turn made the block walker fail on
    /// every Oblivion static mesh and silently return empty scenes.
    pub fn read_bool(&mut self) -> io::Result<bool> {
        if self.header.version >= NifVersion(0x04010001) {
            // 4.1.0.1+: bool is u8
            Ok(self.read_u8()? != 0)
        } else {
            // Pre-4.1.0.1: bool is u32
            Ok(self.read_u32_le()? != 0)
        }
    }

    /// Read a 1-byte boolean (`bool` type in niftools, NOT `NiBool`).
    /// NiGeometryData and related blocks use 1-byte bools for
    /// has_vertices, has_normals, has_colors, etc. in all versions.
    pub fn read_byte_bool(&mut self) -> io::Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    pub fn read_bytes(&mut self, len: usize) -> io::Result<Vec<u8>> {
        self.check_alloc(len)?;
        let mut buf = vec![0u8; len];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// File-driven pre-allocation for `Vec<T>` of length `count`.
    ///
    /// Bounds `count` against the bytes remaining in the stream — each
    /// on-disk element occupies at least one byte (even an `Option`
    /// reference is a 4-byte BlockRef), so a claimed count larger than
    /// the rest of the file is necessarily corrupt and we reject it
    /// before allocating any capacity.
    ///
    /// Used in place of the raw `Vec::with_capacity(count as usize)`
    /// anywhere `count` is a `u32` / `u16` read straight out of the
    /// stream — otherwise a corrupt NIF can trip a giant allocation
    /// before the subsequent reads discover the truncation.
    ///
    /// The bound is on-disk bytes, **not** `size_of::<T>()`, because
    /// element types like `(f32, String)` carry heap pointers far
    /// larger than their serialized representation; a `size_of`-based
    /// check produces false positives on legitimate small NIFs.
    ///
    /// See #388 / OBL-D5-C1 — every Oblivion content sweep used to
    /// abort the process on a crafted or drifted `NiTextKeyExtraData`.
    pub fn allocate_vec<T>(&self, count: u32) -> io::Result<Vec<T>> {
        let pos = self.cursor.position() as usize;
        let total = self.cursor.get_ref().len();
        let remaining = total.saturating_sub(pos);
        if (count as usize) > remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "NIF claims {count} elements but only {remaining} bytes remain at position {pos} in {total}-byte stream"
                ),
            ));
        }
        Ok(Vec::with_capacity(count as usize))
    }

    /// Validate a file-driven allocation request before `vec![0u8; n]`.
    ///
    /// Rejects claims that (a) exceed the remaining bytes in the stream
    /// — preventing the parser from allocating gigabytes for a block
    /// that physically can't contain them — and (b) breach the hard
    /// [`MAX_SINGLE_ALLOC_BYTES`] cap. Failure short-circuits BEFORE
    /// the allocation, so a corrupt file can't OOM the process.
    ///
    /// Called by every size-prefixed reader (`read_bytes`,
    /// `read_sized_string`, and the bulk array helpers) that would
    /// otherwise trust an attacker-controlled length. `pub` so that
    /// non-stream call sites (e.g. the header block-size table) can
    /// validate before pre-sizing their own buffers. See #113, #388.
    pub fn check_alloc(&self, bytes: usize) -> io::Result<()> {
        if bytes > MAX_SINGLE_ALLOC_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "NIF requested {bytes}-byte allocation, exceeds hard cap \
                     ({MAX_SINGLE_ALLOC_BYTES})"
                ),
            ));
        }
        let pos = self.cursor.position() as usize;
        let total = self.cursor.get_ref().len();
        let remaining = total.saturating_sub(pos);
        if bytes > remaining {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "NIF requested {bytes}-byte read at position {pos}, \
                     only {remaining} bytes remaining in {total}-byte stream"
                ),
            ));
        }
        Ok(())
    }

    // ── Bulk reads (geometry hot path) ────────────────────────────────
    //
    // Read entire arrays in a single read_exact call instead of per-element
    // calls, reducing function call + bounds check overhead from O(N) to O(1).
    // See #291.

    /// Read `count` NiPoint3 values (3×f32 each) in one bulk read.
    pub fn read_ni_point3_array(&mut self, count: usize) -> io::Result<Vec<NiPoint3>> {
        let byte_count = count * 12;
        self.check_alloc(byte_count)?;
        let mut buf = vec![0u8; byte_count];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf
            .chunks_exact(12)
            .map(|c| NiPoint3 {
                x: f32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                y: f32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                z: f32::from_le_bytes([c[8], c[9], c[10], c[11]]),
            })
            .collect())
    }

    /// Read `count` RGBA color values (4×f32 each) in one bulk read.
    pub fn read_ni_color4_array(&mut self, count: usize) -> io::Result<Vec<[f32; 4]>> {
        let byte_count = count * 16;
        self.check_alloc(byte_count)?;
        let mut buf = vec![0u8; byte_count];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf
            .chunks_exact(16)
            .map(|c| {
                [
                    f32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                    f32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                    f32::from_le_bytes([c[8], c[9], c[10], c[11]]),
                    f32::from_le_bytes([c[12], c[13], c[14], c[15]]),
                ]
            })
            .collect())
    }

    /// Read `count` UV pairs (2×f32 each) in one bulk read.
    pub fn read_uv_array(&mut self, count: usize) -> io::Result<Vec<[f32; 2]>> {
        let byte_count = count * 8;
        self.check_alloc(byte_count)?;
        let mut buf = vec![0u8; byte_count];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf
            .chunks_exact(8)
            .map(|c| {
                [
                    f32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                    f32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                ]
            })
            .collect())
    }

    /// Read `count` u16 values in one bulk read.
    pub fn read_u16_array(&mut self, count: usize) -> io::Result<Vec<u16>> {
        let byte_count = count * 2;
        self.check_alloc(byte_count)?;
        let mut buf = vec![0u8; byte_count];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect())
    }

    /// Read `count` u32 values in one bulk read.
    pub fn read_u32_array(&mut self, count: usize) -> io::Result<Vec<u32>> {
        let byte_count = count * 4;
        self.check_alloc(byte_count)?;
        let mut buf = vec![0u8; byte_count];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect())
    }

    /// Read `count` f32 values in one bulk read.
    pub fn read_f32_array(&mut self, count: usize) -> io::Result<Vec<f32>> {
        let byte_count = count * 4;
        self.check_alloc(byte_count)?;
        let mut buf = vec![0u8; byte_count];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect())
    }

    // ── NIF-specific reads ─────────────────────────────────────────────

    /// Read a string. Format depends on version:
    /// - Pre-20.1.0.1: length-prefixed (u32 length + bytes)
    /// - 20.1.0.1+: string table index (u32 → header.strings[index])
    ///
    /// Returns `Arc<str>` so that string-table reads (the common path on
    /// 20.1+ files) are a cheap pointer copy + atomic increment, not a
    /// fresh allocation. The legacy length-prefixed path allocates once.
    ///
    /// NOTE: the threshold `0x14010001` must match the one in
    /// `header.rs` that decides whether to populate `header.strings`.
    /// A mismatch would corrupt reads on 20.1.0.1/20.1.0.2 files.
    pub fn read_string(&mut self) -> io::Result<Option<Arc<str>>> {
        if self.header.version >= NifVersion(0x14010001) {
            // String table index — Arc::clone is just a refcount bump.
            let idx = self.read_i32_le()?;
            if idx < 0 {
                Ok(None)
            } else {
                Ok(self.header.strings.get(idx as usize).cloned())
            }
        } else {
            // Length-prefixed inline string (Morrowind / pre-20.1).
            let len = self.read_u32_le()? as usize;
            if len == 0 {
                return Ok(None);
            }
            let bytes = self.read_bytes(len)?;
            let s = String::from_utf8_lossy(&bytes);
            Ok(Some(Arc::from(s.as_ref())))
        }
    }

    /// Read a sized string (always length-prefixed, ignoring version).
    /// Used in headers and certain block fields.
    ///
    /// Tries zero-copy `String::from_utf8` first; falls back to lossy
    /// replacement only when the bytes contain invalid UTF-8. Avoids
    /// the unconditional copy from `from_utf8_lossy().into_owned()` on
    /// the hot path (NIF strings are almost always valid ASCII). #254.
    pub fn read_sized_string(&mut self) -> io::Result<String> {
        let len = self.read_u32_le()? as usize;
        let bytes = self.read_bytes(len)?;
        match String::from_utf8(bytes) {
            Ok(s) => Ok(s),
            Err(e) => Ok(String::from_utf8_lossy(e.as_bytes()).into_owned()),
        }
    }

    /// Read a short string (u8 length prefix + bytes).
    ///
    /// Same zero-copy-first strategy as `read_sized_string`. #254.
    pub fn read_short_string(&mut self) -> io::Result<String> {
        let len = self.read_u8()? as usize;
        let mut bytes = self.read_bytes(len)?;
        // Short strings include a null terminator — pop it before conversion.
        if bytes.last() == Some(&0) {
            bytes.pop();
        }
        match String::from_utf8(bytes) {
            Ok(s) => Ok(s),
            Err(e) => Ok(String::from_utf8_lossy(e.as_bytes()).into_owned()),
        }
    }

    /// Read a block reference (i32, where -1 = null).
    pub fn read_block_ref(&mut self) -> io::Result<BlockRef> {
        let val = self.read_i32_le()?;
        if val < 0 {
            Ok(BlockRef::NULL)
        } else {
            Ok(BlockRef(val as u32))
        }
    }

    /// Read an array of block references.
    pub fn read_block_ref_list(&mut self) -> io::Result<Vec<BlockRef>> {
        let count = self.read_u32_le()? as usize;
        let mut refs = Vec::with_capacity(count);
        for _ in 0..count {
            refs.push(self.read_block_ref()?);
        }
        Ok(refs)
    }

    // ── Math type reads ────────────────────────────────────────────────

    pub fn read_ni_point3(&mut self) -> io::Result<NiPoint3> {
        Ok(NiPoint3 {
            x: self.read_f32_le()?,
            y: self.read_f32_le()?,
            z: self.read_f32_le()?,
        })
    }

    pub fn read_ni_color(&mut self) -> io::Result<NiColor> {
        Ok(NiColor {
            r: self.read_f32_le()?,
            g: self.read_f32_le()?,
            b: self.read_f32_le()?,
        })
    }

    pub fn read_ni_matrix3(&mut self) -> io::Result<NiMatrix3> {
        let mut rows = [[0.0f32; 3]; 3];
        for row in &mut rows {
            for val in row.iter_mut() {
                *val = self.read_f32_le()?;
            }
        }
        Ok(NiMatrix3 { rows })
    }

    /// Read an NiQuatTransform: translation (3 floats), rotation (4 floats: w,x,y,z), scale (1 float).
    pub fn read_ni_quat_transform(&mut self) -> io::Result<NiQuatTransform> {
        let translation = self.read_ni_point3()?;
        let w = self.read_f32_le()?;
        let x = self.read_f32_le()?;
        let y = self.read_f32_le()?;
        let z = self.read_f32_le()?;
        let scale = self.read_f32_le()?;
        Ok(NiQuatTransform {
            translation,
            rotation: [w, x, y, z],
            scale,
        })
    }

    pub fn read_ni_transform(&mut self) -> io::Result<NiTransform> {
        // Gamebryo serialization order: translation, rotation, scale
        // (see NiAVObject::LoadBinary in Gamebryo 2.3 source)
        let translation = self.read_ni_point3()?;
        let rotation = self.read_ni_matrix3()?;
        let scale = self.read_f32_le()?;
        // Sanitize once at parse time so downstream code can treat the
        // rotation as a valid rotation matrix. See #277.
        let rotation = crate::rotation::sanitize_rotation(rotation);
        Ok(NiTransform {
            rotation,
            translation,
            scale,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal NifHeader for testing stream reads.
    fn test_header(version: NifVersion) -> NifHeader {
        NifHeader {
            version,
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("hello"), Arc::from("world")],
            max_string_length: 5,
            num_groups: 0,
        }
    }

    #[test]
    fn read_primitives() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = vec![
            0x42, // u8: 0x42
            0x34, 0x12, // u16le: 0x1234
            0x78, 0x56, 0x34, 0x12, // u32le: 0x12345678
            0x00, 0x00, 0x80, 0x3F, // f32le: 1.0
        ];
        let mut stream = NifStream::new(&data, &header);

        assert_eq!(stream.read_u8().unwrap(), 0x42);
        assert_eq!(stream.read_u16_le().unwrap(), 0x1234);
        assert_eq!(stream.read_u32_le().unwrap(), 0x12345678);
        assert_eq!(stream.read_f32_le().unwrap(), 1.0);
        assert_eq!(stream.position(), data.len() as u64);
    }

    #[test]
    fn read_block_ref_valid_and_null() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = vec![
            0x05, 0x00, 0x00, 0x00, // i32: 5 (valid ref)
            0xFF, 0xFF, 0xFF, 0xFF, // i32: -1 (null ref)
        ];
        let mut stream = NifStream::new(&data, &header);

        let r1 = stream.read_block_ref().unwrap();
        assert_eq!(r1.index(), Some(5));

        let r2 = stream.read_block_ref().unwrap();
        assert!(r2.is_null());
    }

    #[test]
    fn read_block_ref_list() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = vec![
            0x02, 0x00, 0x00, 0x00, // count: 2
            0x00, 0x00, 0x00, 0x00, // ref 0
            0x03, 0x00, 0x00, 0x00, // ref 3
        ];
        let mut stream = NifStream::new(&data, &header);

        let refs = stream.read_block_ref_list().unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].index(), Some(0));
        assert_eq!(refs[1].index(), Some(3));
    }

    #[test]
    fn read_string_from_table() {
        // Version >= 20.1.0.1 reads from string table
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = vec![
            0x01, 0x00, 0x00, 0x00, // string table index 1
            0xFF, 0xFF, 0xFF, 0xFF, // index -1 (null)
        ];
        let mut stream = NifStream::new(&data, &header);

        assert_eq!(stream.read_string().unwrap().as_deref(), Some("world"));
        assert_eq!(stream.read_string().unwrap(), None);
    }

    #[test]
    fn read_string_table_boundary_at_20_1_0_1() {
        // Regression for #172: string-table dispatch must kick in at
        // exactly 20.1.0.1 per nif.xml, not 20.1.0.3 as it used to.
        // At 20.1.0.1 the reader should take the string-table path.
        let header = test_header(NifVersion(0x14010001));
        let data: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x00, // string table index 0
        ];
        let mut stream = NifStream::new(&data, &header);
        // If the threshold is still 0x14010003 this would fall through
        // to the inline path and try to read 0 bytes → return None.
        // With the corrected threshold we read index 0 → "hello".
        assert_eq!(stream.read_string().unwrap().as_deref(), Some("hello"));
    }

    #[test]
    fn read_string_inline_below_20_1_0_1() {
        // Just below the threshold: 20.1.0.0 must still use inline strings.
        let header = test_header(NifVersion(0x14010000));
        let data: Vec<u8> = vec![
            0x03, 0x00, 0x00, 0x00, // length: 3
            b'f', b'o', b'o', //
        ];
        let mut stream = NifStream::new(&data, &header);
        assert_eq!(stream.read_string().unwrap().as_deref(), Some("foo"));
    }

    #[test]
    fn read_string_inline_old_version() {
        // Version < 20.1.0.3 reads length-prefixed inline
        let header = test_header(NifVersion(0x0A000100)); // 10.0.1.0
        let data: Vec<u8> = vec![
            0x04, 0x00, 0x00, 0x00, // length: 4
            b't', b'e', b's', b't', // "test"
            0x00, 0x00, 0x00, 0x00, // length: 0 (null string)
        ];
        let mut stream = NifStream::new(&data, &header);

        assert_eq!(stream.read_string().unwrap().as_deref(), Some("test"));
        assert_eq!(stream.read_string().unwrap(), None);
    }

    #[test]
    fn read_ni_point3() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = [
            1.0f32.to_le_bytes(),
            2.0f32.to_le_bytes(),
            3.0f32.to_le_bytes(),
        ]
        .concat();
        let mut stream = NifStream::new(&data, &header);

        let p = stream.read_ni_point3().unwrap();
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        assert_eq!(p.z, 3.0);
    }

    #[test]
    fn read_bool_version_dependent() {
        // Per nif.xml, type `bool` is 8-bit from 4.1.0.1 onward and
        // 32-bit for older content.

        // v20.2.0.7 (FO3/FNV/Skyrim+): bool is u8
        let header_new = test_header(NifVersion::V20_2_0_7);
        let data_new: Vec<u8> = vec![0x01];
        let mut stream = NifStream::new(&data_new, &header_new);
        assert!(stream.read_bool().unwrap());
        assert_eq!(stream.position(), 1);

        // v20.0.0.5 (Oblivion): bool is u8 (>= 4.1.0.1)
        let header_oblivion = test_header(NifVersion::V20_0_0_5);
        let data_oblivion: Vec<u8> = vec![0x01];
        let mut stream = NifStream::new(&data_oblivion, &header_oblivion);
        assert!(stream.read_bool().unwrap());
        assert_eq!(stream.position(), 1);

        // v4.0.0.2 (pre-NetImmerse 4.1): bool is u32
        let header_old = test_header(NifVersion::V4_0_0_2);
        let data_old: Vec<u8> = vec![0x01, 0x00, 0x00, 0x00];
        let mut stream = NifStream::new(&data_old, &header_old);
        assert!(stream.read_bool().unwrap());
        assert_eq!(stream.position(), 4);
    }

    #[test]
    fn skip_advances_position() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data = vec![0u8; 100];
        let mut stream = NifStream::new(&data, &header);

        stream.skip(50);
        assert_eq!(stream.position(), 50);
        stream.skip(25);
        assert_eq!(stream.position(), 75);
    }

    #[test]
    fn read_ni_transform_translation_before_rotation() {
        // Regression: NiTransform serialization order is translation, rotation, scale
        // (matches Gamebryo 2.3 NiAVObject::LoadBinary). A previous bug read rotation first.
        let header = test_header(NifVersion::V20_2_0_7);
        let mut data = Vec::new();
        // Translation: (10.0, 20.0, 30.0)
        data.extend_from_slice(&10.0f32.to_le_bytes());
        data.extend_from_slice(&20.0f32.to_le_bytes());
        data.extend_from_slice(&30.0f32.to_le_bytes());
        // Rotation: identity
        for v in &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0f32] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // Scale: 2.5
        data.extend_from_slice(&2.5f32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let t = stream.read_ni_transform().unwrap();

        assert_eq!(t.translation.x, 10.0);
        assert_eq!(t.translation.y, 20.0);
        assert_eq!(t.translation.z, 30.0);
        assert_eq!(t.rotation.rows[0], [1.0, 0.0, 0.0]);
        assert_eq!(t.rotation.rows[1], [0.0, 1.0, 0.0]);
        assert_eq!(t.rotation.rows[2], [0.0, 0.0, 1.0]);
        assert_eq!(t.scale, 2.5);
        // 3 + 9 + 1 = 13 floats = 52 bytes
        assert_eq!(stream.position(), 52);
    }

    #[test]
    fn read_ni_transform_non_identity_rotation() {
        // Regression: ensure a non-trivial rotation doesn't get mixed up with translation.
        let header = test_header(NifVersion::V20_2_0_7);
        let mut data = Vec::new();
        // Translation: (0, 0, 0)
        for _ in 0..3 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // Rotation: 90° around Z (in row-major): [[0,-1,0],[1,0,0],[0,0,1]]
        for v in &[0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0f32] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // Scale: 1.0
        data.extend_from_slice(&1.0f32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let t = stream.read_ni_transform().unwrap();

        assert_eq!(t.translation.x, 0.0);
        assert_eq!(t.rotation.rows[0], [0.0, -1.0, 0.0]);
        assert_eq!(t.rotation.rows[1], [1.0, 0.0, 0.0]);
        assert_eq!(t.scale, 1.0);
    }

    #[test]
    fn skip_within_bounds_succeeds() {
        let data = [0u8; 16];
        let header = test_header(NifVersion::V20_2_0_7);
        let mut stream = NifStream::new(&data, &header);
        assert!(stream.skip(8).is_ok());
        assert_eq!(stream.position(), 8);
        assert!(stream.skip(8).is_ok());
        assert_eq!(stream.position(), 16);
    }

    #[test]
    fn skip_past_end_returns_error() {
        let data = [0u8; 10];
        let header = test_header(NifVersion::V20_2_0_7);
        let mut stream = NifStream::new(&data, &header);
        let err = stream.skip(11).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        // Cursor must NOT have advanced on error.
        assert_eq!(stream.position(), 0);
    }

    #[test]
    fn skip_overflow_returns_error() {
        let data = [0u8; 10];
        let header = test_header(NifVersion::V20_2_0_7);
        let mut stream = NifStream::new(&data, &header);
        stream.skip(5).unwrap();
        let err = stream.skip(u64::MAX).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(stream.position(), 5);
    }

    /// Regression: #113 / audit NIF-13 — `read_bytes` with a size larger
    /// than what remains in the stream must fail before allocating, and
    /// fail specifically with `UnexpectedEof` so block-size recovery can
    /// swallow the error.
    #[test]
    fn read_bytes_oversized_request_errors_before_alloc() {
        let data = [0u8; 64];
        let header = test_header(NifVersion::V20_2_0_7);
        let mut stream = NifStream::new(&data, &header);
        let err = stream.read_bytes(1_000_000).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        // Cursor untouched on the early check.
        assert_eq!(stream.position(), 0);
    }

    /// Regression: #113 / audit NIF-13 — requests over the hard cap
    /// fail with `InvalidData` regardless of how much data the stream
    /// actually has. Guards against a corrupt file that claims e.g. a
    /// 1 GB ByteArray.
    #[test]
    fn read_bytes_over_hard_cap_errors_regardless_of_stream_size() {
        // Backing buffer larger than the cap — pretend we mmapped a
        // huge file — so the only remaining safeguard is the cap.
        let data = vec![0u8; MAX_SINGLE_ALLOC_BYTES + 1];
        let header = test_header(NifVersion::V20_2_0_7);
        let mut stream = NifStream::new(&data, &header);
        let err = stream.read_bytes(MAX_SINGLE_ALLOC_BYTES + 1).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(stream.position(), 0);
    }

    /// Legitimate use at the exact cap must succeed — the cap is
    /// inclusive at the limit.
    #[test]
    fn read_bytes_at_cap_succeeds() {
        let cap = 16; // miniature "cap" for test speed
        let data = vec![0u8; cap];
        let header = test_header(NifVersion::V20_2_0_7);
        let mut stream = NifStream::new(&data, &header);
        let out = stream.read_bytes(cap).unwrap();
        assert_eq!(out.len(), cap);
        assert_eq!(stream.position() as usize, cap);
    }
}
