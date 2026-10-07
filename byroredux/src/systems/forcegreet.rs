//! Force-greet — the Dialogue AI package procedure (FO3/FNV `PKDT`
//! procedure 15), #5367 Phase F.
//!
//! A procedure-15 package makes the NPC approach the player and open a
//! conversation *without activation*, speaking the package's authored
//! topic (`PKDD`'s FormID, decoded on [`PackRecord::dialogue_topic`]) or
//! the master's generic greeting when none is authored. Vanilla FO3/FNV
//! never lists these packages on NPC_ defaults (0 references across both
//! masters) — they are installed by quest scripts — so the runtime keys
//! on a [`ForceGreetDirective`] bridge component that any installer can
//! stamp: the `dialogue.forcegreet` console door today, quest/alias
//! package installers when those paths carry one.

use crate::systems::npc_dialogue::forcegreet_open;
use byroredux_core::ecs::components::{GlobalTransform, Transform};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_core::math::Vec3;

/// The force-greet bridge state. One-shot: consumed the moment the
/// conversation opens (removed with the directive). `NOT_SAVED_BY_DESIGN`
/// — a save taken mid-approach re-greets after load, which is the
/// behavior the procedure describes anyway.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ForceGreetDirective {
    /// The topic the package authors (`PKDD`), or `None` for the
    /// master's generic greeting.
    pub topic: Option<u32>,
    /// Open distance in Bethesda units (vanilla's force-greet radius
    /// family is 100-200; the console door defaults to 128).
    pub radius: f32,
}

impl Component for ForceGreetDirective {
    type Storage = SparseSetStorage<Self>;
}

/// System: walk directive carriers toward the player; on arrival open
/// the conversation through the same selection the activation path
/// uses. Registered in `Stage::Update` after the ambient package
/// system. Movement rides the shared `step_toward` locomotion helper
/// (KCC-backed, navmesh-routing when a resident tile covers the actor).
pub(crate) fn forcegreet_system(world: &World, dt: f32) {
    let directives: Vec<(EntityId, ForceGreetDirective)> = world
        .query::<ForceGreetDirective>()
        .map(|query| query.iter().map(|(npc, d)| (npc, *d)).collect())
        .unwrap_or_default();
    if directives.is_empty() {
        return;
    }
    let Some(player) = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0)
    else {
        return;
    };
    let Some(player_pos) = world.get::<GlobalTransform>(player).map(|t| t.translation) else {
        return;
    };
    let physics_guard = world.try_resource::<byroredux_physics::PhysicsWorld>();
    let physics = physics_guard.as_deref();
    let mut opened: Vec<EntityId> = Vec::new();
    for (npc, directive) in directives {
        // Refusing NPCs (dead/combat/unconscious) drop the directive —
        // the same refusal gate the activation path applies.
        if crate::systems::npc_dialogue::npc_refuses_dialogue(world, npc).is_some() {
            opened.push(npc); // consume below
            continue;
        }
        let Some(current) = world.get::<GlobalTransform>(npc).map(|t| t.translation) else {
            continue;
        };
        let flat = Vec3::new(player_pos.x - current.x, 0.0, player_pos.z - current.z);
        if flat.length() > directive.radius {
            // Still approaching: one walk step toward the player.
            let speed = world
                .get::<crate::components::WalkSpeed>(npc)
                .map(|speed| speed.0)
                .unwrap_or(crate::systems::locomotion::LOCOMOTION_WALK_SPEED);
            let target = Vec3::new(player_pos.x, current.y, player_pos.z);
            let (new_pos, new_rotation) = crate::systems::locomotion::step_toward(
                current,
                world
                    .get::<Transform>(npc)
                    .map(|t| t.rotation)
                    .unwrap_or_default(),
                target,
                dt,
                speed,
                physics,
            );
            if let Some(mut transforms) = world.query_mut::<GlobalTransform>() {
                if let Some(t) = transforms.get_mut(npc) {
                    t.translation = new_pos;
                }
            }
            if let (Some(new_rotation), Some(mut transforms)) =
                (new_rotation, world.query_mut::<Transform>())
            {
                if let Some(t) = transforms.get_mut(npc) {
                    t.rotation = new_rotation;
                }
            }
            continue;
        }
        if forcegreet_open(world, npc, directive.topic) {
            opened.push(npc);
        }
    }
    // Consume directives whose conversation opened (or whose carrier
    // refuses one) — one greet per install.
    if !opened.is_empty() {
        if let Some(mut directives) = world.query_mut::<ForceGreetDirective>() {
            for npc in opened {
                directives.remove(npc);
            }
        }
    }
}
