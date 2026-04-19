//! Bindless texture registry — all textures in a single descriptor array.
//!
//! Instead of per-texture descriptor sets, all textures live in a global
//! `sampler2D textures[]` array (Vulkan descriptor indexing). The draw
//! loop binds this once per frame; the fragment shader indexes into it
//! via a per-instance `texture_index` from the instance SSBO.
//!
//! Two copies of the bindless set exist (one per frame-in-flight) to avoid
//! descriptor write hazards when `update_rgba` replaces a texture while
//! another frame's command buffer is still executing.

use crate::vulkan::allocator::SharedAllocator;
use crate::vulkan::texture::Texture;
use anyhow::{Context, Result};
use ash::vk;
use std::collections::{HashMap, VecDeque};

/// Handle into the TextureRegistry (index into the bindless array).
pub type TextureHandle = u32;

/// Maximum frames in flight — textures must survive this many frames after replacement.
const MAX_FRAMES_IN_FLIGHT: usize = 2;

struct TextureEntry {
    /// Live texture, or `None` after the handle has been dropped via
    /// [`TextureRegistry::drop_texture`]. Bindless indexing still works
    /// for dropped handles: they are redirected to the fallback
    /// checkerboard so a stale draw call degrades gracefully instead of
    /// reading a freed `VkImageView`. See #372.
    texture: Option<Texture>,
    /// Ring of replaced / dropped textures awaiting deferred destruction.
    /// See issues #134, #372.
    pending_destroy: VecDeque<(u64, Texture)>,
}

/// Bindless texture registry.
///
/// All textures are stored in a `sampler2D textures[max_textures]` descriptor
/// array. Two copies exist (per frame-in-flight) for safe descriptor updates.
pub struct TextureRegistry {
    textures: Vec<TextureEntry>,
    path_map: HashMap<String, TextureHandle>,
    fallback_handle: TextureHandle,
    descriptor_pool: vk::DescriptorPool,
    /// Layout for set 0: binding 0 = sampler2D[max_textures], PARTIALLY_BOUND.
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    /// One bindless descriptor set per frame-in-flight.
    bindless_sets: Vec<vk::DescriptorSet>,
    /// Shared sampler for all textures.
    pub shared_sampler: vk::Sampler,
    max_textures: u32,
    /// Monotonic frame counter for deferred-destroy aging (issue #134).
    current_frame_id: u64,
}

