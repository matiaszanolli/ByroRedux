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
/// to converge from cold, short enough that a 20-run matrix stays tolerable.
const FRAMES: u32 = 60;

/// Moving camera paths exercised. A parked path is deliberately excluded:
/// full temporal convergence hides disocclusion, reprojection, and cut-reset
/// failures instead of testing them.
const PATHS: [&str; 4] = ["pan", "orbit", "dolly", "cut"];

/// FSR presets scored against the reference, in ascending upscale ratio.
const PRESETS: [&str; 4] = ["native-aa", "quality", "balanced", "performance"];

/// Per-preset acceptance thresholds.
///
/// **Measured, not chosen.** Three full matrix runs on an RTX 4070 Ti
/// (driver as of 2026-07-24) produced these worst-case values across the four
/// moving paths plus the then-present static control:
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
/// `renderer-stepped` fixes simulation at 60 Hz and the camera path is
/// frame-indexed. The bounds below therefore sit ~0.01 SSIM
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
    println!("{:<9} {:<13} metrics", "path", "preset");
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

/// Same matrix, against real game content instead of Cornell.
///
/// Opt-in through `BYROREDUX_QUALITY_GAME` / `BYROREDUX_QUALITY_CELL` (a
/// `--game` profile key and a cell editor ID), because game data cannot be
/// redistributed and is not present on every machine. Cornell is what the
/// committed thresholds fence; this exists because a Cornell box is a
/// generous scene for an upscaler — no alpha-tested foliage, no dense
/// high-frequency texture detail, no decals — and a preset that only holds up
/// there has not been shown to hold up at all.
///
/// Prints the table and asserts nothing. Thresholds calibrated on one
/// person's copy of one game would fence nobody else's checkout, and a test
/// that silently skips for most contributors is worse than an explicit
/// reporting tool. The numbers belong in the plan doc; the frames stay local.
///
/// ```bash
/// BYROREDUX_QUALITY_GAME=fo4 BYROREDUX_QUALITY_CELL=DmndDugoutInn01 \
///   cargo test --release -p byroredux --test upscaler_quality -- --ignored --nocapture game
/// ```
#[test]
#[ignore = "requires game data on disk; opt-in via --ignored + BYROREDUX_QUALITY_GAME"]
fn fsr_presets_on_game_content_report_only() {
    let (Ok(game), Ok(cell)) = (
        std::env::var("BYROREDUX_QUALITY_GAME"),
        std::env::var("BYROREDUX_QUALITY_CELL"),
    ) else {
        eprintln!(
            "skipped: set BYROREDUX_QUALITY_GAME and BYROREDUX_QUALITY_CELL              (e.g. fo4 / DmndDugoutInn01) to score real game content"
        );
        return;
    };

    let workdir = std::env::temp_dir().join("byroredux_upscaler_quality_game");
    std::fs::create_dir_all(&workdir).expect("create work dir");
    let scene = SceneArgs::Game {
        game: game.clone(),
        cell: cell.clone(),
    };

    println!(
        "
FSR quality matrix — {game}/{cell}, reference: TAA at native, {FRAMES} frames"
    );
    println!("{:<9} {:<13} metrics", "path", "preset");
    for path in PATHS {
        let reference_png = workdir.join(format!("{path}_reference_taa.png"));
        capture_scene(&reference_png, &scene, path, "taa", None);
        let reference = load(&reference_png);

        for preset in PRESETS {
            let candidate_png = workdir.join(format!("{path}_fsr_{preset}.png"));
            capture_scene(&candidate_png, &scene, path, "fsr3", Some(preset));
            let candidate = load(&candidate_png);
            match compare(&reference, &candidate) {
                Ok(metrics) => println!("{path:<9} {preset:<13} {metrics}"),
                Err(error) => println!("{path:<9} {preset:<13} FAILED: {error}"),
            }
        }
    }
    println!(
        "captures retained in {} (local artifact — do not commit)",
        workdir.display()
    );
}

/// Which scene a capture loads.
enum SceneArgs {
    /// The engine-owned Cornell box. Redistributable, so it is what the
    /// committed thresholds are calibrated against.
    Cornell,
    /// A `--game <profile> --cell <editor id>` load. Local only.
    Game { game: String, cell: String },
}

impl SceneArgs {
    fn to_args(&self) -> Vec<String> {
        match self {
            Self::Cornell => vec!["--cornell".to_owned()],
            Self::Game { game, cell } => vec![
                "--game".to_owned(),
                game.clone(),
                "--cell".to_owned(),
                cell.clone(),
            ],
        }
    }
}

/// Run the engine once and capture its final frame.
///
/// `renderer-stepped` advances animation at a fixed 60 Hz while the camera
/// path advances by frame index. Two captures therefore exercise temporal
/// reconstruction under motion without reintroducing wall-clock feedback.
fn capture(out: &Path, camera_path: &str, upscaler: &str, quality: Option<&str>) {
    capture_scene(out, &SceneArgs::Cornell, camera_path, upscaler, quality);
}

fn capture_scene(
    out: &Path,
    scene: &SceneArgs,
    camera_path: &str,
    upscaler: &str,
    quality: Option<&str>,
) {
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
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    args.extend(scene.to_args());
    args.extend(
        [
            "--bench-frames",
            &frames,
            "--bench-mode",
            "renderer-stepped",
            "--bench-camera",
            camera_path,
            "--upscaler",
            upscaler,
            "--screenshot",
            out_s,
        ]
        .iter()
        .map(|s| (*s).to_owned()),
    );
    if let Some(quality) = quality {
        args.push("--fsr-quality".to_owned());
        args.push(quality.to_owned());
    }

    let status = Command::new(env!("CARGO"))
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
