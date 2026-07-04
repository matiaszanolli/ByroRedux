//! Indirect-draw hash tests — sibling of `instance_hash_tests` /
//! `material_hash_tests`.
//!
//! Pins the dirty-gate contract for `upload_indirect_draws` (#1809 /
//! PERF-D4-NEW-03). Each test mirrors a counterpart in
//! `instance_hash_tests.rs`.

use super::descriptors::hash_indirect_slice;
use ash::vk;

fn sample_draw(seed: u32) -> vk::DrawIndexedIndirectCommand {
    vk::DrawIndexedIndirectCommand {
        index_count: seed,
        instance_count: seed & 0xff,
        first_index: seed.wrapping_mul(3),
        vertex_offset: (seed as i32).wrapping_mul(7),
        first_instance: seed.wrapping_mul(31),
    }
}

/// Identical slices produce identical hashes — the steady-state case
/// the dirty-gate is designed to detect (static interior cell issuing
/// the same batches, and therefore the same indirect commands, every
/// frame).
#[test]
fn identical_slices_hash_to_same_value() {
    let draws: Vec<_> = (0..32).map(sample_draw).collect();
    let h1 = hash_indirect_slice(&draws);
    let h2 = hash_indirect_slice(&draws);
    assert_eq!(
        h1, h2,
        "identical slice contents must hash to the same value — \
         the dirty-gate skip relies on this",
    );
}

/// A single-field change in one command produces a different hash.
/// Without this, a real batch change (a draw's index range shifted,
/// instance count changed, etc.) would silently skip the upload and
/// the GPU would issue stale indirect draws.
#[test]
fn single_field_change_changes_hash() {
    let mut draws: Vec<_> = (0..32).map(sample_draw).collect();
    let h_before = hash_indirect_slice(&draws);
    draws[15].index_count ^= 1;
    let h_after = hash_indirect_slice(&draws);
    assert_ne!(
        h_before, h_after,
        "a single field change must shift the hash — \
         else the upload would skip a real indirect-command update",
    );
}

/// A length-only change produces a different hash even when every
/// existing entry is byte-identical. Without this, growing the batch
/// list by one more pipeline-group split would silently skip the
/// upload.
#[test]
fn length_change_changes_hash() {
    let draws: Vec<_> = (0..32).map(sample_draw).collect();
    let mut grown = draws.clone();
    grown.push(vk::DrawIndexedIndirectCommand::default());
    assert_ne!(
        hash_indirect_slice(&draws),
        hash_indirect_slice(&grown),
        "length change must shift the hash",
    );
}

/// Empty-slice hash is deterministic. Production routes `count == 0`
/// through an early-out before the hash computation, so this is
/// documentary — but pinning the boundary behaviour stops drift if
/// the early-out is ever moved.
#[test]
fn empty_slice_hash_is_deterministic() {
    let h1 = hash_indirect_slice(&[]);
    let h2 = hash_indirect_slice(&[]);
    assert_eq!(h1, h2);
}
