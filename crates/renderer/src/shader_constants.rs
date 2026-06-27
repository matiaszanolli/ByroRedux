// Shared constants that appear in both Rust renderer code and GLSL shaders.
//
// `build.rs` generates `shaders/include/shader_constants.glsl` from this
// same data (both files `include!` `shader_constants_data.rs`). Every
// affected shader then `#include "include/shader_constants.glsl"` at the
// top, compiled with `glslangValidator -V -I crates/renderer/shaders …`.
//
// Adding a constant: edit `shader_constants_data.rs`, run
// `cargo build -p byroredux-renderer` (re-gen header), recompile shaders.

// Pull in all pub consts from the single source of truth.
include!("shader_constants_data.rs");

/// Total cluster count (derived — not emitted to GLSL header separately).
pub const TOTAL_CLUSTERS: u32 = CLUSTER_TILES_X * CLUSTER_TILES_Y * CLUSTER_SLICES_Z;

/// Per-vertex size in bytes (derived from VERTEX_STRIDE_FLOATS).
pub const VERTEX_STRIDE_BYTES: u64 = VERTEX_STRIDE_FLOATS as u64 * 4;

#[cfg(test)]
mod tests {
    use super::*;

    /// Single source of truth for the `DBG_*` debug-viz bit catalog,
    /// shared by `generated_header_contains_all_defines` (value-pin) and
    /// `triangle_frag_dbg_bits_not_redeclared` (no-shadow). Adding a bit
    /// here covers BOTH contracts automatically — the divergence #1482
    /// fixed (value-pin covered only 4 of 13 bits, so a `build.rs`
    /// mis-value on the other 9 would ship silently) cannot recur. Keep in
    /// emit order to match `build.rs`.
    const DBG_BITS: &[(&str, u32)] = &[
        ("DBG_BYPASS_POM", DBG_BYPASS_POM),
        ("DBG_BYPASS_DETAIL", DBG_BYPASS_DETAIL),
        ("DBG_VIZ_NORMALS", DBG_VIZ_NORMALS),
        ("DBG_VIZ_TANGENT", DBG_VIZ_TANGENT),
        ("DBG_BYPASS_NORMAL_MAP", DBG_BYPASS_NORMAL_MAP),
        ("DBG_RESERVED_20", DBG_RESERVED_20),
        ("DBG_VIZ_RENDER_LAYER", DBG_VIZ_RENDER_LAYER),
        ("DBG_VIZ_GLASS_PASSTHRU", DBG_VIZ_GLASS_PASSTHRU),
        ("DBG_DISABLE_SPECULAR_AA", DBG_DISABLE_SPECULAR_AA),
        ("DBG_DISABLE_HALF_LAMBERT_FILL", DBG_DISABLE_HALF_LAMBERT_FILL),
        ("DBG_BYPASS_VERTEX_COLOR", DBG_BYPASS_VERTEX_COLOR),
        ("DBG_DISABLE_AO", DBG_DISABLE_AO),
        ("DBG_LEGACY_LIGHT_ATTEN", DBG_LEGACY_LIGHT_ATTEN),
    ];

    #[test]
    fn max_bones_per_mesh_matches_core() {
        assert_eq!(
            MAX_BONES_PER_MESH as usize,
            byroredux_core::ecs::components::MAX_BONES_PER_MESH,
            "shader_constants::MAX_BONES_PER_MESH must equal \
             byroredux_core::ecs::components::MAX_BONES_PER_MESH"
        );
    }

    #[test]
    fn vertex_stride_matches_vertex_struct() {
        assert_eq!(
            (VERTEX_STRIDE_FLOATS * 4) as usize,
            std::mem::size_of::<crate::Vertex>(),
            "VERTEX_STRIDE_FLOATS ({VERTEX_STRIDE_FLOATS}) × 4 must equal size_of::<Vertex>()"
        );
    }

