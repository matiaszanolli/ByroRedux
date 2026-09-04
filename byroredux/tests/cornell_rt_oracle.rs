//! Hardware-gated L0-L5 transport and material oracle.
//!
//! Run on an RT-capable integration worker:
//!
//! ```bash
//! cargo test --release -p byroredux --test cornell_rt_oracle -- --ignored --nocapture
//! ```
//!
//! The tests deliberately use raw renderer debug outputs rather than normal
//! presentation. L0-L2 isolate direct surface transport; L3-L4 capture the
//! HDR-linear composite term so the volumetric integral is present while
//! exposure, ACES, bloom, grading and presentation dither remain bypassed.
//! L5 uses the categorical material-lobe view for dielectric, metal, glass,
//! and normal-mapped probe geometry.

use image::RgbImage;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const FRAMES: &str = "30";
const DIRECT_DEBUG: &str = "0x4000000";
const SHADOW_VISIBILITY_DEBUG: &str = "0x84000000";
const COMPOSITE_DEBUG: &str = "0";
static CAPTURE_SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// Keeps ad-hoc test artifacts temporary while allowing the RT integration
/// runner to retain every capture and process log. The workflow sets
/// `BYROREDUX_RT_ARTIFACT_DIR`; ordinary local runs preserve the old tempdir
/// behaviour.
struct OracleArtifacts {
    path: PathBuf,
    _temporary: Option<tempfile::TempDir>,
}

