//! Ambient faction hostility (#4414): an NPC that sees an actor it would
//! attack starts combat against it, with no script involved.
//!
//! Before this, only `Actor.StartCombat` put an NPC into combat, so a
//! bandit camp stood idle while the player walked through it. This system
//! is the combat-start half. The fight itself is the existing
//! `npc_combat_ai_system`, which this arms by inserting the same
//! [`AiCombatState`] `StartCombat` does.
//!
//! ## The rules, and where each comes from
//!
//! - **Reaction** (how a perceiver treats a target). An override set by
//!   `Faction.SetEnemy` ([`FactionRelations`]) wins, then the load order's
//!   authored `XNAM` relation (the SDK `FactionRelationshipCatalog`), and
//!   finally Neutral: "This is how all factions relate to each other by
//!   default" (CK / GECK Faction). Relations are directed. Fellow members of
//!   one faction are allies only because the data says so: vanilla authors
//!   the self-relation explicitly, and no engine-added implicit rule is
//!   documented.
//! - **Aggression** (CK / GECK "AI Data"). Unaggressive "will not initiate
//!   combat". Aggressive attacks Enemies, Very Aggressive attacks Enemies
//!   and Neutrals, and Frenzied attacks anyone. Friends and Allies are never
//!   attacked except by Frenzied actors. Cowardly actors "NEVER engage in
//!   combat under any circumstances".
//! - **Aggro radius.** With "Aggro Radius Behavior" set, a Neutral or Enemy
//!   inside [`ActorAiData::attack_radius`] is attacked at once, whatever the
//!   actor's aggression. The warn behaviour of the outer radii is not
//!   modelled.
//! - **Detection range.** `fSneakMaxDistance`, times
//!   `fSneakExteriorDistanceMult` outdoors (CK Detection: "Actors within the
//!   detection radius (fSneakMaxDistance)"). Without that GMST there is no
//!   [`DetectionConfig`] and no ambient hostility at all. The range is not
//!   invented.
//!
//! ## Engine policy (no source documents the original)
//!
//! - **Several factions per actor.** A perceiver never attacks a target if
//!   any faction pair between them is Friend or Ally. Otherwise any Enemy
//!   pair makes the target an Enemy, and anything else is Neutral. How the
//!   original engine combines conflicting memberships is undocumented.
//! - **Detection is a sight test.** A target counts once it is within range
//!   and in line of sight through solid geometry. The detection meter
//!   (sound, light, sneak skill, view cone) is not modelled.
//! - **Evaluation runs every [`EVALUATION_PERIOD_SECS`]**, not per frame,
//!   and the nearest visible target is chosen.
//! - **A faction rank below zero is not membership.** Bethesda content uses
//!   rank -1 to take an actor out of a faction.

use byroredux_core::ecs::components::{Dead, FactionRanks, GlobalTransform};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::records::{ActorAiData, Aggression, Confidence, EsmIndex};
use byroredux_scripting::{AiCombatState, FactionRelations, LoadOrderIdentity};
use byroredux_sdk::identity::FormRef;
use byroredux_sdk::relationships::{CombatReaction, FactionRelationshipCatalog};

/// Seconds between hostility evaluations. Engine policy: combat start is a
/// perception decision, and re-deciding it every frame for every NPC buys
/// nothing a player could see.
pub(crate) const EVALUATION_PERIOD_SECS: f32 = 0.5;

/// An NPC's `AIDT` combat disposition, stamped at spawn from the resolved
/// "Use AI Data" terminal. Only actors that carry it perceive and start
/// combat.
///
/// Not saved: it is rebuilt from the plugin's `NPC_` record every spawn,
/// the same posture as `WalkSpeed`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CombatDisposition(pub(crate) ActorAiData);

impl Component for CombatDisposition {
    type Storage = SparseSetStorage<Self>;
}

/// Detection range from the load order's GMSTs. Absent when the game does
/// not author `fSneakMaxDistance`, which disables ambient hostility.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DetectionConfig {
    /// `fSneakMaxDistance`, in BU.
    pub(crate) max_distance: f32,
    /// `fSneakExteriorDistanceMult`, or `1.0` (no scaling) when the game
    /// does not author it.
    pub(crate) exterior_mult: f32,
}

