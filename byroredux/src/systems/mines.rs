//! Proximity mines and their explosion — the Story Manager's `TMEE` event (M43).
//!
//! `OnStoryMineExplosion(akVictim, akAttacker)` (`Quest.pex`): fired when a mine goes off. The game's own
//! data defines a mine as a **projectile** record with the *Alt. Trigger* flag, a trigger distance ("Explosion -
//! Alt. Trigger - Proximity"), a fuse ("... - Timer") and the explosion it detonates into (xEdit `wbPROJ`);
//! the vanilla frag mine is `flags 0x66`, proximity 100, timer 1.5, explosion `fragMineExplosion` (damage 100,
//! inner radius 100, outer radius 500 — read from `Fallout4.esm`). A mine in the world is a placed reference to
//! such a projectile.
//!
//! What runs here, with the engine-policy parts named:
//! - **Trigger.** A live, placed actor — the player or an NPC — comes within the record's proximity distance of
//!   the mine (3-D distance between the placements). The nearest such actor is the *victim* of the event.
//!   Engine policy: no friend/foe test and no detection delay (the game settings `fMinesDelayMin`,
//!   `fMineExteriorRadiusMult` and `iProjectileMineShooterCanTrigger` are not modelled).
//! - **Fuse.** The record's timer counts down from the trigger; the mine then goes off. Reading "Timer" as a
//!   post-trigger fuse is an inference from the field name and the vanilla value (1.5 s).
//! - **Blast.** Every live actor within the explosion's outer radius takes a hit: full `damage` out to the
//!   inner radius, falling off linearly to nothing at the outer radius. The falloff curve is engine policy —
//!   the record gives the two radii, not the curve. Force, stagger and the explosion's visuals are not
//!   modelled.
//! - **After.** `TMEE` is raised (`R1` the victim; `R2` is none — a placed mine has no owner) and the mine is
//!   spent: hidden, disabled in the persistent enable ledger so it does not come back with its cell.
//!
//! Player-placed mines (the mine weapons) are not modelled; only mines placed in the world data.

use byroredux_core::ecs::components::{AnimatedVisibility, Children, Dead, GlobalTransform};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::records::EsmIndex;
use byroredux_scripting::story_manager::{raise_story_event, SmTree, StoryEvent};
use std::collections::HashMap;
use std::sync::Arc;

/// The `SMEN` mnemonic of the mine-explosion event.
pub(crate) const TMEE: [u8; 4] = *b"TMEE";

/// Seconds between trigger scans.
const SCAN_SECS: f32 = 0.25;

/// What one mine base does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MineSpec {
    pub proximity: f32,
    pub fuse: f32,
    pub damage: f32,
    pub inner_radius: f32,
    pub outer_radius: f32,
}

/// Every mine projectile of the load order, by its base `PROJ` FormID. `NOT_SAVED_BY_DESIGN`: rebuilt from the
/// records every boot.
#[derive(Debug, Default, Clone)]
pub(crate) struct MineBook(Arc<HashMap<u32, MineSpec>>);

impl Resource for MineBook {}

impl MineBook {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

pub(crate) fn install_mine_book(world: &mut World, index: &EsmIndex) -> usize {
    let mines: HashMap<u32, MineSpec> = index
        .projectiles
        .values()
        .filter(|proj| proj.is_proximity_mine())
        .filter_map(|proj| {
            let blast = index.explosions.get(&proj.explosion)?;
            Some((
                proj.form_id,
                MineSpec {
                    proximity: proj.alt_trigger_proximity,
                    fuse: proj.alt_trigger_timer.max(0.0),
                    damage: blast.damage.max(0.0),
                    inner_radius: blast.inner_radius.max(0.0),
                    outer_radius: blast.radius.max(blast.inner_radius).max(0.0),
                },
            ))
        })
        .collect();
    let count = mines.len();
    world.insert_resource(MineBook(Arc::new(mines)));
    count
}

/// A mine that has been tripped and is burning its fuse. `NOT_SAVED_BY_DESIGN`: a load restores the mine
/// armed-for-nothing, as it was placed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ArmedMine {
    pub victim: EntityId,
    pub fuse_left: f32,
}

impl Component for ArmedMine {
    type Storage = SparseSetStorage<Self>;
}

