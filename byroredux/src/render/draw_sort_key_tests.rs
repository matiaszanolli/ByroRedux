use super::{draw_sort_key, DrawCommand};

/// Minimal DrawCommand builder — only the fields that affect the
/// sort key are interesting. Everything else is zeroed.
fn cmd(alpha_blend: bool, is_decal: bool, two_sided: bool) -> DrawCommand {
    use byroredux_core::ecs::components::RenderLayer;
    DrawCommand {
        mesh_handle: 0,
        texture_handle: 0,
        model_matrix: [0.0; 16],
        alpha_blend,
        src_blend: 6,
        dst_blend: 7,
        two_sided,
        is_decal,
        render_layer: if is_decal {
            RenderLayer::Decal
        } else {
            RenderLayer::Architecture
        },
        bone_offset: 0,
        normal_map_index: 0,
        dark_map_index: 0,
        glow_map_index: 0,
        detail_map_index: 0,
        gloss_map_index: 0,
        parallax_map_index: 0,
        parallax_height_scale: 0.0,
        parallax_max_passes: 0.0,
        env_map_index: 0,
        env_mask_index: 0,
        alpha_threshold: 0.0,
        alpha_test_func: 0,
        roughness: 0.5,
        metalness: 0.0,
        emissive_mult: 0.0,
        emissive_color: [0.0; 3],
        specular_strength: 0.0,
        specular_color: [0.0; 3],
        diffuse_color: [1.0; 3],
        ambient_color: [1.0; 3],
        vertex_offset: 0,
        index_offset: 0,
        vertex_count: 0,
        sort_depth: 0,
        in_tlas: false,
        in_raster: true,
        entity_id: 0,
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        material_alpha: 1.0,
        avg_albedo: [0.0; 3],
        material_kind: 0,
        z_test: true,
        z_write: true,
        z_function: 3,
        terrain_tile_index: None,
        skin_tint_rgba: [0.0; 4],
        hair_tint_rgb: [0.0; 3],
        multi_layer_envmap_strength: 0.0,
        eye_left_center: [0.0; 3],
        eye_cubemap_scale: 0.0,
        eye_right_center: [0.0; 3],
        multi_layer_inner_thickness: 0.0,
        multi_layer_refraction_scale: 0.0,
        multi_layer_inner_scale: [1.0, 1.0],
        sparkle_rgba: [0.0; 4],
        effect_falloff: [1.0, 1.0, 1.0, 1.0, 0.0],
        material_id: 0,
        vertex_color_emissive: false,
        effect_shader_flags: 0,
        is_water: false,
    }
}

/// Regression for #500 (PERF D3-M2): a stale debug_assert! in
/// `draw_frame` had the sort-key tuple fields in the wrong order.
/// This test owns the sort contract in the same crate as the sort
/// itself, so drift can't happen silently.
///
/// Cluster order must be:
///   1. alpha_blend   (opaque before transparent)
///   2. is_decal
///   3. two_sided
/// #renderlayer — slot 1 of the sort tuple was widened from
/// `is_decal as u8 ∈ {0,1}` to `render_layer as u8 ∈ {0..3}`.
/// Verify that consecutive same-layer draws cluster correctly so
/// the batch coalescer and `vkCmdSetDepthBias` change-tracking
/// in `draw.rs` see runs of one layer at a time, not interleaved
/// layers thrashing the dynamic state.
#[test]
fn sort_key_clusters_by_render_layer_within_alpha_blend() {
    use byroredux_core::ecs::components::RenderLayer;
    let layers = [
        RenderLayer::Decal,        // 3
        RenderLayer::Architecture, // 0
        RenderLayer::Actor,        // 2
        RenderLayer::Clutter,      // 1
    ];
    let mut cmds: Vec<DrawCommand> = layers
        .iter()
        .map(|&l| {
            let mut c = cmd(false, false, false);
            c.render_layer = l;
            c.is_decal = l == RenderLayer::Decal;
            c
        })
        .collect();
    cmds.sort_by_key(draw_sort_key);
    let observed: Vec<u8> = cmds.iter().map(|c| c.render_layer as u8).collect();
    // Ascending — Architecture (0) drawn first owns the depth
    // buffer; Decal (3) drawn last wins every coplanar tie via
    // its strongest bias.
    assert_eq!(observed, vec![0u8, 1, 2, 3]);
}

