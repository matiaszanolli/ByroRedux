//! Hand Equipment PRD (`docs/prd/hand-equipment.md`; design `docs/engine/hand-equipment.md`): what each
//! of an actor's two hands holds, on Skyrim.
//!
//! The state is the actor's `EquipmentSlots`: its `weapon` is the right hand, its `left_hand` a
//! one-handed weapon wielded in the left, and a shield one of its biped occupants. [`hand_item`] reads
//! a hand from it with [`HandCatalog`], the load order's weapon kinds and slots and its shields, so a
//! two-handed weapon (a `BothHands` slot) fills both hands. Phase 2 adds torches and spells.

use std::collections::{HashMap, HashSet};

use byroredux_core::ecs::components::{EquipmentSlots, Inventory, InventoryIndex};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::{EntityId, World};
use byroredux_plugin::esm::records::{slot_hands, EsmIndex, ItemKind, SlotHands};

/// A weapon's kind and the hands its slot takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandWeapon {
    /// Skyrim WEAP `DNAM` animation type (UESP *Skyrim Mod:Mod File Format/WEAP*: 0 Other, 1
    /// OneHandSword, 2 OneHandDagger, 3 OneHandAxe, 4 OneHandMace, 5 TwoHandSword, 6 TwoHandAxe, 7
    /// Bow, 8 Staff, 9 Crossbow).
    pub anim_type: u8,
    pub hands: SlotHands,
}

/// The load order's weapons by base FormID, and its shields (armor whose `ETYP` slot takes the left
/// hand only). Empty off Skyrim. `NOT_SAVED_BY_DESIGN`: rebuilt from the records every boot.
#[derive(Debug, Default, Clone)]
pub struct HandCatalog {
    weapons: HashMap<u32, HandWeapon>,
    shields: HashSet<u32>,
}

impl Resource for HandCatalog {}

impl HandCatalog {
    pub fn from_index(index: &EsmIndex) -> Self {
        if index.game != byroredux_plugin::esm::reader::GameKind::Skyrim {
            return Self::default();
        }
        let mut catalog = Self::default();
        for (&form, item) in &index.items {
            match &item.kind {
                ItemKind::Weapon { anim_type, equip_type, .. } => {
                    let hands = slot_hands(&index.equip_slots, *equip_type);
                    catalog.weapons.insert(form, HandWeapon { anim_type: *anim_type, hands });
                }
                ItemKind::Armor { equip_type, .. }
                    if *equip_type != 0 && slot_hands(&index.equip_slots, *equip_type) == SlotHands::Left =>
                {
                    catalog.shields.insert(form);
                }
                _ => {}
            }
        }
        catalog
    }

    pub fn weapon(&self, form: u32) -> Option<HandWeapon> {
        self.weapons.get(&form).copied()
    }

    /// Whether the weapon `form` takes both hands.
    pub fn two_handed(&self, form: u32) -> bool {
        self.weapon(form).is_some_and(|weapon| weapon.hands == SlotHands::Both)
    }

    pub fn is_shield(&self, form: u32) -> bool {
        self.shields.contains(&form)
    }

    /// Add a weapon by hand (test worlds; the records path is [`HandCatalog::from_index`]).
    pub fn insert_weapon(&mut self, form: u32, weapon: HandWeapon) {
        self.weapons.insert(form, weapon);
    }

    /// Add a shield by hand (test worlds).
    pub fn insert_shield(&mut self, form: u32) {
        self.shields.insert(form);
    }
}

/// One of the two hands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Left,
    Right,
}

/// What a hand holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandItem {
    Empty,
    /// A weapon (`both` when its slot takes both hands), by its inventory entry and base FormID.
    Weapon { index: InventoryIndex, base: u32, anim_type: u8, both: bool },
    Shield { index: InventoryIndex, base: u32 },
}

impl HandItem {
    /// CK *GetEquippedItemType - Actor*: 0 nothing, 1 one-handed sword, 2 dagger, 3 axe, 4 mace, 5
    /// two-handed sword, 6 two-handed axe / mace, 7 bow, 8 staff, 10 shield, 12 crossbow (9 a spell
    /// and 11 a torch are Phase 2's). A weapon's code is its animation type, a crossbow's (9) 12.
    pub fn item_type_code(&self) -> i32 {
        match *self {
            HandItem::Empty => 0,
            HandItem::Weapon { anim_type: 9, .. } => 12,
            HandItem::Weapon { anim_type: kind @ 1..=8, .. } => i32::from(kind),
            HandItem::Weapon { .. } => 0,
            HandItem::Shield { .. } => 10,
        }
    }

    /// The weapon's base FormID, if a weapon.
    pub fn weapon(&self) -> Option<u32> {
        match *self {
            HandItem::Weapon { base, .. } => Some(base),
            _ => None,
        }
    }
}

