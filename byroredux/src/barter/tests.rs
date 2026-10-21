use super::*;
use byroredux_core::character::CharacterRulesProfile;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_core::ecs::components::ItemStack;
use byroredux_plugin::esm::records::common::CommonItemFields;
use byroredux_plugin::esm::records::{ContainerRecord, InventoryEntry, ItemKind, ItemRecord};

const SWORD: u32 = 0x0001_2EB7;
const GEM: u32 = 0x0006_3B42;
const GEM_KEYWORD: u32 = 0x0009_14EE;
const VENDOR_LIST: u32 = 0x0009_17E8;
const FACTION: u32 = 0x0005_1596;
const CHEST_REF: u32 = 0x000A_6C25;
const CHEST_BASE: u32 = 0x000A_6C24;

fn rules(
    game_max: f32,
    game_min: f32,
    scale: f32,
    cap: Option<f32>,
    floor: f32,
    ceiling: f32,
) -> BarterRules {
    BarterRules {
        skill: 0x0000_045D,
        skill_scale: scale,
        skill_cap: cap,
        barter_max: game_max,
        barter_min: game_min,
        buy_floor: floor,
        sell_ceiling: ceiling,
        perk_hooks: None,
        vendors: HashMap::from([(
            FACTION,
            VendorData {
                buy_sell_list: VENDOR_LIST,
                container: CHEST_REF,
                ..Default::default()
            },
        )]),
        containers: HashMap::from([(CHEST_REF, CHEST_BASE)]),
        lists: HashMap::from([(VENDOR_LIST, HashSet::from([GEM_KEYWORD]))]),
        item_keywords: HashMap::from([(GEM, vec![GEM_KEYWORD])]),
    }
}

fn skyrim() -> BarterRules {
    rules(3.3, 2.0, 100.0, Some(100.0), 1.05, 0.95)
}

/// UESP *Skyrim:Speech* §Prices: "At 0 skill and no perks, the final price
/// factor is 3.3 for buying and 0.303 for selling … At 100 skill … 2 for
/// buying and 0.5 for selling", and skill past 100 has no effect.
#[test]
fn skyrim_prices_follow_the_speech_formula() {
    let rules = skyrim();
    assert!((rules.price_factor(0.0) - 3.3).abs() < 1e-6);
    assert!((rules.price_factor(15.0) - 3.105).abs() < 1e-5);
    assert!((rules.price_factor(100.0) - 2.0).abs() < 1e-6);
    assert!((rules.price_factor(150.0) - 2.0).abs() < 1e-6);
    assert_eq!(rules.buy_price(100, rules.price_factor(0.0), 1.0), 330);
    assert_eq!(rules.sell_price(100, rules.price_factor(0.0), 1.0), 30);
    assert_eq!(rules.buy_price(100, rules.price_factor(100.0), 1.0), 200);
    assert_eq!(rules.sell_price(100, rules.price_factor(100.0), 1.0), 50);
    // "At 100 skill and all haggling perks, the final price factor is 1.54
    // for buying and 0.65 for selling" (Haggling80: ×0.77 / ×1.3).
    assert_eq!(rules.buy_price(100, 2.0, 0.77), 154);
    assert_eq!(rules.sell_price(100, 2.0, 1.3), 65);
}

/// Fallout wiki *Charisma (Fallout 4)*: "Buying Price Modifier = 3.50 −
/// Charisma × 0.15", the selling modifier its reciprocal, purchases at 120 %
/// of value at least and sales at 80 % at most.
#[test]
fn fallout4_prices_follow_charisma_and_clamp() {
    let rules = rules(3.5, 2.0, 10.0, None, 1.2, 0.8);
    assert!((rules.price_factor(5.0) - 2.75).abs() < 1e-6);
    assert_eq!(rules.buy_price(100, rules.price_factor(5.0), 1.0), 275);
    assert_eq!(rules.sell_price(100, rules.price_factor(5.0), 1.0), 36);
    // Charisma past 10 still counts, until the clamps hold.
    let high = rules.price_factor(20.0);
    assert_eq!(rules.buy_price(100, high, 1.0), 120);
    assert_eq!(rules.sell_price(100, high, 1.0), 80);
}

#[test]
fn the_vendor_list_admits_by_keyword_or_form_and_inverts() {
    let rules = skyrim();
    let mut vendor = rules.vendors[&FACTION].clone();
    assert!(rules.buys(&vendor, GEM), "keyword in the list");
    assert!(!rules.buys(&vendor, SWORD));
    vendor.everything_not_in_list = true;
    assert!(
        !rules.buys(&vendor, GEM),
        "the list names what is not traded"
    );
    assert!(rules.buys(&vendor, SWORD));
}

fn misc(form_id: u32, name: &str, value: u32) -> ItemRecord {
    ItemRecord {
        form_id,
        common: CommonItemFields {
            full_name: name.to_owned(),
            value,
            ..Default::default()
        },
        kind: ItemKind::Misc,
    }
}

