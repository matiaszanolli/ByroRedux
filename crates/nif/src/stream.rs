//! Version-aware binary stream reader for NIF files.
//!
//! NifStream wraps a byte cursor and carries the header context so that
//! version-dependent reads (string format, block references, etc.) are
//! handled in one place rather than scattered through block parsers.

use crate::header::NifHeader;
use crate::types::{BlockRef, NiColor, NiMatrix3, NiPoint3, NiTransform};
use crate::version::NifVersion;
use std::io::{self, Cursor, Read};

/// Binary reader with NIF header context for version-aware parsing.
pub struct NifStream<'a> {
    cursor: Cursor<&'a [u8]>,
    header: &'a NifHeader,
}

impl<'a> NifStream<'a> {
    pub fn new(data: &'a [u8], header: &'a NifHeader) -> Self {
        Self {
            cursor: Cursor::new(data),
            header,
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

    pub fn position(&self) -> u64 {
        self.cursor.position()
    }

    pub fn set_position(&mut self, pos: u64) {
        self.cursor.set_position(pos);
    }

    pub fn skip(&mut self, n: u64) {
        let pos = self.cursor.position();
        self.cursor.set_position(pos + n);
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

    pub fn read_f32_le(&mut self) -> io::Result<f32> {
        let mut buf = [0u8; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(f32::from_le_bytes(buf))
    }

    /// Read a NiBool (version-dependent size).
    /// Post-Oblivion (>= 20.2.0.7): u32. Pre-Oblivion: u8.
    /// Used for NiNode children, NiTriBasedGeom, etc.
    pub fn read_bool(&mut self) -> io::Result<bool> {
        if self.header.version >= NifVersion::V20_2_0_7 {
            // Post-Oblivion: NiBool is u32
            Ok(self.read_u32_le()? != 0)
        } else {
            // Pre-Oblivion: NiBool is u8
            Ok(self.read_u8()? != 0)
        }
    }

    /// Read a 1-byte boolean (`bool` type in niftools, NOT `NiBool`).
    /// NiGeometryData and related blocks use 1-byte bools for
    /// has_vertices, has_normals, has_colors, etc. in all versions.
    pub fn read_byte_bool(&mut self) -> io::Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    pub fn read_bytes(&mut self, len: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf)
    }

    // ── NIF-specific reads ─────────────────────────────────────────────

    /// Read a string. Format depends on version:
    /// - Pre-20.1: length-prefixed (u32 length + bytes)
    /// - 20.1+: string table index (u32 → header.strings[index])
    pub fn read_string(&mut self) -> io::Result<Option<String>> {
        if self.header.version >= NifVersion(0x14010003) {
            // String table index
            let idx = self.read_i32_le()?;
            if idx < 0 {
                Ok(None)
            } else {
                Ok(self.header.strings.get(idx as usize).cloned())
            }
        } else {
            // Length-prefixed inline string
            let len = self.read_u32_le()? as usize;
            if len == 0 {
                return Ok(None);
            }
            let bytes = self.read_bytes(len)?;
            Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
        }
    }

    /// Read a sized string (always length-prefixed, ignoring version).
    /// Used in headers and certain block fields.
    pub fn read_sized_string(&mut self) -> io::Result<String> {
        let len = self.read_u32_le()? as usize;
        let bytes = self.read_bytes(len)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Read a short string (u8 length prefix + bytes).
    pub fn read_short_string(&mut self) -> io::Result<String> {
        let len = self.read_u8()? as usize;
        let bytes = self.read_bytes(len)?;
        // Short strings include a null terminator
        let s = if bytes.last() == Some(&0) {
            String::from_utf8_lossy(&bytes[..bytes.len() - 1]).into_owned()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        Ok(s)
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

    pub fn read_ni_transform(&mut self) -> io::Result<NiTransform> {
        // Gamebryo serialization order: translation, rotation, scale
        // (see NiAVObject::LoadBinary in Gamebryo 2.3 source)
        let translation = self.read_ni_point3()?;
        let rotation = self.read_ni_matrix3()?;
        let scale = self.read_f32_le()?;
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
            strings: vec!["hello".to_string(), "world".to_string()],
            max_string_length: 5,
            num_groups: 0,
        }
    }

    #[test]
    fn read_primitives() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = vec![
            0x42,                               // u8: 0x42
            0x34, 0x12,                          // u16le: 0x1234
            0x78, 0x56, 0x34, 0x12,              // u32le: 0x12345678
            0x00, 0x00, 0x80, 0x3F,              // f32le: 1.0
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
            0x05, 0x00, 0x00, 0x00,              // i32: 5 (valid ref)
            0xFF, 0xFF, 0xFF, 0xFF,              // i32: -1 (null ref)
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
            0x02, 0x00, 0x00, 0x00,              // count: 2
            0x00, 0x00, 0x00, 0x00,              // ref 0
            0x03, 0x00, 0x00, 0x00,              // ref 3
        ];
        let mut stream = NifStream::new(&data, &header);

        let refs = stream.read_block_ref_list().unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].index(), Some(0));
        assert_eq!(refs[1].index(), Some(3));
    }

    #[test]
    fn read_string_from_table() {
        // Version >= 20.1.0.3 reads from string table
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = vec![
            0x01, 0x00, 0x00, 0x00,              // string table index 1
            0xFF, 0xFF, 0xFF, 0xFF,              // index -1 (null)
        ];
        let mut stream = NifStream::new(&data, &header);

        assert_eq!(stream.read_string().unwrap(), Some("world".to_string()));
        assert_eq!(stream.read_string().unwrap(), None);
    }

    #[test]
    fn read_string_inline_old_version() {
        // Version < 20.1.0.3 reads length-prefixed inline
        let header = test_header(NifVersion(0x0A000100)); // 10.0.1.0
        let data: Vec<u8> = vec![
            0x04, 0x00, 0x00, 0x00,              // length: 4
            b't', b'e', b's', b't',              // "test"
            0x00, 0x00, 0x00, 0x00,              // length: 0 (null string)
        ];
        let mut stream = NifStream::new(&data, &header);

        assert_eq!(stream.read_string().unwrap(), Some("test".to_string()));
        assert_eq!(stream.read_string().unwrap(), None);
    }

    #[test]
    fn read_ni_point3() {
        let header = test_header(NifVersion::V20_2_0_7);
        let data: Vec<u8> = [
            1.0f32.to_le_bytes(),
            2.0f32.to_le_bytes(),
            3.0f32.to_le_bytes(),
        ].concat();
        let mut stream = NifStream::new(&data, &header);

        let p = stream.read_ni_point3().unwrap();
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        assert_eq!(p.z, 3.0);
    }

    #[test]
    fn read_bool_version_dependent() {
        // v20.2.0.7: bool is u32
        let header_new = test_header(NifVersion::V20_2_0_7);
        let data_new: Vec<u8> = vec![0x01, 0x00, 0x00, 0x00];
        let mut stream = NifStream::new(&data_new, &header_new);
        assert!(stream.read_bool().unwrap());
        assert_eq!(stream.position(), 4); // consumed 4 bytes

        // v10.0.1.0: bool is u8
        let header_old = test_header(NifVersion(0x0A000100));
        let data_old: Vec<u8> = vec![0x01];
        let mut stream = NifStream::new(&data_old, &header_old);
        assert!(stream.read_bool().unwrap());
        assert_eq!(stream.position(), 1); // consumed 1 byte
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
}
