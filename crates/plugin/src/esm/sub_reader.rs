//! Sequential cursor for sub-record decoding.
//!
//! ESM sub-records are dense little-endian byte blobs with fixed-offset
//! field layouts. Pre-cursor every walker did `read_u32_at(&data, OFFSET)`
//! pairs that scattered the implicit field order across magic offsets,
//! and any insertion or removal of a field forced every subsequent offset
//! to shift in lockstep — easy to miss, easy to break.
//!
//! [`SubReader`] reads sequentially, so a field's position in source
//! mirrors its position on disk. The schema lives in the read order
//! instead of in the comment block above it.
//!
//! ## Strict vs lenient
//!
//! Two read flavors are provided side by side:
//!
//! * `u32()` / `f32()` / `i32()` / etc. — `Result`-flavored. Errors on
//!   truncation. Use these in code that should propagate parse errors
//!   up to the caller.
//! * `u32_or_default()` / `f32_or_default()` / etc. — `unwrap_or(0)` /
//!   `unwrap_or(0.0)` shorthand for the existing convention where every
//!   walker tolerates truncated DATA blobs by zero-filling missing
//!   fields. Direct drop-in replacement for the `read_u32_at(&data, OFFSET).unwrap_or(0)`
//!   pattern.
//!
//! Soft reads do **not** advance the cursor on truncation — once a read
//! fails, every subsequent read also fails. This matches the pre-cursor
//! behaviour where `read_*_at` calls always read from absolute offsets,
//! not from a moving cursor.
//!
//! ## FormID remap
//!
//! [`SubReader`] is a pure byte decoder; it knows nothing about the
//! [`crate::esm::reader::EsmReader`]'s FormID mod-index remap. Callers
//! that read cross-record FormID references (XCIM, XLOC, XOWN, NAME,
//! etc.) must wrap the raw `u32` through `reader.remap_form_id(raw)`
//! at the call site, the same as before. See `EsmReader::remap_form_id`.
//!
//! ## See also
//!
//! Replaces inline `read_u32_at` / `read_f32_at` / `from_le_bytes`
//! patterns in:
//! * [`crate::esm::cell::walkers`]
//! * [`crate::esm::records::items`]
//! * [`crate::esm::records::misc`]
//! * [`crate::esm::records::actor`]
//! * etc.

use anyhow::{bail, Result};

