//! Oblivion record-to-canonical actor-value translation.
//!
//! The executable's stat calculator is traced in CHARAL §9. The roster and
//! skill/attribute relationships are shared with the resolver and CHARAL.

use super::{oblivion_actor_values::OBLIVION_ACTOR_VALUES, EsmIndex};
use crate::equip::ResolvedNpc;
use byroredux_core::character::{Attribute, AttributeSet, SkillSet};

const AUTO_CALC: u32 = 0x10;
const PC_LEVEL_OFFSET: u32 = 0x80;

fn push(index: &EsmIndex, output: &mut Vec<(u32, f32)>, code: u32, value: f32) {
    if let Some(id) = index.oblivion_actor_value(code, true) {
        output.push((id, value));
    }
}

fn level(npc: &super::NpcRecord, player_level: Option<u16>) -> Option<u16> {
    if npc.acbs_flags & PC_LEVEL_OFFSET == 0 {
        return Some(npc.level.max(0) as u16);
    }
    let player = player_level? as i16;
    let max = i32::from(npc.oblivion_pools?.calc_max);
    let min = i32::from(npc.calc_min);
    let value = i32::from(player.wrapping_add(npc.level));
    // 0x467832–0x467872: zero min/max are disabled bounds; a positive
    // min is checked first, then a positive max, then the level-1 floor.
    let value = if min > 0 && value < min {
        min
    } else if max > 0 && value > max {
        max
    } else {
        value.max(1)
    };
    Some(value.min(i32::from(u16::MAX)) as u16)
}

/// Resolve an actor's spawn level using its selected record model.
pub fn actor_level_at_spawn(npc: &super::NpcRecord, player_level: Option<u16>) -> Option<u16> {
    if npc.oblivion_pools.is_some() {
        level(npc, player_level)
    } else {
        Some(super::effective_actor_level(npc).max(0) as u16)
    }
}

fn npc_values(
    resolved: &ResolvedNpc<'_>,
    index: &EsmIndex,
    player_level: Option<u16>,
    out: &mut Vec<(u32, f32)>,
) {
    let npc = resolved.stats;
    let Some(authored) = &npc.oblivion_npc_stats else {
        return;
    };
    let Some(pools) = npc.oblivion_pools else {
        return;
    };
    // The player's new-game template is the authored Player NPC record.
    if npc.acbs_flags & (AUTO_CALC | PC_LEVEL_OFFSET) == 0 || resolved.shell.form_id == 7 {
        for (i, value) in authored.attributes.iter().enumerate() {
            push(index, out, i as u32, f32::from(*value));
        }
        for (i, value) in authored.skills.iter().enumerate() {
            push(index, out, i as u32 + 12, f32::from(*value));
        }
        push(index, out, 8, f32::from(authored.health));
        push(index, out, 9, f32::from(pools.magicka));
        push(index, out, 10, f32::from(pools.fatigue));
        push(index, out, 11, f32::from(authored.attributes[0]) * 5.0);
        return;
    }
    let Some(level) = level(npc, player_level) else {
        return;
    };
    let (Some(race), Some(class)) = (
        index.races.get(&resolved.r#traits.race_form_id),
        index.classes.get(&npc.class_form_id),
    ) else {
        return;
    };
    let (Some(attributes), Some(favored), Some(specialization)) = (
        &race.base_attributes,
        class.primary_attributes,
        class.specialization,
    ) else {
        return;
    };
    if specialization > 2 || class.major_skills.len() != 7 {
        return;
    }
    let gender = if resolved.r#traits.acbs_flags & 1 != 0 {
        &attributes.female
    } else {
        &attributes.male
    };
    let bases = [
        gender.strength,
        gender.intelligence,
        gender.willpower,
        gender.agility,
        gender.speed,
        gender.endurance,
        gender.personality,
        gender.luck,
    ];
    let n = (i32::from(level) - 1) as f32;
    let mut computed = [0.0; 8];
    for (i, (&base, attr)) in bases
        .iter()
        .zip(AttributeSet::TES_CLASSIC.members())
        .enumerate()
    {
        // The NPC calculator skips Personality (0x522345–0x52234f),
        // leaving its DATA byte in place rather than recomputing it.
        if *attr == Attribute::Personality {
            computed[i] = f32::from(authored.attributes[i]);
            push(index, out, i as u32, computed[i]);
            continue;
        }
        let favored_bonus = if favored.0 == i as u32 || favored.1 == i as u32 {
            5.0
        } else {
            0.0
        };
        let growth = if matches!(attr, Attribute::Luck | Attribute::Personality) {
            0.0
        } else {
            (12..33)
                .filter_map(|code| {
                    let skill = SkillSet::OBLIVION.get(OBLIVION_ACTOR_VALUES[code])?;
                    (skill.governing == Some(*attr)).then_some(
                        if class.major_skills.contains(&(code as u32)) {
                            1.0
                        } else {
                            0.2
                        },
                    )
                })
                .sum::<f32>()
                * n
        };
        computed[i] = (f32::from(base) + favored_bonus + growth)
            .round_ties_even()
            .clamp(0.0, 100.0);
        push(index, out, i as u32, computed[i]);
    }
    for code in 12..33 {
        let major = class.major_skills.contains(&(code as u32));
        let spec = ((code - 12) / 7) as u32 == specialization;
        let mut value = if major { 25.0 + n } else { 5.0 + 0.1 * n };
        if spec {
            value += 5.0 + 0.5 * n;
        }
        for &(_, bonus) in race
            .skill_bonuses
            .iter()
            .filter(|(skill, _)| usize::from(*skill) == code)
        {
            value += f32::from(bonus);
        }
        push(
            index,
            out,
            code as u32,
            value.round_ties_even().clamp(0.0, 100.0),
        );
    }
    let setting = |name, default| {
        index
            .game_setting_float(name)
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    };
    let attr_mult = setting("fNPCAttributeHealthMult", 0.5);
    let per_level = setting("iNPCBasePerLevelHealthMult", 4.0);
    let low_mult = setting("fLowLevelNPCBaseHealthMult", 0.4);
    let low_max = setting("iLowLevelNPCMaxLevel", 4.0);
    let favored_endurance = f32::from(u8::from(favored.0 == 5 || favored.1 == 5));
    let class_mod = if specialization == 0 {
        1.0
    } else if specialization == 1 {
        -1.0
    } else {
        0.0
    };
    let raw_health = ((computed[0] + computed[5]) * attr_mult).trunc()
        + n * (per_level + favored_endurance + class_mod);
    let factor = if low_max > 0.0 && n < low_max {
        low_mult + (1.0 - low_mult) * n / low_max
    } else {
        1.0
    };
    push(index, out, 8, (raw_health * factor).trunc());
    push(index, out, 9, 2.5 * computed[1]);
    push(
        index,
        out,
        10,
        computed[0] + computed[2] + computed[3] + computed[5],
    );
    push(index, out, 11, 5.0 * computed[0]);
}

