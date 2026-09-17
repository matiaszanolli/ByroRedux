use super::*;
use crate::esm::records::common::LocalizedPluginGuard;
use crate::esm::records::test_support::sub;

#[test]
fn legacy_art_tip_locations_and_screen_type_survive_remapping() {
    let mut location = Vec::new();
    location.extend_from_slice(&0x0100_1234u32.to_le_bytes());
    location.extend_from_slice(&0x000d_a726u32.to_le_bytes());
    location.extend_from_slice(&(-2i16).to_le_bytes());
    location.extend_from_slice(&(-17i16).to_le_bytes());
    let subs = [
        sub(b"EDID", b"TownScreen\0"),
        sub(b"ICON", b"interface\\loading\\town.dds\0"),
        sub(b"DESC", b"An authored tip.\0"),
        sub(b"LNAM", &location),
        sub(b"LNAM", [0; 12]),
        sub(b"WMI1", 0x0100_2222u32.to_le_bytes()),
    ];
    let remap = Some(FormIdRemap::regular(5, vec![2]));
    let record = parse_lscr(0x0500_0123, 0x400, &subs, GameKind::Fallout3NV, &remap);
    assert_eq!(record.form_id, 0x0500_0123);
    assert_eq!(record.flags, 0x400);
    assert_eq!(record.icon, "interface\\loading\\town.dds");
    assert_eq!(record.description, "An authored tip.");
    assert_eq!(record.screen_type, 0x0500_2222);
    assert_eq!(
        record.locations[0],
        LoadScreenLocation {
            direct: 0x0500_1234,
            world: 0x020d_a726,
            grid_x: -17,
            grid_y: -2,
        }
    );
    assert_eq!(record.locations[1].direct, 0);
    assert_eq!(record.locations[1].world, 0);
    assert!(record.malformed_fields.is_empty());
    let oblivion = parse_lscr(1, 0, &subs, GameKind::Oblivion, &None);
    assert_eq!(oblivion.locations.len(), 2);
    assert_eq!(
        oblivion.screen_type, 0,
        "FNV-only WMI1 must not leak into TES4"
    );
}

