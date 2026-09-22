//! Binary reader — little-endian, offset-tracking, with the BGSM/BGEM
//! length-prefixed string convention.
//!
//! BGSM strings: `u32 length` + `length` chars, where `length` COUNTS
//! the terminating '\0'. Reader trims the trailing NUL.

use crate::{Error, Result};

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    /// #4672 — set when a string decoded with UTF-8 replacement
    /// (lossy, matching the reference Material-Editor's
    /// replacement-fallback decoder). The parse entry logs once per
    /// file instead of failing the whole material over one bad byte.
    had_replacement: bool,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            pos: 0,
            had_replacement: false,
        }
    }

    /// #4672 — did any string need lossy replacement?
    pub(crate) fn had_replacement(&self) -> bool {
        self.had_replacement
    }

    #[cfg(test)]
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn need(&self, n: usize) -> Result<()> {
        if self.remaining() < n {
            return Err(Error::UnexpectedEof {
                offset: self.pos,
                need: n,
                have: self.remaining(),
            });
        }
        Ok(())
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8> {
        self.need(1)?;
        let v = self.bytes[self.pos];
        self.pos += 1;
        Ok(v)
    }

    pub(crate) fn read_bool(&mut self) -> Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    pub(crate) fn read_u32(&mut self) -> Result<u32> {
        self.need(4)?;
        let v = u32::from_le_bytes([
            self.bytes[self.pos],
            self.bytes[self.pos + 1],
            self.bytes[self.pos + 2],
            self.bytes[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    pub(crate) fn read_f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.read_u32()?))
    }

    /// Read a 3×f32 color triplet.
    pub(crate) fn read_color(&mut self) -> Result<[f32; 3]> {
        Ok([self.read_f32()?, self.read_f32()?, self.read_f32()?])
    }

    /// Read a length-prefixed string. The stored `length` counts the
    /// trailing '\0'; we trim it so Rust strings stay NUL-free.
    /// Empty strings (len=0 OR len=1 + '\0') both return `""`.
    pub(crate) fn read_string(&mut self) -> Result<String> {
        let start = self.pos;
        let len = self.read_u32()?;
        if len == 0 {
            return Ok(String::new());
        }
        // Implausibility bound — string can't exceed what's left of the file.
        if len as usize > self.remaining() {
            return Err(Error::StringTooLong {
                offset: start,
                len,
                remaining: self.remaining(),
            });
        }
        self.need(len as usize)?;
        let slice = &self.bytes[self.pos..self.pos + len as usize];
        self.pos += len as usize;

        // Drop the trailing NUL if present.
        let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
        // #4672 — decode LOSSILY, matching the reference Material-Editor
        // (.NET BinaryReader.ReadChars under a replacement-fallback UTF-8
        // decoder) and this workspace's own BSA/BA2 name tables. The old
        // strict `String::from_utf8` dropped an otherwise-valid material
        // over one non-UTF-8 byte in any string field. `from_utf8_lossy`
        // borrows for valid input, so the Owned arm is the exact
        // "replacement happened" signal.
        let cow = String::from_utf8_lossy(&slice[..end]);
        if matches!(cow, std::borrow::Cow::Owned(_)) {
            self.had_replacement = true;
        }
        Ok(cow.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_string_trims_trailing_nul() {
        // length=4 (counts NUL), bytes = "abc\0"
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(b"abc\0");
        let mut r = Reader::new(&bytes);
        assert_eq!(r.read_string().unwrap(), "abc");
        assert_eq!(r.pos(), 8);
    }

    #[test]
    fn read_string_empty_handles_both_zero_and_one() {
        // length=0 → empty
        let bytes = 0u32.to_le_bytes();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.read_string().unwrap(), "");

        // length=1 + "\0" → empty (after NUL trim)
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.push(0);
        let mut r = Reader::new(&bytes);
        assert_eq!(r.read_string().unwrap(), "");
    }

    /// #4672 — one non-UTF-8 byte must not fail the string read: the
    /// decode is lossy (replacement char), the reader flags it, and the
    /// rest of the field still parses. Byte-level framing (the length
    /// prefix) is unaffected by the replacement, so `pos` advances by
    /// the stored length either way.
    #[test]
    fn read_string_decodes_lossily_and_flags_replacement() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&6u32.to_le_bytes());
        // "ab" + one invalid 0xCD byte + "ef" + NUL; length 6 counts the NUL.
        bytes.extend_from_slice(&[b'a', b'b', 0xCD, b'e', b'f', 0x00]);
        let mut r = Reader::new(&bytes);
        let s = r.read_string().expect("a non-UTF-8 byte must not fail the read");
        assert!(s.contains('\u{FFFD}'), "lossy replacement marker expected: {s:?}");
        assert!(r.had_replacement());

        // Clean string: no flag.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(b"abc\0");
        let mut r = Reader::new(&bytes);
        assert_eq!(r.read_string().unwrap(), "abc");
        assert!(!r.had_replacement());
    }

    #[test]
    fn read_string_rejects_implausible_length() {
        // length = 1 GB but only 4 bytes in buffer.
        let bytes = 0x4000_0000u32.to_le_bytes();
        let mut r = Reader::new(&bytes);
        assert!(matches!(r.read_string(), Err(Error::StringTooLong { .. })));
    }

    #[test]
    fn read_color_yields_three_floats() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&0.5f32.to_le_bytes());
        bytes.extend_from_slice(&0.25f32.to_le_bytes());
        let mut r = Reader::new(&bytes);
        assert_eq!(r.read_color().unwrap(), [1.0, 0.5, 0.25]);
    }
}
