//! Image comparison metrics for the upscaler quality harness.
//!
//! The pre-existing golden test compares per-pixel against a checked-in
//! baseline, which answers "did this change the image". That is the wrong
//! question for an upscaler: FSR reconstructs from fewer samples, so it is
//! *supposed* to differ from a native render everywhere, and a per-pixel
//! threshold either passes trivially or fails always.
//!
//! The question that matters is "does the reconstruction still look like the
//! reference", which is what SSIM measures — structural agreement over local
//! windows, insensitive to the uniform brightness and contrast offsets a
//! resolve pass legitimately introduces, sensitive to smearing, ghosting,
//! and missing detail.
//!
//! SSIM alone is not enough, though: it is a mean over the frame, so a small
//! but catastrophic region (one smeared object, one block of corrupt pixels)
//! can hide behind a good average. Hence the plan's three-metric rule, all
//! reported together:
//!
//! - **SSIM** — structural agreement, the headline number.
//! - **Max channel error** — the worst single pixel, which catches the local
//!   catastrophe SSIM averages away.
//! - **Outlier percentage** — how much of the frame is badly wrong, which
//!   distinguishes "one bad pixel" from "a bad region".
//!
//! Exact hashes are deliberately *not* a gate. They are driver- and
//! device-dependent for anything involving floating-point reduction order,
//! which is most of this renderer.

use image::RgbImage;

/// Per-channel error above this (of 255) counts a pixel as an outlier.
///
/// Sits above the tone-mapped quantization noise two runs of the same
/// configuration produce (~8/255 measured on Cornell) so run-to-run
/// nondeterminism cannot inflate the outlier rate, but well below the
/// magnitude of a real reconstruction artifact.
pub const OUTLIER_CHANNEL_DELTA: u8 = 24;

/// Result of comparing a candidate frame against a reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageMetrics {
    /// Mean SSIM over the luma plane, in `[-1, 1]`. 1.0 is identical.
    pub ssim: f64,
    /// Largest single-channel absolute difference, in `[0, 255]`.
    pub max_channel_delta: u8,
    /// Percentage of pixels with any channel differing by more than
    /// [`OUTLIER_CHANNEL_DELTA`].
    pub outlier_pct: f64,
    /// Mean absolute per-channel difference, in `[0, 255]`. Reported for
    /// context rather than gated — it is the number that best distinguishes
    /// "slightly softer" from "systematically darker".
    pub mean_abs_delta: f64,
}

/// Linear-light comparison used by the reference-vs-candidate renderer gate.
///
/// Screenshots are stored as sRGB PNGs, but renderer regressions are energy
/// errors. Convert channels back to linear light before measuring so the same
/// encoded-byte delta is not treated as equally significant in shadows and
/// highlights. Percentiles keep one pathological texel from dominating the
/// verdict while `max_abs_delta` still records that texel for diagnosis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearImageMetrics {
    /// Mean SSIM over linear Rec.709 luma, in `[-1, 1]`.
    pub ssim: f64,
    /// Largest absolute linear-light channel error, in `[0, 1]`.
    pub max_abs_delta: f64,
    /// Nearest-rank 95th-percentile absolute channel error.
    pub p95_abs_delta: f64,
    /// Nearest-rank 99th-percentile absolute channel error.
    pub p99_abs_delta: f64,
    /// Percentage of pixels with any linear channel error strictly above the
    /// caller-supplied outlier threshold.
    pub outlier_pct: f64,
    /// Mean absolute linear channel error.
    pub mean_abs_delta: f64,
}

impl std::fmt::Display for ImageMetrics {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "ssim {:.4}, max Δ {}, outliers {:.3}%, mean Δ {:.2}",
            self.ssim, self.max_channel_delta, self.outlier_pct, self.mean_abs_delta
        )
    }
}

impl std::fmt::Display for LinearImageMetrics {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "linear ssim {:.6}, max Δ {:.6}, p95 Δ {:.6}, p99 Δ {:.6}, \
             outliers {:.4}%, mean Δ {:.6}",
            self.ssim,
            self.max_abs_delta,
            self.p95_abs_delta,
            self.p99_abs_delta,
            self.outlier_pct,
            self.mean_abs_delta,
        )
    }
}

