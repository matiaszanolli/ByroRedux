/// Shared constants that appear in both Rust renderer code and GLSL shaders.
///
/// `build.rs` generates `shaders/include/shader_constants.glsl` from this
/// same data (both files `include!` `shader_constants_data.rs`). Every
/// affected shader then `#include "include/shader_constants.glsl"` at the
/// top, compiled with `glslangValidator -V -I crates/renderer/shaders …`.
///
/// Adding a constant: edit `shader_constants_data.rs`, run
/// `cargo build -p byroredux-renderer` (re-gen header), recompile shaders.

// Pull in all pub consts from the single source of truth.
include!("shader_constants_data.rs");

/// Total cluster count (derived — not emitted to GLSL header separately).
pub const TOTAL_CLUSTERS: u32 = CLUSTER_TILES_X * CLUSTER_TILES_Y * CLUSTER_SLICES_Z;

/// Per-vertex size in bytes (derived from VERTEX_STRIDE_FLOATS).
pub const VERTEX_STRIDE_BYTES: u64 = VERTEX_STRIDE_FLOATS as u64 * 4;

#[cfg(test)]
mod tests {
    use super::*;

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
        let header =
            include_str!("../shaders/include/shader_constants.glsl");
        for (name, expected) in [
            ("CLUSTER_TILES_X", format!("#define CLUSTER_TILES_X {CLUSTER_TILES_X}u")),
            ("CLUSTER_TILES_Y", format!("#define CLUSTER_TILES_Y {CLUSTER_TILES_Y}u")),
            ("CLUSTER_SLICES_Z", format!("#define CLUSTER_SLICES_Z {CLUSTER_SLICES_Z}u")),
            ("MAX_LIGHTS_PER_CLUSTER", format!("#define MAX_LIGHTS_PER_CLUSTER {MAX_LIGHTS_PER_CLUSTER}u")),
            ("VERTEX_STRIDE_FLOATS", format!("#define VERTEX_STRIDE_FLOATS {VERTEX_STRIDE_FLOATS}u")),
            ("MAX_BONES_PER_MESH", format!("#define MAX_BONES_PER_MESH {MAX_BONES_PER_MESH}u")),
            ("GLASS_RAY_BUDGET", format!("#define GLASS_RAY_BUDGET {GLASS_RAY_BUDGET}u")),
            ("GLASS_RAY_COST", format!("#define GLASS_RAY_COST {GLASS_RAY_COST}u")),
            ("WORKGROUP_X", format!("#define WORKGROUP_X {WORKGROUP_X}")),
            ("WORKGROUP_Y", format!("#define WORKGROUP_Y {WORKGROUP_Y}")),
            ("WORKGROUP_Z", format!("#define WORKGROUP_Z {WORKGROUP_Z}")),
            ("THREADS_PER_CLUSTER", format!("#define THREADS_PER_CLUSTER {THREADS_PER_CLUSTER}")),
            ("BLOOM_INTENSITY", format!("#define BLOOM_INTENSITY {BLOOM_INTENSITY:?}")),
            ("VOLUME_FAR", format!("#define VOLUME_FAR {VOLUME_FAR:?}")),
            ("WATER_CALM", format!("#define WATER_CALM {WATER_CALM}u")),
            ("WATER_RIVER", format!("#define WATER_RIVER {WATER_RIVER}u")),
            ("WATER_RAPIDS", format!("#define WATER_RAPIDS {WATER_RAPIDS}u")),
            ("WATER_WATERFALL", format!("#define WATER_WATERFALL {WATER_WATERFALL}u")),
            ("DBG_BYPASS_POM", format!("#define DBG_BYPASS_POM {DBG_BYPASS_POM}u")),
            ("DBG_VIZ_NORMALS", format!("#define DBG_VIZ_NORMALS {DBG_VIZ_NORMALS}u")),
            ("DBG_BYPASS_NORMAL_MAP", format!("#define DBG_BYPASS_NORMAL_MAP {DBG_BYPASS_NORMAL_MAP}u")),
            ("DBG_DISABLE_HALF_LAMBERT_FILL", format!("#define DBG_DISABLE_HALF_LAMBERT_FILL {DBG_DISABLE_HALF_LAMBERT_FILL}u")),
        ] {
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
            ("cluster_cull.comp", include_str!("../shaders/cluster_cull.comp")),
            ("triangle.frag", include_str!("../shaders/triangle.frag")),
            ("triangle.vert", include_str!("../shaders/triangle.vert")),
            ("skin_vertices.comp", include_str!("../shaders/skin_vertices.comp")),
            ("composite.frag", include_str!("../shaders/composite.frag")),
            ("bloom_downsample.comp", include_str!("../shaders/bloom_downsample.comp")),
            ("bloom_upsample.comp", include_str!("../shaders/bloom_upsample.comp")),
            ("volumetrics_inject.comp", include_str!("../shaders/volumetrics_inject.comp")),
            ("volumetrics_integrate.comp", include_str!("../shaders/volumetrics_integrate.comp")),
        ] {
            assert!(
                src.contains("#include \"include/shader_constants.glsl\""),
                "{shader}: must `#include \"include/shader_constants.glsl\"` at the top",
            );
        }
    }

    /// Helper: assert that the line declaring `name` in `src` (under a
    /// `const <type> <name>` prefix) contains `value_token`.
    /// Tolerates GLSL alignment padding (`const uint NAME     = 0u;`).
    fn assert_shader_const_value(
        src: &str,
        decl_prefix: &str,
        name: &str,
        value_token: &str,
        rust_origin: &str,
    ) {
        let needle = format!("{decl_prefix} {name}");
        let pos = src
            .find(&needle)
            .unwrap_or_else(|| panic!("shader missing `{needle}` declaration"));
        let line_end = src[pos..].find('\n').map(|n| pos + n).unwrap_or(src.len());
        let line = src[pos..line_end].trim();
        assert!(
            line.contains(value_token),
            "shader declaration `{line}` does not contain expected value token `{value_token}` \
             (Rust side: {rust_origin})",
        );
    }

    /// TD4-203 — `composite.frag::BLOOM_INTENSITY` must match
    /// `shader_constants::BLOOM_INTENSITY`. When the shader migrates
    /// to `#include "include/shader_constants.glsl"`, drop the local
    /// declaration and rely on the auto-generated `#define`.
    #[test]
    fn composite_frag_bloom_intensity_matches() {
        let src = include_str!("../shaders/composite.frag");
        let token = format!("= {BLOOM_INTENSITY:?};");
        assert_shader_const_value(src, "const float", "BLOOM_INTENSITY", &token, "BLOOM_INTENSITY");
    }

    /// TD4-204 — `composite.frag::VOLUME_FAR` must match
    /// `shader_constants::VOLUME_FAR`. Same migration path.
    #[test]
    fn composite_frag_volume_far_matches() {
        let src = include_str!("../shaders/composite.frag");
        let token = format!("= {VOLUME_FAR:?};");
        assert_shader_const_value(src, "const float", "VOLUME_FAR", &token, "VOLUME_FAR");
    }

    /// TD4-205 — Water motion-kind enum in `water.frag` must match
    /// the Rust constants. WATR records key off these values.
    #[test]
    fn water_frag_motion_enum_matches() {
        let src = include_str!("../shaders/water.frag");
        for (name, value) in [
            ("WATER_CALM", WATER_CALM),
            ("WATER_RIVER", WATER_RIVER),
            ("WATER_RAPIDS", WATER_RAPIDS),
            ("WATER_WATERFALL", WATER_WATERFALL),
        ] {
            let token = format!("= {value}u;");
            assert_shader_const_value(src, "const uint", name, &token, name);
        }
    }

    /// TD4-206 — DBG_* bit flags in `triangle.frag` must match the
    /// Rust constants. Console commands set these by name; drift
    /// would silently route the wrong bit.
    #[test]
    fn triangle_frag_dbg_bits_match() {
        let src = include_str!("../shaders/triangle.frag");
        for (name, value) in [
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
        ] {
            let token = format!("= 0x{value:X}u;");
            assert_shader_const_value(src, "const uint", name, &token, name);
        }
    }

    /// TD4-208 — `cluster_cull.comp::THREADS_PER_CLUSTER` must match
    /// `shader_constants::THREADS_PER_CLUSTER`. Shader currently shadows
    /// the generated `#define`; both must agree.
    #[test]
    fn cluster_cull_threads_per_cluster_matches() {
        let src = include_str!("../shaders/cluster_cull.comp");
        // cluster_cull writes the value without `u` suffix.
        let token = format!("= {THREADS_PER_CLUSTER};");
        assert_shader_const_value(
            src,
            "const uint",
            "THREADS_PER_CLUSTER",
            &token,
            "THREADS_PER_CLUSTER",
        );
    }
}
