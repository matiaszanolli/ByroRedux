//! `GRAS` parser regression tests (#3807).
//!
//! The `DATA` fixtures below are **verbatim bytes from vanilla records**,
//! not hand-built payloads, so a field-order mistake fails here rather
//! than surviving to the real-data gate.

use super::*;
use crate::esm::records::tree::ObjectBounds;

fn mk_sub(code: &[u8; 4], data: Vec<u8>) -> SubRecord {
    SubRecord {
        sub_type: *code,
        data,
    }
}

fn zstring(code: &[u8; 4], s: &str) -> SubRecord {
    let mut z = s.as_bytes().to_vec();
    z.push(0);
    mk_sub(code, z)
}

fn obnd(min: [i16; 3], max: [i16; 3]) -> SubRecord {
    let mut d = Vec::with_capacity(ObjectBounds::WIRE_SIZE);
    for v in min.iter().chain(max.iter()) {
        d.extend_from_slice(&v.to_le_bytes());
    }
    mk_sub(b"OBND", d)
}

/// Oblivion `BWCattail01` (0x000984C8) `DATA`, byte for byte.
///
/// density 50, slope 0..45, distance-from-water 0, application 0,
/// position_range 32.0, height_range 0.2, colour_range 0.5,
/// wave_period 10.0, flags 0x02. Note the `+3` padding byte is `0x01`
/// and the `+30` padding word is non-zero — this record is exactly why
/// those offsets are skipped rather than asserted zero.
fn cattail_data() -> SubRecord {
    let mut d = Vec::with_capacity(GRAS_DATA_LEN);
    d.push(50); // density
    d.push(0); // min_slope
    d.push(45); // max_slope
    d.push(1); // +3 padding (uninitialised in the real record)
    d.extend_from_slice(&0u16.to_le_bytes()); // distance_from_water
    d.extend_from_slice(&426u16.to_le_bytes()); // +6 padding (uninitialised)
    d.extend_from_slice(&0u32.to_le_bytes()); // water_distance_application
    d.extend_from_slice(&32.0f32.to_le_bytes()); // position_range
    d.extend_from_slice(&0.2f32.to_le_bytes()); // height_range
    d.extend_from_slice(&0.5f32.to_le_bytes()); // colour_range
    d.extend_from_slice(&10.0f32.to_le_bytes()); // wave_period
    d.push(GRAS_FLAG_UNIFORM_SCALING); // flags
    d.push(77); // +29 padding (uninitialised)
    d.extend_from_slice(&156u16.to_le_bytes()); // +30 padding (uninitialised)
    assert_eq!(d.len(), GRAS_DATA_LEN);
    mk_sub(b"DATA", d)
}

#[test]
fn parse_gras_decodes_every_data_field_in_order() {
    let rec = parse_gras(
        0x0009_84C8,
        &[
            zstring(b"EDID", "BWCattail01"),
            zstring(b"MODL", r"Plants\BWCattail01.NIF"),
            mk_sub(b"MODB", 127.964_78f32.to_le_bytes().to_vec()),
            cattail_data(),
        ],
    );

    assert_eq!(rec.form_id, 0x0009_84C8);
    assert_eq!(rec.editor_id, "BWCattail01");
    assert_eq!(rec.model_path, r"Plants\BWCattail01.NIF");
    assert!(rec.has_data);
    assert_eq!(rec.density, 50);
    assert_eq!(rec.min_slope, 0);
    assert_eq!(rec.max_slope, 45);
    assert_eq!(rec.distance_from_water, 0);
    assert_eq!(rec.water_distance_application, 0);
    assert_eq!(rec.position_range, 32.0);
    assert_eq!(rec.height_range, 0.2);
    assert_eq!(rec.colour_range, 0.5);
    assert_eq!(rec.wave_period, 10.0);
    assert_eq!(rec.flags, GRAS_FLAG_UNIFORM_SCALING);
    assert!(rec.scales_uniformly());
    assert!(!rec.fits_to_slope());
}

