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
//! `--bench-mode renderer-static` fixes delta-time at zero and holds the
//! authored camera, so animation, camera spin, and the spinning-cube
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

/// The engine flags the baseline is captured with, and the single source of
/// truth for both the capture and the staleness check below.
///
/// `--upscaler taa` is load-bearing, not cosmetic (#3849). `parse_args`
/// defaults `--upscaler` to `fsr3`, so omitting it rendered the golden through
/// the vendored FidelityFX chain at a reduced internal resolution — an FFI
/// upscaler whose output can shift with driver version, which is structurally
/// unsuitable as a pixel reference. Native TAA is deterministic for a fixed
/// frame count.
const CAPTURE_ARGS: &[&str] = &["--bench-mode", "renderer-static", "--upscaler", "taa"];

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

    let manifest = capture_manifest_path(&baseline);

    if std::env::var("BYROREDUX_REGEN_GOLDEN").is_ok() {
        if let Some(parent) = baseline.parent() {
            std::fs::create_dir_all(parent).expect("create golden dir");
        }
        std::fs::copy(&actual, &baseline).expect("copy actual to baseline");
        // Record what it was captured with, in the same breath. A baseline
        // whose invocation is not written down is the state #3849 describes:
        // unusable, and not visibly so.
        std::fs::write(&manifest, capture_manifest_body()).expect("write capture manifest");
        eprintln!(
            "regenerated baseline: {} ({} bytes)\n  capture manifest: {}",
            baseline.display(),
            actual_bytes.len(),
            manifest.display()
        );
        return;
    }

    assert_baseline_was_captured_with_the_current_invocation(&manifest);

    let baseline_bytes = std::fs::read(&baseline).unwrap_or_else(|_| {
        panic!(
            "baseline missing at {} — capture one with:\n  \
             BYROREDUX_REGEN_GOLDEN=1 cargo test --release -p byroredux -- --ignored cube_demo_golden_frame",
            baseline.display()
        )
    });

    compare_or_fail(&baseline_bytes, &actual_bytes, &baseline, &actual);
}

/// Sidecar recording the invocation a baseline PNG was captured with.
fn capture_manifest_path(baseline: &Path) -> PathBuf {
    baseline.with_extension("capture")
}

/// The manifest body: one flag per line, plus the frame count. `#` lines are
/// provenance prose and are ignored by the comparison, so a hand-written
/// manifest can say where its PNG came from.
fn capture_manifest_body() -> String {
    let mut body = String::from(
        "# Invocation the sibling golden PNG was captured with (#3849).\n\
         # Regenerate both together: BYROREDUX_REGEN_GOLDEN=1 cargo test --release \\\n\
         #   -p byroredux -- --ignored cube_demo_golden_frame\n",
    );
    body.push_str(&format!("frames={FRAMES}\n"));
    for arg in CAPTURE_ARGS {
        body.push_str(arg);
        body.push('\n');
    }
    body
}

