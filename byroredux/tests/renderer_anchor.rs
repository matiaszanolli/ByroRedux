//! Deterministic reference-vs-candidate renderer gate.
//!
//! This is deliberately an ignored integration test: it launches two explicit
//! engine binaries on the same machine, captures the same five deterministic
//! camera paths, rejects scene-state incompatibility before looking at pixels,
//! and retains every artifact needed to diagnose a failed comparison.
//!
//! Use `scripts/check-render-anchor.sh`; invoking this test directly requires
//! the `BYROREDUX_ANCHOR_*` environment contract assembled by that wrapper.

mod image_metrics;

use image::{Rgb, RgbImage};
use image_metrics::{compare_linear, compare_linear_low_pass, LinearImageMetrics};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_PATHS: [&str; 5] = ["static", "pan", "orbit", "dolly", "cut"];

#[derive(Debug, Clone, PartialEq, Serialize)]
struct BenchFingerprint {
    mode: String,
    camera: String,
    frames: u32,
    sim_time_s: String,
    entities: u64,
    meshes: u64,
    textures: u64,
    draws: String,
    lights: u64,
    tlas: u64,
    state_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
struct BenchPerformance {
    frame_p50_ms: f64,
    frame_p95_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
struct CaptureManifest {
    schema: u32,
    role: String,
    binary_path: PathBuf,
    binary_sha256: String,
    scene_args: Vec<String>,
    selected_gpu: String,
    upscaler_summary: String,
    fingerprint: BenchFingerprint,
    performance: BenchPerformance,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct Thresholds {
    min_ssim: f64,
    max_abs_delta: f64,
    max_p99_abs_delta: f64,
    max_outlier_pct: f64,
    max_mean_abs_delta: f64,
    outlier_abs_delta: f64,
    max_frame_p50_ratio: f64,
    max_frame_p95_ratio: f64,
    frame_time_allowance_ms: f64,
}

impl Thresholds {
    fn from_env() -> Result<Self, String> {
        Ok(Self {
            min_ssim: env_f64("BYROREDUX_ANCHOR_MIN_SSIM", 0.999)?,
            // Same-binary captures can contain a handful of high-delta RT
            // samples even after convergence (0.307 observed at 10 frames),
            // while p99/outlier/mean remain near zero. Keep max as a local
            // corruption tripwire without letting one stochastic ray veto an
            // otherwise identical frame; the injected 64x64 fault still
            // reaches ~1.0 and is independently caught by every area metric.
            max_abs_delta: env_f64("BYROREDUX_ANCHOR_MAX_ABS_DELTA", 0.35)?,
            max_p99_abs_delta: env_f64("BYROREDUX_ANCHOR_MAX_P99_DELTA", 0.01)?,
            max_outlier_pct: env_f64("BYROREDUX_ANCHOR_MAX_OUTLIER_PCT", 0.10)?,
            max_mean_abs_delta: env_f64("BYROREDUX_ANCHOR_MAX_MEAN_DELTA", 0.001)?,
            outlier_abs_delta: env_f64("BYROREDUX_ANCHOR_OUTLIER_DELTA", 0.03)?,
            // Three complete 60-frame same-binary matrices measured maxima of
            // 1.071×/1.074× and +0.40/+0.50 ms at p50/p95. The envelope below
            // leaves a small rounding/run-load margin while still fencing a
            // sustained double-digit regression.
            max_frame_p50_ratio: env_f64("BYROREDUX_ANCHOR_MAX_P50_RATIO", 1.10)?,
            max_frame_p95_ratio: env_f64("BYROREDUX_ANCHOR_MAX_P95_RATIO", 1.10)?,
            frame_time_allowance_ms: env_f64("BYROREDUX_ANCHOR_FRAME_ALLOWANCE_MS", 0.10)?,
        })
    }
}

#[derive(Debug)]
struct Config {
    reference_bin: PathBuf,
    candidate_bin: PathBuf,
    output_dir: PathBuf,
    workdir: PathBuf,
    frames: u32,
    paths: Vec<String>,
    scene_args: Vec<String>,
    perturbation: Option<String>,
    use_xvfb: bool,
    thresholds: Thresholds,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        let reference_bin = required_path("BYROREDUX_ANCHOR_REFERENCE_BIN")?;
        let candidate_bin = required_path("BYROREDUX_ANCHOR_CANDIDATE_BIN")?;
        let output_dir = required_path("BYROREDUX_ANCHOR_OUT")?;
        let workdir = env::var_os("BYROREDUX_ANCHOR_WORKDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .into()
            });
        let frames = env::var("BYROREDUX_ANCHOR_FRAMES")
            .unwrap_or_else(|_| "60".to_owned())
            .parse::<u32>()
            .map_err(|error| {
                format!("BYROREDUX_ANCHOR_FRAMES must be a positive integer: {error}")
            })?;
        if frames == 0 {
            return Err("BYROREDUX_ANCHOR_FRAMES must be greater than zero".to_owned());
        }

