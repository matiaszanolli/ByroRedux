//! M47.2 — projectiles launched by `Weapon.Fire` (the host half of
//! [`byroredux_scripting::PendingProjectileLaunches`]).
//!
//! CK *Fire - Weapon*: "Fires the weapon from the specified object reference,
//! using the specified ammo", `akAmmo` None being the reference's current ammo.
//! The ammo (`AMMO.DATA`) names its projectile (`PROJ`); the projectile record
//! (xEdit `wbPROJ`) gives the speed in game units per second, the range in game
//! units, a lifetime in seconds (0: live out the range) and a gravity the CK
//! describes only as "how much gravity has an effect on the projectile".
//!
//! What flies here: a straight line along the source reference's forward, at the
//! record's speed, until it hits a solid, travels its range or outlives its
//! lifetime. A hit on a live actor queues a [`HitEvent`] for the damage pipeline
//! (so health, death, bleedout, `OnHit` and the story events all follow) worth
//! the weapon's damage plus the ammo's (Skyrim: a bow's damage and its arrow's
//! add). Not modelled, and said so: a projectile with a non-zero gravity (the
//! unit is unspecified, so it is refused rather than guessed), the projectile
//! model, impact force on bodies, and the explosion a record names.
//!
//! Projectiles are plain data in [`ActiveProjectiles`], not entities: systems
//! cannot spawn, and nothing here needs a scene node. Transient, never saved.

use byroredux_core::ecs::components::{Dead, GlobalTransform};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::records::ItemKind;

/// The corridor a flying projectile sweeps, at least: authored actor bone
/// colliders are small boxes with gaps between them (the melee sweep measured
/// 16-18 BU boxes on FNV humanoids), and a zero-width segment can thread them.
/// An engine approximation, like the melee corridor — not game data.
const MIN_HIT_CORRIDOR_BU: f32 = 4.0;

/// How far past the source the projectile starts, so a ray that starts at a wall
/// opening does not begin inside the wall's own collider.
const MUZZLE_OFFSET_BU: f32 = 2.0;

/// One projectile in flight.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Projectile {
    pub position: Vec3,
    pub velocity: Vec3,
    pub range_left: f32,
    /// Seconds left; `None` for a lifetime of 0 (the CK: "arrows are set to 0.00
    /// to live forever" — they end on range or impact).
    pub lifetime_left: Option<f32>,
    pub damage: f32,
    pub shooter: EntityId,
    pub radius: f32,
    /// Bodies owned by the shooter when this projectile launched. This list is
    /// stable for the projectile's flight and avoids walking every physics
    /// entity to rebuild it each frame.
    launcher_bodies: std::sync::Arc<[byroredux_physics::RigidBodyHandle]>,
}

/// Every projectile in flight.
#[derive(Debug, Default)]
pub(crate) struct ActiveProjectiles(pub Vec<Projectile>);

impl Resource for ActiveProjectiles {}

/// How far above its placement an actor's body centre is: an NPC's placement root stands at its feet, the
/// player body's transform is already its capsule centre.
fn body_centre_offset(world: &World, entity: EntityId) -> Vec3 {
    let player = world.try_resource::<crate::systems::PlayerEntity>().and_then(|player| player.0);
    if Some(entity) == player || world.get::<byroredux_core::ecs::components::ActorVitals>(entity).is_none() {
        return Vec3::ZERO;
    }
    Vec3::Y * crate::npc_spawn::FALLBACK_ACTOR_CAPSULE_CENTRE_HEIGHT
}

/// What a shot with `weapon` (and `ammo`, 0 for the weapon's own) is made of: the weapon's damage, the
/// ammunition's, and the projectile record the ammunition names.
pub(crate) fn resolve_shot<'a>(
    index: &'a byroredux_plugin::esm::records::EsmIndex,
    weapon: u32,
    ammo: u32,
) -> Result<(f32, f32, &'a byroredux_plugin::esm::records::ProjRecord), String> {
    let (weapon_damage, ammo_damage, ammo_form, projectile_form) = resolve_ammunition(index, weapon, ammo)?;
    let proj = index
        .projectiles
        .get(&projectile_form)
        .ok_or_else(|| format!("ammo {ammo_form:08X} names no projectile record"))?;
    if proj.gravity != 0.0 {
        return Err(format!(
            "{} has gravity {} and the gravity unit is unspecified",
            proj.editor_id, proj.gravity
        ));
    }
    if !(proj.muzzle_speed.is_finite() && proj.muzzle_speed > 0.0) {
        return Err(format!("{} has no speed", proj.editor_id));
    }
    Ok((weapon_damage, ammo_damage, proj))
}

