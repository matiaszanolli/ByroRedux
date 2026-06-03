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
use rustc_hash::FxHashMap;
use std::sync::Once;

/// First-frame overflow latch for [`MaterialTable::intern`]. Wired through
/// a `Once` so the warn fires exactly once per session; #797's regression
/// guard is the truthful pairing of the upload-side warn message
/// (`scene_buffer.rs:978`) with actual default-to-0 behaviour.
static INTERN_OVERFLOW_WARNED: Once = Once::new();

/// std430 GPU-side material record. **300 bytes** per material.
/// Size history: 272 B → 260 B (#804 R1-N4 dropped `avg_albedo_r/g/b`)
/// → 296 B (#1249 Disney sheen/subsurface) → 300 B (#1250 `anisotropic`).
/// Pinned by `gpu_material_size_is_300_bytes`.
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
/// by `gpu_material_size_is_300_bytes` and
/// `gpu_material_field_offsets_match_shader_contract` (added #806 to
/// catch within-vec4 reorderings the size pin alone would miss).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuMaterial {
    // ── PBR scalars (vec4 #1) ───────────────────────────────────────
    pub roughness: f32,     // offset 0
    pub metalness: f32,     // offset 4
    pub emissive_mult: f32, // offset 8
    /// Bitfield of material-level flags consumed by the fragment
    /// shader. Bit 0 (`MAT_FLAG_VERTEX_COLOR_EMISSIVE`): per-vertex
    /// `fragColor.rgb` drives self-illumination instead of modulating
    /// albedo — set when the source NIF declared
    /// `NiVertexColorProperty.vertex_mode = SOURCE_EMISSIVE`. Pre-#695
    /// this slot was an unused pad; routing the bit through here keeps
    /// the std430 layout pinned by `gpu_material_size_is_300_bytes`
    /// (300 B post-#1250 Disney lobe).
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
    /// #890 Stage 2c — bindless texture index of the BSEffectShaderProperty
    /// `greyscale_texture` (1D color LUT). `0` means "no LUT bound"
    /// (the renderer falls back to sampling the source texture as
    /// plain RGB, which produces visibly wrong output for content that
    /// authored `MAT_FLAG_EFFECT_PALETTE_COLOR` — every Skyrim+ flame,
    /// most magic spells, FO4 power-armor glow effects). When the
    /// material's `EFFECT_PALETTE_COLOR` or `EFFECT_PALETTE_ALPHA`
    /// bit is set and this index is non-zero, the fragment shader's
    /// `MATERIAL_KIND_EFFECT_SHADER` branch samples the LUT at
    /// `vec2(texColor.a, 0.5)` (1D-as-2D — Skyrim DXT5 fire atlases
    /// store the meaningful flame intensity in the alpha channel) and
    /// uses the LUT result as the visible colour. See
    /// `triangle.frag` (`MATERIAL_KIND_EFFECT_SHADER` branch) +
    /// `cell_loader/spawn.rs` (resolution site). Previously named
    /// `_pad_falloff` — repacked #890 Stage 2c.
    pub greyscale_lut_index: u32,   // offset 256

    // ── BGSM translucency parameter suite (vec4 #18; offsets 260-280) ─
    //
    // #1147 / FO4-D6-003 Phase 2b. The translucency fields are read by
    // `triangle.frag` only when `material_flags & BGSM_TRANSLUCENCY != 0`
    // (matches the SLSF1/SLSF2 gating pattern the EFFECT_PALETTE_COLOR
    // family uses). Stored as scalar f32s rather than a `vec3 + f32`
    // pair so the byte-Hash dedup contract holds (per
    // `feedback_audit_findings.md`: std430 vec3 alignment would
    // silently desync the raw-bytes hash).
    /// `BgsmFile.translucency_subsurface_color.r`. Authored RGB of the
    /// transmitted/scattered light beneath the surface. Multiplied by
    /// the per-fragment albedo when `BGSM_TRANSLUCENCY_MIX_ALBEDO` is
    /// set; used raw otherwise. Default zero — no contribution when
    /// the gating flag is unset.
    pub translucency_subsurface_r: f32, // offset 260
    pub translucency_subsurface_g: f32, // offset 264
    pub translucency_subsurface_b: f32, // offset 268
    /// `BgsmFile.translucency_transmissive_scale`. Intensity scalar
    /// for the back-side / wraparound transmission term. Higher =
    /// brighter rim glow on thin objects. BGSM-authored range is
    /// typically 0.5 – 4.0 on FO4 content.
    pub translucency_transmissive_scale: f32, // offset 272

    // ── BGSM translucency turbulence (vec4 #19; offsets 276-280) ─────
    /// `BgsmFile.translucency_turbulence`. Adds a noise-driven
    /// perturbation to the transmission term so SSS doesn't appear
    /// uniformly smooth on materials authored for variation
    /// (vegetation, frost-rimmed glass). 0.0 = no turbulence. The
    /// shader consumes it as a multiplier on a `sin(viewDotN * k)`
    /// term to keep cost trivial.
    pub translucency_turbulence: f32, // offset 276
    // ── PBR IOR (vec4 #20; offsets 280-292) ──────────────────────────
    /// Refractive index — per-material η that drives the Schlick F0
    /// derivation `F0 = ((1-η)/(1+η))²` instead of the legacy hardcoded
    /// `vec3(0.04)` dielectric default. Default 1.5 (soda-lime glass /
    /// generic dielectric — `F0 = 0.04` matches the pre-#1248 behaviour
    /// so legacy NIF content with no authored IOR renders unchanged).
    /// Other common values: water 1.33 (F0 ≈ 0.020), ice 1.31, polished
    /// stone 1.54, diamond 2.42 (F0 ≈ 0.172). FO4 BGSM v9+ and
    /// Starfield .mat materials author this explicitly; older NIF
    /// content inherits the default. See #1248.
    pub ior: f32, // offset 280
    // ── Disney diffuse lobe (vec4 #21; offsets 284-296) ──────────────
    /// Disney "subsurface" diffuse-lobe weight (0 = pure Burley
    /// diffuse, 1 = full Hanrahan-Krueger fake-SSS). Cheap
    /// approximation for waxy / marble / skin without a full BSSRDF.
    /// Distinct from `translucency_*` (BGSM v>=8 back-light
    /// transmission). Reference impl:
    /// knightcrawler25/GLSL-PathTracer `disney.glsl:67-87` (MIT).
    /// Default 0.0 keeps the pre-#1249 Lambert behaviour. See #1249.
    pub subsurface: f32, // offset 284
    /// Disney "sheen" Fresnel-weighted edge highlight strength
    /// (0 = no sheen, 1 = full velvet/silk). Layered on top of the
    /// diffuse lobe so fabric-class materials get their characteristic
    /// edge brightening. Default 0.0. See #1249.
    pub sheen: f32, // offset 288
    /// Disney "sheen tint" — interpolation factor between white sheen
    /// (0.0) and base-colour-tinted sheen (1.0). Standard Disney shape;
    /// `mix(vec3(1.0), albedo, sheenTint)` is the per-pixel sheen
    /// colour the lobe multiplies into. Default 0.0 → white sheen.
    /// See #1249.
    pub sheen_tint: f32, // offset 292
    /// Anisotropic GGX strength [0, 1] (#1250). Drives the standard
    /// Disney `aspect = sqrt(1 - anisotropic * 0.9)` split:
    /// `ax = roughness / aspect, ay = roughness * aspect`. Default
    /// 0.0 → isotropic (`ax = ay = roughness`); the anisotropic GGX
    /// helper degenerates exactly to the isotropic NDF in that case,
    /// preserving the pre-#1250 lobe shape for legacy NIF content.
    /// Hair / brushed metal / vinyl materials grow visible directional
    /// streak when authored values land. The `0.9` cap prevents
    /// complete needle degeneracy at anisotropic = 1. Reference:
    /// knightcrawler25/GLSL-PathTracer `pathtrace.glsl:100-102` (MIT).
    pub anisotropic: f32, // offset 296 → total 300
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
            greyscale_lut_index: 0,
            // #1147 Phase 2b — neutral defaults so legacy materials
            // (BGSM_TRANSLUCENCY flag unset, ~99% of FO3/FNV/Skyrim
            // content) read these as no-ops even if the shader-side
            // gating ever desyncs from the flag check.
            translucency_subsurface_r: 0.0,
            translucency_subsurface_g: 0.0,
            translucency_subsurface_b: 0.0,
            translucency_transmissive_scale: 0.0,
            translucency_turbulence: 0.0,
            // #1248 — generic dielectric default. F0 = ((1-1.5)/(1+1.5))²
            // ≈ 0.04 — reproduces the pre-#1248 hardcoded vec3(0.04)
            // shader behaviour so legacy NIF content with no authored
            // IOR renders byte-identical.
            ior: 1.5,
            // #1249 — Disney diffuse lobe defaults all zero: legacy
            // content (every NIF without MAT_FLAG_BGSM_PBR) routes
            // through the Lambert branch and never touches these
            // fields. Authored BGSM v>=8 materials can override.
            subsurface: 0.0,
            sheen: 0.0,
            sheen_tint: 0.0,
            // #1250 — isotropic default. ax = ay = roughness; the
            // anisotropic GGX helper produces the same lobe shape as
            // the legacy isotropic distributionGGX call.
            anisotropic: 0.0,
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

    // ── `BSEffectShaderProperty` flag bits (#890 Stage 2) ────────────
    //
    // Captured CPU-side by the four `is_*_from_modern_shader_flags`
    // helpers in `crates/nif/src/import/material/mod.rs` (which check
    // both the typed flag word AND the FO76/Starfield CRC32 list union).
    // Packed into `Material.effect_shader_flags` at the importer boundary
    // and OR'd into `GpuMaterial.material_flags` by
    // [`DrawCommand::to_gpu_material`] so the fragment shader's
    // `MATERIAL_KIND_EFFECT_SHADER` branch can branch on them.
    //
    // Bit positions must stay in lockstep with `triangle.frag` —
    // the GLSL refers to the same `0x...u` literals.

    /// `SLSF1::Soft_Effect` (nif.xml bit 30) — near-camera depth
    /// feathering for soft particles (smoke, dust, force-field haze).
    /// Stage 2a only plumbs the bit; the shader-side soft-depth fade
    /// awaits the depth-attachment-as-shader-resource wiring (#890
    /// Stage 2b — RenderDoc-required render-pass restructure).
    pub const EFFECT_SOFT: u32 = 1 << 1;
    /// `SLSF1::Greyscale_To_PaletteColor` (nif.xml bit 4) — sample the
    /// `greyscale_texture` as a colour palette LUT indexed by the
    /// source-texture luminance. The shader-side consumer (Stage 2c,
    /// #890) lives in `triangle.frag`'s `MATERIAL_KIND_EFFECT_SHADER`
    /// branch and samples `textures[greyscaleLutIndex]` at
    /// `vec2(source.r, 0.5)` when this flag is set and
    /// `greyscale_lut_index != 0`.
    pub const EFFECT_PALETTE_COLOR: u32 = 1 << 2;
    /// `SLSF1::Greyscale_To_PaletteAlpha` (nif.xml bit 5) — same
    /// `greyscale_texture` indexed for the alpha channel. Stage 2c
    /// shader consumer samples the LUT's alpha at the same UV.
    pub const EFFECT_PALETTE_ALPHA: u32 = 1 << 3;
    /// `SLSF2::Effect_Lighting` (nif.xml bit 30) — scene-lit
    /// `BSEffectShaderProperty` surface. The fragment shader's
    /// effect-shader branch modulates the pure-additive emit term by
    /// the cell ambient + directional sun, parallel to the lit-mesh
    /// path. Live in Stage 2a.
    pub const EFFECT_LIT: u32 = 1 << 4;

    // ── Material-feature flags (per `feedback_format_translation.md`) ─
    //
    // These bits gate optional material *features*, not source formats.
    // Originally landed under the `BGSM_*` prefix because the BGSM
    // translator was the only writer (#1077 / FO4-D6-003 Phase 2a),
    // but Stage 3 of the format-translation rollout renames them to
    // `MAT_*` so future translators (.mat / legacy NIF inline shaders /
    // procedural materials) can populate the same bits without the
    // shader pretending the underlying file format matters. The
    // BGSM-specific `MAT_AUTHORED_BY_BGSM` telemetry bit below records
    // provenance for debug inspection without ever reaching the shader.
    //
    // Captured CPU-side by `pack_bgsm_material_flags` in
    // `byroredux/src/cell_loader.rs` and OR'd into `effect_shader_flags`
    // at the importer boundary; forwarded unchanged to
    // `GpuMaterial.material_flags` by `DrawCommand::to_gpu_material`.

    /// Material uses the Disney BSDF path (metalness/roughness with
    /// Burley diffuse, GGX specular, optional sheen / subsurface /
    /// clearcoat). When clear, the fragment shader runs the
    /// legacy Lambert + simple-GGX path. Currently set by the BGSM
    /// translator on `BgsmFile.pbr == true`; future translators (.mat,
    /// Disney-authored legacy NIFs) can set it without re-introducing
    /// a BGSM branch.
    pub const PBR_BSDF: u32 = 1 << 5;
    /// Material has subsurface / translucency authoring (skin, foliage,
    /// thin glass, wax). Drives the SSS path; the parameter suite
    /// (`translucency_subsurface_color`, `translucency_transmissive_scale`,
    /// `translucency_turbulence`) is plumbed through `Material`.
    pub const TRANSLUCENCY: u32 = 1 << 6;
    /// Material's normal map is authored in object/model space rather
    /// than tangent space. When set, the fragment shader's normal
    /// decode skips the TBN transform and uses the sampled normal
    /// directly. Skyrim+ skin shaders set this; legacy meshes don't.
    pub const MODEL_SPACE_NORMALS: u32 = 1 << 7;
    /// Translucent surface is a thick volume (skin / muscle / wax)
    /// rather than a thin sheet (paper / leaf / cloth). Changes the
    /// SSS path's view-dependent transmission falloff. Only
    /// meaningful when [`TRANSLUCENCY`] is also set.
    pub const TRANSLUCENCY_THICK_OBJECT: u32 = 1 << 8;
    /// SSS transmitted colour mixes the per-fragment albedo into the
    /// authored `translucency_subsurface_color` rather than using the
    /// latter raw. Skyrim+ skin shaders set this; FO4 vegetation
    /// typically does not. Only meaningful when [`TRANSLUCENCY`] is
    /// also set.
    pub const TRANSLUCENCY_MIX_ALBEDO: u32 = 1 << 9;

    // ── Pre-Stage-3 aliases — kept so external callers compile
    // through the rename. New code should reference the un-prefixed
    // names above. Targeted for removal in a follow-up sweep after
    // every reader has migrated.
    pub const BGSM_PBR: u32 = PBR_BSDF;
    pub const BGSM_TRANSLUCENCY: u32 = TRANSLUCENCY;
    pub const BGSM_MODEL_SPACE_NORMALS: u32 = MODEL_SPACE_NORMALS;
    pub const BGSM_TRANSLUCENCY_THICK_OBJECT: u32 = TRANSLUCENCY_THICK_OBJECT;
    pub const BGSM_TRANSLUCENCY_MIX_ALBEDO: u32 = TRANSLUCENCY_MIX_ALBEDO;

    /// Bit-shift for the 8-bit `BSEffectShaderProperty.lighting_influence`
    /// byte packed into `material_flags` bits 16–23. Extract in GLSL as
    /// `float((materialFlags >> MAT_FLAG_EFFECT_LI_SHIFT) & 0xFFu) / 255.0`.
    /// Bits 11–15 are reserved for future single-bit flags; bits 24–31
    /// are free. Only meaningful when `EFFECT_LIT` is also set — the byte
    /// scales the directional + ambient contribution in the
    /// `MATERIAL_KIND_EFFECT_SHADER` lit branch. `0` (packed value of
    /// `lighting_influence == 0`) disables scene lighting; `255` is full
    /// contribution. See #890 Stage 2 and `pack_effect_shader_flags`.
    pub const EFFECT_LI_SHIFT: u32 = 16;

    /// "This material came from a BGSM (or BGEM) external file" — set
    /// on every BGSM-sourced surface regardless of the `BgsmFile.pbr`
    /// flag. Disambiguates Bethesda's two shading conventions:
    ///   * BGSM-authored (Skyrim SE / FO4 / FO76) → spec-glossiness:
    ///     `F0 = specular_color * specular_mult` directly, no metalness
    ///     mix. Vanilla content uniformly authors per-material spec_color
    ///     (e.g. steel ≈ 0.95/0.93/0.88) with smoothness driving the
    ///     visible-lobe width (metal = high smoothness → sharp
    ///     reflection; cloth = low smoothness → soft sheen).
    ///   * Legacy NIF (Oblivion / FO3 / FNV) → metallic-roughness with
    ///     keyword-classified metalness; spec_color is a Phong-era
    ///     tint scalar applied AFTER the BRDF, not F0 itself.
    ///
    /// Set when `merge_bgsm_into_mesh` resolves a `.bgsm` or `.bgem`
    /// material file successfully (i.e. real authored data merged into
    /// `ImportedMesh`). NOT set when the mesh has an inline
    /// `BSLightingShaderProperty` only — Skyrim BSLighting without an
    /// external BGSM uses the same convention but doesn't currently
    /// surface a distinguishing signal end-to-end, so it continues
    /// on the legacy path (matches pre-fix behaviour, no regression).
    ///
    /// Distinct from [`BGSM_PBR`] which gates the Disney BSDF /
    /// translucency suite — `bgsm.pbr=true` is virtually never authored
    /// in vanilla content (sampled across 793 metal/crate/cargo
    /// vanilla FO4 BGSMs in `Fallout4 - Materials.ba2`: 0 with
    /// pbr=true), so `BGSM_PBR` alone is a dead gate. `BGSM_AUTHORED`
    /// is the load-bearing flag for the spec-glossiness routing.
    pub const BGSM_AUTHORED: u32 = 1 << 10;
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

