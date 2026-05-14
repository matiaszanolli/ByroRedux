//! Low-level byte / half-float / normal decoders.
//!
//! Used by every mesh extractor; pinned to a single file so the bit-twiddling
//! lives in one place.





pub fn read_f32_le(bytes: &[u8], offset: usize) -> Option<f32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(f32::from_le_bytes(slice.try_into().ok()?))
}

#[inline]
pub fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes(slice.try_into().ok()?))
}

#[inline]
pub fn half_to_f32(h: u16) -> f32 {
    // Same IEEE 754 binary16 decode as `tri_shape::half_to_f32`.
    // Re-declared so `import/mesh.rs` doesn't depend on a
    // `pub(crate)` export in tri_shape that might churn.
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as i32;
    let mant = (h & 0x3FF) as u32;
    let bits = if exp == 0 {
        if mant == 0 {
            sign << 31
        } else {
            // Subnormal — normalise.
            let mut m = mant;
            let mut e = -14_i32;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= 0x3FF;
            (sign << 31) | (((e + 127) as u32) << 23) | (m << 13)
        }
    } else if exp == 31 {
        // Inf / NaN — preserve mantissa for NaN payloads.
        (sign << 31) | (0xFFu32 << 23) | (mant << 13)
    } else {
        (sign << 31) | (((exp - 15 + 127) as u32) << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

#[inline]
pub fn byte_to_normal(b: u8) -> f32 {
    // Same `(b / 127.5) - 1.0` as `tri_shape::byte_to_normal`.
    (b as f32 / 127.5) - 1.0
}

