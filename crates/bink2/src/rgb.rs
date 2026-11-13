//! P33: the colour converter — a frame's planes (Y, Cr, Cb) to RGBA for display.

use crate::frame::Frame;

/// The BT.601 full-range coefficients (P33) at 2^16 for a chroma value scaled by 16 (the sum of the
/// bilinear weights): round(c / 16 * 65536).
pub(crate) const K_R: i32 = 5743; // 1.402 (Cr)
pub(crate) const K_GR: i32 = 2925; // 0.714136 (Cr)
pub(crate) const K_GB: i32 = 1410; // 0.344136 (Cb)
pub(crate) const K_B: i32 = 7258; // 1.772 (Cb)

#[inline]
fn clamp_u8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// The neighbour a centred 2x upsampling blends at 1/4 for output index `o` (3/4 goes to `o / 2`),
/// the plane's edge sample repeated.
#[inline]
fn other(o: usize, n: usize) -> usize {
    if o.is_multiple_of(2) {
        (o / 2).saturating_sub(1)
    } else {
        (o / 2 + 1).min(n - 1)
    }
}

/// What one output row is made of: its luma row and, per chroma plane, the chroma row it lies
/// nearest (weight 3/4) and the one beyond (1/4).
pub(crate) struct RgbaRow<'a> {
    pub y: &'a [u8],
    pub cr: [&'a [u8]; 2],
    pub cb: [&'a [u8]; 2],
}

/// Pixels `xs` of a row into `out` (RGBA, from pixel 0): full-range BT.601 on bilinear, centred
/// chroma (P33), in 32-bit fixed point.
pub(crate) fn rgba_row(r: &RgbaRow, out: &mut [u8], xs: std::ops::Range<usize>) {
    let cw = r.cr[0].len();
    // a chroma sample blended vertically (x4)
    let v = |p: &[&[u8]; 2], i: usize| 3 * p[0][i] as i32 + p[1][i] as i32;
    for x in xs {
        let (i, n) = (x / 2, other(x, cw));
        // then horizontally (x16), less 16 x 128
        let cr = 3 * v(&r.cr, i) + v(&r.cr, n) - 16 * 128;
        let cb = 3 * v(&r.cb, i) + v(&r.cb, n) - 16 * 128;
        let y = ((r.y[x] as i32) << 16) + (1 << 15);
        out[4 * x..4 * x + 4].copy_from_slice(&[
            clamp_u8((y + K_R * cr) >> 16),
            clamp_u8((y - K_GB * cb - K_GR * cr) >> 16),
            clamp_u8((y + K_B * cb) >> 16),
            255,
        ]);
    }
}

impl Frame {
    /// P33: the picture (`width` x `height`) as RGBA, 4 bytes a pixel, alpha 255.
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = vec![0; self.width * self.height * 4];
        self.rgba_into(&mut out);
        out
    }

    /// `to_rgba` into `out` (at least `width * height * 4` bytes). Full-range BT.601, chroma upsampled
    /// bilinearly with its samples centred between the luma pixels (P33). The DLL's own converter
    /// rounds differently: within 2 of it on every pixel of the frame measured (P33, open).
    pub fn rgba_into(&self, out: &mut [u8]) {
        let (w, h) = (self.width, self.height);
        assert!(out.len() >= w * h * 4, "RGBA buffer too small");
        let [luma, cr, cb] = &self.planes;
        let ch = cr.height;
        fn crow(p: &crate::frame::Plane, j: usize) -> &[u8] {
            &p.px[j * p.width..(j + 1) * p.width]
        }
        for (row, line) in out[..w * h * 4].chunks_exact_mut(w * 4).enumerate() {
            let (j, k) = (row / 2, other(row, ch));
            let r = RgbaRow {
                y: &luma.px[row * luma.width..row * luma.width + w],
                cr: [crow(cr, j), crow(cr, k)],
                cb: [crow(cb, j), crow(cb, k)],
            };
            if !crate::simd::rgba_row(&r, line, w) {
                rgba_row(&r, line, 0..w);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::frame::Frame;

    fn frame(w: usize, h: usize, mut fill: impl FnMut(usize, usize, usize) -> u8) -> Frame {
        let mut f = Frame::new(w, h);
        for (p, plane) in f.planes.iter_mut().enumerate() {
            let pw = plane.width;
            for (i, v) in plane.px.iter_mut().enumerate() {
                *v = fill(p, i % pw, i / pw);
            }
        }
        f
    }

    #[test]
    fn neutral_chroma_is_grey_and_a_colour_follows_bt601() {
        let grey = frame(40, 24, |p, x, y| if p == 0 { (x * 7 + y * 3) as u8 } else { 128 });
        let rgba = grey.to_rgba();
        for (i, px) in rgba.chunks_exact(4).enumerate() {
            let y = grey.planes[0].px[(i / 40) * grey.planes[0].width + i % 40];
            assert_eq!(px, [y, y, y, 255]);
        }
        // Y 128, Cr = Cb = 192: R = 128 + 1.402 * 64, G = 128 - (0.344136 + 0.714136) * 64, B = 128 + 1.772 * 64
        let colour = frame(32, 32, |p, _, _| if p == 0 { 128 } else { 192 });
        assert_eq!(&colour.to_rgba()[..4], [218, 60, 241, 255]);
    }

    #[test]
    fn matches_the_float_model() {
        let mut s = 0x9e3779b97f4a7c15u64;
        let mut rnd = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let f = frame(50, 30, |_, _, _| rnd() as u8);
        let rgba = f.to_rgba();
        let [l, cr, cb] = &f.planes;
        let up = |p: &crate::frame::Plane, x: usize, y: usize| {
            let at = |o: usize, n: usize| {
                let c = (o as f64 + 0.5) / 2.0 - 0.5;
                let i0 = c.floor().clamp(0.0, (n - 1) as f64) as usize;
                let i1 = (c.floor() + 1.0).clamp(0.0, (n - 1) as f64) as usize;
                (i0, i1, c - c.floor())
            };
            let ((x0, x1, wx), (y0, y1, wy)) = (at(x, p.width), at(y, p.height));
            let v = |x: usize, y: usize| p.px[y * p.width + x] as f64;
            (v(x0, y0) * (1.0 - wx) + v(x1, y0) * wx) * (1.0 - wy) + (v(x0, y1) * (1.0 - wx) + v(x1, y1) * wx) * wy
        };
        for y in 0..30 {
            for x in 0..50 {
                let (yv, r, b) = (l.px[y * l.width + x] as f64, up(cr, x, y) - 128.0, up(cb, x, y) - 128.0);
                let want = [yv + 1.402 * r, yv - 0.344136 * b - 0.714136 * r, yv + 1.772 * b];
                for (k, v) in want.iter().enumerate() {
                    let got = rgba[(y * 50 + x) * 4 + k] as f64;
                    assert!((got - v.round().clamp(0.0, 255.0)).abs() <= 1.0, "pixel ({x}, {y}) channel {k}");
                }
            }
        }
    }
}
