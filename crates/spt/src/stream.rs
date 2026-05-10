//! Tiny little-endian byte reader for the `.spt` parser.
//!
//! Mirrors the surface of `byroredux_nif::stream::NifStream` but
//! without the version-aware string handling — `.spt` is a single
//! version (every file ships `__IdvSpt_02_`), so the readers here
//! are unconditional.
//!
//! Position-overrun is reported as `io::Error` rather than panicking;
//! the parser surfaces those as `Err(SptParseError::Truncated)` with
//! an offset stamp so consumers can pinpoint where the corruption
//! lives.

use std::io;

/// Cursor over a `.spt` byte slice with explicit overrun checking.
pub struct SptStream<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> SptStream<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// Current byte offset.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Total stream length.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Number of bytes left to read.
    pub fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    /// True at end-of-stream (no more bytes available).
    pub fn is_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    /// Read `n` bytes verbatim. Returns `UnexpectedEof` if the
    /// stream doesn't have that many bytes left.
    pub fn read_bytes(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "spt stream: requested {n} bytes at offset {} but only {} remain",
                    self.pos,
                    self.remaining(),
                ),
            ));
        }
        let out = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    pub fn read_u8(&mut self) -> io::Result<u8> {
        let b = self.read_bytes(1)?;
        Ok(b[0])
    }

    pub fn read_u32_le(&mut self) -> io::Result<u32> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn read_f32_le(&mut self) -> io::Result<f32> {
        Ok(f32::from_bits(self.read_u32_le()?))
    }

    pub fn read_vec3_le(&mut self) -> io::Result<[f32; 3]> {
        Ok([
            self.read_f32_le()?,
            self.read_f32_le()?,
            self.read_f32_le()?,
        ])
    }

    /// Read a `u32` length prefix followed by `length` bytes,
    /// returning the bytes as a UTF-8 string (lossy on invalid
    /// sequences). Length is clamped at 64 KiB to keep a corrupt
    /// length value from allocating gigabytes — `.spt` strings
    /// observed in vanilla content max out at ~525 B.
    pub fn read_string_lp(&mut self) -> io::Result<String> {
        let len = self.read_u32_le()? as usize;
        if len > 65_536 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "spt string at offset {}: length {} exceeds 64 KiB sanity cap",
                    self.pos.saturating_sub(4),
                    len,
                ),
            ));
        }
        let bytes = self.read_bytes(len)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    /// Peek the next 4 bytes as a u32 without advancing.
    pub fn peek_u32_le(&self) -> Option<u32> {
        if self.remaining() < 4 {
            return None;
        }
        let s = self.pos;
        Some(u32::from_le_bytes([
            self.bytes[s],
            self.bytes[s + 1],
            self.bytes[s + 2],
            self.bytes[s + 3],
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_u32_advances_cursor() {
        let bytes = [0xEA, 0x03, 0x00, 0x00, 0x42, 0x00];
        let mut s = SptStream::new(&bytes);
        assert_eq!(s.read_u32_le().unwrap(), 1002);
        assert_eq!(s.position(), 4);
        assert_eq!(s.read_u8().unwrap(), 0x42);
    }

    #[test]
    fn read_string_lp_round_trip() {
        let mut bytes = vec![];
        bytes.extend_from_slice(&12u32.to_le_bytes());
        bytes.extend_from_slice(b"__IdvSpt_02_");
        let mut s = SptStream::new(&bytes);
        assert_eq!(s.read_string_lp().unwrap(), "__IdvSpt_02_");
        assert!(s.is_eof());
    }

    #[test]
    fn read_string_lp_rejects_oversized_length() {
        let bytes = [0x00, 0x00, 0x10, 0x00]; // u32 LE = 0x00100000 > 65 536
        let mut s = SptStream::new(&bytes);
        let err = s.read_string_lp().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn read_vec3_le_round_trip() {
        let mut bytes = vec![];
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&2.0f32.to_le_bytes());
        bytes.extend_from_slice(&3.0f32.to_le_bytes());
        let mut s = SptStream::new(&bytes);
        assert_eq!(s.read_vec3_le().unwrap(), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn underflow_reports_unexpected_eof() {
        let bytes = [0x01, 0x02];
        let mut s = SptStream::new(&bytes);
        let err = s.read_u32_le().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn peek_u32_le_does_not_advance() {
        let bytes = [0xEA, 0x03, 0x00, 0x00];
        let s = SptStream::new(&bytes);
        assert_eq!(s.peek_u32_le(), Some(1002));
        assert_eq!(s.position(), 0);
    }
}
