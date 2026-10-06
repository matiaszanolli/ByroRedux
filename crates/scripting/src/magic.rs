//! Magic runtime (#4415): actor spell lists and the constant (ability)
//! value modifiers they carry.
//!
//! The per-game records (`SPEL` + `SPIT`, `MGEF` archetype + actor value,
//! `SPLO`, `LVSP`) are translated once, at ESM load, into a canonical
//! [`SpellCatalog`]: for every spell, whether its effects persist while the
//! spell is on the actor and, if so, the actor-value changes they make —
//! already resolved to the AVIF FormIDs that key `ActorValues`. Nothing past
//! that boundary branches on the game.
//!
//! What is applied: an actor's constant-effect spells (abilities, diseases,
//! addictions) change the **permanent** modifier of the actor values their
//! Recover-flagged Value Modifier / Peak Value Modifier / Dual Value
//! Modifier effects name — the Creation Kit wiki's "Actor Value" page: the
//! permanent modifier is "adjusted when Abilities or Enchantments change the
//! value", and a Recover effect "will modify their actor value once at the
//! start, then modify it back once the Effect expires". Effects without
//! Recover change the value every second instead; there is no per-second
//! magic tick yet, so they are left out rather than applied once. Cast
//! spells (fire-and-forget, concentration) have no caster runtime and only
//! sit on the list.

use byroredux_core::ecs::components::ActorValues;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::{
    EsmIndex, MagicArchetype, SpellType, MGEF_FLAG_DETRIMENTAL, MGEF_FLAG_RECOVER,
};
use std::collections::HashMap;

/// One actor-value change a constant-effect spell makes while it is on the
/// actor: `amount` added to the value's permanent modifier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstantModifier {
    /// The AVIF FormID keying `ActorValues`.
    pub actor_value: u32,
    pub amount: f32,
}

/// A spell as the runtime needs it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanonicalSpell {
    /// The spell's effects persist while it is on the actor (Ability,
    /// Disease, Addiction — the GECK's "Actor Effect" page: their effect
    /// duration "is ignored … lasts until removed").
    pub constant: bool,
    /// For a constant spell, the value changes applied while it is on the
    /// actor; empty otherwise.
    pub constant_modifiers: Vec<ConstantModifier>,
}

/// Every spell of the loaded plugin set, canonical. Installed at ESM load;
/// immutable afterwards.
#[derive(Debug, Clone, Default)]
pub struct SpellCatalog {
    spells: HashMap<u32, CanonicalSpell>,
}

impl Resource for SpellCatalog {}

impl SpellCatalog {
    /// Translate every `SPEL` in `index`.
    pub fn from_index(index: &EsmIndex) -> Self {
        Self {
            spells: index
                .spells
                .keys()
                .map(|&form_id| (form_id, canonical_spell(index, form_id)))
                .collect(),
        }
    }

    pub fn get(&self, form_id: u32) -> Option<&CanonicalSpell> {
        self.spells.get(&form_id)
    }
}

