//! SKSE's `Actor` equipment reads (LMC-SCR-004 catalog growth, `docs/engine/extender-catalog.md` §12):
//! `GetWornForm`, `GetWornItemId`, `GetEquippedObject`, `GetEquippedItemId` and `IsSwimming`, on Skyrim.
//!
//! SKSE64's `PapyrusActor.cpp` at the catalog's pinned commit:
//! - `GetWornForm(mask)` is the first worn item in the actor's inventory order whose biped slots share a bit with
//!   `mask` (`MatchBySlot`: `parts & mask != 0`); only armor and clothes have biped slots, so a weapon never matches.
//!   The engine's worn slots are bit positions of the same mask, so the worn entries whose slot bit is in `mask` are
//!   the candidates and the lowest inventory entry is the first;
//! - `GetEquippedObject(slot)`: `2` is the voice (the equipped shout), `0` the left hand, and any other number the
//!   right (`slot == kSlotID_Left`), which this keeps. A hand holds a weapon, a spell, a shield or a torch;
//! - `GetEquippedItemId(slot)` and `GetWornItemId(mask)` are `HashItemId(name, formID)`
//!   (`HashUtil.cpp`): the CRC-32 of the item's name continued from its plugin-local object id (24 bits, 12 for a light
//!   plugin), with the name from the entry's display-name extra data (not served: a mod's rename) else the base name;
//!   `0` for no item, for an item with no name, and for a hand holding a spell, which is no inventory entry;
//! - `IsSwimming` is the actor's swimming state flag, here the character system's walk/swim verdict
//!   ([`crate::obscript_state::SwimQuery`]).
//!
//! Not served, and stopped with their reason: the voice slot (the equipped shout is not tracked), the actor's
//! `addedSpells` (`GetSpellCount` / `GetNthSpell` count only the spells a script added, which the engine's spell list
//! does not separate from the record's and the race's) and `IsAIEnabled` (no state).

use super::*;
use crate::form_facts::FormFacts;
use crate::hands::{hand_item, Hand};
use byroredux_core::ecs::components::{EquipmentSlots, Inventory};

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &["getwornform", "getwornitemid", "getequippedobject", "getequippeditemid", "isswimming"];

/// SKSE64's `HashUtil::CRC32(str, start)`: the CRC-32 (reflected, polynomial `0xEDB88320`) of `bytes` whose register
/// starts at `!start`.
fn crc32_from(bytes: &[u8], start: u32) -> u32 {
    const TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut value = i as u32;
            let mut bit = 0;
            while bit < 8 {
                value = if value & 1 != 0 { 0xEDB8_8320 ^ (value >> 1) } else { value >> 1 };
                bit += 1;
            }
            table[i] = value;
            i += 1;
        }
        table
    };
    let mut result = !start;
    for byte in bytes {
        result = (result >> 8) ^ TABLE[((result & 0xFF) as u8 ^ byte) as usize];
    }
    !result
}

/// `HashItemId(name, formID)`: the name as the game's single-byte string (a character past `0xFF` as its UTF-8).
fn hash_item_id(name: &str, form: u32) -> i32 {
    let bytes: Vec<u8> = if name.chars().all(|c| (c as u32) <= 0xFF) {
        name.chars().map(|c| c as u8).collect()
    } else {
        name.as_bytes().to_vec()
    };
    crc32_from(&bytes, crate::load_order::plugin_local_object_id(form)) as i32
}

