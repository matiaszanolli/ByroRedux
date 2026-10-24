//! Paying the fine and going to jail (Oblivion): `PayFineThief`, `PayFine`,
//! `GoToJail` and the sentence they start.
//!
//! The CS wiki's definitions:
//!
//! * *PayFineThief*: "takes the amount of gold out your inventory equal to your
//!   crime gold, and wipes your crime gold clean."
//! * *PayFine*: that, plus "takes all stolen items out of your inventory and
//!   sends you to the exit of the nearest jail."
//! * *GoToJail*: "wipes your crime gold, takes all stolen items out of your
//!   inventory and sends you to the nearest jail ... After the player was moved
//!   to the cell, the game waits for the player to rest and then teleports him
//!   to the prison's exit." The sentence is "player bounty / iCrimeDaysInPrisonMod"
//!   days ([`CrimeModel::sentence_days`]).
//!
//! A jail is "a linked pair of `PrisonMarker` doors, one in the prison cell
//! (interior) and one at the exit (exterior)", read from the load order into
//! [`JailTable`]; nothing here names a city. "Nearest" is the jail whose exit
//! lies in the player's worldspace, then the closest of those; a player who is
//! indoors, or in a worldspace without a jail, gets none and the command is
//! declined whole (no half-paid fine).
//!
//! Resting in the cell's bed ends the sentence: the app's bed interaction
//! (`crime::jail_bed_system`) calls [`serve_sentence`]; the `jail.serve`
//! console command is the same call without the walk.
//!
//! The confiscated goods go to the jail's `StolenGoods` chest ("the stolen items
//! are put into the Stolengoods chest that needs to be placed in the same
//! interior cell as the Prisonmarker"), where the player can fetch them back:
//! each is recorded in the ledger's evidence, queued as an [`EvidenceDeposit`],
//! and the app's deposit system moves it into the chest (resident or not) and
//! releases the evidence. A jail whose chest authors its own contents is not
//! used (a deposit never replaces them), and its goods stay in the evidence.
//!
//! Not modelled: the skill loss for time served (UESP gives no rule for which
//! skill).

use std::sync::Arc;

use byroredux_core::character::CrimeModel;
use byroredux_core::ecs::components::GlobalTransform;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

use crate::cell_kind::{cell_kind_of, CellKind};
use crate::fragment::{remove_items, PlayerMoveRequest};
use crate::guards::{bounty_of, can_pay_bounty, CURRENCY_FORM};
use crate::obscript_ledger::{JailSentence, ObScriptLedger};
use crate::obscript_more::PendingSleep;

/// The editor id of the door base a jail is built from (CS wiki *Jail*: "a
/// predefined door with this name"). It is the modding contract: a mod that
/// places a linked pair of these makes a jail.
pub const JAIL_MARKER_EDITOR_ID: &str = "PrisonMarker";

/// The editor id of the container a jail keeps confiscated goods in (CS wiki
/// *Jail*: "a Stolengoods container, placed in the jail interior cell").
pub const JAIL_EVIDENCE_EDITOR_ID: &str = "StolenGoods";

/// Confiscated goods on their way into a jail's chest. `NOT_SAVED_BY_DESIGN`: drained
/// within the frame that queued them; the ledger's evidence is the durable record until delivered.
#[derive(Debug, Default)]
pub struct PendingEvidenceDeposits(pub Vec<EvidenceDeposit>);

impl Resource for PendingEvidenceDeposits {}

/// `count` of `item` for the chest placed as reference `chest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceDeposit {
    pub chest: u32,
    pub item: u32,
    pub count: u32,
}

