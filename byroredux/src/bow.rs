//! The player's bow (player-archery PRD, `docs/prd/player-archery.md`; Phase 1, #7055): the equipped arrow, the draw,
//! the release and the spend, one system the key and the console's `bow.shoot` share (BOW-DRW-005, BOW-DX-002).
//!
//! The per-game numbers are the `BowModel` row's (BOW-MOD-001): this module asks it and holds no game constant of
//! its own except the two it labels provisional ([`SHOT_INTERVAL_SECONDS`], [`OBLIVION_ARROW_RANGE_BU`]), which no
//! source documents (BOW-DRW-004, BOW-FLT-003). The arrow rides the shared projectile flight
//! ([`crate::systems::projectiles`]) and its hit takes the shared damage path (BOW-G-004).
//!
//! **Phase 1 scope.** The arrow's damage is the bow's base damage plus the arrow's, as the gun's shot is; the
//! Oblivion rating and the sneak multiplier are Phase 2 (BOW-DMG), and the Marksman feed is Phase 3 (BOW-USE).

use byroredux_core::character::{ArrowTrajectory, BowModel};
use byroredux_core::ecs::components::{EquippedAmmo, EquippedWeapon, Inventory};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::settings::{SettingChoice, SettingEntry, SettingValue, SettingsError, SettingsRegistry};
use byroredux_plugin::esm::records::{EsmIndex, ItemKind};
use std::fmt;

use crate::interaction::{ActionState, InputAction};
use crate::systems::projectiles::{arrow_shot, ActiveProjectiles, ArrowSpec};
use crate::systems::{player_can_act, PlayerEntity, PlayerMode};

/// The interval between two shots until a bow animation exists: the melee cooldown (BOW-DRW-004). Provisional, and
/// labelled so in `bow.status`: Oblivion ties the cadence to the bow's speed without a number, and Skyrim's rest
/// stage is about 0.6 s (observational).
pub(crate) const SHOT_INTERVAL_SECONDS: f32 = crate::combat::MELEE_COOLDOWN_SECONDS;

/// An Oblivion arrow's range in world units (BOW-FLT-003). Provisional: no source documents one. The figure is the
/// cap Skyrim's documented arrows carry (`fVisibleNavmeshMoveDist`), the only arrow range in the lineage, so the
/// two games' arrows end alike until Oblivion's is documented.
pub(crate) const OBLIVION_ARROW_RANGE_BU: f32 = 4096.0;

/// The registry key of the trajectory setting (BOW-SET-001).
pub(crate) const TRAJECTORY_SETTING_ID: &str = "gameplay.arrow_trajectory";

/// Register the trajectory setting. Both values are listed; ballistic is labelled unavailable and resolves to
/// straight while no documented gravity rule exists (BOW-FLT-004). The default is seeded per game by
/// [`install_bow_model`], after the game is known and before the stored values load.
pub(crate) fn register_settings(registry: &mut SettingsRegistry) -> Result<(), SettingsError> {
    registry.register(SettingEntry::choice(
        TRAJECTORY_SETTING_ID,
        "Gameplay",
        "Arrow trajectory",
        "Straight arrows fly where the crosshair points; ballistic arrows fall under gravity, as the original games' do.",
        ArrowTrajectory::Straight.id(),
        vec![
            SettingChoice::new(ArrowTrajectory::Straight.id(), "Straight"),
            SettingChoice::new(ArrowTrajectory::Ballistic.id(), "Ballistic (unavailable: flies straight)"),
        ],
    ))
}

/// The trajectory the player chose and the one that flies: ballistic resolves to straight until the gravity rule
/// exists, and the report says so (BOW-SET-001). Straight when the setting is missing.
pub(crate) fn trajectory(world: &World) -> (ArrowTrajectory, ArrowTrajectory) {
    let chosen = world
        .try_resource::<SettingsRegistry>()
        .and_then(|registry| match &registry.get(TRAJECTORY_SETTING_ID)?.value {
            SettingValue::Choice(id) => ArrowTrajectory::from_id(id),
            _ => None,
        })
        .unwrap_or_default();
    (chosen, chosen.effective())
}

