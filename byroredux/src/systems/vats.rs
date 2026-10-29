//! VATS mode machine (#6350) — the enter/exit transitions on the VATS
//! key edge, and (Phase 3c) the targeting scan + queue execution that
//! ride the same resource.
//!
//! The system runs in `Stage::Early` after `player_controller_system`
//! (so this frame's `ActionState` edges are refreshed) and — like every
//! Early system — it runs even while the world is frozen: dt is a
//! parameter, not a precondition, the same property the loading cover's
//! debug drain already relies on.

use crate::components::{VatsMode, VatsState};
use crate::interaction::ActionState;
use crate::interaction::InputAction::Vats;
use crate::systems::{player_can_act, PlayerEntity, PlayerMode};
use byroredux_core::character::VatsModel;
use byroredux_core::ecs::{EntityId, World};

/// Enter/exit VATS on the bound key's press edge.
///
/// Entry gates (each its own degraded "no", never a failure):
/// * a live [`VatsModel`] — non-Fallout loads never enter;
/// * `player_can_act` — a dead player, or none at all (fly-cam, tests);
/// * the scripting layer's `Control::Vats` domain enabled
///   (`InputEnableLayer`'s first reader — absent resource = enabled).
///
/// Exit clears the scan and queue; un-executed AP reservations are the
/// caller's to refund (Phase 3c's confirm path owns the spend).
pub(crate) fn vats_mode_system(world: &World, _dt: f32) {
    let pressed = world
        .try_resource::<ActionState>()
        .is_some_and(|actions| actions.was_pressed(Vats));
    if !pressed {
        return;
    }
    // Read-only mode check first, then drop the guard before the gate
    // checks: `player_can_act` itself reads `VatsState` (the VATS
    // suppression gate), and holding this write lock across it is the
    // same-thread deadlock the lock tracker exists to catch.
    let entering = world
        .try_resource::<VatsState>()
        .is_some_and(|vats| vats.mode == VatsMode::None);
    if !entering {
        if let Some(mut vats) = world.try_resource_mut::<VatsState>() {
            vats.exit_all();
        }
        return;
    }
    if vats_entry_refusal(world).is_some() {
        return;
    }
    if let Some(mut vats) = world.try_resource_mut::<VatsState>() {
        vats.mode = VatsMode::TargetSelect;
    }
    // Entering is a scan: fill the target list the overlay and queue
    // read. Re-runs (`vats.scan`) re-price against live positions.
    vats_scan(world);
}

/// Why VATS cannot be entered right now, or `None` when it can: the one list
/// of entry gates the key ([`vats_mode_system`]) and `vats.enter` share, so
/// the two cannot drift apart (#6469). The caller has already read the mode.
pub(crate) fn vats_entry_refusal(world: &World) -> Option<&'static str> {
    if world.try_resource::<VatsModel>().is_none() {
        return Some("no VATS tuning (non-Fallout load?)");
    }
    if !player_can_act(world) {
        return Some("player cannot act");
    }
    if world
        .try_resource::<PlayerMode>()
        .is_none_or(|mode| *mode == PlayerMode::FlyCam)
    {
        return Some("no player body (fly-cam)");
    }
    if !control_vats_enabled(world) {
        return Some("the VATS control is disabled");
    }
    None
}