/// One jail: the marker inside the cell and the one at the exit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Jail {
    /// The interior `PrisonMarker` reference — where `GoToJail` puts the player.
    pub cell_marker: u32,
    /// The exterior `PrisonMarker` reference — where the player is released.
    pub exit_marker: u32,
    /// The worldspace the exit stands in.
    pub exit_worldspace: u32,
    /// The exit's placement, in Bethesda (Z-up) units.
    pub exit_position: [f32; 3],
    /// The `StolenGoods` chest in the jail's cell, when it authors no contents of its own.
    pub evidence_chest: Option<u32>,
}

/// Every jail the load order defines. Rebuilt from the records each boot.
#[derive(Debug, Default, Clone)]
pub struct JailTable(Arc<Vec<Jail>>);

impl Resource for JailTable {}

impl JailTable {
    /// A table of `jails`, for tests that stand in for a load order.
    #[cfg(test)]
    pub(crate) fn for_test(jails: Vec<Jail>) -> Self {
        Self(Arc::new(jails))
    }

    pub fn jails(&self) -> &[Jail] {
        &self.0
    }
}

/// Install the load order's jails: each interior `PrisonMarker` whose teleport
/// names an exterior `PrisonMarker` placed in a worldspace.
pub fn install_jail_table(world: &mut World, index: &EsmIndex) {
    let markers: Vec<u32> = index
        .cells
        .statics
        .iter()
        .filter(|(_, object)| object.editor_id.eq_ignore_ascii_case(JAIL_MARKER_EDITOR_ID))
        .map(|(&form, _)| form)
        .collect();
    let mut jails = Vec::new();
    if !markers.is_empty() {
        // Every marker standing in an exterior cell: reference -> (worldspace, placement).
        let mut exits = std::collections::HashMap::new();
        let mut note = |worldspace: &str, cell: &byroredux_plugin::esm::cell::CellData| {
            let Some(worldspace) = index.cells.worldspaces.get(&worldspace.to_ascii_lowercase()) else { return };
            for reference in cell.references.iter().filter(|r| markers.contains(&r.base_form_id)) {
                exits.insert(reference.form_id, (worldspace.form_id, reference.position));
            }
        };
        for (name, cell) in &index.cells.worldspace_persistent_cells {
            note(name, cell);
        }
        for (name, grid) in &index.cells.exterior_cells {
            for cell in grid.values() {
                note(name, cell);
            }
        }
        // The evidence chests whose container authors no contents (a deposit must not replace them).
        let empty_chests: Vec<u32> = index
            .containers
            .iter()
            .filter(|(_, container)| container.editor_id.eq_ignore_ascii_case(JAIL_EVIDENCE_EDITOR_ID) && container.contents.is_empty())
            .map(|(&form, _)| form)
            .collect();
        for cell in index.cells.cells.values() {
            let evidence_chest = cell.references.iter().find(|r| empty_chests.contains(&r.base_form_id)).map(|r| r.form_id);
            for reference in cell.references.iter().filter(|r| markers.contains(&r.base_form_id)) {
                let Some(teleport) = reference.teleport else { continue };
                if let Some(&(exit_worldspace, exit_position)) = exits.get(&teleport.destination) {
                    jails.push(Jail {
                        cell_marker: reference.form_id,
                        exit_marker: teleport.destination,
                        exit_worldspace,
                        exit_position,
                        evidence_chest,
                    });
                }
            }
        }
        jails.sort_by_key(|jail| jail.cell_marker);
    }
    world.insert_resource(JailTable(Arc::new(jails)));
}

/// The jail nearest the player: those whose exit is in the player's worldspace,
/// then the closest exit. `None` for a player indoors or in a worldspace without one.
pub fn nearest_jail(world: &World, player: EntityId) -> Option<Jail> {
    let CellKind::Exterior { worldspace } = cell_kind_of(world, player)? else {
        return None;
    };
    let here = byroredux_core::math::coord::yup_to_zup_pos(world.get::<GlobalTransform>(player)?.translation.to_array());
    let squared = |jail: &Jail| {
        jail.exit_position.iter().zip(here).map(|(a, b)| (a - b) * (a - b)).sum::<f32>()
    };
    world
        .try_resource::<JailTable>()?
        .jails()
        .iter()
        .filter(|jail| jail.exit_worldspace == worldspace)
        .min_by(|a, b| squared(a).total_cmp(&squared(b)))
        .copied()
}