/// Install (or clear) the [`BowModel`] for a freshly loaded index and seed the trajectory setting's default from
/// it: the profile's `bow` row decides whether the game has a player bow at all (Oblivion, Skyrim), the load
/// order's settings retune it.
pub(crate) fn install_bow_model(world: &mut World, index: &EsmIndex) {
    match index.character_rules.bow() {
        Some(model) => {
            let model = model.with_gmst(|name| index.game_setting_float(name));
            if let Some(mut registry) = world.try_resource_mut::<SettingsRegistry>()
                && let Err(error) = registry.set_default(TRAJECTORY_SETTING_ID, SettingValue::Choice(model.default_trajectory.id().to_owned()))
            {
                log::warn!("could not seed the arrow trajectory default: {error}");
            }
            world.insert_resource(model);
        }
        None => {
            world.remove_resource::<BowModel>();
        }
    }
}

/// The bow in the player's hand: the weapon's form and the two record numbers the draw and the damage read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WieldedBow {
    pub weapon: u32,
    pub damage: f32,
    /// The bow's authored speed; 1 where the record has none.
    pub speed: f32,
}

/// The bow `player` wields, or `None` where the game has no bow row, nothing is wielded or the weapon is not one.
pub(crate) fn wielded_bow(world: &World, player: EntityId) -> Option<WieldedBow> {
    let model = *world.try_resource::<BowModel>()?;
    let weapon = world.get::<EquippedWeapon>(player)?.base_form_id;
    let index = world.try_resource::<crate::components::LoadedCellIndex>()?;
    match &index.0.items.get(&weapon)?.kind {
        ItemKind::Weapon { damage, speed, oblivion_type, anim_type, .. } if model.is_bow(*oblivion_type, *anim_type) => {
            Some(WieldedBow { weapon, damage: *damage as f32, speed: if *speed > 0.0 { *speed } else { 1.0 } })
        }
        _ => None,
    }
}

/// The arrow type `player` has equipped and still carries: an equipped form with no counted stack reads as none
/// (BOW-AMM-001, BOW-AMM-002), so a spent, dropped or removed arrow clears the slot without a hook on each path.
pub(crate) fn equipped_ammo(world: &World, player: EntityId) -> Option<u32> {
    let form = world.get::<EquippedAmmo>(player)?.0?;
    let carried = world.get::<Inventory>(player).is_some_and(|inventory| inventory.count_of(form) > 0);
    carried.then_some(form)
}

/// Write the equipped arrow slot (`None` clears it). The row is written, never removed, so the additive save overlay
/// carries a clear (BOW-AMM-002).
pub(crate) fn set_equipped_ammo(world: &World, player: EntityId, ammo: Option<u32>) {
    if let Some(mut slots) = world.query_mut::<EquippedAmmo>() {
        slots.insert(player, EquippedAmmo(ammo));
    }
}

/// Clear an equipped arrow type the player no longer carries (BOW-AMM-001): the sweep the system runs every frame
/// and a load runs once, so the slot never names an arrow that is gone.
pub(crate) fn reconcile_equipped_ammo(world: &World, player: EntityId) {
    let named = world.get::<EquippedAmmo>(player).and_then(|slot| slot.0);
    if named.is_some() && equipped_ammo(world, player).is_none() {
        set_equipped_ammo(world, player, None);
    }
}

/// Why a shot did not leave (BOW-DX-001, BOW-DX-003): the one wording the log and `bow.status` share.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Refusal {
    /// Dead, in VATS or out of character mode.
    CannotAct,
    /// No bow in hand (or the game has no bow row).
    NoBow,
    /// No arrow equipped, or none left.
    NoArrows,
    /// Seconds until the next shot may leave.
    Cooldown(f32),
    /// Skyrim's release before the draw was complete (Q4): nothing fires and the arrow stays nocked.
    DrawIncomplete { held: f32, needed: f32 },
    /// No camera to aim along.
    NoAim,
    /// The arrow could not be made (no speed, no aim).
    Failed(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CannotAct => write!(f, "cannot act"),
            Self::NoBow => write!(f, "no bow"),
            Self::NoArrows => write!(f, "no arrows"),
            Self::Cooldown(left) => write!(f, "cooldown {left:.2}s"),
            Self::DrawIncomplete { held, needed } => write!(f, "draw incomplete ({held:.2}s of {needed:.2}s)"),
            Self::NoAim => write!(f, "no aim"),
            Self::Failed(why) => write!(f, "the arrow could not be made: {why}"),
        }
    }
}

