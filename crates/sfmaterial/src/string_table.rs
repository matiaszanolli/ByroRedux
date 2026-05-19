use crate::{Error, Result};

/// `STRT` chunk reader. The chunk payload is a flat blob of
/// NUL-terminated ASCII strings; class names + field names are
/// addressed by byte offset into this blob.
///
/// Gibbed's `StringTable.Get(offset)` seeks to `offset` and reads
/// a NUL-terminated ASCII string. We pre-decode lazily by walking
/// from `offset` until a NUL — cheap because the table is small
/// (~tens of KB on vanilla).
#[derive(Debug)]
pub struct StringTable {
    bytes: Vec<u8>,
}

impl StringTable {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// Resolve a STRT offset. Empty string when `offset == 0` (Gibbed
    /// treats negative as oob; we mirror that — vanilla CDB never
    /// authors a negative offset).
    pub fn get(&self, offset: i32) -> Result<String> {
        if offset < 0 || (offset as usize) >= self.bytes.len() {
            return Err(Error::StringTableOob {
                offset,
                len: self.bytes.len(),
            });
        }
        let start = offset as usize;
        let nul = self.bytes[start..]
            .iter()
            .position(|&b| b == 0)
            .map(|p| start + p)
            .unwrap_or(self.bytes.len());
        // CDB strings are documented as ASCII; lossy UTF-8 decode is a
        // safe superset and avoids a separate ASCII validation pass.
        Ok(String::from_utf8_lossy(&self.bytes[start..nul]).into_owned())
    }

    pub fn raw(&self) -> &[u8] {
        &self.bytes
    }
}
