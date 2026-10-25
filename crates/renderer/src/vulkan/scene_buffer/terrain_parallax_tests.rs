//! #6197 — `terrainParallaxDisplaceUV` (`material_sampling.glsl`).
//!
//! Like the height blend's tests, these run a CPU mirror of the shader
//! function (same arithmetic, step for step) against a synthetic height
//! field, and pin the GLSL lines the mirror stands for.

type V3 = [f32; 3];
type V2 = [f32; 2];

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn sub3(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale3(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[derive(Clone, Copy)]
struct Tile {
    depth: f32,
    fade_start: f32,
    fade_end: f32,
}

/// `mix(maxSteps, minSteps, clamp(vz, 0, 1)) + 0.5` truncated.
fn step_count(vz: f32) -> i32 {
    let t = vz.clamp(0.0, 1.0);
    // Rust's `as i32` saturates (NaN -> 0); the GLSL conversion is undefined for NaN, so the
    // shader clamps the integer and this mirrors the clamp (#6346).
    ((20.0 + (6.0 - 20.0) * t + 0.5) as i32).clamp(6, 20)
}

/// Mirror of `parallaxSecantWeight` (`ray_hit.glsl`).
fn secant_weight(after: f32, before: f32) -> f32 {
    let span = after - before;
    after / if span.abs() < 1.0e-6 { -1.0e-6 } else { span }
}

fn fade_of(tile: Tile, distance: f32) -> f32 {
    if tile.fade_end > tile.fade_start {
        let t = ((distance - tile.fade_start) / (tile.fade_end - tile.fade_start)).clamp(0.0, 1.0);
        1.0 - t * t * (3.0 - 2.0 * t)
    } else if distance < tile.fade_end {
        1.0
    } else {
        0.0
    }
}

/// CPU mirror of `terrainParallaxDisplaceUV`. `height(uv)` stands for
/// `byroTerrainBlendedHeight` (negative = no height-mapped layer in play).
#[allow(clippy::too_many_arguments)]
fn displace(
    uv: V2,
    d_uv_dx: V2,
    d_uv_dy: V2,
    d_p_dx: V3,
    d_p_dy: V3,
    view: V3,
    n: V3,
    distance: f32,
    tile: Tile,
    height: impl Fn(V2) -> f32,
) -> V2 {
    let relief = tile.depth * fade_of(tile, distance);
    if relief <= 0.0 {
        return uv;
    }
    let frame_n = if dot(view, n) >= 0.0 { n } else { scale3(n, -1.0) };
    let vz = dot(view, frame_n);
    let lateral = scale3(sub3(view, scale3(frame_n, vz)), 1.0 / vz.max(0.05));

    let g11 = dot(d_p_dx, d_p_dx);
    let g12 = dot(d_p_dx, d_p_dy);
    let g22 = dot(d_p_dy, d_p_dy);
    let det = g11 * g22 - g12 * g12;
    if det <= 1.0e-6 * g11 * g22 || g11 * g22 <= 1.0e-12 {
        return uv;
    }
    let r1 = dot(lateral, d_p_dx);
    let r2 = dot(lateral, d_p_dy);
    let a = (g22 * r1 - g12 * r2) / det;
    let b = (g11 * r2 - g12 * r1) / det;
    let planar = [
        (a * d_uv_dx[0] + b * d_uv_dy[0]) * relief,
        (a * d_uv_dx[1] + b * d_uv_dy[1]) * relief,
    ];

    let steps = step_count(vz);
    let layer_depth = 1.0 / steps as f32;
    let delta = [planar[0] / steps as f32, planar[1] / steps as f32];

    let h0 = height(uv);
    if h0 < 0.0 {
        return uv;
    }
    let mut current = uv;
    let mut current_depth = 0.0f32;
    let mut surface = 1.0 - h0;
    let mut prev_surface = surface;
    let mut taken = 0;
    for _ in 0..steps {
        if current_depth >= surface {
            break;
        }
        prev_surface = surface;
        current = [current[0] - delta[0], current[1] - delta[1]];
        current_depth += layer_depth;
        surface = 1.0 - height(current).max(0.0);
        taken += 1;
    }
    if taken == 0 {
        return uv;
    }
    let prev = [current[0] + delta[0], current[1] + delta[1]];
    let after = surface - current_depth;
    let before = prev_surface - (current_depth - layer_depth);
    let weight = secant_weight(after, before).clamp(0.0, 1.0);
    [
        current[0] + (prev[0] - current[0]) * weight,
        current[1] + (prev[1] - current[1]) * weight,
    ]
}

const UP: V3 = [0.0, 1.0, 0.0];
const TILE: Tile = Tile { depth: 100.0, fade_start: 700.0, fade_end: 1400.0 };

/// A camera-facing quad on flat ground: screen derivatives spanning the
/// ground plane, with `map` the world→UV mapping (rows = u, v over x, z).
fn quad(map: [[f32; 2]; 2]) -> (V2, V2, V3, V3) {
    let d_p_dx: V3 = [0.10, 0.0, 0.02];
    let d_p_dy: V3 = [0.01, 0.0, 0.12];
    let uv_of = |p: V3| -> V2 {
        [
            map[0][0] * p[0] + map[0][1] * p[2],
            map[1][0] * p[0] + map[1][1] * p[2],
        ]
    };
    (uv_of(d_p_dx), uv_of(d_p_dy), d_p_dx, d_p_dy)
}

fn view_from(dir: V3) -> V3 {
    let len = dot(dir, dir).sqrt();
    scale3(dir, 1.0 / len)
}

fn close(a: V2, b: V2, eps: f32) -> bool {
    (a[0] - b[0]).abs() <= eps && (a[1] - b[1]).abs() <= eps
}

#[test]
fn a_surface_at_the_top_does_not_move_the_uv() {
    let (dx, dy, px, py) = quad([[0.001, 0.0], [0.0, 0.001]]);
    let uv = [3.25, 1.5];
    let out = displace(uv, dx, dy, px, py, view_from([0.4, 0.5, 0.3]), UP, 300.0, TILE, |_| 1.0);
    assert_eq!(out, uv);
}

/// Flat ground sunk to half the relief: the hit sits at depth 0.5, so the UV
/// slides by exactly half of the full-depth slide (the step grid lands on it).
#[test]
fn a_flat_sunken_surface_slides_the_uv_by_its_depth() {
    let k = 0.001;
    let (dx, dy, px, py) = quad([[k, 0.0], [0.0, k]]);
    let view = view_from([0.4, 0.5, 0.3]);
    let uv = [3.25, 1.5];
    let out = displace(uv, dx, dy, px, py, view, UP, 300.0, TILE, |_| 0.5);

    // Expected: the view ray descends away from the camera, `lateral` = the
    // horizontal part of `view` over `vz`; half of `relief` deep.
    let lateral = [view[0] / view[1], view[2] / view[1]];
    let relief = TILE.depth * fade_of(TILE, 300.0);
    let want = [
        uv[0] - 0.5 * relief * k * lateral[0],
        uv[1] - 0.5 * relief * k * lateral[1],
    ];
    assert!(close(out, want, 1.0e-6), "{out:?} vs {want:?}");
}

/// The slide is read off the screen-space Jacobian, so it must come out the
/// same world-space displacement for a mirrored or axis-swapped UV mapping —
/// the case a hand-built tangent frame can get the handedness of wrong.
#[test]
fn the_slide_follows_the_view_ray_whatever_the_uv_orientation() {
    let k = 0.0013;
    let view = view_from([-0.3, 0.6, 0.5]);
    let lateral = [view[0] / view[1], view[2] / view[1]];
    let relief = TILE.depth * fade_of(TILE, 500.0);
    for (name, map) in [
        ("u=x v=z", [[k, 0.0], [0.0, k]]),
        ("mirrored u", [[-k, 0.0], [0.0, k]]),
        ("mirrored v", [[k, 0.0], [0.0, -k]]),
        ("axes swapped", [[0.0, k], [k, 0.0]]),
        ("rotated", [[0.8 * k, 0.6 * k], [-0.6 * k, 0.8 * k]]),
    ] {
        let (dx, dy, px, py) = quad(map);
        let uv = [10.0, -4.0];
        // Fully sunk (height 0): the hit is at depth 1, the whole slide.
        let out = displace(uv, dx, dy, px, py, view, UP, 500.0, TILE, |_| 0.0);
        let slide = [
            relief * (map[0][0] * lateral[0] + map[0][1] * lateral[1]),
            relief * (map[1][0] * lateral[0] + map[1][1] * lateral[1]),
        ];
        let want = [uv[0] - slide[0], uv[1] - slide[1]];
        assert!(close(out, want, 1.0e-5), "{name}: {out:?} vs {want:?}");
    }
}

#[test]
fn the_effect_fades_out_with_distance_and_ends_at_the_range() {
    let (dx, dy, px, py) = quad([[0.001, 0.0], [0.0, 0.001]]);
    let view = view_from([0.4, 0.5, 0.3]);
    let uv = [3.25, 1.5];
    let shift = |distance: f32| {
        let out = displace(uv, dx, dy, px, py, view, UP, distance, TILE, |_| 0.0);
        ((out[0] - uv[0]).powi(2) + (out[1] - uv[1]).powi(2)).sqrt()
    };
    assert_eq!(shift(TILE.fade_end), 0.0, "plain shading at the range");
    assert_eq!(shift(TILE.fade_end + 500.0), 0.0);
    let full = shift(TILE.fade_start);
    assert!(full > 0.0);
    assert_eq!(shift(10.0), full, "full strength inside the fade start");
    let mut last = full;
    for i in 1..=10 {
        let d = TILE.fade_start + (TILE.fade_end - TILE.fade_start) * i as f32 / 10.0;
        let s = shift(d);
        assert!(s <= last + 1.0e-7, "monotone fade at {d}: {s} > {last}");
        last = s;
    }
    // A degenerate range is a hard cut, never an undefined smoothstep.
    let cut = Tile { fade_start: 900.0, fade_end: 900.0, ..TILE };
    assert!(fade_of(cut, 899.0) == 1.0 && fade_of(cut, 900.0) == 0.0);
}

#[test]
fn depth_zero_missing_heights_and_degenerate_quads_leave_the_uv_alone() {
    let (dx, dy, px, py) = quad([[0.001, 0.0], [0.0, 0.001]]);
    let view = view_from([0.4, 0.5, 0.3]);
    let uv = [3.25, 1.5];
    let off = Tile { depth: 0.0, ..TILE };
    assert_eq!(displace(uv, dx, dy, px, py, view, UP, 100.0, off, |_| 0.0), uv);
    assert_eq!(displace(uv, dx, dy, px, py, view, UP, 100.0, TILE, |_| -1.0), uv);
    // Parallel screen derivatives: no basis to read the slide from.
    assert_eq!(displace(uv, dx, dy, px, scale3(px, 2.0), view, UP, 100.0, TILE, |_| 0.0), uv);
}

/// #6346 — the loop bound holds for a NaN `vz` (a zero interpolated normal), and the secant weight
/// stays finite where `after - before + 1e-6` was exactly 0/0 — without moving an ordinary pixel.
#[test]
fn degenerate_inputs_keep_the_march_bounded_and_finite() {
    assert!((6..=20).contains(&step_count(f32::NAN)));
    assert!((6..=20).contains(&step_count(f32::INFINITY)));
    // The old form: span of exactly -1e-6 made the denominator 0.
    let (after, before) = (-0.5e-6f32, 0.5e-6f32);
    assert!(secant_weight(after, before).is_finite());
    assert!(secant_weight(0.0, 0.0).is_finite());
    // An ordinary crossing and an ordinary no-crossing are the plain ratio.
    assert!((secant_weight(-0.1, 0.3) - 0.25).abs() < 1e-6);
    assert!((secant_weight(0.5, 0.3) - 2.5).abs() < 1e-5);
}

#[test]
fn steps_grow_toward_grazing_angles() {
    assert_eq!(step_count(1.0), 6, "looking straight down");
    assert_eq!(step_count(0.05), 19, "grazing limit (vz is clamped at 0.05 for the slide only)");
    assert_eq!(step_count(0.0), 20);
    let mut last = 0;
    for i in (0..=10).rev() {
        let s = step_count(i as f32 / 10.0);
        assert!(s >= last, "steps must not fall as the view flattens");
        last = s;
    }
}

// ── The GLSL carries what the mirror stands for ─────────────────────────

fn function_body<'a>(src: &'a str, header: &str) -> &'a str {
    let body = src
        .split_once(header)
        .unwrap_or_else(|| panic!("`{header}` must stay in the shader source"))
        .1;
    &body[..body.find("\n}\n").expect("function end")]
}

