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
/// 32768 × 64 B = 2 MB/frame × 3 frames-in-flight = 6 MB total. Slot 0
/// is a reserved identity fallback (used by rigid vertices through
/// the sum-of-weights escape hatch and by `SkinnedMesh` bones that
/// failed to resolve). The remaining slots are assigned sequentially
/// per skinned mesh, with each mesh consuming `MAX_BONES_PER_MESH`
/// (128) slots for simplicity. That gives ~255 skinned meshes per
/// frame — covers ~36 NPCs at 7 skinned meshes each (skeleton + body
/// + 6 sub-meshes) plus rigid scene content. Pre-M41.0 the cap was
/// 4096 (~31 meshes) which suited the no-NPC-spawn baseline; once
/// M41.0 Phase 1b started spawning multiple actors per cell the
/// silent-bind-pose-fallback hid spawned NPCs (FNV Prospector
/// rendered the first ~4 actors then dropped the rest). The proper
/// fix is variable-stride packing (M29.5); this constant just buys
/// headroom until then. See `bone_palette` overflow path in
/// `byroredux/src/render.rs:216`.
pub const MAX_TOTAL_BONES: usize = 32768;

/// Slot 0 of the bone palette is always the identity matrix.
pub const IDENTITY_BONE_SLOT: u32 = 0;

/// Maximum instances per frame. 8192 × 400 B = 3.1 MB/frame — trivial.
/// Covers large exterior cells with multiple loaded cells (~5000+ references).
pub const MAX_INSTANCES: usize = 8192;

/// Maximum number of `VkDrawIndexedIndirectCommand` entries held in
/// the per-frame indirect buffer. Each entry is 20 bytes, so 8192
/// entries × 20 B = 160 KB per frame × 3 frames-in-flight = 480 KB
/// total. Sized generously — real scenes with instanced batching from
/// #272 rarely emit more than a few hundred entries. See #309.
pub const MAX_INDIRECT_DRAWS: usize = 8192;

/// Maximum number of `GpuTerrainTile` slots held in the per-frame
/// terrain-tile SSBO. 1024 × 32 B = 32 KB per frame — one slot per
/// terrain-mesh entity. A 3×3 loaded-cell grid emits 9 tiles; larger
/// exterior loads stay well under the cap. Capped at 65535 by the
/// 16-bit index packed into `GpuInstance.flags` (bits 16..31). See #470.
pub const MAX_TERRAIN_TILES: usize = 1024;

/// Maximum number of unique materials per frame in the
/// [`super::material::MaterialTable`] SSBO. 4096 × 260 B = 1.04 MB
/// per frame × 3 frames-in-flight = 3.3 MB total — trivial.
///
/// Real interior cells dedup to 50–200 unique materials; a 3×3
/// exterior grid lands around 300–600. The cap is sized 6–10× over
/// the largest observed scene to absorb future content. See R1.
pub const MAX_MATERIALS: usize = 4096;

/// Per-frame stride for the shared ray-budget buffer (#683 / MEM-2-8).
/// Each frame's slot must start on a `minStorageBufferOffsetAlignment`
/// boundary; 256 covers every common desktop / mobile GPU
/// (NVIDIA = 16, AMD = 4, Intel = 16, mobile up to 256). The actual
/// payload is 4 bytes — the rest is alignment padding. Total buffer
/// at MAX_FRAMES_IN_FLIGHT = 2 is 512 bytes.
pub const RAY_BUDGET_STRIDE: vk::DeviceSize = 256;

/// Per-instance flag bits on [`GpuInstance::flags`].
/// Kept in lockstep with the inline comments in `draw.rs` flag assembly
/// and with the fragment shader's `flags & N` checks.
pub const INSTANCE_FLAG_NON_UNIFORM_SCALE: u32 = 1 << 0;
pub const INSTANCE_FLAG_ALPHA_BLEND: u32 = 1 << 1;
pub const INSTANCE_FLAG_CAUSTIC_SOURCE: u32 = 1 << 2;
/// Terrain splat bit — tells the fragment shader to consume the
/// per-vertex splat weights (locations 6/7) and sample the 8 layer
/// textures indexed by `GpuTerrainTile` at the tile index packed into
/// the top 16 bits of `flags`. See #470.
pub const INSTANCE_FLAG_TERRAIN_SPLAT: u32 = 1 << 3;
/// Bit offset for the terrain tile index inside `GpuInstance.flags`.
/// `(flags >> INSTANCE_TERRAIN_TILE_SHIFT) & 0xFFFF` yields the tile slot.
pub const INSTANCE_TERRAIN_TILE_SHIFT: u32 = 16;
pub const INSTANCE_TERRAIN_TILE_MASK: u32 = 0xFFFF;
/// Bit offset for the [`RenderLayer`](byroredux_core::ecs::components::RenderLayer)
/// classification inside `GpuInstance.flags`. Layer is a 2-bit value
/// (Architecture / Clutter / Actor / Decal); bits 4..5 are unused by
/// any other flag, so packing here is collision-free.
/// `(flags >> INSTANCE_RENDER_LAYER_SHIFT) & 0x3u` yields the
/// [`RenderLayer`] discriminant. Consumed by the fragment shader's
/// debug-viz branch (`DBG_VIZ_RENDER_LAYER = 0x40`).
pub const INSTANCE_RENDER_LAYER_SHIFT: u32 = 4;
pub const INSTANCE_RENDER_LAYER_MASK: u32 = 0x3;

/// Engine-synthesized material kinds for [`GpuInstance::material_kind`].
///
/// The low range (0..=19) is reserved for Skyrim+
/// `BSLightingShaderProperty.shader_type` values the NIF importer
/// forwards verbatim — `SkinTint`, `HairTint`, `EyeEnvmap`, etc.
/// (see #344). The high range (100..) is reserved for kinds the
/// engine classifies itself from heuristics against the NIF material.
///
/// `Glass` is the first such kind (#Tier C Phase 2): alpha-blend
/// material, metalness < 0.3, not a decal. The fragment shader branches
/// on this value to dispatch the RT reflection + refraction path —
/// replaces the pre-Phase-2 per-pixel `texColor.a` heuristic that
/// flickered across textures. Callers (`render.rs`) must compute the
/// kind BEFORE populating `DrawCommand.material_kind`.
pub const MATERIAL_KIND_GLASS: u32 = 100;

/// `EffectShader` (`#706` / FX-1): Skyrim+ `BSEffectShaderProperty`
/// surface — fire flames, magic auras, glow rings, force fields, dust
/// planes, decals over emissive cones. The fragment shader branches on
/// this value to short-circuit lit shading: no scene point/spot lights,
/// no ambient, no GI bounce reads — output is `emissive_color *
/// emissive_mult * texColor.rgba`. Without this branch, fires get
/// modulated by every nearby lantern + ambient term + RT GI bounce,
/// producing rainbow-tinted flames where Bethesda authored a pure
/// orange/yellow additive surface.
///
/// Callers (`render.rs`) override the base shader_type-derived kind
/// to this value when `Material.effect_shader.is_some()`. Pre-existing
/// effect-shader data (falloff cone, greyscale palette, lighting_influence)
/// captured via #345 rides through on the same instance — the variant
/// branch in the fragment shader is the missing renderer-side dispatch
/// (SK-D3-02 follow-up).
pub const MATERIAL_KIND_EFFECT_SHADER: u32 = 101;

