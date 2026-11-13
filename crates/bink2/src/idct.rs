//! Fixed-point 8x8 inverse DCT (wiki formula, confirmed bit-exact by P12) and dequantisation.

use crate::Error;
use std::ops::{Add, Shr, Sub};

/// P12: coefficient scan order.
pub(crate) const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7,
    14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39,
    46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// The integer types the residual is reconstructed in: the same arithmetic in either while nothing
/// overflows. i32 when every coefficient of a macroblock's plane is below `NARROW`, else i64.
pub(crate) trait Coef:
    Copy + Default + Add<Output = Self> + Sub<Output = Self> + Shr<u32, Output = Self>
{
    /// A value the caller knows fits.
    fn of(v: i64) -> Self;
    fn wide(self) -> i64;

    /// `idct_block`, then the final `>> 6`.
    fn idct_px(c: &mut [Self; 64]) {
        idct_block(c);
        for v in c.iter_mut() {
            *v = *v >> 6;
        }
    }

    /// `px[i] = clamp(px[i] + g[i], 0, 255)`.
    fn add_clamped(px: &mut [u8], g: &[Self]) {
        for (d, &v) in px.iter_mut().zip(g) {
            *d = (*d as i64 + v.wide()).clamp(0, 255) as u8;
        }
    }

    /// A block's pixels clamped to 0..255.
    fn clamp_u8(px: &[Self; 64]) -> [u8; 64] {
        px.map(|v| v.wide().clamp(0, 255) as u8)
    }
}
impl Coef for i32 {
    #[inline]
    fn of(v: i64) -> Self {
        v as i32
    }
    #[inline]
    fn wide(self) -> i64 {
        self as i64
    }
    #[inline]
    fn idct_px(c: &mut [Self; 64]) {
        if !crate::simd::idct_px(c) {
            idct_block(c);
            for v in c.iter_mut() {
                *v >>= 6;
            }
        }
    }
    #[inline]
    fn add_clamped(px: &mut [u8], g: &[Self]) {
        if !px.len().is_multiple_of(16) || !crate::simd::add_clamped(px, g) {
            for (d, &v) in px.iter_mut().zip(g) {
                *d = (*d as i32 + v).clamp(0, 255) as u8;
            }
        }
    }
    #[inline]
    fn clamp_u8(px: &[Self; 64]) -> [u8; 64] {
        let mut out = [0u8; 64];
        if !crate::simd::clamp_u8(px, &mut out) {
            out = px.map(|v| v.clamp(0, 255) as u8);
        }
        out
    }
}
impl Coef for i64 {
    #[inline]
    fn of(v: i64) -> Self {
        v
    }
    #[inline]
    fn wide(self) -> i64 {
        self
    }
}

#[inline]
fn a<T: Coef>(v: T) -> T {
    v + (v >> 2)
}
#[inline]
fn b<T: Coef>(v: T) -> T {
    v >> 1
}
#[inline]
fn c<T: Coef>(v: T) -> T {
    v - (v >> 2) - (v >> 4)
}
#[inline]
fn d<T: Coef>(v: T) -> T {
    v + (v >> 2) - (v >> 4)
}
#[inline]
fn e<T: Coef>(v: T) -> T {
    v >> 2
}

#[inline]
fn idct8<T: Coef>(s: [T; 8]) -> [T; 8] {
    let t00 = s[3] + s[5];
    let t01 = s[3] - s[5];
    let t02 = a(s[2]) + b(s[6]);
    let t03 = b(s[2]) - a(s[6]);
    let t0 = s[0] + s[4] + t02;
    let t1 = s[0] + s[4] - t02;
    let t2 = s[0] - s[4];
    let t3 = s[1] + t00;
    let t4 = s[1] - t00;
    let t5 = t01 + s[7];
    let t6 = t01 - s[7];
    let t7 = t4 + c(t6);
    let t8 = c(t4) - t6;
    let t9 = d(t3) + e(t5);
    let t10 = e(t3) - d(t5);
    let t11 = t2 + t03;
    let t12 = t2 - t03;
    [t0 + t9, t11 + t7, t12 + t8, t1 + t10, t1 - t10, t12 - t8, t11 - t7, t0 - t9]
}

/// Rows first, then columns (P12), in place: the column outputs before the final `>> 6`, i.e. in units
/// of 1/64 pixel.
pub(crate) fn idct_block<T: Coef>(c: &mut [T; 64]) {
    for row in c.chunks_exact_mut(8) {
        let v = idct8(row.try_into().unwrap());
        row.copy_from_slice(&v);
    }
    for col in 0..8 {
        let v = idct8(std::array::from_fn(|r| c[r * 8 + col]));
        for (r, x) in v.into_iter().enumerate() {
            c[r * 8 + col] = x;
        }
    }
}

