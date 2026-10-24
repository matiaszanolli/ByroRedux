//! Guards and the player's bounty, as the legacy condition functions and
//! ObScript getters ask for them (Oblivion).
//!
//! CS *IsGuard*: "1 if the NPC's class is marked as a Guard class" — the
//! `CLAS` `DATA` flags' Guard bit (Oblivion, FO3 and FNV author it), so which classes are guards is the load
//! order's data ([`GuardClasses`], re-derived from the records every boot),
//! not a list here. A mod that flags another class as a guard class makes its
//! members guards.
//!
//! The bounty is the saved [`ObScriptLedger`]'s `crime_gold`. Only the player
//! accrues one (nothing in the engine raises an NPC's), so every other actor's
//! answers 0.

use std::collections::HashSet;
use std::sync::Arc;

use byroredux_core::character::Background;
use byroredux_core::ecs::components::Inventory;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

use crate::obscript_ledger::ObScriptLedger;

/// The base form of the currency item (`Gold001`, `0x0F` of every Bethesda
/// master that has one).
pub const CURRENCY_FORM: u32 = 0x0000_000F;

/// The `CLAS` forms the load order marks as guard classes.
#[derive(Debug, Default, Clone)]
pub struct GuardClasses(Arc<HashSet<u32>>);

impl Resource for GuardClasses {}

/// Install the load order's guard classes (empty on Skyrim and later, whose
/// classes carry no flags word).
pub fn install_guard_classes(world: &mut World, index: &EsmIndex) {
    let classes = index
        .classes
        .iter()
        .filter(|(_, class)| class.is_guard())
        .map(|(&form, _)| form)
        .collect();
    world.insert_resource(GuardClasses(Arc::new(classes)));
}

/// Whether `actor`'s class is a guard class.
pub fn is_guard(world: &World, actor: EntityId) -> bool {
    let Some(background) = world.get::<Background>(actor) else {
        return false;
    };
    world
        .try_resource::<GuardClasses>()
        .is_some_and(|classes| classes.0.contains(&background.class_form_id))
}

/// `actor`'s bounty: the player's ledger value; 0 for anyone else.
pub fn bounty_of(world: &World, actor: EntityId) -> i32 {
    let is_player = crate::papyrus_demo::PapyrusPlayerEntity::get(world) == Some(actor);
    if !is_player {
        return 0;
    }
    world.try_resource::<ObScriptLedger>().map_or(0, |ledger| ledger.crime_gold)
}

/// The player's bounty (0 with no player spawned).
pub fn player_bounty(world: &World) -> i32 {
    crate::papyrus_demo::PapyrusPlayerEntity::get(world).map_or(0, |player| bounty_of(world, player))
}

/// CS *GetCrime*: `criminal` committed a `kind` crime against `actor` and `actor` knows of it. The
/// crime is "against the calling actor" when its victim is the actor's reference or its base (a
/// theft from an actor's property records the base). `kind` is `None` for a crime-type number
/// the engine does not carry, which nothing is recorded under.
pub fn knows_crime_against(world: &World, actor: EntityId, criminal: u32, kind: Option<byroredux_core::character::CrimeKind>) -> bool {
    let (Some(kind), Some(knower)) = (kind, crate::obscript_state::reference_form(world, actor)) else {
        return false;
    };
    let base = world.get::<crate::scene::SceneAliasCandidate>(actor).map_or(0, |candidate| candidate.base_form_id);
    world
        .try_resource::<crate::crime_knowledge::CrimeKnowledge>()
        .is_some_and(|knowledge| knowledge.knows_against_any(knower, criminal, kind, &[knower, base]))
}

/// CS *CanPayCrimeGold*: whether `actor` holds at least as much gold as their
/// bounty.
pub fn can_pay_bounty(world: &World, actor: EntityId) -> bool {
    let gold: u32 = world.get::<Inventory>(actor).map_or(0, |inventory| {
        inventory
            .items
            .iter()
            .filter(|stack| stack.base_form_id == CURRENCY_FORM)
            .map(|stack| stack.count)
            .sum()
    });
    i64::from(gold) >= i64::from(bounty_of(world, actor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::ItemStack;
    use byroredux_plugin::esm::records::ClassRecord;

    fn class(flags: Option<u32>) -> ClassRecord {
        ClassRecord {
            form_id: 0,
            editor_id: String::new(),
            full_name: String::new(),
            description: String::new(),
            base_attributes: [0; 7],
            skyrim_skill_weights: None,
            skyrim_pool_weights: None,
            tag_skills: Vec::new(),
            primary_attributes: None,
            specialization: None,
            major_skills: Vec::new(),
            flags_oblivion: flags,
            class_flags: flags,
        }
    }

    #[test]
    fn a_guard_is_an_actor_whose_class_carries_the_guard_flag() {
        let mut index = EsmIndex::default();
        index.classes.insert(0x10, class(Some(0b11))); // Playable | Guard
        index.classes.insert(0x11, class(Some(0b01))); // Playable only
        index.classes.insert(0x12, class(None)); // Skyrim's: no flags word
        let mut world = World::new();
        world.register::<Background>();
        install_guard_classes(&mut world, &index);
        let spawn = |world: &mut World, class_form_id| {
            let actor = world.spawn();
            world.insert(actor, Background { race_form_id: 1, class_form_id });
            actor
        };
        let (guard, citizen, foreign) = (spawn(&mut world, 0x10), spawn(&mut world, 0x11), spawn(&mut world, 0x12));
        assert!(is_guard(&world, guard));
        assert!(!is_guard(&world, citizen) && !is_guard(&world, foreign));
        let classless = world.spawn();
        assert!(!is_guard(&world, classless), "no class, no guard");
    }

    #[test]
    fn only_the_player_has_a_bounty_and_paying_needs_the_gold() {
        let mut world = World::new();
        world.register::<Inventory>();
        let player = world.spawn();
        let other = world.spawn();
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        let mut ledger = ObScriptLedger::default();
        ledger.crime_gold = 40;
        world.insert_resource(ledger);
        assert_eq!((bounty_of(&world, player), bounty_of(&world, other)), (40, 0));
        world.insert(player, Inventory { items: vec![ItemStack::new(CURRENCY_FORM, 39)] });
        assert!(!can_pay_bounty(&world, player), "39 gold against 40");
        world.get_mut::<Inventory>(player).unwrap().items.push(ItemStack::new(CURRENCY_FORM, 1));
        assert!(can_pay_bounty(&world, player));
    }
}