/// The weapon's damage, the ammunition's, the ammunition's form and its projectile form. Fallout's
/// weapons name a form list of the ammo they take (`AmmoList10mm`): the first entry is the ammunition
/// used when none is named (the GECK says only that the list is what the weapon accepts).
pub(crate) fn resolve_ammunition(
    index: &byroredux_plugin::esm::records::EsmIndex,
    weapon: u32,
    ammo: u32,
) -> Result<(f32, f32, u32, u32), String> {
    let (weapon_damage, weapon_ammo) = match index.items.get(&weapon).map(|item| &item.kind) {
        Some(ItemKind::Weapon { damage, ammo_form, .. }) => (*damage as f32, *ammo_form),
        _ => return Err(format!("{weapon:08X} is not a weapon")),
    };
    let ammo_form = if ammo != 0 { ammo } else { weapon_ammo };
    let ammo_form = index.form_lists.get(&ammo_form).and_then(|list| list.entries.first().copied()).unwrap_or(ammo_form);
    if ammo_form == 0 {
        return Err("no ammo named and the weapon carries none".into());
    }
    match index.items.get(&ammo_form).map(|item| &item.kind) {
        Some(ItemKind::Ammo { damage, projectile_form, .. }) => Ok((weapon_damage, *damage, ammo_form, *projectile_form)),
        _ => Err(format!("{ammo_form:08X} is not ammunition")),
    }
}

/// Why a launch did not start (logged at debug).
fn launch_of(
    world: &World,
    index: &byroredux_plugin::esm::records::EsmIndex,
    launch: byroredux_scripting::ProjectileLaunch,
) -> Result<Projectile, String> {
    let (weapon_damage, ammo_damage, proj) = resolve_shot(index, launch.weapon, launch.ammo)?;
    // (`resolve_entity_by_global_form_id` maps PlayerRef `0x14` to the player.)
    let source = byroredux_scripting::condition::resolve_entity_by_global_form_id(world, launch.source)
        .ok_or_else(|| format!("source {:08X} is not loaded", launch.source))?;
    let transform = world
        .get::<GlobalTransform>(source)
        .map(|t| (t.translation, t.rotation, t.scale))
        .ok_or("the source has no placement")?;
    let forward = (transform.1 * -Vec3::Z).normalize_or_zero();
    if forward == Vec3::ZERO {
        return Err("the source has no forward".into());
    }
    // `UseWeapon` aims at its target's centre from the shooter's, instead of along the shooter's forward.
    let aim = if launch.aim_at != 0 {
        byroredux_scripting::condition::resolve_entity_by_global_form_id(world, launch.aim_at)
            .and_then(|target| world.get::<GlobalTransform>(target).map(|t| (target, t.translation)))
            .map(|(target, position)| (position + body_centre_offset(world, target), body_centre_offset(world, source)))
    } else {
        None
    };
    let direction = match aim {
        Some((target_point, shooter_offset)) => (target_point - (transform.0 + shooter_offset)).normalize_or_zero(),
        None => forward,
    };
    let direction = if direction == Vec3::ZERO { forward } else { direction };
    // The model's own `ProjectileNode` when it authors one (a dart port's muzzle is
    // 16 units ahead of its origin, outside the wall it is mounted in); else a
    // short step past the source.
    let start = match world.get::<crate::components::ProjectileMuzzle>(source) {
        Some(muzzle) => transform.0 + transform.1 * (muzzle.0 * transform.2),
        None => transform.0 + aim.map_or(Vec3::ZERO, |(_, shooter_offset)| shooter_offset) + direction * MUZZLE_OFFSET_BU,
    };
    Ok(Projectile {
        position: start,
        velocity: direction * proj.muzzle_speed,
        range_left: if proj.range > 0.0 { proj.range } else { f32::INFINITY },
        lifetime_left: (proj.lifetime > 0.0).then_some(proj.lifetime),
        damage: weapon_damage + ammo_damage.max(0.0),
        shooter: source,
        radius: proj.collision_radius.max(0.0),
        launcher_bodies: std::sync::Arc::from([]),
    })
}

