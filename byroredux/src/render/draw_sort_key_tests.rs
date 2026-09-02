use super::{draw_sort_key, sort_draw_commands, DrawCommand};

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
        wireframe: false,
        flat_shading: false,
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
        ior: 1.5,        // #1248
        subsurface: 0.0, // #1249
        sheen: 0.0,
        sheen_tint: 0.0,
        anisotropic: 0.0, // #1250
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
        greyscale_lut_index: 0,
        supplemental_texture_indices: [0; 16],
        translucency_subsurface_color: [0.0; 3],
        translucency_transmissive_scale: 0.0,
        translucency_turbulence: 0.0,
        shader_color: [0.0; 3],
        shader_float: 0.0,
        glass_fresnel_color: [1.0; 3],
        glass_refraction_scale: 0.05,
        glass_blur_scale: 0.4,
        glass_blur_scale_factor: 1.0,
        lighting_effect_1: 0.0,
        lighting_effect_2: 0.0,
        subsurface_rolloff: 0.0,
        rimlight_power: 0.0,
        backlight_power: 0.0,
        fresnel_power: 5.0,
        grayscale_to_palette_scale: 1.0,
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
///
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
    let mut cmds = [
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
    let mut cmds = [far, near];
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
    let mut cmds = [near, far];
    cmds.sort_by_key(draw_sort_key);
    assert_eq!(cmds[0].sort_depth, 900);
    assert_eq!(cmds[1].sort_depth, 100);
}

/// FO4 glass and the transparent surfaces visible through it frequently
/// occupy different render layers. Alpha-over compositing must remain
/// globally back-to-front across that state boundary; otherwise a distant
/// floor puddle/decal is submitted after the nearer glass and appears as a
/// rectangular patch on the dome.
#[test]
fn alpha_over_sorts_back_to_front_across_render_layers() {
    use byroredux_core::ecs::components::RenderLayer;

    let mut near_glass = cmd(true, false, true);
    near_glass.render_layer = RenderLayer::Architecture;
    near_glass.sort_depth = 100;
    near_glass.entity_id = 1;

    let mut far_puddle = cmd(true, true, false);
    far_puddle.render_layer = RenderLayer::Decal;
    far_puddle.sort_depth = 900;
    far_puddle.entity_id = 2;

    let mut cmds = [near_glass, far_puddle];
    cmds.sort_by_key(draw_sort_key);

    assert_eq!(cmds[0].entity_id, 2, "far alpha-over surface draws first");
    assert_eq!(cmds[1].entity_id, 1, "near glass composites last");
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
    additive_far.dst_blend = 0; // Gamebryo ONE == 0 — true additive
    additive_far.sort_depth = 900;
    let mut alpha_far = cmd(true, false, false);
    alpha_far.src_blend = 6;
    alpha_far.dst_blend = 7;
    alpha_far.sort_depth = 500;
    let mut cmds = [alpha_near, additive_far, alpha_far];
    cmds.sort_by_key(draw_sort_key);
    // Additive (dst=0) sorts before alpha (dst=7) by u32 order.
    // Both alpha draws land together, sorted back-to-front within.
    assert_eq!(cmds[0].dst_blend, 0);
    assert_eq!(cmds[1].dst_blend, 7);
    assert_eq!(cmds[1].sort_depth, 500);
    assert_eq!(cmds[2].dst_blend, 7);
    assert_eq!(cmds[2].sort_depth, 100);
}

/// Fire-refraction meshes are scene-composition proxies, not ordinary
/// alpha-over surfaces. They must replace/distort the opaque background
/// before both additive and alpha-over flame cards draw, regardless of
/// camera depth. Otherwise normal back-to-front sorting puts the nearer
/// haze plane last and it masks the flames it belongs to.
#[test]
fn fire_refraction_composes_between_opaque_and_effect_draws() {
    use byroredux_renderer::{MATERIAL_KIND_EFFECT_SHADER, MATERIAL_KIND_FIRE_REFRACTION};

    let mut opaque = cmd(false, false, false);
    opaque.entity_id = 1;

    let mut fire_refraction = cmd(true, false, true);
    fire_refraction.material_kind = MATERIAL_KIND_FIRE_REFRACTION;
    fire_refraction.z_write = false;
    fire_refraction.sort_depth = 100;
    fire_refraction.entity_id = 2;

    let mut additive_flame = cmd(true, false, true);
    additive_flame.material_kind = MATERIAL_KIND_EFFECT_SHADER;
    additive_flame.dst_blend = 0;
    additive_flame.sort_depth = 900;
    additive_flame.entity_id = 3;

    let mut alpha_over_flame = cmd(true, false, true);
    alpha_over_flame.material_kind = MATERIAL_KIND_EFFECT_SHADER;
    alpha_over_flame.dst_blend = 7;
    alpha_over_flame.sort_depth = 900;
    alpha_over_flame.entity_id = 4;

    let mut cmds = [alpha_over_flame, additive_flame, fire_refraction, opaque];
    cmds.sort_by_key(draw_sort_key);

    assert_eq!(
        cmds.iter().map(|c| c.entity_id).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "opaque scene, then heat haze, then all ordinary effect draws"
    );
}

