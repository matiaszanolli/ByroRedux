//! Tests for `terrain_splat_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`terrain_splat_tests::FOO`).

//! Regression tests for #470 — LAND splat layer packing. Covers
//! quantization, seam max-reconciliation, and absent-quadrant
//! handling. Pure-Rust, no GPU.
use super::{quadrant_samples_for_vertex, splat_weight_for_vertex, CellSplatLayer};

fn mk_layer(per_quadrant_alpha: [Option<Vec<f32>>; 4]) -> CellSplatLayer {
    CellSplatLayer {
        texture_index: 1,
        per_quadrant_alpha,
    }
}

#[test]
fn splat_quantization_full_and_empty_map_to_boundary_bytes() {
    // Single-quadrant full-coverage grid → every vertex in SW
    // reads 255; vertices outside SW (NE corner) read 0.
    let alpha = vec![1.0_f32; 17 * 17];
    let layer = mk_layer([Some(alpha), None, None, None]);
    // (0,0) is SW(0,0) only.
    assert_eq!(splat_weight_for_vertex(&layer, 0, 0), 255);
    // (32,32) is NE(16,16) only, which has no alpha.
    assert_eq!(splat_weight_for_vertex(&layer, 32, 32), 0);
}

#[test]
fn splat_seam_reconciliation_takes_max_across_quadrants() {
    // Col 16 is shared between SW (local col 16) and SE (local col 0).
    // SW paints alpha=1.0 on its east edge; SE paints alpha=0.0.
    // Max wins → seam vertex reads 255, not 127.
    let mut sw_alpha = vec![0.0_f32; 17 * 17];
    for row in 0..17 {
        sw_alpha[row * 17 + 16] = 1.0; // SW east edge
    }
    let se_alpha = vec![0.0_f32; 17 * 17]; // SE paints nothing
    let layer = mk_layer([Some(sw_alpha), Some(se_alpha), None, None]);
    // Global (row=0, col=16) is on the SW/SE seam.
    assert_eq!(splat_weight_for_vertex(&layer, 0, 16), 255);
}

#[test]
fn quadrant_samples_classify_corner_as_four_way() {
    // The dead-center vertex (16,16) sits on SW/SE/NW/NE.
    let samples = quadrant_samples_for_vertex(16, 16);
    let present: Vec<u8> = samples
        .iter()
        .map(|(q, _, _)| *q)
        .filter(|q| *q < 4)
        .collect();
    assert_eq!(present, vec![0, 1, 2, 3]);
}

#[test]
fn quadrant_samples_interior_vertex_belongs_to_single_quadrant() {
    let samples = quadrant_samples_for_vertex(5, 10);
    let present: Vec<u8> = samples
        .iter()
        .map(|(q, _, _)| *q)
        .filter(|q| *q < 4)
        .collect();
    assert_eq!(present, vec![0]); // SW only
                                  // Local coords match global for SW.
    assert_eq!(samples[0], (0, 5, 10));
}

#[test]
fn splat_round_trip_through_u8_preserves_half_alpha_within_tolerance() {
    // alpha = 0.5 → quantized 128 (round(127.5) = 128 under
    // banker's rounding; Rust's f32::round is half-away-from-zero
    // so 127.5 → 128).
    let alpha = vec![0.5_f32; 17 * 17];
    let layer = mk_layer([Some(alpha), None, None, None]);
    let w = splat_weight_for_vertex(&layer, 0, 0);
    assert!(
        w == 127 || w == 128,
        "alpha=0.5 should quantize to ~128, got {}",
        w
    );
}

#[test]
fn splat_absent_quadrant_yields_zero() {
    // A layer with `None` on every quadrant — e.g. the no-ATXT
    // case — must produce zero everywhere. Guards against a
    // sampler that forgets to short-circuit on None.
    let layer = mk_layer([None, None, None, None]);
    for row in [0, 16, 32] {
        for col in [0, 16, 32] {
            assert_eq!(
                splat_weight_for_vertex(&layer, row, col),
                0,
                "absent-everywhere layer must read 0 at ({},{})",
                row,
                col
            );
        }
    }
}
