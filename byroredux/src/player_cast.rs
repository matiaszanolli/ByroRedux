//! The player's cast action's frontend (player-spellcasting PRD, `docs/prd/player-spellcasting.md`; Phase 1, #7079).
//!
//! The gate, the spend and the readied spell are `byroredux_scripting::player_cast`'s; this module is the input half
//! (the `CastSpell`, `NextSpell` and `PreviousSpell` actions, one system) and the install of the rules the player's
//! profile carries. The system is the one owner of the spend (PSC-CST-009): the input path only sets the edge, and the
//! console's `spell.cast` runs the same [`cast_now`].

use byroredux_core::character::SpellCastModel;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_plugin::esm::records::EsmIndex;
use byroredux_scripting::player_cast::{self, CastReport, TargetLaunch};

use crate::interaction::{ActionState, InputAction};
use crate::systems::{PlayerEntity, PlayerMode};

/// The casting interval until a cast animation exists: the melee cooldown (PSC-CST-006). Provisional, and said so in
/// `cast.status`.
pub(crate) const CAST_INTERVAL_SECONDS: f32 = crate::combat::MELEE_COOLDOWN_SECONDS;

/// Install (or clear) the [`SpellCastModel`] for a freshly loaded index: the profile's `spell_cast` row decides whether
/// the game has a player cast action at all (Oblivion), the load order's settings retune it.
pub(crate) fn install_spell_cast_model(world: &mut World, index: &EsmIndex) {
    match index.character_rules.spell_cast() {
        Some(model) => {
            world.insert_resource(model.with_gmst(|name| index.game_setting_float(name)));
        }
        None => {
            world.remove_resource::<SpellCastModel>();
        }
    }
}

/// Whether the player may cast right now: alive, not in VATS, combat controls not locked by a script
/// (#7337), and in character mode.
fn can_cast(world: &World) -> bool {
    world.try_resource::<PlayerMode>().is_some_and(|mode| *mode == PlayerMode::Character)
        && crate::systems::player_may_fight(world)
}

/// The first live actor hit by the camera ray within the configured activation pick length (PSC-TGT-002).
fn touch_target(world: &World, player: EntityId) -> Option<EntityId> {
    let reach = world.try_resource::<SpellCastModel>()?.touch_pick_length;
    let (origin, direction) = crate::interaction::camera_ray(world)?;
    crate::combat::actor_in_reach(world, player, origin, direction, reach, 0.0)
        .ok()
        .map(|(actor, _)| actor)
}

/// One cast action for the player: the key's and `spell.cast`'s common path. A Target spell's projectile is launched
/// here, along the camera's aim, when the cast started (PSC-TGT-003).
pub(crate) fn cast_now(world: &World) -> Option<CastReport> {
    let player = PlayerEntity::get(world)?;
    let report = player_cast::cast_readied(world, player, can_cast(world), CAST_INTERVAL_SECONDS, &|| touch_target(world, player));
    if let Ok(started) = &report.outcome
        && let Some(launch) = started.launch
        && !launch_spell(world, player, launch)
    {
        // No aim, or no projectile storage: the Target effects cannot fly, which is a miss.
        player_cast::missed_target(world, launch.spell);
    }
    Some(report)
}

/// Fly the Target effects of `launch` from the camera along its aim; `false` when nothing could be launched.
fn launch_spell(world: &World, player: EntityId, launch: TargetLaunch) -> bool {
    let Some((origin, direction)) = crate::interaction::camera_ray(world) else { return false };
    let Some(shot) = crate::systems::projectiles::spell_shot(world, player, launch, origin, direction) else { return false };
    let Some(mut active) = world.try_resource_mut::<crate::systems::projectiles::ActiveProjectiles>() else { return false };
    active.0.push(shot);
    true
}

