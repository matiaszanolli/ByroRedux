//! #6196 / #6202 / #6203 — `byroTerrainHeightBlend` (`bindings.glsl`).
//!
//! The blend needs a Vulkan device to run as written, so these tests run a
//! CPU mirror of it (the same arithmetic, lane for lane) and pin the GLSL
//! lines the mirror stands for, so neither can move without the other.

const LAYERS: usize = 8;
const LANES: usize = LAYERS + 1;

/// CPU mirror of `byroTerrainHeightBlend`. `w` are the ordered-`mix` splat
/// weights, `heights[i]` the height sample of lane `i` (`None` = no height
/// map; index `LAYERS` is the BTXT base). Returns the blended weights.
fn height_blend(w: [f32; LAYERS], heights: [Option<f32>; LANES], band: f32) -> [f32; LAYERS] {
    let mut share = [0.0f32; LANES];
    let mut rest = 1.0f32;
    for i in (0..LAYERS).rev() {
        let wi = w[i].clamp(0.0, 1.0);
        share[i] = wi * rest;
        rest *= 1.0 - wi;
    }
    share[LAYERS] = rest;

    let mut h = [0.0f32; LANES];
    let mut has_height = [false; LANES];
    let mut count = 0;
    let mut height_share = 0.0f32;
    for i in 0..LANES {
        has_height[i] = heights[i].is_some() && share[i] > 0.0;
        if has_height[i] {
            h[i] = heights[i].unwrap();
            height_share += share[i];
            count += 1;
        }
    }
    if count < 2 || height_share <= 1.0e-6 {
        return w;
    }

    let fade_share = 1.0 / LANES as f32;
    let mut fade = [0.0f32; LANES];
    let mut top = -1.0e30f32;
    for i in 0..LANES {
        if has_height[i] {
            let rel = share[i] / height_share;
            fade[i] = (rel / fade_share).min(1.0);
            h[i] = fade[i] * h[i] + rel;
            top = top.max(h[i]);
        }
    }
    let floor_score = top - band.max(1.0e-4);
    let mut total = 0.0f32;
    for i in 0..LANES {
        if has_height[i] {
            h[i] = fade[i] * (h[i] - floor_score).max(0.0);
            total += h[i];
        }
    }
    for i in 0..LANES {
        if has_height[i] {
            share[i] = height_share * h[i] / total;
        }
    }

    let mut out = [0.0f32; LAYERS];
    let mut composed = share[LAYERS];
    for i in 0..LAYERS {
        composed += share[i];
        out[i] = if composed > 1.0e-6 {
            share[i] / composed
        } else {
            0.0
        };
    }
    out
}

/// Ordered-`mix` weights → per-lane surface shares (base last).
fn shares(w: [f32; LAYERS]) -> [f32; LANES] {
    let mut share = [0.0f32; LANES];
    let mut rest = 1.0f32;
    for i in (0..LAYERS).rev() {
        share[i] = w[i] * rest;
        rest *= 1.0 - w[i];
    }
    share[LAYERS] = rest;
    share
}

fn one_layer(w0: f32) -> [f32; LAYERS] {
    let mut w = [0.0; LAYERS];
    w[0] = w0;
    w
}

fn heights(layer0: f32, base: f32) -> [Option<f32>; LANES] {
    let mut h = [None; LANES];
    h[0] = Some(layer0);
    h[LAYERS] = Some(base);
    h
}

/// #6202 — the painted layer's blended share must tend to 0 with its
/// painted weight. Before the fade, `h_layer = 1, h_base = 0, band 0.2`
/// gave 0.500 at `w = 1e-4` and 0.000 at `w = 0`: a hard seam on the
/// triangle edge where the paint reaches zero.
#[test]
fn a_vanishing_layer_fades_out_continuously() {
    for (h_layer, h_base, band) in [
        (1.0, 0.0, 0.2),
        (0.95, 0.1, 0.2),
        (0.7, 0.3, 1.0),
        (1.0, 0.0, 1.0),
    ] {
        let at_zero = shares(height_blend(one_layer(0.0), heights(h_layer, h_base), band))[0];
        assert_eq!(at_zero, 0.0);
        for w in [1.0e-2f32, 1.0e-3, 1.0e-4, 1.0e-6] {
            let s = shares(height_blend(one_layer(w), heights(h_layer, h_base), band))[0];
            // Faded by `w / (S·k)` = 9w, times an in-band part over a total
            // that the unfaded base already fills to `band`: at most 9w.
            assert!(
                s <= 9.0 * w + 1.0e-6,
                "h=({h_layer}, {h_base}) band {band}: share {s} at w={w} does not vanish with w"
            );
        }
    }
}

/// #6202 — the base reaches share 0 under a fully opaque layer; the base's
/// blended share must vanish there too, not jump from band-sized to 0.
#[test]
fn the_base_fades_out_under_a_fully_opaque_layer() {
    let h = heights(0.0, 1.0);
    assert_eq!(shares(height_blend(one_layer(1.0), h, 0.2))[LAYERS], 0.0);
    for eps in [1.0e-2f32, 1.0e-4, 1.0e-6] {
        let base = shares(height_blend(one_layer(1.0 - eps), h, 0.2))[LAYERS];
        assert!(
            base <= 9.0 * eps + 1.0e-6,
            "base share {base} at 1 − w = {eps}"
        );
    }
}