impl TextureRegistry {
    /// Create a new bindless texture registry.
    ///
    /// Requires `descriptorBindingPartiallyBound` and `runtimeDescriptorArray`
    /// to be enabled on the device (Vulkan 1.2 core features).
    pub fn new(
        device: &ash::Device,
        _swapchain_image_count: u32, // unused with bindless — kept for API compat
        max_textures: u32,
        max_sampler_anisotropy: f32,
    ) -> Result<Self> {
        // Descriptor set layout: binding 0 = sampler2D[max_textures].
        // PARTIALLY_BOUND allows uninitialized array elements (the shader
        // only accesses indices that correspond to loaded textures).
        // UPDATE_AFTER_BIND allows writing new texture descriptors to a set
        // while a prior frame's command buffer still references it — safe
        // because only previously-unbound array indices are written.
        let binding_flags = [vk::DescriptorBindingFlags::PARTIALLY_BOUND
            | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND];
        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);

        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(max_textures)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);

        // Validate against every shader that consumes the bindless texture
        // array — triangle.frag, ui.frag, composite.vert/frag (#427).
        // Composite references the bindless array via set=1 in the pipeline
        // layout, but validation here asserts set=0 agreement with triangle/ui.
        crate::vulkan::reflect::validate_set_layout(
            0,
            std::slice::from_ref(&binding),
            &[
                crate::vulkan::reflect::ReflectedShader {
                    name: "triangle.frag",
                    spirv: crate::vulkan::pipeline::TRIANGLE_FRAG_SPV,
                },
                crate::vulkan::reflect::ReflectedShader {
                    name: "ui.frag",
                    spirv: crate::vulkan::pipeline::UI_FRAG_SPV,
                },
            ],
            "bindless textures (set=0)",
            &[],
        )
        .expect("bindless texture layout drifted against triangle/ui frag shaders (see #427)");

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .bindings(std::slice::from_ref(&binding))
            .push_next(&mut binding_flags_info);

        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .context("Failed to create bindless texture descriptor set layout")?
        };

        // Pool: 2 sets (per frame-in-flight), each with max_textures samplers.
        let pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: max_textures * MAX_FRAMES_IN_FLIGHT as u32,
        };
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND)
            .pool_sizes(std::slice::from_ref(&pool_size))
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32);

        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .context("Failed to create bindless texture descriptor pool")?
        };

        // Allocate per-frame-in-flight descriptor sets.
        let layouts = vec![descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);
        let bindless_sets = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .context("Failed to allocate bindless texture descriptor sets")?
        };

        // Shared sampler: LINEAR/REPEAT with anisotropic filtering.
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

        log::info!(
            "TextureRegistry created: bindless array[{}] × {} frames, anisotropy {}",
            max_textures,
            MAX_FRAMES_IN_FLIGHT,
            if anisotropy_enable {
                format!("{:.0}×", max_sampler_anisotropy)
            } else {
                "disabled".to_string()
            },
        );

        Ok(Self {
            textures: Vec::new(),
            path_map: HashMap::new(),
            fallback_handle: 0,
            descriptor_pool,
            descriptor_set_layout,
            bindless_sets,
            shared_sampler,
            max_textures,
            current_frame_id: 0,
        })
    }

    /// Register the fallback texture as handle 0. Must be called once after new().
    pub fn set_fallback(&mut self, device: &ash::Device, fallback_texture: Texture) -> Result<()> {
        let handle = self.textures.len() as TextureHandle;
        self.write_texture_to_all_sets(device, handle, &fallback_texture);
        self.textures.push(TextureEntry {
            texture: Some(fallback_texture),
            pending_destroy: VecDeque::new(),
        });
        self.fallback_handle = handle;
        Ok(())
    }

    /// Load a DDS texture from raw bytes, or return a cached handle if already loaded.
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

        let handle = self.textures.len() as TextureHandle;
        self.write_texture_to_all_sets(device, handle, &texture);
        self.textures.push(TextureEntry {
            texture: Some(texture),
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

    /// Get the bindless descriptor set for a frame-in-flight.
    ///
    /// This single set contains ALL textures. Bind it once per frame —
    /// the fragment shader indexes into `textures[texture_index]` via
    /// the per-instance data.
    pub fn descriptor_set(&self, frame_index: usize) -> vk::DescriptorSet {
        self.bindless_sets[frame_index]
    }

    /// Register an RGBA texture directly (for dynamic UI textures).
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

        let handle = self.textures.len() as TextureHandle;
        self.write_texture_to_all_sets(device, handle, &texture);
        self.textures.push(TextureEntry {
            texture: Some(texture),
            pending_destroy: VecDeque::new(),
        });

        Ok(handle)
    }

    /// Drop a texture. Its GPU resources move into the deferred-destroy
    /// ring, the bindless descriptor slot is redirected to the fallback
    /// checkerboard (so any stale draw call degrades gracefully instead
    /// of sampling a freed `VkImageView`), and the path-cache entry is
    /// purged so a re-upload of the same path produces a fresh handle.
    ///
    /// Handles stay stable: the dropped slot retains its index in the
    /// bindless array forever — reuse would produce silent material
    /// corruption on any dangling `GpuInstance.texture_index` reference.
    /// See #372. No-op on an unknown or already-dropped handle.
    pub fn drop_texture(&mut self, device: &ash::Device, handle: TextureHandle) {
        let Some(entry) = self.textures.get_mut(handle as usize) else {
            return;
        };
        let Some(old) = entry.texture.take() else {
            return;
        };
        entry
            .pending_destroy
            .push_back((self.current_frame_id, old));

        // Redirect the bindless slot to the fallback texture so any
        // GpuInstance still referencing this handle reads the
        // checkerboard instead of a freed image view.
        let fallback_idx = self.fallback_handle as usize;
        if fallback_idx < self.textures.len() {
            if let Some(fallback) = self.textures[fallback_idx].texture.as_ref() {
                let image_view = fallback.image_view;
                let sampler = fallback.sampler;
                let image_info = vk::DescriptorImageInfo::default()
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image_view(image_view)
                    .sampler(sampler);
                for &set in &self.bindless_sets {
                    let write = vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(0)
                        .dst_array_element(handle)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(std::slice::from_ref(&image_info));
                    unsafe {
                        device.update_descriptor_sets(&[write], &[]);
                    }
                }
            }
        }

        // Purge path_map entries pointing at this handle so a subsequent
        // load_dds of the same path creates a fresh texture. Linear scan
        // is fine: drops happen on cell unload, not per-frame.
        self.path_map.retain(|_, &mut h| h != handle);
    }

    /// Drain deferred-destroy queues across all entries, destroying
    /// textures whose age is now `>= MAX_FRAMES_IN_FLIGHT`. Called once
    /// per frame alongside the mesh/BLAS deferred-destroy ticks. The
    /// `update_rgba` path also drains inline; this pass catches entries
    /// queued by [`drop_texture`] where no subsequent update call fires.
    pub fn tick_deferred_destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        let current_frame_id = self.current_frame_id;
        for entry in &mut self.textures {
            while let Some(&(queued, _)) = entry.pending_destroy.front() {
                if !should_destroy_pending(current_frame_id, queued) {
                    break;
                }
                if let Some((_, mut old)) = entry.pending_destroy.pop_front() {
                    old.destroy(device, allocator);
                }
            }
        }
    }

    /// Advance the frame counter for deferred-destroy aging (issue #134).
    pub fn begin_frame(&mut self) {
        self.current_frame_id = self.current_frame_id.wrapping_add(1);
    }

    #[cfg(test)]
    pub(crate) fn current_frame_id(&self) -> u64 {
        self.current_frame_id
    }

    /// Replace the texture data for an existing handle with new RGBA pixels.
    ///
    /// Uses deferred destruction: the replaced texture is kept alive until
    /// `MAX_FRAMES_IN_FLIGHT` frames have elapsed. See issue #134.
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
        let current_frame_id = self.current_frame_id;
        let entry = &mut self.textures[handle as usize];

        // Drain old textures that have aged out.
        while let Some(&(queued, _)) = entry.pending_destroy.front() {
            if !should_destroy_pending(current_frame_id, queued) {
                break;
            }
            if let Some((_, mut old)) = entry.pending_destroy.pop_front() {
                old.destroy(device, allocator);
            }
        }

        // Swap in the new texture. If the entry was dropped earlier this
        // quietly revives it (bindless slot reactivates on the descriptor
        // write below).
        let new_texture = Texture::from_rgba(
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
        if let Some(prev) = entry.texture.replace(new_texture) {
            entry.pending_destroy.push_back((current_frame_id, prev));
        }

        // Update the bindless array entry in all frame sets.
        // Extract the data we need before re-borrowing self.
        let live = self.textures[handle as usize]
            .texture
            .as_ref()
            .expect("entry was just populated above");
        let image_view = live.image_view;
        let sampler = live.sampler;
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(image_view)
            .sampler(sampler);
        for &set in &self.bindless_sets {
            let write = vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .dst_array_element(handle)
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

    /// Recreate descriptor sets for a new swapchain.
    ///
    /// With bindless textures, the descriptor sets are independent of swapchain
    /// image count. This method recreates them to ensure a clean state and
    /// re-writes all texture bindings.
    pub fn recreate_descriptor_sets(
        &mut self,
        device: &ash::Device,
        _new_swapchain_image_count: u32,
    ) -> Result<()> {
        // Destroy old pool (frees all sets implicitly).
        unsafe {
            device.destroy_descriptor_pool(self.descriptor_pool, None);
        }

        // Recreate pool + sets (must match new() flags: UPDATE_AFTER_BIND).
        let pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: self.max_textures * MAX_FRAMES_IN_FLIGHT as u32,
        };
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND)
            .pool_sizes(std::slice::from_ref(&pool_size))
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32);
        self.descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .context("Failed to recreate bindless texture descriptor pool")?
        };

        let layouts = vec![self.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(&layouts);
        self.bindless_sets = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .context("Failed to reallocate bindless texture descriptor sets")?
        };

        // Re-write all texture bindings. Skip dropped slots — the new
        // descriptor set starts fresh, and the loop in drop_texture will
        // redirect them to the fallback on their next update.
        for (i, entry) in self.textures.iter().enumerate() {
            if let Some(ref texture) = entry.texture {
                self.write_texture_to_sets_inner(device, i as TextureHandle, texture);
            }
        }

        log::info!(
            "TextureRegistry descriptor sets recreated: {} textures (bindless)",
            self.textures.len(),
        );

        Ok(())
    }

    /// Destroy all textures, descriptor pool, and layout.
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for entry in &mut self.textures {
            for (_, mut pending) in entry.pending_destroy.drain(..) {
                pending.destroy(device, allocator);
            }
            if let Some(mut t) = entry.texture.take() {
                t.destroy(device, allocator);
            }
        }
        self.textures.clear();
        self.path_map.clear();

        unsafe {
            device.destroy_sampler(self.shared_sampler, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }

    /// Write a texture's image view + sampler to all per-frame bindless sets.
    fn write_texture_to_all_sets(
        &self,
        device: &ash::Device,
        handle: TextureHandle,
        texture: &Texture,
    ) {
        self.write_texture_to_sets_inner(device, handle, texture);
    }

    fn write_texture_to_sets_inner(
        &self,
        device: &ash::Device,
        handle: TextureHandle,
        texture: &Texture,
    ) {
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.image_view)
            .sampler(texture.sampler);

        for &set in &self.bindless_sets {
            let write = vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .dst_array_element(handle)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(std::slice::from_ref(&image_info));

            unsafe {
                device.update_descriptor_sets(&[write], &[]);
            }
        }
    }
}

