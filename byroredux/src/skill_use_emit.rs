//! Oblivion skill-use emitters at the hit site (#7051 Phase 2a; PRD `oblivion-skill-use-leveling`
//! OSL-USE-002 and OSL-USE-003; design `docs/engine/skill-use.md` §2).
//!
//! `combat_damage_system` resolves every `HitEvent` once, so this is the one place that knows a blow landed
//! on a living target. From it:
//!
//! * a **melee blow the player lands** feeds the skill of the equipped weapon (Blade for a blade, Blunt for a
//!   blunt weapon, Hand to Hand unarmed), usage 0: UESP's "each hit on a target";
//! * a **hit the player takes** feeds Light or Heavy Armor by the piece struck, usage 0: a roll of 0 to 99
//!   over the slots' `iArmorDamage*Chance` weights (CS), rolled again up to seven times while the struck
//!   slot is empty. The weights are documented; the roll order, the shield as remainder and the attempt
//!   count are an engine choice ([`ARMOR_HIT_ATTEMPTS`], #7310).
//!
//! * **seconds of a stance** (`stance_use_system`): each full second the player moves while sneaking and
//!   undetected feeds Sneak (usage 0), while swimming Athletics (usage 1), while running Athletics (usage 0).
//!   The per-second unit is UESP's (*Oblivion:Increasing Skills*: "time (secs) spent running", "…swimming",
//!   "…undetected while moving in sneak mode"). One stance at a time, in that order, and a timer that
//!   accumulates only while its stance holds and resets when it fires, are engine choices (#7331);
//! * **a jump** (`on_jump`) feeds Acrobatics (usage 0): UESP's "each jump or dodge".
//!
//! Emitters only queue ([`byroredux_scripting::oblivion_skill_use::queue_skill_use`]); nothing here touches
//! the player's actor values. A staff in hand feeds nothing (it casts); a bow feeds Marksman only through its
//! arrow's hit (#7055), and a hit the player blocks feeds Block (#7054), both in [`on_landed_hit`].

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
/// Marksman: fed by the player's arrow hits (player-archery BOW-USE-001).
const MARKSMAN: u32 = 28;
/// Block: fed by a blocked hit on the blocking player (#7054).
const BLOCK: u32 = 15;
/// Oblivion `WEAP` `DATA` type 5: a bow.
const BOW_TYPE: u8 = 5;

