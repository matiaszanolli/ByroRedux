//! #4617 — regression guard for #4206's tangent pre-size in
//! `decode_bs_vertex_stream`.
//!
//! The dhat fixture (`tests/heap_allocation_bounds.rs`) never set
//! `VF_TANGENTS | VF_NORMALS`, so a revert of the pre-size to `Vec::new()`
//! stayed green and brought back log2(n) realloc+copy cycles per mesh block
//! on every normal-mapped Skyrim SE+/FO4+/Starfield mesh.
//!
//! The vertex count is deliberately not a power of two. Growing a
//! `Vec<[f32; 4]>` from empty by doubling lands on exactly 16 entries for a
//! 16-vertex mesh, so an exact-capacity assertion could not tell a pre-size
//! from push growth there; for 100 vertices growth ends at 128, the pre-size
//! at 100.

use super::*;
use crate::header::NifHeader;

const VERTEX_COUNT: usize = 100;

/// `VERTEX_COUNT` FO4-style half-precision vertices for `attrs`: 8 B position
/// plus `Bitangent X`, 4 B normal plus `Bitangent Y` when `VF_NORMALS`, and a
/// 4 B tangent plus `Bitangent Z` quad when `VF_NORMALS && VF_TANGENTS`.
/// Returns the stream bytes and the per-vertex stride.
fn packed_vertices(attrs: u16) -> (Vec<u8>, usize) {
    let mut stride = 8;
    if attrs & VF_NORMALS != 0 {
        stride += 4;
        if attrs & VF_TANGENTS != 0 {
            stride += 4;
        }
    }
    // Non-degenerate byte-normals; the values themselves are irrelevant.
    let mut bytes = Vec::with_capacity(stride * VERTEX_COUNT);
    for _ in 0..VERTEX_COUNT {
        bytes.extend((0..stride).map(|i| 0x40 + (i as u8 & 0x3F)));
    }
    (bytes, stride)
}

fn decode(attrs: u16) -> DecodedBsVertices {
    let (bytes, stride) = packed_vertices(attrs);
    let header = NifHeader::test_fo4();
    let mut stream = NifStream::new(&bytes, &header);
    decode_bs_vertex_stream(
        &mut stream,
        VERTEX_COUNT,
        attrs,
        stride,
        /* full_precision = */ false,
        /* is_skinned = */ false,
    )
    .expect("synthetic packed-vertex stream must decode")
}

/// A descriptor carrying the tangent quad on every vertex reserves the
/// tangent `Vec` up front, exactly.
#[test]
fn tangent_quad_descriptor_presizes_the_tangent_vec_exactly() {
    let decoded = decode(VF_VERTEX | VF_NORMALS | VF_TANGENTS);
    assert_eq!(decoded.tangents.len(), VERTEX_COUNT);
    assert_eq!(
        decoded.tangents.capacity(),
        VERTEX_COUNT,
        "tangents must be reserved once for the whole mesh (#4206); a capacity \
         of {} is push-doubling growth",
        VERTEX_COUNT.next_power_of_two()
    );
}

/// The pre-size is conditional. A mesh with no tangent quad must not pay for
/// an `n × 16 B` reservation it never fills.
#[test]
fn descriptors_without_a_tangent_quad_reserve_nothing() {
    for (label, attrs) in [
        ("normals only", VF_VERTEX | VF_NORMALS),
        (
            "tangents without normals (the quad is gated on both)",
            VF_VERTEX | VF_TANGENTS,
        ),
    ] {
        let decoded = decode(attrs);
        assert!(decoded.tangents.is_empty(), "{label}");
        assert_eq!(decoded.tangents.capacity(), 0, "{label}");
    }
}
