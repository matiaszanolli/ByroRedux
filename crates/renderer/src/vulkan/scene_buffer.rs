//! Per-frame scene data buffers for multi-light rendering.
//!
//! Manages an SSBO for the light array and a UBO for camera data,
//! with per-frame-in-flight double-buffering to avoid write-after-read hazards.
//! Bound as descriptor set 1 in the pipeline layout.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;

/// Maximum lights we can upload per frame. The SSBO is pre-allocated to this size.
/// 512 lights × 48 bytes = 24 KB per frame — trivial.
const MAX_LIGHTS: usize = 512;

/// Maximum bones we can upload per frame across all skinned meshes.
/// 4096 × 64 B = 256 KB/frame. Slot 0 is a reserved identity fallback
/// (used by rigid vertices through the sum-of-weights escape hatch and
/// by `SkinnedMesh` bones that failed to resolve). The remaining 4095
/// slots are assigned sequentially per skinned mesh, with each mesh
/// consuming `MAX_BONES_PER_MESH` (128) slots for simplicity. That
/// gives ~31 skinned meshes per frame — more than enough for a cell
/// full of actors.
pub const MAX_TOTAL_BONES: usize = 4096;

/// Slot 0 of the bone palette is always the identity matrix.
pub const IDENTITY_BONE_SLOT: u32 = 0;

/// Maximum instances per frame. 8192 × 160 B = 1.28 MB/frame — trivial.
/// Covers large exterior cells with multiple loaded cells (~5000+ references).
pub const MAX_INSTANCES: usize = 8192;

/// Maximum number of `VkDrawIndexedIndirectCommand` entries held in
/// the per-frame indirect buffer. Each entry is 20 bytes, so 8192
/// entries × 20 B = 160 KB per frame × 3 frames-in-flight = 480 KB
/// total. Sized generously — real scenes with instanced batching from
/// #272 rarely emit more than a few hundred entries. See #309.
pub const MAX_INDIRECT_DRAWS: usize = 8192;

/// Per-instance data uploaded to the instance SSBO each frame.
///
/// The vertex shader reads `instances[gl_InstanceIndex]` instead of push
/// constants, enabling instanced drawing: consecutive draws with the same
/// mesh + pipeline can be batched into a single `cmd_draw_indexed` call.
///
/// **CRITICAL**: All fields use scalar types (f32/u32) or vec4-equivalent
/// `[f32; 4]` — NEVER `[f32; 3]`. In std430 layout, a vec3 is aligned to
/// 16 bytes (same as vec4), which would silently mismatch a tightly-packed
/// `#[repr(C)]` Rust struct where `[f32; 3]` is only 12 bytes.
///
/// Layout: 160 bytes per instance, 16-byte aligned (10×16).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuInstance {
    pub model: [[f32; 4]; 4],  // 64 B, offset 0
    pub texture_index: u32,    // 4 B, offset 64
    pub bone_offset: u32,      // 4 B, offset 68
    pub normal_map_index: u32, // 4 B, offset 72
    pub roughness: f32,        // 4 B, offset 76
    pub metalness: f32,        // 4 B, offset 80
    pub emissive_mult: f32,    // 4 B, offset 84
    /// Emissive RGB + specular_strength packed as vec4 to avoid vec3 alignment.
    pub emissive_r: f32, // 4 B, offset 88
    pub emissive_g: f32,       // 4 B, offset 92
    pub emissive_b: f32,       // 4 B, offset 96
    pub specular_strength: f32, // 4 B, offset 100
    /// Specular RGB + padding packed to avoid vec3.
    pub specular_r: f32, // 4 B, offset 104
    pub specular_g: f32,       // 4 B, offset 108
    pub specular_b: f32,       // 4 B, offset 112
    /// Offset into the global vertex SSBO (in vertices, not bytes).
    pub vertex_offset: u32, // 4 B, offset 116
    /// Offset into the global index SSBO (in indices, not bytes).
    pub index_offset: u32, // 4 B, offset 120
    /// Vertex count for this mesh (for bounds checking).
    pub vertex_count: u32, // 4 B, offset 124
    /// Alpha test threshold [0,1]. 0.0 = no alpha test. #263.
    pub alpha_threshold: f32, // 4 B, offset 128
    /// Alpha test comparison function (Gamebryo TestFunction). #263.
    pub alpha_test_func: u32, // 4 B, offset 132
    /// Bindless texture index for dark/lightmap (0 = none). #264.
    pub dark_map_index: u32, // 4 B, offset 136
    /// Pre-computed average albedo for GI bounce approximation.
    /// Avoids 11 divergent memory ops per GI ray hit by replacing
    /// full UV lookup + texture sample with a single SSBO read.
    pub avg_albedo_r: f32, // 4 B, offset 140
    pub avg_albedo_g: f32,     // 4 B, offset 144
    pub avg_albedo_b: f32,     // 4 B, offset 148
    /// Per-instance flags.
    ///   bit 0 — has non-uniform scale (needs inverse-transpose normal transform). See #273.
    ///   bit 1 — `alpha_blend` enabled (NiAlphaProperty blend bit). Used by the
    ///           fragment shader for its `isGlass`/`isWindow` classification.
    ///   bit 2 — caustic source: mesh is a plausible refractive surface
    ///           (alpha-blend, non-metal). The caustic compute pass scatters
    ///           caustic splats from every pixel whose instance has this bit. #321.
    pub flags: u32, // 4 B, offset 152
    /// `BSLightingShaderProperty.shader_type` enum value (0–19), used by
    /// the fragment shader's per-variant dispatch (SkinTint, HairTint,
    /// EyeEnvmap, SparkleSnow, MultiLayerParallax, …). 0 = Default lit
    /// — the safe fall-through for non-Skyrim+ meshes that have no
    /// BSLightingShaderProperty backing. Repurposed from the previous
    /// `_pad1` field so the struct stays at 160 bytes (10×16, std430).
    /// Plumbing only here — the actual variant branches in
    /// `triangle.frag` land per-variant in follow-up PRs. See #344.
    pub material_kind: u32, // 4 B, offset 156 → total 160
                               // Struct is 160 bytes (10×16), 16-byte aligned for std430.
}

