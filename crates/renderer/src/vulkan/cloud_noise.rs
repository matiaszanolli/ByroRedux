//! SKYAL — the cloud density volumes, shared by every cloud consumer.
//!
//! The volumetric cloud body (`shaders/include/clouds.glsl`) is marched in
//! two places: once per texel by the sky-cube bake, and once per clear-depth
//! pixel by the composite background. Both must sample the same field or the
//! sky seen directly and the sky seen in a reflection disagree.
//!
//! The volumes therefore belong to neither pipeline. The bake is optional
//! (it may fail under VRAM pressure); composite is mandatory, is rebuilt on
//! every resize, and its descriptor set is not PARTIALLY_BOUND, so it needs
//! valid views unconditionally. A resolution-independent resource owned by
//! the context, created before both, is the one shape that satisfies all of
//! that. It is also small — 64³ + 32³ texels of R8, 288 KiB — so there is no
//! degraded mode worth having: failing to create it is an init error.
//!
//! The texels are the two volumes `volumetrics/noise.rs` already generates
//! for froxel fog, which are the Perlin-Worley base / Worley-dominated detail
//! pair Schneider & Vos's cloud method wants. `VolumetricsPipeline` still
//! uploads its own copy of the same texels; folding it onto this resource is
//! possible but is left for a change that touches that pipeline.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::image::{GpuImage, GpuImageDesc};
use super::volumetrics::noise::{
    cached_base_density_noise, cached_detail_density_noise, BASE_NOISE_SIZE, DETAIL_NOISE_SIZE,
};
use anyhow::{Context, Result};
use ash::vk;

/// The handles a consumer needs to bind the volumes. `Copy` so it can be
/// passed by value into pipeline constructors and the resize path.
#[derive(Clone, Copy, Debug)]
pub struct CloudNoiseViews {
    pub base: vk::ImageView,
    pub detail: vk::ImageView,
    /// `LINEAR` / `REPEAT` on all three axes — both volumes are generated
    /// tileable, and the march relies on that to advect a world-space field
    /// through them without a seam.
    pub sampler: vk::Sampler,
}

pub struct CloudNoiseVolumes {
    base: Option<GpuImage>,
    detail: Option<GpuImage>,
    sampler: vk::Sampler,
}

impl CloudNoiseVolumes {
    /// Create both volumes, upload their texels, and leave them in
    /// `SHADER_READ_ONLY_OPTIMAL`.
    ///
    /// # Safety
    ///
    /// Caller must ensure `device`, `queue` and `pool` are valid and live,
    /// the device is not lost, and `pool` belongs to `device`. The images are
    /// created here, so no command buffer can be in flight against them.
    pub unsafe fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<Self> {
        let mut partial = Self {
            base: None,
            detail: None,
            sampler: vk::Sampler::null(),
        };
        let result = partial.create_and_upload(device, allocator, queue, pool);
        match result {
            Ok(()) => Ok(partial),
            Err(error) => {
                // SAFETY: every handle in `partial` was created by this device
                // in `create_and_upload`, and the one-time submit (if it ran)
                // waited on the queue before returning.
                unsafe { partial.destroy(device, allocator) };
                Err(error)
            }
        }
    }