impl Resource for DetectionConfig {}

impl DetectionConfig {
    pub(crate) fn from_index(index: &EsmIndex) -> Option<Self> {
        let max_distance = index
            .game_setting_float("fSneakMaxDistance")
            .filter(|d| d.is_finite() && *d > 0.0)?;
        let exterior_mult = index
            .game_setting_float("fSneakExteriorDistanceMult")
            .filter(|m| m.is_finite() && *m > 0.0)
            .unwrap_or(1.0);
        Some(Self {
            max_distance,
            exterior_mult,
        })
    }

    fn range(&self, exterior: bool) -> f32 {
        if exterior {
            self.max_distance * self.exterior_mult
        } else {
            self.max_distance
        }
    }
}

/// Install (or clear) the [`DetectionConfig`] for a freshly loaded index.
pub(crate) fn install_detection_config(world: &mut World, index: &EsmIndex) {
    match DetectionConfig::from_index(index) {
        Some(config) => {
            world.insert_resource(config);
        }
        None => {
            world.remove_resource::<DetectionConfig>();
        }
    }
}

/// How a perceiver's factions, taken together, treat a target's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Standing {
    /// Some faction pair is Friend or Ally.
    Friendly,
    Enemy,
    Neutral,
}

/// The engine-policy combination of every directed faction pair (see the
/// module doc). An override set by script wins over the authored relation.
pub(crate) fn standing(
    perceiver: &[FormRef],
    target: &[FormRef],
    overrides: &FactionRelations,
    catalog: &FactionRelationshipCatalog,
) -> Standing {
    let mut enemy = false;
    for &own in perceiver {
        for &other in target {
            let reaction = overrides.reaction(own, other).or_else(|| {
                catalog
                    .relationship(own, other)
                    .and_then(|relation| relation.combat_reaction())
            });
            match reaction {
                Some(CombatReaction::Friend | CombatReaction::Ally) => return Standing::Friendly,
                Some(CombatReaction::Enemy) => enemy = true,
                Some(CombatReaction::Neutral) | None => {}
            }
        }
    }
    if enemy {
        Standing::Enemy
    } else {
        Standing::Neutral
    }
}

/// Whether an actor with this disposition attacks a target it perceives.
pub(crate) fn attacks(disposition: &ActorAiData, standing: Standing, distance: f32) -> bool {
    if disposition.confidence == Confidence::Cowardly {
        return false;
    }
    if disposition.aggression == Aggression::Frenzied {
        return true;
    }
    if standing == Standing::Friendly {
        return false;
    }
    if disposition
        .attack_radius
        .is_some_and(|radius| distance <= radius)
    {
        return true;
    }
    match disposition.aggression {
        Aggression::Unaggressive => false,
        Aggression::Aggressive => standing == Standing::Enemy,
        Aggression::VeryAggressive | Aggression::Frenzied => true,
    }
}

/// One actor as the evaluation sees it.
struct ActorSnapshot {
    entity: EntityId,
    /// The point sight rays start and end at.
    sight_point: Vec3,
    disposition: Option<ActorAiData>,
    /// Range into [`HostilityScratch::factions`].
    factions: std::ops::Range<usize>,
}

/// Closure-persistent scratch, so a steady evaluation stops allocating.
#[derive(Default)]
struct HostilityScratch {
    elapsed: f32,
    actors: Vec<ActorSnapshot>,
    faction_ids: Vec<u32>,
    factions: Vec<FormRef>,
    candidates: Vec<(f32, usize)>,
    starts: Vec<(EntityId, EntityId)>,
}