/// The march sits under a data-dependent `break`; implicit derivatives there
/// are undefined (the class #3622 / #4016 fixed), so the function may only
/// use the gradients it is handed.
#[test]
fn glsl_parallax_uses_only_explicit_gradients() {
    let src = include_str!("../../../shaders/include/material_sampling.glsl");
    let body = function_body(src, "vec2 terrainParallaxDisplaceUV(");
    for banned in ["dFdx(", "dFdy(", "fwidth(", "texture(", "textureQueryLod("] {
        assert!(
            !body.contains(banned),
            "terrainParallaxDisplaceUV reads `{banned}` — implicit derivatives are undefined \
             under its data-dependent break; take them as parameters (#4016)"
        );
    }
    for line in [
        "byroTerrainBlendedHeight(splat0, splat1, tile, uv, dUVdx, dUVdy, band)",
        "byroTerrainBlendedHeight(splat0, splat1, tile, currentUV, dUVdx, dUVdy, band)",
    ] {
        assert!(body.contains(line), "terrainParallaxDisplaceUV lost `{line}`");
    }
    for line in [
        "1.0 - smoothstep(tile.parallaxFadeStart, tile.parallaxFadeEnd, cameraDistance)",
        "const float minSteps = 6.0;",
        "const float maxSteps = 20.0;",
        "int steps = int(mix(maxSteps, minSteps, clamp(vz, 0.0, 1.0)) + 0.5);",
        "steps = clamp(steps, int(minSteps), int(maxSteps));",
        "currentUV -= deltaUV;",
    ] {
        assert!(body.contains(line), "terrainParallaxDisplaceUV lost `{line}` (#6197)");
    }
}

