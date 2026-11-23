//! Oblivion skill-use leveling: the one entry point (PRD
//! `oblivion-skill-use-leveling`, OSL-XP-001, OSL-XP-004, OSL-XP-007,
//! OSL-LVL-001/002, OSL-API-001/006; design `docs/engine/skill-use.md`).
//!
//! Skill use banks experience against the player's skill; enough of it raises
//! the skill's **base** actor value by one (never the permanent-modifier
//! layer), and every rank-up feeds [`SkillUseLevel`]'s tallies. Everything
//! reaches the rules through [`bank_skill_use`] (use, trainers) or
//! [`raise_skill_ranks`] (`AdvancePCSkill` / `ModPCSkill`); both read the
//! cost from the ruleset's [`LevelingModel::SkillUse`] settings, so there is no
//! game test here. A ruleset with another leveling model changes nothing.
//!
//! Emitters append to [`PendingSkillUse`]; one system (the host's) drains it.
//! The banked experience is the per-skill [`SkillProgress`] the Skyrim path
//! already saves. The skill is named by its Oblivion actor-value code
//! (Armorer 12 … Speechcraft 32).

use std::collections::HashMap;

use byroredux_core::character::{
    CharacterRuleset, SkillClass, SkillSet, SkillUseCosts, SkillUseLevel, SKILL_USE_CAP,
};
use byroredux_core::ecs::components::ActorValues;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::oblivion_actor_values::OBLIVION_ACTOR_VALUES;
use byroredux_plugin::esm::records::EsmIndex;

use crate::skills::SkillProgress;

/// One skill use an emitter in the combat, magic, movement or detection
/// system queued: the skill's Oblivion actor-value code and the experience
/// the action earned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillUseRecord {
    pub skill: u32,
    pub amount: f32,
}

/// The skill uses emitters queued this frame, drained by one system
/// (OSL-XP-007). Emitters lock nothing of the player's. Transient.
#[derive(Debug, Default)]
pub struct PendingSkillUse(pub Vec<SkillUseRecord>);

impl Resource for PendingSkillUse {}

/// What a `CLAS` says about skills: the specialization (`0` Combat, `1` Magic,
/// `2` Stealth) and the seven major skills (Oblivion actor-value codes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassSkills {
    /// The class's editor ID, for `levelup` to name the class it read (OSL-CLS-001).
    pub name: String,
    pub specialization: u32,
    pub major_skills: Vec<u32>,
}

/// The classes the load order authors, and which one the player has
/// (OSL-CLS-001). Read at each skill use (OSL-CLS-002): an edit to a record
/// here, or a script setting the player's `Background` class, changes the
/// next cost (OSL-CLS-003). Derived from the load order; not saved.
#[derive(Debug, Default, Clone)]
pub struct SkillUseClasses {
    /// The class of the player's `NPC_` record (`0x7`): the interim class
    /// until a character-creation flow exists (OSL-NG-001).
    pub player_class: Option<u32>,
    pub classes: HashMap<u32, ClassSkills>,
}

impl Resource for SkillUseClasses {}

/// Build [`SkillUseClasses`] from the load order. A `CLAS` carries a
/// specialization and seven major skills only where the parser decodes them
/// (Oblivion's), so the table holds exactly those and is empty for every other
/// game; no game test is needed (#7212), and the player's class is read only
/// when the table has one.
pub fn install_skill_use_classes(world: &mut World, index: &EsmIndex) {
    let mut out = SkillUseClasses::default();
    for (&form, class) in &index.classes {
        if let Some(specialization) = class.specialization {
            out.classes.insert(
                form,
                ClassSkills {
                    name: class.editor_id.clone(),
                    specialization,
                    major_skills: class.major_skills.clone(),
                },
            );
        }
    }
    if !out.classes.is_empty() {
        out.player_class = index
            .npcs
            .get(&byroredux_plugin::esm::records::actor::player_npc_form_id(index.game))
            .map(|npc| npc.class_form_id);
    }
    world.insert_resource(out);
}

/// This subsystem's transient world setup.
pub fn install(world: &mut World) {
    world.init_resource::<PendingSkillUse>();
    world.init_resource::<SkillUseClasses>();
    world.init_resource::<SleepCompletions>();
}

/// The class of `player`: a class its `Background` names, else the player
/// record's. `None` when neither is a known class.
pub fn player_class(world: &World, player: EntityId) -> Option<ClassSkills> {
    let classes = world.try_resource::<SkillUseClasses>()?;
    world
        .get::<byroredux_core::character::Background>(player)
        .and_then(|background| classes.classes.get(&background.class_form_id))
        .or_else(|| classes.player_class.and_then(|form| classes.classes.get(&form)))
        .cloned()
}

/// A skill's name, actor value and governing attribute, with the rules to
/// bank it by.
struct Skill {
    av: u32,
    code: u32,
    costs: SkillUseCosts,
    class: SkillClass,
    governing: Option<usize>,
    /// The major increases a level takes (`iLevelUpSkillCount`, 10).
    per_level: u16,
}

fn resolve(world: &World, player: EntityId, code: u32) -> Option<Skill> {
    if !(byroredux_core::character::OBLIVION_SKILL_AV_FIRST
        ..=byroredux_core::character::OBLIVION_SKILL_AV_LAST)
        .contains(&code)
    {
        return None;
    }
    let leveling = world.try_resource::<CharacterRuleset>()?.leveling;
    let costs = leveling.skill_use_costs()?;
    let per_level = per_level_of(&leveling)?;
    let name = *OBLIVION_ACTOR_VALUES.get(code as usize)?;
    let av = crate::obscript_actor_values::actor_value_form(world, code as f32)?;
    let class = player_class(world, player).map_or_else(SkillClass::default, |class| {
        SkillClass::of(code, class.specialization, &class.major_skills)
    });
    let governing = SkillSet::OBLIVION
        .governing(name)
        .and_then(SkillUseLevel::attribute_index);
    Some(Skill { av, code, costs, class, governing, per_level })
}

