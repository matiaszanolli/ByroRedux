//! Texture registry — maps texture paths to GPU-resident textures with per-texture descriptor sets.

use crate::vulkan::allocator::SharedAllocator;
use crate::vulkan::texture::Texture;
use anyhow::{Context, Result};
use ash::vk;
use std::collections::{HashMap, VecDeque};

/// Handle into the TextureRegistry (mirrors MeshHandle pattern).
pub type TextureHandle = u32;

/// Maximum frames in flight — textures must survive this many frames after replacement.
const MAX_FRAMES_IN_FLIGHT: usize = 2;

struct TextureEntry {
    texture: Texture,
    descriptor_sets: Vec<vk::DescriptorSet>,
    /// Ring of replaced textures awaiting deferred destruction.
    /// Textures are pushed on each update_rgba call and only destroyed
    /// when the ring exceeds MAX_FRAMES_IN_FLIGHT entries, guaranteeing
    /// no in-flight command buffer still references them.
    pending_destroy: VecDeque<Texture>,
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
    /// Shared sampler for all textures (LINEAR/REPEAT, max LOD clamped by image).
    pub shared_sampler: vk::Sampler,
    swapchain_image_count: u32,
    max_textures: u32,
}

impl TextureRegistry {
    /// Create a new texture registry (no fallback yet — call `set_fallback` after).
    ///
    /// `max_sampler_anisotropy` is the clamped `maxSamplerAnisotropy`
    /// limit from the physical device (see `DeviceCapabilities`), or
    /// `0.0` if the device does not support `samplerAnisotropy`. When
    /// greater than 1.0 the shared sampler enables anisotropic
    /// filtering — significantly improves ground/wall texture quality
    /// at oblique angles, which is the dominant case for Bethesda
    /// content. See issue #136.
    pub fn new(
        device: &ash::Device,
        swapchain_image_count: u32,
        max_textures: u32,
        max_sampler_anisotropy: f32,
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

        // Shared sampler: LINEAR/REPEAT with max_lod high enough for any mip chain.
        // The actual image's mip count naturally clamps sampling.
        //
        // Anisotropic filtering is enabled when the device exposes
        // samplerAnisotropy and the caller passes a limit > 1.0. The
        // value is already clamped to 16× in DeviceCapabilities, so we
        // just forward it here. See issue #136.
        let anisotropy_enable = max_sampler_anisotropy > 1.0;
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(anisotropy_enable)
            .max_anisotropy(if anisotropy_enable {
                max_sampler_anisotropy
            } else {
                1.0
            })
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .min_lod(0.0)
            .max_lod(16.0);

        let shared_sampler = unsafe {
            device
                .create_sampler(&sampler_info, None)
                .context("Failed to create shared texture sampler")?
        };

        let registry = Self {
            textures: Vec::new(),
            path_map: HashMap::new(),
            fallback_handle: 0,
            descriptor_pool,
            descriptor_set_layout,
            shared_sampler,
            swapchain_image_count,
            max_textures,
        };

        log::info!(
            "TextureRegistry created: pool for {} textures × {} swapchain images, anisotropy {}",
            max_textures,
            swapchain_image_count,
            if anisotropy_enable {
                format!("{:.0}×", max_sampler_anisotropy)
            } else {
                "disabled".to_string()
            },
        );

        Ok(registry)
    }

    /// Register the fallback texture as handle 0. Must be called once after new().
    pub fn set_fallback(&mut self, device: &ash::Device, fallback_texture: Texture) -> Result<()> {
        let sets = self.allocate_and_write_sets(device, &fallback_texture)?;
        self.textures.push(TextureEntry {
            texture: fallback_texture,
            descriptor_sets: sets,
            pending_destroy: VecDeque::new(),
        });
        self.fallback_handle = 0;
        Ok(())
    }

    /// Load a DDS texture from raw bytes, or return a cached handle if the path is already loaded.
    pub fn load_dds(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        path: &str,
        dds_bytes: &[u8],
    ) -> Result<TextureHandle> {
        let normalized = normalize_path(path);

        if let Some(&handle) = self.path_map.get(&normalized) {
            return Ok(handle);
        }

        let texture = Texture::from_dds(
            device,
            allocator,
            queue,
            command_pool,
            dds_bytes,
            self.shared_sampler,
        )
        .with_context(|| format!("Failed to load DDS texture '{}'", path))?;

        let sets = self.allocate_and_write_sets(device, &texture)?;
        let handle = self.textures.len() as TextureHandle;

        self.textures.push(TextureEntry {
            texture,
            descriptor_sets: sets,
            pending_destroy: VecDeque::new(),
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

    /// Register an RGBA texture directly (not from a DDS file or path).
    /// Returns a handle that can be used with `update_rgba` for dynamic updates.
    pub fn register_rgba(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<TextureHandle> {
        let texture = Texture::from_rgba(
            device,
            allocator,
            queue,
            command_pool,
            width,
            height,
            pixels,
            self.shared_sampler,
        )
        .context("Failed to create dynamic RGBA texture")?;

        let sets = self.allocate_and_write_sets(device, &texture)?;
        let handle = self.textures.len() as TextureHandle;

        self.textures.push(TextureEntry {
            texture,
            descriptor_sets: sets,
            pending_destroy: VecDeque::new(),
        });

        Ok(handle)
    }

    /// Replace the texture data for an existing handle with new RGBA pixels.
    ///
    /// Uses deferred destruction: the old texture is kept alive as
    /// `pending_destroy` until the NEXT call, giving in-flight frames
    /// time to finish using it. No device_wait_idle stall.
    pub fn update_rgba(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        handle: TextureHandle,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<()> {
        let entry = &mut self.textures[handle as usize];

        // Drain textures old enough that no in-flight command buffer references them.
        // With MAX_FRAMES_IN_FLIGHT=2, we keep the 2 most recent replacements alive.
        while entry.pending_destroy.len() >= MAX_FRAMES_IN_FLIGHT {
            if let Some(mut old) = entry.pending_destroy.pop_front() {
                old.destroy(device, allocator);
            }
        }

        // Move current texture to pending ring, swap in the new one.
        let mut prev = Texture::from_rgba(
            device,
            allocator,
            queue,
            command_pool,
            width,
            height,
            pixels,
            self.shared_sampler,
        )
        .context("Failed to create updated dynamic RGBA texture")?;
        std::mem::swap(&mut entry.texture, &mut prev);
        entry.pending_destroy.push_back(prev);

        // Re-write the existing descriptor sets to point to the new image.
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(entry.texture.image_view)
            .sampler(entry.texture.sampler);

        for &set in &entry.descriptor_sets {
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

        Ok(())
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
        let total_sets = self.max_textures * new_swapchain_image_count;
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
            for mut pending in entry.pending_destroy.drain(..) {
                pending.destroy(device, allocator);
            }
            entry.texture.destroy(device, allocator);
        }
        self.textures.clear();
        self.path_map.clear();

        unsafe {
            device.destroy_sampler(self.shared_sampler, None);
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
