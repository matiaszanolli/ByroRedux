//! The executable-defined Oblivion actor-value roster, installed after merging plugins.
//!
//! The order follows xEdit's TES4 `wbActorValueEnum`; spellings are the
//! Construction Set's script names. Indices 38 and 39 have records but are
//! deliberately excluded from vanilla actor-value commands.

use super::{AvifRecord, EsmIndex};
use crate::esm::reader::{GameKind, GlobalSlot};

pub const OBLIVION_ACTOR_VALUES: [&str; 72] = [
    "Strength",
    "Intelligence",
    "Willpower",
    "Agility",
    "Speed",
    "Endurance",
    "Personality",
    "Luck",
    "Health",
    "Magicka",
    "Fatigue",
    "Encumbrance",
    "Armorer",
    "Athletics",
    "Blade",
    "Block",
    "Blunt",
    "HandToHand",
    "HeavyArmor",
    "Alchemy",
    "Alteration",
    "Conjuration",
    "Destruction",
    "Illusion",
    "Mysticism",
    "Restoration",
    "Acrobatics",
    "LightArmor",
    "Marksman",
    "Mercantile",
    "Security",
    "Sneak",
    "Speechcraft",
    "Aggression",
    "Confidence",
    "Energy",
    "Responsibility",
    "Bounty",
    "Fame",
    "Infamy",
    "MagickaMultiplier",
    "NightEyeBonus",
    "AttackBonus",
    "DefendBonus",
    "CastingPenalty",
    "Blindness",
    "Chameleon",
    "Invisibility",
    "Paralysis",
    "Silence",
    "Confusion",
    "DetectItemRange",
    "SpellAbsorbChance",
    "SpellReflectChance",
    "SwimSpeedMultiplier",
    "WaterBreathing",
    "WaterWalking",
    "StuntedMagicka",
    "DetectLifeRange",
    "ReflectDamage",
    "Telekinesis",
    "ResistFire",
    "ResistFrost",
    "ResistDisease",
    "ResistMagic",
    "ResistNormalWeapons",
    "ResistParalysis",
    "ResistPoison",
    "ResistShock",
    "Vampirism",
    "Darkness",
    "ResistWaterDamage",
];

pub fn vanilla_name(index: u32) -> Option<&'static str> {
    if matches!(index, 38 | 39) {
        return None;
    }
    OBLIVION_ACTOR_VALUES.get(index as usize).copied()
}

impl EsmIndex {
    /// Install the roster into a free regular slot, atomically and idempotently.
    pub fn install_oblivion_actor_values(&mut self, slot: GlobalSlot) -> Result<(), String> {
        if self.game != GameKind::Oblivion {
            return Err("engine actor values require an Oblivion index".into());
        }
        if !matches!(slot, GlobalSlot::Regular(0..=0xfb)) {
            return Err(format!("invalid engine actor-value slot {slot:?}"));
        }
        if let Some(previous) = self.oblivion_actor_value_slot {
            if previous != slot {
                return Err(format!(
                    "engine actor-value identity already installed at {previous:?}"
                ));
            }
            if OBLIVION_ACTOR_VALUES
                .iter()
                .enumerate()
                .all(|(index, name)| {
                    let id = slot.compose(index as u32 + 1);
                    self.actor_values
                        .get(&id)
                        .is_some_and(|row| row.form_id == id && row.editor_id == *name)
                        && self.record_types.get(&id) == Some(b"AVIF")
                })
            {
                return Ok(());
            }
            return Err(format!(
                "engine actor-value slot {slot:?} contains conflicting records"
            ));
        }
        if self
            .record_types
            .keys()
            .chain(self.actor_values.keys())
            .any(|id| id >> 24 == slot.compose(0) >> 24)
        {
            return Err(format!(
                "engine actor-value slot {slot:?} is already occupied"
            ));
        }
        for (index, name) in OBLIVION_ACTOR_VALUES.iter().enumerate() {
            let form_id = slot.compose(index as u32 + 1);
            self.actor_values.insert(
                form_id,
                AvifRecord {
                    form_id,
                    editor_id: (*name).into(),
                    ..Default::default()
                },
            );
            self.record_types.insert(form_id, *b"AVIF");
        }
        self.oblivion_actor_value_slot = Some(slot);
        Ok(())
    }

