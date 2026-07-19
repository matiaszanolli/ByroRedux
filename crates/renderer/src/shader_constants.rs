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

    // #1860 — `DBG_BITS` moved to `shader_constants_data.rs` (shared with
    // `build.rs`'s header emit, see that file's doc comment) so it's a
    // single source of truth for the emit, the value-pin below, the
    // no-redeclare guard (`triangle_frag_dbg_bits_not_redeclared`), and
    // the count-parity test right below. `use super::*` (top of this
    // module) brings it into scope from the `include!`d data file.

    /// #1860 — pins that `DBG_BITS` cannot silently drift behind a new
    /// `pub const DBG_*` again: every constant declared in
    /// `shader_constants_data.rs` must have a matching catalog entry.
    /// Counts `pub const DBG_` occurrences in the data file's source text
    /// rather than re-declaring the list, so this test fails the moment a
    /// new DBG_* constant is added without a catalog entry — the exact
    /// gap #1482 fixed once and #1860 found had regrown to 5 constants.
    #[test]
    fn dbg_bits_catalog_covers_every_dbg_constant() {
        let data_src = include_str!("shader_constants_data.rs");
        // Exclude `DBG_BITS` itself — it's the catalog, typed
        // `&[(&str, u32)]`, not one of the `u32` bit constants it lists.
        let declared = data_src
            .lines()
            .filter(|l| l.trim_start().starts_with("pub const DBG_"))
            .filter(|l| !l.trim_start().starts_with("pub const DBG_BITS"))
            .count();
        assert_eq!(
            DBG_BITS.len(),
            declared,
            "DBG_BITS has {} entries but shader_constants_data.rs declares {} \
             `pub const DBG_*` constants — a new DBG_* constant was added \
             without a matching DBG_BITS catalog entry",
            DBG_BITS.len(),
            declared,
        );
    }

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
            // #1920 — 10 defines `build.rs` emits that this value-pin had
            // never covered (found by an audit sweep alongside the
            // 2-day-old SHADOW_MASK_* pair, which shipped without a pin).
            ("CLUSTER_NEAR", format!("#define CLUSTER_NEAR {CLUSTER_NEAR:?}")),
            ("CLUSTER_FAR_FLOOR", format!("#define CLUSTER_FAR_FLOOR {CLUSTER_FAR_FLOOR:?}")),
            ("CLUSTER_FAR_FALLBACK", format!("#define CLUSTER_FAR_FALLBACK {CLUSTER_FAR_FALLBACK:?}")),
            ("VERTEX_NORMAL_OFFSET_FLOATS", format!("#define VERTEX_NORMAL_OFFSET_FLOATS {VERTEX_NORMAL_OFFSET_FLOATS}u")),
            ("VERTEX_UV_OFFSET_FLOATS", format!("#define VERTEX_UV_OFFSET_FLOATS {VERTEX_UV_OFFSET_FLOATS}u")),
            ("SHADOW_MASK_OPAQUE", format!("#define SHADOW_MASK_OPAQUE {SHADOW_MASK_OPAQUE}u")),
            ("SHADOW_MASK_GLASS", format!("#define SHADOW_MASK_GLASS {SHADOW_MASK_GLASS}u")),
            ("GI_HIT_LIGHT_CAP", format!("#define GI_HIT_LIGHT_CAP {GI_HIT_LIGHT_CAP}u")),
            ("CAUSTIC_FIXED_SCALE", format!("#define CAUSTIC_FIXED_SCALE {CAUSTIC_FIXED_SCALE:?}")),
            ("ENABLE_LEGACY_WRS", format!("#define ENABLE_LEGACY_WRS {ENABLE_LEGACY_WRS}")),
            // DBG_* bits are pinned below via the shared DBG_BITS catalog
            // (every constant, count-checked by
            // dbg_bits_catalog_covers_every_dbg_constant) — see #1482 / #1860.
            ("INSTANCE_FLAG_NON_UNIFORM_SCALE", format!("#define INSTANCE_FLAG_NON_UNIFORM_SCALE {INSTANCE_FLAG_NON_UNIFORM_SCALE}u")),
            ("INSTANCE_FLAG_ALPHA_BLEND", format!("#define INSTANCE_FLAG_ALPHA_BLEND {INSTANCE_FLAG_ALPHA_BLEND}u")),
            ("INSTANCE_FLAG_CAUSTIC_SOURCE", format!("#define INSTANCE_FLAG_CAUSTIC_SOURCE {INSTANCE_FLAG_CAUSTIC_SOURCE}u")),
            ("INSTANCE_FLAG_TERRAIN_SPLAT", format!("#define INSTANCE_FLAG_TERRAIN_SPLAT {INSTANCE_FLAG_TERRAIN_SPLAT}u")),
            ("INSTANCE_RENDER_LAYER_SHIFT", format!("#define INSTANCE_RENDER_LAYER_SHIFT {INSTANCE_RENDER_LAYER_SHIFT}u")),
            ("INSTANCE_RENDER_LAYER_MASK", format!("#define INSTANCE_RENDER_LAYER_MASK {INSTANCE_RENDER_LAYER_MASK}u")),
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
        // Every DBG_* bit, driven from the shared catalog so this
        // value-pin can never again cover a subset (#1482 / #1860).
        for (name, value) in DBG_BITS {
            let expected = format!("#define {name} {value}u");
            assert!(
                header.contains(&expected),
                "shader_constants.glsl missing or wrong value for {name}: expected `{expected}`",
            );
        }
    }

    /// Verify all affected shaders include the shared header.
    ///
    /// #1780 (D14-LOW-01) — this allow-list MUST cover every shader that
    /// consumes a generated macro from `shader_constants.glsl`; a shader
    /// that drops the `#include` would otherwise compile against undefined
    /// identifiers (`WORKGROUP_X`, `INSTANCE_FLAG_CAUSTIC_SOURCE`, …) and no
    /// `cargo test` would catch it (the SPIR-V is pre-compiled). The list
    /// previously omitted six header-including shaders — `caustic_splat.comp`
    /// (uses `INSTANCE_FLAG_CAUSTIC_SOURCE` + `WORKGROUP_X/Y`), `water.frag`
    /// (`WATER_*` + `CAUSTIC_FIXED_SCALE`), and the four compute passes whose
    /// `local_size_x = WORKGROUP_X` qualifier reads the header
    /// (`ssao.comp`, `svgf_atrous.comp`, `svgf_temporal.comp`, `taa.comp`).
    /// Cross-check when adding a shader: `grep -L` the include across
    /// `shaders/*.{comp,frag,vert}` and reconcile against this list.
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
            // #1780 — previously-unlisted header consumers.
            (
                "caustic_splat.comp",
                include_str!("../shaders/caustic_splat.comp"),
            ),
            ("water.frag", include_str!("../shaders/water.frag")),
            ("ssao.comp", include_str!("../shaders/ssao.comp")),
            (
                "svgf_atrous.comp",
                include_str!("../shaders/svgf_atrous.comp"),
            ),
            (
                "svgf_temporal.comp",
                include_str!("../shaders/svgf_temporal.comp"),
            ),
            ("taa.comp", include_str!("../shaders/taa.comp")),
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

    /// #2045 (TD7-101) — `triangle.frag` must NOT redeclare
    /// `INST_RENDER_LAYER_SHIFT`/`_MASK` (the pre-fix hand-written
    /// names) or `INSTANCE_RENDER_LAYER_SHIFT`/`_MASK` (the generated
    /// names) as a local `const uint`. Pre-fix, these two were
    /// hand-written directly in the shader with no lockstep test — a
    /// regression back to that pattern would silently drop the
    /// `#define`d values from `shader_constants.glsl`.
    #[test]
    fn triangle_frag_render_layer_bits_not_redeclared() {
        let src = include_str!("../shaders/triangle.frag");
        for needle in [
            "const uint INST_RENDER_LAYER_SHIFT",
            "const uint INST_RENDER_LAYER_MASK",
            "const uint INSTANCE_RENDER_LAYER_SHIFT",
            "const uint INSTANCE_RENDER_LAYER_MASK",
        ] {
            assert!(
                !src.contains(needle),
                "triangle.frag must not redeclare {needle} — \
                 the #define from shader_constants.glsl is the source of truth (#2045)",
            );
        }
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

    /// Shared scan for `<accessor> & N` where `N` is a bare numeric
    /// literal instead of a `#define`d `INSTANCE_FLAG_*` name. `accessor`
    /// is matched as a plain substring (e.g. `"inst.flags"` for the
    /// triangle shaders' struct-field access, or `"flags"` for
    /// `caustic_splat.comp`'s local variable — case-sensitive, so it
    /// doesn't false-match `sceneFlags` / `render_debug_flags`, whose
    /// `Flags`/`_flags` casing or trailing context never lands on a
    /// bare `accessor & digit` pattern). Skips comment lines. The regex
    /// would be `accessor\s*&\s*\d+u`, but a hand-rolled scan keeps the
    /// test free of regex deps.
    fn assert_no_bare_flags_literal(path: &str, src: &str, accessor: &str) {
        for (lineno, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                continue;
            }
            let Some(start) = line.find(accessor) else {
                continue;
            };
            let rest = &line[start + accessor.len()..];
            // The next non-whitespace char must be either nothing
            // (declaration like `<accessor> = ...`), `.` (field
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
                "{path}:{} uses bare numeric literal on `{accessor}`; \
                 use the `INSTANCE_FLAG_*` `#define` from shader_constants.glsl. \
                 Offending line: `{}`",
                lineno + 1,
                line.trim(),
            );
        }
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
            assert_no_bare_flags_literal(path, src, "inst.flags");
        }
    }

    /// #1234 / #1934 (CAUSTIC-D14-01) — `caustic_splat.comp` reads its
    /// per-instance flags into a local `flags` variable
    /// (`uint flags = instances[instIdx].flags;`) and tests it as
    /// `flags & INSTANCE_FLAG_CAUSTIC_SOURCE`. The #1234 fix (bare `4u` →
    /// the named constant) had no regression coverage: this shader isn't
    /// in `triangle_shaders_use_named_instance_flag_constants`'s list, and
    /// even if it were, that test searches for the `inst.flags` struct-
    /// access token, not caustic's local-variable accessor — a revert to
    /// `flags & 4u` would compile clean and pass the whole suite. Reuses
    /// the same bare-literal scan with `"flags"` as the accessor.
    #[test]
    fn caustic_splat_comp_uses_named_instance_flag_constant() {
        assert_no_bare_flags_literal(
            "caustic_splat.comp",
            include_str!("../shaders/caustic_splat.comp"),
            "flags",
        );
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

    /// #2045 (TD7-101) — `INSTANCE_RENDER_LAYER_SHIFT`/`_MASK` were
    /// previously hand-written directly in `triangle.frag` with no
    /// lockstep test, unlike every other `INSTANCE_FLAG_*` bit pinned by
    /// `instance_flag_bits_match_scene_buffer_consts` above. Now sourced
    /// from the generated header; this pins the shader-side mirror in
    /// `shader_constants_data.rs` equal to the authoritative
    /// `scene_buffer::constants` values so `RenderLayer`'s bit-packing
    /// can't silently drift between the two layers.
    #[test]
    fn instance_render_layer_bits_match_scene_buffer_consts() {
        use crate::vulkan::scene_buffer::{
            INSTANCE_RENDER_LAYER_MASK as SB_RENDER_LAYER_MASK,
            INSTANCE_RENDER_LAYER_SHIFT as SB_RENDER_LAYER_SHIFT,
        };
        assert_eq!(INSTANCE_RENDER_LAYER_SHIFT, SB_RENDER_LAYER_SHIFT);
        assert_eq!(INSTANCE_RENDER_LAYER_MASK, SB_RENDER_LAYER_MASK);
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

    /// #1799 / PERF-D5-NEW-01 — the shipped default must keep the legacy
    /// WRS arm preprocessed OUT of `triangle.frag`. Flipping this back to
    /// `1` (e.g. to A/B) is a deliberate, source-controlled, recompile-
    /// required action; it must never silently become the shipped default.
    #[test]
    fn legacy_wrs_arm_defaults_to_disabled() {
        assert_eq!(
            ENABLE_LEGACY_WRS, 0,
            "ENABLE_LEGACY_WRS must default to 0 (compiled out) — flipping \
             it to 1 re-enables the per-frame register/local-memory cost \
             this issue exists to eliminate"
        );
    }

    /// #1799 / PERF-D5-NEW-01 — the legacy 16-slot WRS reservoir arrays
    /// (`resLight`/`resWSel`) must be declared strictly inside an
    /// `#if ENABLE_LEGACY_WRS` / `#endif` block, not merely read/written
    /// behind a runtime `if`. A runtime-only guard around the *usage*
    /// doesn't stop the compiler from still declaring — and therefore
    /// budgeting the per-invocation register / local-memory footprint
    /// of — the arrays on every frame, including the ~100% of production
    /// frames that take the ReSTIR path and never touch them.
    #[test]
    fn triangle_frag_legacy_wrs_arrays_are_compile_time_gated() {
        let src = include_str!("../shaders/triangle.frag");

        let gate_pos = src
            .find("#if ENABLE_LEGACY_WRS")
            .expect("triangle.frag must have an ENABLE_LEGACY_WRS compile-time gate");
        let decl_pos = src
            .find("uint  resLight[NUM_RESERVOIRS];")
            .expect("triangle.frag must declare the legacy resLight reservoir array");
        let endif_pos = src[gate_pos..]
            .find("#endif")
            .map(|i| gate_pos + i)
            .expect("the ENABLE_LEGACY_WRS gate must be closed with #endif");

        assert!(
            gate_pos < decl_pos && decl_pos < endif_pos,
            "resLight[NUM_RESERVOIRS] must be declared strictly inside the \
             FIRST #if ENABLE_LEGACY_WRS / #endif block (#1799 / PERF-D5-NEW-01)"
        );
    }

    /// The renderer-evaluation suite relies on these switches representing
    /// independent estimator dimensions. Keep this contract close to the
    /// shader source so a refactor cannot silently turn the A/B captures into
    /// equivalent modes.
    #[test]
    fn triangle_frag_restir_reuse_dimensions_are_independently_gated() {
        let src = include_str!("../shaders/triangle.frag");

        assert!(
            src.contains("bool useSpatial = (dbgFlags & DBG_DISABLE_SPATIAL) == 0u;"),
            "DBG_DISABLE_SPATIAL must independently gate spatial reservoir reuse"
        );
        assert!(
            src.contains("bool useTemporal = (dbgFlags & DBG_DISABLE_TEMPORAL) == 0u;"),
            "DBG_DISABLE_TEMPORAL must independently gate temporal reservoir reuse"
        );
        assert!(
            src.contains("if (useTemporal && shadowFade > 0.01"),
            "temporal reprojection must be conditional on useTemporal"
        );
    }
}
