use crate::MapRole;
use image::{Rgba, RgbaImage};

/// Joint-bilateral upscale of a semantic map using the learned high-resolution
/// color result as the guide and the original color map as its low-resolution
/// correspondence. A 2x2 footprint keeps the operation O(output pixels).
pub(crate) fn guided_upscale(
    reference_low: &RgbaImage,
    reference_high: &RgbaImage,
    map_low: &RgbaImage,
    scale: u32,
    role: MapRole,
    sigma: f32,
) -> RgbaImage {
    let output_width = map_low.width().saturating_mul(scale);
    let output_height = map_low.height().saturating_mul(scale);
    let mut output = RgbaImage::new(output_width, output_height);
    let sigma_denominator = 2.0 * sigma * sigma;

    for y in 0..output_height {
        for x in 0..output_width {
            let u = (x as f32 + 0.5) / output_width as f32;
            let v = (y as f32 + 0.5) / output_height as f32;
            let guide = sample_bilinear(reference_high, u, v);

            let map_x = u * map_low.width() as f32 - 0.5;
            let map_y = v * map_low.height() as f32 - 0.5;
            let x0 = map_x.floor() as i32;
            let y0 = map_y.floor() as i32;
            let tx = map_x - x0 as f32;
            let ty = map_y - y0 as f32;

            let mut weights = [0.0f32; 4];
            let mut samples = [[0.0f32; 4]; 4];
            let candidates = [
                (x0, y0, (1.0 - tx) * (1.0 - ty)),
                (x0 + 1, y0, tx * (1.0 - ty)),
                (x0, y0 + 1, (1.0 - tx) * ty),
                (x0 + 1, y0 + 1, tx * ty),
            ];

            for (index, (candidate_x, candidate_y, spatial_weight)) in
                candidates.into_iter().enumerate()
            {
                let candidate_x = candidate_x.clamp(0, map_low.width() as i32 - 1) as u32;
                let candidate_y = candidate_y.clamp(0, map_low.height() as i32 - 1) as u32;
                let candidate_u = (candidate_x as f32 + 0.5) / map_low.width() as f32;
                let candidate_v = (candidate_y as f32 + 0.5) / map_low.height() as f32;
                let low_guide = sample_bilinear(reference_low, candidate_u, candidate_v);
                let color_distance = squared_rgb_distance(guide, low_guide);
                let edge_weight = (-color_distance / sigma_denominator).exp();
                weights[index] = spatial_weight * edge_weight;
                samples[index] = rgba_to_unit(*map_low.get_pixel(candidate_x, candidate_y));
            }

            let weight_sum: f32 = weights.iter().sum();
            if weight_sum <= f32::EPSILON {
                weights = [
                    (1.0 - tx) * (1.0 - ty),
                    tx * (1.0 - ty),
                    (1.0 - tx) * ty,
                    tx * ty,
                ];
            }

            let pixel = if role == MapRole::Normal {
                blend_normal(samples, weights)
            } else {
                blend_channels(samples, weights)
            };
            output.put_pixel(x, y, unit_to_rgba(pixel));
        }
    }
    output
}

pub(crate) fn merge_reference_alpha(
    reference_low: &RgbaImage,
    reference_high: &RgbaImage,
    scale: u32,
    sigma: f32,
) -> RgbaImage {
    let alpha_guide = guided_upscale(
        reference_low,
        reference_high,
        reference_low,
        scale,
        MapRole::Mask,
        sigma,
    );
    let mut merged = reference_high.clone();
    for (pixel, alpha) in merged.pixels_mut().zip(alpha_guide.pixels()) {
        pixel.0[3] = alpha.0[3];
    }
    merged
}

fn blend_channels(samples: [[f32; 4]; 4], weights: [f32; 4]) -> [f32; 4] {
    let weight_sum = weights.iter().sum::<f32>().max(f32::EPSILON);
    let mut result = [0.0f32; 4];
    for (sample, weight) in samples.into_iter().zip(weights) {
        for channel in 0..4 {
            result[channel] += sample[channel] * weight;
        }
    }
    for channel in &mut result {
        *channel /= weight_sum;
    }
    result
}

fn blend_normal(samples: [[f32; 4]; 4], weights: [f32; 4]) -> [f32; 4] {
    let weight_sum = weights.iter().sum::<f32>().max(f32::EPSILON);
    let mut vector = [0.0f32; 3];
    let mut alpha = 0.0f32;
    for (sample, weight) in samples.into_iter().zip(weights) {
        vector[0] += (sample[0] * 2.0 - 1.0) * weight;
        vector[1] += (sample[1] * 2.0 - 1.0) * weight;
        vector[2] += (sample[2] * 2.0 - 1.0) * weight;
        alpha += sample[3] * weight;
    }
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length > 1.0e-6 {
        for component in &mut vector {
            *component /= length;
        }
    } else {
        vector = [0.0, 0.0, 1.0];
    }
    [
        vector[0] * 0.5 + 0.5,
        vector[1] * 0.5 + 0.5,
        vector[2] * 0.5 + 0.5,
        alpha / weight_sum,
    ]
}

