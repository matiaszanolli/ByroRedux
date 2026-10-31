//! What the player's crimes cost them (CHARAL crime family).
//!
//! Two layers. **Every game** classifies a crime (theft from an owner, a strike on someone
//! who is not fighting you) and records who saw it in the saved `CrimeKnowledge`, which
//! `GetCrime` / `GetCrimeKnown` read. **A game with a [`CrimeModel`]** (Oblivion and Skyrim so
//! far; the profile row says so) then fines it, by its [`BountyScope`]: Oblivion keeps one bounty
//! on the player (flags the factions, sends guards, jails); Skyrim reports to each witness's crime
//! faction and fines what that faction's Crime tab says ([`charge_crime_factions`]). Theft,
//! assault and murder are wired. The Oblivion rules below are the CS wiki's (CS wiki
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
//! * A witnessed crime **calls the guards** ([`raise_alarm`]): the guards (class Guard flag)
//!   within `iCrimeAlarmRecDistance` of a witness, the `iNumberGuardsCrimeResponse` nearest
//!   the player, learn of it and walk up to open a greeting — the force-greet directive the
//!   dialogue runtime already runs — which the arrest greeting (`IsTurnArrest`) answers.
//! * A **wanted player is rushed on sight** ([`make_guard_rush_system`]): a guard who sees the
//!   player with a bounty at or past `iCrimeGoldAttackMin` comes for them the same way.
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
//! witnesses' disposition drop, a guard kill that incurs a bounty with no witness, hostile spell effects
//! as assault, creatures (domestic ones are murder victims in the original)
//! as victims, and Shivering Isles' separate bounty.
//!
//! "Non-hostile" is the hostility pass's own judgement
//! (`faction_hostility::actor_would_attack`) plus not already fighting the player; "NPC" is
//! an actor without [`CreatureActor`].

use byroredux_core::character::{BountyScope, CrimeKind, CrimeModel};
use byroredux_core::ecs::components::{CreatureActor, Dead, FactionRanks, GlobalTransform};
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