#[test]
fn sort_key_clusters_by_alpha_decal_twosided() {
    // Construct every 2³ combination in scrambled order.
    let mut cmds = vec![
        cmd(true, true, true),
        cmd(false, false, false),
        cmd(true, false, true),
        cmd(false, true, false),
        cmd(true, true, false),
        cmd(false, false, true),
        cmd(true, false, false),
        cmd(false, true, true),
    ];
    cmds.sort_by_key(draw_sort_key);

    let observed: Vec<(bool, bool, bool)> = cmds
        .iter()
        .map(|c| (c.alpha_blend, c.is_decal, c.two_sided))
        .collect();
    let expected = [
        (false, false, false),
        (false, false, true),
        (false, true, false),
        (false, true, true),
        (true, false, false),
        (true, false, true),
        (true, true, false),
        (true, true, true),
    ];
    assert_eq!(observed, expected.to_vec());
}

/// Opaque draws sort front-to-back within the same
/// (is_decal, two_sided, depth_state) cluster — the last key slot
/// carries `sort_depth` ascending so early-Z benefits most draws.
#[test]
fn opaque_within_cluster_sorts_front_to_back() {
    let mut near = cmd(false, false, false);
    near.sort_depth = 100;
    let mut far = cmd(false, false, false);
    far.sort_depth = 900;
    let mut cmds = vec![far, near];
    cmds.sort_by_key(draw_sort_key);
    assert_eq!(cmds[0].sort_depth, 100);
    assert_eq!(cmds[1].sort_depth, 900);
}

/// Transparent draws sort back-to-front for correct blending —
/// the key uses `!sort_depth` so larger depth sorts first.
#[test]
fn transparent_within_cluster_sorts_back_to_front() {
    let mut near = cmd(true, false, false);
    near.sort_depth = 100;
    let mut far = cmd(true, false, false);
    far.sort_depth = 900;
    let mut cmds = vec![near, far];
    cmds.sort_by_key(draw_sort_key);
    assert_eq!(cmds[0].sort_depth, 900);
    assert_eq!(cmds[1].sort_depth, 100);
}

/// Regression for #499: interleaved additive and alpha-blend draws
/// sort into separate `(src_blend, dst_blend)` cohorts so the
/// blend-pipeline cache doesn't thrash on every depth alternation.
#[test]
fn transparent_clusters_by_blend_factors() {
    let mut alpha_near = cmd(true, false, false);
    alpha_near.src_blend = 6;
    alpha_near.dst_blend = 7;
    alpha_near.sort_depth = 100;
    let mut additive_far = cmd(true, false, false);
    additive_far.src_blend = 6;
    additive_far.dst_blend = 1;
    additive_far.sort_depth = 900;
    let mut alpha_far = cmd(true, false, false);
    alpha_far.src_blend = 6;
    alpha_far.dst_blend = 7;
    alpha_far.sort_depth = 500;
    let mut cmds = vec![alpha_near, additive_far, alpha_far];
    cmds.sort_by_key(draw_sort_key);
    // Additive (dst=1) sorts before alpha (dst=7) by u32 order.
    // Both alpha draws land together, sorted back-to-front within.
    assert_eq!(cmds[0].dst_blend, 1);
    assert_eq!(cmds[1].dst_blend, 7);
    assert_eq!(cmds[1].sort_depth, 500);
    assert_eq!(cmds[2].dst_blend, 7);
    assert_eq!(cmds[2].sort_depth, 100);
}