    /// Verify the generated GLSL header contains the expected #define lines.
    /// This pins that build.rs actually emitted the current values.
    #[test]
    fn generated_header_contains_all_defines() {
        let header = include_str!("../shaders/include/shader_constants.glsl");
        for (name, expected) in [
            ("CLUSTER_TILES_X", format!("#define CLUSTER_TILES_X {CLUSTER_TILES_X}u")),
            ("CLUSTER_TILES_Y", format!("#define CLUSTER_TILES_Y {CLUSTER_TILES_Y}u")),
            ("CLUSTER_SLICES_Z", format!("#define CLUSTER_SLICES_Z {CLUSTER_SLICES_Z}u")),
            ("MAX_LIGHTS_PER_CLUSTER", format!("#define MAX_LIGHTS_PER_CLUSTER {MAX_LIGHTS_PER_CLUSTER}u")),
            ("VERTEX_STRIDE_FLOATS", format!("#define VERTEX_STRIDE_FLOATS {VERTEX_STRIDE_FLOATS}u")),
            ("MAX_BONES_PER_MESH", format!("#define MAX_BONES_PER_MESH {MAX_BONES_PER_MESH}u")),
            // No `u` suffix — used in a `layout(local_size_x = …)` qualifier (#1758).
            ("SKIN_WORKGROUP_SIZE", format!("#define SKIN_WORKGROUP_SIZE {SKIN_WORKGROUP_SIZE}")),
            ("MATERIAL_KIND_GLASS", format!("#define MATERIAL_KIND_GLASS {MATERIAL_KIND_GLASS}u")),
            ("MATERIAL_KIND_EFFECT_SHADER", format!("#define MATERIAL_KIND_EFFECT_SHADER {MATERIAL_KIND_EFFECT_SHADER}u")),
            ("MATERIAL_KIND_NO_LIGHTING", format!("#define MATERIAL_KIND_NO_LIGHTING {MATERIAL_KIND_NO_LIGHTING}u")),
            ("GLASS_RAY_BUDGET", format!("#define GLASS_RAY_BUDGET {GLASS_RAY_BUDGET}u")),
            ("GLASS_RAY_COST", format!("#define GLASS_RAY_COST {GLASS_RAY_COST}u")),
            ("WORKGROUP_X", format!("#define WORKGROUP_X {WORKGROUP_X}")),
            ("WORKGROUP_Y", format!("#define WORKGROUP_Y {WORKGROUP_Y}")),
            ("WORKGROUP_Z", format!("#define WORKGROUP_Z {WORKGROUP_Z}")),
            ("THREADS_PER_CLUSTER", format!("#define THREADS_PER_CLUSTER {THREADS_PER_CLUSTER}")),
            ("BLOOM_INTENSITY", format!("#define BLOOM_INTENSITY {BLOOM_INTENSITY:?}")),
            ("VOLUME_FAR", format!("#define VOLUME_FAR {VOLUME_FAR:?}")),
            ("NORMAL_ALPHA_SPEC_BIT", format!("#define NORMAL_ALPHA_SPEC_BIT {NORMAL_ALPHA_SPEC_BIT}u")),
            ("WATER_CALM", format!("#define WATER_CALM {WATER_CALM}u")),
            ("WATER_RIVER", format!("#define WATER_RIVER {WATER_RIVER}u")),
            ("WATER_RAPIDS", format!("#define WATER_RAPIDS {WATER_RAPIDS}u")),
            ("WATER_WATERFALL", format!("#define WATER_WATERFALL {WATER_WATERFALL}u")),
            // DBG_* bits are pinned below via the shared DBG_BITS catalog
            // (all 13, not the 4 that used to live here) — see #1482.
            ("INSTANCE_FLAG_NON_UNIFORM_SCALE", format!("#define INSTANCE_FLAG_NON_UNIFORM_SCALE {INSTANCE_FLAG_NON_UNIFORM_SCALE}u")),
            ("INSTANCE_FLAG_ALPHA_BLEND", format!("#define INSTANCE_FLAG_ALPHA_BLEND {INSTANCE_FLAG_ALPHA_BLEND}u")),
            ("INSTANCE_FLAG_CAUSTIC_SOURCE", format!("#define INSTANCE_FLAG_CAUSTIC_SOURCE {INSTANCE_FLAG_CAUSTIC_SOURCE}u")),
            ("INSTANCE_FLAG_TERRAIN_SPLAT", format!("#define INSTANCE_FLAG_TERRAIN_SPLAT {INSTANCE_FLAG_TERRAIN_SPLAT}u")),
            ("INSTANCE_FLAG_FLAT_SHADING", format!("#define INSTANCE_FLAG_FLAT_SHADING {INSTANCE_FLAG_FLAT_SHADING}u")),
            ("INSTANCE_FLAG_DIFFUSE_ALPHA", format!("#define INSTANCE_FLAG_DIFFUSE_ALPHA {INSTANCE_FLAG_DIFFUSE_ALPHA}u")),
            ("MAT_FLAG_VERTEX_COLOR_EMISSIVE", format!("#define MAT_FLAG_VERTEX_COLOR_EMISSIVE {MAT_FLAG_VERTEX_COLOR_EMISSIVE}u")),
            ("MAT_FLAG_EFFECT_SOFT", format!("#define MAT_FLAG_EFFECT_SOFT {MAT_FLAG_EFFECT_SOFT}u")),
            ("MAT_FLAG_EFFECT_PALETTE_COLOR", format!("#define MAT_FLAG_EFFECT_PALETTE_COLOR {MAT_FLAG_EFFECT_PALETTE_COLOR}u")),
            ("MAT_FLAG_EFFECT_PALETTE_ALPHA", format!("#define MAT_FLAG_EFFECT_PALETTE_ALPHA {MAT_FLAG_EFFECT_PALETTE_ALPHA}u")),
            ("MAT_FLAG_EFFECT_LIT", format!("#define MAT_FLAG_EFFECT_LIT {MAT_FLAG_EFFECT_LIT}u")),
            ("MAT_FLAG_PBR_BSDF", format!("#define MAT_FLAG_PBR_BSDF {MAT_FLAG_PBR_BSDF}u")),
            ("MAT_FLAG_TRANSLUCENCY", format!("#define MAT_FLAG_TRANSLUCENCY {MAT_FLAG_TRANSLUCENCY}u")),
            ("MAT_FLAG_MODEL_SPACE_NORMALS", format!("#define MAT_FLAG_MODEL_SPACE_NORMALS {MAT_FLAG_MODEL_SPACE_NORMALS}u")),
            ("MAT_FLAG_TRANSLUCENCY_THICK_OBJECT", format!("#define MAT_FLAG_TRANSLUCENCY_THICK_OBJECT {MAT_FLAG_TRANSLUCENCY_THICK_OBJECT}u")),
            ("MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO", format!("#define MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO {MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO}u")),
            ("MAT_FLAG_EFFECT_LI_SHIFT", format!("#define MAT_FLAG_EFFECT_LI_SHIFT {MAT_FLAG_EFFECT_LI_SHIFT}u")),
            // BGSM_AUTHORED intentionally NOT mirrored to GLSL — see build.rs.
        ] {
            assert!(
                header.contains(&expected),
                "shader_constants.glsl missing or wrong value for {name}: expected `{expected}`",
            );
        }
        // All 13 DBG_* bits, driven from the shared catalog so this
        // value-pin can never again cover a subset (#1482).
        for (name, value) in DBG_BITS {
            let expected = format!("#define {name} {value}u");
            assert!(
                header.contains(&expected),
                "shader_constants.glsl missing or wrong value for {name}: expected `{expected}`",
            );
        }
    }