#[test]
fn skyrim_model_transforms_and_conditions_are_not_legacy_images() {
    let mut condition = vec![0; 32];
    condition[0] = 4; // global comparand
    condition[4..8].copy_from_slice(&0x0100_5555u32.to_le_bytes());
    let subs = [
        sub(b"NNAM", 0x0100_1234u32.to_le_bytes()),
        sub(b"SNAM", 2.0f32.to_le_bytes()),
        sub(b"RNAM", [1, 0, 254, 255, 3, 0]),
        sub(b"ONAM", [76, 255, 180, 0]),
        sub(
            b"XNAM",
            [1.0f32, 2.0, 3.0]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
        sub(b"MOD2", b"camera\\orbit.nif\0"),
        sub(b"CTDA", &condition),
        sub(b"ICON", b"not-a-skyrim-field.dds\0"),
    ];
    let record = parse_lscr(
        1,
        0,
        &subs,
        GameKind::Skyrim,
        &Some(FormIdRemap::regular(7, vec![0])),
    );
    assert_eq!(record.model, 0x0700_1234);
    assert_eq!(record.initial_scale, Some(2.0));
    assert_eq!(record.initial_rotation, Some([1, -2, 3]));
    assert_eq!(record.rotation_bounds, Some([-180, 180]));
    assert_eq!(record.initial_translation, Some([1.0, 2.0, 3.0]));
    assert_eq!(record.camera_path, "camera\\orbit.nif");
    assert_eq!(record.conditions.len(), 1);
    assert_eq!(
        record.conditions[0].comparand,
        crate::esm::records::condition::ConditionValue::Global(0x0700_5555)
    );
    assert!(record.icon.is_empty());
    assert!(record.malformed_fields.is_empty());
}

#[test]
fn later_games_keep_transform_and_zoom_without_skyrim_scale() {
    for game in [GameKind::Fallout4, GameKind::Starfield] {
        let record = parse_lscr(
            1,
            0x8400,
            &[
                sub(b"TNAM", 0x0100_0011u32.to_le_bytes()),
                sub(
                    b"ZNAM",
                    [-1.0f32, 1.0]
                        .into_iter()
                        .flat_map(f32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ),
                sub(b"SNAM", 3.0f32.to_le_bytes()),
                sub(b"ICON", b"loadscreens\\test.dds\0"),
            ],
            game,
            &Some(FormIdRemap::regular(4, vec![0])),
        );
        assert_eq!(record.flags, 0x8400);
        assert_eq!(record.transform, 0x0400_0011);
        assert_eq!(record.zoom_bounds, Some([-1.0, 1.0]));
        assert_eq!(record.initial_scale, None);
        assert_eq!(!record.icon.is_empty(), game == GameKind::Starfield);
    }
}

#[test]
fn malformed_restrictions_are_not_silently_made_unrestricted() {
    let record = parse_lscr(
        1,
        0,
        &[
            sub(b"LNAM", [0; 11]),
            sub(b"WMI1", [0; 3]),
            sub(b"CTDA", [0; 2]),
        ],
        GameKind::Fallout3NV,
        &None,
    );
    assert_eq!(record.malformed_fields, [*b"LNAM", *b"WMI1", *b"CTDA"]);
    assert!(record.locations.is_empty());
    assert!(record.conditions.is_empty());
}

#[test]
fn non_finite_transforms_are_flagged() {
    let record = parse_lscr(
        1,
        0,
        &[sub(b"SNAM", f32::NAN.to_le_bytes())],
        GameKind::Skyrim,
        &None,
    );
    assert_eq!(record.initial_scale, None);
    assert_eq!(record.malformed_fields, [*b"SNAM"]);
}

#[test]
fn localized_tip_uses_the_existing_lstring_path() {
    let _guard = LocalizedPluginGuard::new(true);
    let record = parse_lscr(
        1,
        0,
        &[sub(b"DESC", 0x1234u32.to_le_bytes())],
        GameKind::Skyrim,
        &None,
    );
    assert_eq!(record.description, "<lstring 0x00001234>");
}

#[test]
fn localized_tip_resolves_from_companion_dlstrings() {
    use crate::esm::records::common::StringsTableGuard;
    use crate::esm::strings_table::{StringTableSet, StringsTable};
    let text = b"Localized loading tip\0";
    let mut table = Vec::new();
    for word in [1u32, text.len() as u32 + 4, 0x1234, 0, text.len() as u32] {
        table.extend_from_slice(&word.to_le_bytes());
    }
    table.extend_from_slice(text);
    let _localized = LocalizedPluginGuard::new(true);
    let _strings = StringsTableGuard::new(StringTableSet {
        dlstrings: Some(StringsTable::parse(&table, true).unwrap()),
        ..Default::default()
    });
    let record = parse_lscr(
        1,
        0,
        &[sub(b"DESC", 0x1234u32.to_le_bytes())],
        GameKind::Skyrim,
        &None,
    );
    assert_eq!(record.description, "Localized loading tip");
}

#[test]
fn every_truncated_fixed_field_is_safe_and_visible() {
    for (game, tag, len) in [
        (GameKind::Fallout3NV, *b"LNAM", 12),
        (GameKind::Fallout3NV, *b"WMI1", 4),
        (GameKind::Skyrim, *b"NNAM", 4),
        (GameKind::Skyrim, *b"SNAM", 4),
        (GameKind::Skyrim, *b"RNAM", 6),
        (GameKind::Skyrim, *b"ONAM", 4),
        (GameKind::Skyrim, *b"XNAM", 12),
        (GameKind::Fallout4, *b"TNAM", 4),
        (GameKind::Starfield, *b"ZNAM", 8),
    ] {
        for size in 0..len {
            let record = parse_lscr(1, 0, &[sub(&tag, vec![0; size])], game, &None);
            assert_eq!(record.malformed_fields, [tag]);
        }
    }
}
