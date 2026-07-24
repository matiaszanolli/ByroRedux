//! Shared exposure input for temporal upscaling and final composition.
//!
//! FSR consumes exposure as a 1x1 `R32_SFLOAT` texture. The renderer does not
//! have automatic exposure yet, so this resource stores the existing fixed HDR
//! exposure explicitly instead of letting the upscaler and composite pass grow
//! independent constants. A future auto-exposure pass can replace the one-time
//! clear with a storage-image write without changing the input contract.

use super::allocator::SharedAllocator;
use super::descriptors::color_subresource_single_mip;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;
use gpu_allocator::MemoryLocation;

pub const EXPOSURE_FORMAT: vk::Format = vk::Format::R32_SFLOAT;
pub const DEFAULT_EXPOSURE: f32 = 0.85;

/// Persistent 1x1 exposure texture shared by FSR and final composition.
pub struct ExposureResource {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<vk_alloc::Allocation>,
    device: ash::Device,
    allocator: Option<SharedAllocator>,
    value: f32,
}

impl ExposureResource {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
    ) -> Result<Self> {
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(EXPOSURE_FORMAT)
            .extent(vk::Extent3D {
                width: 1,
                height: 1,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe {
            // SAFETY: `image_info` is fully initialized and contains no
            // pointers that outlive this call.
            device
                .create_image(&image_info, None)
                .context("create exposure image")?
        };

        let requirements = unsafe {
            // SAFETY: `image` was just created by this device and remains live.
            device.get_image_memory_requirements(image)
        };
        let allocation_result = allocator.lock().expect("allocator lock poisoned").allocate(
            &vk_alloc::AllocationCreateDesc {
                name: "fsr_exposure_1x1",
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            },
        );
        let allocation = match allocation_result {
            Ok(allocation) => allocation,
            Err(error) => {
                unsafe {
                    // SAFETY: allocation failed, so `image` is live and unbound.
                    device.destroy_image(image, None);
                }
                return Err(error).context("allocate exposure image");
            }
        };

        if let Err(error) = unsafe {
            // SAFETY: `allocation` satisfies the queried requirements for the
            // still-unbound `image` and belongs to this logical device.
            device.bind_image_memory(image, allocation.memory(), allocation.offset())
        } {
            unsafe {
                // SAFETY: binding failed and no view exists, so the image can
                // be destroyed before its allocation is returned.
                device.destroy_image(image, None);
            }
            allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(allocation)
                .ok();
            return Err(error).context("bind exposure image memory");
        }

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(EXPOSURE_FORMAT)
            .subresource_range(color_subresource_single_mip());
        let view = match unsafe {
            // SAFETY: `image` is live, bound, and format-compatible with the
            // single-mip color view described by `view_info`.
            device.create_image_view(&view_info, None)
        } {
            Ok(view) => view,
            Err(error) => {
                unsafe {
                    // SAFETY: no view was created; destroy the bound image
                    // before returning its allocation.
                    device.destroy_image(image, None);
                }
                allocator
                    .lock()
                    .expect("allocator lock poisoned")
                    .free(allocation)
                    .ok();
                return Err(error).context("create exposure image view");
            }
        };

        let mut resource = Self {
            image,
            view,
            allocation: Some(allocation),
            device: device.clone(),
            allocator: Some(allocator.clone()),
            value: DEFAULT_EXPOSURE,
        };
        if let Err(error) = resource.initialize(queue, command_pool) {
            resource.destroy();
            return Err(error);
        }
        Ok(resource)
    }

    fn initialize(
        &self,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(&self.device, queue, command_pool, |cmd| {
            let range = color_subresource_single_mip();
            let to_transfer = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .image(self.image)
                .subresource_range(range);
            unsafe {
                // SAFETY: `cmd` is recording; the image is at its declared
                // initial layout and has not been submitted before.
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_transfer],
                );
                self.device.cmd_clear_color_image(
                    cmd,
                    self.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue {
                        float32: [self.value, 0.0, 0.0, 0.0],
                    },
                    &[range],
                );
            }

            let to_shader_read = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(self.image)
                .subresource_range(range);
            unsafe {
                // SAFETY: the transfer clear above is ordered before future
                // compute/fragment shader reads of the same subresource.
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::COMPUTE_SHADER
                        | vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_shader_read],
                );
            }
            Ok(())
        })
        .context("initialize exposure image")
    }

    pub fn image(&self) -> vk::Image {
        self.image
    }

    pub fn view(&self) -> vk::ImageView {
        self.view
    }

    pub const fn layout(&self) -> vk::ImageLayout {
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    }

    pub const fn extent(&self) -> vk::Extent2D {
        vk::Extent2D {
            width: 1,
            height: 1,
        }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn destroy(&mut self) {
        if self.image == vk::Image::null() {
            return;
        }
        unsafe {
            // SAFETY: callers ensure no GPU work references this persistent
            // resource. The view is destroyed before its backing image.
            self.device.destroy_image_view(self.view, None);
            self.device.destroy_image(self.image, None);
        }
        self.view = vk::ImageView::null();
        self.image = vk::Image::null();
        if let (Some(allocator), Some(allocation)) = (&self.allocator, self.allocation.take()) {
            allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(allocation)
                .ok();
        }
        self.allocator = None;
    }
}

impl Drop for ExposureResource {
    fn drop(&mut self) {
        self.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fsr_exposure_contract_is_one_r32_float_texel() {
        assert_eq!(EXPOSURE_FORMAT, vk::Format::R32_SFLOAT);
        assert_eq!(DEFAULT_EXPOSURE, 0.85);
    }
}