/// Compare `candidate` against `reference`.
///
/// Both images must have identical dimensions — for the upscaler matrix they
/// do by construction, since every preset renders to the same *output*
/// resolution and only the internal render resolution varies. That is exactly
/// what makes the comparison meaningful.
pub fn compare(reference: &RgbImage, candidate: &RgbImage) -> Result<ImageMetrics, String> {
    if reference.dimensions() != candidate.dimensions() {
        return Err(format!(
            "dimension mismatch: reference {:?} vs candidate {:?}",
            reference.dimensions(),
            candidate.dimensions()
        ));
    }

    let mut max_channel_delta = 0u8;
    let mut outliers = 0u64;
    let mut sum_abs = 0u64;
    for (r, c) in reference.pixels().zip(candidate.pixels()) {
        let mut is_outlier = false;
        for channel in 0..3 {
            let delta = r[channel].abs_diff(c[channel]);
            max_channel_delta = max_channel_delta.max(delta);
            sum_abs += u64::from(delta);
            if delta > OUTLIER_CHANNEL_DELTA {
                is_outlier = true;
            }
        }
        if is_outlier {
            outliers += 1;
        }
    }

    let pixels = u64::from(reference.width()) * u64::from(reference.height());
    Ok(ImageMetrics {
        ssim: mean_ssim(reference, candidate),
        max_channel_delta,
        outlier_pct: outliers as f64 / pixels as f64 * 100.0,
        mean_abs_delta: sum_abs as f64 / (pixels * 3) as f64,
    })
}

/// Compare two tone-mapped PNGs after decoding their sRGB channels back to
/// linear light.
pub fn compare_linear(
    reference: &RgbImage,
    candidate: &RgbImage,
    outlier_abs_delta: f64,
) -> Result<LinearImageMetrics, String> {
    validate_linear_inputs(reference, candidate, outlier_abs_delta)?;
    let reference_planes = linear_rgb_planes(reference);
    let candidate_planes = linear_rgb_planes(candidate);
    Ok(compare_linear_planes(
        &reference_planes,
        &candidate_planes,
        reference.width(),
        reference.height(),
        outlier_abs_delta,
    ))
}

/// Compare low-frequency image structure in linear light.
///
/// The path tracer intentionally emits stochastic high-frequency samples. A
/// semantically irrelevant shader-control-flow edit can change that sample
/// realization without changing the converged image, so raw per-pixel metrics
/// are diagnostic rather than a safe cross-binary gate. Apply a small,
/// deterministic 5x5 binomial filter (`[1,4,6,4,1] / 16`, separable) to each
/// linear RGB plane before scoring. Large/localized faults survive; individual
/// Monte Carlo speckles do not dominate the verdict.
pub fn compare_linear_low_pass(
    reference: &RgbImage,
    candidate: &RgbImage,
    outlier_abs_delta: f64,
) -> Result<LinearImageMetrics, String> {
    validate_linear_inputs(reference, candidate, outlier_abs_delta)?;
    let (width, height) = reference.dimensions();
    let reference_planes =
        linear_rgb_planes(reference).map(|plane| binomial_low_pass_5(&plane, width, height));
    let candidate_planes =
        linear_rgb_planes(candidate).map(|plane| binomial_low_pass_5(&plane, width, height));
    Ok(compare_linear_planes(
        &reference_planes,
        &candidate_planes,
        width,
        height,
        outlier_abs_delta,
    ))
}

fn validate_linear_inputs(
    reference: &RgbImage,
    candidate: &RgbImage,
    outlier_abs_delta: f64,
) -> Result<(), String> {
    if reference.dimensions() != candidate.dimensions() {
        return Err(format!(
            "dimension mismatch: reference {:?} vs candidate {:?}",
            reference.dimensions(),
            candidate.dimensions()
        ));
    }
    if !(0.0..=1.0).contains(&outlier_abs_delta) || !outlier_abs_delta.is_finite() {
        return Err(format!(
            "linear outlier threshold must be finite and in [0, 1], got {outlier_abs_delta}"
        ));
    }

    let pixel_count = u64::from(reference.width()) * u64::from(reference.height());
    if pixel_count == 0 {
        return Err("cannot compare empty images".to_owned());
    }
    Ok(())
}