/// Per-terrain-tile data uploaded to the terrain-tile SSBO each cell load.
/// Indexed in the fragment shader via
/// `(instance.flags >> 16) & 0xFFFF` when the splat bit is set.
/// 8 × u32 = 32 bytes, std430-compatible.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GpuTerrainTile {
    /// Bindless texture indices for layers 0-7. Unused slots are 0
    /// (the fallback "error" texture); splat weights for unused layers
    /// are zero so the fragment's `mix` is a no-op.
    pub layer_texture_index: [u32; 8],
}

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
/// **Shader Struct Sync**: the matching `struct GpuInstance` declaration
/// in `triangle.vert`, `triangle.frag`, `ui.vert` and `caustic_splat.comp`
/// MUST be updated in lockstep. The `struct_gpuinstance_matches_all_shaders`
/// test below greps the four .comp/.vert/.frag files for the final trailing
/// u32 slot name — when you add a field here, update the expected suffix
/// in the assertion and rename the sentinel to match the new last field.
///
/// Layout: 112 bytes per instance, 16-byte aligned (7×16). R1 Phase 6
/// collapsed the per-material fields (texture indices, PBR scalars,
/// alpha state, Skyrim+ shader-variant payloads, BSEffect falloff,
/// BGSM UV transform, NiMaterialProperty diffuse/ambient, ~30 fields
/// total) onto a separate per-frame `MaterialTable` SSBO indexed by
/// `material_id`; the fragment shader reads them via
/// `materials[gpuInstance.material_id]`. What remains here is
/// strictly per-DRAW data: the model matrix, mesh refs, the
/// caustic-source `avg_albedo` (still consumed by `caustic_splat.comp`
/// off its own descriptor set), `flags` (mixed per-instance bits +
/// terrain tile slot), and the `material_id` indirection.
///
/// **Layout history** (every step preserves earlier offsets):
///   - 192 → 224 (#492, UV + material_alpha)
///   - 224 → 320 (#562, Skyrim+ BSLightingShaderProperty variants)
///   - 320 → 352 (#221, NiMaterialProperty diffuse + ambient)
///   - 352 → 384 (#620, BSEffectShaderProperty falloff cone)
///   - 384 → 400 (R1 Phase 3, `material_id` slot)
///   - 400 → 112 (R1 Phase 6, drop the migrated per-material fields)
///
/// The `size_of::<GpuInstance>() == 112` test below asserts the
/// invariant; shader-side `GpuInstance` must match.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuInstance {
    pub model: [[f32; 4]; 4], // 64 B, offset 0
    /// Diffuse / albedo bindless texture index. Held on the per-instance
    /// struct (not migrated to the material table) because the UI quad
    /// path appends an instance with a per-frame texture handle without
    /// going through the material table; keeping it here costs 4 B per
    /// instance and avoids a UI-specific material-intern dance.
    pub texture_index: u32, // 4 B, offset 64
    /// Bone palette base offset for skinned meshes. Per-DRAW; rigid
    /// instances set `0` (the identity slot at palette index 0).
    pub bone_offset: u32, // 4 B, offset 68
    /// Offset into the global vertex SSBO (in vertices, not bytes).
    pub vertex_offset: u32, // 4 B, offset 72
    /// Offset into the global index SSBO (in indices, not bytes).
    pub index_offset: u32, // 4 B, offset 76
    /// Vertex count for this mesh (for bounds checking).
    pub vertex_count: u32, // 4 B, offset 80
    /// Per-instance flags.
    ///   bit 0 — has non-uniform scale (needs inverse-transpose normal transform). See #273.
    ///   bit 1 — `alpha_blend` enabled (NiAlphaProperty blend bit). Used by the
    ///           fragment shader for its `isGlass`/`isWindow` classification.
    ///   bit 2 — caustic source: real refractive surface. The caustic compute
    ///           pass scatters caustic splats from every pixel whose instance
    ///           has this bit. Set by the CPU gate `draw::is_caustic_source`,
    ///           which requires `MATERIAL_KIND_GLASS` (engine-classified glass)
    ///           or Skyrim+ `MultiLayerParallax` (kind 11) with a non-zero
    ///           refraction scale. Pre-#922 fired for every alpha-blend +
    ///           low-metal draw, which over-included hair, foliage, particles,
    ///           decals and FX cards. See #321 (original) / #922 (gate tighten).
    ///   bits 3 — terrain-splat enable.
    ///   bits 16..32 — terrain tile slot (when bit 3 is set). See #470.
    pub flags: u32, // 4 B, offset 84
    /// R1 — index into the per-frame `MaterialTable` SSBO. Most
    /// per-material reads go through `materials[material_id].<field>`;
    /// Phase 6 dropped the redundant per-instance copies that used to
    /// inflate this struct from 112 B (now) to 400 B.
    pub material_id: u32, // 4 B, offset 88
    pub _pad_id0: f32,        // 4 B, offset 92
    /// Pre-computed average albedo for GI bounce approximation.
    /// Avoids 11 divergent memory ops per GI ray hit by replacing
    /// full UV lookup + texture sample with a single SSBO read.
    /// Kept on the per-instance struct (not migrated) because
    /// `caustic_splat.comp` reads it from its own descriptor set
    /// (set 0 binding 5) and migrating that path requires adding
    /// a separate `MaterialBuffer` binding to the caustic compute
    /// pipeline — deferred to a follow-up R1 cleanup.
    pub avg_albedo_r: f32, // 4 B, offset 96
    pub avg_albedo_g: f32,    // 4 B, offset 100
    pub avg_albedo_b: f32,    // 4 B, offset 104
    pub _pad_albedo: f32,     // 4 B, offset 108 → total 112
                              // Struct is 112 bytes (7×16), 16-byte aligned for std430.
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
            vertex_offset: 0,
            index_offset: 0,
            vertex_count: 0,
            flags: 0,
            // #807 — slot 0 is reserved by `MaterialTable::new()` /
            // `clear()` for the neutral-lit `GpuMaterial::default()`.
            // Default-initialised instances (UI quad, debug overlays,
            // future synthetic / editor-preview meshes) safely read
            // `materials[0]` and get a sane neutral record rather than
            // aliasing whichever user material happened to intern
            // first. User-interned distinct materials start at id 1.
            material_id: 0,
            _pad_id0: 0.0,
            avg_albedo_r: 0.5,
            avg_albedo_g: 0.5,
            avg_albedo_b: 0.5,
            _pad_albedo: 0.0,
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
    /// xyz = active TOD/weather zenith colour in linear RGB (mirrors
    /// `CompositeParams.sky_zenith.xyz`); w = reserved. Sourced from
    /// the same `SkyParams.zenith_color` that drives `compute_sky` so
    /// the triangle.frag window-portal escape transmits a sky tint
    /// matching whatever the composite pass paints behind the world.
    /// Pre-#925 the window-portal site hardcoded `vec3(0.6, 0.75, 1.0)`
    /// (clear-noon blue), so interior cells with windows looked midday
    /// regardless of TOD / weather. See audit REN-D15-NEW-03.
    pub sky_tint: [f32; 4],
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
            // Default to the pre-#925 hardcoded sky so unbootstrapped
            // frames (engine just opened, sky_params not yet computed)
            // render windows the same way they always have.
            sky_tint: [0.6, 0.75, 1.0, 0.0],
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
    /// HOST_VISIBLE | TRANSFER_SRC staging buffer per frame-in-flight that the
    /// CPU writes the bone palette into each frame ([`upload_bones`]). The
    /// matching [`bone_device_buffers`] slot is the actual storage-buffer
    /// binding consumed by skinning shaders; [`record_bone_copy`] schedules
    /// the staging→device transfer + visibility barrier on the command
    /// buffer that will read the palette.
    ///
    /// Pre-#921 the descriptor pointed at this host-visible buffer
    /// directly, so every skinned vertex performed a PCIe round-trip
    /// through host-mapped storage per shader invocation (~2 MB read per
    /// frame on a single skinned mesh and 4 KB × bones across every
    /// fragment that referenced a bone — verified against AUDIT_RENDERER
    /// Dim 12 / REN-D12-NEW-04).
    bone_staging_buffers: Vec<GpuBuffer>,
    /// DEVICE_LOCAL | STORAGE_BUFFER | TRANSFER_DST bone palette per
    /// frame-in-flight. Bound as descriptor binding 3 (current frame)
    /// and binding 12 (previous frame, the OTHER slot in the ring — see
    /// SH-3 / #641). Populated each frame by [`record_bone_copy`] via a
    /// `cmd_copy_buffer` from the matching [`bone_staging_buffers`] slot
    /// followed by a TRANSFER→COMPUTE_SHADER|VERTEX_SHADER memory barrier.
    /// Slot 0 is seeded with the identity matrix at construction
    /// (see [`seed_identity_bones`]) so rigid vertices that fall through
    /// to the palette and binding-12 reads on the very first frame see
    /// a valid transform.
    bone_device_buffers: Vec<GpuBuffer>,
    /// Bytes most recently written into [`bone_staging_buffers[frame]`]
    /// by [`upload_bones`]. [`record_bone_copy`] copies exactly this
    /// many bytes — avoids transferring the full ~2 MB
    /// `MAX_TOTAL_BONES × mat4` slab when only a handful of bones were
    /// actually written. Reset by [`upload_bones`]; pinned at the
    /// identity-slot size by the init seed so frames without skinned
    /// content still refresh the identity row.
    bone_upload_bytes: Vec<vk::DeviceSize>,
    /// One SSBO per frame-in-flight (per-instance data for instanced drawing).
    /// Each entry contains model matrix + texture index + bone offset.
    instance_buffers: Vec<GpuBuffer>,
    /// One SSBO per frame-in-flight ([`super::material::GpuMaterial`]
    /// table). Indexed by `GpuInstance.materialId`. Phase 4 (R1)
    /// migrates one field (`roughness`) onto this path; Phases 5–6
    /// migrate the rest and finally drop the redundant per-instance
    /// copies. Sized for [`MAX_MATERIALS`] entries.
    material_buffers: Vec<GpuBuffer>,
    /// Per-frame-in-flight content hash of the most recent
    /// successful `upload_materials` write. The next call computes
    /// the hash of the new slice and skips the
    /// `copy_nonoverlapping + flush_if_needed` pair when it matches
    /// — a static interior cell where `build_render_data` produces
    /// a byte-identical materials slice each frame is the steady-
    /// state case. `None` until the first upload so the cold path
    /// is unconditional. See #878 / DIM8-01.
    last_uploaded_material_hash: [Option<u64>; MAX_FRAMES_IN_FLIGHT],
    /// One `INDIRECT_BUFFER`-usage buffer per frame-in-flight for
    /// `vkCmdDrawIndexedIndirect`. Holds
    /// `VkDrawIndexedIndirectCommand` entries uploaded CPU-side each
    /// frame. The draw loop collapses consecutive batches sharing
    /// `(pipeline_key, is_decal)` into one indirect draw call reading
    /// a contiguous range of this buffer. See #309.
    indirect_buffers: Vec<GpuBuffer>,
    /// Single DEVICE_LOCAL SSBO holding `MAX_TERRAIN_TILES`
    /// `GpuTerrainTile` entries. Rewritten only at cell transitions
    /// via [`SceneBuffers::upload_terrain_tiles`] — a staging copy
    /// into GPU memory. Pre-#497 this was double-buffered HOST_VISIBLE,
    /// which wasted 32 KB of scarce BAR heap for read-only data. All
    /// frame-in-flight descriptor sets point at the same buffer since
    /// there are no per-frame contents. Fragment shader reads
    /// `terrainTiles[tile_idx]` when `INSTANCE_FLAG_TERRAIN_SPLAT` is
    /// set on the instance. See #470 / #497.
    terrain_tile_buffer: GpuBuffer,
    /// Single HOST_VISIBLE buffer holding `MAX_FRAMES_IN_FLIGHT` ray-budget
    /// counter slots, one per frame-in-flight, each [`RAY_BUDGET_STRIDE`]
    /// bytes apart so they satisfy `minStorageBufferOffsetAlignment` on
    /// every common device. Each frame's descriptor set writes binding 11
    /// at `offset = frame * RAY_BUDGET_STRIDE, range = 4`. The CPU zeroes
    /// the active frame's slot before each render pass; the fragment
    /// shader atomically increments it per IOR ray pair fired and skips
    /// Phase-3 glass once the budget is exhausted.
    ///
    /// Pre-fix this was `Vec<GpuBuffer>` with one allocation per frame
    /// for a single u32 — `gpu-allocator` rounded each up to the
    /// alignment-padded sub-allocation size and could reserve a fresh
    /// 64 KB host-visible block to satisfy the layout. The single shared
    /// buffer collapses both frames into one ~512 B sub-allocation.
    /// See #683 / MEM-2-8.
    ray_budget_buffer: GpuBuffer,
    /// Size of the terrain tile buffer in bytes — stashed so upload
    /// paths don't have to recompute it from `MAX_TERRAIN_TILES`.
    terrain_tile_buf_size: vk::DeviceSize,
    /// Descriptor pool for scene descriptor sets.
    descriptor_pool: vk::DescriptorPool,
    /// Layout for set 1: binding 0 = SSBO (lights), binding 1 = UBO (camera),
    /// binding 2 = TLAS (RT only), binding 3 = SSBO (bone palette),
    /// binding 4 = SSBO (instance data), …, binding 12 = SSBO (previous-frame
    /// bone palette — SH-3 / #641, vertex-stage only, points at the
    /// frame-in-flight ring's other slot so motion vectors on skinned
    /// vertices reflect actual joint motion rather than zero).
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    /// One descriptor set per frame-in-flight.
    pub descriptor_sets: Vec<vk::DescriptorSet>,
    /// Tracks whether the TLAS binding has been written for each frame.
    pub tlas_written: Vec<bool>,
}