fn creature_values(
    resolved: &ResolvedNpc<'_>,
    index: &EsmIndex,
    player_level: Option<u16>,
    out: &mut Vec<(u32, f32)>,
) {
    let npc = resolved.stats;
    let Some(stats) = &npc.oblivion_creature_stats else {
        return;
    };
    for (i, value) in stats.attributes.iter().enumerate() {
        push(index, out, i as u32, f32::from(*value));
    }
    for code in 12..33 {
        let value = match (code - 12) / 7 {
            0 => stats.combat,
            1 => stats.magic,
            _ => stats.stealth,
        };
        push(index, out, code, f32::from(value));
    }
    if let Some(pools) = npc.oblivion_pools {
        let multiplier = if npc.acbs_flags & PC_LEVEL_OFFSET == 0 {
            Some(1.0)
        } else {
            level(npc, player_level).map(f32::from)
        };
        if let Some(multiplier) = multiplier {
            push(index, out, 8, f32::from(stats.health) * multiplier);
            push(index, out, 9, f32::from(pools.magicka) * multiplier);
            push(index, out, 10, f32::from(pools.fatigue) * multiplier);
        }
    }
    push(index, out, 11, f32::from(stats.attributes[0]) * 5.0);
}

/// View-specific direct source keys for a creature's grouped skill values.
pub fn creature_skill_aliases(
    resolved: &ResolvedNpc<'_>,
    index: &EsmIndex,
) -> Vec<(u32, byroredux_core::ecs::components::ActorValueAlias)> {
    use byroredux_core::character::NpcStatModel;
    use byroredux_core::ecs::components::ActorValueAlias;
    if !resolved.stats.is_creature
        || resolved.stats.oblivion_creature_stats.is_none()
        || index.character_rules.creature_stat_model() != NpcStatModel::OblivionCreature
    {
        return Vec::new();
    }
    let mut result = Vec::new();
    for code in 12..33 {
        let group = 12 + ((code - 12) / 7) * 7;
        let current = if code == 28 { 12 } else { group };
        if code == current && code == group {
            continue;
        }
        if let (Some(key), Some(current), Some(base)) = (
            index.oblivion_actor_value(code, true),
            index.oblivion_actor_value(current, true),
            index.oblivion_actor_value(group, true),
        ) {
            result.push((key, ActorValueAlias { current, base }));
        }
    }
    result
}