    fn create_and_upload(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        for (is_base, size) in [(true, BASE_NOISE_SIZE), (false, DETAIL_NOISE_SIZE)] {
            let image = GpuImage::create(
                device,
                allocator,
                &GpuImageDesc::color_3d(
                    "sky cloud noise",
                    vk::Extent3D {
                        width: size,
                        height: size,
                        depth: size,
                    },
                    vk::Format::R8_UNORM,
                    vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
                ),
            )?;
            if is_base {
                self.base = Some(image);
            } else {
                self.detail = Some(image);
            }
        }

        // SAFETY: fully-populated create info; the handle is owned by `self`
        // from the assignment onward and freed by `destroy`.
        self.sampler = unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::REPEAT)
                        .address_mode_v(vk::SamplerAddressMode::REPEAT)
                        .address_mode_w(vk::SamplerAddressMode::REPEAT),
                    None,
                )
                .context("sky cloud noise sampler")?
        };

        // Memoized in `volumetrics::noise` — regenerating ~10^7 hashes here
        // would repeat work the froxel pipeline has already paid for.
        let payloads = [
            (cached_base_density_noise(), BASE_NOISE_SIZE),
            (cached_detail_density_noise(), DETAIL_NOISE_SIZE),
        ];
        let mut staging: Vec<GpuBuffer> = Vec::with_capacity(payloads.len());
        for (bytes, _) in payloads {
            let mut buffer = match GpuBuffer::create_host_visible(
                device,
                allocator,
                bytes.len() as vk::DeviceSize,
                vk::BufferUsageFlags::TRANSFER_SRC,
            ) {
                Ok(buffer) => buffer,
                Err(error) => {
                    for mut done in staging {
                        done.destroy(device, allocator);
                    }
                    return Err(error);
                }
            };
            if let Err(error) = buffer.write_mapped(device, bytes) {
                buffer.destroy(device, allocator);
                for mut done in staging {
                    done.destroy(device, allocator);
                }
                return Err(error);
            }
            staging.push(buffer);
        }

        let images = [
            self.base.as_ref().expect("base noise").image,
            self.detail.as_ref().expect("detail noise").image,
        ];
        let range = super::descriptors::color_subresource_single_mip();
        let result = super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let to_dst = images
                .map(|image| super::descriptors::image_barrier_undef_to_transfer_dst(image, 1));
            // SAFETY: `cmd` is recording; both images were created above and
            // are exclusively owned by `self`.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &to_dst,
                );
            }

            let subresource = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .base_array_layer(0)
                .layer_count(1);
            for (index, (_, size)) in payloads.iter().enumerate() {
                let copy = vk::BufferImageCopy::default()
                    .image_subresource(subresource)
                    .image_extent(vk::Extent3D {
                        width: *size,
                        height: *size,
                        depth: *size,
                    });
                // SAFETY: the staging buffer is live and fully populated; the
                // destination is in TRANSFER_DST_OPTIMAL with a matching R8
                // extent and TRANSFER_DST usage.
                unsafe {
                    device.cmd_copy_buffer_to_image(
                        cmd,
                        staging[index].buffer,
                        images[index],
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[copy],
                    );
                }
            }

            // Both consumers sample from their own shader stage (compute for
            // the bake, fragment for composite), so publish the transfer
            // writes to every stage that can read them.
            let ready = images.map(|image| {
                vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image(image)
                    .subresource_range(range)
            });
            // SAFETY: `cmd` is recording and both images were written by the
            // copies above; this publishes those writes and moves each image
            // into the layout both consumers' descriptors declare.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::COMPUTE_SHADER
                        | vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &ready,
                );
            }
            Ok(())
        });

        // The one-time submit waits on the queue before returning, so no
        // staging buffer can still be referenced here.
        for mut buffer in staging {
            buffer.destroy(device, allocator);
        }
        result
    }

    pub fn views(&self) -> CloudNoiseViews {
        CloudNoiseViews {
            base: self.base.as_ref().expect("cloud base noise").view,
            detail: self.detail.as_ref().expect("cloud detail noise").view,
            sampler: self.sampler,
        }
    }

    /// # Safety
    ///
    /// Caller must ensure `device` and `allocator` are valid and live, the
    /// device is not lost, and that no in-flight command buffer references
    /// these volumes — every consumer holding their views must already be
    /// destroyed, or at least idle.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for mut image in [self.base.take(), self.detail.take()].into_iter().flatten() {
            image.destroy(device, allocator);
        }
        if self.sampler != vk::Sampler::null() {
            // SAFETY: created by this device; no consumer is in flight (see
            // the function's safety contract).
            unsafe { device.destroy_sampler(self.sampler, None) };
            self.sampler = vk::Sampler::null();
        }
    }
}

#[cfg(test)]
mod tests {
    const SRC: &str = include_str!("cloud_noise.rs");
    const INIT: &str = include_str!("context/init.rs");
    const TEARDOWN: &str = include_str!("context/teardown.rs");

    /// The cloud texels are the froxel pipeline's existing volumes, not a
    /// second, near-identical generator.
    #[test]
    fn the_volumes_reuse_the_existing_generator() {
        assert!(
            SRC.contains("cached_base_density_noise()")
                && SRC.contains("cached_detail_density_noise()"),
            "cloud noise must upload the volumes volumetrics/noise.rs already generates",
        );
    }

    /// Both consumers bind these views, so the resource has to exist before
    /// either is constructed — and composite is mandatory with a
    /// non-PARTIALLY_BOUND set, so there is no "construct it later" order.
    #[test]
    fn init_creates_the_volumes_before_both_consumers() {
        let noise = INIT
            .find("CloudNoiseVolumes::new(")
            .expect("init constructs the cloud noise volumes");
        let bake = INIT
            .find("SkyCubePipeline::new(")
            .expect("init constructs the sky-cube bake");
        let composite = INIT
            .find("CompositePipeline::new(")
            .expect("init constructs composite");
        assert!(
            noise < bake && noise < composite,
            "CloudNoiseVolumes must be created before the sky-cube bake and composite, \
             both of which write its views into their descriptor sets at construction",
        );
    }

    /// Composite and the bake both hold these views in descriptor sets, so
    /// the volumes must outlive both.
    #[test]
    fn teardown_destroys_the_volumes_after_both_consumers() {
        let noise = TEARDOWN
            .find("cloud_noise.destroy(")
            .expect("teardown destroys the cloud noise volumes");
        let bake = TEARDOWN
            .find("sky_cube.destroy(")
            .expect("teardown destroys the sky-cube bake");
        let composite = TEARDOWN
            .find("composite.destroy(")
            .expect("teardown destroys composite");
        assert!(
            noise > bake && noise > composite,
            "the cloud noise volumes must be destroyed after every pipeline that binds them",
        );
    }

    /// The march advects a world-space field through the volumes and relies
    /// on them tiling. `CLAMP_TO_EDGE` would smear the last texel slab across
    /// the sky once the field drifted past one tile.
    #[test]
    fn the_sampler_repeats_on_all_three_axes() {
        for axis in ["address_mode_u", "address_mode_v", "address_mode_w"] {
            assert!(
                SRC.contains(&format!(".{axis}(vk::SamplerAddressMode::REPEAT)")),
                "the cloud noise sampler must REPEAT along {axis}",
            );
        }
    }
}
