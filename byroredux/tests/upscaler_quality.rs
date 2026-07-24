//! FSR upscaler image-quality matrix (execution phase 6 of the FSR plan).
//!
//! Renders the engine-owned Cornell scene once per (camera path × upscaler)
//! pair and scores every FSR preset against the native-resolution TAA render
//! of the same frame.
//!
//! ## Why TAA at native is the reference
//!
//! It is the image the engine ships today. Phase 7's decision is "should FSR
//! Quality become the default", and that question is precisely "would a
//! player notice the switch" — so the thing to measure against is the current
//! look, not an abstract ground truth. FSR Native AA is scored too, which
//! separates the cost of *reconstruction* (present in every preset, including
//! 1.0×) from the cost of *upscaling* (only in the reduced presets).
//!
//! ## Why not checked-in goldens
//!
//! An upscaler reconstructs from fewer samples, so it is supposed to differ
//! from the reference everywhere; a per-pixel golden threshold would either
//! pass trivially or fail always. Both images here are also produced in the
//! same run on the same device, which sidesteps the driver-dependent golden
//! brittleness the plan's risk register calls out — and, since the scene is
//! engine-owned, nothing game-derived is ever written to disk.
//!
//! ## Running
//!
//! ```bash
//! cargo test --release -p byroredux --test upscaler_quality -- --ignored --nocapture
//! ```
//!
//! `--nocapture` is worth it: the test prints the full metric table, which is
//! the artifact this harness exists to produce. Thresholds only fail the run;
//! the table is what gets read.

mod image_metrics;

use image_metrics::{compare, ImageMetrics};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Bench length per capture. Long enough for SVGF and the upscaler's history
/// to converge from cold, short enough that a 25-run matrix stays tolerable.
const FRAMES: u32 = 60;

/// Camera paths exercised. Names match `--bench-camera`.
const PATHS: [&str; 5] = ["static", "pan", "orbit", "dolly", "cut"];

/// FSR presets scored against the reference, in ascending upscale ratio.
const PRESETS: [&str; 4] = ["native-aa", "quality", "balanced", "performance"];

/// Per-preset acceptance thresholds.
///
/// **Measured, not chosen.** Three full matrix runs on an RTX 4070 Ti
/// (driver as of 2026-07-24) produced these worst-case values across all five
/// camera paths:
///
/// | preset      | worst SSIM | worst outliers |
/// |-------------|-----------:|---------------:|
/// | native-aa   |     0.9906 |         0.429% |
/// | quality     |     0.9554 |         1.681% |
/// | balanced    |     0.9460 |         3.183% |
/// | performance |     0.9199 |         5.387% |
///
/// Run-to-run spread over those three runs was |Δssim| ≤ 0.0001 and
/// |Δoutlier| ≤ 0.05 pp — the capture path is effectively deterministic, since
/// `BYROREDUX_FIXED_DT` freezes animation and both the jitter sequence and the
/// camera path are frame-indexed. The bounds below therefore sit ~0.01 SSIM
/// and ~25% relative outliers away from the worst observation: two orders of
/// magnitude above the noise, so they fence real regressions rather than
/// flapping, while still failing on a 1-point SSIM drop or a doubling of the
/// bad-pixel rate.
///
/// These are a regression fence, not a verdict on what looks acceptable —
/// that judgement belongs to phase 7 and is made by looking at frames.
///
/// Thresholds are hardware- and driver-specific by nature. Another GPU may
/// need its own baseline (the plan's risk register anticipates this); when
/// that happens, re-measure rather than loosening these.
struct Thresholds {
    /// Minimum mean SSIM against the native TAA reference.
    min_ssim: f64,
    /// Maximum fraction of the frame allowed to differ badly.
    max_outlier_pct: f64,
}

fn thresholds(preset: &str) -> Thresholds {
    match preset {
        // 1.0× — no upscaling at all, so this bounds the cost of swapping one
        // temporal resolve for another. A drop here would mean FSR disagrees
        // with TAA about the *image*, not about missing samples, and the
        // observed 0.99 says the two resolves substantially agree.
        "native-aa" => Thresholds {
            min_ssim: 0.980,
            max_outlier_pct: 1.0,
        },
        "quality" => Thresholds {
            min_ssim: 0.945,
            max_outlier_pct: 2.2,
        },
        "balanced" => Thresholds {
            min_ssim: 0.935,
            max_outlier_pct: 4.0,
        },
        "performance" => Thresholds {
            min_ssim: 0.910,
            max_outlier_pct: 6.5,
        },
        other => panic!("no thresholds defined for preset '{other}'"),
    }
}

