#ifndef BYRO_GROUNDCOVER_DENSITY_GLSL
#define BYRO_GROUNDCOVER_DENSITY_GLSL

// EXAL ground cover — the density field (§3, #4054).
//
// A single scalar evaluated per candidate point, on the GPU, inside the
// scatter pass. Never precomputed per cell, never stored per blade.
//
//   d_ground = affinity(splat)               // what the ground is made of
//            × slope_gate(normal)            // grass does not grow on cliffs
//            × moisture(height_above_water)  // shoreline lush, ridge sparse
//            × shelter(curvature)            // clumps settle into concavities
//            × clump(noise)                  // the organic term
//
//   d_draw   = d_ground × distance_fade(view)   // LOD only, §6
//
// **These are two different quantities and conflating them is a bug.**
// `d_ground` is intrinsic — a property of the place, identical whether the
// camera is standing on it or a kilometre away. `d_draw` is how much of that
// we are choosing to rasterize this frame. Only the scatter's accept/reject
// test may read `d_draw`; the blade record, §12.1's occlusion, §12.3's ground
// coupling and §12.5's canopy shadow all read `d_ground`. Feeding the faded
// value to those makes the shadow under a meadow lighten as you walk away from
// it — a slow whole-screen brightness change keyed to camera position, very
// visible and very hard to attribute once shipped.
//
// This file is the ONLY definition of the formula. Every parameter is
// canonical in `shader_constants_data.rs` and arrives through the generated
// header; there is deliberately no Rust mirror (§3).
//
// Requires, in this order:
//   #include "include/shader_constants.glsl"
//   #include "include/terrain_sample.glsl"   // TerrainSample

// ── Noise ───────────────────────────────────────────────────────────────
//
// Hand-rolled rather than sampled from a texture: the field is evaluated at
// arbitrary world points at whatever resolution the scatter asks for, and §3
// requires the clump term to be scale-free for exactly that reason. A baked
// noise texture would reintroduce a resolution ceiling — the same defect the
// 17x17 splat grid has, one octave further out.

/// 2-D integer hash → [0,1)^2. The lattice offsets Worley needs.
vec2 byroGcHash2(vec2 cell) {
    // Two independent large-odd multipliers; the xor-shift decorrelates the
    // low bits, which is where a plain sin-based hash bands visibly.
    uvec2 c = uvec2(ivec2(floor(cell)) + ivec2(0x8000, 0x8000));
    uint h = c.x * 0x27D4EB2Du ^ (c.y * 0x9E3779B9u);
    h ^= h >> 15;
    h *= 0x85EBCA6Bu;
    h ^= h >> 13;
    uint h2 = h * 0xC2B2AE35u;
    h2 ^= h2 >> 16;
    return vec2(float(h & 0xFFFFu), float(h2 & 0xFFFFu)) * (1.0 / 65536.0);
}

float byroGcHash1(vec2 cell) {
    return byroGcHash2(cell).x;
}

/// Worley (cellular) F1 distance over a unit lattice, normalised to ~[0,1].
///
/// Clump structure comes from *inverted* F1: near a feature point the distance
/// is small, and that is where a clump sits. 3x3 neighbourhood is exact for
/// jittered points confined to their own cell.
float byroGcWorley(vec2 p) {
    vec2 base = floor(p);
    vec2 frac = p - base;
    float best = 8.0;
    for (int dy = -1; dy <= 1; ++dy) {
        for (int dx = -1; dx <= 1; ++dx) {
            vec2 offset = vec2(float(dx), float(dy));
            vec2 feature = offset + byroGcHash2(base + offset);
            best = min(best, dot(feature - frac, feature - frac));
        }
    }
    // sqrt once, at the end. The maximum F1 for jittered-grid points is ~1.0,
    // so this lands in [0,1] without a further scale.
    return min(sqrt(best), 1.0);
}

/// Value-noise fBm, two octaves. Only used for the ~4000-unit regional term,
/// where the requirement is "slowly varying and non-repeating", not detail.
float byroGcFbm(vec2 p) {
    float total = 0.0;
    float amplitude = 0.5;
    for (int octave = 0; octave < 2; ++octave) {
        vec2 base = floor(p);
        vec2 frac = p - base;
        // Quintic smoothstep — C2 continuous, so the derivative the terrain
        // detail layer (§6 tier 3) takes of this field has no lattice creases.
        vec2 w = frac * frac * frac * (frac * (frac * 6.0 - 15.0) + 10.0);
        float a = byroGcHash1(base);
        float b = byroGcHash1(base + vec2(1.0, 0.0));
        float c = byroGcHash1(base + vec2(0.0, 1.0));
        float d = byroGcHash1(base + vec2(1.0, 1.0));
        total += amplitude * mix(mix(a, b, w.x), mix(c, d, w.x), w.y);
        p *= 2.03;   // irrational-ish lacunarity; 2.0 exactly aligns octaves
        amplitude *= 0.5;
    }
    return total / 0.75;   // normalise 0.5 + 0.25
}

