//! Oblivion skill-use emitters at the hit site (#7051 Phase 2a; PRD `oblivion-skill-use-leveling`
//! OSL-USE-002 and OSL-USE-003; design `docs/engine/skill-use.md` §2).
//!
//! `combat_damage_system` resolves every `HitEvent` once, so this is the one place that knows a blow landed
//! on a living target. From it:
//!
//! * a **melee blow the player lands** feeds the skill of the equipped weapon (Blade for a blade, Blunt for a
//!   blunt weapon, Hand to Hand unarmed), usage 0: UESP's "each hit on a target";
//! * a **hit the player takes** feeds Light or Heavy Armor by the piece struck (`Oblivion.exe` `0x5e5a00`:
//!   a roll of 0 to 99 over the slots' `iArmorDamage*Chance` weights, re-rolled up to seven times while the
//!   struck slot is empty), usage 0.
//!
//! * **seconds of a stance** (`stance_use_system`, `Oblivion.exe` `0x671620`): each full second the player
//!   moves while sneaking and undetected feeds Sneak (usage 0), while swimming Athletics (usage 1), while
//!   running Athletics (usage 0), one at a time in that order; the original's one-second timer accumulates
//!   only while its stance holds and resets when it fires;
//! * **a jump** (`on_jump`) feeds Acrobatics (usage 0): UESP's "each jump or dodge".
//!
//! Emitters only queue ([`byroredux_scripting::oblivion_skill_use::queue_skill_use`]); nothing here touches
//! the player's actor values. A weapon the engine cannot feed (a staff casts, a bow has no shot path here)
//! feeds nothing, and a blocked hit feeds nothing: no producer models a target's own guard (#4324).

use byroredux_core::character::{ArmorHitSlot, ARMOR_HIT_ATTEMPTS};
use byroredux_core::ecs::components::{EquipmentSlots, EquippedWeapon, Inventory};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::ItemKind;

/// Oblivion skill actor-value codes this module feeds.
const BLADE: u32 = 14;
const BLUNT: u32 = 16;
const HAND_TO_HAND: u32 = 17;
const HEAVY_ARMOR: u32 = 18;
const LIGHT_ARMOR: u32 = 27;
const ATHLETICS: u32 = 13;
const ACROBATICS: u32 = 26;
const SNEAK: u32 = 31;

/// The skill a melee weapon of Oblivion `WEAP` type `weapon_type` feeds (0 and 1 blades, 2 and 3 blunt
/// weapons; 4, a staff, casts instead, and 5, a bow, has no melee blow), or Hand to Hand for no weapon.
#[must_use]
pub(crate) fn weapon_skill(weapon_type: Option<u8>) -> Option<u32> {
    match weapon_type {
        None => Some(HAND_TO_HAND),
        Some(0 | 1) => Some(BLADE),
        Some(2 | 3) => Some(BLUNT),
        Some(_) => None,
    }
}

