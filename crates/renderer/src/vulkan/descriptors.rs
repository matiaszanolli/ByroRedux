//! Shared descriptor-set and image-barrier helpers.
//!
//! Builders for the three patterns that dominated the renderer:
//!
//! 1. `WriteDescriptorSet` for COMBINED_IMAGE_SAMPLER / STORAGE_IMAGE /
//!    STORAGE_BUFFER / UNIFORM_BUFFER (82+ sites across 10 modules).
//! 2. `ImageMemoryBarrier` for UNDEFINED→GENERAL on a single-mip COLOR
//!    image (the `initialize_layouts` pattern shared by bloom, svgf,
//!    caustic, taa, volumetrics).
//! 3. Descriptor pool construction with size + max-sets boilerplate.
//!
//! Every helper is a pure builder — it produces a `vk::WriteDescriptorSet`
//! / `vk::ImageMemoryBarrier` byte-equivalent to the inline form, so
//! migration sites can swap in without changing barrier sequencing
//! (the project's policy on Vulkan changes that aren't visible to
//! `cargo test` — see the speculative-Vulkan-fixes feedback memo).
//!
//! Note on queue family indices:
//! * `image_barrier_undef_to_general` leaves `src_queue_family_index`
//!   and `dst_queue_family_index` at their `default()` value (0) to
//!   match every existing `initialize_layouts` site. ByroRedux submits
//!   exclusively to graphics queue family 0, so this is functionally
//!   equivalent to `VK_QUEUE_FAMILY_IGNORED` on the dev hardware.
//! * The texture-upload helpers (`image_barrier_undef_to_transfer_dst`
//!   and `image_barrier_transfer_dst_to_shader_read`) explicitly set
//!   `VK_QUEUE_FAMILY_IGNORED` to match the long-standing
//!   `texture::from_rgba` / `record_dds_upload` convention.
//! Tightening barrier helpers to all-IGNORED is a separate change that
//! needs RenderDoc validation; mixing styles here preserves byte-
//! equivalence with the pre-#1046 inline code.

use ash::vk;

// ── WriteDescriptorSet helpers ──────────────────────────────────────

/// `COMBINED_IMAGE_SAMPLER` write — sampler + view + layout per element
/// in `info`.
#[inline]
pub fn write_combined_image_sampler<'a>(
    dst_set: vk::DescriptorSet,
    binding: u32,
    info: &'a [vk::DescriptorImageInfo],
) -> vk::WriteDescriptorSet<'a> {
    vk::WriteDescriptorSet::default()
        .dst_set(dst_set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(info)
}

/// `STORAGE_IMAGE` write — view + layout per element in `info`, sampler
/// field unused (storage images don't carry samplers).
#[inline]
pub fn write_storage_image<'a>(
    dst_set: vk::DescriptorSet,
    binding: u32,
    info: &'a [vk::DescriptorImageInfo],
) -> vk::WriteDescriptorSet<'a> {
    vk::WriteDescriptorSet::default()
        .dst_set(dst_set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
        .image_info(info)
}

/// `STORAGE_BUFFER` write.
#[inline]
pub fn write_storage_buffer<'a>(
    dst_set: vk::DescriptorSet,
    binding: u32,
    info: &'a [vk::DescriptorBufferInfo],
) -> vk::WriteDescriptorSet<'a> {
    vk::WriteDescriptorSet::default()
        .dst_set(dst_set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        .buffer_info(info)
}

/// `UNIFORM_BUFFER` write.
#[inline]
pub fn write_uniform_buffer<'a>(
    dst_set: vk::DescriptorSet,
    binding: u32,
    info: &'a [vk::DescriptorBufferInfo],
) -> vk::WriteDescriptorSet<'a> {
    vk::WriteDescriptorSet::default()
        .dst_set(dst_set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .buffer_info(info)
}

// ── ImageSubresourceRange helpers ────────────────────────────────────

/// `COLOR` aspect, mip 0, single mip, single array layer — the shape
/// every storage-image / single-mip texture barrier uses.
#[inline]
pub fn color_subresource_single_mip() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    }
}

/// `COLOR` aspect, mip 0..`level_count`, single array layer — for
/// multi-mip texture uploads (DDS path).
#[inline]
pub fn color_subresource_mips(level_count: u32) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count,
        base_array_layer: 0,
        layer_count: 1,
    }
}

// ── ImageMemoryBarrier helpers ───────────────────────────────────────