/// What `apply` does to the skill's value and banked experience.
fn apply(
    world: &World,
    player: EntityId,
    skill: &Skill,
    step: impl FnOnce(u16, f32) -> (u16, f32, u16),
) -> Vec<u16> {
    let banked = world
        .try_resource::<SkillProgress>()
        .and_then(|progress| progress.0.get(&skill.av).copied())
        .unwrap_or(0.0);
    let (value, banked, ranks_reached) = {
        let Some(mut values) = world.query_mut::<ActorValues>() else {
            return Vec::new();
        };
        let Some(values) = values.get_mut(player) else {
            return Vec::new();
        };
        let Some(entry) = values.get_base(skill.av) else {
            log::debug!("skill use: the player has no Oblivion skill {}", skill.code);
            return Vec::new();
        };
        let before = entry.base.max(0.0) as u16;
        let (after, banked, ranks) = step(before, banked);
        if after != before {
            // OSL-LVL-001: the base layer, not the permanent-modifier layer.
            values.set_base(skill.av, f32::from(after));
        }
        (after, banked, ranks)
    };
    if let Some(mut stored) = world.try_resource_mut::<SkillProgress>() {
        stored.0.insert(skill.av, banked);
    }
    let reached: Vec<u16> = (value - ranks_reached + 1..=value).collect();
    if !reached.is_empty() {
        for _ in &reached {
            record_rank_up(world, player, skill);
        }
        log::info!("skill use: skill {} reached {value}", skill.code);
    }
    reached
}

/// The major-skill increases a level takes under `leveling`, when it levels by skill use.
fn per_level_of(leveling: &byroredux_core::character::LevelingModel) -> Option<u16> {
    match leveling {
        byroredux_core::character::LevelingModel::SkillUse { major_skill_ups_per_level, .. } => {
            Some(u16::from(*major_skill_ups_per_level))
        }
        _ => None,
    }
}

fn record_rank_up(world: &World, player: EntityId, skill: &Skill) {
    if let Some(mut levels) = world.query_mut::<SkillUseLevel>() {
        if let Some(level) = levels.get_mut(player) {
            level.record_rank_up(skill.governing, skill.class.major, skill.per_level);
            return;
        }
        let mut level = SkillUseLevel::default();
        level.record_rank_up(skill.governing, skill.class.major, skill.per_level);
        levels.insert(player, level);
    }
}

/// OSL-XP-001: bank `amount` experience against the player's skill `code` and
/// return the values it reached, in order. Empty when the skill only banked, is
/// at 100, is not an Oblivion skill, or the ruleset does not level by use.
///
/// #7210 — the "at 100" gate reads the skill's **current** value, Fortify included (`GetAV(skill) >= 100`,
/// `docs/engine/skill-use.md` §1, owner decision 2026-11-22): a skill fortified to 100 or more banks nothing while
/// the effect lasts, though its rank-ups still write the base layer (OSL-LVL-001).
pub fn bank_skill_use(world: &World, player: EntityId, code: u32, amount: f32) -> Vec<u16> {
    let Some(skill) = resolve(world, player, code) else {
        return Vec::new();
    };
    let current = world.get::<ActorValues>(player).map(|values| values.current(skill.av));
    if current.is_some_and(|value| value >= f32::from(SKILL_USE_CAP)) {
        return Vec::new();
    }
    apply(world, player, &skill, |value, banked| {
        let out = skill.costs.bank(value, banked, skill.class, amount);
        (out.value, out.banked, out.ranks.len() as u16)
    })
}

/// OSL-API-001 / OSL-API-006: raise the player's skill `code` by `ranks` ranks
/// (the unit of `AdvancePCSkill` and `ModPCSkill`, V14), counting each toward the
/// level and the attribute tally. The banked experience is set to 0.0 for both
/// commands: the Beta Patch 1.1 behaviour CS *ModPCSkill* documents (Q5), which CS
/// *AdvancePCSkill* says it shares ("subject to the same issue as ModPCSkill";
/// owner decision 2026-11-23, #7310). A skill stops at 100.
pub fn raise_skill_ranks(world: &World, player: EntityId, code: u32, ranks: u32) -> Vec<u16> {
    let Some(skill) = resolve(world, player, code) else {
        return Vec::new();
    };
    apply(world, player, &skill, |value, _banked| {
        // One per rank, stopping at 100; a skill already past it is left where it is.
        let cap = u32::from(SKILL_USE_CAP.max(value));
        let raised = u32::from(value).saturating_add(ranks).min(cap) as u16;
        (raised, 0.0, raised - value)
    })
}

// ── Levels (#7052, Phase 3) ──────────────────────────────────────────────────────────────────────────

/// Where a sleep request started. A bed menu offers attribute choices on completion;
/// scripted and jail sleep use the console's already-recorded choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepSource {
    Scripted,
    Bed(EntityId),
}

/// The terminal result of a sleep. Waiting does not emit an event (OSL-LVL-004). Transient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SleepCompleted {
    pub hours: f32,
    pub source: SleepSource,
    /// An interruption never authorizes a level (OSL-SLP-002).
    pub interrupted: bool,
}

/// The completed sleeps nothing has read yet, drained by `oblivion_level_system`.
#[derive(Debug, Default)]
pub struct SleepCompletions(pub Vec<SleepCompleted>);

impl Resource for SleepCompletions {}

/// Why `levelup attr` refused a pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickRefusal {
    /// No level is pending (the ruleset may not level by skill use at all).
    NoPendingLevel,
    /// The code is not one of the eight attributes (0 Strength … 7 Luck).
    NotAnAttribute,
    /// The attribute is already one of this level's picks: the three are distinct.
    AlreadyPicked,
    /// Three attributes are already picked for the level.
    LevelFull,
    /// The attribute is at 100 and cannot rise.
    AtMaximum,
}

/// A pick `levelup attr` accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickAccepted {
    pub attribute: u8,
    /// The bonus the level will give it (the ruleset's band for its frozen tally, #7196).
    pub bonus: u8,
    /// Picks the oldest pending level now holds, of three.
    pub picks: usize,
}

/// The attributes a level raises: the picks a level takes.
pub const LEVEL_PICKS: usize = 3;