/// A shot that left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Released {
    pub weapon: u32,
    pub arrow: u32,
    /// Seconds the draw was held.
    pub held: f32,
    pub draw_fraction: f32,
    pub speed: f32,
    pub range: f32,
}

impl fmt::Display for Released {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "released {:08X} with {:08X}: held {:.2}s, draw {:.2}, speed {:.0}, range {:.0}",
            self.weapon, self.arrow, self.held, self.draw_fraction, self.speed, self.range
        )
    }
}

/// The outcome of one release.
pub(crate) type BowReport = Result<Released, Refusal>;

/// The bow's session state: the draw in progress, the interval, and the last report for `bow.status`. Never saved
/// (a load starts with no draw in flight).
#[derive(Debug, Default)]
pub(crate) struct BowState {
    /// Seconds the attack action has been held with a bow in hand; 0 when not drawing.
    pub held: f32,
    /// Seconds until the next shot may leave.
    pub cooldown: f32,
    pub last: Option<BowReport>,
    /// Shots that left.
    pub shots: u32,
}

impl Resource for BowState {}

/// The aim of the player's shot: the camera's ray, its direction tilted up by the model's angle for the view in use
/// (BOW-FLT-002). The first-person view is the one with no camera boom.
fn aim(world: &World, model: &BowModel) -> Option<(Vec3, Vec3)> {
    let (origin, direction) = crate::interaction::camera_ray(world)?;
    let first_person = crate::systems::player_camera_boom(world) <= 0.0;
    let tilt = model.tilt_radians(first_person);
    if tilt == 0.0 {
        return Some((origin, direction));
    }
    let right = direction.cross(Vec3::Y).normalize_or_zero();
    if right == Vec3::ZERO {
        return Some((origin, direction));
    }
    Some((origin, Quat::from_axis_angle(right, tilt) * direction))
}

/// One release of the player's bow after a draw of `held` seconds: the gate, the spend and the arrow, the key's and
/// `bow.shoot`'s common path (BOW-DX-002). Gates in order: the player can act, a bow is wielded, an arrow is equipped
/// and carried, the interval has passed, and the game's draw rule lets this draw go (Skyrim's early release fires
/// nothing, Q4). A refusal spends nothing.
pub(crate) fn shoot_now(world: &World, held: f32) -> BowReport {
    let report = release(world, held);
    match &report {
        Ok(shot) => log::debug!("bow: {shot}"),
        Err(refusal) => log::info!("bow: shot refused: {refusal}"),
    }
    if let Some(mut state) = world.try_resource_mut::<BowState>() {
        if report.is_ok() {
            state.cooldown = SHOT_INTERVAL_SECONDS;
            state.shots = state.shots.saturating_add(1);
        }
        state.last = Some(report.clone());
    }
    report
}

fn release(world: &World, held: f32) -> BowReport {
    let player = PlayerEntity::get(world).ok_or(Refusal::CannotAct)?;
    let in_character = world.try_resource::<PlayerMode>().is_some_and(|mode| *mode == PlayerMode::Character);
    if !in_character || !player_can_act(world) {
        return Err(Refusal::CannotAct);
    }
    let model = *world.try_resource::<BowModel>().ok_or(Refusal::NoBow)?;
    let bow = wielded_bow(world, player).ok_or(Refusal::NoBow)?;
    let arrow = equipped_ammo(world, player).ok_or(Refusal::NoArrows)?;
    let cooldown = world.try_resource::<BowState>().map_or(0.0, |state| state.cooldown);
    if cooldown > 0.0 {
        return Err(Refusal::Cooldown(cooldown));
    }
    let fraction = model
        .draw_fraction(held, bow.speed)
        .ok_or(Refusal::DrawIncomplete { held, needed: model.full_draw_seconds(bow.speed) })?;
    let (origin, direction) = aim(world, &model).ok_or(Refusal::NoAim)?;
    let spec = arrow_spec(world, &model, &bow, arrow, fraction)?;
    let shot = arrow_shot(world, player, spec, origin, direction).ok_or_else(|| Refusal::Failed("no aim or no speed".into()))?;
    // The spend is at the release (BOW-AMM-003: the trigger is a declared gap). The arrow is spent only once it is made.
    byroredux_scripting::fragment::remove_items(world, player, arrow, 1, None);
    if let Some(mut active) = world.try_resource_mut::<ActiveProjectiles>() {
        active.0.push(shot);
    }
    reconcile_equipped_ammo(world, player);
    Ok(Released { weapon: bow.weapon, arrow, held, draw_fraction: fraction, speed: spec.speed, range: spec.range })
}

