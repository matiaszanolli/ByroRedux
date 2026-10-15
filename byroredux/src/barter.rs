//! The barter menu (M47.2): `Actor.ShowBarterMenu()` trades with the actor's
//! vendor faction.
//!
//! **Vendors.** A Skyrim / Fallout 4 `FACT` names its "Merchant Container"
//! (`VENC`, a `REFR`) and its "Vendor Buy/Sell List" (`VEND`, an `FLST`);
//! `VENV`'s "Not/Sell Buy" flag (FO4: "Buy/Sell Everything Not In List?")
//! makes the list name what the vendor does *not* trade. The vendor sells what
//! its container holds, gold included, and buys the player's items the list
//! admits: an item matches by its own FormID or by one of its `KWDA` keywords.
//!
//! **Prices.** UESP *Skyrim:Speech* §Prices:
//!
//! ```text
//! price factor = fBarterMax - (fBarterMax - fBarterMin) * min(skill, 100) / 100
//! buy price    = round(value * buy modifier * price factor)
//! sell price   = round(value * sell modifier / price factor)
//! ```
//!
//! with Skyrim.esm's fBarterMax 3.3 / fBarterMin 2.0 and the modifiers the
//! perks' *Mod Buy Prices* (8) / *Mod Sell Prices* (60) entry points compose
//! from 1.0 (vanilla `Haggling*` / `Allure` condition tab 1 on the vendor,
//! `crates/plugin/examples/perk_entry_census.rs`). Fallout 4 is the same
//! shape over Charisma (Fallout wiki *Charisma (Fallout 4)*: "Buying Price
//! Modifier = 3.50 - Charisma × 0.15", the selling modifier its reciprocal),
//! i.e. Fallout4.esm's fBarterMax 3.5 / fBarterMin 2.0 over Charisma / 10,
//! uncapped. "The minimum purchase price is 120% of the item's value and the
//! maximum selling price is 80%": Fallout4.esm's `fBarterBuyMax` 1.2 and
//! `fBarterSellMax` 0.8; Skyrim's executable carries the same pair as
//! `fBarterBuyMin` 1.05 / `fBarterSellMax` 0.95, applied the same way.
//!
//! **Stock.** A merchant container usually sits in a holding cell that is not
//! loaded. Its contents then live in the reference's parked state
//! ([`crate::cell_loader::reference_state`]): the session stands an entity in
//! for the chest under the chest's own FormID, seeds it from the `CONT` and
//! the parked row exactly as a cell load would, and parks it again when the
//! menu closes — so the stock a later cell load restores is the stock the
//! player traded with.
//!
//! Not modelled: vendor hours and radius (a script opening the menu bypasses
//! both), stolen-goods rules, restocking, FO4's perk prices (its `PERK` entry
//! layout is not decoded), and per-instance items are traded as their base.

use std::collections::{HashMap, HashSet};

use byroredux_core::ecs::components::{FactionRanks, FormIdComponent, Inventory};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Resource, World};
use byroredux_core::form_id::FormIdPool;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{EsmIndex, VendorData};
use byroredux_scripting::condition::{evaluate_function, ConditionFunction};

/// Gold (Skyrim `Gold001`) and caps (Fallout 4 `Caps001`).
const CURRENCY: u32 = 0x0000_000F;

/// Skyrim's *Mod Buy Prices* / *Mod Sell Prices* entry points
/// (`wbEntryPointsEnum`, xEdit `wbDefinitionsTES5`).
const SKYRIM_MOD_BUY_PRICES: u8 = 8;
const SKYRIM_MOD_SELL_PRICES: u8 = 60;

/// The load order's vendors and price rules. `NOT_SAVED_BY_DESIGN`: rebuilt
/// from the records every boot.
#[derive(Debug, Clone)]
pub(crate) struct BarterRules {
    /// The actor value prices scale with (Speechcraft / Charisma).
    skill: u32,
    /// The skill value that reaches `barter_min`.
    skill_scale: f32,
    /// Skyrim: "skill levels over 100 have no effect".
    skill_cap: Option<f32>,
    barter_max: f32,
    barter_min: f32,
    /// Purchase price floor, a multiple of the item's value.
    buy_floor: f32,
    /// Sale price ceiling, a multiple of the item's value.
    sell_ceiling: f32,
    /// The (buy, sell) perk entry points, where the game's are decoded.
    perk_hooks: Option<(u8, u8)>,
    /// Vendor faction → its merchant data.
    vendors: HashMap<u32, VendorData>,
    /// Merchant container `REFR` → its base `CONT`.
    containers: HashMap<u32, u32>,
    /// `VEND` lists' entries.
    lists: HashMap<u32, HashSet<u32>>,
    /// Item → its keywords.
    item_keywords: HashMap<u32, Vec<u32>>,
}