/// Documented Disney-BSDF material presets sourced from
/// `knightcrawler25/GLSL-PathTracer` (MIT) —
/// `assets/hyperion_rect_lights.scene`.
///
/// Use as the fallback when authored BGSM is absent (or as
/// known-good test fixtures). Pre-#1251 the synthetic-material
/// path picked `roughness=0.5, metallic=0.0, F0=0.04` from thin
/// air — the `feedback_no_guessing` memory wants citable
/// references for every physical constant, and the Hyperion
/// preset table is the canonical Disney-BSDF reference.
///
/// Each preset returns a fully-populated `GpuMaterial`. The base
/// `Default::default()` values cover every field this audit's
/// table doesn't override (texture handles, alpha, UV transform,
/// effect-shader payloads, etc.) — so e.g. `polished_metal()`
/// keeps every default neutral and only sets the PBR scalars +
/// IOR that the reference table specifies.
///
/// **Field-shape pin** (#1251): each preset enumerates the audit's
/// authoritative `roughness / metallic / IOR / [optional Disney
/// extension]` fields explicitly. If a future GpuMaterial growth
/// changes the default of any field the presets touch, the
/// per-preset tests pin the expected output.
pub mod presets {
    use super::GpuMaterial;

    /// Silver-class polished metal. Hyperion table:
    /// `color = (0.9, 0.9, 0.9)`, `roughness = 0.001`,
    /// `metallic = 1.0`. Tightest highlight lobe → mirror-like
    /// micro-roughness suitable for chrome / silver / polished
    /// steel hero props.
    pub fn polished_metal() -> GpuMaterial {
        GpuMaterial {
            roughness: 0.001,
            metalness: 1.0,
            diffuse_r: 0.9,
            diffuse_g: 0.9,
            diffuse_b: 0.9,
            // Metals: F0 comes from the per-channel albedo via the
            // shader's `mix(dielectricF0, albedo, metalness)` branch.
            // IOR default 1.5 is irrelevant for metals (the mix lands
            // ~100% albedo at metalness = 1).
            ..Default::default()
        }
    }