// ── The five intrinsic factors ──────────────────────────────────────────

/// `affinity(splat)` — the 8 splat weights dotted with the per-layer canonical
/// `cover_affinity`.
///
/// §3's key reframing: a layer does not *enable* ground cover, it *weights* it.
/// A dirt layer at 0.15 and a grass layer at 0.9 blend into a continuous
/// gradient wherever the painter feathered them, so the vegetation boundary
/// stops coinciding with the texture boundary — which is cause #1 of the
/// vanilla patch look.
///
/// Normalised by total splat weight rather than taken raw: LAND weights do not
/// sum to 1 (each layer's alpha is independent), so an unnormalised dot would
/// make a doubly-painted vertex denser than a singly-painted one for reasons
/// that have nothing to do with what is growing there.
float byroGcAffinity(vec4 splat0, vec4 splat1, vec4 affinity0, vec4 affinity1) {
    float weighted = dot(splat0, affinity0) + dot(splat1, affinity1);
    float total = dot(splat0, vec4(1.0)) + dot(splat1, vec4(1.0));
    // Unpainted ground (every layer zero) is the cell's base texture, which
    // has no LTEX record and therefore no affinity. Falling to 0 would make
    // every unpainted vertex a hard hole; the base layer is ordinary ground,
    // so it takes the same low-but-nonzero default an unrecognised name gets.
    if (total < 1.0e-4) {
        return GROUNDCOVER_DEFAULT_AFFINITY;
    }
    return clamp(weighted / total, 0.0, 1.0);
}

/// `slope_gate(normal)` — smoothstep on the terrain normal's Y component.
///
/// Thresholds are the cosines of the soil angle of repose; see the constants'
/// doc. Ground cover thins out and then stops on steep faces.
float byroGcSlopeGate(vec3 normal) {
    return smoothstep(GROUNDCOVER_SLOPE_GATE_START, GROUNDCOVER_SLOPE_GATE_FULL, normal.y);
}

/// `moisture(height_above_water)` — lush shorelines, sparse high ground.
///
/// `waterY` is the cell's water-plane height in Y-up world space, or
/// `GROUNDCOVER_NO_WATER` when the cell has none.
///
/// **The no-water path returns 1.0, and that is load-bearing** (§3). In a pure
/// product one undefined factor takes the whole field with it, and "this
/// worldspace has no water" is common — the failure would be an entire
/// high-desert worldspace with no ground cover at all and nothing in the log
/// to say why. The term expresses *extra* moisture near water; its absence is
/// neutral, not hostile.
float byroGcMoisture(float height, float waterY) {
    if (waterY <= GROUNDCOVER_NO_WATER) {
        return 1.0;
    }
    float above = height - waterY;
    // Submerged ground is not a shoreline. Below the plane, cover stops.
    if (above < -GROUNDCOVER_MOISTURE_SUBMERGED_DEPTH) {
        return 0.0;
    }
    float t = clamp(above / GROUNDCOVER_MOISTURE_FALLOFF_UNITS, 0.0, 1.0);
    return mix(1.0, GROUNDCOVER_MOISTURE_FLOOR, t);
}

/// `shelter(curvature)` — discrete Laplacian of the heightfield.
///
/// Concave ground accumulates soil, water and seed; convex ground sheds all
/// three. Gives the distribution a relationship with the landform rather than
/// only with the paint. `laplacian` is the second difference in world units
/// over the sample stencil (positive = concave, since the four neighbours sit
/// above the centre).
float byroGcShelter(float laplacian) {
    float shaped = clamp(laplacian * GROUNDCOVER_SHELTER_CURVATURE_SCALE, -1.0, 1.0);
    return clamp(1.0 + shaped * GROUNDCOVER_SHELTER_STRENGTH, 0.0, 1.0 + GROUNDCOVER_SHELTER_STRENGTH);
}

