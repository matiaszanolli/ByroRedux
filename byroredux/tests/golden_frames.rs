//! Golden-frame regression tests.
//!
//! Boots the engine binary with `--bench-frames N --screenshot path`,
//! then per-pixel compares the captured PNG against a baseline checked
//! into `tests/golden/`. The aim is to catch "Phase X made things
//! worse" regressions automatically — exactly the failure mode the
//! Phase 2c volumetric work hit on real content, where the regression
//! was only spotted on a manually-shared screenshot.
//!
//! ## Determinism
//!
//! `BYROREDUX_FIXED_DT=0` (set per-test) overrides the wall-clock
//! delta-time so animation, camera spin, and the spinning-cube
//! rotation stop advancing. TAA jitter still varies per-frame
//! (Halton(2,3) is frame-counter driven, not dt-driven) so the
//! denoiser still converges over the bench window — but at frame N
//! the resulting jitter offset is reproducible, which is what we
//! want.
//!
//! ## Tolerance
//!
//! Per-channel diff up to `PIXEL_TOLERANCE`/255 is ignored to absorb
//! float-precision noise. Test fails if either:
//!   - `> MAX_DIFF_PCT` of pixels differ above tolerance, OR
//!   - any single pixel has a per-channel delta `> MAX_CHANNEL_DELTA`/255.
//!
//! Both thresholds were picked to comfortably PASS bit-for-bit
//! reruns and FAIL the kind of gross visual delta a Phase-2c-style
//! shader regression produces (entire scene tinted, or large dark
//! patches, or missing geometry). Tune as the test corpus grows.
//!
//! ## Running
//!
//! ```bash
//! # Run the goldens (requires Vulkan device + release build).
//! cargo test --release -p byroredux -- --ignored golden
//!
//! # Regenerate a baseline after an INTENTIONAL visual change.
//! BYROREDUX_REGEN_GOLDEN=1 cargo test --release -p byroredux -- --ignored golden
//! ```
//!
//! When the test fails, the actual frame is saved next to the
//! baseline as `<baseline>.actual.png` so you can do a side-by-side
//! diff before deciding whether to fix the regression or regenerate
//! the baseline.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where baseline PNGs live (relative to the test crate's manifest).
const GOLDEN_DIR: &str = "tests/golden";

/// Engine bench length. 60 frames is enough for SVGF + TAA history
/// to converge from cold start while keeping the test fast.
const FRAMES: u32 = 60;

/// Per-channel diff at or below this is treated as noise.
const PIXEL_TOLERANCE: u8 = 8;
/// Test fails if more than this percent of pixels differ above tolerance.
const MAX_DIFF_PCT: f32 = 1.0;
/// Test fails if ANY single pixel's channel delta exceeds this — guards
/// against pathological local changes (e.g., a small but very wrong region).
const MAX_CHANNEL_DELTA: u8 = 32;

#[test]
#[ignore = "requires Vulkan device + release build; opt-in via --ignored"]
fn cube_demo_golden_frame() {
    let baseline = manifest_relative(&format!("{GOLDEN_DIR}/cube_demo_60f.png"));
    let actual = std::env::temp_dir().join("byroredux_golden_cube_demo.png");
    if actual.exists() {
        let _ = std::fs::remove_file(&actual);
    }

    run_engine_screenshot(&actual, FRAMES);

    let actual_bytes = std::fs::read(&actual)
        .unwrap_or_else(|e| panic!("screenshot file missing at {}: {e}", actual.display()));
    assert!(
        actual_bytes.len() > 1024,
        "screenshot too small to be a real PNG ({} bytes)",
        actual_bytes.len()
    );

    if std::env::var("BYROREDUX_REGEN_GOLDEN").is_ok() {
        if let Some(parent) = baseline.parent() {
            std::fs::create_dir_all(parent).expect("create golden dir");
        }
        std::fs::copy(&actual, &baseline).expect("copy actual to baseline");
        eprintln!(
            "regenerated baseline: {} ({} bytes)",
            baseline.display(),
            actual_bytes.len()
        );
        return;
    }

    let baseline_bytes = std::fs::read(&baseline).unwrap_or_else(|_| {
        panic!(
            "baseline missing at {} — capture one with:\n  \
             BYROREDUX_REGEN_GOLDEN=1 cargo test --release -p byroredux -- --ignored cube_demo_golden_frame",
            baseline.display()
        )
    });

    compare_or_fail(&baseline_bytes, &actual_bytes, &baseline, &actual);
}

