//! What SKSE's `Form` reads answer from (LMC-SCR-004 catalog growth, `docs/engine/extender-catalog.md` §9):
//! each Skyrim base form's type and the component values the engine reads from its record.
//!
//! SKSE64's `PapyrusForm.cpp` casts the form to a component: `GetName` to `TESFullName`, `GetWeight` to
//! `TESWeightForm`, the keyword reads to `BGSKeywordForm`, the world-model reads to `TESModel`. A class without the
//! component answers the empty value. Which class carries which component, and the `kFormType` numbers `GetType`
//! answers, are `docs/data/extenders/skse64-form-types.tsv`, generated from SKSE64's headers.
//!
//! A [`FormFact`]'s `None` field is one the engine does not read for that record type. The natives stop on it
//! with that reason rather than answer the empty value a class without the component would give.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::EsmIndex;

/// One `kFormType`: its number, the record signature its header comment names, and which components its class
/// carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormType {
    pub number: i32,
    pub signature: String,
    pub full_name: bool,
    pub weight: bool,
    pub keywords: bool,
    pub model: bool,
}

/// SKSE64's form types, in `kFormType` order.
pub fn form_types() -> &'static [FormType] {
    static TYPES: OnceLock<Vec<FormType>> = OnceLock::new();
    TYPES.get_or_init(|| {
        include_str!("../../../docs/data/extenders/skse64-form-types.tsv")
            .lines()
            .skip(1)
            .filter_map(|line| {
                let cols: Vec<&str> = line.split('\t').collect();
                let flag = |i: usize| cols.get(i) == Some(&"1");
                Some(FormType {
                    number: cols.first()?.parse().ok()?,
                    signature: (*cols.get(1)?).to_owned(),
                    full_name: flag(3),
                    weight: flag(4),
                    keywords: flag(5),
                    model: flag(6),
                })
            })
            .collect()
    })
}

/// The form type of a record signature. SKSE's header comment spells `MESG` as `MESH`.
pub fn form_type(signature: &[u8; 4]) -> Option<&'static FormType> {
    let signature = if signature == b"MESG" { b"MESH" } else { signature };
    let signature = std::str::from_utf8(signature).ok()?;
    form_types().iter().find(|row| row.signature == signature)
}

/// One base form's component values the engine reads (`None`: not read).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormFact {
    pub name: Option<String>,
    pub weight: Option<f32>,
    pub keywords: Option<Vec<u32>>,
    pub model: Option<String>,
    /// A head part's `PNAM` type (`HeadPart.GetType`).
    pub head_part_type: Option<u32>,
}

/// Placed records (references, placed projectiles and hazards) and the world's geometry: most of a load order's
/// records, and no script holds one as a base form. A reference's type comes from its entity.
const PLACED: [&[u8; 4]; 12] =
    [b"REFR", b"ACHR", b"PGRE", b"PHZD", b"PMIS", b"PARW", b"PBEA", b"PFLA", b"PCON", b"PBAR", b"NAVM", b"LAND"];

/// Every Skyrim base form's signature, and the component values the engine reads, by FormID.
/// `NOT_SAVED_BY_DESIGN`: rebuilt from the records every boot.
#[derive(Debug, Default, Clone)]
pub struct FormFacts {
    signatures: Arc<HashMap<u32, [u8; 4]>>,
    facts: Arc<HashMap<u32, FormFact>>,
}

impl Resource for FormFacts {}

impl FormFacts {
    /// A base form's record signature.
    pub fn signature(&self, form: u32) -> Option<[u8; 4]> {
        self.signatures.get(&form).copied()
    }

    /// The values the engine reads of a base form; `None` when it reads none.
    pub fn get(&self, form: u32) -> Option<&FormFact> {
        self.facts.get(&form)
    }

    pub fn len(&self) -> usize {
        self.signatures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.signatures.is_empty()
    }

