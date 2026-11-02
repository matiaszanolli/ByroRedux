//! M-LIGHT reference gates (docs/prd/reference-lighting.md §7.2).
//!
//! Each test runs `scripts/m-light-gates.sh` for one acceptance scenario and
//! asserts that every gate row in its manifest passed. They need an
//! RT-capable Vulkan device, a display or Xvfb, and a Python with Mitsuba 3
//! and NumPy named by `BYROREDUX_MITSUBA_PYTHON` — which is why they live in
//! their own test target rather than `cornell_rt_oracle`, whose CI job has no
//! Mitsuba. Run serialised:
//!
//! ```bash
//! BYROREDUX_MITSUBA_PYTHON=~/mitsuba/bin/python \
//!   cargo test -p byroredux --test m_light_gates -- --ignored --nocapture --test-threads=1
//! ```
//!
//! References are cached under `target/m-light/reference-cache` (RLT-REF-015),
//! so only the first run after a scene change renders them.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// Two gate runs must never drive the GPU at once.
static ENGINE: Mutex<()> = Mutex::new(());

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("byroredux sits one level under the repository root")
        .to_path_buf()
}

fn run_gates(gate: &str) {
    let _guard = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    let root = repo_root();
    let out = std::env::var_os("BYROREDUX_RT_ARTIFACT_DIR")
        .map(|dir| PathBuf::from(dir).join(format!("m-light-{gate}")))
        .unwrap_or_else(|| root.join("target").join(format!("m-light-{gate}")));
    let status = Command::new(root.join("scripts/m-light-gates.sh"))
        .args(["--gates", gate, "--out"])
        .arg(&out)
        .arg("--cache")
        .arg(root.join("target/m-light/reference-cache"))
        .status()
        .expect("run scripts/m-light-gates.sh");
    assert!(
        status.success(),
        "M-LIGHT {gate} failed (exit {status}); manifest and captures under {}",
        out.display()
    );
}

/// A15 — calibration ratio 1 for point emitters, constant across the plane,
/// and the Lambertian flat planes at three view angles (RLT-REF-010,
/// RLT-REF-014).
#[test]
#[ignore = "requires an RT-capable Vulkan device, a display/Xvfb and Mitsuba 3 (BYROREDUX_MITSUBA_PYTHON)"]
fn cornell_calibration_and_material_rungs() {
    run_gates("A15");
}

/// A16 — components sum to the total on both sides, SVGF bypassed and
/// `final` unreconstructed; SVGF's energy change reported (RLT-REF-011,
/// RLT-REF-018, RLT-REF-020).
#[test]
#[ignore = "requires an RT-capable Vulkan device, a display/Xvfb and Mitsuba 3 (BYROREDUX_MITSUBA_PYTHON)"]
fn cornell_components_sum_to_total() {
    run_gates("A16");
}