/// The living actors who can see the player: the people a crime is reported by. Creatures
/// witness only where the game's model says they do (Skyrim's animals do, Oblivion's NPCs only).
fn witnesses(world: &World, player: EntityId) -> Vec<EntityId> {
    let creatures = world.try_resource::<CrimeModel>().is_some_and(|model| model.creatures_witness);
    let observers: Vec<EntityId> = world
        .query::<crate::systems::faction_hostility::CombatDisposition>()
        .map(|q| q.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    observers
        .into_iter()
        .filter(|&observer| {
            observer != player
                && world.get::<Dead>(observer).is_none()
                && (creatures || world.get::<CreatureActor>(observer).is_none())
                && crate::systems::faction_hostility::actor_detects(world, observer, player)
        })
        .collect()
}

/// The crime factions the `witnesses` report to: each witness's own (CK *Crime*: "crimes are
/// reported to an Actor's Crime Faction"), once.
fn reported_to(world: &World, witnesses: &[EntityId]) -> Vec<u32> {
    let mut factions: Vec<u32> = witnesses
        .iter()
        .filter_map(|&witness| {
            let reference = byroredux_scripting::obscript_state::reference_form(world, witness)?;
            byroredux_scripting::crime_faction(world, witness, reference)
        })
        .collect();
    factions.sort_unstable();
    factions.dedup();
    factions
}

/// Add what `kind` fines (a theft of `theft_value` gold) to the bounty of every crime faction a
/// witness reports to — Skyrim's per-hold bounty — and say so. Returns the gold added in total.
fn charge_crime_factions(world: &World, model: CrimeModel, kind: CrimeKind, witnesses: &[EntityId], theft_value: u64) -> i32 {
    use byroredux_scripting::{CrimeGoldChange, FactionCrimeGold};
    let mut total = 0;
    for faction in reported_to(world, witnesses) {
        let Some((violent, amount)) = world
            .try_resource::<byroredux_scripting::crime_faction::CrimeFactionData>()
            .and_then(|data| data.get(faction).and_then(|crime| byroredux_scripting::crime_faction::faction_fine(crime, &model, kind, theft_value)))
        else {
            continue;
        };
        if amount <= 0 {
            continue;
        }
        if let Some(mut gold) = world.try_resource_mut::<FactionCrimeGold>() {
            gold.apply(CrimeGoldChange::Mod { faction, violent, amount });
        }
        total += amount;
        let name = world
            .try_resource::<crate::cell_loader::LoadedCellIndex>()
            .and_then(|index| index.0.factions.get(&faction).map(|record| record.full_name.clone()))
            .filter(|name| !name.is_empty());
        crate::notifications::push(world, format!("{amount} Gold added to {}", name.as_deref().unwrap_or("your bounty")));
    }
    total
}

/// The player committed a `kind` crime against `victim` in sight of `witnesses`: each of them
/// now knows of it (`GetCrime` / `GetCrimeKnown`). Every game records this, whatever it then
/// does about it.
fn record_crime(world: &World, kind: CrimeKind, victim: u32, witnesses: &[EntityId]) {
    for &witness in witnesses {
        byroredux_scripting::crime_knowledge::note_known(world, witness, byroredux_scripting::PLAYER_REFERENCE, kind, victim);
    }
}

/// The reference form `entity` is known by, 0 when it has none.
fn reference_of(world: &World, entity: EntityId) -> u32 {
    byroredux_scripting::obscript_state::reference_form(world, entity).unwrap_or(0)
}

/// The witnesses call for guards. The guards among the NPCs within `iCrimeAlarmRecDistance` of a
/// witness ("the distance from which a crime victim's cries for help can be heard"), the
/// `iNumberGuardsCrimeResponse` nearest the player ("the number of guards that appear when the
/// player commits a crime and it has been reported"), learn of the crime and come to the player:
/// each is stamped with the force-greet directive the dialogue runtime already runs, so it walks
/// up and opens its generic greeting, which an arrest greeting (`IsTurnArrest`) answers.
fn raise_alarm(world: &World, model: CrimeModel, player: EntityId, kind: CrimeKind, victim: u32, witnesses: &[EntityId]) {
    use crate::systems::forcegreet::{ForceGreetDirective, FORCE_GREET_RADIUS};
    // Skyrim's guards are an AI package keyed on `IsGuardFaction`, not this class-flag response.
    if model.scope != BountyScope::Player {
        return;
    }
    let position = |entity: EntityId| world.get::<GlobalTransform>(entity).map(|t| t.translation);
    let (Some(at_player), true) = (position(player), model.guards_responding > 0) else {
        return;
    };
    let heard_from: Vec<_> = witnesses.iter().filter_map(|&w| position(w)).collect();
    let others: Vec<EntityId> = world
        .query::<crate::systems::faction_hostility::CombatDisposition>()
        .map(|q| q.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    let mut guards: Vec<(f32, EntityId)> = others
        .into_iter()
        .filter(|&actor| {
            actor != player
                && byroredux_scripting::guards::is_guard(world, actor)
                && world.get::<Dead>(actor).is_none()
                && world.get::<CreatureActor>(actor).is_none()
                && world.get::<byroredux_scripting::AiCombatState>(actor).is_none()
        })
        .filter_map(|actor| {
            let here = position(actor)?;
            heard_from.iter().any(|w| w.distance(here) <= model.alarm_distance).then_some((here.distance(at_player), actor))
        })
        .collect();
    guards.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, guard) in guards.into_iter().take(model.guards_responding as usize) {
        byroredux_scripting::crime_knowledge::note_known(world, guard, byroredux_scripting::PLAYER_REFERENCE, kind, victim);
        if let Some(mut directives) = world.query_mut::<ForceGreetDirective>() {
            if directives.get(guard).is_none() {
                directives.insert(guard, ForceGreetDirective { topic: None, radius: FORCE_GREET_RADIUS });
            }
        }
    }
}

/// The player took `rows` (`(base form, count)`) from property owned by `owner`. Every game
/// records who saw the theft; where the game has a [`CrimeModel`] the goods are on record as
/// stolen, the owner faction is marked, and a witnessed theft adds its bounty and calls the
/// guards. Returns the bounty added (0 when unwitnessed or the game has no model).
pub(crate) fn report_theft(world: &World, player: EntityId, owner: u32, rows: &[(u32, u32)]) -> i32 {
    let seen_by = witnesses(world, player);
    record_crime(world, CrimeKind::Steal, owner, &seen_by);
    let Some(model) = world.try_resource::<CrimeModel>().map(|model| *model) else {
        return 0;
    };
    let value: u64 = {
        let catalog = world.try_resource::<crate::inventory::InventoryCatalog>();
        rows.iter()
            .map(|&(base, count)| {
                let each = catalog.as_ref().map_or(0, |catalog| catalog.item_value(base));
                u64::from(each) * u64::from(count)
            })
            .sum()
    };
    if model.scope == BountyScope::CrimeFaction {
        return charge_crime_factions(world, model, CrimeKind::Steal, &seen_by, value);
    }
    // The player now holds these stolen: `PayFine` / `GoToJail` take them back.
    if let Some(mut ledger) = world.try_resource_mut::<ObScriptLedger>() {
        for &(base, count) in rows {
            ledger.note_stolen(base, count);
        }
    }
    // The faction flag does not need a witness.
    if byroredux_scripting::is_faction(world, owner) {
        if let Some(mut ledger) = world.try_resource_mut::<ObScriptLedger>() {
            ledger.set_flag(FactionFlag::Steal, owner, true);
        }
    }
    if seen_by.is_empty() {
        return 0;
    }
    let bounty = charge(world, model, model.theft_bounty(value), "stealing");
    raise_alarm(world, model, player, CrimeKind::Steal, owner, &seen_by);
    bounty
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

/// Whether a strike by `aggressor` on `victim` is a crime against a non-hostile NPC (every
/// game: striking someone who is not fighting you). Judge it **before** the strike lands: a kill
/// clears the victim's combat state. Cheap for the common case (an NPC's own strike).
pub(crate) fn player_strike_is_crime(world: &World, aggressor: EntityId, victim: EntityId) -> bool {
    let player = crate::systems::PlayerEntity::get(world);
    if player != Some(aggressor) || victim == aggressor || world.get::<CreatureActor>(victim).is_some() {
        return false;
    }
    let fighting_player = world
        .get::<byroredux_scripting::AiCombatState>(victim)
        .is_some_and(|state| state.target == aggressor);
    !fighting_player && !crate::systems::faction_hostility::actor_would_attack(world, victim, aggressor)
}

/// The player struck `victim`, which [`player_strike_is_crime`] judged a crime; `killed` makes it
/// a murder. Every game records who saw it; where the game has a [`CrimeModel`] the victim's
/// factions are flagged, and a witnessed strike adds its bounty and calls the guards. Returns the
/// bounty added.
pub(crate) fn report_strike(world: &World, player: EntityId, victim: EntityId, killed: bool) -> i32 {
    let kind = if killed { CrimeKind::Murder } else { CrimeKind::Attack };
    let seen_by = witnesses(world, player);
    let victim_ref = reference_of(world, victim);
    record_crime(world, kind, victim_ref, &seen_by);
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
        // M43 — the Story Manager's `ASSU` (Assault Actor) event, once per victim like the
        // charge (`report_strike` is only called for a strike judged a crime).
        crate::systems::story_events::raise_assault_story_event(world, victim, player, true);
    }
    if model.scope == BountyScope::CrimeFaction {
        return charge_crime_factions(world, model, kind, &seen_by, 0);
    }
    let victim_factions: Vec<u32> = world
        .get::<FactionRanks>(victim)
        .map(|ranks| ranks.0.iter().filter(|(_, rank)| *rank >= 0).map(|(faction, _)| *faction).collect())
        .unwrap_or_default();
    if let Some(mut ledger) = world.try_resource_mut::<ObScriptLedger>() {
        let flag = if killed { FactionFlag::Murder } else { FactionFlag::Attack };
        let player_ranks = world.get::<FactionRanks>(player);
        for &faction in &victim_factions {
            // GetPCFactionAttack: "the Player must also be a member of the specified faction".
            let flagged = killed || player_ranks.as_ref().is_some_and(|ranks| ranks.rank(faction).is_some_and(|r| r >= 0));
            if flagged {
                ledger.set_flag(flag, faction, true);
            }
        }
    }
    if seen_by.is_empty() {
        return 0;
    }
    let bounty = if killed {
        charge(world, model, model.murder_gold, "committing murder")
    } else {
        charge(world, model, model.attack_gold, "assaulting someone")
    };
    raise_alarm(world, model, player, kind, victim_ref, &seen_by);
    bounty
}

/// Whether `furniture` has a sleeping position (a bed).
fn has_sleep_marker(furniture: &byroredux_core::ecs::components::Furniture) -> bool {
    use byroredux_core::ecs::components::FurnitureMarkerKind;
    furniture.markers.iter().any(|marker| marker.kind == FurnitureMarkerKind::Sleep)
}

/// Whether `entity` is placed furniture with a sleeping position.
pub(crate) fn is_bed(world: &World, entity: EntityId) -> bool {
    world.get::<byroredux_core::ecs::components::Furniture>(entity).is_some_and(|furniture| has_sleep_marker(&furniture))
}

/// The placed furniture with a sleeping position (a bed).
pub(crate) fn bed_entities(world: &World) -> Vec<EntityId> {
    use byroredux_core::ecs::components::Furniture;
    world
        .query::<Furniture>()
        .map(|furniture| furniture.iter().filter(|(_, furniture)| has_sleep_marker(furniture)).map(|(entity, _)| entity).collect())
        .unwrap_or_default()
}

/// Resting in a bed while serving a jail sentence ends it: the player activated a bed (the
/// canonical `ActivateEvent`), so the clock runs the sentence and the player leaves by the exit
/// ([`byroredux_scripting::jail::serve_sentence`]). Outside a sentence a bed is just furniture.
///
/// `ActivateEvent` is a transient marker that is empty on nearly every frame, so it is read first
/// and only the player's activations are tested for being a bed (#6340); gathering every bed in
/// the world each frame of a sentence, ahead of an event that rarely comes, was the cost. The
/// targets are collected before `Furniture` is touched, so the two guards are never held together.
pub(crate) fn jail_bed_system(world: &World, _dt: f32) {
    // Stepping out into the open air before serving the sentence is an escape (M43, `ESJA`).
    if byroredux_scripting::jail::detect_escape(world) {
        crate::notifications::push(world, "You have escaped from jail");
    }
    if !byroredux_scripting::jail::player_in_jail(world) {
        return;
    }
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return;
    };
    let activated: Vec<EntityId> = world
        .query::<byroredux_scripting::ActivateEvent>()
        .map(|events| events.iter().filter(|(_, event)| event.activator == player).map(|(target, _)| target).collect())
        .unwrap_or_default();
    let rested = activated.into_iter().any(|target| is_bed(world, target));
    if rested && byroredux_scripting::jail::serve_sentence(world) {
        crate::notifications::push(world, "You have served your sentence");
    }
}

