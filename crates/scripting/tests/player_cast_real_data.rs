//! Player-spellcasting PRD, Phase 1 (#7079): the cast rules against the shipped `Oblivion.esm`, end to end
//! (`parse_esm` → `canonical_spell` → [`SpellCastModel`] → the player's cast), on the three spells the PRD's §7.2 names.
//!
//! **Opt-in / `#[ignore]`d** — needs Oblivion.esm on disk:
//! ```bash
//! cargo test -p byroredux-scripting --test player_cast_real_data -- --ignored --nocapture
//! ```
//! Skips gracefully (passing) when the game data isn't present; `BYROREDUX_REQUIRE_GAME_DATA=1` fails instead.

use std::path::PathBuf;

use byroredux_core::character::SpellCastModel;
use byroredux_core::ecs::components::ActorValues;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::reader::{GameKind, GlobalSlot};
use byroredux_plugin::esm::records::{EsmIndex, SpellType};
use byroredux_plugin::esm::test_paths;
use byroredux_scripting::actor_value_names::{condition_actor_value_form, install_actor_value_names};
use byroredux_scripting::magic::{canonical_spell, SpellCatalog, SpellList};
use byroredux_scripting::player_cast::{self, CastRefusal, CastState, ReadiedSpell};

mod common;

/// Heat Shell: Fire Shield 10 pts for 30 s on Self, Alteration (`StandardFireShield2Apprentice`).
const HEAT_SHELL: u32 = 0x000A_9806;
/// Flare: Fire Damage 6 pts on Target, Destruction; stored cost 9, equation 11.15 (`StandardFireDamageTarget1Novice`).
const FLARE: u32 = 0x000A_97DF;
/// Searing Grasp: Fire Damage 45 pts on Touch, a Journeyman spell (`StandardFireDamageTouch3Journeyman`).
const SEARING_GRASP: u32 = 0x000A_97E5;
/// Mirili's Expert Command: Command Creature on Touch, Illusion, Manual Spell Cost, stored 368 and Expert
/// (`SE37CommandCreature4Expert`).
const MIRILI: u32 = 0x0007_B21B;

fn oblivion_index() -> Option<EsmIndex> {
    let dir = std::env::var(test_paths::OBLIVION_ENV).unwrap_or_else(|_| test_paths::OBLIVION_DEFAULT.into());
    let bytes = std::fs::read(PathBuf::from(dir).join("Oblivion.esm")).ok()?;
    let mut index = byroredux_plugin::esm::parse_esm(&bytes).ok()?;
    assert_eq!(index.game, GameKind::Oblivion);
    // The loader gives the engine's records the lowest slot no record occupies.
    let free = (0u8..=0xfb).find(|slot| !index.record_types.keys().any(|id| (id >> 24) as u8 == *slot))?;
    index.install_oblivion_actor_values(GlobalSlot::Regular(free)).ok()?;
    Some(index)
}

/// The installed world and a player with Magicka 200, Fatigue 100, Luck 50 and every school at `skill`.
fn world_with(index: &EsmIndex, skill: f32) -> (World, EntityId, SpellCastModel) {
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.register::<ReadiedSpell>();
    install_actor_value_names(&mut world, index);
    world.insert_resource(SpellCatalog::from_index(index));
    let model = SpellCastModel::OBLIVION.with_gmst(|name| index.game_setting_float(name));
    world.insert_resource(model);
    world.insert_resource(CastState::default());
    let av = |world: &World, code: u32| condition_actor_value_form(world, code).expect("an Oblivion actor value");
    let mut pairs = vec![(av(&world, 7), 50.0), (av(&world, 9), 200.0), (av(&world, 10), 100.0)];
    pairs.extend((20..26).map(|code| (av(&world, code), skill)));
    let player = world.spawn();
    world.insert(player, ActorValues::from_pairs(pairs));
    world.insert(player, SpellList(vec![HEAT_SHELL, FLARE, SEARING_GRASP, MIRILI]));
    (world, player, model)
}

