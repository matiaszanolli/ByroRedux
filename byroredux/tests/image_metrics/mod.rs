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

impl std::fmt::Display for ImageMetrics {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "ssim {:.4}, max Δ {}, outliers {:.3}%, mean Δ {:.2}",
            self.ssim, self.max_channel_delta, self.outlier_pct, self.mean_abs_delta
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

    let mut total = 0.0;
    let mut windows = 0u64;
    let mut y = 0;
    while y + SSIM_WINDOW <= height {
        let mut x = 0;
        while x + SSIM_WINDOW <= width {
            total += window_ssim(&reference_luma, &candidate_luma, width, x, y);
            windows += 1;
            x += SSIM_WINDOW;
        }
        y += SSIM_WINDOW;
    }

    if windows == 0 {
        // Image smaller than one window — compare it as a single window
        // rather than reporting a meaningless 0.0.
        return window_ssim_over(&reference_luma, &candidate_luma, width, 0, 0, width, height);
    }
    total / windows as f64
}

fn window_ssim(reference: &[f64], candidate: &[f64], width: u32, x: u32, y: u32) -> f64 {
    window_ssim_over(
        reference,
        candidate,
        width,
        x,
        y,
        SSIM_WINDOW.min(width),
        SSIM_WINDOW,
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

    ((2.0 * mean_r * mean_c + SSIM_C1) * (2.0 * covariance + SSIM_C2))
        / ((mean_r * mean_r + mean_c * mean_c + SSIM_C1) * (var_r + var_c + SSIM_C2))
}

/// ITU-R BT.601 luma. The frames are already tone-mapped and encoded for
/// display, so this operates in the same space a viewer sees.
fn luma_plane(image: &RgbImage) -> Vec<f64> {
    image
        .pixels()
        .map(|p| 0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2]))
        .collect()
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