/// The cast action: advance the casting interval, keep the readied spell on the player's list, and act on the three
/// edges (PSC-CST-001, PSC-RDY-003). A game with no cast rules ignores them.
pub(crate) fn player_cast_system(world: &World, dt: f32) {
    player_cast::tick_cast_state(world, dt);
    let Some(player) = PlayerEntity::get(world) else { return };
    // PSC-RDY-001: a spell that left the list clears from the ready slot.
    let _ = player_cast::readied(world, player);
    let (cast, next, previous) = world
        .try_resource::<ActionState>()
        .map(|actions| {
            (
                actions.was_pressed(InputAction::CastSpell),
                actions.was_pressed(InputAction::NextSpell),
                actions.was_pressed(InputAction::PreviousSpell),
            )
        })
        .unwrap_or_default();
    if !(cast || next || previous) || world.try_resource::<SpellCastModel>().is_none() {
        return;
    }
    if next || previous {
        let step = if next { 1 } else { -1 };
        match player_cast::cycle_spell(world, player, step) {
            Ok(preview) => log::info!("spell: readied {} ({}, costs {:.0})", preview.name, player_cast::school_name(preview.school), preview.cost),
            Err(refusal) => log::info!("spell: nothing to ready ({refusal})"),
        }
    }
    if cast {
        cast_now(world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::InputState;
    use crate::interaction::ActionBindings;
    use byroredux_core::ecs::components::ActorValues;
    use byroredux_core::ecs::storage::EntityId;
    use byroredux_plugin::esm::records::SpellType;
    use byroredux_scripting::magic::{CanonicalSpell, SpellCastData, SpellCatalog, SpellList};
    use byroredux_scripting::magic_effects::{ActiveMagicEffects, CastEffect};
    use byroredux_scripting::player_cast::{CastState, ReadiedSpell};
    use std::collections::HashMap;
    use winit::keyboard::KeyCode;

    const SPELL: u32 = 0x10;
    const SECOND: u32 = 0x11;
    // Oblivion's Luck, Magicka and Fatigue and the six schools, keyed by their codes (no game installed passes a code through).
    const MAGICKA: u32 = 9;

    fn effect(school: u8) -> byroredux_core::character::EffectCostInput {
        byroredux_core::character::EffectCostInput {
            base_cost: 5.0,
            magnitude: 10.0,
            duration: 0.0,
            area: 0.0,
            range: byroredux_core::character::EffectRange::SelfRange,
            mgef_flags: 0,
            school: Some(school),
        }
    }

    fn spell(name: &str) -> CanonicalSpell {
        CanonicalSpell {
            effects: vec![CastEffect { mgef: 1, script: 0, duration: 30.0, magnitude: 1.0, value: None, conditions: None, range: Default::default(), area: 0.0 }],
            cast: Some(SpellCastData { name: name.into(), spell_type: SpellType::Spell, effects: vec![effect(0)], ..Default::default() }),
            ..Default::default()
        }
    }

    /// A player in character mode who knows two cheap Self spells, with the game's bindings and an empty action state.
    fn world() -> (World, EntityId) {
        let mut world = World::new();
        world.register::<ReadiedSpell>();
        world.register::<ActiveMagicEffects>();
        world.insert_resource(InputState::default());
        world.insert_resource(ActionBindings::default());
        world.insert_resource(ActionState::default());
        world.insert_resource(PlayerMode::Character);
        let mut catalog = HashMap::new();
        catalog.insert(SPELL, spell("Heat Shell"));
        catalog.insert(SECOND, spell("Frost Shell"));
        world.insert_resource(SpellCatalog::from_canonical(catalog));
        world.insert_resource(SpellCastModel::OBLIVION);
        world.insert_resource(CastState::default());
        let player = world.spawn();
        let mut pairs = vec![(7, 50.0), (MAGICKA, 100.0), (10, 100.0)];
        pairs.extend((20..26).map(|code| (code, 0.0)));
        world.insert(player, ActorValues::from_pairs(pairs));
        world.insert(player, SpellList(vec![SPELL, SECOND]));
        world.insert_resource(PlayerEntity(Some(player)));
        (world, player)
    }

    fn press(world: &World, key: KeyCode) {
        world.resource_mut::<InputState>().keys_held.insert(key);
        crate::interaction::refresh_action_state(world);
    }

    fn release(world: &World, key: KeyCode) {
        world.resource_mut::<InputState>().keys_held.remove(&key);
        crate::interaction::refresh_action_state(world);
    }

    fn magicka(world: &World, player: EntityId) -> f32 {
        world.get::<ActorValues>(player).unwrap().current(MAGICKA)
    }

    /// PSC-CST-001: the cast key is `C`, the original's PC default, and its press edge is the only input that casts.
    #[test]
    fn the_cast_key_casts_the_readied_spell_once_per_press() {
        let (world, player) = world();
        player_cast::ready_spell(&world, player, SPELL).unwrap();
        // A press casts: 5 * 10^1.28 * 0.1 = 9.5, times 1.4 at skill 0 and Luck 50, floored to 13.
        press(&world, KeyCode::KeyC);
        player_cast_system(&world, 0.016);
        assert_eq!(magicka(&world, player), 87.0);
        // The key held on the next frame is no new edge.
        crate::interaction::refresh_action_state(&world);
        player_cast_system(&world, 0.016);
        assert_eq!(magicka(&world, player), 87.0);
        // Released and pressed again inside the casting interval: refused, nothing spent.
        release(&world, KeyCode::KeyC);
        player_cast_system(&world, 0.016);
        press(&world, KeyCode::KeyC);
        player_cast_system(&world, 0.016);
        assert_eq!(magicka(&world, player), 87.0);
        assert!(world.resource::<CastState>().last.as_ref().is_some_and(|report| report.outcome.is_err()));
        // After the interval a new press casts again.
        release(&world, KeyCode::KeyC);
        player_cast_system(&world, CAST_INTERVAL_SECONDS);
        press(&world, KeyCode::KeyC);
        player_cast_system(&world, 0.016);
        assert_eq!(magicka(&world, player), 74.0);
    }

    /// Fly-cam is not a cast: the cast action needs character mode (`player_can_act`'s sibling gate).
    #[test]
    fn a_press_in_fly_cam_casts_nothing() {
        let (mut world, player) = world();
        player_cast::ready_spell(&world, player, SPELL).unwrap();
        world.insert_resource(PlayerMode::FlyCam);
        press(&world, KeyCode::KeyC);
        player_cast_system(&world, 0.016);
        assert_eq!(magicka(&world, player), 100.0);
    }

    /// PSC-RDY-003: the arrow keys cycle the readied spell through the castable ones.
    #[test]
    fn the_arrow_keys_cycle_the_readied_spell() {
        let (world, player) = world();
        press(&world, KeyCode::ArrowRight);
        player_cast_system(&world, 0.016);
        assert_eq!(player_cast::readied(&world, player), Some(SPELL));
        release(&world, KeyCode::ArrowRight);
        press(&world, KeyCode::ArrowRight);
        player_cast_system(&world, 0.016);
        assert_eq!(player_cast::readied(&world, player), Some(SECOND));
        release(&world, KeyCode::ArrowRight);
        press(&world, KeyCode::ArrowLeft);
        player_cast_system(&world, 0.016);
        assert_eq!(player_cast::readied(&world, player), Some(SPELL));
    }

    /// A game with no cast rules ignores all three actions: nothing is readied and nothing is cast.
    #[test]
    fn without_cast_rules_the_keys_do_nothing() {
        let (mut world, player) = world();
        world.remove_resource::<SpellCastModel>();
        press(&world, KeyCode::ArrowRight);
        player_cast_system(&world, 0.016);
        press(&world, KeyCode::KeyC);
        player_cast_system(&world, 0.016);
        assert_eq!(player_cast::readied(&world, player), None);
        assert_eq!(magicka(&world, player), 100.0);
    }

    /// Two collinear targets inside and outside the stock pick ray, with the nearer surface beyond the old 105 BU reach.
    fn world_with_actors() -> (World, EntityId, EntityId, EntityId) {
        use byroredux_core::ecs::components::ActorVitals;
        use byroredux_core::ecs::{ActiveCamera, Transform};
        let (mut world, player) = world();
        world.register::<byroredux_physics::RapierHandles>();
        world.register::<ActorVitals>();
        world.register::<byroredux_core::ecs::components::Dead>();
        world.register::<byroredux_physics::ActorColliderOwner>();
        world.insert_resource(crate::systems::projectiles::ActiveProjectiles::default());
        let camera = world.spawn();
        world.insert(camera, Transform::IDENTITY);
        world.insert_resource(ActiveCamera(camera));
        let mut physics = byroredux_physics::PhysicsWorld::new();
        let near_handles = physics.spawn_fixed_cuboid_handles(byroredux_core::math::Vec3::splat(10.0), byroredux_core::math::Vec3::new(0.0, 0.0, -140.0));
        let far_handles = physics.spawn_fixed_cuboid_handles(byroredux_core::math::Vec3::splat(10.0), byroredux_core::math::Vec3::new(0.0, 0.0, -180.0));
        world.insert_resource(physics);
        let (near, far) = (world.spawn(), world.spawn());
        world.insert(near, near_handles);
        world.insert(far, far_handles);
        world.insert(near, ActorVitals { health: 100 });
        world.insert(far, ActorVitals { health: 100 });
        (world, player, near, far)
    }

    fn add_spell(world: &mut World, id: u32, name: &str, range: byroredux_core::character::EffectRange) {
        let mut spell = spell(name);
        let cast = spell.cast.as_mut().unwrap();
        cast.effects[0].range = range;
        cast.target_speed_multiplier = 1.5;
        spell.effects[0].range = range;
        let mut catalog = world.remove_resource::<SpellCatalog>().unwrap();
        catalog.insert_canonical(id, spell);
        world.insert_resource(catalog);
        world.query_mut::<SpellList>().unwrap().get_mut(PlayerEntity::get(world).unwrap()).unwrap().0.push(id);
    }

    #[test]
    fn a_touch_cast_uses_pick_length_and_respects_the_first_obstruction() {
        use byroredux_scripting::magic_effects::ActiveMagicEffects;
        let (mut world, player, near, far) = world_with_actors();
        add_spell(&mut world, 0x20, "Searing Grasp", byroredux_core::character::EffectRange::Touch);
        player_cast::ready_spell(&world, player, 0x20).unwrap();
        let report = cast_now(&world).unwrap();
        assert_eq!(report.outcome.as_ref().unwrap().touched, Some(near), "{report}");
        assert!(world.get::<ActiveMagicEffects>(near).is_some() && world.get::<ActiveMagicEffects>(far).is_none());
        let short_pick = SpellCastModel::OBLIVION.with_gmst(|name| (name == "iActivatePickLength").then_some(75.0));
        world.insert_resource(short_pick);
        player_cast_system(&world, CAST_INTERVAL_SECONDS);
        let before = magicka(&world, player);
        let report = cast_now(&world).unwrap();
        assert!(report.outcome.as_ref().unwrap().no_target, "{report}");
        assert!(magicka(&world, player) < before);

        // The farther actor is reachable with a longer pick ray, but a corpse blocks it.
        let long_pick = SpellCastModel::OBLIVION.with_gmst(|name| (name == "iActivatePickLength").then_some(225.0));
        world.insert_resource(long_pick);
        world.insert(near, byroredux_core::ecs::components::Dead);
        player_cast_system(&world, CAST_INTERVAL_SECONDS);
        let report = cast_now(&world).unwrap();
        assert!(report.outcome.as_ref().unwrap().no_target, "{report}");
        assert!(world.get::<ActiveMagicEffects>(far).is_none());
    }

    /// PSC-TGT-003: a Target cast puts one spell projectile in flight along the camera's aim at the spell's speed.
    #[test]
    fn a_target_cast_launches_a_spell_projectile_along_the_aim() {
        let (mut world, player, _, _) = world_with_actors();
        add_spell(&mut world, 0x21, "Flare", byroredux_core::character::EffectRange::Target);
        player_cast::ready_spell(&world, player, 0x21).unwrap();
        let report = cast_now(&world).unwrap();
        assert!(report.outcome.is_ok(), "{report}");
        let active = world.resource::<crate::systems::projectiles::ActiveProjectiles>();
        assert_eq!(active.0.len(), 1);
        let shot = &active.0[0];
        assert_eq!(shot.spell, Some(0x21));
        assert_eq!(shot.damage, 0.0);
        assert!((shot.velocity - byroredux_core::math::Vec3::new(0.0, 0.0, -1500.0)).length() < 1e-2, "{:?}", shot.velocity);
        assert_eq!(shot.shooter, player);
    }
}
