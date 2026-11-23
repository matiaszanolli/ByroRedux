//! M47.2 — `Game.AdvanceSkill(asSkillName, afMagnitude)`: skill usage XP for the
//! player's skill.
//!
//! The CK page: "Advances the progress of the provided Skill by the given
//! amount (for the player only)... in Skill Usage amounts, so it will count
//! towards skill progression but won't necessarily change the Skill itself...
//! If the new amount exceeds the skill's level threshold, the skill will
//! advance, and possibly level up. This amount is automatically modified by
//! any active effects such as a Guardian Stone."
//!
//! UESP *Skyrim:Leveling* gives the arithmetic, from the skill's AVIF `AVSK`
//! tuple:
//! - the XP an amount grants is `amount × Skill Use Mult + Skill Use Offset`
//!   (the `AdvSkill` console command's rule);
//! - raising a skill from level `L` costs `Skill Improve Mult × L^fSkillUseCurve
//!   + Skill Improve Offset`, with `fSkillUseCurve` 1.95;
//! - the skill's maximum is 100.
//!
//! A rank-up also feeds character XP (`LevelingModel::xp_from_skill_rank`),
//! which the host applies through the shared level roll. The Guardian Stone
//! style modifiers are not modelled.

use std::collections::HashMap;

use byroredux_core::character::{skyrim_skill_xp_to_next, SKYRIM_SKILL_USE_CURVE};
use byroredux_core::ecs::components::ActorValues;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

/// The maximum value of a Skyrim skill (UESP *Skyrim:Leveling*: "the maximum
/// level was 81, since there was no way to gain any more experience once every
/// skill reached its maximum value of 100").
pub const SKILL_CAP: u16 = 100;

/// Each skill AVIF's `AVSK` tuple `[use mult, use offset, improve mult, improve
/// offset]`, by AVIF FormID, and the cost curve's exponent. Derived from the
/// load order; not saved.
#[derive(Debug, Clone)]
pub struct SkillScaling {
    pub skills: HashMap<u32, [f32; 4]>,
    /// `fSkillUseCurve`, the exponent of a rank's cost: the load order's
    /// setting, else [`SKYRIM_SKILL_USE_CURVE`] (LMC-SET-002).
    pub use_curve: f32,
}

impl Default for SkillScaling {
    fn default() -> Self {
        Self { skills: HashMap::new(), use_curve: SKYRIM_SKILL_USE_CURVE }
    }
}

impl Resource for SkillScaling {}

impl SkillScaling {
    pub fn from_index(index: &EsmIndex) -> Self {
        Self {
            skills: index
                .actor_values
                .values()
                .filter_map(|avif| Some((avif.form_id, avif.skill_scaling?)))
                .collect(),
            use_curve: index
                .game_setting_float("fSkillUseCurve")
                .filter(|curve| curve.is_finite())
                .unwrap_or(SKYRIM_SKILL_USE_CURVE),
        }
    }
}

/// The usage XP each skill has banked toward its next level, by AVIF FormID.
/// Saved.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct SkillProgress(pub std::collections::BTreeMap<u32, f32>);

impl Resource for SkillProgress {}

/// `AdvanceSkill` calls scripts made, drained by the host's skill system.
/// Transient.
#[derive(Debug, Default)]
pub struct PendingSkillAdvances(pub Vec<(String, f32)>);

impl Resource for PendingSkillAdvances {}

/// #5802 — this subsystem's world setup, owned here instead of the crate's
/// `register` hub: `Game.AdvanceSkill`'s banked usage XP (saved) and the
/// advances scripts queued (transient).
pub fn install(world: &mut World) {
    world.init_resource::<SkillProgress>();
    world.init_resource::<PendingSkillAdvances>();
}

/// #5802 — this subsystem's saved column. Called by the binary's
/// `build_save_registry` at its place in the row order (the schema
/// fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource_codec("SkillProgress",
        crate::actor_value_save::save_skill_progress, crate::actor_value_save::load_skill_progress);
}