/// The skill a weapon of Oblivion `WEAP` type `weapon_type` feeds (0 and 1 blades, 2 and 3 blunt weapons, 5 a bow's
/// Marksman; 4, a staff, casts instead), or Hand to Hand for no weapon. A bow feeds only through its arrow's hit
/// ([`on_landed_hit`]), never through the weapon in hand.
#[must_use]
pub(crate) fn weapon_skill(weapon_type: Option<u8>) -> Option<u32> {
    match weapon_type {
        None => Some(HAND_TO_HAND),
        Some(0 | 1) => Some(BLADE),
        Some(2 | 3) => Some(BLUNT),
        Some(BOW_TYPE) => Some(MARKSMAN),
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

/// The skill (Light or Heavy Armor) of the piece a hit strikes on `wearer`, or `None` when
/// [`ARMOR_HIT_ATTEMPTS`] rolls find every struck slot empty. `seed` makes the rolls deterministic per hit.
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
/// `seed` varies the armor-piece roll from hit to hit. `arrow` is the hit's provenance (the player's own arrow,
/// BOW-USE-005): an arrow feeds the bow's skill, and a hit from anything else feeds nothing while a bow is wielded,
/// so a script's projectile or a spell cannot earn Marksman. `guarded` is the target's own guard (#7054): the hit met a
/// block that stopped some of it, so a blocking player earns Block (the `SKIL` first use value, usage 0, 1.25).
pub(crate) fn on_landed_hit(world: &World, aggressor: EntityId, target: EntityId, blocked: bool, arrow: bool, guarded: bool, seed: u64) {
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return;
    };
    // A blocked blow never landed; a block is the aggressor's own stance here (#4324).
    if blocked || aggressor == target {
        return;
    }
    if aggressor == player {
        let skill = match (arrow, wielded_type(world, player)) {
            (true, _) => weapon_skill(Some(BOW_TYPE)),
            (false, Some(Some(BOW_TYPE))) => None,
            (false, wielded) => wielded.and_then(weapon_skill),
        };
        if let Some(skill) = skill {
            byroredux_scripting::oblivion_skill_use::queue_skill_use(world, skill, 0, 0.0);
        }
    } else if target == player {
        if guarded {
            byroredux_scripting::oblivion_skill_use::queue_skill_use(world, BLOCK, 0, 0.0);
        }
        if let Some(skill) = struck_armor_skill(world, player, seed) {
            byroredux_scripting::oblivion_skill_use::queue_skill_use(world, skill, 0, 0.0);
        }
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
/// (`1.0 <= timer` fires, otherwise the frame time is added). A stance that does not hold leaves the timer
/// where it was. UESP gives the per-second unit; the timer's shape is an engine choice (#7331).
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
/// or running (Athletics, usage 0). The first that holds wins: an engine choice, since no source says
/// whether two stances can feed in one second (#7331).
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
    // #7281 — a held key counts only when the controller would move the body
    // with it (not dead, not the fly camera, not restrained or in VATS).
    let drives_body = crate::systems::player_movement_input_drives_body(world, player);
    let (moving, running) = world
        .try_resource::<crate::interaction::ActionState>()
        .map(|actions| {
            use crate::interaction::InputAction::*;
            let moving = [MoveForward, MoveBackward, StrafeLeft, StrafeRight].into_iter().any(|action| actions.is_held(action));
            (drives_body && moving, actions.is_held(Sprint))
        })
        .unwrap_or((false, false));
    // #7294 — UESP *Oblivion:Sneak*: "only if there is a person or creature nearby" and the
    // sneaker is not detected. "Nearby" is the detection tick's own range: it names a detector
    // only when a live actor stands within it, so an empty room (no detector) banks nothing.
    let undetected = world
        .try_resource::<crate::detection::PlayerDetection>()
        .is_some_and(|detection| detection.detector.is_some() && detection.level < DetectionLevel::Noticed);
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

/// The player landed from a fall that hurt (#7056): feed Acrobatics at usage 1, the `SKIL` second use value (3.0; UESP:
/// "each time you take damage from falling ... it does not matter how much damage you take").
pub(crate) fn on_fall(world: &World) {
    byroredux_scripting::oblivion_skill_use::queue_skill_use(world, ACROBATICS, 1, 0.0);
}

/// What the level system has already told the player: the count of pending levels it announced. `None` until
/// the first frame, so a save that loads with a level pending does not announce it again. Session state,
/// `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Default)]
pub(crate) struct LevelNotices {
    announced: Option<usize>,
}

impl byroredux_core::ecs::resource::Resource for LevelNotices {}

/// The hint shown with the tenth major increase (UESP *Oblivion:First Time Players*: a message "suggesting
/// that you rest and meditate on what you have learned"). `Oblivion.esm` does not author the `sMeditate`
/// setting that carries it (#7049 V12), so the engine supplies the wording.
const MEDITATE: &str = "You should rest and meditate on what you've learned.";

/// UESP *Oblivion: Leveling*, Notes: what an attempt to level with every attribute at 100 says.
const PERFECTION: &str = "Perfection cannot be improved upon. Your attributes are as high as they can go.";

/// #7209 — the level-up flavour text UESP *Oblivion:Leveling* documents (Messages): levels 2 to 20, then one message for
/// every level from 21 on. `Oblivion.esm` authors only nine of them as `sLevelUp{N}` settings (4, 6, 11, 12, 15, 17,
/// 18, 19, 20, the same text); no file in the load order carries the rest, so the documented table stands in
/// for them, as [`PERFECTION`] and [`MEDITATE`] do.
const LEVEL_UP_MESSAGES: [&str; 20] = [
    "You realize that all your life you have been coasting along as if you were in a dream. Suddenly, facing the trials of the last few days, you have come alive.",
    "You realize that you are catching on to the secret of success. It's just a matter of concentration.",
    "You've done things the hard way. But without taking risks, taking responsibility for failure... how could you have understood?",
    "Everything you do is just a bit easier, more instinctive, more satisfying. It is as though you had suddenly developed keen senses and instincts.",
    "You've learned a lot about Cyrodiil... and about yourself. It's hard to believe how ignorant you were, but now you have so much more to learn.",
    "You resolve to continue pushing yourself. Perhaps there's more to you than you thought.",
    "The secret does seem to be hard work, yes, but it's also a kind of blind passion, an inspiration.",
    "So that's how it works. You plod along, putting one foot before the other, look up, and suddenly, there you are. Right where you wanted to be all along.",
    "You woke today with a new sense of purpose. You're no longer afraid of failure. Failure is just an opportunity to learn something new.",
    "Being smart doesn't hurt. And a little luck now and then is nice. But the key is patience and hard work.",
    "You can't believe how easy it is. You just have to go... a little crazy. And then, suddenly, it all makes sense, and everything you do turns to gold.",
    "It's the most amazing thing. Yesterday it was hard, and today it is easy. Just a good night's sleep, and yesterday's mysteries are today's masteries.",
    "Today you wake up, full of energy and ideas, and you know, somehow, that overnight everything has changed. What a difference a day makes.",
    "Now you just stay at your peak as long as you can. There's no one stronger in Tamriel, but there's always someone younger... a new challenger.",
    "You've been trying too hard, thinking too much. Relax. Trust your instincts. Just be yourself. Do the little things, and the big things take care of themselves.",
    "Life isn't over. You can still get smarter, or cleverer, or more experienced, or meaner... but your body and soul just aren't going to get any younger.",
    "With the life you've been living, the punishment your body has taken... there are limits, and maybe you've reached them. Is this what it's like to grow old?",
    "You're really good. Maybe the best. And that's why it's so hard to get better. But you just keep trying, because that's the way you are.",
    "By superhuman effort, you can avoid slipping backwards for a while. But one day, you'll lose a step, or drop a beat, or miss a detail... and you'll be gone forever.",
    "The results of hard work and dedication always look like luck. But you know you've earned every ounce of your success.",
];

/// The level-up message of the level just reached, after the line that names the level: for levels 2 to 20 the load
/// order's `sLevelUp{N}` flavour text when it authors one, else the documented message ([`LEVEL_UP_MESSAGES`]); from
/// level 21 on, the documented 21+ message (no setting for it is documented, so none is read).
fn level_message(world: &World, level: u16) -> String {
    let authored = (2..=20).contains(&level).then(|| {
        world
            .try_resource::<crate::components::LoadedCellIndex>()
            .and_then(|index| index.0.game_setting_string(&format!("sLevelUp{level}")).map(str::to_owned))
    });
    let documented = LEVEL_UP_MESSAGES[usize::from(level.clamp(2, 21)) - 2];
    match authored.flatten() {
        Some(text) if !text.is_empty() => format!("You are now level {level}. {text}"),
        _ => format!("You are now level {level}. {documented}"),
    }
}

/// OSL-LVL-004, OSL-SLP-001/002, OSL-API-004 (#7052): announce the levels that became pending, and take the
/// oldest pending level at each completed sleep. Only a sleep that ran to its end applies a level, so waiting
/// (which raises no completion) and an interrupted sleep change nothing. A level with fewer than three attributes
/// picked stays pending and says so.
pub(crate) fn oblivion_level_system(world: &World, _dt: f32) {
    use byroredux_scripting::oblivion_skill_use::{apply_pending_level, SleepCompletions, SleepSource};
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
        match sleep.source {
            SleepSource::Bed(sleeper) if sleeper == player => crate::oblivion_rest::offer_level(world, player),
            SleepSource::Bed(_) => {}
            SleepSource::Scripted => report_level_result(world, apply_pending_level(world, player)),
        }
    }
    if let Some(mut notices) = world.try_resource_mut::<LevelNotices>() {
        notices.announced = Some(pending_now(world));
    }
}

pub(crate) fn report_level_result(world: &World, result: byroredux_scripting::oblivion_skill_use::LevelUp) {
    use byroredux_scripting::oblivion_skill_use::LevelUp;
    match result {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #7204 — every Oblivion actor-value code the skill-use writers hard-code names the value it means in the
    /// engine-supplied roster (`OBLIVION_ACTOR_VALUES`, xEdit `wbActorValueEnum` order), so a roster correction or a
    /// mistyped code fails here instead of feeding the wrong skill with every other test still green.
    #[test]
    fn the_hard_coded_skill_codes_name_their_roster_entries() {
        use byroredux_plugin::esm::records::oblivion_actor_values::OBLIVION_ACTOR_VALUES;
        let name = |code: u32| OBLIVION_ACTOR_VALUES[code as usize];
        for (code, expected) in [
            (BLADE, "Blade"),
            (BLUNT, "Blunt"),
            (HAND_TO_HAND, "HandToHand"),
            (HEAVY_ARMOR, "HeavyArmor"),
            (LIGHT_ARMOR, "LightArmor"),
            (ATHLETICS, "Athletics"),
            (ACROBATICS, "Acrobatics"),
            (SNEAK, "Sneak"),
            (MARKSMAN, "Marksman"),
            (BLOCK, "Block"),
            (byroredux_core::character::OBLIVION_SKILL_AV_FIRST, "Armorer"),
            (byroredux_core::character::OBLIVION_SKILL_AV_LAST, "Speechcraft"),
        ] {
            assert_eq!(name(code), expected, "code {code}");
        }
        let fed: Vec<&str> = byroredux_scripting::oblivion_skill_use::FED_SKILLS.iter().map(|&code| name(code)).collect();
        assert_eq!(
            fed,
            [
                "Athletics", "Blade", "Block", "Blunt", "HandToHand", "HeavyArmor", "Alteration", "Conjuration",
                "Destruction", "Illusion", "Mysticism", "Restoration", "Acrobatics", "LightArmor", "Marksman", "Sneak",
            ]
        );
    }

    /// #7209 — every level-up has its flavour text: the load order's `sLevelUp{N}` where it authors one, the documented
    /// message where it does not (the master authors nine of the nineteen), and the 21+ message from level 21 on.
    #[test]
    fn every_level_up_has_its_flavour_text() {
        use byroredux_plugin::esm::records::{EsmIndex, GameSetting, SettingValue};
        let mut world = World::new();
        let mut index = EsmIndex::default();
        index.insert_game_setting(GameSetting { form_id: 1, editor_id: "sLevelUp4".into(), value: SettingValue::String("Authored four.".into()) });
        world.insert_resource(crate::components::LoadedCellIndex(std::sync::Arc::new(index)));
        assert_eq!(level_message(&world, 4), "You are now level 4. Authored four.");
        assert!(level_message(&world, 2).ends_with("you have come alive."), "an unauthored level reads the documented text");
        assert!(level_message(&world, 20).ends_with("gone forever."));
        assert!(level_message(&world, 21).ends_with("every ounce of your success."));
        assert_eq!(
            level_message(&world, 35).replace("level 35", "level 21"),
            level_message(&world, 21),
            "every level past 20 says the 21+ message"
        );
        assert_eq!(LEVEL_UP_MESSAGES.len(), 20, "levels 2 to 20 and 21+");
    }

    /// UESP's weapon pages: one-handed and two-handed blades feed Blade, blunts feed Blunt, the unarmed blow
    /// Hand to Hand, a bow Marksman (through its arrow only); a staff feeds no skill.
    #[test]
    fn a_weapon_type_names_its_skill() {
        assert_eq!(weapon_skill(None), Some(HAND_TO_HAND));
        assert_eq!(weapon_skill(Some(0)), Some(BLADE));
        assert_eq!(weapon_skill(Some(1)), Some(BLADE));
        assert_eq!(weapon_skill(Some(2)), Some(BLUNT));
        assert_eq!(weapon_skill(Some(3)), Some(BLUNT));
        assert_eq!(weapon_skill(Some(4)), None, "a staff casts");
        assert_eq!(weapon_skill(Some(5)), Some(MARKSMAN), "a bow: through its arrow's hit");
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
    const BOW_WEAPON: u32 = 0x102;
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
                armor_rating_x100: 0,
                equip_type: 0,
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
        for record in [weapon(BLADE_WEAPON, 1), weapon(STAFF, 4), weapon(BOW_WEAPON, 5), armor(CUIRASS, 1 << 2, true), armor(LIGHT_BOOTS, 1 << 5, false)] {
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
        on_landed_hit(&world, player, npc, false, false, false, 1);
        assert_eq!(queued(&world), vec![HAND_TO_HAND], "unarmed");
        wield(&mut world, player, BLADE_WEAPON);
        on_landed_hit(&world, player, npc, false, false, false, 2);
        assert_eq!(queued(&world), vec![HAND_TO_HAND, BLADE]);
        on_landed_hit(&world, player, npc, true, false, false, 3);
        on_landed_hit(&world, player, player, false, false, false, 4);
        wield(&mut world, player, STAFF);
        on_landed_hit(&world, player, npc, false, false, false, 5);
        assert_eq!(queued(&world).len(), 2, "blocked, self-inflicted and staff blows feed nothing");
        // The amount is the SKIL value of the action: Blade 0.5, Hand to Hand 0.6.
        let amounts: Vec<f32> = world.resource::<PendingSkillUse>().0.iter().map(|r| r.amount).collect();
        assert_eq!(amounts, vec![0.6, 0.5]);
    }

    /// Player-archery BOW-USE-001 / -002 / -005 (`bow_feeds_only_a_valid_hit`): the player's own arrow landing on a living
    /// target feeds Marksman once at usage 0 (0.8); a blocked arrow feeds nothing; and with a bow wielded, a hit that is
    /// not the arrow (a script's projectile, a gun's shot) feeds nothing at all.
    #[test]
    fn bow_feeds_only_a_valid_hit() {
        let (mut world, player, npc) = world();
        wield(&mut world, player, BOW_WEAPON);
        on_landed_hit(&world, player, npc, false, true, false, 1);
        assert_eq!(queued(&world), vec![MARKSMAN], "the arrow's hit feeds Marksman");
        assert_eq!(world.resource::<PendingSkillUse>().0[0].amount, 0.8, "the SKIL usage-0 value");
        on_landed_hit(&world, player, npc, false, false, false, 2);
        on_landed_hit(&world, player, npc, true, true, false, 3);
        on_landed_hit(&world, player, player, false, true, false, 4);
        assert_eq!(queued(&world), vec![MARKSMAN], "a non-arrow hit with a bow in hand, a blocked arrow and a self-hit feed nothing");
        // The arrow is the provenance: it feeds the bow's skill even when something else is in hand.
        wield(&mut world, player, BLADE_WEAPON);
        on_landed_hit(&world, player, npc, false, true, false, 5);
        assert_eq!(queued(&world), vec![MARKSMAN, MARKSMAN]);
        // ... and a hit that lands on the player is never the player's arrow.
        on_landed_hit(&world, npc, player, false, true, false, 6);
        assert_eq!(queued(&world).iter().filter(|skill| **skill == MARKSMAN).count(), 2);
    }

    /// #7054: a hit that meets the blocking player's guard feeds Block once at 1.25 (usage 0), beside the armor roll; a
    /// hit the player's guard did not meet, and a blow the player lands on a guarded target, feed no Block.
    #[test]
    fn a_blocked_hit_on_the_player_feeds_block() {
        let (world, player, npc) = world();
        on_landed_hit(&world, npc, player, false, false, true, 1);
        let blocks = |world: &World| world.resource::<PendingSkillUse>().0.iter().filter(|record| record.skill == BLOCK).count();
        assert_eq!(blocks(&world), 1, "the guarded hit feeds Block");
        assert_eq!(world.resource::<PendingSkillUse>().0.iter().find(|record| record.skill == BLOCK).unwrap().amount, 1.25);
        on_landed_hit(&world, npc, player, false, false, false, 2);
        assert_eq!(blocks(&world), 1, "an unguarded hit feeds none");
        on_landed_hit(&world, player, npc, false, false, true, 3);
        assert_eq!(blocks(&world), 1, "the guard is the target's: the player's own blow earns no Block");
    }

    /// BOW-USE-003 (`marksman_leaves_the_unfed_list`): code 28 is fed, in the commit that adds its emitter.
    #[test]
    fn marksman_leaves_the_unfed_list() {
        assert!(byroredux_scripting::oblivion_skill_use::FED_SKILLS.contains(&MARKSMAN));
    }

    /// OSL-USE-003: a hit on the player feeds the struck piece's skill: the heavy cuirass 25 of 100 rolls,
    /// the light boots 10, and the shield, helmet, greaves and gauntlets slots (all empty here) re-roll,
    /// so across many hits Heavy Armor outnumbers Light Armor and some hits strike nothing. An NPC's own
    /// armor, and a blow the player lands, feed no armor skill.
    #[test]
    fn a_hit_the_player_takes_feeds_the_struck_pieces_skill() {
        let (world, player, npc) = world();
        for seed in 0..400 {
            on_landed_hit(&world, npc, player, false, false, false, seed);
        }
        let heavy = queued(&world).iter().filter(|&&skill| skill == HEAVY_ARMOR).count();
        let light = queued(&world).iter().filter(|&&skill| skill == LIGHT_ARMOR).count();
        assert_eq!(heavy + light, queued(&world).len(), "only armor skills");
        assert!(heavy > light && light > 0, "heavy {heavy}, light {light}");
        assert!(heavy + light < 400, "an empty slot re-rolls, but seven empty rolls strike nothing");
        // A blocked hit never landed.
        let before = queued(&world).len();
        for seed in 0..50 {
            on_landed_hit(&world, npc, player, true, false, false, seed);
        }
        assert_eq!(queued(&world).len(), before);
    }

    /// A ruleset that does not level by skill use queues nothing, so no other game feeds Oblivion's skills.
    #[test]
    fn another_ruleset_queues_nothing() {
        let (mut world, player, npc) = world();
        world.insert_resource(CharacterRuleset::new(LevelingModel::SKYRIM));
        on_landed_hit(&world, player, npc, false, false, false, 1);
        on_landed_hit(&world, npc, player, false, false, false, 1);
        assert!(queued(&world).is_empty());
    }

    /// The engine's stance timer (#7331): it counts only while its stance holds, fires at a second and
    /// restarts; an interrupted stance keeps its count.
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

    /// The engine's stance order (#7331): sneaking undetected, else swimming, else running (never while
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

    /// #7281 — a movement key held while the controller would not move the body
    /// (dead, in the fly camera, restrained) banks no Athletics; the same key in
    /// Character mode does.
    #[test]
    fn a_held_key_banks_no_stance_use_while_the_body_cannot_move() {
        let (mut world, player, _) = world();
        world.insert_resource(StanceTimers::default());
        world.register::<byroredux_core::ecs::components::Sneaking>();
        world.register::<byroredux_core::ecs::components::Dead>();
        world.register::<byroredux_scripting::ActorControlState>();
        let mut actions = crate::interaction::ActionState::default();
        actions.press_for_test(crate::interaction::InputAction::MoveForward);
        actions.press_for_test(crate::interaction::InputAction::Sprint);
        world.insert_resource(actions);
        let run_a_second = |world: &World| {
            for _ in 0..30 {
                stance_use_system(world, 0.05);
            }
            world.resource::<PendingSkillUse>().0.len()
        };

        world.insert_resource(crate::systems::PlayerMode::FlyCam);
        assert_eq!(run_a_second(&world), 0, "fly camera");
        world.insert_resource(crate::systems::PlayerMode::Character);
        world.insert(player, byroredux_core::ecs::components::Dead);
        assert_eq!(run_a_second(&world), 0, "dead");
        world.remove::<byroredux_core::ecs::components::Dead>(player);
        world.insert(player, byroredux_scripting::ActorControlState { restrained: true, ..Default::default() });
        assert_eq!(run_a_second(&world), 0, "restrained");
        world.remove::<byroredux_scripting::ActorControlState>(player);
        assert_eq!(run_a_second(&world), 1, "a second of running in Character mode banks one use");
    }

    /// #7294 — UESP *Oblivion:Sneak*: Sneak experience accrues "only if there is a person or creature
    /// nearby". With no actor within the detection tick's range (no detector) a second of sneaking
    /// banks nothing; with one in range that has not noticed the player it banks one use.
    #[test]
    fn sneaking_with_nobody_in_range_banks_no_sneak() {
        use byroredux_core::ecs::components::Sneaking;
        let (mut world, player, npc) = world();
        world.insert_resource(StanceTimers::default());
        world.insert_resource(crate::systems::PlayerMode::Character);
        world.register::<Sneaking>();
        world.insert(player, Sneaking);
        let mut actions = crate::interaction::ActionState::default();
        actions.press_for_test(crate::interaction::InputAction::MoveForward);
        world.insert_resource(actions);
        let sneak_a_second = |world: &World| {
            for _ in 0..30 {
                stance_use_system(world, 0.05);
            }
            world.resource::<PendingSkillUse>().0.iter().filter(|r| r.skill == SNEAK).count()
        };
        world.insert_resource(crate::detection::PlayerDetection::default());
        assert_eq!(sneak_a_second(&world), 0, "an empty room");
        world.insert_resource(crate::detection::PlayerDetection { detector: Some(npc), ..Default::default() });
        assert_eq!(sneak_a_second(&world), 1, "someone in range, unaware");
    }

    /// OSL-USE-005 end to end: a player running for a second queues one Athletics use at 0.03 (value 1),
    /// sneaking undetected 0.75 of Sneak, and a jump 0.3 of Acrobatics; swimming would use value 2.
    #[test]
    fn seconds_of_a_stance_and_a_jump_queue_their_skil_amounts() {
        use byroredux_core::ecs::components::Sneaking;
        let (mut world, player, npc) = world();
        world.insert_resource(StanceTimers::default());
        world.insert_resource(crate::systems::PlayerMode::Character);
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
        // Someone within detection range who has not noticed the player (#7294).
        world.insert_resource(crate::detection::PlayerDetection { detector: Some(npc), ..Default::default() });
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
            byroredux_scripting::oblivion_skill_use::SleepCompleted {
                hours: 8.0, source: byroredux_scripting::oblivion_skill_use::SleepSource::Scripted, interrupted,
            },
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
        raise_skill_ranks(&world, player, BLADE, 10);
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

    /// #7196 — a load order's level-up settings reach the level taken: `iLevelUpSkillCount` 5 makes five major
    /// increases a level, `iLevelUp05Mult` 4 is the bonus for Blade's five rank-ups, and `fStatsHealthLevelMult` 0.2
    /// is the Health share of the Endurance after the picks.
    #[test]
    fn a_retuned_level_up_follows_the_load_orders_settings() {
        use byroredux_core::character::{CharacterRuleset, LevelingModel};
        use byroredux_scripting::oblivion_skill_use::{pick_attribute, raise_skill_ranks};
        let (mut world, player) = level_world();
        world.insert_resource(CharacterRuleset::new(LevelingModel::OBLIVION.with_gmst(|name| match name {
            "iLevelUpSkillCount" => Some(5.0),
            "iLevelUp05Mult" => Some(4.0),
            "fStatsHealthLevelMult" => Some(0.2),
            _ => None,
        })));
        oblivion_level_system(&world, 0.0);
        raise_skill_ranks(&world, player, BLADE, 5);
        assert_eq!(pick_attribute(&world, player, 0).unwrap().bonus, 4, "Strength's five rank-ups: iLevelUp05Mult");
        for attribute in [5, 7] {
            pick_attribute(&world, player, attribute).unwrap();
        }
        sleep(&world, false);
        oblivion_level_system(&world, 0.0);
        assert_eq!(level_of(&world, player), 2, "five major increases made the level");
        let names = world.resource::<byroredux_scripting::ActorValueNames>();
        let (health, strength) = (names.resolve("Health").unwrap(), names.resolve("Strength").unwrap());
        drop(names);
        let values = world.get::<byroredux_core::ecs::components::ActorValues>(player).unwrap();
        assert_eq!(values.get_base(strength).unwrap().base, 44.0);
        assert_eq!(values.stored(health), 8.0, "0.2 of Endurance 41, rounded down");
    }

    /// A level whose attributes are not all picked stays pending and the sleep says what to do; another sleep after
    /// the picks takes it.
    #[test]
    fn an_unpicked_level_waits_for_its_picks() {
        use byroredux_scripting::oblivion_skill_use::{pick_attribute, raise_skill_ranks};
        let (world, player) = level_world();
        oblivion_level_system(&world, 0.0);
        raise_skill_ranks(&world, player, BLADE, 10);
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
        raise_skill_ranks(&world, player, BLADE, 10);
        oblivion_level_system(&world, 0.0);
        assert!(crate::notifications::drain(&world).is_empty());
    }

    fn rest_world() -> (World, EntityId, EntityId) {
        use byroredux_core::ecs::components::{Furniture, FurnitureMarker, FurnitureMarkerKind};
        let (mut world, player) = level_world();
        world.insert_resource(crate::oblivion_rest::RestSession::default());
        world.insert_resource(crate::components::GameTimeRes::frozen_at(23.0));
        world.insert_resource(byroredux_scripting::obscript_more::PendingSleep::default());
        world.insert_resource(byroredux_scripting::papyrus_demo::PapyrusPlayerEntity(player));
        let bed = world.spawn();
        world.insert(bed, Furniture { markers: vec![FurnitureMarker {
            local_offset: [0.0; 3], heading_z_radians: None, animation_type: 2, kind: FurnitureMarkerKind::Sleep,
        }] });
        world.insert(bed, byroredux_scripting::ActivateEvent { activator: player });
        (world, player, bed)
    }

    #[test]
    fn oblivion_bed_sleep_offers_only_the_oldest_pending_level() {
        use byroredux_core::character::SkillUseLevel;
        use byroredux_native_ui::{RestAction::*, RestView};
        use byroredux_scripting::oblivion_skill_use::{SleepCompletions, SleepSource, raise_skill_ranks};
        use crate::oblivion_rest::{apply, bed_use_system, snapshot};
        let (world, player, _) = rest_world();
        raise_skill_ranks(&world, player, BLADE, 10);
        byroredux_scripting::oblivion_skill_use::advance_level(&world, player);
        assert_eq!(world.get::<SkillUseLevel>(player).unwrap().pending.len(), 2);

        bed_use_system(&world, 0.0);
        assert_eq!(snapshot(&world), Some(RestView::Duration { sleeping: true, hours: 1 }));
        apply(&world, Confirm);
        apply(&world, Confirm);
        crate::systems::script_requests::sleep_system(&world, 0.0);
        {
            let completions = world.resource::<SleepCompletions>();
            assert_eq!(completions.0.len(), 1);
            let sleep = completions.0[0];
            assert_eq!((sleep.source, sleep.hours, sleep.interrupted), (SleepSource::Bed(player), 1.0, false));
        }
        assert_eq!(world.resource::<crate::components::GameTimeRes>().absolute_hours(), 24.0);
        oblivion_level_system(&world, 0.0);
        let Some(RestView::LevelUp { attributes, .. }) = snapshot(&world) else { panic!("level-up offer") };
        assert_eq!(attributes[0].bonus, 5);
        for code in [0, 5, 6, 6, 7] {
            apply(&world, ToggleAttribute(code));
        }
        assert_eq!(level_of(&world, player), 1);
        apply(&world, Confirm);
        apply(&world, Confirm);
        assert_eq!(level_of(&world, player), 2);
        assert_eq!(world.get::<SkillUseLevel>(player).unwrap().pending.len(), 1);
        let av = |code| byroredux_scripting::obscript_actor_values::actor_value_form(&world, code).unwrap();
        let forms = [av(0.0), av(5.0), av(6.0), av(7.0), av(8.0)];
        let values = world.get::<byroredux_core::ecs::components::ActorValues>(player).unwrap();
        assert_eq!(forms[..4].iter().map(|&form| values.get_base(form).unwrap().base).collect::<Vec<_>>(), [45.0, 41.0, 40.0, 41.0]);
        assert_eq!(values.stored(forms[4]), 4.0);
    }

    #[test]
    fn oblivion_rest_cancel_wait_and_interrupt_keep_a_pending_level() {
        use byroredux_native_ui::{RestAction::*, RestView};
        use byroredux_scripting::oblivion_skill_use::{SleepCompletions, raise_skill_ranks};
        use crate::oblivion_rest::{apply, bed_use_system, request_wait, snapshot};
        let (mut world, player, bed) = rest_world();
        raise_skill_ranks(&world, player, BLADE, 10);
        let clock = |world: &World| world.resource::<crate::components::GameTimeRes>().absolute_hours();
        let unchanged = |world: &World| {
            oblivion_level_system(world, 0.0);
            assert!(snapshot(world).is_none());
            assert_eq!(level_of(world, player), 1);
            assert_eq!(world.get::<byroredux_core::character::SkillUseLevel>(player).unwrap().pending.len(), 1);
        };
        bed_use_system(&world, 0.0);
        apply(&world, Cancel);
        crate::systems::script_requests::sleep_system(&world, 0.0);
        assert_eq!(clock(&world), 23.0);
        unchanged(&world);

        request_wait(&world);
        assert_eq!(snapshot(&world), Some(RestView::Duration { sleeping: false, hours: 1 }));
        apply(&world, SetHours(2));
        apply(&world, Confirm);
        crate::systems::script_requests::sleep_system(&world, 0.0);
        assert_eq!(clock(&world), 25.0);
        assert!(world.resource::<SleepCompletions>().0.is_empty());
        unchanged(&world);

        bed_use_system(&world, 0.0);
        apply(&world, Confirm);
        let enemy = world.spawn();
        world.insert(enemy, byroredux_scripting::AiCombatState { target: player, attack_cooldown_remaining: 0.0 });
        crate::systems::script_requests::sleep_system(&world, 0.0);
        assert_eq!(clock(&world), 25.0);
        assert!(world.resource::<SleepCompletions>().0[0].interrupted);
        unchanged(&world);

        world.despawn(enemy);
        world.insert(bed, byroredux_core::ecs::components::Owned { owner_form_id: 0x333, faction_rank: None, global_var_form_id: None });
        bed_use_system(&world, 0.0);
        assert!(snapshot(&world).is_none(), "third-party bed");

        world.remove::<byroredux_core::ecs::components::Owned>(bed);
        world.get_mut::<byroredux_core::character::SkillUseLevel>(player).unwrap().pending.clear();
        bed_use_system(&world, 0.0);
        apply(&world, Confirm);
        crate::systems::script_requests::sleep_system(&world, 0.0);
        assert_eq!(clock(&world), 26.0);
        assert_eq!(world.resource::<SleepCompletions>().0.len(), 1);
        oblivion_level_system(&world, 0.0);
        assert!(snapshot(&world).is_none(), "bed sleep with no pending level returns to play");
        assert_eq!(level_of(&world, player), 1);
    }
}