#[cfg(test)]
mod evidence_tests {
    use super::*;
    use byroredux_scripting::jail::{EvidenceDeposit, PendingEvidenceDeposits};

    /// With no way to reach the chest the goods stay in the evidence; the queue is drained either way.
    #[test]
    fn an_unreachable_chest_leaves_the_goods_in_the_evidence() {
        let mut world = World::new();
        world.insert_resource(PendingEvidenceDeposits(vec![EvidenceDeposit { chest: 0xC0FFEE, item: 0xAAAA, count: 3 }]));
        let mut ledger = ObScriptLedger::default();
        ledger.note_stolen(0xAAAA, 3);
        ledger.confiscate(0xAAAA, 3);
        world.insert_resource(ledger);
        evidence_deposit_system(&world, 0.0);
        assert!(world.resource::<PendingEvidenceDeposits>().0.is_empty());
        assert_eq!(world.resource::<ObScriptLedger>().evidence(), vec![(0xAAAA, 3)]);
    }
}

#[cfg(test)]
mod bed_tests {
    use super::*;
    use byroredux_core::ecs::components::{Furniture, FurnitureMarker, FurnitureMarkerKind};
    use byroredux_scripting::obscript_ledger::JailSentence;
    use byroredux_scripting::{ActivateEvent, PlayerMoveRequest};

    fn marker(kind: FurnitureMarkerKind) -> FurnitureMarker {
        FurnitureMarker { local_offset: [0.0; 3], heading_z_radians: None, animation_type: 0, kind }
    }