/// #6202 — the faded lane can never be the band's only survivor. Two
/// painted halves with low height and a vanishing third lane with full
/// height: an unfaded score `1 + ε` alone in the band would take the whole
/// height share (100 % → 0 at ε = 0).
#[test]
fn a_vanishing_tall_lane_never_takes_the_whole_surface() {
    let mut h = [None; LANES];
    h[0] = Some(0.0);
    h[1] = Some(1.0);
    h[LAYERS] = Some(0.0);
    for eps in [1.0e-2f32, 1.0e-4, 1.0e-6] {
        let mut w = [0.0; LAYERS];
        // Layer 1 (top) at ε; layer 0 over the base at half.
        w[0] = 0.5;
        w[1] = eps;
        let s = shares(height_blend(w, h, 0.2));
        assert!(s[1] <= 9.0 * eps + 1.0e-6, "vanishing lane kept {}", s[1]);
        assert!(
            (s[0] - 0.5).abs() < 0.05 && (s[LAYERS] - 0.5).abs() < 0.05,
            "{s:?}"
        );
    }
}

/// Away from the rims (every lane at or above `S / lanes`) the fade is 1
/// and the blend is the plain `height + share` rule: the higher layer wins
/// a 50/50 overlap outright when it clears the band, and splits it evenly
/// when the heights tie.
#[test]
fn unfaded_lanes_keep_the_height_plus_share_rule() {
    let won = shares(height_blend(one_layer(0.5), heights(1.0, 0.0), 0.2));
    assert!(
        (won[0] - 1.0).abs() < 1.0e-5 && won[LAYERS].abs() < 1.0e-5,
        "{won:?}"
    );
    let tied = shares(height_blend(one_layer(0.5), heights(0.4, 0.4), 0.2));
    assert!((tied[0] - 0.5).abs() < 1.0e-5, "{tied:?}");
}

/// The shares still sum to 1, and a lane without a height map keeps its
/// exact share whatever the height-mapped lanes do.
#[test]
fn shares_sum_to_one_and_heightless_lanes_are_untouched() {
    let mut h = heights(0.9, 0.1);
    h[2] = None;
    h[3] = Some(0.5);
    let w = [0.3, 0.0, 0.25, 0.02, 0.0, 0.0, 0.0, 0.0];
    let before = shares(w);
    for band in [1.0e-4, 0.05, 0.2, 1.0] {
        let after = shares(height_blend(w, h, band));
        let sum: f32 = after.iter().sum();
        assert!((sum - 1.0).abs() < 1.0e-5, "band {band}: shares sum {sum}");
        assert!(
            (after[2] - before[2]).abs() < 1.0e-5,
            "band {band}: heightless lane moved"
        );
    }
}

/// The GLSL carries the lines the mirror above stands for.
#[test]
fn glsl_blend_fades_each_lane_with_its_share() {
    let src = include_str!("../../../shaders/include/bindings.glsl");
    // #6197 — the arithmetic moved into the core the blend and the parallax's
    // height query share; the pins follow it there.
    let body = src
        .split_once("float byroTerrainBlendCore(")
        .expect("byroTerrainBlendCore must stay in bindings.glsl")
        .1;
    let body = &body[..body.find("\n}\n").expect("function end")];
    for line in [
        "const float fadeShare = 1.0 / float(TERRAIN_SPLAT_LAYERS + 1u);",
        "fade[i] = min(rel / fadeShare, 1.0);",
        "h[i] = fade[i] * hRaw[i] + rel;",
        "h[i] = fade[i] * max(h[i] - floorScore, 0.0);",
        "share[i] = heightShare * h[i] / total;",
    ] {
        assert!(
            body.contains(line),
            "byroTerrainBlendCore lost `{line}` (#6202)"
        );
    }
}

/// #6203 — the ground-cover density sites in `triangle.frag` evaluate the
/// painted splat, the field `groundcover_scatter.comp` places blades from;
/// only the surface shading reads the height-blended `terrainSplat`.
#[test]
fn groundcover_density_sites_read_the_painted_splat() {
    let frag = include_str!("../../../shaders/triangle.frag");
    for line in [
        "gcDetailSample.splat0 = fragSplat0;",
        "gcDetailSample.splat1 = fragSplat1;",
        "gcSample.splat0 = fragSplat0;",
        "gcSample.splat1 = fragSplat1;",
    ] {
        assert!(frag.contains(line), "triangle.frag lost `{line}` (#6203)");
    }
    assert!(
        !frag.contains("Sample.splat0 = terrainSplat[0];"),
        "a ground-cover density site reads the height-blended lanes again (#6203)"
    );
    let scatter = include_str!("../../../shaders/groundcover_scatter.comp");
    assert!(
        !scatter.contains("byroTerrainHeightBlend("),
        "the scatter now height-blends; the density sites in triangle.frag must follow (#6203)"
    );
}
