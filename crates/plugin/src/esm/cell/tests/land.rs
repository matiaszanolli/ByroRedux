//! LAND (landscape heightmap) parsing tests — EX-10/11 item 5 (#2371).
//!
//! No test file exercised `parse_land_record` before this one: real per-game
//! LAND corpora weren't available to derive value-plausibility bounds
//! (height range, VNML unit-vector sanity, VCLR range) without guessing at
//! thresholds — the no-guessing policy applies to test fixtures too, not
//! just production heuristics. What real corpus data DID establish (per the
//! UESP VHGT algorithm this decoder implements, and the render-side decode
//! this file's own doc comment on `normals`/`vertex_colors` documents) is
//! covered here: the VHGT delta-decode arithmetic, VNML/VCLR raw-byte
//! storage, and the ATXT-then-VTXT pairing contract. BTXT/ATXT FormID
//! *existence* (referenced LTEX resolves against the loaded index) is
//! already checked downstream at consume time
//! (`cell_loader::terrain::spawn_terrain_mesh`, which `log::warn!`s and
//! skips a layer whose LTEX isn't in `landscape_textures`) — not a parse-time
//! gap, contrary to this issue's original framing.

use super::super::walkers::parse_land_record;
use super::super::*;

/// Append one sub-record (4-CC + u16 length + payload) to a buffer.
fn put_sub(buf: &mut Vec<u8>, ty: &[u8; 4], payload: &[u8]) {
    buf.extend_from_slice(ty);
    buf.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    buf.extend_from_slice(payload);
}

/// Build a synthetic Tes5Plus-header LAND record from a sub-record list.
fn build_land_record(form_id: u32, subs: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut sub_data = Vec::new();
    for (ty, payload) in subs {
        put_sub(&mut sub_data, ty, payload);
    }
    let mut buf = Vec::new();
    buf.extend_from_slice(b"LAND");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // trailer
    buf.extend_from_slice(&sub_data);
    buf
}

/// Drive `parse_land_record` over a synthetic LAND record's bytes,
/// returning the decoded [`LandscapeData`].
fn parse_synthetic_land(subs: &[(&[u8; 4], Vec<u8>)]) -> LandscapeData {
    let record = build_land_record(0x0001_2345, subs);
    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &record,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let header = reader.read_record_header().unwrap();
    parse_land_record(&mut reader, &header).unwrap()
}

/// A flat VHGT payload: base offset 0.0, every row/column delta 0 —
/// decodes to a perfectly flat 33×33 grid at height 0.0.
fn flat_vhgt() -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&0.0f32.to_le_bytes()); // base offset
    payload.extend_from_slice(&[0u8; 33 * 33]); // every delta byte = 0
    payload.extend_from_slice(&[0u8; 3]); // trailing unknown bytes (real VHGT carries a few)
    payload
}

#[test]
fn vhgt_flat_grid_decodes_to_constant_height() {
    let land = parse_synthetic_land(&[(b"VHGT", flat_vhgt())]);
    assert_eq!(land.heights.len(), 33 * 33);
    assert!(
        land.heights.iter().all(|&h| h == 0.0),
        "an all-zero-delta VHGT must decode to a flat plane at the base offset"
    );
}

/// UESP VHGT algorithm: each row accumulates from the PREVIOUS row's
/// starting height (not from the base offset directly) via the row's own
/// first delta byte, then each column accumulates from the row start.
/// A +1 delta (encoded as `1u8` = +8.0 game units per the `* 8.0` scale)
/// on row 1's first byte must raise every vertex in row 1 by 8.0 relative
/// to row 0, and rows 2+ inherit that same offset since their own first
/// delta is 0.
#[test]
fn vhgt_row_delta_accumulates_forward_not_just_within_row() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&0.0f32.to_le_bytes());
    let mut deltas = vec![0u8; 33 * 33];
    deltas[33] = 1; // row 1's first byte: +8.0
    payload.extend_from_slice(&deltas);
    payload.extend_from_slice(&[0u8; 3]);

    let land = parse_synthetic_land(&[(b"VHGT", payload)]);
    assert_eq!(land.heights[0], 0.0, "row 0 is untouched by row 1's delta");
    assert_eq!(
        land.heights[33], 8.0,
        "row 1 col 0 rises by the encoded 8.0"
    );
    assert_eq!(
        land.heights[33 + 32],
        8.0,
        "row 1's whole row inherits its own first-delta offset (col deltas are all 0 here)"
    );
    assert_eq!(
        land.heights[33 * 2],
        8.0,
        "row 2's first delta is 0, so it inherits row 1's accumulated offset — \
         proves the accumulator carries FORWARD across rows, not just within one"
    );
}

/// A too-short VHGT payload (below the 1093-byte minimum: 4-byte offset +
/// 33*33 delta bytes) must be silently skipped, not panic or corrupt the
/// heightmap — matches the sub-length guard already on every other
/// sub-record arm in `parse_land_record`.
#[test]
fn vhgt_short_payload_is_skipped_not_decoded() {
    let land = parse_synthetic_land(&[(b"VHGT", vec![0u8; 100])]);
    assert!(
        land.heights.iter().all(|&h| h == 0.0),
        "a truncated VHGT must leave the default zeroed heightmap untouched"
    );
}