    /// The facts of a Skyrim load order; empty for any other game, whose extender's form types are not these.
    pub fn from_index(index: &EsmIndex) -> Self {
        if index.game != GameKind::Skyrim {
            return Self::default();
        }
        let signatures: HashMap<u32, [u8; 4]> =
            index.record_types.iter().filter(|(_, signature)| !PLACED.contains(signature)).map(|(form, s)| (*form, *s)).collect();
        let mut facts: HashMap<u32, FormFact> = HashMap::new();
        let mut put = |form: u32, fact: FormFact| {
            let entry = facts.entry(form).or_default();
            entry.name = fact.name.or(entry.name.take());
            entry.weight = fact.weight.or(entry.weight);
            entry.keywords = fact.keywords.or(entry.keywords.take());
            entry.model = fact.model.or(entry.model.take());
            entry.head_part_type = fact.head_part_type.or(entry.head_part_type);
        };
        let named = |name: &str| FormFact { name: Some(name.to_owned()), ..Default::default() };
        // The placeable bases' models; the typed records below add what they read.
        for object in index.cells.statics.values() {
            put(object.form_id, FormFact { model: Some(object.model_path.clone()), ..Default::default() });
        }
        for item in index.items.values() {
            let common = &item.common;
            put(
                item.form_id,
                FormFact {
                    name: Some(common.full_name.clone()),
                    weight: Some(common.weight),
                    keywords: Some(common.keywords.clone()),
                    model: Some(common.model_path.clone()),
                    head_part_type: None,
                },
            );
        }
        for gem in index.soul_gems.values() {
            put(gem.form_id, FormFact { weight: Some(gem.weight), model: Some(gem.model_path.clone()), ..named(&gem.full_name) });
        }
        for npc in index.npcs.values() {
            // The keywords the actor runs with: a templated shell's come from its Use Keywords terminal (#6515), as
            // `KeywordTable`'s do. A race's keywords are the race's, not the NPC's form.
            let keywords = byroredux_plugin::equip::keywords_terminal(npc, index).keywords.clone();
            put(npc.form_id, FormFact { keywords: Some(keywords), ..named(&npc.full_name) });
        }
        for race in index.races.values() {
            put(race.form_id, FormFact { keywords: Some(race.keywords.clone()), ..named(&race.full_name) });
        }
        for location in index.locations.values() {
            put(location.form_id, FormFact { keywords: Some(location.keywords.clone()), ..named(&location.full_name) });
        }
        for container in index.containers.values() {
            put(container.form_id, FormFact { weight: Some(container.weight), model: Some(container.model_path.clone()), ..named(&container.full_name) });
        }
        for activator in index.activators.values() {
            put(activator.form_id, FormFact { model: Some(activator.model_path.clone()), ..named(&activator.full_name) });
        }
        for part in index.head_parts.values() {
            put(part.form_id, FormFact { model: Some(part.model_path.clone()), head_part_type: part.part_type, ..named(&part.full_name) });
        }
        for tree in index.trees.values() {
            put(tree.form_id, FormFact { model: Some(tree.model_path.clone()), ..named(&tree.full_name) });
        }
        for projectile in index.projectiles.values() {
            put(projectile.form_id, FormFact { model: Some(projectile.model_path.clone()), ..named(&projectile.full_name) });
        }
        for grass in index.grasses.values() {
            put(grass.form_id, FormFact { model: Some(grass.model_path.clone()), ..Default::default() });
        }
        let names = [
            index.classes.iter().map(|(id, r)| (*id, r.full_name.as_str())).collect::<Vec<_>>(),
            index.factions.iter().map(|(id, r)| (*id, r.full_name.as_str())).collect(),
            index.spells.iter().map(|(id, r)| (*id, r.full_name.as_str())).collect(),
            index.magic_effects.iter().map(|(id, r)| (*id, r.full_name.as_str())).collect(),
            index.enchantments.iter().map(|(id, r)| (*id, r.full_name.as_str())).collect(),
            index.quests.iter().map(|(id, r)| (*id, r.full_name.as_str())).collect(),
            index.perks.iter().map(|(id, r)| (*id, r.full_name.as_str())).collect(),
            index.messages.iter().map(|(id, r)| (*id, r.full_name.as_str())).collect(),
            index.actor_values.iter().map(|(id, r)| (*id, r.full_name.as_str())).collect(),
            index.eyes.iter().map(|(id, r)| (*id, r.full_name.as_str())).collect(),
            index.explosions.iter().map(|(id, r)| (*id, r.full_name.as_str())).collect(),
            index.waters.iter().map(|(id, r)| (*id, r.full_name.as_str())).collect(),
        ];
        for (form, name) in names.into_iter().flatten() {
            put(form, named(name));
        }
        Self { signatures: Arc::new(signatures), facts: Arc::new(facts) }
    }
}

/// Install the load order's [`FormFacts`]; returns how many forms it holds.
pub fn install_form_facts(world: &mut World, index: &EsmIndex) -> usize {
    let facts = FormFacts::from_index(index);
    let count = facts.len();
    world.insert_resource(facts);
    count
}