fn compare_linear_planes(
    reference: &[Vec<f64>; 3],
    candidate: &[Vec<f64>; 3],
    width: u32,
    height: u32,
    outlier_abs_delta: f64,
) -> LinearImageMetrics {
    let pixel_count = u64::from(width) * u64::from(height);

    let mut deltas = Vec::with_capacity(pixel_count as usize * 3);
    let mut outliers = 0u64;
    let mut sum_abs = 0.0f64;
    for index in 0..pixel_count as usize {
        let mut pixel_is_outlier = false;
        for channel in 0..3 {
            let delta = (reference[channel][index] - candidate[channel][index]).abs();
            sum_abs += delta;
            deltas.push(delta as f32);
            pixel_is_outlier |= delta > outlier_abs_delta;
        }
        if pixel_is_outlier {
            outliers += 1;
        }
    }
    deltas.sort_unstable_by(f32::total_cmp);

    let reference_luma = linear_luma_from_planes(reference);
    let candidate_luma = linear_luma_from_planes(candidate);
    LinearImageMetrics {
        ssim: mean_ssim_planes(
            &reference_luma,
            &candidate_luma,
            width,
            height,
            (0.01f64).powi(2),
            (0.03f64).powi(2),
        ),
        max_abs_delta: f64::from(*deltas.last().expect("non-empty image has channel deltas")),
        p95_abs_delta: nearest_rank(&deltas, 0.95),
        p99_abs_delta: nearest_rank(&deltas, 0.99),
        outlier_pct: outliers as f64 / pixel_count as f64 * 100.0,
        mean_abs_delta: sum_abs / (pixel_count * 3) as f64,
    }
}

/// Window side length for the local SSIM statistics.
///
/// Wang et al. (2004) use an 11×11 Gaussian; this uses a uniform 8×8 window,
/// which is the standard simplification when the input is not pre-blurred.
/// The window has to be wide enough to contain the structures being compared
/// — an upscaler's artifacts are several pixels across — and small enough
/// that a local failure does not average into a large neighbourhood.
const SSIM_WINDOW: u32 = 8;

/// Stabilizing constants from Wang et al. (2004), for 8-bit dynamic range
/// L = 255: C1 = (0.01 L)², C2 = (0.03 L)². They keep the ratio well-defined
/// where local mean or variance approaches zero — which happens constantly
/// here, since the Cornell scene has large flat regions and a black surround.
const SSIM_C1: f64 = (0.01 * 255.0) * (0.01 * 255.0);
const SSIM_C2: f64 = (0.03 * 255.0) * (0.03 * 255.0);

/// Mean SSIM over non-overlapping windows of the luma plane.
///
/// Luma rather than per-channel: the artifacts this harness exists to catch
/// (smearing, ghosting, lost detail) are structural, and computing three
/// channels would triple the cost to report essentially the same number.
/// Chroma-only failures are caught by the max-delta and outlier metrics.
fn mean_ssim(reference: &RgbImage, candidate: &RgbImage) -> f64 {
    let (width, height) = reference.dimensions();
    let reference_luma = luma_plane(reference);
    let candidate_luma = luma_plane(candidate);

    mean_ssim_planes(
        &reference_luma,
        &candidate_luma,
        width,
        height,
        SSIM_C1,
        SSIM_C2,
    )
}

fn mean_ssim_planes(
    reference_luma: &[f64],
    candidate_luma: &[f64],
    width: u32,
    height: u32,
    c1: f64,
    c2: f64,
) -> f64 {
    let mut total = 0.0;
    let mut windows = 0u64;
    let mut y = 0;
    while y + SSIM_WINDOW <= height {
        let mut x = 0;
        while x + SSIM_WINDOW <= width {
            total += window_ssim(reference_luma, candidate_luma, width, x, y, c1, c2);
            windows += 1;
            x += SSIM_WINDOW;
        }
        y += SSIM_WINDOW;
    }

    if windows == 0 {
        // Image smaller than one window — compare it as a single window
        // rather than reporting a meaningless 0.0.
        return window_ssim_over(
            reference_luma,
            candidate_luma,
            width,
            0,
            0,
            width,
            height,
            c1,
            c2,
        );
    }
    total / windows as f64
}

fn window_ssim(
    reference: &[f64],
    candidate: &[f64],
    width: u32,
    x: u32,
    y: u32,
    c1: f64,
    c2: f64,
) -> f64 {
    window_ssim_over(
        reference,
        candidate,
        width,
        x,
        y,
        SSIM_WINDOW.min(width),
        SSIM_WINDOW,
        c1,
        c2,
    )
}

