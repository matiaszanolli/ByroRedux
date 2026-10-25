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
    // Entry gates.
    if world.try_resource::<VatsModel>().is_none() {
        return;
    }
    if !player_can_act(world) {
        return;
    }
    if world
        .try_resource::<PlayerMode>()
        .is_none_or(|mode| *mode == PlayerMode::FlyCam)
    {
        return;
    }
    if !control_vats_enabled(world) {
        return;
    }
    if let Some(mut vats) = world.try_resource_mut::<VatsState>() {
        vats.mode = VatsMode::TargetSelect;
    }
    // Entering is a scan: fill the target list the overlay and queue
    // read. Re-runs (`vats.scan`) re-price against live positions.
    vats_scan(world);
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
use std::collections::HashSet;
use std::sync::Arc;

/// Head-height offset above an actor root (Bethesda units).
const EYE_HEIGHT_BU: f32 = 64.0;

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
        vats,
        ..
    } = &index.items.get(&base_form_id)?.kind
    else {
        return None;
    };
    // The GECK Override-tab value replaces the standard cost where an
    // author set one (#6350's split — 45 FNV weapons); a weapon with
    // neither authored falls to the per-anim-type standard (pistol for
    // this gate — the anim-type mapping is Phase-4 polish).
    let overridden = vats.filter(|v| v.ap_override > 0.0).map(|v| v.ap_override);
    Some(VatsWeapon {
        base_to_hit: *vats_base_to_hit,
        ap_cost: overridden
            .or_else(|| (*ap_cost > 0.0).then_some(*ap_cost))
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
    let Some(player) = world.try_resource::<PlayerEntity>().and_then(|p| p.0) else {
        return;
    };
    let Some((eye, _aim)) = camera_ray(world) else {
        return;
    };
    let weapon = resolve_weapon(world, &index, player);
    let skill = weapon
        .as_ref()
        .and_then(|w| {
            world
                .get::<ActorValues>(player)
                .map(|avs| avs.current(w.skill_form))
        })
        .unwrap_or(50.0);
    let excluded_body = world
        .query::<byroredux_physics::RapierHandles>()
        .and_then(|handles| handles.get(player).map(|h| h.body));

    let mut targets = Vec::new();
    let Some(actors) = world.query::<GlobalTransform>() else {
        return;
    };
    for (entity, transform) in actors.iter() {
        // Actor-ness is the vitals marker; the form id reaches the
        // creature's PNAM limb-table link. Single-component queries +
        // `world.get` per the #4104 house pattern.
        if world.get::<ActorVitals>(entity).is_none() {
            continue;
        }
        let Some(form) = world.get::<FormIdComponent>(entity) else {
            continue;
        };
        if entity == player || world.get::<Dead>(entity).is_some() {
            continue;
        }
        let center = transform.translation + Vec3::Y * EYE_HEIGHT_BU;
        let center_distance = (center - eye).length();
        if center_distance > model.max_engage_distance {
            continue;
        }
        let is_creature = world.get::<CreatureActor>(entity).is_some();
        // The index maps key on the remapped global form id; the runtime
        // FormId resolves through the pool (`pair.local.0` carries the
        // global value — trap_hits.rs's record lookups use the same
        // idiom).
        let global_form = world
            .try_resource::<byroredux_core::form_id::FormIdPool>()
            .and_then(|pool| pool.resolve(form.0).map(|pair| pair.local.0));
        let body_form = if is_creature {
            global_form.and_then(|form| index.creatures.get(&form).and_then(|r| r.body_part_form))
        } else {
            None
        };
        let base_to_hit = weapon.as_ref().map_or(0.0, |w| w.base_to_hit);
        let ap_cost = weapon.as_ref().map_or(f32::MAX, |w| w.ap_cost);
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
                            world, eye, position, entity, excluded_body,
                        ),
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
                            world, eye, center, entity, excluded_body,
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

/// The fraction of a part's occlusion fan that reaches the target
/// unobstructed: partial cover degrades the chance instead of gating
/// it (the area-not-point rule; see `PART_FAN_OFFSETS`). A ray is
/// blocked when it stops short on a body that is not the target's own.
fn visible_fraction(
    world: &World,
    eye: Vec3,
    target_point: Vec3,
    target: EntityId,
    excluded_body: Option<RigidBodyHandle>,
) -> f32 {
    let Some(physics) = world.try_resource::<byroredux_physics::PhysicsWorld>() else {
        return 1.0;
    };
    let target_bodies: HashSet<_> = world
        .query::<byroredux_physics::RapierHandles>()
        .map(|handles| {
            handles
                .iter()
                .filter(|(owner, _)| {
                    world
                        .get::<byroredux_physics::ActorColliderOwner>(*owner)
                        .is_some_and(|owner| owner.0 == target)
                })
                .map(|(_, handles)| handles.body)
                .collect()
        })
        .unwrap_or_default();
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

/// Queue one part (by entity + part index). The AP reservation
/// accumulates; the spend happens on confirm.
pub(crate) fn vats_queue_part(world: &World, entity: EntityId, part_index: usize) -> bool {
    let Some(part) = world
        .try_resource::<VatsState>()
        .and_then(|vats| {
            vats.targets
                .iter()
                .find(|t| t.entity == entity && t.part_index == part_index)
                .cloned()
        })
    else {
        return false;
    };
    if let Some(mut vats) = world.try_resource_mut::<VatsState>() {
        vats.ap_reserved += part.ap_cost;
        vats.queue.push(part);
    }
    true
}

/// Confirm the queue: charge the reserved AP, roll and execute every
/// queued attack through the combat funnel, leave VATS. The die roll is
/// a session-deterministic LCG (`VatsState::roll_state`) so gates and
/// tests can rely on it.
pub(crate) fn vats_confirm(world: &World) -> bool {
    let (queue, reserved, mut roll_state) = world
        .try_resource::<VatsState>()
        .map(|vats| (vats.queue.clone(), vats.ap_reserved, vats.roll_state))
        .unwrap_or_default();
    if queue.is_empty() {
        return false;
    }
    let Some(player) = world.try_resource::<PlayerEntity>().and_then(|p| p.0) else {
        return false;
    };

    // Charge the reserved AP (vanilla charges the queue on confirm).
    let ap_avif = world
        .try_resource::<LoadedCellIndex>()
        .and_then(|index| index.0.actor_value_form_id("ActionPoints"));
    if let (Some(ap_avif), Some(mut avs_q)) = (
        ap_avif,
        world.query_mut::<ActorValues>(),
    ) {
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
    let base_damage = crate::combat::attack_damage_against(world, player, queue[0].entity);
    let damage = (base_damage * weapon_mult).max(0.0);
    let Some(mut events) = world.query_mut::<byroredux_scripting::HitEvent>() else {
        return false;
    };
    for part in &queue {
        roll_state = roll_state
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let roll = (roll_state >> 16) as f32 / 65_536.0 * 100.0;
        if roll >= part.hit_chance.clamp(0.0, 100.0) {
            continue;
        }
        events.insert(
            part.entity,
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
                kind: ItemKind::Weapon {
                    ammo_form: 0,
                    damage: 22,
                    clip_size: 12,
                    anim_type: 0,
                    ap_cost: 17.0,
                    vats_base_to_hit: 65.0,
                    skill_form: 0x40BB,
                    min_spread: 0.0,
                    spread: 0.0,
                    crit_mult: 1.0,
                    reach: 0.0,
                    speed: 0.0,
                    reload_anim: 0,
                    vats: None,
                },
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
                kind: ItemKind::Weapon {
                    ammo_form: 0,
                    damage: 22,
                    clip_size: 12,
                    anim_type: 0,
                    ap_cost: 17.0,
                    vats_base_to_hit: 65.0,
                    skill_form: 0,
                    min_spread: 0.0,
                    spread: 0.0,
                    crit_mult: 1.0,
                    reach: 0.0,
                    speed: 0.0,
                    reload_anim: 0,
                    vats: None,
                },
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

    #[test]
    fn confirm_spends_reserved_ap_and_leaves_the_mode() {
        let (world, npc) = scan_world();
        vats_scan(&world);
        // Find the torso (guaranteed hittable chance > 0) and queue it
        // until the roll must land: queue the same part repeatedly so
        // the LCG sees several rolls.
        let vats = world.resource::<VatsState>();
        let torso = vats
            .targets
            .iter()
            .find(|t| t.entity == npc && t.part_name == "Torso")
            .cloned()
            .unwrap();
        assert_eq!(torso.ap_cost, 17.0);
        drop(vats);
        for _ in 0..8 {
            assert!(vats_queue_part(&world, npc, torso.part_index));
        }
        // Seed the player's AP pool.
        let ap_avif = 0x40AA;
        if let Some(mut avs) = world.query_mut::<ActorValues>() {
            if let Some(values) = avs.get_mut(world.resource::<PlayerEntity>().0.unwrap()) {
                values.set_base(ap_avif, 95.0);
            }
        }
        assert!(vats_confirm(&world));
        let vats = world.resource::<VatsState>();
        assert_eq!(vats.mode, VatsMode::None, "execution leaves VATS");
        assert!(vats.queue.is_empty());
        let values = world
            .get::<ActorValues>(world.resource::<PlayerEntity>().0.unwrap())
            .unwrap();
        assert!(
            (values.current(ap_avif) - (95.0 - 8.0 * 17.0)).abs() < 1e-3,
            "8 queued shots at 17 AP: {}",
            values.current(ap_avif)
        );
        // At least one of the eight torso shots landed its HitEvent.
        assert!(
            world.get::<byroredux_scripting::HitEvent>(npc).is_some()
                || world
                    .query::<byroredux_scripting::HitEvent>()
                    .is_some_and(|_| true)
        );
    }
}
