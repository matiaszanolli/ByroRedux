//! AVX2 versions of the hot kernels, chosen at run time. The scalar code next to each caller stays the
//! reference: every kernel here computes exactly what its scalar twin does (the tests below compare
//! them on random input), and `set_simd(false)` turns them all off.

use std::sync::atomic::{AtomicBool, Ordering};

static ALLOW: AtomicBool = AtomicBool::new(true);

/// Turns the AVX2 kernels off (`false`) or back on where the CPU has them (`true`, the default): the
/// scalar path is the reference.
pub fn set_simd(on: bool) {
    ALLOW.store(on, Ordering::Relaxed);
}

/// Whether the AVX2 kernels run.
#[inline]
pub(crate) fn avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        ALLOW.load(Ordering::Relaxed) && std::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

// Each wrapper runs its kernel and returns true when AVX2 runs; false tells the caller to run the
// scalar twin.
macro_rules! dispatch {
    ($(#[$doc:meta] $name:ident($($arg:ident: $ty:ty),*);)*) => {$(
        #[$doc]
        #[inline]
        #[allow(unused_variables)]
        pub(crate) fn $name($($arg: $ty),*) -> bool {
            #[cfg(target_arch = "x86_64")]
            if avx2() {
                // SAFETY: the CPU has AVX2 (`avx2`)
                unsafe { x86::$name($($arg),*) };
                return true;
            }
            false
        }
    )*};
}