        let paths = env::var("BYROREDUX_ANCHOR_PATHS")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|path| !path.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                DEFAULT_PATHS
                    .iter()
                    .map(|path| (*path).to_owned())
                    .collect()
            });
        if paths.is_empty() {
            return Err("BYROREDUX_ANCHOR_PATHS selected no paths".to_owned());
        }
        for path in &paths {
            if !DEFAULT_PATHS.contains(&path.as_str()) {
                return Err(format!(
                    "unknown anchor camera path '{path}'; expected one of {}",
                    DEFAULT_PATHS.join(",")
                ));
            }
        }

        let scene_args = match env::var("BYROREDUX_ANCHOR_SCENE_ARGS_JSON") {
            Ok(json) => serde_json::from_str::<Vec<String>>(&json)
                .map_err(|error| format!("parse BYROREDUX_ANCHOR_SCENE_ARGS_JSON: {error}"))?,
            Err(_) => vec!["--cornell".to_owned()],
        };
        if scene_args.is_empty() {
            return Err("scene argument list must not be empty".to_owned());
        }

        Ok(Self {
            reference_bin,
            candidate_bin,
            output_dir,
            workdir,
            frames,
            paths,
            scene_args,
            perturbation: env::var("BYROREDUX_ANCHOR_TEST_PERTURB").ok(),
            use_xvfb: env_bool("BYROREDUX_ANCHOR_XVFB", false)?,
            thresholds: Thresholds::from_env()?,
        })
    }
}

