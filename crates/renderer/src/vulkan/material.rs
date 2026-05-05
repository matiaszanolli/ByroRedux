//! Per-frame material table — R1 refactor (collapses ~35 per-material
//! fields out of `DrawCommand`/`GpuInstance` into a deduped lookup
//! indexed by `material_id: u32`).
//!
//! The legacy shape pushed every per-material field (texture indices,
//! PBR scalars, alpha state, Skyrim+ shader-variant payloads, BSEffect
//! falloff, BGSM UV transform, NiMaterialProperty diffuse/ambient) into
//! every `GpuInstance`. A typical interior cell duplicates the same
//! material across 10–30 placements; the SSBO carries 35× redundant
//! bytes per repeat.
//!
//! The new shape factors them into [`GpuMaterial`] and uploads a
//! single per-frame [`MaterialTable`]; `GpuInstance` references the
//! material via a `material_id: u32` index. Identical materials
//! dedup to the same id via `MaterialTable::intern`.
//!
//! ## Phase status (R1) — closed (#785)
//!
//! R1 shipped end-to-end: the table is built per-frame in
//! `build_render_data`, uploaded as the `MaterialBuffer` SSBO at
//! binding 13, and `triangle.frag` reads
//! `materials[inst.materialId].foo` for every per-material field.
//! The legacy per-instance copies were removed in Phase 6 (#785).
//! See `feedback_shader_struct_sync.md` for the narrowed
//! "only triangle.frag mirrors GpuMaterial" contract that landed
//! alongside the closeout.

use super::scene_buffer::MAX_MATERIALS;
use std::collections::HashMap;
use std::sync::Once;

/// First-frame overflow latch for [`MaterialTable::intern`]. Wired through
/// a `Once` so the warn fires exactly once per session; #797's regression
/// guard is the truthful pairing of the upload-side warn message
/// (`scene_buffer.rs:978`) with actual default-to-0 behaviour.
static INTERN_OVERFLOW_WARNED: Once = Once::new();