/// Build the scene descriptor-set-layout bindings (set=1) consumed by
/// the main raster pipeline. Pure data — no Vulkan device required —
/// so this is the seam that `cargo test` reflection tests can call
/// against the include_bytes!'d `triangle.vert.spv` / `triangle.frag.spv`
/// without spinning up a real device. Production `SceneBuffers::new`
/// routes through the same function so test and runtime can't drift.
///
/// `rt_enabled = false` drops binding 2 (TLAS); the shader still
/// declares it because `rayQuery` calls are guarded by a uniform flag
/// at runtime, so the validator must list `[2]` in
/// `optional_shader_bindings` for the no-RT case. See #427 / #950.
pub(crate) fn build_scene_descriptor_bindings(
    rt_enabled: bool,
) -> Vec<vk::DescriptorSetLayoutBinding<'static>> {
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
    // Binding 10: terrain tile SSBO (fragment shader — LAND splat layer
    // texture indices, one entry per terrain entity). #470.
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(10)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    // Binding 11: RT mipmap ray budget counter (fragment shader — u32 atomic).
    // The CPU zeroes this before each render pass; Phase-3 glass fragments
    // atomicAdd to claim a budget slot and skip IOR refraction once exhausted.
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(11)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    // Binding 12: previous-frame bone palette SSBO (vertex shader —
    // skinned-vertex motion vectors, SH-3 / #641). Bound to the OTHER
    // slot in the `bone_buffers` frame-in-flight ring, so reading it
    // yields the palette uploaded last frame. Pre-#641 the vertex
    // shader composed `fragPrevClipPos = prevViewProj * worldPos`
    // using the CURRENT-frame skinned worldPos — every actor pixel
    // had a motion vector encoding only camera + rigid motion, and
    // SVGF / TAA reprojected the wrong source pixel on intra-mesh
    // disocclusions (forearm crossing torso). Same indices/weights as
    // binding 3, so the per-mesh bone offset stamped on the
    // `GpuInstance` still resolves correctly as long as the offset
    // assignment is stable across the two frames.
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(12)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX),
    );
    // Binding 13: material table SSBO (fragment — R1 Phase 4).
    // The fragment stage reads `materials[instance.materialId]`
    // for migrated per-material fields. Vertex stage isn't a
    // consumer today; widen the stage mask if a future migration
    // (e.g. M29.5 GPU skinning) needs material data in the vertex
    // pipeline.
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(13)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    bindings
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
        // Material SSBO: deduplicated `GpuMaterial` table (R1 Phase 4).
        let material_buf_size =
            (std::mem::size_of::<super::material::GpuMaterial>() * MAX_MATERIALS) as vk::DeviceSize;
        // Indirect buffer: one VkDrawIndexedIndirectCommand (20 B) per batch. #309.
        let indirect_buf_size = (std::mem::size_of::<vk::DrawIndexedIndirectCommand>()
            * MAX_INDIRECT_DRAWS) as vk::DeviceSize;
        // Terrain tile SSBO: 32 B per slot × MAX_TERRAIN_TILES. #470.
        let terrain_tile_buf_size =
            (std::mem::size_of::<GpuTerrainTile>() * MAX_TERRAIN_TILES) as vk::DeviceSize;

        // Create per-frame buffers.
        let mut light_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut camera_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        // #921 / REN-D12-NEW-04 — split bone palette into HOST_VISIBLE
        // staging + DEVICE_LOCAL storage. The descriptor binds the
        // device buffers; `upload_bones` writes the staging buffer and
        // `record_bone_copy` schedules the per-frame transfer.
        let mut bone_staging_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut bone_device_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut instance_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut indirect_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        // #683 / MEM-2-8 — single shared buffer covering all frame slots
        // (see `RAY_BUDGET_STRIDE` doc). Created outside the per-frame
        // loop below; each frame's descriptor write picks its slot via
        // an offset into this one allocation.
        let ray_budget_buffer = GpuBuffer::create_host_visible(
            device,
            allocator,
            RAY_BUDGET_STRIDE * MAX_FRAMES_IN_FLIGHT as vk::DeviceSize,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?;
        let mut material_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
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
            bone_staging_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                bone_buf_size,
                vk::BufferUsageFlags::TRANSFER_SRC,
            )?);
            bone_device_buffers.push(GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                bone_buf_size,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
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
            material_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                material_buf_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            // Ray budget counter is now a SINGLE shared buffer (declared
            // outside this loop) with per-frame slots at
            // `frame * RAY_BUDGET_STRIDE`. See #683 / MEM-2-8.
        }
        // Terrain tile SSBO: single DEVICE_LOCAL buffer, uploaded via
        // staging at cell load. TRANSFER_DST needed for the staging
        // copy. Shared across all frame-in-flight descriptor sets
        // since the contents are static until the next cell
        // transition. See #497.
        let terrain_tile_buffer = GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            terrain_tile_buf_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
        )?;

        // Descriptor set layout: set 1.
        let bindings = build_scene_descriptor_bindings(rt_enabled);
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
        // Validate against triangle.vert/frag SPIR-V before creating the layout (#427).
        // When rt_enabled=false the TLAS binding (2) is declared in the shader
        // but intentionally omitted from the layout — the fragment gates every
        // rayQuery behind a uniform flag.
        // Binding 2 (TLAS) is declared in the shader but omitted when rt_enabled=false.
        let optional_bindings: &[u32] = if rt_enabled { &[] } else { &[2] };
        super::reflect::validate_set_layout(
            1,
            &bindings,
            &[
                super::reflect::ReflectedShader {
                    name: "triangle.vert",
                    spirv: super::pipeline::TRIANGLE_VERT_SPV,
                },
                super::reflect::ReflectedShader {
                    name: "triangle.frag",
                    spirv: super::pipeline::TRIANGLE_FRAG_SPV,
                },
            ],
            "scene (set=1)",
            optional_bindings,
        )
        .expect("scene descriptor layout drifted against triangle.vert/frag (see #427)");
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
                // 11 SSBOs per frame: lights(0), bones(3), instances(4), cluster
                // grid(5), light indices(6), vertices(8), indices(9), terrain
                // tiles(10), ray budget counter(11), bones_prev(12 — SH-3 / #641),
                // materials(13 — R1 Phase 4).
                descriptor_count: (MAX_FRAMES_IN_FLIGHT * 11) as u32,
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
            // #921 — descriptors point at the DEVICE buffers; the
            // staging copies happen on the recording command buffer via
            // `record_bone_copy` before the consuming shader stage.
            let bone_buf_info = [vk::DescriptorBufferInfo {
                buffer: bone_device_buffers[i].buffer,
                offset: 0,
                range: bone_buf_size,
            }];
            // Previous-frame bone palette: the OTHER slot in the ring.
            // Frame N writes its palette into `bone_device_buffers[N % MAX]`
            // and binding 12 references `bone_device_buffers[(N + MAX - 1) % MAX]`
            // (last frame's data). SH-3 / #641. With MAX_FRAMES_IN_FLIGHT=2
            // the prev index is `(i + 1) % 2`. The mapping is static —
            // written once here.
            let bone_prev_idx = (i + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT;
            let bone_prev_buf_info = [vk::DescriptorBufferInfo {
                buffer: bone_device_buffers[bone_prev_idx].buffer,
                offset: 0,
                range: bone_buf_size,
            }];
            let instance_buf_info = [vk::DescriptorBufferInfo {
                buffer: instance_buffers[i].buffer,
                offset: 0,
                range: instance_buf_size,
            }];
            let terrain_tile_buf_info = [vk::DescriptorBufferInfo {
                buffer: terrain_tile_buffer.buffer,
                offset: 0,
                range: terrain_tile_buf_size,
            }];
            // #683 / MEM-2-8 — slot into the shared buffer at this
            // frame's stride offset. `range` is the actual u32 payload;
            // the alignment padding is invisible to the shader.
            let ray_budget_buf_info = [vk::DescriptorBufferInfo {
                buffer: ray_budget_buffer.buffer,
                offset: (i as vk::DeviceSize) * RAY_BUDGET_STRIDE,
                range: std::mem::size_of::<u32>() as vk::DeviceSize,
            }];
            let material_buf_info = [vk::DescriptorBufferInfo {
                buffer: material_buffers[i].buffer,
                offset: 0,
                range: material_buf_size,
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
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(10)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&terrain_tile_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(11)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&ray_budget_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(12)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&bone_prev_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(13)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&material_buf_info),
            ];
            unsafe {
                device.update_descriptor_sets(&writes, &[]);
            }
        }

        // Seed slot 0 of each STAGING buffer with the identity matrix so
        // rigid vertices that fall through to the palette (shouldn't
        // happen, but serves as a defensive fallback) produce correct
        // positions rather than collapsing to origin. The matching
        // device buffer is populated by `seed_identity_bones` once the
        // caller can supply a queue + command pool for the initial copy.
        let identity: [[f32; 4]; 4] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        for buf in &mut bone_staging_buffers {
            buf.write_mapped(device, std::slice::from_ref(&identity))?;
        }
        // Track the seeded identity matrix so `record_bone_copy` on the
        // very first frame still propagates slot 0 to device memory even
        // if no skinned content was uploaded that frame.
        let identity_bytes = std::mem::size_of::<[[f32; 4]; 4]>() as vk::DeviceSize;
        let bone_upload_bytes = vec![identity_bytes; MAX_FRAMES_IN_FLIGHT];

        log::info!(
            "Scene buffers created: {} frames, {} max lights ({} bytes/frame)",
            MAX_FRAMES_IN_FLIGHT,
            MAX_LIGHTS,
            light_buf_size,
        );

        Ok(Self {
            light_buffers,
            camera_buffers,
            bone_staging_buffers,
            bone_device_buffers,
            bone_upload_bytes,
            instance_buffers,
            material_buffers,
            indirect_buffers,
            terrain_tile_buffer,
            terrain_tile_buf_size,
            ray_budget_buffer,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
            tlas_written: vec![false; MAX_FRAMES_IN_FLIGHT],
            last_uploaded_material_hash: [None; MAX_FRAMES_IN_FLIGHT],
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

    /// Upload the bone palette for the current frame-in-flight into the
    /// HOST_VISIBLE staging buffer. The matching DEVICE_LOCAL slot is
    /// populated by [`record_bone_copy`] once a recording command buffer
    /// is available — until then the shader still sees last frame's
    /// device contents.
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

        let byte_size = (std::mem::size_of::<[[f32; 4]; 4]>() * count) as vk::DeviceSize;
        let buf = &mut self.bone_staging_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        // SAFETY: [[f32; 4]; 4] is #[repr(C)]-compatible with std430 mat4.
        // bone_staging_buffers are sized for MAX_TOTAL_BONES slots; count is clamped.
        unsafe {
            std::ptr::copy_nonoverlapping(
                palette.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size as usize,
            );
        }
        buf.flush_if_needed(device)?;
        // Record exactly how many bytes need to ride the staging→device
        // copy for this frame so `record_bone_copy` doesn't transfer the
        // full ~2 MB slab when only a few bones were written.
        self.bone_upload_bytes[frame_index] = byte_size;
        Ok(())
    }

    /// Record the staging→device bone-palette copy and the visibility
    /// barrier on `cmd`, scoped to the bytes most recently written by
    /// [`upload_bones`] for this frame.
    ///
    /// The barrier widens the dst stage mask to cover every consumer of
    /// the device buffer:
    ///   * `COMPUTE_SHADER` — M29 GPU pre-skin pass (`SkinComputePipeline`)
    ///     reads the palette before issuing per-vertex skinning into the
    ///     entity's output buffer.
    ///   * `VERTEX_SHADER` — fallback CPU-feeds + raster vertex skinning
    ///     read binding 3 (current frame) and binding 12 (previous frame).
    ///
    /// Callers MUST invoke this on every command buffer that consumes the
    /// palette — both the main per-frame command buffer (steady-state
    /// dispatch + raster vertex stage) and the one-time "prime" command
    /// buffers used for first-sight skinned BLAS builds. The copy is
    /// idempotent: a redundant call on the main cmd buffer after the prime
    /// finished copies the same bytes again, which is harmless.
    pub fn record_bone_copy(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_index: usize,
    ) {
        let byte_size = self.bone_upload_bytes[frame_index];
        if byte_size == 0 {
            return;
        }
        let copy = vk::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: byte_size,
        };
        unsafe {
            device.cmd_copy_buffer(
                cmd,
                self.bone_staging_buffers[frame_index].buffer,
                self.bone_device_buffers[frame_index].buffer,
                &[copy],
            );
            // Make the copied range visible to every shader stage that
            // reads the palette. Buffer barrier (not global) so we don't
            // perturb unrelated cache state on the same submission.
            let barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(self.bone_device_buffers[frame_index].buffer)
                .offset(0)
                .size(byte_size);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::VERTEX_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[barrier],
                &[],
            );
        }
    }

    /// Copy the identity-matrix seed in slot 0 of every staging buffer
    /// to its matching DEVICE_LOCAL slot via a one-time command buffer,
    /// so the first frame's binding-12 read (previous-frame palette) and
    /// the rigid-vertex fallback path see a valid transform in slot 0
    /// from frame 0. Mirrors the pre-#921 invariant where the
    /// host-visible bone buffers were directly mapped and slot 0 was
    /// seeded with the identity by `write_mapped` in `new()`.
    pub fn seed_identity_bones(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
    ) -> Result<()> {
        let identity_bytes = std::mem::size_of::<[[f32; 4]; 4]>() as vk::DeviceSize;
        super::texture::with_one_time_commands(device, queue, command_pool, |cmd| {
            for i in 0..MAX_FRAMES_IN_FLIGHT {
                let copy = vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: identity_bytes,
                };
                unsafe {
                    device.cmd_copy_buffer(
                        cmd,
                        self.bone_staging_buffers[i].buffer,
                        self.bone_device_buffers[i].buffer,
                        &[copy],
                    );
                }
            }
            Ok(())
        })
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

    /// Upload the deduplicated material table for the current
    /// frame-in-flight (R1 Phase 4). Called once per frame after
    /// `build_render_data` has populated the table; the fragment
    /// shader reads `materials[instance.materialId]` for migrated
    /// fields. Empty table is a no-op (no draws → no material reads).
    pub fn upload_materials(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        materials: &[super::material::GpuMaterial],
    ) -> Result<()> {
        let count = materials.len().min(MAX_MATERIALS);
        if materials.len() > MAX_MATERIALS {
            log::warn!(
                "Material table overflow: {} materials submitted, capped at {} \
                 — instances pointing past the cap silently default to material 0",
                materials.len(),
                MAX_MATERIALS,
            );
        }
        if count == 0 {
            return Ok(());
        }

        // #878 / DIM8-01 — dirty-gate via content hash. Static
        // interior cells produce a byte-identical materials slice
        // every frame; skipping the copy + flush in steady state
        // saves ~3 MB/s sustained PCIe traffic at 60 fps with 200
        // unique materials. The hash is computed over the clamped
        // prefix actually written to the buffer (`materials[..count]`)
        // so an overflow that drops trailing materials still
        // re-uploads when the kept prefix changes.
        let hash = hash_material_slice(&materials[..count]);
        if self.last_uploaded_material_hash[frame_index] == Some(hash) {
            return Ok(());
        }

        let buf = &mut self.material_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        let byte_size = std::mem::size_of::<super::material::GpuMaterial>() * count;
        // SAFETY: GpuMaterial is #[repr(C)] with f32/u32 fields and
        // explicit padding (no implicit Drop, no uninitialised bytes).
        // material_buffers are sized for MAX_MATERIALS; count is clamped.
        unsafe {
            std::ptr::copy_nonoverlapping(
                materials.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size,
            );
        }
        buf.flush_if_needed(device)?;
        // Stamp the hash AFTER a successful flush — a flush failure
        // leaves the buffer in an indeterminate state, so we want
        // the next call to re-upload rather than skip.
        self.last_uploaded_material_hash[frame_index] = Some(hash);
        Ok(())
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

    /// Upload terrain tile data into the single DEVICE_LOCAL SSBO via
    /// a staging buffer + one-time `vkCmdCopyBuffer`. Called from the
    /// cell loader path after `spawn_terrain_mesh` packs per-tile layer
    /// texture indices. The data is static until the next cell
    /// transition, so exactly one upload per dirty transition is
    /// enough — no per-frame double-buffering. See #470 / #497.
    pub fn upload_terrain_tiles(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        tiles: &[GpuTerrainTile],
    ) -> Result<()> {
        let count = tiles.len().min(MAX_TERRAIN_TILES);
        if tiles.len() > MAX_TERRAIN_TILES {
            log::warn!(
                "Terrain tile SSBO overflow: {} tiles submitted, capped at {} — excess slots silently dropped. #470",
                tiles.len(),
                MAX_TERRAIN_TILES,
            );
        }
        if count == 0 {
            return Ok(());
        }

        let byte_size = (std::mem::size_of::<GpuTerrainTile>() * count) as vk::DeviceSize;

        // Create a transient staging buffer. Terrain tile uploads run
        // at cell-transition frequency (a few times a minute at most),
        // so skip the StagingPool reuse overhead — a one-shot 32 KB
        // CpuToGpu allocation is cheap and the buffer vanishes cleanly
        // via the guard below on any exit path.
        let staging_info = vk::BufferCreateInfo::default()
            .size(byte_size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let staging_buffer = unsafe {
            device
                .create_buffer(&staging_info, None)
                .context("Failed to create terrain tile staging buffer")?
        };
        let reqs = unsafe { device.get_buffer_memory_requirements(staging_buffer) };
        let mut staging_alloc = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                name: "terrain_tile_staging",
                requirements: reqs,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate terrain tile staging memory")?;
        super::buffer::debug_assert_cpu_to_gpu_mapped(&staging_alloc, "terrain_tile_staging");
        unsafe {
            device
                .bind_buffer_memory(
                    staging_buffer,
                    staging_alloc.memory(),
                    staging_alloc.offset(),
                )
                .context("Failed to bind terrain tile staging buffer")?;
        }

        // SAFETY: GpuTerrainTile is #[repr(C)] with u32-only fields
        // matching std430. Staging was sized to `byte_size` above.
        let mapped = staging_alloc
            .mapped_slice_mut()
            .context("Terrain tile staging not mapped")?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                tiles.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size as usize,
            );
        }

        let copy = vk::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: byte_size,
        };
        let dst = self.terrain_tile_buffer.buffer;
        let result = super::texture::with_one_time_commands(device, queue, command_pool, |cmd| {
            unsafe {
                device.cmd_copy_buffer(cmd, staging_buffer, dst, &[copy]);
            }
            Ok(())
        });

        // Tear down staging regardless of copy outcome.
        unsafe {
            device.destroy_buffer(staging_buffer, None);
        }
        allocator
            .lock()
            .expect("allocator lock poisoned")
            .free(staging_alloc)
            .ok();

        // Suppress "field never read" on the cached size — kept for
        // future layout changes / debugging introspection.
        let _ = self.terrain_tile_buf_size;

        result
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

    /// Get the per-frame DEVICE_LOCAL bone palette buffers (M29 — skin
    /// compute reads them as the bone-matrix source per-dispatch). After
    /// #921 these are the device-side targets of the staging copy
    /// scheduled by [`record_bone_copy`]; the host-visible staging
    /// buffers are private.
    pub fn bone_buffers(&self) -> &[GpuBuffer] {
        &self.bone_device_buffers
    }

    /// Bone palette buffer size in bytes (`MAX_TOTAL_BONES × mat4`).
    pub fn bone_buffer_size(&self) -> vk::DeviceSize {
        (std::mem::size_of::<[[f32; 4]; 4]>() * MAX_TOTAL_BONES) as vk::DeviceSize
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

    /// Zero the ray budget counter for the given frame before the render pass.
    ///
    /// Called from `draw_frame` after uploading instances and before
    /// `cmd_begin_render_pass`. The fragment shader atomically increments this
    /// counter for each Phase-3 IOR glass ray pair it fires; once the count
    /// exceeds `GLASS_RAY_BUDGET` (declared in `triangle.frag`) all further
    /// glass fragments degrade to the tier-1 cheaper path for that frame.
    pub fn reset_ray_budget(&mut self, device: &ash::Device, frame: usize) -> Result<()> {
        // #683 / MEM-2-8 — write the u32 zero at this frame's stride
        // offset within the shared buffer, then flush only that slot's
        // range on non-coherent memory. Mapped slice access bypasses
        // the from-byte-0-only `write_mapped` helper.
        let offset = (frame as vk::DeviceSize) * RAY_BUDGET_STRIDE;
        let off_usize = offset as usize;
        let mapped = self.ray_budget_buffer.mapped_slice_mut()?;
        mapped[off_usize..off_usize + 4].copy_from_slice(&0u32.to_le_bytes());
        self.ray_budget_buffer.flush_range(
            device,
            offset,
            std::mem::size_of::<u32>() as vk::DeviceSize,
        )
    }

    /// Destroy all resources.
    ///
    /// Pre-#732 LIFE-N1 the per-Vec `buf.destroy()` loops below freed
    /// every GPU allocation but never cleared the `Vec`s, so each
    /// `GpuBuffer` struct stayed alive (with `allocation: None` after
    /// `destroy`) and kept its `Arc<Mutex<Allocator>>` clone live until
    /// `SceneBuffers` itself naturally dropped — *after*
    /// `VulkanContext::Drop` had already failed `Arc::try_unwrap` and
    /// taken the warn-and-leak fall-through path. The post-fix
    /// `.clear()` calls drop each `GpuBuffer` immediately so the
    /// allocator unwrap sees a smaller strong count by the time it
    /// runs.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for buf in &mut self.light_buffers {
            buf.destroy(device, allocator);
        }
        self.light_buffers.clear();
        for buf in &mut self.camera_buffers {
            buf.destroy(device, allocator);
        }
        self.camera_buffers.clear();
        for buf in &mut self.bone_staging_buffers {
            buf.destroy(device, allocator);
        }
        self.bone_staging_buffers.clear();
        for buf in &mut self.bone_device_buffers {
            buf.destroy(device, allocator);
        }
        self.bone_device_buffers.clear();
        for buf in &mut self.instance_buffers {
            buf.destroy(device, allocator);
        }
        self.instance_buffers.clear();
        for buf in &mut self.material_buffers {
            buf.destroy(device, allocator);
        }
        self.material_buffers.clear();
        for buf in &mut self.indirect_buffers {
            buf.destroy(device, allocator);
        }
        self.indirect_buffers.clear();
        // #683 / MEM-2-8 — single shared buffer, single destroy.
        self.ray_budget_buffer.destroy(device, allocator);
        self.terrain_tile_buffer.destroy(device, allocator);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}

