//! A scalar height field over a tiling texture, with wrap addressing.

/// Heights in row-major order, top row first — the same layout as the
/// texture the field was derived from, so texel `(x, y)` of the source maps
/// to `data[y * width + x]`.
#[derive(Debug, Clone, PartialEq)]
pub struct HeightField {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

/// The spread a field had before [`HeightField::normalize`] rescaled it,
/// in the field's own units (texels, for a normal-integrated field).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizeRange {
    pub low: f32,
    pub high: f32,
}

/// Fraction of texels clipped at each end by [`HeightField::normalize`].
///
/// A tunable, not a measured quantity: one stray texel (a seam artefact in
/// the normal map, a specular glint in a diffuse) would otherwise set the
/// 0 or 1 end of the whole range and squash every real feature into the
/// middle. 0.5 % per side keeps the clipped tail to a few hundred texels of
/// a 256² field and a few thousand of a 1024² one.
pub const NORMALIZE_CLIP_FRACTION: f32 = 0.005;

impl HeightField {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            data: vec![0.0; width * height],
        }
    }

    /// Texel `(x, y)` with wrap addressing, so derivatives and filters read
    /// across the tiling seam exactly as the GPU sampler will.
    #[inline]
    pub fn at(&self, x: isize, y: isize) -> f32 {
        let x = x.rem_euclid(self.width as isize) as usize;
        let y = y.rem_euclid(self.height as isize) as usize;
        self.data[y * self.width + x]
    }

    /// Rescale to `[0, 1]` between the [`NORMALIZE_CLIP_FRACTION`]
    /// percentiles, clamping the clipped tails. Returns the range that was
    /// mapped onto `[0, 1]`. A flat field (no spread) becomes 0.5 everywhere:
    /// "no relief" sits at mid-height, so it neither wins nor loses a
    /// height-based blend against a layer that has relief.
    pub fn normalize(&mut self) -> NormalizeRange {
        let mut sorted = self.data.clone();
        sorted.sort_unstable_by(f32::total_cmp);
        let n = sorted.len();
        let pick = |fraction: f32| -> f32 {
            let i = ((n - 1) as f32 * fraction).round() as usize;
            sorted[i.min(n - 1)]
        };
        let low = pick(NORMALIZE_CLIP_FRACTION);
        let high = pick(1.0 - NORMALIZE_CLIP_FRACTION);
        let span = high - low;
        // `span` is NaN-free here (sorted finite samples), so `<=` is the
        // whole "no relief" test.
        if span <= f32::EPSILON * low.abs().max(high.abs()).max(1.0) {
            self.data.fill(0.5);
        } else {
            for v in &mut self.data {
                *v = ((*v - low) / span).clamp(0.0, 1.0);
            }
        }
        NormalizeRange { low, high }
    }

    /// Halve both dimensions by 2×2 box averaging. Odd dimensions keep their
    /// last row/column by averaging it with the wrapped first one, which is
    /// what keeps a box-filtered tiling texture tiling.
    pub fn half(&self) -> Self {
        let w = (self.width / 2).max(1);
        let h = (self.height / 2).max(1);
        let mut out = Self::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = (2 * x as isize, 2 * y as isize);
                let sx1 = if self.width > 1 { sx + 1 } else { sx };
                let sy1 = if self.height > 1 { sy + 1 } else { sy };
                out.data[y * w + x] = 0.25
                    * (self.at(sx, sy) + self.at(sx1, sy) + self.at(sx, sy1) + self.at(sx1, sy1));
            }
        }
        out
    }

    /// This field followed by every box-filtered level down to 1×1.
    pub fn mip_chain(self) -> Vec<Self> {
        let mut chain = vec![self];
        loop {
            let last = chain.last().unwrap();
            if last.width == 1 && last.height == 1 {
                return chain;
            }
            let next = last.half();
            chain.push(next);
        }
    }

    /// Largest absolute difference between the left and right columns and
    /// between the top and bottom rows, measured as the step across the wrap
    /// seam against the median interior step. A tileable field crosses its
    /// seam like any other texel boundary; this is the quantity the tiling
    /// tests bound.
    pub fn seam_step_ratio(&self) -> f32 {
        let mut interior = Vec::new();
        let mut seam: f32 = 0.0;
        for y in 0..self.height as isize {
            for x in 0..self.width as isize {
                let dx = (self.at(x + 1, y) - self.at(x, y)).abs();
                let dy = (self.at(x, y + 1) - self.at(x, y)).abs();
                if x == self.width as isize - 1 {
                    seam = seam.max(dx);
                } else {
                    interior.push(dx);
                }
                if y == self.height as isize - 1 {
                    seam = seam.max(dy);
                } else {
                    interior.push(dy);
                }
            }
        }
        interior.sort_unstable_by(f32::total_cmp);
        let max_interior = interior.last().copied().unwrap_or(0.0);
        if max_interior <= 0.0 {
            return if seam <= 0.0 { 0.0 } else { f32::INFINITY };
        }
        seam / max_interior
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_spans_unit_range_and_clips_outliers() {
        let mut f = HeightField::new(100, 10);
        for (i, v) in f.data.iter_mut().enumerate() {
            *v = i as f32;
        }
        // One wild outlier must not set the top of the range.
        f.data[500] = 1.0e6;
        let range = f.normalize();
        assert!(range.high < 1.0e6);
        let min = f.data.iter().copied().fold(f32::INFINITY, f32::min);
        let max = f.data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert_eq!((min, max), (0.0, 1.0));
        assert!(f.data.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn flat_field_normalizes_to_mid_height() {
        let mut f = HeightField::new(8, 8);
        f.data.fill(3.0);
        f.normalize();
        assert!(f.data.iter().all(|&v| v == 0.5));
    }

    #[test]
    fn mip_chain_ends_at_one_texel_and_preserves_mean() {
        let mut f = HeightField::new(16, 8);
        for (i, v) in f.data.iter_mut().enumerate() {
            *v = (i % 7) as f32;
        }
        let mean = f.data.iter().sum::<f32>() / f.data.len() as f32;
        let chain = f.mip_chain();
        let dims: Vec<_> = chain.iter().map(|m| (m.width, m.height)).collect();
        assert_eq!(dims, [(16, 8), (8, 4), (4, 2), (2, 1), (1, 1)]);
        assert!((chain.last().unwrap().data[0] - mean).abs() < 1e-4);
    }
}
