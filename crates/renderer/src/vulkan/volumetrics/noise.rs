//! Deterministic, tileable density volumes generated once at renderer boot.
//!
//! Shared by two consumers. The froxel volumetrics pipeline uses them as a
//! fog density field; the SKYAL sky-cubemap bake uses the same two volumes
//! as the base-shape and erosion noises of its cloud march. They are the
//! right shape for both: the base volume is FBM Perlin blended with an
//! inverted-Worley billow, and the detail volume is Worley-dominated —
//! which is the Perlin-Worley / Worley-erosion pair Schneider & Vos
//! describe (SIGGRAPH 2015, "The Real-Time Volumetric Cloudscapes of
//! Horizon: Zero Dawn"). Generating a second, near-identical set for
//! clouds would have been duplication, not independence.

use super::super::allocator::SharedAllocator;
use super::super::buffer::GpuBuffer;
use super::super::texture::OneTimeCommandError;
use anyhow::Result;
use ash::vk;
use std::sync::OnceLock;

pub(crate) const BASE_NOISE_SIZE: u32 = 64;
pub(crate) const DETAIL_NOISE_SIZE: u32 = 32;

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
pub(crate) fn cached_base_density_noise() -> &'static [u8] {
    BASE_NOISE_CACHE.get_or_init(|| generate_density_noise(BASE_NOISE_SIZE, false))
}

/// Detail-volume counterpart of [`cached_base_density_noise`]; see its doc
/// comment for the resize-regeneration bug this closes.
pub(crate) fn cached_detail_density_noise() -> &'static [u8] {
    DETAIL_NOISE_CACHE.get_or_init(|| generate_density_noise(DETAIL_NOISE_SIZE, true))
}

/// The base-volume slice the cloud shell reads as its 2D weather map
/// (`clouds.glsl` `cloud_column_coverage`): texel layer `BASE_NOISE_SIZE / 2`
/// along the volume's third axis, which the shader samples at the layer's
/// centre (`CLOUD_WEATHER_SLICE_W`).
pub(crate) const CLOUD_WEATHER_SLICE: u32 = BASE_NOISE_SIZE / 2;

/// The weather-map value above which a column holds cloud, so that a
/// `coverage` fraction of all columns does: the `1 - coverage` quantile of
/// the slice's texels. This is what makes `coverage` a horizontal cloud
/// fraction (the meteorological cloud amount) rather than a threshold on 3D
/// noise, which a 3.5 km column through the shell almost always exceeds
/// somewhere — the cause of a near-total deck overhead at every coverage.
/// `>= 1` for zero coverage: no column qualifies.
pub(crate) fn cloud_column_threshold(coverage: f32) -> f32 {
    static SORTED_SLICE: OnceLock<Vec<u8>> = OnceLock::new();
    let sorted = SORTED_SLICE.get_or_init(|| {
        let layer = (BASE_NOISE_SIZE * BASE_NOISE_SIZE) as usize;
        let start = CLOUD_WEATHER_SLICE as usize * layer;
        let mut slice = cached_base_density_noise()[start..start + layer].to_vec();
        slice.sort_unstable();
        slice
    });
    let coverage = if coverage.is_finite() { coverage.clamp(0.0, 1.0) } else { 0.0 };
    if coverage <= 0.0 {
        return 1.0;
    }
    let index = (((1.0 - coverage) * sorted.len() as f32) as usize).min(sorted.len() - 1);
    sorted[index] as f32 / 255.0
}

fn create_staging(
    device: &ash::Device,
    allocator: &SharedAllocator,
    bytes: &[u8],
) -> Result<GpuBuffer> {
    let mut buffer = GpuBuffer::create_host_visible(
        device,
        allocator,
        bytes.len() as vk::DeviceSize,
        vk::BufferUsageFlags::TRANSFER_SRC,
    )?;
    if let Err(error) = buffer.write_mapped(device, bytes) {
        buffer.destroy(device, allocator);
        return Err(error);
    }
    Ok(buffer)
}