impl Resource for BarterRules {}

impl BarterRules {
    fn from_index(index: &EsmIndex) -> Option<Self> {
        let (skill, skill_scale, skill_cap, buy_floor_setting, buy_floor, sell_ceiling, hooks) =
            match index.game {
                // Executable defaults, overridden by the masters' GMSTs.
                GameKind::Skyrim => (
                    "Speechcraft",
                    100.0,
                    Some(100.0),
                    "fBarterBuyMin",
                    1.05,
                    0.95,
                    Some((SKYRIM_MOD_BUY_PRICES, SKYRIM_MOD_SELL_PRICES)),
                ),
                GameKind::Fallout4 => ("Charisma", 10.0, None, "fBarterBuyMax", 1.05, 0.95, None),
                _ => return None,
            };
        let (barter_max, barter_min) = match index.game {
            GameKind::Skyrim => (2.4, 1.4),
            _ => (2.5, 1.5),
        };
        let setting = |name: &str, default: f32| index.game_setting_float(name).unwrap_or(default);
        let vendors: HashMap<u32, VendorData> = index
            .factions
            .values()
            .filter_map(|faction| Some((faction.form_id, faction.vendor.clone()?)))
            .filter(|(_, vendor)| vendor.container != 0)
            .collect();
        let wanted: HashSet<u32> = vendors.values().map(|vendor| vendor.container).collect();
        let mut containers = HashMap::new();
        let cells = &index.cells;
        let all_cells = cells
            .cells
            .values()
            .chain(cells.exterior_cells.values().flat_map(HashMap::values))
            .chain(cells.worldspace_persistent_cells.values());
        for cell in all_cells {
            for placed in &cell.references {
                if wanted.contains(&placed.form_id) {
                    containers.insert(placed.form_id, placed.base_form_id);
                }
            }
        }
        let lists = vendors
            .values()
            .filter(|vendor| vendor.buy_sell_list != 0)
            .filter_map(|vendor| {
                let list = index.form_lists.get(&vendor.buy_sell_list)?;
                Some((list.form_id, list.entries.iter().copied().collect()))
            })
            .collect();
        let item_keywords = index
            .items
            .values()
            .filter(|item| !item.common.keywords.is_empty())
            .map(|item| (item.form_id, item.common.keywords.clone()))
            .collect();
        Some(Self {
            skill: index.actor_value_form_id(skill)?,
            skill_scale,
            skill_cap,
            barter_max: setting("fBarterMax", barter_max),
            barter_min: setting("fBarterMin", barter_min),
            buy_floor: setting(buy_floor_setting, buy_floor),
            sell_ceiling: setting("fBarterSellMax", sell_ceiling),
            perk_hooks: hooks,
            vendors,
            containers,
            lists,
            item_keywords,
        })
    }

    /// The price factor at `skill`.
    fn price_factor(&self, skill: f32) -> f32 {
        let skill = self.skill_cap.map_or(skill, |cap| skill.min(cap)).max(0.0);
        self.barter_max - (self.barter_max - self.barter_min) * skill / self.skill_scale
    }

    /// What the player pays for an item worth `value`.
    fn buy_price(&self, value: u32, factor: f32, modifier: f32) -> u32 {
        let value = value as f32;
        (value * modifier * factor)
            .round()
            .max((value * self.buy_floor).round()) as u32
    }

    /// What the player is paid for an item worth `value`.
    fn sell_price(&self, value: u32, factor: f32, modifier: f32) -> u32 {
        let value = value as f32;
        (value * modifier / factor)
            .round()
            .min((value * self.sell_ceiling).round()) as u32
    }

    /// Whether `vendor` buys `item`.
    fn buys(&self, vendor: &VendorData, item: u32) -> bool {
        let in_list = self.lists.get(&vendor.buy_sell_list).is_some_and(|list| {
            list.contains(&item)
                || self
                    .item_keywords
                    .get(&item)
                    .is_some_and(|keywords| keywords.iter().any(|k| list.contains(k)))
        });
        in_list != vendor.everything_not_in_list
    }
}

/// Install (or remove) [`BarterRules`]; returns how many vendor factions.
pub(crate) fn install_barter_rules(world: &mut World, index: &EsmIndex) -> usize {
    match BarterRules::from_index(index) {
        Some(rules) => {
            let count = rules.vendors.len();
            world.insert_resource(rules);
            count
        }
        None => {
            world.remove_resource::<BarterRules>();
            0
        }
    }
}