impl Default for GpuInstance {
    fn default() -> Self {
        Self {
            model: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            texture_index: 0,
            bone_offset: 0,
            normal_map_index: 0,
            roughness: 0.5,
            metalness: 0.0,
            emissive_mult: 0.0,
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            specular_strength: 1.0,
            specular_r: 1.0,
            specular_g: 1.0,
            specular_b: 1.0,
            vertex_offset: 0,
            index_offset: 0,
            vertex_count: 0,
            alpha_threshold: 0.0,
            alpha_test_func: 0,
            dark_map_index: 0,
            avg_albedo_r: 0.5,
            avg_albedo_g: 0.5,
            avg_albedo_b: 0.5,
            flags: 0,
            material_kind: 0,
        }
    }
}

/// GPU-side light struct (48 bytes, std430 layout).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GpuLight {
    /// xyz = world position, w = radius (Bethesda units).
    pub position_radius: [f32; 4],
    /// rgb = color (0–1), w = type (0=point, 1=spot, 2=directional).
    pub color_type: [f32; 4],
    /// xyz = direction (spot/directional), w = spot outer angle cosine.
    pub direction_angle: [f32; 4],
}

/// GPU-side camera data (256 bytes, std140-compatible).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuCamera {
    /// Combined view-projection matrix (column-major).
    pub view_proj: [[f32; 4]; 4],
    /// Previous frame's view-projection matrix (column-major). Used by the
    /// vertex shader to compute screen-space motion vectors: projecting a
    /// vertex's current world position through both matrices gives the
    /// screen motion that downstream temporal filters (SVGF, TAA) need.
    /// On the very first frame, this equals `view_proj` so motion is zero.
    pub prev_view_proj: [[f32; 4]; 4],
    /// Precomputed `inverse(viewProj)` — used by cluster culling and SSAO
    /// to reconstruct world positions from depth without a per-invocation
    /// matrix inverse on the GPU.
    pub inv_view_proj: [[f32; 4]; 4],
    /// xyz = world position, w = frame counter (for temporal jitter seed).
    pub position: [f32; 4],
    /// x = RT enabled (1.0), y/z/w = ambient light color (RGB).
    pub flags: [f32; 4],
    /// x = screen width, y = screen height, z = fog near, w = fog far.
    pub screen: [f32; 4],
    /// xyz = fog color (RGB 0-1), w = fog enabled (1.0 = yes).
    pub fog: [f32; 4],
    /// xy = sub-pixel projection jitter in NDC space (Halton 2,3 sequence),
    /// applied to `gl_Position.xy` AFTER motion-vector clip positions are
    /// captured so reprojection remains jitter-free. zw = reserved.
    pub jitter: [f32; 4],
}