/// Start this frame's `Weapon.Fire` requests.
pub(crate) fn projectile_launch_system(world: &World, _dt: f32) {
    let launches = world
        .try_resource_mut::<byroredux_scripting::PendingProjectileLaunches>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    if launches.is_empty() {
        return;
    }
    let Some(index) = world
        .try_resource::<crate::cell_loader::LoadedCellIndex>()
        .map(|index| index.0.clone())
    else {
        return;
    };
    let owners: Vec<_> = world
        .query::<byroredux_physics::RapierHandles>()
        .map(|handles| handles.iter().map(|(entity, handles)| (entity, handles.body)).collect())
        .unwrap_or_default();
    let mut launcher_cache = std::collections::HashMap::new();
    let mut started = Vec::new();
    for launch in launches {
        match launch_of(world, &index, launch) {
            Ok(mut projectile) => {
                let bodies = launcher_cache
                    .entry(projectile.shooter)
                    .or_insert_with(|| std::sync::Arc::from(launcher_bodies(world, projectile.shooter, &owners)));
                projectile.launcher_bodies = std::sync::Arc::clone(bodies);
                log::debug!(
                    "Weapon.Fire {:08X} from {:08X}: {} damage at {} u/s",
                    launch.weapon,
                    launch.source,
                    projectile.damage,
                    projectile.velocity.length()
                );
                started.push(projectile);
            }
            Err(why) => log::debug!("Weapon.Fire {:08X} from {:08X}: {why}", launch.weapon, launch.source),
        }
    }
    if let Some(mut active) = world.try_resource_mut::<ActiveProjectiles>() {
        active.0.extend(started);
    }
}

/// What a projectile's step did.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Step {
    /// Still flying, now at this projectile.
    Flying(Projectile),
    /// Ended without hitting anything (range or lifetime).
    Expired,
    /// Ended on a hit `distance` along its step, on this body.
    Hit { distance: f32 },
}

/// Advance `projectile` by `dt`, given the distance to the first solid its step
/// meets (`None`: clear air).
pub(crate) fn advance(projectile: Projectile, dt: f32, mut first_hit: impl FnMut(Vec3, Vec3, f32) -> Option<f32>) -> Step {
    let speed = projectile.velocity.length();
    if speed == 0.0 || dt <= 0.0 {
        return Step::Flying(projectile);
    }
    let length = (speed * dt).min(projectile.range_left);
    let direction = projectile.velocity / speed;
    if let Some(distance) = first_hit(projectile.position, direction, length) {
        return Step::Hit { distance };
    }
    let range_left = projectile.range_left - length;
    let lifetime_left = projectile.lifetime_left.map(|left| left - dt);
    if range_left <= 0.0 || lifetime_left.is_some_and(|left| left <= 0.0) {
        return Step::Expired;
    }
    Step::Flying(Projectile {
        position: projectile.position + direction * length,
        range_left,
        lifetime_left,
        ..projectile
    })
}