/// Significant lines of a manifest: comments and blanks dropped.
fn manifest_significant(body: &str) -> Vec<&str> {
    body.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// Fail with *the actual reason* when the stored PNG predates the invocation
/// the test now runs.
///
/// #3849 — the baseline was last regenerated 2026-06-04, before
/// `--bench-mode renderer-static` existed (2026-08-11) and before FSR3 became
/// the default upscaler. Comparing a native-TAA frame from one timing regime
/// against an FSR3-reconstructed frame from another blows past all three
/// tolerances, so the test failed — reading as "the renderer regressed" when
/// it is 100 % baseline staleness. Worse, the documented recovery
/// (`BYROREDUX_REGEN_GOLDEN=1`) *rewrites* the baseline, so the natural
/// response to that misleading red destroys whatever signal remained.
///
/// Checking the recorded invocation first turns that into an accurate,
/// actionable failure. It also makes the next flag addition visibly
/// baseline-invalidating: add a flag to `CAPTURE_ARGS` without regenerating
/// and this fires immediately, naming the drift.
fn assert_baseline_was_captured_with_the_current_invocation(manifest: &Path) {
    let expected = capture_manifest_body();
    let recorded = std::fs::read_to_string(manifest).unwrap_or_else(|_| {
        panic!(
            "no capture manifest at {} — the baseline PNG beside it was captured with an \n\
             unrecorded invocation and cannot be trusted as a pixel reference (#3849).\n\
             Regenerate both:\n  \
             BYROREDUX_REGEN_GOLDEN=1 cargo test --release -p byroredux -- --ignored cube_demo_golden_frame",
            manifest.display()
        )
    });
    assert_eq!(
        manifest_significant(&recorded),
        manifest_significant(&expected),
        "\nSTALE BASELINE, not a renderer regression (#3849).\n\
         The golden PNG was captured with a different engine invocation than the one \n\
         this test now runs, so any pixel diff below would be meaningless.\n  \
         captured with: {:?}\n  \
         running now:   {:?}\n\
         Review the change, then regenerate:\n  \
         BYROREDUX_REGEN_GOLDEN=1 cargo test --release -p byroredux -- --ignored cube_demo_golden_frame\n",
        manifest_significant(&recorded),
        manifest_significant(&expected),
    );
}

/// Resolve a path relative to the test crate's `CARGO_MANIFEST_DIR`.
/// The test runner sets cwd to the manifest dir already, but resolving
/// explicitly avoids surprises if that ever changes.
fn manifest_relative(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// Invoke `cargo run --release -p byroredux -- --bench-frames N --screenshot OUT`
/// in `renderer-static` mode. Asserts the screenshot file was
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
        // Disable noisy logging — golden test only cares about the
        // rendered frame, not stdout / engine traces.
        .env("RUST_LOG", "warn")
        .args(
            [
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
            ]
            .into_iter()
            .chain(CAPTURE_ARGS.iter().copied()),
        )
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

// ── Staleness-gate tests (#3849) ────────────────────────────────────────
//
// `cube_demo_golden_frame` is `#[ignore]`d behind a Vulkan device, which is
// exactly how its baseline rotted unnoticed for three months. The gate that
// now guards it must not inherit that blind spot, so these run in the default
// lane: they need no GPU, only the committed manifest.

/// Provenance prose must not participate in the comparison, or the manifest
/// cannot explain itself.
#[test]
fn manifest_comparison_ignores_comments_and_blank_lines() {
    let body = "# a comment\n\n  frames=60\n\n# another\n--upscaler\n";
    assert_eq!(
        manifest_significant(body),
        vec!["frames=60", "--upscaler"],
        "only significant lines take part in the staleness check"
    );
}

/// The generated manifest names every flag the capture actually passes. If a
/// flag is added to `CAPTURE_ARGS` but the manifest stops reflecting it, the
/// gate silently stops detecting drift — the failure mode it exists to close.
#[test]
fn generated_manifest_names_the_whole_invocation() {
    let body = capture_manifest_body();
    let lines = manifest_significant(&body);
    assert!(
        lines.contains(&format!("frames={FRAMES}").as_str()),
        "frame count missing from {lines:?}"
    );
    for arg in CAPTURE_ARGS {
        assert!(lines.contains(arg), "{arg} missing from {lines:?}");
    }
    assert!(
        CAPTURE_ARGS.contains(&"--upscaler") && CAPTURE_ARGS.contains(&"taa"),
        "the golden must render through native TAA, not the default FSR3 \
         chain — an FFI upscaler's output can move with the driver, which is \
         not a pixel reference (#3849): {CAPTURE_ARGS:?}"
    );
}

/// Drift in either direction is caught: a manifest that predates a flag, and
/// one that carries a flag the harness no longer passes.
#[test]
fn manifest_comparison_detects_drift_in_both_directions() {
    let generated = capture_manifest_body();
    let current = manifest_significant(&generated);

    let predates_a_flag = "frames=60\n--bench-mode\nrenderer-static\n";
    assert_ne!(
        manifest_significant(predates_a_flag),
        current,
        "a manifest missing --upscaler taa must read as stale"
    );

    let carries_a_removed_flag = format!("{}--no-such-flag\n", capture_manifest_body());
    assert_ne!(
        manifest_significant(&carries_a_removed_flag),
        current,
        "a manifest naming a flag the harness no longer passes must read as stale"
    );
}

/// The committed baseline IS stale, and is committed saying so (#3849): its
/// PNG was captured by `4376f7a6` on 2026-06-04, before `--bench-mode
/// renderer-static` (2026-08-11) and before FSR3 became the `--upscaler`
/// default. Regenerating needs a Vulkan device and overwrites the committed
/// PNG, so it is a human's call at the machine.
///
/// This test pins that known state so the situation stays visible in the
/// default lane rather than only to whoever runs `--ignored` with a GPU.
/// **Whoever regenerates the baseline should flip this to `assert_eq!`** —
/// the failure message tells them so.
#[test]
fn committed_baseline_is_still_the_known_stale_one() {
    let manifest = capture_manifest_path(&manifest_relative(&format!(
        "{GOLDEN_DIR}/cube_demo_60f.png"
    )));
    let recorded = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("capture manifest missing at {}: {e}", manifest.display()));
    assert_ne!(
        manifest_significant(&recorded),
        manifest_significant(&capture_manifest_body()),
        "The committed baseline's capture manifest now MATCHES the current \
         invocation — so it was regenerated. Good: flip this assert_ne! to an \
         assert_eq! and drop the #3849 staleness note from \
         tests/golden/cube_demo_60f.capture."
    );
}