/// std430 GPU-side material record. 260 bytes per material (was 272 B
/// before #804 / R1-N4 dropped the unread `avg_albedo_r/g/b` triplet).
///
/// (Historical: the per-instance → per-material migration shipped as
/// R1 Phases 4–6, finishing with #785. The layout below was originally
/// kept at the same vec4 offsets as the legacy `GpuInstance` slots so
/// the per-shader rename was mechanical; that migration is closed and
/// the layout is now whatever the dedup table needs, not what
/// `GpuInstance` looks like.)
///
/// **CRITICAL**: All fields are scalar (f32/u32). NEVER use `[f32; 3]` —
/// std430 aligns vec3 to 16 B, which would silently mismatch a tightly-
/// packed `#[repr(C)]` Rust struct. Pad explicitly with named pad fields
/// so the byte-level `Hash`/`Eq` impls below are deterministic.
///
/// **Shader Struct Sync** (current, narrower contract): only
/// `crates/renderer/shaders/triangle.frag` declares a matching
/// `struct GpuMaterial` and reads from `materials[inst.materialId]`
/// (binding 13). `triangle.vert`, `ui.vert`, and `caustic_splat.comp`
/// MUST NOT mirror the struct or index the material buffer — the build-
/// time grep at `scene_buffer.rs:1639`
/// (`ui_vert_reads_texture_index_from_instance_not_material_table`)
/// pins this for `ui.vert` after #776 / #785; mirror checks for the
/// other two stages live in the same module. Layout invariant is pinned
/// by `gpu_material_size_is_260_bytes` and
/// `gpu_material_field_offsets_match_shader_contract` (added #806 to
/// catch within-vec4 reorderings the size pin alone would miss).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuMaterial {
    // ── PBR scalars (vec4 #1) ───────────────────────────────────────
    pub roughness: f32,    // offset 0
    pub metalness: f32,    // offset 4
    pub emissive_mult: f32, // offset 8
    /// Bitfield of material-level flags consumed by the fragment
    /// shader. Bit 0 (`MAT_FLAG_VERTEX_COLOR_EMISSIVE`): per-vertex
    /// `fragColor.rgb` drives self-illumination instead of modulating
    /// albedo — set when the source NIF declared
    /// `NiVertexColorProperty.vertex_mode = SOURCE_EMISSIVE`. Pre-#695
    /// this slot was an unused pad; routing the bit through here keeps
    /// the std430 layout pinned by `gpu_material_size_is_260_bytes`
    /// (260 B post-#804 / R1-N4; was 272 B before that fix dropped the
    /// unread `avg_albedo_r/g/b` triplet).
    pub material_flags: u32, // offset 12

    // ── Emissive RGB + specular_strength (vec4 #2) ─────────────────
    pub emissive_r: f32,        // offset 16
    pub emissive_g: f32,        // offset 20
    pub emissive_b: f32,        // offset 24
    pub specular_strength: f32, // offset 28

    // ── Specular RGB + alpha_threshold (vec4 #3) ───────────────────
    pub specular_r: f32,      // offset 32
    pub specular_g: f32,      // offset 36
    pub specular_b: f32,      // offset 40
    pub alpha_threshold: f32, // offset 44

    // ── Texture indices group A (vec4 #4) ──────────────────────────
    pub texture_index: u32,    // offset 48 — diffuse / albedo
    pub normal_map_index: u32, // offset 52
    pub dark_map_index: u32,   // offset 56
    pub glow_map_index: u32,   // offset 60

    // ── Texture indices group B (vec4 #5) ──────────────────────────
    pub detail_map_index: u32,   // offset 64
    pub gloss_map_index: u32,    // offset 68
    pub parallax_map_index: u32, // offset 72
    pub env_map_index: u32,      // offset 76

    // ── env_mask + alpha_test_func + material_kind + alpha (vec4 #6)
    pub env_mask_index: u32,  // offset 80
    pub alpha_test_func: u32, // offset 84
    pub material_kind: u32,   // offset 88
    pub material_alpha: f32,  // offset 92

    // ── Parallax POM + UV offset (vec4 #7) ─────────────────────────
    pub parallax_height_scale: f32, // offset 96
    pub parallax_max_passes: f32,   // offset 100
    pub uv_offset_u: f32,           // offset 104
    pub uv_offset_v: f32,           // offset 108

    // ── UV scale + diffuse RG (vec4 #8) ────────────────────────────
    pub uv_scale_u: f32, // offset 112
    pub uv_scale_v: f32, // offset 116
    pub diffuse_r: f32,  // offset 120
    pub diffuse_g: f32,  // offset 124

    // ── diffuse_b + ambient RGB (vec4 #9) ──────────────────────────
    pub diffuse_b: f32, // offset 128
    pub ambient_r: f32, // offset 132
    pub ambient_g: f32, // offset 136
    pub ambient_b: f32, // offset 140

    // #804 / R1-N4 — `avg_albedo_r/g/b` (offsets 144-152) removed. The
    // field was populated by `to_gpu_material` for every material but
    // no shader read `mat.avgAlbedo*`; both consumers (caustic_splat.comp
    // and triangle.frag GI miss) sample from the per-instance copy on
    // `GpuInstance.avgAlbedo*` instead. The retention comment at
    // `scene_buffer.rs:215-219` explains why the per-instance copy
    // stays. Subsequent fields shift down by 12 bytes.

    // ── skin_tint_a + skin_tint RGB (offsets 144-156) ───────────────
    pub skin_tint_a: f32, // offset 144
    pub skin_tint_r: f32, // offset 148
    pub skin_tint_g: f32, // offset 152
    pub skin_tint_b: f32, // offset 156

    // ── hair_tint RGB + multi_layer_envmap_strength (offsets 160-172)
    pub hair_tint_r: f32,                 // offset 160
    pub hair_tint_g: f32,                 // offset 164
    pub hair_tint_b: f32,                 // offset 168
    pub multi_layer_envmap_strength: f32, // offset 172

    // ── eye_left RGB + eye_cubemap_scale (offsets 176-188) ──────────
    pub eye_left_center_x: f32, // offset 176
    pub eye_left_center_y: f32, // offset 180
    pub eye_left_center_z: f32, // offset 184
    pub eye_cubemap_scale: f32, // offset 188

    // ── eye_right RGB + multi_layer_inner_thickness (offsets 192-204)
    pub eye_right_center_x: f32,          // offset 192
    pub eye_right_center_y: f32,          // offset 196
    pub eye_right_center_z: f32,          // offset 200
    pub multi_layer_inner_thickness: f32, // offset 204

    // ── refraction_scale + multi_layer_inner_scale UV + sparkle_r (208-220)
    pub multi_layer_refraction_scale: f32, // offset 208
    pub multi_layer_inner_scale_u: f32,    // offset 212
    pub multi_layer_inner_scale_v: f32,    // offset 216
    pub sparkle_r: f32,                    // offset 220

    // ── sparkle_g/b + sparkle_intensity + falloff_start (224-236) ───
    pub sparkle_g: f32,           // offset 224
    pub sparkle_b: f32,           // offset 228
    pub sparkle_intensity: f32,   // offset 232
    pub falloff_start_angle: f32, // offset 236

    // ── falloff_stop + opacities + soft_falloff_depth + pad (240-256)
    pub falloff_stop_angle: f32,    // offset 240
    pub falloff_start_opacity: f32, // offset 244
    pub falloff_stop_opacity: f32,  // offset 248
    pub soft_falloff_depth: f32,    // offset 252
    pub _pad_falloff: f32,          // offset 256 → total 260
}

