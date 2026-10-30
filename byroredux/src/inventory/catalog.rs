//! Item-record metadata, the player character / inventory templates, vitals, and the catalog install that seeds the player.

use super::*;

/// Immutable display/equipment metadata derived from the active plugin set.
#[derive(Debug, Clone)]
pub(crate) struct InventoryItemDefinition {
    pub(crate) name: String,
    pub(crate) category: &'static str,
    pub(crate) value: u32,
    pub(crate) weight: f32,
    pub(crate) details: String,
    pub(crate) equip_target: Option<EquipTarget>,
    pub(crate) weapon_damage: Option<f32>,
    /// Authored reach/speed, `0.0` when the source game's weapon layout
    /// isn't decoded (see `ItemKind::Weapon::reach`/`speed`). `combat.rs`
    /// treats `0.0` as "fall back to the unarmed constant."
    pub(crate) weapon_reach: f32,
    pub(crate) weapon_speed: f32,
}

/// Process-local item lookup rebuilt whenever the plugin index is installed.
#[derive(Debug, Clone, Default)]
pub(crate) struct InventoryCatalog {
    pub(super) restorations: FxHashMap<u32, Vec<byroredux_plugin::consumables::ConditionalRestoration>>,
    /// M47.3 — the magic effects an ingestible starts on its user through the caster runtime: its
    /// scripted effects (a chem's `ScriptEffectStart` / `Update` / `Finish`), and — for an item the
    /// restoration plan does not cover — its value effects too.
    pub(super) script_effects: FxHashMap<u32, Vec<byroredux_scripting::magic_effects::CastEffect>>,
    pub(super) entries: FxHashMap<u32, InventoryItemDefinition>,
    pub(super) containers: rustc_hash::FxHashSet<u32>,
    /// #4699 — the Player `NPC_` base (`player_npc_form_id`): the identity
    /// vanilla authors in `XOWN` for player-owned property.
    pub(super) player_base_form_id: u32,
}

impl InventoryCatalog {
    /// A catalog that knows these bases as plain pickup items - the fixture
    /// other systems' tests need to make a placement a pickup target.
    #[cfg(test)]
    pub(crate) fn with_pickup_bases(bases: &[u32]) -> Self {
        let entries = bases
            .iter()
            .map(|&base| {
                (
                    base,
                    InventoryItemDefinition {
                        name: format!("Item {base:X}"),
                        category: "Misc",
                        value: 1,
                        weight: 0.1,
                        details: String::new(),
                        equip_target: None,
                        weapon_damage: None,
                        weapon_reach: 0.0,
                        weapon_speed: 0.0,
                    },
                )
            })
            .collect();
        Self { entries, player_base_form_id: 0x14, ..Self::default() }
    }

    /// One item's catalogued display name, if it has one.
    pub(crate) fn item_name(&self, base_form_id: u32) -> Option<&str> {
        self.entries.get(&base_form_id).map(|definition| definition.name.as_str())
    }

    /// One `base_form_id` item's weight; 0 for an item outside the catalog.
    pub(crate) fn item_weight(&self, base_form_id: u32) -> f32 {
        self.entries
            .get(&base_form_id)
            .map_or(0.0, |definition| definition.weight)
    }

    /// One `base_form_id` item's gold value; 0 for an item outside the catalog.
    pub(crate) fn item_value(&self, base_form_id: u32) -> u32 {
        self.entries.get(&base_form_id).map_or(0, |definition| definition.value)
    }

    /// The total weight `inventory` carries (M47.2 encumbrance, the
    /// inventory menu's total).
    pub(crate) fn inventory_weight(&self, inventory: &Inventory) -> f32 {
        inventory
            .items
            .iter()
            .map(|stack| self.item_weight(stack.base_form_id) * stack.count as f32)
            .sum()
    }