/// What `actor`'s `hand` holds: the right hand its wielded weapon; the left a two-handed right weapon
/// (its slot takes both hands, CK *Equip Slot*), else a weapon wielded there, else a shield it wears.
pub fn hand_item(world: &World, actor: EntityId, hand: Hand) -> HandItem {
    let Some(slots) = world.get::<EquipmentSlots>(actor).map(|slots| (*slots).clone()) else {
        return HandItem::Empty;
    };
    let base_of = |index: InventoryIndex| {
        world.get::<Inventory>(actor).and_then(|inventory| inventory.get(index).map(|stack| stack.base_form_id))
    };
    let catalog = world.try_resource::<HandCatalog>();
    let weapon = |index: InventoryIndex| {
        let base = base_of(index)?;
        let entry = catalog.as_ref().and_then(|catalog| catalog.weapon(base));
        Some(HandItem::Weapon {
            index,
            base,
            anim_type: entry.map_or(0, |weapon| weapon.anim_type),
            both: entry.is_some_and(|weapon| weapon.hands == SlotHands::Both),
        })
    };
    let right = slots.weapon.and_then(weapon).unwrap_or(HandItem::Empty);
    match hand {
        Hand::Right => right,
        Hand::Left => {
            if let HandItem::Weapon { both: true, .. } = right {
                return right;
            }
            if let Some(item) = slots.left_hand.and_then(weapon) {
                return item;
            }
            let mut worn: Vec<InventoryIndex> = slots.occupants.iter().flatten().copied().collect();
            worn.dedup();
            worn.into_iter()
                .find_map(|index| {
                    let base = base_of(index)?;
                    catalog.as_ref().is_some_and(|catalog| catalog.is_shield(base)).then_some(HandItem::Shield { index, base })
                })
                .unwrap_or(HandItem::Empty)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::ItemStack;

    const SWORD: u32 = 0x0001_2EB7;
    const DAGGER: u32 = 0x0001_397E;
    const GREATSWORD: u32 = 0x0001_359D;
    const CROSSBOW: u32 = 0x0200_0801;
    const SHIELD: u32 = 0x0001_2EB6;

    fn world() -> (World, EntityId) {
        let mut world = World::new();
        let mut catalog = HandCatalog::default();
        catalog.insert_weapon(SWORD, HandWeapon { anim_type: 1, hands: SlotHands::Right });
        catalog.insert_weapon(DAGGER, HandWeapon { anim_type: 2, hands: SlotHands::Right });
        catalog.insert_weapon(GREATSWORD, HandWeapon { anim_type: 5, hands: SlotHands::Both });
        catalog.insert_weapon(CROSSBOW, HandWeapon { anim_type: 9, hands: SlotHands::Both });
        catalog.insert_shield(SHIELD);
        world.insert_resource(catalog);
        let actor = world.spawn();
        let mut inventory = Inventory::new();
        for form in [SWORD, DAGGER, GREATSWORD, CROSSBOW, SHIELD] {
            inventory.push(ItemStack::new(form, 1));
        }
        world.insert(actor, inventory);
        world.insert(actor, EquipmentSlots::new());
        (world, actor)
    }

    fn codes(world: &World, actor: EntityId) -> (i32, i32) {
        (hand_item(world, actor, Hand::Left).item_type_code(), hand_item(world, actor, Hand::Right).item_type_code())
    }

    /// CK *GetEquippedItemType*'s codes, hand by hand: empty hands, a sword and a dagger, a sword and a
    /// shield, a two-handed weapon in both hands, a crossbow's 12.
    #[test]
    fn hands_read_what_the_slots_hold() {
        let (world, actor) = world();
        assert_eq!(codes(&world, actor), (0, 0));
        {
            let mut slots = world.query_mut::<EquipmentSlots>().unwrap();
            let slots = slots.get_mut(actor).unwrap();
            slots.equip_weapon(InventoryIndex(0));
            slots.equip_left_hand(InventoryIndex(1));
        }
        assert_eq!(codes(&world, actor), (2, 1), "a dagger left, a sword right");
        assert_eq!(hand_item(&world, actor, Hand::Left).weapon(), Some(DAGGER));
        {
            let mut slots = world.query_mut::<EquipmentSlots>().unwrap();
            let slots = slots.get_mut(actor).unwrap();
            slots.unequip_left_hand();
            slots.equip(1 << 9, InventoryIndex(4));
        }
        assert_eq!(codes(&world, actor), (10, 1), "a shield left");
        {
            let mut slots = world.query_mut::<EquipmentSlots>().unwrap();
            slots.get_mut(actor).unwrap().equip_weapon(InventoryIndex(2));
        }
        assert_eq!(codes(&world, actor), (5, 5), "a two-handed sword takes both hands");
        {
            let mut slots = world.query_mut::<EquipmentSlots>().unwrap();
            slots.get_mut(actor).unwrap().equip_weapon(InventoryIndex(3));
        }
        assert_eq!(codes(&world, actor), (12, 12), "a crossbow is 12");
    }
}