/// Resolve a path relative to the test crate's `CARGO_MANIFEST_DIR`.
/// The test runner sets cwd to the manifest dir already, but resolving
/// explicitly avoids surprises if that ever changes.
fn manifest_relative(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// Invoke `cargo run --release -p byroredux -- --bench-frames N --screenshot OUT`
/// with the determinism env var set. Asserts the screenshot file was
/// captured — does NOT assert clean exit, because the engine currently
/// crashes on shutdown after a successful bench (SIGSEGV in some Vulkan
/// teardown path) AFTER the PNG has already been written. The test
/// cares about the rendered frame, not the shutdown cleanliness; the
/// shutdown crash is filed separately. If/when shutdown is fixed, this
/// can be tightened to assert `status.success()`.
fn run_engine_screenshot(out: &Path, frames: u32) {
    let frames_s = frames.to_string();
    let out_s = out
        .to_str()
        .unwrap_or_else(|| panic!("non-UTF-8 path: {out:?}"));

    let status = Command::new(env!("CARGO"))
        .env("BYROREDUX_FIXED_DT", "0")
        // Disable noisy logging — golden test only cares about the
        // rendered frame, not stdout / engine traces.
        .env("RUST_LOG", "warn")
        .args([
            "run",
            "--release",
            "-p",
            "byroredux",
            "--bin",
            "byroredux",
            "--",
            "--bench-frames",
            &frames_s,
            "--screenshot",
            out_s,
        ])
        .status()
        .expect("spawning cargo run failed");

    // The screenshot is written BEFORE the shutdown sequence that
    // currently crashes, so we trust file presence + size as the
    // success signal. If the file is absent the engine never reached
    // the screenshot stage — different failure mode, fail loud.
    let metadata = std::fs::metadata(out).unwrap_or_else(|_| {
        panic!(
            "engine exit {status:?} and screenshot was NOT written at {} \
             — engine likely crashed before reaching the screenshot stage; \
             rerun with RUST_LOG=info for engine logs",
            out.display()
        )
    });
    assert!(
        metadata.len() > 1024,
        "screenshot at {} is too small to be valid ({} bytes); engine exit {status:?}",
        out.display(),
        metadata.len()
    );
}

/// Per-pixel compare with tolerance. Saves `<baseline>.actual.png`
/// next to the baseline on failure so the caller can eyeball the diff.
fn compare_or_fail(
    baseline_bytes: &[u8],
    actual_bytes: &[u8],
    baseline_path: &Path,
    actual_path: &Path,
) {
    let baseline_img = image::load_from_memory(baseline_bytes)
        .expect("baseline PNG decode failed")
        .to_rgb8();
    let actual_img = image::load_from_memory(actual_bytes)
        .expect("actual PNG decode failed")
        .to_rgb8();

    if baseline_img.dimensions() != actual_img.dimensions() {
        save_actual_next_to_baseline(actual_path, baseline_path);
        panic!(
            "dimensions mismatch: baseline {:?} vs actual {:?} — saved actual next to baseline for inspection",
            baseline_img.dimensions(),
            actual_img.dimensions()
        );
    }

    let total = baseline_img.width() * baseline_img.height();
    let mut max_channel_delta: u8 = 0;
    let mut diff_pixels: u32 = 0;

    for (b, a) in baseline_img.pixels().zip(actual_img.pixels()) {
        let mut over_tolerance = false;
        for c in 0..3 {
            let d = (b[c] as i16 - a[c] as i16).unsigned_abs() as u8;
            if d > max_channel_delta {
                max_channel_delta = d;
            }
            if d > PIXEL_TOLERANCE {
                over_tolerance = true;
            }
        }
        if over_tolerance {
            diff_pixels += 1;
        }
    }

    let diff_pct = (diff_pixels as f32 / total as f32) * 100.0;
    if diff_pct > MAX_DIFF_PCT || max_channel_delta > MAX_CHANNEL_DELTA {
        let saved = save_actual_next_to_baseline(actual_path, baseline_path);
        panic!(
            "golden mismatch:\n  \
             diff_pixels = {diff_pixels} / {total} ({diff_pct:.2}%)\n  \
             max_channel_delta = {max_channel_delta}\n  \
             thresholds: ≤{MAX_DIFF_PCT}% pixels above ±{PIXEL_TOLERANCE}/255, max delta ≤{MAX_CHANNEL_DELTA}/255\n  \
             baseline: {}\n  \
             actual saved at: {}\n  \
             to regenerate baseline (only if the change is intentional):\n    \
             BYROREDUX_REGEN_GOLDEN=1 cargo test --release -p byroredux -- --ignored cube_demo_golden_frame",
            baseline_path.display(),
            saved.display()
        );
    }
}

/// Copy the actual PNG next to the baseline as `<baseline>.actual.png`
/// for human review. Returns the saved path (best-effort: silent if the
/// copy fails — we still want to panic with the diff message).
fn save_actual_next_to_baseline(actual: &Path, baseline: &Path) -> PathBuf {
    let mut saved = baseline.to_path_buf();
    let stem = saved
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("golden")
        .to_string();
    saved.set_file_name(format!("{stem}.actual.png"));
    let _ = std::fs::copy(actual, &saved);
    saved
}
