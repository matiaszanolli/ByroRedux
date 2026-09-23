use super::*;

fn scene_plugin(hedr_version: f32) -> Vec<u8> {
    let mut hedr = hedr_version.to_le_bytes().to_vec();
    hedr.extend_from_slice(&[0; 8]);
    let mut data = build_record(b"TES4", 0, &[(b"HEDR", hedr)]);
    let mut lamp = vec![0u8; 48];
    lamp[4..8].copy_from_slice(&5.0f32.to_le_bytes());
    lamp[8..12].copy_from_slice(&[200, 150, 100, 0]);
    lamp[16..20].copy_from_slice(&2.0f32.to_le_bytes());
    lamp[20..24].copy_from_slice(&90.0f32.to_le_bytes());
    lamp[28..32].copy_from_slice(&1.5f32.to_le_bytes());
    lamp[32..36].copy_from_slice(&0.25f32.to_le_bytes());
    lamp[36..40].copy_from_slice(&0.5f32.to_le_bytes());
    data.extend(wrap_group(
        b"LIGH",
        &build_record(
            b"LIGH",
            10,
            &[(b"EDID", b"lamp\0".to_vec()), (b"DAT2", lamp)],
        ),
    ));
    let pose: Vec<u8> = [2.0f32, 3.0, 4.0, 0.1, 0.2, 0.3]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let mut teleport = 30u32.to_le_bytes().to_vec();
    teleport.extend(&pose);
    let refr = build_record(
        b"REFR",
        20,
        &[
            (b"NAME", 10u32.to_le_bytes().to_vec()),
            (b"DATA", pose),
            (b"XSCL", 2.0f32.to_le_bytes().to_vec()),
            (b"XTEL", teleport),
            (b"XRDS", 8.0f32.to_le_bytes().to_vec()),
        ],
    );
    let mut cell = build_record(
        b"CELL",
        1,
        &[(b"EDID", b"UnitRoom\0".to_vec()), (b"DATA", vec![1, 0])],
    );
    cell.extend(wrap_sub_group(
        1u32.to_le_bytes(),
        6,
        &wrap_sub_group(1u32.to_le_bytes(), 9, &refr),
    ));
    data.extend(wrap_group(b"CELL", &cell));
    data
}

#[test]
fn starfield_public_index_converts_placements_and_lights_together() {
    let bytes = scene_plugin(0.96);
    let index = parse_esm(&bytes).unwrap();
    assert_eq!(index.game, GameKind::Starfield);
    let refr = &index.cells.cells["unitroom"].references[0];
    assert_eq!(refr.position, [140.0, 210.0, 280.0]);
    assert_eq!(refr.teleport.as_ref().unwrap().position, refr.position);
    assert_eq!(refr.radius_override, Some(560.0));
    assert_eq!(refr.scale, 2.0);
    assert_eq!(refr.rotation, [0.1, 0.2, 0.3]);
    let light = index.cells.statics[&10].light_data.as_ref().unwrap();
    assert_eq!(light.radius, 350.0);
    assert_eq!(light.movement_amplitude, 35.0);
    assert_eq!(light.period_secs, 1.5);
    assert_eq!(light.intensity_amplitude, 0.25);
    assert_eq!(light.fov_degrees, 90.0);
    let cell_only = crate::esm::cell::parse_esm_cells(&bytes).unwrap();
    assert_eq!(
        cell_only.cells["unitroom"].references[0].position,
        refr.position
    );
    // Load-order remapping must not skip or repeat the unit conversion.
    let remapped =
        parse_esm_with_load_order(&bytes, Some(FormIdRemap::regular(3, vec![]))).unwrap();
    assert_eq!(
        remapped.cells.cells["unitroom"].references[0].position,
        refr.position
    );
    assert_eq!(
        remapped.cells.statics[&0x0300_000a]
            .light_data
            .as_ref()
            .unwrap()
            .radius,
        350.0
    );
}

#[test]
fn legacy_index_placements_keep_authored_units() {
    for hedr in [0.94, 1.34, 1.71, 1.0, 279.0] {
        let index = parse_esm(&scene_plugin(hedr)).unwrap();
        let refr = &index.cells.cells["unitroom"].references[0];
        assert_eq!(refr.position, [2.0, 3.0, 4.0]);
        assert_eq!(refr.radius_override, Some(8.0));
        assert_eq!(refr.scale, 2.0);
    }
}
