//! Procedural primitive generators — pure CPU vertex / index data for the demo cube,
//! the debug box and sphere, and the UI quad. Split out of `mesh.rs` (#6520).

use crate::vertex::{UiVertex, Vertex};

/// Colored cube geometry: 24 vertices (4 per face), 36 indices, with UVs and normals.
pub fn cube_vertices() -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        // Front face (red-ish), normal = +Z
        Vertex::new(
            [-0.5, -0.5, 0.5],
            [1.0, 0.3, 0.3],
            [0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, -0.5, 0.5],
            [1.0, 0.3, 0.3],
            [0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, 0.5],
            [1.0, 0.5, 0.5],
            [0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, 0.5],
            [1.0, 0.5, 0.5],
            [0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
        // Back face (blue-ish), normal = -Z
        Vertex::new(
            [-0.5, -0.5, -0.5],
            [0.3, 0.3, 1.0],
            [0.0, 0.0, -1.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [0.5, -0.5, -0.5],
            [0.3, 0.3, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, -0.5],
            [0.5, 0.5, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, -0.5],
            [0.5, 0.5, 1.0],
            [0.0, 0.0, -1.0],
            [1.0, 0.0],
        ),
        // Top face (green-ish), normal = +Y
        Vertex::new(
            [-0.5, 0.5, -0.5],
            [0.3, 1.0, 0.3],
            [0.0, 1.0, 0.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, -0.5],
            [0.3, 1.0, 0.3],
            [0.0, 1.0, 0.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, 0.5],
            [0.5, 1.0, 0.5],
            [0.0, 1.0, 0.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, 0.5],
            [0.5, 1.0, 0.5],
            [0.0, 1.0, 0.0],
            [0.0, 0.0],
        ),
        // Bottom face (yellow-ish), normal = -Y
        Vertex::new(
            [-0.5, -0.5, -0.5],
            [1.0, 1.0, 0.3],
            [0.0, -1.0, 0.0],
            [0.0, 0.0],
        ),
        Vertex::new(
            [0.5, -0.5, -0.5],
            [1.0, 1.0, 0.3],
            [0.0, -1.0, 0.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [0.5, -0.5, 0.5],
            [1.0, 1.0, 0.5],
            [0.0, -1.0, 0.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [-0.5, -0.5, 0.5],
            [1.0, 1.0, 0.5],
            [0.0, -1.0, 0.0],
            [0.0, 1.0],
        ),
        // Right face (cyan-ish), normal = +X
        Vertex::new(
            [0.5, -0.5, -0.5],
            [0.3, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, -0.5],
            [0.3, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        Vertex::new(
            [0.5, 0.5, 0.5],
            [0.5, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [0.5, -0.5, 0.5],
            [0.5, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0],
        ),
        // Left face (magenta-ish), normal = -X
        Vertex::new(
            [-0.5, -0.5, -0.5],
            [1.0, 0.3, 1.0],
            [-1.0, 0.0, 0.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [-0.5, 0.5, -0.5],
            [1.0, 0.3, 1.0],
            [-1.0, 0.0, 0.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, 0.5],
            [1.0, 0.5, 1.0],
            [-1.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        Vertex::new(
            [-0.5, -0.5, 0.5],
            [1.0, 0.5, 1.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0],
        ),
    ];

    let indices = vec![
        0, 1, 2, 2, 3, 0, // front
        4, 6, 5, 6, 4, 7, // back
        // ±Y were wound opposite to the other four faces: their geometric
        // normal pointed inward, so a floor seen from above (or a ceiling
        // from below) was a back face and shaded black. Pinned by
        // `primitive_winding_agrees_with_vertex_normals`.
        8, 10, 9, 10, 8, 11, // top
        12, 13, 14, 14, 15, 12, // bottom
        16, 17, 18, 18, 19, 16, // right
        20, 22, 21, 22, 20, 23, // left
    ];

    (vertices, indices)
}

/// A single colored triangle in the XY plane at Z=0, with UVs and normals.
pub fn triangle_vertices(color: [f32; 3]) -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        Vertex::new([0.0, 0.5, 0.0], color, [0.0, 0.0, 1.0], [0.5, 0.0]),
        Vertex::new([-0.5, -0.5, 0.0], color, [0.0, 0.0, 1.0], [0.0, 1.0]),
        Vertex::new([0.5, -0.5, 0.0], color, [0.0, 0.0, 1.0], [1.0, 1.0]),
    ];
    let indices = vec![0, 1, 2];
    (vertices, indices)
}

/// A textured quad in the XY plane at Z=0, with normals.
pub fn quad_vertices() -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        Vertex::new(
            [-0.5, -0.5, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, -0.5, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
    ];
    let indices = vec![0, 1, 2, 2, 3, 0];
    (vertices, indices)
}

/// Parametric axis-aligned box with arbitrary half-extents and a single
/// flat vertex color. Outward-facing normals, per-face UVs. Unlike the
/// uniform-scale `Transform`, the half-extents let a single mesh model a
/// non-cubic shape (thin slabs for walls, tall blocks). Used by the
/// Cornell-box test harness (`byroredux::cornell`) and available to any
/// caller needing a quick colored box primitive.
pub fn box_vertices_colored(half: [f32; 3], color: [f32; 3]) -> (Vec<Vertex>, Vec<u32>) {
    let [hx, hy, hz] = half;
    let v = |p: [f32; 3], n: [f32; 3], uv: [f32; 2]| Vertex::new(p, color, n, uv);
    let vertices = vec![
        // +Z front
        v([-hx, -hy, hz], [0.0, 0.0, 1.0], [0.0, 1.0]),
        v([hx, -hy, hz], [0.0, 0.0, 1.0], [1.0, 1.0]),
        v([hx, hy, hz], [0.0, 0.0, 1.0], [1.0, 0.0]),
        v([-hx, hy, hz], [0.0, 0.0, 1.0], [0.0, 0.0]),
        // -Z back
        v([-hx, -hy, -hz], [0.0, 0.0, -1.0], [1.0, 1.0]),
        v([hx, -hy, -hz], [0.0, 0.0, -1.0], [0.0, 1.0]),
        v([hx, hy, -hz], [0.0, 0.0, -1.0], [0.0, 0.0]),
        v([-hx, hy, -hz], [0.0, 0.0, -1.0], [1.0, 0.0]),
        // +Y top
        v([-hx, hy, -hz], [0.0, 1.0, 0.0], [0.0, 1.0]),
        v([hx, hy, -hz], [0.0, 1.0, 0.0], [1.0, 1.0]),
        v([hx, hy, hz], [0.0, 1.0, 0.0], [1.0, 0.0]),
        v([-hx, hy, hz], [0.0, 1.0, 0.0], [0.0, 0.0]),
        // -Y bottom
        v([-hx, -hy, -hz], [0.0, -1.0, 0.0], [0.0, 0.0]),
        v([hx, -hy, -hz], [0.0, -1.0, 0.0], [1.0, 0.0]),
        v([hx, -hy, hz], [0.0, -1.0, 0.0], [1.0, 1.0]),
        v([-hx, -hy, hz], [0.0, -1.0, 0.0], [0.0, 1.0]),
        // +X right
        v([hx, -hy, -hz], [1.0, 0.0, 0.0], [0.0, 1.0]),
        v([hx, hy, -hz], [1.0, 0.0, 0.0], [0.0, 0.0]),
        v([hx, hy, hz], [1.0, 0.0, 0.0], [1.0, 0.0]),
        v([hx, -hy, hz], [1.0, 0.0, 0.0], [1.0, 1.0]),
        // -X left
        v([-hx, -hy, -hz], [-1.0, 0.0, 0.0], [1.0, 1.0]),
        v([-hx, hy, -hz], [-1.0, 0.0, 0.0], [1.0, 0.0]),
        v([-hx, hy, hz], [-1.0, 0.0, 0.0], [0.0, 0.0]),
        v([-hx, -hy, hz], [-1.0, 0.0, 0.0], [0.0, 1.0]),
    ];
    let indices = vec![
        0, 1, 2, 2, 3, 0, // front
        4, 6, 5, 6, 4, 7, // back
        // ±Y were wound opposite to the other four faces: their geometric
        // normal pointed inward, so a floor seen from above (or a ceiling
        // from below) was a back face and shaded black. Pinned by
        // `primitive_winding_agrees_with_vertex_normals`.
        8, 10, 9, 10, 8, 11, // top
        12, 13, 14, 14, 15, 12, // bottom
        16, 17, 18, 18, 19, 16, // right
        20, 22, 21, 22, 20, 23, // left
    ];
    (vertices, indices)
}

/// A flat-colored UV sphere centered at the origin. `rings` is the number
/// of latitude bands, `segments` the longitude divisions. Smooth (radial)
/// normals, equirectangular UVs. Outward winding matches the engine's
/// front-face convention. Used by the Cornell-box test harness to probe
/// curved-surface RT behaviour (GGX highlight shape, reflection/refraction
/// across the full normal range) that flat primitives can't.
pub fn uv_sphere(
    radius: f32,
    color: [f32; 3],
    rings: u32,
    segments: u32,
) -> (Vec<Vertex>, Vec<u32>) {
    let rings = rings.max(2);
    let segments = segments.max(3);
    let mut vertices = Vec::with_capacity(((rings + 1) * (segments + 1)) as usize);
    for r in 0..=rings {
        // theta: 0 (north pole, +Y) .. PI (south pole, -Y)
        let theta = std::f32::consts::PI * r as f32 / rings as f32;
        let (st, ct) = theta.sin_cos();
        for s in 0..=segments {
            let phi = std::f32::consts::TAU * s as f32 / segments as f32;
            let (sp, cp) = phi.sin_cos();
            let n = [st * cp, ct, st * sp];
            let pos = [n[0] * radius, n[1] * radius, n[2] * radius];
            let uv = [s as f32 / segments as f32, r as f32 / rings as f32];
            vertices.push(Vertex::new(pos, color, n, uv));
        }
    }
    let stride = segments + 1;
    let mut indices = Vec::with_capacity((rings * segments * 6) as usize);
    for r in 0..rings {
        for s in 0..segments {
            let a = r * stride + s;
            let b = a + stride;
            // Wind so the front face points outward (+normal).
            indices.extend_from_slice(&[a, a + 1, b, b, a + 1, b + 1]);
        }
    }
    (vertices, indices)
}

/// Lightweight fullscreen quad for UI overlay — position + UV only (20 B/vertex).
/// The presentation pass uses a positive-height Vulkan viewport: NDC y=-1
/// is the top row. Both DDS assets and Ruffle captures are top-row-first.
pub fn fullscreen_quad_ui_vertices() -> (Vec<UiVertex>, Vec<u32>) {
    let vertices = vec![
        UiVertex::new([-1.0, -1.0, 0.0], [0.0, 0.0]),
        UiVertex::new([1.0, -1.0, 0.0], [1.0, 0.0]),
        UiVertex::new([1.0, 1.0, 0.0], [1.0, 1.0]),
        UiVertex::new([-1.0, 1.0, 0.0], [0.0, 1.0]),
    ];
    let indices = vec![0, 1, 2, 2, 3, 0];
    (vertices, indices)
}

#[cfg(test)]
mod primitive_winding_tests {
    use super::*;

    #[test]
    fn ui_quad_matches_top_left_images_in_positive_height_vulkan_viewport() {
        let (vertices, indices) = fullscreen_quad_ui_vertices();
        assert_eq!(indices, [0, 1, 2, 2, 3, 0]);
        for vertex in vertices {
            assert_eq!(vertex.uv, [
                (vertex.position[0] + 1.0) * 0.5,
                (vertex.position[1] + 1.0) * 0.5,
            ]);
        }
    }

    /// Every triangle's winding-derived normal must point the same way as
    /// its authored vertex normals. RT hit shading and front-face tests use
    /// the geometric normal; a mismatch shades a lit face as a back face.
    fn assert_winding(name: &str, (vertices, indices): (Vec<Vertex>, Vec<u32>)) {
        for (triangle, corners) in indices.chunks_exact(3).enumerate() {
            let [a, b, c] =
                [corners[0], corners[1], corners[2]].map(|index| &vertices[index as usize]);
            let sub = |p: [f32; 3], q: [f32; 3]| [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
            let (e1, e2) = (sub(b.position, a.position), sub(c.position, a.position));
            let geometric = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let authored: [f32; 3] =
                std::array::from_fn(|axis| a.normal[axis] + b.normal[axis] + c.normal[axis]);
            let area: f32 = geometric.iter().map(|v| v * v).sum();
            if area < 1e-12 {
                // Zero-area pole triangles carry no winding to check.
                continue;
            }
            let dot: f32 = (0..3).map(|axis| geometric[axis] * authored[axis]).sum();
            assert!(
                dot > 0.0,
                "{name}: triangle {triangle} winds against its normals \
                 (geometric {geometric:?}, authored {authored:?})"
            );
        }
    }

    #[test]
    fn primitive_winding_agrees_with_vertex_normals() {
        assert_winding("cube", cube_vertices());
        assert_winding("box", box_vertices_colored([2.0, 0.1, 3.0], [1.0; 3]));
        assert_winding("quad", quad_vertices());
        assert_winding("sphere", uv_sphere(1.0, [1.0; 3], 8, 12));
    }
}