/// Content hash of a `GpuMaterial` slice for the dirty-gate in
/// [`SceneBuffers::upload_materials`] (#878 / DIM8-01). Uses
/// `std::collections::hash_map::DefaultHasher` (SipHash-1-3) — its
/// state is documented stable across `new()` calls within one
/// process, so two identical slices in the same run produce the
/// same `u64` and the upload skip is byte-content-addressable.
///
/// SipHash on a 200-material slice (~52 KB) takes ~30 µs, well under
/// the per-frame budget at 60 fps. xxh3 would be ~10× faster but
/// would require a new dependency; the hash itself is well below
/// the signal floor either way.
///
/// Routed through `GpuMaterial::as_bytes`-equivalent slice cast so
/// the same byte view used by `GpuMaterial`'s `Hash`/`Eq` impls
/// (`vulkan/material.rs:280-309`) drives the slice hash too —
/// padding handling stays consistent.
pub(super) fn hash_material_slice(materials: &[super::material::GpuMaterial]) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let byte_size = std::mem::size_of::<super::material::GpuMaterial>() * materials.len();
    // SAFETY: `GpuMaterial` is `#[repr(C)]` with f32/u32 fields and
    // explicit padding fields the producer always initialises (see
    // `GpuMaterial::as_bytes` doc at vulkan/material.rs:281-294).
    // The slice view is contiguous because `[T]` storage is too;
    // `byte_size` matches the slice's footprint exactly.
    let bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(materials.as_ptr() as *const u8, byte_size) };
    hasher.write(bytes);
    hasher.finish()
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
    fn gpu_instance_is_112_bytes_std430_compatible() {
        // R1 Phase 6 collapsed the per-material fields onto the
        // separate `MaterialTable` SSBO. What's left here is
        // strictly per-DRAW: model (64 B) + 4 mesh refs +
        // bone_offset + flags + material_id + avg_albedo (kept
        // for caustic compute reads off its own descriptor set)
        // packed into 7 vec4 slots = 112 B.
        assert_eq!(
            size_of::<GpuInstance>(),
            112,
            "GpuInstance must stay 112 B to match std430 shader layout"
        );
    }

    #[test]
    fn gpu_instance_field_offsets_match_shader_contract() {
        assert_eq!(offset_of!(GpuInstance, model), 0);
        assert_eq!(offset_of!(GpuInstance, texture_index), 64);
        assert_eq!(offset_of!(GpuInstance, bone_offset), 68);
        assert_eq!(offset_of!(GpuInstance, vertex_offset), 72);
        assert_eq!(offset_of!(GpuInstance, index_offset), 76);
        assert_eq!(offset_of!(GpuInstance, vertex_count), 80);
        assert_eq!(offset_of!(GpuInstance, flags), 84);
        assert_eq!(offset_of!(GpuInstance, material_id), 88);
        assert_eq!(offset_of!(GpuInstance, _pad_id0), 92);
        assert_eq!(offset_of!(GpuInstance, avg_albedo_r), 96);
        assert_eq!(offset_of!(GpuInstance, avg_albedo_g), 100);
        assert_eq!(offset_of!(GpuInstance, avg_albedo_b), 104);
        assert_eq!(offset_of!(GpuInstance, _pad_albedo), 108);
    }

    /// R1 Phase 6 sentinel — list of fields that USED to live on
    /// `GpuInstance` and were collapsed onto the `MaterialTable` SSBO.
    /// If this test grows back any of those names, R1 is being undone.
    #[test]
    fn gpu_instance_does_not_re_expand_with_per_material_fields() {
        // Build trivially via Default and rely on the size assertion
        // above (112 B) to fail loudly if a field is reintroduced.
        // The list below is documentary only; the size guard is what
        // catches actual regressions.
        let _ = GpuInstance::default();
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
            ("triangle.vert", include_str!("../../shaders/triangle.vert")),
            ("triangle.frag", include_str!("../../shaders/triangle.frag")),
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
            // R1 Phase 6 — `material_kind` moved off `GpuInstance`
            // into the `MaterialBuffer` SSBO. The assertion that
            // every shader's per-instance struct names a final
            // `materialKind` slot (#417) no longer applies.
            // `triangle.frag` is the only shader that declares a
            // `GpuMaterial` block at all (see binding 13 below).
            assert!(
                !src.contains("uint _pad1"),
                "{name}: GpuInstance slot is still named `_pad1` — \
                 the shader has the pre-#417 layout (Shader Struct \
                 Sync invariant #318 / #417)."
            );
            // R1 Phase 6 — these fields were migrated to the
            // `MaterialBuffer` SSBO and dropped from `GpuInstance`.
            // `material_kind` is now read as `materials[id].materialKind`
            // and `materialId` is the only material-table-related
            // slot left on the per-instance struct.
            for needle in [
                // R1 Phase 3 — material table indirection. Every shader
                // copy declares the slot so the std430 stride stays
                // byte-identical across the four.
                "materialId",
            ] {
                assert!(
                    src.contains(needle),
                    "{name}: GpuInstance must declare `{needle}` (R1 Phase 3+). \
                     Every copy updates in lockstep — see the \
                     feedback_shader_struct_sync memory note."
                );
            }
            // R1 Phase 6 — these names lived on `GpuInstance` before
            // the material-table collapse. A reappearance means the
            // refactor is being undone.
            for stale in [
                "parallaxMapIndex",
                "parallaxHeightScale",
                "parallaxMaxPasses",
                "envMapIndex",
                "envMaskIndex",
                "uvOffsetU",
                "uvScaleU",
                "materialAlpha",
                "skinTintR",
                "hairTintR",
                "multiLayerEnvmapStrength",
                "eyeLeftCenterX",
                "eyeCubemapScale",
                "eyeRightCenterX",
                "multiLayerInnerThickness",
                "multiLayerRefractionScale",
                "multiLayerInnerScaleU",
                "sparkleR",
                "sparkleIntensity",
                "diffuseR",
                "ambientR",
                "falloffStartAngle",
                "falloffStopAngle",
                "falloffStartOpacity",
                "falloffStopOpacity",
                "softFalloffDepth",
            ] {
                // The names CAN appear on the `GpuMaterial` mirror
                // declarations — what's forbidden is reappearance on
                // `struct GpuInstance` after Phase 6 dropped them.
                let gi_start = src.find("struct GpuInstance");
                let gi_end = gi_start.and_then(|s| src[s..].find('}').map(|e| s + e));
                if let (Some(s), Some(e)) = (gi_start, gi_end) {
                    let gi_block = &src[s..e];
                    assert!(
                        !gi_block.contains(stale),
                        "{name}: per-material field `{stale}` reappeared on \
                         `struct GpuInstance` — R1 Phase 6 dropped it. \
                         Read it from `materials[gpuInstance.materialId]` \
                         instead."
                    );
                }
            }
        }
    }

    /// Regression: #776 / #785 — `ui.vert` must read its texture index
    /// from `inst.textureIndex` (per-instance), NOT from
    /// `materials[inst.materialId].textureIndex`. The UI quad is
    /// appended at `draw.rs` with `..GpuInstance::default()`, which
    /// leaves `materialId = 0`. Post-#807 `materials[0]` is the
    /// reserved neutral default — a UI shader that read it would
    /// pull a neutral GpuMaterial (not an arbitrary scene material
    /// as in the pre-#807 days), but the texture index would still
    /// be wrong (the UI texture lives in `inst.textureIndex`, not
    /// in any GpuMaterial slot). The guard stays as defense-in-depth
    /// against future drift. See `scene_buffer.rs:172-176` for the
    /// contract and `feedback_shader_struct_sync.md` for the
    /// broader invariant.
    ///
    /// #785 was a stale-hunk regression of #776 introduced by an
    /// unrelated commit. Static source check so any future drift
    /// fails `cargo test` without needing glslangValidator.
    #[test]
    fn ui_vert_reads_texture_index_from_instance_not_material_table() {
        let src = include_str!("../../shaders/ui.vert");
        assert!(
            src.contains("fragTexIndex = inst.textureIndex"),
            "ui.vert: `fragTexIndex` must be assigned from \
             `inst.textureIndex` (the per-instance UI texture handle). \
             Reading `materials[inst.materialId].textureIndex` samples \
             the first scene material instead — see #776 / #785."
        );
        // Match syntactic declarations only — the surrounding comments
        // legitimately reference `MaterialBuffer` / `materials[…]` to
        // explain why the read is forbidden, and the test must not
        // catch its own documentation.
        assert!(
            !src.contains("buffer MaterialBuffer"),
            "ui.vert: must NOT declare a `MaterialBuffer` SSBO. The UI \
             vertex stage only consumes per-instance `textureIndex`; \
             pulling in the material table re-enables the #776 / #785 \
             failure mode."
        );
        assert!(
            !src.contains("struct GpuMaterial"),
            "ui.vert: must NOT declare `struct GpuMaterial`. Only \
             `triangle.frag` mirrors the material struct (binding 13). \
             See #776 / #785."
        );
        assert!(
            !src.contains("materials[inst"),
            "ui.vert: must NOT index into `materials[inst.…]`. The UI \
             quad's `materialId` is 0 (default-initialized), so any \
             read aliases the first scene material — see #776 / #785."
        );
    }

    /// SH-3 / #641 regression. The vertex shader must compose
    /// `fragPrevClipPos` through the previous-frame bone palette so
    /// motion vectors on skinned vertices encode actual joint motion.
    /// Pre-#641 it composed through the current-frame palette, leaving
    /// every actor body / hand / face pixel with a wrong motion vector
    /// that SVGF + TAA reprojected as a ghost trail.
    ///
    /// Static source check (no `glslangValidator` dependency): the
    /// shader must declare a `bones_prev` SSBO at `set 1, binding 12`
    /// and feed `prevWorldPos` (composed through `bones_prev`) into
    /// `fragPrevClipPos = prevViewProj * …`.
    #[test]
    fn triangle_vert_uses_bones_prev_for_motion_vectors() {
        let src = include_str!("../../shaders/triangle.vert");
        assert!(
            src.contains("binding = 12) readonly buffer BonesPrevBuffer"),
            "triangle.vert must declare a previous-frame bone palette \
             SSBO at `set 1, binding = 12` (SH-3 / #641). Without it \
             skinned vertices produce wrong motion vectors and SVGF / \
             TAA ghost actor limbs in motion."
        );
        assert!(
            src.contains("mat4 bones_prev[]"),
            "triangle.vert: `BonesPrevBuffer` must expose a `mat4 \
             bones_prev[]` array — same layout as `bones[]` so the \
             current and previous palettes can share `inBoneIndices`."
        );
        assert!(
            src.contains("fragPrevClipPos = prevViewProj * prevWorldPos"),
            "triangle.vert: `fragPrevClipPos` must project the \
             previous-frame skinned `prevWorldPos`, not the current \
             frame's `worldPos`. SH-3 / #641 — composing through \
             `bones[]` for both frames is the bug this test guards."
        );
        assert!(
            src.contains("xformPrev"),
            "triangle.vert: a separate `xformPrev` matrix must be \
             composed from `bones_prev` so `prevWorldPos` reflects \
             last frame's joint poses (SH-3 / #641)."
        );
    }

    /// Regression for #575 / SH-1. The global `GlobalVertices` SSBO
    /// is declared as `float vertexData[]` so every read implicitly
    /// reinterprets the bytes as IEEE-754 float. Per the layout
    /// table at the SSBO declaration in triangle.frag:
    ///
    ///   - safe float offsets: `position` (0..2), `color` (3..5),
    ///     `normal` (6..8), `uv` (9..10), `bone_weights` (15..18).
    ///   - **unsafe** offsets (require `floatBitsToUint` /
    ///     `unpackUnorm4x8` recovery): `bone_indices` (11..14),
    ///     `splat_weights_0/1` (19..20).
    ///
    /// Pre-fix, a future RT shader author following the existing
    /// `vertexData[base + N]` pattern could silently read u32 /
    /// packed-u8 bit patterns as floats. This test grep-checks the
    /// only shader that currently reads `vertexData` (triangle.frag)
    /// for any forbidden offset — `+ 11` through `+ 14` (bone
    /// indices) or `+ 19` / `+ 20` (splat weights) — that ISN'T
    /// wrapped in `floatBitsToUint(…)` or `unpackUnorm4x8(…)`.
    ///
    /// `caustic_splat.comp` and `ui.vert` don't bind GlobalVertices
    /// at all and aren't checked. `skin_vertices.comp` reads bone
    /// indices but does so through `floatBitsToUint`; the regex
    /// excludes that pattern.
    #[test]
    fn triangle_frag_no_unsafe_vertex_data_reads() {
        let src = include_str!("../../shaders/triangle.frag");

        // Strip safe-recovery wrappers so a forbidden raw read
        // surfaces as a literal `vertexData[... + 11..14|19|20]`.
        // We don't run a full GLSL parser; instead, line-by-line
        // we reject any line that contains the forbidden offset
        // pattern AND no `floatBitsToUint` / `unpackUnorm4x8` /
        // `floatBitsToInt` recovery call. Whitespace tolerant.
        for (lineno, line) in src.lines().enumerate() {
            // Skip the SSBO-declaration block — it documents the
            // unsafe offsets but doesn't read them.
            if line.contains("WARNING")
                || line.contains("│")
                || line.contains("//")
                    && (line.contains("floatBitsToUint") || line.contains("unpackUnorm4x8"))
            {
                continue;
            }
            // Look for `vertexData[ ... + N ]` where N is 11-14 or
            // 19-20. Tolerate whitespace and the `(vOff + iN)` outer
            // expression that the existing `getHitUV` site uses.
            for forbidden in [11, 12, 13, 14, 19, 20] {
                let needle_simple = format!("+ {}]", forbidden);
                let needle_alt = format!("+{}]", forbidden);
                if line.contains(&needle_simple) || line.contains(&needle_alt) {
                    // Allow the read when it's wrapped in a
                    // recovery call.
                    if line.contains("floatBitsToUint")
                        || line.contains("unpackUnorm4x8")
                        || line.contains("floatBitsToInt")
                    {
                        continue;
                    }
                    panic!(
                        "triangle.frag:{}: unsafe `vertexData[... + {}]` read \
                         (offset {} is {} — not an IEEE-754 float). Use \
                         `floatBitsToUint(...)` or `unpackUnorm4x8(...)` to \
                         recover the bit pattern. See #575 / SH-1.\nLine: {}",
                        lineno + 1,
                        forbidden,
                        forbidden,
                        if (11..=14).contains(&forbidden) {
                            "u32 (bone index)"
                        } else {
                            "packed 4× u8 unorm (splat weight)"
                        },
                        line.trim()
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod material_hash_tests {
    //! Regression tests for #878 / DIM8-01: the dirty-gate in
    //! `upload_materials` skips the per-frame `copy_nonoverlapping +
    //! flush_if_needed` when the new materials slice is byte-
    //! identical to the last upload. The hash function is the part
    //! that's testable without a real Vulkan device — the upload
    //! itself needs `mapped_slice_mut` / `flush_if_needed`. These
    //! tests pin the hash's content-addressing contract:
    //!
    //!   1. Two identical slices produce the same hash → skip fires.
    //!   2. A single byte change anywhere produces a different hash
    //!      → skip stays off and the upload runs.
    //!   3. Empty slice has its own deterministic hash (the `count
    //!      == 0` early-out returns before reaching the hash, but
    //!      the hash itself is still well-defined).
    use super::super::material::GpuMaterial;
    use super::hash_material_slice;

    fn sample_material(seed: u32) -> GpuMaterial {
        let mut m = GpuMaterial::default();
        // Touch a representative subset of fields so the hash
        // depends on real material content rather than padding.
        m.material_flags = seed;
        m.material_kind = (seed & 0xff) as u32;
        m
    }

    /// Pin: identical slices produce identical hashes — the steady-
    /// state case the dirty-gate is designed to detect.
    #[test]
    fn identical_slices_hash_to_same_value() {
        let mats: Vec<GpuMaterial> = (0..16).map(sample_material).collect();
        let h1 = hash_material_slice(&mats);
        let h2 = hash_material_slice(&mats);
        assert_eq!(
            h1, h2,
            "identical slice contents must hash to the same value — \
             the dirty-gate skip relies on this",
        );
    }

    /// Pin: a single-bit change in one material produces a different
    /// hash. Without this, a real material change would silently
    /// skip the upload and the GPU would render with stale data.
    #[test]
    fn single_field_change_changes_hash() {
        let mut mats: Vec<GpuMaterial> = (0..16).map(sample_material).collect();
        let h_before = hash_material_slice(&mats);
        mats[7].material_flags ^= 1;
        let h_after = hash_material_slice(&mats);
        assert_ne!(
            h_before, h_after,
            "a single field change must shift the hash — \
             else the upload would skip a real material update",
        );
    }

    /// Pin: a length-only change (one extra zero-default material
    /// appended) produces a different hash even when every existing
    /// entry is byte-identical. Without this, growing the table by
    /// adding a default-material slot would silently skip the
    /// upload.
    #[test]
    fn length_change_changes_hash() {
        let mats: Vec<GpuMaterial> = (0..16).map(sample_material).collect();
        let mut grown = mats.clone();
        grown.push(GpuMaterial::default());
        assert_ne!(
            hash_material_slice(&mats),
            hash_material_slice(&grown),
            "length change must shift the hash",
        );
    }

    /// Empty-slice hash is deterministic. Production callers route
    /// the `count == 0` case through an early-out before the hash
    /// computation, so this is documentary — but pinning the hash's
    /// behaviour at the boundary stops drift if the early-out is
    /// ever moved.
    #[test]
    fn empty_slice_hash_is_deterministic() {
        let h1 = hash_material_slice(&[]);
        let h2 = hash_material_slice(&[]);
        assert_eq!(h1, h2);
    }
}

#[cfg(test)]
mod scene_descriptor_reflection_tests {
    //! Regression tests for #950 / SAFE-25: the scene descriptor set
    //! layout (set=1) consumed by the main raster pipeline must agree
    //! with `triangle.vert.spv` + `triangle.frag.spv`.
    //!
    //! Production `SceneBuffers::new` calls `validate_set_layout`
    //! before `vkCreateDescriptorSetLayout` — but that runtime check
    //! only fires when a real Vulkan device exists, so CI without GPU
    //! access can't catch a binding-table drift. These tests pull the
    //! bindings through the same `build_scene_descriptor_bindings`
    //! helper production uses and validate against the include_bytes!'d
    //! SPIR-V at `cargo test` time, so drift trips before the first
    //! frame ever runs.
    use super::*;

    fn triangle_shaders() -> [super::super::reflect::ReflectedShader<'static>; 2] {
        [
            super::super::reflect::ReflectedShader {
                name: "triangle.vert",
                spirv: super::super::pipeline::TRIANGLE_VERT_SPV,
            },
            super::super::reflect::ReflectedShader {
                name: "triangle.frag",
                spirv: super::super::pipeline::TRIANGLE_FRAG_SPV,
            },
        ]
    }

    /// RT-enabled path: every binding 0..=13 (with TLAS at 2) must be
    /// declared in `triangle.vert` ∪ `triangle.frag` with the matching
    /// descriptor type. No `optional_shader_bindings` — every declared
    /// binding must be consumed by the layout.
    #[test]
    fn rt_enabled_layout_matches_triangle_shaders() {
        let bindings = build_scene_descriptor_bindings(true);
        super::super::reflect::validate_set_layout(
            1,
            &bindings,
            &triangle_shaders(),
            "scene (set=1, rt=on)",
            &[],
        )
        .expect("scene descriptor layout (rt=on) must match triangle shaders");
    }

    /// RT-disabled path: TLAS binding (2) is intentionally absent from
    /// the layout but still declared in the shader, gated at runtime by
    /// the per-fragment `rayQuery` uniform flag. The validator must list
    /// it in `optional_shader_bindings` so the shader-declared-but-
    /// layout-absent case doesn't fire a false positive.
    #[test]
    fn rt_disabled_layout_matches_triangle_shaders_with_optional_tlas() {
        let bindings = build_scene_descriptor_bindings(false);
        // TLAS (binding 2) is shader-declared but absent from the
        // RT-disabled layout — and must not be in the bindings vec.
        assert!(
            !bindings.iter().any(|b| b.binding == 2),
            "rt_enabled=false must omit binding 2 (TLAS)",
        );
        super::super::reflect::validate_set_layout(
            1,
            &bindings,
            &triangle_shaders(),
            "scene (set=1, rt=off)",
            &[2],
        )
        .expect("scene descriptor layout (rt=off) must match triangle shaders");
    }

    /// Synthetic drift: dropping binding 4 (instance SSBO) from the
    /// layout must produce a descriptive failure. Pin the rejection
    /// path so a future shader change that *removes* a binding without
    /// also removing it from the production helper trips a clear
    /// error rather than silently passing.
    #[test]
    fn dropping_instance_binding_fails_with_diagnostic() {
        let mut bindings = build_scene_descriptor_bindings(true);
        let before = bindings.len();
        bindings.retain(|b| b.binding != 4);
        assert_eq!(
            bindings.len(),
            before - 1,
            "fixture must actually drop binding 4",
        );
        // After removing binding 4 from the Rust side, the shader still
        // declares it — validate must flag the shader's extra binding
        // since it is not in `optional_shader_bindings`.
        let err = super::super::reflect::validate_set_layout(
            1,
            &bindings,
            &triangle_shaders(),
            "scene (set=1, rt=on, drift)",
            &[],
        )
        .expect_err("dropping binding 4 must trip a layout drift error");
        let msg = format!("{err}");
        assert!(
            msg.contains("binding=4"),
            "diagnostic must name the offending binding (4): {msg}",
        );
    }
}
