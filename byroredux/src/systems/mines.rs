//! Proximity mines and their explosion — the Story Manager's `TMEE` event (M43).
//!
//! `OnStoryMineExplosion(akVictim, akAttacker)` (`Quest.pex`): fired when a mine goes off. The game's own
//! data defines a mine as a **projectile** record with the *Alt. Trigger* flag, a trigger distance ("Explosion -
//! Alt. Trigger - Proximity"), a fuse ("... - Timer") and the explosion it detonates into (xEdit `wbPROJ`);
//! the vanilla frag mine is `flags 0x66`, proximity 100, timer 1.5, explosion `fragMineExplosion` (damage 100,
//! inner radius 100, outer radius 500 — read from `Fallout4.esm`). A mine in the world is a placed reference to
//! such a projectile: a `REFR` on Fallout 4, a `PGRE` ("placed grenade") on Fallout 3 / New Vegas, which the
//! cell walker keeps as an ordinary placement (#3542) — `Fallout3.esm` places 344 of them, `FalloutNV.esm` 165.
//! The GECK's own trap tutorial (*Bethsoft Tutorial Non-NPC population*, "Mines") draws the line: the mines
//! under *Special Effects > Projectile* "are armed and ready to explode", while "inactive, lootable mines" are
//! the `1handMineDrop` weapons. A placed `WEAP` mine (`WeapMineFrag` and kin) is therefore loot and is not
//! keyed here (#6559).
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
//! Player-placed mines (thrown from the mine weapons) are not modelled; only mines placed in the world data.

use byroredux_core::ecs::components::{AnimatedVisibility, Dead, GlobalTransform};
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
pub(crate) struct MineClock(crate::systems::PassClock);

impl Resource for MineClock {}

/// Blast hits waiting to land: (target, mine, damage). `HitEvent` is one slot per target and `insert` replaces, so
/// a blast on a target that already holds this frame's hit (a melee strike, a projectile, another mine's blast)
/// waits a frame instead, as `PendingTrapHits` and `npc_combat_ai_system` do (#4324, #5860). A minefield trips in
/// groups and the blasts overlap, so without this the player took one mine's damage where the data says N (#6540).
/// `NOT_SAVED_BY_DESIGN`: a load discards it with the rest of the outgoing session's one-frame hits.
#[derive(Debug, Default)]
pub(crate) struct PendingMineHits(Vec<(EntityId, EntityId, f32)>);

impl Resource for PendingMineHits {}

impl PendingMineHits {
    /// Drop every waiting hit (a load, which replaces the world they name).
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
}