    /// Verify all affected shaders include the shared header.
    #[test]
    fn affected_shaders_include_constants_header() {
        for (shader, src) in [
            (
                "cluster_cull.comp",
                include_str!("../shaders/cluster_cull.comp"),
            ),
            ("triangle.frag", include_str!("../shaders/triangle.frag")),
            ("triangle.vert", include_str!("../shaders/triangle.vert")),
            (
                "skin_vertices.comp",
                include_str!("../shaders/skin_vertices.comp"),
            ),
            (
                "skin_palette.comp",
                include_str!("../shaders/skin_palette.comp"),
            ),
            ("composite.frag", include_str!("../shaders/composite.frag")),
            (
                "bloom_downsample.comp",
                include_str!("../shaders/bloom_downsample.comp"),
            ),
            (
                "bloom_upsample.comp",
                include_str!("../shaders/bloom_upsample.comp"),
            ),
            (
                "volumetrics_inject.comp",
                include_str!("../shaders/volumetrics_inject.comp"),
            ),
            (
                "volumetrics_integrate.comp",
                include_str!("../shaders/volumetrics_integrate.comp"),
            ),
        ] {
            assert!(
                src.contains("#include \"include/shader_constants.glsl\""),
                "{shader}: must `#include \"include/shader_constants.glsl\"` at the top",
            );
        }
    }