impl Default for GpuCamera {
    fn default() -> Self {
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        Self {
            view_proj: identity,
            prev_view_proj: identity,
            inv_view_proj: identity,
            position: [0.0; 4],
            flags: [0.0; 4],
            screen: [1280.0, 720.0, 0.0, 0.0],
            fog: [0.0, 0.0, 0.0, 0.0],
            jitter: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

/// SSBO header: lightCount + padding to 16-byte alignment (std430).
#[repr(C)]
#[derive(Clone, Copy)]
struct LightHeader {
    count: u32,
    _pad: [u32; 3],
}

/// Per-frame scene buffers and their descriptor sets.
pub struct SceneBuffers {
    /// One SSBO per frame-in-flight (header + light array).
    light_buffers: Vec<GpuBuffer>,
    /// One UBO per frame-in-flight (camera data).
    camera_buffers: Vec<GpuBuffer>,
    /// One SSBO per frame-in-flight (bone palette for skinning).
    /// Slot 0 is always the identity matrix; the vertex shader uses it
    /// as a fallback for rigid vertices that land here by accident.
    bone_buffers: Vec<GpuBuffer>,
    /// One SSBO per frame-in-flight (per-instance data for instanced drawing).
    /// Each entry contains model matrix + texture index + bone offset.
    instance_buffers: Vec<GpuBuffer>,
    /// One `INDIRECT_BUFFER`-usage buffer per frame-in-flight for
    /// `vkCmdDrawIndexedIndirect`. Holds
    /// `VkDrawIndexedIndirectCommand` entries uploaded CPU-side each
    /// frame. The draw loop collapses consecutive batches sharing
    /// `(pipeline_key, is_decal)` into one indirect draw call reading
    /// a contiguous range of this buffer. See #309.
    indirect_buffers: Vec<GpuBuffer>,
    /// Descriptor pool for scene descriptor sets.
    descriptor_pool: vk::DescriptorPool,
    /// Layout for set 1: binding 0 = SSBO (lights), binding 1 = UBO (camera),
    /// binding 2 = TLAS (RT only), binding 3 = SSBO (bone palette),
    /// binding 4 = SSBO (instance data).
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    /// One descriptor set per frame-in-flight.
    pub descriptor_sets: Vec<vk::DescriptorSet>,
    /// Tracks whether the TLAS binding has been written for each frame.
    pub tlas_written: Vec<bool>,
}

impl SceneBuffers {
    /// Create scene buffers and descriptor infrastructure.
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        rt_enabled: bool,
    ) -> Result<Self> {
        // Calculate buffer sizes.
        let light_buf_size = (std::mem::size_of::<LightHeader>()
            + std::mem::size_of::<GpuLight>() * MAX_LIGHTS)
            as vk::DeviceSize;
        let camera_buf_size = std::mem::size_of::<GpuCamera>() as vk::DeviceSize;
        // Bone palette: 4 × vec4 (mat4) per slot, std430 layout.
        let bone_buf_size =
            (std::mem::size_of::<[[f32; 4]; 4]>() * MAX_TOTAL_BONES) as vk::DeviceSize;
        // Instance SSBO: per-instance model matrix + texture index + bone offset.
        let instance_buf_size =
            (std::mem::size_of::<GpuInstance>() * MAX_INSTANCES) as vk::DeviceSize;
        // Indirect buffer: one VkDrawIndexedIndirectCommand (20 B) per batch. #309.
        let indirect_buf_size = (std::mem::size_of::<vk::DrawIndexedIndirectCommand>()
            * MAX_INDIRECT_DRAWS) as vk::DeviceSize;

        // Create per-frame buffers.
        let mut light_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut camera_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut bone_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut instance_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut indirect_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            light_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                light_buf_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            camera_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                camera_buf_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )?);
            bone_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                bone_buf_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            instance_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                instance_buf_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            indirect_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                indirect_buf_size,
                vk::BufferUsageFlags::INDIRECT_BUFFER,
            )?);
        }

        // Descriptor set layout: set 1.
        let mut bindings = vec![
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            // Camera UBO is read by both vertex (viewProj) and fragment (cameraPos, sceneFlags).
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];
        if rt_enabled {
            bindings.push(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            );
        }
        // Binding 3: bone palette SSBO (vertex shader — skinning).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX),
        );
        // Binding 4: instance data SSBO (vertex + fragment — instanced drawing + PBR materials).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 5: cluster grid SSBO (fragment shader — clustered lighting).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 6: cluster light indices SSBO (fragment shader — clustered lighting).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(6)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 7: SSAO texture (fragment shader — ambient occlusion).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(7)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 8: global vertex SSBO (fragment shader — RT reflection UV lookup).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(8)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        // Binding 9: global index SSBO (fragment shader — RT reflection UV lookup).
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(9)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
        // Mark bindings 5+6 (cluster data) as PARTIALLY_BOUND so they are
        // valid even when unwritten (cluster cull pipeline may fail to create).
        // The fragment shader guards access with a lightCount > 0 check.
        let binding_flags: Vec<vk::DescriptorBindingFlags> = bindings
            .iter()
            .enumerate()
            .map(|(_, b)| {
                let binding_idx = b.binding;
                if binding_idx >= 5 {
                    vk::DescriptorBindingFlags::PARTIALLY_BOUND
                } else {
                    vk::DescriptorBindingFlags::empty()
                }
            })
            .collect();
        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);
        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&bindings)
            .push_next(&mut binding_flags_info);
        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .context("Failed to create scene descriptor set layout")?
        };

        // Descriptor pool.
        // Two STORAGE_BUFFER descriptors per frame (lights + bones).
        let mut pool_sizes = vec![
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                // 7 SSBOs per frame: lights(0), bones(3), instances(4), cluster grid(5), light indices(6), vertices(8), indices(9).
                descriptor_count: (MAX_FRAMES_IN_FLIGHT * 7) as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                // 1 per frame: SSAO texture (binding 7).
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
        ];
        if rt_enabled {
            pool_sizes.push(vk::DescriptorPoolSize {
                ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            });
        }
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32);
        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .context("Failed to create scene descriptor pool")?
        };

        // Allocate descriptor sets.
        let layouts = vec![descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);
        let descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .context("Failed to allocate scene descriptor sets")?
        };

        // Write descriptor sets to point at the buffers.
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let light_buf_info = [vk::DescriptorBufferInfo {
                buffer: light_buffers[i].buffer,
                offset: 0,
                range: light_buf_size,
            }];
            let camera_buf_info = [vk::DescriptorBufferInfo {
                buffer: camera_buffers[i].buffer,
                offset: 0,
                range: camera_buf_size,
            }];
            let bone_buf_info = [vk::DescriptorBufferInfo {
                buffer: bone_buffers[i].buffer,
                offset: 0,
                range: bone_buf_size,
            }];
            let instance_buf_info = [vk::DescriptorBufferInfo {
                buffer: instance_buffers[i].buffer,
                offset: 0,
                range: instance_buf_size,
            }];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&light_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&camera_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&bone_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(4)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&instance_buf_info),
            ];
            unsafe {
                device.update_descriptor_sets(&writes, &[]);
            }
        }

        // Seed slot 0 of each bone palette with the identity matrix so
        // rigid vertices that fall through to the palette (shouldn't
        // happen, but serves as a defensive fallback) produce correct
        // positions rather than collapsing to origin.
        let identity: [[f32; 4]; 4] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        for buf in &mut bone_buffers {
            buf.write_mapped(device, std::slice::from_ref(&identity))?;
        }

        log::info!(
            "Scene buffers created: {} frames, {} max lights ({} bytes/frame)",
            MAX_FRAMES_IN_FLIGHT,
            MAX_LIGHTS,
            light_buf_size,
        );

        Ok(Self {
            light_buffers,
            camera_buffers,
            bone_buffers,
            instance_buffers,
            indirect_buffers,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
            tlas_written: vec![false; MAX_FRAMES_IN_FLIGHT],
        })
    }

    /// Upload light data for the current frame-in-flight.
    pub fn upload_lights(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        lights: &[GpuLight],
    ) -> Result<()> {
        let count = lights.len().min(MAX_LIGHTS);
        let header = LightHeader {
            count: count as u32,
            _pad: [0; 3],
        };

        let header_size = std::mem::size_of::<LightHeader>();
        let light_size = std::mem::size_of::<GpuLight>();

        // Write directly to mapped GPU memory — no intermediate Vec allocation.
        let buf = &mut self.light_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;

        // SAFETY: LightHeader and GpuLight are #[repr(C)] with plain f32/u32 fields.
        // mapped buffer is sized for MAX_LIGHTS. No overlap between header and light regions.
        unsafe {
            std::ptr::copy_nonoverlapping(
                &header as *const LightHeader as *const u8,
                mapped.as_mut_ptr(),
                header_size,
            );
            if count > 0 {
                std::ptr::copy_nonoverlapping(
                    lights.as_ptr() as *const u8,
                    mapped.as_mut_ptr().add(header_size),
                    light_size * count,
                );
            }
        }

        buf.flush_if_needed(device)
    }

    /// Upload camera data for the current frame-in-flight.
    pub fn upload_camera(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        camera: &GpuCamera,
    ) -> Result<()> {
        self.camera_buffers[frame_index].write_mapped(device, std::slice::from_ref(camera))
    }

    /// Upload the bone palette for the current frame-in-flight.
    ///
    /// `palette` is packed contiguous mat4 entries in column-major glam
    /// layout. Slot 0 is always the identity matrix — callers that
    /// assemble multiple meshes into one palette should keep slot 0 as
    /// identity and start writing mesh bones at slot 1.
    ///
    /// Writes at most `MAX_TOTAL_BONES` entries; extra are silently
    /// clamped and logged once per session by the caller.
    pub fn upload_bones(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        palette: &[[[f32; 4]; 4]],
    ) -> Result<()> {
        let count = palette.len().min(MAX_TOTAL_BONES);
        if count == 0 {
            return Ok(());
        }

        let buf = &mut self.bone_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        // SAFETY: [[f32; 4]; 4] is #[repr(C)]-compatible with std430 mat4.
        // bone_buffers are sized for MAX_TOTAL_BONES slots; count is clamped.
        unsafe {
            std::ptr::copy_nonoverlapping(
                palette.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                std::mem::size_of::<[[f32; 4]; 4]>() * count,
            );
        }
        buf.flush_if_needed(device)
    }

    /// Upload per-instance data for the current frame-in-flight.
    ///
    /// Called once per frame before the render pass. The vertex shader reads
    /// `instances[gl_InstanceIndex]` for model matrix, texture index, and bone offset.
    pub fn upload_instances(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        instances: &[GpuInstance],
    ) -> Result<()> {
        let count = instances.len().min(MAX_INSTANCES);
        if instances.len() > MAX_INSTANCES {
            log::warn!(
                "Instance SSBO overflow: {} instances submitted, capped at {} — excess draws silently dropped. #279 P2-12",
                instances.len(),
                MAX_INSTANCES,
            );
        }
        if count == 0 {
            return Ok(());
        }
        let buf = &mut self.instance_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        let byte_size = std::mem::size_of::<GpuInstance>() * count;
        // SAFETY: GpuInstance is #[repr(C)] with plain f32/u32 fields.
        // instance_buffers are sized for MAX_INSTANCES; count is clamped.
        unsafe {
            std::ptr::copy_nonoverlapping(
                instances.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size,
            );
        }
        buf.flush_if_needed(device)
    }

    /// Get a mutable reference to the mapped instance buffer for direct writes.
    /// Used by the UI overlay to append a single instance after the bulk upload.
    pub fn instance_buffer_mapped_mut(&mut self, frame_index: usize) -> Result<&mut [u8]> {
        self.instance_buffers[frame_index].mapped_slice_mut()
    }

    /// Upload `VkDrawIndexedIndirectCommand` entries for the current
    /// frame-in-flight. The draw loop issues one
    /// `vkCmdDrawIndexedIndirect` per pipeline group, reading a
    /// contiguous range of this buffer. See #309.
    ///
    /// Clamps at [`MAX_INDIRECT_DRAWS`] and logs a warn on overflow —
    /// real scenes with the #272 instanced batching rarely emit more
    /// than a few hundred batches per frame, so the clamp is a
    /// defense-in-depth against unbounded-growth bugs.
    pub fn upload_indirect_draws(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        draws: &[vk::DrawIndexedIndirectCommand],
    ) -> Result<()> {
        let count = draws.len().min(MAX_INDIRECT_DRAWS);
        if draws.len() > MAX_INDIRECT_DRAWS {
            log::warn!(
                "Indirect draw overflow: {} commands submitted, capped at {} — excess draws silently dropped",
                draws.len(),
                MAX_INDIRECT_DRAWS,
            );
        }
        if count == 0 {
            return Ok(());
        }
        let buf = &mut self.indirect_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        let byte_size = std::mem::size_of::<vk::DrawIndexedIndirectCommand>() * count;
        // SAFETY: VkDrawIndexedIndirectCommand is a Vulkan-defined C struct
        // with the exact layout expected by the device. `indirect_buffers`
        // are sized for MAX_INDIRECT_DRAWS; count is clamped above.
        unsafe {
            std::ptr::copy_nonoverlapping(
                draws.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size,
            );
        }
        buf.flush_if_needed(device)
    }

    /// Return the `VkBuffer` handle for the current frame's indirect
    /// buffer. The draw loop passes this to `cmd_draw_indexed_indirect`.
    pub fn indirect_buffer(&self, frame_index: usize) -> vk::Buffer {
        self.indirect_buffers[frame_index].buffer
    }

    /// Get the light buffers (for compute pipeline descriptor writes).
    pub fn light_buffers(&self) -> &[GpuBuffer] {
        &self.light_buffers
    }

    /// Get the camera buffers (for compute pipeline descriptor writes).
    pub fn camera_buffers(&self) -> &[GpuBuffer] {
        &self.camera_buffers
    }

    /// Get the instance buffers (for the caustic pipeline's descriptor writes).
    pub fn instance_buffers(&self) -> &[GpuBuffer] {
        &self.instance_buffers
    }

    /// Light buffer size in bytes.
    pub fn light_buffer_size(&self) -> vk::DeviceSize {
        (std::mem::size_of::<LightHeader>() + std::mem::size_of::<GpuLight>() * MAX_LIGHTS)
            as vk::DeviceSize
    }

    /// Camera buffer size in bytes.
    pub fn camera_buffer_size(&self) -> vk::DeviceSize {
        std::mem::size_of::<GpuCamera>() as vk::DeviceSize
    }

    /// Instance buffer size in bytes.
    pub fn instance_buffer_size(&self) -> vk::DeviceSize {
        (std::mem::size_of::<GpuInstance>() * MAX_INSTANCES) as vk::DeviceSize
    }

    /// Get the descriptor set for the current frame-in-flight.
    pub fn descriptor_set(&self, frame_index: usize) -> vk::DescriptorSet {
        self.descriptor_sets[frame_index]
    }

    /// Write the SSAO texture into the scene descriptor set for a given frame.
    pub fn write_ao_texture(
        &self,
        device: &ash::Device,
        frame_index: usize,
        ao_image_view: vk::ImageView,
        ao_sampler: vk::Sampler,
    ) {
        let image_info = [vk::DescriptorImageInfo::default()
            .sampler(ao_sampler)
            .image_view(ao_image_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_sets[frame_index])
            .dst_binding(7)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_info);
        unsafe {
            device.update_descriptor_sets(&[write], &[]);
        }
    }

    /// Write global geometry SSBO references for RT reflection UV lookups.
    pub fn write_geometry_buffers(
        &self,
        device: &ash::Device,
        frame_index: usize,
        vertex_buffer: vk::Buffer,
        vertex_size: vk::DeviceSize,
        index_buffer: vk::Buffer,
        index_size: vk::DeviceSize,
    ) {
        let vert_info = [vk::DescriptorBufferInfo {
            buffer: vertex_buffer,
            offset: 0,
            range: vertex_size,
        }];
        let idx_info = [vk::DescriptorBufferInfo {
            buffer: index_buffer,
            offset: 0,
            range: index_size,
        }];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_sets[frame_index])
                .dst_binding(8)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&vert_info),
            vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_sets[frame_index])
                .dst_binding(9)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&idx_info),
        ];
        unsafe { device.update_descriptor_sets(&writes, &[]) }
    }

    /// Write cluster buffer references into the scene descriptor set for a given frame.
    /// Called once during init after the cluster cull pipeline is created.
    pub fn write_cluster_buffers(
        &self,
        device: &ash::Device,
        frame_index: usize,
        grid_buffer: vk::Buffer,
        grid_size: vk::DeviceSize,
        index_buffer: vk::Buffer,
        index_size: vk::DeviceSize,
    ) {
        let grid_info = [vk::DescriptorBufferInfo {
            buffer: grid_buffer,
            offset: 0,
            range: grid_size,
        }];
        let index_info = [vk::DescriptorBufferInfo {
            buffer: index_buffer,
            offset: 0,
            range: index_size,
        }];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_sets[frame_index])
                .dst_binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&grid_info),
            vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_sets[frame_index])
                .dst_binding(6)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&index_info),
        ];
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }
    }

    /// Update the TLAS acceleration structure in the descriptor set for a given frame.
    pub fn write_tlas(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        tlas: vk::AccelerationStructureKHR,
    ) {
        self.tlas_written[frame_index] = true;
        let accel_structs = [tlas];
        let mut accel_write = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accel_structs);

        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_sets[frame_index])
            .dst_binding(2)
            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
            .descriptor_count(1)
            .push_next(&mut accel_write);

        unsafe {
            device.update_descriptor_sets(&[write], &[]);
        }
    }

    /// Destroy all resources.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for buf in &mut self.light_buffers {
            buf.destroy(device, allocator);
        }
        for buf in &mut self.camera_buffers {
            buf.destroy(device, allocator);
        }
        for buf in &mut self.bone_buffers {
            buf.destroy(device, allocator);
        }
        for buf in &mut self.instance_buffers {
            buf.destroy(device, allocator);
        }
        for buf in &mut self.indirect_buffers {
            buf.destroy(device, allocator);
        }
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}