/// The open barter: the vendor, its faction, and the entity holding its
/// stock (the merchant container, or a stand-in for an unloaded one).
#[derive(Debug, Clone, Copy)]
struct BarterSession {
    vendor: EntityId,
    faction: u32,
    stock: EntityId,
    stand_in: bool,
}

/// The barter session in progress. `NOT_SAVED_BY_DESIGN`: the menu pauses
/// the game and every page-leave path ends the session (parking a stand-in's
/// stock) before a save can run.
#[derive(Debug, Default)]
pub(crate) struct ActiveBarter(Option<BarterSession>);

impl Resource for ActiveBarter {}

fn player(world: &World) -> Option<EntityId> {
    world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0)
}

/// Open a barter session with `vendor`'s merchant faction.
pub(crate) fn begin_barter(world: &mut World, vendor: EntityId) -> Result<(), &'static str> {
    end_barter(world);
    let (faction, container) = {
        let rules = world
            .try_resource::<BarterRules>()
            .ok_or("this game has no barter rules")?;
        let ranks = world.get::<FactionRanks>(vendor).ok_or("not a vendor")?;
        ranks
            .0
            .iter()
            .find_map(|(faction, _)| Some((*faction, rules.vendors.get(faction)?.container)))
            .ok_or("not a vendor")?
    };
    let (stock, stand_in) =
        match byroredux_scripting::condition::resolve_entity_by_global_form_id(world, container) {
            Some(stock) => (stock, false),
            None => (stand_in_container(world, container)?, true),
        };
    world.init_resource::<ActiveBarter>();
    world.resource_mut::<ActiveBarter>().0 = Some(BarterSession {
        vendor,
        faction,
        stock,
        stand_in,
    });
    Ok(())
}

/// Stand an entity in for the unloaded merchant container `container`: its
/// own FormID, its `CONT` contents, then its parked state, as a cell load
/// would attach them.
fn stand_in_container(world: &mut World, container: u32) -> Result<EntityId, &'static str> {
    let base = world
        .try_resource::<BarterRules>()
        .and_then(|rules| rules.containers.get(&container).copied())
        .ok_or("merchant container not placed")?;
    let pair = world
        .try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
        .and_then(|resolver| resolver.resolve(container))
        .ok_or("merchant container outside the load order")?;
    let index = world
        .try_resource::<crate::cell_loader::LoadedCellIndex>()
        .map(|index| std::sync::Arc::clone(&index.0))
        .ok_or("no record index")?;
    if world.try_resource::<FormIdPool>().is_none() {
        world.insert_resource(FormIdPool::new());
    }
    let id = world.resource_mut::<FormIdPool>().intern(pair);
    let entity = world.spawn();
    world.insert(entity, FormIdComponent(id));
    if !crate::cell_loader::references::attach_container_inventory(world, entity, base, &index) {
        world.despawn(entity);
        return Err("merchant container has no CONT record");
    }
    Ok(entity)
}

/// Close the barter session, parking a stand-in's stock again.
pub(crate) fn end_barter(world: &mut World) {
    let Some(session) = world
        .try_resource_mut::<ActiveBarter>()
        .and_then(|mut active| active.0.take())
    else {
        return;
    };
    if session.stand_in {
        crate::cell_loader::reference_state::capture(world, &[session.stock]);
        crate::cell_loader::unload::release_victim_item_instances(world, &[session.stock]);
        world.despawn(session.stock);
    }
}

fn session(world: &World) -> Option<BarterSession> {
    world
        .try_resource::<ActiveBarter>()
        .and_then(|active| active.0)
}

/// `form`'s count in `entity`'s inventory.
fn count_of(world: &World, entity: EntityId, form: u32) -> u32 {
    world.get::<Inventory>(entity).map_or(0, |inventory| {
        inventory
            .items
            .iter()
            .filter(|stack| stack.base_form_id == form)
            .map(|stack| stack.count)
            .sum()
    })
}

/// Item counts by base form, in first-held order.
fn grouped(world: &World, entity: EntityId) -> Vec<(u32, u32)> {
    let mut rows: Vec<(u32, u32)> = Vec::new();
    if let Some(inventory) = world.get::<Inventory>(entity) {
        for stack in inventory.items.iter().filter(|stack| stack.count > 0) {
            match rows
                .iter_mut()
                .find(|(form, _)| *form == stack.base_form_id)
            {
                Some(row) => row.1 += stack.count,
                None => rows.push((stack.base_form_id, stack.count)),
            }
        }
    }
    rows
}