/// Regression for #506: with ties in the 8-tuple prefix (same
/// mesh, same pipeline state, same depth bucket) the `entity_id`
/// final slot must break them deterministically so two sorts of
/// the same input produce byte-identical output. Pre-#506 the
/// key ended on `mesh_handle` and rayon's work-stealing in
/// `par_sort_unstable_by_key` could reorder tied entries across
/// runs.
#[test]
fn sort_key_is_deterministic_for_full_tuple_ties() {
    // Ten draws that collide on every slot except entity_id —
    // identical mesh, texture, depth bucket, blend factors.
    // `DrawCommand` isn't Clone, so build two independent Vecs
    // from the same factory and feed them opposite starting orders.
    fn make_tied_batch() -> Vec<DrawCommand> {
        (0..10u32)
            .map(|id| {
                let mut c = cmd(false, false, false);
                c.mesh_handle = 42;
                c.texture_handle = 7;
                c.sort_depth = 500;
                c.entity_id = id;
                c
            })
            .collect()
    }

    let mut a = make_tied_batch();
    // Shuffle `a` so a stable sort starting from insertion order
    // wouldn't accidentally produce ordered output.
    a.swap(0, 7);
    a.swap(3, 9);
    a.swap(1, 5);

    let mut b = make_tied_batch();
    b.reverse(); // fully different starting order from `a`

    a.sort_by_key(draw_sort_key);
    b.sort_by_key(draw_sort_key);

    let a_ids: Vec<u32> = a.iter().map(|c| c.entity_id).collect();
    let b_ids: Vec<u32> = b.iter().map(|c| c.entity_id).collect();
    assert_eq!(
        a_ids, b_ids,
        "same input → same output regardless of starting order"
    );
    assert_eq!(
        a_ids,
        (0..10u32).collect::<Vec<_>>(),
        "entity_id breaks ties ascending"
    );
}

/// #934 / PERF-DC-01 — measure serial vs parallel sort cost across
/// scene-sized N. The audit claims rayon's `par_sort_unstable_by_key`
/// loses to `sort_unstable_by_key` on the closure-extracted 9-tuple
/// key at typical Bethesda draw counts (~800–1500), and that the
/// crossover is in the 2K range.
///
/// `#[ignore]` because the timings are environment-dependent — this is
/// a one-shot measurement gate, not a regression test. Run with
/// `cargo test -p byroredux --release manual_bench_draw_sort_serial_vs_parallel -- --ignored --nocapture`.
#[test]
#[ignore]
fn manual_bench_draw_sort_serial_vs_parallel() {
    use rayon::prelude::*;
    use std::time::Instant;
    fn make_inputs(n: usize) -> Vec<DrawCommand> {
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            let mut c = cmd((i % 7) == 0, (i % 13) == 0, (i % 5) == 0);
            // Vary the fields the sort key actually reads so the
            // comparator does real work rather than constant-folding.
            c.mesh_handle = (i as u32 * 2654435761) & 0xFFFF;
            c.entity_id = i as u32;
            c.sort_depth = (i as u32 * 1664525).wrapping_add(1013904223);
            c.src_blend = ((i % 4) as u8) + 5;
            c.dst_blend = ((i % 3) as u8) + 6;
            c.z_test = (i % 2) == 0;
            c.z_write = (i % 3) == 0;
            c.z_function = ((i % 8) as u8) + 1;
            v.push(c);
        }
        v
    }
    const ITERS: u32 = 50;
    for &n in &[400usize, 800, 1500, 2000, 3000, 5000, 10_000] {
        let mut serial_ns = 0u128;
        for _ in 0..ITERS {
            // Rebuild each iteration — DrawCommand isn't Clone, and
            // sort-in-place would otherwise leave a sorted vector that
            // skews subsequent iterations toward the best case.
            let mut v = make_inputs(n);
            let t0 = Instant::now();
            v.sort_unstable_by_key(draw_sort_key);
            serial_ns += t0.elapsed().as_nanos();
            std::hint::black_box(&v);
        }
        let mut par_ns = 0u128;
        for _ in 0..ITERS {
            let mut v = make_inputs(n);
            let t0 = Instant::now();
            v.par_sort_unstable_by_key(draw_sort_key);
            par_ns += t0.elapsed().as_nanos();
            std::hint::black_box(&v);
        }
        let serial = serial_ns / ITERS as u128;
        let par = par_ns / ITERS as u128;
        let winner = if serial < par { "serial" } else { "parallel" };
        let ratio = serial as f64 / par as f64;
        eprintln!(
            "N={:>6}  serial={:>8} ns  parallel={:>8} ns  ratio(s/p)={:>5.2}  winner={}",
            n, serial, par, ratio, winner
        );
    }
}

// ── Option B: !in_raster prefix sorts RT-only draws to the end ────
//
// When `MAX_INSTANCES` cap fires, the SSBO upload silently drops the
// trailing entries. The Option B sort-key prefix ensures the dropped
// entries are guaranteed to be off-frustum RT-only occluders — raster
// draws cluster at the front and are never affected by the cap.