/// Marks a mine that has gone off, so it triggers nothing again this session.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SpentMine;

impl Component for SpentMine {
    type Storage = SparseSetStorage<Self>;
}

/// The scan clock. `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Default)]
pub(crate) struct MineClock(f32);

impl Resource for MineClock {}

/// The damage `damage` at `distance` from a blast with the two radii: full inside `inner`, linear to nothing at
/// `outer`.
pub(crate) fn blast_damage(damage: f32, inner: f32, outer: f32, distance: f32) -> f32 {
    if distance <= inner {
        damage
    } else if distance >= outer || outer <= inner {
        0.0
    } else {
        damage * (outer - distance) / (outer - inner)
    }
}

fn live_actors(world: &World) -> Vec<(EntityId, Vec3)> {
    let player = crate::systems::PlayerEntity::get(world);
    let mut actors: Vec<EntityId> = world
        .query::<crate::systems::faction_hostility::CombatDisposition>()
        .map(|dispositions| dispositions.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    actors.extend(player);
    actors
        .into_iter()
        .filter(|actor| world.get::<Dead>(*actor).is_none())
        .filter_map(|actor| Some((actor, world.get::<GlobalTransform>(actor)?.translation)))
        .collect()
}

fn hide(world: &World, entity: EntityId) {
    let mut all = Vec::new();
    let mut stack = vec![entity];
    while let Some(next) = stack.pop() {
        all.push(next);
        if let Some(children) = world.get::<Children>(next) {
            stack.extend(children.0.iter().copied());
        }
    }
    if let Some(mut visibility) = world.query_mut::<AnimatedVisibility>() {
        for entity in all {
            visibility.insert(entity, AnimatedVisibility(false));
        }
    }
}

/// One placed mine as the console shows it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MineView {
    pub entity: EntityId,
    pub position: Vec3,
    pub base: u32,
    /// `armed` (tripped, burning its fuse), `spent` (gone off) or `waiting`.
    pub state: &'static str,
}

/// Every placed mine in the world, nearest-first order not implied.
pub(crate) fn placed_mines(world: &World) -> Vec<MineView> {
    let Some(book) = world.try_resource::<MineBook>().map(|book| book.0.clone()) else {
        return Vec::new();
    };
    let candidates: Vec<(EntityId, u32)> = world
        .query::<byroredux_scripting::SceneAliasCandidate>()
        .map(|all| all.iter().filter(|(_, c)| book.contains_key(&c.base_form_id)).map(|(e, c)| (e, c.base_form_id)).collect())
        .unwrap_or_default();
    candidates
        .into_iter()
        .filter_map(|(entity, base)| {
            let position = world.get::<GlobalTransform>(entity)?.translation;
            let state = if world.has::<SpentMine>(entity) {
                "spent"
            } else if world.has::<ArmedMine>(entity) {
                "armed"
            } else {
                "waiting"
            };
            Some(MineView { entity, position, base, state })
        })
        .collect()
}