/// splitmix64: a deterministic roll source for the armor-piece pick, seeded by the hit.
fn mix(mut seed: u64) -> u64 {
    seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The skill (Light or Heavy Armor) of the piece a hit strikes on `wearer`, or `None` when seven rolls
/// find every struck slot empty. `seed` makes the rolls deterministic per hit.
#[must_use]
pub(crate) fn struck_armor_skill(world: &World, wearer: EntityId, seed: u64) -> Option<u32> {
    let chances = ArmorHitSlot::ROLLED.map(|(_, name, default)| {
        byroredux_scripting::obscript_state::game_setting(world, name).map_or(default, |value| value.max(0.0) as u32)
    });
    let slots = world.get::<EquipmentSlots>(wearer).map(|slots| (*slots).clone())?;
    let inventory = world.get::<Inventory>(wearer).map(|inventory| (*inventory).clone())?;
    let index = world.try_resource::<crate::components::LoadedCellIndex>()?;
    for attempt in 0..ARMOR_HIT_ATTEMPTS {
        let roll = (mix(seed.wrapping_add(attempt as u64)) % 100) as u32;
        let slot = ArmorHitSlot::from_roll(chances, roll);
        let mask = slot.biped_mask();
        let piece = (0..16)
            .filter(|bit| mask & (1 << bit) != 0)
            .find_map(|bit| slots.occupants[bit as usize])
            .and_then(|row| inventory.items.get(row.0 as usize))
            .and_then(|stack| index.0.items.get(&stack.base_form_id));
        if let Some(ItemKind::Armor { armor_type, .. }) = piece.map(|item| &item.kind) {
            return Some(if *armor_type == Some(1) { HEAVY_ARMOR } else { LIGHT_ARMOR });
        }
    }
    None
}

/// The Oblivion `WEAP` type of the weapon `aggressor` wields: `Some(None)` unarmed, `Some(Some(type))` for a
/// weapon, `None` when the equipped item is not a known weapon.
fn wielded_type(world: &World, aggressor: EntityId) -> Option<Option<u8>> {
    let Some(weapon) = world.get::<EquippedWeapon>(aggressor).map(|weapon| weapon.base_form_id) else {
        return Some(None);
    };
    let index = world.try_resource::<crate::components::LoadedCellIndex>()?;
    match &index.0.items.get(&weapon)?.kind {
        ItemKind::Weapon { oblivion_type, .. } => Some(Some(*oblivion_type)),
        _ => None,
    }
}

/// One `HitEvent` that landed on the living `target`: queue the skill uses it earns the player.
/// `seed` varies the armor-piece roll from hit to hit.
pub(crate) fn on_landed_hit(world: &World, aggressor: EntityId, target: EntityId, blocked: bool, seed: u64) {
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return;
    };
    // A blocked blow never landed; a block is the aggressor's own stance here (#4324).
    if blocked || aggressor == target {
        return;
    }
    if aggressor == player {
        let skill = wielded_type(world, player).and_then(weapon_skill);
        if let Some(skill) = skill {
            byroredux_scripting::oblivion_skill_use::queue_skill_use(world, skill, 0, 0.0);
        }
    } else if target == player
        && let Some(skill) = struck_armor_skill(world, player, seed)
    {
        byroredux_scripting::oblivion_skill_use::queue_skill_use(world, skill, 0, 0.0);
    }
}

/// The one-second timers of the stances (`player + 0x5a4` sneak, `+0x59c` run, `+0x5a0` swim), each
/// accumulating only while its stance holds. Session state, `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Default)]
pub(crate) struct StanceTimers {
    sneak: f32,
    swim: f32,
    run: f32,
}

impl byroredux_core::ecs::resource::Resource for StanceTimers {}

/// Advance one stance timer: it counts `dt` while `active` and, on reaching a second, fires and resets
/// (`0x671855`: `1.0 <= timer` fires, otherwise the frame time is added). A stance that does not hold
/// leaves the timer where it was.
pub(crate) fn stance_step(timer: &mut f32, active: bool, dt: f32) -> bool {
    if !active {
        return false;
    }
    if *timer >= 1.0 {
        *timer = 0.0;
        return true;
    }
    *timer += dt.max(0.0);
    false
}

/// Which stance the player holds this frame: sneaking and undetected (Sneak), swimming (Athletics, usage 1),
/// or running (Athletics, usage 0). The first that holds wins, as the executable's branches do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stance {
    Sneak,
    Swim,
    Run,
}

/// The stance of a moving player. `undetected` is the detection level toward the player below Noticed.
#[must_use]
pub(crate) fn stance_of(moving: bool, sneaking: bool, undetected: bool, swimming: bool, running: bool) -> Option<Stance> {
    if !moving {
        None
    } else if sneaking && undetected {
        Some(Stance::Sneak)
    } else if swimming {
        Some(Stance::Swim)
    } else if running && !sneaking {
        Some(Stance::Run)
    } else {
        None
    }
}