impl Vm<'_> {
    fn actor_entity(&self, actor: u32) -> Option<byroredux_core::ecs::EntityId> {
        if actor == PLAYER_REF {
            crate::papyrus_demo::PapyrusPlayerEntity::get(self.world)
        } else {
            self.entity_of(actor)
        }
    }

    /// The item id of `form` as an inventory entry the actor has: its name, then the hash.
    fn item_id(&self, actor: byroredux_core::ecs::EntityId, form: u32) -> i32 {
        let has = self.world.get::<Inventory>(actor).is_some_and(|inventory| inventory.items.iter().any(|stack| stack.base_form_id == form));
        if !has {
            return 0;
        }
        match self.world.try_resource::<FormFacts>().and_then(|facts| facts.get(form).and_then(|fact| fact.name.clone())) {
            Some(name) => hash_item_id(&name, form),
            None => 0,
        }
    }

    /// The worn item `mask` selects: the lowest inventory entry among those whose slot bit is in the mask.
    fn worn_form(&self, actor: byroredux_core::ecs::EntityId, mask: u32) -> Option<u32> {
        let slots = self.world.get::<EquipmentSlots>(actor)?;
        let inventory = self.world.get::<Inventory>(actor)?;
        let first = (0u8..32).filter(|bit| mask & (1 << bit) != 0).filter_map(|bit| slots.at(bit)).min_by_key(|index| index.0)?;
        inventory.get(first).map(|stack| stack.base_form_id).filter(|form| *form != 0)
    }

    /// The `Actor` equipment reads of [`Vm::object_read`], on Skyrim.
    pub(in crate::fragment::vm) fn read_skse_actor(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        let Value::Form(actor) = *owner else {
            return Ok(None);
        };
        if !READS.contains(&name) {
            return Ok(None);
        }
        let skyrim = self.world.try_resource::<crate::ScriptGame>().is_some_and(|game| game.0 == byroredux_plugin::esm::reader::GameKind::Skyrim);
        if !skyrim {
            return stop(format!("Actor.{name} outside Skyrim"));
        }
        let values = self.eval_args(frame, args)?;
        let Some(entity) = self.actor_entity(actor) else {
            return stop(format!("Actor.{name} on an actor that is not loaded"));
        };
        let number = values.first().and_then(|value| value.int().or_else(|| value.number().map(|n| n as i32))).unwrap_or(0);
        // The held object of a hand, `GetEquippedObject`'s: slot 0 is the left hand, any other the right.
        let hand_form = |this: &Self, slot: i32| -> Run<Option<u32>> {
            if slot == 2 {
                return stop("Actor voice slot: the equipped shout is not tracked");
            }
            let hand = if slot == 0 { Hand::Left } else { Hand::Right };
            Ok(hand_item(this.world, entity, hand).form().filter(|form| *form != 0))
        };
        Ok(Some(match name {
            "isswimming" => match self.world.try_resource::<crate::SwimQuery>() {
                Some(swim) => Value::Bool((swim.0)(self.world, entity)),
                None => return stop("Actor.IsSwimming without the swim query"),
            },
            "getwornform" => self.worn_form(entity, number as u32).map_or(Value::None, Value::Form),
            "getwornitemid" => Value::Int(self.worn_form(entity, number as u32).map_or(0, |form| self.item_id(entity, form))),
            "getequippedobject" => hand_form(self, number)?.map_or(Value::None, Value::Form),
            _ => Value::Int(hand_form(self, number)?.map_or(0, |form| self.item_id(entity, form))),
        }))
    }
}

impl Checker<'_> {
    /// Whether the VM runs SKSE's `Actor` equipment read `name` on an actor.
    pub(in crate::fragment::vm) fn check_skse_actor(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        _statement: bool,
    ) -> Option<Kind> {
        let skyrim = self.game.is_none_or(|game| game == byroredux_plugin::esm::reader::GameKind::Skyrim);
        if !READS.contains(&name) || !skyrim || !matches!(receiver_kind, Kind::Object | Kind::Player) {
            return None;
        }
        let numeric = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num));
        match (name, arg_kinds) {
            ("isswimming", []) => Some(Kind::Bool),
            ("getwornform" | "getequippedobject", [mask]) if numeric(mask) => Some(Kind::Object),
            ("getwornitemid" | "getequippeditemid", [mask]) if numeric(mask) => Some(Kind::Num),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SKSE's `HashUtil::CRC32` is the usual CRC-32 whose register starts at `!start`: with `start` 0 it is the
    /// standard checksum ("123456789" is `0xCBF43926`).
    #[test]
    fn the_item_hash_is_crc32_continued_from_the_object_id() {
        assert_eq!(crc32_from(b"123456789", 0), 0xCBF4_3926);
        assert_eq!(crc32_from(b"", 0), 0);
        assert_ne!(crc32_from(b"Iron Sword", 0x12EB7), crc32_from(b"Iron Sword", 0x12EB8), "the form id is part of the hash");
        // A light plugin's id is its 12 low bits, a regular one's its 24.
        assert_eq!(hash_item_id("x", 0x0001_2EB7), crc32_from(b"x", 0x1_2EB7) as i32);
        assert_eq!(hash_item_id("x", 0xFE00_5ABC), crc32_from(b"x", 0xABC) as i32);
        assert_eq!(hash_item_id("é", 0x10), crc32_from(&[0xE9], 0x10) as i32, "a Latin-1 character is one byte");
        // Independent values: Python's `zlib.crc32(name, start)` is the same function.
        assert_eq!(hash_item_id("Iron Sword", 0x0000_5A0D), 85_618_840);
        assert_eq!(hash_item_id("Iron Armor", 0x0000_5A11), 491_399_181);
        assert_eq!(hash_item_id("x", 0xFE00_5ABC), 1_309_403_438);
    }
}
