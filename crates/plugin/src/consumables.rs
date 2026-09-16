//! Resolve supported base restorative effects into canonical AVIF changes.
//! Never drop an unsupported effect from a mixed potion and consume the rest.
use crate::esm::reader::GameKind;
use crate::esm::records::condition::{ConditionList, ConditionValue, RunOn};
use crate::esm::records::{EsmIndex, ItemKind};

/// GECK body-condition actor values (base 100), not display limb names.
pub const BODY_CONDITION_VALUES: [&str; 7] = [
    "PerceptionCondition",
    "EnduranceCondition",
    "LeftAttackCondition",
    "RightAttackCondition",
    "LeftMobilityCondition",
    "RightMobilityCondition",
    "BrainCondition",
];

#[derive(Debug, Clone, Copy)]
pub struct MedicineScaling {
    pub actor_value: u32,
    pub base: f32,
    pub multiplier: f32,
}

/// An executable restoration with a checked condition list.
#[derive(Debug, Clone)]
pub struct ConditionalRestoration {
    pub actor_value: u32,
    pub magnitude: f32,
    /// Zero applies once; otherwise magnitude is restoration per second.
    pub duration: u32,
    pub conditions: ConditionList,
    pub medicine: Option<MedicineScaling>,
}

/// Compile a conditional consumable only when *every* branch is understood.
/// The general evaluator's unknown-function zero fallback is not safe for
/// selecting effects (`unknown == 0` would incorrectly activate a branch).
/// This capability check intentionally precedes any condition short-circuit.
pub fn conditional_restorations(
    index: &EsmIndex,
    form_id: u32,
) -> Option<Vec<ConditionalRestoration>> {
    let plan = restoration_plan(index, form_id)?;
    (plan.iter().all(|e| e.duration == 0) && plan.iter().any(|e| !e.conditions.is_empty()))
        .then_some(plan)
}

/// Validate the entire authored chain before exposing consumption to gameplay.
/// Non-recover Value Modifiers restore magnitude per second over their duration.
pub fn restoration_plan(index: &EsmIndex, form_id: u32) -> Option<Vec<ConditionalRestoration>> {
    let has_perk = match index.game {
        GameKind::Skyrim => 448,
        GameKind::Fallout3NV => 449,
        _ => return None,
    };
    let ItemKind::Aid {
        authored_effects: Some(effects),
        simple_consumption_header: true,
        medicine,
        ..
    } = &index.items.get(&form_id)?.kind
    else {
        return None;
    };
    if effects.is_empty() {
        return None;
    }
    let medicine = if *medicine {
        let base = index
            .game_setting_float("fMagicMedicineSkillBase")
            .unwrap_or(1.0);
        let multiplier = index
            .game_setting_float("fMagicMedicineSkillMult")
            .unwrap_or(2.0);
        if !base.is_finite() || !multiplier.is_finite() || base < 0.0 || multiplier < 0.0 {
            return None;
        }
        Some(MedicineScaling {
            actor_value: index.actor_value_form_id("Medicine")?,
            base,
            multiplier,
        })
    } else {
        None
    };
    let mut plan = Vec::new();
    for entry in effects {
        if entry.delivery.is_some_and(|delivery| delivery != 0)
            || entry.conditions.len() != entry.condition_flags.len()
            || entry.conditions.iter().zip(&entry.condition_flags).any(|(cond, flags)| {
                !(cond.function_index == has_perk && cond.param_1 != 0
                    || (cond.function_index == 586 && cond.param_1 == 0
                        && index.character_rules == byroredux_core::character::CharacterRulesProfile::FALLOUT_NEW_VEGAS))
                    || flags & 0x1e != 0 // no global/swap-subject/alias/pack flags yet
                    || flags >> 5 > 5 // invalid comparator
                    || !matches!(cond.run_on, RunOn::Subject | RunOn::Target)
                    || cond.param_2 != 0
                    || cond.extra_data_id != 0
                    || cond.param_1_text.is_some()
                    || cond.param_2_text.is_some()
                    || !matches!(cond.comparand, ConditionValue::Literal(value) if value.is_finite())
            })
        { return None; }
        let (actor_value, magnitude) = restoration(index, &entry.effect, true)?;
        let effect = ConditionalRestoration {
            actor_value,
            magnitude,
            duration: entry.effect.duration,
            conditions: entry.conditions.clone(),
            medicine,
        };
        plan.push(effect.clone());
        if index
            .magic_effects
            .get(&entry.effect.effect_form_id)?
            .restores_body_parts
        {
            for name in BODY_CONDITION_VALUES {
                plan.push(ConditionalRestoration {
                    actor_value: index.actor_value_form_id(name)?,
                    ..effect.clone()
                });
            }
        }
    }
    Some(plan)
}