dispatch! {
    /// `inter::luma6`.
    luma6(src: &[u8], sstride: usize, dst: &mut [u8], dstride: usize, fx: i32, fy: i32);
    /// `inter::chroma_bilinear`.
    chroma_bilinear(src: &[u8], sstride: usize, dst: &mut [u8], dstride: usize, fx: i32, fy: i32);
    /// A block's i32 pixels clamped to 0..255.
    clamp_u8(px: &[i32; 64], out: &mut [u8; 64]);
    /// `idct::idct_block` and the final `>> 6`, i32.
    idct_px(c: &mut [i32; 64]);
    /// `px[i] = clamp(px[i] + g[i], 0, 255)`, lengths a multiple of 16.
    add_clamped(px: &mut [u8], g: &[i32]);
    /// `inter::avg_dc_row`.
    avg_dc_row(rows: [&[u8]; 8], n: usize, out: &mut [i32]);
    /// `filters::edge_u8` on a horizontal edge (8 columns from `o`, rows `across` apart).
    hedge_u8(px: &mut [u8], o: usize, across: usize, na: u32, nb: u32);
    /// `filters::smooth_block`.
    smooth_block(g: &[[i32; 3]; 3], out: &mut [[u8; 8]; 8]);
    /// `rgb::rgba_row` over pixels `0..w`.
    rgba_row(r: &crate::rgb::RgbaRow, out: &mut [u8], w: usize);
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use std::arch::x86_64::*;

    /// 16 bytes from `p` widened to 16 lanes of i16.
    #[inline]
    #[target_feature(enable = "avx2")]
    fn widen16(p: &[u8]) -> __m256i {
        let b: &[u8; 16] = p[..16].try_into().unwrap();
        // SAFETY: 16 readable bytes
        _mm256_cvtepu8_epi16(unsafe { _mm_loadu_si128(b.as_ptr() as *const __m128i) })
    }

    /// 16 lanes of i16 clamped to 0..255 into `out`.
    #[inline]
    #[target_feature(enable = "avx2")]
    fn store_u8x16(v: __m256i, out: &mut [u8; 16]) {
        let b = _mm_packus_epi16(_mm256_castsi256_si128(v), _mm256_extracti128_si256::<1>(v));
        // SAFETY: 16 writable bytes
        unsafe { _mm_storeu_si128(out.as_mut_ptr() as *mut __m128i, b) };
    }

    /// `inter::f6r` on 16 lanes (every intermediate of the luma filter fits in i16: P20 inputs are
    /// 0..255, the second pass's -64..319).
    #[inline]
    #[target_feature(enable = "avx2")]
    fn f6r(p: [__m256i; 6]) -> __m256i {
        let s23 = _mm256_add_epi16(p[2], p[3]);
        let s14 = _mm256_add_epi16(p[1], p[4]);
        let s05 = _mm256_add_epi16(p[0], p[5]);
        let v = _mm256_sub_epi16(_mm256_mullo_epi16(s23, _mm256_set1_epi16(19)), _mm256_slli_epi16::<2>(s14));
        let v = _mm256_add_epi16(_mm256_add_epi16(v, s05), _mm256_set1_epi16(16));
        _mm256_srai_epi16::<5>(_mm256_sub_epi16(v, _mm256_and_si256(s23, _mm256_set1_epi16(1))))
    }

    #[target_feature(enable = "avx2")]
    pub(super) fn luma6(src: &[u8], ss: usize, dst: &mut [u8], ds: usize, fx: i32, fy: i32) {
        let row = |y: usize| &src[y * ss..y * ss + 21];
        let out = |dst: &mut [u8], y: usize, v: __m256i| {
            store_u8x16(v, (&mut dst[y * ds..y * ds + 16]).try_into().unwrap())
        };
        let taps = |r: &[u8]| {
            [
                widen16(&r[0..]),
                widen16(&r[1..]),
                widen16(&r[2..]),
                widen16(&r[3..]),
                widen16(&r[4..]),
                widen16(&r[5..]),
            ]
        };
        match (fx, fy) {
            (0, 0) => {
                for y in 0..16 {
                    dst[y * ds..y * ds + 16].copy_from_slice(&row(y + 2)[2..18]);
                }
            }
            (_, 0) => {
                for y in 0..16 {
                    out(dst, y, f6r(taps(row(y + 2))));
                }
            }
            (0, _) => {
                let rows: [__m256i; 21] = std::array::from_fn(|r| widen16(&row(r)[2..]));
                for y in 0..16 {
                    out(dst, y, f6r(rows[y..y + 6].try_into().unwrap()));
                }
            }
            _ => {
                let hp: [__m256i; 21] = std::array::from_fn(|r| f6r(taps(row(r))));
                for y in 0..16 {
                    out(dst, y, f6r(hp[y..y + 6].try_into().unwrap()));
                }
            }
        }
    }

    /// 8 bytes from `p` widened to 8 lanes of i16.
    #[inline]
    #[target_feature(enable = "avx2")]
    fn widen8(p: &[u8]) -> __m128i {
        let b: &[u8; 8] = p[..8].try_into().unwrap();
        // SAFETY: 8 readable bytes
        _mm_cvtepu8_epi16(unsafe { _mm_loadl_epi64(b.as_ptr() as *const __m128i) })
    }

    /// 8 bytes from `p` in the low half.
    #[inline]
    #[target_feature(enable = "avx2")]
    fn load8(p: &[u8]) -> __m128i {
        let b: &[u8; 8] = p[..8].try_into().unwrap();
        // SAFETY: 8 readable bytes
        unsafe { _mm_loadl_epi64(b.as_ptr() as *const __m128i) }
    }

    /// Each row's horizontal pixel pairs (x, x + 1) interleaved, so that one `maddubs` gives a pair's
    /// weighted sum (at most 255 x 16: no saturation); two output rows per 256-bit register.
    #[target_feature(enable = "avx2")]
    pub(super) fn chroma_bilinear(src: &[u8], ss: usize, dst: &mut [u8], ds: usize, fx: i32, fy: i32) {
        let pairs: [__m128i; 9] = std::array::from_fn(|y| {
            let r = &src[y * ss..y * ss + 9];
            _mm_unpacklo_epi8(load8(r), load8(&r[1..]))
        });
        // per 16-bit lane: the low byte weighs pixel x, the high byte pixel x + 1
        let weights = |a: i32, b: i32| _mm256_set1_epi16(((b << 8) | a) as i16);
        let top = weights((4 - fx) * (4 - fy), fx * (4 - fy));
        let bottom = weights((4 - fx) * fy, fx * fy);
        for y in (0..8).step_by(2) {
            let t = _mm256_set_m128i(pairs[y + 1], pairs[y]);
            let b = _mm256_set_m128i(pairs[y + 2], pairs[y + 1]);
            let v = _mm256_add_epi16(_mm256_maddubs_epi16(t, top), _mm256_maddubs_epi16(b, bottom));
            let v = _mm256_srai_epi16::<4>(_mm256_add_epi16(v, _mm256_set1_epi16(8)));
            let px = _mm_packus_epi16(_mm256_castsi256_si128(v), _mm256_extracti128_si256::<1>(v));
            for (k, half) in [px, _mm_unpackhi_epi64(px, px)].into_iter().enumerate() {
                let o: &mut [u8; 8] = (&mut dst[(y + k) * ds..(y + k) * ds + 8]).try_into().unwrap();
                // SAFETY: 8 writable bytes
                unsafe { _mm_storel_epi64(o.as_mut_ptr() as *mut __m128i, half) };
            }
        }
    }

    #[target_feature(enable = "avx2")]
    pub(super) fn clamp_u8(px: &[i32; 64], out: &mut [u8; 64]) {
        for (k, o) in out.chunks_exact_mut(16).enumerate() {
            // SAFETY: 16 i32 read inside `px`
            let (lo, hi) = unsafe {
                let p = px.as_ptr().add(16 * k);
                (_mm256_loadu_si256(p as *const __m256i), _mm256_loadu_si256(p.add(8) as *const __m256i))
            };
            store_u8x16(_mm256_permute4x64_epi64::<0xd8>(_mm256_packs_epi32(lo, hi)), o.try_into().unwrap());
        }
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    fn transpose8(r: [__m256i; 8]) -> [__m256i; 8] {
        let t0 = _mm256_unpacklo_epi32(r[0], r[1]);
        let t1 = _mm256_unpackhi_epi32(r[0], r[1]);
        let t2 = _mm256_unpacklo_epi32(r[2], r[3]);
        let t3 = _mm256_unpackhi_epi32(r[2], r[3]);
        let t4 = _mm256_unpacklo_epi32(r[4], r[5]);
        let t5 = _mm256_unpackhi_epi32(r[4], r[5]);
        let t6 = _mm256_unpacklo_epi32(r[6], r[7]);
        let t7 = _mm256_unpackhi_epi32(r[6], r[7]);
        let u0 = _mm256_unpacklo_epi64(t0, t2);
        let u1 = _mm256_unpackhi_epi64(t0, t2);
        let u2 = _mm256_unpacklo_epi64(t1, t3);
        let u3 = _mm256_unpackhi_epi64(t1, t3);
        let u4 = _mm256_unpacklo_epi64(t4, t6);
        let u5 = _mm256_unpackhi_epi64(t4, t6);
        let u6 = _mm256_unpacklo_epi64(t5, t7);
        let u7 = _mm256_unpackhi_epi64(t5, t7);
        [
            _mm256_permute2x128_si256::<0x20>(u0, u4),
            _mm256_permute2x128_si256::<0x20>(u1, u5),
            _mm256_permute2x128_si256::<0x20>(u2, u6),
            _mm256_permute2x128_si256::<0x20>(u3, u7),
            _mm256_permute2x128_si256::<0x31>(u0, u4),
            _mm256_permute2x128_si256::<0x31>(u1, u5),
            _mm256_permute2x128_si256::<0x31>(u2, u6),
            _mm256_permute2x128_si256::<0x31>(u3, u7),
        ]
    }

    /// `idct::idct8` on 8 lanes of i32.
    #[inline]
    #[target_feature(enable = "avx2")]
    fn idct8(s: [__m256i; 8]) -> [__m256i; 8] {
        let add = |a, b| _mm256_add_epi32(a, b);
        let sub = |a, b| _mm256_sub_epi32(a, b);
        let a = |v| _mm256_add_epi32(v, _mm256_srai_epi32::<2>(v));
        let b = |v| _mm256_srai_epi32::<1>(v);
        let c =
            |v| _mm256_sub_epi32(_mm256_sub_epi32(v, _mm256_srai_epi32::<2>(v)), _mm256_srai_epi32::<4>(v));
        let d =
            |v| _mm256_sub_epi32(_mm256_add_epi32(v, _mm256_srai_epi32::<2>(v)), _mm256_srai_epi32::<4>(v));
        let e = |v| _mm256_srai_epi32::<2>(v);
        let t00 = add(s[3], s[5]);
        let t01 = sub(s[3], s[5]);
        let t02 = add(a(s[2]), b(s[6]));
        let t03 = sub(b(s[2]), a(s[6]));
        let t0 = add(add(s[0], s[4]), t02);
        let t1 = sub(add(s[0], s[4]), t02);
        let t2 = sub(s[0], s[4]);
        let t3 = add(s[1], t00);
        let t4 = sub(s[1], t00);
        let t5 = add(t01, s[7]);
        let t6 = sub(t01, s[7]);
        let t7 = add(t4, c(t6));
        let t8 = sub(c(t4), t6);
        let t9 = add(d(t3), e(t5));
        let t10 = sub(e(t3), d(t5));
        let t11 = add(t2, t03);
        let t12 = sub(t2, t03);
        [
            add(t0, t9),
            add(t11, t7),
            add(t12, t8),
            add(t1, t10),
            sub(t1, t10),
            sub(t12, t8),
            sub(t11, t7),
            sub(t0, t9),
        ]
    }

    #[target_feature(enable = "avx2")]
    pub(super) fn idct_px(c: &mut [i32; 64]) {
        // SAFETY: 8 rows of 8 i32 read and written inside `c`
        let rows: [__m256i; 8] =
            std::array::from_fn(|r| unsafe { _mm256_loadu_si256(c.as_ptr().add(8 * r) as *const __m256i) });
        // rows as lanes: each lane runs one row's transform, then one column's
        let pass1 = idct8(transpose8(rows));
        let out = idct8(transpose8(pass1));
        for (r, v) in out.into_iter().enumerate() {
            unsafe {
                _mm256_storeu_si256(c.as_mut_ptr().add(8 * r) as *mut __m256i, _mm256_srai_epi32::<6>(v))
            };
        }
    }

    /// 16 at a time. Saturating to i16 first changes no result: beyond it the sum is out of 0..255
    /// either way.
    #[target_feature(enable = "avx2")]
    pub(super) fn add_clamped(px: &mut [u8], g: &[i32]) {
        for (p, g) in px.chunks_exact_mut(16).zip(g.chunks_exact(16)) {
            // SAFETY: 16 i32 read from `g`
            let (lo, hi) = unsafe {
                (
                    _mm256_loadu_si256(g.as_ptr() as *const __m256i),
                    _mm256_loadu_si256(g.as_ptr().add(8) as *const __m256i),
                )
            };
            let r = _mm256_permute4x64_epi64::<0xd8>(_mm256_packs_epi32(lo, hi));
            let v = _mm256_adds_epi16(r, widen16(p));
            store_u8x16(v, p.try_into().unwrap());
        }
    }

    /// The 4 rows p1 p0 q0 q1 of 8 columns as i16; the moved ones stored back clamped.
    #[target_feature(enable = "avx2")]
    pub(super) fn hedge_u8(px: &mut [u8], o: usize, a: usize, na: u32, nb: u32) {
        let at = [o - 2 * a, o - a, o, o + a];
        let [p1, p0, q0, q1] = at.map(|i| widen8(&px[i..i + 8]));
        let step = _mm_sub_epi16(q0, p0);
        let d0 = _mm_srai_epi16::<2>(_mm_add_epi16(step, _mm_set1_epi16(2)));
        let d1 = _mm_srai_epi16::<3>(_mm_add_epi16(step, _mm_set1_epi16(4)));
        let mut put = |k: usize, v: __m128i| {
            let o: &mut [u8; 8] = (&mut px[at[k]..at[k] + 8]).try_into().unwrap();
            // SAFETY: 8 writable bytes
            unsafe { _mm_storel_epi64(o.as_mut_ptr() as *mut __m128i, _mm_packus_epi16(v, v)) };
        };
        if na < 8 {
            put(1, _mm_add_epi16(p0, d0));
        }
        if na <= 3 {
            put(0, _mm_add_epi16(p1, d1));
        }
        if nb < 8 {
            put(2, _mm_sub_epi16(q0, d0));
        }
        if nb <= 3 {
            put(3, _mm_sub_epi16(q1, d1));
        }
    }

    /// The horizontal pass with the weights as vectors over x, the vertical one with them per row; i32
    /// lanes (DC differences times weights exceed i16).
    #[target_feature(enable = "avx2")]
    pub(super) fn smooth_block(g: &[[i32; 3]; 3], out: &mut [[u8; 8]; 8]) {
        use crate::filters::W_B;
        let blend = |c: __m256i, lo: __m256i, hi: __m256i, wl: __m256i, wh: __m256i| {
            let l = _mm256_srai_epi32::<7>(_mm256_mullo_epi32(wl, _mm256_sub_epi32(lo, c)));
            let h = _mm256_srai_epi32::<7>(_mm256_mullo_epi32(wh, _mm256_sub_epi32(hi, c)));
            _mm256_add_epi32(c, _mm256_add_epi32(l, h))
        };
        let wl = _mm256_setr_epi32(W_B[0], W_B[1], W_B[2], W_B[3], W_B[4], W_B[5], W_B[6], W_B[7]);
        let wh = _mm256_setr_epi32(W_B[7], W_B[6], W_B[5], W_B[4], W_B[3], W_B[2], W_B[1], W_B[0]);
        let r = g.map(|row| {
            blend(_mm256_set1_epi32(row[1]), _mm256_set1_epi32(row[0]), _mm256_set1_epi32(row[2]), wl, wh)
        });
        let row_px = |y: usize| {
            let v = blend(r[1], r[0], r[2], _mm256_set1_epi32(W_B[y]), _mm256_set1_epi32(W_B[7 - y]));
            _mm256_srai_epi32::<3>(_mm256_add_epi32(v, _mm256_set1_epi32(4)))
        };
        for y in (0..8).step_by(2) {
            let v = _mm256_permute4x64_epi64::<0xd8>(_mm256_packs_epi32(row_px(y), row_px(y + 1)));
            let two: &mut [u8; 16] = out[y..y + 2].as_flattened_mut().try_into().unwrap();
            store_u8x16(v, two);
        }
    }

    /// 16 pixels (8 chroma samples) at a time where every neighbour sample they blend exists; the
    /// edges by the scalar code. The same 32-bit fixed point as `rgb::rgba_row`.
    #[target_feature(enable = "avx2")]
    pub(super) fn rgba_row(r: &crate::rgb::RgbaRow, out: &mut [u8], w: usize) {
        use crate::rgb::{K_B, K_GB, K_GR, K_R};
        let cw = r.cr[0].len();
        let ld = |p: &[u8], at: usize| _mm256_cvtepu8_epi32(load8(&p[at..]));
        // 8 chroma samples from `at`, blended vertically (x4)
        let v = |p: &[&[u8]; 2], at: usize| {
            let a = ld(p[0], at);
            _mm256_add_epi32(_mm256_add_epi32(a, _mm256_add_epi32(a, a)), ld(p[1], at))
        };
        // the 16 output values from sample `i0` on, blended horizontally (x16) less 16 x 128, in pixel
        // order: 8 and 8
        let up = |p: &[&[u8]; 2], i0: usize| {
            let c = v(p, i0);
            let c3 = _mm256_sub_epi32(_mm256_add_epi32(c, _mm256_add_epi32(c, c)), _mm256_set1_epi32(16 * 128));
            let (even, odd) = (_mm256_add_epi32(c3, v(p, i0 - 1)), _mm256_add_epi32(c3, v(p, i0 + 1)));
            let (lo, hi) = (_mm256_unpacklo_epi32(even, odd), _mm256_unpackhi_epi32(even, odd));
            [_mm256_permute2x128_si256::<0x20>(lo, hi), _mm256_permute2x128_si256::<0x31>(lo, hi)]
        };
        let k = |c: i32| _mm256_set1_epi32(c);
        let (lo, hi) = (_mm256_setzero_si256(), k(255));
        let clamp = |v: __m256i| _mm256_min_epi32(_mm256_max_epi32(_mm256_srai_epi32::<16>(v), lo), hi);
        crate::rgb::rgba_row(r, out, 0..2.min(w));
        let mut i0 = 1;
        while 2 * i0 + 16 <= w && i0 + 9 <= cw {
            let (cr, cb) = (up(&r.cr, i0), up(&r.cb, i0));
            for half in 0..2 {
                let x = 2 * i0 + 8 * half;
                let y = _mm256_add_epi32(_mm256_slli_epi32::<16>(ld(r.y, x)), k(1 << 15));
                let red = clamp(_mm256_add_epi32(y, _mm256_mullo_epi32(k(K_R), cr[half])));
                let green = clamp(_mm256_sub_epi32(
                    _mm256_sub_epi32(y, _mm256_mullo_epi32(k(K_GB), cb[half])),
                    _mm256_mullo_epi32(k(K_GR), cr[half]),
                ));
                let blue = clamp(_mm256_add_epi32(y, _mm256_mullo_epi32(k(K_B), cb[half])));
                let px = _mm256_or_si256(
                    _mm256_or_si256(red, _mm256_slli_epi32::<8>(green)),
                    _mm256_or_si256(_mm256_slli_epi32::<16>(blue), k(0xff00_0000u32 as i32)),
                );
                let o: &mut [u8; 32] = (&mut out[4 * x..4 * x + 32]).try_into().unwrap();
                // SAFETY: 32 writable bytes
                unsafe { _mm256_storeu_si256(o.as_mut_ptr() as *mut __m256i, px) };
            }
            i0 += 8;
        }
        crate::rgb::rgba_row(r, out, (2 * i0).min(w)..w);
    }

    /// `n` = 2 or 4 blocks (16 or 32 bytes per row): the column cascade is `pavgb` (round half up),
    /// the sum of a block's 8 columns `psadbw`.
    #[target_feature(enable = "avx2")]
    pub(super) fn avg_dc_row(rows: [&[u8]; 8], n: usize, out: &mut [i32]) {
        if n == 4 {
            // SAFETY: 32 readable bytes per row
            let r: [__m256i; 8] = std::array::from_fn(|k| unsafe {
                _mm256_loadu_si256(rows[k][..32].as_ptr() as *const __m256i)
            });
            let a = [
                _mm256_avg_epu8(r[0], r[1]),
                _mm256_avg_epu8(r[2], r[3]),
                _mm256_avg_epu8(r[4], r[5]),
                _mm256_avg_epu8(r[6], r[7]),
            ];
            let b = [_mm256_avg_epu8(a[0], a[1]), _mm256_avg_epu8(a[2], a[3])];
            let s = _mm256_sad_epu8(_mm256_avg_epu8(b[0], b[1]), _mm256_setzero_si256());
            let mut v = [0i64; 4];
            // SAFETY: 4 i64 written
            unsafe { _mm256_storeu_si256(v.as_mut_ptr() as *mut __m256i, s) };
            for (o, v) in out.iter_mut().zip(v) {
                *o = v as i32;
            }
        } else {
            // SAFETY: 16 readable bytes per row
            let r: [__m128i; 8] =
                std::array::from_fn(|k| unsafe { _mm_loadu_si128(rows[k][..16].as_ptr() as *const __m128i) });
            let a = [
                _mm_avg_epu8(r[0], r[1]),
                _mm_avg_epu8(r[2], r[3]),
                _mm_avg_epu8(r[4], r[5]),
                _mm_avg_epu8(r[6], r[7]),
            ];
            let b = [_mm_avg_epu8(a[0], a[1]), _mm_avg_epu8(a[2], a[3])];
            let s = _mm_sad_epu8(_mm_avg_epu8(b[0], b[1]), _mm_setzero_si128());
            out[0] = _mm_cvtsi128_si32(s);
            out[1] = _mm_extract_epi32::<2>(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idct::{idct_block, NARROW};
    use crate::inter;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        /// Bytes biased towards the extremes, where clamps and rounding live.
        fn byte(&mut self) -> u8 {
            match self.next() % 8 {
                0 => 0,
                1 => 255,
                _ => self.next() as u8,
            }
        }
    }

    #[test]
    fn luma6_matches_scalar() {
        if !avx2() {
            return;
        }
        let mut rng = Rng(0x9e3779b97f4a7c15);
        for _ in 0..3000 {
            let win: [[u8; 23]; 21] = std::array::from_fn(|_| std::array::from_fn(|_| rng.byte()));
            for (fx, fy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (mut a, mut b) = ([[0u8; 17]; 16], [[0u8; 17]; 16]);
                inter::luma6(win.as_flattened(), 23, a.as_flattened_mut(), 17, fx, fy);
                assert!(luma6(win.as_flattened(), 23, b.as_flattened_mut(), 17, fx, fy));
                assert_eq!(a, b, "fx {fx} fy {fy}");
            }
        }
    }

    #[test]
    fn chroma_matches_scalar() {
        if !avx2() {
            return;
        }
        let mut rng = Rng(0x1234_5678_9abc_def1);
        for _ in 0..3000 {
            let win: [[u8; 11]; 9] = std::array::from_fn(|_| std::array::from_fn(|_| rng.byte()));
            for fx in 0..4 {
                for fy in 0..4 {
                    let (mut a, mut b) = ([[0u8; 9]; 8], [[0u8; 9]; 8]);
                    inter::chroma_bilinear(win.as_flattened(), 11, a.as_flattened_mut(), 9, fx, fy);
                    assert!(chroma_bilinear(win.as_flattened(), 11, b.as_flattened_mut(), 9, fx, fy));
                    assert_eq!(a, b, "fx {fx} fy {fy}");
                }
            }
        }
    }

    #[test]
    fn idct_matches_scalar() {
        if !avx2() {
            return;
        }
        let mut rng = Rng(0xdead_beef_cafe_f00d);
        for i in 0..20000 {
            // dense blocks over the whole narrow range, and sparse ones of realistic size
            let (span, density) = if i % 2 == 0 { (NARROW, 64) } else { (4096, 6) };
            let c: [i32; 64] = std::array::from_fn(|_| {
                if rng.next() % 64 < density {
                    (rng.next() % (2 * span - 1)) as i32 - (span as i32 - 1)
                } else {
                    0
                }
            });
            let mut a = c;
            idct_block(&mut a);
            let a = a.map(|v| v >> 6);
            let mut b = c;
            assert!(idct_px(&mut b));
            assert_eq!(a, b);
            let (mut u, want) = ([0u8; 64], a.map(|v| v.clamp(0, 255) as u8));
            assert!(clamp_u8(&a, &mut u));
            assert_eq!(u, want);
        }
    }

    #[test]
    fn add_clamped_matches_scalar() {
        if !avx2() {
            return;
        }
        let mut rng = Rng(0x0f0f_1234_5555_aaaa);
        for _ in 0..5000 {
            let g: Vec<i32> = (0..32)
                .map(|_| match rng.next() % 4 {
                    0 => (rng.next() % (1 << 27)) as i32 - (1 << 26),
                    _ => (rng.next() % 1024) as i32 - 512,
                })
                .collect();
            let px: Vec<u8> = (0..32).map(|_| rng.byte()).collect();
            let want: Vec<u8> =
                px.iter().zip(&g).map(|(&p, &v)| (p as i32 + v).clamp(0, 255) as u8).collect();
            let mut got = px.clone();
            assert!(add_clamped(&mut got, &g));
            assert_eq!(want, got);
        }
    }

    #[test]
    fn hedge_matches_scalar() {
        if !avx2() {
            return;
        }
        let mut rng = Rng(0x7777_1111_3333_9999);
        for _ in 0..20000 {
            let px: Vec<u8> = (0..4 * 11).map(|_| rng.byte()).collect();
            let (na, nb) = ((rng.next() % 70) as u32, (rng.next() % 70) as u32);
            let mut want = px.clone();
            for c in 0..8 {
                let e = [2 + c, 13 + c, 24 + c, 35 + c];
                let step = want[e[2]] as i32 - want[e[1]] as i32;
                let (d0, d1) = ((step + 2) >> 2, (step + 4) >> 3);
                let mut put = |k: usize, d: i32| want[e[k]] = (want[e[k]] as i32 + d).clamp(0, 255) as u8;
                if na < 8 {
                    put(1, d0);
                }
                if na <= 3 {
                    put(0, d1);
                }
                if nb < 8 {
                    put(2, -d0);
                }
                if nb <= 3 {
                    put(3, -d1);
                }
            }
            let mut got = px.clone();
            assert!(hedge_u8(&mut got, 24, 11, na, nb));
            assert_eq!(want, got, "na {na} nb {nb}");
        }
    }

    #[test]
    fn smooth_block_matches_scalar() {
        if !avx2() {
            return;
        }
        let mut rng = Rng(0x2468_ace0_1357_9bdf);
        let blend = |c: i32, lo: i32, hi: i32, k: usize| {
            use crate::filters::W_B;
            c + ((W_B[k] * (lo - c)) >> 7) + ((W_B[7 - k] * (hi - c)) >> 7)
        };
        for _ in 0..20000 {
            let g: [[i32; 3]; 3] =
                std::array::from_fn(|_| std::array::from_fn(|_| (rng.next() % 2400) as i32 - 100));
            let rows: [[i32; 8]; 3] =
                std::array::from_fn(|j| std::array::from_fn(|x| blend(g[j][1], g[j][0], g[j][2], x)));
            let want: [[u8; 8]; 8] = std::array::from_fn(|y| {
                std::array::from_fn(|x| {
                    ((blend(rows[1][x], rows[0][x], rows[2][x], y) + 4) >> 3).clamp(0, 255) as u8
                })
            });
            let mut got = [[0u8; 8]; 8];
            assert!(smooth_block(&g, &mut got));
            assert_eq!(want, got, "{g:?}");
        }
    }

    #[test]
    fn rgba_row_matches_scalar() {
        if !avx2() {
            return;
        }
        let mut rng = Rng(0x1357_2468_aaaa_5555);
        for w in [1usize, 2, 3, 15, 16, 17, 18, 33, 34, 48, 62, 64, 66, 96, 130] {
            for _ in 0..300 {
                let cw = w.div_ceil(2) + (rng.next() % 3) as usize;
                let mut row = |n: usize| -> Vec<u8> { (0..n).map(|_| rng.byte()).collect() };
                let (y, c) = (row(w + 5), [row(cw), row(cw), row(cw), row(cw)]);
                let r = crate::rgb::RgbaRow { y: &y, cr: [&c[0], &c[1]], cb: [&c[2], &c[3]] };
                let (mut a, mut b) = (vec![0u8; 4 * w], vec![0u8; 4 * w]);
                crate::rgb::rgba_row(&r, &mut a, 0..w);
                assert!(rgba_row(&r, &mut b, w));
                assert_eq!(a, b, "w {w} cw {cw}");
            }
        }
    }

    #[test]
    fn avg_dc_matches_scalar() {
        if !avx2() {
            return;
        }
        let mut rng = Rng(0x5151_7272_9393_b4b4);
        for _ in 0..5000 {
            let data: Vec<Vec<u8>> = (0..8).map(|_| (0..32).map(|_| rng.byte()).collect()).collect();
            let rows: [&[u8]; 8] = std::array::from_fn(|k| &data[k][..]);
            for n in [2, 4] {
                let (mut a, mut b) = ([0i32; 4], [0i32; 4]);
                inter::avg_dc_row(rows, n, &mut a);
                assert!(avg_dc_row(rows, n, &mut b[..n]));
                assert_eq!(a[..n], b[..n], "n {n}");
            }
        }
    }
}