    /// A player serving a sentence, a bed and a chair.
    fn cell() -> (World, EntityId, EntityId, EntityId) {
        let mut world = World::new();
        world.register::<Furniture>();
        world.register::<ActivateEvent>();
        let player = world.spawn();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        world.insert_resource(crate::notifications::PlayerNotifications::default());
        world.insert_resource(PlayerMoveRequest::default());
        world.insert_resource(byroredux_scripting::obscript_more::PendingSleep::default());
        let mut ledger = ObScriptLedger::default();
        ledger.jail = Some(JailSentence { exit_marker: 0xE1, days: 2.0 });
        world.insert_resource(ledger);
        let bed = world.spawn();
        world.insert(bed, Furniture { markers: vec![marker(FurnitureMarkerKind::Sleep)] });
        let chair = world.spawn();
        world.insert(chair, Furniture { markers: vec![marker(FurnitureMarkerKind::Sit)] });
        (world, player, bed, chair)
    }

    #[test]
    fn only_furniture_with_a_sleeping_position_is_a_bed() {
        let (world, _player, bed, chair) = cell();
        assert_eq!(bed_entities(&world), vec![bed]);
        assert!(!bed_entities(&world).contains(&chair));
    }

    /// #6340 — the per-entity predicate the system uses and the bulk gather the interaction
    /// prompt uses are one definition.
    #[test]
    fn is_bed_agrees_with_the_bulk_bed_gather() {
        let (world, player, bed, chair) = cell();
        let beds = bed_entities(&world);
        for entity in [bed, chair, player] {
            assert_eq!(is_bed(&world, entity), beds.contains(&entity), "{entity:?}");
        }
    }

    #[test]
    fn resting_in_a_bed_ends_the_sentence() {
        let (mut world, player, bed, _chair) = cell();
        world.insert(bed, ActivateEvent { activator: player });
        jail_bed_system(&world, 0.0);
        assert!(!byroredux_scripting::jail::player_in_jail(&world));
        assert_eq!(world.resource::<PlayerMoveRequest>().0, Some(0xE1), "released at the exit");
        assert_eq!(world.resource::<byroredux_scripting::obscript_more::PendingSleep>().0, 48.0, "two days");
        assert_eq!(crate::notifications::drain(&world), vec!["You have served your sentence".to_owned()]);
    }

    #[test]
    fn a_chair_another_activator_or_no_sentence_end_nothing() {
        let (mut world, player, bed, chair) = cell();
        world.insert(chair, ActivateEvent { activator: player });
        let other = world.spawn();
        world.insert(bed, ActivateEvent { activator: other });
        jail_bed_system(&world, 0.0);
        assert!(byroredux_scripting::jail::player_in_jail(&world), "neither counts as the player resting in a bed");

        let (mut world, player, bed, _chair) = cell();
        world.resource_mut::<ObScriptLedger>().jail = None;
        world.insert(bed, ActivateEvent { activator: player });
        jail_bed_system(&world, 0.0);
        assert_eq!(world.resource::<PlayerMoveRequest>().0, None, "a bed is only furniture outside a sentence");
    }
}

#[cfg(test)]
mod alarm_tests {
    use super::*;
    use crate::systems::faction_hostility::{CombatDisposition, DetectionConfig};
    use crate::systems::forcegreet::ForceGreetDirective;
    use byroredux_core::character::Background;
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_core::math::Vec3;
    use byroredux_plugin::esm::records::{ActorAiData, Aggression, ClassRecord, Confidence};
    use byroredux_scripting::crime_knowledge::CrimeKnowledge;

    const GUARD_CLASS: u32 = 0x77;
    const OWNER: u32 = 0xF00D;

    struct Town {
        world: World,
        player: EntityId,
        witness: EntityId,
        near_guard: EntityId,
        nearer_guard: EntityId,
        far_guard: EntityId,
    }

    /// A player at the origin; a citizen watching from 300; two guards in earshot of the citizen
    /// (one 500 from the player, one 900); a guard 20000 away.
    fn town(model: Option<CrimeModel>) -> Town {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<GlobalTransform>();
        world.register::<CombatDisposition>();
        world.register::<Background>();
        world.register::<FormIdComponent>();
        world.register::<ForceGreetDirective>();
        let mut index = EsmIndex::default();
        index.classes.insert(
            GUARD_CLASS,
            ClassRecord {
                form_id: GUARD_CLASS,
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
                flags_oblivion: None,
                class_flags: Some(0b10),
            },
        );
        byroredux_scripting::guards::install_guard_classes(&mut world, &index);
        if let Some(model) = model {
            world.insert_resource(model);
        }
        world.insert_resource(DetectionConfig { max_distance: 5000.0, exterior_mult: 1.0 });
        world.insert_resource(crate::notifications::PlayerNotifications::default());
        let mut pool = FormIdPool::default();
        let mut next = 0xA0u32;
        let calm = ActorAiData { aggression: Aggression::Unaggressive, confidence: Confidence::Average, responsibility: None, attack_radius: None };
        let mut actor = |world: &mut World, z: f32, class: u32| {
            let entity = world.spawn();
            next += 1;
            let id = pool.intern(FormIdPair { plugin: PluginId::from_filename("Oblivion.esm"), local: LocalFormId(next) });
            world.insert(entity, FormIdComponent(id));
            world.insert(entity, GlobalTransform::new(Vec3::new(0.0, 0.0, z), Default::default(), 1.0));
            world.insert(entity, CombatDisposition(calm));
            world.insert(entity, Background { race_form_id: 1, class_form_id: class });
            entity
        };
        let witness = actor(&mut world, 300.0, 0x10);
        let near_guard = actor(&mut world, -900.0, GUARD_CLASS);
        let nearer_guard = actor(&mut world, 500.0, GUARD_CLASS);
        let far_guard = actor(&mut world, 20000.0, GUARD_CLASS);
        world.insert_resource(pool);
        let player = world.spawn();
        world.insert(player, GlobalTransform::new(Vec3::ZERO, Default::default(), 1.0));
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        world.insert_resource(byroredux_scripting::papyrus_demo::PapyrusPlayerEntity(player));
        Town { world, player, witness, near_guard, nearer_guard, far_guard }
    }

