//! Material-table hash + bindless-overflow tests.
//!
//! Helpers that load shader sources via `include_str!` are also exercised
//! here.

// (size_of from std::mem)

use super::super::material::GpuMaterial;
use super::descriptors::hash_material_slice;

fn sample_material(seed: u32) -> GpuMaterial {
        let mut m = GpuMaterial::default();
        // Touch a representative subset of fields so the hash
        // depends on real material content rather than padding.
        m.material_flags = seed;
        m.material_kind = (seed & 0xff) as u32;
        m
}

/// Pin: identical slices produce identical hashes — the steady-
/// state case the dirty-gate is designed to detect.
#[test]
fn identical_slices_hash_to_same_value() {
        let mats: Vec<GpuMaterial> = (0..16).map(sample_material).collect();
        let h1 = hash_material_slice(&mats);
        let h2 = hash_material_slice(&mats);
        assert_eq!(
            h1, h2,
            "identical slice contents must hash to the same value — \
             the dirty-gate skip relies on this",
        );
}

/// Pin: a single-bit change in one material produces a different
/// hash. Without this, a real material change would silently
/// skip the upload and the GPU would render with stale data.
#[test]
fn single_field_change_changes_hash() {
        let mut mats: Vec<GpuMaterial> = (0..16).map(sample_material).collect();
        let h_before = hash_material_slice(&mats);
        mats[7].material_flags ^= 1;
        let h_after = hash_material_slice(&mats);
        assert_ne!(
            h_before, h_after,
            "a single field change must shift the hash — \
             else the upload would skip a real material update",
        );
}

/// Pin: a length-only change (one extra zero-default material
/// appended) produces a different hash even when every existing
/// entry is byte-identical. Without this, growing the table by
/// adding a default-material slot would silently skip the
/// upload.
#[test]
fn length_change_changes_hash() {
        let mats: Vec<GpuMaterial> = (0..16).map(sample_material).collect();
        let mut grown = mats.clone();
        grown.push(GpuMaterial::default());
        assert_ne!(
            hash_material_slice(&mats),
            hash_material_slice(&grown),
            "length change must shift the hash",
        );
}

/// Empty-slice hash is deterministic. Production callers route
/// the `count == 0` case through an early-out before the hash
/// computation, so this is documentary — but pinning the hash's
/// behaviour at the boundary stops drift if the early-out is
/// ever moved.
#[test]
fn empty_slice_hash_is_deterministic() {
        let h1 = hash_material_slice(&[]);
        let h2 = hash_material_slice(&[]);
        assert_eq!(h1, h2);
}
