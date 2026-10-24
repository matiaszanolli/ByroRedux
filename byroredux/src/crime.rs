//! What the player's crimes cost them (CHARAL crime family).
//!
//! Theft is the first crime wired. The rule is Oblivion's (CS wiki
//! *fCrimeGoldSteal*, UESP *Oblivion:Crime*): stealing from a faction marks
//! that faction ("this does not have to be a crime that was witnessed by
//! anyone", `GetPCFactionSteal`), and a *witnessed* theft adds
//! `item value × fCrimeGoldSteal` to the bounty. Both land in the saved
//! [`ObScriptLedger`] that `GetCrimeGold` / `GetPCFactionSteal` already read,
//! so scripts and the engine see one bounty.
//!
//! The per-game seam is data: [`CrimeModel`] is installed only for a game
//! whose profile carries one, and without it a theft is classified and
//! announced but costs nothing.
//!
//! Not modelled yet: sneaking undetected (a witness is any living NPC who
//! can see the player at detection range, with no stealth roll), the
//! witnesses' disposition drop, guards arresting or paying the fine, assault
//! and murder fines, and Shivering Isles' separate bounty.

use byroredux_core::character::CrimeModel;
use byroredux_core::ecs::components::{CreatureActor, Dead};
use byroredux_core::ecs::{EntityId, World};
use byroredux_plugin::esm::records::EsmIndex;
use byroredux_scripting::obscript_ledger::{FactionFlag, ObScriptLedger};

/// Install (or clear) the [`CrimeModel`] for a freshly loaded index.
pub(crate) fn install_crime_model(world: &mut World, index: &EsmIndex) {
    match index.character_rules.crime() {
        Some(model) => {
            world.insert_resource(model.with_gmst(|name| index.game_setting_float(name)));
        }
        None => {
            world.remove_resource::<CrimeModel>();
        }
    }
}

/// Whether any living NPC can see the player: the people a crime is
/// reported by. Creatures do not report crimes here.
fn witnessed(world: &World, player: EntityId) -> bool {
    let observers: Vec<EntityId> = world
        .query::<crate::systems::faction_hostility::CombatDisposition>()
        .map(|q| q.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    observers.into_iter().any(|observer| {
        observer != player
            && world.get::<Dead>(observer).is_none()
            && world.get::<CreatureActor>(observer).is_none()
            && crate::systems::faction_hostility::actor_detects(world, observer, player)
    })
}

/// Charge the player for a theft of `rows` (`(base form, count)`) from
/// property owned by `owner`. Returns the bounty added (0 when unwitnessed
/// or the game has no crime model).
pub(crate) fn report_theft(world: &World, player: EntityId, owner: u32, rows: &[(u32, u32)]) -> i32 {
    let Some(model) = world.try_resource::<CrimeModel>().map(|model| *model) else {
        return 0;
    };
    // The faction flag does not need a witness.
    if byroredux_scripting::is_faction(world, owner) {
        if let Some(mut ledger) = world.try_resource_mut::<ObScriptLedger>() {
            ledger.set_flag(FactionFlag::Steal, owner, true);
        }
    }
    if !witnessed(world, player) {
        return 0;
    }
    let value: u64 = {
        let catalog = world.try_resource::<crate::inventory::InventoryCatalog>();
        rows.iter()
            .map(|&(base, count)| {
                let each = catalog.as_ref().map_or(0, |catalog| catalog.item_value(base));
                u64::from(each) * u64::from(count)
            })
            .sum()
    };
    let bounty = model.theft_bounty(value);
    let total = world.try_resource_mut::<ObScriptLedger>().map(|mut ledger| {
        ledger.mod_crime_gold(bounty);
        ledger.crime_gold
    });
    if let Some(total) = total {
        crate::notifications::push(world, format!("You were seen stealing. Your bounty is now {total} gold"));
    }
    bounty
}