#[test]
#[ignore = "requires a Vulkan device and a release build; opt-in via --ignored"]
fn fsr_presets_track_the_native_reference_on_every_camera_path() {
    let workdir = std::env::temp_dir().join("byroredux_upscaler_quality");
    std::fs::create_dir_all(&workdir).expect("create work dir");

    let mut rows: Vec<(String, String, ImageMetrics)> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    for path in PATHS {
        let reference_png = workdir.join(format!("{path}_reference_taa.png"));
        capture(&reference_png, path, "taa", None);
        let reference = load(&reference_png);

        for preset in PRESETS {
            let candidate_png = workdir.join(format!("{path}_fsr_{preset}.png"));
            capture(&candidate_png, path, "fsr3", Some(preset));
            let candidate = load(&candidate_png);

            let metrics = compare(&reference, &candidate).unwrap_or_else(|error| {
                panic!("{path}/{preset}: {error}");
            });
            let limits = thresholds(preset);
            if metrics.ssim < limits.min_ssim {
                failures.push(format!(
                    "{path}/{preset}: ssim {:.4} < {:.4}",
                    metrics.ssim, limits.min_ssim
                ));
            }
            if metrics.outlier_pct > limits.max_outlier_pct {
                failures.push(format!(
                    "{path}/{preset}: outliers {:.3}% > {:.3}%",
                    metrics.outlier_pct, limits.max_outlier_pct
                ));
            }
            rows.push((path.to_string(), preset.to_string(), metrics));
        }
    }

    println!("\nFSR quality matrix — reference: TAA at native resolution, {FRAMES} frames/capture");
    println!("{:<9} {:<13} {}", "path", "preset", "metrics");
    for (path, preset, metrics) in &rows {
        println!("{path:<9} {preset:<13} {metrics}");
    }
    println!("captures retained in {}", workdir.display());

    assert!(
        failures.is_empty(),
        "upscaler quality regressions:\n  {}\n\nfull matrix printed above; \
         captures retained in {} for side-by-side inspection",
        failures.join("\n  "),
        workdir.display()
    );
}

/// Run the engine once and capture its final frame.
///
/// `BYROREDUX_FIXED_DT=0` freezes animation so the only thing varying between
/// two captures of the same frame index is the upscaler under test — the
/// camera path is already frame-indexed rather than time-driven.
fn capture(out: &Path, camera_path: &str, upscaler: &str, quality: Option<&str>) {
    if out.exists() {
        let _ = std::fs::remove_file(out);
    }
    let frames = FRAMES.to_string();
    let out_s = out
        .to_str()
        .unwrap_or_else(|| panic!("non-UTF-8 path: {out:?}"));

    let mut args: Vec<String> = [
        "run",
        "--release",
        "-p",
        "byroredux",
        "--bin",
        "byroredux",
        "--",
        "--cornell",
        "--bench-frames",
        &frames,
        "--bench-camera",
        camera_path,
        "--upscaler",
        upscaler,
        "--screenshot",
        out_s,
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    if let Some(quality) = quality {
        args.push("--fsr-quality".to_owned());
        args.push(quality.to_owned());
    }

    let status = Command::new(env!("CARGO"))
        .env("BYROREDUX_FIXED_DT", "0")
        .env("RUST_LOG", "warn")
        .args(&args)
        .status()
        .expect("spawning cargo run failed");

    let metadata = std::fs::metadata(out).unwrap_or_else(|_| {
        panic!(
            "{camera_path}/{upscaler}{}: engine exit {status:?} and no screenshot at {} \
             — rerun with RUST_LOG=info for engine logs",
            quality.map(|q| format!("/{q}")).unwrap_or_default(),
            out.display()
        )
    });
    assert!(
        metadata.len() > 1024,
        "screenshot at {} is too small to be valid ({} bytes)",
        out.display(),
        metadata.len()
    );
}

fn load(path: &PathBuf) -> image::RgbImage {
    image::open(path)
        .unwrap_or_else(|error| panic!("decode {}: {error}", path.display()))
        .to_rgb8()
}