/// `clump(noise)` — the organic term, and the only one with authority above
/// ~128 units.
///
/// §3: the splat authority is a 17x17 alpha grid per 2048-unit quadrant.
/// Bilinear sampling makes it continuous rather than stepped, which fixes the
/// hard step, but it cannot manufacture detail finer than the grid. Stub this
/// to 1.0 "for now" and the result reproduces the vanilla patch look exactly —
/// which is why it is a `float`, not a `#define`, and why it is here rather
/// than deferred to a later phase.
float byroGcClump(vec2 worldXZ) {
    // Worley at the clump scale, inverted so feature points are clump centres.
    float cellular = 1.0 - byroGcWorley(worldXZ / GROUNDCOVER_CLUMP_CELL_UNITS);
    cellular = pow(clamp(cellular, 0.0, 1.0), GROUNDCOVER_CLUMP_CONTRAST);
    // The floor keeps the gaps between clumps thin rather than bare: clumping
    // should read as density variation, not as holes punched in the sward.
    cellular = mix(GROUNDCOVER_CLUMP_FLOOR, 1.0, cellular);

    // Regional fBm — one meadow richer than the next, never a hard boundary.
    float regional = byroGcFbm(worldXZ / GROUNDCOVER_REGION_UNITS);
    regional = mix(1.0 - GROUNDCOVER_REGION_AMPLITUDE, 1.0, clamp(regional, 0.0, 1.0));

    return clamp(cellular * regional, 0.0, 1.0);
}

/// Discrete Laplacian of the heightfield at `worldXZ`, sampled one terrain
/// vertex spacing out in each axis — the input `byroGcShelter` wants.
///
/// Sampled at the vertex spacing rather than at some smaller epsilon because
/// the heightfield HAS no detail below that: a tighter stencil would just
/// measure the bilinear interpolant's own curvature, which is zero inside a
/// quad and undefined on its edges.
///
/// Lives here rather than in the scatter because §12.5's terrain receiver
/// (#4057) evaluates the same field from `triangle.frag`, and a second copy
/// of this stencil would be a second way for the sward's density and the
/// density of the shadow it casts to disagree.
float byroGcLaplacian(uint vertexOffset, vec2 originXZ, vec2 worldXZ, float centreHeight) {
    const float H = LAND_VERTEX_SPACING;
    float total = 0.0;
    float taken = 0.0;
    vec2 offsets[4] = vec2[4](vec2(H, 0.0), vec2(-H, 0.0), vec2(0.0, H), vec2(0.0, -H));
    for (int i = 0; i < 4; ++i) {
        TerrainSample n = byroSampleTerrain(vertexOffset, originXZ, worldXZ + offsets[i]);
        // A neighbour outside the cell is skipped rather than clamped. At a
        // cell seam the stencil is one-sided, which biases the curvature
        // slightly; clamping would instead fold the boundary vertex in twice
        // and read as a ridge along every cell edge — a visible 4096-unit grid
        // in the density, which is precisely the artifact this design exists
        // to remove.
        if (n.valid) {
            total += n.height - centreHeight;
            taken += 1.0;
        }
    }
    return taken > 0.0 ? total * (4.0 / taken) : 0.0;
}

// ── The field ───────────────────────────────────────────────────────────

/// The five intrinsic factors, unmultiplied.
///
/// The product is what the scatter consumes, but the *factors* are what §11.3
/// calibrates — and they have to be reportable separately, because a density
/// field is a pure product and therefore fails in exactly one way: one term
/// goes to zero and takes the whole thing with it. A histogram of the product
/// cannot say which, and the five candidates have five unrelated fixes.
struct GroundCoverFactors {
    float affinity;
    float slope;
    float moisture;
    float shelter;
    float clump;
};

GroundCoverFactors byroGcDensityFactors(
    TerrainSample s,
    vec2 worldXZ,
    vec4 affinity0,
    vec4 affinity1,
    float waterY,
    float laplacian
) {
    GroundCoverFactors f;
    f.affinity = byroGcAffinity(s.splat0, s.splat1, affinity0, affinity1);
    f.slope = byroGcSlopeGate(s.normal);
    f.moisture = byroGcMoisture(s.height, waterY);
    f.shelter = byroGcShelter(laplacian);
    f.clump = byroGcClump(worldXZ);
    return f;
}

float byroGcCombine(GroundCoverFactors f) {
    return clamp(f.affinity * f.slope * f.moisture * f.shelter * f.clump, 0.0, 1.0);
}

/// Intrinsic ground-cover density at a point. **This is the value everything
/// except the scatter's accept test consumes**, and the value written into the
/// blade record.
///
/// `laplacian` comes from the caller because computing it needs four extra
/// terrain samples the caller may already have taken.
float byroGcDensityGround(
    TerrainSample s,
    vec2 worldXZ,
    vec4 affinity0,
    vec4 affinity1,
    float waterY,
    float laplacian
) {
    if (!s.valid) {
        return 0.0;
    }
    return byroGcCombine(
        byroGcDensityFactors(s, worldXZ, affinity0, affinity1, waterY, laplacian));
}

/// `distance_fade(view)` — §6's stochastic thinning, and the ONLY term that
/// separates `d_draw` from `d_ground`.
float byroGcDistanceFade(float viewDistance) {
    return 1.0 - smoothstep(GROUNDCOVER_FADE_START, GROUNDCOVER_DRAW_DISTANCE, viewDistance);
}

#endif // BYRO_GROUNDCOVER_DENSITY_GLSL