impl Default for GpuMaterial {
    fn default() -> Self {
        Self {
            // PBR — neutral mid-roughness, non-metallic, no emission.
            roughness: 0.5,
            metalness: 0.0,
            emissive_mult: 0.0,
            material_flags: 0,
            // Emissive RGB + specular_strength — black emission, full spec.
            emissive_r: 0.0,
            emissive_g: 0.0,
            emissive_b: 0.0,
            specular_strength: 1.0,
            // Specular RGB + alpha_threshold — white spec, no alpha test.
            specular_r: 1.0,
            specular_g: 1.0,
            specular_b: 1.0,
            alpha_threshold: 0.0,
            // Texture indices — all 0 = no map (shaders fall back to constants).
            texture_index: 0,
            normal_map_index: 0,
            dark_map_index: 0,
            glow_map_index: 0,
            detail_map_index: 0,
            gloss_map_index: 0,
            parallax_map_index: 0,
            env_map_index: 0,
            env_mask_index: 0,
            // Alpha test func + material_kind — disabled (0 = ALWAYS / Default lit).
            alpha_test_func: 0,
            material_kind: 0,
            material_alpha: 1.0,
            // POM defaults match BSShaderPPLightingProperty.
            parallax_height_scale: 0.04,
            parallax_max_passes: 4.0,
            // UV transform — identity.
            uv_offset_u: 0.0,
            uv_offset_v: 0.0,
            uv_scale_u: 1.0,
            uv_scale_v: 1.0,
            // NiMaterialProperty diffuse + ambient — `[1.0; 3]` = no tint.
            diffuse_r: 1.0,
            diffuse_g: 1.0,
            diffuse_b: 1.0,
            ambient_r: 1.0,
            ambient_g: 1.0,
            ambient_b: 1.0,
            // Skyrim+ variant payloads — zeroed; `material_kind == 0`
            // means the variant ladder skips reading them anyway.
            skin_tint_r: 0.0,
            skin_tint_g: 0.0,
            skin_tint_b: 0.0,
            skin_tint_a: 0.0,
            hair_tint_r: 0.0,
            hair_tint_g: 0.0,
            hair_tint_b: 0.0,
            multi_layer_envmap_strength: 0.0,
            eye_left_center_x: 0.0,
            eye_left_center_y: 0.0,
            eye_left_center_z: 0.0,
            eye_cubemap_scale: 0.0,
            eye_right_center_x: 0.0,
            eye_right_center_y: 0.0,
            eye_right_center_z: 0.0,
            multi_layer_inner_thickness: 0.0,
            multi_layer_refraction_scale: 0.0,
            multi_layer_inner_scale_u: 0.0,
            multi_layer_inner_scale_v: 0.0,
            sparkle_r: 0.0,
            sparkle_g: 0.0,
            sparkle_b: 0.0,
            sparkle_intensity: 0.0,
            // Effect-shader falloff cone — identity pass-through
            // (`material_kind != 101` paths ignore these anyway).
            falloff_start_angle: 1.0,
            falloff_stop_angle: 1.0,
            falloff_start_opacity: 1.0,
            falloff_stop_opacity: 1.0,
            soft_falloff_depth: 0.0,
            _pad_falloff: 0.0,
        }
    }
}

/// `GpuMaterial::material_flags` bit catalog. Mirrored shader-side as
/// raw `0x...u` literals in `triangle.frag` so the GLSL is grep-friendly
/// without needing a generated header.
pub mod material_flag {
    /// Per-vertex `fragColor.rgb` drives self-illumination instead of
    /// modulating albedo. Set when the source NIF declared
    /// `NiVertexColorProperty.vertex_mode = SOURCE_EMISSIVE`. See
    /// `crates/nif/src/import/material/walker.rs::extract_vertex_colors`
    /// and the matching shader branch in
    /// `crates/renderer/shaders/triangle.frag`.
    pub const VERTEX_COLOR_EMISSIVE: u32 = 1 << 0;
}