/// Translate one spell (see the module doc for what is kept).
pub fn canonical_spell(index: &EsmIndex, form_id: u32) -> CanonicalSpell {
    let Some(spell) = index.spells.get(&form_id) else {
        return CanonicalSpell::default();
    };
    let constant = matches!(
        spell.spell_type,
        SpellType::Ability | SpellType::Disease | SpellType::Addiction
    );
    let mut constant_modifiers = Vec::new();
    if constant {
        for effect in &spell.effects {
            let Some(mgef) = index.resolve_magic_effect(effect.effect_form_id) else {
                continue;
            };
            if mgef.effect_flags & MGEF_FLAG_RECOVER == 0 || !effect.magnitude.is_finite() {
                continue;
            }
            let sign = if mgef.effect_flags & MGEF_FLAG_DETRIMENTAL != 0 {
                -1.0
            } else {
                1.0
            };
            let amount = sign * effect.magnitude;
            let resolve = |reference| index.resolve_actor_value(reference);
            match mgef.archetype {
                Some(MagicArchetype::ValueModifier | MagicArchetype::PeakValueModifier) => {
                    if let Some(actor_value) = mgef.primary_actor_value.and_then(resolve) {
                        constant_modifiers.push(ConstantModifier {
                            actor_value,
                            amount,
                        });
                    }
                }
                Some(MagicArchetype::DualValueModifier) => {
                    if let Some(actor_value) = mgef.primary_actor_value.and_then(resolve) {
                        constant_modifiers.push(ConstantModifier {
                            actor_value,
                            amount,
                        });
                    }
                    if let Some(actor_value) = mgef.secondary_actor_value.and_then(resolve) {
                        constant_modifiers.push(ConstantModifier {
                            actor_value,
                            amount: amount * mgef.second_actor_value_weight,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    CanonicalSpell {
        constant,
        constant_modifiers,
    }
}

/// The spells an actor carries, in the order they arrived: its record's and
/// its race's `SPLO` lists (leveled lists resolved at spawn), then any a
/// script added. Saved — a script's `AddSpell` must survive a reload.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct SpellList(pub Vec<u32>);

impl Component for SpellList {
    type Storage = SparseSetStorage<Self>;
}

/// #4415 — the actor's RACE-authored `SPLO` set, level-resolved at spawn
/// (the exact spells `AddRaceSpells` re-applies and `RemoveRaceSpells`
/// clears). Spawn-derived, re-derived identically on every reload, so it
/// is deliberately not part of a save.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RaceSpells(pub Vec<u32>);

impl Component for RaceSpells {
    type Storage = SparseSetStorage<Self>;
}

/// Put `spell` on `actor` (Papyrus `Actor.AddSpell`) and, if it is a
/// constant spell, apply its value changes. `false` — and no change — when
/// the actor already carries it or has no `SpellList`. The second is a
/// dropped effect, not Papyrus' own no-op, so it is logged (#4822).
pub fn add_spell(world: &World, actor: EntityId, spell: u32) -> bool {
    let mut has_list = false;
    let added = world.query_mut::<SpellList>().is_some_and(|mut lists| {
        lists.get_mut(actor).is_some_and(|list| {
            has_list = true;
            let fresh = !list.0.contains(&spell);
            if fresh {
                list.0.push(spell);
            }
            fresh
        })
    });
    if !has_list {
        log::warn!("AddSpell({spell:08X}) on entity {actor} dropped — the actor has no SpellList");
    }
    if added {
        apply_constant_modifiers(world, actor, spell, 1.0);
    }
    added
}

/// Take `spell` off `actor` (Papyrus `Actor.RemoveSpell`), undoing its value
/// changes. `false` when the actor did not carry it, logged like
/// [`add_spell`] when it has no `SpellList` at all (#4822).
pub fn remove_spell(world: &World, actor: EntityId, spell: u32) -> bool {
    let mut has_list = false;
    let removed = world.query_mut::<SpellList>().is_some_and(|mut lists| {
        lists.get_mut(actor).is_some_and(|list| {
            has_list = true;
            let before = list.0.len();
            list.0.retain(|&s| s != spell);
            list.0.len() != before
        })
    });
    if !has_list {
        log::warn!(
            "RemoveSpell({spell:08X}) on entity {actor} dropped — the actor has no SpellList"
        );
    }
    if removed {
        apply_constant_modifiers(world, actor, spell, -1.0);
    }
    removed
}

/// #4415 — re-apply the actor's racial `SPLO` set (`AddRaceSpells`): the
/// spawn-stamped [`RaceSpells`] component minus what the live `SpellList`
/// already carries. `false` when nothing was missing (Papyrus' idempotent
/// no-op); logged like [`add_spell`] when the actor has no `SpellList`.
pub fn add_race_spells(world: &World, actor: EntityId) -> bool {
    let racial = world
        .get::<RaceSpells>(actor)
        .map(|spells| spells.0.clone())
        .unwrap_or_default();
    let mut any = false;
    for &spell in &racial {
        any |= add_spell(world, actor, spell);
    }
    any
}

/// #4415 — clear exactly the racial set (`RemoveRaceSpells`): every racial
/// spell still on the live list goes, each undoing its constant modifiers;
/// a scripted `AddSpell` survives. `false` when none were present.
pub fn remove_race_spells(world: &World, actor: EntityId) -> bool {
    let racial = world
        .get::<RaceSpells>(actor)
        .map(|spells| spells.0.clone())
        .unwrap_or_default();
    let mut any = false;
    for &spell in &racial {
        any |= remove_spell(world, actor, spell);
    }
    any
}

/// Apply (`direction` 1) or undo (-1) one spell's constant modifiers on the
/// actor's permanent modifiers. Spawn applies an actor's authored spells
/// through this same function, so authored and scripted spells can never
/// disagree about what a spell does.
pub fn apply_constant_modifiers(world: &World, actor: EntityId, spell: u32, direction: f32) {
    let modifiers = world
        .try_resource::<SpellCatalog>()
        .and_then(|catalog| catalog.get(spell).map(|s| s.constant_modifiers.clone()))
        .unwrap_or_default();
    if modifiers.is_empty() {
        return;
    }
    let Some(mut values) = world.query_mut::<ActorValues>() else {
        return;
    };
    let Some(values) = values.get_mut(actor) else {
        return;
    };
    apply_modifiers(values, &modifiers, direction);
}

/// Add (`direction` 1) or remove (-1) constant modifiers on one actor's
/// values — the single application rule spawn and `AddSpell` share.
pub fn apply_modifiers(values: &mut ActorValues, modifiers: &[ConstantModifier], direction: f32) {
    for modifier in modifiers {
        values.mod_permanent(modifier.actor_value, modifier.amount * direction);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::{
        ActorValueRef, AvifRecord, MagicEffectItem, MgefRecord, SpelRecord,
    };

    const HEALTH: u32 = 0x100;
    const STAMINA: u32 = 0x101;

    fn mgef(form_id: u32, flags: u32, archetype: MagicArchetype) -> MgefRecord {
        MgefRecord {
            form_id,
            effect_flags: flags,
            archetype: Some(archetype),
            primary_actor_value: Some(ActorValueRef::Form(HEALTH)),
            secondary_actor_value: Some(ActorValueRef::Form(STAMINA)),
            second_actor_value_weight: 0.5,
            ..Default::default()
        }
    }

    fn spell(form_id: u32, spell_type: SpellType, effects: &[(u32, f32)]) -> SpelRecord {
        SpelRecord {
            form_id,
            spell_type,
            effects: effects
                .iter()
                .map(|&(effect_form_id, magnitude)| MagicEffectItem {
                    effect_form_id,
                    magnitude,
                    area: 0,
                    duration: 0,
                })
                .collect(),
            ..Default::default()
        }
    }

    /// An FO4-shaped index (effects name AVIFs directly): a Recover
    /// fortify, a Recover detrimental drain, a non-Recover regen, and a
    /// Dual Value Modifier.
    fn index() -> EsmIndex {
        let mut index = EsmIndex {
            game: GameKind::Fallout4,
            ..EsmIndex::default()
        };
        for form_id in [HEALTH, STAMINA] {
            index.actor_values.insert(
                form_id,
                AvifRecord {
                    form_id,
                    ..Default::default()
                },
            );
        }
        for m in [
            mgef(0x10, MGEF_FLAG_RECOVER, MagicArchetype::ValueModifier),
            mgef(
                0x11,
                MGEF_FLAG_RECOVER | MGEF_FLAG_DETRIMENTAL,
                MagicArchetype::ValueModifier,
            ),
            mgef(0x12, 0, MagicArchetype::ValueModifier),
            mgef(0x13, MGEF_FLAG_RECOVER, MagicArchetype::DualValueModifier),
        ] {
            index.magic_effects.insert(m.form_id, m);
        }
        for s in [
            spell(
                0x20,
                SpellType::Ability,
                &[(0x10, 25.0), (0x11, 5.0), (0x12, 3.0)],
            ),
            spell(0x21, SpellType::Spell, &[(0x10, 50.0)]),
            spell(0x22, SpellType::Disease, &[(0x13, 10.0)]),
        ] {
            index.spells.insert(s.form_id, s);
        }
        index
    }

    /// #4415 — only constant spells carry modifiers; Recover value
    /// modifiers apply (Detrimental negated), per-second ones do not, and a
    /// Dual Value Modifier scales its second value by the weight.
    #[test]
    fn catalog_translates_constant_value_modifiers() {
        let catalog = SpellCatalog::from_index(&index());
        let ability = catalog.get(0x20).unwrap();
        assert!(ability.constant);
        assert_eq!(
            ability.constant_modifiers,
            vec![
                ConstantModifier {
                    actor_value: HEALTH,
                    amount: 25.0
                },
                ConstantModifier {
                    actor_value: HEALTH,
                    amount: -5.0
                },
            ],
            "the non-Recover regen needs a per-second tick and is left out"
        );
        let cast = catalog.get(0x21).unwrap();
        assert!(!cast.constant && cast.constant_modifiers.is_empty());
        assert_eq!(
            catalog.get(0x22).unwrap().constant_modifiers,
            vec![
                ConstantModifier {
                    actor_value: HEALTH,
                    amount: 10.0
                },
                ConstantModifier {
                    actor_value: STAMINA,
                    amount: 5.0
                },
            ]
        );
    }

    /// #5084 — on Oblivion the SPEL `EFID` is the 4-char effect code,
    /// not a FormID. Pre-fix `canonical_spell` looked the code up in the
    /// FormID-keyed `magic_effects` map, every Oblivion ability silently
    /// translated to no modifiers (#969 predicted this), and the same
    /// miss empties the consumables restoration planners.
    #[test]
    fn oblivion_ability_translates_through_the_effect_code_side_index() {
        let mut index = EsmIndex {
            game: GameKind::Oblivion,
            ..EsmIndex::default()
        };
        index.actor_values.insert(
            HEALTH,
            AvifRecord {
                form_id: HEALTH,
                ..Default::default()
            },
        );
        index.magic_effects.insert(
            0x10,
            mgef(0x10, MGEF_FLAG_RECOVER, MagicArchetype::ValueModifier),
        );
        index.magic_effects_by_code.insert(*b"FOAT", 0x10);
        index.spells.insert(
            0x20,
            spell(
                0x20,
                SpellType::Ability,
                &[(u32::from_le_bytes(*b"FOAT"), 25.0)],
            ),
        );

        let catalog = SpellCatalog::from_index(&index);
        let ability = catalog.get(0x20).unwrap();
        assert!(ability.constant);
        assert_eq!(
            ability.constant_modifiers,
            vec![ConstantModifier {
                actor_value: HEALTH,
                amount: 25.0
            }],
            "an Oblivion EFID code must resolve through magic_effects_by_code (#5084), \
             not silently no-op in the FormID-keyed map"
        );
    }

    /// #4415 — AddSpell applies a constant spell's changes to the permanent
    /// modifier exactly once; RemoveSpell undoes them; a cast spell only
    /// joins the list.
    #[test]
    fn add_and_remove_spell_apply_and_undo_permanent_modifiers() {
        let mut world = World::new();
        world.register::<SpellList>();
        world.insert_resource(SpellCatalog::from_index(&index()));
        let actor = world.spawn();
        let mut values = ActorValues::new();
        values.set_base(HEALTH, 100.0);
        world.insert(actor, values);
        world.insert(actor, SpellList::default());
        let health = |world: &World| world.get::<ActorValues>(actor).unwrap().current(HEALTH);

        assert!(add_spell(&world, actor, 0x20));
        assert_eq!(health(&world), 120.0);
        assert!(
            !add_spell(&world, actor, 0x20),
            "already carried: no double apply"
        );
        assert_eq!(health(&world), 120.0);
        assert!(add_spell(&world, actor, 0x21));
        assert_eq!(health(&world), 120.0, "a cast spell changes nothing");
        assert_eq!(world.get::<SpellList>(actor).unwrap().0, vec![0x20, 0x21]);

        assert!(remove_spell(&world, actor, 0x20));
        assert_eq!(health(&world), 100.0);
        assert!(!remove_spell(&world, actor, 0x20));
        assert_eq!(world.get::<SpellList>(actor).unwrap().0, vec![0x21]);
    }

    /// #4415 — `AddRaceSpells` / `RemoveRaceSpells` operate on exactly the
    /// spawn-stamped racial set: re-apply adds only what is missing
    /// (constant modifiers included), clear removes only the racial
    /// entries and leaves a scripted `AddSpell` alone.
    #[test]
    fn race_spell_pair_reapplies_and_clears_only_the_racial_set() {
        let mut world = World::new();
        world.register::<SpellList>();
        world.register::<RaceSpells>();
        world.insert_resource(SpellCatalog::from_index(&index()));
        let actor = world.spawn();
        // Racial set: 0x20 (constant +25 health) and 0x30. Live list starts
        // with 0x20 already present plus a scripted 0x21.
        world.insert(actor, RaceSpells(vec![0x20, 0x30]));
        world.insert(actor, SpellList(vec![0x20, 0x21]));

        assert!(add_race_spells(&world, actor), "0x30 was missing");
        assert_eq!(
            world.get::<SpellList>(actor).unwrap().0,
            vec![0x20, 0x21, 0x30]
        );
        assert!(!add_race_spells(&world, actor), "idempotent the second time");

        assert!(remove_race_spells(&world, actor));
        assert_eq!(
            world.get::<SpellList>(actor).unwrap().0,
            vec![0x21],
            "the scripted AddSpell survives the racial clear"
        );
        assert!(!remove_race_spells(&world, actor));
    }
}