fn player_of(world: &World) -> Option<EntityId> {
    crate::papyrus_demo::PapyrusPlayerEntity::get(world)
}

/// Take the stolen goods the player still holds into the evidence. Returns how
/// many items left the inventory.
fn confiscate_stolen_goods(world: &World, player: EntityId, chest: Option<u32>) -> u32 {
    let stolen = world.try_resource::<ObScriptLedger>().map(|ledger| ledger.stolen_goods()).unwrap_or_default();
    let mut taken_total = 0;
    for (item, recorded) in stolen {
        let held: u32 = world
            .get::<byroredux_core::ecs::components::Inventory>(player)
            .map_or(0, |inventory| inventory.items.iter().filter(|s| s.base_form_id == item).map(|s| s.count).sum());
        let taken = held.min(recorded);
        if taken > 0 {
            remove_items(world, player, item, taken, None);
            taken_total += taken;
        }
        // What was taken is evidence; whatever was recorded but is no longer held (sold, used,
        // dropped) just leaves the stolen goods.
        if let Some(mut ledger) = world.try_resource_mut::<ObScriptLedger>() {
            if taken > 0 {
                ledger.confiscate(item, taken);
            }
            ledger.forget_stolen(item);
        }
        // What was taken goes to the jail's chest; the evidence stands until it arrives.
        if let (Some(chest), true) = (chest, taken > 0) {
            if let Some(mut deposits) = world.try_resource_mut::<PendingEvidenceDeposits>() {
                deposits.0.push(EvidenceDeposit { chest, item, count: taken });
            }
        }
    }
    taken_total
}

/// `PayFineThief`: gold equal to the bounty leaves the inventory and the bounty is
/// wiped. Declined (`false`) with nothing changed when there is no crime model or
/// the player cannot pay.
pub fn pay_fine_thief(world: &World) -> bool {
    let Some(player) = player_of(world) else { return false };
    if world.try_resource::<CrimeModel>().is_none() || !can_pay_bounty(world, player) {
        return false;
    }
    take_payment(world, player);
    true
}

fn take_payment(world: &World, player: EntityId) {
    let bounty = bounty_of(world, player);
    if bounty > 0 {
        remove_items(world, player, CURRENCY_FORM, bounty as u32, None);
    }
    if let Some(mut ledger) = world.try_resource_mut::<ObScriptLedger>() {
        ledger.crime_gold = 0;
    }
    crate::crime_knowledge::forgive_player(world);
}

/// `PayFine`: `PayFineThief`, then the stolen goods are confiscated and the player
/// is sent to the exit of the nearest jail. Declined whole when any part cannot be
/// done.
pub fn pay_fine(world: &World) -> bool {
    let Some(player) = player_of(world) else { return false };
    if world.try_resource::<CrimeModel>().is_none() || !can_pay_bounty(world, player) {
        return false;
    }
    let Some(jail) = nearest_jail(world, player) else { return false };
    take_payment(world, player);
    confiscate_stolen_goods(world, player, jail.evidence_chest);
    request_move(world, jail.exit_marker)
}