#[cfg(test)]
mod gpu_instance_layout_tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    /// Regression guard for the Shader Struct Sync invariant (#318 / #417).
    /// The `GpuInstance` struct is duplicated across **four** GLSL
    /// sources — `triangle.vert`, `triangle.frag`, `ui.vert`, and (since
    /// the caustic pass #321) `caustic_splat.comp` — and must stay
    /// byte-for-byte identical with the Rust definition. Any drift here
    /// silently corrupts per-instance data on the GPU. Verified offsets
    /// come from the explicit `// offset N` comments inside those shaders.
    /// See the `feedback_shader_struct_sync` memory note for the lockstep
    /// update protocol (grep for `struct GpuInstance` in the shaders tree
    /// before touching this struct).
    #[test]
    fn gpu_instance_is_160_bytes_std430_compatible() {
        assert_eq!(
            size_of::<GpuInstance>(),
            160,
            "GpuInstance must stay 160 B to match std430 shader layout"
        );
    }

    #[test]
    fn gpu_instance_field_offsets_match_shader_contract() {
        assert_eq!(offset_of!(GpuInstance, model), 0);
        assert_eq!(offset_of!(GpuInstance, texture_index), 64);
        assert_eq!(offset_of!(GpuInstance, bone_offset), 68);
        assert_eq!(offset_of!(GpuInstance, normal_map_index), 72);
        assert_eq!(offset_of!(GpuInstance, roughness), 76);
        assert_eq!(offset_of!(GpuInstance, metalness), 80);
        assert_eq!(offset_of!(GpuInstance, emissive_mult), 84);
        assert_eq!(offset_of!(GpuInstance, emissive_r), 88);
        assert_eq!(offset_of!(GpuInstance, emissive_g), 92);
        assert_eq!(offset_of!(GpuInstance, emissive_b), 96);
        assert_eq!(offset_of!(GpuInstance, specular_strength), 100);
        assert_eq!(offset_of!(GpuInstance, specular_r), 104);
        assert_eq!(offset_of!(GpuInstance, specular_g), 108);
        assert_eq!(offset_of!(GpuInstance, specular_b), 112);
        assert_eq!(offset_of!(GpuInstance, vertex_offset), 116);
        assert_eq!(offset_of!(GpuInstance, index_offset), 120);
        assert_eq!(offset_of!(GpuInstance, vertex_count), 124);
        assert_eq!(offset_of!(GpuInstance, alpha_threshold), 128);
        assert_eq!(offset_of!(GpuInstance, alpha_test_func), 132);
        assert_eq!(offset_of!(GpuInstance, dark_map_index), 136);
        assert_eq!(offset_of!(GpuInstance, avg_albedo_r), 140);
        assert_eq!(offset_of!(GpuInstance, avg_albedo_g), 144);
        assert_eq!(offset_of!(GpuInstance, avg_albedo_b), 148);
        assert_eq!(offset_of!(GpuInstance, flags), 152);
        // material_kind reuses the previous _pad1 slot — kept at the
        // same offset so every shader-side `pad1` reference renamed to
        // `materialKind` continues to alias the same 4 bytes. See #344.
        assert_eq!(offset_of!(GpuInstance, material_kind), 156);
    }

    /// Regression: #309 — `VkDrawIndexedIndirectCommand` is a Vulkan-
    /// specified C struct that `cmd_draw_indexed_indirect` reads
    /// directly from the device-side buffer. Its layout is part of
    /// the Vulkan contract (20 bytes, five u32 fields in a fixed
    /// order). Guard the size so a future `ash` upgrade that
    /// accidentally renames / reorders fields breaks the test
    /// instead of silently producing garbage draw params.
    #[test]
    fn draw_indexed_indirect_command_is_20_bytes() {
        assert_eq!(
            size_of::<vk::DrawIndexedIndirectCommand>(),
            20,
            "VkDrawIndexedIndirectCommand must be 20 bytes (5 × u32) per the Vulkan spec"
        );
    }

    /// Regression: #309 — `upload_indirect_draws` clamps at
    /// `MAX_INDIRECT_DRAWS` so a future bug that produces an
    /// unbounded batch list can't overflow the indirect buffer.
    /// 8192 × 20 B = 160 KB per frame; the allocation matches.
    #[test]
    fn indirect_buffer_capacity_matches_max_draw_constant() {
        let bytes_per_command = size_of::<vk::DrawIndexedIndirectCommand>();
        assert_eq!(bytes_per_command, 20);
        assert_eq!(
            bytes_per_command * MAX_INDIRECT_DRAWS,
            20 * 8192,
            "MAX_INDIRECT_DRAWS × sizeof(VkDrawIndexedIndirectCommand) \
             must match the per-frame indirect buffer allocation"
        );
    }

    /// Regression: #417 — every shader that declares its own copy of
    /// `struct GpuInstance` must name the final u32 slot
    /// `materialKind`, not `_pad1` or any other legacy placeholder.
    /// The Rust side guards offsets via
    /// `gpu_instance_field_offsets_match_shader_contract`; this test
    /// guards name-level drift across the four shader copies so a
    /// future refactor that actually reads the field (currently unused
    /// on the caustic path) doesn't silently alias it to padding.
    ///
    /// Walks the shaders tree at compile time via `include_str!` —
    /// works in `cargo test` even on machines that don't have
    /// glslangValidator installed, and catches the missed-rename
    /// failure mode from #417 (caustic_splat.comp still said
    /// `uint _pad1;` after the triangle.* / ui.vert rename).
    #[test]
    fn every_shader_struct_gpu_instance_names_material_kind_slot() {
        const SOURCES: &[(&str, &str)] = &[
            (
                "triangle.vert",
                include_str!("../../shaders/triangle.vert"),
            ),
            (
                "triangle.frag",
                include_str!("../../shaders/triangle.frag"),
            ),
            ("ui.vert", include_str!("../../shaders/ui.vert")),
            (
                "caustic_splat.comp",
                include_str!("../../shaders/caustic_splat.comp"),
            ),
        ];
        for (name, src) in SOURCES {
            assert!(
                src.contains("struct GpuInstance"),
                "{name} no longer declares `struct GpuInstance` — update \
                 the sync list at feedback_shader_struct_sync.md"
            );
            // The struct must declare the trailing slot as
            // `materialKind`, not `_pad1` / `_pad` / `pad1`.
            assert!(
                src.contains("materialKind"),
                "{name}: GpuInstance final slot must be named \
                 `materialKind` (see #417). Found no mention — did \
                 the shader revert to `_pad1`?"
            );
            assert!(
                !src.contains("uint _pad1"),
                "{name}: GpuInstance slot is still named `_pad1` — \
                 rename to `materialKind` to match the other 3 \
                 shaders (Shader Struct Sync invariant #318 / #417)."
            );
        }
    }
}