    fn knows(town: &Town, entity: EntityId, kind: CrimeKind) -> bool {
        let knower = byroredux_scripting::obscript_state::reference_form(&town.world, entity).unwrap();
        town.world.resource::<CrimeKnowledge>().knows(knower, byroredux_scripting::PLAYER_REFERENCE, Some(kind), None)
    }

    fn summoned(town: &Town, entity: EntityId) -> bool {
        town.world.get::<ForceGreetDirective>(entity).is_some()
    }

    #[test]
    fn a_witnessed_theft_calls_the_guards_in_earshot_nearest_the_player_first() {
        let town = town(Some(CrimeModel { guards_responding: 2, ..CrimeModel::OBLIVION }));
        report_theft(&town.world, town.player, OWNER, &[(0x5678, 3)]);
        assert!(knows(&town, town.witness, CrimeKind::Steal), "the citizen saw it");
        for guard in [town.near_guard, town.nearer_guard] {
            assert!(summoned(&town, guard) && knows(&town, guard, CrimeKind::Steal));
        }
        assert!(!summoned(&town, town.far_guard) && !knows(&town, town.far_guard, CrimeKind::Steal), "out of earshot");
        assert!(!summoned(&town, town.witness), "a citizen is not a guard");
    }

    #[test]
    fn only_the_nearest_guards_up_to_the_setting_come() {
        let town = town(Some(CrimeModel { guards_responding: 1, ..CrimeModel::OBLIVION }));
        report_theft(&town.world, town.player, OWNER, &[(0x5678, 3)]);
        assert!(summoned(&town, town.nearer_guard) && !summoned(&town, town.near_guard), "one guard, the nearer");
        let none = super::alarm_tests::town(Some(CrimeModel { guards_responding: 0, ..CrimeModel::OBLIVION }));
        report_theft(&none.world, none.player, OWNER, &[(0x5678, 3)]);
        assert!(!summoned(&none, none.nearer_guard), "0 guards is a real setting");
    }

    #[test]
    fn nobody_comes_for_an_unseen_crime() {
        let mut town = town(Some(CrimeModel::OBLIVION));
        // The citizen and the guards are past detection range.
        town.world.insert_resource(DetectionConfig { max_distance: 100.0, exterior_mult: 1.0 });
        report_theft(&town.world, town.player, OWNER, &[(0x5678, 3)]);
        assert!(!summoned(&town, town.nearer_guard) && town.world.resource::<CrimeKnowledge>().is_empty());
    }

    /// Every game records who saw a crime; only a game with a crime model fines it and sends guards.
    #[test]
    fn a_game_without_a_crime_model_still_records_what_was_seen() {
        let town = town(None);
        report_theft(&town.world, town.player, OWNER, &[(0x5678, 3)]);
        assert!(knows(&town, town.witness, CrimeKind::Steal));
        assert!(!summoned(&town, town.nearer_guard), "no model, no response");
        assert!(town.world.try_resource::<ObScriptLedger>().is_none_or(|ledger| ledger.crime_gold == 0));
    }

    /// Skyrim: the bounty is kept per crime faction, reported to by whoever saw the crime.
    mod skyrim {
        use super::*;
        use byroredux_plugin::esm::records::crime_flags::*;
        use byroredux_plugin::esm::records::FactionCrime;
        use byroredux_scripting::{CrimeFactionOverrides, FactionCrimeGold};

        const WHITERUN: u32 = 0x00_0AA1;
        const RIFTEN: u32 = 0x00_0AA2;

        /// The town with Skyrim's model, the citizen reporting to Whiterun's tracking crime faction.
        fn hold(flags: u32) -> Town {
            let mut town = town(Some(CrimeModel::SKYRIM));
            let mut data = std::collections::HashMap::new();
            data.insert(WHITERUN, FactionCrime { flags, ..Default::default() });
            data.insert(RIFTEN, FactionCrime { flags: TRACK_CRIME | USE_DEFAULT_CRIME_GOLD, ..Default::default() });
            town.world.insert_resource(byroredux_scripting::crime_faction::CrimeFactionData::from_map(data));
            town.world.insert_resource(FactionCrimeGold::default());
            let mut overrides = CrimeFactionOverrides::default();
            let witness_ref = byroredux_scripting::obscript_state::reference_form(&town.world, town.witness).unwrap();
            overrides.set(witness_ref, WHITERUN);
            town.world.insert_resource(overrides);
            town
        }

        #[test]
        fn a_witnessed_theft_is_a_nonviolent_bounty_on_the_witnesses_crime_faction() {
            let town = hold(TRACK_CRIME | USE_DEFAULT_CRIME_GOLD);
            let added = charge_crime_factions(&town.world, CrimeModel::SKYRIM, CrimeKind::Steal, &[town.witness], 101);
            assert_eq!(added, 50);
            assert_eq!(town.world.resource::<FactionCrimeGold>().of(WHITERUN), (50, 0), "(non-violent, violent): half of 101, rounded down");
            report_theft(&town.world, town.player, OWNER, &[]);
            assert_eq!(town.world.resource::<FactionCrimeGold>().of(WHITERUN), (50, 0), "a worthless item adds nothing");
            assert_eq!(town.world.resource::<FactionCrimeGold>().of(RIFTEN), (0, 0), "another hold is untouched");
            assert!(town.world.try_resource::<ObScriptLedger>().is_none_or(|ledger| ledger.crime_gold == 0), "no player-wide bounty");
            assert!(!summoned(&town, town.nearer_guard), "Skyrim's guards are not this response");
        }

