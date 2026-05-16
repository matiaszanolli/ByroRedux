//! Instance-buffer hash tests — sibling of `material_hash_tests`.
//!
//! Pins the dirty-gate contract for `upload_instances` (#1134 /
//! PERF-D8-NEW-01). Each test mirrors a counterpart in
//! `material_hash_tests.rs`.

use super::descriptors::hash_instance_slice;
use super::gpu_types::GpuInstance;

fn sample_instance(seed: u32) -> GpuInstance {
    let mut g = GpuInstance::default();
    // Touch a representative subset of fields so the hash depends on
    // real instance content rather than padding.
    g.texture_index = seed;
    g.material_id = seed & 0xff;
    g.bone_offset = seed.wrapping_mul(31);
    g
}

/// Identical slices produce identical hashes — the steady-state case
/// the dirty-gate is designed to detect (static interior cell rendering
/// the same instances every frame).
#[test]
fn identical_slices_hash_to_same_value() {
    let insts: Vec<GpuInstance> = (0..32).map(sample_instance).collect();
    let h1 = hash_instance_slice(&insts);
    let h2 = hash_instance_slice(&insts);
    assert_eq!(
        h1, h2,
        "identical slice contents must hash to the same value — \
         the dirty-gate skip relies on this",
    );
}

/// A single-bit change in one instance produces a different hash.
/// Without this, a real instance change (entity moved, texture
/// rebound, etc.) would silently skip the upload and the GPU would
/// render with stale data.
#[test]
fn single_field_change_changes_hash() {
    let mut insts: Vec<GpuInstance> = (0..32).map(sample_instance).collect();
    let h_before = hash_instance_slice(&insts);
    insts[15].texture_index ^= 1;
    let h_after = hash_instance_slice(&insts);
    assert_ne!(
        h_before, h_after,
        "a single field change must shift the hash — \
         else the upload would skip a real instance update",
    );
}

/// A length-only change produces a different hash even when every
/// existing entry is byte-identical. Without this, growing the
/// instance list by adding a default-instance slot would silently
/// skip the upload.
#[test]
fn length_change_changes_hash() {
    let insts: Vec<GpuInstance> = (0..32).map(sample_instance).collect();
    let mut grown = insts.clone();
    grown.push(GpuInstance::default());
    assert_ne!(
        hash_instance_slice(&insts),
        hash_instance_slice(&grown),
        "length change must shift the hash",
    );
}

/// Empty-slice hash is deterministic. Production routes `count == 0`
/// through an early-out before the hash computation, so this is
/// documentary — but pinning the boundary behaviour stops drift if
/// the early-out is ever moved.
#[test]
fn empty_slice_hash_is_deterministic() {
    let h1 = hash_instance_slice(&[]);
    let h2 = hash_instance_slice(&[]);
    assert_eq!(h1, h2);
}