/// The padding at `+3`, `+6`, `+29` and `+30` holds uninitialised memory.
/// If any of it were folded into a real field, changing it would move a
/// decoded value — this pins that it cannot.
#[test]
fn parse_gras_ignores_the_uninitialised_padding_bytes() {
    let clean = parse_gras(1, &[cattail_data()]);

    let mut poisoned = cattail_data();
    for offset in [3usize, 6, 7, 29, 30, 31] {
        poisoned.data[offset] = 0xFF;
    }
    let poisoned = parse_gras(1, &[poisoned]);

    assert_eq!(
        clean, poisoned,
        "padding bytes leaked into a decoded field: the DATA field offsets are wrong"
    );
}

/// FO3/FNV `GrassWasteland06` (0x00061EB1) — `OBND` z extent is the height.
#[test]
fn nominal_height_prefers_the_obnd_z_extent() {
    let rec = parse_gras(
        0x0006_1EB1,
        &[
            zstring(b"EDID", "GrassWasteland06"),
            obnd([-34, -31, 0], [32, 36, 70]),
        ],
    );
    assert_eq!(rec.nominal_height(), Some(70.0));
}

/// Oblivion has no `OBND` at all — `MODB`'s bounding radius is the only
/// size signal its 108 records carry.
#[test]
fn nominal_height_falls_back_to_the_oblivion_bound_radius() {
    let rec = parse_gras(
        0x0009_84C8,
        &[mk_sub(b"MODB", 127.964_78f32.to_le_bytes().to_vec())],
    );
    assert!(rec.bounds.is_none());
    assert_eq!(rec.nominal_height(), Some(127.964_78));
}

/// 15 of FNV's 24 records ship an all-zero `OBND` and no `MODB`. That is
/// "never re-bounded", not "zero-height", so the record must report no
/// usable height rather than a zero a consumer would scale by.
#[test]
fn nominal_height_is_none_when_bounds_are_unset_and_no_radius() {
    let rec = parse_gras(1, &[obnd([0, 0, 0], [0, 0, 0]), cattail_data()]);
    assert!(rec.bounds.is_some_and(|b| b.is_unset()));
    assert_eq!(rec.nominal_height(), None);
}

/// An `OBND` present but degenerate falls through to `MODB` rather than
/// reporting a zero height.
#[test]
fn nominal_height_falls_through_a_degenerate_obnd_to_the_radius() {
    let rec = parse_gras(
        1,
        &[
            obnd([-4, -4, 10], [4, 4, 10]),
            mk_sub(b"MODB", 30.0f32.to_le_bytes().to_vec()),
        ],
    );
    assert_eq!(rec.nominal_height(), Some(30.0));
}

/// A short `DATA` is rejected whole. Partially decoding it would read
/// every field after the truncation point from a guessed offset.
#[test]
fn parse_gras_rejects_a_data_payload_of_the_wrong_length() {
    let mut short = cattail_data();
    short.data.truncate(GRAS_DATA_LEN - 1);
    let rec = parse_gras(1, &[zstring(b"EDID", "Truncated"), short]);

    assert!(!rec.has_data, "a 31-byte DATA must not report decoded data");
    assert_eq!(rec.density, 0);
    assert_eq!(rec.wave_period, 0.0);
    // The non-DATA fields still decode — the record is not discarded.
    assert_eq!(rec.editor_id, "Truncated");
}

/// A record with no `DATA` at all still yields its identity fields, and
/// says so via `has_data` rather than reporting authored zeroes.
#[test]
fn parse_gras_without_data_reports_has_data_false() {
    let rec = parse_gras(7, &[zstring(b"EDID", "NoData"), zstring(b"MODL", "a.nif")]);
    assert!(!rec.has_data);
    assert_eq!(rec.editor_id, "NoData");
    assert_eq!(rec.model_path, "a.nif");
    assert_eq!(rec.nominal_height(), None);
}