    /// Generic glass — η = 1.45 (soda-lime / window glass authored
    /// at the Hyperion-table edge of the typical glass range).
    /// `roughness = 0.0` (perfectly smooth surface), `metallic = 0`.
    /// Transmission `spec_trans = 1.0` is a Disney-BSDF extension
    /// not yet plumbed into our GpuMaterial — left as a TODO for
    /// when the transmission lobe lands (#1248-followup).
    pub fn glass() -> GpuMaterial {
        GpuMaterial {
            roughness: 0.0,
            metalness: 0.0,
            ior: 1.45,
            diffuse_r: 1.0,
            diffuse_g: 1.0,
            diffuse_b: 1.0,
            ..Default::default()
        }
    }

    /// Two-coat car paint. Hyperion table: base = green tint
    /// `(0.026, 0.147, 0.075)`, `roughness = 0.01`. Clearcoat
    /// (`clearcoat = 1.0, clearcoat_gloss = 1.0` in Disney
    /// 2012) is a Disney extension not yet on our GpuMaterial —
    /// when it lands, this preset should set those alongside.
    ///
    /// `base` argument lets the caller override the green default
    /// (Hyperion ships green as the demo colour; real car-paint
    /// authoring picks per-vehicle).
    pub fn car_paint(base: [f32; 3]) -> GpuMaterial {
        GpuMaterial {
            roughness: 0.01,
            metalness: 0.0,
            diffuse_r: base[0],
            diffuse_g: base[1],
            diffuse_b: base[2],
            ..Default::default()
        }
    }

    /// Lacquered plastic (Hyperion: orange `(1.0, 0.186, 0.0)`).
    /// Same `roughness = 0.001` mirror-class clearcoat as car
    /// paint, but `metallic = 0` — the smooth-plastic look (toy
    /// cars, painted machinery, glossy lacquered wood).
    pub fn lacquered_plastic(base: [f32; 3]) -> GpuMaterial {
        GpuMaterial {
            roughness: 0.001,
            metalness: 0.0,
            diffuse_r: base[0],
            diffuse_g: base[1],
            diffuse_b: base[2],
            ..Default::default()
        }
    }

    /// Painted matte (Hyperion red base, mild metal sheen):
    /// `roughness = 0.5`, `metallic = 0.2`. Half-rough surface
    /// with a sub-pure-dielectric Fresnel — covers worn paint,
    /// powder-coat finishes, anodised metals that are neither
    /// flat-matte nor mirror.
    pub fn painted_matte(base: [f32; 3]) -> GpuMaterial {
        GpuMaterial {
            roughness: 0.5,
            metalness: 0.2,
            diffuse_r: base[0],
            diffuse_g: base[1],
            diffuse_b: base[2],
            ..Default::default()
        }
    }

