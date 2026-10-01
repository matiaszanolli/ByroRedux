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

/// A WTHR shaped like Starfield.esm's `DefaultWeather` (local FormID
/// 0x15E): FNAM authors 10 / 3000 / 10 / 3000 — metric fog distances,
/// magnitude-matched to the metric LGTM corpus (`DefaultLightingTemplate`
/// 750 / 12000 m). Shared by the Starfield-lift and legacy-passthrough
/// tests below (#5134). `hedr_version` 1.71 (Skyrim SE) pads FNAM to the
/// 32-byte schema that `parse_wthr_skyrim` requires; the other versions
/// take the 16-byte Gamebryo/FO3-era form.
fn weather_plugin(hedr_version: f32) -> Vec<u8> {
    let mut hedr = hedr_version.to_le_bytes().to_vec();
    hedr.extend_from_slice(&[0; 8]);
    let mut data = build_record(b"TES4", 0, &[(b"HEDR", hedr)]);
    let mut fnam: Vec<u8> = [10.0f32, 3000.0, 10.0, 3000.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    if hedr_version == 1.71 {
        // Skyrim's 32-B FNAM tail: power + max for day/night (defaults).
        fnam.extend_from_slice(&1.0f32.to_le_bytes());
        fnam.extend_from_slice(&1.0f32.to_le_bytes());
        fnam.extend_from_slice(&1.0f32.to_le_bytes());
        fnam.extend_from_slice(&1.0f32.to_le_bytes());
    }
    if hedr_version == 0.96 {
        // #5001 — Starfield's 72-byte FNAM tail: DefaultWeather's on-disk
        // powers 0.4, max 0.9, near height 10/120, density 0.05, far
        // height 10/220 day and 10/900 night.
        for value in [
            0.4f32, 0.4, 0.9, 0.9, 10.0, 120.0, 10.0, 120.0, 0.05, 0.05, 10.0, 220.0, 10.0,
            900.0,
        ] {
            fnam.extend_from_slice(&value.to_le_bytes());
        }
    }
    data.extend(wrap_group(
        b"WTHR",
        &build_record(
            b"WTHR",
            0x15E,
            &[
                (b"EDID", b"DefaultWeather\0".to_vec()),
                (b"FNAM", fnam),
            ],
        ),
    ));
    data
}

/// #5134 — Starfield WTHR fog distances must be lifted by
/// `BETHESDA_UNITS_PER_METER` in `spatial_units::normalize`, exactly like
/// the XCLL / LGTM fog above. Pre-fix, `translate_weather` read the raw
/// metres as engine units and `fit_legacy_fog_extinction` divided by 70,
/// fitting a 10 / 3000 m ramp as 0.14 / 42.9 m — every Starfield exterior
/// with a resolved climate fogged out at ~43 m.
#[test]
fn starfield_public_index_lifts_wthr_fog_distances() {
    let index = parse_esm(&weather_plugin(0.96)).unwrap();
    assert_eq!(index.game, GameKind::Starfield);
    let w = &index.weathers[&0x15E];
    assert_eq!(w.editor_id, "DefaultWeather");
    assert_eq!(w.fog_day_near, 700.0);
    assert_eq!(w.fog_day_far, 210_000.0);
    assert_eq!(w.fog_night_near, 700.0);
    assert_eq!(w.fog_night_far, 210_000.0);
    // #5001 — the FO4-form tail now decodes on Starfield too, and the
    // eight height mid/range fields lift with the distances while the
    // dimensionless power/max/density values stay as authored.
    assert_eq!(w.fog_day_power, 0.4);
    assert_eq!(w.fog_night_power, 0.4);
    assert_eq!(w.fog_day_max, 0.9);
    assert_eq!(w.fog_night_max, 0.9);
    let height = w
        .fog_height
        .expect("Starfield 72-byte FNAM must decode its height tail (#5001)");
    assert_eq!(height.day_near_height_mid, 700.0);
    assert_eq!(height.day_near_height_range, 8_400.0);
    assert_eq!(height.night_near_height_mid, 700.0);
    assert_eq!(height.night_near_height_range, 8_400.0);
    assert_eq!(height.day_high_density_scale, 0.05);
    assert_eq!(height.night_high_density_scale, 0.05);
    assert_eq!(height.day_far_height_mid, 700.0);
    assert_eq!(height.day_far_height_range, 15_400.0);
    assert_eq!(height.night_far_height_mid, 700.0);
    assert_eq!(height.night_far_height_range, 63_000.0);
}

/// Companion: every non-Starfield game keeps the authored FNAM values —
/// the unit lift keys on `GameKind::Starfield` exactly like the rest of
/// `normalize` (#4837 sibling rule).
#[test]
fn legacy_index_wthr_keeps_authored_units() {
    for hedr in [0.94, 1.34, 1.71, 1.0, 279.0] {
        let index = parse_esm(&weather_plugin(hedr)).unwrap();
        let w = &index.weathers[&0x15E];
        assert_eq!(
            w.fog_day_far, 3000.0,
            "hedr {hedr}: non-Starfield WTHR fog must stay in authored units"
        );
        assert_eq!(w.fog_day_near, 10.0);
        assert_eq!(w.fog_night_near, 10.0);
        assert_eq!(w.fog_night_far, 3000.0);
    }
}
/// A WATR carrying Starfield.esm `WaterClear` (0x18)'s metric DNAM: depth
/// 8 m, per-metre absorption 0.16558 / 0.096239 / 0.076271, underwater fog
/// -150 / 75 m, noise UV tiles 72.1141 / 39 / 13, noise falloff 100 m.
/// FO76 shares the decoder but authors engine units (#5151).
fn water_plugin(hedr_version: f32) -> Vec<u8> {
    let mut hedr = hedr_version.to_le_bytes().to_vec();
    hedr.extend_from_slice(&[0; 8]);
    let mut data = build_record(b"TES4", 0, &[(b"HEDR", hedr)]);
    let mut dnam = vec![0u8; 160];
    for (offset, value) in [
        (0, 8.0f32),
        (4, 0.16558),
        (8, 0.096239),
        (12, 0.076271),
        (40, -150.0),
        (44, 75.0),
        (120, 72.1141),
        (124, 39.0),
        (128, 13.0),
        (132, 100.0),
    ] {
        dnam[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    data.extend(wrap_group(
        b"WATR",
        &build_record(
            b"WATR",
            0x18,
            &[(b"EDID", b"WaterClear\0".to_vec()), (b"DNAM", dnam)],
        ),
    ));
    data
}

/// #5151 — Starfield WATR DNAM lengths are lifted ×70 and its per-metre
/// absorption ÷70 in `spatial_units::normalize`. Pre-fix red transmission
/// through 1 m of clear water was exp(-70·0.16558) ≈ 9e-6, normals went
/// flat 1.4 m from the camera and underwater fog saturated at ~1 m.
#[test]
fn starfield_public_index_lifts_watr_dnam_distances() {
    let index = parse_esm(&water_plugin(0.96)).unwrap();
    assert_eq!(index.game, GameKind::Starfield);
    let p = &index.waters[&0x18].params;
    assert_eq!(p.depth_amount, 8.0 * 70.0);
    for (actual, metric) in p
        .absorption_coefficients
        .into_iter()
        .zip([0.16558f32, 0.096239, 0.076271])
    {
        assert!((actual - metric / 70.0).abs() < 1.0e-9);
    }
    assert_eq!(p.underwater_fog_near, 0.0);
    assert_eq!(p.underwater_fog_far, 75.0 * 70.0);
    assert_eq!(p.noise_falloff, 100.0 * 70.0);
}

/// Companion: FO76 (same DNAM decoder, engine-unit data) is never lifted.
#[test]
fn fo76_index_watr_keeps_authored_units() {
    let index = parse_esm(&water_plugin(279.0)).unwrap();
    assert_eq!(index.game, GameKind::Fallout76);
    let p = &index.waters[&0x18].params;
    assert_eq!(p.depth_amount, 8.0);
    assert_eq!(p.absorption_coefficients, [0.16558, 0.096239, 0.076271]);
    assert_eq!(p.underwater_fog_far, 75.0);
    assert_eq!(p.noise_falloff, 100.0);
}