fn data() -> Option<EsmIndex> {
    let index = oblivion_index();
    if index.is_none() {
        common::require_or_skip("Oblivion.esm", &"the Oblivion data directory");
        eprintln!("SKIP: Oblivion.esm not found");
    }
    index
}

#[test]
#[ignore = "needs Oblivion.esm on disk"]
fn the_models_settings_are_the_masters() {
    let Some(index) = data() else { return };
    let model = SpellCastModel::OBLIVION.with_gmst(|name| index.game_setting_float(name));
    assert_eq!(model.skill_cost_base, 0.2);
    assert_eq!(model.skill_cost_mult, 1.2);
    assert_eq!(model.magnitude_exponent, 1.28);
    assert_eq!(model.spell_level_min, [25.5, 62.5, 149.5, 399.5]);
    assert_eq!(model.fatigue_cast_base, 1.0);
    assert_eq!(model.fatigue_cast_mult, 0.0);
    // The hand's reach, the CS wiki's "about 5 feet or 105 units" (no setting retunes it).
    assert!((model.touch_reach(1.0) - 105.0).abs() < 1e-3);
    // The spell projectile's base speed and range (CS wiki *fMagicProjectileBaseSpeed*, *fMagicProjectileMaxDistance*).
    assert_eq!((model.projectile_base_speed, model.projectile_max_distance), (1000.0, 10000.0));
}

/// `cast_cost_follows_the_equation` (PRD §7.2 scenario 1): Flare costs 11, not its stored 9, and the cost falls with the skill.
#[test]
#[ignore = "needs Oblivion.esm on disk"]
fn cast_cost_follows_the_equation_not_the_stored_cost() {
    let Some(index) = data() else { return };
    let (world, player, _) = world_with(&index, 33.0);
    let flare = index.spells.get(&FLARE).expect("Flare");
    assert_eq!(flare.cost, 9, "the stored SPIT cost is stale (UESP lists 11)");
    // Skill 33 and Luck 50: the modifier is 1.004, so floor(11.148 * 1.004) = 11.
    let preview = player_cast::preview(&world, player, FLARE).expect("priced");
    assert_eq!(preview.cost, 11.0);
    assert_eq!(preview.school, Some(2), "Destruction");
    // Heat Shell (Self): the equation gives 54.3 at the baseline cost.
    assert_eq!(player_cast::preview(&world, player, HEAT_SHELL).unwrap().cost, 54.0);
    // The same spell at skill 5 and at skill 100 (flare: 14 and 2).
    for (skill, expected) in [(5.0, 14.0), (100.0, 2.0)] {
        let (world, player, _) = world_with(&index, skill);
        assert_eq!(player_cast::preview(&world, player, FLARE).unwrap().cost, expected, "skill {skill}");
    }
    // Mirili's Expert Command is Manual Spell Cost: stored 368 times the Illusion modifier, rounded down (CS wiki *Spell Cost*).
    let (world, player, _) = world_with(&index, 50.0);
    let mirili = player_cast::preview(&world, player, MIRILI).unwrap();
    assert_eq!(mirili.cost, (368.0_f32 * (1.4 - 0.012 * 50.0)).floor(), "{}", mirili.cost);
    assert_eq!(mirili.school, Some(3), "Illusion");
}

/// `cast_mastery_is_the_tier_of_the_base_cost` (PRD §7.2 scenario 1).
#[test]
#[ignore = "needs Oblivion.esm on disk"]
fn cast_mastery_is_the_tier_of_the_base_cost() {
    let Some(index) = data() else { return };
    let (world, player, _) = world_with(&index, 0.0);
    let tier = |spell| player_cast::preview(&world, player, spell).unwrap().tier;
    assert_eq!(tier(FLARE), 0, "Novice");
    assert_eq!(tier(SEARING_GRASP), 2, "Journeyman");
    assert_eq!(tier(MIRILI), 3, "a Manual Spell Cost spell reads its stored Expert level");
}