/// Fly every projectile one step; a hit on a live actor queues its `HitEvent`.
pub(crate) fn projectile_flight_system(world: &World, dt: f32) {
    let flying = match world.try_resource_mut::<ActiveProjectiles>() {
        Some(mut active) if !active.0.is_empty() => std::mem::take(&mut active.0),
        _ => return,
    };
    // Resolve body ownership before taking the physics world (the melee sweep's
    // lock order).
    let owners: std::collections::HashMap<_, _> = world
        .query::<byroredux_physics::RapierHandles>()
        .map(|handles| handles.iter().map(|(entity, handles)| (handles.body, entity)).collect())
        .unwrap_or_default();
    // #5955 — actors' bodies, resolved once per frame before the physics world
    // is taken. `resolve_actor` reads `ActorColliderOwner` / `ActorVitals`, and
    // `PhysicsWorld` is a sink in the canonical lock order: resolving lazily
    // from the ray-cast filter recorded `ActorVitals` → … → `PhysicsWorld` →
    // `ActorVitals`. Per-frame, not per-projectile-step, so the cost is
    // O(owners) once however many projectiles are flying.
    let actor_bodies: std::collections::HashSet<byroredux_physics::RigidBodyHandle> = owners
        .iter()
        .filter(|(_, entity)| resolve_actor(world, **entity).is_some())
        .map(|(body, _)| *body)
        .collect();
    let mut remaining = Vec::with_capacity(flying.len());
    let mut hits: Vec<(EntityId, EntityId, f32)> = Vec::new();
    for projectile in flying {
        let excluded = projectile.launcher_bodies.as_ref();
        let is_actor_body = |body| actor_bodies.contains(&body);
        let mut hit_body = None;
        let step = {
            let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();
            advance(projectile.clone(), dt, |origin, direction, length| {
                let physics = physics.as_ref()?;
                // Solid geometry with a thin ray (the projectile's own radius): a
                // corridor as wide as an actor's bone boxes would start inside the
                // wall the launcher is mounted in. Actors with the wider corridor.
                let solid = physics.cast_ray_corridor_where(origin, direction, length, projectile.radius, &|body| {
                    !is_actor_body(body) && !excluded.contains(&body)
                });
                let actor = physics.cast_ray_corridor_where(
                    origin,
                    direction,
                    length,
                    projectile.radius.max(MIN_HIT_CORRIDOR_BU),
                    &|body| is_actor_body(body) && !excluded.contains(&body),
                );
                let hit = match (solid, actor) {
                    (Some(solid), Some(actor)) => Some(if actor.distance <= solid.distance { actor } else { solid }),
                    (solid, actor) => solid.or(actor),
                }?;
                hit_body = hit.body;
                Some(hit.distance)
            })
        };
        match step {
            Step::Flying(next) => remaining.push(next),
            Step::Expired => log::debug!("projectile from {} expired at {:?}", projectile.shooter, projectile.position),
            Step::Hit { distance } => {
                let target = hit_body.and_then(|body| {
                    owners.get(&body).and_then(|collider| resolve_actor(world, *collider))
                });
                log::debug!(
                    "projectile from {} hit entity {:?} after {distance:.0} u: actor {target:?}",
                    projectile.shooter,
                    hit_body.and_then(|body| owners.get(&body).copied())
                );
                if let Some(target) = target {
                    hits.push((target, projectile.shooter, projectile.damage));
                }
            }
        }
    }
    if let Some(mut active) = world.try_resource_mut::<ActiveProjectiles>() {
        // New launches may have been added meanwhile (none can: this system owns
        // the take), so the survivors simply come back.
        active.0.extend(remaining);
    }
    hits.retain(|&(target, shooter, _)| target != shooter && world.get::<Dead>(target).is_none());
    if hits.is_empty() {
        return;
    }
    // #5860 — queued, not inserted: `HitEvent` is one row per target, and a
    // direct insert replaced a same-frame melee hit. The trap-hit flush, which
    // runs right after this, lands the queue and holds a hit whose target was
    // already struck this frame.
    if let Some(mut pending) = world.try_resource_mut::<byroredux_scripting::PendingTrapHits>() {
        pending.0.extend(hits.into_iter().map(|(target, shooter, damage)| {
            byroredux_scripting::TrapHit {
                target,
                trap: shooter,
                damage,
            }
        }));
    }
}

/// How many parents up a collider entity is searched for its launcher.
const MAX_LAUNCHER_HOPS: usize = 16;

/// Every body that belongs to `shooter`'s reference: its own, those of entities
/// beneath it, and those of the bare collision entities whose `PhysicsSourceForm`
/// names it.
fn launcher_bodies(
    world: &World,
    shooter: EntityId,
    owners: &[(EntityId, byroredux_physics::RigidBodyHandle)],
) -> Vec<byroredux_physics::RigidBodyHandle> {
    let form = world.get::<byroredux_core::ecs::components::FormIdComponent>(shooter).map(|id| id.0);
    owners
        .iter()
        .filter(|(entity, _)| {
            if *entity == shooter {
                return true;
            }
            let mut at = *entity;
            for _ in 0..MAX_LAUNCHER_HOPS {
                let Some(parent) = world.get::<byroredux_core::ecs::Parent>(at).map(|parent| parent.0) else {
                    break;
                };
                if parent == shooter {
                    return true;
                }
                at = parent;
            }
            form.is_some_and(|form| {
                world
                    .get::<byroredux_core::ecs::components::PhysicsSourceForm>(*entity)
                    .is_some_and(|source| source.0 == form)
            })
        })
        .map(|(_, body)| *body)
        .collect()
}