/// In-frustum (in_raster=true) draws sort STRICTLY before off-frustum
/// (in_raster=false) draws regardless of every other key field.
#[test]
fn in_raster_draws_sort_strictly_before_rt_only() {
    // Off-frustum opaque with "best" other key fields (low entity id,
    // low mesh handle, etc.) vs in-frustum transparent with "worst"
    // other key fields. Despite the alpha_blend prefix that would
    // normally put transparent AFTER opaque, the in_raster bit
    // dominates → in_raster draws sort first.
    let mut rt_only_opaque = cmd(false, false, false);
    rt_only_opaque.in_raster = false;
    rt_only_opaque.mesh_handle = 0;
    rt_only_opaque.entity_id = 0;

    let mut in_raster_transparent = cmd(true, false, false);
    in_raster_transparent.in_raster = true;
    in_raster_transparent.mesh_handle = u32::MAX;
    in_raster_transparent.entity_id = u32::MAX;

    let mut cmds = vec![rt_only_opaque, in_raster_transparent];
    cmds.sort_by_key(draw_sort_key);

    assert!(
        cmds[0].in_raster,
        "in_raster=true must sort first; the cap-on-overflow at \
         MAX_INSTANCES should drop RT-only draws before any raster \
         draw — otherwise raster pixels disappear when the cap bites"
    );
    assert!(!cmds[1].in_raster);
}

/// Within the in_raster=true band, the legacy opaque-before-transparent
/// invariant must still hold (back-to-front blend correctness depends on
/// it).
#[test]
fn opaque_before_transparent_within_in_raster_band() {
    let mut opaque = cmd(false, false, false);
    opaque.in_raster = true;
    let mut transparent = cmd(true, false, false);
    transparent.in_raster = true;

    let mut cmds = vec![transparent, opaque];
    cmds.sort_by_key(draw_sort_key);
    assert!(
        !cmds[0].alpha_blend,
        "opaque must sort before transparent within the in_raster band"
    );
    assert!(cmds[1].alpha_blend);
}

/// Within the in_raster=false (RT-only) band, opaque-before-transparent
/// also holds — the secondary sort key still applies; we just relocated
/// the whole band to the end of the array. This preserves shader-
/// pipeline batching potential even on the dropped tail (in case the
/// cap doesn't bite on a given frame).
#[test]
fn opaque_before_transparent_within_rt_only_band() {
    let mut opaque = cmd(false, false, false);
    opaque.in_raster = false;
    let mut transparent = cmd(true, false, false);
    transparent.in_raster = false;

    let mut cmds = vec![transparent, opaque];
    cmds.sort_by_key(draw_sort_key);
    assert!(!cmds[0].alpha_blend);
    assert!(cmds[1].alpha_blend);
    // Both still off-frustum.
    assert!(!cmds[0].in_raster);
    assert!(!cmds[1].in_raster);
}

/// Sibling check: the !in_raster prefix is u8 (0 or 1), narrower than
/// the rest of the key, so promoting it to the front of the tuple
/// can't widen the comparator beyond what `par_sort_unstable_by_key`
/// already pays. This is a compile-time + behavioural check that the
/// sort still terminates with the expected ordering on a moderate
/// mixed batch.
#[test]
fn mixed_in_raster_and_rt_only_partition_is_stable() {
    let mut a = cmd(false, false, false);
    a.in_raster = true;
    a.entity_id = 1;
    let mut b = cmd(true, false, false);
    b.in_raster = false;
    b.entity_id = 2;
    let mut c = cmd(false, false, false);
    c.in_raster = false;
    c.entity_id = 3;
    let mut d = cmd(true, false, false);
    d.in_raster = true;
    d.entity_id = 4;

    // Insertion order intentionally interleaves bands.
    let mut cmds = vec![a, b, c, d];
    cmds.sort_by_key(draw_sort_key);

    // Expected order: in_raster=true (a, d), then in_raster=false (c, b).
    // Within in_raster=true: opaque(a) before transparent(d).
    // Within in_raster=false: opaque(c) before transparent(b).
    assert_eq!(
        cmds.iter().map(|x| x.entity_id).collect::<Vec<_>>(),
        vec![1, 4, 3, 2]
    );
}