/// Feed Sneak, Athletics from the player's stance (OSL-USE-005). Runs every frame in the Late stage
/// ahead of the drain; the timers only advance while their stance holds.
pub(crate) fn stance_use_system(world: &World, dt: f32) {
    use byroredux_core::ecs::components::Sneaking;
    use byroredux_core::stealth::DetectionLevel;
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return;
    };
    let (moving, running) = world
        .try_resource::<crate::interaction::ActionState>()
        .map(|actions| {
            use crate::interaction::InputAction::*;
            let moving = [MoveForward, MoveBackward, StrafeLeft, StrafeRight].into_iter().any(|action| actions.is_held(action));
            (moving, actions.is_held(Sprint))
        })
        .unwrap_or((false, false));
    let undetected = world
        .try_resource::<crate::detection::PlayerDetection>()
        .is_none_or(|detection| detection.level < DetectionLevel::Noticed);
    let stance = stance_of(
        moving,
        world.has::<Sneaking>(player),
        undetected,
        crate::systems::actor_is_swimming(world, player),
        running,
    );
    let fired = {
        let Some(mut timers) = world.try_resource_mut::<StanceTimers>() else {
            return;
        };
        let timers = &mut *timers;
        let mut fired = Vec::new();
        if stance_step(&mut timers.sneak, stance == Some(Stance::Sneak), dt) {
            fired.push((SNEAK, 0));
        }
        if stance_step(&mut timers.swim, stance == Some(Stance::Swim), dt) {
            fired.push((ATHLETICS, 1));
        }
        if stance_step(&mut timers.run, stance == Some(Stance::Run), dt) {
            fired.push((ATHLETICS, 0));
        }
        fired
    };
    for (skill, usage) in fired {
        byroredux_scripting::oblivion_skill_use::queue_skill_use(world, skill, usage, 0.0);
    }
}

/// The player jumped (the controller's `jump_fired` on land, not in water): feed Acrobatics, usage 0.
pub(crate) fn on_jump(world: &World) {
    byroredux_scripting::oblivion_skill_use::queue_skill_use(world, ACROBATICS, 0, 0.0);
}

/// What the level system has already told the player: the count of pending levels it announced. `None` until
/// the first frame, so a save that loads with a level pending does not announce it again. Session state,
/// `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Default)]
pub(crate) struct LevelNotices {
    announced: Option<usize>,
}

impl byroredux_core::ecs::resource::Resource for LevelNotices {}

/// The hint the original shows with the tenth major increase: `sMeditate`, an executable default that
/// `Oblivion.esm` does not author (#7049 V12).
const MEDITATE: &str = "You should rest and meditate on what you've learned.";

/// UESP *Oblivion: Leveling*, Notes: what an attempt to level with every attribute at 100 says.
const PERFECTION: &str = "Perfection cannot be improved upon. Your attributes are as high as they can go.";

/// The level-up message of the level just reached: the load order's `sLevelUp{N}` flavour text when it authors
/// one (`Oblivion.esm` authors levels 2 to 20; a higher level reuses the last), after the line that names the level.
fn level_message(world: &World, level: u16) -> String {
    let flavour = world.try_resource::<crate::components::LoadedCellIndex>().and_then(|index| {
        index.0.game_setting_string(&format!("sLevelUp{}", level.clamp(2, 20))).map(str::to_owned)
    });
    match flavour {
        Some(text) if !text.is_empty() => format!("You are now level {level}. {text}"),
        _ => format!("You are now level {level}."),
    }
}