impl OracleArtifacts {
    fn new(test_name: &str) -> Self {
        if let Some(root) = std::env::var_os("BYROREDUX_RT_ARTIFACT_DIR") {
            let path = PathBuf::from(root).join(test_name);
            std::fs::create_dir_all(&path).unwrap_or_else(|error| {
                panic!(
                    "create persistent Cornell artifact dir {}: {error}",
                    path.display()
                )
            });
            Self {
                path,
                _temporary: None,
            }
        } else {
            let temporary = tempfile::tempdir().expect("create Cornell oracle tempdir");
            let path = temporary.path().to_path_buf();
            Self {
                path,
                _temporary: Some(temporary),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[test]
#[ignore = "requires an RT-capable Vulkan device and a display/Xvfb"]
fn cornell_l0_l2_transport_ladder_matches_analytic_probes() {
    let workdir = OracleArtifacts::new("l0-l2-transport");

    let l0 = capture(
        workdir.path(),
        "l0",
        DIRECT_DEBUG,
        "lights_uploaded=0",
        "tlas_emitted=1",
        &[],
    );
    assert!(
        l0.pixels()
            .all(|pixel| pixel.0.iter().all(|channel| *channel <= 1)),
        "L0 must remain black: zero authored/submitted lights means zero direct transport"
    );

    let l1 = capture(
        workdir.path(),
        "l1",
        DIRECT_DEBUG,
        "lights_uploaded=1",
        "tlas_emitted=1",
        &[],
    );
    // Receiver normal is +Z and the normalized source vector is (1,1,2),
    // hence N.L = 2/sqrt(6). The raw presentation path still writes through
    // an sRGB swapchain, so compare against the analytic linear value encoded
    // with the standard sRGB transfer function.
    let expected_l1 = linear_to_srgb_u8(2.0 / 6.0_f32.sqrt());
    for (x, y) in normalized_probes(&l1, &[(0.35, 0.25), (0.50, 0.50), (0.65, 0.75)]) {
        assert_greyscale_near(&l1, x, y, expected_l1, 2, "L1 analytic Lambert response");
    }

    let l2 = capture(
        workdir.path(),
        "l2",
        SHADOW_VISIBILITY_DEBUG,
        "lights_uploaded=1",
        "tlas_emitted=2",
        &[],
    );
    assert_l2_shadow_transport(&l2);
}

/// L3 is an unobstructed point-lit local medium. L4 changes exactly one thing:
/// a thin opaque partition separates its left half from the light. Besides
/// testing ray-query visibility, the narrow edge band catches any later XY
/// blur or trilinear froxel upscale that mixes the lit column across the wall.
#[test]
#[ignore = "requires an RT-capable Vulkan device and a display/Xvfb"]
fn cornell_l3_l4_volumetric_partition_does_not_leak() {
    let workdir = OracleArtifacts::new("l3-l4-volumetrics");
    let l3 = capture(
        workdir.path(),
        "l3",
        COMPOSITE_DEBUG,
        "lights_uploaded=1",
        "tlas_emitted=1",
        &[],
    );
    let l4 = capture(
        workdir.path(),
        "l4",
        COMPOSITE_DEBUG,
        "lights_uploaded=1",
        // Receiver plus the opaque partition; fog/light are not TLAS meshes.
        "tlas_emitted=2",
        &[],
    );

    let left = [0.30, 0.25, 0.43, 0.75];
    let right = [0.575, 0.25, 0.70, 0.75];
    let edge_left = [0.490_625, 0.25, 0.493_75, 0.75];
    let l3_left = mean_linear_luma(&l3, left);
    let l3_right = mean_linear_luma(&l3, right);
    let l4_left = mean_linear_luma(&l4, left);
    let l4_right = mean_linear_luma(&l4, right);
    let l4_edge_left = mean_linear_luma(&l4, edge_left);

    assert!(
        (l3_left - l3_right).abs() <= 0.01,
        "L3 open medium must be balanced: left={l3_left:.6}, right={l3_right:.6}"
    );
    assert!(
        (l4_right - l3_right).abs() <= 0.01,
        "L4 lit control changed when only the partition was added: L3={l3_right:.6}, L4={l4_right:.6}"
    );
    assert!(
        l4_left <= l3_left * 0.08,
        "L4 broad shadow leaked: shadow={l4_left:.6}, open={l3_left:.6}"
    );
    assert!(
        l4_edge_left <= l4_right * 0.18,
        "L4 wall-adjacent froxel leaked: edge={l4_edge_left:.6}, lit={l4_right:.6}"
    );
}

#[test]
#[ignore = "requires an RT-capable Vulkan device and a display/Xvfb"]
fn cornell_l5_exposes_canonical_material_lobes() {
    let workdir = OracleArtifacts::new("l5-materials");
    let l5 = capture(
        workdir.path(),
        "l5",
        COMPOSITE_DEBUG,
        "lights_uploaded=1",
        // One receiver plus the four canonical material-role probes.
        "tlas_emitted=5",
        &[],
    );

    // The debug view writes stable categorical colours before glass can take
    // an early return. Require meaningful populations for legacy dielectric
    // grey, Disney/PBR gold, and glass blue rather than pinning camera pixels.
    for (name, expected) in [
        ("dielectric", [0.45, 0.45, 0.45]),
        ("metal", [1.00, 0.65, 0.05]),
        ("glass", [0.10, 0.35, 1.00]),
    ] {
        let expected = expected.map(linear_to_srgb_u8);
        let matches = l5
            .pixels()
            .filter(|pixel| {
                pixel
                    .0
                    .iter()
                    .zip(expected)
                    .all(|(actual, target)| actual.abs_diff(target) <= 4)
            })
            .count();
        assert!(matches >= 64, "L5 has no stable {name} lobe population");
    }
}

/// Force the static-BLAS budget below even this tiny scene's live set. The
/// pre-TLAS pass must protect both eligible rigid meshes, preserve a complete
/// TLAS and keep the blocked/unblocked visibility probes unchanged.
#[test]
#[ignore = "requires an RT-capable Vulkan device and a display/Xvfb"]
fn cornell_forced_low_blas_budget_preserves_rt_shadows() {
    let workdir = OracleArtifacts::new("forced-blas-pressure");
    let l2 = capture(
        workdir.path(),
        "l2",
        SHADOW_VISIBILITY_DEBUG,
        "lights_uploaded=1",
        "tlas_emitted=2",
        &["--rt-test-blas-budget-bytes", "1"],
    );
    assert_l2_shadow_transport(&l2);
}

/// Repeat the binary visibility oracle and then translate the complete scene
/// one million units in X/Z. The normalized receiver image and fixed probes
/// must stay stable under camera-relative rendering; detailed selected-ray
/// record equality is exercised by the paired live `render.debug probe`
/// acceptance run documented in the RT recovery plan.
#[test]
#[ignore = "requires an RT-capable Vulkan device and a display/Xvfb"]
fn cornell_l2_visibility_is_repeatable_and_large_coordinate_stable() {
    let workdir = OracleArtifacts::new("l2-repeat-and-large-coordinate");
    let mut captures = Vec::new();
    for _ in 0..3 {
        captures.push(capture(
            workdir.path(),
            "l2",
            SHADOW_VISIBILITY_DEBUG,
            "lights_uploaded=1",
            "tlas_emitted=2",
            &[],
        ));
    }
    for repeat in &captures[1..] {
        assert_visibility_images_stable(&captures[0], repeat, 0.0, "repeated origin run");
    }

    let translated = capture(
        workdir.path(),
        "l2",
        SHADOW_VISIBILITY_DEBUG,
        "lights_uploaded=1",
        "tlas_emitted=2",
        &["--cornell-oracle-world-offset", "1000000,0,-1000000"],
    );
    assert_l2_shadow_transport(&translated);
    assert_visibility_images_stable(
        &captures[0],
        &translated,
        0.005,
        "one-million-unit translated run",
    );
}

fn assert_l2_shadow_transport(l2: &RgbImage) {
    // These normalized points are owned by the fixed oracle camera/manifest:
    // the first lies inside the blocker-cast horizontal shadow arm and the
    // second is the receiver's unobstructed upper-right control.
    let probes = normalized_probes(&l2, &[(0.484_375, 0.638_889), (0.669_531, 0.198_611)]);
    assert_greyscale_near(&l2, probes[0].0, probes[0].1, 0, 2, "L2 blocked visibility");
    assert_greyscale_near(
        &l2,
        probes[1].0,
        probes[1].1,
        255,
        2,
        "L2 unobstructed visibility",
    );

    // Magenta means no reservoir candidate/ray, not an occluder. One global
    // directional must be selectable over the whole receiver, so accepting a
    // magenta interior would make the black shadow assertion ambiguous.
    let (width, height) = l2.dimensions();
    let magenta = l2
        .enumerate_pixels()
        .filter(|(x, y, _)| {
            *x > width / 4 && *x < width * 3 / 4 && *y > height / 20 && *y < height * 19 / 20
        })
        .filter(|(_, _, pixel)| pixel[0] > 240 && pixel[1] < 15 && pixel[2] > 240)
        .count();
    assert_eq!(magenta, 0, "L2 receiver contains no-sample magenta pixels");
}

fn assert_visibility_images_stable(
    reference: &RgbImage,
    candidate: &RgbImage,
    max_changed_fraction: f64,
    label: &str,
) {
    assert_eq!(reference.dimensions(), candidate.dimensions());
    let changed = reference
        .pixels()
        .zip(candidate.pixels())
        .filter(|(left, right)| {
            left.0
                .iter()
                .zip(right.0.iter())
                .any(|(a, b)| a.abs_diff(*b) > 2)
        })
        .count();
    let fraction = changed as f64 / (reference.width() as f64 * reference.height() as f64);
    assert!(
        fraction <= max_changed_fraction,
        "{label} changed {changed} pixels ({:.4}%), limit {:.4}%",
        fraction * 100.0,
        max_changed_fraction * 100.0,
    );
}

fn capture(
    workdir: &Path,
    rung: &str,
    debug_flags: &str,
    expected_lights: &str,
    expected_tlas: &str,
    extra_args: &[&str],
) -> RgbImage {
    let capture_sequence = CAPTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let output_path = workdir.join(format!("{capture_sequence:03}-cornell-{rung}.png"));
    let output_string = output_path
        .to_str()
        .unwrap_or_else(|| panic!("non-UTF-8 output path: {output_path:?}"));

    let mut command = if std::env::var_os("DISPLAY").is_none() {
        let mut command = Command::new("xvfb-run");
        // Winit prefers a stale inherited Wayland session hint over X11 on
        // some headless workers. Clear both hints inside xvfb-run so the
        // generated DISPLAY is the only available presentation backend.
        command.args([
            "-a",
            "env",
            "-u",
            "WAYLAND_DISPLAY",
            "-u",
            "XDG_SESSION_TYPE",
            env!("CARGO"),
        ]);
        command
    } else {
        Command::new(env!("CARGO"))
    };
    command
        .env("RUST_LOG", "warn")
        .env("BYROREDUX_RENDER_DEBUG", debug_flags)
        .args([
            "run",
            "--release",
            "-p",
            "byroredux",
            "--bin",
            "byroredux",
            "--",
            "--cornell-oracle",
            rung,
            "--upscaler",
            "taa",
            "--bench-frames",
            FRAMES,
            "--bench-mode",
            "renderer-static",
            "--screenshot",
            output_string,
        ])
        .args(extra_args);
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("launch Cornell {rung}: {error}"));

    // The integration workflow uploads this directory with `if: always()`.
    // Retaining process logs next to the image makes a failed analytic probe
    // diagnosable without reproducing the runner's GPU/driver state.
    std::fs::write(output_path.with_extension("stdout.log"), &output.stdout)
        .expect("write Cornell oracle stdout artifact");
    std::fs::write(output_path.with_extension("stderr.log"), &output.stderr)
        .expect("write Cornell oracle stderr artifact");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Cornell {rung} exited with {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    for expected in [
        "rt-integrity:",
        "verdict=PASS",
        expected_lights,
        expected_tlas,
    ] {
        assert!(
            stdout.contains(expected),
            "Cornell {rung} output is missing `{expected}`\nstdout:\n{stdout}"
        );
    }
    if let Some(index) = extra_args
        .iter()
        .position(|arg| *arg == "--rt-test-blas-budget-bytes")
    {
        let bytes = extra_args
            .get(index + 1)
            .expect("test BLAS budget flag must carry a value");
        let marker = format!("RT TEST BLAS memory budget override active: {bytes} bytes");
        assert!(
            stderr.contains(&marker),
            "Cornell {rung} did not activate the requested pressure override `{marker}`\nstderr:\n{stderr}"
        );
    }

    image::open(&output_path)
        .unwrap_or_else(|error| {
            panic!(
                "load Cornell {rung} capture {}: {error}",
                output_path.display()
            )
        })
        .to_rgb8()
}

fn normalized_probes(image: &RgbImage, probes: &[(f32, f32)]) -> Vec<(u32, u32)> {
    let (width, height) = image.dimensions();
    probes
        .iter()
        .map(|&(x, y)| {
            (
                (x * width as f32).round().clamp(0.0, (width - 1) as f32) as u32,
                (y * height as f32).round().clamp(0.0, (height - 1) as f32) as u32,
            )
        })
        .collect()
}

fn mean_linear_luma(image: &RgbImage, region: [f32; 4]) -> f32 {
    let (width, height) = image.dimensions();
    let x0 = (region[0] * width as f32)
        .floor()
        .clamp(0.0, width as f32 - 1.0) as u32;
    let y0 = (region[1] * height as f32)
        .floor()
        .clamp(0.0, height as f32 - 1.0) as u32;
    let x1 = (region[2] * width as f32)
        .ceil()
        .clamp((x0 + 1) as f32, width as f32) as u32;
    let y1 = (region[3] * height as f32)
        .ceil()
        .clamp((y0 + 1) as f32, height as f32) as u32;

    let mut sum = 0.0;
    let mut count = 0_u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let pixel = image.get_pixel(x, y);
            let linear = pixel.0.map(srgb_u8_to_linear);
            sum += linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
            count += 1;
        }
    }
    sum / count as f32
}

fn assert_greyscale_near(
    image: &RgbImage,
    x: u32,
    y: u32,
    expected: u8,
    tolerance: u8,
    label: &str,
) {
    let pixel = image.get_pixel(x, y);
    for channel in pixel.0 {
        assert!(
            channel.abs_diff(expected) <= tolerance,
            "{label} at ({x},{y}) was {:?}, expected {expected} +/- {tolerance}",
            pixel.0
        );
    }
}

fn linear_to_srgb_u8(linear: f32) -> u8 {
    let encoded = if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (encoded.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn srgb_u8_to_linear(encoded: u8) -> f32 {
    let encoded = encoded as f32 / 255.0;
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

#[test]
fn analytic_l1_encoding_is_pinned() {
    assert_eq!(linear_to_srgb_u8(2.0 / 6.0_f32.sqrt()), 233);
    assert!((srgb_u8_to_linear(233) - 2.0 / 6.0_f32.sqrt()).abs() < 0.005);
}