    /// Skin / wax / marble — the Hanrahan-Krueger fake-SSS preset
    /// (#1249). Hyperion: `color = (0.93, 0.89, 0.85)` (warm
    /// flesh / wax tone), `roughness = 1.0` (Disney's "fully
    /// rough"), `metallic = 0.0`, `subsurface = 1.0` (full
    /// HK fake-SSS contribution).
    pub fn skin_wax_marble(base: [f32; 3]) -> GpuMaterial {
        GpuMaterial {
            roughness: 1.0,
            metalness: 0.0,
            subsurface: 1.0,
            diffuse_r: base[0],
            diffuse_g: base[1],
            diffuse_b: base[2],
            // PBR flag so the Disney-diffuse branch fires at the
            // shader (#1249 gates on `MAT_FLAG_BGSM_PBR`).
            material_flags: super::material_flag::BGSM_PBR,
            ..Default::default()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn polished_metal_matches_hyperion_table() {
            let m = polished_metal();
            assert_eq!(m.roughness, 0.001);
            assert_eq!(m.metalness, 1.0);
            assert_eq!(m.diffuse_r, 0.9);
            assert_eq!(m.diffuse_g, 0.9);
            assert_eq!(m.diffuse_b, 0.9);
        }

        #[test]
        fn glass_matches_hyperion_table() {
            let m = glass();
            assert_eq!(m.roughness, 0.0);
            assert_eq!(m.metalness, 0.0);
            assert_eq!(m.ior, 1.45);
        }

        #[test]
        fn car_paint_matches_hyperion_table() {
            let m = car_paint([0.026, 0.147, 0.075]);
            assert_eq!(m.roughness, 0.01);
            assert_eq!(m.metalness, 0.0);
            assert_eq!(m.diffuse_r, 0.026);
            assert_eq!(m.diffuse_g, 0.147);
            assert_eq!(m.diffuse_b, 0.075);
        }

        #[test]
        fn lacquered_plastic_matches_hyperion_table() {
            let m = lacquered_plastic([1.0, 0.186, 0.0]);
            assert_eq!(m.roughness, 0.001);
            assert_eq!(m.metalness, 0.0);
            assert_eq!(m.diffuse_r, 1.0);
            assert_eq!(m.diffuse_g, 0.186);
            assert_eq!(m.diffuse_b, 0.0);
        }

        #[test]
        fn painted_matte_matches_hyperion_table() {
            let m = painted_matte([1.0, 0.0, 0.0]);
            assert_eq!(m.roughness, 0.5);
            assert_eq!(m.metalness, 0.2);
        }

        #[test]
        fn skin_wax_marble_matches_hyperion_table_and_fires_pbr_gate() {
            let m = skin_wax_marble([0.93, 0.89, 0.85]);
            assert_eq!(m.roughness, 1.0);
            assert_eq!(m.metalness, 0.0);
            assert_eq!(m.subsurface, 1.0);
            assert_eq!(m.diffuse_r, 0.93);
            // BGSM_PBR flag must be set so the shader's Disney-
            // diffuse branch consumes `subsurface`.
            assert_ne!(
                m.material_flags & super::super::material_flag::BGSM_PBR,
                0,
                "skin_wax_marble must set BGSM_PBR so the shader's \
                 disneyDiffuseTerm branch fires and consumes subsurface = 1.0"
            );
        }

        /// Every preset must inherit the GpuMaterial::default() shape
        /// for fields it doesn't override. Future field additions that
        /// drift a preset's default away from the audit's Hyperion
        /// table values would fail this pin.
        #[test]
        fn presets_inherit_defaults_for_unset_fields() {
            let m = polished_metal();
            // Default `ior = 1.5` is left untouched by polished_metal
            // (metals don't care about dielectric IOR per the docstring).
            assert_eq!(m.ior, 1.5);
            // Default `anisotropic = 0` — polished metal stays isotropic
            // (brushed metal would override).
            assert_eq!(m.anisotropic, 0.0);
            // Default `sheen = 0` — metal doesn't get fabric edge brighten.
            assert_eq!(m.sheen, 0.0);
        }
    }
}

/// Canonical material hash — FxHash (#1368) over the 50 live scalar
/// fields of [`GpuMaterial`] in declaration order. Used by
/// [`MaterialTable::intern_by_hash`] to dedup without hashing the full
/// 300-byte struct.
///
/// **Lockstep contract** (#781 / PERF-N4): [`DrawCommand::material_hash`]
/// walks the same field sequence, in the same order, against the
/// `DrawCommand` source fields that `to_gpu_material` reads. Drift
/// between the two walks is caught by
/// `material_hash_matches_gpu_material_field_hash` in
/// `vulkan::context::draw_command_tests`. Any new GpuMaterial field
/// MUST be added to BOTH walks (and to the contract test).
///
/// `greyscale_lut_index` (offset 256, #890 Stage 2c) is hashed at the
/// end of the walk — distinct LUT handles must produce distinct hashes
/// so palette-colour effect materials don't dedup across LUTs.
pub(super) fn hash_gpu_material_fields(mat: &GpuMaterial) -> u64 {
    use std::hash::Hasher;
    let mut h = rustc_hash::FxHasher::default();
    // PBR scalars + flags
    h.write_u32(mat.roughness.to_bits());
    h.write_u32(mat.metalness.to_bits());
    h.write_u32(mat.emissive_mult.to_bits());
    h.write_u32(mat.material_flags);
    // Emissive RGB + specular_strength
    h.write_u32(mat.emissive_r.to_bits());
    h.write_u32(mat.emissive_g.to_bits());
    h.write_u32(mat.emissive_b.to_bits());
    h.write_u32(mat.specular_strength.to_bits());
    // Specular RGB + alpha_threshold
    h.write_u32(mat.specular_r.to_bits());
    h.write_u32(mat.specular_g.to_bits());
    h.write_u32(mat.specular_b.to_bits());
    h.write_u32(mat.alpha_threshold.to_bits());
    // Texture indices group A
    h.write_u32(mat.texture_index);
    h.write_u32(mat.normal_map_index);
    h.write_u32(mat.dark_map_index);
    h.write_u32(mat.glow_map_index);
    // Texture indices group B
    h.write_u32(mat.detail_map_index);
    h.write_u32(mat.gloss_map_index);
    h.write_u32(mat.parallax_map_index);
    h.write_u32(mat.env_map_index);
    // env_mask + alpha_test_func + material_kind + material_alpha
    h.write_u32(mat.env_mask_index);
    h.write_u32(mat.alpha_test_func);
    h.write_u32(mat.material_kind);
    h.write_u32(mat.material_alpha.to_bits());
    // Parallax POM + UV offset
    h.write_u32(mat.parallax_height_scale.to_bits());
    h.write_u32(mat.parallax_max_passes.to_bits());
    h.write_u32(mat.uv_offset_u.to_bits());
    h.write_u32(mat.uv_offset_v.to_bits());
    // UV scale + diffuse RG
    h.write_u32(mat.uv_scale_u.to_bits());
    h.write_u32(mat.uv_scale_v.to_bits());
    h.write_u32(mat.diffuse_r.to_bits());
    h.write_u32(mat.diffuse_g.to_bits());
    // diffuse_b + ambient RGB
    h.write_u32(mat.diffuse_b.to_bits());
    h.write_u32(mat.ambient_r.to_bits());
    h.write_u32(mat.ambient_g.to_bits());
    h.write_u32(mat.ambient_b.to_bits());
    // Skyrim+ skin tint A/R/G/B
    h.write_u32(mat.skin_tint_a.to_bits());
    h.write_u32(mat.skin_tint_r.to_bits());
    h.write_u32(mat.skin_tint_g.to_bits());
    h.write_u32(mat.skin_tint_b.to_bits());
    // hair tint RGB + multi_layer_envmap_strength
    h.write_u32(mat.hair_tint_r.to_bits());
    h.write_u32(mat.hair_tint_g.to_bits());
    h.write_u32(mat.hair_tint_b.to_bits());
    h.write_u32(mat.multi_layer_envmap_strength.to_bits());
    // Eye left + eye_cubemap_scale
    h.write_u32(mat.eye_left_center_x.to_bits());
    h.write_u32(mat.eye_left_center_y.to_bits());
    h.write_u32(mat.eye_left_center_z.to_bits());
    h.write_u32(mat.eye_cubemap_scale.to_bits());
    // Eye right + multi_layer_inner_thickness
    h.write_u32(mat.eye_right_center_x.to_bits());
    h.write_u32(mat.eye_right_center_y.to_bits());
    h.write_u32(mat.eye_right_center_z.to_bits());
    h.write_u32(mat.multi_layer_inner_thickness.to_bits());
    // refraction + multi_layer_inner_scale UV + sparkle_r
    h.write_u32(mat.multi_layer_refraction_scale.to_bits());
    h.write_u32(mat.multi_layer_inner_scale_u.to_bits());
    h.write_u32(mat.multi_layer_inner_scale_v.to_bits());
    h.write_u32(mat.sparkle_r.to_bits());
    // sparkle GB + sparkle_intensity + falloff_start_angle
    h.write_u32(mat.sparkle_g.to_bits());
    h.write_u32(mat.sparkle_b.to_bits());
    h.write_u32(mat.sparkle_intensity.to_bits());
    h.write_u32(mat.falloff_start_angle.to_bits());
    // falloff_stop + opacities + soft_falloff_depth
    h.write_u32(mat.falloff_stop_angle.to_bits());
    h.write_u32(mat.falloff_start_opacity.to_bits());
    h.write_u32(mat.falloff_stop_opacity.to_bits());
    h.write_u32(mat.soft_falloff_depth.to_bits());
    // greyscale LUT bindless handle (#890 Stage 2c, offset 256)
    h.write_u32(mat.greyscale_lut_index);
    // #1147 Phase 2b — BGSM v>=8 translucency suite (offsets 260-280).
    // Must match `DrawCommand::material_hash` walk order so the two
    // hashes stay byte-equal-safe (pinned by
    // `material_hash_matches_gpu_material_field_hash`).
    h.write_u32(mat.translucency_subsurface_r.to_bits());
    h.write_u32(mat.translucency_subsurface_g.to_bits());
    h.write_u32(mat.translucency_subsurface_b.to_bits());
    h.write_u32(mat.translucency_transmissive_scale.to_bits());
    h.write_u32(mat.translucency_turbulence.to_bits());
    // #1248 — per-material refractive index (offset 280). Must mirror
    // the matching trailing write in `DrawCommand::material_hash` so
    // the byte-equal-safe contract pinned by
    // `material_hash_matches_gpu_material_field_hash` holds.
    h.write_u32(mat.ior.to_bits());
    // #1249 — Disney diffuse lobe (offsets 284-292). Same lockstep
    // requirement as ior above.
    h.write_u32(mat.subsurface.to_bits());
    h.write_u32(mat.sheen.to_bits());
    h.write_u32(mat.sheen_tint.to_bits());
    // #1250 — anisotropic GGX strength (offset 296). Same lockstep.
    h.write_u32(mat.anisotropic.to_bits());
    h.finish()
}

/// Per-frame deduplicated material table. Cleared at frame start, populated
/// during `build_render_data`, uploaded as an SSBO before draw.
///
/// Identical materials (byte-equal `GpuMaterial`) collapse to the same id;
/// distinct materials get fresh ids in insertion order. The reverse map
/// (`FxHashMap<u64, u32>` keyed on [`hash_gpu_material_fields`]) keeps
/// `intern` O(1) amortised. Pre-#781 the index keyed on `GpuMaterial`
/// itself, requiring a 300-byte byte-hash on every lookup AND forcing
/// the caller to construct the full `GpuMaterial` even on dedup hits.
/// The fast path now goes through [`Self::intern_by_hash`], which takes
/// a precomputed u64 + a closure that produces the `GpuMaterial` only
/// on miss.
pub struct MaterialTable {
    /// Insertion-ordered material storage, indexed by `material_id`.
    materials: Vec<GpuMaterial>,
    /// Reverse lookup for dedup, keyed on
    /// [`hash_gpu_material_fields`]'s u64 output. Cleared in lockstep
    /// with `materials`. See #781 / PERF-N4.
    index: FxHashMap<u64, u32>,
    /// R1 telemetry — total `intern()` calls this frame (one per
    /// `DrawCommand`). Read alongside `len()` to compute the dedup
    /// ratio and surfaced via `ctx.scratch`. The counter exists so a
    /// regression that breaks byte-equality dedup (alignment hole,
    /// non-deterministic float in the producer) shows up as a
    /// dropping ratio in telemetry rather than silently inflating
    /// VRAM. See #780 / PERF-N1.
    interned_count: usize,
    /// Per-frame count of `intern_by_hash` calls that hit the
    /// [`MAX_MATERIALS`] cap and were routed to the neutral default
    /// (id `0`) instead of getting a fresh slot. Reset at frame
    /// start by [`Self::clear`]. Surfaced through
    /// [`Self::overflow_count`] so the `mem` console command can
    /// report "how badly is the cap blown" — a single `Once`-gated
    /// warning loses count visibility (which #797 / SAFE-22's
    /// cap-and-warn intentionally accepted for cheapness; the
    /// counter restores it without the per-overflow log spam).
    /// Non-zero means raising [`MAX_MATERIALS`] is appropriate.
    overflow_count: usize,
    /// Debug-only counter: number of times `intern_by_hash` detected a
    /// true hash collision (two distinct `GpuMaterial` values producing
    /// the same FxHash u64). Only reachable in debug builds where the
    /// byte-equality check fires; the corresponding `debug_assert!` will
    /// panic first in practice, but the counter is wired so that future
    /// soft-warning modes can surface the collision count via telemetry
    /// (e.g. the `mem` console command). Zero overhead in release.
    /// See #1414.
    #[cfg(debug_assertions)]
    collision_count: usize,
}

impl Default for MaterialTable {
    fn default() -> Self {
        Self::new()
    }
}

impl MaterialTable {
    pub fn new() -> Self {
        let mut t = Self {
            materials: Vec::new(),
            index: FxHashMap::default(),
            interned_count: 0,
            overflow_count: 0,
            #[cfg(debug_assertions)]
            collision_count: 0,
        };
        t.seed_neutral_default();
        t
    }