fn sample_bilinear(image: &RgbaImage, u: f32, v: f32) -> [f32; 4] {
    let x = u.clamp(0.0, 1.0) * image.width() as f32 - 0.5;
    let y = v.clamp(0.0, 1.0) * image.height() as f32 - 0.5;
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let sample = |sx: i32, sy: i32| {
        let sx = sx.clamp(0, image.width() as i32 - 1) as u32;
        let sy = sy.clamp(0, image.height() as i32 - 1) as u32;
        rgba_to_unit(*image.get_pixel(sx, sy))
    };
    let a = sample(x0, y0);
    let b = sample(x0 + 1, y0);
    let c = sample(x0, y0 + 1);
    let d = sample(x0 + 1, y0 + 1);
    let mut result = [0.0; 4];
    for channel in 0..4 {
        let top = a[channel] * (1.0 - tx) + b[channel] * tx;
        let bottom = c[channel] * (1.0 - tx) + d[channel] * tx;
        result[channel] = top * (1.0 - ty) + bottom * ty;
    }
    result
}

fn squared_rgb_distance(a: [f32; 4], b: [f32; 4]) -> f32 {
    let dr = a[0] - b[0];
    let dg = a[1] - b[1];
    let db = a[2] - b[2];
    (dr * dr + dg * dg + db * db) / 3.0
}

fn rgba_to_unit(pixel: Rgba<u8>) -> [f32; 4] {
    pixel.0.map(|channel| channel as f32 / 255.0)
}

fn unit_to_rgba(pixel: [f32; 4]) -> Rgba<u8> {
    Rgba(pixel.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_scalar_map_stays_constant() {
        let low_guide = RgbaImage::from_pixel(2, 2, Rgba([64, 80, 96, 255]));
        let high_guide = RgbaImage::from_pixel(8, 8, Rgba([64, 80, 96, 255]));
        let map = RgbaImage::from_pixel(2, 2, Rgba([23, 45, 67, 89]));
        let output = guided_upscale(&low_guide, &high_guide, &map, 4, MapRole::Mask, 0.12);
        assert_eq!(output.dimensions(), (8, 8));
        assert!(output
            .pixels()
            .all(|pixel| *pixel == Rgba([23, 45, 67, 89])));
    }

    #[test]
    fn learned_reference_places_the_companion_edge() {
        let low_guide = RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let high_guide = RgbaImage::from_fn(4, 2, |x, _| {
            if x < 2 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let map = RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });

        let output = guided_upscale(&low_guide, &high_guide, &map, 2, MapRole::Mask, 0.12);

        assert!(output.get_pixel(1, 0)[0] < 10);
        assert!(output.get_pixel(2, 0)[0] > 245);
    }

    #[test]
    fn normal_output_is_renormalized_and_preserves_alpha() {
        let low_guide = RgbaImage::from_pixel(1, 1, Rgba([128, 128, 128, 255]));
        let high_guide = RgbaImage::from_pixel(4, 4, Rgba([128, 128, 128, 255]));
        let map = RgbaImage::from_pixel(1, 1, Rgba([200, 128, 200, 77]));
        let output = guided_upscale(&low_guide, &high_guide, &map, 4, MapRole::Normal, 0.12);
        let pixel = output.get_pixel(0, 0);
        let x = pixel[0] as f32 / 255.0 * 2.0 - 1.0;
        let y = pixel[1] as f32 / 255.0 * 2.0 - 1.0;
        let z = pixel[2] as f32 / 255.0 * 2.0 - 1.0;
        let length = (x * x + y * y + z * z).sqrt();
        assert!((length - 1.0).abs() < 0.02, "normal length {length}");
        assert_eq!(pixel[3], 77);
    }

    #[test]
    fn learned_reference_rgb_keeps_guided_original_alpha() {
        let low = RgbaImage::from_pixel(1, 1, Rgba([10, 20, 30, 41]));
        let high = RgbaImage::from_pixel(2, 2, Rgba([200, 210, 220, 255]));
        let merged = merge_reference_alpha(&low, &high, 2, 0.12);
        assert!(merged
            .pixels()
            .all(|pixel| *pixel == Rgba([200, 210, 220, 41])));
    }
}