/// The Phase 1 exit: a Self cast spends Magicka and Fatigue on `Oblivion.esm`, and a spell the player is too unskilled for
/// is refused without spending.
#[test]
#[ignore = "needs Oblivion.esm on disk"]
fn a_self_cast_spends_magicka_and_fatigue_on_the_real_master() {
    let Some(index) = data() else { return };
    // Heat Shell's base cost is 54, an Apprentice spell: at skill 0 (a Novice) it is refused and nothing is spent.
    let (world, player, _) = world_with(&index, 0.0);
    let (magicka, fatigue) = (condition_actor_value_form(&world, 9).unwrap(), condition_actor_value_form(&world, 10).unwrap());
    let current = |world: &World, player, av: u32| world.get::<ActorValues>(player).unwrap().current(av);
    player_cast::ready_spell(&world, player, HEAT_SHELL).expect("Heat Shell is castable");
    let report = player_cast::cast_readied(&world, player, true, 0.45, &|| None);
    assert_eq!(report.outcome, Err(CastRefusal::Mastery { school: Some(0), needs: 1, has: 0 }));
    assert_eq!((current(&world, player, magicka), current(&world, player, fatigue)), (200.0, 100.0));

    // At skill 25 the player is an Apprentice: floor(54.3 * (1.4 - 0.012 * 25)) = 59 Magicka and 1 Fatigue.
    let (mut world, player, _) = world_with(&index, 25.0);
    world.insert_resource(byroredux_core::character::CharacterRuleset::new(byroredux_core::character::LevelingModel::OBLIVION));
    byroredux_scripting::oblivion_skill_use::install(&mut world);
    player_cast::ready_spell(&world, player, HEAT_SHELL).unwrap();
    let started = player_cast::cast_readied(&world, player, true, 0.45, &|| None).outcome.expect("an Apprentice casts it");
    assert_eq!((started.magicka, started.fatigue, started.school), (59.0, 1.0, Some(0)));
    assert!(started.effects_started, "Fire Shield lands on the caster");
    assert_eq!(current(&world, player, magicka), 200.0 - 59.0);
    assert_eq!(current(&world, player, fatigue), 100.0 - 1.0);
    // The Self cast fed its school once: Alteration (code 20), the SKIL second use value, 4.0 (PSC-USE-001).
    assert_eq!(started.fed, Some(0));
    let queued: Vec<_> = world.resource::<byroredux_scripting::oblivion_skill_use::PendingSkillUse>().0.iter().map(|record| (record.skill, record.amount)).collect();
    assert_eq!(queued, vec![(20, 4.0)]);
}

#[test]
#[ignore = "needs Oblivion.esm on disk"]
fn every_vanilla_spell_carries_cast_data_and_a_finite_cost() {
    let Some(index) = data() else { return };
    let model = SpellCastModel::OBLIVION.with_gmst(|name| index.game_setting_float(name));
    let (mut spells, mut ranges_known) = (0usize, 0usize);
    for (&form_id, record) in &index.spells {
        if record.spell_type != SpellType::Spell {
            continue;
        }
        spells += 1;
        let canonical = canonical_spell(&index, form_id);
        let Some(data) = canonical.cast else { continue };
        ranges_known += 1;
        let cost = model.actual_cost(&data.effects, data.manual().map(|(cost, _)| cost), |_| 50.0, 50.0);
        assert!(cost.is_finite() && cost >= 0.0, "{form_id:08X} {}: cost {cost}", data.name);
        assert!(model.spell_tier(&data.effects, data.manual()) <= 4);
    }
    assert!(spells > 500, "{spells} Spell records");
    assert_eq!(ranges_known, spells, "every Oblivion Spell carries its effects' ranges");
}

