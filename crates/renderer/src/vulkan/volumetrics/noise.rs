//! Deterministic, tileable density volumes generated once at renderer boot.

use std::sync::OnceLock;

pub(super) const BASE_NOISE_SIZE: u32 = 64;
pub(super) const DETAIL_NOISE_SIZE: u32 = 32;

fn avalanche_hash(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

fn lattice_hash(x: i32, y: i32, z: i32, period: i32, seed: u32) -> u32 {
    let wrap = |value: i32| value.rem_euclid(period) as u32;
    avalanche_hash(
        wrap(x).wrapping_mul(0x8da6_b343)
            ^ wrap(y).wrapping_mul(0xd816_3841)
            ^ wrap(z).wrapping_mul(0xcb1a_b31f)
            ^ seed,
    )
}

fn hash_unit(hash: u32) -> f32 {
    (hash >> 8) as f32 * (1.0 / 16_777_216.0)
}

fn gradient(hash: u32) -> [f32; 3] {
    const GRADIENTS: [[f32; 3]; 12] = [
        [1.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
        [1.0, -1.0, 0.0],
        [-1.0, -1.0, 0.0],
        [1.0, 0.0, 1.0],
        [-1.0, 0.0, 1.0],
        [1.0, 0.0, -1.0],
        [-1.0, 0.0, -1.0],
        [0.0, 1.0, 1.0],
        [0.0, -1.0, 1.0],
        [0.0, 1.0, -1.0],
        [0.0, -1.0, -1.0],
    ];
    GRADIENTS[hash as usize % GRADIENTS.len()]
}

fn fade(value: f32) -> f32 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Periodic improved-Perlin sample over a unit torus.
fn tileable_perlin(uvw: [f32; 3], period: i32, seed: u32) -> f32 {
    let p = uvw.map(|axis| axis * period as f32);
    let cell = p.map(|axis| axis.floor() as i32);
    let frac = [
        p[0] - cell[0] as f32,
        p[1] - cell[1] as f32,
        p[2] - cell[2] as f32,
    ];
    let weight = [fade(frac[0]), fade(frac[1]), fade(frac[2])];
    let mut corners = [[[0.0f32; 2]; 2]; 2];
    for (dz, plane) in corners.iter_mut().enumerate() {
        for (dy, row) in plane.iter_mut().enumerate() {
            for (dx, value) in row.iter_mut().enumerate() {
                let g = gradient(lattice_hash(
                    cell[0] + dx as i32,
                    cell[1] + dy as i32,
                    cell[2] + dz as i32,
                    period,
                    seed,
                ));
                let delta = [
                    frac[0] - dx as f32,
                    frac[1] - dy as f32,
                    frac[2] - dz as f32,
                ];
                *value = g[0] * delta[0] + g[1] * delta[1] + g[2] * delta[2];
            }
        }
    }
    let z0 = lerp(
        lerp(corners[0][0][0], corners[0][0][1], weight[0]),
        lerp(corners[0][1][0], corners[0][1][1], weight[0]),
        weight[1],
    );
    let z1 = lerp(
        lerp(corners[1][0][0], corners[1][0][1], weight[0]),
        lerp(corners[1][1][0], corners[1][1][1], weight[0]),
        weight[1],
    );
    (0.5 + 0.5 * lerp(z0, z1, weight[2])).clamp(0.0, 1.0)
}

/// Periodic Worley F1 sample. Feature-point hashes wrap while their positions
/// stay in the query's nearest unwrapped cells, keeping the torus continuous.
fn tileable_worley(uvw: [f32; 3], cells: i32, seed: u32) -> f32 {
    let p = uvw.map(|axis| axis * cells as f32);
    let base = p.map(|axis| axis.floor() as i32);
    let mut min_distance_sq = f32::INFINITY;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cell = [base[0] + dx, base[1] + dy, base[2] + dz];
                let h = lattice_hash(cell[0], cell[1], cell[2], cells, seed);
                let feature = [
                    hash_unit(h),
                    hash_unit(avalanche_hash(h ^ 0x68bc_21eb)),
                    hash_unit(avalanche_hash(h ^ 0x02e5_be93)),
                ];
                let delta = [
                    cell[0] as f32 + feature[0] - p[0],
                    cell[1] as f32 + feature[1] - p[1],
                    cell[2] as f32 + feature[2] - p[2],
                ];
                min_distance_sq = min_distance_sq
                    .min(delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]);
            }
        }
    }
    min_distance_sq.sqrt().clamp(0.0, 1.0)
}

