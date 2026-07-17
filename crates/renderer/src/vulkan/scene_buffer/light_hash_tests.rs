//! Light-buffer hash tests — sibling of `instance_hash_tests` /
//! `material_hash_tests` / `indirect_hash_tests`.
//!
//! Pins the dirty-gate contract for `upload_lights` (#2036 / PERF-D4-01).
//! Each test mirrors a counterpart in `instance_hash_tests.rs`.

use super::descriptors::hash_light_slice;
use super::gpu_types::GpuLight;

fn sample_light(seed: u32) -> GpuLight {
    let mut g = GpuLight::default();
    // Touch a representative subset of fields so the hash depends on
    // real light content rather than padding.
    g.position_radius = [seed as f32, 0.0, 0.0, 10.0];
    g.color_type = [1.0, 1.0, 1.0, (seed % 3) as f32];
    g.params = [seed as f32 * 0.1, 0.0, 0.0, 0.0];
    g
}

/// Identical slices produce identical hashes — the steady-state case
/// the dirty-gate is designed to detect (a static interior cell with no
/// dynamic lights uploading the same lights every frame).
#[test]
fn identical_slices_hash_to_same_value() {
    let lights: Vec<GpuLight> = (0..8).map(sample_light).collect();
    let h1 = hash_light_slice(&lights);
    let h2 = hash_light_slice(&lights);
    assert_eq!(
        h1, h2,
        "identical slice contents must hash to the same value — \
         the dirty-gate skip relies on this",
    );
}

/// A single-bit change in one light produces a different hash. Without
/// this, a real light change (a flickering torch's color/radius) would
/// silently skip the upload and the GPU would render with stale data.
#[test]
fn single_field_change_changes_hash() {
    let mut lights: Vec<GpuLight> = (0..8).map(sample_light).collect();
    let h_before = hash_light_slice(&lights);
    lights[3].position_radius[3] += 1.0;
    let h_after = hash_light_slice(&lights);
    assert_ne!(
        h_before, h_after,
        "a single field change must shift the hash — \
         else the upload would skip a real light update",
    );
}

/// A length-only change produces a different hash even when every
/// existing entry is byte-identical. Without this, a new light entering
/// the scene (appended after the existing entries) would silently skip
/// the upload.
#[test]
fn length_change_changes_hash() {
    let lights: Vec<GpuLight> = (0..8).map(sample_light).collect();
    let mut grown = lights.clone();
    grown.push(GpuLight::default());
    assert_ne!(
        hash_light_slice(&lights),
        hash_light_slice(&grown),
        "length change must shift the hash",
    );
}

/// Empty-slice hash is deterministic. A scene with zero lights hashes
/// the same every call, so back-to-back empty-scene frames still hit
/// the dirty-gate skip after the header's first zero-count write.
#[test]
fn empty_slice_hash_is_deterministic() {
    let h1 = hash_light_slice(&[]);
    let h2 = hash_light_slice(&[]);
    assert_eq!(h1, h2);
}
