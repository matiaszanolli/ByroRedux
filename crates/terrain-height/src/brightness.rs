//! Diffuse brightness → height, the fallback for a layer with no normal map.
//!
//! Brightness is a weak proxy for height: it carries paint (a red stone on
//! grey dirt) as well as relief. The high-pass keeps the small-scale part
//! (pebbles, cracks, the cavity darkening between stones, which is where
//! brightness does track relief) and drops the large-scale part (colour
//! patches across the tile, which is where it does not).

use crate::field::HeightField;
use crate::Rgba;

/// The high-pass cut-off: the Gaussian sigma subtracted from the luminance,
/// as a fraction of the texture's width. A tunable, not a measured value —
/// features larger than roughly `2·sigma` are treated as colour variation
/// and removed. 1/32 of a 1024-texel layer is 32 texels; at the LAND tiling
/// rate that is a few centimetres of ground.
pub const HIGHPASS_SIGMA_FRACTION: f32 = 1.0 / 32.0;

/// Rec. 709 luma of the stored (display-referred) values. Brightness here is
/// a shape cue, not a light quantity, so no transfer-function decode.
fn luma(px: &[u8]) -> f32 {
    (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) / 255.0
}

/// High-passed luminance of `img`, zero mean, in luminance units.
pub fn highpass_luminance(img: &Rgba) -> HeightField {
    let mut field = HeightField::new(img.width, img.height);
    for (dst, px) in field.data.iter_mut().zip(img.pixels.chunks_exact(4)) {
        *dst = luma(px);
    }
    let sigma = (img.width as f32 * HIGHPASS_SIGMA_FRACTION).max(1.0);
    let blurred = gaussian_blur_wrap(&field, sigma);
    for (v, b) in field.data.iter_mut().zip(&blurred.data) {
        *v -= b;
    }
    field
}

/// Separable Gaussian blur with wrap addressing, so the filtered field tiles
/// exactly as its source does.
pub fn gaussian_blur_wrap(field: &HeightField, sigma: f32) -> HeightField {
    let radius = (3.0 * sigma).ceil() as isize;
    let kernel: Vec<f32> = (-radius..=radius)
        .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
        .collect();
    let norm: f32 = kernel.iter().sum();
    let kernel: Vec<f32> = kernel.iter().map(|k| k / norm).collect();

    let mut horizontal = HeightField::new(field.width, field.height);
    for y in 0..field.height as isize {
        for x in 0..field.width as isize {
            let mut acc = 0.0;
            for (k, w) in (-radius..=radius).zip(&kernel) {
                acc += w * field.at(x + k, y);
            }
            horizontal.data[y as usize * field.width + x as usize] = acc;
        }
    }
    let mut out = HeightField::new(field.width, field.height);
    for y in 0..field.height as isize {
        for x in 0..field.width as isize {
            let mut acc = 0.0;
            for (k, w) in (-radius..=radius).zip(&kernel) {
                acc += w * horizontal.at(x, y + k);
            }
            out.data[y as usize * field.width + x as usize] = acc;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::OwnedRgba;

    /// A large-scale colour gradient is removed; small-scale detail stays.
    #[test]
    fn highpass_drops_broad_colour_and_keeps_fine_detail() {
        let (w, h) = (128, 128);
        let mut img = OwnedRgba::new(w, h);
        for y in 0..h {
            for x in 0..w {
                // A broad periodic brightness swell across the tile …
                let broad = 0.5 + 0.4 * (std::f32::consts::TAU * x as f32 / w as f32).sin();
                // … plus a fine 4-texel checker of pebbles.
                let fine = if (x / 2 + y / 2) % 2 == 0 {
                    0.05
                } else {
                    -0.05
                };
                let v = ((broad + fine) * 255.0).round() as u8;
                img.set(x, y, [v, v, v, 255]);
            }
        }
        let field = highpass_luminance(&img.view());
        // The swell's amplitude (0.4) is gone; the checker's (0.05) is not.
        let max = field.data.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(max < 0.12, "broad swell survived: {max}");
        assert!(max > 0.03, "fine detail removed: {max}");
    }
}
