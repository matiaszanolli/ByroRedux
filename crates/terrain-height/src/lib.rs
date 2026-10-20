//! Per-terrain-layer height maps (#6195).
//!
//! Turns one terrain layer's texture into a tileable, normalised `[0, 1]`
//! height map, stored as a BC4 `.dds` with a full mip chain. Three sources,
//! in the priority order the caller applies:
//!
//! 1. [`from_authored`] — an authored parallax / height / displacement map;
//! 2. [`from_normal_map`] — the layer's normal map integrated into a height
//!    field ([`integrate`]), which recovers real shape;
//! 3. [`from_brightness`] — the high-passed luminance of the diffuse
//!    ([`brightness`]), for a layer with neither.
//!
//! Pure CPU and engine-agnostic: no archive, game or GPU knowledge. Which
//! texture fills which role for a given game is decided by the caller, at the
//! translation layer, and the renderer only ever sees "a height map per
//! layer".

pub mod bc4;
pub mod brightness;
pub mod field;
pub mod integrate;

pub use field::{HeightField, NormalizeRange};
pub use integrate::{Gradients, GreenConvention};

/// Bumped whenever generation output changes for the same input, so a
/// cache keyed on it regenerates instead of serving stale maps.
pub const GENERATOR_VERSION: u32 = 1;

/// Largest edge, in texels, of a generated height map. A source larger than
/// this is box-reduced first. A budget, not a quality limit of the method:
/// a 1024² BC4 map with mips is 0.67 MiB, and a resident exterior carries a
/// few dozen distinct layers.
pub const MAX_EDGE: usize = 1024;

/// The high-pass applied to a normal-integrated height, as a fraction of the
/// map's width (see [`Gradients::integrate`]). A tunable, not a measured
/// value: features broader than about `2·sigma` are treated as bias and
/// removed. 1/16 of the width (64 texels of a 1024² map) keeps a stone that
/// spans a tenth of the tile.
pub const NORMALS_HIGHPASS_SIGMA_FRACTION: f32 = 1.0 / 16.0;

/// A borrowed RGBA8 image, top row first.
#[derive(Clone, Copy)]
pub struct Rgba<'a> {
    pub width: usize,
    pub height: usize,
    pub pixels: &'a [u8],
}

/// Which source a height map was generated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HeightSource {
    /// An authored height / parallax / displacement map, rescaled only.
    Authored,
    /// The layer's normal map, integrated.
    Normals,
    /// The layer's diffuse, high-pass filtered.
    Brightness,
}

/// What the debugger and the later phases need to know about a map.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HeightMeta {
    pub source: HeightSource,
    pub generator_version: u32,
    /// Dimensions of the stored top mip.
    pub width: u32,
    pub height: u32,
    /// The span mapped onto `[0, 1]`, in the source's own units: texels of
    /// the stored top mip for [`HeightSource::Normals`] (so a world-space
    /// amplitude follows from the layer's tiling: `relief × tile_size /
    /// width`), luminance for [`HeightSource::Brightness`], authored texel
    /// value / 255 for [`HeightSource::Authored`].
    pub relief: f32,
    /// [`HeightSource::Normals`] only: the green convention the map was read
    /// with (the game's, from the caller).
    pub green: Option<GreenConvention>,
    /// [`HeightSource::Normals`] only, diagnostic: the convention the
    /// integrability test picks from this map alone, and how decisively
    /// (`other_curl / chosen_curl`). A disagreement with `green` flags a map
    /// authored for the other convention.
    pub detected_green: Option<GreenConvention>,
    pub convention_decisiveness: Option<f32>,
}

/// A generated map: a complete BC4 `.dds` plus its metadata.
pub struct Generated {
    pub dds: Vec<u8>,
    pub meta: HeightMeta,
}

/// Box-reduce `img` until both edges fit [`MAX_EDGE`]. `None` when it
/// already fits.
fn reduce(img: &Rgba) -> Option<(usize, usize, Vec<u8>)> {
    if img.width <= MAX_EDGE && img.height <= MAX_EDGE {
        return None;
    }
    let (mut w, mut h, mut px) = (img.width, img.height, img.pixels.to_vec());
    while w > MAX_EDGE || h > MAX_EDGE {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut out = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let s = |sx: usize, sy: usize| px[((sy % h) * w + (sx % w)) * 4 + c] as u32;
                    let sum = s(2 * x, 2 * y)
                        + s(2 * x + 1, 2 * y)
                        + s(2 * x, 2 * y + 1)
                        + s(2 * x + 1, 2 * y + 1);
                    out[(y * nw + x) * 4 + c] = ((sum + 2) / 4) as u8;
                }
            }
        }
        (w, h, px) = (nw, nh, out);
    }
    Some((w, h, px))
}

