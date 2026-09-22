//! Tests for `terrain_splat_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`terrain_splat_tests::FOO`).

//! Regression tests for #470 — LAND splat layer packing. Covers
//! quantization, seam max-reconciliation, and absent-quadrant
//! handling. Pure-Rust, no GPU.
use super::terrain::{
    authored_grass_for_splat_layers, base_transition_alpha, base_transition_layers_for_bases,
    quadrant_samples_for_vertex, splat_weight_for_vertex, CellSplatLayer,
};

fn mk_layer(per_quadrant_alpha: [Option<Vec<f32>>; 4]) -> CellSplatLayer {
    CellSplatLayer {
        ltex_form_id: None,
        // #4054 — irrelevant to splat packing, which is what these pin.
        cover_affinity: crate::groundcover_translate::DEFAULT_AFFINITY,
        diffuse_index: 1,
        normal_index: 0,
        specular_index: 0,
        per_quadrant_alpha,
    }
}

#[test]
fn authored_grass_binding_follows_the_packed_splat_lane_order() {
    let alpha = [None, None, None, None];
    let mut first = mk_layer(alpha.clone());
    first.ltex_form_id = Some(0x10);
    let mut second = mk_layer(alpha.clone());
    second.ltex_form_id = Some(0x20);
    let third = mk_layer(alpha);
    let map = std::collections::HashMap::from([(0x10, vec![0xA0]), (0x20, vec![0xB0])]);

    assert_eq!(
        authored_grass_for_splat_layers(&[first, second, third], &map),
        [
            vec![0xA0],
            vec![0xB0],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new()
        ]
    );
}

/// #4642 — `LTEX.GNAM` is an array (151 of 184 grass-bearing vanilla
/// LTEXs author 2–4 species). The per-lane binding must carry the whole
/// authored list in authored order, not the last-wins single grass the
/// pre-#4642 map kept.
#[test]
fn authored_grass_binding_carries_the_full_gnam_array_in_order() {
    let alpha = [None, None, None, None];
    let mut layer = mk_layer(alpha);
    layer.ltex_form_id = Some(0x30);
    let map = std::collections::HashMap::from([(0x30, vec![0x111, 0x222, 0x333])]);
    let bound = authored_grass_for_splat_layers(&[layer], &map);
    assert_eq!(bound[0], vec![0x111, 0x222, 0x333]);
    assert!(
        bound[1..].iter().all(|l| l.is_empty()),
        "lanes with no LTEX link stay empty"
    );
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

#[test]
fn btxt_transition_feathers_only_against_a_different_neighbor() {
    // SW and SE share a grass base, while NW is rock. The SW→NW edge gets
    // the half-weight blend but the SW→SE edge remains solid grass.
    let bases = [Some(0x10), Some(0x10), Some(0x20), Some(0x20)];
    let sw = base_transition_alpha(0, &bases);
    assert_eq!(
        sw[8 * 17 + 8],
        1.0,
        "quadrant interior stays fully weighted"
    );
    assert_eq!(
        sw[16 * 17 + 8],
        0.5,
        "different north neighbor is feathered"
    );
    assert_eq!(
        sw[8 * 17 + 16],
        1.0,
        "same-base east neighbor is not feathered"
    );
}

#[test]
fn btxt_transition_plan_keeps_only_noncanonical_quadrant_bases() {
    // Canonical SW/NW grass stays in the entity base material. SE's rock and
    // NE's executable-default dirt become the two deterministic splat lanes.
    let bases = [Some(0x10), Some(0x20), Some(0x10), None];
    let transitions = base_transition_layers_for_bases(&bases, &[true; 4], Some(0x10));
    assert_eq!(transitions.len(), 2);
    // BTree order gives default (`None`) the first lane, then rock.
    assert_eq!(transitions[0].0, None);
    assert!(transitions[0].1[3].is_some());
    assert!(transitions[0].1[..3].iter().all(Option::is_none));
    assert_eq!(transitions[1].0, Some(0x20));
    assert!(transitions[1].1[1].is_some());
    assert!(transitions[1].1[0].is_none());
    assert!(transitions[1].1[2].is_none());
    assert!(transitions[1].1[3].is_none());
}

#[test]
fn btxt_transition_plan_ignores_absent_land_quadrants() {
    // A malformed partial LAND must not invent default-texture coverage for
    // its missing quadrants.
    let transitions = base_transition_layers_for_bases(
        &[Some(0x10), None, None, None],
        &[true, false, false, false],
        Some(0x10),
    );
    assert!(transitions.is_empty());
}