        #[test]
        fn assault_and_murder_are_violent_and_land_on_the_same_faction() {
            let mut town = hold(TRACK_CRIME | USE_DEFAULT_CRIME_GOLD);
            report_strike(&town.world, town.player, town.far_guard, false);
            assert_eq!(town.world.resource::<FactionCrimeGold>().of(WHITERUN), (0, 40), "(non-violent, violent)");
            town.world.register::<Dead>();
            town.world.insert(town.far_guard, Dead);
            report_strike(&town.world, town.player, town.far_guard, true);
            assert_eq!(town.world.resource::<FactionCrimeGold>().of(WHITERUN), (0, 1040), "assault, then the murder");
        }

        #[test]
        fn a_faction_that_ignores_the_crime_or_does_not_track_hears_nothing() {
            let town = hold(TRACK_CRIME | USE_DEFAULT_CRIME_GOLD | IGNORE_ASSAULT);
            report_strike(&town.world, town.player, town.far_guard, false);
            assert_eq!(town.world.resource::<FactionCrimeGold>().of(WHITERUN), (0, 0));
            let town = hold(USE_DEFAULT_CRIME_GOLD);
            report_strike(&town.world, town.player, town.far_guard, false);
            assert_eq!(town.world.resource::<FactionCrimeGold>().of(WHITERUN), (0, 0), "not tracking");
        }

        #[test]
        fn an_actor_with_no_crime_faction_reports_nothing_but_still_knows() {
            let mut town = hold(TRACK_CRIME | USE_DEFAULT_CRIME_GOLD);
            town.world.insert_resource(CrimeFactionOverrides::default()); // the citizen's faction is gone
            report_strike(&town.world, town.player, town.far_guard, false);
            assert_eq!(town.world.resource::<FactionCrimeGold>().of(WHITERUN), (0, 0));
            assert!(knows(&town, town.witness, CrimeKind::Attack), "it saw the assault");
        }

        #[test]
        fn animals_witness_crimes_in_skyrim_but_not_in_oblivion() {
            for (model, expected) in [(CrimeModel::SKYRIM, true), (CrimeModel::OBLIVION, false)] {
                let mut town = town(Some(model));
                town.world.register::<CreatureActor>();
                town.world.insert(town.witness, CreatureActor);
                // Only the (now creature) citizen is in sight.
                town.world.insert_resource(DetectionConfig { max_distance: 350.0, exterior_mult: 1.0 });
                report_theft(&town.world, town.player, OWNER, &[(0x5678, 3)]);
                assert_eq!(knows(&town, town.witness, CrimeKind::Steal), expected);
            }
        }
    }

    #[test]
    fn a_witnessed_murder_is_known_with_its_victim_and_calls_the_guards() {
        let mut town = town(Some(CrimeModel::OBLIVION));
        // The far guard is the one who dies: it stands too far off to see anything.
        let victim = town.far_guard;
        town.world.register::<Dead>();
        town.world.insert(victim, Dead);
        report_strike(&town.world, town.player, victim, true);
        let victim_ref = byroredux_scripting::obscript_state::reference_form(&town.world, victim).unwrap();
        let witness_ref = byroredux_scripting::obscript_state::reference_form(&town.world, town.witness).unwrap();
        assert!(town.world.resource::<CrimeKnowledge>().knows(witness_ref, byroredux_scripting::PLAYER_REFERENCE, Some(CrimeKind::Murder), Some(victim_ref)));
        assert!(summoned(&town, town.nearer_guard), "the guards come for a murder");
        assert!(!summoned(&town, victim), "the dead do not");
        assert_eq!(town.world.resource::<ObScriptLedger>().crime_gold, 1000);
    }

    #[test]
    fn a_blow_that_does_not_kill_is_an_attack_that_is_known() {
        let town = town(Some(CrimeModel::OBLIVION));
        report_strike(&town.world, town.player, town.far_guard, false);
        assert!(knows(&town, town.witness, CrimeKind::Attack) && !knows(&town, town.witness, CrimeKind::Murder));
        assert_eq!(town.world.resource::<ObScriptLedger>().crime_gold, 40);
    }
}

/// Move the goods a confiscation queued into the jails' `StolenGoods` chests, loaded or not, and
/// release them from the ledger's evidence once they are there. A chest that cannot be reached
/// leaves its goods in the evidence.
pub(crate) fn evidence_deposit_system(world: &World, _dt: f32) {
    let deposits = match world.try_resource_mut::<byroredux_scripting::jail::PendingEvidenceDeposits>() {
        Some(mut pending) if !pending.0.is_empty() => std::mem::take(&mut pending.0),
        _ => return,
    };
    for deposit in deposits {
        if crate::cell_loader::reference_state::deposit_into_reference(world, deposit.chest, deposit.item, deposit.count) {
            if let Some(mut ledger) = world.try_resource_mut::<ObScriptLedger>() {
                ledger.release_evidence(deposit.item, deposit.count);
            }
        } else {
            log::warn!("evidence deposit: chest {:08X} cannot be reached; {} of {:08X} stay in the evidence", deposit.chest, deposit.count, deposit.item);
        }
    }
}

/// How often the guards look for a wanted player. Engine pacing, not a game rule (the hostility pass
/// evaluates every `EVALUATION_PERIOD_SECS` for the same reason).
const RUSH_PERIOD_SECS: f32 = 1.0;

