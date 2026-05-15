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
}