    /// Reset for a new frame. Retains the underlying allocation so
    /// per-frame churn doesn't hit the heap. Re-seeds slot 0 with the
    /// neutral-lit default so `material_id == 0` always resolves to
    /// a safe-to-read GpuMaterial — see [`Self::seed_neutral_default`].
    pub fn clear(&mut self) {
        self.materials.clear();
        self.index.clear();
        self.interned_count = 0;
        self.overflow_count = 0;
        #[cfg(debug_assertions)]
        {
            self.collision_count = 0;
        }
        self.seed_neutral_default();
    }

    /// #807 — pre-push `GpuMaterial::default()` into slot 0 so the id
    /// is reserved as the "neutral default" rather than being
    /// overloaded with three distinct meanings (default-init UI quad,
    /// first-interned user material, over-cap fallback). Subsequent
    /// `intern` calls of a byte-equal default dedup to slot 0 instead
    /// of pushing again; user-interned distinct materials start at
    /// id 1. Over-cap interns still return 0, which now legitimately
    /// resolves to the neutral material rather than aliasing whatever
    /// happened to be interned first.
    ///
    /// `interned_count` is NOT bumped — the seed is internal accounting,
    /// not a producer-driven intern call. The `len / interned_count`
    /// dedup ratio in telemetry stays comparable to pre-#807 frames
    /// when at least one user material is interned (one extra slot in
    /// the numerator on no-user-material frames; trivial for the
    /// dedup-quality signal #780 / PERF-N1 watches for).
    fn seed_neutral_default(&mut self) {
        let neutral = GpuMaterial::default();
        let hash = hash_gpu_material_fields(&neutral);
        self.materials.push(neutral);
        self.index.insert(hash, 0);
    }

    // NOTE on first-frame upload (REN-D14-NEW-04, INFO, audit
    // 2026-05-09): `new()` calls `seed_neutral_default` immediately;
    // the first frame after construction then re-runs `clear()` →
    // `seed_neutral_default` AND uploads the (identical) neutral
    // entry. That re-upload is one std430-aligned `GpuMaterial`
    // (300 B) of redundant host→device traffic per first frame
    // and is not visible in steady-state telemetry. Documented
    // here rather than skipped because the alternative (suppress
    // first-frame clear) gates the seed on a `dirty` flag, which
    // is more state-machine bookkeeping than the one trivial
    // upload costs.