/// How long a guard who rushed the player waits before rushing again. Engine pacing, not a game
/// rule: the original's arrest greeting has no way out, while ours closes with the conversation
/// page, and without the wait a guard would re-open it the moment it closed.
const RUSH_RETRY_SECS: f32 = 30.0;

/// Guards who see a wanted player come for them. `iCrimeGoldAttackMin` is "the minimum bounty level
/// required for guards to rush a player on sight": at or past it, a guard (class Guard flag) who
/// can see the player walks up and opens its greeting — the same force-greet directive the alarm
/// uses — which the authored arrest greeting answers. Below it a guard waits to be spoken to.
/// The higher figures the wiki quotes are conditions in that greeting, not code here.
pub(crate) fn make_guard_rush_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut elapsed = 0.0f32;
    // Seconds before each guard may rush again.
    let mut waiting: std::collections::HashMap<EntityId, f32> = std::collections::HashMap::new();
    move |world: &World, dt: f32| {
        if dt.is_finite() {
            elapsed += dt.max(0.0);
            for wait in waiting.values_mut() {
                *wait -= dt.max(0.0);
            }
            waiting.retain(|_, wait| *wait > 0.0);
        }
        if elapsed < RUSH_PERIOD_SECS {
            return;
        }
        elapsed = 0.0;
        rush_wanted_player(world, &mut waiting);
    }
}