    /// TD4-203 / #1126 — `composite.frag` must NOT redeclare
    /// `BLOOM_INTENSITY` as a `const float`. The `#define`d value
    /// from the included `shader_constants.glsl` is the single source
    /// of truth. A local `const float BLOOM_INTENSITY = ...;` after
    /// `#include` shadows the macro and breaks recompile-from-source
    /// (textually substitutes to `const float 0.15 = 0.15;`). Positive
    /// coverage that the value flows through correctly lives in
    /// `generated_header_contains_all_defines` (verifies the `#define`
    /// is emitted with the right value).
    #[test]
    fn composite_frag_bloom_intensity_not_redeclared() {
        let src = include_str!("../shaders/composite.frag");
        assert!(
            !src.contains("const float BLOOM_INTENSITY"),
            "composite.frag must not redeclare BLOOM_INTENSITY — \
             the #define from shader_constants.glsl is the source of truth (#1126)",
        );
    }

    /// TD4-204 / #1126 — same shape as the BLOOM_INTENSITY check above.
    #[test]
    fn composite_frag_volume_far_not_redeclared() {
        let src = include_str!("../shaders/composite.frag");
        assert!(
            !src.contains("const float VOLUME_FAR"),
            "composite.frag must not redeclare VOLUME_FAR — \
             the #define from shader_constants.glsl is the source of truth (#1126)",
        );
    }

    /// TD4-205 / #1256 — Water motion-kind enum source-of-truth.
    /// Pre-#1256 water.frag declared local `const uint WATER_CALM = 0u;`
    /// (etc.) duplicating the `#define`s in `shader_constants.glsl`.
    /// #1256 made water.frag `#include` the generated header so the
    /// constants flow through directly; the local `const uint`
    /// declarations now collide with the macros at compile time
    /// (preventing the redeclaration class of bug).
    ///
    /// Post-#1256 this test verifies water.frag does NOT redeclare —
    /// mirror of `triangle_frag_dbg_bits_not_redeclared` (line 189)
    /// pattern. The positive coverage that the values flow through
    /// correctly lives in `generated_header_contains_all_defines`
    /// (verifies each `#define` is emitted with the right value).
    #[test]
    fn water_frag_motion_enum_matches() {
        let src = include_str!("../shaders/water.frag");
        for name in [
            "WATER_CALM",
            "WATER_RIVER",
            "WATER_RAPIDS",
            "WATER_WATERFALL",
        ] {
            let needle = format!("const uint {name}");
            assert!(
                !src.contains(&needle),
                "water.frag must not redeclare {name} — \
                 the #define from shader_constants.glsl is the source of truth (#1256)",
            );
        }
    }

    /// TD4-206 / #1162 — `triangle.frag` must NOT redeclare any of the
    /// `DBG_*` bit flags (the shared `DBG_BITS` catalog) as `const uint`.
    /// The `#define`d values from the included `shader_constants.glsl` are
    /// the single source of truth. A local `const uint DBG_FOO = 0xN u;`
    /// after `#include` shadows the macro and breaks recompile-from-source
    /// (textually substitutes to `const uint 1u = 0x1u;`). Positive
    /// coverage that the value flows through correctly lives in
    /// `generated_header_contains_all_defines` (verifies each `#define`
    /// is emitted with the right value) — both tests now iterate the same
    /// `DBG_BITS` list, so they cannot drift (#1482).
    #[test]
    fn triangle_frag_dbg_bits_not_redeclared() {
        let src = include_str!("../shaders/triangle.frag");
        for (name, _) in DBG_BITS {
            let needle = format!("const uint {name}");
            assert!(
                !src.contains(&needle),
                "triangle.frag must not redeclare {name} — \
                 the #define from shader_constants.glsl is the source of truth (#1162)",
            );
        }
    }

    /// TD4-208 / #1151 — `cluster_cull.comp` must NOT redeclare
    /// `THREADS_PER_CLUSTER` as a `const uint`. The `#define`d value
    /// from the included `shader_constants.glsl` is the single source
    /// of truth. Positive coverage that the value flows through
    /// correctly lives in `generated_header_contains_all_defines`.
    #[test]
    fn cluster_cull_threads_per_cluster_not_redeclared() {
        let src = include_str!("../shaders/cluster_cull.comp");
        assert!(
            !src.contains("const uint THREADS_PER_CLUSTER"),
            "cluster_cull.comp must not redeclare THREADS_PER_CLUSTER — \
             the #define from shader_constants.glsl is the source of truth (#1151)",
        );
    }