    /// Insert a material (or return the existing id if byte-equal to
    /// one already in the table). Returns the `material_id` the GPU
    /// will use to look it up.
    ///
    /// Slot 0 is reserved for the neutral-lit `GpuMaterial::default()`
    /// (see [`Self::seed_neutral_default`] / #807); user-interned
    /// distinct materials start at id 1 and grow up to (but not past)
    /// [`MAX_MATERIALS`].
    ///
    /// Capped at [`MAX_MATERIALS`] entries — over-cap interns return
    /// id `0` and share the neutral-default material's record for the
    /// rest of the frame. See #797 / SAFE-22: the upload at
    /// `scene_buffer.rs:975` truncates the buffer to `MAX_MATERIALS`
    /// entries, so without this cap a `DrawCommand` carrying an over-
    /// cap `material_id` would index past the SSBO end on the GPU
    /// (implementation-defined OOB read; AMD returns zeros, NVIDIA
    /// returns last-valid-page contents, Intel may DEVICE_LOST).
    /// The over-cap → neutral mapping is now semantically clean —
    /// pre-#807 it aliased "the first user-interned material this
    /// frame," which was an overload.
    ///
    /// Real interior cells dedup to 50–200 unique materials and a
    /// 3×3 exterior grid lands at 300–600 — well under the 4096 cap
    /// (`scene_buffer.rs:60-62`). The overflow path is reachable
    /// today only on modded / synthetic / future Starfield-FO76
    /// large-exterior content.
    pub fn intern(&mut self, material: GpuMaterial) -> u32 {
        let hash = hash_gpu_material_fields(&material);
        self.intern_by_hash(hash, || material)
    }

