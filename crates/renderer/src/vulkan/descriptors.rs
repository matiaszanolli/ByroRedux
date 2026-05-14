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

    /// Derive pool sizes from a layout's bindings (#1030 /
    /// REN-D10-NEW-09/10). Sums `descriptor_count` per
    /// `DescriptorType` across `bindings` then multiplies by
    /// `set_count` (the number of sets that will be allocated from
    /// this pool — typically `MAX_FRAMES_IN_FLIGHT`).
    ///
    /// Lets the layout-bindings array act as the single source of
    /// truth for both `vkCreateDescriptorSetLayout` and pool sizing.
    /// Hand-derived sizes silently lag any binding added to the
    /// layout — the audit caught composite (7 samplers) and svgf
    /// (8+2+1) on that drift. Self-derived sizes prevent it.
    ///
    /// `max_sets` is NOT set here — the caller chooses it separately
    /// (often `set_count` itself, but tests + non-FIF pools can
    /// diverge).
    ///
    /// For pools that back multiple layouts (e.g. bloom's down +
    /// up pyramid layouts share one pool), chain
    /// [`Self::add_layout_bindings`] for each additional layout.
    pub fn from_layout_bindings(
        bindings: &[vk::DescriptorSetLayoutBinding],
        set_count: u32,
    ) -> Self {
        Self::new().add_layout_bindings(bindings, set_count)
    }

    /// Accumulate another layout's bindings into an existing builder.
    /// Lets one pool back multiple layouts (bloom's down + up pyramid
    /// sets, for example) while keeping the "layout is single source
    /// of truth" property of [`Self::from_layout_bindings`]. Entries
    /// with the same `DescriptorType` are merged with the existing
    /// pool sizes (`descriptor_count` summed) rather than emitted as
    /// duplicates — Vulkan requires each pool-sizes entry's type be
    /// unique.
    pub fn add_layout_bindings(
        mut self,
        bindings: &[vk::DescriptorSetLayoutBinding],
        set_count: u32,
    ) -> Self {
        use std::collections::BTreeMap;
        // Per-type totals for this layout. BTreeMap gives a
        // deterministic emission order so a regression test pinning
        // the pool-sizes vec layout stays stable across runs.
        let mut additions: BTreeMap<i32, u32> = BTreeMap::new();
        for b in bindings {
            *additions.entry(b.descriptor_type.as_raw()).or_insert(0) +=
                b.descriptor_count * set_count;
        }
        for (raw_ty, count) in additions {
            let ty = vk::DescriptorType::from_raw(raw_ty);
            if let Some(existing) = self.sizes.iter_mut().find(|s| s.ty == ty) {
                existing.descriptor_count += count;
            } else {
                self.sizes.push(vk::DescriptorPoolSize {
                    ty,
                    descriptor_count: count,
                });
            }
        }
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

    /// Test-only accessor exposing the accumulated pool sizes so the
    /// `from_layout_bindings_*` regression tests can pin the
    /// derivation without `build()`'ing a real Vulkan pool.
    #[cfg(test)]
    pub(crate) fn sizes(&self) -> &[vk::DescriptorPoolSize] {
        &self.sizes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the helper's per-type aggregation. The composite layout
    /// post-#1030 has 7 `COMBINED_IMAGE_SAMPLER` + 1 `UNIFORM_BUFFER`
    /// bindings; deriving from the array must produce exactly those
    /// two entries with the right per-set count.
    #[test]
    fn from_layout_bindings_aggregates_per_type() {
        let bindings: Vec<vk::DescriptorSetLayoutBinding> = (0..7u32)
            .map(|b| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(b)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            })
            .chain(std::iter::once(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(7)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            ))
            .collect();
        let builder = DescriptorPoolBuilder::from_layout_bindings(&bindings, 2);
        let sizes = builder.sizes();
        assert_eq!(sizes.len(), 2, "two distinct DescriptorTypes");
        // BTreeMap iteration order is deterministic across runs;
        // COMBINED_IMAGE_SAMPLER = 1, UNIFORM_BUFFER = 6 — sampler
        // sorts first.
        assert_eq!(sizes[0].ty, vk::DescriptorType::COMBINED_IMAGE_SAMPLER);
        assert_eq!(sizes[0].descriptor_count, 7 * 2);
        assert_eq!(sizes[1].ty, vk::DescriptorType::UNIFORM_BUFFER);
        assert_eq!(sizes[1].descriptor_count, 1 * 2);
    }

    /// Multi-descriptor-count bindings (the bindless case) must scale
    /// by the binding's own `descriptor_count`, not collapse to 1.
    #[test]
    fn from_layout_bindings_respects_per_binding_count() {
        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(1024) // bindless slot count
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
        let builder = DescriptorPoolBuilder::from_layout_bindings(&bindings, 1);
        let sizes = builder.sizes();
        assert_eq!(sizes.len(), 1);
        assert_eq!(sizes[0].descriptor_count, 1024);
    }

    /// Repeated `DescriptorType` across bindings collapses to a
    /// single pool entry with the summed count — matches Vulkan's
    /// requirement that each pool-sizes entry's type is unique.
    #[test]
    fn from_layout_bindings_dedupes_same_type() {
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(2)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        let builder = DescriptorPoolBuilder::from_layout_bindings(&bindings, 3);
        let sizes = builder.sizes();
        assert_eq!(sizes.len(), 1, "STORAGE_BUFFER × 2 bindings collapses");
        assert_eq!(sizes[0].descriptor_count, (1 + 2) * 3);
    }

    /// The core #1030 regression: adding a binding to the layout
    /// silently bumps the derived pool size. Pin this by adding a
    /// binding and re-deriving — the pre-#1030 hand-derived count
    /// would have stayed at the old value and tripped a startup
    /// pool-allocation failure.
    #[test]
    fn from_layout_bindings_tracks_new_bindings() {
        let mut bindings = vec![vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
        let before = DescriptorPoolBuilder::from_layout_bindings(&bindings, 2);
        assert_eq!(before.sizes()[0].descriptor_count, 2);
        // Simulate a future contributor adding a binding to the
        // layout without bumping any pool-size hand-count.
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        let after = DescriptorPoolBuilder::from_layout_bindings(&bindings, 2);
        assert_eq!(
            after.sizes()[0].descriptor_count,
            4,
            "the new binding must lift the derived pool size automatically"
        );
    }
}