pub(super) fn derive(
    resolved: &ResolvedNpc<'_>,
    index: &EsmIndex,
    player_level: Option<u16>,
) -> Vec<(u32, f32)> {
    let mut values = Vec::with_capacity(37);
    if resolved.stats.is_creature {
        creature_values(resolved, index, player_level, &mut values);
    } else {
        npc_values(resolved, index, player_level, &mut values);
    }
    if let Some(ai) = resolved.ai_data.oblivion_ai_values {
        for (i, value) in ai.iter().enumerate() {
            push(index, &mut values, 33 + i as u32, f32::from(*value));
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::super::actor::{
        ClassRecord, GenderedAttributes, OblivionActorPools, OblivionNpcStats, RaceAttributes,
        RaceRecord,
    };
    use super::*;
    use crate::esm::reader::{GameKind, GlobalSlot};

    fn fixture() -> (EsmIndex, super::super::NpcRecord) {
        let mut index = EsmIndex {
            game: GameKind::Oblivion,
            character_rules: byroredux_core::character::CharacterRulesProfile::OBLIVION,
            ..Default::default()
        };
        index
            .install_oblivion_actor_values(GlobalSlot::Regular(1))
            .unwrap();
        index.races.insert(
            1,
            RaceRecord {
                base_attributes: Some(RaceAttributes {
                    male: GenderedAttributes {
                        strength: 40,
                        intelligence: 40,
                        willpower: 40,
                        agility: 40,
                        speed: 40,
                        endurance: 40,
                        personality: 40,
                        luck: 40,
                    },
                    female: GenderedAttributes::default(),
                }),
                skill_bonuses: vec![(12, 10)],
                ..Default::default()
            },
        );
        index.classes.insert(
            2,
            ClassRecord {
                primary_attributes: Some((0, 5)),
                specialization: Some(0),
                major_skills: vec![12, 14, 19, 20, 21, 22, 23],
                ..Default::default()
            },
        );
        let npc = super::super::NpcRecord {
            form_id: 0x100,
            race_form_id: 1,
            class_form_id: 2,
            level: 1,
            oblivion_npc_stats: Some(OblivionNpcStats {
                skills: [17; 21],
                health: 123,
                attributes: [40; 8],
            }),
            oblivion_pools: Some(OblivionActorPools {
                magicka: 55,
                fatigue: 77,
                calc_max: 0,
            }),
            oblivion_ai_values: Some([20, 30, 40, 50]),
            ..Default::default()
        };
        (index, npc)
    }

    fn value(rows: &[(u32, f32)], code: u32) -> Option<f32> {
        rows.iter()
            .find_map(|(id, value)| (*id == 0x0100_0000 + code + 1).then_some(*value))
    }

    #[test]
    fn stored_npc_uses_authored_values_and_ai() {
        let (index, npc) = fixture();
        let rows = derive(&ResolvedNpc::resolve(&npc, &index), &index, None);
        assert_eq!(rows.len(), 37);
        assert_eq!(
            (
                value(&rows, 8),
                value(&rows, 9),
                value(&rows, 10),
                value(&rows, 11)
            ),
            (Some(123.0), Some(55.0), Some(77.0), Some(200.0))
        );
        assert_eq!(
            (
                value(&rows, 12),
                value(&rows, 32),
                value(&rows, 33),
                value(&rows, 36)
            ),
            (Some(17.0), Some(17.0), Some(20.0), Some(50.0))
        );
    }

    #[test]
    fn class_calculation_uses_level_race_bonuses_and_setting_order() {
        let (index, mut npc) = fixture();
        npc.acbs_flags = AUTO_CALC;
        npc.oblivion_npc_stats.as_mut().unwrap().attributes[6] = 61;
        let rows = derive(&ResolvedNpc::resolve(&npc, &index), &index, None);
        assert_eq!(value(&rows, 6), Some(61.0));
        assert_eq!(
            (
                value(&rows, 0),
                value(&rows, 5),
                value(&rows, 12),
                value(&rows, 19)
            ),
            (Some(45.0), Some(45.0), Some(40.0), Some(25.0))
        );
        assert_eq!(value(&rows, 8), Some(18.0));
        npc.level = 3;
        let rows = derive(&ResolvedNpc::resolve(&npc, &index), &index, None);
        assert_eq!(
            (
                value(&rows, 0),
                value(&rows, 5),
                value(&rows, 12),
                value(&rows, 19)
            ),
            (Some(48.0), Some(48.0), Some(43.0), Some(27.0))
        );
        assert_eq!(value(&rows, 8), Some(42.0));
    }

    #[test]
    fn offset_level_uses_player_and_respects_optional_bounds() {
        let (_, mut npc) = fixture();
        npc.acbs_flags = PC_LEVEL_OFFSET;
        npc.level = -2;
        assert_eq!(level(&npc, None), None);
        assert_eq!(level(&npc, Some(20)), Some(18));
        npc.calc_min = 10;
        npc.oblivion_pools.as_mut().unwrap().calc_max = 15;
        assert_eq!(level(&npc, Some(5)), Some(10));
        assert_eq!(level(&npc, Some(20)), Some(15));
    }
}