/// Sequential cursor over a sub-record's data buffer.
///
/// See module docs for usage. Read order mirrors disk order; truncation
/// on a strict read leaves the cursor in place so the caller can
/// inspect [`Self::position`] / [`Self::remaining`] for diagnostics.
#[derive(Debug, Clone)]
pub struct SubReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> SubReader<'a> {
    /// Wrap a slice. Cursor starts at offset 0.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Current cursor offset (0-based byte index into the original slice).
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Bytes left to read from the cursor onward.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// `true` when no bytes remain.
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Total length of the underlying slice (does not change as the
    /// cursor advances).
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// The unread tail. Useful when handing the rest off to a sub-decoder.
    pub fn rest(&self) -> &'a [u8] {
        &self.data[self.pos.min(self.data.len())..]
    }

    /// Advance past `n` bytes. Errors if fewer than `n` bytes remain.
    pub fn skip(&mut self, n: usize) -> Result<()> {
        if self.remaining() < n {
            bail!(
                "SubReader::skip({n}): only {} bytes remaining (pos={}, len={})",
                self.remaining(),
                self.pos,
                self.data.len()
            );
        }
        self.pos += n;
        Ok(())
    }

    /// Best-effort skip — caps at the end of the buffer instead of erroring.
    /// Useful in lenient sequential-read paths where you'd otherwise
    /// `.skip(n).ok();`.
    pub fn skip_or_eof(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.data.len());
    }

    // ── Strict reads (advance only on success) ──────────────────────────

    /// Read one `u8`. Errors on truncation.
    pub fn u8(&mut self) -> Result<u8> {
        if self.remaining() < 1 {
            bail!("SubReader::u8: truncated at pos {}", self.pos);
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    /// Read one little-endian `u16`. Errors on truncation.
    pub fn u16(&mut self) -> Result<u16> {
        let bytes = self.fixed::<2>()?;
        Ok(u16::from_le_bytes(bytes))
    }

    /// Read one little-endian `u32`. Errors on truncation.
    pub fn u32(&mut self) -> Result<u32> {
        let bytes = self.fixed::<4>()?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Read one little-endian `i16`. Errors on truncation.
    pub fn i16(&mut self) -> Result<i16> {
        let bytes = self.fixed::<2>()?;
        Ok(i16::from_le_bytes(bytes))
    }

    /// Read one little-endian `i32`. Errors on truncation.
    pub fn i32(&mut self) -> Result<i32> {
        let bytes = self.fixed::<4>()?;
        Ok(i32::from_le_bytes(bytes))
    }

    /// Read one little-endian `f32`. Errors on truncation.
    pub fn f32(&mut self) -> Result<f32> {
        let bytes = self.fixed::<4>()?;
        Ok(f32::from_le_bytes(bytes))
    }

    /// Read a fixed-length byte array. The const generic is the field
    /// length, so type inference makes this `r.fixed::<3>()` →
    /// `[u8; 3]`. Errors on truncation.
    pub fn fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        if self.remaining() < N {
            bail!(
                "SubReader::fixed::<{N}>: truncated at pos {} (remaining={})",
                self.pos,
                self.remaining()
            );
        }
        let mut out = [0u8; N];
        out.copy_from_slice(&self.data[self.pos..self.pos + N]);
        self.pos += N;
        Ok(out)
    }

    /// Read `N` consecutive `f32`s into an array. Errors on truncation.
    pub fn f32_array<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut out = [0.0f32; N];
        for slot in &mut out {
            *slot = self.f32()?;
        }
        Ok(out)
    }

    /// Read a 4-byte RGBA color stored as `[u8; 4]` and return the RGB
    /// channels normalized to `[0.0, 1.0]`. The 4th byte is consumed but
    /// discarded — common pattern for XCLL ambient/directional/fog
    /// colors where the alpha byte is unused padding.
    /// Errors on truncation.
    pub fn rgb_color(&mut self) -> Result<[f32; 3]> {
        let bytes = self.fixed::<4>()?;
        Ok([
            bytes[0] as f32 / 255.0,
            bytes[1] as f32 / 255.0,
            bytes[2] as f32 / 255.0,
        ])
    }

    /// Like [`Self::rgb_color`] but also returns the 4th byte normalized
    /// as the alpha channel. Errors on truncation.
    pub fn rgba_color(&mut self) -> Result<[f32; 4]> {
        let bytes = self.fixed::<4>()?;
        Ok([
            bytes[0] as f32 / 255.0,
            bytes[1] as f32 / 255.0,
            bytes[2] as f32 / 255.0,
            bytes[3] as f32 / 255.0,
        ])
    }

    // ── Lenient reads (zero-default on truncation, do NOT advance) ──────
    //
    // Direct drop-ins for the `read_u32_at(&data, X).unwrap_or(0)` style.
    // On failure the cursor stays put, so the next read also fails — same
    // semantics as the pre-cursor absolute-offset reads where every read
    // is independent.

    /// Lenient `u8` — returns 0 on truncation.
    pub fn u8_or_default(&mut self) -> u8 {
        self.u8().unwrap_or(0)
    }

    /// Lenient `u16` — returns 0 on truncation.
    pub fn u16_or_default(&mut self) -> u16 {
        self.u16().unwrap_or(0)
    }

    /// Lenient `u32` — returns 0 on truncation.
    pub fn u32_or_default(&mut self) -> u32 {
        self.u32().unwrap_or(0)
    }

    /// Lenient `i16` — returns 0 on truncation.
    pub fn i16_or_default(&mut self) -> i16 {
        self.i16().unwrap_or(0)
    }

    /// Lenient `i32` — returns 0 on truncation.
    pub fn i32_or_default(&mut self) -> i32 {
        self.i32().unwrap_or(0)
    }

    /// Lenient `f32` — returns 0.0 on truncation.
    pub fn f32_or_default(&mut self) -> f32 {
        self.f32().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_reader_is_empty() {
        let r = SubReader::new(&[]);
        assert!(r.is_empty());
        assert_eq!(r.remaining(), 0);
        assert_eq!(r.position(), 0);
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn sequential_strict_reads_advance() {
        let data: [u8; 11] = [
            0x42, // u8
            0x34, 0x12, // u16 = 0x1234
            0x78, 0x56, 0x34, 0x12, // u32 = 0x12345678
            0x00, 0x00, 0x80, 0x3f, // f32 = 1.0
        ];
        let mut r = SubReader::new(&data);
        assert_eq!(r.u8().unwrap(), 0x42);
        assert_eq!(r.u16().unwrap(), 0x1234);
        assert_eq!(r.u32().unwrap(), 0x1234_5678);
        assert_eq!(r.f32().unwrap(), 1.0);
        assert!(r.is_empty());
        assert_eq!(r.position(), 11);
    }

    #[test]
    fn truncated_strict_read_errors_and_does_not_advance() {
        let data = [0x01u8, 0x02];
        let mut r = SubReader::new(&data);
        // Two bytes can satisfy a u16 but not a u32.
        assert!(r.u32().is_err(), "u32 must error on 2-byte buffer");
        assert_eq!(
            r.position(),
            0,
            "failed strict read must leave cursor untouched"
        );
        // Falling through to a u16 still works.
        assert_eq!(r.u16().unwrap(), 0x0201);
    }

    #[test]
    fn lenient_reads_zero_default_on_truncate_and_stick() {
        let mut r = SubReader::new(&[]);
        assert_eq!(r.u32_or_default(), 0);
        assert_eq!(r.f32_or_default(), 0.0);
        // Cursor still at 0 — failed reads don't advance.
        assert_eq!(r.position(), 0);
    }

    #[test]
    fn skip_advances_or_errors() {
        let mut r = SubReader::new(&[0u8; 10]);
        r.skip(4).unwrap();
        assert_eq!(r.position(), 4);
        assert!(r.skip(100).is_err());
        assert_eq!(r.position(), 4, "failed skip must not advance");
    }

    #[test]
    fn skip_or_eof_caps_at_buffer_end() {
        let mut r = SubReader::new(&[0u8; 10]);
        r.skip_or_eof(100);
        assert_eq!(r.position(), 10);
        assert!(r.is_empty());
    }

    #[test]
    fn fixed_reads_const_generic_array() {
        let data = [0xDE, 0xAD, 0xBE, 0xEF];
        let mut r = SubReader::new(&data);
        let arr = r.fixed::<4>().unwrap();
        assert_eq!(arr, [0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn f32_array_reads_n_consecutive_floats() {
        // [1.0, 2.0, 3.0]
        let mut data = Vec::new();
        for v in [1.0f32, 2.0, 3.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut r = SubReader::new(&data);
        let arr = r.f32_array::<3>().unwrap();
        assert_eq!(arr, [1.0, 2.0, 3.0]);
        assert!(r.is_empty());
    }

    #[test]
    fn rgb_color_normalizes_first_three_bytes_and_consumes_fourth() {
        // 4 bytes: 255, 128, 64, 99 → RGB = [1.0, ~0.502, ~0.251]
        let data = [255u8, 128, 64, 99];
        let mut r = SubReader::new(&data);
        let c = r.rgb_color().unwrap();
        assert!((c[0] - 1.0).abs() < 1e-6);
        assert!((c[1] - 128.0 / 255.0).abs() < 1e-6);
        assert!((c[2] - 64.0 / 255.0).abs() < 1e-6);
        assert_eq!(r.position(), 4, "alpha byte must be consumed");
    }

    #[test]
    fn rgba_color_returns_alpha_in_fourth_slot() {
        let data = [255u8, 0, 0, 128];
        let mut r = SubReader::new(&data);
        let c = r.rgba_color().unwrap();
        assert!((c[0] - 1.0).abs() < 1e-6);
        assert_eq!(c[1], 0.0);
        assert_eq!(c[2], 0.0);
        assert!((c[3] - 128.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn rest_returns_unread_tail() {
        let mut r = SubReader::new(&[1u8, 2, 3, 4, 5]);
        r.skip(2).unwrap();
        assert_eq!(r.rest(), &[3, 4, 5]);
    }

    /// Demonstration: the XCLL prefix walk that lives in
    /// `cell/walkers.rs` collapses from a wall of magic offsets into
    /// a sequential decode that mirrors the on-disk schema.
    #[test]
    fn xcll_prefix_decode_demo() {
        // Build the 28-byte XCLL prefix:
        //   ambient RGB(A) | directional RGB(A) | fog-near RGB(A)
        //   fog_near f32   | fog_far f32        | rot_x i32 | rot_y i32
        let mut data = Vec::new();
        data.extend_from_slice(&[255, 128, 64, 0]); // ambient
        data.extend_from_slice(&[64, 128, 255, 0]); // directional
        data.extend_from_slice(&[200, 200, 200, 0]); // fog color
        data.extend_from_slice(&50.0f32.to_le_bytes()); // fog near
        data.extend_from_slice(&500.0f32.to_le_bytes()); // fog far
        data.extend_from_slice(&45i32.to_le_bytes()); // rot x
        data.extend_from_slice(&30i32.to_le_bytes()); // rot y

        let mut r = SubReader::new(&data);
        let ambient = r.rgb_color().unwrap();
        let directional = r.rgb_color().unwrap();
        let fog_color = r.rgb_color().unwrap();
        let fog_near = r.f32().unwrap();
        let fog_far = r.f32().unwrap();
        let rot_x = r.i32().unwrap();
        let rot_y = r.i32().unwrap();

        assert!((ambient[0] - 1.0).abs() < 1e-6);
        assert!((directional[2] - 1.0).abs() < 1e-6);
        assert!((fog_color[1] - 200.0 / 255.0).abs() < 1e-6);
        assert_eq!(fog_near, 50.0);
        assert_eq!(fog_far, 500.0);
        assert_eq!(rot_x, 45);
        assert_eq!(rot_y, 30);
        assert!(r.is_empty(), "exact-fit read must consume the buffer");
    }

    /// Demonstration: a Skyrim ARMO DATA decode (12 bytes: value u32,
    /// health u32, weight f32) — a recurring shape that pre-cursor lived
    /// as three `read_u32_at` / `read_f32_at` calls with manually
    /// offset 0/4/8 indices.
    #[test]
    fn armo_data_skyrim_demo() {
        let mut data = Vec::new();
        data.extend_from_slice(&100u32.to_le_bytes()); // value
        data.extend_from_slice(&500u32.to_le_bytes()); // health
        data.extend_from_slice(&5.5f32.to_le_bytes()); // weight

        let mut r = SubReader::new(&data);
        let value = r.u32_or_default();
        let health = r.u32_or_default();
        let weight = r.f32_or_default();
        assert_eq!(value, 100);
        assert_eq!(health, 500);
        assert_eq!(weight, 5.5);
    }

    /// Lenient reads on a truncated buffer must zero-default *and*
    /// leave the cursor wedged so subsequent reads also see zeros — the
    /// existing convention every call site relies on.
    #[test]
    fn lenient_chain_after_truncation_keeps_zero_filling() {
        let data = [0u8, 0]; // 2 bytes — can't satisfy a u32
        let mut r = SubReader::new(&data);
        assert_eq!(r.u32_or_default(), 0);
        assert_eq!(r.position(), 0, "lenient miss must not advance");
        assert_eq!(r.f32_or_default(), 0.0);
        assert_eq!(r.position(), 0);
    }
}