fn restoration(
    index: &EsmIndex,
    effect: &crate::esm::records::MagicEffectItem,
    allow_timed: bool,
) -> Option<(u32, f32)> {
    if (!allow_timed && effect.duration != 0)
        || effect.area != 0
        || !effect.magnitude.is_finite()
        || effect.magnitude <= 0.0
    {
        return None;
    }
    let mgef = index.magic_effects.get(&effect.effect_form_id)?;
    let av = if effect.duration == 0 {
        mgef.instant_restoration_av?
    } else {
        mgef.timed_restoration_av?
    };
    let name = match (index.game, av) {
        (GameKind::Skyrim, 24) | (GameKind::Fallout3NV, 16) => "Health",
        (GameKind::Skyrim, 25) => "Magicka",
        (GameKind::Skyrim, 26) => "Stamina",
        (GameKind::Fallout3NV, 12) => "ActionPoints",
        _ => return None,
    };
    Some((index.actor_value_form_id(name)?, effect.magnitude))
}

/// `(AVIF FormID, restoration amount)` for every effect, or None if any
/// effect, condition, script, or actor-value mapping cannot be applied.
pub fn instant_restorations(index: &EsmIndex, form_id: u32) -> Option<Vec<(u32, f32)>> {
    if !matches!(index.game, GameKind::Skyrim | GameKind::Fallout3NV) {
        return None;
    }
    let ItemKind::Aid {
        immediate_effects: Some(effects),
        medicine: false,
        ..
    } = &index.items.get(&form_id)?.kind
    else {
        return None;
    };
    if effects.is_empty() {
        return None;
    }
    effects
        .iter()
        .map(|effect| {
            if index
                .magic_effects
                .get(&effect.effect_form_id)?
                .restores_body_parts
            {
                return None;
            }
            restoration(index, effect, false)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::reader::{FormIdRemap, SubRecord};
    use crate::esm::records::test_support::sub;
    use crate::esm::records::{parse_alch_for_game, parse_mgef_for_game, AvifRecord};
    fn fixture() -> (EsmIndex, Vec<SubRecord>, Vec<SubRecord>) {
        let remap = Some(FormIdRemap::regular(2, vec![0]));
        let alch = vec![
            sub(b"ENIT", [0; 20]),
            sub(b"EFID", 0x0100_0020u32.to_le_bytes()),
            sub(
                b"EFIT",
                [25f32.to_le_bytes(), 0u32.to_le_bytes(), 0u32.to_le_bytes()].concat(),
            ),
        ];
        let mut data = vec![0; 152];
        data[68..72].copy_from_slice(&24u32.to_le_bytes());
        data[80..84].copy_from_slice(&1u32.to_le_bytes());
        let mgef = vec![sub(b"DATA", data)];
        let mut index = EsmIndex {
            game: GameKind::Skyrim,
            ..Default::default()
        };
        index
            .items
            .insert(1, parse_alch_for_game(1, &alch, GameKind::Skyrim, &remap));
        index.magic_effects.insert(
            0x0200_0020,
            parse_mgef_for_game(0x0200_0020, &mgef, GameKind::Skyrim, &remap),
        );
        index.actor_values.insert(
            30,
            AvifRecord {
                form_id: 30,
                editor_id: "AVHealth".into(),
                ..Default::default()
            },
        );
        (index, alch, mgef)
    }
    #[test]
    fn value_and_parts_requires_all_limb_mappings_and_resolves_medicine_gmsts() {
        use crate::esm::records::{GameSetting, SettingValue};
        let (mut index, mut alch, _) = fixture();
        index.game = GameKind::Fallout3NV;
        alch[0].data[4] = 4; // Medicine, not padding or Food
        alch[2].data = [30u32, 0, 0, 0, 16]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let remap = Some(FormIdRemap::regular(2, vec![0]));
        index
            .items
            .insert(1, parse_alch_for_game(1, &alch, index.game, &remap));
        let mut data = vec![0; 72];
        data[..4].copy_from_slice(&0x70u32.to_le_bytes());
        data[64..68].copy_from_slice(&34u32.to_le_bytes());
        data[68..72].copy_from_slice(&16u32.to_le_bytes());
        index.magic_effects.insert(
            0x0200_0020,
            parse_mgef_for_game(0x0200_0020, &[sub(b"DATA", data)], index.game, &remap),
        );
        index.actor_values.insert(
            77,
            AvifRecord {
                form_id: 77,
                editor_id: "Medicine".into(),
                ..Default::default()
            },
        );
        assert!(
            restoration_plan(&index, 1).is_none(),
            "must not silently drop missing limb mappings"
        );
        for (i, name) in BODY_CONDITION_VALUES.into_iter().enumerate() {
            let id = 40 + i as u32;
            index.actor_values.insert(
                id,
                AvifRecord {
                    form_id: id,
                    editor_id: name.into(),
                    ..Default::default()
                },
            );
        }
        index.game_settings.insert(
            2,
            GameSetting {
                form_id: 2,
                editor_id: "fMagicMedicineSkillBase".into(),
                value: SettingValue::Float(1.5),
            },
        );
        index.game_settings.insert(
            3,
            GameSetting {
                form_id: 3,
                editor_id: "fMagicMedicineSkillMult".into(),
                value: SettingValue::Float(3.0),
            },
        );
        let plan = restoration_plan(&index, 1).unwrap();
        assert_eq!(plan.len(), 8);
        assert_eq!(
            plan.iter().map(|e| e.actor_value).collect::<Vec<_>>(),
            vec![30, 40, 41, 42, 43, 44, 45, 46]
        );
        for effect in plan {
            let scale = effect.medicine.unwrap();
            assert_eq!(
                (scale.actor_value, scale.base, scale.multiplier),
                (77, 1.5, 3.0)
            );
        }
        assert!(
            instant_restorations(&index, 1).is_none(),
            "constant-health-only API cannot represent this item"
        );
        index.game_settings.get_mut(&3).unwrap().value = SettingValue::Float(f32::NAN);
        assert!(restoration_plan(&index, 1).is_none());
    }

    #[test]
    fn timed_plan_preserves_duration_and_rejects_unsupported_lifecycle() {
        let (mut index, mut alch, mgef) = fixture();
        let remap = Some(FormIdRemap::regular(2, vec![0]));
        alch[2].data[8..12].copy_from_slice(&3u32.to_le_bytes());
        index
            .items
            .insert(1, parse_alch_for_game(1, &alch, index.game, &remap));
        let plan = restoration_plan(&index, 1).unwrap();
        assert_eq!(
            (plan[0].actor_value, plan[0].magnitude, plan[0].duration),
            (30, 25.0, 3)
        );
        assert!(instant_restorations(&index, 1).is_none());
        for variant in 0..5 {
            let mut invalid = mgef.clone();
            match variant {
                0 => invalid[0].data[..4].copy_from_slice(&0x200u32.to_le_bytes()),
                1 => invalid[0].data[..4].copy_from_slice(&0x20000u32.to_le_bytes()),
                2 => invalid[0].data[..4].copy_from_slice(&0x100u32.to_le_bytes()),
                3 => invalid[0].data[..4].copy_from_slice(&0x10000000u32.to_le_bytes()),
                _ => invalid[0].data[56..60].copy_from_slice(&1f32.to_le_bytes()),
            }
            index.magic_effects.insert(
                0x0200_0020,
                parse_mgef_for_game(0x0200_0020, &invalid, index.game, &remap),
            );
            assert!(restoration_plan(&index, 1).is_none(), "variant {variant}");
        }
    }

    #[test]
    fn fallout_integer_effects_resolve_vitals_without_reading_padding_as_flags() {
        let (mut index, mut alch, _) = fixture();
        index.game = GameKind::Fallout3NV;
        alch[0].data[4..8].copy_from_slice(&[1, 0xcd, 0xcd, 0xcd]);
        // Integer magnitude, self delivery, stale EFIT AV cache. MGEF is
        // authoritative, as in xEdit's FO3/FNV EFIT after-load fixup.
        alch[2].data = [25u32, 0, 0, 0, u32::MAX]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let remap = Some(FormIdRemap::regular(2, vec![0]));
        index
            .items
            .insert(1, parse_alch_for_game(1, &alch, index.game, &remap));
        for (av, name) in [(16u32, "Health"), (12, "ActionPoints")] {
            let mut data = vec![0; 72];
            data[..4].copy_from_slice(&0x470u32.to_le_bytes()); // self/touch/target + FX persist
            data[22..24].copy_from_slice(&[0xcd, 0xcd]); // counter-count padding
            data[68..72].copy_from_slice(&av.to_le_bytes());
            index.magic_effects.insert(
                0x0200_0020,
                parse_mgef_for_game(
                    0x0200_0020,
                    &[sub(b"DATA", data.clone())],
                    index.game,
                    &remap,
                ),
            );
            index.actor_values.get_mut(&30).unwrap().editor_id = name.into();
            assert_eq!(instant_restorations(&index, 1), Some(vec![(30, 25.0)]));
            assert_eq!(
                index.magic_effects[&0x0200_0020].timed_restoration_av,
                Some(av)
            );
            for flag in [0x80u32, 0x10000000] {
                let mut invalid = data.clone();
                invalid[..4].copy_from_slice(&(0x470 | flag).to_le_bytes());
                let parsed = parse_mgef_for_game(1, &[sub(b"DATA", invalid)], index.game, &None);
                assert_eq!(parsed.instant_restoration_av, Some(av));
                assert!(parsed.timed_restoration_av.is_none());
            }
            for flag in [1u32, 2, 4, 0x100, 0x80000, 0x100000] {
                let mut invalid = data.clone();
                invalid[..4].copy_from_slice(&(0x470 | flag).to_le_bytes());
                assert!(
                    parse_mgef_for_game(1, &[sub(b"DATA", invalid)], index.game, &None)
                        .instant_restoration_av
                        .is_none(),
                    "flag {flag:x}"
                );
            }
            for offset in [20, 64, 68] {
                let mut invalid = data.clone();
                invalid[offset..offset + 4].copy_from_slice(&1u32.to_le_bytes());
                assert!(
                    parse_mgef_for_game(1, &[sub(b"DATA", invalid)], index.game, &None)
                        .instant_restoration_av
                        .is_none(),
                    "offset {offset}"
                );
            }
            data.truncate(71);
            assert!(
                parse_mgef_for_game(1, &[sub(b"DATA", data)], index.game, &None)
                    .instant_restoration_av
                    .is_none()
            );
        }
        for variant in 0..8 {
            let mut invalid = alch.clone();
            match variant {
                0 => invalid[2].data.truncate(12), // Skyrim shape is not Fallout
                1 => invalid[2].data[12] = 1,      // touch
                2 => invalid[2].data[8] = 1,       // timed
                3 => invalid[2].data[4] = 1,       // area
                4 => invalid[0].data[8] = 1,       // withdrawal effect
                5 => invalid[0].data[12..16].copy_from_slice(&0.1f32.to_le_bytes()),
                6 => invalid.push(sub(b"CTDA", [])),
                _ => invalid.push(sub(b"SCRI", [])),
            }
            assert!(
                matches!(
                    parse_alch_for_game(1, &invalid, index.game, &None).kind,
                    ItemKind::Aid {
                        immediate_effects: None,
                        ..
                    }
                ),
                "variant {variant}"
            );
        }
    }

    #[test]
    fn conditional_plan_preflights_every_branch_and_record_header() {
        let (mut index, alch, _) = fixture();
        let mut condition = vec![0; 32];
        condition[8..10].copy_from_slice(&448u16.to_le_bytes());
        condition[12..16].copy_from_slice(&0x0100_0030u32.to_le_bytes());
        let remap = Some(FormIdRemap::regular(2, vec![0]));
        let mut subs = alch.clone();
        subs.push(sub(b"CTDA", condition.clone()));
        index
            .items
            .insert(1, parse_alch_for_game(1, &subs, index.game, &remap));
        let plan = conditional_restorations(&index, 1).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].actor_value, 30);
        assert_eq!(plan[0].conditions[0].param_1, 0x0200_0030);
        assert!(instant_restorations(&index, 1).is_none());
        for variant in 0..14 {
            let mut invalid = subs.clone();
            match variant {
                0 => invalid[3].data[8..10].copy_from_slice(&65535u16.to_le_bytes()), // unknown == 0
                1 => invalid[3].data[0] = 0x02, // swap subject/target
                2 => invalid[3].data[0] = 0xc0, // invalid comparator
                3 => invalid[3].data[20] = 255, // must not default to Subject
                4 => invalid[3].data[20] = 2,   // unsupported reference context
                5 => invalid[3].data[4..8].copy_from_slice(&f32::NAN.to_le_bytes()),
                6 => invalid.push(sub(b"SCRI", [0; 4])),
                7 => invalid[0].data[6] = 2, // poison
                8 => invalid[0].data[8] = 1, // addiction spell
                9 => invalid[0].data[12..16].copy_from_slice(&0.5f32.to_le_bytes()),
                10 => invalid[2].data[8] = 1, // timed effect
                11 => invalid[3].data[8..10].copy_from_slice(&449u16.to_le_bytes()), // wrong game
                12 => invalid[0].data.truncate(19),
                _ => {
                    // An unsupported branch cannot be hidden behind a known
                    // false gate or a short-circuiting OR group.
                    invalid.extend([
                        sub(b"EFID", 0x0100_0999u32.to_le_bytes()),
                        alch[2].clone(),
                        sub(b"CTDA", condition.clone()),
                    ]);
                }
            }
            index
                .items
                .insert(1, parse_alch_for_game(1, &invalid, index.game, &remap));
            assert!(
                conditional_restorations(&index, 1).is_none(),
                "variant {variant}"
            );
        }
    }

    #[test]
    fn authored_restoration_resolves_remapped_effect_and_canonical_vital() {
        let (index, _, _) = fixture();
        assert_eq!(instant_restorations(&index, 1), Some(vec![(30, 25.0)]));
    }
    #[test]
    fn unsupported_effect_never_becomes_partial_potion() {
        let (mut index, _, _) = fixture();
        let ItemKind::Aid {
            immediate_effects: Some(effects),
            ..
        } = &mut index.items.get_mut(&1).unwrap().kind
        else {
            unreachable!()
        };
        effects.push(crate::esm::records::MagicEffectItem {
            effect_form_id: 999,
            magnitude: 10.0,
            ..Default::default()
        });
        assert!(instant_restorations(&index, 1).is_none());
    }
    #[test]
    fn poison_conditions_scripts_duration_and_malformed_payloads_are_unavailable() {
        let (_, alch, _) = fixture();
        for variant in 0..8 {
            let mut subs = alch.clone();
            match variant {
                0 => subs.push(sub(b"CTDA", [])),
                1 => subs.push(sub(b"VMAD", [])),
                2 => subs[0].data[6] = 2, // poison bit 17
                3 => subs[2].data[8] = 1, // duration
                4 => subs[2].data[4] = 1, // area
                5 => {
                    subs[2].data.truncate(11);
                }
                6 => subs[2].data[..4].copy_from_slice(&f32::NAN.to_le_bytes()),
                _ => subs.push(sub(b"EFID", 3u32.to_le_bytes())),
            }
            let item = parse_alch_for_game(1, &subs, GameKind::Skyrim, &None);
            assert!(
                matches!(
                    item.kind,
                    ItemKind::Aid {
                        immediate_effects: None,
                        ..
                    }
                ),
                "variant {variant}"
            );
        }
    }
    #[test]
    fn magic_archetypes_flags_and_game_are_not_guessed() {
        let (_, _, mgef) = fixture();
        for offset in [0, 64, 84, 128, 132, 136] {
            let mut subs = mgef.clone();
            subs[0].data[offset] = 1;
            assert!(parse_mgef_for_game(1, &subs, GameKind::Skyrim, &None)
                .instant_restoration_av
                .is_none());
        }
        for flag in [2u32, 4, 0x400] {
            let mut subs = mgef.clone();
            subs[0].data[..4].copy_from_slice(&flag.to_le_bytes());
            assert!(parse_mgef_for_game(1, &subs, GameKind::Skyrim, &None)
                .instant_restoration_av
                .is_none());
        }
        assert!(parse_mgef_for_game(1, &mgef, GameKind::Fallout4, &None)
            .instant_restoration_av
            .is_none());
    }
}