/// What the arrow is made of: the model's speed and range over the arrow's and the projectile's records, and the
/// damage the shared path will deal (Phase 1: the bow's base damage plus the arrow's).
fn arrow_spec(world: &World, model: &BowModel, bow: &WieldedBow, arrow: u32, fraction: f32) -> Result<ArrowSpec, Refusal> {
    let index = world
        .try_resource::<crate::components::LoadedCellIndex>()
        .map(|index| index.0.clone())
        .ok_or_else(|| Refusal::Failed("no loaded index".into()))?;
    let (ammo_speed, ammo_damage, projectile_form) = match index.items.get(&arrow).map(|item| &item.kind) {
        Some(ItemKind::Ammo { speed, damage, projectile_form, .. }) => (*speed, *damage, *projectile_form),
        _ => return Err(Refusal::Failed(format!("{arrow:08X} is not ammunition"))),
    };
    // Skyrim's arrow names a `PROJ` (speed, range, radius, lifetime); Oblivion has none. The `PROJ` gravity field is not
    // read: a straight arrow ignores it (BOW-FLT-004).
    let proj = index.projectiles.get(&projectile_form);
    let speed = model.arrow_speed(ammo_speed, proj.map_or(0.0, |proj| proj.muzzle_speed), fraction);
    let range = model.range(proj.map_or(0.0, |proj| proj.range));
    Ok(ArrowSpec {
        damage: bow.damage + ammo_damage.max(0.0),
        speed,
        range: if range.is_finite() { range } else { OBLIVION_ARROW_RANGE_BU },
        radius: proj.map_or(0.0, |proj| proj.collision_radius),
        lifetime: proj.and_then(|proj| (proj.lifetime > 0.0).then_some(proj.lifetime)),
    })
}

/// Whether the player's attack is a bow's: a bow in hand never swings as a melee weapon (BOW-DRW-001), so the melee
/// input path asks this before it arms a swing.
pub(crate) fn bow_in_hand(world: &World) -> bool {
    PlayerEntity::get(world).is_some_and(|player| wielded_bow(world, player).is_some())
}

