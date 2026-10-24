//! What the player's crimes cost them (CHARAL crime family).
//!
//! Theft, assault and murder are wired. The rules are Oblivion's (CS wiki
//! *Crime Overview*, *fCrimeGoldSteal*, *iCrimeGoldXX*; UESP
//! *Oblivion:Crime*, *Oblivion:Infamy*):
//!
//! * **Theft** from a faction marks that faction ("this does not have to be a
//!   crime that was witnessed by anyone", `GetPCFactionSteal`), and a
//!   *witnessed* theft adds `item value × fCrimeGoldSteal` to the bounty.
//! * **Assault** is striking a non-hostile NPC; a witnessed one adds
//!   `iCrimeGoldAttack`. It is charged once per victim: the original turns
//!   the victim hostile, and a hostile victim is self-defence, which "is not
//!   a crime". A kill in one hit is only a murder ("you will only receive a
//!   bounty for murder").
//! * **Murder** is killing a non-hostile NPC; a witnessed one adds
//!   `iCrimeGoldMurder`. The victim's factions get the Murder flag whoever
//!   saw it (`GetPCFactionMurder`).
//! * Any crime that adds 1000 gold earns one infamy point per 1000.
//!
//! Everything lands in the saved [`ObScriptLedger`] that `GetCrimeGold` /
//! `GetPCInfamy` / `GetPCFaction*` already read, so scripts and the engine
//! see one bounty.
//!
//! The per-game seam is data: [`CrimeModel`] is installed only for a game
//! whose profile carries one, and without it a theft is classified and
//! announced but costs nothing.
//!
//! Not modelled yet: sneaking undetected (a witness is any living NPC who
//! can see the player at detection range, with no stealth roll), the
//! witnesses' disposition drop, guards arresting or paying the fine, a
//! guard kill that incurs a bounty with no witness, hostile spell effects
//! as assault, creatures (domestic ones are murder victims in the original)
//! as victims, and Shivering Isles' separate bounty.
//!
//! "Non-hostile" is the hostility pass's own judgement
//! (`faction_hostility::actor_would_attack`) plus not already fighting the player; "NPC" is
//! an actor without [`CreatureActor`].

use byroredux_core::character::CrimeModel;
use byroredux_core::ecs::components::{CreatureActor, Dead, FactionRanks};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
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
    charge(world, model, model.theft_bounty(value), "stealing")
}

/// Add `bounty` to the player's ledger with the infamy it earns, and say so.
fn charge(world: &World, model: CrimeModel, bounty: i32, doing: &str) -> i32 {
    let total = world.try_resource_mut::<ObScriptLedger>().map(|mut ledger| {
        ledger.mod_crime_gold(bounty);
        ledger.infamy += model.infamy_for(bounty);
        ledger.crime_gold
    });
    if let Some(total) = total {
        crate::notifications::push(world, format!("You were seen {doing}. Your bounty is now {total} gold"));
    }
    bounty
}

/// Marks an NPC the player has already assaulted, so the hits that follow
/// are not charged again. Transient: a loaded save forgets it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AssaultedByPlayer;
impl Component for AssaultedByPlayer {
    type Storage = SparseSetStorage<Self>;
}

/// Whether a strike by `aggressor` on `victim` is a crime against a
/// non-hostile NPC. Judge it **before** the strike lands: a kill clears the
/// victim's combat state. Cheap for the common case (an NPC's own strike).
pub(crate) fn player_strike_is_crime(world: &World, aggressor: EntityId, victim: EntityId) -> bool {
    if world.try_resource::<CrimeModel>().is_none() {
        return false;
    }
    let player = world.try_resource::<crate::systems::PlayerEntity>().and_then(|player| player.0);
    if player != Some(aggressor) || victim == aggressor || world.get::<CreatureActor>(victim).is_some() {
        return false;
    }
    let fighting_player = world
        .get::<byroredux_scripting::AiCombatState>(victim)
        .is_some_and(|state| state.target == aggressor);
    !fighting_player && !crate::systems::faction_hostility::actor_would_attack(world, victim, aggressor)
}

/// Charge the player for a strike [`player_strike_is_crime`] judged a crime;
/// `killed` makes it a murder. Returns the bounty added.
pub(crate) fn report_strike(world: &World, player: EntityId, victim: EntityId, killed: bool) -> i32 {
    let Some(model) = world.try_resource::<CrimeModel>().map(|model| *model) else {
        return 0;
    };
    let assaulted_before = world.get::<AssaultedByPlayer>(victim).is_some();
    if !killed && assaulted_before {
        return 0;
    }
    if !assaulted_before {
        if let Some(mut marks) = world.query_mut::<AssaultedByPlayer>() {
            marks.insert(victim, AssaultedByPlayer);
        }
    }
    let victim_factions: Vec<u32> = world
        .get::<FactionRanks>(victim)
        .map(|ranks| ranks.0.iter().filter(|(_, rank)| *rank >= 0).map(|(faction, _)| *faction).collect())
        .unwrap_or_default();
    if let Some(mut ledger) = world.try_resource_mut::<ObScriptLedger>() {
        let kind = if killed { FactionFlag::Murder } else { FactionFlag::Attack };
        let player_ranks = world.get::<FactionRanks>(player);
        for &faction in &victim_factions {
            // GetPCFactionAttack: "the Player must also be a member of the specified faction".
            let flagged = killed || player_ranks.as_ref().is_some_and(|ranks| ranks.rank(faction).is_some_and(|r| r >= 0));
            if flagged {
                ledger.set_flag(kind, faction, true);
            }
        }
    }
    if !witnessed(world, player) {
        return 0;
    }
    if killed {
        charge(world, model, model.murder_gold, "committing murder")
    } else {
        charge(world, model, model.attack_gold, "assaulting someone")
    }
}
