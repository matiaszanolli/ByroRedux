//! Tempering (improving) — the pieces the scripting crate owns (M43).
//!
//! UESP *Skyrim:Smithing*: improving a weapon or armor at a grindstone or workbench raises its quality by the
//! player's Smithing skill, and the material perks improve it "100% better". The data says how:
//!
//! * the improving recipes (`COBJ` at `CraftingSmithingSharpeningWheel` / `CraftingSmithingArmorTable`) gate
//!   enchanted items on a perk — conditions `EPTemperingItemIsEnchanted != 1 OR HasPerk(ArcaneBlacksmith)`
//!   (xEdit names function 659); [`TemperingItem`] is what that function reads;
//! * the Smithing material perks (`SteelSmithing`, `DwarvenSmithing`, …) are entry-point perks on 76 `Mod
//!   Tempering Health`, `Multiply` by `2.0`, gated by a condition tab whose subject is the *item* (`HasKeyword`
//!   the material keyword). [`tempering_multiplier`] composes them with the item standing in as that subject.
//!
//! The item is not an entity, so a scratch entity ([`TemperingSubject`]) carries its base form for the length of
//! one evaluation: the keyword table already holds every `ARMO` / `WEAP` base's keywords.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;

/// The item being tempered, as the conditions see it. `NOT_SAVED_BY_DESIGN`: set for one evaluation.
#[derive(Debug, Default, Clone, Copy)]
pub struct TemperingItem {
    pub enchanted: bool,
}

impl Resource for TemperingItem {}

/// Whether the item being tempered is enchanted (`EPTemperingItemIsEnchanted`); false with none set.
pub fn item_is_enchanted(world: &World) -> bool {
    world.try_resource::<TemperingItem>().is_some_and(|item| item.enchanted)
}

/// Run `f` with the item being tempered marked enchanted or not, then clear the mark.
pub fn with_item_enchanted<R>(world: &World, enchanted: bool, f: impl FnOnce() -> R) -> R {
    if let Some(mut item) = world.try_resource_mut::<TemperingItem>() {
        item.enchanted = enchanted;
    }
    let result = f();
    if let Some(mut item) = world.try_resource_mut::<TemperingItem>() {
        item.enchanted = false;
    }
    result
}

/// The scratch entity an item's base form is lent to, so item-tab conditions have a subject. Created once at
/// boot by [`install`]. `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Clone, Copy)]
pub struct TemperingSubject(pub EntityId);

impl Resource for TemperingSubject {}

/// Install the tempering resources: the context and the scratch subject.
pub fn install(world: &mut World) {
    world.init_resource::<TemperingItem>();
    world.register::<crate::scene::SceneAliasCandidate>();
    let subject = world.spawn();
    world.insert_resource(TemperingSubject(subject));
}

/// The factor `owner`'s perks multiply the improvement of `item_base` by (`Mod Tempering Health`); `1.0` when
/// none applies or the game has no perk table. Above `1.0` means a material perk applies to the item.
pub fn tempering_multiplier(world: &World, owner: EntityId, item_base: u32, enchanted: bool) -> f32 {
    let Some(subject) = world.try_resource::<TemperingSubject>().map(|subject| subject.0) else {
        return 1.0;
    };
    // Lend the item's base form to the scratch subject and its enchantment to the context.
    if let Some(mut candidates) = world.query_mut::<crate::scene::SceneAliasCandidate>() {
        candidates.insert(subject, crate::scene::SceneAliasCandidate { base_form_id: item_base, ..Default::default() });
    }
    if let Some(mut item) = world.try_resource_mut::<TemperingItem>() {
        item.enchanted = enchanted;
    }
    let value = crate::perk_entry_points::compose_entry_point(
        world,
        owner,
        crate::perk_entry_points::skyrim::MOD_TEMPERING_HEALTH,
        1.0,
        &[Some(subject)],
    );
    if let Some(mut item) = world.try_resource_mut::<TemperingItem>() {
        item.enchanted = false;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::character::Perks;
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue, RunOn};
    use byroredux_plugin::esm::records::{EsmIndex, ItemKind, ItemRecord, PerkConditionTab, PerkEntry, PerkEntryBody, PerkFunctionData, PerkRecord};

    const STEEL_SMITHING: u32 = 0xCB40D;
    const STEEL_KEYWORD: u32 = 0x6BBE6;
    const STEEL_SWORD: u32 = 0x13989;
    const IRON_SWORD: u32 = 0x12EB7;

    /// The vanilla Steel Smithing shape: `Mod Tempering Health` × 2.0, with an item-tab condition `HasKeyword(steel)`.
    fn world() -> (World, EntityId, EntityId) {
        let has_keyword = Condition {
            function_index: 560,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(1.0),
            param_1: STEEL_KEYWORD,
            run_on: RunOn::Subject,
            ..Default::default()
        };
        let entry = PerkEntry {
            rank: 0,
            priority: 0,
            body: PerkEntryBody::EntryPoint {
                entry_point_index: crate::perk_entry_points::skyrim::MOD_TEMPERING_HEALTH,
                function: 3,
                tab_count: 2,
                param_type: 1,
                function_data: PerkFunctionData::Float(2.0),
                formatter: Vec::new(),
                extra_flags: Vec::new(),
            },
            condition_tabs: vec![PerkConditionTab { tab: 1, conditions: vec![has_keyword] }],
        };
        let mut index = EsmIndex {
            game: GameKind::Skyrim,
            character_rules: byroredux_core::character::CharacterRulesProfile::SKYRIM,
            ..Default::default()
        };
        index.perks.insert(STEEL_SMITHING, PerkRecord { form_id: STEEL_SMITHING, editor_id: "SteelSmithing".into(), entries: vec![entry], ..Default::default() });
        for (base, keywords) in [(STEEL_SWORD, vec![STEEL_KEYWORD]), (IRON_SWORD, Vec::new())] {
            let mut item = ItemRecord { form_id: base, common: Default::default(), kind: ItemKind::weapon_stub(8, 0) };
            item.common.keywords = keywords;
            index.items.insert(base, item);
        }
        let mut world = World::new();
        crate::register(&mut world);
        world.register::<Perks>();
        world.register::<byroredux_core::ecs::components::ActorValues>();
        world.register::<byroredux_core::ecs::components::Dead>();
        crate::keywords::install_keyword_table(&mut world, &index);
        crate::perk_entry_points::install_perk_entry_points(&mut world, &index);
        let player = world.spawn();
        let mut perks = Perks::default();
        perks.set_rank(STEEL_SMITHING, 1);
        world.insert(player, perks);
        let nobody = world.spawn();
        world.insert(nobody, Perks::default());
        (world, player, nobody)
    }

    /// Steel Smithing doubles the improvement of a steel item only: the iron sword has no steel keyword, and a
    /// player without the perk gets nothing from it.
    #[test]
    fn the_material_perk_multiplies_only_items_of_its_material() {
        let (world, player, nobody) = world();
        assert_eq!(tempering_multiplier(&world, player, STEEL_SWORD, false), 2.0);
        assert_eq!(tempering_multiplier(&world, player, IRON_SWORD, false), 1.0, "iron has no material perk");
        assert_eq!(tempering_multiplier(&world, nobody, STEEL_SWORD, false), 1.0);
    }

    #[test]
    fn the_enchanted_mark_is_set_for_the_evaluation_and_cleared_after() {
        let (world, _player, _nobody) = world();
        assert!(!item_is_enchanted(&world));
        assert!(with_item_enchanted(&world, true, || item_is_enchanted(&world)));
        assert!(!item_is_enchanted(&world), "cleared after");
    }
}
