use super::*;
use byroredux_core::character::CharacterRulesProfile;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{AvifRecord, GameSetting, SettingValue};

fn index(game: GameKind) -> EsmIndex {
    // The parser selects the profile from the game (#5921 reads the
    // over-encumbrance rule off it).
    let character_rules = match game {
        GameKind::Fallout3NV => CharacterRulesProfile::FALLOUT_NEW_VEGAS,
        GameKind::Skyrim => CharacterRulesProfile::SKYRIM,
        GameKind::Fallout4 => CharacterRulesProfile::FALLOUT4,
        _ => unreachable!("not used by these tests"),
    };
    let mut index = EsmIndex {
        game,
        character_rules,
        ..Default::default()
    };
    for (form_id, editor_id) in [(0x2D1, "CarryWeight"), (0x2D2, "InventoryWeight")] {
        index.actor_values.insert(
            form_id,
            AvifRecord {
                form_id,
                editor_id: editor_id.into(),
                ..Default::default()
            },
        );
    }
    index
}

/// The rules need CarryWeight; the movement effect is per game.
#[test]
fn rules_install_per_game() {
    let mut world = World::new();
    assert!(!install_encumbrance_rules(&mut world, &EsmIndex::default()), "no AVIF");
    assert!(world.try_resource::<EncumbranceRules>().is_none());

    assert!(install_encumbrance_rules(&mut world, &index(GameKind::Fallout3NV)));
    assert_eq!(
        *world.resource::<EncumbranceRules>(),
        EncumbranceRules {
            carry_weight: 0x2D1,
            inventory_weight: Some(0x2D2),
            effect: Some(OverEncumbered::SpeedMult(0.4)),
        }
    );
    let mut authored = index(GameKind::Fallout3NV);
    authored.game_settings.insert(
        0x10,
        GameSetting {
            form_id: 0x10,
            editor_id: "fMoveEncumEffect".into(),
            value: SettingValue::Float(0.5),
        },
    );
    install_encumbrance_rules(&mut world, &authored);
    assert_eq!(
        world.resource::<EncumbranceRules>().effect,
        Some(OverEncumbered::SpeedMult(0.5))
    );
    for invalid in [f32::NAN, f32::INFINITY, -0.25] {
        authored.game_settings.get_mut(&0x10).unwrap().value = SettingValue::Float(invalid);
        install_encumbrance_rules(&mut world, &authored);
        assert_eq!(
            world.resource::<EncumbranceRules>().effect,
            Some(OverEncumbered::SpeedMult(FALLOUT_MOVE_ENCUM_EFFECT)),
            "invalid fMoveEncumEffect {invalid:?} falls back to the engine default"
        );
    }
    authored.game_settings.get_mut(&0x10).unwrap().value = SettingValue::Float(4.0);
    install_encumbrance_rules(&mut world, &authored);
    assert_eq!(
        world.resource::<EncumbranceRules>().effect,
        Some(OverEncumbered::SpeedMult(1.0)),
        "over-encumbrance cannot increase movement speed"
    );
    install_encumbrance_rules(&mut world, &index(GameKind::Skyrim));
    assert_eq!(world.resource::<EncumbranceRules>().effect, Some(OverEncumbered::NoSprint));
    install_encumbrance_rules(&mut world, &index(GameKind::Fallout4));
    assert_eq!(world.resource::<EncumbranceRules>().effect, None);
}

/// Over-encumbered is strictly more than the capacity.
#[test]
fn movement_follows_the_effect_once_over() {
    let at = |weight, effect| {
        PlayerEncumbrance {
            weight,
            capacity: 100.0,
            effect,
        }
        .movement()
    };
    let slow = Some(OverEncumbered::SpeedMult(0.4));
    assert_eq!(at(100.0, slow), (1.0, true), "at capacity");
    assert_eq!(at(100.5, slow), (0.4, true));
    assert_eq!(at(150.0, Some(OverEncumbered::SpeedMult(f32::NAN))), (0.4, true));
    assert_eq!(at(150.0, Some(OverEncumbered::SpeedMult(-2.0))), (0.4, true));
    assert_eq!(at(150.0, Some(OverEncumbered::SpeedMult(3.0))), (1.0, true));
    assert_eq!(at(150.0, Some(OverEncumbered::NoSprint)), (1.0, false));
    assert_eq!(at(150.0, None), (1.0, true));
}