    /// #1190 (TD4-NEW-01) — `triangle.frag` must NOT redeclare any
    /// `MAT_FLAG_*` bit as a local `const uint`. The `#define`d
    /// values from the included `shader_constants.glsl` are the
    /// single source of truth, mirrored from `material_flag::*` in
    /// `crates/renderer/src/vulkan/material.rs`. A local
    /// `const uint MAT_FLAG_FOO = 0xN u;` after `#include` shadows
    /// the macro and breaks recompile-from-source (textually
    /// substitutes to `const uint 1u = 0x1u;`).
    #[test]
    fn triangle_frag_mat_flag_bits_not_redeclared() {
        let src = include_str!("../shaders/triangle.frag");
        for name in [
            "MAT_FLAG_VERTEX_COLOR_EMISSIVE",
            "MAT_FLAG_EFFECT_SOFT",
            "MAT_FLAG_EFFECT_PALETTE_COLOR",
            "MAT_FLAG_EFFECT_PALETTE_ALPHA",
            "MAT_FLAG_EFFECT_LIT",
        ] {
            let needle = format!("const uint {name}");
            assert!(
                !src.contains(&needle),
                "triangle.frag must not redeclare {name} — \
                 the #define from shader_constants.glsl is the source of truth (#1190)",
            );
        }
    }

    /// #1401 — `triangle.frag` must NOT redeclare `MATERIAL_KIND_*`
    /// as local `const uint`. The `#define`d values from the included
    /// `shader_constants.glsl` are the single source of truth,
    /// mirrored from `scene_buffer/constants.rs`. A local
    /// `const uint MATERIAL_KIND_GLASS = 100u;` after `#include`
    /// shadows the macro and breaks recompile-from-source.
    #[test]
    fn triangle_frag_material_kind_not_redeclared() {
        let src = include_str!("../shaders/triangle.frag");
        for name in [
            "MATERIAL_KIND_GLASS",
            "MATERIAL_KIND_EFFECT_SHADER",
            "MATERIAL_KIND_NO_LIGHTING",
        ] {
            let needle = format!("const uint {name}");
            assert!(
                !src.contains(&needle),
                "triangle.frag must not redeclare {name} — \
                 the #define from shader_constants.glsl is the source of truth (#1401)",
            );
        }
    }

    /// #1401 — Pin shader-side `MATERIAL_KIND_*` values against the
    /// authoritative Rust constants in `scene_buffer/constants.rs`.
    #[test]
    fn material_kind_matches_scene_buffer_consts() {
        use crate::vulkan::scene_buffer::{
            MATERIAL_KIND_EFFECT_SHADER as SB_EFFECT_SHADER, MATERIAL_KIND_GLASS as SB_GLASS,
            MATERIAL_KIND_NO_LIGHTING as SB_NO_LIGHTING,
        };
        assert_eq!(MATERIAL_KIND_GLASS, SB_GLASS);
        assert_eq!(MATERIAL_KIND_EFFECT_SHADER, SB_EFFECT_SHADER);
        assert_eq!(MATERIAL_KIND_NO_LIGHTING, SB_NO_LIGHTING);
    }