/// OSL-API-005 / OSL-ATT-001: pick attribute `attribute` (0 to 7) for the oldest pending level. The picks
/// are distinct, at most three, and an attribute at 100 cannot be picked. The level applies at the next
/// completed sleep.
pub fn pick_attribute(world: &World, player: EntityId, attribute: u32) -> Result<PickAccepted, PickRefusal> {
    if attribute > 7 {
        return Err(PickRefusal::NotAnAttribute);
    }
    let base = crate::obscript_actor_values::actor_value_form(world, attribute as f32)
        .and_then(|av| world.get::<ActorValues>(player).and_then(|values| values.get_base(av).map(|entry| entry.base)));
    if base.is_some_and(|base| base >= 100.0) {
        return Err(PickRefusal::AtMaximum);
    }
    let leveling = world.try_resource::<CharacterRuleset>().ok_or(PickRefusal::NoPendingLevel)?.leveling;
    let mut levels = world.query_mut::<SkillUseLevel>().ok_or(PickRefusal::NoPendingLevel)?;
    let level = levels.get_mut(player).and_then(|level| level.pending.first_mut()).ok_or(PickRefusal::NoPendingLevel)?;
    if level.picks.contains(&(attribute as u8)) {
        return Err(PickRefusal::AlreadyPicked);
    }
    if level.picks.len() >= LEVEL_PICKS {
        return Err(PickRefusal::LevelFull);
    }
    level.picks.push(attribute as u8);
    Ok(PickAccepted { attribute: attribute as u8, bonus: level.bonus(attribute as usize, &leveling), picks: level.picks.len() })
}

/// What a completed sleep did with the pending levels.
#[derive(Debug, Clone, PartialEq)]
pub enum LevelUp {
    /// No level was pending.
    NoPending,
    /// A level is pending but fewer than three attributes are picked: nothing applied.
    NotReady { picks: usize },
    /// Every attribute is at 100: no level (OSL-LVL-006), the pending level stays.
    Perfection,
    /// The level applied: the new level, each raised attribute as `(code, from, to)`, and the Health gain.
    Applied { level: u16, raised: Vec<(u8, f32, f32)>, health_gain: f32 },
}

/// OSL-LVL-004 / OSL-ATT-001 / OSL-HP-001: take the oldest pending level, if its three attributes are picked.
/// Each pick rises by its frozen bonus, to at most 100 (the unspent tally rolls over to nothing); Health's
/// stored term gains `floor(0.1 × Endurance)` with Endurance read **after** the picks (V8; the stored sum is
/// truncated, V7); the level rises by one and the level's `major_skill_ups_per_level` leave the count. Magicka
/// and Fatigue's stored terms are untouched (V9). Nothing here is gated on sleeping: the caller is the system
/// that reads a completed sleep.
pub fn apply_pending_level(world: &World, player: EntityId) -> LevelUp {
    apply_level(world, player, None)
}

/// Apply the confirmed menu choices to the level offered after sleep. Validation happens
/// before mutating any attribute, and the saved console picks survive a refusal.
pub fn apply_level_menu_picks(
    world: &World,
    player: EntityId,
    level: u16,
    tallies: [u16; 8],
    picks: [u8; LEVEL_PICKS],
) -> LevelUp {
    use byroredux_core::character::CharacterLevel;
    if world.get::<CharacterLevel>(player).map_or(1, |value| value.level) != level {
        return LevelUp::NoPending;
    }
    apply_level(world, player, Some((tallies, picks)))
}

fn apply_level(world: &World, player: EntityId, selection: Option<([u16; 8], [u8; LEVEL_PICKS])>) -> LevelUp {
    let Some(leveling) = world.try_resource::<CharacterRuleset>().map(|rules| rules.leveling) else {
        return LevelUp::NoPending;
    };
    // #7196 — the level's settings (`iLevelUpSkillCount`, the `iLevelUpXXMult` bands, `fStatsHealthLevelMult`)
    // as the load order authors them, through the ruleset's model.
    let (Some(per_level), Some(health_per_endurance)) = (per_level_of(&leveling), leveling.level_up_health_per_endurance()) else {
        return LevelUp::NoPending;
    };
    let attributes: [_; 8] = std::array::from_fn(|code| crate::obscript_actor_values::actor_value_form(world, code as f32));
    let (Some(endurance), Some(health)) = (attributes[5], crate::obscript_actor_values::actor_value_form(world, 8.0)) else {
        return LevelUp::NoPending;
    };
    let mut raised = Vec::new();
    let health_gain: f32;
    {
        let Some((mut values, mut levels)) = world.query_2_mut_mut::<ActorValues, SkillUseLevel>() else {
            return LevelUp::NoPending;
        };
        let (Some(values), Some(level)) = (values.get_mut(player), levels.get_mut(player)) else {
            return LevelUp::NoPending;
        };
        let Some(pending) = level.pending.first() else { return LevelUp::NoPending };
        let picks = if let Some((tallies, picks)) = selection.as_ref() {
            if pending.tallies != *tallies {
                return LevelUp::NoPending;
            }
            picks.as_slice()
        } else {
            &pending.picks
        };
        let base = |form: u32| values.get_base(form).map(|entry| entry.base);
        let at_max = attributes.iter().all(|form| form.and_then(base).is_none_or(|value| value >= 100.0));
        if at_max {
            return LevelUp::Perfection;
        }
        if picks.len() < LEVEL_PICKS {
            return LevelUp::NotReady { picks: picks.len() };
        }
        if selection.is_some() {
            let eligible = picks.iter().enumerate().filter(|(index, code)| {
                !picks[..*index].contains(code)
                    && attributes.get(**code as usize).copied().flatten().and_then(base).is_some_and(|value| value < 100.0)
            }).count();
            if eligible != LEVEL_PICKS {
                return LevelUp::NotReady { picks: eligible };
            }
        }
        for &code in picks {
            let Some(av) = attributes.get(code as usize).copied().flatten() else { continue };
            let from = values.get_base(av).map_or(0.0, |entry| entry.base);
            let to = (from + f32::from(pending.bonus(code as usize, &leveling))).min(100.0);
            values.set_base(av, to);
            raised.push((code, from, to));
        }
        let endurance_now = values.get_base(endurance).map_or(0.0, |entry| entry.base).max(0.0) as u16;
        health_gain = byroredux_core::character::apply_oblivion_level_up_health(values, health, endurance_now, health_per_endurance);
        level.pending.remove(0);
        level.major_increases = level.major_increases.saturating_sub(per_level);
    }
    let new_level = world.query_mut::<byroredux_core::character::CharacterLevel>().map_or(2, |mut levels| {
        if let Some(level) = levels.get_mut(player) {
            level.level = level.level.saturating_add(1);
            return level.level;
        }
        levels.insert(player, byroredux_core::character::CharacterLevel { level: 2, xp: 0 });
        2
    });
    LevelUp::Applied { level: new_level, raised, health_gain }
}