/// The `Control::Vats` input-enable domain — the first reader of the
/// domain the scripting crate has tracked since it was written
/// (`input_layers.rs`: "VATS … answer the Is…Enabled queries; nothing
/// reads them yet"). Absent layer resource or absent layer stack
/// defaults to enabled.
fn control_vats_enabled(world: &World) -> bool {
    world
        .try_resource::<byroredux_scripting::input_layers::InputLayers>()
        .is_none_or(|layers| layers.enabled(byroredux_scripting::input_layers::Control::Vats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::VatsMode;
    use crate::components::InputState;
    use crate::interaction::{refresh_action_state, ActionBindings};
    use crate::systems::PlayerEntity;
    use byroredux_core::ecs::components::Dead;
    use byroredux_scripting::input_layers::{Control, InputLayers};
    use winit::keyboard::KeyCode;

    fn armed_world() -> World {
        let mut world = World::new();
        world.insert_resource(InputState::default());
        world.insert_resource(ActionBindings::default());
        world.insert_resource(ActionState::default());
        world.insert_resource(VatsState::default());
        world.insert_resource(VatsModel::FALLOUT3);
        world.insert_resource(PlayerMode::Character);
        let player = world.spawn();
        world.insert_resource(PlayerEntity(Some(player)));
        world
    }

    fn press_vats(world: &World) {
        world
            .resource_mut::<InputState>()
            .keys_held
            .insert(KeyCode::KeyV);
        refresh_action_state(world);
    }

    fn release_all(world: &World) {
        world.resource_mut::<InputState>().keys_held.clear();
        refresh_action_state(world);
    }

    #[test]
    fn vats_key_enters_and_exits_target_select() {
        let world = armed_world();
        assert_eq!(
            world.resource::<VatsState>().mode,
            VatsMode::None,
            "idle state"
        );
        assert_eq!(world.resource::<VatsState>().world_time_scale(), 1.0);

        press_vats(&world);
        vats_mode_system(&world, 0.0);
        let vats = world.resource::<VatsState>();
        assert_eq!(vats.mode, VatsMode::TargetSelect);
        // The published world time scale: FO3/FNV freeze solid.
        assert_eq!(vats.world_time_scale(), 0.0);
        // Normal player input is suppressed while targeting.
        assert!(!crate::systems::player_can_act(&world));
        drop(vats);

        // Release, press again: exit restores the running world.
        release_all(&world);
        press_vats(&world);
        vats_mode_system(&world, 0.0);
        let vats = world.resource::<VatsState>();
        assert_eq!(vats.mode, VatsMode::None);
        assert_eq!(vats.world_time_scale(), 1.0);
        assert!(crate::systems::player_can_act(&world));
    }

    #[test]
    fn entry_refuses_without_a_model_a_live_player_or_on_foot() {
        // No VatsModel (a non-Fallout load): never enters.
        let mut world = armed_world();
        world.remove_resource::<VatsModel>();
        press_vats(&world);
        vats_mode_system(&world, 0.0);
        assert_eq!(world.resource::<VatsState>().mode, VatsMode::None);

        // Dead player: no.
        let mut world = armed_world();
        let player = world.resource::<PlayerEntity>().0.unwrap();
        world.insert(player, Dead);
        press_vats(&world);
        vats_mode_system(&world, 0.0);
        assert_eq!(world.resource::<VatsState>().mode, VatsMode::None);

        // Fly-cam: no.
        let mut world = armed_world();
        world.remove_resource::<PlayerMode>();
        world.insert_resource(PlayerMode::FlyCam);
        press_vats(&world);
        vats_mode_system(&world, 0.0);
        assert_eq!(world.resource::<VatsState>().mode, VatsMode::None);
    }

    /// #6350 — the first reader of the `Control::Vats` input-enable
    /// domain: a Papyrus layer that disables VATS blocks entry, and an
    /// absent layer resource defaults to enabled.
    #[test]
    fn entry_honours_the_vats_control_domain() {
        let mut world = armed_world();
        let mut layers = InputLayers::default();
        let layer = layers.create();
        layers.set_enabled(layer, Control::Vats, false);
        world.insert_resource(layers);
        press_vats(&world);
        vats_mode_system(&world, 0.0);
        assert_eq!(world.resource::<VatsState>().mode, VatsMode::None);

        // Re-enable through the layer: entry works again.
        let mut layers = world.resource_mut::<InputLayers>();
        layers.set_enabled(layer, Control::Vats, true);
        drop(layers);
        release_all(&world);
        press_vats(&world);
        vats_mode_system(&world, 0.0);
        assert_eq!(world.resource::<VatsState>().mode, VatsMode::TargetSelect);
    }

    /// #6469 — the key and `vats.enter` share one gate list, so each reason the key refuses for is a
    /// reason the console refuses for too (it used to skip the fly-cam and control-domain gates).
    #[test]
    fn the_entry_refusal_covers_every_gate_the_key_applies() {
        assert_eq!(vats_entry_refusal(&armed_world()), None);

        let mut world = armed_world();
        world.remove_resource::<VatsModel>();
        assert!(vats_entry_refusal(&world).is_some_and(|why| why.contains("tuning")));

        let mut world = armed_world();
        let player = world.resource::<PlayerEntity>().0.unwrap();
        world.insert(player, Dead);
        assert!(vats_entry_refusal(&world).is_some_and(|why| why.contains("cannot act")));

        let mut world = armed_world();
        world.remove_resource::<PlayerMode>();
        world.insert_resource(PlayerMode::FlyCam);
        assert!(vats_entry_refusal(&world).is_some_and(|why| why.contains("fly-cam")));

        let mut world = armed_world();
        let mut layers = InputLayers::default();
        let layer = layers.create();
        layers.set_enabled(layer, Control::Vats, false);
        world.insert_resource(layers);
        assert!(vats_entry_refusal(&world).is_some_and(|why| why.contains("disabled")));
    }
}

// ── Phase 3c: the targeting scan, the queue, and execution ──────────

use crate::cell_loader::LoadedCellIndex;
use crate::components::VatsTargetPart;
use crate::interaction::camera_ray;
use byroredux_core::character::VatsHitInput;
use byroredux_core::ecs::components::{
    ActorValues, ActorVitals, CreatureActor, Dead, EquippedWeapon, FormIdComponent,
    GlobalTransform,
};
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::records::{BptdPart, DEFAULT_BODY_PART_DATA, ItemKind};
use byroredux_physics::RigidBodyHandle;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Head-height offset above an actor root (Bethesda units).
const EYE_HEIGHT_BU: f32 = 64.0;

/// The skill value [`VatsModel::to_hit_chance`] treats as neutral
/// (`skill_term = 1 + (skill/100 - 0.5)` is 1.0 at 50).
const NEUTRAL_SKILL: f32 = 50.0;

/// The per-part occlusion fan: the part's visible fraction is sampled by
/// three rays across its vertical extent (centre ± 18 BU ≈ a limb's
/// scale on a humanoid). Not yet the full projected-area integration
/// `fVATSScreenPercentFactor` describes — `VatsHitInput::screen_percent`
/// (0..=1) is the refinement seam, and the smoke gate's plumbing-scope
/// note says exactly that.
const PART_FAN_OFFSETS: [f32; 3] = [0.0, 18.0, -18.0];

/// The player's weapon inputs, snapshotted once per scan/confirm —
/// every number straight from the load order's records.
struct VatsWeapon {
    base_to_hit: f32,
    ap_cost: f32,
    skill_form: u32,
}

fn resolve_weapon(
    world: &World,
    index: &byroredux_plugin::esm::records::EsmIndex,
    player: EntityId,
) -> Option<VatsWeapon> {
    let base_form_id = world.get::<EquippedWeapon>(player)?.base_form_id;
    let ItemKind::Weapon {
        ap_cost,
        vats_base_to_hit,
        skill_form,
        ..
    } = &index.items.get(&base_form_id)?.kind
    else {
        return None;
    };
    // The weapon's regular cost is DNAM@68 (`ap_cost`); a weapon that authors none falls to
    // the per-anim-type standard (pistol for this gate — the anim-type mapping is Phase-4
    // polish). The `VATS` sub-record's AP is the melee special attack's (#6450), not this
    // attack's, so it is never charged here.
    Some(VatsWeapon {
        base_to_hit: *vats_base_to_hit,
        ap_cost: (*ap_cost > 0.0)
            .then_some(*ap_cost)
            .unwrap_or(world.resource::<VatsModel>().ap.attack_pistol),
        skill_form: *skill_form,
    })
}

/// Resolve an actor's limb table: `CREA.PNAM` for creatures, the
/// engine-reserved `DefaultBodyPartData` slot for humanoids, and `None`
/// when neither resolves — the caller then degrades to the implicit
/// single-part table (#6350's rule: a modded load order must never fail
/// the scan, it must degrade).
fn resolve_limb_table(
    index: &byroredux_plugin::esm::records::EsmIndex,
    body_form: Option<u32>,
    is_creature: bool,
) -> Option<&[BptdPart]> {
    let slot = body_form
        .filter(|_| is_creature)
        .unwrap_or(DEFAULT_BODY_PART_DATA);
    index
        .body_parts
        .get(&slot)
        .map(|bptd| bptd.parts.as_slice())
        .filter(|parts| !parts.is_empty())
}

/// Run the targeting scan: every living actor inside the engage
/// distance contributes its limb table's parts, each priced through
/// [`VatsModel::to_hit_chance`] with the equipped weapon. Ranged
/// weapons only for this gate — melee's reach/warp dialect is a
/// follow-up lane; with no weapon at all the scan yields empty and the
/// mode exits cleanly.
pub(crate) fn vats_scan(world: &World) {
    if let Some(mut vats) = world.try_resource_mut::<VatsState>() {
        vats.targets.clear();
    }
    let Some(model) = world.try_resource::<VatsModel>().map(|model| *model) else {
        return;
    };
    let Some(index) = world
        .try_resource::<LoadedCellIndex>()
        .map(|loaded| Arc::clone(&loaded.0))
    else {
        return;
    };
    let Some(player) = PlayerEntity::get(world) else {
        return;
    };
    let Some((eye, _aim)) = camera_ray(world) else {
        return;
    };
    // No equipped weapon -> nothing to price (and no AP cost to charge):
    // the scan yields empty and the mode exits cleanly. Pricing a
    // weaponless part at `f32::MAX` AP once let one queued shot drive the
    // pool to -3.4e38 (#6422).
    let Some(weapon) = resolve_weapon(world, &index, player) else {
        return;
    };
    // A weapon that names no skill (`skill_form == 0`) is priced at the
    // neutral skill, not at the absent-AV default 0 — which would read as
    // "unskilled" and halve the chance. A skill the player does not carry
    // keeps the neutral value too (`ActorValues` absent altogether).
    let skill = if weapon.skill_form == 0 {
        NEUTRAL_SKILL
    } else {
        world
            .get::<ActorValues>(player)
            .map(|avs| avs.current(weapon.skill_form))
            .unwrap_or(NEUTRAL_SKILL)
    };
    let excluded_body = world
        .query::<byroredux_physics::RapierHandles>()
        .and_then(|handles| handles.get(player).map(|h| h.body));

    let mut targets = Vec::new();
    // #6444 — gather the candidates and drop the `GlobalTransform` guard
    // before pricing anything (the #5954 gather-then-filter pattern). Holding
    // it across the loop took `ActorVitals`, `RapierHandles`, the pools and a
    // second `GlobalTransform` read under it, closing the existing
    // `ActorVitals -> RapierHandles -> GlobalTransform` chain.
    let candidates: Vec<(EntityId, Vec3)> = {
        let Some(actors) = world.query::<GlobalTransform>() else {
            return;
        };
        actors.iter().map(|(entity, transform)| (entity, transform.translation)).collect()
    };
    // Built once per scan, not once per priced part.
    let bodies_by_target = target_bodies_by_owner(world);
    for (entity, translation) in candidates {
        // Actor-ness is the vitals marker; the form id reaches the
        // creature's PNAM limb-table link. Single-component queries +
        // `world.get` per the #4104 house pattern.
        if world.get::<ActorVitals>(entity).is_none() {
            continue;
        }
        let Some(form_id) = world.get::<FormIdComponent>(entity).map(|form| form.0) else {
            continue;
        };
        if entity == player || world.get::<Dead>(entity).is_some() {
            continue;
        }
        let center = translation + Vec3::Y * EYE_HEIGHT_BU;
        let center_distance = (center - eye).length();
        if center_distance > model.max_engage_distance {
            continue;
        }
        let is_creature = world.get::<CreatureActor>(entity).is_some();
        // The index maps key on the remapped global form id; the runtime
        // FormId resolves through the pool (`FormIdPool::local_of`).
        let global_form = world
            .try_resource::<byroredux_core::form_id::FormIdPool>()
            .and_then(|pool| pool.local_of(form_id));
        let body_form = if is_creature {
            global_form.and_then(|form| index.creatures.get(&form).and_then(|r| r.body_part_form))
        } else {
            None
        };
        let base_to_hit = weapon.base_to_hit;
        let ap_cost = weapon.ap_cost;
        match resolve_limb_table(&index, body_form, is_creature) {
            Some(parts) => {
                for (part_index, part) in parts.iter().enumerate() {
                    let position =
                        bone_position(world, entity, &part.vats_target_node).unwrap_or(center);
                    let distance = (position - eye).length();
                    if distance > model.max_engage_distance {
                        continue;
                    }
                    let hit_chance = model.to_hit_chance(VatsHitInput {
                        weapon_base_to_hit: base_to_hit,
                        part_to_hit: part.to_hit_chance as f32,
                        distance,
                        skill,
                        screen_percent: visible_fraction(
                            world, &bodies_by_target, eye, position, entity, excluded_body,
                        ),
                        // Always the first shot: the model's Concentrated Fire
                        // step is the *perk's* bonus ("accuracy gained by the
                        // perk", GECK iVATSConcentratedFireBonus), so a repeat
                        // count may only reach it once the player's perk is
                        // resolved — the perk-gated lane is still open (#6422).
                        concentrated_shot: 1,
                    });
                    targets.push(VatsTargetPart {
                        entity,
                        part_index,
                        part_name: part.name.clone(),
                        hit_chance,
                        ap_cost,
                    });
                }
            }
            None => {
                // Degraded: no resolvable limb table -> one implicit
                // whole-body part at the default profile.
                targets.push(VatsTargetPart {
                    entity,
                    part_index: usize::MAX,
                    part_name: "Body".to_string(),
                    hit_chance: model.to_hit_chance(VatsHitInput {
                        weapon_base_to_hit: base_to_hit,
                        part_to_hit: 100.0,
                        distance: center_distance,
                        skill,
                        screen_percent: visible_fraction(
                            world, &bodies_by_target, eye, center, entity, excluded_body,
                        ),
                        concentrated_shot: 1,
                    }),
                    ap_cost,
                });
            }
        }
    }
    if let Some(mut vats) = world.try_resource_mut::<VatsState>() {
        vats.targets = targets;
    }
}

/// A bone's world position through the actor's subtree name map. `None`
/// when the skeleton carries no such node — the part falls back to the
/// actor's centre.
fn bone_position(world: &World, actor: EntityId, node: &str) -> Option<Vec3> {
    let key = world
        .try_resource::<byroredux_core::string::StringPool>()
        .and_then(|pool| pool.get(node))?;
    let bone = world
        .try_resource::<crate::components::SubtreeCache>()
        .and_then(|subtrees| subtrees.map.get(&actor)?.get(&key).copied())?;
    world.get::<GlobalTransform>(bone).map(|t| t.translation)
}

/// Every actor-collider body, grouped by the actor it belongs to (#6444): the
/// bodies a ray may touch without counting as cover. One pass over the physics
/// handles per scan; the owner lookups run with the `RapierHandles` guard
/// already dropped.
fn target_bodies_by_owner(world: &World) -> HashMap<EntityId, HashSet<RigidBodyHandle>> {
    let bodies: Vec<(EntityId, RigidBodyHandle)> = world
        .query::<byroredux_physics::RapierHandles>()
        .map(|handles| handles.iter().map(|(owner, handles)| (owner, handles.body)).collect())
        .unwrap_or_default();
    let mut by_target: HashMap<EntityId, HashSet<RigidBodyHandle>> = HashMap::new();
    for (owner, body) in bodies {
        if let Some(target) = world
            .get::<byroredux_physics::ActorColliderOwner>(owner)
            .map(|owner| owner.0)
        {
            by_target.entry(target).or_default().insert(body);
        }
    }
    by_target
}

/// The fraction of a part's occlusion fan that reaches the target
/// unobstructed: partial cover degrades the chance instead of gating
/// it (the area-not-point rule; see `PART_FAN_OFFSETS`). A ray is
/// blocked when it stops short on a body that is not the target's own
/// (`bodies_by_target`, from [`target_bodies_by_owner`]).
fn visible_fraction(
    world: &World,
    bodies_by_target: &HashMap<EntityId, HashSet<RigidBodyHandle>>,
    eye: Vec3,
    target_point: Vec3,
    target: EntityId,
    excluded_body: Option<RigidBodyHandle>,
) -> f32 {
    let Some(physics) = world.try_resource::<byroredux_physics::PhysicsWorld>() else {
        return 1.0;
    };
    let empty = HashSet::new();
    let target_bodies = bodies_by_target.get(&target).unwrap_or(&empty);
    let mut visible = 0usize;
    for offset in PART_FAN_OFFSETS {
        let point = target_point + Vec3::Y * offset;
        let delta = point - eye;
        let dist = delta.length();
        let dir = delta.normalize_or_zero();
        let blocked = physics
            .cast_ray(eye, dir, dist, excluded_body)
            .is_some_and(|hit| {
                hit.distance < dist - 1.0
                    && !hit.body.is_some_and(|body| target_bodies.contains(&body))
            });
        if !blocked {
            visible += 1;
        }
    }
    visible as f32 / PART_FAN_OFFSETS.len() as f32
}

/// Why [`vats_queue_part`] declined a part.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum VatsQueueRefusal {
    /// The scan no longer lists that (entity, part).
    PartGone,
    /// The queue's total AP would exceed what the player has.
    NotEnoughAp { needed: f32, available: f32 },
    /// VATS is not active: nothing to queue into (#6469).
    NotInVats,
}

/// Whether any VATS mode is up.
pub(crate) fn vats_active(world: &World) -> bool {
    world
        .try_resource::<VatsState>()
        .is_some_and(|vats| vats.mode != VatsMode::None)
}

/// The player's ActionPoints AVIF and its current value, when the player
/// carries the pool. `None` on a load order that resolves no AP AVIF or a
/// player without the stamped pool — VATS then has no AP economy to
/// enforce (the degraded rule: a modded load order must never fail the
/// mode, it degrades).
fn player_ap(world: &World) -> Option<(u32, f32)> {
    let player = PlayerEntity::get(world)?;
    let avif = world
        .try_resource::<LoadedCellIndex>()
        .and_then(|index| index.0.actor_value_form_id("ActionPoints"))?;
    let avs = world.get::<ActorValues>(player)?;
    avs.get(avif).map(|_| (avif, avs.current(avif)))
}

/// Whether `needed` AP exceeds `available`. NaN counts as unaffordable.
fn exceeds_ap(needed: f32, available: f32) -> bool {
    needed.is_nan() || needed > available
}

/// Queue one part (by entity + part index). The AP reservation
/// accumulates; the spend happens on confirm. A part the player cannot
/// pay for on top of what is already reserved is refused (vanilla: the
/// queue ends where the AP does).
pub(crate) fn vats_queue_part(
    world: &World,
    entity: EntityId,
    part_index: usize,
) -> Result<(), VatsQueueRefusal> {
    if !vats_active(world) {
        return Err(VatsQueueRefusal::NotInVats);
    }
    let Some(part) = world
        .try_resource::<VatsState>()
        .and_then(|vats| {
            vats.targets
                .iter()
                .find(|t| t.entity == entity && t.part_index == part_index)
                .cloned()
        })
    else {
        return Err(VatsQueueRefusal::PartGone);
    };
    let reserved = world
        .try_resource::<VatsState>()
        .map_or(0.0, |vats| vats.ap_reserved);
    let needed = reserved + part.ap_cost;
    if let Some((_, available)) = player_ap(world) {
        if exceeds_ap(needed, available) {
            return Err(VatsQueueRefusal::NotEnoughAp { needed, available });
        }
    }
    if let Some(mut vats) = world.try_resource_mut::<VatsState>() {
        vats.ap_reserved = needed;
        vats.queue.push(part);
    }
    Ok(())
}

/// Confirm the queue: charge the reserved AP, roll and execute every
/// queued attack through the combat funnel, leave VATS. The die roll is
/// a session-deterministic LCG (`VatsState::roll_state`) so gates and
/// tests can rely on it.
pub(crate) fn vats_confirm(world: &World) -> bool {
    if !vats_active(world) {
        return false;
    }
    let (queue, reserved, mut roll_state) = world
        .try_resource::<VatsState>()
        .map(|vats| (vats.queue.clone(), vats.ap_reserved, vats.roll_state))
        .unwrap_or_default();
    if queue.is_empty() {
        return false;
    }
    let Some(player) = PlayerEntity::get(world) else {
        return false;
    };

    // Charge the reserved AP (vanilla charges the queue on confirm). The
    // queue was gated at queue time and the world is frozen while VATS is
    // up, but the pool is re-read here so a stale or hand-built
    // reservation can never overdraw it.
    let ap = player_ap(world);
    if let Some((_, available)) = ap {
        if exceeds_ap(reserved, available) {
            return false;
        }
    }
    if let (Some((ap_avif, _)), Some(mut avs_q)) = (ap, world.query_mut::<ActorValues>()) {
        if let Some(avs) = avs_q.get_mut(player) {
            avs.apply_damage(ap_avif, reserved.max(0.0));
        }
    }

    // Execute: die roll per shot, then the combat funnel. The damage is
    // the player's resolved attack damage × the weapon's authored VATS
    // damage multiplier; per-part routing (BPTD damage mult → limb
    // condition AVs) is the Phase-4 lane and the smoke gate says so.
    let weapon_mult = world
        .get::<EquippedWeapon>(player)
        .and_then(|weapon| {
            let index = world.try_resource::<LoadedCellIndex>()?;
            match &index.0.items.get(&weapon.base_form_id)?.kind {
                ItemKind::Weapon { vats, .. } => Some(
                    vats.filter(|v| v.damage_mult > 0.0)
                        .map(|v| v.damage_mult)
                        .unwrap_or(1.0),
                ),
                _ => None,
            }
        })
        .unwrap_or(1.0);
    // Roll every queued shot first (pure), tallying the landed ones per
    // target. `HitEvent` is one component per entity, so a second `insert`
    // for the same target would overwrite the first: shots at one target
    // fold into a single event carrying their summed damage (#6422). The
    // target's own defenses then apply once to that total in the combat
    // funnel — the Phase-4 per-part routing owns anything finer.
    let mut landed: Vec<(EntityId, u32)> = Vec::new();
    for part in &queue {
        roll_state = roll_state
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let roll = (roll_state >> 16) as f32 / 65_536.0 * 100.0;
        if roll >= part.hit_chance.clamp(0.0, 100.0) {
            continue;
        }
        match landed.iter_mut().find(|(entity, _)| *entity == part.entity) {
            Some((_, shots)) => *shots += 1,
            None => landed.push((part.entity, 1)),
        }
    }
    // Per-shot damage is priced against each target (the aggressor's
    // outgoing perk entry points compose per target), before the
    // `HitEvent` write lock is taken.
    let hits: Vec<(EntityId, f32)> = landed
        .into_iter()
        .map(|(target, shots)| {
            let shot = (crate::combat::attack_damage_against(world, player, target)
                * weapon_mult)
                .max(0.0);
            (target, shot * shots as f32)
        })
        .collect();
    let Some(mut events) = world.query_mut::<byroredux_scripting::HitEvent>() else {
        return false;
    };
    for (target, damage) in hits {
        events.insert(
            target,
            byroredux_scripting::HitEvent {
                aggressor: player,
                source: player,
                projectile: 0,
                damage,
                power_attack: false,
                sneak_attack: false,
                bash_attack: false,
                blocked: false,
            },
        );
    }
    drop(events);
    if let Some(mut vats) = world.try_resource_mut::<VatsState>() {
        vats.roll_state = roll_state;
        vats.exit_all();
    }
    true
}

/// Cancel: leave every mode without spending.
pub(crate) fn vats_cancel(world: &World) {
    if let Some(mut vats) = world.try_resource_mut::<VatsState>() {
        vats.exit_all();
    }
}

#[cfg(test)]
mod scan_tests {
    use super::*;
    use crate::components::VatsMode;
    use byroredux_core::ecs::components::ActorVitals;
    use byroredux_core::ecs::{EntityId, Transform};
    use byroredux_core::ecs::{ActiveCamera, TotalTime};
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_core::ecs::components::InventoryIndex;
    use byroredux_plugin::esm::records::{BptdPart, BptdRecord, GameSetting, SettingValue};

    /// A scan-ready world: player at the origin with an equipped 10mm
    /// (65 % base / 17 AP), one settler-class NPC 512 BU ahead wired to
    /// the DefaultBodyPartData slot, camera behind the player.
    fn scan_world() -> (World, EntityId) {
        let mut world = World::new();
        world.register::<ActorValues>();
        world.register::<ActorVitals>();
        world.register::<EquippedWeapon>();
        world.register::<Dead>();
        world.register::<FormIdComponent>();
        world.register::<GlobalTransform>();
        world.register::<byroredux_scripting::HitEvent>();
        world.insert_resource(VatsState {
            mode: VatsMode::TargetSelect,
            ..Default::default()
        });
        world.insert_resource(VatsModel::FALLOUT3);
        world.insert_resource(crate::interaction::ActionState::default());
        world.insert_resource(PlayerMode::Character);
        world.insert_resource(TotalTime(0.0));

        // Index: DefaultBodyPartData with a head + torso, the ActionPoints
        // AVIF, and the 10mm weapon record.
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.body_parts.insert(
            DEFAULT_BODY_PART_DATA,
            BptdRecord {
                form_id: DEFAULT_BODY_PART_DATA,
                editor_id: "DefaultBodyPartData".into(),
                skeleton_model: String::new(),
                parts: vec![
                    BptdPart {
                        name: "Head".into(),
                        node: "Bip01 Neck1".into(),
                        vats_target_node: "Bip01 Head".into(),
                        damage_mult: 2.0,
                        to_hit_chance: 20,
                        actor_value: 25,
                        ..Default::default()
                    },
                    BptdPart {
                        name: "Torso".into(),
                        node: "Bip01".into(),
                        vats_target_node: "Bip01 Spine2".into(),
                        damage_mult: 1.0,
                        to_hit_chance: 60,
                        actor_value: 26,
                        ..Default::default()
                    },
                ],
            },
        );
        index.actor_values.insert(
            0x40AA,
            byroredux_plugin::esm::records::AvifRecord {
                form_id: 0x40AA,
                editor_id: "ActionPoints".into(),
                ..Default::default()
            },
        );
        index.insert_game_setting(GameSetting {
            form_id: 0x1,
            editor_id: "fVATSMaxChance".into(),
            value: SettingValue::Float(95.0),
        });
        index.items.insert(
            0x0434F,
            byroredux_plugin::esm::records::ItemRecord {
                form_id: 0x0434F,
                common: byroredux_plugin::esm::records::common::CommonItemFields::default(),
                kind: ItemKind::weapon_stub(22, 12).with_weapon_vats_stats(17.0, 65.0, 0x40BB),
            },
        );
        world.insert_resource(crate::cell_loader::LoadedCellIndex(std::sync::Arc::new(
            index,
        )));

        // The player, camera, and one NPC ahead.
        let player = world.spawn();
        world.insert_resource(PlayerEntity(Some(player)));
        world.insert(player, ActorValues::from_pairs([]));
        world.insert(player, EquippedWeapon {
            inventory_index: InventoryIndex(0),
            base_form_id: 0x0434F,
            damage: 22.0,
            reach: 0.0,
            speed: 1.0,
        });
        let camera = world.spawn();
        world.insert(camera, Transform::from_translation(Vec3::new(0.0, 64.0, 0.0)));
        world.insert_resource(ActiveCamera(camera));

        // Pool-interned global form id for the NPC's record lookup.
        let mut pool = FormIdPool::default();
        let npc_form = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x0007A1),
        });
        world.insert_resource(pool);

        let npc = world.spawn();
        world.insert(
            npc,
            Transform::from_translation(Vec3::new(0.0, 0.0, -512.0)),
        );
        world.insert(npc, GlobalTransform {
            translation: Vec3::new(0.0, 0.0, -512.0),
            rotation: Default::default(),
            scale: 1.0,
        });
        world.insert(npc, ActorVitals { health: 0 });
        world.insert(npc, FormIdComponent(npc_form));
        (world, npc)
    }

    #[test]
    fn scan_prices_every_part_of_the_default_limb_table() {
        let (world, npc) = scan_world();
        vats_scan(&world);
        let vats = world.resource::<VatsState>();
        let parts: Vec<&VatsTargetPart> =
            vats.targets.iter().filter(|t| t.entity == npc).collect();
        assert_eq!(parts.len(), 2, "head + torso of the default table");
        let head = parts.iter().find(|p| p.part_name == "Head").unwrap();
        let torso = parts.iter().find(|p| p.part_name == "Torso").unwrap();
        // The authored bases: torso 60 > head 20 at any range, and both
        // under the neutral-conditions ceiling of 65 (weapon base).
        assert!(torso.hit_chance > head.hit_chance);
        assert!(torso.hit_chance <= 65.0 + 1e-3);
        assert!(head.hit_chance > 0.0, "a visible head is hittable");
        assert_eq!(head.ap_cost, 17.0, "the weapon's authored standard cost");
    }

    #[test]
    fn degraded_actor_without_a_limb_table_gets_one_implicit_part() {
        let (mut world, npc) = scan_world();
        // Empty the default slot: the actor degrades to one Body part.
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.items.insert(
            0x0434F,
            byroredux_plugin::esm::records::ItemRecord {
                form_id: 0x0434F,
                common: byroredux_plugin::esm::records::common::CommonItemFields::default(),
                kind: ItemKind::weapon_stub(22, 12).with_weapon_vats_stats(17.0, 65.0, 0),
            },
        );
        world.insert_resource(crate::cell_loader::LoadedCellIndex(std::sync::Arc::new(
            index,
        )));
        vats_scan(&world);
        let vats = world.resource::<VatsState>();
        let parts: Vec<&VatsTargetPart> =
            vats.targets.iter().filter(|t| t.entity == npc).collect();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].part_name, "Body");
    }

    /// Seed the player's AP pool (the tests' stand-in for the ruleset's
    /// stamped `ActionPoints` row).
    fn seed_ap(world: &World, base: f32) {
        let player = world.resource::<PlayerEntity>().0.unwrap();
        let mut avs = world.query_mut::<ActorValues>().unwrap();
        avs.get_mut(player).unwrap().set_base(0x40AA, base);
    }

    fn ap(world: &World) -> f32 {
        let player = world.resource::<PlayerEntity>().0.unwrap();
        world.get::<ActorValues>(player).unwrap().current(0x40AA)
    }

    fn torso(world: &World, npc: EntityId) -> VatsTargetPart {
        world
            .resource::<VatsState>()
            .targets
            .iter()
            .find(|t| t.entity == npc && t.part_name == "Torso")
            .cloned()
            .unwrap()
    }

    #[test]
    fn confirm_spends_reserved_ap_and_leaves_the_mode() {
        let (world, npc) = scan_world();
        vats_scan(&world);
        let torso = torso(&world, npc);
        assert_eq!(torso.ap_cost, 17.0);
        // 8 shots x 17 AP = 136 fits a 140 pool.
        seed_ap(&world, 140.0);
        for _ in 0..8 {
            assert_eq!(vats_queue_part(&world, npc, torso.part_index), Ok(()));
        }
        assert!(vats_confirm(&world));
        let vats = world.resource::<VatsState>();
        assert_eq!(vats.mode, VatsMode::None, "execution leaves VATS");
        assert!(vats.queue.is_empty());
        assert!(
            (ap(&world) - (140.0 - 8.0 * 17.0)).abs() < 1e-3,
            "8 queued shots at 17 AP: {}",
            ap(&world)
        );
    }

    /// #6422 — the queue ends where the AP does. 95 AP buys five 17-AP
    /// shots (85); the sixth (102) is refused with the figures, the
    /// reservation stays 85, and confirm charges exactly that.
    #[test]
    fn queue_refuses_a_part_the_pool_cannot_pay() {
        let (world, npc) = scan_world();
        vats_scan(&world);
        let torso = torso(&world, npc);
        seed_ap(&world, 95.0);
        for _ in 0..5 {
            assert_eq!(vats_queue_part(&world, npc, torso.part_index), Ok(()));
        }
        assert_eq!(
            vats_queue_part(&world, npc, torso.part_index),
            Err(VatsQueueRefusal::NotEnoughAp {
                needed: 102.0,
                available: 95.0
            })
        );
        let vats = world.resource::<VatsState>();
        assert_eq!(vats.queue.len(), 5, "the refused shot is not queued");
        assert!((vats.ap_reserved - 85.0).abs() < 1e-3);
        drop(vats);
        assert!(vats_confirm(&world));
        assert!((ap(&world) - 10.0).abs() < 1e-3, "{}", ap(&world));
    }

    /// Damage already spent (regen has not caught up) shrinks what can be
    /// queued: the gate reads `current`, not `base`.
    #[test]
    fn queue_gate_reads_the_current_pool_not_the_base() {
        let (world, npc) = scan_world();
        vats_scan(&world);
        let torso = torso(&world, npc);
        seed_ap(&world, 95.0);
        let player = world.resource::<PlayerEntity>().0.unwrap();
        world
            .query_mut::<ActorValues>()
            .unwrap()
            .get_mut(player)
            .unwrap()
            .apply_damage(0x40AA, 80.0);
        assert!(matches!(
            vats_queue_part(&world, npc, torso.part_index),
            Err(VatsQueueRefusal::NotEnoughAp { .. })
        ));
    }

    /// #6469 — with no VATS mode up there is nothing to queue into or confirm: a
    /// dead or absent player cannot land VATS hits from the console.
    #[test]
    fn queue_and_confirm_refuse_outside_vats() {
        let (world, npc) = scan_world();
        vats_scan(&world);
        let torso = torso(&world, npc);
        seed_ap(&world, 140.0);
        assert_eq!(vats_queue_part(&world, npc, torso.part_index), Ok(()));
        world.resource_mut::<VatsState>().exit_all();
        // `exit_all` cleared the queue; refill it by hand as a stale reservation would.
        {
            let mut vats = world.resource_mut::<VatsState>();
            vats.queue.push(torso.clone());
            vats.ap_reserved = 17.0;
        }
        assert_eq!(
            vats_queue_part(&world, npc, torso.part_index),
            Err(VatsQueueRefusal::NotInVats)
        );
        assert!(!vats_confirm(&world), "no mode, no execution");
        assert_eq!(ap(&world), 140.0, "and no AP was spent");
        assert!(world.get::<byroredux_scripting::HitEvent>(npc).is_none());
    }

    /// A confirm whose reservation exceeds the pool (hand-built or stale)
    /// refuses and leaves both the pool and the queue untouched.
    #[test]
    fn confirm_refuses_an_overdrawn_reservation() {
        let (world, npc) = scan_world();
        vats_scan(&world);
        let torso = torso(&world, npc);
        seed_ap(&world, 10.0);
        {
            let mut vats = world.resource_mut::<VatsState>();
            vats.queue.push(torso);
            vats.ap_reserved = 17.0;
        }
        assert!(!vats_confirm(&world));
        assert_eq!(ap(&world), 10.0);
        assert_eq!(world.resource::<VatsState>().queue.len(), 1);
    }

    /// #6422 — no equipped weapon: the scan yields nothing (it used to
    /// price every part at `f32::MAX` AP).
    #[test]
    fn a_weaponless_scan_yields_no_targets() {
        let (mut world, _npc) = scan_world();
        let player = world.resource::<PlayerEntity>().0.unwrap();
        world.remove::<EquippedWeapon>(player);
        vats_scan(&world);
        assert!(world.resource::<VatsState>().targets.is_empty());
    }

    /// #6422 — `HitEvent` is one component per entity, so several landed
    /// shots at one target must fold into a single event with the summed
    /// damage rather than overwrite each other.
    #[test]
    fn landed_shots_at_one_target_sum_into_one_hit_event() {
        let (world, npc) = scan_world();
        vats_scan(&world);
        let mut sure = torso(&world, npc);
        sure.hit_chance = 100.0; // a roll is always < 100
        seed_ap(&world, 95.0);
        {
            let mut vats = world.resource_mut::<VatsState>();
            for _ in 0..3 {
                vats.queue.push(sure.clone());
            }
            vats.ap_reserved = 3.0 * sure.ap_cost;
        }
        let player = world.resource::<PlayerEntity>().0.unwrap();
        let one_shot = crate::combat::attack_damage_against(&world, player, npc);
        assert!(one_shot > 0.0, "fixture must deal damage");
        assert!(vats_confirm(&world));
        let hit = *world
            .get::<byroredux_scripting::HitEvent>(npc)
            .expect("the landed shots produce a HitEvent");
        assert!(
            (hit.damage - 3.0 * one_shot).abs() < 1e-3,
            "3 landed shots: {} vs 3 x {one_shot}",
            hit.damage
        );
    }
}
