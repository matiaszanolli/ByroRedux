//! `#[repr(C)]` types uploaded to the per-frame scene SSBOs.
//!
//! `GpuInstance`, `GpuLight`, `GpuCamera`, `GpuTerrainTile` + their `Default`
//! impls. Byte layout is shader-contract-critical — see the layout tests in
//! [`super::gpu_instance_layout_tests`].


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

/// GPU-side camera data (288 bytes, std140-compatible).
///
/// Layout pinned by `gpu_camera_layout_pin` test — three `mat4`
/// (3×64 = 192 B) + nine `vec4` (9×16 = 144 B, wait — six trailing vec4s
/// post-#925 / #1028: position, flags, screen, fog, jitter, sky_tint
/// = 6×16 = 96 B) → 288 B. Every shader that re-declares this struct
/// MUST keep field order and field count in lockstep:
///
/// * `triangle.vert`, `triangle.frag`, `water.vert`, `water.frag`
///   (set 1, binding 1).
/// * `cluster_cull.comp` (set 0, binding 1).
/// * `caustic_splat.comp` (set 0, binding 4).
///
/// See [`feedback_shader_struct_sync`] for the broader policy and
/// #1028 / R-D6-01 for the audit that caught `triangle.vert` /
/// `cluster_cull.comp` / `caustic_splat.comp` lagging behind the
/// `sky_tint` addition.
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
    /// `CompositeParams.sky_zenith.xyz`); w = sun angular radius (rad,
    /// half-angle of the directional-light disk used for PCSS-lite
    /// shadow jitter in `triangle.frag` — see #1023 / REN-D20-NEW-01).
    /// Sourced from the same `SkyParams.zenith_color` /
    /// `sun_angular_radius` that drives `compute_sky` so the
    /// triangle.frag window-portal escape transmits a sky tint
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
            // w = sun_angular_radius (rad); default matches the
            // pre-#1023 triangle.frag hardcoded const (0.020 ≈ 1.15°).
            sky_tint: [0.6, 0.75, 1.0, 0.020],
        }
    }
}