/// Regression for #2237 (REN-D12-02): the fire-refraction composition-phase
/// override is scoped to `MATERIAL_KIND_EFFECT_SHADER` (its flame cards)
/// only. An unrelated alpha-blended transparent sharing screen space with a
/// fire-refraction proxy (e.g. a glass pane) must still sort by normal
/// back-to-front depth against the proxy, not be globally forced after it.
#[test]
fn fire_refraction_sorts_by_depth_against_unrelated_glass() {
    use byroredux_renderer::{MATERIAL_KIND_FIRE_REFRACTION, MATERIAL_KIND_GLASS};

    let mut fire_refraction = cmd(true, false, true);
    fire_refraction.material_kind = MATERIAL_KIND_FIRE_REFRACTION;
    fire_refraction.dst_blend = 7;
    fire_refraction.sort_depth = 100; // nearer
    fire_refraction.entity_id = 1;

    let mut glass_behind = cmd(true, false, true);
    glass_behind.material_kind = MATERIAL_KIND_GLASS;
    glass_behind.dst_blend = 7;
    glass_behind.sort_depth = 900; // farther — must draw first
    glass_behind.entity_id = 2;

    let mut cmds = [fire_refraction, glass_behind];
    cmds.sort_by_key(draw_sort_key);

    assert_eq!(
        cmds.iter().map(|c| c.entity_id).collect::<Vec<_>>(),
        vec![2, 1],
        "farther glass draws before the nearer fire-refraction proxy, by depth"
    );
}

/// Regression for #1649: additive (Gamebryo `dst_blend == ONE == 0`) is
/// order-independent, so same-mesh additive draws (e.g. an emitter's
/// particle billboards) must sort *contiguously by mesh* — not depth-
/// interleaved — so the CPU batch-merge can collapse them into one
/// instanced indirect draw. Two interleaved meshes at mixed depths must
/// come out grouped by mesh.
#[test]
fn additive_same_mesh_draws_stay_contiguous_for_instancing() {
    let depths = [100u32, 900, 300];
    let mut cmds = Vec::new();
    for (i, &d) in depths.iter().enumerate() {
        // Two meshes (7, 9) emitted interleaved at varying depths.
        for mesh in [7u32, 9u32] {
            let mut c = cmd(true, false, false);
            c.src_blend = 6;
            c.dst_blend = 0; // additive
            c.mesh_handle = mesh;
            c.sort_depth = d;
            c.entity_id = (i as u32) * 2 + mesh;
            cmds.push(c);
        }
    }
    cmds.sort_by_key(draw_sort_key);
    // All mesh-7 draws must be contiguous, then all mesh-9 draws — no
    // depth-driven interleaving that would break the same-mesh run the
    // batch-merge depends on.
    let meshes: Vec<u32> = cmds.iter().map(|c| c.mesh_handle).collect();
    assert_eq!(
        meshes,
        vec![7, 7, 7, 9, 9, 9],
        "additive draws group by mesh"
    );
}