/// Phase 2 exit (PRD §9): a Touch spell lands on the actor in reach and a miss still pays; a Target spell launches at the
/// speed of its `MGEF`; an area effect is reported ignored; all on `Oblivion.esm`.
#[test]
#[ignore = "needs Oblivion.esm on disk"]
fn touch_and_target_resolve_on_the_real_master() {
    let Some(index) = data() else { return };
    let (mut world, player, _) = world_with(&index, 100.0);
    world.register::<byroredux_scripting::magic_effects::ActiveMagicEffects>();
    let dummy = world.spawn();
    let current = |world: &World, av: u32| world.get::<ActorValues>(player).unwrap().current(condition_actor_value_form(world, av).unwrap());
    // Searing Grasp (Fire Damage 45 on Touch): lands on the dummy in reach, not on the player.
    player_cast::ready_spell(&world, player, SEARING_GRASP).unwrap();
    let report = player_cast::cast_readied(&world, player, true, 0.45, &|| Some(dummy));
    let started = report.outcome.clone().expect("a Journeyman at skill 100 casts it");
    assert_eq!(started.touched, Some(dummy), "{report}");
    assert!(started.effects_started && !started.no_target);
    assert!(world.get::<byroredux_scripting::magic_effects::ActiveMagicEffects>(dummy).is_some());
    assert!(world.get::<byroredux_scripting::magic_effects::ActiveMagicEffects>(player).is_none(), "a Touch spell is not a Self spell");
    let after_hit = current(&world, 9);
    assert!(after_hit < 200.0);
    // Out of reach: no target, the same cost paid.
    player_cast::tick_cast_state(&world, 1.0);
    let report = player_cast::cast_readied(&world, player, true, 0.45, &|| None);
    assert!(report.outcome.as_ref().unwrap().no_target, "{report}");
    assert_eq!(after_hit - current(&world, 9), 200.0 - after_hit, "a miss pays what a hit pays");
    // Flare (Fire Damage 6 on Target): one projectile, at its MGEF's speed, and nothing started at the release.
    player_cast::tick_cast_state(&world, 1.0);
    player_cast::ready_spell(&world, player, FLARE).unwrap();
    let report = player_cast::cast_readied(&world, player, true, 0.45, &|| Some(dummy));
    let launch = report.outcome.as_ref().unwrap().launch.expect("a Target spell launches");
    let speed = index.spells[&FLARE].effects[0].effect_form_id;
    let mgef = index.resolve_magic_effect(speed).expect("Flare's effect resolves");
    assert_eq!(launch.speed, 1000.0 * mgef.projectile_speed, "base speed times the MGEF's multiplier");
    assert!(launch.speed > 0.0, "{}", mgef.editor_id);
    assert_eq!(launch.range, 10000.0);
    assert!(!report.outcome.as_ref().unwrap().effects_started);
    // A vanilla spell with a Touch or Target effect that has an area reports it ignored and does not land it.
    let area_spell = index
        .spells
        .iter()
        .filter(|(_, record)| record.spell_type == SpellType::Spell)
        .filter_map(|(&form_id, _)| canonical_spell(&index, form_id).cast.map(|data| (form_id, data)))
        .find(|(_, data)| data.effects.iter().all(|effect| effect.range != byroredux_core::character::EffectRange::SelfRange) && data.effects.iter().any(|effect| effect.area > 0.0))
        .map(|(form_id, _)| form_id)
        .expect("Oblivion.esm has area spells");
    let (mut world, player, _) = world_with(&index, 100.0);
    world.register::<byroredux_scripting::magic_effects::ActiveMagicEffects>();
    let dummy = world.spawn();
    world.query_mut::<byroredux_scripting::magic::SpellList>().unwrap().get_mut(player).unwrap().0.push(area_spell);
    // Mastery can stop a Master spell at skill 100 only above 399 base cost; the cast reaches the resolve step or is refused by name.
    match player_cast::ready_spell(&world, player, area_spell) {
        Ok(_) => {
            let report = player_cast::cast_readied(&world, player, true, 0.45, &|| Some(dummy));
            if let Ok(started) = &report.outcome {
                assert!(started.area_ignored > 0, "{report}");
            }
        }
        Err(refusal) => panic!("{area_spell:08X}: {refusal}"),
    }
}