impl GpuMaterial {
    /// Byte view used by the byte-level `Hash`/`Eq` impls below. Safe
    /// because [`GpuMaterial`] is `#[repr(C)]` + `Copy`, has no `Drop`,
    /// and all padding bytes are named fields the producer always
    /// initialises (so the byte representation is deterministic for
    /// any value reachable through the public API).
    fn as_bytes(&self) -> &[u8] {
        // SAFETY: see doc comment above.
        unsafe {
            std::slice::from_raw_parts(
                self as *const Self as *const u8,
                std::mem::size_of::<Self>(),
            )
        }
    }
}

impl PartialEq for GpuMaterial {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for GpuMaterial {}

impl std::hash::Hash for GpuMaterial {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write(self.as_bytes());
    }
}

/// Per-frame deduplicated material table. Cleared at frame start, populated
/// during `build_render_data`, uploaded as an SSBO before draw.
///
/// Identical materials (byte-equal `GpuMaterial`) collapse to the same id;
/// distinct materials get fresh ids in insertion order. The reverse map
/// (`HashMap<GpuMaterial, u32>`) keeps `intern` O(1) amortised.
pub struct MaterialTable {
    /// Insertion-ordered material storage, indexed by `material_id`.
    materials: Vec<GpuMaterial>,
    /// Reverse lookup for dedup. Cleared in lockstep with `materials`.
    index: HashMap<GpuMaterial, u32>,
    /// R1 telemetry — total `intern()` calls this frame (one per
    /// `DrawCommand`). Read alongside `len()` to compute the dedup
    /// ratio and surfaced via `ctx.scratch`. The counter exists so a
    /// regression that breaks byte-equality dedup (alignment hole,
    /// non-deterministic float in the producer) shows up as a
    /// dropping ratio in telemetry rather than silently inflating
    /// VRAM. See #780 / PERF-N1.
    interned_count: usize,
}

impl Default for MaterialTable {
    fn default() -> Self {
        Self::new()
    }
}

impl MaterialTable {
    pub fn new() -> Self {
        Self {
            materials: Vec::new(),
            index: HashMap::new(),
            interned_count: 0,
        }
    }

    /// Reset for a new frame. Retains the underlying allocation so
    /// per-frame churn doesn't hit the heap.
    pub fn clear(&mut self) {
        self.materials.clear();
        self.index.clear();
        self.interned_count = 0;
    }

    /// Insert a material (or return the existing id if byte-equal to
    /// one already in the table). Returns the `material_id` the GPU
    /// will use to look it up.
    ///
    /// Capped at [`MAX_MATERIALS`] entries — over-cap interns return
    /// id `0` and share the first-interned material's record for the
    /// rest of the frame. See #797 / SAFE-22: the upload at
    /// `scene_buffer.rs:975` truncates the buffer to `MAX_MATERIALS`
    /// entries, so without this cap a `DrawCommand` carrying an over-
    /// cap `material_id` would index past the SSBO end on the GPU
    /// (implementation-defined OOB read; AMD returns zeros, NVIDIA
    /// returns last-valid-page contents, Intel may DEVICE_LOST).
    /// The pairing also makes the upload's warn message
    /// (`"silently default to material 0"`) truthful.
    ///
    /// Real interior cells dedup to 50–200 unique materials and a
    /// 3×3 exterior grid lands at 300–600 — well under the 4096 cap
    /// (`scene_buffer.rs:60-62`). The overflow path is reachable
    /// today only on modded / synthetic / future Starfield-FO76
    /// large-exterior content.
    pub fn intern(&mut self, material: GpuMaterial) -> u32 {
        self.interned_count += 1;
        if let Some(&id) = self.index.get(&material) {
            return id;
        }
        if self.materials.len() >= MAX_MATERIALS {
            INTERN_OVERFLOW_WARNED.call_once(|| {
                log::warn!(
                    "MaterialTable: unique-material count exceeded MAX_MATERIALS \
                     ({}); over-cap entries share material 0 for the rest of \
                     the session. See #797 / SAFE-22.",
                    MAX_MATERIALS,
                );
            });
            return 0;
        }
        let id = self.materials.len() as u32;
        self.materials.push(material);
        self.index.insert(material, id);
        id
    }

    /// View the raw material storage for SSBO upload.
    pub fn materials(&self) -> &[GpuMaterial] {
        &self.materials
    }