/// Host-visible staging buffers holding both volumes' texels, base first.
/// The caller destroys them once its one-time submit has completed.
///
/// #4344 — both consumers (the froxel pipeline's `initialize_layouts` and
/// SKYAL's `CloudNoiseVolumes`) upload these same memoized texels into their
/// own R8 images, and each had grown its own copy of the staging-buffer and
/// buffer-to-image-copy sequence. The images and their barriers stay with the
/// consumers: the layouts they start from and the stages they publish to
/// legitimately differ (compute-only for froxels, compute + fragment for
/// clouds).
pub(crate) fn create_density_noise_staging(
    device: &ash::Device,
    allocator: &SharedAllocator,
) -> Result<[GpuBuffer; 2]> {
    // #5947 — a failure here precedes any submission, so it is tagged
    // `NotSubmitted`: the callers' `may_be_in_flight` check treats an untyped
    // error as "state unknown" and would otherwise leak a pipeline whose
    // images the GPU never saw.
    let not_submitted = |error: anyhow::Error| {
        anyhow::Error::from(OneTimeCommandError::not_submitted(
            error,
            "create density-noise staging",
        ))
    };
    let mut base =
        create_staging(device, allocator, cached_base_density_noise()).map_err(not_submitted)?;
    match create_staging(device, allocator, cached_detail_density_noise()) {
        Ok(detail) => Ok([base, detail]),
        Err(error) => {
            base.destroy(device, allocator);
            Err(not_submitted(error))
        }
    }
}

/// Dispose of the staging buffers once the consumer's one-time submit has
/// returned `upload`.
///
/// #5947 — "the submit waited, so nothing can reference them" only holds for
/// a success or a [`OneTimeCommandError::NotSubmitted`] failure. A failed
/// `vkQueueSubmit`, or a fence-wait failure other than device loss, leaves
/// the copies possibly pending (`MaybeInFlight`), so a host-side destroy
/// could race them: leak instead, as the #4891 sites in `buffer.rs`,
/// `blas_static.rs` and `texture_registry/upload.rs` do. Both consumers go
/// through here so the policy cannot drift between them.
pub(crate) fn release_density_noise_staging(
    device: &ash::Device,
    allocator: &SharedAllocator,
    staging: [GpuBuffer; 2],
    upload: &Result<()>,
) {
    if let Err(error) = upload {
        if OneTimeCommandError::may_be_in_flight(error) {
            std::mem::forget(staging);
            return;
        }
    }
    for mut buffer in staging {
        buffer.destroy(device, allocator);
    }
}