/// `GoToJail`: the bounty is wiped (no gold is taken), the stolen goods are
/// confiscated, and the player is sent to the nearest jail's cell to serve
/// `bounty / iCrimeDaysInPrisonMod` days. Declined whole when there is no jail
/// to go to or the player is already serving.
pub fn go_to_jail(world: &World) -> bool {
    let Some(player) = player_of(world) else { return false };
    let Some(model) = world.try_resource::<CrimeModel>().map(|model| *model) else { return false };
    if world.try_resource::<ObScriptLedger>().is_none_or(|ledger| ledger.jail.is_some()) {
        return false;
    }
    let Some(jail) = nearest_jail(world, player) else { return false };
    let days = model.sentence_days(bounty_of(world, player));
    confiscate_stolen_goods(world, player, jail.evidence_chest);
    if let Some(mut ledger) = world.try_resource_mut::<ObScriptLedger>() {
        ledger.crime_gold = 0;
        ledger.jail = Some(JailSentence { exit_marker: jail.exit_marker, days });
    }
    crate::crime_knowledge::forgive_player(world);
    request_move(world, jail.cell_marker)
}

/// Whether the player is serving a sentence (CS *IsPlayerInJail*).
pub fn player_in_jail(world: &World) -> bool {
    world.try_resource::<ObScriptLedger>().is_some_and(|ledger| ledger.jail.is_some())
}

/// The player has rested out their sentence: the clock runs the sentence's days,
/// the player is sent to the exit, and the sentence ends. `false` when none is
/// being served. The trigger is the player resting in the cell bed
/// (`crime::jail_bed_system`); the `jail.serve` console command calls it too.
pub fn serve_sentence(world: &World) -> bool {
    let Some(sentence) = world.try_resource::<ObScriptLedger>().and_then(|ledger| ledger.jail) else {
        return false;
    };
    if let Some(mut sleep) = world.try_resource_mut::<PendingSleep>() {
        sleep.0 += sentence.days * 24.0;
    }
    if let Some(mut ledger) = world.try_resource_mut::<ObScriptLedger>() {
        ledger.jail = None;
    }
    request_move(world, sentence.exit_marker)
}

