//! Shared triangle-strip → triangle-list conversion.
//!
//! Every NIF/Havok strip format (`NiTriStripsData`, `NiSkinPartition`,
//! `bhkCompressedMeshShapeData` chunk strips) encodes triangles as a single
//! run of vertex indices using the OpenGL/Vulkan strip convention: even
//! triangles keep the natural order, odd triangles swap the last two
//! indices to preserve a consistent CCW winding (D3D's convention swaps the
//! FIRST two instead, producing CW — NOT used here). Strip-stitching
//! degenerate triangles (any two indices equal) are skipped.
//!
//! #2298 — this logic was hand-copied at three sites
//! (`NiTriStripsData::to_triangles`, `NiSkinPartition`'s inline destrip, and
//! `resolve_compressed_mesh`'s chunk-strip walk) with only comments pinning
//! them to agree; `resolve_compressed_mesh`'s copy silently diverged once
//! (fixed incidentally by `3b9227341`, not by design). Centralized here so
//! there is exactly one implementation to keep correct.
pub fn destrip<T: Copy + PartialEq>(strip: &[T]) -> impl Iterator<Item = [T; 3]> + '_ {
    (2..strip.len()).filter_map(move |i| {
        let (a, b, c) = if i % 2 == 0 {
            (strip[i - 2], strip[i - 1], strip[i])
        } else {
            (strip[i - 2], strip[i], strip[i - 1])
        };
        if a != b && b != c && a != c {
            Some([a, b, c])
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::destrip;

    #[test]
    fn even_odd_winding_matches_opengl_convention() {
        // Strip [0,1,2,3,4] -> tris (0,1,2), (1,3,2) [odd swap], (2,3,4).
        let strip = [0u16, 1, 2, 3, 4];
        let tris: Vec<_> = destrip(&strip).collect();
        assert_eq!(tris, vec![[0, 1, 2], [1, 3, 2], [2, 3, 4]]);
    }

    #[test]
    fn degenerate_stitch_triangles_are_skipped() {
        // Repeated index (stitching) at position 2 makes both the
        // even triangle (0,1,1) and the following odd triangle (1,2,1)
        // degenerate — both must be dropped, not emitted.
        let strip = [0u16, 1, 1, 2];
        let tris: Vec<_> = destrip(&strip).collect();
        assert!(tris.is_empty());

        // A stitch degenerate followed by a genuine triangle: only the
        // real one survives.
        let strip = [0u16, 1, 1, 2, 3];
        let tris: Vec<_> = destrip(&strip).collect();
        assert_eq!(tris, vec![[1, 2, 3]]);
    }

    #[test]
    fn shorter_than_a_triangle_yields_nothing() {
        let strip = [0u16, 1];
        assert!(destrip(&strip).next().is_none());
    }
}