    pub(crate) fn sdk_metadata(&self, form_id: u32) -> Option<ItemMetadata> {
        let definition = self.entries.get(&form_id)?;
        let category = match definition.category {
            "Misc" => ItemCategory::Misc,
            "Junk" => ItemCategory::Junk,
            "Mods" => ItemCategory::Mod,
            "Book" => ItemCategory::Book,
            "Scroll" => ItemCategory::Scroll,
            "Light" => ItemCategory::Light,
            "Apparatus" => ItemCategory::Apparatus,
            "Note" => ItemCategory::Note,
            "Ingredient" => ItemCategory::Ingredient,
            "Aid" => ItemCategory::Aid,
            "Key" => ItemCategory::Key,
            "Ammo" => ItemCategory::Ammo,
            "Armor" => ItemCategory::Armor,
            "Weapon" => ItemCategory::Weapon,
            _ => return None,
        };
        ItemMetadata::new(
            definition.name.clone(),
            category,
            definition.value,
            definition.weight,
        )
        .ok()
    }

    /// A base whose placements are picked up directly: a catalogued item
    /// that is not a container. The per-placement half (no own inventory,
    /// not dead, not already taken) is [`is_pickup_target`].
    pub(crate) fn is_pickup_base(&self, base_form_id: u32) -> bool {
        !self.containers.contains(&base_form_id) && self.entries.contains_key(&base_form_id)
    }
}

impl Resource for InventoryCatalog {}

/// Starting state copied onto the player body once it exists. It is metadata,
/// not live gameplay state: subsequent cell loads may replace this resource
/// but never overwrite an already-spawned player's inventory.
#[derive(Debug, Clone, Default)]
pub(crate) struct PlayerInventoryTemplate {
    pub(super) inventory: Inventory,
    pub(super) equipment: EquipmentSlots,
    pub(super) equipped_weapon: Option<EquippedWeapon>,
}

impl Resource for PlayerInventoryTemplate {}

/// The player's spawn-time CHARAL seed, stamped onto the body once it
/// exists (#4458). Built from the base Player `NPC_` record through
/// `derive_npc_actor_values` — the same derivation every NPC's
/// `stamp_actor_values` performs — so the consumable and drowning paths,
/// which gate on the player carrying `ActorValues` + `ActorVitals`, have a
/// populated (never empty/zero-SPECIAL) set to read. `None` fields mean
/// the derivation yielded nothing (no records / unsupported profile): the
/// body then carries no actor values, the pre-#4458 state, rather than a
/// wrong-by-construction empty one.
#[derive(Debug, Clone, Default)]
pub(crate) struct PlayerCharacterTemplate {
    pub(super) values: Option<byroredux_core::ecs::components::ActorValues>,
    pub(super) vitals: Option<byroredux_core::ecs::components::ActorVitals>,
    /// #4678 (CHAR-2026-09-21-D4-03) — the Player record's own level. The
    /// same derivation that produced `values` is keyed on it, so a player
    /// without a `CharacterLevel` read 0 in `GetLevel` /
    /// `GetXPForNextLevel` while its Health assumed level 1.
    pub(super) level: Option<u16>,
    /// #4678 — race/class provenance straight off the resolved Player
    /// record: exactly what `derive_npc_actor_values` already consumed,
    /// which is the honest value the old note claimed didn't exist.
    pub(super) background: Option<byroredux_core::character::Background>,
    /// #4699 — the Player record's own (TPLT-resolved) faction membership,
    /// the exemption the theft rule reads. Independent of the actor-value
    /// derivation: a record whose values derive to nothing still has its
    /// factions.
    pub(super) factions: Option<byroredux_core::ecs::components::FactionRanks>,
    /// #4415 — the Player record's spell list (own + race `SPLO`, leveled
    /// lists resolved) and the permanent value changes its constant spells
    /// make, translated through the same canonical path as NPC spawn.
    pub(super) spells: Vec<u32>,
    pub(super) spell_modifiers: Vec<byroredux_scripting::ConstantModifier>,
    /// #5413 — the racial `SPLO` subset on its own, exactly what
    /// [`stamp_spell_list`](crate::npc_spawn::stamp_spell_list) stamps on
    /// NPC placement roots. The scripted `AddRaceSpells`/`RemoveRaceSpells`
    /// pair reads the [`byroredux_scripting::RaceSpells`] component it
    /// feeds; without it both were silent no-ops on the player (the only
    /// actor the #4415 lowering ever targets).
    pub(super) racial_spells: Vec<u32>,
    /// #5920 — the Player record's own perks (Skyrim's `PRKR`: 12 innate perks
    /// on the Player `NPC_`), the same `(perk FormID, rank)` source and "ride
    /// no template flag" rule as
    /// [`stamp_character_components`](crate::npc_spawn::stamp_character_components).
    /// Empty on games that author none (FO3 / FNV / Oblivion).
    pub(super) perks: Vec<(u32, u8)>,
}