/// Normalize a texture path: lowercase, forward slashes.
fn normalize_path(path: &str) -> String {
    path.to_ascii_lowercase().replace('\\', "/")
}

fn should_destroy_pending(current_frame: u64, queued_frame: u64) -> bool {
    current_frame.wrapping_sub(queued_frame) >= MAX_FRAMES_IN_FLIGHT as u64
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

    #[test]
    fn should_destroy_pending_honors_frame_gap() {
        assert!(!should_destroy_pending(0, 0));
        assert!(!should_destroy_pending(1, 0));
        assert!(should_destroy_pending(MAX_FRAMES_IN_FLIGHT as u64, 0));
        assert!(should_destroy_pending(1000, 0));
    }

    #[test]
    fn multiple_same_frame_calls_do_not_authorize_destruction() {
        let current_frame = 10;
        for _ in 0..5 {
            assert!(!should_destroy_pending(current_frame, current_frame));
        }
        assert!(should_destroy_pending(
            current_frame + MAX_FRAMES_IN_FLIGHT as u64,
            current_frame
        ));
    }

    #[test]
    fn frame_counter_math_is_wrap_safe() {
        let queued = u64::MAX - 1;
        let current = queued.wrapping_add(MAX_FRAMES_IN_FLIGHT as u64);
        assert!(should_destroy_pending(current, queued));
        let current = queued.wrapping_add(MAX_FRAMES_IN_FLIGHT as u64 - 1);
        assert!(!should_destroy_pending(current, queued));
    }
}