fn window_ssim_over(
    reference: &[f64],
    candidate: &[f64],
    width: u32,
    x0: u32,
    y0: u32,
    window_width: u32,
    window_height: u32,
    c1: f64,
    c2: f64,
) -> f64 {
    let n = f64::from(window_width * window_height);
    let mut mean_r = 0.0;
    let mut mean_c = 0.0;
    for y in y0..y0 + window_height {
        for x in x0..x0 + window_width {
            let index = (y * width + x) as usize;
            mean_r += reference[index];
            mean_c += candidate[index];
        }
    }
    mean_r /= n;
    mean_c /= n;

    let mut var_r = 0.0;
    let mut var_c = 0.0;
    let mut covariance = 0.0;
    for y in y0..y0 + window_height {
        for x in x0..x0 + window_width {
            let index = (y * width + x) as usize;
            let dr = reference[index] - mean_r;
            let dc = candidate[index] - mean_c;
            var_r += dr * dr;
            var_c += dc * dc;
            covariance += dr * dc;
        }
    }
    // Sample variance (n-1) per Wang et al.; identical inputs still give
    // exactly 1.0 because the covariance term matches the variances.
    let denominator = (n - 1.0).max(1.0);
    var_r /= denominator;
    var_c /= denominator;
    covariance /= denominator;

    ((2.0 * mean_r * mean_c + c1) * (2.0 * covariance + c2))
        / ((mean_r * mean_r + mean_c * mean_c + c1) * (var_r + var_c + c2))
}

/// ITU-R BT.601 luma. The frames are already tone-mapped and encoded for
/// display, so this operates in the same space a viewer sees.
fn luma_plane(image: &RgbImage) -> Vec<f64> {
    image
        .pixels()
        .map(|p| 0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2]))
        .collect()
}

fn linear_rgb_planes(image: &RgbImage) -> [Vec<f64>; 3] {
    let len = image.width() as usize * image.height() as usize;
    let mut planes = [
        Vec::with_capacity(len),
        Vec::with_capacity(len),
        Vec::with_capacity(len),
    ];
    for pixel in image.pixels() {
        for channel in 0..3 {
            planes[channel].push(srgb_u8_to_linear(pixel[channel]));
        }
    }
    planes
}

fn linear_luma_from_planes(planes: &[Vec<f64>; 3]) -> Vec<f64> {
    (0..planes[0].len())
        .map(|index| {
            0.2126 * planes[0][index] + 0.7152 * planes[1][index] + 0.0722 * planes[2][index]
        })
        .collect()
}

fn binomial_low_pass_5(input: &[f64], width: u32, height: u32) -> Vec<f64> {
    const WEIGHTS: [f64; 5] = [1.0, 4.0, 6.0, 4.0, 1.0];
    const NORMALIZER: f64 = 16.0;
    let width = width as usize;
    let height = height as usize;
    let mut horizontal = vec![0.0; input.len()];
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            for (kernel_index, weight) in WEIGHTS.iter().enumerate() {
                let sample_x = x
                    .saturating_add(kernel_index)
                    .saturating_sub(2)
                    .min(width - 1);
                sum += input[y * width + sample_x] * weight;
            }
            horizontal[y * width + x] = sum / NORMALIZER;
        }
    }

    let mut output = vec![0.0; input.len()];
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            for (kernel_index, weight) in WEIGHTS.iter().enumerate() {
                let sample_y = y
                    .saturating_add(kernel_index)
                    .saturating_sub(2)
                    .min(height - 1);
                sum += horizontal[sample_y * width + x] * weight;
            }
            output[y * width + x] = sum / NORMALIZER;
        }
    }
    output
}

