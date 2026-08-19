//! Tests for `vertex_desc_offset_mismatches` / `check_vertex_desc_offsets`.
//!
//! #2578 / SK-D1-03 — `decode_bs_vertex_stream`'s fixed field-order walk
//! never consults the `BSVertexDesc` offset nibbles for the attributes it
//! decodes; these tests pin the cross-check that catches a disagreement
//! between the walk's assumption and the descriptor's own table, most
//! notably the hypothetical this issue raised: `UV2 Offset` (nif.xml pos
//! 12) sitting *between* `UV1 Offset` (pos 8) and `Normal Offset` (pos 16)
//! would put every attribute after UV1 four bytes later than the walk
//! (which never advances its cursor for VF_UVS_2) assumes.

use super::*;

/// Build a synthetic `BSVertexDesc` from explicit nibble offsets (in
/// 4-byte quad units, matching the on-disk convention) plus the
/// 12-bit attribute mask and the low `Vertex Data Size` nibble.
fn build_vertex_desc(attrs: u16, size_quads: u64, offsets: &[(u32, u64)]) -> u64 {
    let mut desc = (attrs as u64) << 44 | (size_quads & 0xF);
    for &(pos, nibble) in offsets {
        assert!(nibble <= 0xF, "nibble {nibble} overflows a 4-bit field");
        desc |= nibble << pos;
    }
    desc
}

/// A static (unskinned) full-precision mesh whose offset nibbles agree
/// exactly with the fixed field-order walk: Vertex(16B) → UV1 → Normal →
/// Tangent → Color. No mismatch should surface.
#[test]
fn agreeing_offsets_report_no_mismatch_unskinned_full_precision() {
    let attrs = VF_VERTEX | VF_UVS | VF_NORMALS | VF_TANGENTS | VF_VERTEX_COLORS;
    // Byte cursor: 0 →(+16 pos)→ 16 →(+4 uv)→ 20 →(+4 normal)→ 24 →(+4 tangent)→ 28 →(+4 color)→ 32.
    let vertex_desc = build_vertex_desc(
        attrs,
        8, // 32 bytes / 4
        &[
            (8, 4),   // UV1 Offset = 16/4
            (16, 5),  // Normal Offset = 20/4
            (20, 6),  // Tangent Offset = 24/4
            (24, 7),  // Color Offset = 28/4
        ],
    );
    let mismatches = vertex_desc_offset_mismatches(vertex_desc, attrs, /* full_precision */ true, false);
    assert!(mismatches.is_empty(), "unexpected mismatches: {mismatches:?}");
}

/// A skinned SSE NPC body (always full-precision): Vertex → UV1 → Normal
/// → Tangent → Color → Skinning Data. No mismatch should surface.
#[test]
fn agreeing_offsets_report_no_mismatch_skinned() {
    let attrs =
        VF_VERTEX | VF_UVS | VF_NORMALS | VF_TANGENTS | VF_VERTEX_COLORS | VF_SKINNED;
    // Byte cursor: 16 → 20 (uv) → 24 (normal) → 28 (tangent) → 32 (color) → 44 (skin, +12).
    let vertex_desc = build_vertex_desc(
        attrs,
        11, // 44 bytes / 4
        &[
            (8, 4),   // UV1 Offset = 16/4
            (16, 5),  // Normal Offset = 20/4
            (20, 6),  // Tangent Offset = 24/4
            (24, 7),  // Color Offset = 28/4
            (28, 8),  // Skinning Data Offset = 32/4
        ],
    );
    let mismatches = vertex_desc_offset_mismatches(vertex_desc, attrs, /* full_precision */ true, true);
    assert!(mismatches.is_empty(), "unexpected mismatches: {mismatches:?}");
}

