//! Hand Equipment PRD (`docs/prd/hand-equipment.md`; design `docs/engine/hand-equipment.md`): what each
//! of an actor's two hands holds, on Skyrim.
//!
//! The state is the actor's `EquipmentSlots`: its `weapon` is the right hand, its `left_hand` a
//! one-handed weapon or a torch held in the left, `left_spell` / `right_spell` a spell readied in a hand,
//! and a shield one of its biped occupants. [`hand_item`] reads a hand from it with [`HandCatalog`], the
//! load order's weapon kinds and slots, its shields, torches and spell slots, so a two-handed weapon or
//! spell (a `BothHands` slot) fills both hands. [`equip_to_hand`] is the one writer of the hands: the
//! inventory menu, the console and the scripts' equips all go through it.

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

/// Missing weapon `ETYP` uses the one-hand fallback. An authored slot must resolve to a hand.
pub fn weapon_slot_hands(index: &EsmIndex, equip_type: u32) -> SlotHands {
    if equip_type == 0 {
        SlotHands::Either
    } else {
        slot_hands(&index.equip_slots, equip_type)
    }
}

/// The load order's weapons by base FormID, its shields (armor whose `ETYP` slot takes the left hand
/// only), its torches (lights with the "can be carried" flag) and its spells' slots. Empty off Skyrim.
/// `NOT_SAVED_BY_DESIGN`: rebuilt from the records every boot.
#[derive(Debug, Default, Clone)]
pub struct HandCatalog {
    weapons: HashMap<u32, HandWeapon>,
    shields: HashSet<u32>,
    torches: HashSet<u32>,
    spells: HashMap<u32, SlotHands>,
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
                    let hands = weapon_slot_hands(index, *equip_type);
                    catalog.weapons.insert(form, HandWeapon { anim_type: *anim_type, hands });
                }
                ItemKind::Armor { equip_type, .. }
                    if *equip_type != 0 && slot_hands(&index.equip_slots, *equip_type) == SlotHands::Left =>
                {
                    catalog.shields.insert(form);
                }
                // Only a carried light is an item (`parse_carryable_light`); it goes in the left hand
                // (UESP *Skyrim:Block*: "equip one to your left hand").
                ItemKind::Light { .. } => {
                    catalog.torches.insert(form);
                }
                _ => {}
            }
        }
        for (&form, spell) in &index.spells {
            if spell.equip_type != 0 {
                catalog.spells.insert(form, slot_hands(&index.equip_slots, spell.equip_type));
            }
        }
        catalog
    }

    /// Whether the load order has a hand model at all (Skyrim).
    pub fn is_active(&self) -> bool {
        !self.weapons.is_empty() || !self.shields.is_empty() || !self.torches.is_empty()
    }

    pub fn weapon(&self, form: u32) -> Option<HandWeapon> {
        self.weapons.get(&form).copied()
    }

    pub fn weapon_hands(&self, form: u32) -> SlotHands {
        self.weapon(form).map_or(SlotHands::Either, |weapon| weapon.hands)
    }

    /// Whether the weapon `form` takes both hands.
    pub fn two_handed(&self, form: u32) -> bool {
        self.weapon_hands(form) == SlotHands::Both
    }

    pub fn is_shield(&self, form: u32) -> bool {
        self.shields.contains(&form)
    }

    pub fn is_torch(&self, form: u32) -> bool {
        self.torches.contains(&form)
    }

    /// The hands the spell `form`'s slot names; `None` for a spell with no slot.
    pub fn spell_hands(&self, form: u32) -> Option<SlotHands> {
        self.spells.get(&form).copied()
    }

    /// Add a weapon by hand (test worlds; the records path is [`HandCatalog::from_index`]).
    pub fn insert_weapon(&mut self, form: u32, weapon: HandWeapon) {
        self.weapons.insert(form, weapon);
    }

    /// Add a shield by hand (test worlds).
    pub fn insert_shield(&mut self, form: u32) {
        self.shields.insert(form);
    }

    /// Add a torch by hand (test worlds).
    pub fn insert_torch(&mut self, form: u32) {
        self.torches.insert(form);
    }

    /// Add a spell's slot by hand (test worlds).
    pub fn insert_spell(&mut self, form: u32, hands: SlotHands) {
        self.spells.insert(form, hands);
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
    Torch { index: InventoryIndex, base: u32 },
    /// A spell readied in the hand, by FormID.
    Spell { form: u32 },
}