/// The actor a collider belongs to, if it belongs to one.
fn resolve_actor(world: &World, collider_entity: EntityId) -> Option<EntityId> {
    world
        .get::<byroredux_physics::ActorColliderOwner>(collider_entity)
        .map(|owner| owner.0)
        .or_else(|| {
            world
                .get::<byroredux_core::ecs::components::ActorVitals>(collider_entity)
                .map(|_| collider_entity)
        })
        .filter(|actor| world.get::<byroredux_core::ecs::components::ActorVitals>(*actor).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dart() -> Projectile {
        Projectile {
            position: Vec3::ZERO,
            velocity: Vec3::new(0.0, 0.0, -1300.0),
            range_left: 10000.0,
            lifetime_left: None,
            damage: 12.0,
            shooter: 7,
            radius: 1.0,
            launcher_bodies: std::sync::Arc::from([]),
        }
    }

    fn index_with_dart(gravity: f32) -> byroredux_plugin::esm::records::EsmIndex {
        use byroredux_plugin::esm::records::{common::CommonItemFields, EsmIndex, ItemRecord, ProjRecord};
        let mut index = EsmIndex::default();
        index.items.insert(
            0xA1,
            ItemRecord {
                form_id: 0xA1,
                common: CommonItemFields::default(),
                kind: ItemKind::Weapon {
                    ammo_form: 0,
                    damage: 3,
                    clip_size: 0,
                    anim_type: 0,
                    ap_cost: 0.0,
                    skill_form: 0,
                    min_spread: 0.0,
                    spread: 0.0,
                    crit_mult: 1.0,
                    reach: 0.0,
                    speed: 1.0,
                    reload_anim: 0,
                    vats: None,
                },
            },
        );
        index.items.insert(
            0xA2,
            ItemRecord {
                form_id: 0xA2,
                common: CommonItemFields::default(),
                kind: ItemKind::Ammo {
                    damage: 2.0,
                    dt_mult: 1.0,
                    spread: 0.0,
                    projectile_form: 0xA3,
                    casing_form: 0,
                    health: 0,
                    clip_rounds: 0,
                },
            },
        );
        index.projectiles.insert(
            0xA3,
            ProjRecord {
                form_id: 0xA3,
                editor_id: "TrapDartWooden01".into(),
                muzzle_speed: 1300.0,
                range: 10000.0,
                gravity,
                collision_radius: 1.0,
                ..Default::default()
            },
        );
        index
    }

    /// A launch resolves weapon + ammo + projectile into a straight flight along
    /// the source's forward, worth the weapon's damage plus the ammo's; a
    /// projectile with gravity is refused (the unit is unspecified), as is a
    /// weapon that names no ammo when none is passed.
    #[test]
    fn a_launch_resolves_the_records_into_a_flight() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
        use byroredux_scripting::ProjectileLaunch;
        let mut world = World::new();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();
        let mut pool = FormIdPool::default();
        let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(0xB1) });
        world.insert_resource(pool);
        let port = world.spawn();
        world.insert(port, FormIdComponent(id));
        world.insert(
            port,
            GlobalTransform { translation: Vec3::new(10.0, 20.0, 30.0), ..GlobalTransform::IDENTITY },
        );
        let launch = ProjectileLaunch { source: 0xB1, weapon: 0xA1, ammo: 0xA2, aim_at: 0 };
        let flight = launch_of(&world, &index_with_dart(0.0), launch).expect("a dart flies");
        assert_eq!(flight.shooter, port);
        assert_eq!(flight.damage, 5.0, "weapon 3 + ammo 2");
        assert_eq!(flight.velocity, Vec3::new(0.0, 0.0, -1300.0), "speed along the source's forward");
        assert_eq!(flight.position, Vec3::new(10.0, 20.0, 28.0), "starts past the source");
        assert_eq!((flight.range_left, flight.lifetime_left), (10000.0, None));
        // A launcher whose model authors a `ProjectileNode` starts there (a dart port's
        // is 16 units ahead of its origin, outside the wall it is mounted in).
        world.register::<crate::components::ProjectileMuzzle>();
        world
            .query_mut::<crate::components::ProjectileMuzzle>()
            .unwrap()
            .insert(port, crate::components::ProjectileMuzzle(Vec3::new(0.0, 0.0, -16.0)));
        let muzzled = launch_of(&world, &index_with_dart(0.0), launch).expect("a dart flies");
        assert_eq!(muzzled.position, Vec3::new(10.0, 20.0, 14.0), "the ProjectileNode, 16 units along forward");
        // Gravity has no specified unit: refused. No ammo anywhere: refused.
        assert!(launch_of(&world, &index_with_dart(0.8), launch).is_err());
        assert!(launch_of(&world, &index_with_dart(0.0), ProjectileLaunch { ammo: 0, ..launch }).is_err());
    }

    /// A dart flying into a live actor's collider queues the hit the flush lands
    /// as its `HitEvent` (shooter as the aggressor, the launch's damage); into a
    /// bare wall it only ends; and into the open it flies on.
    #[test]
    fn flight_into_an_actor_queues_a_hit_and_into_a_wall_only_ends() {
        use byroredux_core::ecs::components::ActorVitals;
        use byroredux_physics::PhysicsWorld;
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<byroredux_physics::RapierHandles>();
        world.register::<ActorVitals>();
        world.register::<Dead>();
        world.register::<byroredux_physics::ActorColliderOwner>();
        world.insert_resource(ActiveProjectiles::default());
        let mut physics = PhysicsWorld::new();
        let actor_handles = physics.spawn_fixed_cuboid_handles(Vec3::splat(20.0), Vec3::new(0.0, 0.0, -500.0));
        let wall_handles = physics.spawn_fixed_cuboid_handles(Vec3::splat(20.0), Vec3::new(300.0, 0.0, -500.0));
        world.insert_resource(physics);
        let shooter = world.spawn();
        let actor = world.spawn();
        let wall = world.spawn();
        world.insert(actor, actor_handles);
        world.insert(wall, wall_handles);
        world.insert(actor, ActorVitals { health: 0x3E8 });
        let shot = |x: f32| Projectile { position: Vec3::new(x, 0.0, 0.0), ..Projectile { shooter, ..dart() } };
        world.resource_mut::<ActiveProjectiles>().0 = vec![shot(0.0), shot(300.0), shot(-300.0)];
        // 0.5 s at 1300 u/s is 650 units: past both slabs at z -500, short of range.
        projectile_flight_system(&world, 0.5);
        assert!(world.resource::<ActiveProjectiles>().0.iter().all(|p| p.position.x == -300.0), "only the dart that missed flies on");
        byroredux_scripting::pending_trap_hit_flush_system(&world, 0.0);
        let events = world.query::<byroredux_scripting::HitEvent>().unwrap();
        let hit = events.get(actor).expect("the actor was hit");
        assert_eq!((hit.aggressor, hit.damage), (shooter, 12.0));
        assert!(events.get(wall).is_none(), "a bare wall takes no HitEvent");
    }

    #[test]
    fn flight_ignores_the_launchers_cached_body() {
        use byroredux_core::ecs::components::ActorVitals;
        use byroredux_physics::PhysicsWorld;
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<byroredux_physics::RapierHandles>();
        world.register::<ActorVitals>();
        world.register::<Dead>();
        world.register::<byroredux_physics::ActorColliderOwner>();
        world.insert_resource(ActiveProjectiles::default());

        let mut physics = PhysicsWorld::new();
        let launcher_handles = physics.spawn_fixed_cuboid_handles(Vec3::splat(20.0), Vec3::new(0.0, 0.0, -50.0));
        let actor_handles = physics.spawn_fixed_cuboid_handles(Vec3::splat(20.0), Vec3::new(0.0, 0.0, -500.0));
        world.insert_resource(physics);

        let shooter = world.spawn();
        let actor = world.spawn();
        world.insert(shooter, launcher_handles);
        world.insert(actor, actor_handles);
        world.insert(actor, ActorVitals { health: 0x3E8 });
        world.resource_mut::<ActiveProjectiles>().0.push(Projectile {
            shooter,
            launcher_bodies: std::sync::Arc::from([launcher_handles.body]),
            ..dart()
        });

        projectile_flight_system(&world, 0.5);
        byroredux_scripting::pending_trap_hit_flush_system(&world, 0.0);
        let hit = world.get::<byroredux_scripting::HitEvent>(actor).expect("the actor beyond the launcher is hit");
        assert_eq!(hit.aggressor, shooter);
    }

    /// #5860 — a dart landing the frame a melee strike hit the same actor waits
    /// a frame rather than replacing the strike, so both hits reach the damage
    /// pipeline.
    #[test]
    fn a_same_frame_melee_hit_survives_a_projectile_hit() {
        use byroredux_core::ecs::components::ActorVitals;
        use byroredux_physics::PhysicsWorld;
        use byroredux_scripting::HitEvent;
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<byroredux_physics::RapierHandles>();
        world.register::<ActorVitals>();
        world.register::<Dead>();
        world.register::<byroredux_physics::ActorColliderOwner>();
        world.insert_resource(ActiveProjectiles::default());
        let mut physics = PhysicsWorld::new();
        let actor_handles = physics.spawn_fixed_cuboid_handles(Vec3::splat(20.0), Vec3::new(0.0, 0.0, -500.0));
        world.insert_resource(physics);
        let (shooter, swordsman, actor) = (world.spawn(), world.spawn(), world.spawn());
        world.insert(actor, actor_handles);
        world.insert(actor, ActorVitals { health: 0x3E8 });
        world.insert(
            actor,
            HitEvent {
                aggressor: swordsman,
                source: swordsman,
                projectile: 0,
                damage: 25.0,
                power_attack: false,
                sneak_attack: false,
                bash_attack: false,
                blocked: false,
            },
        );
        world.resource_mut::<ActiveProjectiles>().0 = vec![Projectile { shooter, ..dart() }];
        let landed = |world: &World| {
            world
                .query::<HitEvent>()
                .unwrap()
                .get(actor)
                .map(|event| (event.aggressor, event.damage))
        };

        projectile_flight_system(&world, 0.5);
        byroredux_scripting::pending_trap_hit_flush_system(&world, 0.0);
        assert_eq!(landed(&world), Some((swordsman, 25.0)), "the melee hit stands");

        // `event_cleanup_system` drains the frame's hits; the dart lands next.
        world.remove::<HitEvent>(actor);
        byroredux_scripting::pending_trap_hit_flush_system(&world, 0.0);
        assert_eq!(landed(&world), Some((shooter, 12.0)));
    }

    /// A projectile advances speed × dt along its velocity, spending range, and
    /// ends on range or lifetime.
    #[test]
    fn flight_advances_and_expires() {
        let Step::Flying(next) = advance(dart(), 0.1, |_, _, _| None) else { panic!("still flying") };
        assert!((next.position.z + 130.0).abs() < 1e-3, "{:?}", next.position);
        assert!((next.range_left - 9870.0).abs() < 1e-3);
        let short = Projectile { range_left: 100.0, ..dart() };
        assert_eq!(advance(short, 0.1, |_, _, _| None), Step::Expired, "130 units spend a 100 unit range");
        let timed = Projectile { lifetime_left: Some(0.05), ..dart() };
        assert_eq!(advance(timed, 0.1, |_, _, _| None), Step::Expired);
    }

    /// The sweep asks about exactly the step ahead, and a hit ends the flight.
    #[test]
    fn a_solid_in_the_step_ends_the_flight() {
        let seen = std::cell::Cell::new((Vec3::ZERO, Vec3::ZERO, 0.0));
        let step = advance(dart(), 0.1, |origin, direction, length| {
            seen.set((origin, direction, length));
            Some(40.0)
        });
        assert_eq!(step, Step::Hit { distance: 40.0 });
        let (origin, direction, length) = seen.get();
        assert_eq!(origin, Vec3::ZERO);
        assert!((direction - Vec3::NEG_Z).length() < 1e-6);
        assert!((length - 130.0).abs() < 1e-3);
    }
}
