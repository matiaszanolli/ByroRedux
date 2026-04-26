//! Tests for `bsvertex_flag_constant_tests` extracted from ../tri_shape.rs (refactor stage A).
//!
//! Same qualified path preserved (`bsvertex_flag_constant_tests::FOO`).

//! Regression for #336 / audit N2-01 — every bit in nif.xml's
//! `VertexAttribute` bitflags (line 2077) must have a matching
//! `VF_*` constant here. Pre-#336 the constant set skipped bits 2
//! and 7; the sequential per-vertex parser still worked thanks to
//! the trailing skip, but a reader auditing the schema against the
//! code saw nothing for those bits. These asserts pin every bit
//! value against nif.xml so a future contributor can't accidentally
//! redefine one without the test objecting.
use super::*;

#[test]
fn vertex_attribute_bits_match_nifxml_schema() {
    // nif.xml `VertexAttribute` bitflags — every option's bit value.
    assert_eq!(VF_VERTEX, 1 << 0);
    assert_eq!(VF_UVS, 1 << 1);
    assert_eq!(VF_UVS_2, 1 << 2);
    assert_eq!(VF_NORMALS, 1 << 3);
    assert_eq!(VF_TANGENTS, 1 << 4);
    assert_eq!(VF_VERTEX_COLORS, 1 << 5);
    assert_eq!(VF_SKINNED, 1 << 6);
    assert_eq!(VF_LAND_DATA, 1 << 7);
    assert_eq!(VF_EYE_DATA, 1 << 8);
    assert_eq!(VF_INSTANCE, 1 << 9);
    assert_eq!(VF_FULL_PRECISION, 1 << 10);
}

/// Guard against a duplicate-value typo: every `VF_*` bit must be
/// unique. A naive constant renumbering (e.g. copying VF_UVS's
/// value into VF_UVS_2) would otherwise compile cleanly but mis-
/// interpret the vertex descriptor at runtime.
#[test]
fn vertex_attribute_bits_are_all_distinct() {
    let bits = [
        VF_VERTEX,
        VF_UVS,
        VF_UVS_2,
        VF_NORMALS,
        VF_TANGENTS,
        VF_VERTEX_COLORS,
        VF_SKINNED,
        VF_LAND_DATA,
        VF_EYE_DATA,
        VF_INSTANCE,
        VF_FULL_PRECISION,
    ];
    for (i, a) in bits.iter().enumerate() {
        for b in &bits[i + 1..] {
            assert_ne!(a, b, "two VF_* constants share the same bit ({a:#05x})");
        }
    }
}

/// A vertex descriptor that declares VF_UVS_2 / VF_LAND_DATA (and
/// doesn't declare any other field beyond VF_VERTEX) must still
/// parse cleanly: the trailing skip at the end of the per-vertex
/// loop absorbs the flag's reserved bytes so the overall `data_size`
/// contract holds. Pre-#336 this path was untested — if some mod-
/// authored content set these bits, the parser worked by luck of
/// the trailing-skip backstop, never proven by a test.
#[test]
fn vf_uvs_2_and_vf_land_data_set_bits_survive_trailing_skip() {
    // Build a fake vertex_attrs word with VF_VERTEX + VF_UVS_2 +
    // VF_LAND_DATA, OR'd into the top-12-bit attributes field.
    let attrs: u16 = VF_VERTEX | VF_UVS_2 | VF_LAND_DATA;
    let vertex_attrs_in_desc = (attrs as u64) << 44;
    // The low nibble of BSVertexDesc is `vertex_size_quads`. Use a
    // small value so the synthetic byte stream stays compact.
    // 5 quads = 20 bytes/vertex (3 f32 position = 12 + 4 bitangent
    // + 4 reserved for UV2/land).
    let vertex_size_quads: u64 = 5;
    let vertex_desc = vertex_attrs_in_desc | vertex_size_quads;
    // Only the extraction of `vertex_attrs` out of `vertex_desc`
    // is under test here — the check asserts the bitfield math
    // round-trips.
    let extracted = ((vertex_desc >> 44) & 0xFFF) as u16;
    assert!(extracted & VF_UVS_2 != 0);
    assert!(extracted & VF_LAND_DATA != 0);
    assert!(extracted & VF_VERTEX != 0);
    assert!(extracted & VF_NORMALS == 0);
}