impl HandItem {
    /// CK *GetEquippedItemType - Actor*: 0 nothing, 1 one-handed sword, 2 dagger, 3 axe, 4 mace, 5
    /// two-handed sword, 6 two-handed axe / mace, 7 bow, 8 staff, 9 a spell, 10 shield, 11 torch, 12
    /// crossbow. A weapon's code is its animation type, a crossbow's (9) 12.
    pub fn item_type_code(&self) -> i32 {
        match *self {
            HandItem::Empty => 0,
            HandItem::Weapon { anim_type: 9, .. } => 12,
            HandItem::Weapon { anim_type: kind @ 1..=8, .. } => i32::from(kind),
            HandItem::Weapon { .. } => 0,
            HandItem::Spell { .. } => 9,
            HandItem::Shield { .. } => 10,
            HandItem::Torch { .. } => 11,
        }
    }

    /// The weapon's base FormID, if a weapon.
    pub fn weapon(&self) -> Option<u32> {
        match *self {
            HandItem::Weapon { base, .. } => Some(base),
            _ => None,
        }
    }

    /// The spell's FormID, if a spell.
    pub fn spell(&self) -> Option<u32> {
        match *self {
            HandItem::Spell { form } => Some(form),
            _ => None,
        }
    }

    /// The base FormID of the item or spell the hand holds; `None` when empty.
    pub fn form(&self) -> Option<u32> {
        match *self {
            HandItem::Empty => None,
            HandItem::Weapon { base, .. } | HandItem::Shield { base, .. } | HandItem::Torch { base, .. } => Some(base),
            HandItem::Spell { form } => Some(form),
        }
    }
}