fn rush_wanted_player(world: &World, waiting: &mut std::collections::HashMap<EntityId, f32>) {
    use crate::systems::forcegreet::{ForceGreetDirective, FORCE_GREET_RADIUS};
    let Some(model) = world.try_resource::<CrimeModel>().map(|model| *model).filter(|model| model.scope == BountyScope::Player) else { return };
    let bounty = byroredux_scripting::guards::player_bounty(world);
    if bounty <= 0 || bounty < model.rush_bounty || byroredux_scripting::jail::player_in_jail(world) {
        return;
    }
    let Some(player) = crate::systems::PlayerEntity::get(world) else { return };
    // Mid-conversation, or unable to act: nobody is rushed (the dialogue runtime defers too).
    let talking = world.try_resource::<crate::systems::npc_dialogue::DialogueSurfaceState>().is_some_and(|surface| surface.npc.is_some());
    if talking || !crate::systems::player_can_act(world) {
        return;
    }
    let actors: Vec<EntityId> = world
        .query::<crate::systems::faction_hostility::CombatDisposition>()
        .map(|q| q.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    for guard in actors {
        if guard == player
            || waiting.contains_key(&guard)
            || !byroredux_scripting::guards::is_guard(world, guard)
            || world.get::<Dead>(guard).is_some()
            || world.get::<CreatureActor>(guard).is_some()
            || world.get::<byroredux_scripting::AiCombatState>(guard).is_some()
            || !crate::systems::faction_hostility::actor_detects(world, guard, player)
        {
            continue;
        }
        if let Some(mut directives) = world.query_mut::<ForceGreetDirective>() {
            if directives.get(guard).is_none() {
                directives.insert(guard, ForceGreetDirective { topic: None, radius: FORCE_GREET_RADIUS });
                waiting.insert(guard, RUSH_RETRY_SECS);
            }
        }
    }
}

#[cfg(test)]
mod rush_tests {
    use super::*;
    use crate::systems::faction_hostility::{CombatDisposition, DetectionConfig};
    use crate::systems::forcegreet::ForceGreetDirective;
    use byroredux_core::character::Background;
    use byroredux_core::math::Vec3;
    use byroredux_plugin::esm::records::{ActorAiData, Aggression, ClassRecord, Confidence, EsmIndex};

    const GUARD_CLASS: u32 = 0x77;

    /// A player with `bounty`, a guard 400 away who can see them, a guard 20000 away, a citizen 400 away.
    fn street(bounty: i32) -> (World, EntityId, EntityId, EntityId, EntityId) {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<GlobalTransform>();
        world.register::<CombatDisposition>();
        world.register::<Background>();
        world.register::<ForceGreetDirective>();
        let mut index = EsmIndex::default();
        index.classes.insert(
            GUARD_CLASS,
            ClassRecord {
                form_id: GUARD_CLASS,
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
                flags_oblivion: None,
                class_flags: Some(0b10),
            },
        );
        byroredux_scripting::guards::install_guard_classes(&mut world, &index);
        world.insert_resource(CrimeModel::OBLIVION);
        world.insert_resource(DetectionConfig { max_distance: 2000.0, exterior_mult: 1.0 });
        let mut ledger = ObScriptLedger::default();
        ledger.crime_gold = bounty;
        world.insert_resource(ledger);
        let calm = ActorAiData { aggression: Aggression::Unaggressive, confidence: Confidence::Average, responsibility: None, attack_radius: None };
        let actor = |world: &mut World, z: f32, class: u32| {
            let entity = world.spawn();
            world.insert(entity, GlobalTransform::new(Vec3::new(0.0, 0.0, z), Default::default(), 1.0));
            world.insert(entity, CombatDisposition(calm));
            world.insert(entity, Background { race_form_id: 1, class_form_id: class });
            entity
        };
        let near = actor(&mut world, 400.0, GUARD_CLASS);
        let far = actor(&mut world, 20000.0, GUARD_CLASS);
        let citizen = actor(&mut world, -400.0, 0x10);
        let player = world.spawn();
        world.insert(player, GlobalTransform::new(Vec3::ZERO, Default::default(), 1.0));
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        world.insert_resource(byroredux_scripting::papyrus_demo::PapyrusPlayerEntity(player));
        (world, player, near, far, citizen)
    }

    fn rushed(world: &World, entity: EntityId) -> bool {
        world.get::<ForceGreetDirective>(entity).is_some()
    }

    fn tick(world: &World, system: &mut impl FnMut(&World, f32), secs: f32) {
        system(world, secs);
    }

    #[test]
    fn a_guard_who_sees_a_player_at_the_rush_bounty_comes_for_them() {
        let (world, _player, near, far, citizen) = street(500);
        tick(&world, &mut make_guard_rush_system(), 1.0);
        assert!(rushed(&world, near));
        assert!(!rushed(&world, far), "out of sight");
        assert!(!rushed(&world, citizen), "a citizen is not a guard");
    }

    #[test]
    fn below_the_setting_a_guard_waits_to_be_spoken_to() {
        let (world, _player, near, ..) = street(499);
        tick(&world, &mut make_guard_rush_system(), 1.0);
        assert!(!rushed(&world, near));
        let (world, _player, near, ..) = street(0);
        tick(&world, &mut make_guard_rush_system(), 1.0);
        assert!(!rushed(&world, near), "no bounty, no rush");
    }

    #[test]
    fn the_threshold_is_the_load_orders_setting() {
        let (mut world, _player, near, ..) = street(600);
        world.insert_resource(CrimeModel { rush_bounty: 1000, ..CrimeModel::OBLIVION });
        tick(&world, &mut make_guard_rush_system(), 1.0);
        assert!(!rushed(&world, near), "a mod that raised iCrimeGoldAttackMin raises it");
    }

    #[test]
    fn a_guard_does_not_rush_again_the_moment_the_conversation_closes() {
        let (world, _player, near, ..) = street(900);
        let mut system = make_guard_rush_system();
        tick(&world, &mut system, 1.0);
        assert!(rushed(&world, near));
        // The greeting opened and consumed the directive.
        world.query_mut::<ForceGreetDirective>().unwrap().remove(near);
        tick(&world, &mut system, 1.0);
        assert!(!rushed(&world, near), "still waiting out the retry");
        tick(&world, &mut system, RUSH_RETRY_SECS);
        assert!(rushed(&world, near), "and rushes again after it");
    }

    #[test]
    fn nobody_is_rushed_while_the_player_is_in_a_conversation() {
        use crate::systems::npc_dialogue::DialogueSurfaceState;
        let (mut world, _player, near, far, _citizen) = street(900);
        world.insert_resource(DialogueSurfaceState { npc: Some(far), ..Default::default() });
        tick(&world, &mut make_guard_rush_system(), 1.0);
        assert!(!rushed(&world, near), "the dialogue runtime would defer it; so do we");
    }

    #[test]
    fn nobody_is_rushed_mid_conversation_in_jail_or_without_a_crime_model() {
        let (mut world, _player, near, ..) = street(900);
        world.resource_mut::<ObScriptLedger>().jail = Some(byroredux_scripting::obscript_ledger::JailSentence { exit_marker: 1, days: 1.0 });
        tick(&world, &mut make_guard_rush_system(), 1.0);
        assert!(!rushed(&world, near), "a prisoner is not rushed");
        world.resource_mut::<ObScriptLedger>().jail = None;
        world.remove_resource::<CrimeModel>();
        tick(&world, &mut make_guard_rush_system(), 1.0);
        assert!(!rushed(&world, near), "no model, no rule");
    }
}

#[cfg(test)]
mod install_tests {
    use super::*;
    use byroredux_core::character::CharacterRulesProfile;
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::{GameSetting, SettingValue};

    fn index(game: GameKind, profile: CharacterRulesProfile) -> EsmIndex {
        EsmIndex { game, character_rules: profile, ..Default::default() }
    }

    /// #6426 — the profile row picks the model and the load order's GMSTs
    /// overlay it: Oblivion installs its fines with an authored
    /// `iCrimeGoldAttackMin` replacing the default, and Skyrim installs the
    /// per-crime-faction dialect.
    #[test]
    fn the_profile_row_installs_the_model_and_the_load_order_overlays_it() {
        let mut world = World::new();
        let mut oblivion = index(GameKind::Oblivion, CharacterRulesProfile::OBLIVION);
        oblivion.insert_game_setting(GameSetting {
            form_id: 0x1,
            editor_id: "iCrimeGoldAttackMin".to_string(),
            value: SettingValue::Int(750),
        });
        install_crime_model(&mut world, &oblivion);
        let model = *world.resource::<CrimeModel>();
        assert_eq!(model.rush_bounty, 750, "the authored setting overlays the default");
        assert_eq!(
            CrimeModel { rush_bounty: CrimeModel::OBLIVION.rush_bounty, ..model },
            CrimeModel::OBLIVION,
            "nothing else moved"
        );

        install_crime_model(&mut world, &index(GameKind::Skyrim, CharacterRulesProfile::SKYRIM));
        assert_eq!(*world.resource::<CrimeModel>(), CrimeModel::SKYRIM);
    }

    /// A game with no crime model clears whatever the previous load
    /// installed, so a stale Oblivion/Skyrim model never survives a switch.
    #[test]
    fn a_profile_without_a_crime_model_clears_the_previous_one() {
        for (game, profile) in [
            (GameKind::Fallout3NV, CharacterRulesProfile::FALLOUT3),
            (GameKind::Fallout3NV, CharacterRulesProfile::FALLOUT_NEW_VEGAS),
            (GameKind::Fallout4, CharacterRulesProfile::FALLOUT4),
        ] {
            let mut world = World::new();
            world.insert_resource(CrimeModel::OBLIVION);
            install_crime_model(&mut world, &index(game, profile));
            assert!(
                world.try_resource::<CrimeModel>().is_none(),
                "{} must clear a stale crime model",
                profile.name()
            );
        }
    }
}