/// OSL-API-002: ObScript `AdvancePCLevel`: "raises the player's level by bringing up the level-up menu".
/// Here it freezes the tallies as they stand into a pending level, to be picked and taken at the next
/// completed sleep. `false` for a ruleset that does not level by skill use or a player with no tallies yet.
pub fn advance_level(world: &World, player: EntityId) -> bool {
    if world.try_resource::<CharacterRuleset>().and_then(|r| per_level_of(&r.leveling)).is_none() {
        return false;
    }
    let Some(mut levels) = world.query_mut::<SkillUseLevel>() else {
        return false;
    };
    match levels.get_mut(player) {
        Some(level) => level.freeze_pending(),
        None => {
            let mut level = SkillUseLevel::default();
            level.freeze_pending();
            levels.insert(player, level);
        }
    }
    true
}

/// The Oblivion skills (actor-value codes) an engine system feeds today. The one list behind `levelup`'s
/// unfed report and the PRD's fed-skills metric (§8, OSL-USE-006): a skill joins it in the commit that
/// adds its emitter, never before. Blade, Blunt and Hand to Hand are fed by a melee hit that lands on a
/// living target, Light and Heavy Armor by a hit the player takes (#7051 2a); Athletics by seconds of
/// running or swimming, Sneak by seconds of moving undetected in the sneak stance, Acrobatics by a jump
/// (2b) and by a fall that damages the player (#7056), Marksman by the player's own arrow hitting a living target
/// (`HitEvent::arrow`, player-archery #7055), Block by a hit the player blocks (the target-side guard, #7054). The six
/// magic schools (20 to 25) are fed by the player's cast action (player-spellcasting #7081): one use, usage 1,
/// for the spell's primary school; the console's `magic.cast` and a script's `Cast` feed nothing.
pub const FED_SKILLS: &[u32] = &[13, 14, 15, 16, 17, 18, 20, 21, 22, 23, 24, 25, 26, 27, 28, 31];

/// Queue one use of the skill `code` for the drain (OSL-XP-007): the experience is the skill's use
/// value `usage` times `mult` (0 for none), `skill_use_amount`. An emitter calls this and nothing else;
/// it locks nothing of the player's. A ruleset that does not level by skill use queues nothing. Returns
/// whether a use was queued.
pub fn queue_skill_use(world: &World, code: u32, usage: u8, mult: f32) -> bool {
    let levels_by_use = world
        .try_resource::<CharacterRuleset>()
        .is_some_and(|ruleset| ruleset.leveling.skill_use_costs().is_some());
    if !levels_by_use || !FED_SKILLS.contains(&code) {
        return false;
    }
    let Some(amount) = byroredux_core::character::skill_use_amount(code, usage, mult) else {
        return false;
    };
    let Some(mut pending) = world.try_resource_mut::<PendingSkillUse>() else {
        return false;
    };
    pending.0.push(SkillUseRecord { skill: code, amount });
    true
}

