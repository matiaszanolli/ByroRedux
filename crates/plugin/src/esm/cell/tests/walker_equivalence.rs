//! TD2-2026-10-05-01 / #5309 — the interior and exterior CELL walkers
//! decode walker-shared sub-records through one
//! `CellSubrecordFields::absorb`. This test feeds the *same* sub-record
//! list through both walkers and pins field-by-field equivalence, so a
//! future arm that lands on only one side (the #1220 failure mode)
//! fails here.

use super::super::walkers::parse_cell_group;
use super::wrld::{
    build_cell_record, build_world_children_group, build_wrld_group, build_wrld_record,
    parse_synthetic_wrld,
};
use crate::esm::reader::{EsmReader, EsmVariant, GameKind};
use std::collections::HashMap;

/// The sub-record payloads both walkers receive — identical bytes, so
/// any decode divergence between the two paths is a bug, not data.
fn shared_subs() -> Vec<(&'static [u8; 4], Vec<u8>)> {
    let form = |id: u32| id.to_le_bytes().to_vec();
    let mut forms3 = Vec::new();
    forms3.extend(form(0x0000_0201));
    forms3.extend(form(0x0000_0202));
    forms3.extend(form(0x0000_0203));
    // XCRI: mesh_count=2, ref_count=1, 2 hashes + 1 visibility-group ref.
    let mut xcri = Vec::new();
    xcri.extend(form(2));
    xcri.extend(form(1));
    xcri.extend(form(0x0000_AAAA));
    xcri.extend(form(0x0000_BBBB));
    xcri.extend(form(0x0000_CCCC));
    let mut xpri = Vec::new();
    xpri.extend(form(0x0000_0301));
    xpri.extend(form(0x0000_0302));
    let mut xclr = Vec::new();
    xclr.extend(form(0x0000_0201));
    xclr.extend(form(0x0000_0202));
    vec![
        (b"FULL", b"The Same Name\0".to_vec()),
        (b"XCLW", 10.0_f32.to_le_bytes().to_vec()),
        (b"XCIM", form(0x0000_0101)),
        (b"XCWT", form(0x0000_0102)),
        (b"XCAS", form(0x0000_0103)),
        (b"XCMO", form(0x0000_0104)),
        (b"XCMT", vec![0x07]),
        (b"XCCM", form(0x0000_0105)),
        (b"XLCN", form(0x0000_0106)),
        (b"XEZN", form(0x0000_0107)),
        (b"XCLR", xclr),
        (b"LTMP", form(0x0000_0108)),
        (b"XOWN", form(0x0000_0109)),
        (b"XRNK", 5_i32.to_le_bytes().to_vec()),
        (b"XGLB", form(0x0000_010A)),
        (b"RCLR", vec![0x12, 0x34, 0x56]),
        (b"XCRI", xcri),
        (b"XPRI", xpri),
    ]
}

#[test]
fn interior_and_exterior_walkers_decode_the_same_shared_sub_records_identically() {
    // ── Interior: a CELL record with DATA(interior) + the shared list. ──
    let mut interior_subs = vec![
        (b"EDID", b"EquivalenceCell\0".to_vec()),
        (b"DATA", vec![0x01]), // interior, no show-sky
    ];
    interior_subs.extend(shared_subs());

    let mut sub_data = Vec::new();
    for (ty, payload) in interior_subs.iter() {
        let mut s = Vec::new();
        s.extend_from_slice(*ty);
        s.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        s.extend_from_slice(payload);
        sub_data.extend_from_slice(&s);
    }
    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0xDEAD_0001_u32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    let mut reader = EsmReader::with_variant(&buf, EsmVariant::Tes5Plus);
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(&mut reader, end, &mut cells, GameKind::Fallout3NV).unwrap();
    let interior = cells
        .get("equivalencecell")
        .expect("interior CELL must be indexed");

    // ── Exterior: the same shared list behind XCLC, on a worldspace
    //    persistent CELL. ──
    let mut exterior_subs = vec![
        (b"EDID", b"EquivalenceCell\0".to_vec()),
        (
            b"XCLC",
            {
                let mut g = Vec::new();
                g.extend(3_i32.to_le_bytes());
                g.extend((-2_i32).to_le_bytes());
                g
            },
        ),
    ];
    exterior_subs.extend(shared_subs());

    let wrld = build_wrld_record(0x0000_003c, &[(b"EDID", b"EquivWorld\0".to_vec())]);
    let cell = build_cell_record(0x0000_00d7, &exterior_subs);
    let children = build_world_children_group(0x0000_003c, &cell);
    let wrld_buf = build_wrld_group(&[wrld, children]);
    let (_worldspaces, _climates, _exterior, persistent) = parse_synthetic_wrld(&wrld_buf);
    let exterior = persistent
        .get("equivworld")
        .expect("worldspace persistent CELL must be indexed");

    // ── Field-by-field equivalence over every walker-shared field. ──
    assert_eq!(interior.display_name, exterior.display_name);
    assert_eq!(interior.water_height, exterior.water_height);
    assert_eq!(
        interior.water_height_is_explicit,
        exterior.water_height_is_explicit
    );
    assert_eq!(interior.image_space_form, exterior.image_space_form);
    assert_eq!(interior.water_type_form, exterior.water_type_form);
    assert_eq!(interior.acoustic_space_form, exterior.acoustic_space_form);
    assert_eq!(interior.music_type_form, exterior.music_type_form);
    assert_eq!(interior.music_type_enum, exterior.music_type_enum);
    assert_eq!(interior.climate_override, exterior.climate_override);
    assert_eq!(interior.location_form, exterior.location_form);
    assert_eq!(interior.encounter_zone_form, exterior.encounter_zone_form);
    assert_eq!(interior.regions, exterior.regions);
    assert_eq!(
        interior.lighting_template_form,
        exterior.lighting_template_form
    );
    assert_eq!(interior.ownership, exterior.ownership);
    assert_eq!(
        interior.regional_color_override,
        exterior.regional_color_override
    );
    // The #1220 regression: the exterior walker hardcoded the precombine
    // pair empty after the interior one gained them.
    assert_eq!(
        interior.precombined_mesh_hashes,
        exterior.precombined_mesh_hashes,
        "precombine hashes must decode identically on both walkers"
    );
    assert_eq!(interior.precombined_mesh_hashes, vec![0xAAAA, 0xBBBB]);
    assert_eq!(interior.absorbed_refs, exterior.absorbed_refs);
    assert_eq!(
        interior.absorbed_refs,
        [0x0301, 0x0302].into_iter().collect::<std::collections::HashSet<_>>()
    );
    // Sanity: the shared decode actually fired (a silently-skipped list
    // would make the equivalence asserts vacuously true).
    assert_eq!(exterior.ownership.as_ref().unwrap().owner_form_id, 0x109);
    assert_eq!(exterior.water_height, Some(10.0));
    assert_eq!(exterior.music_type_enum, Some(7));
    assert_eq!(exterior.regional_color_override, Some([0x12, 0x34, 0x56]));
}
