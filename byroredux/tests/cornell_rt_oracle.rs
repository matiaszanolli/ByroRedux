//! Hardware-gated L0-L2 ray-transport oracle.
//!
//! Run on an RT-capable integration worker:
//!
//! ```bash
//! cargo test --release -p byroredux --test cornell_rt_oracle -- --ignored --nocapture
//! ```
//!
//! The test deliberately uses raw renderer debug outputs rather than normal
//! presentation. Exposure, ACES, temporal upscale, fog, bloom and grading are
//! bypassed, so each sampled pixel answers exactly one transport question.

use image::RgbImage;
use std::path::Path;
use std::process::Command;

const FRAMES: &str = "30";
const DIRECT_DEBUG: &str = "0x4000000";
const SHADOW_VISIBILITY_DEBUG: &str = "0x84000000";

#[test]
#[ignore = "requires an RT-capable Vulkan device and a display/Xvfb"]
fn cornell_l0_l2_transport_ladder_matches_analytic_probes() {
    let workdir = tempfile::tempdir().expect("create Cornell oracle tempdir");

    let l0 = capture(
        workdir.path(),
        "l0",
        DIRECT_DEBUG,
        "lights_uploaded=0",
        "tlas_emitted=1",
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
    );
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

fn capture(
    workdir: &Path,
    rung: &str,
    debug_flags: &str,
    expected_lights: &str,
    expected_tlas: &str,
) -> RgbImage {
    let output_path = workdir.join(format!("cornell-{rung}.png"));
    let output_string = output_path
        .to_str()
        .unwrap_or_else(|| panic!("non-UTF-8 output path: {output_path:?}"));

    let mut command = if std::env::var_os("DISPLAY").is_none() {
        let mut command = Command::new("xvfb-run");
        command.args(["-a", env!("CARGO")]);
        command
    } else {
        Command::new(env!("CARGO"))
    };
    let output = command
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
        .output()
        .unwrap_or_else(|error| panic!("launch Cornell {rung}: {error}"));

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

#[test]
fn analytic_l1_encoding_is_pinned() {
    assert_eq!(linear_to_srgb_u8(2.0 / 6.0_f32.sqrt()), 233);
}