#[test]
fn vnml_and_vclr_store_raw_bytes_verbatim() {
    let mut vnml = vec![0u8; 33 * 33 * 3];
    vnml[0] = 200;
    vnml[1] = 50;
    vnml[2] = 255;
    let mut vclr = vec![0u8; 33 * 33 * 3];
    vclr[3] = 10;
    vclr[4] = 20;
    vclr[5] = 30;

    let land = parse_synthetic_land(&[(b"VNML", vnml.clone()), (b"VCLR", vclr.clone())]);
    assert_eq!(
        land.normals.as_deref(),
        Some(&vnml[..3267]),
        "VNML must be stored as raw undecoded bytes (decode happens at consume time)"
    );
    assert_eq!(land.vertex_colors.as_deref(), Some(&vclr[..3267]));
}

#[test]
fn vnml_and_vclr_absent_when_no_sub_record_authored() {
    let land = parse_synthetic_land(&[(b"VHGT", flat_vhgt())]);
    assert!(land.normals.is_none());
    assert!(land.vertex_colors.is_none());
}

/// BTXT assigns the quadrant's base texture; the quadrant byte selects
/// which of the 4 quadrants (0=SW, 1=SE, 2=NW, 3=NE) it lands in.
#[test]
fn btxt_assigns_base_texture_to_its_quadrant() {
    let mut sw = Vec::new();
    sw.extend_from_slice(&0xAAAAu32.to_le_bytes());
    sw.push(0); // quadrant 0 = SW
    sw.extend_from_slice(&[0u8; 3]);
    let mut ne = Vec::new();
    ne.extend_from_slice(&0xBBBBu32.to_le_bytes());
    ne.push(3); // quadrant 3 = NE
    ne.extend_from_slice(&[0u8; 3]);

    let land = parse_synthetic_land(&[(b"BTXT", sw), (b"BTXT", ne)]);
    assert_eq!(land.quadrants[0].base, Some(0xAAAA));
    assert_eq!(land.quadrants[3].base, Some(0xBBBB));
    assert_eq!(land.quadrants[1].base, None);
    assert_eq!(land.quadrants[2].base, None);
}

/// ATXT opens a layer section; the immediately-following VTXT carries that
/// layer's sparse per-vertex alpha (position(u16) + unused(u16) +
/// opacity(f32) rows). A VTXT with no preceding ATXT is dropped — there's
/// no layer for it to attach to.
#[test]
fn atxt_then_vtxt_produces_one_alpha_layer_at_the_right_position() {
    let mut atxt = Vec::new();
    atxt.extend_from_slice(&0xCCCCu32.to_le_bytes()); // LTEX form
    atxt.push(1); // quadrant 1 = SE
    atxt.push(0); // unused
    atxt.extend_from_slice(&2u16.to_le_bytes()); // layer index

    let mut vtxt = Vec::new();
    vtxt.extend_from_slice(&5u16.to_le_bytes()); // position = 5
    vtxt.extend_from_slice(&0u16.to_le_bytes()); // unused
    vtxt.extend_from_slice(&0.75f32.to_le_bytes()); // opacity

    let land = parse_synthetic_land(&[(b"ATXT", atxt), (b"VTXT", vtxt)]);
    let layers = &land.quadrants[1].layers;
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].ltex_form_id, 0xCCCC);
    assert_eq!(layers[0].layer, 2);
    let alpha = layers[0].alpha.as_ref().expect("VTXT must populate alpha");
    assert_eq!(alpha.len(), 17 * 17);
    assert_eq!(alpha[5], 0.75);
    assert!(
        alpha.iter().enumerate().all(|(i, &a)| i == 5 || a == 0.0),
        "only the authored sparse position should be non-zero"
    );
}

#[test]
fn vtxt_without_a_preceding_atxt_is_dropped() {
    let mut vtxt = Vec::new();
    vtxt.extend_from_slice(&0u16.to_le_bytes());
    vtxt.extend_from_slice(&0u16.to_le_bytes());
    vtxt.extend_from_slice(&1.0f32.to_le_bytes());

    let land = parse_synthetic_land(&[(b"VTXT", vtxt)]);
    assert!(
        land.quadrants.iter().all(|q| q.layers.is_empty()),
        "an orphan VTXT (no preceding ATXT) must not fabricate a layer"
    );
}

/// Two ATXT/VTXT pairs in the same quadrant append as two distinct layers
/// in authored order, not overwrite each other.
#[test]
fn multiple_atxt_vtxt_pairs_append_as_distinct_layers() {
    let mut atxt1 = Vec::new();
    atxt1.extend_from_slice(&0x1111u32.to_le_bytes());
    atxt1.push(2); // NW
    atxt1.push(0);
    atxt1.extend_from_slice(&0u16.to_le_bytes());
    let mut vtxt1 = Vec::new();
    vtxt1.extend_from_slice(&0u16.to_le_bytes());
    vtxt1.extend_from_slice(&0u16.to_le_bytes());
    vtxt1.extend_from_slice(&0.5f32.to_le_bytes());

    let mut atxt2 = Vec::new();
    atxt2.extend_from_slice(&0x2222u32.to_le_bytes());
    atxt2.push(2); // NW
    atxt2.push(0);
    atxt2.extend_from_slice(&1u16.to_le_bytes());
    let mut vtxt2 = Vec::new();
    vtxt2.extend_from_slice(&1u16.to_le_bytes());
    vtxt2.extend_from_slice(&0u16.to_le_bytes());
    vtxt2.extend_from_slice(&0.25f32.to_le_bytes());

    let land = parse_synthetic_land(&[
        (b"ATXT", atxt1),
        (b"VTXT", vtxt1),
        (b"ATXT", atxt2),
        (b"VTXT", vtxt2),
    ]);
    let layers = &land.quadrants[2].layers;
    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].ltex_form_id, 0x1111);
    assert_eq!(layers[1].ltex_form_id, 0x2222);
}