/// The draw and the release (BOW-DRW-001, BOW-DRW-005): holding the attack action with a bow in hand draws, and its
/// release edge fires. The one owner of the draw, the spend and the interval; it never touches the player's
/// `ActorValues` from the input path.
pub(crate) fn player_bow_system(world: &World, dt: f32) {
    let Some(player) = PlayerEntity::get(world) else {
        return;
    };
    let (held_now, released) = world
        .try_resource::<ActionState>()
        .map(|actions| (actions.is_held(InputAction::Attack), actions.was_released(InputAction::Attack)))
        .unwrap_or((false, false));
    reconcile_equipped_ammo(world, player);
    let bow = bow_in_hand(world);
    let drawn = {
        let Some(mut state) = world.try_resource_mut::<BowState>() else {
            return;
        };
        state.cooldown = (state.cooldown - dt.max(0.0)).max(0.0);
        if !bow {
            state.held = 0.0;
            return;
        }
        if held_now {
            state.held += dt.max(0.0);
        }
        let drawn = state.held;
        if released || !held_now {
            state.held = 0.0;
        }
        drawn
    };
    if released && drawn > 0.0 {
        // The report is kept in `BowState` for `bow.status`; the key has nothing to do with a refusal but log it.
        let _report = shoot_now(world, drawn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{InputState, LoadedCellIndex};
    use crate::interaction::ActionBindings;
    use byroredux_core::ecs::components::{Dead, ItemStack};
    use byroredux_core::ecs::{ActiveCamera, Transform};
    use byroredux_plugin::esm::records::{common::CommonItemFields, ItemRecord, ProjRecord};
    use std::sync::Arc;
    use winit::event::MouseButton;

    const BOW: u32 = 0x25231;
    const ARROW: u32 = 0x17829;
    const OTHER_ARROW: u32 = 0x1782A;

    fn weapon(damage: u32, weapon_type: u8, anim_type: u8, speed: f32) -> ItemKind {
        let mut kind = ItemKind::weapon_stub(damage, 0).with_weapon_speed(speed);
        if let ItemKind::Weapon { oblivion_type, anim_type: anim, .. } = &mut kind {
            *oblivion_type = weapon_type;
            *anim = anim_type;
        }
        kind
    }

    fn arrow(damage: f32, speed: f32, projectile_form: u32) -> ItemKind {
        ItemKind::Ammo { damage, dt_mult: 1.0, spread: 0.0, projectile_form, casing_form: 0, health: 0, clip_rounds: 0, speed, flags: 0 }
    }

    fn item(kind: ItemKind) -> ItemRecord {
        ItemRecord { form_id: 0, common: CommonItemFields::default(), kind }
    }

    /// An Oblivion player in character mode with a bow in hand and ten arrows equipped, a camera looking down -Z,
    /// and the records of `Oblivion.esm`'s iron bow (damage 8) and iron arrow (damage 8, speed 1.0).
    fn world(model: BowModel, bow: ItemKind, arrows: ItemKind) -> (World, EntityId) {
        world_with(model, bow, arrows, None)
    }

    fn world_with(model: BowModel, bow: ItemKind, arrows: ItemKind, projectile: Option<ProjRecord>) -> (World, EntityId) {
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.items.insert(BOW, item(bow));
        index.items.insert(ARROW, item(arrows));
        index.items.insert(OTHER_ARROW, item(arrow(2.0, 1.0, 0)));
        if let Some(projectile) = projectile {
            index.projectiles.insert(projectile.form_id, projectile);
        }
        let mut world = World::new();
        world.register::<EquippedWeapon>();
        world.register::<EquippedAmmo>();
        world.register::<Inventory>();
        world.register::<Dead>();
        world.insert_resource(InputState::default());
        world.insert_resource(ActionBindings::default());
        world.insert_resource(ActionState::default());
        world.insert_resource(PlayerMode::Character);
        world.insert_resource(model);
        world.insert_resource(BowState::default());
        world.insert_resource(ActiveProjectiles::default());
        world.insert_resource(LoadedCellIndex(Arc::new(index)));
        let camera = world.spawn();
        world.insert(camera, Transform::IDENTITY);
        world.insert_resource(ActiveCamera(camera));
        let player = world.spawn();
        let mut inventory = Inventory::new();
        let bow_row = inventory.add(ItemStack { base_form_id: BOW, count: 1, instance: None });
        inventory.add(ItemStack { base_form_id: ARROW, count: 10, instance: None });
        inventory.add(ItemStack { base_form_id: OTHER_ARROW, count: 3, instance: None });
        world.insert(player, inventory);
        world.insert(player, EquippedWeapon { inventory_index: bow_row, base_form_id: BOW, damage: 8.0, reach: 0.0, speed: 1.0 });
        world.insert(player, EquippedAmmo(Some(ARROW)));
        world.insert_resource(PlayerEntity(Some(player)));
        (world, player)
    }

    fn oblivion() -> (World, EntityId) {
        world(BowModel::OBLIVION, weapon(8, 5, 0, 1.0), arrow(8.0, 1.0, 0))
    }

    fn arrows(world: &World, player: EntityId, form: u32) -> u32 {
        world.get::<Inventory>(player).unwrap().count_of(form)
    }

    /// BOW-DRW-001, BOW-AMM-003, BOW-FLT-001: a release spends exactly one arrow of the equipped type and puts one arrow
    /// in flight along the aim at the draw's speed (`a_shot_spends_one_arrow`).
    #[test]
    fn a_shot_spends_one_arrow() {
        let (world, player) = oblivion();
        let shot = shoot_now(&world, 1.875).expect("a full draw fires");
        assert_eq!(arrows(&world, player, ARROW), 9);
        assert_eq!(arrows(&world, player, OTHER_ARROW), 3, "only the equipped type is spent");
        assert_eq!((shot.draw_fraction, shot.speed, shot.range), (1.0, 1500.0, OBLIVION_ARROW_RANGE_BU));
        let active = world.resource::<ActiveProjectiles>();
        assert_eq!(active.0.len(), 1);
        let arrow = &active.0[0];
        assert_eq!(arrow.shooter, player);
        assert_eq!(arrow.damage, 16.0, "bow 8 + arrow 8 (Phase 1; the rating is Phase 2)");
        assert!((arrow.velocity - byroredux_core::math::Vec3::new(0.0, 0.0, -1500.0)).length() < 1e-2, "{:?}", arrow.velocity);
        assert_eq!(world.resource::<BowState>().shots, 1);
    }

    /// The last arrow of the equipped type clears the slot (BOW-AMM-001), and the next release is refused.
    #[test]
    fn the_last_arrow_clears_the_slot() {
        let (world, player) = oblivion();
        world.query_mut::<Inventory>().unwrap().get_mut(player).unwrap().items[1].count = 1;
        shoot_now(&world, 1.0).unwrap();
        assert_eq!(arrows(&world, player, ARROW), 0);
        assert_eq!(world.get::<EquippedAmmo>(player).map(|slot| *slot), Some(EquippedAmmo(None)));
        world.resource_mut::<BowState>().cooldown = 0.0;
        assert_eq!(shoot_now(&world, 1.0), Err(Refusal::NoArrows));
    }

    /// BOW-AMM-003, BOW-DX-003: each gate refuses with its named reason and spends nothing
    /// (`shot_refuses_each_gate`).
    #[test]
    fn shot_refuses_each_gate() {
        // No arrows equipped.
        let (mut world, player) = oblivion();
        set_equipped_ammo(&world, player, None);
        assert_eq!(shoot_now(&world, 1.0), Err(Refusal::NoArrows));
        // An equipped type the player no longer carries reads as none.
        set_equipped_ammo(&world, player, Some(0xDEAD));
        assert_eq!(shoot_now(&world, 1.0), Err(Refusal::NoArrows));
        set_equipped_ammo(&world, player, Some(ARROW));
        // The interval.
        shoot_now(&world, 1.0).unwrap();
        assert!(matches!(shoot_now(&world, 1.0), Err(Refusal::Cooldown(left)) if left > 0.0));
        assert_eq!(arrows(&world, player, ARROW), 9, "a refusal spends nothing");
        // A dead player cannot act; fly-cam is not a shot.
        world.resource_mut::<BowState>().cooldown = 0.0;
        world.insert(player, Dead);
        assert_eq!(shoot_now(&world, 1.0), Err(Refusal::CannotAct));
        world.remove::<Dead>(player);
        world.insert_resource(PlayerMode::FlyCam);
        assert_eq!(shoot_now(&world, 1.0), Err(Refusal::CannotAct));
        world.insert_resource(PlayerMode::Character);
        // No bow in hand: a sword is not a bow.
        let (world, _) = self::world(BowModel::OBLIVION, weapon(8, 0, 0, 1.0), arrow(8.0, 1.0, 0));
        assert_eq!(shoot_now(&world, 1.0), Err(Refusal::NoBow));
        // The game has no bow rules.
        let (mut world, _) = oblivion();
        world.remove_resource::<BowModel>();
        assert_eq!(shoot_now(&world, 1.0), Err(Refusal::NoBow));
        assert!(world.resource::<ActiveProjectiles>().0.is_empty());
    }

    /// BOW-DRW-002: the speed follows the draw timer at 0 s, 0.5 s and 1.875 s.
    #[test]
    fn the_arrow_speed_follows_the_draw_held() {
        for (held, speed) in [(0.0, 386.0), (0.5, 683.0), (1.875, 1500.0)] {
            let (world, _) = oblivion();
            let shot = shoot_now(&world, held).unwrap();
            assert!((shot.speed - speed).abs() < 1.0, "held {held}: {}", shot.speed);
        }
    }

    /// Skyrim's arrow flies at its `PROJ` speed, ignores the `PROJ` gravity (BOW-FLT-004), ends at the lesser of the
    /// record's range and the cap, and a release before the draw is complete fires nothing and spends nothing (Q4).
    #[test]
    fn a_skyrim_arrow_ignores_gravity_and_an_early_release_fires_nothing() {
        let proj = ProjRecord { form_id: 0x500, editor_id: "ArrowIronProjectile".into(), muzzle_speed: 3600.0, range: 5000.0, gravity: 0.35, collision_radius: 2.0, ..Default::default() };
        let (world, player) = world_with(BowModel::SKYRIM, weapon(10, 0, 7, 1.0), arrow(8.0, 0.0, 0x500), Some(proj));
        let needed = BowModel::SKYRIM.full_draw_seconds(1.0);
        assert!(matches!(shoot_now(&world, needed - 0.1), Err(Refusal::DrawIncomplete { .. })));
        assert_eq!(arrows(&world, player, ARROW), 10, "the arrow stays nocked");
        assert!(world.resource::<ActiveProjectiles>().0.is_empty());
        let shot = shoot_now(&world, needed).expect("a full draw fires, gravity or not");
        assert_eq!((shot.speed, shot.range), (3600.0, 4096.0));
        let active = world.resource::<ActiveProjectiles>();
        assert_eq!(active.0[0].radius, 2.0);
        // Tilted up by f1PArrowTiltUpAngle (2 degrees) in first person.
        let up = active.0[0].velocity.normalize().y;
        assert!((up - 2f32.to_radians().sin()).abs() < 1e-3, "{up}");
    }

    /// BOW-DRW-001: a bow draws while the attack action is held and fires on the release edge; it never arms a melee swing.
    #[test]
    fn the_attack_action_draws_and_the_release_fires() {
        let (world, player) = oblivion();
        let press = |world: &World| {
            world.resource_mut::<InputState>().mouse_buttons_held.insert(MouseButton::Left);
            crate::interaction::refresh_action_state(world);
        };
        let release = |world: &World| {
            world.resource_mut::<InputState>().mouse_buttons_held.remove(&MouseButton::Left);
            crate::interaction::refresh_action_state(world);
        };
        press(&world);
        player_bow_system(&world, 0.5);
        player_bow_system(&world, 0.5);
        assert_eq!(arrows(&world, player, ARROW), 10, "holding the draw spends nothing");
        release(&world);
        player_bow_system(&world, 0.016);
        assert_eq!(arrows(&world, player, ARROW), 9);
        let shot = world.resource::<BowState>().last.clone().unwrap().unwrap();
        assert!((shot.held - 1.0).abs() < 1e-3, "two half-second frames of draw: {}", shot.held);
        assert!(bow_in_hand(&world));
    }

    /// BOW-SET-001: ballistic is in the list, labelled unavailable, and resolves to straight with that stated.
    #[test]
    fn ballistic_is_listed_but_flies_straight() {
        let mut world = World::new();
        let mut registry = SettingsRegistry::default();
        register_settings(&mut registry).unwrap();
        world.insert_resource(registry);
        assert_eq!(trajectory(&world), (ArrowTrajectory::Straight, ArrowTrajectory::Straight));
        world.resource_mut::<SettingsRegistry>().set(TRAJECTORY_SETTING_ID, SettingValue::Choice("ballistic".into())).unwrap();
        assert_eq!(trajectory(&world), (ArrowTrajectory::Ballistic, ArrowTrajectory::Straight));
        assert!(world.resource_mut::<SettingsRegistry>().set(TRAJECTORY_SETTING_ID, SettingValue::Choice("lob".into())).is_err());
    }

    /// BOW-SET-002: the per-game default comes from the row, seeded when the model is installed.
    #[test]
    fn the_default_trajectory_follows_the_games_row() {
        let mut world = World::new();
        let mut registry = SettingsRegistry::default();
        register_settings(&mut registry).unwrap();
        world.insert_resource(registry);
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.character_rules = byroredux_core::character::CharacterRulesProfile::OBLIVION;
        install_bow_model(&mut world, &index);
        assert_eq!(world.resource::<BowModel>().default_trajectory, ArrowTrajectory::Ballistic);
        assert_eq!(trajectory(&world), (ArrowTrajectory::Ballistic, ArrowTrajectory::Straight));
        index.character_rules = byroredux_core::character::CharacterRulesProfile::FALLOUT_NEW_VEGAS;
        install_bow_model(&mut world, &index);
        assert!(world.try_resource::<BowModel>().is_none(), "Fallout has no bow row");
    }
}