/// FO4+ half-precision, no tangents: Vertex(8B) → UV1 → Normal → Color.
/// No mismatch should surface.
#[test]
fn agreeing_offsets_report_no_mismatch_half_precision_no_tangents() {
    let attrs = VF_VERTEX | VF_UVS | VF_NORMALS | VF_VERTEX_COLORS;
    // Byte cursor: 0 →(+8 pos)→ 8 →(+4 uv)→ 12 →(+4 normal)→ 16 →(+4 color)→ 20.
    let vertex_desc = build_vertex_desc(
        attrs,
        5, // 20 bytes / 4
        &[
            (8, 2),  // UV1 Offset = 8/4
            (16, 3), // Normal Offset = 12/4
            (24, 4), // Color Offset = 16/4
        ],
    );
    let mismatches =
        vertex_desc_offset_mismatches(vertex_desc, attrs, /* full_precision */ false, false);
    assert!(mismatches.is_empty(), "unexpected mismatches: {mismatches:?}");
}

/// Isolated single-field drift: `Normal Offset` declares one quad later
/// than the walk assumes, everything else agrees. Exactly one mismatch,
/// naming "Normal", with the declared/assumed bytes the walk would need
/// to reconcile.
#[test]
fn single_field_drift_is_reported() {
    let attrs = VF_VERTEX | VF_UVS | VF_NORMALS | VF_VERTEX_COLORS;
    let vertex_desc = build_vertex_desc(
        attrs,
        6, // arbitrary; not load-bearing for this check
        &[
            (8, 4),  // UV1 Offset = 16/4 — agrees (VF_VERTEX full-precision → cursor 16)
            (16, 6), // Normal Offset = 24/4 — walk assumes 20/4; one quad later
            (24, 6), // Color Offset = 24/4 — walk assumes 24/4 (Normal not advanced in the walk's own bookkeeping) — agrees
        ],
    );
    let mismatches = vertex_desc_offset_mismatches(vertex_desc, attrs, /* full_precision */ true, false);
    assert_eq!(mismatches, vec![("Normal", 24, 20)]);
}

/// The issue's own hypothetical: `VF_UVS_2` is set (4 undecoded bytes
/// the walk never advances its cursor for) and the exporter actually
/// placed UV2 *mid-vertex*, right after UV1 — matching nif.xml's nibble
/// ordering (`UV2 Offset` sits between `UV1 Offset` and `Normal
/// Offset`). Every attribute after UV1 (Normal, Color) then declares an
/// offset four bytes later than the walk assumes, and the cross-check
/// must catch all of them.
#[test]
fn vf_uvs_2_mid_vertex_placement_drifts_every_later_field() {
    let attrs = VF_VERTEX | VF_UVS | VF_UVS_2 | VF_NORMALS | VF_VERTEX_COLORS;
    // The walk's own (UV2-blind) bookkeeping checks each field against the
    // cursor position *before* advancing past it: UV1 is checked at byte 16
    // (right after the 16-byte position), then the cursor moves to 20 and
    // Normal is checked there, then to 24 where Color is checked — the walk
    // never adds UV2's 4 bytes because it doesn't know they exist.
    //
    // True mid-vertex UV2 layout: 16 (pos) → 20 (uv1) → 24 (uv2, +4,
    // undecoded) → 28 (normal) → 32 (color). Declaring the descriptor this
    // way means Normal and Color each declare a byte offset 4 later than
    // the walk assumed for them (20→24, 24→28).
    let vertex_desc = build_vertex_desc(
        attrs,
        9, // 36 bytes / 4
        &[
            (8, 4),  // UV1 Offset = 16/4 — agrees (walk assumes 16 too)
            (12, 5), // UV2 Offset = 20/4 — not cross-checked (undecoded), informational only
            (16, 6), // Normal Offset = 24/4 — walk assumes byte 20: +4 drift
            (24, 7), // Color Offset = 28/4 — walk assumes byte 24: +4 drift
        ],
    );
    let mismatches = vertex_desc_offset_mismatches(vertex_desc, attrs, /* full_precision */ true, false);
    assert_eq!(mismatches, vec![("Normal", 24, 20), ("Color", 28, 24)]);
}