/// Advance the player's skill `name` by `magnitude` usage units. Returns the
/// levels the skill reached, in order (empty when it only banked XP, or when
/// the skill is unknown, absent from the player, or capped).
pub fn advance_skill(world: &World, player: byroredux_core::ecs::storage::EntityId, name: &str, magnitude: f32) -> Vec<u16> {
    let Some(skill) = world
        .try_resource::<crate::ActorValueNames>()
        .and_then(|names| names.resolve(name))
    else {
        log::debug!("AdvanceSkill: no actor value named '{name}'");
        return Vec::new();
    };
    let Some(([use_mult, use_offset, improve_mult, improve_offset], use_curve)) = world
        .try_resource::<SkillScaling>()
        .and_then(|scaling| Some((*scaling.skills.get(&skill)?, scaling.use_curve)))
    else {
        log::debug!("AdvanceSkill: '{name}' is not a skill");
        return Vec::new();
    };
    // The XP the usage amount grants.
    let gained = magnitude.max(0.0) * use_mult + use_offset;
    let mut progress = world
        .try_resource::<SkillProgress>()
        .and_then(|progress| progress.0.get(&skill).copied())
        .unwrap_or(0.0)
        + gained;
    let mut reached = Vec::new();
    {
        let Some(mut values) = world.query_mut::<ActorValues>() else {
            return Vec::new();
        };
        let Some(values) = values.get_mut(player) else {
            return Vec::new();
        };
        let Some(entry) = values.get_base(skill) else {
            log::debug!("AdvanceSkill: the player has no '{name}' skill");
            return Vec::new();
        };
        let mut level = entry.base.max(0.0) as u16;
        loop {
            if level >= SKILL_CAP {
                progress = 0.0;
                break;
            }
            let cost = skyrim_skill_xp_to_next(level, improve_mult, improve_offset, use_curve);
            if cost <= 0.0 || progress < cost {
                break;
            }
            progress -= cost;
            level += 1;
            reached.push(level);
        }
        if let Some(&last) = reached.last() {
            values.set_base(skill, f32::from(last));
        }
    }
    if let Some(mut stored) = world.try_resource_mut::<SkillProgress>() {
        stored.0.insert(skill, progress);
    }
    // M43 — the Story Manager's `SKIL` (Skill Increase). CK *OnStoryIncreaseSkill*: "If the skill has
    // gained multiple levels at the same time, this event still gets triggered only once." Its one
    // parameter is the skill's name, which no event slot carries.
    if !reached.is_empty() {
        crate::story_manager::raise_story_event(
            world,
            player,
            crate::story_manager::StoryEvent {
                mnemonic: *b"SKIL",
                reference_1: player,
                reference_2: None,
                location_1: None,
                location_2: None,
                scripted: None,
            },
        );
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::storage::EntityId;
    use byroredux_plugin::esm::records::AvifRecord;

    const LOCKPICKING: u32 = 0x0000_0F99;

    /// A world whose player has Lockpicking `level`; Lockpicking's AVSK is the
    /// UESP example (use mult 45, use offset 10, improve mult 0.25, offset 300).
    fn world_with_lockpicking(level: f32) -> (World, EntityId) {
        world_with_lockpicking_in(level, EsmIndex::default())
    }

    /// [`world_with_lockpicking`] over `index`, which may author settings.
    fn world_with_lockpicking_in(level: f32, mut index: EsmIndex) -> (World, EntityId) {
        let mut world = World::new();
        index.actor_values.insert(
            LOCKPICKING,
            AvifRecord {
                form_id: LOCKPICKING,
                editor_id: "AVLockpicking".into(),
                skill_scaling: Some([45.0, 10.0, 0.25, 300.0]),
                ..Default::default()
            },
        );
        crate::install_actor_value_names(&mut world, &index);
        world.insert_resource(SkillProgress::default());
        world.register::<ActorValues>();
        let player = world.spawn();
        world.insert(player, ActorValues::from_pairs([(LOCKPICKING, level)]));
        (world, player)
    }

    /// A skill that gains several levels raises `SKIL` once; one that only banks XP raises nothing.
    #[test]
    fn a_skill_increase_raises_one_story_event() {
        let (mut world, player) = world_with_lockpicking(15.0);
        crate::story_manager::register(&mut world);
        let raised = |world: &World| {
            world
                .query::<crate::story_manager::StoryEvent>()
                .map(|events| events.iter().filter(|(_, e)| e.mnemonic == *b"SKIL").count())
                .unwrap_or(0)
        };
        advance_skill(&world, player, "Lockpicking", 0.0);
        assert_eq!(raised(&world), 0, "banked XP only");
        assert!(advance_skill(&world, player, "Lockpicking", 50.0).len() >= 5);
        assert_eq!(raised(&world), 1, "several levels, one event");
    }

    /// UESP's own example: 50 XP of Lockpicking is `50 × 45 + 10 = 2260` skill
    /// XP, and 15 → 16 costs `0.25 × 15^1.95 + 300 ≈ 349.13`. So 2260 clears
    /// levels 15 through 21 and leaves the remainder banked.
    #[test]
    fn usage_xp_clears_levels_by_the_published_curve() {
        let (world, player) = world_with_lockpicking(15.0);
        let reached = advance_skill(&world, player, "Lockpicking", 50.0);
        let cost = |level: u16| 0.25 * f32::from(level).powf(1.95) + 300.0;
        let mut left = 2260.0f32;
        let mut expected = Vec::new();
        let mut level = 15;
        while left >= cost(level) {
            left -= cost(level);
            level += 1;
            expected.push(level);
        }
        assert_eq!(reached, expected);
        assert!(reached.len() >= 5, "{reached:?}");
        let progress = world.resource::<SkillProgress>().0[&LOCKPICKING];
        assert!((progress - left).abs() < 0.01, "{progress} vs {left}");
        let stored = world.get::<ActorValues>(player).unwrap().get(LOCKPICKING).unwrap().base;
        assert_eq!(stored, f32::from(*reached.last().unwrap()));
    }

    /// LMC-SET-002 — a load order that retunes `fSkillUseCurve` changes every rank's cost: at a curve of 1.0 Lockpicking
    /// 15 → 16 costs `0.25 × 15 + 300 = 303.75`, so the same 2260 XP clears more levels than at the default 1.95.
    #[test]
    fn the_load_orders_skill_use_curve_sets_the_cost() {
        use byroredux_plugin::esm::records::{GameSetting, SettingValue};
        let mut index = EsmIndex::default();
        index.insert_game_setting(GameSetting { form_id: 0x0000_0123, editor_id: "fSkillUseCurve".into(), value: SettingValue::Float(1.0) });
        let (world, player) = world_with_lockpicking_in(15.0, index);
        assert_eq!(world.resource::<SkillScaling>().use_curve, 1.0);
        let reached = advance_skill(&world, player, "Lockpicking", 50.0);
        let cost = |level: u16| 0.25 * f32::from(level) + 300.0;
        let (mut left, mut level, mut expected) = (2260.0f32, 15u16, Vec::new());
        while left >= cost(level) {
            left -= cost(level);
            level += 1;
            expected.push(level);
        }
        assert_eq!(reached, expected);
        let (default_world, default_player) = world_with_lockpicking(15.0);
        assert!(reached.len() > advance_skill(&default_world, default_player, "Lockpicking", 50.0).len());
    }

    #[test]
    fn a_small_advance_only_banks_xp_and_a_capped_skill_banks_nothing() {
        let (world, player) = world_with_lockpicking(15.0);
        assert!(advance_skill(&world, player, "Lockpicking", 1.0).is_empty());
        assert_eq!(world.resource::<SkillProgress>().0[&LOCKPICKING], 55.0);
        assert!(advance_skill(&world, player, "Sneak", 5.0).is_empty(), "not an actor value");
        let (world, player) = world_with_lockpicking(100.0);
        assert!(advance_skill(&world, player, "Lockpicking", 500.0).is_empty());
        assert_eq!(world.resource::<SkillProgress>().0[&LOCKPICKING], 0.0);
    }
}