fn count(world: &World, entity: EntityId, form: u32) -> u32 {
    count_of(world, entity, form)
}

/// A merchant chest in an unloaded cell: the session stands in for it, the
/// trades move items and gold both ways, and the stock parks on close — so the
/// next session (or the chest's own cell load) sees the traded stock.
#[test]
fn barter_trades_with_an_unloaded_merchant_chest_and_parks_its_stock() {
    let mut index = EsmIndex {
        game: GameKind::Skyrim,
        character_rules: CharacterRulesProfile::SKYRIM,
        ..Default::default()
    };
    index.items.insert(SWORD, misc(SWORD, "Iron Sword", 25));
    index.items.insert(GEM, misc(GEM, "Garnet", 100));
    index.items.insert(CURRENCY, misc(CURRENCY, "Gold", 1));
    index.containers.insert(
        CHEST_BASE,
        ContainerRecord {
            form_id: CHEST_BASE,
            editor_id: String::new(),
            full_name: String::new(),
            model_path: String::new(),
            weight: 0.0,
            flags: 0,
            open_sound: 0,
            close_sound: 0,
            script_form_id: 0,
            script_instance: None,
            contents: vec![
                InventoryEntry {
                    item_form_id: SWORD,
                    count: 2,
                },
                InventoryEntry {
                    item_form_id: CURRENCY,
                    count: 1000,
                },
            ],
        },
    );
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.insert_resource(FormIdPool::new());
    world
        .insert_resource(crate::cell_loader::reference_state::PersistentReferenceStates::default());
    world.insert_resource(crate::notifications::PlayerNotifications::default());
    world.insert_resource(ActiveBarter::default());
    let order = crate::cell_loader::load_order::LoadOrder::all_regular(vec!["Skyrim.esm".into()]);
    world.insert_resource(
        crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
    );
    crate::inventory::install_catalog(&mut world, &index);
    world.insert_resource(crate::cell_loader::LoadedCellIndex(std::sync::Arc::new(
        index,
    )));
    world.insert_resource(skyrim());
    let player = world.spawn();
    world.insert_resource(crate::systems::PlayerEntity(Some(player)));
    world.insert(
        player,
        Inventory {
            items: vec![ItemStack::new(CURRENCY, 500), ItemStack::new(GEM, 1)],
        },
    );
    let vendor = world.spawn();
    world.insert(vendor, FactionRanks::from_pairs([(FACTION, 0)]));

    begin_barter(&mut world, vendor).expect("vendor faction with a placed chest");
    let view = snapshot(&world).unwrap();
    // No Speech on the player: factor 3.3. 25 × 3.3 = 82.5 → 83; 100 / 3.3 → 30.
    assert_eq!(
        view.buy
            .iter()
            .map(|i| (i.form_id, i.count, i.price))
            .collect::<Vec<_>>(),
        vec![(SWORD, 2, 83)]
    );
    assert_eq!(
        view.sell
            .iter()
            .map(|i| (i.form_id, i.count, i.price))
            .collect::<Vec<_>>(),
        vec![(GEM, 1, 30)]
    );
    assert_eq!((view.player_gold, view.vendor_gold), (500, 1000));

    use byroredux_debug_ui::BarterAction;
    apply_action(&world, BarterAction::Buy { form_id: SWORD }).unwrap();
    apply_action(&world, BarterAction::Sell { form_id: GEM }).unwrap();
    assert_eq!(count(&world, player, SWORD), 1);
    assert_eq!(count(&world, player, GEM), 0);
    assert_eq!(count(&world, player, CURRENCY), 500 - 83 + 30);
    assert!(
        apply_action(&world, BarterAction::Sell { form_id: SWORD }).is_err(),
        "swords are not on this vendor's list"
    );

    end_barter(&mut world);
    assert!(session(&world).is_none());
    // #5867 — a dead vendor (a corpse's OnEnd fragment reaching
    // `ShowBarterMenu`) opens no session.
    world.register::<byroredux_core::ecs::components::Dead>();
    world.insert(vendor, byroredux_core::ecs::components::Dead);
    assert_eq!(begin_barter(&mut world, vendor), Err("that actor is dead"));
    assert!(session(&world).is_none());
    world.remove::<byroredux_core::ecs::components::Dead>(vendor);
    begin_barter(&mut world, vendor).unwrap();
    let view = snapshot(&world).unwrap();
    assert_eq!(
        view.vendor_gold,
        1000 + 83 - 30,
        "the traded stock was parked"
    );
    let stock: Vec<(u32, u32)> = view.buy.iter().map(|i| (i.form_id, i.count)).collect();
    assert_eq!(stock, vec![(SWORD, 1), (GEM, 1)]);
    end_barter(&mut world);
}