fn srgb_u8_to_linear(channel: u8) -> f64 {
    let encoded = f64::from(channel) / 255.0;
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

fn nearest_rank(sorted: &[f32], quantile: f64) -> f64 {
    let rank = (sorted.len() as f64 * quantile).ceil() as usize;
    f64::from(sorted[rank.saturating_sub(1).min(sorted.len() - 1)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn solid(width: u32, height: u32, rgb: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(width, height, Rgb(rgb))
    }

    /// Deterministic pseudo-texture — a real image has local variance, which
    /// is what SSIM is actually measuring. A flat field would make several of
    /// these tests pass vacuously.
    fn checkerboard(width: u32, height: u32, scale: u32) -> RgbImage {
        RgbImage::from_fn(width, height, |x, y| {
            let on = ((x / scale) + (y / scale)).is_multiple_of(2);
            let v = if on { 200 } else { 40 };
            Rgb([v, v.saturating_sub(10), v.saturating_add(10)])
        })
    }

    #[test]
    fn identical_images_score_a_perfect_ssim_and_zero_error() {
        let image = checkerboard(64, 64, 4);
        let metrics = compare(&image, &image).unwrap();
        assert!(
            (metrics.ssim - 1.0).abs() < 1e-9,
            "ssim {} != 1.0",
            metrics.ssim
        );
        assert_eq!(metrics.max_channel_delta, 0);
        assert_eq!(metrics.outlier_pct, 0.0);
        assert_eq!(metrics.mean_abs_delta, 0.0);
    }

    #[test]
    fn identical_linear_images_have_zero_error_at_every_percentile() {
        let image = checkerboard(64, 64, 4);
        let metrics = compare_linear(&image, &image, 0.03).unwrap();
        assert!((metrics.ssim - 1.0).abs() < 1e-12);
        assert_eq!(metrics.max_abs_delta, 0.0);
        assert_eq!(metrics.p95_abs_delta, 0.0);
        assert_eq!(metrics.p99_abs_delta, 0.0);
        assert_eq!(metrics.outlier_pct, 0.0);
        assert_eq!(metrics.mean_abs_delta, 0.0);
    }

    #[test]
    fn linear_metrics_catch_a_localized_magenta_fault() {
        let reference = checkerboard(128, 128, 4);
        let mut candidate = reference.clone();
        for y in 32..96 {
            for x in 32..96 {
                candidate.put_pixel(x, y, Rgb([255, 0, 255]));
            }
        }
        let metrics = compare_linear(&reference, &candidate, 0.03).unwrap();
        assert!(metrics.ssim < 0.9, "{}", metrics.ssim);
        assert!(metrics.max_abs_delta > 0.5, "{}", metrics.max_abs_delta);
        assert!(metrics.p99_abs_delta > 0.1, "{}", metrics.p99_abs_delta);
        assert!(metrics.outlier_pct > 20.0, "{}", metrics.outlier_pct);
    }

    #[test]
    fn linear_low_pass_attenuates_a_single_stochastic_speckle() {
        let reference = checkerboard(64, 64, 8);
        let mut candidate = reference.clone();
        candidate.put_pixel(32, 32, Rgb([255, 0, 255]));
        let raw = compare_linear(&reference, &candidate, 0.03).unwrap();
        let filtered = compare_linear_low_pass(&reference, &candidate, 0.03).unwrap();
        assert!(filtered.max_abs_delta < raw.max_abs_delta / 4.0);
        assert!(filtered.ssim > raw.ssim);
    }

    #[test]
    fn linear_low_pass_preserves_a_localized_structural_fault() {
        let reference = checkerboard(128, 128, 8);
        let mut candidate = reference.clone();
        for y in 32..96 {
            for x in 32..96 {
                candidate.put_pixel(x, y, Rgb([255, 0, 255]));
            }
        }
        let metrics = compare_linear_low_pass(&reference, &candidate, 0.03).unwrap();
        assert!(metrics.ssim < 0.9, "{}", metrics.ssim);
        assert!(metrics.max_abs_delta > 0.5, "{}", metrics.max_abs_delta);
        assert!(metrics.outlier_pct > 20.0, "{}", metrics.outlier_pct);
    }

    #[test]
    fn linear_metric_rejects_invalid_inputs() {
        let image = solid(8, 8, [0, 0, 0]);
        let different_size = solid(16, 8, [0, 0, 0]);
        let empty = solid(0, 0, [0, 0, 0]);
        assert!(compare_linear(&image, &different_size, 0.03).is_err());
        assert!(compare_linear(&image, &image, -0.1).is_err());
        assert!(compare_linear(&image, &image, f64::NAN).is_err());
        assert!(compare_linear(&empty, &empty, 0.03).is_err());
    }

    /// A blurred image keeps its structure but loses local contrast, which is
    /// the exact signature of an over-smoothing upscaler. SSIM must drop
    /// noticeably while the max-delta stays bounded — if SSIM did not react
    /// here it would be useless for the thing this harness measures.
    #[test]
    fn blur_lowers_ssim_because_local_contrast_is_lost() {
        let sharp = checkerboard(64, 64, 4);
        let blurred = image::imageops::blur(&sharp, 2.0);
        let metrics = compare(&sharp, &blurred).unwrap();
        assert!(
            metrics.ssim < 0.9,
            "blur should cost structural similarity, got {}",
            metrics.ssim
        );
        assert!(
            metrics.ssim > 0.0,
            "blur is not a total loss: {}",
            metrics.ssim
        );
    }

    /// The case SSIM alone would miss: a frame that is correct nearly
    /// everywhere but catastrophically wrong in one small region. The mean
    /// SSIM barely moves; max-delta and the outlier rate are what catch it.
    /// This is why the harness gates on three numbers rather than one.
    #[test]
    fn a_small_corrupt_region_is_caught_by_max_delta_not_by_ssim() {
        let reference = checkerboard(128, 128, 4);
        let mut candidate = reference.clone();
        for y in 0..8 {
            for x in 0..8 {
                candidate.put_pixel(x, y, Rgb([255, 0, 255]));
            }
        }
        let metrics = compare(&reference, &candidate).unwrap();
        assert!(
            metrics.ssim > 0.95,
            "mean SSIM should barely notice 0.4% of the frame, got {}",
            metrics.ssim
        );
        assert!(
            metrics.max_channel_delta > OUTLIER_CHANNEL_DELTA,
            "max delta missed the corrupt block: {}",
            metrics.max_channel_delta
        );
        assert!(
            metrics.outlier_pct > 0.0,
            "outlier rate missed the corrupt block"
        );
    }

    /// A uniform brightness shift is a legitimate resolve-pass difference,
    /// not a structural failure. SSIM should stay high (its luminance term
    /// tolerates it) while the mean-delta reports the offset — that split is
    /// why both numbers are reported.
    #[test]
    fn uniform_brightness_shift_barely_moves_ssim_but_shows_in_mean_delta() {
        let reference = checkerboard(64, 64, 4);
        let brighter = RgbImage::from_fn(64, 64, |x, y| {
            let p = reference.get_pixel(x, y);
            Rgb([
                p[0].saturating_add(6),
                p[1].saturating_add(6),
                p[2].saturating_add(6),
            ])
        });
        let metrics = compare(&reference, &brighter).unwrap();
        assert!(
            metrics.ssim > 0.95,
            "a uniform shift is not a structural failure: {}",
            metrics.ssim
        );
        assert!(metrics.mean_abs_delta >= 5.0, "{}", metrics.mean_abs_delta);
        assert_eq!(metrics.outlier_pct, 0.0, "6/255 is below the outlier bar");
    }

    /// Structurally unrelated content must score far from 1.0, or the metric
    /// would pass anything.
    #[test]
    fn unrelated_images_score_poorly() {
        let a = checkerboard(64, 64, 4);
        let b = checkerboard(64, 64, 13);
        let metrics = compare(&a, &b).unwrap();
        assert!(metrics.ssim < 0.5, "ssim {}", metrics.ssim);
    }

    /// Flat fields have zero variance, which is where a naive SSIM divides by
    /// zero. The stabilizing constants must keep identical flats at 1.0.
    #[test]
    fn flat_fields_do_not_divide_by_zero() {
        let flat = solid(32, 32, [10, 10, 10]);
        let metrics = compare(&flat, &flat).unwrap();
        assert!(
            (metrics.ssim - 1.0).abs() < 1e-9,
            "flat-field ssim {}",
            metrics.ssim
        );
        let black = solid(32, 32, [0, 0, 0]);
        assert!(compare(&black, &black).unwrap().ssim > 0.999);
    }

    /// Images smaller than one window still produce a real number rather
    /// than the 0.0 an empty window loop would yield.
    #[test]
    fn images_smaller_than_a_window_still_compare() {
        let a = checkerboard(4, 4, 1);
        assert!((compare(&a, &a).unwrap().ssim - 1.0).abs() < 1e-9);
    }

    #[test]
    fn dimension_mismatch_is_an_error_not_a_panic() {
        let a = solid(8, 8, [0, 0, 0]);
        let b = solid(16, 8, [0, 0, 0]);
        assert!(compare(&a, &b).is_err());
    }

    /// The outlier threshold is a boundary, so pin both sides of it: a delta
    /// exactly at the threshold is noise, one above it is an outlier.
    #[test]
    fn outlier_threshold_is_exclusive() {
        let base = solid(16, 16, [100, 100, 100]);
        let at = solid(16, 16, [100 + OUTLIER_CHANNEL_DELTA, 100, 100]);
        let over = solid(16, 16, [100 + OUTLIER_CHANNEL_DELTA + 1, 100, 100]);
        assert_eq!(compare(&base, &at).unwrap().outlier_pct, 0.0);
        assert_eq!(compare(&base, &over).unwrap().outlier_pct, 100.0);
    }
}