/// Regression for #1994 (DIM2-01): the additive-blend branch must put the
/// pipeline-bind boundary (`depth_state`, which folds in `wireframe` —
/// D2-NEW-05 / #1806) *ahead* of `mesh_handle` in the sort key, mirroring
/// the Opaque branch. Pre-fix, `mesh_handle` sorted ahead of `depth_state`
/// for additive draws, so two meshes each present in both wireframe and
/// fill variants sorted as `meshA-fill, meshA-wire, meshB-fill, meshB-wire`
/// (4 pipeline binds) instead of `fill, fill, wire, wire` (2 binds).
#[test]
fn additive_wireframe_and_fill_draws_do_not_interleave_across_meshes() {
    let mut cmds = Vec::new();
    for mesh in [7u32, 9u32] {
        for wireframe in [false, true] {
            let mut c = cmd(true, false, false);
            c.src_blend = 6;
            c.dst_blend = 0; // additive
            c.mesh_handle = mesh;
            c.wireframe = wireframe;
            c.entity_id = mesh * 2 + wireframe as u32;
            cmds.push(c);
        }
    }
    cmds.sort_by_key(draw_sort_key);
    let flags: Vec<bool> = cmds.iter().map(|c| c.wireframe).collect();
    // Exactly one run per wireframe value — fill draws of both meshes
    // together, then wireframe draws of both meshes together (or vice
    // versa), never interleaved by mesh.
    let first = flags[0];
    let boundary = flags
        .iter()
        .position(|&w| w != first)
        .unwrap_or(flags.len());
    assert!(
        flags[boundary..].iter().all(|&w| w != first),
        "additive wireframe={} and wireframe={} draws must not interleave \
         across meshes; got {:?}",
        first,
        !first,
        flags,
    );
}

/// Counterpart to the above: true alpha-over (`dst_blend == 7`) is
/// order-dependent, so it must stay depth-sorted (back-to-front) even
/// when that splits a same-mesh run — correctness wins over batching.
#[test]
fn alpha_over_same_mesh_draws_stay_depth_sorted() {
    let depths = [100u32, 900, 300];
    let mut cmds = Vec::new();
    for (i, &d) in depths.iter().enumerate() {
        for mesh in [7u32, 9u32] {
            let mut c = cmd(true, false, false);
            c.src_blend = 6;
            c.dst_blend = 7; // alpha-over
            c.mesh_handle = mesh;
            c.sort_depth = d;
            c.entity_id = (i as u32) * 2 + mesh;
            cmds.push(c);
        }
    }
    cmds.sort_by_key(draw_sort_key);
    // Depth dominates mesh: larger sort_depth first (back-to-front).
    let order: Vec<u32> = cmds.iter().map(|c| c.sort_depth).collect();
    assert_eq!(order, vec![900, 900, 300, 300, 100, 100]);
}