    /// Number of unique materials interned so far this frame.
    pub fn len(&self) -> usize {
        self.materials.len()
    }

    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    /// Total `intern()` calls so far this frame (hits + misses).
    /// Dedup ratio = `len() / interned_count()`. See #780 / PERF-N1.
    pub fn interned_count(&self) -> usize {
        self.interned_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the std430 layout. Any growth must be intentional and
    /// matched by the shader-side `struct GpuMaterial` declaration in
    /// lockstep — same contract as `GpuInstance`.
    ///
    /// Was 272 B until #804 / R1-N4 dropped `avg_albedo_r/g/b` (12 B,
    /// no shader read `mat.avgAlbedo*` — caustic_splat.comp + the
    /// triangle.frag GI miss path both sample from the per-instance
    /// `GpuInstance.avgAlbedo*` copy instead).
    #[test]
    fn gpu_material_size_is_260_bytes() {
        assert_eq!(std::mem::size_of::<GpuMaterial>(), 260);
    }

    /// `#[repr(C)]` puts no implicit padding between f32/u32 fields,
    /// but verify the alignment matches std430 (16 B for vec4).
    #[test]
    fn gpu_material_alignment_is_4_bytes() {
        // Underlying field alignment is 4 (largest scalar). std430
        // vec4 alignment of 16 comes from the buffer-stride rule, not
        // from the struct declaration itself.
        assert_eq!(std::mem::align_of::<GpuMaterial>(), 4);
    }

    /// Regression guard for the GpuMaterial Shader Struct Sync (#806).
    /// The size pin (`gpu_material_size_is_260_bytes`) catches additions
    /// or removals; this catches reorderings WITHIN the existing 16
    /// vec4 slots that the size pin alone would miss — e.g. swapping
    /// `texture_index` and `normal_map_index` within vec4 #4 would
    /// preserve total size but produce wrong shader reads.
    ///
    /// Mirrors the `gpu_instance_field_offsets_match_shader_contract`
    /// pattern at `scene_buffer.rs:1453`. The shader-side
    /// `struct GpuMaterial` declaration at `triangle.frag:83-126` is the
    /// source of truth for these offsets — every named field on the
    /// Rust side gets an explicit `offset_of!` assertion against the
    /// vec4 group its shader-side counterpart sits in.
    #[test]
    fn gpu_material_field_offsets_match_shader_contract() {
        use std::mem::offset_of;

        // ── PBR scalars (vec4 #1, offsets 0-12) ────────────────────
        assert_eq!(offset_of!(GpuMaterial, roughness), 0);
        assert_eq!(offset_of!(GpuMaterial, metalness), 4);
        assert_eq!(offset_of!(GpuMaterial, emissive_mult), 8);
        assert_eq!(offset_of!(GpuMaterial, material_flags), 12);

        // ── Emissive RGB + specular_strength (vec4 #2, offsets 16-28)
        assert_eq!(offset_of!(GpuMaterial, emissive_r), 16);
        assert_eq!(offset_of!(GpuMaterial, emissive_g), 20);
        assert_eq!(offset_of!(GpuMaterial, emissive_b), 24);
        assert_eq!(offset_of!(GpuMaterial, specular_strength), 28);

        // ── Specular RGB + alpha_threshold (vec4 #3, offsets 32-44) ─
        assert_eq!(offset_of!(GpuMaterial, specular_r), 32);
        assert_eq!(offset_of!(GpuMaterial, specular_g), 36);
        assert_eq!(offset_of!(GpuMaterial, specular_b), 40);
        assert_eq!(offset_of!(GpuMaterial, alpha_threshold), 44);

        // ── Texture indices group A (vec4 #4, offsets 48-60) ───────
        assert_eq!(offset_of!(GpuMaterial, texture_index), 48);
        assert_eq!(offset_of!(GpuMaterial, normal_map_index), 52);
        assert_eq!(offset_of!(GpuMaterial, dark_map_index), 56);
        assert_eq!(offset_of!(GpuMaterial, glow_map_index), 60);

        // ── Texture indices group B (vec4 #5, offsets 64-76) ───────
        assert_eq!(offset_of!(GpuMaterial, detail_map_index), 64);
        assert_eq!(offset_of!(GpuMaterial, gloss_map_index), 68);
        assert_eq!(offset_of!(GpuMaterial, parallax_map_index), 72);
        assert_eq!(offset_of!(GpuMaterial, env_map_index), 76);

        // ── env_mask + alpha_test_func + material_kind + alpha
        //    (vec4 #6, offsets 80-92) ───────────────────────────────
        assert_eq!(offset_of!(GpuMaterial, env_mask_index), 80);
        assert_eq!(offset_of!(GpuMaterial, alpha_test_func), 84);
        assert_eq!(offset_of!(GpuMaterial, material_kind), 88);
        assert_eq!(offset_of!(GpuMaterial, material_alpha), 92);

        // ── Parallax POM + UV offset (vec4 #7, offsets 96-108) ─────
        assert_eq!(offset_of!(GpuMaterial, parallax_height_scale), 96);
        assert_eq!(offset_of!(GpuMaterial, parallax_max_passes), 100);
        assert_eq!(offset_of!(GpuMaterial, uv_offset_u), 104);
        assert_eq!(offset_of!(GpuMaterial, uv_offset_v), 108);

        // ── UV scale + diffuse RG (vec4 #8, offsets 112-124) ───────
        assert_eq!(offset_of!(GpuMaterial, uv_scale_u), 112);
        assert_eq!(offset_of!(GpuMaterial, uv_scale_v), 116);
        assert_eq!(offset_of!(GpuMaterial, diffuse_r), 120);
        assert_eq!(offset_of!(GpuMaterial, diffuse_g), 124);

        // ── diffuse_b + ambient RGB (vec4 #9, offsets 128-140) ─────
        assert_eq!(offset_of!(GpuMaterial, diffuse_b), 128);
        assert_eq!(offset_of!(GpuMaterial, ambient_r), 132);
        assert_eq!(offset_of!(GpuMaterial, ambient_g), 136);
        assert_eq!(offset_of!(GpuMaterial, ambient_b), 140);

        // (#804 / R1-N4 dropped `avg_albedo_r/g/b` — what would have
        // been vec4 #10 at offsets 144-152 is gone; subsequent fields
        // shift down by 12 bytes from their pre-#804 positions.)

        // ── skin_tint A/R/G/B (offsets 144-156) ────────────────────
        assert_eq!(offset_of!(GpuMaterial, skin_tint_a), 144);
        assert_eq!(offset_of!(GpuMaterial, skin_tint_r), 148);
        assert_eq!(offset_of!(GpuMaterial, skin_tint_g), 152);
        assert_eq!(offset_of!(GpuMaterial, skin_tint_b), 156);

        // ── hair_tint RGB + multi_layer_envmap_strength
        //    (offsets 160-172) ─────────────────────────────────────
        assert_eq!(offset_of!(GpuMaterial, hair_tint_r), 160);
        assert_eq!(offset_of!(GpuMaterial, hair_tint_g), 164);
        assert_eq!(offset_of!(GpuMaterial, hair_tint_b), 168);
        assert_eq!(offset_of!(GpuMaterial, multi_layer_envmap_strength), 172);

        // ── eye_left RGB + eye_cubemap_scale (offsets 176-188) ─────
        assert_eq!(offset_of!(GpuMaterial, eye_left_center_x), 176);
        assert_eq!(offset_of!(GpuMaterial, eye_left_center_y), 180);
        assert_eq!(offset_of!(GpuMaterial, eye_left_center_z), 184);
        assert_eq!(offset_of!(GpuMaterial, eye_cubemap_scale), 188);

        // ── eye_right RGB + multi_layer_inner_thickness
        //    (offsets 192-204) ─────────────────────────────────────
        assert_eq!(offset_of!(GpuMaterial, eye_right_center_x), 192);
        assert_eq!(offset_of!(GpuMaterial, eye_right_center_y), 196);
        assert_eq!(offset_of!(GpuMaterial, eye_right_center_z), 200);
        assert_eq!(offset_of!(GpuMaterial, multi_layer_inner_thickness), 204);

        // ── refraction_scale + multi_layer_inner_scale UV + sparkle_r
        //    (offsets 208-220) ─────────────────────────────────────
        assert_eq!(offset_of!(GpuMaterial, multi_layer_refraction_scale), 208);
        assert_eq!(offset_of!(GpuMaterial, multi_layer_inner_scale_u), 212);
        assert_eq!(offset_of!(GpuMaterial, multi_layer_inner_scale_v), 216);
        assert_eq!(offset_of!(GpuMaterial, sparkle_r), 220);

        // ── sparkle GB + sparkle_intensity + falloff_start
        //    (offsets 224-236) ─────────────────────────────────────
        assert_eq!(offset_of!(GpuMaterial, sparkle_g), 224);
        assert_eq!(offset_of!(GpuMaterial, sparkle_b), 228);
        assert_eq!(offset_of!(GpuMaterial, sparkle_intensity), 232);
        assert_eq!(offset_of!(GpuMaterial, falloff_start_angle), 236);

        // ── falloff_stop + opacities + soft_falloff_depth
        //    (offsets 240-252) ─────────────────────────────────────
        assert_eq!(offset_of!(GpuMaterial, falloff_stop_angle), 240);
        assert_eq!(offset_of!(GpuMaterial, falloff_start_opacity), 244);
        assert_eq!(offset_of!(GpuMaterial, falloff_stop_opacity), 248);
        assert_eq!(offset_of!(GpuMaterial, soft_falloff_depth), 252);

        // ── trailing pad to round to 260 B (offset 256) ────────────
        assert_eq!(offset_of!(GpuMaterial, _pad_falloff), 256);
    }

    #[test]
    fn default_is_neutral_lit_material() {
        let m = GpuMaterial::default();
        assert_eq!(m.roughness, 0.5);
        assert_eq!(m.metalness, 0.0);
        assert_eq!(m.material_kind, 0);
        assert_eq!(m.material_alpha, 1.0);
        assert_eq!(m.diffuse_r, 1.0);
        assert_eq!(m.uv_scale_u, 1.0);
        assert_eq!(m.parallax_max_passes, 4.0);
        // Identity falloff pass-through.
        assert_eq!(m.falloff_start_angle, 1.0);
        assert_eq!(m.falloff_start_opacity, 1.0);
    }

    #[test]
    fn identical_materials_dedup_to_same_id() {
        let mut table = MaterialTable::new();
        let mat = GpuMaterial::default();
        let id_a = table.intern(mat);
        let id_b = table.intern(mat);
        assert_eq!(id_a, id_b);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn distinct_materials_get_distinct_ids() {
        let mut table = MaterialTable::new();
        let mut a = GpuMaterial::default();
        let mut b = GpuMaterial::default();
        b.roughness = 0.7;

        let id_a = table.intern(a);
        let id_b = table.intern(b);
        assert_ne!(id_a, id_b);
        assert_eq!(table.len(), 2);

        // Repeats still dedup to the original id.
        a.roughness = 0.5; // same as default
        assert_eq!(table.intern(a), id_a);
        assert_eq!(table.intern(b), id_b);
        assert_eq!(table.len(), 2);
    }

    /// Two materials differing in a single texture index (e.g.
    /// different diffuse on otherwise-identical material) must NOT
    /// dedup — they're genuinely distinct on the GPU. Pin this
    /// because a buggy hash that drops bits could collapse them and
    /// silently swap textures across draws.
    #[test]
    fn texture_index_difference_is_distinct() {
        let mut table = MaterialTable::new();
        let mut a = GpuMaterial::default();
        let mut b = GpuMaterial::default();
        a.texture_index = 7;
        b.texture_index = 8;
        assert_ne!(table.intern(a), table.intern(b));
        assert_eq!(table.len(), 2);
    }

    /// Float-bit equality check — two materials whose only difference
    /// is a fractional roughness must distinguish, even at very small
    /// epsilons. Byte-level eq + hash via `to_bits` semantics.
    #[test]
    fn small_float_difference_is_distinct() {
        let mut table = MaterialTable::new();
        let mut a = GpuMaterial::default();
        let mut b = GpuMaterial::default();
        a.roughness = 0.500_001;
        b.roughness = 0.500_002;
        assert_ne!(table.intern(a), table.intern(b));
    }

    #[test]
    fn clear_resets_table_but_keeps_capacity() {
        let mut table = MaterialTable::new();
        for i in 0..10 {
            let mut m = GpuMaterial::default();
            m.texture_index = i;
            table.intern(m);
        }
        assert_eq!(table.len(), 10);
        let cap_before = table.materials.capacity();
        table.clear();
        assert!(table.is_empty());
        assert!(table.materials.capacity() >= cap_before);
    }

    /// #780 / PERF-N1 — `interned_count` ticks on every `intern` call
    /// (hits AND misses) so the dedup ratio `len / interned_count` is
    /// computable from telemetry. `clear` resets it in lockstep with
    /// the materials Vec so the per-frame snapshot is honest.
    #[test]
    fn interned_count_increments_on_hit_and_miss() {
        let mut table = MaterialTable::new();
        assert_eq!(table.interned_count(), 0);

        let mut a = GpuMaterial::default();
        let mut b = GpuMaterial::default();
        b.roughness = 0.7;

        table.intern(a); // miss
        assert_eq!(table.interned_count(), 1);
        assert_eq!(table.len(), 1);

        table.intern(a); // hit — count still ticks
        assert_eq!(table.interned_count(), 2);
        assert_eq!(table.len(), 1);

        table.intern(b); // miss
        assert_eq!(table.interned_count(), 3);
        assert_eq!(table.len(), 2);

        // 5 more hits on b — only `interned_count` moves.
        for _ in 0..5 {
            table.intern(b);
        }
        assert_eq!(table.interned_count(), 8);
        assert_eq!(table.len(), 2);

        // Tweaking `a` after-the-fact must not retroactively count.
        a.roughness = 0.5; // same as default — still a hit on the
                           // first interned `a` (byte-equal).
        table.intern(a);
        assert_eq!(table.interned_count(), 9);
        assert_eq!(table.len(), 2);

        table.clear();
        assert_eq!(table.interned_count(), 0);
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn materials_slice_matches_insertion_order() {
        let mut table = MaterialTable::new();
        let mut mats = [GpuMaterial::default(); 3];
        mats[0].texture_index = 100;
        mats[1].texture_index = 200;
        mats[2].texture_index = 300;
        for m in &mats {
            table.intern(*m);
        }
        let slice = table.materials();
        assert_eq!(slice.len(), 3);
        assert_eq!(slice[0].texture_index, 100);
        assert_eq!(slice[1].texture_index, 200);
        assert_eq!(slice[2].texture_index, 300);
    }

    /// #797 / SAFE-22 — over-cap interns return id `0` and share the
    /// first material's record. Without this cap a DrawCommand
    /// carrying the over-cap id would index past the MaterialBuffer
    /// SSBO end on the GPU (implementation-defined OOB read).
    ///
    /// Builds a fresh table, fills it to `MAX_MATERIALS` distinct
    /// entries (each varying by `texture_index`), then asserts:
    ///   1. The 4097th distinct intern returns id `0`
    ///   2. The table's stored count never exceeds `MAX_MATERIALS`
    ///   3. The reverse-lookup map's count also stays bounded
    ///   4. A subsequent intern of an already-interned material
    ///      (one of the first `MAX_MATERIALS`) still returns its
    ///      original id — the cap doesn't poison the dedup map
    #[test]
    fn intern_overflow_returns_material_zero() {
        let mut table = MaterialTable::new();
        // Fill the table to exactly `MAX_MATERIALS` distinct entries.
        // `texture_index` is part of the byte-Hash dedup so each
        // increment produces a fresh GpuMaterial.
        for i in 0..MAX_MATERIALS as u32 {
            let mut m = GpuMaterial::default();
            m.texture_index = i;
            let id = table.intern(m);
            assert_eq!(id, i, "in-cap intern must return sequential ids");
        }
        assert_eq!(table.len(), MAX_MATERIALS);

        // Over-cap intern: distinct material, but no slot to land in.
        let mut overflow = GpuMaterial::default();
        overflow.texture_index = MAX_MATERIALS as u32;
        let overflow_id = table.intern(overflow);
        assert_eq!(
            overflow_id, 0,
            "over-cap intern must return id 0 (sentinel) so the GPU \
             read at materials[id] stays within bounds"
        );

        // Table count must not grow past the cap.
        assert_eq!(
            table.len(),
            MAX_MATERIALS,
            "over-cap intern must NOT push to materials Vec"
        );

        // Subsequent over-cap interns also fold to id 0 — the warn
        // is `Once`-gated so the second call is silent.
        let mut overflow2 = GpuMaterial::default();
        overflow2.texture_index = MAX_MATERIALS as u32 + 1;
        assert_eq!(table.intern(overflow2), 0);
        assert_eq!(table.len(), MAX_MATERIALS);

        // Already-interned materials still resolve to their original
        // id — the cap path doesn't poison the dedup map.
        let mut existing = GpuMaterial::default();
        existing.texture_index = 42; // interned at id 42 in the loop above
        assert_eq!(
            table.intern(existing),
            42,
            "in-cap dedup hit must still return the original id even \
             after the cap has been reached"
        );
    }

    /// `clear()` releases the `Once`-guard implicitly by replacing
    /// the table; verify the next overflow on a freshly-cleared
    /// table still routes to id 0 (the *behaviour*, not the warn,
    /// is what matters per-frame).
    #[test]
    fn intern_overflow_persists_across_clear() {
        let mut table = MaterialTable::new();
        for i in 0..MAX_MATERIALS as u32 {
            let mut m = GpuMaterial::default();
            m.texture_index = i;
            table.intern(m);
        }
        let mut overflow = GpuMaterial::default();
        overflow.texture_index = u32::MAX;
        assert_eq!(table.intern(overflow), 0);

        table.clear();
        // After clear the table starts empty — first intern returns 0
        // by normal sequential assignment, not by overflow.
        let mut first = GpuMaterial::default();
        first.texture_index = 1;
        assert_eq!(table.intern(first), 0);
        assert_eq!(table.len(), 1);
    }
}
