//! Texture registry — maps texture paths to GPU-resident textures with per-texture descriptor sets.

use crate::vulkan::allocator::SharedAllocator;
use crate::vulkan::texture::Texture;
use anyhow::{Context, Result};
use ash::vk;
use std::collections::HashMap;

/// Handle into the TextureRegistry (mirrors MeshHandle pattern).
pub type TextureHandle = u32;

struct TextureEntry {
    texture: Texture,
    descriptor_sets: Vec<vk::DescriptorSet>,
}

/// Registry mapping texture paths to GPU-resident textures with cached descriptor sets.
///
/// Each texture gets its own descriptor sets (one per swapchain image) so the draw loop
/// can bind per-mesh textures by swapping descriptor sets between draw calls.
pub struct TextureRegistry {
    textures: Vec<TextureEntry>,
    path_map: HashMap<String, TextureHandle>,
    fallback_handle: TextureHandle,
    descriptor_pool: vk::DescriptorPool,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    swapchain_image_count: u32,
}

impl TextureRegistry {
    /// Create a new texture registry.
    ///
    /// Registers `fallback_texture` as handle 0 (used when texture loading fails or no path given).
    pub fn new(
        device: &ash::Device,
        swapchain_image_count: u32,
        max_textures: u32,
        fallback_texture: Texture,
    ) -> Result<Self> {
        // Descriptor set layout: binding 0 = combined image sampler, fragment stage.
        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);

        let layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(std::slice::from_ref(&binding));

        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .context("Failed to create texture descriptor set layout")?
        };

        let total_sets = max_textures * swapchain_image_count;
        let pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: total_sets,
        };

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(std::slice::from_ref(&pool_size))
            .max_sets(total_sets)
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);

        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .context("Failed to create texture descriptor pool")?
        };

        let mut registry = Self {
            textures: Vec::new(),
            path_map: HashMap::new(),
            fallback_handle: 0,
            descriptor_pool,
            descriptor_set_layout,
            swapchain_image_count,
        };

        // Register fallback as handle 0.
        let sets = registry.allocate_and_write_sets(device, &fallback_texture)?;
        registry.textures.push(TextureEntry {
            texture: fallback_texture,
            descriptor_sets: sets,
        });

        log::info!(
            "TextureRegistry created: pool for {} textures × {} swapchain images",
            max_textures,
            swapchain_image_count,
        );

        Ok(registry)
    }

    /// Load a DDS texture from raw bytes, or return a cached handle if the path is already loaded.
    pub fn load_dds(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        path: &str,
        dds_bytes: &[u8],
    ) -> Result<TextureHandle> {
        let normalized = normalize_path(path);

        if let Some(&handle) = self.path_map.get(&normalized) {
            return Ok(handle);
        }

        let texture = Texture::from_dds(device, allocator, queue, command_pool, dds_bytes)
            .with_context(|| format!("Failed to load DDS texture '{}'", path))?;

        let sets = self.allocate_and_write_sets(device, &texture)?;
        let handle = self.textures.len() as TextureHandle;

        self.textures.push(TextureEntry {
            texture,
            descriptor_sets: sets,
        });
        self.path_map.insert(normalized, handle);

        Ok(handle)
    }

    /// Look up a cached texture by path. Returns `None` if not loaded.
    pub fn get_by_path(&self, path: &str) -> Option<TextureHandle> {
        self.path_map.get(&normalize_path(path)).copied()
    }

    /// Handle for the fallback checkerboard texture (always 0).
    pub fn fallback(&self) -> TextureHandle {
        self.fallback_handle
    }

    /// Get the descriptor set for a texture handle and swapchain image index.
    pub fn descriptor_set(&self, handle: TextureHandle, image_index: usize) -> vk::DescriptorSet {
        let entry = &self.textures[handle as usize];
        entry.descriptor_sets[image_index]
    }

    /// Number of loaded textures (including fallback).
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Recreate all descriptor sets for a new swapchain image count.
    ///
    /// Called on swapchain recreation. Destroys the old pool, creates a new one,
    /// and re-writes all texture descriptors. Textures themselves are preserved.
    pub fn recreate_descriptor_sets(
        &mut self,
        device: &ash::Device,
        new_swapchain_image_count: u32,
    ) -> Result<()> {
        // Destroy old pool (frees all sets implicitly).
        unsafe {
            device.destroy_descriptor_pool(self.descriptor_pool, None);
        }

        // Create new pool.
        let max_textures = 1024u32;
        let total_sets = max_textures * new_swapchain_image_count;
        let pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: total_sets,
        };
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(std::slice::from_ref(&pool_size))
            .max_sets(total_sets)
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);

        self.descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .context("Failed to recreate texture descriptor pool")?
        };
        self.swapchain_image_count = new_swapchain_image_count;

        // Re-allocate and write descriptor sets for all textures.
        for entry in &mut self.textures {
            entry.descriptor_sets = Self::allocate_and_write_sets_inner(
                device,
                &entry.texture,
                self.descriptor_set_layout,
                self.descriptor_pool,
                new_swapchain_image_count,
            )?;
        }

        log::info!(
            "TextureRegistry descriptor sets recreated: {} textures × {} images",
            self.textures.len(),
            new_swapchain_image_count,
        );

        Ok(())
    }

    /// Destroy all textures, descriptor pool, and layout.
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for entry in &mut self.textures {
            entry.texture.destroy(device, allocator);
        }
        self.textures.clear();
        self.path_map.clear();

        unsafe {
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }

    /// Allocate descriptor sets for a texture and write the combined image sampler.
    fn allocate_and_write_sets(
        &self,
        device: &ash::Device,
        texture: &Texture,
    ) -> Result<Vec<vk::DescriptorSet>> {
        Self::allocate_and_write_sets_inner(
            device,
            texture,
            self.descriptor_set_layout,
            self.descriptor_pool,
            self.swapchain_image_count,
        )
    }

    fn allocate_and_write_sets_inner(
        device: &ash::Device,
        texture: &Texture,
        layout: vk::DescriptorSetLayout,
        pool: vk::DescriptorPool,
        count: u32,
    ) -> Result<Vec<vk::DescriptorSet>> {
        let layouts = vec![layout; count as usize];

        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);

        let sets = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .context("Failed to allocate texture descriptor sets")?
        };

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

        Ok(sets)
    }
}

/// Normalize a texture path: lowercase, forward slashes.
fn normalize_path(path: &str) -> String {
    path.to_ascii_lowercase().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_backslashes_and_case() {
        assert_eq!(
            normalize_path(r"Textures\Architecture\Walls\Wall01_d.dds"),
            "textures/architecture/walls/wall01_d.dds"
        );
    }

    #[test]
    fn normalize_already_clean() {
        assert_eq!(
            normalize_path("textures/clutter/food/beerbottle.dds"),
            "textures/clutter/food/beerbottle.dds"
        );
    }
}