    /// Resolve an index, optionally including the two non-vanilla values.
    pub fn oblivion_actor_value(&self, index: u32, include_reserved: bool) -> Option<u32> {
        OBLIVION_ACTOR_VALUES.get(index as usize)?;
        if !include_reserved {
            vanilla_name(index)?;
        }
        let id = self.oblivion_actor_value_slot?.compose(index + 1);
        self.actor_values.contains_key(&id).then_some(id)
    }

    /// Explicit non-vanilla lookup, including Fame and Infamy.
    pub fn oblivion_actor_value_by_name(&self, name: &str, include_reserved: bool) -> Option<u32> {
        let index = OBLIVION_ACTOR_VALUES
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(name))?;
        self.oblivion_actor_value(index as u32, include_reserved)
    }
}

#[cfg(test)]
mod tests {
    use super::super::ActorValueRef;
    use super::*;

    #[test]
    fn install_and_resolve_the_roster_without_reserved_vanilla_values() {
        let mut index = EsmIndex {
            game: GameKind::Oblivion,
            ..Default::default()
        };
        let slot = GlobalSlot::Regular(3);
        index.install_oblivion_actor_values(slot).unwrap();
        index.install_oblivion_actor_values(slot).unwrap();
        assert_eq!(index.actor_values.len(), 72);
        for (i, name) in OBLIVION_ACTOR_VALUES.iter().enumerate() {
            let expected = slot.compose(i as u32 + 1);
            assert_eq!(
                index.oblivion_actor_value_by_name(name, true),
                Some(expected)
            );
            let vanilla = (!matches!(i, 38 | 39)).then_some(expected);
            assert_eq!(
                index.resolve_actor_value(ActorValueRef::Index(i as u32)),
                vanilla
            );
            assert_eq!(index.oblivion_actor_value_by_name(name, false), vanilla);
            assert_eq!(index.actor_value_form_id(name), vanilla);
        }
        assert_eq!(index.resolve_actor_value(ActorValueRef::Index(72)), None);
        assert!(index
            .install_oblivion_actor_values(GlobalSlot::Regular(4))
            .is_err());
    }

    #[test]
    fn occupied_slot_is_rejected_without_partial_installation() {
        let mut index = EsmIndex {
            game: GameKind::Oblivion,
            ..Default::default()
        };
        index.record_types.insert(0x0300_1234, *b"NPC_");
        assert!(index
            .install_oblivion_actor_values(GlobalSlot::Regular(3))
            .is_err());
        assert!(index.actor_values.is_empty());
        assert!(index.oblivion_actor_value_slot.is_none());
    }

    #[test]
    fn no_other_rust_file_restates_the_oblivion_skill_roster() {
        fn scan(dir: &std::path::Path, source: &std::path::Path) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    scan(&path, source);
                } else if path.extension().is_some_and(|ext| ext == "rs") && path != source {
                    let text = std::fs::read_to_string(&path).unwrap();
                    // A dozen consecutive names from the distinctive 21-skill
                    // roster catches a second table without banning individual
                    // names used legitimately by consumers and tests.
                    for window in OBLIVION_ACTOR_VALUES[12..33].windows(12) {
                        let mut rest = text.as_str();
                        let mut found = true;
                        for name in window {
                            let needle = format!("\"{name}\"");
                            if let Some(pos) = rest.find(&needle) {
                                rest = &rest[pos + needle.len()..];
                            } else {
                                found = false;
                                break;
                            }
                        }
                        assert!(
                            !found,
                            "duplicate Oblivion actor-value roster in {}",
                            path.display()
                        );
                    }
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = root.join("crates/plugin/src/esm/records/oblivion_actor_values.rs");
        for dir in ["crates", "byroredux", "tools"] {
            scan(&root.join(dir), &source);
        }
    }
}
