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
//! ## Phase status (R1)
//!
//! Phase 1 (this module): types + dedup table only. Nothing reads
//! `material_id` yet; the legacy per-instance fields still ship the
//! ground truth. Phase 2 wires the table into `build_render_data`,
//! Phase 3 plumbs the SSBO + `material_id` field into `GpuInstance` +
//! shaders, Phases 4–6 migrate fields one slice at a time and finally
//! drop the redundant per-instance copies.

use std::collections::HashMap;

/// std430 GPU-side material record. 272 bytes per material (17 × vec4).
///
/// Mirrors the per-material fields of [`super::scene_buffer::GpuInstance`]
/// at the same offsets within each vec4 group — this keeps the Phase 4–5
/// shader-side migration mechanical (rename `instance.foo` to
/// `materials[instance.material_id].foo`, no layout shuffling).
///
/// **CRITICAL**: All fields are scalar (f32/u32). NEVER use `[f32; 3]` —
/// std430 aligns vec3 to 16 B, which would silently mismatch a tightly-
/// packed `#[repr(C)]` Rust struct. Pad explicitly with named pad fields
/// so the byte-level `Hash`/`Eq` impls below are deterministic.
///
/// **Shader Struct Sync**: matching `struct GpuMaterial` declarations
/// in the GLSL shaders MUST be added in lockstep when Phase 3 lands.
/// The `gpu_material_size_is_272_bytes` test below pins the layout
/// invariant.
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
    /// the 272 B std430 layout pinned by `gpu_material_size_is_272_bytes`.
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

    // ── avg_albedo RGB + skin_tint_a (vec4 #10) ────────────────────
    pub avg_albedo_r: f32, // offset 144
    pub avg_albedo_g: f32, // offset 148
    pub avg_albedo_b: f32, // offset 152
    pub skin_tint_a: f32,  // offset 156

    // ── skin_tint RGB + hair_tint_r (vec4 #11) ─────────────────────
    pub skin_tint_r: f32, // offset 160
    pub skin_tint_g: f32, // offset 164
    pub skin_tint_b: f32, // offset 168
    pub hair_tint_r: f32, // offset 172

    // ── hair_tint GB + multi_layer_envmap_strength + eye_left_x (vec4 #12)
    pub hair_tint_g: f32,                 // offset 176
    pub hair_tint_b: f32,                 // offset 180
    pub multi_layer_envmap_strength: f32, // offset 184
    pub eye_left_center_x: f32,           // offset 188

    // ── eye_left YZ + eye_cubemap_scale + eye_right_x (vec4 #13) ───
    pub eye_left_center_y: f32, // offset 192
    pub eye_left_center_z: f32, // offset 196
    pub eye_cubemap_scale: f32, // offset 200
    pub eye_right_center_x: f32, // offset 204

    // ── eye_right YZ + multi_layer_inner_thickness + refraction (vec4 #14)
    pub eye_right_center_y: f32,           // offset 208
    pub eye_right_center_z: f32,           // offset 212
    pub multi_layer_inner_thickness: f32,  // offset 216
    pub multi_layer_refraction_scale: f32, // offset 220

    // ── multi_layer_inner_scale UV + sparkle RG (vec4 #15) ─────────
    pub multi_layer_inner_scale_u: f32, // offset 224
    pub multi_layer_inner_scale_v: f32, // offset 228
    pub sparkle_r: f32,                 // offset 232
    pub sparkle_g: f32,                 // offset 236

    // ── sparkle_b + sparkle_intensity + falloff angles (vec4 #16) ──
    pub sparkle_b: f32,         // offset 240
    pub sparkle_intensity: f32, // offset 244
    pub falloff_start_angle: f32, // offset 248
    pub falloff_stop_angle: f32, // offset 252

    // ── falloff opacities + soft_falloff_depth + pad (vec4 #17) ────
    pub falloff_start_opacity: f32, // offset 256
    pub falloff_stop_opacity: f32,  // offset 260
    pub soft_falloff_depth: f32,    // offset 264
    pub _pad_falloff: f32,          // offset 268 → total 272
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
            // avg_albedo — mid-gray fallback for GI bounce.
            avg_albedo_r: 0.5,
            avg_albedo_g: 0.5,
            avg_albedo_b: 0.5,
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
        }
    }

    /// Reset for a new frame. Retains the underlying allocation so
    /// per-frame churn doesn't hit the heap.
    pub fn clear(&mut self) {
        self.materials.clear();
        self.index.clear();
    }

    /// Insert a material (or return the existing id if byte-equal to
    /// one already in the table). Returns the `material_id` the GPU
    /// will use to look it up.
    pub fn intern(&mut self, material: GpuMaterial) -> u32 {
        if let Some(&id) = self.index.get(&material) {
            return id;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the std430 layout. Any growth must be intentional and
    /// matched by the shader-side `struct GpuMaterial` declaration in
    /// lockstep — same contract as `GpuInstance`.
    #[test]
    fn gpu_material_size_is_272_bytes() {
        assert_eq!(std::mem::size_of::<GpuMaterial>(), 272);
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
}