    /// Hot-path intern entry: take a precomputed u64 hash + a closure
    /// that produces the [`GpuMaterial`] only on dedup miss. The
    /// closure is NOT invoked when the hash already maps to a stored
    /// material — `to_gpu_material` (the dominant 300-byte construction
    /// cost) is skipped on the ~97% dedup-hit path. See #781 / PERF-N4.
    ///
    /// **Hash quality contract**: callers must produce a u64 that is a
    /// pure function of the same fields [`hash_gpu_material_fields`]
    /// reads, in the same order. The lockstep is pinned by
    /// `vulkan::context::draw_command_tests::material_hash_matches_gpu_material_field_hash`
    /// for [`DrawCommand::material_hash`]; any other producer must
    /// uphold the same invariant or risk silent miscoloring.
    ///
    /// **Collision policy**: in debug builds we construct the
    /// `GpuMaterial` even on hits and assert it byte-equals the stored
    /// one — a hash collision (or a drift between the producer hash
    /// and `hash_gpu_material_fields`) fires a panic with the colliding
    /// hash in the message. In release we trust the hash; collisions
    /// (rare on FxHash's 64-bit output over 50 scalar fields, #1368)
    /// would silently alias to the first-seen material at that hash.
    pub fn intern_by_hash(
        &mut self,
        hash: u64,
        material_factory: impl FnOnce() -> GpuMaterial,
    ) -> u32 {
        self.interned_count += 1;
        if let Some(&id) = self.index.get(&hash) {
            #[cfg(debug_assertions)]
            {
                let mat = material_factory();
                if self.materials[id as usize] != mat {
                    self.collision_count += 1;
                }
                debug_assert!(
                    self.materials[id as usize] == mat,
                    "MaterialTable hash collision: hash {:#018x} maps to two distinct \
                     GpuMaterial values (this is either a hasher quality issue or — \
                     more likely — drift between the producer hash and \
                     `hash_gpu_material_fields`).",
                    hash,
                );
            }
            return id;
        }
        if self.materials.len() >= MAX_MATERIALS {
            self.overflow_count += 1;
            INTERN_OVERFLOW_WARNED.call_once(|| {
                log::warn!(
                    "MaterialTable: unique-material count exceeded MAX_MATERIALS \
                     ({}); over-cap entries share the neutral-default material 0 \
                     for the rest of the session. See #797 / SAFE-22 + #807. \
                     Per-frame overflow count via `mem` command.",
                    MAX_MATERIALS,
                );
            });
            return 0;
        }
        let mat = material_factory();
        let id = self.materials.len() as u32;
        self.materials.push(mat);
        self.index.insert(hash, id);
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

    /// Number of intern calls this frame that were routed to id `0`
    /// (the neutral-default fallback) because the table hit
    /// [`MAX_MATERIALS`]. `0` in the common case; non-zero means
    /// raising the cap is appropriate. Surfaced through the `mem`
    /// console command.
    pub fn overflow_count(&self) -> usize {
        self.overflow_count
    }

    /// Number of true FxHash collisions detected this frame (two
    /// distinct `GpuMaterial` byte strings mapping to the same u64
    /// hash). Only available in debug builds — zero in release.
    /// Non-zero triggers a `debug_assert!` panic in the same
    /// `intern_by_hash` call, so this counter primarily serves future
    /// soft-warning modes or test introspection. See #1414.
    #[cfg(debug_assertions)]
    pub fn collision_count(&self) -> usize {
        self.collision_count
    }

    /// Number of user-interned unique materials this frame, excluding
    /// the seeded neutral default at slot 0. Use this in telemetry
    /// where "how many distinct materials did this frame's content
    /// actually need" is the question.
    ///
    /// Pre-fix the only available count was `len()`, which read as
    /// off-by-one on every frame: a single-mesh debug scene with one
    /// real material shows `len() == 2` (seeded slot 0 + 1 user
    /// material), making the dedup signal noisier than it should be.
    /// `len().saturating_sub(1)` is the corrected count; the
    /// `saturating_sub` defends against the impossible-by-construction
    /// `len() == 0` case (the seed runs in `new()` + `clear()`).
    /// See REN-D14-NEW-03 (audit 2026-05-09).
    pub fn unique_user_count(&self) -> usize {
        self.materials.len().saturating_sub(1)
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
    ///
    /// Grew 260 → 280 under #1147 / FO4-D6-003 Phase 2b (+20 B for
    /// `translucency_subsurface_r/g/b` + `translucency_transmissive_scale`
    /// + `translucency_turbulence`), then 280 → 284 under #1248 (+4 B
    /// for `ior`, the per-material refractive index that drives
    /// Schlick F0 derivation), then 284 → 296 under #1249 (+12 B for
    /// the Disney diffuse lobe — `subsurface` + `sheen` + `sheen_tint`),
    /// then 296 → 300 under #1250 (+4 B for `anisotropic`, the GGX
    /// ax/ay aspect ratio driver). Test name includes the size so a future
    /// size shift updates it in lockstep with the assertion.
    #[test]
    fn gpu_material_size_is_300_bytes() {
        assert_eq!(std::mem::size_of::<GpuMaterial>(), 300);
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

    /// Regression guard for `GpuMaterial` GLSL field names —
    /// REN-D14-NEW-02 (audit 2026-05-09). The offset pin
    /// (`gpu_material_field_offsets_match_shader_contract`) and the
    /// size pin (`gpu_material_size_is_300_bytes`) catch byte-level
    /// drift, but neither catches a GLSL-side field rename: the
    /// shader still reads from the same offset, the value still
    /// arrives in the right register, but the field's MEANING in
    /// the source no longer matches the Rust struct. A future
    /// reader chasing a "what does `mat.foo` mean?" question hits a
    /// dead end.
    ///
    /// This test asserts that every documented GLSL field name on
    /// the shader-side `struct GpuMaterial` declaration at
    /// `triangle.frag:110-184` is present in the file. Renaming the
    /// Rust field is fine; renaming the GLSL field fails this test
    /// and forces an audit of every reader downstream.
    #[test]
    fn gpu_material_glsl_field_names_pinned() {
        let src = include_str!("../../shaders/triangle.frag");
        // Authoritative list — every named field declared inside
        // `struct GpuMaterial { ... };` at `triangle.frag:110-184`.
        // Update both sites together when renaming a field on the
        // GLSL side; the Rust-side rename + this list keep the
        // contract bidirectional. The trailing `;` in the needle
        // disambiguates field declarations from incidental uses of
        // the same identifier in comments / other structs.
        for name in &[
            "roughness;", "metalness;", "emissiveMult;", "materialFlags;",
            "emissiveR,", "emissiveG,", "emissiveB,", "specularStrength;",
            "specularR,", "specularG,", "specularB,", "alphaThreshold;",
            "textureIndex,", "normalMapIndex,", "darkMapIndex,", "glowMapIndex;",
            "detailMapIndex,", "glossMapIndex,", "parallaxMapIndex,", "envMapIndex;",
            "envMaskIndex,", "alphaTestFunc,", "materialKind;",
            "materialAlpha;",
            "parallaxHeightScale,", "parallaxMaxPasses,", "uvOffsetU,", "uvOffsetV;",
            "uvScaleU,", "uvScaleV,", "diffuseR,", "diffuseG;",
            "diffuseB,", "ambientR,", "ambientG,", "ambientB;",
            "skinTintA,", "skinTintR,", "skinTintG,", "skinTintB;",
            "hairTintR,", "hairTintG,", "hairTintB,", "multiLayerEnvmapStrength;",
            "eyeLeftCenterX,", "eyeLeftCenterY,", "eyeLeftCenterZ,", "eyeCubemapScale;",
            "eyeRightCenterX,", "eyeRightCenterY,", "eyeRightCenterZ,", "multiLayerInnerThickness;",
            "multiLayerRefractionScale,", "multiLayerInnerScaleU,", "multiLayerInnerScaleV,", "sparkleR;",
            "sparkleG,", "sparkleB,", "sparkleIntensity,", "falloffStartAngle;",
            "falloffStopAngle,", "falloffStartOpacity,", "falloffStopOpacity,", "softFalloffDepth;",
            "greyscaleLutIndex;",
            // #1147 Phase 2b — BGSM translucency suite
            "translucencySubsurfaceR,", "translucencySubsurfaceG,", "translucencySubsurfaceB;",
            "translucencyTransmissiveScale;",
            "translucencyTurbulence;",
            // #1248 — per-material refractive index for Schlick F0
            "ior;",
            // #1249 — Disney diffuse lobe (subsurface + sheen + sheenTint)
            "subsurface;", "sheen;", "sheenTint;",
            // #1250 — anisotropic GGX ax/ay driver
            "anisotropic;",
        ] {
            assert!(
                src.contains(name),
                "triangle.frag: expected GpuMaterial GLSL field needle `{}` not found. \
                 If you renamed a field, update both the GLSL source and this list.",
                name
            );
        }
    }

    /// Regression guard for the GpuMaterial Shader Struct Sync (#806).
    /// The size pin (`gpu_material_size_is_300_bytes`) catches additions
    /// or removals; this catches reorderings WITHIN the existing 16
    /// vec4 slots that the size pin alone would miss — e.g. swapping
    /// `texture_index` and `normal_map_index` within vec4 #4 would
    /// preserve total size but produce wrong shader reads.
    ///
    /// Mirrors the `gpu_instance_field_offsets_match_shader_contract`
    /// pattern at `scene_buffer.rs:1453`. The shader-side
    /// `struct GpuMaterial` declaration at `triangle.frag:110-184` is the
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

        // ── greyscale palette LUT bindless handle, #890 Stage 2c
        //    (offset 256) ─────────────────────────────────────────
        assert_eq!(offset_of!(GpuMaterial, greyscale_lut_index), 256);

        // ── BGSM translucency parameter suite, #1147 Phase 2b
        //    (offsets 260-280) ─────────────────────────────────────
        assert_eq!(offset_of!(GpuMaterial, translucency_subsurface_r), 260);
        assert_eq!(offset_of!(GpuMaterial, translucency_subsurface_g), 264);
        assert_eq!(offset_of!(GpuMaterial, translucency_subsurface_b), 268);
        assert_eq!(offset_of!(GpuMaterial, translucency_transmissive_scale), 272);
        assert_eq!(offset_of!(GpuMaterial, translucency_turbulence), 276);

        // ── PBR IOR (#1248, offset 280) ──────────────────────────
        assert_eq!(offset_of!(GpuMaterial, ior), 280);

        // ── Disney diffuse lobe (#1249, offsets 284-292) ──────────
        assert_eq!(offset_of!(GpuMaterial, subsurface), 284);
        assert_eq!(offset_of!(GpuMaterial, sheen), 288);
        assert_eq!(offset_of!(GpuMaterial, sheen_tint), 292);

        // ── Anisotropic GGX (#1250, offset 296) ───────────────────
        assert_eq!(offset_of!(GpuMaterial, anisotropic), 296);
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

    /// #807 — `MaterialTable::new()` reserves slot 0 for the neutral
    /// `GpuMaterial::default()` so `material_id == 0` is always a
    /// safe-to-read fallback rather than aliasing whichever user
    /// material happened to intern first.
    #[test]
    fn new_seeds_neutral_default_at_slot_zero() {
        let table = MaterialTable::new();
        assert_eq!(table.len(), 1, "slot 0 must be pre-seeded");
        // GpuMaterial has byte-PartialEq but no Debug, so use assert!.
        assert!(
            table.materials()[0] == GpuMaterial::default(),
            "slot 0 must hold the neutral-lit default"
        );
        // No user-driven intern calls yet — telemetry stays honest.
        assert_eq!(table.interned_count(), 0);
    }

    /// #1032 / REN-D14-NEW-01 — `unique_user_count` excludes the
    /// seeded slot 0 so `mat.stats` reports actual user-distinct
    /// material counts. Pin the contract on the four shapes that
    /// matter:
    ///   * fresh table (no user interns) → 0
    ///   * one user material → 1 (not 2)
    ///   * default-only interns (dedup to slot 0) → 0
    ///   * post-clear → 0
    #[test]
    fn unique_user_count_excludes_seeded_slot() {
        let mut table = MaterialTable::new();
        assert_eq!(
            table.unique_user_count(),
            0,
            "fresh table has only the seeded neutral; zero user materials"
        );
        assert_eq!(
            table.len(),
            1,
            "sanity: len() still counts the seeded slot"
        );

        let mut user = GpuMaterial::default();
        user.roughness = 0.7;
        table.intern(user);
        assert_eq!(
            table.unique_user_count(),
            1,
            "one user material — pre-fix `mat.stats` reported 2 here"
        );
        assert_eq!(table.len(), 2, "sanity: len() = seeded + 1 user");

        // Interning the default GpuMaterial dedups to slot 0 — it
        // bumps `interned_count` but NOT the user count.
        let mut bare_default_table = MaterialTable::new();
        let _ = bare_default_table.intern(GpuMaterial::default());
        let _ = bare_default_table.intern(GpuMaterial::default());
        assert_eq!(
            bare_default_table.unique_user_count(),
            0,
            "default-only interns dedup to slot 0 — zero distinct user materials"
        );

        table.clear();
        assert_eq!(
            table.unique_user_count(),
            0,
            "clear re-seeds slot 0 only — user count drops to zero"
        );
    }

    /// #807 — `clear()` re-seeds slot 0 so the per-frame contract
    /// (id 0 == neutral default) holds at frame start, not just at
    /// engine boot.
    #[test]
    fn clear_re_seeds_neutral_default() {
        let mut table = MaterialTable::new();
        let mut user = GpuMaterial::default();
        user.roughness = 0.7;
        table.intern(user); // slot 1
        assert_eq!(table.len(), 2);

        table.clear();
        assert_eq!(table.len(), 1, "clear must leave slot 0 seeded");
        assert!(
            table.materials()[0] == GpuMaterial::default(),
            "clear must re-seed the neutral-lit default at slot 0"
        );
        assert_eq!(table.interned_count(), 0);
    }

    #[test]
    fn identical_materials_dedup_to_same_id() {
        let mut table = MaterialTable::new();
        let mat = GpuMaterial::default();
        let id_a = table.intern(mat);
        let id_b = table.intern(mat);
        assert_eq!(id_a, id_b);
        // Slot 0 (neutral default) absorbs both interns — the table
        // already had 1 entry seeded, so len stays at 1. #807.
        assert_eq!(id_a, 0, "default GpuMaterial must dedup to slot 0");
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn distinct_materials_get_distinct_ids() {
        let mut table = MaterialTable::new();
        let a = GpuMaterial::default();
        let mut b = GpuMaterial::default();
        b.roughness = 0.7;

        let id_a = table.intern(a);
        let id_b = table.intern(b);
        assert_ne!(id_a, id_b);
        // `a` dedupes to the seeded slot 0; `b` is distinct → slot 1.
        // Total len = 2 (seeded neutral + one user material). #807.
        assert_eq!(id_a, 0);
        assert_eq!(id_b, 1);
        assert_eq!(table.len(), 2);

        // Repeats still dedup to the original id.
        let mut a2 = GpuMaterial::default();
        a2.roughness = 0.5; // same as default
        assert_eq!(table.intern(a2), id_a);
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
        // Slot 0 = seeded neutral, slot 1 = `a`, slot 2 = `b`. #807.
        assert_eq!(table.len(), 3);
    }

    /// #890 Stage 2c — two `BSEffectShaderProperty` materials that
    /// differ ONLY in their `greyscale_lut_index` MUST dedup to
    /// distinct slots. Pre-Stage-2c the field at offset 256 was
    /// `_pad_falloff`, intentionally excluded from
    /// `hash_gpu_material_fields` (and therefore from
    /// `MaterialTable::intern`'s reverse index) because it was always
    /// 0.0. Now that the slot carries a real bindless handle, the
    /// hash MUST include it — otherwise two fire-effect meshes
    /// referencing different palette LUTs (e.g.
    /// `GradFireExplosion.dds` vs `GradPlasmaCold.dds`) would collapse
    /// to the same `material_id` and the second mesh would sample
    /// the wrong LUT.
    #[test]
    fn greyscale_lut_index_difference_is_distinct() {
        let mut table = MaterialTable::new();
        let mut a = GpuMaterial::default();
        let mut b = GpuMaterial::default();
        a.material_kind = 101; // MATERIAL_KIND_EFFECT_SHADER
        a.material_flags = material_flag::EFFECT_PALETTE_COLOR;
        a.greyscale_lut_index = 42;
        b.material_kind = 101;
        b.material_flags = material_flag::EFFECT_PALETTE_COLOR;
        b.greyscale_lut_index = 43;
        let id_a = table.intern(a);
        let id_b = table.intern(b);
        assert_ne!(
            id_a, id_b,
            "different greyscale_lut_index must NOT dedup — pre-Stage-2c the offset-256 \
             slot was excluded from hash_gpu_material_fields"
        );
        // Sanity: the hash function itself must produce different
        // outputs so the reverse-index lookup splits them.
        assert_ne!(
            hash_gpu_material_fields(&a),
            hash_gpu_material_fields(&b),
            "hash_gpu_material_fields must include greyscale_lut_index"
        );
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
        // Loop interns 10 materials. i=0 hits the seeded neutral slot;
        // i=1..9 each push a fresh slot. Total len = 1 (neutral) + 9
        // (user) = 10. #807.
        for i in 0..10 {
            let mut m = GpuMaterial::default();
            m.texture_index = i;
            table.intern(m);
        }
        assert_eq!(table.len(), 10);
        let cap_before = table.materials.capacity();
        table.clear();
        // Post-clear the seeded neutral default is re-pushed (#807),
        // so `len()` is 1 — not 0. The underlying allocation
        // capacity stays at the pre-clear size.
        assert_eq!(table.len(), 1);
        assert!(
            table.materials()[0] == GpuMaterial::default(),
            "post-clear slot 0 must hold the seeded neutral default"
        );
        assert!(table.materials.capacity() >= cap_before);
    }

    /// #780 / PERF-N1 — `interned_count` ticks on every `intern` call
    /// (hits AND misses) so the dedup ratio `len / interned_count` is
    /// computable from telemetry. `clear` resets it in lockstep with
    /// the materials Vec so the per-frame snapshot is honest.
    ///
    /// Post-#807: `intern(GpuMaterial::default())` is now a HIT on the
    /// seeded slot 0 (not a miss as it was pre-fix). `interned_count`
    /// still ticks because the producer-side `intern` call rate is
    /// unchanged — only the dedup hit/miss accounting shifts.
    #[test]
    fn interned_count_increments_on_hit_and_miss() {
        let mut table = MaterialTable::new();
        assert_eq!(table.interned_count(), 0);
        // Seed counts as a slot but NOT a producer intern (#807).
        assert_eq!(table.len(), 1);

        let a = GpuMaterial::default();
        let mut b = GpuMaterial::default();
        b.roughness = 0.7;

        table.intern(a); // hit on seeded slot 0
        assert_eq!(table.interned_count(), 1);
        assert_eq!(table.len(), 1);

        table.intern(a); // hit again — count still ticks
        assert_eq!(table.interned_count(), 2);
        assert_eq!(table.len(), 1);

        table.intern(b); // miss → push slot 1
        assert_eq!(table.interned_count(), 3);
        assert_eq!(table.len(), 2);

        // 5 more hits on b — only `interned_count` moves.
        for _ in 0..5 {
            table.intern(b);
        }
        assert_eq!(table.interned_count(), 8);
        assert_eq!(table.len(), 2);

        // Tweaking a fresh local must not retroactively count against
        // the original — byte-equal to default still hits slot 0.
        let mut a2 = GpuMaterial::default();
        a2.roughness = 0.5; // same as default
        table.intern(a2);
        assert_eq!(table.interned_count(), 9);
        assert_eq!(table.len(), 2);

        table.clear();
        assert_eq!(table.interned_count(), 0);
        // Post-clear the seeded neutral persists (#807).
        assert_eq!(table.len(), 1);
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
        // Slot 0 is the seeded neutral default (#807); user materials
        // start at slot 1 in insertion order.
        assert_eq!(slice.len(), 4);
        assert!(slice[0] == GpuMaterial::default(), "slot 0 = neutral");
        assert_eq!(slice[1].texture_index, 100);
        assert_eq!(slice[2].texture_index, 200);
        assert_eq!(slice[3].texture_index, 300);
    }

    /// #797 / SAFE-22 + #807 — over-cap interns return id `0` and
    /// share the neutral-default material's record (slot 0 is reserved
    /// for the neutral default per #807, which makes the over-cap
    /// fallback semantically clean: "use the neutral material" rather
    /// than "alias whichever user material happened to intern first").
    /// Without this cap a DrawCommand carrying the over-cap id would
    /// index past the MaterialBuffer SSBO end on the GPU
    /// (implementation-defined OOB read).
    ///
    /// Builds a fresh table, fills it to `MAX_MATERIALS` distinct
    /// entries (each varying by `texture_index`), then asserts:
    ///   1. The first `intern` of `texture_index = 0` HITS the seeded
    ///      neutral slot (id 0), and `intern` of `texture_index = i`
    ///      for `i >= 1` pushes a distinct slot at id `i` — total
    ///      table grows to exactly `MAX_MATERIALS` slots.
    ///   2. The next over-cap intern returns id `0` (the neutral).
    ///   3. The reverse-lookup map's count also stays bounded.
    ///   4. A subsequent intern of an already-interned material
    ///      still returns its original id — the cap doesn't poison
    ///      the dedup map.
    #[test]
    fn intern_overflow_returns_material_zero() {
        let mut table = MaterialTable::new();
        // Fill the table to exactly `MAX_MATERIALS` distinct entries.
        // `texture_index` is part of the byte-Hash dedup so each
        // increment produces a fresh GpuMaterial. Lucky alignment:
        // `texture_index = i` lands at slot `i` because the seeded
        // neutral has `texture_index = 0`, and `intern` of i=0 hits
        // it. Subsequent i=1..MAX_MATERIALS-1 each push a fresh slot.
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
        // After clear the seeded neutral default re-occupies slot 0
        // (#807). A user intern of a material distinct from neutral
        // pushes at slot 1 — NOT slot 0, since slot 0 is reserved.
        let mut first = GpuMaterial::default();
        first.texture_index = 1;
        assert_eq!(table.intern(first), 1);
        assert_eq!(table.len(), 2);

        // Interning the neutral default itself dedupes to slot 0.
        assert_eq!(table.intern(GpuMaterial::default()), 0);
    }
}
