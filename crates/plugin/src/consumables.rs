//! Resolve fully supported restorative ingestibles into canonical AVIF changes.
//! Never drop an unsupported effect from a mixed potion and consume the rest.
use crate::esm::reader::GameKind;
use crate::esm::records::condition::{ConditionList, ConditionValue, RunOn};
use crate::esm::records::{EsmIndex, ItemKind};

/// An executable instantaneous restoration with a checked condition list.
#[derive(Debug, Clone)]
pub struct ConditionalRestoration {
    pub actor_value: u32,
    pub magnitude: f32,
    pub conditions: ConditionList,
}

/// Compile a conditional consumable only when *every* branch is understood.
/// The general evaluator's unknown-function zero fallback is not safe for
/// selecting effects (`unknown == 0` would incorrectly activate a branch).
/// This capability check intentionally precedes any condition short-circuit.
pub fn conditional_restorations(
    index: &EsmIndex,
    form_id: u32,
) -> Option<Vec<ConditionalRestoration>> {
    let has_perk = match index.game {
        GameKind::Skyrim => 448,
        GameKind::Fallout3NV => 449,
        _ => return None,
    };
    let ItemKind::Aid {
        authored_effects: Some(effects),
        simple_consumption_header: true,
        ..
    } = &index.items.get(&form_id)?.kind
    else {
        return None;
    };
    if effects.is_empty() || !effects.iter().any(|e| !e.conditions.is_empty()) {
        return None;
    }
    effects.iter().map(|entry| {
        if entry.delivery.is_some_and(|delivery| delivery != 0)
            || entry.conditions.len() != entry.condition_flags.len()
            || entry.conditions.iter().zip(&entry.condition_flags).any(|(cond, flags)| {
                cond.function_index != has_perk
                    || flags & 0x1e != 0 // no global/swap-subject/alias/pack flags yet
                    || flags >> 5 > 5 // invalid comparator
                    || !matches!(cond.run_on, RunOn::Subject | RunOn::Target)
                    || cond.param_1 == 0
                    || cond.param_2 != 0
                    || cond.extra_data_id != 0
                    || cond.param_1_text.is_some()
                    || cond.param_2_text.is_some()
                    || !matches!(cond.comparand, ConditionValue::Literal(value) if value.is_finite())
            })
        { return None; }
        let (actor_value, magnitude) = restoration(index, &entry.effect)?;
        Some(ConditionalRestoration { actor_value, magnitude, conditions: entry.conditions.clone() })
    }).collect()
}

fn restoration(
    index: &EsmIndex,
    effect: &crate::esm::records::MagicEffectItem,
) -> Option<(u32, f32)> {
    if effect.duration != 0
        || effect.area != 0
        || !effect.magnitude.is_finite()
        || effect.magnitude <= 0.0
    {
        return None;
    }
    let av = index
        .magic_effects
        .get(&effect.effect_form_id)?
        .instant_restoration_av?;
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
        .map(|effect| restoration(index, effect))
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