fn faction_hostility_system_inner(world: &World, dt: f32, scratch: &mut HostilityScratch) {
    if dt.is_finite() {
        scratch.elapsed += dt.max(0.0);
    }
    if scratch.elapsed < EVALUATION_PERIOD_SECS {
        return;
    }
    scratch.elapsed = 0.0;

    let Some(config) = world.try_resource::<DetectionConfig>().map(|c| *c) else {
        return;
    };
    let exterior = world
        .try_resource::<crate::components::CellLightingRes>()
        .is_some_and(|cell| !cell.is_interior);
    let range = config.range(exterior);

    let HostilityScratch {
        actors,
        faction_ids,
        factions,
        candidates,
        starts,
        ..
    } = scratch;
    actors.clear();
    faction_ids.clear();
    factions.clear();
    starts.clear();

    // Snapshot every actor one storage at a time — no two guards overlap.
    let dispositions: Vec<(EntityId, ActorAiData)> = world
        .query::<CombatDisposition>()
        .map(|q| q.iter().map(|(entity, d)| (entity, d.0)).collect())
        .unwrap_or_default();
    if dispositions.is_empty() {
        return;
    }
    let player = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0);
    let actor_centre = Vec3::Y * crate::npc_spawn::FALLBACK_ACTOR_CAPSULE_CENTRE_HEIGHT;
    let everyone = dispositions
        .iter()
        .map(|&(entity, disposition)| (entity, Some(disposition)))
        .chain(player.map(|player| (player, None)));
    for (entity, disposition) in everyone {
        if world.get::<Dead>(entity).is_some() {
            continue;
        }
        let Some(position) = world.get::<GlobalTransform>(entity).map(|t| t.translation) else {
            continue;
        };
        // The player body's transform is already its capsule centre; an NPC
        // placement root stands at its feet.
        let sight_point = if disposition.is_some() {
            position + actor_centre
        } else {
            position
        };
        let start = faction_ids.len();
        if let Some(ranks) = world.get::<FactionRanks>(entity) {
            faction_ids.extend(
                ranks
                    .0
                    .iter()
                    .filter(|(_, rank)| *rank >= 0)
                    .map(|(faction, _)| *faction),
            );
        }
        actors.push(ActorSnapshot {
            entity,
            sight_point,
            disposition,
            factions: start..faction_ids.len(),
        });
    }

    // Faction ids → portable identity, the key both relation tables use. A
    // faction whose plugin is not in the load order resolves to nothing and
    // drops out of the pair scan, so its ranges are rebuilt over the kept ids.
    {
        let Some(identity) = world.try_resource::<LoadOrderIdentity>() else {
            return;
        };
        for actor in actors.iter_mut() {
            let start = factions.len();
            factions.extend(
                faction_ids[actor.factions.clone()]
                    .iter()
                    .filter_map(|&faction| identity.form_ref(faction)),
            );
            actor.factions = start..factions.len();
        }
    }
    let overrides = world
        .try_resource::<FactionRelations>()
        .map(|relations| relations.clone())
        .unwrap_or_default();
    let catalog = world
        .try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
        .map(|resolver| resolver.faction_relationships())
        .unwrap_or_default();
    let fighting: Vec<EntityId> = world
        .query::<AiCombatState>()
        .map(|q| q.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    let player_body = player.and_then(|player| {
        world
            .query::<byroredux_physics::RapierHandles>()
            .and_then(|handles| handles.get(player).map(|h| h.body))
    });

    for (perceiver_index, perceiver) in actors.iter().enumerate() {
        let Some(disposition) = perceiver.disposition else {
            continue;
        };
        if disposition.confidence == Confidence::Cowardly || fighting.contains(&perceiver.entity) {
            continue;
        }
        candidates.clear();
        for (target_index, target) in actors.iter().enumerate() {
            if target_index == perceiver_index {
                continue;
            }
            let distance = perceiver.sight_point.distance(target.sight_point);
            if distance > range {
                continue;
            }
            let standing = standing(
                &factions[perceiver.factions.clone()],
                &factions[target.factions.clone()],
                &overrides,
                &catalog,
            );
            if attacks(&disposition, standing, distance) {
                candidates.push((distance, target_index));
            }
        }
        if candidates.is_empty() {
            continue;
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Nearest target in sight. The physics guard is taken per cast and
        // never overlaps a storage guard (#3580). No physics world (tests,
        // physics-less scenes) means nothing occludes.
        let target = candidates.iter().find_map(|&(_, target_index)| {
            let target = &actors[target_index];
            let blocked = world
                .try_resource::<byroredux_physics::PhysicsWorld>()
                .is_some_and(|physics| {
                    physics.line_of_sight_blocked(
                        perceiver.sight_point,
                        target.sight_point,
                        player_body,
                    )
                });
            (!blocked).then_some(target.entity)
        });
        if let Some(target) = target {
            starts.push((perceiver.entity, target));
        }
    }

    if starts.is_empty() {
        return;
    }
    if let Some(mut states) = world.query_mut::<AiCombatState>() {
        for &(attacker, target) in starts.iter() {
            if states.get(attacker).is_none() {
                states.insert(
                    attacker,
                    AiCombatState {
                        target,
                        attack_cooldown_remaining: 0.0,
                    },
                );
            }
        }
    }
}

/// System factory with persistent scratch (#4613's pattern).
pub(crate) fn make_faction_hostility_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = HostilityScratch::default();
    move |world: &World, dt: f32| faction_hostility_system_inner(world, dt, &mut scratch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::Transform;
    use byroredux_core::form_id::PluginId;
    use byroredux_plugin::esm::reader::GlobalSlot;
    use byroredux_sdk::relationships::FactionRelationship;

    const BANDITS: u32 = 0x0001_BCC0;
    const GUARDS: u32 = 0x0002_8848;
    const PLAYER_FACTION: u32 = 0x0000_0DB1;

    fn identity() -> LoadOrderIdentity {
        LoadOrderIdentity::new(vec![(
            GlobalSlot::Regular(0),
            PluginId::from_filename("Skyrim.esm"),
        )])
    }

    fn form(faction: u32) -> FormRef {
        identity().form_ref(faction).unwrap()
    }

    fn disposition(aggression: Aggression) -> ActorAiData {
        ActorAiData {
            aggression,
            confidence: Confidence::Average,
            attack_radius: None,
        }
    }

    #[test]
    fn aggression_rules_follow_the_ai_data_definitions() {
        let aggressive = disposition(Aggression::Aggressive);
        let very = disposition(Aggression::VeryAggressive);
        let frenzied = disposition(Aggression::Frenzied);
        let calm = disposition(Aggression::Unaggressive);
        assert!(attacks(&aggressive, Standing::Enemy, 100.0));
        assert!(!attacks(&aggressive, Standing::Neutral, 100.0));
        assert!(attacks(&very, Standing::Neutral, 100.0));
        assert!(!attacks(&very, Standing::Friendly, 100.0));
        assert!(attacks(&frenzied, Standing::Friendly, 100.0));
        assert!(!attacks(&calm, Standing::Enemy, 100.0));
        // Aggro radius: a Neutral inside it is attacked even by an
        // Unaggressive actor; Friends never are.
        let guard = ActorAiData {
            attack_radius: Some(512.0),
            ..calm
        };
        assert!(attacks(&guard, Standing::Neutral, 500.0));
        assert!(!attacks(&guard, Standing::Neutral, 600.0));
        assert!(!attacks(&guard, Standing::Friendly, 100.0));
        // Cowardly never fights, not even when Frenzied.
        let coward = ActorAiData {
            confidence: Confidence::Cowardly,
            ..frenzied
        };
        assert!(!attacks(&coward, Standing::Enemy, 1.0));
    }

    #[test]
    fn standing_prefers_script_overrides_and_any_friendly_pair() {
        let catalog = FactionRelationshipCatalog::new(
            [
                FactionRelationship::new(form(BANDITS), form(BANDITS), 0, 2).unwrap(),
                FactionRelationship::new(form(GUARDS), form(BANDITS), 0, 1).unwrap(),
            ],
            false,
        )
        .unwrap();
        let none = FactionRelations::default();
        assert_eq!(
            standing(&[form(BANDITS)], &[form(BANDITS)], &none, &catalog),
            Standing::Friendly
        );
        assert_eq!(
            standing(&[form(GUARDS)], &[form(BANDITS)], &none, &catalog),
            Standing::Enemy
        );
        // Directed: the bandits authored nothing toward the guards.
        assert_eq!(
            standing(&[form(BANDITS)], &[form(GUARDS)], &none, &catalog),
            Standing::Neutral
        );
        // Guards authored no self-relation here, so the extra pair is
        // Neutral and the Enemy pair decides...
        assert_eq!(
            standing(
                &[form(GUARDS)],
                &[form(BANDITS), form(GUARDS)],
                &none,
                &catalog
            ),
            Standing::Enemy
        );
        // ...while one friendly pair outweighs everything else.
        assert_eq!(
            standing(
                &[form(BANDITS)],
                &[form(GUARDS), form(BANDITS)],
                &none,
                &catalog
            ),
            Standing::Friendly
        );
        // A scripted SetEnemy overrides the authored reaction.
        let mut overrides = FactionRelations::default();
        overrides.set_enemy(form(GUARDS), form(BANDITS), true, false);
        assert_eq!(
            standing(&[form(GUARDS)], &[form(BANDITS)], &overrides, &catalog),
            Standing::Neutral
        );
    }

    fn spawn_actor(world: &mut World, at: Vec3, faction: u32, ai: Option<ActorAiData>) -> EntityId {
        let entity = world.spawn();
        world.insert(entity, Transform::new(at, Default::default(), 1.0));
        world.insert(entity, GlobalTransform::new(at, Default::default(), 1.0));
        world.insert(entity, FactionRanks::from_pairs([(faction, 0)]));
        if let Some(ai) = ai {
            world.insert(entity, CombatDisposition(ai));
        }
        entity
    }

    fn fixture() -> World {
        let mut world = World::new();
        world.register::<AiCombatState>();
        world.register::<CombatDisposition>();
        world.insert_resource(identity());
        world.insert_resource(FactionRelations::default());
        world.insert_resource(DetectionConfig {
            max_distance: 2500.0,
            exterior_mult: 2.1,
        });
        world
    }

    /// End to end: a Very Aggressive bandit starts combat against the
    /// in-range player (Neutral) but not against a fellow bandit (the
    /// authored self-Ally is not in this catalog, so the bandits are only
    /// spared here by the scripted override) or an out-of-range villager.
    #[test]
    fn a_very_aggressive_actor_attacks_the_nearest_neutral_in_range() {
        let mut world = fixture();
        let very = disposition(Aggression::VeryAggressive);
        let bandit = spawn_actor(&mut world, Vec3::ZERO, BANDITS, Some(very));
        let other_bandit = spawn_actor(&mut world, Vec3::new(50.0, 0.0, 0.0), BANDITS, Some(very));
        world.resource_mut::<FactionRelations>().set_reaction(
            form(BANDITS),
            form(BANDITS),
            CombatReaction::Ally,
        );
        let player = spawn_actor(&mut world, Vec3::new(0.0, 0.0, 800.0), PLAYER_FACTION, None);
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        let far = spawn_actor(
            &mut world,
            Vec3::new(0.0, 0.0, 9000.0),
            GUARDS,
            Some(disposition(Aggression::Unaggressive)),
        );

        let mut system = make_faction_hostility_system();
        system(&world, EVALUATION_PERIOD_SECS * 0.5);
        assert!(
            world.get::<AiCombatState>(bandit).is_none(),
            "throttled: nothing is evaluated before the period elapses"
        );
        system(&world, EVALUATION_PERIOD_SECS);
        for attacker in [bandit, other_bandit] {
            assert_eq!(
                world.get::<AiCombatState>(attacker).map(|s| s.target),
                Some(player)
            );
        }
        assert!(world.get::<AiCombatState>(far).is_none());
        assert!(world.get::<AiCombatState>(player).is_none());
    }

    #[test]
    fn no_detection_config_means_no_ambient_hostility() {
        let mut world = fixture();
        world.remove_resource::<DetectionConfig>();
        let frenzied = disposition(Aggression::Frenzied);
        let a = spawn_actor(&mut world, Vec3::ZERO, BANDITS, Some(frenzied));
        spawn_actor(&mut world, Vec3::X * 10.0, GUARDS, Some(frenzied));
        let mut system = make_faction_hostility_system();
        system(&world, EVALUATION_PERIOD_SECS);
        assert!(world.get::<AiCombatState>(a).is_none());
    }
}