/// Run `f` on `img`, reduced to [`MAX_EDGE`] first if needed.
fn with_budget<T>(img: &Rgba, f: impl FnOnce(&Rgba) -> T) -> T {
    match reduce(img) {
        Some((width, height, pixels)) => f(&Rgba {
            width,
            height,
            pixels: &pixels,
        }),
        None => f(img),
    }
}

fn finish(mut field: HeightField, mut meta: HeightMeta) -> Generated {
    let range = field.normalize();
    meta.relief = range.high - range.low;
    meta.width = field.width as u32;
    meta.height = field.height as u32;
    Generated {
        dds: bc4::write_dds(&field.mip_chain()),
        meta,
    }
}

fn meta(source: HeightSource) -> HeightMeta {
    HeightMeta {
        source,
        generator_version: GENERATOR_VERSION,
        width: 0,
        height: 0,
        relief: 0.0,
        green: None,
        detected_green: None,
        convention_decisiveness: None,
    }
}

/// An authored height map: its red channel (a grey or BC4 map decodes the
/// height there), rescaled to `[0, 1]`.
pub fn from_authored(img: &Rgba) -> Generated {
    with_budget(img, |img| {
        let mut field = HeightField::new(img.width, img.height);
        for (dst, px) in field.data.iter_mut().zip(img.pixels.chunks_exact(4)) {
            *dst = px[0] as f32 / 255.0;
        }
        finish(field, meta(HeightSource::Authored))
    })
}

/// A tangent-space normal map, integrated into a height field. `green` is
/// the convention the game reads its normal maps with; `None` falls back to
/// the integrability test's per-map choice.
pub fn from_normal_map(img: &Rgba, green: Option<GreenConvention>) -> Generated {
    with_budget(img, |img| {
        let gradients = Gradients::from_normal_map(img);
        let (detected, decisiveness) = gradients.detect_convention();
        let green = green.unwrap_or(detected);
        let mut meta = meta(HeightSource::Normals);
        meta.green = Some(green);
        meta.detected_green = Some(detected);
        meta.convention_decisiveness = Some(decisiveness);
        let sigma = img.width as f32 * NORMALS_HIGHPASS_SIGMA_FRACTION;
        finish(gradients.integrate(green, Some(sigma)), meta)
    })
}

/// The diffuse's high-passed luminance.
pub fn from_brightness(img: &Rgba) -> Generated {
    with_budget(img, |img| {
        finish(
            brightness::highpass_luminance(img),
            meta(HeightSource::Brightness),
        )
    })
}