/// Land every waiting blast hit whose target holds no `HitEvent` yet; the rest stay for the next frame. A target
/// that died or left the world while waiting takes no hit.
fn land_mine_hits(world: &World) {
    let hits = match world.try_resource_mut::<PendingMineHits>() {
        Some(mut pending) if !pending.0.is_empty() => std::mem::take(&mut pending.0),
        _ => return,
    };
    let mut held = Vec::new();
    if let Some(mut events) = world.query_mut::<byroredux_scripting::HitEvent>() {
        for hit @ (target, mine, damage) in hits {
            if world.has::<Dead>(target) || !world.has::<GlobalTransform>(target) {
                continue;
            }
            if events.get(target).is_some() {
                held.push(hit);
                continue;
            }
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
    } else {
        held = hits;
    }
    if !held.is_empty() {
        if let Some(mut pending) = world.try_resource_mut::<PendingMineHits>() {
            pending.0.extend(held);
        }
    }
}

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

/// The placed actors and the player, living, with where they stand.
fn live_actors(world: &World) -> Vec<(EntityId, Vec3)> {
    let mut actors = crate::systems::faction_hostility::living_actors(world);
    actors.extend(crate::systems::PlayerEntity::get(world).filter(|player| world.get::<Dead>(*player).is_none()));
    actors
        .into_iter()
        .filter_map(|actor| Some((actor, world.get::<GlobalTransform>(actor)?.translation)))
        .collect()
}

/// Hide a spent mine's whole subtree. The walk is `collect_subtree` (#6577): the mine's `Children` come from a
/// save-loaded hierarchy, and an unguarded stack walk spins forever on a cyclic one (#5513).
fn hide(world: &World, entity: EntityId) {
    let all = byroredux_core::ecs::collect_subtree(world, entity);
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
    // Blasts that waited a frame for their target's slot (#6540).
    land_mine_hits(world);
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
    // The blasts just staged land now where their targets are free: this frame's first blast on each.
    land_mine_hits(world);
    let due = world.try_resource_mut::<MineClock>().is_some_and(|mut clock| clock.0.due(dt, SCAN_SECS).is_some());
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
    if let Some(mut pending) = world.try_resource_mut::<PendingMineHits>() {
        pending.0.extend(hits.into_iter().map(|(target, damage)| (target, mine, damage)));
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
    use byroredux_core::ecs::components::Children;
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
        world.insert_resource(PendingMineHits::default());
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

    /// A second mine of the same record next to the first (as a minefield lays them), already armed with a fuse
    /// that runs out this frame.
    fn armed_neighbour(world: &mut World, at: Vec3) -> EntityId {
        let mine = world.spawn();
        world.insert(mine, GlobalTransform { translation: at, ..Default::default() });
        world.insert(mine, byroredux_scripting::SceneAliasCandidate { reference_form_id: 0x78, base_form_id: BASE, ..Default::default() });
        mine
    }

    /// #6540 — two mines whose fuses run out together land both blasts on a shared target, one per frame, instead
    /// of the second replacing the first; a melee hit already on the target that frame stands too.
    #[test]
    fn overlapping_blasts_and_a_same_frame_hit_all_land_one_per_frame() {
        let (mut world, mine_a, victim) = setup();
        let mine_b = armed_neighbour(&mut world, Vec3::new(10.0, 0.0, 0.0));
        let melee = world.spawn();
        for mine in [mine_a, mine_b] {
            world.insert(mine, ArmedMine { victim, fuse_left: 0.1 });
        }
        world.insert(
            victim,
            byroredux_scripting::HitEvent {
                aggressor: melee,
                source: melee,
                projectile: 0,
                damage: 7.0,
                power_attack: false,
                sneak_attack: false,
                bash_attack: false,
                blocked: false,
            },
        );
        let on_victim = |world: &World| world.get::<byroredux_scripting::HitEvent>(victim).map(|h| (h.aggressor, h.damage));
        let drain = |world: &World| {
            world.query_mut::<byroredux_scripting::HitEvent>().unwrap().remove(victim);
        };

        mine_system(&world, 0.2);
        assert!(world.has::<SpentMine>(mine_a) && world.has::<SpentMine>(mine_b), "both mines went off");
        assert_eq!(on_victim(&world), Some((melee, 7.0)), "the melee hit on the blast frame stands");
        assert_eq!(world.resource::<PendingMineHits>().0.len(), 2, "both blasts wait");

        let mut landed = Vec::new();
        for _ in 0..2 {
            drain(&world); // `event_cleanup_system` empties the frame's hits
            mine_system(&world, 0.0);
            landed.push(on_victim(&world).expect("a blast lands"));
        }
        assert_eq!(landed.iter().map(|hit| hit.1).collect::<Vec<_>>(), vec![100.0, 100.0], "N mines, N blasts");
        assert_eq!(landed.iter().map(|hit| hit.0).collect::<std::collections::HashSet<_>>(), [mine_a, mine_b].into_iter().collect(), "one per mine");
        assert!(world.resource::<PendingMineHits>().0.is_empty());
        drain(&world);
        mine_system(&world, 0.0);
        assert_eq!(on_victim(&world), None, "nothing is left to land");
    }

    /// #6540 — a blast waiting for a free slot is dropped if its target dies meanwhile.
    #[test]
    fn a_blast_waiting_on_a_target_that_died_takes_no_hit() {
        let (mut world, mine, victim) = setup();
        world.insert(mine, ArmedMine { victim, fuse_left: 0.1 });
        world.insert(victim, byroredux_scripting::HitEvent { aggressor: mine, source: mine, projectile: 0, damage: 1.0, power_attack: false, sneak_attack: false, bash_attack: false, blocked: false });
        mine_system(&world, 0.2);
        assert_eq!(world.resource::<PendingMineHits>().0.len(), 1);
        world.query_mut::<byroredux_scripting::HitEvent>().unwrap().remove(victim);
        world.insert(victim, Dead);
        mine_system(&world, 0.0);
        assert!(world.get::<byroredux_scripting::HitEvent>(victim).is_none());
        assert!(world.resource::<PendingMineHits>().0.is_empty());
    }

    /// #6559 — FO3 / FNV arm their minefields as `PGRE` placements of the mine `PROJ`, so the book keys them
    /// by base like FO4's `REFR`s; the placed mine `WEAP`s are the GECK's "inactive, lootable mines" and stay
    /// out of it. Counts are the shipped masters' (every cell, interior and exterior).
    #[test]
    #[ignore = "needs Fallout3.esm and FalloutNV.esm on disk"]
    fn fo3_and_fnv_armed_mines_are_placed_projectiles_the_book_keys() {
        use byroredux_plugin::esm::records::ItemKind;
        use byroredux_plugin::esm::test_paths;
        for (path, armed_placements, loot_placements) in
            [(test_paths::fo3_esm(), 344usize, 97usize), (test_paths::fnv_esm(), 165, 17)]
        {
            let Ok(bytes) = std::fs::read(&path) else {
                test_paths::require_or_skip(&path.display().to_string());
                continue;
            };
            let index = byroredux_plugin::esm::records::parse_esm(&bytes).expect("parse");
            let mut world = World::new();
            install_mine_book(&mut world, &index);
            let book = world.resource::<MineBook>().0.clone();
            let mine_weapon = |base: u32| {
                index.items.get(&base).is_some_and(|item| {
                    matches!(item.kind, ItemKind::Weapon { .. }) && item.common.editor_id.to_ascii_lowercase().starts_with("weapmine")
                })
            };
            let placements: Vec<u32> = index
                .cells
                .cells
                .values()
                .chain(index.cells.exterior_cells.values().flat_map(|cells| cells.values()))
                .flat_map(|cell| cell.references.iter().map(|placed| placed.base_form_id))
                .collect();
            let armed = placements.iter().filter(|base| book.contains_key(base)).count();
            let loot = placements.iter().filter(|&&base| mine_weapon(base)).count();
            assert_eq!(armed, armed_placements, "{}: placed mine projectiles the book keys", path.display());
            assert_eq!(loot, loot_placements, "{}: placed mine weapons (loot)", path.display());
            assert!(
                placements.iter().filter(|&&base| mine_weapon(base)).all(|base| !book.contains_key(base)),
                "a placed mine weapon is loot, not an armed mine"
            );
        }
    }

    /// #6577 — a spent mine hides its whole subtree, and a cyclic `Children` graph (a corrupt save) ends the
    /// walk instead of hanging `Stage::Update`: the hand-rolled stack walk `hide` used to carry never stopped.
    #[test]
    fn a_spent_mine_hides_a_cyclic_subtree_and_terminates() {
        let (mut world, mine, victim) = setup();
        let mesh = world.spawn();
        world.insert(mine, Children(vec![mesh]));
        world.insert(mesh, Children(vec![mine]));
        let book = world.resource::<MineBook>().0.clone();
        detonate(&world, &book, mine, victim);
        for entity in [mine, mesh] {
            assert_eq!(world.get::<AnimatedVisibility>(entity).map(|v| v.0), Some(false), "{entity:?} hidden");
        }
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