fn smooth_unit(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(super) fn generate_density_noise(size: u32, detail: bool) -> Vec<u8> {
    let mut texels = Vec::with_capacity(size as usize * size as usize * size as usize);
    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let uvw = [
                    x as f32 / size as f32,
                    y as f32 / size as f32,
                    z as f32 / size as f32,
                ];
                let density = if detail {
                    let cells8 = 1.0 - tileable_worley(uvw, 8, 0x4d2c_6df3);
                    let cells16 = 1.0 - tileable_worley(uvw, 16, 0x9e37_79b9);
                    let perlin16 = tileable_perlin(uvw, 16, 0x1656_67b1);
                    smooth_unit((cells8 * 0.50 + cells16 * 0.30 + perlin16 * 0.20 - 0.08) / 0.84)
                } else {
                    let perlin = tileable_perlin(uvw, 4, 0x243f_6a88) * 0.571_428_6
                        + tileable_perlin(uvw, 8, 0x85a3_08d3) * 0.285_714_3
                        + tileable_perlin(uvw, 16, 0x1319_8a2e) * 0.142_857_1;
                    let billow = 1.0 - tileable_worley(uvw, 4, 0xa409_3822);
                    smooth_unit((perlin * 0.68 + billow * 0.32 - 0.10) / 0.80)
                };
                texels.push((density * 255.0 + 0.5) as u8);
            }
        }
    }
    texels
}

static BASE_NOISE_CACHE: OnceLock<Vec<u8>> = OnceLock::new();
static DETAIL_NOISE_CACHE: OnceLock<Vec<u8>> = OnceLock::new();

/// Memoized [`generate_density_noise`] for the boot-time base volume.
///
/// #2231 / REN-D5-03 — `VolumetricsPipeline::initialize_layouts` runs on
/// every window resize (the whole pipeline, including its noise images, is
/// rebuilt because the froxel grid follows render resolution — see
/// `context/resize.rs`), but this noise is resolution-independent and
/// deterministic (`generated_volumes_are_deterministic_and_useful` below):
/// regenerating it via ~10^7 hash evaluations bought nothing on a resize
/// and was a real CPU stall every time. Computed once per process and
/// reused for every subsequent resize's fresh-image upload.
pub(super) fn cached_base_density_noise() -> &'static [u8] {
    BASE_NOISE_CACHE.get_or_init(|| generate_density_noise(BASE_NOISE_SIZE, false))
}

/// Detail-volume counterpart of [`cached_base_density_noise`]; see its doc
/// comment for the resize-regeneration bug this closes.
pub(super) fn cached_detail_density_noise() -> &'static [u8] {
    DETAIL_NOISE_CACHE.get_or_init(|| generate_density_noise(DETAIL_NOISE_SIZE, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periodic_primitives_match_at_every_torus_boundary() {
        for axis in 0..3 {
            let mut a = [0.173, 0.419, 0.731];
            let mut b = a;
            a[axis] = 0.0;
            b[axis] = 1.0;
            assert!((tileable_perlin(a, 8, 17) - tileable_perlin(b, 8, 17)).abs() < 1.0e-6);
            assert!((tileable_worley(a, 8, 17) - tileable_worley(b, 8, 17)).abs() < 1.0e-6);
        }
    }

    #[test]
    fn generated_volumes_are_deterministic_and_useful() {
        for (size, detail) in [(16, false), (16, true)] {
            let a = generate_density_noise(size, detail);
            let b = generate_density_noise(size, detail);
            assert_eq!(a, b);
            assert_eq!(a.len(), size as usize * size as usize * size as usize);
            let min = *a.iter().min().unwrap();
            let max = *a.iter().max().unwrap();
            let mean = a.iter().map(|&v| v as f32).sum::<f32>() / a.len() as f32;
            assert!(
                max - min > 96,
                "noise field lacks dynamic range: {min}..{max}"
            );
            assert!(
                (55.0..200.0).contains(&mean),
                "noise mean is unusable: {mean}"
            );
        }
    }

    /// #2231 / REN-D5-03 — pins that the boot-noise accessors are actually
    /// memoized (not just individually deterministic, which the test above
    /// already covers): repeated calls must return the exact same backing
    /// bytes as a single ground-truth `generate_density_noise` call,
    /// proving `initialize_layouts` on a resize reuses the cached buffer
    /// instead of re-running ~10^7 hash evaluations.
    #[test]
    fn cached_density_noise_matches_direct_generation_and_is_stable_across_calls() {
        let expected_base = generate_density_noise(BASE_NOISE_SIZE, false);
        assert_eq!(cached_base_density_noise(), expected_base.as_slice());
        assert_eq!(
            cached_base_density_noise().as_ptr(),
            cached_base_density_noise().as_ptr(),
            "second call must return the same backing allocation, not a fresh one"
        );

        let expected_detail = generate_density_noise(DETAIL_NOISE_SIZE, true);
        assert_eq!(cached_detail_density_noise(), expected_detail.as_slice());
        assert_eq!(
            cached_detail_density_noise().as_ptr(),
            cached_detail_density_noise().as_ptr(),
            "second call must return the same backing allocation, not a fresh one"
        );
    }
}