/// Decode the top mip of a BC4 `.dds` written by [`bc4::write_dds`] back to
/// `[0, 1]` heights — the debugger's CPU-side view of a cached map.
pub fn decode_top_mip(dds: &[u8]) -> Option<HeightField> {
    if dds.len() < 148 || &dds[0..4] != b"DDS " || &dds[84..88] != b"DX10" {
        return None;
    }
    let rd = |o: usize| u32::from_le_bytes(dds[o..o + 4].try_into().unwrap()) as usize;
    let (height, width) = (rd(12), rd(16));
    if rd(128) != 80 || width == 0 || height == 0 {
        return None;
    }
    let (bw, bh) = (width.div_ceil(4), height.div_ceil(4));
    let payload = dds.get(148..148 + bw * bh * 8)?;
    let mut field = HeightField::new(width, height);
    for by in 0..bh {
        for bx in 0..bw {
            let o = (by * bw + bx) * 8;
            let values = bc4::decode_block(&payload[o..o + 8]);
            for (i, v) in values.iter().enumerate() {
                let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                if x < width && y < height {
                    field.data[y * width + x] = *v as f32 / 255.0;
                }
            }
        }
    }
    Some(field)
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub struct OwnedRgba {
        pub width: usize,
        pub height: usize,
        pub pixels: Vec<u8>,
    }

    impl OwnedRgba {
        pub fn new(width: usize, height: usize) -> Self {
            Self {
                width,
                height,
                pixels: vec![0; width * height * 4],
            }
        }
        pub fn set(&mut self, x: usize, y: usize, px: [u8; 4]) {
            let o = (y * self.width + x) * 4;
            self.pixels[o..o + 4].copy_from_slice(&px);
        }
        pub fn view(&self) -> Rgba<'_> {
            Rgba {
                width: self.width,
                height: self.height,
                pixels: &self.pixels,
            }
        }
    }

    /// A periodic, non-separable test surface: two crossed waves plus a few
    /// wrapped Gaussian "stones", one of them straddling the tile corner so
    /// the seam is exercised.
    pub fn bumpy_field(w: usize, h: usize) -> HeightField {
        let mut f = HeightField::new(w, h);
        let tau = std::f32::consts::TAU;
        let stones = [(0.0, 0.0, 0.08), (0.3, 0.6, 0.12), (0.7, 0.25, 0.06)];
        for y in 0..h {
            for x in 0..w {
                let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
                let mut z =
                    0.3 * (tau * u).sin() * (tau * 2.0 * v).cos() + 0.2 * (tau * (u + v)).sin();
                for &(cu, cv, r) in &stones {
                    // Wrapped distance on the torus.
                    let du = (u - cu + 0.5).rem_euclid(1.0) - 0.5;
                    let dv = (v - cv + 0.5).rem_euclid(1.0) - 0.5;
                    z += (-(du * du + dv * dv) / (2.0 * r * r)).exp();
                }
                f.data[y * w + x] = z;
            }
        }
        f
    }

    /// Bake `field` (height in texel units × `scale`) into an 8-bit normal
    /// map with the given green convention, by central differences.
    pub fn bake_normal_map(field: &HeightField, green: GreenConvention, scale: f32) -> OwnedRgba {
        let mut img = OwnedRgba::new(field.width, field.height);
        for y in 0..field.height {
            for x in 0..field.width {
                let (xi, yi) = (x as isize, y as isize);
                let p = scale * 0.5 * (field.at(xi + 1, yi) - field.at(xi - 1, yi));
                let q = scale * 0.5 * (field.at(xi, yi + 1) - field.at(xi, yi - 1));
                // n ∝ (-p, ±q, 1): green-up stores n.y = +q (see `integrate`).
                let ny = match green {
                    GreenConvention::YUp => q,
                    GreenConvention::YDown => -q,
                };
                let len = (p * p + ny * ny + 1.0).sqrt();
                let enc = |c: f32| ((c / len * 0.5 + 0.5) * 255.0).round() as u8;
                img.set(x, y, [enc(-p), enc(ny), enc(1.0), 255]);
            }
        }
        img
    }

    pub fn correlation(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len() as f32;
        let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
        let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
        for (x, y) in a.iter().zip(b) {
            num += (x - ma) * (y - mb);
            da += (x - ma) * (x - ma);
            db += (y - mb) * (y - mb);
        }
        num / (da.sqrt() * db.sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    /// Phase 1 acceptance: every generated map tiles. The left/right and
    /// top/bottom edges must meet like any interior texel pair — no seam step
    /// larger than the largest interior step.
    #[test]
    fn generated_maps_tile_across_both_edges() {
        let truth = bumpy_field(64, 64);
        let normals = bake_normal_map(&truth, GreenConvention::YDown, 6.0);
        let mut diffuse = OwnedRgba::new(64, 64);
        for (i, v) in truth.data.iter().enumerate() {
            let g = ((v * 0.3 + 0.4).clamp(0.0, 1.0) * 255.0) as u8;
            diffuse.set(i % 64, i / 64, [g, g, g, 255]);
        }
        for generated in [
            from_normal_map(&normals.view(), None),
            from_brightness(&diffuse.view()),
            from_authored(&diffuse.view()),
        ] {
            let top = decode_top_mip(&generated.dds).unwrap();
            let ratio = top.seam_step_ratio();
            assert!(
                ratio <= 1.0,
                "{:?}: seam step ratio {ratio}",
                generated.meta.source
            );
        }
    }

    /// Phase 1 acceptance: heights are normalised to exactly `[0, 1]` per
    /// texture, and the map records its source.
    #[test]
    fn generated_maps_span_the_unit_range() {
        let truth = bumpy_field(32, 32);
        let normals = bake_normal_map(&truth, GreenConvention::YUp, 3.0);
        let generated = from_normal_map(&normals.view(), None);
        assert_eq!(generated.meta.source, HeightSource::Normals);
        assert_eq!(generated.meta.green, Some(GreenConvention::YUp));
        let top = decode_top_mip(&generated.dds).unwrap();
        let min = top.data.iter().copied().fold(1.0f32, f32::min);
        let max = top.data.iter().copied().fold(0.0f32, f32::max);
        assert_eq!((min, max), (0.0, 1.0));
        assert!(generated.meta.relief > 0.0);
    }

    /// An oversized source is reduced to the [`MAX_EDGE`] budget, and the
    /// reduction keeps it tileable.
    #[test]
    fn oversized_sources_are_reduced_to_the_budget() {
        let big = bake_normal_map(
            &bumpy_field(2 * MAX_EDGE, MAX_EDGE / 2),
            GreenConvention::YDown,
            6.0,
        );
        let generated = from_normal_map(&big.view(), None);
        assert_eq!(
            (generated.meta.width, generated.meta.height),
            (MAX_EDGE as u32, (MAX_EDGE / 4) as u32)
        );
        assert_eq!(generated.meta.green, Some(GreenConvention::YDown));
    }
}