/// OSL-LVL-004, OSL-SLP-001/002, OSL-API-004 (#7052): announce the levels that became pending, and take the
/// oldest pending level at each completed sleep. Only a sleep that ran to its end applies a level, so waiting
/// (which raises no completion) and an interrupted sleep change nothing. A level with fewer than three attributes
/// picked stays pending and says so.
pub(crate) fn oblivion_level_system(world: &World, _dt: f32) {
    use byroredux_scripting::oblivion_skill_use::{apply_pending_level, LevelUp, SleepCompletions};
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return;
    };
    let completions = world
        .try_resource_mut::<SleepCompletions>()
        .map(|mut completions| std::mem::take(&mut completions.0))
        .unwrap_or_default();
    let pending_now = |world: &World| world.get::<byroredux_core::character::SkillUseLevel>(player).map_or(0, |level| level.pending.len());
    // A level that became pending since the last frame (a rank-up, `AdvancePCLevel`).
    let announced = world.try_resource::<LevelNotices>().and_then(|notices| notices.announced);
    let now = pending_now(world);
    if announced.is_some_and(|announced| now > announced) {
        crate::notifications::push(world, MEDITATE);
    }
    for sleep in completions.into_iter().filter(|sleep| !sleep.interrupted) {
        match apply_pending_level(world, player) {
            LevelUp::NoPending => {}
            LevelUp::NotReady { picks } => crate::notifications::push(
                world,
                format!("A level is waiting: choose three attributes with `levelup attr <attribute>` ({picks} of 3), then sleep."),
            ),
            LevelUp::Perfection => crate::notifications::push(world, PERFECTION),
            LevelUp::Applied { level, raised, health_gain } => {
                log::info!("level up: {level}, raised {raised:?}, Health +{health_gain}");
                crate::notifications::push(world, level_message(world, level));
            }
        }
        let _ = sleep;
    }
    if let Some(mut notices) = world.try_resource_mut::<LevelNotices>() {
        notices.announced = Some(pending_now(world));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UESP's weapon pages: one-handed and two-handed blades feed Blade, blunts feed Blunt, the unarmed blow
    /// Hand to Hand; a staff and a bow feed no melee skill.
    #[test]
    fn a_weapon_type_names_its_skill() {
        assert_eq!(weapon_skill(None), Some(HAND_TO_HAND));
        assert_eq!(weapon_skill(Some(0)), Some(BLADE));
        assert_eq!(weapon_skill(Some(1)), Some(BLADE));
        assert_eq!(weapon_skill(Some(2)), Some(BLUNT));
        assert_eq!(weapon_skill(Some(3)), Some(BLUNT));
        assert_eq!(weapon_skill(Some(4)), None, "a staff casts");
        assert_eq!(weapon_skill(Some(5)), None, "no bow shot path");
    }

    /// The roll source is deterministic and spreads over 0..100.
    #[test]
    fn the_roll_is_deterministic_and_spread() {
        assert_eq!(mix(7), mix(7));
        assert_ne!(mix(7), mix(8));
        let mut seen = [false; 10];
        for seed in 0..200u64 {
            seen[(mix(seed) % 100 / 10) as usize] = true;
        }
        assert!(seen.iter().all(|&decile| decile), "{seen:?}");
    }

    use byroredux_core::character::{CharacterRuleset, LevelingModel};
    use byroredux_core::ecs::components::{InventoryIndex, ItemStack};
    use byroredux_plugin::esm::records::{EsmIndex, ItemRecord};
    use byroredux_scripting::oblivion_skill_use::{PendingSkillUse, SkillUseRecord};

    const BLADE_WEAPON: u32 = 0x100;
    const STAFF: u32 = 0x101;
    const CUIRASS: u32 = 0x200;
    const LIGHT_BOOTS: u32 = 0x201;

    fn weapon(form_id: u32, oblivion_type: u8) -> ItemRecord {
        let mut kind = ItemKind::weapon_stub(10, 0);
        if let ItemKind::Weapon { oblivion_type: slot, .. } = &mut kind {
            *slot = oblivion_type;
        }
        ItemRecord { form_id, common: Default::default(), kind }
    }

    fn armor(form_id: u32, biped_flags: u64, heavy: bool) -> ItemRecord {
        ItemRecord {
            form_id,
            common: Default::default(),
            kind: ItemKind::Armor {
                biped_flags,
                dt: 0.0,
                dr: 0,
                health: 0,
                slot_mask: 0,
                armor_rating_x100: 0,
                armor_type: Some(u32::from(heavy)),
                female_model_path: String::new(),
                armatures: Vec::new(),
            },
        }
    }

    /// A world with Oblivion's ruleset, a player wearing a heavy cuirass (Upper Body) and light boots (Foot),
    /// and one blade and one staff to wield.
    fn world() -> (World, EntityId, EntityId) {
        let mut world = World::new();
        world.register::<Inventory>();
        world.register::<EquipmentSlots>();
        world.register::<EquippedWeapon>();
        world.insert_resource(CharacterRuleset::new(LevelingModel::OBLIVION));
        world.insert_resource(PendingSkillUse::default());
        let mut index = EsmIndex::default();
        for record in [weapon(BLADE_WEAPON, 1), weapon(STAFF, 4), armor(CUIRASS, 1 << 2, true), armor(LIGHT_BOOTS, 1 << 5, false)] {
            index.items.insert(record.form_id, record);
        }
        world.insert_resource(crate::components::LoadedCellIndex(std::sync::Arc::new(index)));
        let player = world.spawn();
        let npc = world.spawn();
        let mut inventory = Inventory::new();
        inventory.push(ItemStack { base_form_id: CUIRASS, count: 1, instance: None });
        inventory.push(ItemStack { base_form_id: LIGHT_BOOTS, count: 1, instance: None });
        world.insert(player, inventory);
        let mut slots = EquipmentSlots::new();
        slots.occupants[2] = Some(InventoryIndex(0));
        slots.occupants[5] = Some(InventoryIndex(1));
        world.insert(player, slots);
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        (world, player, npc)
    }

    fn queued(world: &World) -> Vec<u32> {
        world.resource::<PendingSkillUse>().0.iter().map(|SkillUseRecord { skill, .. }| *skill).collect()
    }

    fn wield(world: &mut World, player: EntityId, base_form_id: u32) {
        let weapon = EquippedWeapon { inventory_index: InventoryIndex(0), base_form_id, damage: 10.0, reach: 0.0, speed: 0.0 };
        world.insert(player, weapon);
    }

    /// OSL-USE-002: the player's landed blow feeds the wielded weapon's skill (once, usage 0), Hand to Hand
    /// when unarmed; a blocked blow, a staff, and a blow on oneself feed nothing.
    #[test]
    fn a_landed_melee_blow_feeds_the_weapons_skill() {
        let (mut world, player, npc) = world();
        on_landed_hit(&world, player, npc, false, 1);
        assert_eq!(queued(&world), vec![HAND_TO_HAND], "unarmed");
        wield(&mut world, player, BLADE_WEAPON);
        on_landed_hit(&world, player, npc, false, 2);
        assert_eq!(queued(&world), vec![HAND_TO_HAND, BLADE]);
        on_landed_hit(&world, player, npc, true, 3);
        on_landed_hit(&world, player, player, false, 4);
        wield(&mut world, player, STAFF);
        on_landed_hit(&world, player, npc, false, 5);
        assert_eq!(queued(&world).len(), 2, "blocked, self-inflicted and staff blows feed nothing");
        // The amount is the SKIL value of the action: Blade 0.5, Hand to Hand 0.6.
        let amounts: Vec<f32> = world.resource::<PendingSkillUse>().0.iter().map(|r| r.amount).collect();
        assert_eq!(amounts, vec![0.6, 0.5]);
    }

    /// OSL-USE-003: a hit on the player feeds the struck piece's skill: the heavy cuirass 25 of 100 rolls,
    /// the light boots 10, and the shield, helmet, greaves and gauntlets slots (all empty here) re-roll,
    /// so across many hits Heavy Armor outnumbers Light Armor and some hits strike nothing. An NPC's own
    /// armor, and a blow the player lands, feed no armor skill.
    #[test]
    fn a_hit_the_player_takes_feeds_the_struck_pieces_skill() {
        let (world, player, npc) = world();
        for seed in 0..400 {
            on_landed_hit(&world, npc, player, false, seed);
        }
        let heavy = queued(&world).iter().filter(|&&skill| skill == HEAVY_ARMOR).count();
        let light = queued(&world).iter().filter(|&&skill| skill == LIGHT_ARMOR).count();
        assert_eq!(heavy + light, queued(&world).len(), "only armor skills");
        assert!(heavy > light && light > 0, "heavy {heavy}, light {light}");
        assert!(heavy + light < 400, "an empty slot re-rolls, but seven empty rolls strike nothing");
        // A blocked hit never landed.
        let before = queued(&world).len();
        for seed in 0..50 {
            on_landed_hit(&world, npc, player, true, seed);
        }
        assert_eq!(queued(&world).len(), before);
    }

    /// A ruleset that does not level by skill use queues nothing, so no other game feeds Oblivion's skills.
    #[test]
    fn another_ruleset_queues_nothing() {
        let (mut world, player, npc) = world();
        world.insert_resource(CharacterRuleset::new(LevelingModel::SKYRIM));
        on_landed_hit(&world, player, npc, false, 1);
        on_landed_hit(&world, npc, player, false, 1);
        assert!(queued(&world).is_empty());
    }

    /// `0x671855`: the timer counts only while its stance holds, fires at a second and restarts; an
    /// interrupted stance keeps its count.
    #[test]
    fn a_stance_timer_fires_each_second_of_its_stance() {
        let mut timer = 0.0;
        let mut fired = 0;
        for _ in 0..300 {
            fired += usize::from(stance_step(&mut timer, true, 0.05));
        }
        assert!((14..=15).contains(&fired), "15 s at 20 Hz: {fired}"); // a fire frame adds no time
        let mut paused = 0.5;
        assert!(!stance_step(&mut paused, false, 10.0));
        assert_eq!(paused, 0.5, "a stance that does not hold leaves the count");
    }

    /// The executable's branch order: sneaking undetected, else swimming, else running (never while
    /// sneaking); nothing while standing still.
    #[test]
    fn the_stance_follows_the_branch_order() {
        assert_eq!(stance_of(true, true, true, true, true), Some(Stance::Sneak));
        assert_eq!(stance_of(true, true, false, true, false), Some(Stance::Swim), "detected: not sneaking for the skill");
        assert_eq!(stance_of(true, false, true, true, true), Some(Stance::Swim));
        assert_eq!(stance_of(true, false, true, false, true), Some(Stance::Run));
        assert_eq!(stance_of(true, true, false, false, true), None, "detected and sneaking runs nothing");
        assert_eq!(stance_of(true, false, true, false, false), None, "walking");
        assert_eq!(stance_of(false, true, true, true, true), None, "standing still");
    }

    /// OSL-USE-005 end to end: a player running for a second queues one Athletics use at 0.03 (value 1),
    /// sneaking undetected 0.75 of Sneak, and a jump 0.3 of Acrobatics; swimming would use value 2.
    #[test]
    fn seconds_of_a_stance_and_a_jump_queue_their_skil_amounts() {
        use byroredux_core::ecs::components::Sneaking;
        let (mut world, player, _) = world();
        world.insert_resource(StanceTimers::default());
        world.register::<Sneaking>();
        let mut actions = crate::interaction::ActionState::default();
        actions.press_for_test(crate::interaction::InputAction::MoveForward);
        actions.press_for_test(crate::interaction::InputAction::Sprint);
        world.insert_resource(actions);
        for _ in 0..30 {
            stance_use_system(&world, 0.05);
        }
        let amounts = |world: &World| -> Vec<(u32, f32)> {
            world.resource::<PendingSkillUse>().0.iter().map(|r| (r.skill, r.amount)).collect()
        };
        assert_eq!(amounts(&world), vec![(ATHLETICS, 0.03)], "one second of running in 30 frames of 0.05 s");
        world.insert(player, Sneaking);
        for _ in 0..30 {
            stance_use_system(&world, 0.05);
        }
        assert_eq!(amounts(&world), vec![(ATHLETICS, 0.03), (SNEAK, 0.75)]);
        on_jump(&world);
        assert_eq!(amounts(&world).last(), Some(&(ACROBATICS, 0.3)));
    }

    /// A player at level 1 with Blade 5, Endurance 40 and Health 80, and the Oblivion roster installed.
    fn level_world() -> (World, EntityId) {
        use byroredux_core::ecs::components::ActorValues;
        use byroredux_plugin::esm::reader::{GameKind, GlobalSlot};
        let (mut world, player, _) = world();
        let mut index = EsmIndex { game: GameKind::Oblivion, ..Default::default() };
        index.install_oblivion_actor_values(GlobalSlot::Regular(0)).unwrap();
        byroredux_scripting::install_actor_value_names(&mut world, &index);
        world.insert_resource(byroredux_scripting::ScriptGame(GameKind::Oblivion));
        world.insert_resource(byroredux_scripting::skills::SkillProgress::default());
        world.insert_resource(byroredux_scripting::oblivion_skill_use::SleepCompletions::default());
        world.insert_resource(LevelNotices::default());
        world.insert_resource(crate::notifications::PlayerNotifications::default());
        // A class whose seven majors include Blade, so Blade's rank-ups count toward the level.
        world.insert_resource(byroredux_scripting::oblivion_skill_use::SkillUseClasses {
            player_class: Some(0x100),
            classes: std::collections::HashMap::from([(
                0x100,
                byroredux_scripting::oblivion_skill_use::ClassSkills {
                    name: "Test".into(),
                    specialization: 0,
                    major_skills: vec![BLADE],
                },
            )]),
        });
        world.register::<ActorValues>();
        world.register::<byroredux_core::character::SkillUseLevel>();
        world.register::<byroredux_core::character::CharacterLevel>();
        let av = |code: u32| index.oblivion_actor_value(code, true).unwrap();
        let mut pairs: Vec<(u32, f32)> = (0..8).map(|code| (av(code), 40.0)).collect();
        pairs.extend([(av(8), 80.0), (av(14), 5.0)]);
        world.insert(player, ActorValues::from_pairs(pairs));
        world.insert(player, byroredux_core::character::CharacterLevel { level: 1, xp: 0 });
        (world, player)
    }

    fn level_of(world: &World, player: EntityId) -> u16 {
        world.get::<byroredux_core::character::CharacterLevel>(player).unwrap().level
    }

    fn sleep(world: &World, interrupted: bool) {
        world.resource_mut::<byroredux_scripting::oblivion_skill_use::SleepCompletions>().0.push(
            byroredux_scripting::oblivion_skill_use::SleepCompleted { hours: 8.0, interrupted },
        );
    }

    /// PRD §7.2 scenarios 4 and 7 (OSL-LVL-004, OSL-SLP-001/002, OSL-API-004): ten major increases make a level
    /// pending and say so; three picks and a completed sleep take it (and tell the player the new level); a frame
    /// with no sleep (waiting) and an interrupted sleep take nothing; the stored Health term gains a tenth of the Endurance.
    #[test]
    fn only_a_completed_sleep_takes_a_picked_level() {
        use byroredux_scripting::oblivion_skill_use::{pick_attribute, raise_skill_ranks};
        let (world, player) = level_world();
        oblivion_level_system(&world, 0.0); // seeds the announcement count
        raise_skill_ranks(&world, player, BLADE, 10, false);
        oblivion_level_system(&world, 0.0);
        let told = crate::notifications::drain(&world);
        assert_eq!(told, vec![MEDITATE.to_string()], "the tenth major increase says to rest");
        for attribute in [0, 5, 7] {
            pick_attribute(&world, player, attribute).unwrap();
        }
        // Waiting: no completed sleep, so nothing changes however many frames pass.
        for _ in 0..5 {
            oblivion_level_system(&world, 1.0);
        }
        assert_eq!(level_of(&world, player), 1);
        // An interrupted sleep does not apply it either.
        sleep(&world, true);
        oblivion_level_system(&world, 0.0);
        assert_eq!(level_of(&world, player), 1);
        assert!(crate::notifications::drain(&world).is_empty());
        // A completed sleep does.
        sleep(&world, false);
        oblivion_level_system(&world, 0.0);
        assert_eq!(level_of(&world, player), 2);
        let told = crate::notifications::drain(&world);
        assert_eq!(told.len(), 1);
        assert!(told[0].starts_with("You are now level 2."), "{told:?}");
        let health = world.resource::<byroredux_scripting::ActorValueNames>().resolve("Health").unwrap();
        let strength = world.resource::<byroredux_scripting::ActorValueNames>().resolve("Strength").unwrap();
        let values = world.get::<byroredux_core::ecs::components::ActorValues>(player).unwrap();
        assert_eq!(values.get_base(strength).unwrap().base, 45.0, "Blade's ten rank-ups give Strength +5");
        // Endurance 40 had no rank-ups, so its pick gave +1 (an empty tally): 41, a tenth of which rounds down to 4.
        assert_eq!(values.stored(health), 4.0);
        // A second sleep with nothing pending does nothing.
        drop(values);
        sleep(&world, false);
        oblivion_level_system(&world, 0.0);
        assert_eq!(level_of(&world, player), 2);
    }

    /// A level whose attributes are not all picked stays pending and the sleep says what to do; another sleep after
    /// the picks takes it.
    #[test]
    fn an_unpicked_level_waits_for_its_picks() {
        use byroredux_scripting::oblivion_skill_use::{pick_attribute, raise_skill_ranks};
        let (world, player) = level_world();
        oblivion_level_system(&world, 0.0);
        raise_skill_ranks(&world, player, BLADE, 10, false);
        crate::notifications::drain(&world);
        sleep(&world, false);
        oblivion_level_system(&world, 0.0);
        let told = crate::notifications::drain(&world);
        assert!(told.iter().any(|line| line.contains("choose three attributes")), "{told:?}");
        assert_eq!(level_of(&world, player), 1);
        for attribute in [1, 2, 3] {
            pick_attribute(&world, player, attribute).unwrap();
        }
        sleep(&world, false);
        oblivion_level_system(&world, 0.0);
        assert_eq!(level_of(&world, player), 2);
    }

    /// A save that loads with a level pending does not announce it again: the first frame only seeds the count.
    #[test]
    fn a_loaded_pending_level_is_not_announced_again() {
        use byroredux_scripting::oblivion_skill_use::raise_skill_ranks;
        let (world, player) = level_world();
        raise_skill_ranks(&world, player, BLADE, 10, false);
        oblivion_level_system(&world, 0.0);
        assert!(crate::notifications::drain(&world).is_empty());
    }
}