/// System: trip mines on approach, burn their fuse, and blast.
pub(crate) fn mine_system(world: &World, dt: f32) {
    let book = match world.try_resource::<MineBook>() {
        Some(book) if !book.0.is_empty() => book.0.clone(),
        _ => return,
    };
    // Armed mines count down every frame; the trigger scan runs on its own cadence.
    let armed: Vec<(EntityId, ArmedMine)> = world
        .query::<ArmedMine>()
        .map(|mines| mines.iter().map(|(entity, mine)| (entity, *mine)).collect())
        .unwrap_or_default();
    for (mine, state) in armed {
        let left = state.fuse_left - dt.max(0.0);
        if left > 0.0 {
            if let Some(mut mines) = world.query_mut::<ArmedMine>() {
                mines.insert(mine, ArmedMine { fuse_left: left, ..state });
            }
            continue;
        }
        detonate(world, &book, mine, state.victim);
    }
    let due = match world.try_resource_mut::<MineClock>() {
        Some(mut clock) => {
            clock.0 += dt.max(0.0);
            if clock.0 < SCAN_SECS {
                false
            } else {
                clock.0 = 0.0;
                true
            }
        }
        None => false,
    };
    if !due {
        return;
    }
    let placed: Vec<(EntityId, MineSpec, Vec3)> = world
        .query::<byroredux_scripting::SceneAliasCandidate>()
        .map(|candidates| {
            candidates
                .iter()
                .filter_map(|(entity, candidate)| Some((entity, *book.get(&candidate.base_form_id)?)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|(entity, _)| !world.has::<SpentMine>(*entity) && !world.has::<ArmedMine>(*entity))
        .filter_map(|(entity, spec)| Some((entity, spec, world.get::<GlobalTransform>(entity)?.translation)))
        .collect();
    if placed.is_empty() {
        return;
    }
    let actors = live_actors(world);
    for (mine, spec, at) in placed {
        let nearest = actors
            .iter()
            .map(|(actor, position)| (*actor, position.distance(at)))
            .filter(|(_, distance)| *distance <= spec.proximity)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((victim, _)) = nearest {
            log::info!("mine {mine:?} tripped by {victim:?}: {:.1}s fuse", spec.fuse);
            if let Some(mut mines) = world.query_mut::<ArmedMine>() {
                mines.insert(mine, ArmedMine { victim, fuse_left: spec.fuse });
            }
        }
    }
}

fn detonate(world: &World, book: &HashMap<u32, MineSpec>, mine: EntityId, victim: EntityId) {
    let Some(spec) = world
        .get::<byroredux_scripting::SceneAliasCandidate>(mine)
        .and_then(|candidate| book.get(&candidate.base_form_id).copied())
    else {
        return;
    };
    let Some(at) = world.get::<GlobalTransform>(mine).map(|t| t.translation) else {
        return;
    };
    log::info!("mine {mine:?} detonated: victim {victim:?}, damage {} out to {}", spec.damage, spec.outer_radius);
    // The blast: a hit on every live actor inside the outer radius, scaled by the falloff.
    let hits: Vec<(EntityId, f32)> = live_actors(world)
        .into_iter()
        .filter_map(|(actor, position)| {
            let damage = blast_damage(spec.damage, spec.inner_radius, spec.outer_radius, position.distance(at));
            (damage > 0.0).then_some((actor, damage))
        })
        .collect();
    if let Some(mut events) = world.query_mut::<byroredux_scripting::HitEvent>() {
        for (target, damage) in hits {
            events.insert(
                target,
                byroredux_scripting::HitEvent {
                    aggressor: mine,
                    source: mine,
                    projectile: mine,
                    damage,
                    power_attack: false,
                    sneak_attack: false,
                    bash_attack: false,
                    blocked: false,
                },
            );
        }
    }
    // `OnStoryMineExplosion(akVictim, akAttacker)`: the victim in R1; a placed mine has no attacker.
    if world.try_resource::<SmTree>().is_some_and(|tree| tree.roots_by_mnemonic.contains_key(&TMEE)) {
        raise_story_event(
            world,
            victim,
            StoryEvent {
                mnemonic: TMEE,
                reference_1: victim,
                reference_2: None,
                location_1: None,
                location_2: None,
                scripted: None,
            },
        );
    }
    // Spent: no longer armed, hidden, and disabled in the persistent ledger so it does not return with its cell.
    if let Some(mut armed) = world.query_mut::<ArmedMine>() {
        armed.remove(mine);
    }
    if let Some(mut spent) = world.query_mut::<SpentMine>() {
        spent.insert(mine, SpentMine);
    }
    hide(world, mine);
    if let Some(form) = world
        .get::<byroredux_scripting::SceneAliasCandidate>(mine)
        .map(|candidate| candidate.reference_form_id)
        .filter(|form| *form != 0)
    {
        if let Some(mut state) = world.try_resource_mut::<byroredux_scripting::ReferenceEnableState>() {
            state.set_enabled(form, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::faction_hostility::CombatDisposition;
    use byroredux_plugin::esm::records::{ActorAiData, Aggression, Confidence, SmNodeKind, SmNodeRecord};

    const BASE: u32 = 0xE56C3;
    const SPEC: MineSpec = MineSpec { proximity: 100.0, fuse: 1.5, damage: 100.0, inner_radius: 100.0, outer_radius: 500.0 };

    fn setup() -> (World, EntityId, EntityId) {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<GlobalTransform>();
        world.register::<CombatDisposition>();
        world.register::<Dead>();
        world.register::<ArmedMine>();
        world.register::<SpentMine>();
        world.register::<AnimatedVisibility>();
        world.register::<Children>();
        world.insert_resource(MineBook(Arc::new(HashMap::from([(BASE, SPEC)]))));
        world.insert_resource(MineClock::default());
        let records = HashMap::from([(
            0x10,
            SmNodeRecord { form_id: 0x10, kind: SmNodeKind::Event, event_mnemonic: Some(TMEE), ..Default::default() },
        )]);
        byroredux_scripting::story_manager::install_story_manager(&mut world, &records);
        let mine = world.spawn();
        world.insert(mine, GlobalTransform { translation: Vec3::ZERO, ..Default::default() });
        world.insert(mine, byroredux_scripting::SceneAliasCandidate { reference_form_id: 0x77, base_form_id: BASE, ..Default::default() });
        let victim = world.spawn();
        world.insert(victim, GlobalTransform { translation: Vec3::new(60.0, 0.0, 0.0), ..Default::default() });
        world.insert(
            victim,
            CombatDisposition(ActorAiData { aggression: Aggression::Unaggressive, confidence: Confidence::Average, responsibility: None, attack_radius: None }),
        );
        (world, mine, victim)
    }

    #[test]
    fn the_blast_is_full_inside_the_inner_radius_and_fades_to_the_outer() {
        assert_eq!(blast_damage(100.0, 100.0, 500.0, 50.0), 100.0);
        assert_eq!(blast_damage(100.0, 100.0, 500.0, 300.0), 50.0);
        assert_eq!(blast_damage(100.0, 100.0, 500.0, 500.0), 0.0);
        assert_eq!(blast_damage(100.0, 100.0, 500.0, 900.0), 0.0);
        assert_eq!(blast_damage(100.0, 200.0, 200.0, 250.0), 0.0, "no falloff band: nothing past the radius");
    }

    /// An actor inside the proximity trips the mine; the fuse runs; then it hits everyone in the blast, raises
    /// `TMEE` on the victim and is spent. Nothing happens to a mine that nobody approached.
    #[test]
    fn a_mine_trips_burns_its_fuse_then_blasts_and_raises_the_event() {
        let (mut world, mine, victim) = setup();
        let bystander = world.spawn();
        world.insert(bystander, GlobalTransform { translation: Vec3::new(300.0, 0.0, 0.0), ..Default::default() });
        world.insert(
            bystander,
            CombatDisposition(ActorAiData { aggression: Aggression::Unaggressive, confidence: Confidence::Average, responsibility: None, attack_radius: None }),
        );
        mine_system(&world, 0.3);
        assert!(world.get::<ArmedMine>(mine).is_some(), "the actor within 100 trips the mine");
        mine_system(&world, 1.0);
        assert!(world.get::<ArmedMine>(mine).is_some(), "the fuse has 0.2 s left");
        assert!(world.get::<SpentMine>(mine).is_none());
        mine_system(&world, 0.5);
        assert!(world.get::<SpentMine>(mine).is_some() && world.get::<ArmedMine>(mine).is_none(), "gone off");
        let hit = world.get::<byroredux_scripting::HitEvent>(victim).map(|h| *h).expect("the victim is hit");
        assert_eq!(hit.damage, 100.0);
        let far = world.get::<byroredux_scripting::HitEvent>(bystander).map(|h| *h).expect("the bystander is in the blast");
        assert_eq!(far.damage, 50.0, "300 is halfway between the radii");
        let event = world.get::<StoryEvent>(victim).map(|e| *e).expect("TMEE raised");
        assert_eq!((event.mnemonic, event.reference_1, event.reference_2), (TMEE, victim, None));
        assert_eq!(world.get::<AnimatedVisibility>(mine).map(|v| v.0), Some(false), "the spent mine is hidden");
    }

    #[test]
    fn a_distant_actor_trips_nothing() {
        let (mut world, mine, victim) = setup();
        world.insert(victim, GlobalTransform { translation: Vec3::new(400.0, 0.0, 0.0), ..Default::default() });
        for _ in 0..10 {
            mine_system(&world, 0.3);
        }
        assert!(world.get::<ArmedMine>(mine).is_none() && world.get::<SpentMine>(mine).is_none());
    }
}
