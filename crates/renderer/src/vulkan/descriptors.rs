//! Descriptor sets for texture sampling.

use super::texture::Texture;
use anyhow::{Context, Result};
use ash::vk;

/// Descriptor pool, layout, and per-swapchain-image descriptor sets.
pub struct DescriptorState {
    pub pool: vk::DescriptorPool,
    pub layout: vk::DescriptorSetLayout,
    pub sets: Vec<vk::DescriptorSet>,
}

impl DescriptorState {
    /// Create descriptor layout (binding 0 = combined image sampler, fragment stage),
    /// allocate one descriptor set per swapchain image, and write the texture into each.
    pub fn new(
        device: &ash::Device,
        swapchain_image_count: u32,
        texture: &Texture,
    ) -> Result<Self> {
        // Layout: one combined image sampler at binding 0, fragment stage.
        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);

        let bindings = [binding];
        let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);

        let layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .context("Failed to create descriptor set layout")?
        };

        // Pool: enough for N combined image samplers (one per swapchain image).
        let pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: swapchain_image_count,
        };

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(std::slice::from_ref(&pool_size))
            .max_sets(swapchain_image_count);

        let pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .context("Failed to create descriptor pool")?
        };

        // Allocate N identical sets (same layout).
        let layouts: Vec<vk::DescriptorSetLayout> =
            vec![layout; swapchain_image_count as usize];

        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);

        let sets = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .context("Failed to allocate descriptor sets")?
        };

        // Write the texture into each set.
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.image_view)
            .sampler(texture.sampler);

        for &set in &sets {
            let write = vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(std::slice::from_ref(&image_info));

            unsafe {
                device.update_descriptor_sets(&[write], &[]);
            }
        }

        log::info!(
            "Descriptor sets created: {} sets, 1 combined image sampler each",
            swapchain_image_count
        );

        Ok(Self { pool, layout, sets })
    }

    /// Destroy the descriptor pool and layout.
    /// Sets are freed implicitly when the pool is destroyed.
    pub fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.layout, None);
        }
    }
}