/// Record the two staging → image copies, base first, each at its volume's
/// full `size³` extent.
///
/// # Safety
///
/// `cmd` must be recording, `staging` must be the buffers
/// [`create_density_noise_staging`] returned (still live), and `images` must
/// be `R8_UNORM` 3D images of `BASE_NOISE_SIZE³` / `DETAIL_NOISE_SIZE³` with
/// `TRANSFER_DST` usage, already in `TRANSFER_DST_OPTIMAL`.
pub(crate) unsafe fn record_density_noise_upload(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    staging: &[GpuBuffer; 2],
    images: [vk::Image; 2],
) {
    let subresource = vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1);
    for ((buffer, image), size) in staging
        .iter()
        .zip(images)
        .zip([BASE_NOISE_SIZE, DETAIL_NOISE_SIZE])
    {
        let copy = vk::BufferImageCopy::default()
            .image_subresource(subresource)
            .image_extent(vk::Extent3D {
                width: size,
                height: size,
                depth: size,
            });
        // SAFETY: the caller's contract — `cmd` recording, buffer live and
        // fully populated, destination in TRANSFER_DST_OPTIMAL with a
        // matching R8 extent and TRANSFER_DST usage.
        unsafe {
            device.cmd_copy_buffer_to_image(
                cmd,
                buffer.buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[copy],
            );
        }
    }
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

    /// #5947 — the staging buffers are freed only through the shared
    /// disposal, which leaks them when the one-time submit may still be in
    /// flight. A consumer that goes back to destroying them unconditionally
    /// after its submit would race a pending copy on a failed
    /// `vkQueueSubmit` / non-DEVICE_LOST fence wait; and a consumer that
    /// destroys its images on that class of failure would race the same copy.
    #[test]
    fn density_noise_staging_disposal_honours_the_in_flight_policy() {
        for (name, src) in [
            (
                "cloud_noise.rs",
                crate::source_scan::production_text_or_whole(include_str!("../cloud_noise.rs")),
            ),
            (
                "volumetrics/init.rs",
                crate::source_scan::production_text_or_whole(include_str!("init.rs")),
            ),
        ] {
            assert!(
                src.contains("release_density_noise_staging("),
                "{name} must dispose of the density-noise staging through the shared policy"
            );
            assert!(
                !src.contains("staging.iter_mut()"),
                "{name} still destroys the density-noise staging unconditionally"
            );
        }
        let cloud = crate::source_scan::production_text_or_whole(include_str!("../cloud_noise.rs"));
        assert!(
            cloud.contains("OneTimeCommandError::may_be_in_flight(&error)"),
            "cloud_noise.rs must leak its images when the upload may be in flight"
        );
        for (name, src) in [
            ("context/init.rs + init_stages.rs", crate::vulkan::context::INIT_SOURCES),
            ("context/resize.rs", include_str!("../context/resize.rs")),
        ] {
            assert!(
                src.contains("#5947"),
                "{name} must branch the failed volumetrics init on the in-flight class (#5947)"
            );
        }
    }

    /// #4344 — both consumers must go through the shared upload path. The
    /// duplication this closes was not the generator (already shared) but the
    /// staging + `cmd_copy_buffer_to_image` sequence around it, which each
    /// consumer had spelled out with its own extents; a size fixed in one
    /// copy and not the other would have uploaded a truncated volume.
    #[test]
    fn both_density_noise_consumers_use_the_shared_upload() {
        for (name, src) in [
            ("cloud_noise.rs", include_str!("../cloud_noise.rs")),
            ("volumetrics/init.rs", include_str!("init.rs")),
        ] {
            assert!(
                src.contains("create_density_noise_staging(")
                    && src.contains("record_density_noise_upload("),
                "{name} must upload the density volumes through volumetrics::noise"
            );
            assert!(
                !src.contains("cmd_copy_buffer_to_image"),
                "{name} still records its own density-noise copy"
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

    /// The column threshold makes `coverage` the fraction of weather-map
    /// texels that hold cloud, measured on the very slice it is drawn from.
    #[test]
    fn cloud_column_threshold_selects_the_coverage_fraction_of_columns() {
        let layer = (BASE_NOISE_SIZE * BASE_NOISE_SIZE) as usize;
        let start = CLOUD_WEATHER_SLICE as usize * layer;
        let slice = &cached_base_density_noise()[start..start + layer];
        for coverage in [0.1f32, 0.4, 0.55, 0.7, 0.86] {
            let threshold = cloud_column_threshold(coverage);
            let above = slice
                .iter()
                .filter(|&&t| t as f32 / 255.0 >= threshold)
                .count() as f32
                / layer as f32;
            assert!(
                (above - coverage).abs() < 0.03,
                "coverage {coverage}: {above} of the columns pass threshold {threshold}"
            );
        }
        assert!(cloud_column_threshold(0.0) >= 1.0, "no coverage, no cloudy column");
        assert!(cloud_column_threshold(0.4) > cloud_column_threshold(0.86));
        assert_eq!(
            crate::shader_constants::CLOUD_WEATHER_SLICE_W,
            (CLOUD_WEATHER_SLICE as f32 + 0.5) / BASE_NOISE_SIZE as f32,
            "the shader must sample the slice the threshold is measured on"
        );
    }
}