/// OSL-API-003: what `levelup` reports for an Oblivion player — the class it read and its
/// major skills, the level count, each attribute's tally, and per skill the value, the experience
/// banked and its cost, the pending levels with their offered bonuses and picks, and the skills
/// no emitter feeds yet. `None` when the ruleset does not level by skill use.
pub fn skill_use_report(world: &World, player: EntityId) -> Option<Vec<String>> {
    let rules = world.try_resource::<CharacterRuleset>()?.leveling;
    let byroredux_core::character::LevelingModel::SkillUse { major_skill_ups_per_level, .. } = rules else {
        return None;
    };
    const SPECIALIZATIONS: [&str; 3] = ["Combat", "Magic", "Stealth"];
    const ATTRIBUTES: [&str; 8] = [
        "Strength", "Intelligence", "Willpower", "Agility", "Speed", "Endurance", "Personality", "Luck",
    ];
    let mut out = Vec::new();
    match player_class(world, player) {
        Some(class) => out.push(format!(
            "levelup: class {} ({}), major skills: {}",
            class.name,
            SPECIALIZATIONS.get(class.specialization as usize).copied().unwrap_or("?"),
            class
                .major_skills
                .iter()
                .map(|&code| OBLIVION_ACTOR_VALUES.get(code as usize).copied().unwrap_or("?"))
                .collect::<Vec<_>>()
                .join(", "),
        )),
        None => out.push("levelup: class unknown (no Player class in the load order)".to_string()),
    }
    let level = world.get::<SkillUseLevel>(player).map(|level| level.clone()).unwrap_or_default();
    out.push(format!(
        "levelup: major increases {}/{}; attribute tallies: {}",
        level.major_increases,
        major_skill_ups_per_level,
        ATTRIBUTES
            .iter()
            .zip(level.attribute_tallies)
            .map(|(name, tally)| format!("{name} {tally}"))
            .collect::<Vec<_>>()
            .join(", "),
    ));
    for (i, pending) in level.pending.iter().enumerate() {
        let offered = ATTRIBUTES
            .iter()
            .enumerate()
            .map(|(code, name)| format!("{name} +{}", pending.bonus(code, &rules)))
            .collect::<Vec<_>>()
            .join(", ");
        let picked = pending
            .picks
            .iter()
            .map(|&code| ATTRIBUTES.get(code as usize).copied().unwrap_or("?"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push(format!(
            "levelup: pending level {} offers {offered}; picked {}/{LEVEL_PICKS}{}{}",
            i + 1,
            pending.picks.len(),
            if picked.is_empty() { String::new() } else { format!(" ({picked})") },
            if pending.picks.len() == LEVEL_PICKS { "; sleep to take it" } else { "; `levelup attr <attribute>` picks" },
        ));
    }
    for code in byroredux_core::character::OBLIVION_SKILL_AV_FIRST..=byroredux_core::character::OBLIVION_SKILL_AV_LAST {
        let Some(skill) = resolve(world, player, code) else { continue };
        let value = world
            .get::<ActorValues>(player)
            .and_then(|values| values.get_base(skill.av).map(|entry| entry.base.max(0.0) as u16));
        let Some(value) = value else { continue };
        let banked = world
            .try_resource::<SkillProgress>()
            .and_then(|progress| progress.0.get(&skill.av).copied())
            .unwrap_or(0.0);
        let tags = match (skill.class.major, skill.class.specialization) {
            (true, true) => " major, specialization",
            (true, false) => " major",
            (false, true) => " specialization",
            (false, false) => "",
        };
        out.push(format!(
            "levelup: {} {value} banked {banked:.2} / cost {:.2}{tags}",
            OBLIVION_ACTOR_VALUES[code as usize],
            skill.costs.cost(value, skill.class),
        ));
    }
    // OSL-USE-006 / 007: a skill no system feeds is reported, never fed by a stand-in action.
    let unfed: Vec<&str> = (byroredux_core::character::OBLIVION_SKILL_AV_FIRST
        ..=byroredux_core::character::OBLIVION_SKILL_AV_LAST)
        .filter(|code| !FED_SKILLS.contains(code))
        .filter_map(|code| OBLIVION_ACTOR_VALUES.get(code as usize).copied())
        .collect();
    out.push(format!(
        "levelup: fed skills {}/21; unfed: {}",
        FED_SKILLS.len(),
        unfed.join(", ")
    ));
    Some(out)
}

/// Drain [`PendingSkillUse`] into [`bank_skill_use`] for the player. The one
/// system that writes the player's skills for skill use (OSL-XP-007).
pub fn skill_use_system(world: &World, player: Option<EntityId>) {
    let pending = world
        .try_resource_mut::<PendingSkillUse>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    let Some(player) = player else {
        return;
    };
    for record in pending {
        bank_skill_use(world, player, record.skill, record.amount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::reader::GameKind;

    /// #7212 — the class table follows the data, not the game: a `CLAS` with a decoded specialization joins it under
    /// any game, one without (every non-Oblivion class) never does, and the player's class is read only beside it.
    #[test]
    fn the_class_table_holds_the_classes_with_a_specialization_whatever_the_game() {
        use byroredux_plugin::esm::records::actor::{ClassRecord, NpcRecord};
        let class = |form_id: u32, specialization: Option<u32>| ClassRecord {
            form_id,
            editor_id: format!("Class{form_id:X}"),
            specialization,
            major_skills: vec![14, 15],
            ..Default::default()
        };
        let mut index = EsmIndex { game: GameKind::Skyrim, ..Default::default() };
        index.classes.insert(0x10, class(0x10, Some(0)));
        index.classes.insert(0x11, class(0x11, None));
        index.npcs.insert(7, NpcRecord { form_id: 7, class_form_id: 0x10, ..Default::default() });
        let mut world = World::new();
        install_skill_use_classes(&mut world, &index);
        let table = world.resource::<SkillUseClasses>();
        assert_eq!(table.classes.keys().copied().collect::<Vec<_>>(), vec![0x10], "only the class with a specialization");
        assert_eq!(table.player_class, Some(0x10));
        drop(table);

        let mut fallout = EsmIndex { game: GameKind::Oblivion, ..Default::default() };
        fallout.classes.insert(0x11, class(0x11, None));
        fallout.npcs.insert(7, NpcRecord { form_id: 7, class_form_id: 0x11, ..Default::default() });
        install_skill_use_classes(&mut world, &fallout);
        let table = world.resource::<SkillUseClasses>();
        assert!(table.classes.is_empty() && table.player_class.is_none(), "no specialization, no table, even under Oblivion");
    }
    use byroredux_core::character::{Background, LevelingModel};
    use byroredux_plugin::esm::reader::GlobalSlot;

    const BLADE: u32 = 14;
    const SNEAK: u32 = 31;

    /// A world with Oblivion's roster installed, the Oblivion ruleset, a class
    /// (Combat, Blade and Block major) and a player with Blade 5 and Sneak 5.
    fn world() -> (World, EntityId, u32, u32) {
        let mut index = EsmIndex { game: GameKind::Oblivion, ..Default::default() };
        index.install_oblivion_actor_values(GlobalSlot::Regular(0)).unwrap();
        let mut world = World::new();
        crate::install_actor_value_names(&mut world, &index);
        world.insert_resource(crate::ScriptGame(GameKind::Oblivion));
        world.insert_resource(CharacterRuleset::new(LevelingModel::OBLIVION));
        world.insert_resource(SkillProgress::default());
        world.insert_resource(SkillUseClasses {
            player_class: Some(0x100),
            classes: HashMap::from([(
                0x100,
                ClassSkills { name: "Test".into(), specialization: 0, major_skills: vec![BLADE, 15] },
            )]),
        });
        world.register::<ActorValues>();
        world.register::<SkillUseLevel>();
        world.register::<Background>();
        let player = world.spawn();
        let blade = index.oblivion_actor_value(BLADE, true).unwrap();
        let sneak = index.oblivion_actor_value(SNEAK, true).unwrap();
        world.insert(player, ActorValues::from_pairs([(blade, 5.0), (sneak, 5.0)]));
        (world, player, blade, sneak)
    }

    fn base(world: &World, player: EntityId, av: u32) -> f32 {
        world.get::<ActorValues>(player).unwrap().get_base(av).unwrap().base
    }

    /// OSL-XP-001 / OSL-LVL-001/002: banking ranks the skill's base, feeds the
    /// governing attribute's tally (Blade → Strength) and, being major, the
    /// level count. The surplus stays banked.
    #[test]
    fn use_ranks_the_base_and_feeds_the_tallies() {
        let (world, player, blade, _) = world();
        // Blade 5, major and in the Combat specialization: (0.35·5)^1.5 · 0.75 · 0.75 = 1.30.
        let reached = bank_skill_use(&world, player, BLADE, 2.0);
        assert_eq!(reached, vec![6]);
        assert_eq!(base(&world, player, blade), 6.0);
        let level = world.get::<SkillUseLevel>(player).unwrap();
        assert_eq!(level.major_increases, 1);
        assert_eq!(level.attribute_tallies[0], 1, "Blade is governed by Strength");
        let banked = world.resource::<SkillProgress>().0[&blade];
        assert!((banked - (2.0 - 1.302)).abs() < 0.01, "{banked}");
    }

    /// #7210 — a skill whose current value, Fortify included, is at 100 or more banks nothing: base Blade 95 with
    /// Fortify Blade 10 neither ranks nor banks, and the same skill unfortified does.
    #[test]
    fn a_skill_fortified_to_100_banks_nothing() {
        let (mut world, player, blade, _) = world();
        world.get_mut::<ActorValues>(player).unwrap().set_base(blade, 95.0);
        world.get_mut::<ActorValues>(player).unwrap().mod_temporary(blade, 10.0);
        assert!(bank_skill_use(&world, player, BLADE, 1000.0).is_empty());
        assert_eq!(base(&world, player, blade), 95.0);
        assert!(world.resource::<SkillProgress>().0.get(&blade).is_none_or(|banked| *banked == 0.0), "nothing banked");
        world.get_mut::<ActorValues>(player).unwrap().mod_temporary(blade, -10.0);
        assert_eq!(bank_skill_use(&world, player, BLADE, 1000.0), vec![96, 97, 98, 99, 100]);
    }

    /// A skill outside the class's majors ranks slower and feeds the
    /// attribute tally but not the level count (OSL-LVL-002).
    #[test]
    fn a_minor_skill_tallies_its_attribute_but_not_the_level() {
        let (world, player, _, sneak) = world();
        // Sneak 5, minor and outside Combat: 2.894.
        assert!(bank_skill_use(&world, player, SNEAK, 2.0).is_empty());
        assert_eq!(bank_skill_use(&world, player, SNEAK, 1.0), vec![6]);
        assert_eq!(base(&world, player, sneak), 6.0);
        let level = world.get::<SkillUseLevel>(player).unwrap();
        assert_eq!(level.major_increases, 0);
        assert_eq!(level.attribute_tallies[3], 1, "Sneak is governed by Agility");
    }

    /// OSL-CLS-002 / OSL-CLS-003: the class is read at each use, so editing the
    /// record's major skills between two uses changes the second cost, and a
    /// player `Background` class overrides the record's.
    #[test]
    fn the_class_is_read_at_each_use() {
        let (mut world, player, blade, _) = world();
        assert_eq!(bank_skill_use(&world, player, BLADE, 2.0), vec![6]);
        // Blade stops being major: the next rank costs more, so an amount that
        // would have ranked it is now short.
        world
            .resource_mut::<SkillUseClasses>()
            .classes
            .get_mut(&0x100)
            .unwrap()
            .major_skills
            .clear();
        let banked = world.resource::<SkillProgress>().0[&blade];
        let minor_cost = SkillUseCosts::OBLIVION
            .cost(6, SkillClass { specialization: true, major: false });
        assert!(minor_cost > banked);
        assert!(bank_skill_use(&world, player, BLADE, minor_cost - banked - 0.01).is_empty());
        // A script giving the player another class (Stealth) takes effect on the next use.
        world.resource_mut::<SkillUseClasses>().classes.insert(
            0x200,
            ClassSkills { name: "Other".into(), specialization: 2, major_skills: vec![BLADE] },
        );
        world.insert(player, Background { race_form_id: 1, class_form_id: 0x200 });
        assert_eq!(player_class(&world, player).unwrap().specialization, 2);
    }

    /// V2 / OSL-XP-003: a skill at 100 banks nothing.
    #[test]
    fn a_skill_at_100_banks_nothing() {
        let (world, player, blade, _) = world();
        world.query_mut::<ActorValues>().unwrap().get_mut(player).unwrap().set_base(blade, 100.0);
        assert!(bank_skill_use(&world, player, BLADE, 1.0e6).is_empty());
        assert!(!world.resource::<SkillProgress>().0.get(&blade).is_some_and(|b| *b > 0.0));
    }

    /// A ruleset that does not level by skill use, a code outside the 21
    /// skills and an unknown player change nothing.
    #[test]
    fn what_is_not_an_oblivion_skill_use_changes_nothing() {
        let (mut world, player, blade, _) = world();
        assert!(bank_skill_use(&world, player, 11, 50.0).is_empty(), "Encumbrance is not a skill");
        assert!(bank_skill_use(&world, player, 33, 50.0).is_empty());
        assert!(bank_skill_use(&world, 9999, BLADE, 50.0).is_empty());
        world.insert_resource(CharacterRuleset::new(LevelingModel::SKYRIM));
        assert!(bank_skill_use(&world, player, BLADE, 50.0).is_empty());
        assert_eq!(base(&world, player, blade), 5.0);
    }

    /// OSL-API-001 / OSL-API-006 (V14): `Amount` is ranks, and both commands
    /// leave 0.0 banked (Q5; #7310 for AdvancePCSkill), even when no rank is
    /// raised. Both feed the tallies once per rank.
    #[test]
    fn the_script_calls_raise_ranks() {
        let (world, player, blade, _) = world();
        bank_skill_use(&world, player, BLADE, 0.5);
        assert_eq!(raise_skill_ranks(&world, player, BLADE, 2), vec![6, 7]);
        assert_eq!(base(&world, player, blade), 7.0);
        assert_eq!(world.resource::<SkillProgress>().0[&blade], 0.0);
        let level = world.get::<SkillUseLevel>(player).unwrap();
        assert_eq!((level.major_increases, level.attribute_tallies[0]), (2, 2));
        drop(level);
        bank_skill_use(&world, player, BLADE, 1.0);
        assert_eq!(raise_skill_ranks(&world, player, BLADE, 1), vec![8]);
        assert_eq!(world.resource::<SkillProgress>().0[&blade], 0.0);
        // #7310 — no surplus rule: experience banked toward a rank that is not raised is cleared too.
        bank_skill_use(&world, player, BLADE, 1.0);
        assert!(world.resource::<SkillProgress>().0[&blade] > 0.0);
        assert_eq!(raise_skill_ranks(&world, player, BLADE, 0), Vec::<u16>::new());
        assert_eq!(world.resource::<SkillProgress>().0[&blade], 0.0, "banked experience is set to 0.0 (Q5)");
        // A skill stops at 100.
        assert_eq!(raise_skill_ranks(&world, player, BLADE, 500).last(), Some(&100));
        assert_eq!(base(&world, player, blade), 100.0);
    }

    /// OSL-XP-007: emitters only append; the drain applies them.
    #[test]
    fn the_drain_applies_the_pending_records() {
        let (mut world, player, blade, _) = world();
        install(&mut world);
        world.resource_mut::<PendingSkillUse>().0.push(SkillUseRecord { skill: BLADE, amount: 2.0 });
        skill_use_system(&world, Some(player));
        assert_eq!(base(&world, player, blade), 6.0);
        assert!(world.resource::<PendingSkillUse>().0.is_empty());
    }

    /// OSL-API-003 / OSL-CLS-001: `levelup` names the class it read, its major skills, the level
    /// count, the tallies and each skill's banked experience and cost; another model reports nothing.
    #[test]
    fn the_report_names_the_class_and_each_skill() {
        let (mut world, player, _, _) = world();
        bank_skill_use(&world, player, BLADE, 2.0);
        let report = skill_use_report(&world, player).unwrap().join("\n");
        assert!(report.contains("class Test (Combat), major skills: Blade, Block"), "{report}");
        assert!(report.contains("major increases 1/10; attribute tallies: Strength 1,"), "{report}");
        assert!(report.contains("Blade 6 banked 0.70 / cost 1.71 major, specialization"), "{report}");
        assert!(report.contains("Sneak 5 banked 0.00 / cost 2.89"), "{report}");
        world.insert_resource(CharacterRuleset::new(LevelingModel::SKYRIM));
        assert!(skill_use_report(&world, player).is_none());
    }

    /// OSL-USE-001 / OSL-USE-006: a fed skill queues its SKIL amount; an unfed one queues nothing, and
    /// `levelup` lists it; another model queues nothing.
    #[test]
    fn an_emitter_queues_the_skil_amount_and_only_for_fed_skills() {
        let (mut world, player, _, _) = world();
        install(&mut world);
        queue_skill_use(&world, BLADE, 0, 0.0);
        queue_skill_use(&world, 30, 0, 0.0); // Security: no lock-picking game
        queue_skill_use(&world, 15, 0, 0.0); // Block: a blocked hit on the player
        queue_skill_use(&world, 17, 0, 0.0); // Hand to Hand
        queue_skill_use(&world, 99, 0, 0.0);
        assert_eq!(
            world.resource::<PendingSkillUse>().0,
            vec![
                SkillUseRecord { skill: BLADE, amount: 0.5 },
                SkillUseRecord { skill: 15, amount: 1.25 },
                SkillUseRecord { skill: 17, amount: 0.6 },
            ]
        );
        let report = skill_use_report(&world, player).unwrap().join("\n");
        assert!(report.contains("fed skills 16/21; unfed: "), "{report}");
        for unfed in ["Security", "Speechcraft", "Mercantile", "Alchemy", "Armorer"] {
            assert!(report.split("unfed: ").nth(1).unwrap().contains(unfed), "{unfed}: {report}");
        }
        assert!(!report.split("unfed: ").nth(1).unwrap().contains("Blade"));
        assert!(!report.split("unfed: ").nth(1).unwrap().contains("Block"), "Block is fed by a blocked hit on the player: {report}");
        assert!(!report.split("unfed: ").nth(1).unwrap().contains("Marksman"), "Marksman is fed by the player's arrow: {report}");
        for school in ["Alteration", "Conjuration", "Destruction", "Illusion", "Mysticism", "Restoration"] {
            assert!(!report.split("unfed: ").nth(1).unwrap().contains(school), "{school} is fed by the cast: {report}");
        }
        world.resource_mut::<PendingSkillUse>().0.clear();
        world.insert_resource(CharacterRuleset::new(LevelingModel::SKYRIM));
        queue_skill_use(&world, BLADE, 0, 0.0);
        assert!(world.resource::<PendingSkillUse>().0.is_empty());
    }

    /// A world whose player also carries the eight attributes at 40 (Luck 50), Health at `2 × Endurance` and a level.
    fn level_world() -> (World, EntityId) {
        let (mut world, player, ..) = world();
        world.register::<byroredux_core::character::CharacterLevel>();
        let avs: Vec<u32> = (0..9usize)
            .map(|code| crate::obscript_actor_values::actor_value_form(&world, code as f32).unwrap())
            .collect();
        {
            let mut values = world.query_mut::<ActorValues>().unwrap();
            let values = values.get_mut(player).unwrap();
            for (code, av) in avs.iter().enumerate() {
                let base = match code {
                    7 => 50.0,
                    8 => 80.0,
                    _ => 40.0,
                };
                values.set_base(*av, base);
            }
        }
        world.insert(player, byroredux_core::character::CharacterLevel { level: 1, xp: 0 });
        (world, player)
    }

    fn attribute(world: &World, player: EntityId, code: usize) -> f32 {
        let av = crate::obscript_actor_values::actor_value_form(world, code as f32).unwrap();
        base(world, player, av)
    }

    /// OSL-LVL-003 / OSL-LVL-005: ten Blade rank-ups (major) make one pending level whose tallies are frozen with
    /// the tenth in them; ten more make a second.
    #[test]
    fn ten_major_increases_make_a_level_pending() {
        let (world, player) = level_world();
        raise_skill_ranks(&world, player, BLADE, 9);
        assert!(world.get::<SkillUseLevel>(player).unwrap().pending.is_empty());
        raise_skill_ranks(&world, player, BLADE, 1);
        {
            let level = world.get::<SkillUseLevel>(player).unwrap();
            assert_eq!(level.pending.len(), 1);
            assert_eq!(level.pending[0].tallies[0], 10, "Blade is Strength's; the tenth counts (V13)");
            assert_eq!(level.attribute_tallies, [0; 8]);
        }
        raise_skill_ranks(&world, player, BLADE, 10);
        assert_eq!(world.get::<SkillUseLevel>(player).unwrap().pending.len(), 2);
        // A minor skill's rank-up tallies its attribute but never freezes a level.
        raise_skill_ranks(&world, player, SNEAK, 12);
        assert_eq!(world.get::<SkillUseLevel>(player).unwrap().pending.len(), 2);
    }

    /// OSL-ATT-001 / OSL-API-005: three distinct picks; refusals for none pending, a fourth, a repeat, a
    /// non-attribute and an attribute at 100.
    #[test]
    fn a_level_takes_three_distinct_picks() {
        let (world, player) = level_world();
        assert_eq!(pick_attribute(&world, player, 0), Err(PickRefusal::NoPendingLevel));
        raise_skill_ranks(&world, player, BLADE, 10);
        assert_eq!(pick_attribute(&world, player, 0), Ok(PickAccepted { attribute: 0, bonus: 5, picks: 1 }));
        assert_eq!(pick_attribute(&world, player, 0), Err(PickRefusal::AlreadyPicked));
        assert_eq!(pick_attribute(&world, player, 8), Err(PickRefusal::NotAnAttribute));
        assert_eq!(pick_attribute(&world, player, 7), Ok(PickAccepted { attribute: 7, bonus: 1, picks: 2 }));
        let av = crate::obscript_actor_values::actor_value_form(&world, 5.0).unwrap();
        world.query_mut::<ActorValues>().unwrap().get_mut(player).unwrap().set_base(av, 100.0);
        assert_eq!(pick_attribute(&world, player, 5), Err(PickRefusal::AtMaximum));
        assert_eq!(pick_attribute(&world, player, 4), Ok(PickAccepted { attribute: 4, bonus: 1, picks: 3 }));
        assert_eq!(pick_attribute(&world, player, 3), Err(PickRefusal::LevelFull));
    }

    /// OSL-LVL-003 / OSL-ATT-001 / OSL-HP-001 / OSL-HP-003: the level takes nothing until three picks, then each pick
    /// rises by its frozen bonus (the +5 Strength, +1 Luck and Endurance's +1 here), caps at 100, the level
    /// rises, the count falls by ten, and Health's stored term gains a tenth of the Endurance read after the picks;
    /// Magicka and Fatigue stored terms are untouched.
    #[test]
    fn a_picked_level_applies_its_bonuses_and_health() {
        let (world, player) = level_world();
        assert_eq!(apply_pending_level(&world, player), LevelUp::NoPending);
        raise_skill_ranks(&world, player, BLADE, 10);
        assert_eq!(apply_pending_level(&world, player), LevelUp::NotReady { picks: 0 });
        for code in [0, 5, 7] {
            pick_attribute(&world, player, code).unwrap();
        }
        // Make Endurance 49 so the +1 bonus (no Endurance rank-ups were earned) makes it 50: a tenth is 5.
        let end = crate::obscript_actor_values::actor_value_form(&world, 5.0).unwrap();
        world.query_mut::<ActorValues>().unwrap().get_mut(player).unwrap().set_base(end, 49.0);
        let outcome = apply_pending_level(&world, player);
        let LevelUp::Applied { level, raised, health_gain } = outcome else { panic!("{outcome:?}") };
        assert_eq!(level, 2);
        assert_eq!(raised, vec![(0, 40.0, 45.0), (5, 49.0, 50.0), (7, 50.0, 51.0)]);
        assert_eq!(health_gain, 5.0, "Endurance after the picks (V8): floor(0.1 × 50)");
        assert_eq!(attribute(&world, player, 0), 45.0);
        let health = crate::obscript_actor_values::actor_value_form(&world, 8.0).unwrap();
        let magicka = crate::obscript_actor_values::actor_value_form(&world, 9.0).unwrap();
        let values = world.get::<ActorValues>(player).unwrap();
        assert_eq!(values.stored(health), 5.0);
        assert_eq!(values.stored(magicka), 0.0);
        drop(values);
        let level = world.get::<SkillUseLevel>(player).unwrap();
        assert!(level.pending.is_empty());
        assert_eq!(level.major_increases, 0, "ten taken");
    }

    /// OSL-ATT-001: the bonus a pick takes is the frozen tally's, a cap of 100 holds, and an unspent tally rolls
    /// over to nothing: rank-ups after the freeze change nothing about this level.
    #[test]
    fn the_bonus_is_frozen_and_capped() {
        let (world, player) = level_world();
        raise_skill_ranks(&world, player, BLADE, 10);
        // Rank-ups after the freeze (three more of Blade, Strength's) do not enter the pending level.
        raise_skill_ranks(&world, player, BLADE, 3);
        for code in [0, 4, 7] {
            pick_attribute(&world, player, code).unwrap();
        }
        let strength = crate::obscript_actor_values::actor_value_form(&world, 0.0).unwrap();
        world.query_mut::<ActorValues>().unwrap().get_mut(player).unwrap().set_base(strength, 98.0);
        let LevelUp::Applied { raised, .. } = apply_pending_level(&world, player) else { panic!() };
        assert_eq!(raised[0], (0, 98.0, 100.0), "+5 capped at 100");
        let level = world.get::<SkillUseLevel>(player).unwrap();
        assert_eq!(level.attribute_tallies[0], 3, "the post-freeze rank-ups wait for the next level");
        assert_eq!(level.major_increases, 3, "thirteen less the ten the level took");
    }

    /// OSL-LVL-006: with every attribute at 100 there is no level, and the pending one stays.
    #[test]
    fn perfection_cannot_be_improved_upon() {
        let (world, player) = level_world();
        raise_skill_ranks(&world, player, BLADE, 10);
        for code in [0, 5, 7] {
            pick_attribute(&world, player, code).unwrap();
        }
        for code in 0..8usize {
            let av = crate::obscript_actor_values::actor_value_form(&world, code as f32).unwrap();
            world.query_mut::<ActorValues>().unwrap().get_mut(player).unwrap().set_base(av, 100.0);
        }
        assert_eq!(apply_pending_level(&world, player), LevelUp::Perfection);
        assert_eq!(world.get::<byroredux_core::character::CharacterLevel>(player).unwrap().level, 1);
        assert_eq!(world.get::<SkillUseLevel>(player).unwrap().pending.len(), 1);
    }

    /// OSL-ATT-002: Health, Magicka and Fatigue follow the raised attributes through their formulas: here that
    /// is the formulas' own business (`player_derived_stats_system`); the level's job is only to write the
    /// attributes' base, which `a_picked_level_applies_its_bonuses_and_health` pins.
    /// OSL-API-002: `AdvancePCLevel` freezes the tallies as they stand into a pending level.
    #[test]
    fn advance_level_makes_a_level_pending_now() {
        let (world, player) = level_world();
        raise_skill_ranks(&world, player, BLADE, 4);
        assert!(advance_level(&world, player));
        let level = world.get::<SkillUseLevel>(player).unwrap();
        assert_eq!(level.pending.len(), 1);
        assert_eq!(level.pending[0].tallies[0], 4);
        drop(level);
        let (mut other, player2) = level_world();
        other.insert_resource(CharacterRuleset::new(LevelingModel::SKYRIM));
        assert!(!advance_level(&other, player2));
    }

    /// OSL-API-003: `levelup` lists each pending level with the bonuses it offers and the picks made.
    #[test]
    fn the_report_lists_pending_levels() {
        let (world, player) = level_world();
        raise_skill_ranks(&world, player, BLADE, 10);
        pick_attribute(&world, player, 0).unwrap();
        let report = skill_use_report(&world, player).unwrap().join("\n");
        assert!(report.contains("pending level 1 offers Strength +5, Intelligence +1"), "{report}");
        assert!(report.contains("picked 1/3 (Strength); `levelup attr <attribute>` picks"), "{report}");
    }
}