/// UNDEFINED → GENERAL on a single-mip COLOR image with access masks
/// matching the shared `initialize_layouts` pattern across
/// bloom/svgf/caustic/taa/volumetrics:
///   src_access = empty
///   dst_access = SHADER_READ | SHADER_WRITE
/// Caller is responsible for the surrounding `cmd_pipeline_barrier`
/// invocation (TOP_OF_PIPE → COMPUTE_SHADER in every call site today).
#[inline]
pub fn image_barrier_undef_to_general(image: vk::Image) -> vk::ImageMemoryBarrier<'static> {
    vk::ImageMemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::GENERAL)
        .image(image)
        .subresource_range(color_subresource_single_mip())
}

/// UNDEFINED → TRANSFER_DST_OPTIMAL on a (potentially multi-mip) COLOR
/// image. Mirrors the explicit `texture.rs` convention:
///   src_queue_family = dst_queue_family = QUEUE_FAMILY_IGNORED
///   src_access = empty
///   dst_access = TRANSFER_WRITE
/// Used as the first half of a texture upload (UNDEFINED → DST, copy,
/// DST → SHADER_READ).
#[inline]
pub fn image_barrier_undef_to_transfer_dst(
    image: vk::Image,
    mip_count: u32,
) -> vk::ImageMemoryBarrier<'static> {
    vk::ImageMemoryBarrier::default()
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(color_subresource_mips(mip_count))
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
}

/// TRANSFER_DST_OPTIMAL → SHADER_READ_ONLY_OPTIMAL on a (potentially
/// multi-mip) COLOR image. Pair with `image_barrier_undef_to_transfer_dst`.
#[inline]
pub fn image_barrier_transfer_dst_to_shader_read(
    image: vk::Image,
    mip_count: u32,
) -> vk::ImageMemoryBarrier<'static> {
    vk::ImageMemoryBarrier::default()
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(color_subresource_mips(mip_count))
        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::SHADER_READ)
}

// ── Descriptor pool builder ─────────────────────────────────────────

/// Builder for `vk::DescriptorPool` that accumulates `(type, count)`
/// entries and materialises the pool with `build()`. Folds the
/// `DescriptorPoolSize[]` + `DescriptorPoolCreateInfo` + raw
/// `create_descriptor_pool` boilerplate present in 8 pipelines.
///
/// ```ignore
/// let pool = DescriptorPoolBuilder::new()
///     .pool(vk::DescriptorType::COMBINED_IMAGE_SAMPLER, n_samplers)
///     .pool(vk::DescriptorType::UNIFORM_BUFFER, n_ubos)
///     .max_sets(n_sets)
///     .build(device, "ssao descriptor pool")?;
/// ```
pub struct DescriptorPoolBuilder {
    sizes: Vec<vk::DescriptorPoolSize>,
    max_sets: u32,
    flags: vk::DescriptorPoolCreateFlags,
}

impl Default for DescriptorPoolBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DescriptorPoolBuilder {
    pub fn new() -> Self {
        Self {
            sizes: Vec::with_capacity(4),
            max_sets: 0,
            flags: vk::DescriptorPoolCreateFlags::empty(),
        }
    }

    /// Append a `(descriptor_type, count)` pool entry. Calling with the
    /// same `ty` twice is allowed (Vulkan sums the counts).
    pub fn pool(mut self, ty: vk::DescriptorType, descriptor_count: u32) -> Self {
        self.sizes.push(vk::DescriptorPoolSize {
            ty,
            descriptor_count,
        });
        self
    }

    pub fn max_sets(mut self, n: u32) -> Self {
        self.max_sets = n;
        self
    }

    /// Pool create flags (e.g. `FREE_DESCRIPTOR_SET` for pools that
    /// need per-set `vkFreeDescriptorSets`).
    pub fn flags(mut self, flags: vk::DescriptorPoolCreateFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Materialise the pool. `label` is the `anyhow::Context` annotation
    /// emitted if `create_descriptor_pool` fails.
    pub fn build(
        self,
        device: &ash::Device,
        label: &'static str,
    ) -> anyhow::Result<vk::DescriptorPool> {
        use anyhow::Context;
        let info = vk::DescriptorPoolCreateInfo::default()
            .flags(self.flags)
            .pool_sizes(&self.sizes)
            .max_sets(self.max_sets);
        unsafe { device.create_descriptor_pool(&info, None) }.context(label)
    }
}