#[derive(Debug, Serialize)]
struct PathResult {
    path: String,
    passed: bool,
    correctness_passed: bool,
    performance_passed: bool,
    reference_manifest: PathBuf,
    candidate_manifest: PathBuf,
    reference_png: PathBuf,
    candidate_png: PathBuf,
    compared_candidate_png: PathBuf,
    heatmap_png: PathBuf,
    metrics_json: PathBuf,
    comparison_filter: &'static str,
    metrics: MetricValues,
    raw_metrics: MetricValues,
    reference_performance: BenchPerformance,
    candidate_performance: BenchPerformance,
    correctness_failures: Vec<String>,
    performance_failures: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct MetricValues {
    ssim: f64,
    max_abs_delta: f64,
    p95_abs_delta: f64,
    p99_abs_delta: f64,
    outlier_pct: f64,
    mean_abs_delta: f64,
}

impl From<LinearImageMetrics> for MetricValues {
    fn from(metrics: LinearImageMetrics) -> Self {
        Self {
            ssim: metrics.ssim,
            max_abs_delta: metrics.max_abs_delta,
            p95_abs_delta: metrics.p95_abs_delta,
            p99_abs_delta: metrics.p99_abs_delta,
            outlier_pct: metrics.outlier_pct,
            mean_abs_delta: metrics.mean_abs_delta,
        }
    }
}

#[derive(Debug, Serialize)]
struct Summary {
    schema: u32,
    passed: bool,
    correctness_passed: bool,
    performance_passed: bool,
    reference_binary: PathBuf,
    candidate_binary: PathBuf,
    scene_args: Vec<String>,
    frames: u32,
    paths: Vec<String>,
    thresholds: Thresholds,
    perturbation: Option<String>,
    failures: Vec<String>,
    correctness_failures: Vec<String>,
    performance_failures: Vec<String>,
    results: Vec<PathResult>,
}

#[test]
#[ignore = "requires two explicit engine binaries and a Vulkan display; use check-render-anchor.sh"]
fn reference_binary_matches_candidate_on_every_camera_path() {
    if let Err(error) = run_from_env() {
        panic!("renderer anchor gate failed: {error}");
    }
}

fn run_from_env() -> Result<(), String> {
    let config = Config::from_env()?;
    if !config.use_xvfb {
        verify_display()?;
    }
    fs::create_dir_all(&config.output_dir).map_err(|error| {
        format!(
            "create anchor artifact directory {}: {error}",
            config.output_dir.display()
        )
    })?;

    let reference_sha = sha256_file(&config.reference_bin)?;
    let candidate_sha = sha256_file(&config.candidate_bin)?;
    let mut results = Vec::with_capacity(config.paths.len());
    let mut all_correctness_failures = Vec::new();
    let mut all_performance_failures = Vec::new();

    for path in &config.paths {
        eprintln!("anchor: capture {path} reference");
        let reference = capture(
            &config,
            "reference",
            &config.reference_bin,
            &reference_sha,
            path,
        )?;
        eprintln!("anchor: capture {path} candidate");
        let candidate = capture(
            &config,
            "candidate",
            &config.candidate_bin,
            &candidate_sha,
            path,
        )?;

        let compatibility_failures = manifest_mismatches(&reference.manifest, &candidate.manifest);
        if !compatibility_failures.is_empty() {
            let failures = compatibility_failures
                .into_iter()
                .map(|failure| format!("{path}: manifest mismatch: {failure}"))
                .collect::<Vec<_>>();
            all_correctness_failures.extend(failures);
            continue;
        }

        let reference_image = load_rgb(&reference.png)?;
        let mut candidate_image = load_rgb(&candidate.png)?;
        let compared_candidate_png = if let Some(perturbation) = config.perturbation.as_deref() {
            apply_perturbation(&mut candidate_image, perturbation)?;
            let path = config
                .output_dir
                .join("candidate")
                .join(format!("{path}-perturbed.png"));
            candidate_image
                .save(&path)
                .map_err(|error| format!("save perturbed candidate {}: {error}", path.display()))?;
            path
        } else {
            candidate.png.clone()
        };

        let raw_metrics = compare_linear(
            &reference_image,
            &candidate_image,
            config.thresholds.outlier_abs_delta,
        )?;
        let metrics = compare_linear_low_pass(
            &reference_image,
            &candidate_image,
            config.thresholds.outlier_abs_delta,
        )?;
        let heatmap_png = config
            .output_dir
            .join("diff")
            .join(format!("{path}-heatmap.png"));
        save_heatmap(&reference_image, &candidate_image, &heatmap_png)?;

        let verdict = judge(
            metrics,
            reference.manifest.performance,
            candidate.manifest.performance,
            config.thresholds,
        );
        let correctness_failures = verdict
            .correctness_failures
            .into_iter()
            .map(|failure| format!("{path}: {failure}"))
            .collect::<Vec<_>>();
        let performance_failures = verdict
            .performance_failures
            .into_iter()
            .map(|failure| format!("{path}: {failure}"))
            .collect::<Vec<_>>();
        all_correctness_failures.extend(correctness_failures.iter().cloned());
        all_performance_failures.extend(performance_failures.iter().cloned());

        let metrics_json = config
            .output_dir
            .join("diff")
            .join(format!("{path}-metrics.json"));
        let result = PathResult {
            path: path.clone(),
            passed: correctness_failures.is_empty() && performance_failures.is_empty(),
            correctness_passed: correctness_failures.is_empty(),
            performance_passed: performance_failures.is_empty(),
            reference_manifest: reference.manifest_path,
            candidate_manifest: candidate.manifest_path,
            reference_png: reference.png,
            candidate_png: candidate.png,
            compared_candidate_png,
            heatmap_png,
            metrics_json: metrics_json.clone(),
            comparison_filter: "linear RGB separable 5x5 binomial low-pass",
            metrics: metrics.into(),
            raw_metrics: raw_metrics.into(),
            reference_performance: reference.manifest.performance,
            candidate_performance: candidate.manifest.performance,
            correctness_failures,
            performance_failures,
        };
        write_json(&metrics_json, &result)?;
        eprintln!("anchor: {path}: gated {metrics}; raw {raw_metrics}");
        results.push(result);
    }

    let summary_path = config.output_dir.join("summary.json");
    let all_paths_compared = results.len() == config.paths.len();
    let correctness_passed = all_correctness_failures.is_empty() && all_paths_compared;
    let performance_passed = all_performance_failures.is_empty() && all_paths_compared;
    let mut all_failures = all_correctness_failures.clone();
    all_failures.extend(all_performance_failures.iter().cloned());
    let summary = Summary {
        schema: 1,
        passed: correctness_passed && performance_passed,
        correctness_passed,
        performance_passed,
        reference_binary: config.reference_bin,
        candidate_binary: config.candidate_bin,
        scene_args: config.scene_args,
        frames: config.frames,
        paths: config.paths,
        thresholds: config.thresholds,
        perturbation: config.perturbation,
        failures: all_failures,
        correctness_failures: all_correctness_failures,
        performance_failures: all_performance_failures,
        results,
    };
    write_json(&summary_path, &summary)?;

    if summary.passed {
        println!(
            "renderer anchor PASS: {} paths; artifacts in {}",
            summary.results.len(),
            config.output_dir.display()
        );
        Ok(())
    } else {
        Err(format!(
            "{} failure(s):\n  {}\nsummary: {}",
            summary.failures.len(),
            summary.failures.join("\n  "),
            summary_path.display()
        ))
    }
}

fn verify_display() -> Result<(), String> {
    let output = Command::new("xdpyinfo")
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("DYLD_LIBRARY_PATH")
        .output()
        .map_err(|error| format!("launch xdpyinfo display preflight: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "display preflight failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

struct Capture {
    manifest: CaptureManifest,
    manifest_path: PathBuf,
    png: PathBuf,
}

fn capture(
    config: &Config,
    role: &str,
    binary: &Path,
    binary_sha256: &str,
    camera_path: &str,
) -> Result<Capture, String> {
    let role_dir = config.output_dir.join(role);
    fs::create_dir_all(&role_dir)
        .map_err(|error| format!("create {}: {error}", role_dir.display()))?;
    let png = role_dir.join(format!("{camera_path}.png"));
    let log_path = role_dir.join(format!("{camera_path}.log"));
    let manifest_path = role_dir.join(format!("{camera_path}.manifest.json"));
    if png.exists() {
        fs::remove_file(&png)
            .map_err(|error| format!("remove stale {}: {error}", png.display()))?;
    }

    let mode = if camera_path == "static" {
        "renderer-static"
    } else {
        "renderer-stepped"
    };
    let mut args = config.scene_args.clone();
    args.extend([
        "--bench-frames".to_owned(),
        config.frames.to_string(),
        "--bench-mode".to_owned(),
        mode.to_owned(),
        "--bench-camera".to_owned(),
        camera_path.to_owned(),
        "--upscaler".to_owned(),
        "taa".to_owned(),
        "--screenshot".to_owned(),
        png.to_string_lossy().into_owned(),
    ]);

    let xvfb_log = role_dir.join(format!("{camera_path}.xvfb.log"));
    let mut command = if config.use_xvfb {
        let mut command = Command::new("xvfb-run");
        command.arg("-a").arg("-e").arg(&xvfb_log).arg(binary);
        command.env_remove("WAYLAND_DISPLAY");
        command.env("WINIT_UNIX_BACKEND", "x11");
        command
    } else {
        Command::new(binary)
    };
    let output = command
        .current_dir(&config.workdir)
        // Cargo injects its test/dependency search path into integration-test
        // processes. The engine is an external artifact, not a test helper;
        // letting that path leak into X11/Vulkan driver loading can make an
        // otherwise valid Xvfb display fail during winit initialization.
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("DYLD_LIBRARY_PATH")
        .env(
            "RUST_LOG",
            env::var("BYROREDUX_ANCHOR_RUST_LOG").unwrap_or_else(|_| "info".to_owned()),
        )
        .args(&args)
        .output()
        .map_err(|error| {
            format!(
                "launch {} for {role}/{camera_path}: {error}",
                binary.display()
            )
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let display = env::var("DISPLAY").unwrap_or_else(|_| "<unset>".to_owned());
    let xauthority = env::var("XAUTHORITY").unwrap_or_else(|_| "<unset>".to_owned());
    let winit_backend = env::var("WINIT_UNIX_BACKEND").unwrap_or_else(|_| "<unset>".to_owned());
    let combined = format!(
        "--- environment ---\nDISPLAY={display}\nXAUTHORITY={xauthority}\n\
         WINIT_UNIX_BACKEND={winit_backend}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    fs::write(&log_path, &combined)
        .map_err(|error| format!("write capture log {}: {error}", log_path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{role}/{camera_path}: engine exited with {}; log: {}",
            output.status,
            log_path.display()
        ));
    }
    let png_size = fs::metadata(&png)
        .map_err(|error| {
            format!(
                "{role}/{camera_path}: no screenshot at {}: {error}; log: {}",
                png.display(),
                log_path.display()
            )
        })?
        .len();
    if png_size <= 1024 {
        return Err(format!(
            "{role}/{camera_path}: screenshot {} is only {png_size} bytes",
            png.display()
        ));
    }

    let bench_line = stdout
        .lines()
        .find(|line| line.starts_with("bench: "))
        .ok_or_else(|| {
            format!(
                "{role}/{camera_path}: no bench summary in {}",
                log_path.display()
            )
        })?;
    let manifest = CaptureManifest {
        schema: 1,
        role: role.to_owned(),
        binary_path: binary.to_owned(),
        binary_sha256: binary_sha256.to_owned(),
        scene_args: config.scene_args.clone(),
        selected_gpu: value_after_marker(&combined, "Selected GPU:")?,
        upscaler_summary: value_after_marker(&combined, "Frame upscaler:")?,
        fingerprint: parse_fingerprint(bench_line)?,
        performance: parse_performance(bench_line)?,
    };
    write_json(&manifest_path, &manifest)?;
    Ok(Capture {
        manifest,
        manifest_path,
        png,
    })
}

fn manifest_mismatches(reference: &CaptureManifest, candidate: &CaptureManifest) -> Vec<String> {
    let mut mismatches = Vec::new();
    compare_field(
        &mut mismatches,
        "scene_args",
        &reference.scene_args,
        &candidate.scene_args,
    );
    compare_field(
        &mut mismatches,
        "selected_gpu",
        &reference.selected_gpu,
        &candidate.selected_gpu,
    );
    compare_field(
        &mut mismatches,
        "upscaler_summary",
        &reference.upscaler_summary,
        &candidate.upscaler_summary,
    );
    compare_field(
        &mut mismatches,
        "fingerprint",
        &reference.fingerprint,
        &candidate.fingerprint,
    );
    mismatches
}

fn compare_field<T: std::fmt::Debug + PartialEq>(
    mismatches: &mut Vec<String>,
    field: &str,
    reference: &T,
    candidate: &T,
) {
    if reference != candidate {
        mismatches.push(format!(
            "{field}: reference {reference:?}, candidate {candidate:?}"
        ));
    }
}

fn parse_fingerprint(line: &str) -> Result<BenchFingerprint, String> {
    Ok(BenchFingerprint {
        mode: bench_value(line, "mode")?.to_owned(),
        camera: bench_value(line, "camera")?.to_owned(),
        frames: parse_bench(line, "frames")?,
        sim_time_s: bench_value(line, "sim_time_s")?.to_owned(),
        entities: parse_bench(line, "entities")?,
        meshes: parse_bench(line, "meshes")?,
        textures: parse_bench(line, "textures")?,
        draws: bench_value(line, "draws")?.to_owned(),
        lights: parse_bench(line, "lights")?,
        tlas: parse_bench(line, "tlas")?,
        state_hash: bench_value(line, "state_hash")?.to_owned(),
    })
}

fn parse_performance(line: &str) -> Result<BenchPerformance, String> {
    Ok(BenchPerformance {
        frame_p50_ms: parse_bench(line, "frame_p50_ms")?,
        frame_p95_ms: parse_bench(line, "frame_p95_ms")?,
    })
}

fn bench_value<'a>(line: &'a str, key: &str) -> Result<&'a str, String> {
    let prefix = format!("{key}=");
    line.split_ascii_whitespace()
        .find_map(|token| token.strip_prefix(&prefix))
        .map(|value| value.trim_matches(|ch| ch == '[' || ch == ']'))
        .ok_or_else(|| format!("bench summary missing {key}=...: {line}"))
}

fn parse_bench<T: std::str::FromStr>(line: &str, key: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    bench_value(line, key)?
        .parse::<T>()
        .map_err(|error| format!("parse {key} in bench summary: {error}"))
}

fn value_after_marker(log: &str, marker: &str) -> Result<String, String> {
    log.lines()
        .find_map(|line| {
            line.find(marker)
                .map(|index| line[index + marker.len()..].trim().to_owned())
        })
        .ok_or_else(|| format!("capture log missing '{marker}'"))
}

struct Verdict {
    correctness_failures: Vec<String>,
    performance_failures: Vec<String>,
}

fn judge(
    metrics: LinearImageMetrics,
    reference: BenchPerformance,
    candidate: BenchPerformance,
    limits: Thresholds,
) -> Verdict {
    let mut correctness_failures = Vec::new();
    if metrics.ssim < limits.min_ssim {
        correctness_failures.push(format!(
            "linear SSIM {:.6} < {:.6}",
            metrics.ssim, limits.min_ssim
        ));
    }
    if metrics.max_abs_delta > limits.max_abs_delta {
        correctness_failures.push(format!(
            "max linear delta {:.6} > {:.6}",
            metrics.max_abs_delta, limits.max_abs_delta
        ));
    }
    if metrics.p99_abs_delta > limits.max_p99_abs_delta {
        correctness_failures.push(format!(
            "p99 linear delta {:.6} > {:.6}",
            metrics.p99_abs_delta, limits.max_p99_abs_delta
        ));
    }
    if metrics.outlier_pct > limits.max_outlier_pct {
        correctness_failures.push(format!(
            "linear outliers {:.4}% > {:.4}%",
            metrics.outlier_pct, limits.max_outlier_pct
        ));
    }
    if metrics.mean_abs_delta > limits.max_mean_abs_delta {
        correctness_failures.push(format!(
            "mean linear delta {:.6} > {:.6}",
            metrics.mean_abs_delta, limits.max_mean_abs_delta
        ));
    }
    let mut performance_failures = Vec::new();
    judge_frame_time(
        &mut performance_failures,
        "p50",
        reference.frame_p50_ms,
        candidate.frame_p50_ms,
        limits.max_frame_p50_ratio,
        limits.frame_time_allowance_ms,
    );
    judge_frame_time(
        &mut performance_failures,
        "p95",
        reference.frame_p95_ms,
        candidate.frame_p95_ms,
        limits.max_frame_p95_ratio,
        limits.frame_time_allowance_ms,
    );
    Verdict {
        correctness_failures,
        performance_failures,
    }
}

fn judge_frame_time(
    failures: &mut Vec<String>,
    label: &str,
    reference_ms: f64,
    candidate_ms: f64,
    max_ratio: f64,
    allowance_ms: f64,
) {
    let ceiling = reference_ms * max_ratio + allowance_ms;
    if candidate_ms > ceiling {
        failures.push(format!(
            "frame {label} {candidate_ms:.3} ms > {ceiling:.3} ms \
             (reference {reference_ms:.3} ms × {max_ratio:.3} + {allowance_ms:.3} ms)"
        ));
    }
}

fn apply_perturbation(image: &mut RgbImage, perturbation: &str) -> Result<(), String> {
    match perturbation {
        "magenta-block" => {
            let width = image.width().min(64);
            let height = image.height().min(64);
            let x0 = image.width().saturating_sub(width) / 2;
            let y0 = image.height().saturating_sub(height) / 2;
            for y in y0..y0 + height {
                for x in x0..x0 + width {
                    image.put_pixel(x, y, Rgb([255, 0, 255]));
                }
            }
            Ok(())
        }
        other => Err(format!(
            "unknown BYROREDUX_ANCHOR_TEST_PERTURB '{other}'; expected magenta-block"
        )),
    }
}

fn save_heatmap(reference: &RgbImage, candidate: &RgbImage, out: &Path) -> Result<(), String> {
    let parent = out
        .parent()
        .ok_or_else(|| format!("heatmap path has no parent: {}", out.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    let heatmap = RgbImage::from_fn(reference.width(), reference.height(), |x, y| {
        let reference = reference.get_pixel(x, y);
        let candidate = candidate.get_pixel(x, y);
        Rgb([
            reference[0].abs_diff(candidate[0]).saturating_mul(4),
            reference[1].abs_diff(candidate[1]).saturating_mul(4),
            reference[2].abs_diff(candidate[2]).saturating_mul(4),
        ])
    });
    heatmap
        .save(out)
        .map_err(|error| format!("save heatmap {}: {error}", out.display()))
}

fn load_rgb(path: &Path) -> Result<RgbImage, String> {
    image::open(path)
        .map_err(|error| format!("decode {}: {error}", path.display()))
        .map(|image| image.to_rgb8())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|error| format!("run sha256sum for {}: {error}", path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "sha256sum failed for {} with {}",
            path.display(),
            output.status
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("sha256sum output was not UTF-8: {error}"))?
        .split_ascii_whitespace()
        .next()
        .map(str::to_owned)
        .ok_or_else(|| format!("sha256sum returned no hash for {}", path.display()))
}

fn required_path(name: &str) -> Result<PathBuf, String> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing required environment variable {name}"))
}

fn env_f64(name: &str, default: f64) -> Result<f64, String> {
    let value = match env::var(name) {
        Ok(value) => value
            .parse::<f64>()
            .map_err(|error| format!("{name} must be a number: {error}"))?,
        Err(_) => default,
    };
    if !value.is_finite() || value < 0.0 {
        return Err(format!(
            "{name} must be finite and non-negative, got {value}"
        ));
    }
    Ok(value)
}

fn env_bool(name: &str, default: bool) -> Result<bool, String> {
    match env::var(name) {
        Ok(value) => match value.as_str() {
            "1" | "true" | "yes" => Ok(true),
            "0" | "false" | "no" => Ok(false),
            _ => Err(format!(
                "{name} must be one of 1/true/yes or 0/false/no, got '{value}'"
            )),
        },
        Err(_) => Ok(default),
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("serialize {}: {error}", path.display()))?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BENCH: &str = "bench: mode=renderer-stepped gate=fixed-dt+camera dt=0.016666668 \
        camera=orbit frames=60 wall_fps=61.0 wall_ms=16.39 frame_p50_ms=15.25 \
        frame_p95_ms=17.50 frame_max_ms=20.00 sim_time_s=1.000000 entities=8 meshes=4 \
        textures=6 draws=3/2b/1c lights=2 tlas=3 state_hash=0123456789abcdef";

    fn limits() -> Thresholds {
        Thresholds {
            min_ssim: 0.999,
            max_abs_delta: 0.08,
            max_p99_abs_delta: 0.01,
            max_outlier_pct: 0.1,
            max_mean_abs_delta: 0.001,
            outlier_abs_delta: 0.03,
            max_frame_p50_ratio: 1.2,
            max_frame_p95_ratio: 1.25,
            frame_time_allowance_ms: 0.75,
        }
    }

    #[test]
    fn bench_parser_extracts_state_and_performance_contracts() {
        let fingerprint = parse_fingerprint(BENCH).unwrap();
        assert_eq!(fingerprint.mode, "renderer-stepped");
        assert_eq!(fingerprint.camera, "orbit");
        assert_eq!(fingerprint.frames, 60);
        assert_eq!(fingerprint.draws, "3/2b/1c");
        assert_eq!(fingerprint.tlas, 3);
        assert_eq!(fingerprint.state_hash, "0123456789abcdef");

        let performance = parse_performance(BENCH).unwrap();
        assert_eq!(performance.frame_p50_ms, 15.25);
        assert_eq!(performance.frame_p95_ms, 17.50);
    }

    #[test]
    fn judge_accepts_an_identical_candidate() {
        let metrics = LinearImageMetrics {
            ssim: 1.0,
            max_abs_delta: 0.0,
            p95_abs_delta: 0.0,
            p99_abs_delta: 0.0,
            outlier_pct: 0.0,
            mean_abs_delta: 0.0,
        };
        let performance = BenchPerformance {
            frame_p50_ms: 10.0,
            frame_p95_ms: 12.0,
        };
        let verdict = judge(metrics, performance, performance, limits());
        assert!(verdict.correctness_failures.is_empty());
        assert!(verdict.performance_failures.is_empty());
    }

    #[test]
    fn judge_names_visual_and_frame_time_regressions() {
        let metrics = LinearImageMetrics {
            ssim: 0.9,
            max_abs_delta: 0.5,
            p95_abs_delta: 0.1,
            p99_abs_delta: 0.2,
            outlier_pct: 3.0,
            mean_abs_delta: 0.05,
        };
        let verdict = judge(
            metrics,
            BenchPerformance {
                frame_p50_ms: 10.0,
                frame_p95_ms: 10.0,
            },
            BenchPerformance {
                frame_p50_ms: 20.0,
                frame_p95_ms: 20.0,
            },
            limits(),
        );
        assert_eq!(verdict.correctness_failures.len(), 5);
        assert_eq!(verdict.performance_failures.len(), 2);
        assert!(verdict
            .correctness_failures
            .iter()
            .any(|failure| failure.contains("SSIM")));
        assert!(verdict
            .performance_failures
            .iter()
            .any(|failure| failure.contains("frame p95")));
    }

    #[test]
    fn controlled_perturbation_changes_only_a_center_block() {
        let mut image = RgbImage::from_pixel(128, 96, Rgb([1, 2, 3]));
        apply_perturbation(&mut image, "magenta-block").unwrap();
        assert_eq!(image.get_pixel(0, 0), &Rgb([1, 2, 3]));
        assert_eq!(image.get_pixel(64, 48), &Rgb([255, 0, 255]));
    }
}