/// Below this magnitude of every coefficient the IDCT stays inside i32 (its gain over both passes is
/// under 70), and so does the residual after it: the in-macroblock filter (P25) at most quadruples
/// it, the prediction adds at most 255.
pub(crate) const NARROW: u64 = 1 << 24;

/// One block's pixels before the final clamp — the IDCT outputs >> 6 (P12) — from its DC term and its
/// dequantised AC as (raster index, value). A DC-only block is flat.
pub(crate) fn block_pixels<T: Coef>(dc_term: i64, ac: &[(u8, i64)], out: &mut [T; 64]) {
    if ac.is_empty() {
        *out = [T::of(dc_term >> 6); 64];
        return;
    }
    *out = [T::default(); 64];
    out[0] = T::of(dc_term);
    for &(z, v) in ac {
        out[z as usize] = T::of(v);
    }
    T::idct_px(out);
}

/// The reference keeps exact arithmetic only below this magnitude per dequantised value; beyond it the
/// stream is desynchronised or in the unprobed extreme range (STATE.md, open item 3).
const LIMIT_BITS: u32 = 40;

#[inline]
fn shifted(product: i64, shift: i32) -> Result<i64, Error> {
    if product == 0 {
        return Ok(0);
    }
    if shift < 0 {
        return Err(Error::NegativeQuantiser);
    }
    let bits = 64 - product.unsigned_abs().leading_zeros();
    if bits + shift as u32 > LIMIT_BITS {
        return Err(Error::Overflow("dequantised coefficient"));
    }
    Ok(product << shift)
}

/// round(2^(m/4) * 2^19): the DC multiplier's fixed-point roots (P28).
const DC_ROOT: [i64; 4] = [524288, 623487, 741455, 881744];

/// DC, for a quantiser already clamped at 8 (P16): magnitude first, then the sign (P13). The
/// multiplier is 4 x round(2^(q/4) * 1024) with 2^((q & 3) / 4) taken at 19 fractional bits (P28;
/// P13's `DC_QUANT[q & 3] << (q >> 2)` differs at q = 14, 15, 18, 19, 22, 23).
pub(crate) fn dequant_dc(level: i32, q: i32) -> Result<i64, Error> {
    let s = q >> 2;
    if !(0..=40).contains(&s) {
        return Err(Error::Overflow("DC quantiser"));
    }
    let d = ((DC_ROOT[(q & 3) as usize] << s) + 256) >> 9;
    let v = (shifted(level.unsigned_abs() as i64 * d, 0)? + 512) >> 10;
    Ok(if level < 0 { -v } else { v })
}

/// AC: signed, floored (P16).
pub(crate) fn dequant_ac(level: i32, mult: i32, q: i32) -> Result<i64, Error> {
    Ok((shifted(level as i64 * mult as i64, q >> 2)? + 0x40) >> 7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_only_block_is_flat() {
        let mut c = [0i64; 64];
        c[0] = 1024 * 8 + 32;
        idct_block(&mut c);
        assert!(c.iter().all(|&v| v >> 6 == 128), "{:?}", &c[..8]);
        let mut px = [0i32; 64];
        block_pixels(-77, &[], &mut px);
        assert_eq!(px, [-77 >> 6; 64]);
    }

    #[test]
    fn narrow_and_wide_idct_agree() {
        // pseudo-random blocks just under the narrow bound: i32 and i64 give the same outputs
        let mut x = 0x2545f4914f6cdd1du64;
        for _ in 0..2000 {
            let mut c: [i64; 64] = std::array::from_fn(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x % (2 * NARROW - 1)) as i64 - (NARROW as i64 - 1)
            });
            let mut n = c.map(|v| v as i32);
            idct_block(&mut n);
            idct_block(&mut c);
            assert_eq!(n.map(|v| v as i64), c);
        }
    }

    #[test]
    fn dequant_rounds_dc_by_magnitude_and_floors_ac() {
        assert_eq!(dequant_dc(-1, 8).unwrap(), -4);
        assert_eq!(dequant_dc(1, 8).unwrap(), 4);
        assert_eq!(dequant_dc(6, 27).unwrap(), 646, "P13");
        assert_eq!(dequant_dc(59, 14).unwrap(), 667, "P28");
        assert_eq!(dequant_dc(60, 19).unwrap(), 1615, "P28");
        assert_eq!(dequant_ac(-1, 1313, 0).unwrap(), (-1313 + 0x40) >> 7);
        assert_eq!(dequant_ac(1, 1313, -1), Err(Error::NegativeQuantiser));
    }
}