/// What `actor`'s `hand` holds: the right hand its readied spell, else its wielded weapon; the left a
/// two-handed right weapon (its slot takes both hands, CK *Equip Slot*), else a spell readied there,
/// else a weapon or torch held there, else a shield it wears.
pub fn hand_item(world: &World, actor: EntityId, hand: Hand) -> HandItem {
    let Some(slots) = world.get::<EquipmentSlots>(actor).map(|slots| (*slots).clone()) else {
        return HandItem::Empty;
    };
    let base_of = |index: InventoryIndex| {
        world.get::<Inventory>(actor).and_then(|inventory| inventory.get(index).map(|stack| stack.base_form_id))
    };
    let catalog = world.try_resource::<HandCatalog>();
    let held = |index: InventoryIndex| {
        let base = base_of(index)?;
        if catalog.as_ref().is_some_and(|catalog| catalog.is_torch(base)) {
            return Some(HandItem::Torch { index, base });
        }
        let entry = catalog.as_ref().and_then(|catalog| catalog.weapon(base));
        Some(HandItem::Weapon {
            index,
            base,
            anim_type: entry.map_or(0, |weapon| weapon.anim_type),
            both: entry.is_some_and(|weapon| weapon.hands == SlotHands::Both),
        })
    };
    let right_weapon = slots.weapon.and_then(held);
    match hand {
        Hand::Right => match slots.right_spell {
            Some(form) => HandItem::Spell { form },
            None => right_weapon.unwrap_or(HandItem::Empty),
        },
        Hand::Left => {
            if let Some(item @ HandItem::Weapon { both: true, .. }) = right_weapon {
                return item;
            }
            if let Some(form) = slots.left_spell {
                return HandItem::Spell { form };
            }
            if let Some(item) = slots.left_hand.and_then(held) {
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

/// Whether `actor` holds a torch in either hand (CK *IsTorchOut*: "a light object equipped with the
/// 'This light can be carried' box checked").
pub fn torch_out(world: &World, actor: EntityId) -> bool {
    [Hand::Left, Hand::Right].into_iter().any(|hand| matches!(hand_item(world, actor, hand), HandItem::Torch { .. }))
}

/// What goes into a hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandEquip {
    /// A weapon's inventory entry and resolved equip slot.
    Weapon { index: InventoryIndex, hands: SlotHands },
    /// A shield's inventory entry and its biped mask (its model's slot).
    Shield { index: InventoryIndex, mask: u64 },
    Torch { index: InventoryIndex },
    /// A spell by FormID, with the hands its slot names.
    Spell { form: u32, hands: SlotHands },
}

/// What [`equip_to_hand`] needs to know of the actor's hands besides its slots, read before they are
/// borrowed: [`hand_facts`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandFacts {
    /// The right hand holds something that takes both hands (a two-handed weapon or spell).
    pub right_two_handed: bool,
    /// The shields the actor wears (their entries), which an item taking the left hand takes off.
    pub shields_worn: Vec<InventoryIndex>,
    /// How many of the item the row holds: one item cannot be in both hands.
    pub stack_count: u32,
}

/// The facts [`equip_to_hand`] needs of `actor`, for the inventory row `row` it is about to equip.
pub fn hand_facts(world: &World, actor: EntityId, row: Option<InventoryIndex>) -> HandFacts {
    let catalog = world.try_resource::<HandCatalog>();
    let Some(slots) = world.get::<EquipmentSlots>(actor).map(|slots| (*slots).clone()) else {
        return HandFacts::default();
    };
    let inventory = world.get::<Inventory>(actor);
    let base = |idx: InventoryIndex| inventory.as_ref().and_then(|inventory| inventory.get(idx).map(|stack| stack.base_form_id));
    let is_shield = |form: u32| catalog.as_ref().is_some_and(|catalog| catalog.is_shield(form));
    let mut shields_worn: Vec<InventoryIndex> =
        slots.occupants.iter().flatten().copied().filter(|&idx| base(idx).is_some_and(is_shield)).collect();
    shields_worn.dedup();
    let right_weapon_two_handed =
        slots.weapon.and_then(base).is_some_and(|form| catalog.as_ref().is_some_and(|catalog| catalog.two_handed(form)));
    let right_spell_two_handed = slots
        .right_spell
        .is_some_and(|form| catalog.as_ref().and_then(|catalog| catalog.spell_hands(form)) == Some(SlotHands::Both));
    HandFacts {
        right_two_handed: right_weapon_two_handed || right_spell_two_handed,
        shields_worn,
        stack_count: row
            .and_then(|row| inventory.as_ref().and_then(|inventory| inventory.get(row).map(|stack| stack.count)))
            .unwrap_or(0),
    }
}

/// What an equip took off: inventory entries, spells, and whether the right hand's weapon changed (the
/// wielded-weapon cache follows it).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandChange {
    pub items: Vec<InventoryIndex>,
    pub spells: Vec<u32>,
    pub right_weapon_changed: bool,
}

/// The hands `item` takes when equipped into `hand`, as (left, right). An explicit non-hand slot
/// has no destination.
pub fn hands_taken(item: HandEquip, hand: Hand) -> Option<(bool, bool)> {
    let asked = (hand == Hand::Left, hand == Hand::Right);
    Some(match item {
        HandEquip::Weapon { hands: SlotHands::Right | SlotHands::Either, .. } => asked,
        HandEquip::Shield { .. } | HandEquip::Torch { .. } => (true, false),
        // CK *EquipSpell - Actor*: the source "will be overridden by the Spell's Equip Slot"; a
        // BothHands slot takes both (HND-SLOT-002).
        HandEquip::Weapon { hands: SlotHands::Both, .. } | HandEquip::Spell { hands: SlotHands::Both, .. } => (true, true),
        HandEquip::Weapon { hands: SlotHands::Left, .. } | HandEquip::Spell { hands: SlotHands::Left, .. } => (true, false),
        HandEquip::Spell { hands: SlotHands::Right, .. } => (false, true),
        HandEquip::Spell { hands: SlotHands::Either, .. } => asked,
        HandEquip::Weapon { hands: SlotHands::None, .. } | HandEquip::Spell { hands: SlotHands::None, .. } => return None,
    })
}

/// Equip `item` into `hand` by the equip-slot rules (CK *Equip Slot*: "Items marked with the same slot
/// can't be equipped simultaneously. Attempting to equip one will automatically unequip the other"):
/// what held each hand it takes comes off first, a two-handed occupant from both hands, and one item
/// of a stack moved to the other hand leaves the first. Returns what came off. A non-hand slot
/// returns `None` without changing occupancy.
pub fn equip_to_hand(slots: &mut EquipmentSlots, item: HandEquip, hand: Hand, facts: &HandFacts) -> Option<HandChange> {
    let (into_left, into_right) = hands_taken(item, hand)?;
    let (mut left, mut right) = (into_left, into_right);
    if facts.right_two_handed {
        // A two-handed occupant fills both hands: taking either frees both.
        (left, right) = (true, true);
    }
    let index = match item {
        HandEquip::Weapon { index, .. } | HandEquip::Shield { index, .. } | HandEquip::Torch { index } => Some(index),
        HandEquip::Spell { .. } => None,
    };
    // A single item moves rather than doubles. Clear an older right-hand copy of a fixed-left weapon too.
    if let Some(index) = index.filter(|_| {
        facts.stack_count < 2 || matches!(item, HandEquip::Weapon { hands: SlotHands::Left, .. })
    }) {
        left |= slots.left_hand == Some(index);
        right |= slots.weapon == Some(index);
    }
    let mut change = HandChange::default();
    if left {
        change.items.extend(slots.left_hand.take());
        change.spells.extend(slots.left_spell.take());
        for &shield in &facts.shields_worn {
            if slots.release(shield) {
                change.items.push(shield);
            }
        }
    }
    if right {
        if slots.weapon.take().inspect(|&weapon| change.items.push(weapon)).is_some() {
            change.right_weapon_changed = true;
        }
        if let Some(spell) = slots.right_spell.take()
            && !change.spells.contains(&spell)
        {
            change.spells.push(spell);
        }
    }
    match item {
        HandEquip::Weapon { index, .. } => {
            if into_right {
                slots.weapon = Some(index);
                change.right_weapon_changed = true;
            } else {
                slots.left_hand = Some(index);
            }
        }
        HandEquip::Shield { index, mask } => change.items.extend(slots.equip(mask, index)),
        HandEquip::Torch { index } => slots.left_hand = Some(index),
        HandEquip::Spell { form, .. } => {
            if into_left {
                slots.left_spell = Some(form);
            }
            if into_right {
                slots.right_spell = Some(form);
            }
        }
    }
    if let Some(index) = index {
        change.items.retain(|&taken| taken != index || !slots.is_equipped(index));
    }
    if let HandEquip::Spell { form, .. } = item {
        change.spells.retain(|&taken| taken != form || (slots.left_spell != Some(form) && slots.right_spell != Some(form)));
    }
    change.items.dedup();
    Some(change)
}

/// Take the spell `form` out of `hand` (CK *UnequipSpell - Actor*), or out of both hands when its
/// slot takes both. Returns whether it was there.
pub fn unequip_spell(slots: &mut EquipmentSlots, form: u32, hand: Hand, hands: Option<SlotHands>) -> bool {
    let both = hands == Some(SlotHands::Both);
    let mut taken = false;
    if (hand == Hand::Left || both) && slots.left_spell == Some(form) {
        slots.left_spell = None;
        taken = true;
    }
    if (hand == Hand::Right || both) && slots.right_spell == Some(form) {
        slots.right_spell = None;
        taken = true;
    }
    taken
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
    const TORCH: u32 = 0x0001_D4EC;
    const FLAMES: u32 = 0x0001_2FCD;
    const RITUAL: u32 = 0x0010_F7EE;

    fn world() -> (World, EntityId) {
        let mut world = World::new();
        let mut catalog = HandCatalog::default();
        catalog.insert_weapon(SWORD, HandWeapon { anim_type: 1, hands: SlotHands::Right });
        catalog.insert_weapon(DAGGER, HandWeapon { anim_type: 2, hands: SlotHands::Right });
        catalog.insert_weapon(GREATSWORD, HandWeapon { anim_type: 5, hands: SlotHands::Both });
        catalog.insert_weapon(CROSSBOW, HandWeapon { anim_type: 9, hands: SlotHands::Both });
        catalog.insert_shield(SHIELD);
        catalog.insert_torch(TORCH);
        catalog.insert_spell(FLAMES, SlotHands::Either);
        catalog.insert_spell(RITUAL, SlotHands::Both);
        world.insert_resource(catalog);
        let actor = world.spawn();
        let mut inventory = Inventory::new();
        for form in [SWORD, DAGGER, GREATSWORD, CROSSBOW, SHIELD, TORCH] {
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

    /// Equip `item` into `hand` on the actor through the one writer, as the callers do.
    fn equip(world: &World, actor: EntityId, item: HandEquip, hand: Hand) -> HandChange {
        let row = match item {
            HandEquip::Weapon { index, .. } | HandEquip::Shield { index, .. } | HandEquip::Torch { index } => Some(index),
            HandEquip::Spell { .. } => None,
        };
        let facts = hand_facts(world, actor, row);
        let mut slots = world.query_mut::<EquipmentSlots>().unwrap();
        equip_to_hand(slots.get_mut(actor).unwrap(), item, hand, &facts).expect("a hand item")
    }

    /// The Phase 2 hand rules: a torch takes the left hand (CK *IsTorchOut*'s 11), a spell its hand
    /// (9) and a two-handed one both, and whatever held a hand comes off, a two-handed occupant from
    /// both hands.
    #[test]
    fn torches_and_spells_take_their_hands_by_the_slot_rules() {
        let (world, actor) = world();
        let sword = HandEquip::Weapon { index: InventoryIndex(0), hands: SlotHands::Right };
        let dagger = HandEquip::Weapon { index: InventoryIndex(1), hands: SlotHands::Right };
        equip(&world, actor, sword, Hand::Right);
        equip(&world, actor, dagger, Hand::Left);
        let change = equip(&world, actor, HandEquip::Torch { index: InventoryIndex(5) }, Hand::Right);
        assert_eq!(change.items, vec![InventoryIndex(1)], "the torch takes the dagger's hand");
        assert_eq!(codes(&world, actor), (11, 1));
        assert!(torch_out(&world, actor));

        let change = equip(&world, actor, HandEquip::Spell { form: FLAMES, hands: SlotHands::Either }, Hand::Right);
        assert_eq!((change.items, change.right_weapon_changed), (vec![InventoryIndex(0)], true), "the spell takes the sword's hand");
        assert_eq!(codes(&world, actor), (11, 9));
        assert_eq!(hand_item(&world, actor, Hand::Right).spell(), Some(FLAMES));
        // The same spell in the other hand too (one spell readied twice).
        let change = equip(&world, actor, HandEquip::Spell { form: FLAMES, hands: SlotHands::Either }, Hand::Left);
        assert_eq!((change.items, change.spells), (vec![InventoryIndex(5)], vec![]), "the torch comes off");
        assert_eq!(codes(&world, actor), (9, 9));
        assert!(!torch_out(&world, actor));

        let change = equip(&world, actor, HandEquip::Spell { form: RITUAL, hands: SlotHands::Both }, Hand::Left);
        assert_eq!(change.spells, vec![FLAMES], "a two-handed spell takes both hands");
        assert_eq!(world.get::<EquipmentSlots>(actor).map(|slots| (slots.left_spell, slots.right_spell)), Some((Some(RITUAL), Some(RITUAL))));
        let change = equip(&world, actor, HandEquip::Shield { index: InventoryIndex(4), mask: 1 << 9 }, Hand::Left);
        assert_eq!(change.spells, vec![RITUAL], "a shield frees both hands of a two-handed spell");
        assert_eq!(codes(&world, actor), (10, 0));

        // A greatsword takes the shield off; a torch then frees both hands of it.
        equip(&world, actor, HandEquip::Weapon { index: InventoryIndex(2), hands: SlotHands::Both }, Hand::Right);
        assert_eq!(codes(&world, actor), (5, 5));
        let change = equip(&world, actor, HandEquip::Torch { index: InventoryIndex(5) }, Hand::Left);
        assert_eq!((change.items, change.right_weapon_changed), (vec![InventoryIndex(2)], true));
        assert_eq!(codes(&world, actor), (11, 0));

        // A Voice power is no hand's.
        let facts = hand_facts(&world, actor, None);
        let mut slots = world.query_mut::<EquipmentSlots>().unwrap();
        let slots = slots.get_mut(actor).unwrap();
        assert_eq!(equip_to_hand(slots, HandEquip::Spell { form: 7, hands: SlotHands::None }, Hand::Left, &facts), None);
        slots.right_spell = Some(FLAMES);
        assert!(unequip_spell(slots, FLAMES, Hand::Right, Some(SlotHands::Either)));
        assert!(!unequip_spell(slots, FLAMES, Hand::Right, Some(SlotHands::Either)));
    }
}