/// Regression for #506: with ties in the 10-tuple prefix (same
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
/// scene-sized N. Rayon's `par_sort_unstable_by_key` loses to
/// `sort_unstable_by_key` on the closure-extracted key at typical
/// Bethesda draw counts (~800–1500).
///
/// #2173 — the key is now an **11-tuple** (`883f57cd` added the stable
/// surface ID); re-measuring with the wider key moved the crossover from
/// ~2K to ~3K, and `DRAW_SORT_PARALLEL_THRESHOLD` was updated to match.
/// Re-run this whenever `draw_sort_key`'s arity or field types change —
/// the threshold is calibration, not a constant with a principled value.
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
            c.mesh_handle = (i as u32).wrapping_mul(2654435761) & 0xFFFF;
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
    // #2173 — the 2000..3000 range is sampled finely because that is where
    // the crossover sits after the sort key widened from 10 to 11 tuples
    // (`883f57cd`); the original coarse list could only say "somewhere
    // between 2000 and 3000".
    for &n in &[
        400usize, 800, 1500, 2000, 2250, 2500, 2750, 3000, 5000, 10_000,
    ] {
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

/// #2681 (PERF-D2-04) — third arm: decorate-sort-undecorate. Extracts the
/// 11-tuple key once per element into a `Vec<(Key, usize)>` (a fraction of
/// `DrawCommand`'s ~480 bytes), sorts THAT by key (O(N) key extractions
/// instead of `sort_unstable_by_key`'s ~2·N·log₂N re-extractions from the
/// full struct on every comparison — the same transform #2034 already
/// applied to `collect_lights`), then applies the resulting permutation via
/// a `Vec<Option<DrawCommand>>` scratch buffer (`DrawCommand` isn't `Clone`,
/// so `.take()` is the obviously-correct way to move each element into its
/// sorted slot without an in-place cycle-walk).
///
/// Per the issue: **do not land the production change on this reasoning
/// alone** — only if this arm measurably wins over `manual_bench_draw_sort_
/// serial_vs_parallel`'s serial/parallel arms at the N=1800–3400 range the
/// runtime baselines actually occupy (see PERF-D2-03).
///
/// **Measured outcome (2026-08-13, this repo's dev hardware, two
/// independent runs): the hypothesis does NOT hold.** Decorate-sort-
/// undecorate is 2–3× SLOWER than the current `sort_unstable_by_key`
/// baseline across nearly the entire swept range (N=400..2750, and again at
/// N=5000/10000), reproducibly. It only "wins" at N=3000 and N=3400
/// specifically, and that pair doesn't fit a real algorithmic crossover —
/// both neighboring points (N=2750 below, N=5000 above) favor the baseline
/// heavily, and `decorate_serial` itself drops implausibly between N=2750
/// and N=3400 with no corresponding rise afterward, consistent with a
/// measurement artifact (scheduler/thermal/cache noise) rather than a
/// signal. **Conclusion: the production sort is NOT changed.** The
/// mechanism reasoning in the issue (per-comparison key extraction
/// dominates) doesn't predict what's actually measured here, most likely
/// because `draw_sort_key`'s ~10 touched fields sit within the first couple
/// of `DrawCommand` cache lines rather than scattered across all ~8, and/or
/// the optimizer inlines the extraction into the comparator well enough
/// that the un-decorated approach's per-swap cost (large-struct `ptr::swap`
/// during the sort itself, same either way at this element count) dominates
/// over decorate's saved extractions. This bench is kept as the permanent,
/// re-runnable falsification — re-run it (not the reasoning) before ever
/// revisiting this optimization.
///
/// `cargo test -p byroredux --release manual_bench_draw_sort_decorate_sort_undecorate -- --ignored --nocapture`.
#[test]
#[ignore]
fn manual_bench_draw_sort_decorate_sort_undecorate() {
    use rayon::prelude::*;
    use std::time::Instant;
    fn make_inputs(n: usize) -> Vec<DrawCommand> {
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            let mut c = cmd((i % 7) == 0, (i % 13) == 0, (i % 5) == 0);
            c.mesh_handle = (i as u32).wrapping_mul(2654435761) & 0xFFFF;
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
    fn decorate_sort_undecorate(v: Vec<DrawCommand>, parallel: bool) -> Vec<DrawCommand> {
        let mut keyed: Vec<_> = v
            .iter()
            .enumerate()
            .map(|(i, c)| (draw_sort_key(c), i))
            .collect();
        if parallel {
            keyed.par_sort_unstable_by_key(|&(k, _)| k);
        } else {
            keyed.sort_unstable_by_key(|&(k, _)| k);
        }
        let mut slots: Vec<Option<DrawCommand>> = v.into_iter().map(Some).collect();
        keyed
            .iter()
            .map(|&(_, idx)| slots[idx].take().expect("each index visited exactly once"))
            .collect()
    }
    const ITERS: u32 = 50;
    for &n in &[
        400usize, 800, 1500, 1800, 2000, 2250, 2500, 2750, 3000, 3400, 5000, 10_000,
    ] {
        let mut baseline_ns = 0u128;
        for _ in 0..ITERS {
            let mut v = make_inputs(n);
            let t0 = Instant::now();
            v.sort_unstable_by_key(draw_sort_key);
            baseline_ns += t0.elapsed().as_nanos();
            std::hint::black_box(&v);
        }
        let mut decorate_serial_ns = 0u128;
        for _ in 0..ITERS {
            let v = make_inputs(n);
            let t0 = Instant::now();
            let sorted = decorate_sort_undecorate(v, false);
            decorate_serial_ns += t0.elapsed().as_nanos();
            std::hint::black_box(&sorted);
        }
        let mut decorate_par_ns = 0u128;
        for _ in 0..ITERS {
            let v = make_inputs(n);
            let t0 = Instant::now();
            let sorted = decorate_sort_undecorate(v, true);
            decorate_par_ns += t0.elapsed().as_nanos();
            std::hint::black_box(&sorted);
        }
        let baseline = baseline_ns / ITERS as u128;
        let dec_serial = decorate_serial_ns / ITERS as u128;
        let dec_par = decorate_par_ns / ITERS as u128;
        let best_decorate = dec_serial.min(dec_par);
        let ratio = baseline as f64 / best_decorate as f64;
        let winner = if baseline <= best_decorate {
            "baseline"
        } else if dec_serial <= dec_par {
            "decorate-serial"
        } else {
            "decorate-parallel"
        };
        eprintln!(
            "N={:>6}  baseline={:>8} ns  decorate_serial={:>8} ns  decorate_par={:>8} ns  \
             ratio(baseline/best_decorate)={:>5.2}  winner={}",
            n, baseline, dec_serial, dec_par, ratio, winner
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

    let mut cmds = [rt_only_opaque, in_raster_transparent];
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

    let mut cmds = [transparent, opaque];
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

    let mut cmds = [transparent, opaque];
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
    let mut cmds = [a, b, c, d];
    cmds.sort_by_key(draw_sort_key);

    // Expected order: in_raster=true (a, d), then in_raster=false (c, b).
    // Within in_raster=true: opaque(a) before transparent(d).
    // Within in_raster=false: opaque(c) before transparent(b).
    assert_eq!(
        cmds.iter().map(|x| x.entity_id).collect::<Vec<_>>(),
        vec![1, 4, 3, 2]
    );
}

/// The production path only sorts draws that can produce raster batches.
/// RT-only occluders must remain in the instance/TLAS tail, but their
/// pipeline/mesh/depth ordering has no raster consumer.
#[test]
fn production_sort_orders_only_the_raster_prefix() {
    let mut rt_opaque = cmd(false, false, false);
    rt_opaque.in_raster = false;
    rt_opaque.entity_id = 10;

    let mut raster_transparent = cmd(true, false, false);
    raster_transparent.entity_id = 20;

    let mut rt_transparent = cmd(true, false, false);
    rt_transparent.in_raster = false;
    rt_transparent.entity_id = 30;

    let mut raster_opaque = cmd(false, false, false);
    raster_opaque.entity_id = 40;

    let mut draws = vec![rt_opaque, raster_transparent, rt_transparent, raster_opaque];
    let raster_len = sort_draw_commands(&mut draws);

    assert_eq!(raster_len, 2);
    assert!(draws[..raster_len].iter().all(|draw| draw.in_raster));
    assert!(draws[raster_len..].iter().all(|draw| !draw.in_raster));
    assert_eq!(draws[0].entity_id, 40, "opaque raster draw sorts first");
    assert_eq!(
        draws[1].entity_id, 20,
        "transparent raster draw sorts after opaque"
    );
}

#[test]
fn production_sort_handles_an_rt_only_frame() {
    let mut first = cmd(false, false, false);
    first.in_raster = false;
    let mut second = cmd(true, false, false);
    second.in_raster = false;
    let mut draws = vec![first, second];

    assert_eq!(sort_draw_commands(&mut draws), 0);
    assert!(draws.iter().all(|draw| !draw.in_raster));
}

// ── D2-NEW-05 / #1806 — wireframe is a PipelineKey axis, so the sort
// key must cluster by it too, or a wireframe draw lands mid-run among
// fill draws and forces an extra `cmd_bind_pipeline` per flip.

/// Two draws identical in every other field must sort into different
/// clusters when only `wireframe` differs — otherwise the sort key
/// can't tell `PipelineKey::Opaque { wireframe: true }` apart from
/// `PipelineKey::Opaque { wireframe: false }` and the batch-merge pass
/// would try to extend a batch across the pipeline-bind boundary.
#[test]
fn wireframe_flag_changes_the_sort_key() {
    let fill = cmd(false, false, false);
    let mut wire = cmd(false, false, false);
    wire.wireframe = true;
    assert_ne!(
        draw_sort_key(&fill),
        draw_sort_key(&wire),
        "wireframe must be a distinguishing sort-key axis (D2-NEW-05 / #1806)"
    );
}

/// Regression for #1806: same-mesh draws that alternate `wireframe`
/// true/false must sort into two contiguous runs (all wireframe first
/// or all fill first — either is fine, as long as they aren't
/// interleaved), matching the same batch-clustering property the
/// existing additive/mesh tests pin for the other `PipelineKey` axes.
/// Pre-fix, `draw_sort_key` never read `cmd.wireframe`, so these would
/// interleave in whatever order the other fields (all tied here)
/// happened to break ties.
#[test]
fn wireframe_and_fill_draws_of_same_mesh_do_not_interleave() {
    let mut cmds = Vec::new();
    for i in 0..6u32 {
        let mut c = cmd(false, false, false);
        c.mesh_handle = 42;
        c.wireframe = (i % 2) == 0;
        c.entity_id = i;
        cmds.push(c);
    }
    cmds.sort_by_key(draw_sort_key);

    let flags: Vec<bool> = cmds.iter().map(|c| c.wireframe).collect();
    // Find the boundary and assert everything before it shares one
    // value and everything after shares the other — i.e. exactly one
    // run per wireframe value, never more.
    let first = flags[0];
    let boundary = flags
        .iter()
        .position(|&w| w != first)
        .unwrap_or(flags.len());
    assert!(
        flags[boundary..].iter().all(|&w| w != first),
        "wireframe={} and wireframe={} draws must not interleave; got {:?}",
        first,
        !first,
        flags,
    );
}