    /// #1190 (TD4-NEW-01) — `triangle.frag` + `triangle.vert` must
    /// NOT test `inst.flags` with bare numeric literals. Every
    /// active `inst.flags & N` site must use a `#define`d
    /// `INSTANCE_FLAG_*` name from the included
    /// `shader_constants.glsl`. The flat_shading bit (formerly
    /// pinned at 128u by a single-purpose test) is now covered here
    /// alongside every other instance-flag bit.
    ///
    /// This catches both the recurrence of the
    /// `& 128u` / `& 8u` / `& 2u` / `& 1u` patterns the original
    /// audit flagged, and any future hand-rolled bit added without
    /// going through `shader_constants_data.rs`.
    #[test]
    fn triangle_shaders_use_named_instance_flag_constants() {
        for (path, src) in [
            ("triangle.frag", include_str!("../shaders/triangle.frag")),
            ("triangle.vert", include_str!("../shaders/triangle.vert")),
        ] {
            // Skim each non-comment line for the offending pattern.
            // The regex would be `inst\.flags\s*&\s*\d+u`, but a
            // hand-rolled scan keeps the test free of regex deps.
            for (lineno, line) in src.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                    continue;
                }
                let Some(start) = line.find("inst.flags") else {
                    continue;
                };
                let rest = &line[start + "inst.flags".len()..];
                // The next non-whitespace char must be either nothing
                // (declaration like `inst.flags = ...`), `.` (field
                // access — there is none today, but future-proof),
                // or `&`. If it's `&`, the token immediately after
                // the `&` and whitespace must NOT be a digit.
                let rest_trimmed = rest.trim_start();
                let Some(after_amp) = rest_trimmed.strip_prefix('&') else {
                    continue;
                };
                let after_amp_trimmed = after_amp.trim_start();
                let Some(first_char) = after_amp_trimmed.chars().next() else {
                    continue;
                };
                assert!(
                    !first_char.is_ascii_digit(),
                    "{path}:{} uses bare numeric literal on `inst.flags`; \
                     use the `INSTANCE_FLAG_*` `#define` from shader_constants.glsl. \
                     Offending line: `{}`",
                    lineno + 1,
                    line.trim(),
                );
            }
        }
    }

    /// #1190 (TD4-NEW-01) — The shader-side mirror of `INSTANCE_FLAG_*`
    /// in `shader_constants_data.rs` must equal the authoritative
    /// Rust-side values in `scene_buffer/constants.rs`. Two layers,
    /// one truth: drift here means the shader and the CPU pipeline
    /// disagree on which bit means which thing.
    #[test]
    fn instance_flag_bits_match_scene_buffer_consts() {
        use crate::vulkan::scene_buffer::{
            INSTANCE_FLAG_ALPHA_BLEND as SB_ALPHA_BLEND,
            INSTANCE_FLAG_CAUSTIC_SOURCE as SB_CAUSTIC_SOURCE,
            INSTANCE_FLAG_DIFFUSE_ALPHA as SB_DIFFUSE_ALPHA,
            INSTANCE_FLAG_FLAT_SHADING as SB_FLAT_SHADING,
            INSTANCE_FLAG_NON_UNIFORM_SCALE as SB_NON_UNIFORM_SCALE,
            INSTANCE_FLAG_TERRAIN_SPLAT as SB_TERRAIN_SPLAT,
        };
        assert_eq!(INSTANCE_FLAG_NON_UNIFORM_SCALE, SB_NON_UNIFORM_SCALE);
        assert_eq!(INSTANCE_FLAG_ALPHA_BLEND, SB_ALPHA_BLEND);
        assert_eq!(INSTANCE_FLAG_CAUSTIC_SOURCE, SB_CAUSTIC_SOURCE);
        assert_eq!(INSTANCE_FLAG_TERRAIN_SPLAT, SB_TERRAIN_SPLAT);
        assert_eq!(INSTANCE_FLAG_FLAT_SHADING, SB_FLAT_SHADING);
        assert_eq!(INSTANCE_FLAG_DIFFUSE_ALPHA, SB_DIFFUSE_ALPHA);
    }

    /// #1190 (TD4-NEW-01) — Same pin, for `MAT_FLAG_*` against
    /// `material_flag::*` in `vulkan/material.rs`.
    #[test]
    fn material_flag_bits_match_material_consts() {
        use crate::vulkan::material::material_flag;
        assert_eq!(
            MAT_FLAG_VERTEX_COLOR_EMISSIVE,
            material_flag::VERTEX_COLOR_EMISSIVE
        );
        assert_eq!(MAT_FLAG_EFFECT_SOFT, material_flag::EFFECT_SOFT);
        assert_eq!(
            MAT_FLAG_EFFECT_PALETTE_COLOR,
            material_flag::EFFECT_PALETTE_COLOR
        );
        assert_eq!(
            MAT_FLAG_EFFECT_PALETTE_ALPHA,
            material_flag::EFFECT_PALETTE_ALPHA
        );
        assert_eq!(MAT_FLAG_EFFECT_LIT, material_flag::EFFECT_LIT);
        // Bits 5-9 — Disney BSDF / SSS / model-space-normals suite
        // (#1285, was hand-written in triangle.frag without this pin).
        assert_eq!(MAT_FLAG_PBR_BSDF, material_flag::PBR_BSDF);
        assert_eq!(MAT_FLAG_TRANSLUCENCY, material_flag::TRANSLUCENCY);
        assert_eq!(
            MAT_FLAG_MODEL_SPACE_NORMALS,
            material_flag::MODEL_SPACE_NORMALS
        );
        assert_eq!(
            MAT_FLAG_TRANSLUCENCY_THICK_OBJECT,
            material_flag::TRANSLUCENCY_THICK_OBJECT
        );
        assert_eq!(
            MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO,
            material_flag::TRANSLUCENCY_MIX_ALBEDO
        );
        // Lighting-influence shift — a byte-field offset, not a single-bit flag.
        assert_eq!(MAT_FLAG_EFFECT_LI_SHIFT, material_flag::EFFECT_LI_SHIFT);
        // BGSM_AUTHORED intentionally NOT mirrored to GLSL — see build.rs.
    }
}