impl Resource for PlayerCharacterTemplate {}

/// The native HUD vitals bars' canonical keys — (display label, AVIF FormID)
/// pairs resolved once per plugin load from the same AVIF table `ActorValues`
/// is keyed by. Presentation state only: never serialized, rebuilt from
/// records on every `install_catalog`, exactly like the restorable-effect
/// catalog it sits beside.
#[derive(Debug, Clone, Default)]
pub(crate) struct PlayerVitals {
    pub(super) bars: Vec<(&'static str, u32)>,
    /// #4675 — canonical AVIF editor id → resolved FormID, from the same
    /// `vital_bar_candidates` table. The HUD drivers' bar tables carry
    /// editor ids (never literal FormIDs — the old hardcoded 0x2C9/0x2D0
    /// were fallout.rs's unit-test fixture ids, not real AVIFs) and
    /// resolve through this once per lookup instead of embedding ids.
    pub(super) by_editor_id: FxHashMap<&'static str, u32>,
    /// The rad meter (FO3 / FNV): `(label, AVIF FormID, full scale)`. The
    /// scale is the highest authored radiation sickness stage (`RADS`
    /// threshold — 1000 rads in `FalloutNV.esm`'s `Rad5`), so the gauge
    /// fills as the player nears the last stage. `None` when the profile
    /// names no radiation value or the master authors no stages.
    pub(super) radiation: Option<(&'static str, u32, f32)>,
}

impl PlayerVitals {
    /// The resolved key for a canonical AVIF editor id (#4675).
    pub(crate) fn resolved(&self, editor_id: &str) -> Option<u32> {
        self.by_editor_id.get(editor_id).copied()
    }
}

impl Resource for PlayerVitals {}

pub(super) fn build_player_vitals(index: &EsmIndex) -> PlayerVitals {
    // #4679 — the roster is `CharacterRulesProfile` data (same move #4447
    // made for `body_condition_base`), so a pool the game does not author
    // — or an Oblivion profile whose pre-#3768 resolver cannot resolve
    // any AVIF yet — drops out per candidate instead of the consumer
    // guessing per GameKind.
    let mut by_editor_id = FxHashMap::default();
    let bars = index
        .character_rules
        .vital_pools()
        .iter()
        .filter_map(|&(label, editor_id)| {
            let avif = index.actor_value_form_id(editor_id)?;
            by_editor_id.insert(editor_id, avif);
            Some((label, avif))
        })
        .collect();
    let radiation = index.character_rules.radiation_gauge().and_then(|(label, editor_id)| {
        let avif = index.actor_value_form_id(editor_id)?;
        let scale = index.radiation_stages.values().map(|stage| stage.threshold).max()?;
        (scale > 0).then(|| {
            by_editor_id.insert(editor_id, avif);
            (label, avif, scale as f32)
        })
    });
    PlayerVitals { bars, by_editor_id, radiation }
}

/// Compose the player's vitals bars from canonical `ActorValues`. Returns
/// `None` when there is no player body, it carries no actor values, or the
/// catalog resolved no bars — the HUD then draws nothing rather than empty
/// bars. `max` is the composed undamaged value (`base + set_override +
/// permanent + temporary`); `current` additionally subtracts the damage layer, so the
/// bar shows the red-equivalent missing share the same way combat damage
/// and restorative consumption read it.
pub(crate) fn vitals_snapshot(
    world: &World,
) -> Option<Vec<byroredux_debug_ui::VitalBarView>> {
    use byroredux_core::ecs::components::ActorValues;
    let player = PlayerEntity::get(world)?;
    let vitals = world.try_resource::<PlayerVitals>()?;
    let values = world.get::<ActorValues>(player)?;
    let mut bars: Vec<byroredux_debug_ui::VitalBarView> = vitals
        .bars
        .iter()
        .filter_map(|&(label, avif)| {
            let entry = values.get(avif)?;
            let max = entry.undamaged();
            if max <= 0.0 {
                return None;
            }
            Some(byroredux_debug_ui::VitalBarView {
                label,
                current: max - entry.damage,
                max,
            })
        })
        .collect();
    // The rad meter accumulates rather than depletes: it shows the current
    // rads (none yet is an empty gauge, not a missing one) against the
    // last sickness stage, and is drawn only beside real pools.
    if let (Some((label, avif, scale)), false) = (vitals.radiation, bars.is_empty()) {
        bars.push(byroredux_debug_ui::VitalBarView {
            label,
            current: values.current(avif).clamp(0.0, scale),
            max: scale,
        });
    }
    (!bars.is_empty()).then_some(bars)
}

/// Derive the player's actor values from the base Player `NPC_` record —
/// never the placed reference (`0x14`) — mirroring `stamp_actor_values`
/// (`npc_spawn.rs`) line for line: `derive_npc_actor_values`, then vitals
/// keyed by the resolved Health AVIF only when Health actually landed.
///
/// #4674 (CHAR-2026-09-21-D4-01) — with one difference from the NPC path:
/// the ruleset's `PlayerOnly` rows (FO3/FNV/FO4 Health + AP, Skyrim Light
/// Armor, Oblivion's pools) are evaluated HERE, for the player, against
/// the remaining seed — and the NPC-baked carried answers for those same
/// keys are dropped first. The NPC path answers Health/AP by baked values
/// or an NPC curve; the captures state the player's live values come from
/// the player formulas (FO4: Health 85 / AP 70, not the carried 150/100
/// the PRPS-then-DNAM push order was leaving). Stamping the evaluated
/// values means every consumer — `GetActorValue`'s carried fast path,
/// `vitals_snapshot`, combat damage, drowning — reads them without
/// needing player identity.
///
/// #5039 — the stamp is a cache of the formula, not a one-shot:
/// `attach_to_player` re-runs [`CharacterRuleset::refresh_player_only_bases`]
/// after the record's constant spells land, and
/// [`player_derived_stats_system`] re-runs it whenever an input (END, AGI,
/// level, …) moves afterwards.
///
/// [`CharacterRuleset::refresh_player_only_bases`]: byroredux_core::character::CharacterRuleset::refresh_player_only_bases
pub(super) fn build_player_character_template(index: &EsmIndex) -> PlayerCharacterTemplate {
    let Some(player) = index.npcs.get(&player_npc_form_id(index.game)) else {
        return PlayerCharacterTemplate::default();
    };
    // #4457 — resolve once, derive through the resolved view.
    let resolved = byroredux_plugin::equip::ResolvedNpc::resolve(player, index);
    let factions = crate::npc_spawn::faction_ranks_of(&resolved);
    let spells = byroredux_plugin::equip::resolve_actor_spells(&resolved, index);
    // #5413 — the racial subset beside the merged list, same pair
    // `stamp_spell_list` stamps on NPC roots, so the scripted
    // AddRaceSpells/RemoveRaceSpells pair works on the player too.
    let racial_spells = byroredux_plugin::equip::resolve_racial_spells(&resolved, index);
    let spell_modifiers: Vec<_> = spells
        .iter()
        .flat_map(|&spell| {
            byroredux_scripting::magic::canonical_spell(index, spell).constant_modifiers
        })
        .collect();
    let perks = resolved.shell.perks.clone();
    let pairs = byroredux_plugin::esm::records::derive_resolved_actor_values(&resolved, index);
    if pairs.is_empty() {
        return PlayerCharacterTemplate {
            factions,
            spells,
            spell_modifiers,
            racial_spells,
            perks,
            ..Default::default()
        };
    }
    // #4678 — level + Background ride the same resolved Player record the
    // derivation just consumed, so the stamps can't disagree with the
    // values they sit beside. #5047 — from the terminals, exactly like
    // `stamp_character_components`: level from the Use-Stats source,
    // race from the Use-Traits source, class from the Use-Stats source —
    // a templated Player record then stamps one consistent answer instead
    // of a terminal body race beside a shell Background race.
    let level = byroredux_plugin::esm::records::effective_actor_level(resolved.stats);
    let background = Some(byroredux_core::character::Background {
        race_form_id: resolved.r#traits.race_form_id,
        class_form_id: resolved.stats.class_form_id,
    });
    // The ruleset is optional (profiles that build none); without it the
    // derivation stays the NPC answer, the pre-#4674 behaviour, rather
    // than an invented empty seed.
    match crate::npc_spawn::build_character_ruleset(index) {
        Some(ruleset) => {
            let player_only = ruleset.player_only_output_avifs();
            // Drop the NPC path's answers for player-only stats, then
            // evaluate the rows against the remaining SPECIAL/skills seed.
            let seed: Vec<_> = pairs
                .into_iter()
                .filter(|(id, _)| !player_only.contains(id))
                .collect();
            let mut values = byroredux_core::ecs::components::ActorValues::from_pairs(seed);
            let level = level.max(0) as u16;
            // Only Absolute rows are stamped (the #2933 contract): a
            // Multiplier row's eval is a ratio no consumer may read as a
            // value.
            ruleset.refresh_player_only_bases(&mut values, level);
            let health = index.health_actor_value_key().filter(|health| {
                values.get(*health).is_some()
            });
            PlayerCharacterTemplate {
                values: Some(values),
                vitals: health.map(|health| byroredux_core::ecs::components::ActorVitals {
                    health,
                }),
                level: Some(level),
                background,
                factions,
                spells,
                spell_modifiers,
                racial_spells,
                perks,
            }
        }
        None => {
            let health = index
                .health_actor_value_key()
                .filter(|health| pairs.iter().any(|(form_id, _)| form_id == health));
            PlayerCharacterTemplate {
                values: Some(byroredux_core::ecs::components::ActorValues::from_pairs(
                    pairs,
                )),
                vitals: health.map(|health| byroredux_core::ecs::components::ActorVitals {
                    health,
                }),
                level: Some(level.max(0) as u16),
                background,
                factions,
                spells,
                spell_modifiers,
                racial_spells,
                perks,
            }
        }
    }
}

/// The effects an ingestible starts on its user through the caster runtime: every authored effect of
/// an item the restoration plan does not cover, or only the scripted ones of an item it does (`planned`).
/// `None` refuses the whole item (#6229), with the restoration plan's all-or-nothing rule: the caster
/// runtime evaluates no effect condition, delivers only to self, and plays no poison, addiction or
/// withdrawal, so a header that is not simple consumption, an effect-local CTDA, a touch/target delivery,
/// an area, or an effect the runtime does not model makes the item unavailable rather than partial.
pub(super) fn caster_effects(
    index: &EsmIndex,
    form_id: u32,
    planned: bool,
) -> Option<Vec<byroredux_scripting::magic_effects::CastEffect>> {
    let ItemKind::Aid {
        authored_effects: Some(authored),
        simple_consumption_header: true,
        ..
    } = &index.items.get(&form_id)?.kind
    else {
        return None;
    };
    let cast = byroredux_scripting::magic_effects::consumable_cast_effects(index, form_id);
    let mut effects = Vec::new();
    for (entry, effect) in authored.iter().zip(cast) {
        if planned && effect.script == 0 {
            continue; // the plan applies it
        }
        if !effect.is_modelled()
            || !entry.conditions.is_empty()
            || entry.delivery.is_some_and(|delivery| delivery != 0)
            || entry.effect.area != 0
        {
            return None;
        }
        effects.push(effect);
    }
    Some(effects)
}

/// Rebuild item presentation metadata and the base player's starting loadout
/// from the resolved plugin index.
pub(crate) fn install_catalog(world: &mut World, index: &EsmIndex) {
    let mut restorations: FxHashMap<_, _> = index
        .items
        .keys()
        .filter_map(|&id| {
            byroredux_plugin::consumables::restoration_plan(index, id).map(|effects| (id, effects))
        })
        .collect();
    let mut script_effects: FxHashMap<_, _> = FxHashMap::default();
    for &id in index.items.keys() {
        let planned = restorations.contains_key(&id);
        match caster_effects(index, id, planned) {
            Some(effects) if !effects.is_empty() => {
                script_effects.insert(id, effects);
            }
            Some(_) => {}
            // A planned item whose scripted effect is refused is refused whole, never half-drunk.
            None if planned => {
                restorations.remove(&id);
            }
            None => {}
        }
    }
    let entries = index
        .items
        .iter()
        .map(|(&form_id, item)| {
            let authored_name = item.common.full_name.trim();
            let editor_id = item.common.editor_id.trim();
            let name = if !authored_name.is_empty() {
                authored_name.to_owned()
            } else if !editor_id.is_empty() {
                editor_id.to_owned()
            } else {
                format!("Item {form_id:08X}")
            };
            let (category, mut details, equip_target) = describe_kind(&item.kind);
            if matches!(item.kind, ItemKind::Aid { .. }) {
                details = if let Some(plan) = restorations.get(&form_id) {
                    if plan.iter().any(|e| e.duration > 0) {
                        "Restores vital resources over time".to_owned()
                    } else if plan.iter().any(|e| !e.conditions.is_empty()) {
                        "Restores vital resources when its conditions are met".to_owned()
                    } else {
                        "Restores vital resources immediately".to_owned()
                    }
                } else if script_effects.contains_key(&form_id) {
                    "Applies its effects when used".to_owned()
                } else {
                    "Consumable (effects unavailable)".to_owned()
                };
            }
            let (weapon_damage, weapon_reach, weapon_speed) = match &item.kind {
                ItemKind::Weapon {
                    damage,
                    reach,
                    speed,
                    ..
                } => (Some(*damage as f32), *reach, *speed),
                _ => (None, 0.0, 0.0),
            };
            (
                form_id,
                InventoryItemDefinition {
                    name,
                    category,
                    value: item.common.value,
                    weight: item.common.weight.max(0.0),
                    details,
                    equip_target,
                    weapon_damage,
                    weapon_reach,
                    weapon_speed,
                },
            )
        })
        .collect();
    world.insert_resource(InventoryCatalog {
        restorations,
        script_effects,
        entries,
        containers: index.containers.keys().copied().collect(),
        player_base_form_id: player_npc_form_id(index.game),
    });
    world.insert_resource(build_player_template(index));
    world.insert_resource(build_player_character_template(index));
    world.insert_resource(build_player_vitals(index));
}

pub(super) fn describe_kind(kind: &ItemKind) -> (&'static str, String, Option<EquipTarget>) {
    match kind {
        ItemKind::Armor {
            biped_flags,
            dt,
            dr,
            armor_rating_x100,
            ..
        } => {
            let protection = if *armor_rating_x100 > 0 {
                format!("Armor rating {:.1}", *armor_rating_x100 as f32 / 100.0)
            } else if *dt > 0.0 || *dr > 0 {
                format!("DT {:.1} · DR {}", dt, dr)
            } else {
                "Armor".to_owned()
            };
            (
                "Armor",
                protection,
                // The authored mask verbatim — every bit of it addresses a
                // biped slot, including bit 31 (#3112).
                (*biped_flags != 0).then_some(EquipTarget::BipedSlots(*biped_flags)),
            )
        }
        ItemKind::Weapon { damage, .. } => (
            "Weapon",
            format!("Damage {damage}"),
            Some(EquipTarget::Weapon),
        ),
        ItemKind::Ammo { damage, .. } => ("Ammo", format!("Damage {damage:.1}"), None),
        ItemKind::Aid { .. } => ("Aid", "Consumable".to_owned(), None),
        ItemKind::Ingredient { .. } => ("Ingredient", "Ingredient".to_owned(), None),
        ItemKind::Book { .. } => ("Book", "Book".to_owned(), None),
        ItemKind::Scroll { .. } => ("Scroll", "Scroll (casting unavailable)".to_owned(), None),
        ItemKind::Apparatus { .. } => (
            "Apparatus",
            "Alchemy apparatus (crafting unavailable)".to_owned(),
            None,
        ),
        ItemKind::Light { .. } => (
            "Light",
            "Carried light (equipping unavailable)".to_owned(),
            None,
        ),
        ItemKind::Note { .. } => ("Note", "Note".to_owned(), None),
        ItemKind::Key => ("Key", "Key".to_owned(), None),
        ItemKind::Mod { .. } => ("Mods", "Loose object modification".to_owned(), None),
        ItemKind::Junk => ("Junk", "Scrappable components".to_owned(), None),
        ItemKind::Misc => ("Misc", "Miscellaneous item".to_owned(), None),
    }
}

pub(super) fn build_player_template(index: &EsmIndex) -> PlayerInventoryTemplate {
    build_player_template_for(index, player_npc_form_id(index.game))
}

pub(super) fn build_player_template_for(index: &EsmIndex, player_form_id: u32) -> PlayerInventoryTemplate {
    let Some(player) = index.npcs.get(&player_form_id) else {
        log::error!(
            "Player inventory unavailable: {:?} NPC_ {:08X} is missing from the resolved index",
            index.game,
            player_form_id,
        );
        return PlayerInventoryTemplate::default();
    };
    // #4457 — the player path resolves the TPLT view once and reads both
    // the derivation and the CNTO carry list through it.
    // #5047 — level and outfit read the Use-Stats / Use-Inventory
    // terminals, the same sources `NpcSpawnJob` consumes, so a templated
    // Player expands its gear at the level the engine actually uses.
    let resolved = byroredux_plugin::equip::ResolvedNpc::resolve(player, index);
    let actor_level = effective_actor_level(resolved.stats);
    let mut inventory = Inventory::new();
    let mut equipment = EquipmentSlots::new();
    let mut equipped_weapon = None;
    let mut expanded = Vec::new();

    // Skyrim+ authors initial worn gear through OTFT. Older games generally
    // equip armor directly from CNTO, so their carried armor is equipped below.
    if let Some(outfit_id) = resolved.inventory.default_outfit {
        if let Some(outfit) = index.outfits.get(&outfit_id) {
            for &form_id in &outfit.items {
                expanded.clear();
                byroredux_plugin::equip::expand_leveled_form_id(
                    form_id,
                    actor_level,
                    index,
                    &mut expanded,
                );
                for resolved in expanded.drain(..) {
                    let item_index = add_stack(&mut inventory, resolved, 1);
                    equip_armor(index, &mut equipment, resolved, item_index);
                    prefer_weapon(index, resolved, item_index, &mut equipped_weapon);
                }
            }
        }
    }

    let equip_carried_armor = resolved.inventory.default_outfit.is_none();
    for entry in &resolved.inventory.inventory {
        let count = entry.count.max(0) as u32;
        if count == 0 {
            continue;
        }
        expanded.clear();
        byroredux_plugin::equip::expand_leveled_form_id(
            entry.item_form_id,
            actor_level,
            index,
            &mut expanded,
        );
        for resolved in expanded.drain(..) {
            let item_index = add_stack(&mut inventory, resolved, count);
            if equip_carried_armor {
                equip_armor(index, &mut equipment, resolved, item_index);
            }
            prefer_weapon(index, resolved, item_index, &mut equipped_weapon);
        }
    }

    PlayerInventoryTemplate {
        inventory,
        equipment,
        equipped_weapon,
    }
}

pub(super) fn add_stack(inventory: &mut Inventory, form_id: u32, count: u32) -> InventoryIndex {
    inventory.add(ItemStack::new(form_id, count))
}

pub(super) fn equip_armor(
    index: &EsmIndex,
    equipment: &mut EquipmentSlots,
    form_id: u32,
    inventory_index: InventoryIndex,
) {
    let Some(ItemKind::Armor { biped_flags, .. }) =
        index.items.get(&form_id).map(|item| &item.kind)
    else {
        return;
    };
    if *biped_flags != 0 {
        equipment.equip(*biped_flags, inventory_index);
    }
}

/// Select one deterministic weapon from authored inventory: highest base
/// damage wins, then lowest FormID breaks ties. Multiple LVLI outcomes can
/// still be carried, but only one becomes live combat state.
pub(super) fn prefer_weapon(
    index: &EsmIndex,
    form_id: u32,
    inventory_index: InventoryIndex,
    equipped: &mut Option<EquippedWeapon>,
) {
    let Some(ItemKind::Weapon {
        damage,
        reach,
        speed,
        ..
    }) = index.items.get(&form_id).map(|item| &item.kind)
    else {
        return;
    };
    let candidate = EquippedWeapon {
        inventory_index,
        base_form_id: form_id,
        damage: *damage as f32,
        reach: *reach,
        speed: *speed,
    };
    if equipped.is_none_or(|current| {
        candidate.damage > current.damage
            || (candidate.damage == current.damage && candidate.base_form_id < current.base_form_id)
    }) {
        *equipped = Some(candidate);
    }
}

/// Attach canonical inventory state to a newly-created player body.
pub(crate) fn attach_to_player(world: &mut World, player: byroredux_core::ecs::EntityId) {
    let template = world
        .try_resource::<PlayerInventoryTemplate>()
        .map(|template| template.clone())
        .unwrap_or_default();
    world.insert(player, template.inventory);
    let mut equipment = template.equipment;
    // #3112 — mirror the template's starting weapon into the equipment's
    // weapon slot. `prefer_weapon` only fills `equipped_weapon`, so pre-fix
    // the slots and the component disagreed from spawn: the very first menu
    // toggle re-derived `EquippedWeapon` from an empty slot and dropped the
    // authored starting weapon.
    if let Some(weapon) = template.equipped_weapon {
        equipment.equip_weapon(weapon.inventory_index);
    }
    world.insert(player, equipment);
    if let Some(weapon) = template.equipped_weapon {
        world.insert(player, weapon);
    }
    // #4458 — the player's CHARAL seed rides the same one-shot attach as
    // the inventory seed: `ActorValues` + `ActorVitals` derived from the
    // base Player NPC_ record, the exact stamps `stamp_actor_values` puts
    // on every NPC. The consumable (`consume_item`) and drowning
    // (`apply_player_drowning_damage`) paths gate on precisely these two
    // components; before this they were structurally Unavailable in every
    // fresh session.
    let character = world
        .try_resource::<PlayerCharacterTemplate>()
        .map(|template| template.clone())
        .unwrap_or_default();
    if let Some(mut values) = character.values {
        // #4415 — the Player record's constant spells (racial abilities).
        byroredux_scripting::magic::apply_modifiers(&mut values, &character.spell_modifiers, 1.0);
        // #5039 — the template evaluated the player-only pools before these
        // modifiers existed; an END/AGI ability must reach Health/AP now.
        if let (Some(ruleset), Some(level)) = (
            world.try_resource::<byroredux_core::character::CharacterRuleset>(),
            character.level,
        ) {
            ruleset.refresh_player_only_bases(&mut values, level);
        }
        world.insert(player, values);
    }
    world.insert(player, byroredux_scripting::SpellList(character.spells));
    // #5413 — the racial subset beside the list, the same pair
    // `stamp_spell_list` stamps on NPC placement roots: the scripted
    // AddRaceSpells/RemoveRaceSpells pair reads this component, and the
    // player body path is the one placement route that skips the stamp.
    world.insert(player, byroredux_scripting::RaceSpells(character.racial_spells));
    if let Some(vitals) = character.vitals {
        world.insert(player, vitals);
    }
    // #4678 — the level the Health derivation assumed, and the race/class
    // provenance from the same record: `GetLevel`/`GetXPForNextLevel` and
    // the loot/melee level-1 defaults now read real data instead of
    // divergent fallbacks (0 in one consumer, 1 in the next).
    if let Some(level) = character.level {
        world.insert(
            player,
            byroredux_core::character::CharacterLevel {
                level,
                xp: 0,
            },
        );
    }
    if let Some(background) = character.background {
        world.insert(player, background);
    }
    // #4699 — the player's own authored factions (FNV PlayerFaction and
    // 21 more; 4 on Skyrim), so the theft rule's membership exemption and
    // `GetFactionRank`/`GetInFaction` on the player read real data.
    if let Some(factions) = character.factions {
        world.insert(player, factions);
    }
    // #5920 — the Player record's own perks (Skyrim authors 12), stamped like
    // the NPC path does: the component only exists when there is something to
    // own, and `levelup perk` / script `AddPerk` create it on demand.
    if !character.perks.is_empty() {
        world.insert(
            player,
            byroredux_core::character::Perks {
                entries: character
                    .perks
                    .iter()
                    .map(|&(perk_form_id, rank)| byroredux_core::character::PerkRank { perk_form_id, rank })
                    .collect(),
            },
        );
    }
}
