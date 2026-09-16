//! Authored ALCH effect chains. Parsing is not a runtime capability check.
use super::super::condition::{push_ctda, ConditionList};
use super::super::misc::MagicEffectItem;
use super::*;

#[derive(Debug, Clone)]
pub struct ConsumableEffect {
    pub effect: MagicEffectItem,
    /// FO3/FNV EFIT delivery (0=self, 1=touch, 2=target); Skyrim uses MGEF.
    pub delivery: Option<u32>,
    /// FO3/FNV redundant EFIT AV cache. MGEF remains authoritative.
    pub actor_value_cache: Option<u32>,
    pub conditions: ConditionList,
    /// Original CTDA flag bytes, one per condition. Retains flags that the
    /// general Condition representation does not yet interpret; consumers
    /// must not mistake a parsed chain for fully executable behavior.
    pub condition_flags: Vec<u8>,
}

/// Preserve effect-local CTDAs and remapped identities without dropping a
/// malformed condition and accidentally making an effect unconditional.
pub(super) fn parse_effects(
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> Option<Vec<ConsumableEffect>> {
    if !matches!(game, GameKind::Skyrim | GameKind::Fallout3NV) {
        return None;
    }
    let mut effects: Vec<ConsumableEffect> = Vec::new();
    let mut pending = None;
    for sub in subs {
        match &sub.sub_type {
            b"EFID" => {
                if sub.data.len() != 4 || pending.is_some() {
                    return None;
                }
                let id = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
                if id == 0 {
                    return None;
                }
                pending = Some(id);
            }
            b"EFIT" => {
                let effect_form_id = pending.take()?;
                let fallout = game == GameKind::Fallout3NV;
                if sub.data.len() != if fallout { 20 } else { 12 } {
                    return None;
                }
                let mut r = SubReader::new(&sub.data);
                let magnitude = if fallout {
                    r.u32_or_default() as f32
                } else {
                    r.f32_or_default()
                };
                if !magnitude.is_finite() {
                    return None;
                }
                let area = r.u32_or_default();
                let duration = r.u32_or_default();
                effects.push(ConsumableEffect {
                    effect: MagicEffectItem {
                        effect_form_id,
                        magnitude,
                        area,
                        duration,
                    },
                    delivery: fallout.then(|| r.u32_or_default()),
                    actor_value_cache: fallout.then(|| r.u32_or_default()),
                    conditions: Vec::new(),
                    condition_flags: Vec::new(),
                });
            }
            b"CTDA" => {
                if pending.is_some() || !matches!(sub.data.len(), 20 | 24 | 28 | 32) {
                    return None;
                }
                // The generic parser maps unknown Run-On values to Subject.
                // Never let that fallback change who a consumable tests.
                if sub.data.len() >= 28
                    && u32::from_le_bytes(sub.data[20..24].try_into().unwrap()) > 7
                {
                    return None;
                }
                let effect = effects.last_mut()?;
                let before = effect.conditions.len();
                push_ctda(sub, remap, &mut effect.conditions);
                if effect.conditions.len() != before + 1 {
                    return None;
                }
                effect.condition_flags.push(sub.data[0]);
            }
            b"CIS1" | b"CIS2" => {
                if pending.is_some() {
                    return None;
                }
                let effect = effects.last_mut()?;
                if effect.conditions.is_empty() {
                    return None;
                }
                push_ctda(sub, remap, &mut effect.conditions);
            }
            // Not a valid condition encoding for the supported ALCH layouts.
            b"CTDT" => return None,
            _ => {}
        }
    }
    (pending.is_none() && !effects.is_empty()).then_some(effects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::condition::{ConditionStringId, ConditionValue, RunOn};
    use crate::esm::records::test_support::sub;

    #[test]
    fn condition_groups_remap_per_effect_and_keep_timed_delivery() {
        let mut condition = vec![0; 28];
        condition[0] = 0x05; // OR + global comparand
        condition[4..8].copy_from_slice(&0x0100_0099u32.to_le_bytes());
        condition[8..10].copy_from_slice(&449u16.to_le_bytes());
        condition[12..16].copy_from_slice(&0x0100_0030u32.to_le_bytes());
        condition[20..24].copy_from_slice(&2u32.to_le_bytes()); // Reference
        condition[24..28].copy_from_slice(&0x0100_0040u32.to_le_bytes());
        let mut second = condition.clone();
        second[0] = 0;
        second[4..8].copy_from_slice(&1f32.to_le_bytes());
        let subs = vec![
            sub(b"ENIT", [0; 20]),
            sub(b"EFID", 0x0100_0020u32.to_le_bytes()),
            sub(
                b"EFIT",
                [5u32, 10, 6, 2, 16]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
            sub(b"CTDA", condition),
            sub(b"CIS1", b"ExampleScript\0"),
            sub(b"CIS2", b"ExampleVariable\0"),
            sub(b"CTDA", second.clone()),
            sub(b"EFID", 0x0100_0021u32.to_le_bytes()),
            sub(
                b"EFIT",
                [30u32, 0, 0, 0, 16]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
            sub(b"CTDA", second),
        ];
        let remap = Some(FormIdRemap::regular(2, vec![0]));
        let item = parse_alch_for_game(1, &subs, GameKind::Fallout3NV, &remap);
        let ItemKind::Aid {
            authored_effects: Some(effects),
            immediate_effects,
            ..
        } = item.kind
        else {
            panic!("missing chains")
        };
        assert!(immediate_effects.is_none());
        assert_eq!(effects.len(), 2);
        assert_eq!(effects[0].effect.effect_form_id, 0x0200_0020);
        assert_eq!(effects[1].effect.effect_form_id, 0x0200_0021);
        assert_eq!(effects[0].effect.magnitude, 5.0);
        assert_eq!(effects[0].effect.duration, 6);
        assert_eq!(effects[0].effect.area, 10);
        assert_eq!(effects[0].delivery, Some(2));
        assert_eq!(effects[0].actor_value_cache, Some(16));
        assert_eq!(effects[0].condition_flags, [5, 0]);
        assert_eq!(effects[1].condition_flags, [0]);
        let cond = effects[0].conditions[0];
        assert_eq!(cond.function_index, 449);
        assert_eq!(cond.param_1, 0x0200_0030);
        assert_eq!(cond.reference_form_id, 0x0200_0040);
        assert_eq!(cond.run_on, RunOn::Reference);
        assert_eq!(cond.comparand, ConditionValue::Global(0x0200_0099));
        assert!(cond.or_next);
        assert_eq!(
            cond.param_1_text,
            Some(ConditionStringId::from_text("ExampleScript"))
        );
        assert_eq!(
            cond.param_2_text,
            Some(ConditionStringId::from_text("ExampleVariable"))
        );
        assert_eq!(effects[1].conditions.len(), 1);
        assert!(!effects[1].conditions[0].or_next);
    }

    #[test]
    fn malformed_or_orphan_condition_never_enables_consumption() {
        let valid = vec![
            sub(b"ENIT", [0; 20]),
            sub(b"EFID", 20u32.to_le_bytes()),
            sub(b"EFIT", [25f32.to_le_bytes(), [0; 4], [0; 4]].concat()),
        ];
        for variant in 0..7 {
            let mut subs = valid.clone();
            match variant {
                0 => subs.push(sub(b"CTDA", [0; 19])),
                1 => subs.push(sub(b"CTDA", [0; 31])),
                2 => subs.insert(0, sub(b"CTDA", [0; 32])),
                3 => subs.insert(2, sub(b"CTDA", [0; 32])),
                4 => subs.push(sub(b"CIS1", b"orphan\0")),
                5 => subs.push(sub(b"CTDT", [0; 20])),
                _ => subs.push(sub(b"EFID", 21u32.to_le_bytes())),
            }
            let item = parse_alch_for_game(1, &subs, GameKind::Skyrim, &None);
            assert!(
                matches!(
                    item.kind,
                    ItemKind::Aid {
                        authored_effects: None,
                        immediate_effects: None,
                        ..
                    }
                ),
                "variant {variant}"
            );
        }
    }

    #[test]
    #[ignore = "requires installed Fallout 3 and New Vegas masters"]
    fn real_stimpak_conditions_stay_attached_to_their_authored_effects() {
        for (game_dir, master, magnitudes, durations, predicates) in [
            (
                "Fallout 3 goty",
                "Fallout3.esm",
                vec![30.0, 36.0],
                vec![0, 0],
                1,
            ),
            (
                "Fallout New Vegas",
                "FalloutNV.esm",
                vec![5.0, 6.0, 30.0, 36.0],
                vec![6, 6, 0, 0],
                2,
            ),
        ] {
            let path = format!("/mnt/data/SteamLibrary/steamapps/common/{game_dir}/Data/{master}");
            let index =
                crate::esm::parse_esm(&std::fs::read(path).expect("installed master required"))
                    .unwrap();
            let ItemKind::Aid {
                authored_effects: Some(effects),
                immediate_effects,
                ..
            } = &index.items[&0x15169].kind
            else {
                panic!("missing Stimpak effects in {master}")
            };
            assert!(immediate_effects.is_none());
            assert_eq!(
                effects
                    .iter()
                    .map(|e| e.effect.magnitude)
                    .collect::<Vec<_>>(),
                magnitudes
            );
            assert_eq!(
                effects
                    .iter()
                    .map(|e| e.effect.duration)
                    .collect::<Vec<_>>(),
                durations
            );
            for (i, effect) in effects.iter().enumerate() {
                assert_eq!(effect.conditions.len(), predicates);
                assert_eq!(effect.condition_flags, vec![0; predicates]);
                let perk = effect
                    .conditions
                    .iter()
                    .find(|c| c.function_index == 449)
                    .unwrap();
                assert_eq!(perk.param_1, 0x94EBF);
                assert_eq!(perk.comparand, ConditionValue::Literal((i % 2) as f32));
                assert_eq!(effect.delivery, Some(0));
            }
        }
    }
}