/// The session's (price factor, buy modifier, sell modifier).
fn price_terms(
    world: &World,
    rules: &BarterRules,
    player: EntityId,
    vendor: EntityId,
) -> (f32, f32, f32) {
    let skill = evaluate_function(
        ConditionFunction::GetActorValue,
        &byroredux_plugin::esm::records::condition::Condition {
            param_1: rules.skill,
            ..Default::default()
        },
        player,
        world,
    );
    let (buy, sell) = rules.perk_hooks.map_or((1.0, 1.0), |(buy, sell)| {
        let subjects = [Some(vendor)];
        (
            byroredux_scripting::perk_entry_points::compose_entry_point(
                world, player, buy, 1.0, &subjects,
            ),
            byroredux_scripting::perk_entry_points::compose_entry_point(
                world, player, sell, 1.0, &subjects,
            ),
        )
    });
    (rules.price_factor(skill), buy, sell)
}

/// The barter page's view of the open session.
pub(crate) fn snapshot(world: &World) -> Option<byroredux_debug_ui::BarterSnapshot> {
    let session = session(world)?;
    let player = player(world)?;
    let rules = world.try_resource::<BarterRules>()?.clone();
    let vendor_data = rules.vendors.get(&session.faction)?.clone();
    let (factor, buy_mod, sell_mod) = price_terms(world, &rules, player, session.vendor);
    let catalog = world
        .try_resource::<crate::inventory::InventoryCatalog>()?
        .clone();
    let view = |form: u32, count: u32, price: u32| {
        let name = catalog
            .sdk_metadata(form)
            .map_or_else(|| format!("{form:08X}"), |item| item.name().to_owned());
        byroredux_debug_ui::BarterItemView {
            form_id: form,
            name,
            count,
            price,
        }
    };
    let value = |form: u32| catalog.sdk_metadata(form).map(|item| item.value());
    let buy = grouped(world, session.stock)
        .into_iter()
        .filter(|(form, _)| *form != CURRENCY)
        .filter_map(|(form, count)| {
            Some(view(
                form,
                count,
                rules.buy_price(value(form)?, factor, buy_mod),
            ))
        })
        .collect();
    let sell = grouped(world, player)
        .into_iter()
        .filter(|(form, _)| *form != CURRENCY && rules.buys(&vendor_data, *form))
        .filter_map(|(form, count)| {
            Some(view(
                form,
                count,
                rules.sell_price(value(form)?, factor, sell_mod),
            ))
        })
        .collect();
    let vendor_name = byroredux_scripting::relationships::actor_base(world, session.vendor)
        .and_then(|base| {
            world
                .try_resource::<byroredux_scripting::MessageCatalog>()?
                .display_name(base)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Merchant".to_owned());
    Some(byroredux_debug_ui::BarterSnapshot {
        vendor_name,
        player_gold: count_of(world, player, CURRENCY),
        vendor_gold: count_of(world, session.stock, CURRENCY),
        buy,
        sell,
    })
}

/// Buy or sell one item; `Err` says why the trade was refused.
pub(crate) fn apply_action(
    world: &World,
    action: byroredux_debug_ui::BarterAction,
) -> Result<(), &'static str> {
    use byroredux_debug_ui::BarterAction;
    let snapshot = snapshot(world).ok_or("no barter in progress")?;
    let session = session(world).ok_or("no barter in progress")?;
    let player = player(world).ok_or("no player")?;
    match action {
        BarterAction::Buy { form_id } => {
            let item = snapshot
                .buy
                .iter()
                .find(|item| item.form_id == form_id)
                .ok_or("the merchant has none left")?;
            if snapshot.player_gold < item.price {
                return Err("You don't have enough gold.");
            }
            byroredux_scripting::remove_items(world, session.stock, form_id, 1, Some(player));
            byroredux_scripting::remove_items(
                world,
                player,
                CURRENCY,
                item.price,
                Some(session.stock),
            );
        }
        BarterAction::Sell { form_id } => {
            let item = snapshot
                .sell
                .iter()
                .find(|item| item.form_id == form_id)
                .ok_or("the merchant won't buy that")?;
            if snapshot.vendor_gold < item.price {
                return Err("The merchant doesn't have enough gold.");
            }
            byroredux_scripting::remove_items(world, player, form_id, 1, Some(session.stock));
            byroredux_scripting::remove_items(
                world,
                session.stock,
                CURRENCY,
                item.price,
                Some(player),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