/// #6346 — all three marchers take their secant weight from the one guarded helper; none keeps the
/// `+ 1e-6` form that is 0/0 at a span of exactly -1e-6.
#[test]
fn every_parallax_march_uses_the_guarded_secant_weight() {
    for (name, src) in [
        ("material_sampling.glsl", include_str!("../../../shaders/include/material_sampling.glsl")),
        ("ray_hit.glsl", include_str!("../../../shaders/include/ray_hit.glsl")),
    ] {
        let code = squashed(src);
        assert!(!code.contains("afterDepth - beforeDepth + 1"), "{name} still divides by the nudged span");
        assert!(
            code.matches("parallaxSecantWeight(afterDepth, beforeDepth)").count()
                >= if name == "material_sampling.glsl" { 2 } else { 1 },
            "{name} lost a parallaxSecantWeight call"
        );
    }
}

/// Collapse whitespace so a pin does not depend on how a call is wrapped.
fn squashed(code: &str) -> String {
    code.lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// #6345 — the march rewrites `sampleUV` for terrain fragments, so every
/// base-layer read after it must take the gradients captured *before* it
/// (`splatUVdx/dy`, `splatPosdx/dy`). An implicit `texture()` or `perturbNormal`
/// differentiates the displaced UV, whose derivative jumps where neighbouring
/// quad lanes stop on different march layers: a too-coarse mip and a rotated
/// tangent frame on the base layer, which the (explicit) splat layers did not
/// share. For every other fragment the captured gradients are the derivatives
/// of the same `sampleUV`, so the explicit form is the same read.
#[test]
fn triangle_frag_reads_the_terrain_base_layer_with_the_pre_march_gradients() {
    let frag = squashed(include_str!("../../../shaders/triangle.frag"));
    for required in [
        "textureGrad( textures[nonuniformEXT(fragTexIndex)], sampleUV, splatUVdx, splatUVdy)",
        "textureGrad( textures[nonuniformEXT(handle)], sampleUV, splatUVdx, splatUVdy)",
        "textureGrad( textures[nonuniformEXT(mat.specularMapIndex)], sampleUV, splatUVdx, splatUVdy )",
        "textureGrad( textures[nonuniformEXT(mat.reflectanceMapIndex)], sampleUV, splatUVdx, splatUVdy )",
        "textureGrad( textures[nonuniformEXT(normalMapIdx)], sampleUV, splatUVdx, splatUVdy)",
        "perturbNormalGrad( N, sampleUV, normalMapIdx, fragTangent, splatPosdx, splatPosdy, splatUVdx, splatUVdy)",
    ] {
        assert!(
            frag.contains(required),
            "triangle.frag lost the explicit-gradient base-layer read `{required}` (#6345)"
        );
    }
    // The implicit forms must not come back on those textures. `texture(` is
    // not a prefix of `textureGrad(`, so these needles match only the bad form.
    for banned in [
        "texture(textures[nonuniformEXT(fragTexIndex)]",
        "texture(textures[nonuniformEXT(handle)]",
        "texture(textures[nonuniformEXT(mat.specularMapIndex)]",
        "texture(textures[nonuniformEXT(mat.reflectanceMapIndex)]",
        "texture(textures[nonuniformEXT(normalMapIdx)]",
        "perturbNormal(N",
    ] {
        assert!(
            !frag.contains(banned),
            "triangle.frag reads `{banned}` with implicit derivatives of a UV the terrain \
             parallax march may have displaced (#6345); use the captured splat gradients"
        );
    }
}

/// The call site: gradients captured in quad-uniform flow first, the effect
/// gated on the per-tile depth and the POM bypass, and run *before* the blend
/// so the blend and every splat loop read the displaced UV.
#[test]
fn triangle_frag_displaces_before_the_blend_with_the_painted_weights() {
    let frag = include_str!("../../../shaders/triangle.frag");
    let grads = frag
        .find("vec2 splatUVdx = dFdx(sampleUV);")
        .expect("splat gradients");
    let call = frag
        .find("sampleUV = terrainParallaxDisplaceUV(")
        .expect("triangle.frag must call terrainParallaxDisplaceUV");
    let blend = frag
        .find("byroTerrainHeightBlend(\n")
        .or_else(|| frag.find("byroTerrainHeightBlend("))
        .expect("blend call");
    assert!(grads < call, "the gradients must be captured before the march");
    assert!(call < blend, "the displaced UV must exist before the height blend runs");
    let site = &frag[call.saturating_sub(400)..call + 500];
    assert!(site.contains("terrainTile.parallaxDepth > 0.0"), "{site}");
    assert!(site.contains("(dbgFlags & DBG_BYPASS_POM) == 0u"), "{site}");
    assert!(
        site.contains("terrainSplat[0], terrainSplat[1], terrainTile"),
        "the march is driven by the painted weights, before the blend rewrites them"
    );
}