fn request_move(world: &World, reference: u32) -> bool {
    match world.try_resource_mut::<PlayerMoveRequest>() {
        Some(mut request) => {
            request.0 = Some(reference);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{CellFormId, CellRoot, Inventory, ItemStack};

    const BRAVIL: u32 = 0xB0;
    const ANVIL: u32 = 0xA0;
    const ARROW: u32 = 0x5678;

    fn jail(cell_marker: u32, exit_marker: u32, worldspace: u32, at: [f32; 3]) -> Jail {
        Jail { cell_marker, exit_marker, exit_worldspace: worldspace, exit_position: at, evidence_chest: Some(0xC0FFEE) }
    }

    /// A player in the exterior of `worldspace` at `at` (Z-up), holding `gold` and the arrows.
    fn world_with_player(worldspace: u32, at: [f32; 3], gold: u32) -> (World, EntityId) {
        let mut world = World::new();
        crate::register(&mut world);
        world.register::<GlobalTransform>();
        world.register::<Inventory>();
        world.register::<CellFormId>();
        world.register::<CellRoot>();
        world.register::<crate::cell_kind::ExteriorCell>();
        world.insert_resource(CrimeModel::OBLIVION);
        world.insert_resource(PendingSleep::default());
        world.insert_resource(PendingEvidenceDeposits::default());
        world.insert_resource(PlayerMoveRequest::default());
        world.insert_resource(JailTable(Arc::new(vec![
            jail(1, 2, BRAVIL, [1000.0, 0.0, 0.0]),
            jail(3, 4, BRAVIL, [5000.0, 0.0, 0.0]),
            jail(5, 6, ANVIL, [10.0, 0.0, 0.0]),
        ])));
        let player = world.spawn();
        let yup = byroredux_core::math::coord::zup_to_yup_pos(at);
        world.insert(player, GlobalTransform::new(byroredux_core::math::Vec3::from_array(yup), Default::default(), 1.0));
        world.insert(player, Inventory { items: vec![ItemStack::new(CURRENCY_FORM, gold), ItemStack::new(ARROW, 10)] });
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        world.insert_resource(crate::cell_kind::PlayerCellKind(Some(CellKind::Exterior { worldspace })));
        (world, player)
    }

    fn held(world: &World, player: EntityId, item: u32) -> u32 {
        world.get::<Inventory>(player).unwrap().items.iter().filter(|s| s.base_form_id == item).map(|s| s.count).sum()
    }

    fn start(world: &mut World, bounty: i32, stolen: &[(u32, u32)]) {
        let mut ledger = ObScriptLedger::default();
        ledger.crime_gold = bounty;
        for &(item, count) in stolen {
            ledger.note_stolen(item, count);
        }
        world.insert_resource(ledger);
    }

    #[test]
    fn the_nearest_jail_is_the_closest_exit_in_the_players_worldspace() {
        let (world, player) = world_with_player(BRAVIL, [4500.0, 0.0, 0.0], 0);
        assert_eq!(nearest_jail(&world, player).map(|j| j.exit_marker), Some(4), "4500 is nearer 5000 than 1000");
        let (world, player) = world_with_player(ANVIL, [4500.0, 0.0, 0.0], 0);
        assert_eq!(nearest_jail(&world, player).map(|j| j.exit_marker), Some(6), "another worldspace's jail is the only one");
        let (world, player) = world_with_player(0xC0, [0.0; 3], 0);
        assert_eq!(nearest_jail(&world, player), None, "a worldspace with no jail");
        let (mut world, player) = world_with_player(BRAVIL, [0.0; 3], 0);
        world.insert_resource(crate::cell_kind::PlayerCellKind(Some(CellKind::Interior)));
        assert_eq!(nearest_jail(&world, player), None, "indoors");
    }

    #[test]
    fn pay_fine_thief_takes_the_gold_and_wipes_the_bounty_only() {
        let (mut world, player) = world_with_player(BRAVIL, [0.0; 3], 100);
        start(&mut world, 40, &[(ARROW, 4)]);
        assert!(pay_fine_thief(&world));
        assert_eq!(held(&world, player, CURRENCY_FORM), 60);
        assert_eq!(world.resource::<ObScriptLedger>().crime_gold, 0);
        assert_eq!(held(&world, player, ARROW), 10, "the stolen goods stay");
        assert_eq!(world.resource::<PlayerMoveRequest>().0, None, "and so does the player");
    }

    #[test]
    fn a_player_who_cannot_pay_changes_nothing() {
        let (mut world, player) = world_with_player(BRAVIL, [0.0; 3], 39);
        start(&mut world, 40, &[(ARROW, 4)]);
        assert!(!pay_fine_thief(&world) && !pay_fine(&world));
        assert_eq!(held(&world, player, CURRENCY_FORM), 39);
        assert_eq!(world.resource::<ObScriptLedger>().crime_gold, 40);
        assert_eq!(held(&world, player, ARROW), 10);
    }

    #[test]
    fn pay_fine_pays_confiscates_what_is_still_held_and_leaves_by_the_exit() {
        let (mut world, player) = world_with_player(BRAVIL, [900.0, 0.0, 0.0], 100);
        // Six arrows recorded as stolen, but only 10 held of which the player may keep 4: the
        // confiscation takes the six; a second item that is long gone leaves the record.
        start(&mut world, 40, &[(ARROW, 6), (0x9999, 3)]);
        assert!(pay_fine(&world));
        assert_eq!(held(&world, player, CURRENCY_FORM), 60);
        assert_eq!(held(&world, player, ARROW), 4);
        let ledger = world.resource::<ObScriptLedger>();
        assert_eq!(ledger.crime_gold, 0);
        assert!(ledger.stolen_goods().is_empty());
        assert_eq!(ledger.evidence(), vec![(ARROW, 6)], "only what was actually found is evidence");
        assert_eq!(world.resource::<PlayerMoveRequest>().0, Some(2), "the exit of the nearest jail");
        assert_eq!(ledger.jail, None);
        assert_eq!(
            world.resource::<PendingEvidenceDeposits>().0,
            vec![EvidenceDeposit { chest: 0xC0FFEE, item: ARROW, count: 6 }],
            "the six arrows are on their way to the jail's chest"
        );
    }

    #[test]
    fn pay_fine_without_a_jail_to_go_to_is_declined_whole() {
        let (mut world, player) = world_with_player(0xC0, [0.0; 3], 100);
        start(&mut world, 40, &[(ARROW, 6)]);
        assert!(!pay_fine(&world));
        assert_eq!((held(&world, player, CURRENCY_FORM), held(&world, player, ARROW)), (100, 10));
        assert_eq!(world.resource::<ObScriptLedger>().crime_gold, 40, "nothing was half done");
    }

    #[test]
    fn go_to_jail_wipes_the_bounty_for_free_and_starts_a_sentence() {
        let (mut world, player) = world_with_player(BRAVIL, [900.0, 0.0, 0.0], 5);
        start(&mut world, 250, &[(ARROW, 6)]);
        assert!(go_to_jail(&world));
        assert_eq!(held(&world, player, CURRENCY_FORM), 5, "no gold is taken, even a player who could not pay");
        assert_eq!(held(&world, player, ARROW), 4);
        let sentence = world.resource::<ObScriptLedger>().jail.expect("serving");
        assert_eq!((sentence.exit_marker, sentence.days), (2, 2.5), "250 gold is 2.5 days");
        assert_eq!(world.resource::<ObScriptLedger>().crime_gold, 0);
        assert_eq!(world.resource::<PlayerMoveRequest>().0, Some(1), "to the cell, not the exit");
        assert!(player_in_jail(&world));
        assert!(!go_to_jail(&world), "already serving");
    }

    #[test]
    fn serving_the_sentence_runs_the_clock_and_releases_at_the_exit() {
        let (mut world, _player) = world_with_player(BRAVIL, [900.0, 0.0, 0.0], 0);
        start(&mut world, 250, &[]);
        assert!(!serve_sentence(&world), "nothing to serve yet");
        assert!(go_to_jail(&world));
        world.resource_mut::<PlayerMoveRequest>().0 = None;
        assert!(serve_sentence(&world));
        assert_eq!(world.resource::<PendingSleep>().0, 60.0, "2.5 days");
        assert_eq!(world.resource::<PlayerMoveRequest>().0, Some(2));
        assert!(!player_in_jail(&world));
    }

    #[test]
    fn paying_or_serving_forgives_what_the_witnesses_knew() {
        use crate::crime_knowledge::{CrimeEntry, CrimeKnowledge};
        use byroredux_core::character::CrimeKind;
        let entry = |criminal| CrimeEntry { knower: 0xA1, criminal, kind: CrimeKind::Steal, victim: 0 };
        for go_to_jail_instead in [false, true] {
            let (mut world, _player) = world_with_player(BRAVIL, [900.0, 0.0, 0.0], 100);
            start(&mut world, 40, &[]);
            let mut knowledge = CrimeKnowledge::default();
            knowledge.record(entry(crate::PLAYER_REFERENCE));
            knowledge.record(entry(0xEE));
            world.insert_resource(knowledge);
            assert!(if go_to_jail_instead { go_to_jail(&world) } else { pay_fine_thief(&world) });
            let knowledge = world.resource::<CrimeKnowledge>();
            assert!(!knowledge.knows(0xA1, crate::PLAYER_REFERENCE, None, None), "the player is forgiven");
            assert!(knowledge.knows(0xA1, 0xEE, None, None), "someone else's crime is not");
        }
    }

    #[test]
    fn a_game_without_a_crime_model_has_no_fines_or_jail() {
        let (mut world, _player) = world_with_player(BRAVIL, [0.0; 3], 100);
        start(&mut world, 40, &[]);
        world.remove_resource::<CrimeModel>();
        assert!(!pay_fine_thief(&world) && !pay_fine(&world) && !go_to_jail(&world));
    }
}
